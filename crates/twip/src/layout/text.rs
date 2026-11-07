//! Paragraph shaping

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::fields::{Shown, format_field};
use crate::docx::model::*;
use crate::docx::props::VertAlign;
use crate::docx::review::run_in;
use crate::font::{Face, Fonts, Glyph};

use super::cache::{CacheKey, cached};
use super::env::TextEnv;
use super::ids::{ParaId, Unit};
use super::job::{Memos, Yield};
use super::space::ShapeSpace;
use super::units::Lu;

pub(crate) const EM_MARK: f64 = 3.4 / 11.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Space,
    Tab,
    LineBreak,
    PageBreak,
    ColumnBreak,
    Object,
    Anchor,
    ListNumber,
    Separator,
    ContSeparator,
}

#[derive(Clone, Debug)]
pub struct PieceStyle {
    pub run: Run,
    pub face: Option<Rc<Face>>,
    pub size: f64,
    pub raise: f64,
    pub squeeze_avg: f64,
}

impl PartialEq for PieceStyle {
    fn eq(&self, o: &Self) -> bool {
        let face = match (&self.face, &o.face) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        face && (Rc::ptr_eq(&self.run, &o.run) || *self.run == *o.run) && self.size == o.size && self.raise == o.raise
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Picture(Picture),
    Float(Rc<Float>),
    Note(NoteKind, i64),
    PTab(crate::docx::PTab),
    Expansion(Rc<super::expansion::Expansion>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: Kind,
    pub break_after: bool,
    pub soft_hyphen: bool,
    pub style: u32,
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    pub glyphs: (u32, u32),
    pub text: (u32, u32),
    pub src: Option<Unit>,
    pub object: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub struct ShapedPara {
    pub pieces: Box<[Piece]>,
    pub styles: Box<[PieceStyle]>,
    pub glyph_ids: Box<[u32]>,
    pub glyph_advances: Box<[f64]>,
    pub glyph_dx: Box<[f64]>,
    pub text: Box<str>,
    pub objects: Box<[Object]>,
    pub join: Option<Rc<super::join::JoinMap>>,
    pub em1000: bool,
}

impl ShapedPara {
    pub fn metrics(&self, face: &Face, run: &RunStyle, size: f64) -> (f64, f64, f64) {
        metrics_at(face, size, self.em1000 && run.east_asia_named)
    }

    pub fn style(&self, i: usize) -> &PieceStyle {
        &self.styles[self.pieces[i].style as usize]
    }

    pub fn face(&self, i: usize) -> Option<&Rc<Face>> {
        self.style(i).face.as_ref()
    }

    pub fn run(&self, i: usize) -> &Run {
        &self.style(i).run
    }

    pub fn text(&self, i: usize) -> &str {
        let (a, b) = self.pieces[i].text;
        &self.text[a as usize..b as usize]
    }

    pub fn glyphs(&self, i: usize) -> impl Iterator<Item = (u32, f64, f64)> + '_ {
        let (a, b) = self.pieces[i].glyphs;
        (a as usize..b as usize).map(|k| (self.glyph_ids[k], self.glyph_advances[k], self.glyph_dx[k]))
    }

    pub fn object(&self, i: usize) -> Option<&Object> {
        self.pieces[i].object.map(|k| &self.objects[k as usize])
    }

    pub fn note(&self, i: usize) -> Option<(NoteKind, i64)> {
        match self.object(i) {
            Some(Object::Note(k, id)) => Some((*k, *id)),
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    pub fn bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<ShapedPara>()
            + self.pieces.len() * size_of::<Piece>()
            + self.styles.len() * size_of::<PieceStyle>()
            + self.glyph_ids.len() * (4 + 8 + 8)
            + self.text.len()
            + self.objects.len() * size_of::<Object>()
    }
}

pub fn metrics(face: &Face, size: f64) -> (f64, f64, f64) {
    metrics_at(face, size, false)
}

pub fn metrics_at(face: &Face, size: f64, em1000: bool) -> (f64, f64, f64) {
    if em1000 {
        let r = |v: f64| (v * 1000.0 / face.upem).round();
        metrics_from(FontMetrics { ascent: r(face.ascent), descent: r(face.descent), leading: r(face.leading), upem: 1000.0 }, size)
    } else {
        metrics_from(FontMetrics { ascent: face.ascent, descent: face.descent, leading: face.leading, upem: face.upem }, size)
    }
}

/// Vertical metrics in font units.
#[derive(Clone, Copy)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
    pub upem: f64,
}

/// Ascent, descent and gap in points at `size`.
pub fn metrics_from(m: FontMetrics, size: f64) -> (f64, f64, f64) {
    let FontMetrics { ascent: a, descent: d, leading: g, upem } = m;
    let above = Lu::from_font(a + g, size, upem);
    let gap = Lu::from_font(g, size, upem);
    let below = Lu::from_font(d, size, upem);
    (Lu(above.0 - gap.0).to_pt(), below.to_pt(), gap.to_pt())
}

fn width_at_em1000(advance: f64, upem: f64, size: f64) -> f64 {
    let w = (advance * 1000.0 / upem).round();
    (w * 2.0 * size * 2048.0 / 1000.0).floor() / super::units::LU_PER_PT
}

pub fn hp(x: f64) -> i64 {
    (2.0 * x).round() as i64
}

pub fn symbol_unicode(family: &str, c: char) -> char {
    let cp = c as u32;
    let code = match cp {
        0xF020..=0xF0FF => cp - 0xF000,
        0x20..=0xFF if matches!(family, "Symbol" | "Wingdings") => cp,
        _ => return c,
    };
    let mapped = match family {
        "Symbol" => match code {
            0xB7 => 0x2022,
            0xA7 => 0x2663,
            0xA8 => 0x2666,
            0xA9 => 0x2665,
            0xAA => 0x2660,
            0xB0 => 0xB0,
            0xB1 => 0xB1,
            0xB4 => 0xD7,
            0xB8 => 0xF7,
            0xAE => 0x2192,
            0xAC => 0x2190,
            0xD6 => 0x221A,
            0xE5 => 0x2211,
            _ => 0,
        },
        "Wingdings" => match code {
            0xA7 => 0x25AA,
            0x6C => 0x25CF,
            0x6E => 0x25A0,
            0x6F => 0x25A1,
            0x71 => 0x2751,
            0x75 => 0x25C6,
            0x76 => 0x2756,
            0xA8 => 0x25FB,
            0xD8 => 0x27A2,
            0xE8 => 0x2794,
            0xFC => 0x2713,
            0xFD => 0x2612,
            0xFE => 0x2611,
            0x9F => 0x2022,
            _ => 0,
        },
        _ => 0,
    };
    char::from_u32(mapped).filter(|_| mapped != 0).unwrap_or(c)
}

pub(crate) fn family_for(c: char, run: &RunStyle) -> &str {
    let cp = c as u32;
    if is_east_asian(cp) || (run.hint_east_asia && shared_with_east_asia(cp)) {
        return &run.east_asia;
    }
    if cp < 0x80 { &run.ascii } else { &run.h_ansi }
}

fn is_east_asian(cp: u32) -> bool {
    matches!(cp, 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// List suffix space width, in em.
const LIST_SUFFIX_SPACE_EM: f64 = 569.0 / 2048.0;

/// East Asian punctuation family.
const EAST_ASIAN_PUNCTUATION: &str = "MS Mincho";

fn shared_with_east_asia(cp: u32) -> bool {
    match cp {
        0xA1 | 0xA4 | 0xA7 | 0xA8 | 0xAA | 0xAD | 0xAF | 0xD7 | 0xF7 => true,
        0xB0..=0xB4 | 0xB6..=0xBA | 0xBC..=0xBF => true,
        0x200B..=0x200F | 0x2028..=0x202E => false,
        0x2000..=0x2BFF => true,
        _ => false,
    }
}

struct Builder<'a> {
    fonts: &'a Fonts,
    pieces: Vec<Piece>,
    styles: Vec<PieceStyle>,
    glyph_ids: Vec<u32>,
    glyph_advances: Vec<f64>,
    glyph_dx: Vec<f64>,
    text: String,
    objects: Vec<Object>,
    em1000: bool,
}

struct NewPiece {
    kind: Kind,
    style: PieceStyle,
    ascent: f64,
    descent: f64,
    width: f64,
    break_after: bool,
    soft_hyphen: bool,
    object: Option<Object>,
}

impl Builder<'_> {
    fn face(&self, family: &str, run: &RunStyle) -> Option<Rc<Face>> {
        self.fonts.face(family, run.category(family), run.bold, run.italic)
    }

    fn style_index(&mut self, s: PieceStyle) -> u32 {
        match self.styles.iter().rev().take(8).position(|x| *x == s) {
            Some(back) => (self.styles.len() - 1 - back) as u32,
            None => {
                self.styles.push(s);
                (self.styles.len() - 1) as u32
            }
        }
    }

    fn push(&mut self, p: NewPiece, text: &str, glyphs: &[Glyph]) {
        let g0 = self.glyph_ids.len() as u32;
        for g in glyphs {
            self.glyph_ids.push(g.id);
            self.glyph_advances.push(g.advance);
            self.glyph_dx.push(g.dx);
        }
        let t0 = self.text.len() as u32;
        self.text.push_str(text);
        let object = p.object.map(|o| {
            self.objects.push(o);
            (self.objects.len() - 1) as u32
        });
        let style = self.style_index(p.style);
        self.pieces.push(Piece {
            kind: p.kind,
            break_after: p.break_after,
            soft_hyphen: p.soft_hyphen,
            style,
            width: p.width,
            ascent: p.ascent,
            descent: p.descent,
            glyphs: (g0, self.glyph_ids.len() as u32),
            text: (t0, self.text.len() as u32),
            src: None,
            object,
        });
    }

    fn vertical(&self, run: &RunStyle, face: &Face) -> (f64, f64) {
        let base = run.size;
        let (size, mut raise) = match run.vert_align {
            VertAlign::Baseline => (base, 0.0),
            VertAlign::Superscript => {
                let size = hp(base * face.sup_size / face.upem).max(2) as f64 / 2.0;
                let (a, _, e) = metrics(face, base);
                let (asup, _, _) = metrics(face, size);
                let (a_hp, asup_hp) = (hp(a + e), hp(asup));
                let limit = (a_hp / 3).max(a_hp - asup_hp);
                let r = hp(base * face.sup_offset / face.upem).min(limit);
                (size, r as f64 / 2.0)
            }
            VertAlign::Subscript => {
                let size = hp(base * face.sub_size / face.upem).max(2) as f64 / 2.0;
                let (_, d, _) = metrics(face, base);
                let (_, dsub, _) = metrics(face, size);
                let off = hp(base * face.sub_offset / face.upem);
                let limit = hp(d) - hp(dsub);
                let drop = if limit > 0 { off.min(limit.max(0)).max(0) } else { off.max(0) };
                (size, -(drop as f64) / 2.0)
            }
        };
        raise += run.position;
        (size, raise)
    }

    fn new_piece(&self, kind: Kind, run: &Run, face: Option<Rc<Face>>, size: f64, raise: f64) -> NewPiece {
        let (metric_size, shift) = if run.vert_align == VertAlign::Baseline { (size, raise) } else { (run.size, run.position) };
        let (mut ascent, mut descent) = match &face {
            Some(f) => {
                let (a, d, e) = metrics_at(f, metric_size, self.em1000 && run.east_asia_named);
                (a + e, d)
            }
            None => (0.0, 0.0),
        };
        ascent = (ascent + shift).max(0.0);
        descent = (descent - shift).max(0.0);
        NewPiece {
            kind,
            style: PieceStyle { run: run.clone(), face, size, raise, squeeze_avg: 0.0 },
            ascent,
            descent,
            width: 0.0,
            break_after: false,
            soft_hyphen: false,
            object: None,
        }
    }

    fn flush_text(&mut self, pend: &mut String, runs: &mut Vec<(usize, Run)>, units: &mut Vec<u32>) -> Result<(), Yield> {
        let Some(r) = runs.first().map(|(_, r)| r.clone()) else { return Ok(()) };
        let first = self.pieces.len();
        self.text(pend, &r)?;
        let mut restyled: Vec<(u32, usize, u32)> = Vec::new();
        let (mut c, mut seg) = (0usize, 0usize);
        for i in first..self.pieces.len() {
            let u = units.get(c).or(units.last()).copied().unwrap_or(0);
            self.pieces[i].src = Some(Unit(u));
            while runs.get(seg + 1).is_some_and(|(at, _)| *at <= c) {
                seg += 1;
            }
            if seg > 0 {
                let old = self.pieces[i].style;
                let style = match restyled.iter().find(|(s, k, _)| *s == old && *k == seg) {
                    Some(&(_, _, new)) => new,
                    None => {
                        let mut st = self.styles[old as usize].clone();
                        let own = &runs[seg].1;
                        st.run = if Rc::ptr_eq(&st.run, &r) { own.clone() } else { Rc::new(RunStyle { rev: own.rev.clone(), ..(*st.run).clone() }) };
                        let new = self.style_index(st);
                        restyled.push((old, seg, new));
                        new
                    }
                };
                self.pieces[i].style = style;
            }
            let (a, b) = self.pieces[i].text;
            c += self.text[a as usize..b as usize].chars().count();
        }
        pend.clear();
        runs.clear();
        units.clear();
        Ok(())
    }

    fn text(&mut self, text: &str, run: &Run) -> Result<(), Yield> {
        if run.small_caps && !run.caps {
            let mut small = (**run).clone();
            small.small_caps = false;
            small.caps = true;
            small.size = ((hp(run.size) * 4 + 2) / 5).clamp(2, 3276) as f64 / 2.0;
            let small = Rc::new(small);
            let mut plain = (**run).clone();
            plain.small_caps = false;
            let plain = Rc::new(plain);
            let mut seg = String::new();
            let mut seg_small = false;
            for c in text.chars() {
                let is_small =
                    c.is_lowercase() || matches!(c, ' ' | '\u{3000}' | '\u{2005}' | '\u{300}' | '\u{301}' | '\u{303}' | '\u{309}' | '\u{323}');
                if is_small != seg_small && !seg.is_empty() {
                    self.text(&seg, if seg_small { &small } else { &plain })?;
                    seg.clear();
                }
                seg_small = is_small;
                seg.push(c);
            }
            if !seg.is_empty() {
                self.text(&seg, if seg_small { &small } else { &plain })?;
            }
            return Ok(());
        }
        let mut shown: String = if run.caps { text.to_uppercase() } else { text.to_string() };
        if shown.chars().any(|c| symbol_unicode(family_for(c, run), c) != c) {
            shown = shown
                .chars()
                .map(|c| {
                    let fam = family_for(c, run);
                    let m = symbol_unicode(fam, c);
                    let installed = self.face(fam, run).is_some_and(|f| f.family.eq_ignore_ascii_case(fam));
                    if installed { c } else { m }
                })
                .collect();
        }
        let mut stretch_start = 0;
        let mut stretch_face: Option<Rc<Face>> = None;
        let mut idx = Vec::new();
        let mut faces: Vec<(*const str, Option<Rc<Face>>)> = Vec::new();
        for (i, c) in shown.char_indices() {
            let mut fam = family_for(c, run);
            let mut face = None;
            for _ in 0..2 {
                face = match faces.iter().find(|(f, _)| std::ptr::eq(*f, fam)) {
                    Some((_, f)) => f.clone(),
                    None => {
                        let f = self.face(fam, run);
                        faces.push((fam as *const str, f.clone()));
                        f
                    }
                };
                let cp = c as u32;
                if run.hint_east_asia
                    && !is_east_asian(cp)
                    && shared_with_east_asia(cp)
                    && std::ptr::eq(fam, run.east_asia.as_str())
                    && face.as_ref().is_some_and(|f| !f.east_asian)
                {
                    fam = if run.east_asia == "Times New Roman" {
                        &run.h_ansi
                    } else if self.fonts.resolves(EAST_ASIAN_PUNCTUATION, run.bold, run.italic) {
                        EAST_ASIAN_PUNCTUATION
                    } else {
                        if let Some(f) = self.fonts.east_asian_punctuation(run.bold, run.italic).filter(|f| f.has_glyph(c)) {
                            face = Some(f);
                        }
                        break;
                    };
                    continue;
                }
                break;
            }
            if let Some(f) = &face
                && !f.has_glyph(c)
                && !c.is_whitespace()
                && c != '\u{AD}'
                && c != '\u{2011}'
                && let Some(fb) = self.fonts.fallback(c, run.bold, run.italic)
            {
                face = Some(fb);
            }
            let same = match (&face, &stretch_face) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if i > 0 && !same {
                idx.push((stretch_start, i, stretch_face.take()));
                stretch_start = i;
            }
            stretch_face = face;
        }
        if stretch_start < shown.len() {
            idx.push((stretch_start, shown.len(), stretch_face));
        }
        for (a, b, face) in idx {
            crate::prof::time("stretch", || self.shape_stretch(&shown[a..b], run, face));
        }
        Ok(())
    }

    fn shape_stretch(&mut self, s: &str, run: &Run, face: Option<Rc<Face>>) {
        let Some(face) = face else { return };
        let (size, raise) = self.vertical(run, &face);
        let kern = run.kern > 0.0 && size >= run.kern - 1e-9;
        let shaped_text: String = s.chars().map(|c| if c == '\u{2011}' { '-' } else { c }).collect();
        let ligatures = if run.spacing != 0.0 { run.ligatures & 16 } else { run.ligatures };
        let glyphs = face.shape(&shaped_text, kern, ligatures);
        let scale = face.scale(size) * run.scale / 100.0;
        let mut starts: Vec<u32> = glyphs.iter().map(|g| g.cluster).collect();
        starts.sort_unstable();
        starts.dedup();
        let src_chars: Vec<char> = s.chars().collect();
        let shaped_map: Vec<u32> = shaped_text.char_indices().map(|(i, _)| i as u32).collect();
        let mut by_cluster: Vec<usize> = (0..glyphs.len()).collect();
        by_cluster.sort_by_key(|&i| glyphs[i].cluster);
        let mut gi = 0;
        let mut cluster_glyphs: Vec<Glyph> = Vec::new();
        let mut chars = String::new();
        for (k, &cs) in starts.iter().enumerate() {
            let ce = starts.get(k + 1).copied().unwrap_or(shaped_text.len() as u32);
            let (c0, c1) = (shaped_map.partition_point(|&b| b < cs), shaped_map.partition_point(|&b| b < ce));
            chars.clear();
            chars.extend(&src_chars[c0..c1]);
            let g0 = gi;
            while gi < by_cluster.len() && glyphs[by_cluster[gi]].cluster == cs {
                gi += 1;
            }
            cluster_glyphs.clear();
            cluster_glyphs.extend(by_cluster[g0..gi].iter().map(|&i| glyphs[i]));
            let first = chars.chars().next().unwrap_or(' ');
            let kind = if first == ' ' || first == '\u{3000}' || first == '\u{2002}' || first == '\u{2003}' { Kind::Space } else { Kind::Text };
            let n_chars = c1 - c0;
            let mut p = self.new_piece(kind, run, Some(face.clone()), size, raise);
            p.width = if self.em1000 && run.east_asia_named {
                let at = |g: &Glyph| width_at_em1000(face.advance(g.id), face.upem, size) + (g.advance - face.advance(g.id)) * face.scale(size);
                cluster_glyphs.iter().map(at).sum::<f64>() * run.scale / 100.0
            } else {
                cluster_glyphs.iter().map(|g| g.advance).sum::<f64>() * scale
            } + run.spacing * n_chars as f64;
            p.break_after = kind == Kind::Space || first == '-' || first == '\u{2010}' || first == '\u{2013}';
            if first == '\u{AD}' {
                p.soft_hyphen = true;
                p.break_after = true;
                p.width = 0.0;
            }
            if chars.contains('\u{2011}') {
                let shown = chars.replace('\u{2011}', "-");
                self.push(p, &shown, &cluster_glyphs);
            } else {
                let text = std::mem::take(&mut chars);
                self.push(p, &text, &cluster_glyphs);
                chars = text;
            }
        }
    }

    fn atom(&mut self, p: NewPiece, text: &str) {
        self.push(p, text, &[]);
    }

    fn paragraph(&mut self, para: &Paragraph, space: &ShapeSpace) -> Result<(), Yield> {
        if let Some(l) = &para.list {
            let start = self.pieces.len();
            let mut unkerned = (*l.run).clone();
            unkerned.kern = 0.0;
            self.text(&l.text, &Rc::new(unkerned))?;
            for p in &mut self.pieces[start..] {
                p.kind = Kind::ListNumber;
                p.break_after = false;
            }
            match l.suff.as_str() {
                "space" => {
                    let s = self.pieces.len();
                    self.text(" ", &l.run)?;
                    for i in s..self.pieces.len() {
                        let size = self.styles[self.pieces[i].style as usize].size;
                        let p = &mut self.pieces[i];
                        p.break_after = false;
                        p.width = LIST_SUFFIX_SPACE_EM * size * l.run.scale / 100.0 + l.run.spacing;
                    }
                }
                "nothing" => {}
                _ => {
                    let f = self.face(&l.run.ascii, &l.run);
                    let mut t = self.new_piece(Kind::Tab, &l.run, f, l.run.size, 0.0);
                    t.ascent = 0.0;
                    t.descent = 0.0;
                    self.atom(t, "\t");
                }
            }
        }
        let (mut field, mut note) = (0usize, 0usize);
        let mut at: u32 = 0;
        let shown = crate::docx::fields::layout_shown(&para.inlines);
        let mut result_of: Vec<Option<u32>> = vec![None; para.inlines.len()];
        {
            let starts: Vec<u32> = para.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
            for s in crate::docx::fields::spans(&para.inlines) {
                if let (Inline::FieldBegin { view: crate::docx::FieldView::Result, .. }, Some(sep)) = (&para.inlines[s.begin], s.sep) {
                    for k in sep + 1..s.end {
                        result_of[k].get_or_insert(starts[s.begin]);
                    }
                }
            }
        }
        let mut pend = String::new();
        let mut pend_runs: Vec<(usize, Run)> = Vec::new();
        let mut pend_units: Vec<u32> = Vec::new();
        let view = space.view;
        if let Some(f) = space.float.clone().filter(|f| !para.inlines.iter().any(|i| matches!(i, Inline::Float(g, _) if g.id == f.id))) {
            let run = para.mark_in(view);
            let mut p = self.new_piece(Kind::Anchor, run, None, run.size, 0.0);
            p.object = Some(Object::Float(f));
            self.atom(p, "");
        }
        let expanded = space.expansion.as_ref().and_then(|e| {
            crate::docx::fields::spans(&para.inlines)
                .into_iter()
                .find(|s| matches!(para.inlines[s.begin], Inline::FieldBegin { id, .. } if id == e.field))
                .map(|s| (s.begin..=s.end, e.clone()))
        });
        for (k, inl) in para.inlines.iter().enumerate() {
            let evaluated = matches!(inl, Inline::FieldBegin { kind: Some(_), .. });
            if let Some((span, e)) = expanded.as_ref().filter(|(s, _)| s.contains(&k)) {
                if evaluated {
                    field += 1;
                }
                if k == *span.start()
                    && let Some(run) = inl.run()
                {
                    self.flush_text(&mut pend, &mut pend_runs, &mut pend_units)?;
                    let mut p = self.new_piece(Kind::Object, run_in(run, view), None, run.size, 0.0);
                    (p.width, p.ascent, p.descent) = (e.size.0, e.size.1, 0.0);
                    p.object = Some(Object::Expansion(e.clone()));
                    p.break_after = true;
                    if let Some(q) = self.pieces.last_mut().filter(|q| q.kind != Kind::ListNumber) {
                        q.break_after = true;
                    }
                    self.atom(p, "\u{FFFC}");
                    if let Some(last) = self.pieces.last_mut() {
                        last.src = Some(Unit(at));
                    }
                }
                at += inl.units() as u32;
                continue;
            }
            if inl.run().is_some_and(|r| r.hidden || view.hides(&r.rev)) || shown[k] == Shown::Not {
                match inl {
                    _ if evaluated => field += 1,
                    Inline::NoteRef { .. } | Inline::NoteMark(..) => note += 1,
                    _ => {}
                }
                at += inl.units() as u32;
                continue;
            }
            if let (Inline::Text(t, run), Shown::Itself) = (inl, &shown[k]) {
                let run = run_in(run, view);
                if pend_runs.first().is_some_and(|(_, p)| !shapes_alike(p, run)) {
                    self.flush_text(&mut pend, &mut pend_runs, &mut pend_units)?;
                }
                if !pend_runs.last().is_some_and(|(_, p)| Rc::ptr_eq(p, run) || **p == **run) {
                    pend_runs.push((pend_units.len(), run.clone()));
                }
                pend.push_str(t);
                match result_of[k] {
                    Some(b) => pend_units.extend((0..t.chars().count()).map(|_| b)),
                    None => pend_units.extend((0..t.chars().count() as u32).map(|c| at + c)),
                }
                at += inl.units() as u32;
                continue;
            }
            if matches!(inl, Inline::Bookmark(_) | Inline::Range(_) | Inline::CommentRef { .. } | Inline::ControlStart(_) | Inline::ControlEnd(_)) {
                continue;
            }
            self.flush_text(&mut pend, &mut pend_runs, &mut pend_units)?;
            let first = self.pieces.len();
            if let Shown::Value(v) = &shown[k] {
                let text = if evaluated {
                    field += 1;
                    space.fields.get(field - 1).cloned().flatten().unwrap_or_else(|| v.clone())
                } else {
                    v.clone()
                };
                if let (false, Some(run)) = (text.is_empty(), inl.run()) {
                    self.text(&text, run_in(run, view))?;
                }
            }
            let run = inl.run().map(|r| run_in(r, view));
            match (inl, run) {
                (Inline::Text(t, _), Some(run)) => self.text(t, run)?,
                (Inline::Tab(_), Some(run)) => {
                    let f = self.face(&run.ascii, run);
                    let p = self.new_piece(Kind::Tab, run, f, run.size, run.position);
                    self.atom(p, "\t");
                }
                (Inline::PTab(t, _), Some(run)) => {
                    let f = self.face(&run.ascii, run);
                    let mut p = self.new_piece(Kind::Tab, run, f, run.size, run.position);
                    p.object = Some(Object::PTab(*t));
                    self.atom(p, "\t");
                }
                (Inline::Break(BreakKind::Page | BreakKind::Column, _), Some(_)) if space.in_cell => {}
                (Inline::Break(k, _), Some(run)) => {
                    let kind = match k {
                        BreakKind::Line => Kind::LineBreak,
                        BreakKind::Page => Kind::PageBreak,
                        BreakKind::Column => Kind::ColumnBreak,
                    };
                    let f = self.face(&run.ascii, run);
                    let p = self.new_piece(kind, run, f, run.size, 0.0);
                    self.atom(p, "");
                }
                (Inline::Sym { ch, font, control, .. }, Some(run)) => {
                    let mut r = (**run).clone();
                    r.ascii = font.clone();
                    r.h_ansi = font.clone();
                    if control.is_some() && run.hint_east_asia {
                        r.east_asia = font.clone();
                    }
                    self.text(&ch.to_string(), &Rc::new(r))?;
                }
                (Inline::Picture(pic, _), Some(run)) => {
                    let mut p = self.new_piece(Kind::Object, run, None, run.size, 0.0);
                    let [l, t, r, b] = pic.effect;
                    p.width = pic.w + l + r;
                    p.ascent = (pic.h + t + b + run.position).max(0.0);
                    p.descent = (-run.position).max(0.0);
                    p.object = Some(Object::Picture(pic.clone()));
                    p.break_after = true;
                    if let Some(q) = self.pieces.last_mut().filter(|q| q.kind != Kind::ListNumber) {
                        q.break_after = true;
                    }
                    self.atom(p, "\u{FFFC}");
                }
                (Inline::Float(f, _), Some(_)) if space.hide == Some(f.id) => {}
                (Inline::Float(f, _), Some(run)) => {
                    let mut p = self.new_piece(Kind::Anchor, run, None, run.size, 0.0);
                    let previewed = space.float.clone().filter(|p| p.id == f.id);
                    p.object = Some(Object::Float(previewed.unwrap_or_else(|| Rc::new((**f).clone()))));
                    self.atom(p, "");
                }
                (Inline::NoteRef { kind, id, custom, .. }, Some(run)) => {
                    let number = space.notes.get(note).cloned().unwrap_or_default();
                    note += 1;
                    if !*custom {
                        let start = self.pieces.len();
                        self.text(&number, run)?;
                        if start < self.pieces.len() {
                            self.objects.push(Object::Note(*kind, *id));
                            self.pieces[start].object = Some((self.objects.len() - 1) as u32);
                        }
                    }
                }
                (Inline::NoteMark(_, _, _), Some(run)) => {
                    let number = space.notes.get(note).cloned().unwrap_or_default();
                    note += 1;
                    self.text(&number, run)?;
                }
                (Inline::Separator { continuation, .. }, Some(run)) => {
                    let f = self.face(&run.ascii, run);
                    let kind = if *continuation { Kind::ContSeparator } else { Kind::Separator };
                    let p = self.new_piece(kind, run, f, run.size, 0.0);
                    self.atom(p, "");
                }
                _ => {}
            }
            let units = inl.units() as u32;
            for i in first..self.pieces.len() {
                self.pieces[i].src = Some(Unit(at));
            }
            at += units;
        }
        self.flush_text(&mut pend, &mut pend_runs, &mut pend_units)?;
        if let Some(dd) = &space.balance
            && (dd.is_empty() || self.fonts.face(dd, None, false, false).is_some_and(|f| f.east_asian))
        {
            self.balance_spaces();
        }
        if space.compress {
            for k in 0..self.pieces.len() {
                let p = &self.pieces[k];
                if p.break_after
                    && matches!(self.text[p.text.0 as usize..].chars().next(), Some('-' | '\u{2013}'))
                    && self.east_asia_face(p.style).is_some()
                {
                    self.pieces[k].break_after = false;
                }
            }
            for s in 0..self.styles.len() {
                self.styles[s].squeeze_avg = self.east_asia_face(s as u32).map_or(0.0, |f| f.avg_width);
            }
        }
        Ok(())
    }

    fn east_asia_face(&self, style: u32) -> Option<Rc<Face>> {
        let st = &self.styles[style as usize];
        let named = || self.face(&st.run.east_asia, &st.run).filter(|f| f.east_asian);
        named().or_else(|| if st.run.east_asia_unset { st.face.clone() } else { None })
    }

    fn balance_spaces(&mut self) {
        fn text<'b>(b: &'b Builder, k: usize) -> &'b str {
            &b.text[b.pieces[k].text.0 as usize..b.pieces[k].text.1 as usize]
        }
        let space = |b: &Self, k: usize| matches!(text(b, k), " " | "\u{a0}");
        let east = |b: &Self, k: Option<usize>, before: bool| {
            let Some(k) = k else { return true };
            let t = text(b, k);
            let c = if before { t.chars().last() } else { t.chars().next() };
            let run = &b.styles[b.pieces[k].style as usize].run;
            space(b, k)
                || b.pieces[k].kind == Kind::Text
                    && c.is_some_and(|c| is_east_asian(c as u32) || run.hint_east_asia && shared_with_east_asia(c as u32))
        };
        let n = self.pieces.len();
        for k in 0..n {
            if !space(self, k) || !(east(self, k.checked_sub(1), true) || east(self, (k + 1 < n).then_some(k + 1), false)) {
                continue;
            }
            let st = &self.styles[self.pieces[k].style as usize];
            let named = self.face(&st.run.east_asia, &st.run).is_some_and(|f| f.east_asian);
            let em = if named {
                Some(0.5)
            } else if st.run.east_asia_unset {
                st.face.as_ref().map(|f| f.avg_width)
            } else {
                None
            };
            if let Some(em) = em {
                self.pieces[k].width = em * st.size * st.run.scale / 100.0 + st.run.spacing;
            }
        }
    }

