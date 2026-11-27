//! Character and paragraph formatting

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::Compat;
use crate::docx::para_props::{self, ParaDirect};
use crate::docx::para_xml;
use crate::docx::resolve::joined_style;
use crate::docx::resolvers::Resolvers;
use crate::docx::write::props::Cx;
use crate::docx::{Inline, ParaStyle, Row, Ruby, Run, RunStyle, Table};
use crate::font::Fonts;
use crate::format::cascade::TableRegion;
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::index::Container;
use super::step::{Step, TableProps};
use super::{Editor, Tx};
use crate::document::format::ShadingPattern;
use crate::document::line::BorderStyle;
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::char_props::RunDirect;
use crate::docx::run_xml;
use crate::format::{CharEdit, CharProps, Color, ParaBorders, ParaEdit, ParaProp, Prop, ScriptClass, Shading, TextBorder};

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

    fn set(self, e: &mut CharEdit, on: bool) {
        use crate::format::Toggle as T;
        match self {
            Toggle::Bold => {
                e.toggle(T::BOLD, on);
                e.toggle(T::BOLD_COMPLEX, on);
            }
            Toggle::Italic => {
                e.toggle(T::ITALIC, on);
                e.toggle(T::ITALIC_COMPLEX, on);
            }
            Toggle::Underline => {
                let style = if on { crate::docx::Underline::Single } else { crate::docx::Underline::None };
                e.underline = Some(crate::format::UnderlineProps { style, color: None });
                e.whole(Prop::Underline);
            }
            Toggle::Strike => e.toggle(T::STRIKE, on),
            Toggle::Subscript | Toggle::Superscript => {
                use crate::docx::props::VertAlign;
                e.vertical = Some(match (self, on) {
                    (_, false) => VertAlign::Baseline,
                    (Toggle::Subscript, true) => VertAlign::Subscript,
                    _ => VertAlign::Superscript,
                });
                e.whole(Prop::Vertical);
            }
        }
    }
}

/// A size in points as half-points can hold it, between 1 and 1638.
fn half_points(pt: f64) -> f64 {
    ((pt * 2.0).round() as i64).clamp(2, 3276) as f64 / 2.0
}

/// Sets the size of every script class.
fn sized(e: &mut CharEdit, pt: f64) {
    (e.size, e.complex_size) = (Some(pt), Some(pt));
    e.whole(Prop::Size);
    e.whole(Prop::ComplexSize);
}

/// Points to the nearest twip, as the paragraph commands write lengths.
pub(in crate::editing) fn twip(pt: f64) -> f64 {
    (pt * 20.0).round() / 20.0
}

/// A colour as a command chose it: its theme colour when it is one.
pub(in crate::editing) fn colour(c: super::theme::WordColour) -> crate::format::Color {
    c.theme.filter(|r| crate::docx::props::theme_name(r.slot).is_some()).map_or(crate::format::Color::Rgb(c.rgb), crate::format::Color::Theme)
}

/// Every script class's font the family `name`, with no hint: set whole, the classes in the order `order` gives.
pub(super) fn family(e: &mut CharEdit, name: &str, order: [ScriptClass; 4]) {
    let f = Some(crate::format::FontRef::Family(name.into()));
    e.fonts = crate::format::ScriptFonts { latin: f.clone(), latin_extended: f.clone(), east_asian: f.clone(), complex: f };
    e.hint = None;
    for c in order {
        e.whole(Prop::Font(c));
    }
}

/// The order the font command sets the script classes' fonts in.
const FONT_ORDER: [ScriptClass; 4] = [ScriptClass::Latin, ScriptClass::LatinExtended, ScriptClass::Complex, ScriptClass::EastAsian];
/// The order a font for the text typed next is set in.
const PENDING_FONT_ORDER: [ScriptClass; 4] = [ScriptClass::Latin, ScriptClass::LatinExtended, ScriptClass::EastAsian, ScriptClass::Complex];

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
        // A field edge's run carries its tracked changes.
        Inline::Opaque(o) if o.field.is_some() => {
            Inline::Opaque(Rc::new(crate::docx::Opaque { xml: o.xml.clone(), field: o.field.clone(), src: o.src.clone(), run: Some(run) }))
        }
        Inline::Bookmark(_) | Inline::Range(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) | Inline::Opaque(_) => i.clone(),
    }
}

