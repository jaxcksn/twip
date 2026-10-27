//! The clipboard: copy keeps an internal fragment of the model (formatting, lists, tables) and writes
//! plain text and HTML; paste of a fragment from this editor keeps its formatting, re-resolving each run in its new
//! place and writing the properties that would otherwise change; anything else pastes as plain text in the
//! insertion point's formatting. Clipboard data → [`Fragment`] → insertion, so rich external paste can be added later
//! by converting other data to a fragment.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::docx::props::VertAlign;
use crate::docx::write::props::{Props, esc};
use crate::docx::{Block, Inline, Paragraph, Run, RunStyle, VMerge};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::Selection;
use super::step::{Step, para_len};

/// A copied piece of a document: blocks, the first and last possibly partial paragraphs (`open_end`: the last one's
/// mark wasn't copied, so it merges into the paragraph it is pasted into).
#[derive(Clone, Debug)]
pub struct Fragment {
    pub id: u64,
    pub blocks: Vec<Block>,
    pub open_end: bool,
    /// A copied cell block (its blocks are one table of its cells): pasted over the cells at the target.
    pub cells: Option<super::cell_clipboard::CellCopy>,
    /// The content controls its blocks hold, as they were when copied: a paste adds copies of them.
    pub controls: std::collections::HashMap<crate::docx::ControlId, crate::docx::Control>,
}

/// The inlines of `p` in units `a..b` (texts cut, other inlines whole).
fn slice(p: &Paragraph, a: Unit, b: Unit) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut at = 0u32;
    for i in &p.inlines {
        let n = i.units() as u32;
        let (s, e) = (at, at + n);
        at = e;
        if n == 0 {
            // A control's markers on the range's edges come too (one whose partner the range leaves out is dropped
            // by `drop_unpaired`).
            let control = matches!(i, Inline::ControlStart(_) | Inline::ControlEnd(_));
            if (s > a.0 && s < b.0) || (control && s >= a.0 && s <= b.0) {
                out.push(i.clone());
            }
            continue;
        }
        if e <= a.0 || s >= b.0 {
            continue;
        }
        match i {
            Inline::Text(t, r) => {
                let (from, to) = (a.0.max(s) - s, b.0.min(e) - s);
                out.push(Inline::Text(t.chars().skip(from as usize).take((to - from) as usize).collect(), r.clone()));
            }
            other => out.push(other.clone()),
        }
    }
    out
}

/// A deep copy with fresh identities (paragraphs, tables, floats) and no sources: content to insert.
pub(super) fn fresh(b: &Block) -> Block {
    match b {
        Block::Para(p) => {
            let mut p = p.clone();
            p.id = crate::docx::next_para_id();
            p.src = None;
            for i in &mut p.inlines {
                if let Inline::Float(f, _) = i {
                    f.id = crate::docx::next_para_id();
                    if let Some(tb) = &mut f.text_box {
                        tb.blocks = tb.blocks.iter().map(fresh).collect();
                    }
                }
            }
            Block::Para(p)
        }
        Block::Table(t) => {
            let mut t = t.clone();
            t.id = crate::docx::next_para_id();
            t.src = None;
            for r in &mut t.rows {
                r.src = None;
                for c in &mut r.cells {
                    c.blocks = c.blocks.iter().map(fresh).collect();
                }
            }
            Block::Table(t)
        }
    }
}

/// Plain text of blocks: paragraphs on lines, cells separated by tabs.
fn plain(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                out.extend(super::text::UnitText::of(p).text.chars().map(|c| match c {
                    '\u{2028}' => '\n',
                    '\u{c}' | '\u{FFFC}' => ' ',
                    c => c,
                }));
                out.push('\n');
            }
            Block::Table(t) => {
                for r in &t.rows {
                    for (k, c) in r.cells.iter().enumerate() {
                        if k > 0 {
                            out.push('\t');
                        }
                        let mut s = String::new();
                        plain(&c.blocks, &mut s);
                        out.push_str(s.trim_end_matches('\n').replace('\n', " ").as_str());
                    }
                    out.push('\n');
                }
            }
        }
    }
}