    fn finish(mut self) -> ShapedPara {
        let text = |p: &Piece| &self.text[p.text.0 as usize..p.text.1 as usize];
        let n = self.pieces.len();
        let mut a = 0;
        while a < n {
            if text(&self.pieces[a]) != " " {
                a += 1;
                continue;
            }
            let mut b = a;
            while b < n && text(&self.pieces[b]) == " " {
                b += 1;
            }
            let nbsp = |k: usize| self.pieces.get(k).is_some_and(|p| text(p) == "\u{a0}");
            if (a > 0 && nbsp(a - 1)) || nbsp(b) {
                for p in &mut self.pieces[a..b] {
                    p.break_after = false;
                }
            }
            a = b;
        }
        ShapedPara {
            pieces: self.pieces.into_boxed_slice(),
            styles: self.styles.into_boxed_slice(),
            glyph_ids: self.glyph_ids.into_boxed_slice(),
            glyph_advances: self.glyph_advances.into_boxed_slice(),
            glyph_dx: self.glyph_dx.into_boxed_slice(),
            text: self.text.into_boxed_str(),
            objects: self.objects.into_boxed_slice(),
            join: None,
            em1000: self.em1000,
        }
    }
}

fn shapes_alike(a: &Run, b: &Run) -> bool {
    let RunStyle {
        ascii,
        h_ansi,
        east_asia,
        cs,
        east_asia_unset,
        east_asia_named,
        hint_east_asia,
        size,
        bold,
        italic,
        caps,
        small_caps,
        strike,
        dstrike,
        hidden,
        underline,
        color,
        highlight,
        shading,
        vert_align,
        position,
        spacing,
        scale,
        kern,
        ligatures,
        em,
        toc,
        lang,
        style_id,
        direct: _,
        rev: _,
        categories,
    } = &**a;
    Rc::ptr_eq(a, b)
        || (*ascii == b.ascii
            && *h_ansi == b.h_ansi
            && *east_asia == b.east_asia
            && *cs == b.cs
            && *east_asia_unset == b.east_asia_unset
            && *east_asia_named == b.east_asia_named
            && *hint_east_asia == b.hint_east_asia
            && *size == b.size
            && *bold == b.bold
            && *italic == b.italic
            && *caps == b.caps
            && *small_caps == b.small_caps
            && *strike == b.strike
            && *dstrike == b.dstrike
            && *hidden == b.hidden
            && *underline == b.underline
            && *color == b.color
            && *highlight == b.highlight
            && *shading == b.shading
            && *vert_align == b.vert_align
            && *position == b.position
            && *spacing == b.spacing
            && *scale == b.scale
            && *kern == b.kern
            && *ligatures == b.ligatures
            && *em == b.em
            && *toc == b.toc
            && *lang == b.lang
            && *style_id == b.style_id
            && *categories == b.categories)
}