/// `p` with its highlight alone, and what it keeps beside it.
fn highlight_alone(p: &CharProps) -> CharProps {
    let highlight = |q: &crate::format::QName| &*q.ns == crate::docx::xml::W && &*q.local == "highlight";
    let foreign = p.foreign.as_deref().map(|f| crate::format::ForeignProps {
        children: Vec::new(),
        attributes: f.attributes.iter().filter(|a| a.on.as_ref().is_none_or(highlight)).cloned().collect(),
        kept: f.kept.iter().filter(|c| highlight(&c.name)).cloned().collect(),
    });
    CharProps { highlight: p.highlight, foreign: foreign.filter(|f| *f != Default::default()).map(Rc::new), ..Default::default() }
}

/// A run's new authored properties from its own and its formatting; `None` leaves them.
pub(super) type RubyEdit<'a> = dyn Fn(&RunDirect, &RunStyle) -> Option<RunDirect> + 'a;

/// A [`RubyEdit`] that edits each run's authored properties with `edit`, which says whether it changed them.
pub(super) fn ruby_edit(edit: impl Fn(&mut CharEdit, &RunStyle) -> bool) -> impl Fn(&RunDirect, &RunStyle) -> Option<RunDirect> {
    move |d, run| {
        let mut e = CharEdit::new(d.props().map_or_else(CharProps::default, |p| (**p).clone()));
        edit(&mut e, run).then(|| d.edit(e))
    }
}

/// A style separator mark's flags (hidden, and the separator's), which formatting keeps.
struct Separator {
    hidden: Option<bool>,
    spec: Option<bool>,
}

fn separator(p: &CharProps) -> Separator {
    Separator { hidden: p.toggles.get(crate::format::Toggle::HIDDEN), spec: p.spec_vanish }
}

impl Separator {
    fn restore(&self, p: &mut CharProps) {
        if let Some(on) = self.hidden {
            p.toggles.put(crate::format::Toggle::HIDDEN, on);
        }
        if self.spec.is_some() {
            p.spec_vanish = self.spec;
        }
    }
}

/// `xml`, a definition's `w:rPr` (a style's, a list level's) in the part `cx` describes, with a typed edit of its
/// properties, written again as an edit of it; as it is when `edit` changes nothing.
#[cfg(test)]
pub(super) fn edit_rpr_xml(
    xml: Option<&str>,
    colours: &crate::docx::props::ThemeColours,
    cx: &Cx,
    edit: impl FnOnce(&mut CharEdit) -> bool,
) -> Option<String> {
    let direct = RunDirect::from_xml(xml, cx);
    let mut e = CharEdit::new(direct.props().map_or_else(CharProps::default, |q| (**q).clone()));
    if !edit(&mut e) {
        return xml.map(str::to_string);
    }
    direct.edit(e).xml(colours, cx).map(std::borrow::Cow::into_owned)
}

/// `xml` (a `w:pPr`, or none) with `edit`'s typed edit of the properties it holds, written over it; as it is when
/// `edit` returns false.
#[cfg(test)]
pub(super) fn edit_ppr_xml(
    xml: Option<&str>,
    colours: &crate::docx::props::ThemeColours,
    cx: &Cx,
    edit: impl FnOnce(&mut ParaEdit) -> bool,
) -> Option<String> {
    let direct = ParaDirect::from_xml(xml, cx);
    let mut e = ParaEdit::new((**direct.get()).clone());
    if !edit(&mut e) {
        return xml.map(str::to_string);
    }
    direct.edit(e).xml(colours, cx).map(std::borrow::Cow::into_owned)
}

/// A paragraph's formatting resolved again from its direct XML, with what reading gave it beyond that
/// ([`Resolver::para_again`]), and what its runs resolve over.
pub(super) struct Again {
    pub style: ParaStyle,
    /// Each part's first unit and base: one part, or one per paragraph a vanished mark joined onto it.
    bases: Vec<(u32, CharProps)>,
}

