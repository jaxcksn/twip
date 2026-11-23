//! Character and paragraph formatting

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::Compat;
use crate::docx::props::RunProps;
use crate::docx::resolve::{Resolver, joined_style};
use crate::docx::styles::Region;
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Inline, ParaStyle, Raw, Row, Ruby, Run, RunStyle, Table};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::index::Container;
use super::step::{Step, TableProps};
use super::{Editor, Tx};

const NEW_ROOT: &str = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#;

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
    fn label(self) -> &'static str {
        match self {
            Toggle::Bold => "Bold",
            Toggle::Italic => "Italic",
            Toggle::Underline => "Underline",
            Toggle::Strike => "Strikethrough",
            Toggle::Subscript => "Subscript",
            Toggle::Superscript => "Superscript",
        }
    }

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

pub(super) fn with_run(i: &Inline, run: &Run) -> Inline {
    let run = run.clone();
    match i {
        Inline::Text(t, _) => Inline::Text(t.clone(), run),
        Inline::Tab(_) => Inline::Tab(run),
        Inline::PTab(t, _) => Inline::PTab(*t, run),
        Inline::Break(k, _) => Inline::Break(*k, run),
        Inline::Sym { ch, font, control, .. } => Inline::Sym { ch: *ch, font: font.clone(), run, control: *control },
        Inline::Picture(p, _) => Inline::Picture(p.clone(), run),
        Inline::Float(f, _) => Inline::Float(f.clone(), run),
        Inline::NoteRef { kind, id, custom, .. } => Inline::NoteRef { kind: *kind, id: *id, custom: *custom, run },
        Inline::NoteMark(k, id, _) => Inline::NoteMark(*k, *id, run),
        Inline::Separator { continuation, full, .. } => Inline::Separator { continuation: *continuation, full: *full, run },
        Inline::FieldBegin { id, kind, lock, dirty, view, label, value, form, .. } => Inline::FieldBegin {
            id: *id,
            kind: kind.clone(),
            lock: *lock,
            dirty: *dirty,
            view: *view,
            label: label.clone(),
            value: value.clone(),
            form: form.clone(),
            run,
        },
        Inline::FieldSep { id, .. } => Inline::FieldSep { id: *id, run },
        Inline::FieldEnd { id, .. } => Inline::FieldEnd { id: *id, run },
        Inline::CommentRef { id, .. } => Inline::CommentRef { id: *id, run },
        Inline::Ruby(r, _) => Inline::Ruby(r.clone(), run),
        Inline::DateBlock(name, text, _) => Inline::DateBlock(name, text.clone(), run),
        Inline::Bookmark(_) | Inline::Range(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) | Inline::Opaque(_) => i.clone(),
    }
}

/// A new `w:rPr` for a run from its own and its formatting.
pub(super) type RubyEdit<'a> = dyn Fn(Option<&str>, &RunStyle) -> Option<String> + 'a;

/// A paragraph's formatting resolved again from its direct XML, with what reading gave it beyond that
/// ([`Resolver::para_again`]), and what its runs resolve over.
pub(super) struct Again {
    pub style: ParaStyle,
    /// Each part's first unit and base: one part, or one per paragraph a vanished mark joined onto it.
    bases: Vec<(u32, RunProps)>,
}

impl Again {
    /// The part unit `at` is in.
    pub fn part(&self, at: u32) -> usize {
        self.bases.iter().rposition(|(s, _)| *s <= at).unwrap_or(0)
    }

    pub fn base(&self, at: u32) -> &RunProps {
        &self.bases[self.part(at)].1
    }

    /// What the paragraph mark resolves over: the last part's base.
    pub fn mark_base(&self) -> &RunProps {
        &self.bases[self.bases.len() - 1].1
    }
}