// Shape cache
#[derive(Debug)]
pub struct ShapeKey {
    para: Paragraph,
    space: ShapeSpace,
    fonts: u64,
}

pub struct ShapeProbe<'a> {
    pub para: &'a Paragraph,
    pub space: &'a ShapeSpace,
    pub fonts: u64,
}

impl CacheKey for ShapeKey {
    type Probe<'a> = ShapeProbe<'a>;

    fn bucket(p: &ShapeProbe) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = super::cache::KeyHasher::default();
        p.para.id.hash(&mut h);
        p.para.inlines.len().hash(&mut h);
        for i in &p.para.inlines {
            if let Inline::Text(t, _) = i {
                t.hash(&mut h);
            }
        }
        h.finish()
    }

    fn matches(&self, p: &ShapeProbe) -> bool {
        self.fonts == p.fonts && self.space == *p.space && same_para(&self.para, p.para)
    }

    fn from_probe(p: &ShapeProbe) -> ShapeKey {
        ShapeKey { para: p.para.clone(), space: p.space.clone(), fonts: p.fonts }
    }
}

pub fn same_para(a: &Paragraph, b: &Paragraph) -> bool {
    let run = |x: &Run, y: &Run| Rc::ptr_eq(x, y) || **x == **y;
    a.id == b.id
        && (Rc::ptr_eq(&a.style, &b.style) || *a.style == *b.style)
        && run(&a.mark, &b.mark)
        && match (&a.list, &b.list) {
            (None, None) => true,
            (Some(x), Some(y)) => x.text == y.text && x.jc == y.jc && x.suff == y.suff && run(&x.run, &y.run),
            _ => false,
        }
        && a.inlines == b.inlines
}