impl Again {
    /// The part unit `at` is in.
    pub fn part(&self, at: u32) -> usize {
        self.bases.iter().rposition(|(s, _)| *s <= at).unwrap_or(0)
    }

    pub fn base(&self, at: u32) -> &CharProps {
        &self.bases[self.part(at)].1
    }

    /// What the paragraph mark resolves over: the last part's base.
    pub fn mark_base(&self) -> &CharProps {
        &self.bases[self.bases.len() - 1].1
    }
}

/// `t`'s properties a table style gives, resolved again from its and its rows' and cells' authored properties as reading
/// resolves them: its style (the default one for one it names that's gone), borders, shading, band sizes, layout, alignment,
/// indent, width and spacing, its rows' heights, splitting and header flags, and every cell's borders, shading, margins
/// and alignment, when they changed. A table joined from several as read keeps the alignment, indent and width its
/// first row took from them.
fn table_again(t: &Table, res: &Resolvers) -> Option<(TableProps, Option<Vec<Row>>)> {
    use crate::docx::props::Borders;
    use crate::docx::table_props::{cell_on, row_on, table_on, tbl_props};
    let c = res.cascade();
    let colours = &c.colours;
    let style_id = c.table_style(t.direct.get().style.as_deref()).map(str::to_string);
    let style_region = c.table_region(style_id.as_deref(), &[]);
    let tbl = table_on(&style_region.format.table, t.direct.get(), colours);
    let b = &tbl.borders;
    let mut props = TableProps::of(t);
    props.style_id = style_id.clone();
    props.borders = [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()];
    props.shd = tbl.shd.clone().filter(|s| s.flat().is_some() || s.hatch().is_some());
    props.bands = (tbl.row_band.unwrap_or(0), tbl.col_band.unwrap_or(0));
    props.fixed = tbl.fixed.unwrap_or(false);
    props.cell_spacing = tbl.cell_spacing.unwrap_or(0.0);
    let generic = c.rules.compat == Compat::Generic;
    // Joined tables' rows carry the shading of their own table: their first row's alignment came from that table.
    let joined = t.rows.iter().any(|r| r.table_shading.is_some());
    if !joined {
        let mut row_jc: Option<Option<crate::docx::props::Jc>> = None;
        let (mut first_jc, mut first_width, mut first_ind) = (None, None, None);
        for (ri, row) in t.rows.iter().enumerate() {
            let mut trp = row_on(&style_region.format.row, row.direct.get());
            let ex = tbl_props(row.exceptions.get(), colours);
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
        .take_while(|(i, r)| row_on(&style_region.format.row, r.direct.get()).header == Some(true) || *i == 0 && look.first_row)
        .count();
    let unset = if style_id.is_none() { 0.5 } else { 0.0 };
    let mut rows = t.rows.clone();
    for (ri, row) in rows.iter_mut().enumerate() {
        let trp = row_on(&style_region.format.row, row.direct.get());
        (row.height, row.cant_split, row.header) = (trp.height, trp.cant_split.unwrap_or(false), trp.header.unwrap_or(false));
        let ex = tbl_props(row.exceptions.get(), colours);
        let mut row_tbl = tbl.clone();
        row_tbl.borders.merge(&ex.borders);
        row_tbl.margins.merge(&ex.margins);
        let rb = &row_tbl.borders;
        row.borders = (ex.borders != Borders::default())
            .then(|| Rc::new([rb.top.clone(), rb.left.clone(), rb.bottom.clone(), rb.right.clone(), rb.inside_h.clone(), rb.inside_v.clone()]));
        let ncols = row.cells.len();
        for (ci, cell) in row.cells.iter_mut().enumerate() {
            let region = c.cell_region(style_id.as_deref(), look, props.bands, (ri, ci), (nrows, ncols), headers);
            let tcp = cell_on(&region.format.cell, cell.direct.get(), colours);
            crate::docx::style_cell(cell, &tcp, &row_tbl.margins, unset);
        }
    }
    let rows = (rows != t.rows).then_some(rows);
    Some((props, rows))
}

/// A ruby's base and annotation text resolved again over `base` from the authored properties its XML holds, `edit`
/// first giving the base text's runs new ones (from their own and their formatting), the annotation's too when it says
/// so, written into the XML. `None` when it stays as it is.
pub(super) fn ruby_again(r: &Ruby, base: &CharProps, edit: Option<(&RubyEdit, bool)>, res: &Resolvers, cx: &Cx) -> Option<Rc<Ruby>> {
    use crate::docx::xml::is_w;
    let raw = r.xml.0.as_deref()?;
    let (mut splices, parts) = cx.parse(raw, |text, el| {
        let off = text.len() - raw.len() - "</x>".len();
        let mut splices: Vec<(std::ops::Range<usize>, String)> = Vec::new();
        let mut parts: [Vec<(String, RunDirect)>; 2] = [Vec::new(), Vec::new()];
        for (k, name) in ["rubyBase", "rt"].into_iter().enumerate() {
            let Some(part) = el.children().find(|c| is_w(*c, name)) else { continue };
            for run in part.descendants().filter(|d| is_w(*d, "r")) {
                let rpr = run.children().find(|c| is_w(*c, "rPr"));
                let xml = rpr.map(|n| &text[n.range()]);
                let mut direct = RunDirect::from_xml(xml, cx);
                let old = if k == 0 { &r.base } else { &r.text };
                // Runs with text, new properties going before their first child
                let first = run.children().find(|c| is_w(*c, "t")).and_then(|_| run.first_child());
                if let Some((edit, _)) = edit.filter(|e| k == 0 || e.1)
                    && let Some(first) = first
                    && let Some((_, old)) = old.get(parts[k].len()).or(old.last())
                    && let Some(d) = edit(&direct, old)
                {
                    let now = d.xml(&res.cascade().colours, cx);
                    if now.as_deref() != xml {
                        let at = match rpr {
                            Some(n) => n.range().start - off..n.range().end - off,
                            None => first.range().start - off..first.range().start - off,
                        };
                        let now = now.map(std::borrow::Cow::into_owned);
                        direct = RunDirect::from_xml(now.as_deref(), cx);
                        splices.push((at, now.unwrap_or_default()));
                    }
                }
                for t in run.children().filter(|c| is_w(*c, "t")) {
                    parts[k].push((t.text().unwrap_or_default().to_string(), direct.clone()));
                }
            }
        }
        (splices, parts)
    })?;
    let typed = base.clone();
    let again = |old: &[(String, Run)], new: Vec<(String, RunDirect)>| -> Vec<(String, Run)> {
        new.into_iter()
            .enumerate()
            .map(|(k, (t, direct))| {
                let run = match old.get(k) {
                    Some((_, o)) => run_xml::again(res, &typed, o, direct),
                    None => Rc::new(run_xml::resolve(res, &typed, direct, false)),
                };
                (t, run)
            })
            .collect()
    };
    let [b, t] = parts;
    let (base_text, text) = (again(&r.base, b), again(&r.text, t));
    let same = |a: &[(String, crate::docx::Run)], b: &[(String, crate::docx::Run)]| {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 && x.1.layout_eq(&y.1))
    };
    if splices.is_empty() && same(&base_text, &r.base) && same(&text, &r.text) {
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
            if let Some((props, rows)) = table_again(t, &res) {
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
            let mark = run_xml::rerun(&res, again.mark_base(), &p.mark);
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
                        let again = run_xml::rerun(&res, base, run);
                        if again.layout_eq(run) { run.clone() } else { again }
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
            let style_changed = !p.style.layout_eq(&again.style) || !p.mark.layout_eq(&mark);
            let (style, mark) = (Rc::new(again.style), if !p.mark.layout_eq(&mark) { mark } else { p.mark.clone() });
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
        let c = res.cascade();
        let next = c.styles.get(id)?.next.as_deref().filter(|n| *n != id && c.styles.styles.contains_key(*n))?.to_string();
        let cx = self.cx();
        let mut e = ParaEdit::new((**p.style.direct.get()).clone());
        e.style = Some(next.as_str().into());
        e.whole(ParaProp::Style);
        let region = self.region_of_para(para, &res);
        second.style = Rc::new(para_xml::of(&res, &p.style.direct.edit(e), region.as_ref(), &cx).style);
        Some(second)
    }

    /// The theme colours a theme colour's value is written as.
    pub(super) fn colours(&self) -> crate::docx::props::ThemeColours {
        self.doc.resolver.as_ref().map_or_else(Default::default, |r| r.cascade().colours)
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
            (None, Some(_), Some(r)) => r.typed().styles.cx(),
            _ => self.cx(),
        }
    }

    pub(super) fn region_of_para(&self, para: ParaId, res: &Resolvers) -> Option<TableRegion> {
        let slot = self.index.para_slot(para)?;
        let Container::Cell { table, row, cell } = slot.container else { return None };
        Some(res.cascade().region_of(self.index.table(&self.doc, table)?, row, cell))
    }

    /// What the runs at unit `at` of `para` resolve over.
    pub(super) fn run_base(&self, para: ParaId, at: Unit, res: &Resolvers, cx: &Cx) -> Option<CharProps> {
        let mut again = self.para_again(para, None, res, cx)?;
        let k = again.part(at.0);
        Some(again.bases.swap_remove(k).1)
    }

    /// `para` resolved again from its authored properties (`direct` in place of them, when given) and, for a
    /// paragraph vanished marks joined onto its start, from those paragraphs' too, joined as reading joins them.
    pub(super) fn para_again(&self, para: ParaId, direct: Option<ParaDirect>, res: &Resolvers, cx: &Cx) -> Option<Again> {
        let p = self.index.para(&self.doc, para)?;
        let region = self.region_of_para(para, res);
        let last = para_xml::of(res, direct.as_ref().unwrap_or(&p.style.direct), region.as_ref(), cx);
        let parts = p.src.as_ref().filter(|s| s.joined.len() + 1 == s.ranges.len()).map_or(&[][..], |s| &s.joined[..]);
        if parts.is_empty() {
            let style = crate::docx::para_xml::again(res, &p.style, last.style, region.as_ref(), cx);
            return Some(Again { style, bases: vec![(0, last.base)] });
        }
        let compat = res.cascade().rules.compat;
        let (legacy, generic) = (compat == Compat::Legacy, compat == Compat::Generic);
        let len = super::step::para_len(p).0;
        let mut bases = Vec::with_capacity(parts.len() + 1);
        let (mut joined, mut spec): (Option<Rc<ParaStyle>>, Option<Rc<ParaStyle>>) = (None, None);
        let mut start = 0;
        for part in parts {
            let r = crate::docx::para_xml::of(res, &part.ppr, region.as_ref(), cx);
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
        Some(Again { style: crate::docx::para_xml::again(res, &p.style, style, region.as_ref(), cx), bases })
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
    pub(super) fn cell_ranges(
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

    pub(super) fn pending_font(&mut self, font: &str) {
        self.pending_typed(&|e| family(e, font, PENDING_FONT_ORDER));
    }

    /// The formatting text typed at the caret takes: its run's, with a typed edit of its authored properties.
    pub(super) fn pending_typed(&mut self, edit: &dyn Fn(&mut CharEdit)) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        let Some(caret) = self.caret() else { return };
        let (Some((para, at)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else { return };
        let Some(base) = self.run_base(para, at, &res, &cx) else { return };
        let mut e = CharEdit::new(run.direct.props().map_or_else(CharProps::default, |p| (**p).clone()));
        edit(&mut e);
        self.pending = Some(run_xml::again(&res, &base, &run, run.direct.edit(e)));
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

    /// Formats the selected runs with a typed edit of their authored properties.
    pub(super) fn format_runs_typed(&mut self, label: &'static str, edit: &dyn Fn(&mut CharEdit, &RunStyle), fonts: &Fonts) -> Outcome {
        self.format_runs_typed_settled(label, edit, &|_, _| false, fonts)
    }

    /// [`Editor::format_runs_typed`], then `settle` sees each run as `edit` leaves it (its styles under it) and may
    /// change its properties again, returning whether it did. A style being formatted is edited alone.
    pub(super) fn format_runs_typed_settled(
        &mut self,
        label: &'static str,
        edit: &dyn Fn(&mut CharEdit, &RunStyle),
        settle: &dyn Fn(&mut CharEdit, &RunStyle) -> bool,
        fonts: &Fonts,
    ) -> Outcome {
        if self.label_target.is_some() {
            return self.format_label_part(edit, settle);
        }
        if self.style_target.is_some() {
            return self.format_style_rpr(edit);
        }
        if self.refuse_expanded() || self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        // A style separator mark keeps its flags
        let typed = |direct: &RunDirect, run: &RunStyle, base: &CharProps, mark: bool| -> RunDirect {
            let mut e = CharEdit::new(direct.props().map_or_else(CharProps::default, |p| (**p).clone()));
            let sep = (mark && direct.spec_vanish()).then(|| separator(&e));
            edit(&mut e, run);
            if let Some(flags) = &sep {
                flags.restore(&mut e);
            }
            let new = run_xml::again(&res, base, run, direct.edit(e.clone()));
            settle(&mut e, &new);
            direct.edit(e)
        };
        let patch_as = |run: &RunStyle, base: &CharProps, mark: bool| -> Run {
            let base = base.clone();
            run_xml::again(&res, &base, run, typed(&run.direct, run, &base, mark))
        };
        let patch = |run: &RunStyle, base: &CharProps| patch_as(run, base, false);
        let mut ranges: Vec<(ParaId, Unit, Unit)> = Vec::new();
        // The paragraphs whose marks are in the selection.
        let mut marks: Vec<ParaId> = Vec::new();
        if self.selection.is_collapsed() && !self.is_discontiguous() {
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
            if mark.same_run(&p.mark) {
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
                        let typed_base = base.clone();
                        let edit = |d: &RunDirect, run: &RunStyle| Some(typed(d, run, &typed_base, false));
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

    /// Paragraph formatting as a typed edit of each selected paragraph's authored properties; of the style being
    /// formatted instead, or of the selection's style when it updates automatically.
    pub(super) fn format_paras_typed(
        &mut self,
        label: &'static str,
        edit: &dyn Fn(&mut ParaEdit, &crate::docx::Paragraph),
        fonts: &Fonts,
    ) -> Outcome {
        if self.style_target.is_some() {
            return self.format_style_ppr(edit);
        }
        if let Some(id) = self.auto_redefined() {
            return self.format_auto_redefined_typed(&id, edit, fonts);
        }
        self.format_paras_direct_typed(label, edit, fonts)
    }

    /// [`Self::format_paras_typed`] on the paragraphs themselves, even in a style that updates automatically: for
    /// what isn't direct formatting (applying a style).
    pub(super) fn format_paras_direct_typed(
        &mut self,
        label: &'static str,
        edit: &dyn Fn(&mut ParaEdit, &crate::docx::Paragraph),
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
            let mut e = ParaEdit::new((**p.style.direct.get()).clone());
            edit(&mut e, p);
            let direct = p.style.direct.edit(e);
            let Some(r) = self.para_again(para, Some(direct), &res, &cx) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if r.style.layout_eq(&p.style) && r.style.authored_eq(&p.style) {
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
        match f {
            &Format::Set(t, on) => self.format_runs_typed(t.label(), &|e, _| t.set(e, on), fonts),
            Format::FontSize(pt) => {
                let pt = half_points(*pt);
                self.format_runs_typed("Font Size", &|p, _| sized(p, pt), fonts)
            }
            Format::GrowFont(grow) => {
                const SIZES: [f64; 16] = [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0];
                let grow = *grow;
                self.format_runs_typed(
                    "Font Size",
                    &|p, run| {
                        let s = run.size;
                        let next = if grow {
                            SIZES.iter().copied().find(|&x| x > s + 0.01).unwrap_or(((s / 10.0).floor() + 1.0) * 10.0)
                        } else {
                            SIZES.iter().rev().copied().find(|&x| x < s - 0.01).unwrap_or((s - 1.0).max(1.0))
                        };
                        sized(p, half_points(next));
                    },
                    fonts,
                )
            }
            Format::FontColor(c) => {
                let c = c.map_or(crate::format::Color::Auto, colour);
                self.format_runs_typed(
                    "Font Color",
                    &|e, _| {
                        e.color = Some(c);
                        e.whole(Prop::Color);
                    },
                    fonts,
                )
            }
            Format::Highlight(name) => {
                let h = *name;
                self.format_runs_typed(
                    "Highlight",
                    &|e, _| {
                        e.highlight = h.map(Some);
                        e.whole(Prop::Highlight);
                    },
                    fonts,
                )
            }
            Format::FontFamily(name) => self.format_runs_typed("Font", &|e, _| family(e, name, FONT_ORDER), fonts),
            Format::ClearFormatting => self.format_runs_typed("Clear Formatting", &|e, _| e.props = CharProps::default(), fonts),
            Format::Indent(more) => {
                let more = *more;
                self.indent_list(more, fonts).unwrap_or_else(|| self.indent_plain(more, fonts))
            }
            Format::Hanging(more) => {
                let (more, tab) = (*more, self.tab_grid());
                self.format_paras_typed(
                    "Hanging Indent",
                    &|e, q| {
                        // The first line stays where it is; the other lines move to the next or last tab stop.
                        let first_line = q.style.ind_left + q.style.ind_first;
                        let left = grid_step(q.style.ind_left, tab, more);
                        let first = first_line - left;
                        e.indent_left = Some(twip(left));
                        e.indent_first = Some(if first < 0.0 { -twip(-first) } else { twip(first) });
                        e.part(ParaProp::IndentLeft);
                        e.part(ParaProp::IndentFirst);
                    },
                    fonts,
                )
            }
            &Format::StepFont(grow) => {
                self.format_runs_typed("Font Size", &|p, run| sized(p, half_points(if grow { run.size + 1.0 } else { run.size - 1.0 })), fonts)
            }
            Format::ClearParagraph => self.format_paras_direct_typed("Reset Paragraph", &|e, _| e.props = para_props::reset(&e.props, true), fonts),
            Format::ParaBorders(which) => {
                let which = which.clone();
                self.format_paras_typed(
                    "Borders",
                    &|e, _| {
                        let line = Some(TextBorder { style: BorderStyle::Single, width: 0.5, space: 1.0, shadow: false, color: Some(Color::Auto) });
                        e.borders = match which.as_str() {
                            "box" => ParaBorders { top: line, left: line, bottom: line, right: line, between: None },
                            "bottom" => ParaBorders { bottom: line, ..ParaBorders::default() },
                            _ => ParaBorders::default(),
                        };
                        e.whole(ParaProp::Borders);
                    },
                    fonts,
                )
            }
            Format::ParaShading(c) => {
                let c = *c;
                self.format_paras_typed(
                    "Shading",
                    &|e, _| {
                        e.shading = c.map(|c| Shading { pattern: ShadingPattern::Clear, color: Some(Color::Auto), fill: Some(colour(c)) });
                        e.whole(ParaProp::Shading);
                    },
                    fonts,
                )
            }
            Format::ParaStyle(id) => {
                let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
                if !res.cascade().styles.get(id).is_some_and(|s| s.kind == crate::format::StyleKind::Paragraph) {
                    return Outcome::Unchanged("no such paragraph style");
                }
                let id: Rc<str> = id.as_str().into();
                self.format_paras_direct_typed(
                    "Style",
                    &|e, _| {
                        e.style = Some(id.clone());
                        e.whole(ParaProp::Style);
                    },
                    fonts,
                )
            }
            Format::CharStyle(id) => {
                let id: Rc<str> = id.as_str().into();
                self.format_runs_typed(
                    "Style",
                    &|e, _| {
                        e.style = Some(id.clone());
                        e.whole(Prop::Style);
                    },
                    fonts,
                )
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
        self.format_paras_typed(
            "Indent",
            &|e, q| {
                if q.style.num.is_none_or(|ListRef { list: ListId(id), level: _ }| id == 0) {
                    e.indent_left = Some(twip(grid_step(q.style.ind_left, tab, more)));
                    e.part(ParaProp::IndentLeft);
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
        let &(first, ListRef { list: ListId(num), level: ilvl }) = items.first()?;
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
        self.index.para(&self.doc, para).map_or_else(Vec::new, hidden_marks_of)
    }
}

/// The hidden paragraph marks of `p`, as [`Editor::hidden_marks`] gives them.
pub fn hidden_marks_of(p: &crate::docx::Paragraph) -> Vec<(Unit, bool)> {
    let parts = p.src.as_ref().filter(|s| s.joined.len() + 1 == s.ranges.len()).map_or(&[][..], |s| &s.joined[..]);
    let own = p.mark.hidden.then(|| (super::step::para_len(p), p.mark.direct.spec_vanish()));
    parts.iter().map(|j| (Unit(j.end), j.spec_vanish)).chain(own).collect()
}

impl Editor {
    /// Ctrl+Alt+Enter: a style separator, a hidden paragraph mark (`w:vanish`, `w:specVanish`) that runs the paragraph
    /// before it into the one after, each keeping its own properties. At a paragraph's end with a paragraph after it,
    /// that paragraph's mark becomes one; elsewhere the paragraph splits at the caret as Enter splits it (a selection
    /// replaced) and the first part's mark becomes one. One undo step, the caret after it.
    pub fn insert_style_separator(&mut self, fonts: &Fonts) -> Outcome {
        if self.refuse_expanded() || self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let Some(focus) = self.selection.focus() else { return Outcome::unavailable("no caret in text") };
        let Some((para, at)) = focus.para_offset() else { return Outcome::unavailable("no caret in text") };
        let Some(slot) = self.index.para_slot(para) else { return Outcome::Unchanged("no paragraph") };
        let next = match self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(slot.block + 1)) {
            Some(crate::docx::Block::Para(n))
                if self.selection.is_collapsed() && self.index.para(&self.doc, para).is_some_and(|p| super::step::para_len(p) == at) =>
            {
                Some(ParaId(n.id))
            }
            _ => None,
        };
        self.joined("Style Separator", fonts, |e| {
            let Some(next) = next else {
                return match e.split_at_caret(false, fonts) {
                    (Outcome::Changed(_), Some(first)) => e.separator_mark(first, fonts),
                    (out, _) => out,
                };
            };
            let out = e.separator_mark(para, fonts);
            if matches!(out, Outcome::Changed(_)) {
                e.selection = super::Selection::Caret(super::Position::text(focus.story, next, Unit(0)));
            }
            out
        })
    }

    /// `para`'s mark made a style separator.
    fn separator_mark(&mut self, para: ParaId, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let (Some(again), Some(p)) = (self.para_again(para, None, &res, &cx), self.index.para(&self.doc, para)) else {
            return Outcome::Unchanged("no paragraph");
        };
        let mut e = run_xml::edit_of(&p.mark);
        e.toggle(crate::format::Toggle::HIDDEN, true);
        e.spec_vanish = Some(true);
        e.whole(Prop::SpecVanish);
        let mark = run_xml::again(&res, again.mark_base(), &p.mark, p.mark.direct.edit(e));
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
            let runs = e.format_runs_typed("Clear Formatting", &|p, _| p.props = highlight_alone(&p.props), fonts);
            if matches!(runs, Outcome::Refused(_)) {
                return runs;
            }
            let paras = e.format_paras_direct_typed("Clear Formatting", &|e, _| e.props = para_props::reset(&e.props, false), fonts);
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
    Highlight(Option<crate::format::Highlight>),
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
    ParaShading(Option<super::theme::WordColour>),
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
