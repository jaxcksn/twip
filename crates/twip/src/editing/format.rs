//! Character and paragraph formatting. Direct properties are patched as
//! written and re-resolved through the document's resolver, so an edit resolves exactly as reading would.

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::props::{Jc, RunProps};
use crate::docx::resolve::Resolver;
use crate::docx::styles::Region;
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Inline, Run, RunStyle};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::index::Container;
use super::step::Step;
use super::{Editor, Tx};

/// The root a document without a source part (a new one) writes its properties under.
const NEW_ROOT: &str = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#;

/// A character property a toggle command turns on or off (Ctrl+B, Ctrl+I, Ctrl+U, Ctrl+=, Ctrl+Shift+=, Strikethrough).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Bold,
    Italic,
    Underline,
    Strike,
    Subscript,
    Superscript,
}

impl Toggle {
    pub(super) fn on(self, r: &RunStyle) -> bool {
        match self {
            Toggle::Bold => r.bold,
            Toggle::Italic => r.italic,
            Toggle::Underline => r.underline.is_some(),
            Toggle::Strike => r.strike,
            Toggle::Subscript => r.vert_align == crate::docx::props::VertAlign::Subscript,
            Toggle::Superscript => r.vert_align == crate::docx::props::VertAlign::Superscript,
        }
    }

    /// Sets the property in a run's `w:rPr` (both script forms for bold and italic, as Word writes them).
    fn set(self, p: &mut Props, w: &str, on: bool) {
        let flag = |name: &str| if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") };
        match self {
            Toggle::Bold => {
                p.set("b", flag("b"));
                p.set("bCs", flag("bCs"));
            }
            Toggle::Italic => {
                p.set("i", flag("i"));
                p.set("iCs", flag("iCs"));
            }
            Toggle::Underline => p.set("u", format!("<{w}u {w}val=\"{}\"/>", if on { "single" } else { "none" })),
            Toggle::Strike => p.set("strike", flag("strike")),
            // Subscript and superscript exclude each other: turning one on replaces the other.
            Toggle::Subscript | Toggle::Superscript => {
                let v = match (self, on) {
                    (_, false) => "baseline",
                    (Toggle::Subscript, true) => "subscript",
                    _ => "superscript",
                };
                p.set("vertAlign", format!("<{w}vertAlign {w}val=\"{v}\"/>"))
            }
        }
    }
}

/// An inline carrying `run` instead of its own.
pub(super) fn with_run(i: &Inline, run: &Run) -> Inline {
    let run = run.clone();
    match i {
        Inline::Text(t, _) => Inline::Text(t.clone(), run),
        Inline::Tab(_) => Inline::Tab(run),
        Inline::PTab(t, _) => Inline::PTab(*t, run),
        Inline::Break(k, _) => Inline::Break(*k, run),
        Inline::Sym { ch, font, control, .. } => Inline::Sym { ch: *ch, font: font.clone(), run, control: *control },
        Inline::Picture(p, _) => Inline::Picture(p.clone(), run),
        Inline::NoteRef { kind, id, custom, .. } => Inline::NoteRef { kind: *kind, id: *id, custom: *custom, run },
        Inline::NoteMark(k, id, _) => Inline::NoteMark(*k, *id, run),
        Inline::FieldBegin { id, kind, lock, dirty, view, label, value, .. } => Inline::FieldBegin {
            id: *id,
            kind: kind.clone(),
            lock: *lock,
            dirty: *dirty,
            view: *view,
            label: label.clone(),
            value: value.clone(),
            run,
        },
        Inline::FieldSep { id, .. } => Inline::FieldSep { id: *id, run },
        Inline::FieldEnd { id, .. } => Inline::FieldEnd { id: *id, run },
        // Anchored objects, separators and bookmarks keep what they have.
        other => other.clone(),
    }
}