pub fn shape(para: &Paragraph, space: &ShapeSpace, fonts: &Fonts, memos: &mut Memos) -> Result<Rc<ShapedPara>, Yield> {
    let probe = ShapeProbe { para, space, fonts: fonts.revision() };
    cached(memos, |c| &mut c.shaped, probe, ShapedPara::bytes, |m| shape_uncached(para, space, fonts, m))
}

pub fn shape_uncached(para: &Paragraph, space: &ShapeSpace, fonts: &Fonts, memos: &mut Memos) -> Result<ShapedPara, Yield> {
    let mut b = Builder {
        fonts,
        pieces: Vec::new(),
        styles: Vec::new(),
        glyph_ids: Vec::new(),
        glyph_advances: Vec::new(),
        glyph_dx: Vec::new(),
        text: String::new(),
        objects: Vec::new(),
        em1000: space.em1000,
    };
    super::job::unit("shaping call", || crate::prof::time("itemize", || b.paragraph(para, space)))?;
    memos.meter.unit_done();
    Ok(ShapedPara { join: space.join.clone(), ..b.finish() })
}

pub fn shape_space(
    para: &Paragraph,
    fields: &FieldValues,
    notes: &HashMap<(NoteKind, i64), String>,
    text: &TextEnv,
    preview: Option<&super::Preview>,
) -> ShapeSpace {
    let mut space = ShapeSpace {
        balance: text.default_east_asia.clone().filter(|_| text.balance_spaces),
        compress: text.compress_punctuation,
        em1000: text.compat == crate::docx::Compat::Legacy && text.word97_line_breaks,
        view: text.review.view,
        ..ShapeSpace::default()
    };
    let spans = crate::docx::fields::spans(&para.inlines);
    for (k, inl) in para.inlines.iter().enumerate() {
        match inl {
            Inline::FieldBegin { kind: Some(kind), .. } => {
                let code = spans.iter().find(|s| s.begin == k).map(|s| crate::docx::fields::code_text(&para.inlines, *s)).unwrap_or_default();
                space.fields.push(fields.value(kind, code.trim()))
            }
            Inline::NoteRef { kind, id, .. } | Inline::NoteMark(kind, id, _) => {
                space.notes.push(notes.get(&(*kind, *id)).cloned().unwrap_or_default())
            }
            _ => {}
        }
    }
    if let Some(v) = preview {
        if v.anchor == ParaId(para.id) {
            space.float = Some(Rc::new(v.float.clone()));
        } else if para.inlines.iter().any(|i| matches!(i, Inline::Float(f, _) if f.id == v.float.id)) {
            space.hide = Some(v.float.id);
        }
    }
    space
}