fn css_run(r: &RunStyle) -> String {
    let mut s = format!("font-family:'{}';font-size:{}pt", esc(&r.ascii), r.size);
    if r.bold {
        s.push_str(";font-weight:bold");
    }
    if r.italic {
        s.push_str(";font-style:italic");
    }
    let mut deco = Vec::new();
    if r.underline.is_some() {
        deco.push("underline");
    }
    if r.strike || r.dstrike {
        deco.push("line-through");
    }
    if !deco.is_empty() {
        s.push_str(&format!(";text-decoration:{}", deco.join(" ")));
    }
    if let Some(c) = r.color {
        s.push_str(&format!(";color:#{c:06X}"));
    }
    if let Some(h) = r.highlight {
        s.push_str(&format!(";background:#{h:06X}"));
    }
    match r.vert_align {
        VertAlign::Superscript => s.push_str(";vertical-align:super"),
        VertAlign::Subscript => s.push_str(";vertical-align:sub"),
        VertAlign::Baseline => {}
    }
    if r.small_caps {
        s.push_str(";font-variant:small-caps");
    }
    if r.caps {
        s.push_str(";text-transform:uppercase");
    }
    s
}

/// HTML for other applications: paragraphs with run styles, lists as their labels, tables with spans.
fn html(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                let align = match p.style.jc {
                    crate::docx::props::Jc::Center => "center",
                    crate::docx::props::Jc::Right => "right",
                    crate::docx::props::Jc::Both | crate::docx::props::Jc::Distribute => "justify",
                    crate::docx::props::Jc::Left => "left",
                };
                out.push_str(&format!("<p style=\"margin:0;text-align:{align}\">"));
                if let Some(l) = &p.list {
                    out.push_str(&format!("<span style=\"{}\">{}&#9;</span>", css_run(&l.run), esc(&l.text)));
                }
                // Fields as their results show: codes and markers left out.
                for (i, shown) in p.inlines.iter().zip(crate::docx::fields::shown(&p.inlines)) {
                    let text = match (shown, i) {
                        (crate::docx::fields::Shown::Value(v), _) => esc(&v),
                        (crate::docx::fields::Shown::Not, _) => continue,
                        (_, Inline::Text(t, _)) => esc(t),
                        (_, Inline::Tab(_) | Inline::PTab(..)) => "&#9;".into(),
                        (_, Inline::Break(..)) => "<br>".into(),
                        (_, Inline::Sym { ch, .. }) => esc(&ch.to_string()),
                        _ => continue,
                    };
                    match i.run() {
                        Some(r) => out.push_str(&format!("<span style=\"{}\">{text}</span>", css_run(r))),
                        None => out.push_str(&text),
                    }
                }
                if p.inlines.iter().all(|i| i.units() == 0) {
                    out.push_str("<br>");
                }
                out.push_str("</p>");
            }
            Block::Table(t) => {
                out.push_str("<table style=\"border-collapse:collapse\">");
                let g = super::table::Grid::of(t);
                for (ri, r) in t.rows.iter().enumerate() {
                    out.push_str("<tr>");
                    for (ci, c) in r.cells.iter().enumerate() {
                        if c.v_merge == VMerge::Continue {
                            continue;
                        }
                        let col = g.rows[ri][ci].0;
                        let rows = if c.v_merge == VMerge::Restart {
                            1 + (ri + 1..t.rows.len()).take_while(|&k| g.cell_at(k, col).is_some_and(|kc| g.continues(k, kc))).count()
                        } else {
                            1
                        };
                        out.push_str(&format!(
                            "<td style=\"border:1px solid #000;vertical-align:top\" colspan=\"{}\" rowspan=\"{rows}\">",
                            c.span.max(1)
                        ));
                        html(&c.blocks, out);
                        out.push_str("</td>");
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
            }
        }
    }
}

impl Fragment {
    pub fn text(&self) -> String {
        let mut s = String::new();
        plain(&self.blocks, &mut s);
        if self.open_end && s.ends_with('\n') {
            s.pop();
        }
        s
    }

