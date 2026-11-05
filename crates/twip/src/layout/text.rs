//! Paragraph content → shaped pieces: one per glyph cluster, tab, break or object, with widths and
//! vertical metrics. Width-free, so a paragraph is shaped once and broken at any width.
//!
//! Shaping applies font slots, fallback, small caps, FONT-3, FONT-7 to FONT-10, LIST-7, BREAK-7, FIELD-1 and NOTE-4.
//! The storage is compact: one glyph array, one advance array and one text buffer per paragraph, with pieces as
//! ranges into them.

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

/// The height an emphasis mark adds, per point of the text's size (LINE-15).
pub(crate) const EM_MARK: f64 = 3.4 / 11.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// A space-like character: a break opportunity after it; hangs at line end (BREAK-2).
    Space,
    Tab,
    LineBreak,
    PageBreak,
    ColumnBreak,
    /// Inline picture: a glyph whose bottom sits on the baseline (FLOAT-5).
    Object,
    /// Anchored float: no width.
    Anchor,
    /// The list number (LIST-1 to LIST-7).
    ListNumber,
    /// A note separator's rule (NOTE-3): no width; drawn 144pt long.
    Separator,
    /// The continuation separator: 144pt in compat 15, the text width in compat <= 14 (NOTE-3).
    ContSeparator,
}

/// What a piece's style index names: everything pieces of one shaped stretch share.
#[derive(Clone, Debug)]
pub struct PieceStyle {
    pub run: Run,
    pub face: Option<Rc<Face>>,
    /// Effective size (after superscript/subscript, FONT-8).
    pub size: f64,
    /// Baseline shift, points (+ = up).
    pub raise: f64,
    /// With compressPunctuation, an East Asian run's average character width (em fraction, OS/2 xAvgCharWidth; 0
    /// otherwise): a single space of it can shrink to half that when its line squeezes a word in (BREAK-9, `fit_line`).
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

/// A piece's object: what an `Object`, `Anchor` or note reference piece stands for.
#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Picture(Picture),
    Float(Rc<Float>),
    /// A footnote/endnote reference: the note it names (set on the first piece of its number).
    Note(NoteKind, i64),
    /// A `w:ptab` tab piece's position.
    PTab(crate::docx::PTab),
    /// A reference field shown as its sub-template's body ([`super::expansion`]).
    Expansion(Rc<super::expansion::Expansion>),
}

/// One piece: compact, with ranges into its paragraph's arrays.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: Kind,
    /// A break may follow this piece (after spaces, hyphens, soft hyphens).
    pub break_after: bool,
    /// A soft hyphen: shows "-" only when a line breaks after it.
    pub soft_hyphen: bool,
    pub style: u32,
    /// Natural width, points.
    pub width: f64,
    /// Ascent incl. external leading (A + E) and descent (D), points, after raising/lowering (FONT-7).
    pub ascent: f64,
    pub descent: f64,
    /// Glyphs (font units, origins relative to the piece start) and text, as ranges into the paragraph's arrays.
    pub glyphs: (u32, u32),
    pub text: (u32, u32),
    /// Unit offset of the piece's first character in its paragraph; `None` for list numbers and their suffix.
    pub src: Option<Unit>,
    pub object: Option<u32>,
}

/// A paragraph shaped: its pieces and the arrays they index into.
#[derive(Debug, PartialEq)]
pub struct ShapedPara {
    pub pieces: Box<[Piece]>,
    pub styles: Box<[PieceStyle]>,
    pub glyph_ids: Box<[u32]>,
    pub glyph_advances: Box<[f64]>,
    pub glyph_dx: Box<[f64]>,
    pub text: Box<str>,
    pub objects: Box<[Object]>,
    /// A joined paragraph's sources ([`ShapeSpace::join`], REV-6): piece sources are its units.
    pub join: Option<Rc<super::join::JoinMap>>,
    /// FONT-18: [`ShapeSpace::em1000`].
    pub em1000: bool,
}

impl ShapedPara {
    /// A run's font metrics ([`metrics_at`]): at a 1000-unit em for the runs FONT-18 picks.
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

    /// Piece `i`'s glyphs: (id, advance, x offset), font units.
    pub fn glyphs(&self, i: usize) -> impl Iterator<Item = (u32, f64, f64)> + '_ {
        let (a, b) = self.pieces[i].glyphs;
        (a as usize..b as usize).map(|k| (self.glyph_ids[k], self.glyph_advances[k], self.glyph_dx[k]))
    }

    pub fn object(&self, i: usize) -> Option<&Object> {
        self.pieces[i].object.map(|k| &self.objects[k as usize])
    }

    /// The note piece `i` references (NOTE-5: notes are placed with the line that references them).
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

    /// Estimated heap bytes (cache budgets).
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