// Field values
#[derive(Clone, Default, Debug, PartialEq)]
pub struct FieldValues {
    pub page: Option<String>,
    pub num_pages: Option<String>,
    pub section: Option<String>,
    pub section_pages: Option<String>,
    pub page_refs: Option<Rc<HashMap<String, String>>>,
    pub style_refs: Vec<(String, String)>,
    pub page_fmt: Option<String>,
}

impl FieldValues {
    pub fn value(&self, kind: &FieldKind, code: &str) -> Option<String> {
        match kind {
            FieldKind::Page => self.page.clone().map(|p| format_field(&p, code, self.page_fmt.as_deref())),
            FieldKind::NumPages => self.num_pages.clone().map(|p| format_field(&p, code, None)),
            FieldKind::SectionPages => self.section_pages.clone().map(|p| format_field(&p, code, None)),
            FieldKind::Section => self.section.clone().map(|p| format_field(&p, code, None)),
            FieldKind::PageRef(name) => self
                .page_refs
                .as_ref()
                .map(|m| m.get(name).map_or_else(|| "Error! Bookmark not defined.".to_string(), |p| format_field(p, code, None))),
            FieldKind::StyleRef(st) => {
                let key = crate::docx::fields::style_key(st);
                self.style_refs.iter().find(|(k, _)| *k == key).map(|(_, t)| t.clone())
            }
            _ => None,
        }
    }
}