/// `t`'s properties a table style gives, resolved again from its and its rows' and cells' direct XML as reading resolves
/// them: its style (the default one for one it names that's gone), borders, shading, band sizes, layout, alignment,
/// indent, width and spacing, its rows' heights, splitting and header flags, and every cell's borders, shading, margins
/// and alignment, when they changed. A table joined from several as read keeps the alignment, indent and width its
/// first row took from them.
fn table_again(t: &Table, res: &Resolver, cx: &Cx) -> Option<(TableProps, Option<Vec<Row>>)> {
    use crate::docx::props::{Borders, TblProps, TcProps, TrProps};
    let colours = &res.styles.colours;
    let parse_tbl = |raw: &Raw| raw.0.as_deref().and_then(|x| cx.parse(x, |_, n| TblProps::parse(Some(n), colours))).unwrap_or_default();
    let direct = parse_tbl(&t.direct);
    let style_id = res.styles.table_style(direct.style.as_deref()).map(str::to_string);
    let style_region = res.styles.table_region(style_id.as_deref(), &[]);
    let mut tbl = style_region.tbl.clone();
    tbl.merge(&direct);
    let b = &tbl.borders;
    let mut props = TableProps::of(t);
    props.style_id = style_id.clone();
    props.borders = [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()];
    props.shd = tbl.shd.clone().filter(|s| s.flat().is_some() || s.hatch().is_some());
    props.bands = (tbl.row_band.unwrap_or(0), tbl.col_band.unwrap_or(0));
    props.fixed = tbl.fixed.unwrap_or(false);
    props.cell_spacing = tbl.cell_spacing.unwrap_or(0.0);
    let generic = res.styles.compat == Compat::Generic;
    let parse_tr = |raw: &Raw| raw.0.as_deref().and_then(|x| cx.parse(x, |_, n| TrProps::parse(Some(n)))).unwrap_or_default();
    // Joined tables' rows carry the shading of their own table: their first row's alignment came from that table.
    let joined = t.rows.iter().any(|r| r.table_shading.is_some());
    if !joined {
        let mut row_jc: Option<Option<crate::docx::props::Jc>> = None;
        let (mut first_jc, mut first_width, mut first_ind) = (None, None, None);
        for (ri, row) in t.rows.iter().enumerate() {
            let mut trp = style_region.tr.clone();
            trp.merge(&parse_tr(&row.direct));
            let ex = parse_tbl(&row.exceptions);
            trp.jc = trp.jc.or(ex.jc);
            row_jc = Some(if ri == 0 || row_jc == Some(trp.jc) { trp.jc } else { None });
            if ri == 0 {
                first_jc = trp.jc;
                let own = ex.fixed != Some(true) || tbl.fixed == Some(true);
                (first_width, first_ind) = (ex.width.filter(|_| own), ex.ind.filter(|_| own));
            }
        }
        props.jc = first_jc.filter(|_| generic && t.floating.is_none()).or(tbl.jc).or(row_jc.flatten()).unwrap_or(crate::docx::props::Jc::Left);
        props.width = first_width.or(tbl.width).unwrap_or(crate::docx::props::Width::Auto);
        let ind = first_ind.or(tbl.ind);
        (props.ind, props.ind_pct) = (ind.map_or(0.0, crate::docx::props::Width::dxa), ind.and_then(crate::docx::props::Width::pct));
    }
    let look = t.look.unwrap_or(crate::docx::props::Look {
        first_row: true,
        last_row: false,
        first_col: true,
        last_col: false,
        no_h_band: false,
        no_v_band: true,
    });
    let nrows = t.rows.len();
    let headers = t
        .rows
        .iter()
        .enumerate()
        .take_while(|(i, r)| {
            let mut trp = style_region.tr.clone();
            trp.merge(&r.direct.0.as_deref().and_then(|x| cx.parse(x, |_, n| TrProps::parse(Some(n)))).unwrap_or_default());
            trp.header == Some(true) || *i == 0 && look.first_row
        })
        .count();
    let unset = if style_id.is_none() { 0.5 } else { 0.0 };
    let mut rows = t.rows.clone();
    for (ri, row) in rows.iter_mut().enumerate() {
        let mut trp = style_region.tr.clone();
        trp.merge(&parse_tr(&row.direct));
        (row.height, row.cant_split, row.header) = (trp.height.clone(), trp.cant_split.unwrap_or(false), trp.header.unwrap_or(false));
        let ex = parse_tbl(&row.exceptions);
        let mut row_tbl = tbl.clone();
        row_tbl.borders.merge(&ex.borders);
        row_tbl.margins.merge(&ex.margins);
        let rb = &row_tbl.borders;
        row.borders = (ex.borders != Borders::default())
            .then(|| Rc::new([rb.top.clone(), rb.left.clone(), rb.bottom.clone(), rb.right.clone(), rb.inside_h.clone(), rb.inside_v.clone()]));
        let ncols = row.cells.len();
        for (ci, c) in row.cells.iter_mut().enumerate() {
            let region = res.cell_region(style_id.as_deref(), look, props.bands, (ri, ci), (nrows, ncols), headers);
            let mut tcp = region.tc.clone();
            tcp.merge(&c.direct.0.as_deref().and_then(|x| cx.parse(x, |_, n| TcProps::parse(Some(n), colours))).unwrap_or_default());
            crate::docx::style_cell(c, &tcp, &row_tbl.margins, unset);
        }
    }
    let rows = (rows != t.rows).then_some(rows);
    Some((props, rows))
}