    pub fn html(&self) -> String {
        let mut s = String::from("<meta charset=\"utf-8\">");
        html(&self.blocks, &mut s);
        s
    }
}

impl Editor {
    /// The selection as a fragment: within one container, its blocks (the ends cut, tables whole); across cells,
    /// each touched paragraph's selected part.
    pub fn copy_fragment(&mut self) -> Option<Fragment> {
        if matches!(self.selection, Selection::Cells { .. }) {
            let id = self.clipboard.as_ref().map_or(1, |f| f.id + 1);
            return self.copy_cells(id);
        }
        // Counted per editor, so a replay of the same input log names the same fragments.
        let id = self.clipboard.as_ref().map_or(1, |f| f.id + 1);
        // A block: each line's piece as a paragraph of its own.
        if let Selection::Block { .. } = self.selection {
            let mut out = Vec::new();
            for (_, _, para, a, b) in self.block_pieces() {
                let Some(p) = self.index.para(&self.doc, para) else { continue };
                let mut q = p.clone();
                q.inlines = slice(p, a, b);
                out.push(Block::Para(q));
            }
            return (!out.is_empty()).then_some(Fragment { id, blocks: out, open_end: true, cells: None, controls: Default::default() });
        }
        let (a, b) = self.selection.ordered(&self.index)?;
        if a.same_place(&b) {
            return None;
        }
        let (pa, oa) = a.para_offset()?;
        let (pb, ob) = match b.para_offset() {
            Some(x) => x,
            None => {
                // A row end mark at the end: up to the paragraph before it.
                let p = self.nav().left(b, false);
                p.para_offset()?
            }
        };
        let (sa, sb) = (self.index.para_slot(pa)?, self.index.para_slot(pb)?);
        if sa.container == sb.container {
            let blocks = self.index.blocks(&self.doc, sa.container)?;
            let mut out = Vec::new();
            for (k, blk) in blocks.iter().enumerate().take(sb.block + 1).skip(sa.block) {
                match blk {
                    Block::Para(p) => {
                        let from = if k == sa.block { oa } else { Unit(0) };
                        let to = if k == sb.block { ob } else { para_len(p) };
                        let mut q = p.clone();
                        q.inlines = slice(p, from, to);
                        out.push(Block::Para(q));
                    }
                    t @ Block::Table(_) => out.push(t.clone()),
                }
            }
            let open_end = self.index.para(&self.doc, pb).is_some_and(|p| ob < para_len(p) || sa.block == sb.block) || b.para_offset().is_none();
            return Some(Fragment { id, blocks: out, open_end, cells: None, controls: Default::default() });
        }
        // Across cells or levels: the selected part of each paragraph, in reading order.
        let mut out = Vec::new();
        for (para, s, e) in self.selected_ranges() {
            if let Some(p) = self.index.para(&self.doc, para) {
                let mut q = p.clone();
                q.inlines = slice(p, s, e);
                out.push(Block::Para(q));
            }
        }
        Some(Fragment { id, blocks: out, open_end: true, cells: None, controls: Default::default() })
    }