pub fn para_id(p: &Paragraph) -> ParaId {
    ParaId(p.id)
}

pub fn shape_run(text: &str, run: &Run, fonts: &Fonts) -> ShapedPara {
    let mut b = Builder {
        fonts,
        pieces: Vec::new(),
        styles: Vec::new(),
        glyph_ids: Vec::new(),
        glyph_advances: Vec::new(),
        glyph_dx: Vec::new(),
        text: String::new(),
        objects: Vec::new(),
        em1000: false,
    };
    let _ = b.text(text, run);
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::review::{AuthorId, MarkKind, RevisionId, RevisionMark, RunRevisions};
    use crate::layout::cache::LayoutCache;
    use crate::layout::job::Meter;
    use crate::layout::tests::{fonts, plain_doc};

    #[test]
    fn revisions_dont_split_a_stretch() {
        let Some(doc) = plain_doc(&["one".into()]) else { return };
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { unreachable!() };
        let mut p = p.clone();
        let base = p.inlines.iter().find_map(Inline::run).expect("a run").clone();
        let mark = Rc::new(RevisionMark { id: RevisionId(1), author: AuthorId(0), date: None, kind: MarkKind::Insert, move_name: None });
        let inserted = Rc::new(RunStyle { rev: RunRevisions { ins: Some(mark), ..Default::default() }, ..(*base).clone() });
        p.inlines = vec![Inline::Text("ab".into(), base.clone()), Inline::Text("cd".into(), inserted), Inline::Text("ef".into(), base)];
        let (fonts, mut cache, mut meter) = (fonts(), LayoutCache::new(), Meter::unlimited());
        let mut memos = Memos { cache: &mut cache, job: None, meter: &mut meter };
        crate::prof::take();
        let shaped = shape_uncached(&p, &ShapeSpace::default(), &fonts, &mut memos).expect("unlimited");
        let stretches = crate::prof::take().iter().find(|t| t.0 == "stretch").map_or(0, |t| t.2);
        assert_eq!(stretches, 1);
        let inserted: Vec<bool> = (0..shaped.len()).map(|i| shaped.run(i).rev.ins.is_some()).collect();
        assert_eq!(inserted, [false, false, true, true, false, false]);
    }
}