/// A ruby's base and annotation text resolved again over `base` from its XML, `edit` first giving the base text's runs
/// new direct formatting (from their `w:rPr` and formatting), the annotation's too when it says so. `None` when it stays
/// as it is.
pub(super) fn ruby_again(r: &Ruby, base: &RunProps, edit: Option<(&RubyEdit, bool)>, res: &Resolver, cx: &Cx) -> Option<Rc<Ruby>> {
    use crate::docx::xml::is_w;
    let raw = r.xml.0.as_deref()?;
    let (mut splices, parts) = cx.parse(raw, |text, el| {
        let off = text.len() - raw.len() - "</x>".len();
        let mut splices: Vec<(std::ops::Range<usize>, String)> = Vec::new();
        let mut parts: [Vec<(String, Option<String>)>; 2] = [Vec::new(), Vec::new()];
        for (k, name) in ["rubyBase", "rt"].into_iter().enumerate() {
            let Some(part) = el.children().find(|c| is_w(*c, name)) else { continue };
            for run in part.descendants().filter(|d| is_w(*d, "r")) {
                let rpr = run.children().find(|c| is_w(*c, "rPr"));
                let mut xml = rpr.map(|n| text[n.range()].to_string());
                let old = if k == 0 { &r.base } else { &r.text };
                // Runs with text, new properties going before their first child
                let first = run.children().find(|c| is_w(*c, "t")).and_then(|_| run.first_child());
                if let Some((edit, _)) = edit.filter(|e| k == 0 || e.1)
                    && let Some(first) = first
                    && let Some((_, old)) = old.get(parts[k].len()).or(old.last())
                {
                    let now = edit(xml.as_deref(), old);
                    if now != xml {
                        let at = match rpr {
                            Some(n) => n.range().start - off..n.range().end - off,
                            None => first.range().start - off..first.range().start - off,
                        };
                        splices.push((at, now.clone().unwrap_or_default()));
                        xml = now;
                    }
                }
                for t in run.children().filter(|c| is_w(*c, "t")) {
                    parts[k].push((t.text().unwrap_or_default().to_string(), xml.clone()));
                }
            }
        }
        (splices, parts)
    })?;
    let again = |old: &[(String, Run)], new: Vec<(String, Option<String>)>| -> Vec<(String, Run)> {
        new.into_iter()
            .enumerate()
            .map(|(k, (t, rpr))| {
                let run = match old.get(k) {
                    Some((_, o)) => res.rerun_xml(base, o, rpr.as_deref(), cx),
                    None => res.run_from_xml(base, rpr.as_deref(), cx),
                };
                (t, run)
            })
            .collect()
    };
    let [b, t] = parts;
    let (base_text, text) = (again(&r.base, b), again(&r.text, t));
    if splices.is_empty() && base_text == r.base && text == r.text {
        return None;
    }
    let mut xml = raw.to_string();
    // Last first, in document order
    splices.sort_unstable_by_key(|(at, _)| std::cmp::Reverse(at.start));
    for (at, with) in splices {
        xml.replace_range(at, &with);
    }
    Some(Rc::new(Ruby { base: base_text, text, xml: crate::docx::Raw(Some(Rc::from(xml.as_str()))), ..(*r).clone() }))
}