    /// A run that looks as `src` does wherever it goes: its direct formatting, plus explicit properties for what
    /// resolving it over `base` would change (keep source formatting).
    fn keep_look(&self, src: &Run, base: &crate::docx::props::RunProps, cache: &mut HashMap<*const RunStyle, Run>) -> Run {
        if let Some(r) = cache.get(&Rc::as_ptr(src)) {
            return r.clone();
        }
        let Some(res) = self.doc.resolver.clone() else { return src.clone() };
        let cx = self.cx();
        let mut out = res.run_from_xml(base, src.direct.0.as_deref(), &cx);
        if *out != **src {
            let w = cx.w.clone();
            let mut p = Props::parse(src.direct.0.as_deref(), "rPr", &cx);
            let flag = |name: &str, on: bool| if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") };
            if out.bold != src.bold {
                p.set("b", flag("b", src.bold));
                p.set("bCs", flag("bCs", src.bold));
            }
            if out.italic != src.italic {
                p.set("i", flag("i", src.italic));
                p.set("iCs", flag("iCs", src.italic));
            }
            if out.caps != src.caps {
                p.set("caps", flag("caps", src.caps));
            }
            if out.small_caps != src.small_caps {
                p.set("smallCaps", flag("smallCaps", src.small_caps));
            }
            if out.strike != src.strike {
                p.set("strike", flag("strike", src.strike));
            }
            if out.hidden != src.hidden {
                p.set("vanish", flag("vanish", src.hidden));
            }
            if out.underline != src.underline {
                p.set("u", format!("<{w}u {w}val=\"{}\"/>", src.underline.as_deref().unwrap_or("none")));
            }
            if (out.size - src.size).abs() > 0.01 {
                let hp = (src.size * 2.0).round() as i64;
                p.set("sz", format!("<{w}sz {w}val=\"{hp}\"/>"));
                p.set("szCs", format!("<{w}szCs {w}val=\"{hp}\"/>"));
            }
            if out.ascii != src.ascii || out.h_ansi != src.h_ansi || out.east_asia != src.east_asia || out.cs != src.cs {
                p.set(
                    "rFonts",
                    format!(
                        "<{w}rFonts {w}ascii=\"{}\" {w}hAnsi=\"{}\" {w}eastAsia=\"{}\" {w}cs=\"{}\"/>",
                        esc(&src.ascii),
                        esc(&src.h_ansi),
                        esc(&src.east_asia),
                        esc(&src.cs)
                    ),
                );
            }
            if out.color != src.color {
                p.set("color", format!("<{w}color {w}val=\"{}\"/>", src.color.map_or("auto".to_string(), |c| format!("{c:06X}"))));
            }
            if out.vert_align != src.vert_align {
                let v = match src.vert_align {
                    VertAlign::Superscript => "superscript",
                    VertAlign::Subscript => "subscript",
                    VertAlign::Baseline => "baseline",
                };
                p.set("vertAlign", format!("<{w}vertAlign {w}val=\"{v}\"/>"));
            }
            out = res.run_from_xml(base, p.xml().as_deref(), &cx);
        }
        cache.insert(Rc::as_ptr(src), out.clone());
        out
    }

    /// Inlines with their runs fixed to keep their look in paragraph `para`'s context.
    fn fit_inlines(&self, para: ParaId, inlines: &[Inline]) -> Vec<Inline> {
        let Some(res) = self.doc.resolver.clone() else { return inlines.to_vec() };
        let cx = self.cx();
        let Some(p) = self.index.para(&self.doc, para) else { return inlines.to_vec() };
        let region = self.region_of_para(para, &res);
        let base = res.para_from_xml(p.style.direct.0.as_deref(), region.as_ref(), &cx).base;
        let mut cache = HashMap::new();
        inlines
            .iter()
            .map(|i| match i.run() {
                Some(r) => super::format::with_run(i, &self.keep_look(r, &base, &mut cache)),
                None => i.clone(),
            })
            .collect()
    }