/// Font metrics in points for one face at one size: ascent, descent and line gap, on the layout-unit grid. The ascent
/// with the gap and the descent are each rounded once (UNIT-4, LINE-1); the gap is split off the rounded sum.
pub fn metrics(face: &Face, size: f64) -> (f64, f64, f64) {
    metrics_at(face, size, false)
}

/// [`metrics`], or with `em1000` (FONT-18) from the metrics at a 1000-unit em, each rounded to a whole unit first.
pub fn metrics_at(face: &Face, size: f64, em1000: bool) -> (f64, f64, f64) {
    let (a, d, g, upem) = if em1000 {
        let r = |v: f64| (v * 1000.0 / face.upem).round();
        (r(face.ascent), r(face.descent), r(face.leading), 1000.0)
    } else {
        (face.ascent, face.descent, face.leading, face.upem)
    };
    let above = Lu::from_font(a + g, size, upem);
    let gap = Lu::from_font(g, size, upem);
    let below = Lu::from_font(d, size, upem);
    (Lu(above.0 - gap.0).to_pt(), below.to_pt(), gap.to_pt())
}

/// FONT-18: a glyph's width at a 1000-unit em: its advance rounded to 1/1000 em, then down to layout units, in points.
fn width_at_em1000(advance: f64, upem: f64, size: f64) -> f64 {
    let w = (advance * 1000.0 / upem).round();
    (w * 2.0 * size * 2048.0 / 1000.0).floor() / super::units::LU_PER_PT
}

/// Half-points, rounded half away from zero (UNIT-5).
pub fn hp(x: f64) -> i64 {
    (2.0 * x).round() as i64
}