impl Editor {
    /// Resolves the transaction's stale paragraphs again (and those directly in its stale tables' cells), as steps
    /// of the same transaction: a paragraph whose properties or runs resolve differently in their context now gets
    /// what reading the saved document would give it.
    pub(super) fn reresolve_stale(&mut self, tx: &mut Tx) {
        let Some(res) = self.doc.resolver.clone() else { return };
        // Cells of stale tables: their properties again from their style region at their place now (banding, first
        // and last rows and columns) and their direct `w:tcPr`. Cells made from nothing (no direct XML) keep theirs.
        let cx = self.cx();
        for &table in &tx.stale_tables.clone() {
            let Some(t) = self.index.table(&self.doc, table) else { continue };
            let look = t.look.unwrap_or(crate::docx::props::Look {
                first_row: true,
                last_row: false,
                first_col: true,
                last_col: false,
                no_h_band: false,
                no_v_band: true,
            });
            let nrows = t.rows.len();
            let mut rows = t.rows.clone();
            for (ri, row) in rows.iter_mut().enumerate() {
                let ncols = row.cells.len();
                for (ci, c) in row.cells.iter_mut().enumerate() {
                    let Some(raw) = c.direct.0.clone() else { continue };
                    let region = res.cell_region(t.style_id.as_deref(), look, t.bands, (ri, ci), (nrows, ncols));
                    let mut tcp = region.tc.clone();
                    tcp.merge(&cx.parse(&raw, |_, n| crate::docx::props::TcProps::parse(Some(n))).unwrap_or_default());
                    c.shading = tcp.shading.flatten();
                    c.top = tcp.borders.top.clone();
                    c.left = tcp.borders.left.clone();
                    c.bottom = tcp.borders.bottom.clone();
                    c.right = tcp.borders.right.clone();
                    c.v_align = tcp.v_align.clone().unwrap_or_else(|| "top".into());
                    c.text_direction = tcp.text_direction.clone().unwrap_or_else(|| "lrTb".into());
                    c.no_wrap = tcp.no_wrap.unwrap_or(false);
                }
            }
            if rows != t.rows {
                let _ = self.step(tx, Step::Rows { table, range: 0..nrows, with: rows, grid: None });
            }
        }
        let mut paras = std::mem::take(&mut tx.stale_paras);
        for table in std::mem::take(&mut tx.stale_tables) {
            if let Some(t) = self.index.table(&self.doc, table) {
                for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                    paras.extend(c.blocks.iter().filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(ParaId(p.id)) } else { None }));
                }
            }
        }
        paras.sort_unstable_by_key(|p| p.0);
        paras.dedup();
        let cx = self.cx();
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let region = self.region_of_para(para, &res);
            let r = res.para_from_xml(p.style.direct.0.as_deref(), region.as_ref(), &cx);
            let mark = res.run_from_xml(&r.base, p.mark.direct.0.as_deref(), &cx);
            let mut cache: HashMap<*const RunStyle, Run> = HashMap::new();
            let mut changed = false;
            let with: Vec<Inline> = p
                .inlines
                .iter()
                .map(|i| match i.run() {
                    Some(run) => {
                        let new = cache.entry(Rc::as_ptr(run)).or_insert_with(|| {
                            let again = res.rerun(&r.base, run, &cx);
                            if *again == **run { run.clone() } else { again }
                        });
                        changed |= !Rc::ptr_eq(new, run);
                        with_run(i, &new.clone())
                    }
                    None => i.clone(),
                })
                .collect();
            let style_changed = *p.style != r.style || *p.mark != *mark;
            let (style, mark) = (Rc::new(r.style), if *p.mark != *mark { mark } else { p.mark.clone() });
            if style_changed {
                let list = p.list.clone();
                let _ = self.step(tx, Step::Para { para, style, mark, list });
            }
            if changed {
                let _ = self.step(tx, Step::Restyle { para, inlines: with });
            }
        }
    }

    /// The paragraph Enter starts at the end of `para`: in its style's next style when that differs, its
    /// direct properties otherwise kept. `None` when the style has no other next style.
    pub(super) fn next_style_paragraph(&self, para: ParaId, mut second: crate::docx::Paragraph) -> Option<crate::docx::Paragraph> {
        let res = self.doc.resolver.clone()?;
        let p = self.index.para(&self.doc, para)?;
        let id = p.style.style_id.as_deref()?;
        let next = res.styles.by_id.get(id)?.next.clone().filter(|n| n != id && res.styles.by_id.contains_key(n))?;
        let cx = self.cx();
        let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
        pr.set("pStyle", format!("<{w}pStyle {w}val=\"{}\"/>", crate::docx::write::props::esc(&next), w = cx.w));
        let region = self.region_of_para(para, &res);
        second.style = Rc::new(res.para_from_xml(pr.xml().as_deref(), region.as_ref(), &cx).style);
        Some(second)
    }

    pub(super) fn cx(&self) -> Cx {
        Cx::of_xml(self.doc.source.as_ref().map_or(NEW_ROOT, |s| s.xml.as_str()))
    }

    /// The table-style region of the cell a paragraph is in.
    pub(super) fn region_of_para(&self, para: ParaId, res: &Resolver) -> Option<Region> {
        let slot = self.index.para_slot(para)?;
        let Container::Cell { table, row, cell } = slot.container else { return None };
        Some(res.region_of(self.index.table(&self.doc, table)?, row, cell))
    }

    /// What a paragraph's runs resolve over (docDefaults, its cell's region, its style chain).
    pub(super) fn run_base(&self, para: ParaId, res: &Resolver, cx: &Cx) -> Option<RunProps> {
        let p = self.index.para(&self.doc, para)?;
        Some(res.para_from_xml(p.style.direct.0.as_deref(), self.region_of_para(para, res).as_ref(), cx).base)
    }

    /// The unit ranges of each paragraph a text selection covers, in document order.
    pub(super) fn selected_ranges(&mut self) -> Vec<(ParaId, Unit, Unit)> {
        if let super::selection::Selection::Block { .. } = self.selection {
            return self.block_pieces().into_iter().map(|(_, _, p, a, b)| (p, a, b)).collect();
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Vec::new() };
        let mut out = Vec::new();
        let mut p = a;
        for _ in 0..1_000_000 {
            if self.index.cmp(&p, &b) != std::cmp::Ordering::Less {
                break;
            }
            if let Some((para, off)) = p.para_offset() {
                let len = self.index.para(&self.doc, para).map_or(Unit(0), super::step::para_len);
                let end = match b.para_offset() {
                    Some((pb, ob)) if pb == para => ob,
                    _ => len,
                };
                out.push((para, off, end));
                let at_end = super::position::Position::text(p.story, para, len);
                let next = self.nav().right(at_end, false);
                if next == at_end {
                    break;
                }
                p = next;
            } else {
                let next = self.nav().right(p, false);
                if next == p {
                    break;
                }
                p = next;
            }
        }
        out
    }

    /// The paragraphs a paragraph command applies to: the selection's, or the caret's.
    pub(super) fn selected_paras(&mut self) -> Vec<ParaId> {
        let mut v: Vec<ParaId> = self.selected_ranges().into_iter().map(|r| r.0).collect();
        if v.is_empty() {
            v.extend(self.caret().and_then(|c| c.para_offset()).map(|(p, _)| p));
        }
        v.dedup();
        v
    }

    /// Ctrl+B, Ctrl+I, Ctrl+U, Ctrl+=, Ctrl+Shift+=, Strikethrough: on for the whole selection unless all of it
    /// already has it; at a collapsed caret, by the caret's run.
    pub(super) fn toggle(&mut self, t: Toggle, fonts: &Fonts) -> Outcome {
        let on = if self.selection.is_collapsed() {
            let Some(run) = self.caret().and_then(|c| self.run_at(&c)) else { return Outcome::Unchanged("no caret") };
            !t.on(&run)
        } else {
            let ranges: Vec<_> = self.selected_ranges().into_iter().filter(|r| r.1 < r.2).collect();
            !ranges.iter().all(|&r| self.runs_in(r).iter().all(|run| t.on(run)))
        };
        self.format_runs("Font", &|p, w, _| t.set(p, w, on), fonts)
    }

    /// What is inserted next has the caret's formatting patched by `edit`.
    pub(super) fn pending_props(&mut self, edit: &dyn Fn(&mut Props, &str)) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        let Some(caret) = self.caret() else { return };
        let (Some((para, _)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else { return };
        let Some(base) = self.run_base(para, &res, &cx) else { return };
        let mut p = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        edit(&mut p, &cx.w);
        self.pending = Some(res.run_from_xml(&base, p.xml().as_deref(), &cx));
    }

    /// What is typed next takes `font` (Insert Symbol).
    pub(super) fn pending_font(&mut self, font: &str) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        let Some(caret) = self.caret() else { return };
        let (Some((para, _)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else { return };
        let Some(base) = self.run_base(para, &res, &cx) else { return };
        let mut p = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        let n = crate::docx::write::props::esc(font);
        let w = &cx.w;
        p.set("rFonts", format!("<{w}rFonts {w}ascii=\"{n}\" {w}hAnsi=\"{n}\" {w}eastAsia=\"{n}\" {w}cs=\"{n}\"/>"));
        self.pending = Some(res.run_from_xml(&base, p.xml().as_deref(), &cx));
    }

    /// The runs holding units a..b of a paragraph.
    pub(super) fn runs_in(&self, (para, a, b): (ParaId, Unit, Unit)) -> Vec<Run> {
        let Some(p) = self.index.para(&self.doc, para) else { return Vec::new() };
        let mut at = 0u32;
        let mut out = Vec::new();
        for i in &p.inlines {
            let n = i.units() as u32;
            if n > 0
                && at < b.0
                && at + n > a.0
                && let Some(r) = i.run()
            {
                out.push(r.clone());
            }
            at += n;
        }
        out
    }

    /// A character formatting command: `edit` patches each affected run's direct properties (it sees the run). Over
    /// the selection; at a collapsed caret, inside a word the word's letters, at a paragraph's end the mark
    /// too, and in every case what is typed next.
    pub(super) fn format_runs(&mut self, label: &'static str, edit: &dyn Fn(&mut Props, &str, &RunStyle), fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let w = cx.w.clone();
        let patch = |run: &Run, base: &RunProps| -> Run {
            let mut p = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
            edit(&mut p, &w, run);
            res.run_from_xml(base, p.xml().as_deref(), &cx)
        };
        let mut ranges: Vec<(ParaId, Unit, Unit)> = Vec::new();
        let mut mark_too: Option<ParaId> = None;
        if self.selection.is_collapsed() {
            let Some(caret) = self.caret() else { return Outcome::Unchanged("no caret") };
            let (Some((para, off)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else { return Outcome::Unchanged("no paragraph") };
            let Some(base) = self.run_base(para, &res, &cx) else { return Outcome::Unchanged("no paragraph") };
            self.pending = Some(patch(&run, &base));
            let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Moved };
            let text = super::text::UnitText::of(p);
            match text.letters_around(off) {
                Some((a, b)) => ranges.push((para, a, b)),
                None if off == text.len() => mark_too = Some(para),
                None => return Outcome::Moved,
            }
        } else {
            ranges = self.selected_ranges().into_iter().filter(|r| r.1 < r.2).collect();
        }
        let mut tx = self.begin(label);
        if let Some(para) = mark_too
            && let Some(base) = self.run_base(para, &res, &cx)
            && let Some(p) = self.index.para(&self.doc, para)
        {
            let (style, list) = (p.style.clone(), p.list.clone());
            let mark = patch(&p.mark, &base);
            if self.step(&mut tx, Step::Para { para, style, mark, list }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("formatting failed");
            }
        }
        for (para, a, b) in ranges {
            let Some(base) = self.run_base(para, &res, &cx) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            // The units a..b with each run replaced, zero-unit inlines strictly inside kept in place (Step::Text replaces
            // those too).
            let mut cache: HashMap<*const RunStyle, Run> = HashMap::new();
            let mut with = Vec::new();
            let mut at = 0u32;
            for i in &p.inlines {
                let n = i.units() as u32;
                let (s, e) = (at, at + n);
                at = e;
                if n == 0 {
                    if s > a.0 && s < b.0 {
                        with.push(i.clone());
                    }
                    continue;
                }
                if e <= a.0 || s >= b.0 {
                    continue;
                }
                let Some(run) = i.run() else { continue };
                let new = cache.entry(Rc::as_ptr(run)).or_insert_with(|| patch(run, &base)).clone();
                match i {
                    Inline::Text(text, _) => {
                        let (from, to) = (a.0.max(s) - s, b.0.min(e) - s);
                        let part: String = text.chars().skip(from as usize).take((to - from) as usize).collect();
                        with.push(Inline::Text(part, new));
                    }
                    other => with.push(with_run(other, &new)),
                }
            }
            if self.step(&mut tx, Step::Text { para, range: a..b, with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("formatting failed");
            }
        }
        self.commit(tx, None, fonts)
    }

    /// Ctrl+L/E/R/J: the selected paragraphs' alignment; to left when they all already have it (Ctrl+E, Ctrl+J and
    /// Ctrl+R toggle).
    pub(super) fn align(&mut self, jc: Jc, fonts: &Fonts) -> Outcome {
        let paras = self.selected_paras();
        let all = paras.iter().all(|&p| self.index.para(&self.doc, p).is_some_and(|q| q.style.jc == jc));
        let target = if all && jc != Jc::Left { Jc::Left } else { jc };
        let val = match target {
            Jc::Center => "center",
            Jc::Right => "right",
            Jc::Both => "both",
            Jc::Distribute => "distribute",
            Jc::Left => "left",
        };
        self.format_paras("Paragraph Formatting", &|pr, cx, _| pr.set("jc", format!("<{w}jc {w}val=\"{val}\"/>", w = cx.w)), fonts)
    }

    /// A paragraph formatting command: `edit` patches each selected paragraph's direct properties (it sees the
    /// paragraph as it is); paragraphs it leaves resolving the same are untouched.
    pub(super) fn format_paras(&mut self, label: &'static str, edit: &dyn Fn(&mut Props, &Cx, &crate::docx::Paragraph), fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let paras = self.selected_paras();
        let mut tx = self.begin(label);
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
            edit(&mut pr, &cx, p);
            let region = self.region_of_para(para, &res);
            let r = res.para_from_xml(pr.xml().as_deref(), region.as_ref(), &cx);
            if r.style == *p.style {
                continue;
            }
            let (mark, list) = (p.mark.clone(), p.list.clone());
            if self.step(&mut tx, Step::Para { para, style: Rc::new(r.style), mark, list }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("formatting failed");
            }
            // A new paragraph style changes what its runs resolve over.
            tx.stale_paras.push(para);
        }
        self.commit(tx, None, fonts)
    }

    /// The character and paragraph formatting commands besides the toggles and alignment.
    pub(super) fn format(&mut self, f: &Format, fonts: &Fonts) -> Outcome {
        let twips = |pt: f64| ((pt * 20.0).round() as i64).to_string();
        match f {
            Format::FontSize(pt) => {
                let hp = ((pt * 2.0).round() as i64).max(2);
                self.format_runs(
                    "Font Size",
                    &|p, w, _| {
                        p.set("sz", format!("<{w}sz {w}val=\"{hp}\"/>"));
                        p.set("szCs", format!("<{w}szCs {w}val=\"{hp}\"/>"));
                    },
                    fonts,
                )
            }
            Format::GrowFont(grow) => {
                // The font size list: the next size up or down from each run's own.
                const SIZES: [f64; 16] = [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0];
                let grow = *grow;
                self.format_runs(
                    "Font Size",
                    &|p, w, run| {
                        let s = run.size;
                        let next = if grow {
                            SIZES.iter().copied().find(|&x| x > s + 0.01).unwrap_or(((s / 10.0).floor() + 1.0) * 10.0)
                        } else {
                            SIZES.iter().rev().copied().find(|&x| x < s - 0.01).unwrap_or((s - 1.0).max(1.0))
                        };
                        let hp = (next * 2.0).round() as i64;
                        p.set("sz", format!("<{w}sz {w}val=\"{hp}\"/>"));
                        p.set("szCs", format!("<{w}szCs {w}val=\"{hp}\"/>"));
                    },
                    fonts,
                )
            }
            Format::FontColor(c) => {
                let v = c.map_or("auto".to_string(), |c| format!("{c:06X}"));
                self.format_runs("Font Color", &|p, w, _| p.set("color", format!("<{w}color {w}val=\"{v}\"/>")), fonts)
            }
            Format::Highlight(name) => {
                let name = name.clone();
                self.format_runs(
                    "Highlight",
                    &|p, w, _| match &name {
                        Some(n) => p.set("highlight", format!("<{w}highlight {w}val=\"{}\"/>", crate::docx::write::props::esc(n))),
                        None => p.remove("highlight"),
                    },
                    fonts,
                )
            }
            Format::FontFamily(name) => {
                let n = crate::docx::write::props::esc(name);
                self.format_runs(
                    "Font",
                    &|p, w, _| p.set("rFonts", format!("<{w}rFonts {w}ascii=\"{n}\" {w}hAnsi=\"{n}\" {w}cs=\"{n}\" {w}eastAsia=\"{n}\"/>")),
                    fonts,
                )
            }
            Format::ClearFormatting => {
                // The runs' direct formatting goes (their character style too); paragraphs keep theirs.
                // shortcut: only the runs are cleared; the paragraph isn't reset to Normal.
                self.format_runs("Clear Formatting", &|p, _, _| *p = Props::parse(None, "rPr", &Cx::of_xml(NEW_ROOT)), fonts)
            }
            Format::LineSpacing(m) => {
                let line = ((m * 240.0).round() as i64).to_string();
                self.format_paras(
                    "Line Spacing",
                    &|p, cx, _| p.attrs("spacing", &[("line", Some(line.clone())), ("lineRule", Some("auto".into()))], cx),
                    fonts,
                )
            }
            Format::SpaceBefore => {
                // Add Space Before (12 pt), or Remove it when the first paragraph has some.
                let has = self.selected_paras().first().and_then(|&p| self.index.para(&self.doc, p)).is_some_and(|p| p.style.space_before > 0.0);
                let v = if has { "0" } else { "240" };
                self.format_paras(
                    "Paragraph Spacing",
                    &|p, cx, _| p.attrs("spacing", &[("before", Some(v.into())), ("beforeAutospacing", None)], cx),
                    fonts,
                )
            }
            Format::RemoveSpaceAfter => self.format_paras(
                "Paragraph Spacing",
                &|p, cx, _| p.attrs("spacing", &[("after", Some("0".into())), ("afterAutospacing", None)], cx),
                fonts,
            ),
            Format::Indent(more) => {
                // Increase / Decrease Indent: the left indent by half an inch, not below zero.
                let more = *more;
                self.format_paras(
                    "Indent",
                    &|p, cx, q| {
                        let left = if more { q.style.ind_left + 36.0 } else { (q.style.ind_left - 36.0).max(0.0) };
                        p.attrs("ind", &[("left", Some(twips(left))), ("start", None)], cx)
                    },
                    fonts,
                )
            }
            Format::KeepWithNext => {
                let on = !self.selected_paras().first().and_then(|&p| self.index.para(&self.doc, p)).is_some_and(|p| p.style.keep_next);
                self.format_paras(
                    "Keep With Next",
                    &|p, cx, _| {
                        p.set("keepNext", if on { format!("<{w}keepNext/>", w = cx.w) } else { format!("<{w}keepNext {w}val=\"0\"/>", w = cx.w) })
                    },
                    fonts,
                )
            }
            Format::Font(v) => {
                let v = v.clone();
                self.format_runs(
                    "Font",
                    &|p, w, _| {
                        let flag = |p: &mut Props, name: &str, on: bool| {
                            p.set(name, if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") })
                        };
                        if !v.font.is_empty() {
                            let n = crate::docx::write::props::esc(&v.font);
                            p.set("rFonts", format!("<{w}rFonts {w}ascii=\"{n}\" {w}hAnsi=\"{n}\" {w}eastAsia=\"{n}\" {w}cs=\"{n}\"/>"));
                        }
                        if let Some(sz) = v.size.filter(|s| (1.0..=1638.0).contains(s)) {
                            let hp = (sz * 2.0).round() as i64;
                            p.set("sz", format!("<{w}sz {w}val=\"{hp}\"/>"));
                            p.set("szCs", format!("<{w}szCs {w}val=\"{hp}\"/>"));
                        }
                        flag(p, "b", v.bold);
                        p.remove("bCs");
                        flag(p, "i", v.italic);
                        p.remove("iCs");
                        p.set("u", format!("<{w}u {w}val=\"{}\"/>", crate::docx::write::props::esc(&v.underline)));
                        flag(p, "strike", v.strike);
                        flag(p, "dstrike", v.dstrike);
                        let vert = match v.vert {
                            crate::docx::props::VertAlign::Superscript => "superscript",
                            crate::docx::props::VertAlign::Subscript => "subscript",
                            crate::docx::props::VertAlign::Baseline => "baseline",
                        };
                        p.set("vertAlign", format!("<{w}vertAlign {w}val=\"{vert}\"/>"));
                        flag(p, "smallCaps", v.small_caps);
                        flag(p, "caps", v.caps);
                        if v.hidden {
                            p.set("vanish", format!("<{w}vanish/>"))
                        } else {
                            p.remove("vanish")
                        }
                        if v.spacing == 0.0 {
                            p.remove("spacing")
                        } else {
                            p.set("spacing", format!("<{w}spacing {w}val=\"{}\"/>", (v.spacing * 20.0).round() as i64))
                        }
                        if v.position == 0.0 {
                            p.remove("position")
                        } else {
                            p.set("position", format!("<{w}position {w}val=\"{}\"/>", (v.position * 2.0).round() as i64))
                        }
                    },
                    fonts,
                )
            }
            Format::Paragraph(v) => {
                let v = v.clone();
                let jc = match v.jc {
                    Jc::Center => "center",
                    Jc::Right => "right",
                    Jc::Both => "both",
                    Jc::Distribute => "distribute",
                    Jc::Left => "left",
                };
                let (rule, line) = match v.line_rule {
                    crate::docx::props::LineRule::Auto => ("auto", ((v.line * 240.0).round() as i64).to_string()),
                    crate::docx::props::LineRule::AtLeast => ("atLeast", twips(v.line)),
                    crate::docx::props::LineRule::Exact => ("exact", twips(v.line)),
                };
                self.format_paras(
                    "Paragraph Formatting",
                    &|p, cx, _| {
                        let w = &cx.w;
                        p.set("jc", format!("<{w}jc {w}val=\"{jc}\"/>"));
                        let first = if v.first > 0.0 {
                            format!(" {w}firstLine=\"{}\"", twips(v.first))
                        } else if v.first < 0.0 {
                            format!(" {w}hanging=\"{}\"", twips(-v.first))
                        } else {
                            String::new()
                        };
                        p.set("ind", format!("<{w}ind {w}left=\"{}\" {w}right=\"{}\"{first}/>", twips(v.left), twips(v.right)));
                        p.set(
                            "spacing",
                            format!(
                                "<{w}spacing {w}before=\"{}\" {w}after=\"{}\" {w}line=\"{line}\" {w}lineRule=\"{rule}\"/>",
                                twips(v.before),
                                twips(v.after)
                            ),
                        );
                        for (name, on) in [("keepNext", v.keep_next), ("keepLines", v.keep_lines), ("pageBreakBefore", v.page_break_before)] {
                            if on { p.set(name, format!("<{w}{name}/>")) } else { p.remove(name) }
                        }
                        p.set("widowControl", if v.widow { format!("<{w}widowControl/>") } else { format!("<{w}widowControl {w}val=\"0\"/>") });
                    },
                    fonts,
                )
            }
            Format::ParaBorders(which) => {
                let which = which.clone();
                self.format_paras(
                    "Borders",
                    &|p, cx, _| {
                        let w = &cx.w;
                        let line = |side: &str| format!("<{w}{side} {w}val=\"single\" {w}sz=\"4\" {w}space=\"1\" {w}color=\"auto\"/>");
                        match which.as_str() {
                            "box" => p.set("pBdr", format!("<{w}pBdr>{}{}{}{}</{w}pBdr>", line("top"), line("left"), line("bottom"), line("right"))),
                            "bottom" => p.set("pBdr", format!("<{w}pBdr>{}</{w}pBdr>", line("bottom"))),
                            _ => p.remove("pBdr"),
                        }
                    },
                    fonts,
                )
            }
            Format::ParaShading(c) => {
                let c = *c;
                self.format_paras(
                    "Shading",
                    &|p, cx, _| match c {
                        Some(c) => p.set("shd", format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\" {w}fill=\"{c:06X}\"/>", w = cx.w)),
                        None => p.remove("shd"),
                    },
                    fonts,
                )
            }
            Format::ParaStyle(id) => {
                let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
                if !res.styles.by_id.contains_key(id) {
                    return Outcome::Unchanged("no such style");
                }
                let v = crate::docx::write::props::esc(id);
                self.format_paras("Style", &|p, cx, _| p.set("pStyle", format!("<{w}pStyle {w}val=\"{v}\"/>", w = cx.w)), fonts)
            }
        }
    }
}

/// The formatting commands of [`Editor::format`].
#[derive(Clone, Debug, PartialEq)]
pub enum Format {
    FontSize(f64),
    /// Grow Font (true) or Shrink Font (false): the next size in the font size list.
    GrowFont(bool),
    /// A colour (RGB), or automatic.
    FontColor(Option<u32>),
    /// A highlight colour name (`yellow`, `green`, …), or none.
    Highlight(Option<String>),
    FontFamily(String),
    ClearFormatting,
    /// Line spacing as a multiple of single.
    LineSpacing(f64),
    SpaceBefore,
    RemoveSpaceAfter,
    /// Increase (true) or Decrease Indent.
    Indent(bool),
    KeepWithNext,
    ParaStyle(String),
    /// The Font dialog: every value set.
    Font(FontValues),
    /// The Paragraph dialog: every value set.
    Paragraph(ParaValues),
    /// Borders on the selected paragraphs: `box`, `bottom` or `none` (a ½ pt single line).
    ParaBorders(String),
    /// Paragraph shading (RGB), or none.
    ParaShading(Option<u32>),
}

/// The Font dialog's values.
#[derive(Clone, Debug, PartialEq)]
pub struct FontValues {
    /// Empty: the fonts stay.
    pub font: String,
    pub size: Option<f64>,
    pub bold: bool,
    pub italic: bool,
    /// `w:u` value (`none`, `single`, `double`, …).
    pub underline: String,
    pub strike: bool,
    pub dstrike: bool,
    pub vert: crate::docx::props::VertAlign,
    pub small_caps: bool,
    pub caps: bool,
    pub hidden: bool,
    /// Character spacing and raised position, points.
    pub spacing: f64,
    pub position: f64,
}

/// The Paragraph dialog's values (lengths in points).
#[derive(Clone, Debug, PartialEq)]
pub struct ParaValues {
    pub jc: Jc,
    pub left: f64,
    pub right: f64,
    /// + first line, − hanging.
    pub first: f64,
    pub before: f64,
    pub after: f64,
    pub line_rule: crate::docx::props::LineRule,
    /// Auto: lines; exact and at least: points.
    pub line: f64,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow: bool,
}