    /// Pastes a fragment at the caret, replacing the selection: the first paragraph's content joins the
    /// paragraph at the caret, whole blocks go between, and an open last paragraph joins the rest after the caret.
    pub fn paste_fragment(&mut self, f: &Fragment, fonts: &Fonts) -> Outcome {
        if let Some(o) = self.paste_cells(f, fonts) {
            return o;
        }
        // Smart paste for one open paragraph: the spaces it needs beside its neighbours, decided before the
        // selection goes.
        let spaces = match f.blocks.as_slice() {
            [Block::Para(p)] if f.open_end => self.smart_spaces(&super::text::UnitText::of(p).text),
            _ => (false, false),
        };
        if self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let mut tx = self.begin("Paste");
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => self.past_row_end(p),
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let Some((para, at)) = caret.para_offset() else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let mut blocks: Vec<Block> = f.blocks.iter().map(fresh).collect();
        for (id, c) in adopt(&mut blocks, &f.controls) {
            let _ = self.step(&mut tx, Step::Control { id, control: Some(c) });
        }
        // Whole pasted blocks are in the block-level controls the caret's paragraph is in.
        let dest: Vec<crate::docx::ControlId> = self.index.para(&self.doc, para).map(|p| p.controls.clone()).unwrap_or_default();
        if !dest.is_empty() {
            for b in &mut blocks {
                match b {
                    Block::Para(p) => p.controls.splice(0..0, dest.iter().copied()),
                    Block::Table(t) => t.controls.splice(0..0, dest.iter().copied()),
                };
            }
        }
        let n = blocks.len();
        let first_inlines = match blocks.first() {
            Some(Block::Para(p)) => Some(p.inlines.clone()),
            _ => None,
        };
        let units = |v: &[Inline]| v.iter().map(|i| i.units() as u32).sum::<u32>();
        // One open paragraph: its content goes in at the caret.
        if n == 1
            && f.open_end
            && let Some(inl) = &first_inlines
        {
            let mut with = self.fit_inlines(para, inl);
            let space = |i: Option<&Inline>| i.and_then(|i| i.run()).map(|r| Inline::Text(" ".into(), r.clone()));
            if spaces.0
                && let Some(s) = space(with.first())
            {
                with.insert(0, s);
            }
            if spaces.1
                && let Some(s) = space(with.last())
            {
                with.push(s);
            }
            let k = units(&with);
            if self.step(&mut tx, Step::Text { para, range: at..at, with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("paste failed");
            }
            self.selection = Selection::Caret(Position::text(caret.story, para, Unit(at.0 + k)));
            return self.commit(tx, None, fonts);
        }
        // Whole paragraphs at a paragraph's start, the first in a block control of its own: they go in before it as
        // they are, so the control keeps all its paragraphs; an open last paragraph's content joins the one at the
        // caret.
        if at == Unit(0) && n > 1 && matches!(blocks.first(), Some(Block::Para(p)) if p.controls.iter().any(|c| !dest.contains(c))) {
            let mut middle = blocks;
            let last = match middle.last() {
                Some(Block::Para(_)) if f.open_end => match middle.pop() {
                    Some(Block::Para(p)) => Some(p.inlines),
                    _ => None,
                },
                _ => None,
            };
            let Some(slot) = self.index.para_slot(para) else {
                self.abort(tx);
                return Outcome::Unchanged("no paragraph");
            };
            let ids: Vec<ParaId> = middle.iter().filter_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None }).collect();
            if self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: middle }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("paste failed");
            }
            for p in ids {
                if let Some(q) = self.index.para(&self.doc, p).cloned() {
                    let with = self.fit_inlines(p, &q.inlines);
                    let _ = self.step(&mut tx, Step::Restyle { para: p, inlines: with });
                }
            }
            let mut k = 0;
            if let Some(inl) = last {
                let with = self.fit_inlines(para, &inl);
                k = units(&with);
                if k > 0 && self.step(&mut tx, Step::Text { para, range: Unit(0)..Unit(0), with, edges: Default::default() }).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("paste failed");
                }
            }
            self.selection = Selection::Caret(Position::text(caret.story, para, Unit(k)));
            return self.commit(tx, None, fonts);
        }
        // Split at the caret; the first paragraph's content ends the first half.
        let Some(second) = self.split_template(para) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let tail = ParaId(second.id);
        if self.step(&mut tx, Step::Split { para, at, second }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("split failed");
        }
        let mut middle: Vec<Block> = blocks.clone();
        if let Some(inl) = &first_inlines {
            middle.remove(0);
            let with = self.fit_inlines(para, inl);
            if self.step(&mut tx, Step::Text { para, range: at..at, with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("paste failed");
            }
        }
        let last_open = f.open_end && matches!(middle.last(), Some(Block::Para(_)));
        let last_inlines = if last_open {
            match middle.pop() {
                Some(Block::Para(p)) => Some(p.inlines),
                _ => None,
            }
        } else {
            None
        };
        let Some(slot) = self.index.para_slot(tail) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let ids: Vec<ParaId> = middle.iter().filter_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None }).collect();
        if !middle.is_empty() && self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: middle }).is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("paste failed");
        }
        // Whole pasted paragraphs keep their look in their new place.
        for p in ids {
            if let Some(q) = self.index.para(&self.doc, p).cloned() {
                let with = self.fit_inlines(p, &q.inlines);
                let _ = self.step(&mut tx, Step::Restyle { para: p, inlines: with });
            }
        }
        let caret_after = match last_inlines {
            Some(inl) => {
                let with = self.fit_inlines(tail, &inl);
                let k = units(&with);
                if self.step(&mut tx, Step::Text { para: tail, range: Unit(0)..Unit(0), with, edges: Default::default() }).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("paste failed");
                }
                Position::text(caret.story, tail, Unit(k))
            }
            None => Position::text(caret.story, tail, Unit(0)),
        };
        self.selection = Selection::Caret(caret_after);
        self.commit(tx, None, fonts)
    }
}