/// Script slot for a character (FONT-12).
/// A Symbol/Wingdings private-use character (U+F0xx), or its Latin-1 code (a symbol font shows the same glyph: bullet
/// "·" in Symbol, "§" in Wingdings, FONT-13), as the Unicode character it shows, for the text a page carries
/// (accessibility, copy, comparison) and the character drawn when the symbol font isn't installed; other characters
/// unchanged.
pub fn symbol_unicode(family: &str, c: char) -> char {
    let cp = c as u32;
    let code = match cp {
        0xF020..=0xF0FF => cp - 0xF000,
        0x20..=0xFF if matches!(family, "Symbol" | "Wingdings") => cp,
        _ => return c,
    };
    let mapped = match family {
        "Symbol" => match code {
            0xB7 => 0x2022, // bullet
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
            0xA7 => 0x25AA, // small square bullet
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

fn family_for(c: char, run: &RunStyle) -> &str {
    let cp = c as u32;
    if is_east_asian(cp) || (run.hint_east_asia && shared_with_east_asia(cp)) {
        return &run.east_asia;
    }
    if cp < 0x80 { &run.ascii } else { &run.h_ansi }
}

fn is_east_asian(cp: u32) -> bool {
    matches!(cp, 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// Characters `w:hint="eastAsia"` sends to the East Asian font (FONT-12).
fn shared_with_east_asia(cp: u32) -> bool {
    match cp {
        0xA1 | 0xA4 | 0xA7 | 0xA8 | 0xAA | 0xAD | 0xAF | 0xD7 | 0xF7 => true,
        0xB0..=0xB4 | 0xB6..=0xBA | 0xBC..=0xBF => true,
        0x200B..=0x200F | 0x2028..=0x202E => false,
        0x2000..=0x2BFF => true,
        _ => false,
    }
}

/// Builds a [`ShapedPara`]: the arrays, deduplicated styles, and pieces as they are appended.
struct Builder<'a> {
    fonts: &'a Fonts,
    pieces: Vec<Piece>,
    styles: Vec<PieceStyle>,
    glyph_ids: Vec<u32>,
    glyph_advances: Vec<f64>,
    glyph_dx: Vec<f64>,
    text: String,
    objects: Vec<Object>,
    /// FONT-18: [`ShapeSpace::em1000`].
    em1000: bool,
}

/// A piece being made, before its glyphs and text are appended to the builder's arrays.
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
        self.fonts.face(family, run.bold, run.italic)
    }

    fn style_index(&mut self, s: PieceStyle) -> u32 {
        // Stretches of one run and face share a style; look back a little rather than hash.
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

    /// Effective size, baseline shift and vertical-metric adjustment for a run (FONT-7 to FONT-10).
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
                // shortcut: FONT-10's limit isn't fitted for Calibri/Arial/Times.
                let drop = if limit > 0 { off.min(limit.max(0)).max(0) } else { off.max(0) };
                (size, -(drop as f64) / 2.0)
            }
        };
        raise += run.position;
        (size, raise)
    }

    /// A piece's vertical metrics (LINE-2, FONT-7) for a face at a size.
    fn new_piece(&self, kind: Kind, run: &Run, face: Option<Rc<Face>>, size: f64, raise: f64) -> NewPiece {
        // LINE-2: a superscript or subscript run counts its unreduced size's ascent and descent; its smaller size and
        // its raise or drop only place the glyphs (FONT-9, FONT-10).
        let (metric_size, shift) = if run.vert_align == VertAlign::Baseline { (size, raise) } else { (run.size, run.position) };
        let (mut ascent, mut descent) = match &face {
            Some(f) => {
                let (a, d, e) = metrics_at(f, metric_size, self.em1000 && run.east_asia_named);
                (a + e, d)
            }
            None => (0.0, 0.0),
        };
        // FONT-7: raised text grows the line above, lowered text below.
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

    /// Shapes the pending text stretch (see `paragraph`): its pieces' sources are their first characters' units, and
    /// each piece draws in the run its first character comes from (`runs`: each run's first character, in order).
    fn flush_text(&mut self, pend: &mut String, runs: &mut Vec<(usize, Run)>, units: &mut Vec<u32>) -> Result<(), Yield> {
        let Some(r) = runs.first().map(|(_, r)| r.clone()) else { return Ok(()) };
        let first = self.pieces.len();
        self.text(pend, &r)?;
        // Styles of the stretch's own run (or the runs shaping made of it, such as small caps) with another run's
        // revisions: (style, run index, restyled).
        let mut restyled: Vec<(u32, usize, u32)> = Vec::new();
        let (mut c, mut seg) = (0usize, 0usize);
        for i in first..self.pieces.len() {
            let u = units.get(c).or(units.last()).copied().unwrap_or(0);
            self.pieces[i].src = Some(Unit(u));
            while runs.get(seg + 1).is_some_and(|(at, _)| *at <= c) {
                seg += 1;
            }
            // shortcut: a cluster across two runs (a mark combining with the character before it) draws in the first.
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

    /// Shapes `text` in `run` and appends one piece per cluster.
    fn text(&mut self, text: &str, run: &Run) -> Result<(), Yield> {
        if run.small_caps && !run.caps {
            // Small caps (FONT-6).
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
                // Lower-case letters, every space and the Vietnamese tone marks are small; the rest keeps its size.
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
        // FONT-13: a code in a symbol font that isn't installed draws as the character it shows, in a face that has it.
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
        // Split into same-face stretches.
        let mut stretch_start = 0;
        let mut stretch_face: Option<Rc<Face>> = None;
        let mut idx = Vec::new();
        // The run's faces by family (family_for picks one of the run's few font slots): looked up once per run.
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
                // FONT-12: a hinted character whose East Asian font isn't one is drawn in MS Mincho, or in the hAnsi
                // font when that font is Times New Roman.
                let cp = c as u32;
                if run.hint_east_asia
                    && !is_east_asian(cp)
                    && shared_with_east_asia(cp)
                    && std::ptr::eq(fam, run.east_asia.as_str())
                    && face.as_ref().is_some_and(|f| !f.east_asian)
                {
                    fam = if run.east_asia == "Times New Roman" { &run.h_ansi } else { "MS Mincho" };
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
        // FONT-3: kerning at or above the w:kern threshold.
        let kern = run.kern > 0.0 && size >= run.kern - 1e-9;
        // A non-breaking hyphen draws as a hyphen; soft hyphens draw only at a line end.
        let shaped_text: String = s.chars().map(|c| if c == '\u{2011}' { '-' } else { c }).collect();
        // FONT-4: character spacing turns the ligatures off, not contextual alternates.
        let ligatures = if run.spacing != 0.0 { run.ligatures & 16 } else { run.ligatures };
        let glyphs = face.shape(&shaped_text, kern, ligatures);
        let scale = face.scale(size) * run.scale / 100.0;
        // Group glyphs by cluster; clusters are byte offsets into `shaped_text` (same offsets as `s` for 1:1 maps).
        let mut starts: Vec<u32> = glyphs.iter().map(|g| g.cluster).collect();
        starts.sort_unstable();
        starts.dedup();
        let src_chars: Vec<char> = s.chars().collect();
        let shaped_map: Vec<u32> = shaped_text.char_indices().map(|(i, _)| i as u32).collect();
        // Glyph indices in cluster order (a stable sort keeps each cluster's glyphs in visual order).
        let mut by_cluster: Vec<usize> = (0..glyphs.len()).collect();
        by_cluster.sort_by_key(|&i| glyphs[i].cluster);
        let mut gi = 0;
        let mut cluster_glyphs: Vec<Glyph> = Vec::new();
        let mut chars = String::new();
        for (k, &cs) in starts.iter().enumerate() {
            let ce = starts.get(k + 1).copied().unwrap_or(shaped_text.len() as u32);
            // Source characters of this cluster (the maps are sorted: binary search instead of a scan per cluster).
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
                // FONT-18: each glyph at a 1000-unit em, its kerning added in design units.
                let at = |g: &Glyph| width_at_em1000(face.advance(g.id), face.upem, size) + (g.advance - face.advance(g.id)) * face.scale(size);
                cluster_glyphs.iter().map(at).sum::<f64>() * run.scale / 100.0
            } else {
                cluster_glyphs.iter().map(|g| g.advance).sum::<f64>() * scale
            } + run.spacing * n_chars as f64;
            // BREAK-3: a break after an en dash too.
            p.break_after = kind == Kind::Space || first == '-' || first == '\u{2010}' || first == '\u{2013}';
            if first == '\u{AD}' {
                p.soft_hyphen = true;
                p.break_after = true;
                p.width = 0.0;
            }
            // BREAK-7: a non-breaking hyphen shows as an ordinary hyphen; it still doesn't break.
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

    /// A piece without glyphs.
    fn atom(&mut self, p: NewPiece, text: &str) {
        self.push(p, text, &[]);
    }

    fn paragraph(&mut self, para: &Paragraph, space: &ShapeSpace) -> Result<(), Yield> {
        // shortcut: a list numbering change shows the current number in Original too.
        if let Some(l) = &para.list {
            let start = self.pieces.len();
            // LIST-13: the number is a separate object, not kerned.
            let mut unkerned = (*l.run).clone();
            unkerned.kern = 0.0;
            self.text(&l.text, &Rc::new(unkerned))?;
            for p in &mut self.pieces[start..] {
                p.kind = Kind::ListNumber;
                p.break_after = false;
            }
            // LIST-2, LIST-6: the suffix in the number's font: a tab (default), one space (Arial-wide), or nothing.
            match l.suff.as_str() {
                "space" => {
                    let s = self.pieces.len();
                    self.text(" ", &l.run)?;
                    // LIST-2: the space is as wide as an Arial space at the number's size, whatever its font.
                    let arial = self.fonts.face("Arial", l.run.bold, l.run.italic);
                    let space = arial.as_ref().and_then(|f| Some(f.advance(f.glyph(' ')?) / f.upem));
                    for i in s..self.pieces.len() {
                        let size = self.styles[self.pieces[i].style as usize].size;
                        let p = &mut self.pieces[i];
                        p.break_after = false;
                        if let Some(em) = space {
                            p.width = em * size * l.run.scale / 100.0 + l.run.spacing;
                        }
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
        // Field markers and codes take no space unless a field shows its code; an evaluated field draws
        // its value at its begin marker (its result isn't drawn).
        let shown = crate::docx::fields::layout_shown(&para.inlines);
        // A field showing its result is one caret unit: its result's characters take its begin's unit, so
        // a click on them is before or after the field, as on a drawn value or label.
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
        // FONT-16: text in equal formatting is shaped as one stretch across what isn't drawn between it (field
        // markers and codes, control markers); each character keeps its own unit. Runs differing only in their
        // revisions shape as one stretch too (revision marks don't change the glyphs); each piece keeps its own run.
        let mut pend = String::new();
        let mut pend_runs: Vec<(usize, Run)> = Vec::new();
        let mut pend_units: Vec<u32> = Vec::new();
        let view = space.view;
        // Live layout: a previewed float re-anchored here is anchored at the paragraph's start, as the release puts it.
        if let Some(f) = space.float.clone().filter(|f| !para.inlines.iter().any(|i| matches!(i, Inline::Float(g, _) if g.id == f.id))) {
            let run = para.mark_in(view);
            let mut p = self.new_piece(Kind::Anchor, run, None, run.size, 0.0);
            p.object = Some(Object::Float(f));
            self.atom(p, "");
        }
        // A reference field shown as its sub-template's body: one object at its begin marker; the rest of the field
        // draws nothing.
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
                    // The object is the field's begin unit: a click on it is before or after the field.
                    if let Some(last) = self.pieces.last_mut() {
                        last.src = Some(Unit(at));
                    }
                }
                at += inl.units() as u32;
                continue;
            }
            // STYLE-7: hidden text (and anything else in a hidden run) takes no space and draws nothing; its units
            // still count, so the units after it keep their place, and fields and notes keep their numbering. REV-1:
            // so does revised content the markup view hides.
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
                // REV-2: the formatting the view shows.
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
                // Draws nothing and takes no units: the text on either side stays one stretch.
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
                // BREAK-10: a page or column break in a table cell is nothing.
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
                    // FONT-12: a checkbox's symbol in a run hinted East Asian goes through that slot; without the hint it
                    // draws in its own font.
                    if control.is_some() && run.hint_east_asia {
                        r.east_asia = font.clone();
                    }
                    self.text(&ch.to_string(), &Rc::new(r))?;
                }
                (Inline::Picture(pic, _), Some(run)) => {
                    let mut p = self.new_piece(Kind::Object, run, None, run.size, 0.0);
                    // FLOAT-5: the box is the extent and its effect extents, raised or lowered like text (FONT-7).
                    let [l, t, r, b] = pic.effect;
                    p.width = pic.w + l + r;
                    p.ascent = (pic.h + t + b + run.position).max(0.0);
                    p.descent = (-run.position).max(0.0);
                    p.object = Some(Object::Picture(pic.clone()));
                    // BREAK-3: a line may end before and after an inline picture.
                    p.break_after = true;
                    if let Some(q) = self.pieces.last_mut().filter(|q| q.kind != Kind::ListNumber) {
                        q.break_after = true;
                    }
                    self.atom(p, "\u{FFFC}");
                }
                // Live layout: a previewed float anchored elsewhere isn't laid out here.
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
                // Fields draw above, as what they show; markers draw nothing.
                _ => {}
            }
            // Sources: an atom's pieces all at its one unit (text goes through `flush_text`).
            let units = inl.units() as u32;
            for i in first..self.pieces.len() {
                self.pieces[i].src = Some(Unit(at));
            }
            at += units;
        }
        self.flush_text(&mut pend, &mut pend_runs, &mut pend_units)?;
        // FONT-14: only when the document default East Asian font is one (or the theme's empty slot).
        if let Some(dd) = &space.balance
            && (dd.is_empty() || self.fonts.face(dd, false, false).is_some_and(|f| f.east_asian))
        {
            self.balance_spaces();
        }
        if space.compress {
            // BREAK-8: no break after a hyphen-minus or an en dash in an East Asian run.
            for k in 0..self.pieces.len() {
                let p = &self.pieces[k];
                if p.break_after
                    && matches!(self.text[p.text.0 as usize..].chars().next(), Some('-' | '\u{2013}'))
                    && self.east_asia_face(p.style).is_some()
                {
                    self.pieces[k].break_after = false;
                }
            }
            // BREAK-9: such a run's spaces can shrink to half its average character width (see `fit_line`).
            for s in 0..self.styles.len() {
                self.styles[s].squeeze_avg = self.east_asia_face(s as u32).map_or(0.0, |f| f.avg_width);
            }
        }
        Ok(())
    }

    /// The font a piece's run counts as East Asian with (balanced spaces, compressed punctuation): its East Asian
    /// font when that is one, or its own when the East Asian slot is the theme's empty one (FONT-14, STYLE-8).
    fn east_asia_face(&self, style: u32) -> Option<Rc<Face>> {
        let st = &self.styles[style as usize];
        let named = || self.face(&st.run.east_asia, &st.run).filter(|f| f.east_asian);
        named().or_else(|| if st.run.east_asia_unset { st.face.clone() } else { None })
    }

    /// Widens the spaces next to a space, the paragraph's start or end, or a character in the East Asian font slot
    /// (FONT-14).
    fn balance_spaces(&mut self) {
        fn text<'b>(b: &'b Builder, k: usize) -> &'b str {
            &b.text[b.pieces[k].text.0 as usize..b.pieces[k].text.1 as usize]
        }
        let space = |b: &Self, k: usize| matches!(text(b, k), " " | "\u{a0}");
        // The neighbour's character next to the space: none (the paragraph's edge), a space, or in the East Asian slot.
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
            // Half the em with an East Asian font in the slot; the space's own font's average character with the
            // theme's empty slot.
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
        // BREAK-3: a run of spaces directly before or after a no-break space is no break opportunity.
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

/// Whether two runs shape alike: equal formatting, revisions aside.
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
            && *style_id == b.style_id)
}

/// What a shaped paragraph depends on: the paragraph (content and identity: its lines carry its id), the values of
/// its page-dependent fields and note numbers, and the font catalog's revision.
#[derive(Debug)]
pub struct ShapeKey {
    para: Paragraph,
    space: ShapeSpace,
    fonts: u64,
}

/// A [`ShapeKey`] borrowed.
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

/// Paragraph equality for caching: identity, style, mark, list label and content (source positions excluded: they
/// don't change layout). Shared `Rc`s compare by pointer first.
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

/// Shapes a paragraph (cached by content, [`ShapeSpace`] and font revision).
pub fn shape(para: &Paragraph, space: &ShapeSpace, fonts: &Fonts, memos: &mut Memos) -> Result<Rc<ShapedPara>, Yield> {
    let probe = ShapeProbe { para, space, fonts: fonts.revision() };
    cached(memos, |c| &mut c.shaped, probe, ShapedPara::bytes, |m| shape_uncached(para, space, fonts, m))
}

/// [`shape`] without the cache.
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
    // shortcut: shaping is one unit per paragraph until jobs keep partial pieces (JobMemo).
    super::job::unit("shaping call", || crate::prof::time("itemize", || b.paragraph(para, space)))?;
    memos.meter.unit_done();
    Ok(ShapedPara { join: space.join.clone(), ..b.finish() })
}

/// The field values and note numbers a paragraph's [`ShapeSpace`] holds, from the values the page or story gives, and
/// the previewed float when the paragraph anchors it.
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

/// Values page-dependent fields take where a paragraph is laid out (FIELD-1).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct FieldValues {
    pub page: Option<String>,
    pub num_pages: Option<String>,
    pub section: Option<String>,
    pub section_pages: Option<String>,
    /// PAGEREF: bookmark → page number, from the previous layout pass.
    pub page_refs: Option<Rc<HashMap<String, String>>>,
    /// STYLEREF in a header or footer: style key → the text found for this page.
    pub style_refs: Vec<(String, String)>,
    /// The section's page number format (`pgNumType/@w:fmt`).
    pub page_fmt: Option<String>,
}

impl FieldValues {
    /// The text a field shows: its computed value with its number and case switches (FIELD-3), or `None` for its cached
    /// result.
    pub fn value(&self, kind: &FieldKind, code: &str) -> Option<String> {
        match kind {
            FieldKind::Page => self.page.clone().map(|p| format_field(&p, code, self.page_fmt.as_deref())),
            FieldKind::NumPages => self.num_pages.clone().map(|p| format_field(&p, code, None)),
            FieldKind::SectionPages => self.section_pages.clone().map(|p| format_field(&p, code, None)),
            FieldKind::Section => self.section.clone().map(|p| format_field(&p, code, None)),
            // FIELD-2: a PAGEREF to a bookmark the document doesn't have shows the error text, once pages are known.
            FieldKind::PageRef(name) => self
                .page_refs
                .as_ref()
                .map(|m| m.get(name).map_or_else(|| "Error! Bookmark not defined.".to_string(), |p| format_field(p, code, None))),
            FieldKind::StyleRef(st) => {
                let key = crate::docx::fields::style_key(st);
                self.style_refs.iter().find(|(k, _)| *k == key).map(|(_, t)| t.clone())
            }
            // REF, SEQ, IF, DATE, body STYLEREF: evaluated after reading (docx::fields).
            _ => None,
        }
    }
}

/// The paragraph identity of a document paragraph.
pub fn para_id(p: &Paragraph) -> ParaId {
    ParaId(p.id)
}

/// `text` shaped in `run` alone (line numbers): its pieces, without a paragraph.
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
    // Shaping a run never yields (no budget is consulted).
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

    /// Text in runs differing only in their revisions shapes as one stretch, and each piece keeps its own run.
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