impl Editor {
    pub(super) fn reresolve_stale(&mut self, tx: &mut Tx) {
        if tx.stale_tables.is_empty() && tx.stale_paras.is_empty() {
            return;
        }
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        for &table in &tx.stale_tables.clone() {
            let Some(t) = self.index.table(&self.doc, table) else { continue };
            if let Some((props, rows)) = table_again(t, &res, &cx) {
                if props != TableProps::of(t) {
                    let _ = self.step(tx, Step::TableProps { table, props });
                }
                if let Some(rows) = rows {
                    let n = rows.len();
                    let _ = self.step(tx, Step::Rows { table, range: 0..n, with: rows, grid: None });
                }
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
        for para in paras {
            let Some(again) = self.para_again(para, None, &res, &cx) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mark = res.rerun(again.mark_base(), &p.mark, &cx);
            let mut cache: HashMap<(*const RunStyle, usize), Run> = HashMap::new();
            let mut changed = false;
            let mut at = 0u32;
            let mut with: Vec<Inline> = Vec::with_capacity(p.inlines.len());
            for i in &p.inlines {
                let k = again.part(at);
                at += i.units() as u32;
                let Some(run) = i.run() else {
                    with.push(i.clone());
                    continue;
                };
                let base = &again.bases[k].1;
                let new = cache
                    .entry((Rc::as_ptr(run), k))
                    .or_insert_with(|| {
                        let again = res.rerun(base, run, &cx);
                        if *again == **run { run.clone() } else { again }
                    })
                    .clone();
                changed |= !Rc::ptr_eq(&new, run);
                let mut i = with_run(i, &new);
                if let Inline::Ruby(r, _) = &mut i
                    && let Some(n) = ruby_again(r, base, None, &res, &cx)
                {
                    (*r, changed) = (n, true);
                }
                with.push(i);
            }
            let style_changed = *p.style != again.style || *p.mark != *mark;
            let (style, mark) = (Rc::new(again.style), if *p.mark != *mark { mark } else { p.mark.clone() });
            if style_changed {
                let list = p.list.clone();
                let _ = self.step(tx, Step::Para { para, style, mark, list });
            }
            if changed {
                let _ = self.step(tx, Step::Restyle { para, inlines: with });
            }
        }
    }

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

    /// The names of the part character formatting is written in: the numbering part when a list's number is
    /// formatted, the styles part when a style is, else the document's. Whether it declares the w14 namespace
    /// follows.
    pub(super) fn formatted_part_cx(&self) -> Cx {
        match (&self.label_target, &self.style_target, &self.doc.resolver) {
            (Some(t), _, _) => t.cx.clone(),
            (None, Some(_), Some(r)) => r.styles.cx(),
            _ => self.cx(),
        }
    }

    pub(super) fn region_of_para(&self, para: ParaId, res: &Resolver) -> Option<Region> {
        let slot = self.index.para_slot(para)?;
        let Container::Cell { table, row, cell } = slot.container else { return None };
        Some(res.region_of(self.index.table(&self.doc, table)?, row, cell))
    }

    /// What the runs at unit `at` of `para` resolve over.
    pub(super) fn run_base(&self, para: ParaId, at: Unit, res: &Resolver, cx: &Cx) -> Option<RunProps> {
        let mut again = self.para_again(para, None, res, cx)?;
        let k = again.part(at.0);
        Some(again.bases.swap_remove(k).1)
    }

    /// `para` resolved again from its direct `w:pPr` (`ppr` in place of it, when given) and, for a paragraph vanished
    /// marks joined onto its start, from those paragraphs' too, joined as reading joins them.
    pub(super) fn para_again(&self, para: ParaId, ppr: Option<Option<&str>>, res: &Resolver, cx: &Cx) -> Option<Again> {
        let p = self.index.para(&self.doc, para)?;
        let region = self.region_of_para(para, res);
        let last = res.para_from_xml(ppr.unwrap_or(p.style.direct.0.as_deref()), region.as_ref(), cx);
        let parts = p.src.as_ref().filter(|s| s.joined.len() + 1 == s.ranges.len()).map_or(&[][..], |s| &s.joined[..]);
        if parts.is_empty() {
            let style = res.para_again(&p.style, last.style, region.as_ref(), cx);
            return Some(Again { style, bases: vec![(0, last.base)] });
        }
        let (legacy, generic) = (res.styles.compat == Compat::Legacy, res.styles.compat == Compat::Generic);
        let len = super::step::para_len(p).0;
        let mut bases = Vec::with_capacity(parts.len() + 1);
        let (mut joined, mut spec): (Option<Rc<ParaStyle>>, Option<Rc<ParaStyle>>) = (None, None);
        let mut start = 0;
        for part in parts {
            let r = res.para_from_xml(part.ppr.0.as_deref(), region.as_ref(), cx);
            let taken = spec.take();
            let style = match joined.take() {
                None => Rc::new(r.style),
                Some(first) => Rc::new(joined_style(&first, &Rc::new(r.style), start, taken.as_deref(), legacy)),
            };
            // A style separator on empty paragraphs: the first one's alignment and indents for the next.
            if part.spec_vanish && part.end == 0 && generic {
                spec = Some(taken.unwrap_or_else(|| style.clone()));
            }
            bases.push((start.min(len), r.base));
            start = part.end;
            joined = Some(style);
        }
        let style = joined_style(joined.as_ref()?, &Rc::new(last.style), start, spec.as_deref(), legacy);
        bases.push((start.min(len), last.base));
        Some(Again { style: res.para_again(&p.style, style, region.as_ref(), cx), bases })
    }

    pub(crate) fn selected_ranges(&mut self) -> Vec<(ParaId, Unit, Unit)> {
        use super::selection::{ObjectRef, Selection};
        if self.is_discontiguous() {
            return self.multi_pieces();
        }
        match self.selection {
            Selection::Block { .. } => return self.block_pieces().into_iter().map(|(_, _, p, a, b)| (p, a, b)).collect(),
            Selection::Cells { table, anchor, focus, .. } => return self.cell_ranges(table, anchor, focus),
            Selection::Object(ObjectRef::Inline { para, offset }) => return vec![(para, offset, Unit(offset.0 + 1))],
            _ => {}
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

    /// Every paragraph of the cells from `anchor` to `focus` (nested tables' too), whole.
    fn cell_ranges(
        &self,
        table: crate::layout::TableId,
        anchor: super::selection::CellRef,
        focus: super::selection::CellRef,
    ) -> Vec<(ParaId, Unit, Unit)> {
        fn paras(blocks: &[crate::docx::Block], out: &mut Vec<(ParaId, Unit, Unit)>) {
            for b in blocks {
                match b {
                    crate::docx::Block::Para(p) => out.push((ParaId(p.id), Unit(0), super::step::para_len(p))),
                    crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| paras(&c.blocks, out)),
                }
            }
        }
        let Some(t) = self.index.table(&self.doc, table) else { return Vec::new() };
        let g = crate::docx::grid::Grid::of(t);
        let (r0, r1) = (anchor.row.min(focus.row), anchor.row.max(focus.row));
        let (c0, c1) = (anchor.col.min(focus.col), anchor.col.max(focus.col));
        let mut out = Vec::new();
        for (r, row) in t.rows.iter().enumerate().take(r1 + 1).skip(r0) {
            for (k, cell) in row.cells.iter().enumerate() {
                if g.rows[r].get(k).is_some_and(|&(c, n, _)| c + n.max(1) > c0 && c <= c1) {
                    paras(&cell.blocks, &mut out);
                }
            }
        }
        out
    }

    /// The selected paragraphs: those the selection covers, else the caret's, else the selected object's.
    pub(super) fn selected_paras(&mut self) -> Vec<ParaId> {
        let mut v: Vec<ParaId> = self.selected_ranges().into_iter().map(|r| r.0).collect();
        if v.is_empty() {
            v.extend(self.caret().and_then(|c| c.para_offset()).map(|(p, _)| p));
        }
        if v.is_empty()
            && let super::selection::Selection::Object(super::selection::ObjectRef::Floating(f)) = self.selection
        {
            v.extend(self.index.float_anchor(f));
        }
        v.dedup();
        v
    }

    pub(super) fn pending_props(&mut self, edit: &dyn Fn(&mut Props, &str)) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        let Some(caret) = self.caret() else { return };
        let (Some((para, at)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else { return };
        let Some(base) = self.run_base(para, at, &res, &cx) else { return };
        let mut p = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        edit(&mut p, &cx.w);
        self.pending = Some(res.rerun_xml(&base, &run, p.xml().as_deref(), &cx));
    }

    pub(super) fn pending_font(&mut self, font: &str) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        let Some(caret) = self.caret() else { return };
        let (Some((para, at)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else { return };
        let Some(base) = self.run_base(para, at, &res, &cx) else { return };
        let mut p = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        let n = crate::docx::write::props::esc(font);
        let w = &cx.w;
        p.set("rFonts", format!("<{w}rFonts {w}ascii=\"{n}\" {w}hAnsi=\"{n}\" {w}eastAsia=\"{n}\" {w}cs=\"{n}\"/>"));
        self.pending = Some(res.rerun_xml(&base, &run, p.xml().as_deref(), &cx));
    }

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

    pub(super) fn format_runs(&mut self, label: &'static str, edit: &dyn Fn(&mut Props, &str, &RunStyle), fonts: &Fonts) -> Outcome {
        self.format_runs_settled(label, edit, &|_, _, _| false, fonts)
    }

    /// [`Editor::format_runs`], then `settle` sees each run as `edit` leaves it (its styles under it) and may change
    /// its properties again, returning whether it did. A style being formatted is edited alone.
    pub(super) fn format_runs_settled(
        &mut self,
        label: &'static str,
        edit: &dyn Fn(&mut Props, &str, &RunStyle),
        settle: &dyn Fn(&mut Props, &str, &RunStyle) -> bool,
        fonts: &Fonts,
    ) -> Outcome {
        if self.label_target.is_some() {
            return self.format_label_part(edit, settle);
        }
        if self.style_target.is_some() {
            return self.format_style_part("rPr", &|p, cx, _, run| edit(p, &cx.w, run));
        }
        if self.refuse_expanded() || self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let w = cx.w.clone();
        // A style separator mark keeps its flags
        let rpr = |xml: Option<&str>, run: &RunStyle, base: &RunProps, mark: bool| -> Option<String> {
            let mut p = Props::parse(xml, "rPr", &cx);
            let sep = mark && p.get("specVanish").is_some() && xml.is_some_and(|x| crate::docx::write::props::flag_of(x, "specVanish"));
            let flags = sep.then(|| ["vanish", "specVanish"].map(|k| p.get(k).map(str::to_string)));
            edit(&mut p, &w, run);
            for (k, x) in ["vanish", "specVanish"].into_iter().zip(flags.into_iter().flatten()) {
                if let Some(x) = x {
                    p.set(k, x);
                }
            }
            let new = res.rerun_xml(base, run, p.xml().as_deref(), &cx);
            settle(&mut p, &w, &new);
            p.xml()
        };
        let patch_as = |run: &RunStyle, base: &RunProps, mark: bool| -> Run {
            res.rerun_xml(base, run, rpr(run.direct.0.as_deref(), run, base, mark).as_deref(), &cx)
        };
        let patch = |run: &RunStyle, base: &RunProps| patch_as(run, base, false);
        let mut ranges: Vec<(ParaId, Unit, Unit)> = Vec::new();
        // The paragraphs whose marks are in the selection.
        let mut marks: Vec<ParaId> = Vec::new();
        if self.selection.is_collapsed() {
            let Some(caret) = self.caret() else { return Outcome::Unchanged("no caret") };
            let (Some((para, off)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else { return Outcome::Unchanged("no paragraph") };
            let Some(base) = self.run_base(para, off, &res, &cx) else { return Outcome::Unchanged("no paragraph") };
            self.pending = Some(patch(&run, &base));
            let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Moved };
            let text = super::text::UnitText::of(p);
            match text.letters_around(off) {
                Some((a, b)) => ranges.push((para, a, b)),
                None if off == text.len() => marks.push(para),
                None => return Outcome::Moved,
            }
        } else {
            ranges = self.selected_ranges();
            marks = self.selected_marks(&ranges);
        }
        let mut tx = self.begin(label);
        for para in marks {
            let Some(again) = self.para_again(para, None, &res, &cx) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mark = patch_as(&p.mark, again.mark_base(), true);
            if *mark == *p.mark {
                continue;
            }
            let (style, list) = (p.style.clone(), p.list.clone());
            if self.step(&mut tx, Step::Para { para, style, mark, list }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("formatting failed");
            }
        }
        for (para, a, b) in ranges.into_iter().filter(|r| r.1 < r.2) {
            let Some(again) = self.para_again(para, None, &res, &cx) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut cache: HashMap<(*const RunStyle, usize), Run> = HashMap::new();
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
                let k = again.part(s);
                let base = &again.bases[k].1;
                let new = cache.entry((Rc::as_ptr(run), k)).or_insert_with(|| patch(run, base)).clone();
                match i {
                    Inline::Text(text, _) => {
                        let (from, to) = (a.0.max(s) - s, b.0.min(e) - s);
                        let part: String = text.chars().skip(from as usize).take((to - from) as usize).collect();
                        with.push(Inline::Text(part, new));
                    }
                    Inline::Ruby(r, _) => {
                        let edit = |xml: Option<&str>, run: &RunStyle| rpr(xml, run, base, false);
                        with.push(Inline::Ruby(ruby_again(r, base, Some((&edit, false)), &res, &cx).unwrap_or_else(|| r.clone()), new));
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

    /// The paragraphs of `ranges`, the selected ones, whose marks are selected too: every one the selection runs past
    /// (to the next paragraph's start, say), and the last when the selection runs to its story's end (a selection can't
    /// end after its last mark otherwise); in selected cells, every one. A column block holds none.
    pub(super) fn selected_marks(&mut self, ranges: &[(ParaId, Unit, Unit)]) -> Vec<ParaId> {
        use super::selection::Selection;
        if self.is_discontiguous() {
            return self.multi_marks();
        }
        match self.selection {
            Selection::Block { .. } => return Vec::new(),
            Selection::Cells { .. } => return ranges.iter().map(|r| r.0).collect(),
            _ => {}
        }
        let Some(&(last, _, end)) = ranges.last() else { return Vec::new() };
        let mut out: Vec<ParaId> = ranges[..ranges.len() - 1].iter().map(|r| r.0).collect();
        let len = self.index.para(&self.doc, last).map_or(Unit(0), super::step::para_len);
        if end == len
            && let Some(story) = self.index.para_slot(last).map(|s| self.index.story(s.container))
        {
            let at = super::position::Position::text(story, last, len);
            let past = self.selection.ordered(&self.index).is_some_and(|(_, b)| self.index.cmp(&b, &at) == std::cmp::Ordering::Greater);
            if past || self.nav().right(at, false) == at {
                out.push(last);
            }
        }
        out
    }

    pub(super) fn format_paras(&mut self, label: &'static str, edit: &dyn Fn(&mut Props, &Cx, &crate::docx::Paragraph), fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return self.format_style_part("pPr", &|p, cx, para, _| edit(p, cx, para));
        }
        if let Some(id) = self.auto_redefined() {
            return self.format_auto_redefined(&id, edit, fonts);
        }
        self.format_paras_direct(label, edit, fonts)
    }

    /// [`Self::format_paras`] on the paragraphs themselves, even in a style that updates automatically: for what
    /// isn't direct formatting (applying a style).
    pub(super) fn format_paras_direct(
        &mut self,
        label: &'static str,
        edit: &dyn Fn(&mut Props, &Cx, &crate::docx::Paragraph),
        fonts: &Fonts,
    ) -> Outcome {
        if self.refuse_expanded() || self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let paras = self.selected_paras();
        let mut tx = self.begin(label);
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
            edit(&mut pr, &cx, p);
            let Some(r) = self.para_again(para, Some(pr.xml().as_deref()), &res, &cx) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if r.style == *p.style {
                continue;
            }
            let (mark, list) = (p.mark.clone(), p.list.clone());
            if self.step(&mut tx, Step::Para { para, style: Rc::new(r.style), mark, list }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("formatting failed");
            }
            tx.stale_paras.push(para);
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn format(&mut self, f: &Format, fonts: &Fonts) -> Outcome {
        let twips = |pt: f64| ((pt * 20.0).round() as i64).to_string();
        match f {
            &Format::Set(t, on) => self.format_runs(t.label(), &|p, w, _| t.set(p, w, on), fonts),
            Format::FontSize(pt) => {
                let hp = ((pt * 2.0).round() as i64).clamp(2, 3276);
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
                        let hp = ((next * 2.0).round() as i64).clamp(2, 3276);
                        p.set("sz", format!("<{w}sz {w}val=\"{hp}\"/>"));
                        p.set("szCs", format!("<{w}szCs {w}val=\"{hp}\"/>"));
                    },
                    fonts,
                )
            }
            Format::FontColor(c) => {
                let c = *c;
                self.format_runs(
                    "Font Color",
                    &|p, w, _| p.set("color", super::commands::colour_attrs(&format!("<{w}color/>"), w, "val", super::commands::THEME_COLOR, c)),
                    fonts,
                )
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
            Format::ClearFormatting => self.format_runs("Clear Formatting", &|p, _, _| *p = Props::parse(None, "rPr", &Cx::of_xml(NEW_ROOT)), fonts),
            Format::Indent(more) => {
                let more = *more;
                self.indent_list(more, fonts).unwrap_or_else(|| self.indent_plain(more, fonts))
            }
            Format::Hanging(more) => {
                let (more, tab) = (*more, self.tab_grid());
                self.format_paras(
                    "Hanging Indent",
                    &|p, cx, q| {
                        // The first line stays where it is; the other lines move to the next or last tab stop.
                        let first_line = q.style.ind_left + q.style.ind_first;
                        let left = grid_step(q.style.ind_left, tab, more);
                        let first = first_line - left;
                        let (first, hanging) = if first < 0.0 { (None, Some(twips(-first))) } else { (Some(twips(first)), None) };
                        p.attrs("ind", &[("left", Some(twips(left))), ("start", None), ("firstLine", first), ("hanging", hanging)], cx)
                    },
                    fonts,
                )
            }
            &Format::StepFont(grow) => self.format_runs(
                "Font Size",
                &|p, w, run| {
                    let next = if grow { run.size + 1.0 } else { run.size - 1.0 };
                    let hp = ((next * 2.0).round() as i64).clamp(2, 3276);
                    p.set("sz", format!("<{w}sz {w}val=\"{hp}\"/>"));
                    p.set("szCs", format!("<{w}szCs {w}val=\"{hp}\"/>"));
                },
                fonts,
            ),
            Format::ClearParagraph => self.format_paras_direct(
                "Reset Paragraph",
                &|p, _, _| p.kids.retain(|(n, _)| matches!(n.as_str(), "pStyle" | "rPr" | "sectPr" | "pPrChange" | "cnfStyle" | "divId")),
                fonts,
            ),
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
                if !res.styles.by_id.get(id).is_some_and(|s| s.kind == crate::docx::styles::StyleKind::Paragraph) {
                    return Outcome::Unchanged("no such paragraph style");
                }
                let v = crate::docx::write::props::esc(id);
                self.format_paras_direct("Style", &|p, cx, _| p.set("pStyle", format!("<{w}pStyle {w}val=\"{v}\"/>", w = cx.w)), fonts)
            }
            Format::CharStyle(id) => {
                let v = crate::docx::write::props::esc(id);
                self.format_runs("Style", &|p, w, _| p.set("rStyle", format!("<{w}rStyle {w}val=\"{v}\"/>")), fonts)
            }
        }
    }
}

impl Editor {
    /// The default tab stop, which indents step along.
    pub(super) fn tab_grid(&self) -> f64 {
        let t = self.doc.settings.default_tab;
        if t >= 1.0 { t } else { 36.0 }
    }

    /// Increase or Decrease Indent on the selected paragraphs that aren't list items: their left indents to the next (or
    /// last) default tab stop.
    fn indent_plain(&mut self, more: bool, fonts: &Fonts) -> Outcome {
        let tab = self.tab_grid();
        let twips = |pt: f64| ((pt * 20.0).round() as i64).to_string();
        self.format_paras(
            "Indent",
            &|p, cx, q| {
                if q.style.num.is_none_or(|(id, _)| id == 0) {
                    p.attrs("ind", &[("left", Some(twips(grid_step(q.style.ind_left, tab, more)))), ("start", None)], cx);
                }
            },
            fonts,
        )
    }

    /// Increase or Decrease Indent with list items selected: the whole list moves when its first selected item is the
    /// list's first (at the first level), else the items change level; other paragraphs selected with them step along the
    /// tab grid, in the same undo step. `None` when no selected paragraph is a list item.
    fn indent_list(&mut self, more: bool, fonts: &Fonts) -> Option<Outcome> {
        if self.style_target.is_some() {
            return None;
        }
        let paras = self.selected_paras();
        let items = self.list_items(&paras);
        let &(first, (num, ilvl)) = items.first()?;
        let whole = ilvl == 0 && self.first_in_list(first, num);
        let list = move |e: &mut Self| if whole { e.shift_list(num, more, fonts) } else { e.list_level(!more, fonts) };
        if items.len() == paras.len() {
            return Some(list(self));
        }
        Some(self.joined("Indent", fonts, |e| {
            let items = list(e);
            if matches!(items, Outcome::Refused(_)) {
                return items;
            }
            let plain = e.indent_plain(more, fonts);
            if matches!(plain, Outcome::Changed(_)) { plain } else { items }
        }))
    }

    /// Whether `para` is list `num`'s first paragraph in the order the list numbers through them, its stories and
    /// tables too: a walk that stops at the list's first paragraph.
    fn first_in_list(&self, para: ParaId, num: i64) -> bool {
        super::lists::first_numbered(&self.doc, &self.index, num) == Some(para)
    }

    /// Moves every level of list `num` a default tab stop right (or left, no further than the margin).
    fn shift_list(&mut self, num: i64, more: bool, fonts: &Fonts) -> Outcome {
        use crate::api::command::{ListApplyTo, ListDefinitionChange};
        use crate::api::value::Points;
        let Some(mut levels) = self.list_levels(num) else { return Outcome::Unchanged("no list") };
        let tab = self.tab_grid();
        let least = levels.iter().map(|l| l.number_position.0.min(l.text_indent.0)).fold(f64::MAX, f64::min);
        let by = if more { tab } else { -tab.min(least.max(0.0)) };
        if by == 0.0 {
            return Outcome::Unchanged("the list is at the margin");
        }
        for l in &mut levels {
            l.number_position = Points(l.number_position.0 + by);
            l.text_indent = Points(l.text_indent.0 + by);
            l.tab = l.tab.map(|t| Points(t.0 + by));
        }
        self.set_list_definition(ListDefinitionChange { levels, apply_to: ListApplyTo::WholeList, name: None }, fonts)
    }

    /// The hidden paragraph marks of `para`: where each paragraph a vanished mark joined onto it as read ends, then its
    /// own mark at its end when that is hidden (made since reading: it joins the next paragraph), each with whether it
    /// is a style separator (`specVanish`).
    pub fn hidden_marks(&self, para: ParaId) -> Vec<(Unit, bool)> {
        let Some(p) = self.index.para(&self.doc, para) else { return Vec::new() };
        let parts = p.src.as_ref().filter(|s| s.joined.len() + 1 == s.ranges.len()).map_or(&[][..], |s| &s.joined[..]);
        let own = p
            .mark
            .hidden
            .then(|| (super::step::para_len(p), p.mark.direct.0.as_deref().is_some_and(|x| crate::docx::write::props::flag_of(x, "specVanish"))));
        parts.iter().map(|j| (Unit(j.end), j.spec_vanish)).chain(own).collect()
    }

    /// Ctrl+Alt+Enter: a style separator, a hidden paragraph mark (`w:vanish`, `w:specVanish`) that runs the paragraph
    /// before it into the one after, each keeping its own properties. At a paragraph's end with a paragraph after it,
    /// that paragraph's mark becomes one; elsewhere the paragraph splits at the caret as Enter splits it (a selection
    /// replaced) and the first part's mark becomes one. One undo step, the caret after it.
    pub fn insert_style_separator(&mut self, fonts: &Fonts) -> Outcome {
        if self.refuse_expanded() || self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let Some((para, at)) = self.selection.focus().and_then(|p| p.para_offset()) else { return Outcome::unavailable("no caret in text") };
        let Some(slot) = self.index.para_slot(para) else { return Outcome::Unchanged("no paragraph") };
        let at_end = self.selection.is_collapsed()
            && self.index.para(&self.doc, para).is_some_and(|p| super::step::para_len(p) == at)
            && matches!(self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(slot.block + 1)), Some(crate::docx::Block::Para(_)));
        self.joined("Style Separator", fonts, |e| {
            let target = if at_end {
                para
            } else {
                match e.split_at_caret(false, fonts) {
                    (Outcome::Changed(_), Some(first)) => first,
                    (out, _) => return out,
                }
            };
            e.separator_mark(target, fonts)
        })
    }

    /// `para`'s mark made a style separator.
    fn separator_mark(&mut self, para: ParaId, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let (Some(again), Some(p)) = (self.para_again(para, None, &res, &cx), self.index.para(&self.doc, para)) else {
            return Outcome::Unchanged("no paragraph");
        };
        let mut props = Props::parse(p.mark.direct.0.as_deref(), "rPr", &cx);
        let w = &cx.w;
        props.set("vanish", format!("<{w}vanish/>"));
        props.set("specVanish", format!("<{w}specVanish/>"));
        let mark = res.rerun_xml(again.mark_base(), &p.mark, props.xml().as_deref(), &cx);
        let (style, list) = (p.style.clone(), p.list.clone());
        let mut tx = self.begin("Style Separator");
        if self.step(&mut tx, Step::Para { para, style, mark, list }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the mark stayed");
        }
        tx.stale_paras.push(para);
        self.commit(tx, None, fonts)
    }

    /// Ctrl+] and Ctrl+[: the selection's font sizes a point larger or smaller, each run its own.
    pub fn step_font_size(&mut self, grow: bool, fonts: &Fonts) -> Outcome {
        self.format(&Format::StepFont(grow), fonts)
    }

    /// Ctrl+T and Ctrl+Shift+T: the selected paragraphs' hanging indent a default tab stop larger or smaller, the first
    /// line staying where it is.
    pub fn hanging_indent(&mut self, more: bool, fonts: &Fonts) -> Outcome {
        self.format(&Format::Hanging(more), fonts)
    }

    /// Ctrl+Q: the selected paragraphs' direct formatting removed, their styles kept.
    pub fn clear_paragraph_formatting(&mut self, fonts: &Fonts) -> Outcome {
        self.format(&Format::ClearParagraph, fonts)
    }

    /// Clear All Formatting: the selection's direct character formatting (its highlight aside) and character styles
    /// removed, and its paragraphs' direct formatting, the paragraphs in the default paragraph style. One undo step.
    pub fn clear_all_formatting(&mut self, fonts: &Fonts) -> Outcome {
        self.joined("Clear Formatting", fonts, |e| {
            let runs = e.format_runs(
                "Clear Formatting",
                &|p, _, _| p.kids.retain(|(n, _)| matches!(n.as_str(), "highlight" | "ins" | "del" | "moveFrom" | "moveTo" | "rPrChange")),
                fonts,
            );
            if matches!(runs, Outcome::Refused(_)) {
                return runs;
            }
            let paras = e.format_paras_direct(
                "Clear Formatting",
                &|p, _, _| p.kids.retain(|(n, _)| matches!(n.as_str(), "rPr" | "sectPr" | "pPrChange" | "cnfStyle" | "divId")),
                fonts,
            );
            if matches!(paras, Outcome::Changed(_)) { paras } else { runs }
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Format {
    Set(Toggle, bool),
    FontSize(f64),
    GrowFont(bool),
    /// A colour, or automatic.
    FontColor(Option<super::theme::WordColour>),
    Highlight(Option<String>),
    FontFamily(String),
    ClearFormatting,
    Indent(bool),
    /// A hanging indent one tab stop larger (or smaller).
    Hanging(bool),
    /// Every run's size a point larger (or smaller).
    StepFont(bool),
    /// The paragraphs' direct formatting removed, their styles kept.
    ClearParagraph,
    ParaStyle(String),
    CharStyle(String),
    ParaBorders(String),
    ParaShading(Option<u32>),
}

/// `left` moved to the next (or last) multiple of `tab`, within 0 to 1584 points.
pub(super) fn grid_step(left: f64, tab: f64, more: bool) -> f64 {
    let at = left / tab;
    let to = if more { (at + 1e-6).floor() + 1.0 } else { (at - 1e-6).ceil() - 1.0 };
    (to * tab).clamp(0.0, 1584.0)
}

#[test]
fn indents_step_along_the_tab_grid() {
    assert_eq!(grid_step(0.0, 36.0, true), 36.0);
    assert_eq!(grid_step(10.0, 36.0, true), 36.0);
    assert_eq!(grid_step(36.0, 36.0, true), 72.0);
    assert_eq!(grid_step(40.0, 36.0, false), 36.0);
    assert_eq!(grid_step(36.0, 36.0, false), 0.0);
    assert_eq!(grid_step(0.0, 36.0, false), 0.0);
    assert_eq!(grid_step(1580.0, 36.0, true), 1584.0);
}