/// Markers whose partner a copy's range left out (an inline control's start without its end, a field cut in half)
/// leave the copied paragraphs: their content stays as text.
pub(super) fn drop_unpaired(blocks: &mut [Block]) {
    use crate::docx::{ControlId, FieldId};
    for b in blocks {
        match b {
            Block::Para(p) => {
                let starts: Vec<ControlId> = p.inlines.iter().filter_map(|i| if let Inline::ControlStart(c) = i { Some(*c) } else { None }).collect();
                let ends: Vec<ControlId> = p.inlines.iter().filter_map(|i| if let Inline::ControlEnd(c) = i { Some(*c) } else { None }).collect();
                let begins: Vec<FieldId> =
                    p.inlines.iter().filter_map(|i| if let Inline::FieldBegin { id, .. } = i { Some(*id) } else { None }).collect();
                let field_ends: Vec<FieldId> =
                    p.inlines.iter().filter_map(|i| if let Inline::FieldEnd { id, .. } = i { Some(*id) } else { None }).collect();
                p.inlines.retain(|i| match i {
                    Inline::ControlStart(c) => ends.contains(c),
                    Inline::ControlEnd(c) => starts.contains(c),
                    Inline::FieldBegin { id, .. } | Inline::FieldSep { id, .. } => field_ends.contains(id),
                    Inline::FieldEnd { id, .. } => begins.contains(id),
                    _ => true,
                });
            }
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| drop_unpaired(&mut c.blocks)),
        }
    }
}

/// Fresh ids for every field and content control in `blocks`, and
/// the definitions of the controls (from `defs`) under their new ids.
pub(super) fn adopt(
    blocks: &mut [Block],
    defs: &std::collections::HashMap<crate::docx::ControlId, crate::docx::Control>,
) -> Vec<(crate::docx::ControlId, crate::docx::Control)> {
    use crate::docx::{ControlId, FieldId};
    use std::collections::HashMap;
    struct Ids {
        fields: HashMap<FieldId, FieldId>,
        controls: HashMap<ControlId, ControlId>,
    }
    impl Ids {
        fn field(&mut self, f: &mut FieldId) {
            *f = *self.fields.entry(*f).or_insert_with(FieldId::next);
        }
        fn control(&mut self, c: &mut ControlId) {
            *c = *self.controls.entry(*c).or_insert_with(ControlId::next);
        }
    }
    fn walk(blocks: &mut [Block], ids: &mut Ids) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    p.controls.iter_mut().for_each(|c| ids.control(c));
                    for i in &mut p.inlines {
                        match i {
                            Inline::FieldBegin { id, .. } | Inline::FieldSep { id, .. } | Inline::FieldEnd { id, .. } => ids.field(id),
                            Inline::ControlStart(c) | Inline::ControlEnd(c) | Inline::Sym { control: Some(c), .. } => ids.control(c),
                            Inline::Float(f, _) => {
                                if let Some(tb) = &mut f.text_box {
                                    walk(&mut tb.blocks, ids);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => {
                    t.controls.iter_mut().for_each(|c| ids.control(c));
                    for r in &mut t.rows {
                        r.controls.iter_mut().for_each(|c| ids.control(c));
                        for c in &mut r.cells {
                            c.controls.iter_mut().for_each(|k| ids.control(k));
                            walk(&mut c.blocks, ids);
                        }
                    }
                }
            }
        }
    }
    let mut ids = Ids { fields: HashMap::new(), controls: HashMap::new() };
    walk(blocks, &mut ids);
    let mut out: Vec<(ControlId, crate::docx::Control)> = ids
        .controls
        .iter()
        .filter_map(|(old, new)| {
            let mut c = defs.get(old)?.clone();
            // A copy is a new control: its `w:id` is written anew (the props otherwise stay).
            c.props.rest.retain(|(n, _)| n != "id");
            Some((*new, crate::docx::Control::new(c.props)))
        })
        .collect();
    out.sort_by_key(|(id, _)| *id);
    out
}

/// Every content control `blocks` hold, as the document defines them (what a copy carries).
pub(super) fn controls_of(blocks: &[Block], doc: &crate::docx::Document) -> std::collections::HashMap<crate::docx::ControlId, crate::docx::Control> {
    fn ids(blocks: &[Block], out: &mut Vec<crate::docx::ControlId>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    out.extend(p.controls.iter().copied());
                    for i in &p.inlines {
                        match i {
                            Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => out.push(*c),
                            Inline::Float(f, _) => {
                                if let Some(tb) = &f.text_box {
                                    ids(&tb.blocks, out);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => {
                    out.extend(t.controls.iter().copied());
                    for r in &t.rows {
                        out.extend(r.controls.iter().copied());
                        for c in &r.cells {
                            out.extend(c.controls.iter().copied());
                            ids(&c.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut v = Vec::new();
    ids(blocks, &mut v);
    v.into_iter().filter_map(|c| Some((c, doc.controls.get(&c)?.clone()))).collect()
}

/// Takes every content control out of `blocks`: memberships and inline markers (their content stays).
pub(super) fn strip_controls(blocks: &mut [Block]) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                p.controls.clear();
                p.inlines.retain(|i| !matches!(i, Inline::ControlStart(_) | Inline::ControlEnd(_)));
                for i in &mut p.inlines {
                    if let Inline::Sym { control, .. } = i {
                        *control = None;
                    }
                }
            }
            Block::Table(t) => {
                t.controls.clear();
                for r in &mut t.rows {
                    r.controls.clear();
                    for c in &mut r.cells {
                        c.controls.clear();
                        strip_controls(&mut c.blocks);
                    }
                }
            }
        }
    }
}

/// Block-level controls only part of which a copy holds (one paragraph of a two-paragraph repeating item) leave its
/// blocks: a control is copied only with all its content.
pub(super) fn drop_partial(blocks: &mut [Block], index: &super::index::DocIndex) {
    use super::index::Loc;
    use crate::docx::ElementId;
    use crate::layout::TableId;
    fn ids(blocks: &[Block], paras: &mut HashSet<ParaId>, tables: &mut HashSet<TableId>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    paras.insert(ParaId(p.id));
                }
                Block::Table(t) => {
                    tables.insert(TableId(t.id));
                    t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| ids(&c.blocks, paras, tables));
                }
            }
        }
    }
    let (mut paras, mut tables) = (HashSet::new(), HashSet::new());
    ids(blocks, &mut paras, &mut tables);
    let whole = |c: &crate::docx::ControlId| {
        index.element(ElementId::Control(*c)).is_some_and(|locs| {
            locs.iter().all(|l| match l {
                Loc::Para(p) => paras.contains(p),
                Loc::Table(t) | Loc::Row(t, _) | Loc::Cell(t, _, _) => tables.contains(t),
            })
        })
    };
    fn walk(blocks: &mut [Block], keep: &dyn Fn(&crate::docx::ControlId) -> bool) {
        for b in blocks {
            match b {
                Block::Para(p) => p.controls.retain(|c| keep(c)),
                Block::Table(t) => {
                    t.controls.retain(|c| keep(c));
                    t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| walk(&mut c.blocks, keep));
                }
            }
        }
    }
    walk(blocks, &whole);
}
