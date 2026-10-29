//! Merge-designer operations: fields and content controls the application inserts, changes and removes, data parts,
//! element marks and looks, the external drop caret, and the `hit` query. Arguments arrive typed; a refusal is an
//! error naming why, and changes nothing.

use crate::api::command::{
    CommandResult, DataPartRef, ElementClass, ElementField, FieldLabels, FieldResults, FillControl, InsertControl, InsertField, MarkElements,
    PlaceholderLook as LookArg, PlaceholderLooks, PlaceholderMatch, RepeatingSection, ReplaceInElements, SetCheckbox, SetControl, SetField,
    Severity as SeverityArg, WriteDataPart,
};
use crate::api::error::Error;
use crate::api::review::Body;
use crate::api::value::{ControlKind as KindArg, ControlLock, ControlOptions, DataBinding};
use crate::docx::{Block, Control, ControlId, ControlKind, ControlProps, ElementId, FieldId, FieldView, Inline, Lock};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::elements;
use super::index::{Container, Loc};
use super::position::{Position, StoryId};
use super::selection::Selection;
use super::step::Step;

/// A refusal: what the document doesn't allow here, or an argument naming nothing that exists.
type Refused = Error;

/// How the host has a control drawn: a box with a fill (ARGB), label and icon, or an image.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Look {
    Box(super::view::PlaceholderLook),
    Image(std::rc::Rc<Vec<u8>>),
}

/// Which controls a look is for.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum TagMatch {
    Exact(String),
    Prefix(String),
}

/// The host's looks: by element, then exact tag, then the longest prefix, then the default.
#[derive(Clone, Debug, Default)]
pub(super) struct Looks {
    by_element: std::collections::HashMap<ElementId, Look>,
    by_tag: Vec<(TagMatch, Look)>,
    default: Option<Look>,
}
impl Looks {
    fn find(&self, id: ElementId, tag: Option<&str>) -> Option<&Look> {
        if let Some(l) = self.by_element.get(&id) {
            return Some(l);
        }
        if let Some(t) = tag {
            if let Some((_, l)) = self.by_tag.iter().find(|(m, _)| matches!(m, TagMatch::Exact(x) if x == t)) {
                return Some(l);
            }
            let prefix = self.by_tag.iter().filter_map(|(m, l)| match m {
                TagMatch::Prefix(p) if t.starts_with(p.as_str()) => Some((p.len(), l)),
                _ => None,
            });
            if let Some((_, l)) = prefix.max_by_key(|(n, _)| *n) {
                return Some(l);
            }
        }
        self.default.as_ref()
    }

    fn is_empty(&self) -> bool {
        self.by_element.is_empty() && self.by_tag.is_empty() && self.default.is_none()
    }
}

/// A host's mark on an element.
#[derive(Clone, Debug)]
pub(super) struct Mark {
    pub id: ElementId,
    pub severity: super::view::Severity,
    pub message: String,
}

/// The 1×1 light grey PNG a new picture control shows until it is filled (hosts may draw it as a labeled box).
const PICTURE_PLACEHOLDER: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65,
    84, 120, 156, 99, 120, 241, 226, 5, 0, 5, 214, 2, 233, 151, 52, 29, 132, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Where an element is in the document.
#[derive(Clone, Debug)]
pub(super) enum Where {
    /// A field (its begin to its end marker), or an inline control (its start to its end marker): the inline indexes
    /// and the units at them (`end` is after a field's end marker, at a control's end marker).
    Inline { story: StoryId, para: ParaId, first: usize, last: usize, start: Unit, end: Unit },
    /// A checkbox: its face's unit.
    Sym { story: StoryId, para: ParaId, at: Unit },
    /// A block-level control: its blocks, one run of a container's.
    Blocks { container: Container, range: std::ops::Range<usize> },
    /// A row-level control (a repeating section or item over rows): its rows of the table.
    Rows { table: crate::layout::TableId, range: std::ops::Range<usize> },
}

fn units_before(inlines: &[Inline], k: usize) -> u32 {
    inlines[..k].iter().map(|i| i.units() as u32).sum()
}

/// An argument naming nothing usable.
fn bad(name: &str) -> Refused {
    Error::invalid(format!("bad {name}"))
}

/// An element id that names no element.
fn no_element() -> Refused {
    Error::invalid("no such element")
}

/// What a lock, or the place, doesn't allow.
fn locked() -> Refused {
    Error::unavailable("locked")
}

/// What the selection's shape doesn't allow (part of a paragraph, two tables, a region's edge).
fn shape(why: &str) -> Refused {
    Error::unavailable(why)
}

/// The result of a refusable operation, as the command's outcome.
pub(super) fn outcome(r: Result<Outcome, Refused>) -> Outcome {
    r.unwrap_or_else(Outcome::Refused)
}

/// A field code as Word stores it: one space on each side.
fn padded(code: &str) -> String {
    format!(" {} ", code.trim())
}

fn lock_of(l: ControlLock) -> Lock {
    match l {
        ControlLock::None => Lock::None,
        ControlLock::Control => Lock::ControlLocked,
        ControlLock::Content => Lock::ContentLocked,
        ControlLock::Both => Lock::Both,
    }
}

/// The plain text of a body (formatted results come with the keep-source fragment paste).
fn plain(b: &Body) -> String {
    match b {
        Body::Text(t) => t.clone(),
        Body::Rich(r) => r.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n"),
    }
}

/// `w:dataBinding` for a binding, in the document's `w` prefix.
fn binding_xml(b: &DataBinding, w: &str) -> String {
    let esc = crate::docx::write::props::esc;
    let mut x = format!("<{w}dataBinding");
    if let Some(p) = &b.prefix_mappings {
        x.push_str(&format!(" {w}prefixMappings=\"{}\"", esc(p)));
    }
    x.push_str(&format!(" {w}xpath=\"{}\"", esc(&b.xpath)));
    if let Some(i) = &b.store_item_id {
        x.push_str(&format!(" {w}storeItemID=\"{}\"", esc(i)));
    }
    x + "/>"
}

/// The element an id names, as the document knows it.
fn element_id(id: &crate::api::value::ElementId) -> Result<ElementId, Refused> {
    elements::parse_id(&id.0).ok_or_else(no_element)
}

/// The API's id of an element.
pub(crate) fn api_id(e: ElementId) -> crate::api::value::ElementId {
    crate::api::value::ElementId(elements::id_str(e))
}

impl Editor {
    /// The element `id` names, and where it is.
    pub(super) fn element(&self, id: &crate::api::value::ElementId) -> Result<(ElementId, Where), Refused> {
        let e = element_id(id)?;
        let w = self.locate(e).ok_or_else(no_element)?;
        Ok((e, w))
    }

    /// Where an element is now.
    pub(super) fn locate(&self, e: ElementId) -> Option<Where> {
        let locs = self.index.element(e)?;
        let story_of = |para: ParaId| self.index.para_slot(para).map(|s| self.index.story(s.container));
        match e {
            ElementId::Field(f) => {
                let para = locs.iter().find_map(|l| if let Loc::Para(p) = l { Some(*p) } else { None })?;
                let p = self.index.para(&self.doc, para)?;
                let first = p.inlines.iter().position(|i| matches!(i, Inline::FieldBegin { id, .. } if *id == f))?;
                let s = crate::docx::fields::span_at(&p.inlines, first)?;
                let (start, end) = (units_before(&p.inlines, first), units_before(&p.inlines, s.end) + 1);
                Some(Where::Inline { story: story_of(para)?, para, first, last: s.end, start: Unit(start), end: Unit(end) })
            }
            ElementId::Control(c) => {
                for l in locs {
                    let Loc::Para(para) = l else { continue };
                    let p = self.index.para(&self.doc, *para)?;
                    if let Some(first) = p.inlines.iter().position(|i| matches!(i, Inline::ControlStart(x) if *x == c)) {
                        let last = p.inlines.iter().position(|i| matches!(i, Inline::ControlEnd(x) if *x == c))?;
                        let (start, end) = (units_before(&p.inlines, first), units_before(&p.inlines, last));
                        return Some(Where::Inline { story: story_of(*para)?, para: *para, first, last, start: Unit(start), end: Unit(end) });
                    }
                    if let Some(k) = p.inlines.iter().position(|i| matches!(i, Inline::Sym { control: Some(x), .. } if *x == c)) {
                        return Some(Where::Sym { story: story_of(*para)?, para: *para, at: Unit(units_before(&p.inlines, k)) });
                    }
                }
                // A row-level control: its rows of one table.
                let rows: Vec<(crate::layout::TableId, usize)> =
                    locs.iter().filter_map(|l| if let Loc::Row(t, r) = l { Some((*t, *r)) } else { None }).collect();
                if let Some(&(table, _)) = rows.first() {
                    let at: Vec<usize> = rows.iter().filter(|(t, _)| *t == table).map(|(_, r)| *r).collect();
                    let (lo, hi) = (*at.iter().min()?, *at.iter().max()?);
                    return Some(Where::Rows { table, range: lo..hi + 1 });
                }
                // A block-level control: its paragraphs and tables, which sit together in one container.
                let slots: Vec<super::index::Slot> = locs
                    .iter()
                    .filter_map(|l| match l {
                        Loc::Para(p) => self.index.para_slot(*p),
                        Loc::Table(t) => self.index.table_slot(*t),
                        _ => None,
                    })
                    .collect();
                let container = slots.first()?.container;
                // Only the blocks directly in that container (a member table's paragraphs are inside it).
                let at: Vec<usize> = slots.iter().filter(|s| s.container == container).map(|s| s.block).collect();
                let (lo, hi) = (*at.iter().min()?, *at.iter().max()?);
                Some(Where::Blocks { container, range: lo..hi + 1 })
            }
        }
    }

    /// Refuses a selection whose content can't be edited (inside a control whose content is locked).
    fn editable_target(&self) -> Result<(), Refused> {
        match self.selection.focus() {
            Some(p) if !self.editable(&p) => Err(locked()),
            Some(_) => Ok(()),
            // A cell block (rows for a region): no text place to check.
            None if matches!(self.selection, Selection::Cells { .. }) => Ok(()),
            None => Err(Error::unavailable("no place to insert")),
        }
    }

    /// The text position at a document point, the innermost element there, and whether it can be edited (not a
    /// repeated header row's copy, nor in a locked control). Lays out the page; the selection doesn't move.
    pub fn hit_test(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(Position, Option<ElementId>, bool)> {
        let (p, repeat) = self.hit_full(x, y, fonts)?;
        let element = elements::context(&self.doc, &self.index, &Selection::Caret(p)).first().map(|(e, _)| *e);
        Some((p, element, !repeat && self.editable(&p)))
    }

    /// The external drop caret at a document point, where text can go in; none away from one. View only.
    pub fn set_drop(&mut self, at: Option<(f64, f64)>, fonts: &Fonts) {
        self.drop = at.and_then(|(x, y)| self.hit_test(x, y, fonts)).filter(|h| h.2).map(|h| h.0);
    }

    /// Whether text can go in at `p`: not inside a control whose content is locked (at its end marker is outside it).
    pub(super) fn editable(&self, p: &Position) -> bool {
        elements::context(&self.doc, &self.index, &Selection::Caret(*p)).iter().all(|(e, _)| {
            let ElementId::Control(c) = e else { return true };
            if self.doc.controls.get(c).is_none_or(|k| k.props.lock.editable()) {
                return true;
            }
            matches!(self.locate(*e), Some(Where::Inline { para, end, .. }) if p.para_offset() == Some((para, end)))
        })
    }

    /// Where an element's content runs, as a text range: a field begin to end, an inline control between its
    /// markers, a checkbox's face, a block control's paragraphs.
    pub(super) fn element_range(&self, w: &Where) -> Option<(Position, Position)> {
        match w {
            Where::Inline { story, para, start, end, .. } => Some((Position::text(*story, *para, *start), Position::text(*story, *para, *end))),
            Where::Sym { story, para, at } => Some((Position::text(*story, *para, *at), Position::text(*story, *para, Unit(at.0 + 1)))),
            Where::Blocks { container, range } => {
                let story = self.index.story(*container);
                let blocks = self.index.blocks(&self.doc, *container)?;
                let first = blocks[range.start..range.end].iter().find_map(super::first_para_of)?;
                let (last, len) = blocks[range.start..range.end].iter().rev().find_map(super::last_para_of)?;
                Some((Position::text(story, first, Unit(0)), Position::text(story, last, len)))
            }
            Where::Rows { table, range } => {
                let t = self.index.table(&self.doc, *table)?;
                let story = self.index.table_slot(*table).map(|s| self.index.story(s.container))?;
                let first = t.rows[range.start].cells.iter().flat_map(|c| c.blocks.iter()).find_map(super::first_para_of)?;
                let (last, len) = t.rows[range.end - 1].cells.iter().rev().flat_map(|c| c.blocks.iter().rev()).find_map(super::last_para_of)?;
                Some((Position::text(story, first, Unit(0)), Position::text(story, last, len)))
            }
        }
    }

    /// Marks on elements, drawn with their severity and message; `clear` takes the others away first. View only.
    pub(super) fn mark_elements(&mut self, m: MarkElements) -> Outcome {
        let mut new = Vec::new();
        for mark in m.marks {
            let Ok(id) = element_id(&mark.id) else { return Outcome::Refused(no_element()) };
            if self.index.element(id).is_none() {
                return Outcome::Refused(no_element());
            }
            let severity = match mark.severity {
                SeverityArg::Error => super::view::Severity::Error,
                SeverityArg::Warning => super::view::Severity::Warning,
                SeverityArg::Info => super::view::Severity::Info,
            };
            new.push(Mark { id, severity, message: mark.message });
        }
        if m.clear.unwrap_or(false) {
            self.marks.clear();
        }
        self.marks.retain(|m| !new.iter().any(|n| n.id == m.id));
        self.marks.extend(new);
        Outcome::Moved
    }

    /// The marks on `pages`, and the tooltip of the one under the pointer. Marks whose element is gone are dropped.
    pub(super) fn mark_views(&mut self, pages: std::ops::Range<usize>) -> (Vec<super::view::MarkView>, Option<super::view::Tooltip>) {
        let marks = std::mem::take(&mut self.marks);
        let mut kept = Vec::with_capacity(marks.len());
        let mut out = Vec::new();
        let mut tip = None;
        let hover = self.hover.and_then(|(x, y)| self.stack().page_at(x, y));
        for m in marks {
            let Some(w) = self.locate(m.id) else { continue };
            if let Some((a, b)) = self.element_range(&w) {
                for (page, rect) in self.range_rects(a, b, pages.clone()) {
                    if let Some((hp, hx, hy)) = hover
                        && hp == page
                        && rect[0] <= hx
                        && hx <= rect[2]
                        && rect[1] <= hy
                        && hy <= rect[3]
                    {
                        tip = Some(super::view::Tooltip { text: m.message.clone(), page, rect });
                    }
                    out.push(super::view::MarkView { page, rect, severity: m.severity });
                }
            }
            kept.push(m);
        }
        self.marks = kept;
        (out, tip)
    }

    /// The tooltip under the pointer, on its page only: a mark's message, else a link's address.
    pub fn hover_tooltip(&mut self) -> Option<(crate::api::services::TooltipKind, super::view::Tooltip)> {
        use crate::api::services::TooltipKind;
        let (x, y) = self.hover?;
        let (page, hx, hy) = self.stack().page_at(x, y)?;
        if let (_, Some(t)) = self.mark_views(page..page + 1) {
            return Some((TooltipKind::Mark, t));
        }
        let link = self.hit_laid_out(x, y).and_then(|p| self.link_at(p))?;
        let (a, b) = self.locate(ElementId::Field(link.field)).and_then(|w| self.element_range(&w))?;
        let rect =
            self.range_rects(a, b, page..page + 1).into_iter().map(|(_, r)| r).find(|r| r[0] <= hx && hx <= r[2] && r[1] <= hy && hy <= r[3])?;
        Some((TooltipKind::Link, super::view::Tooltip { text: link.address, page, rect }))
    }

    /// Element `id` selected (a header's or footer's entered for it), and shown unless `reveal` is false.
    pub fn select_element(&mut self, id: &crate::api::value::ElementId, reveal: bool, fonts: &Fonts) -> Outcome {
        let Ok((_, w)) = self.element(id) else { return Outcome::Refused(no_element()) };
        let Some((a, b)) = self.element_range(&w) else { return Outcome::Refused(no_element()) };
        match &w {
            Where::Rows { table, range } => {
                use super::selection::{CellRef, CellShape};
                self.set_selection(Selection::Cells {
                    table: *table,
                    anchor: CellRef { row: range.start, col: 0 },
                    focus: CellRef { row: range.end - 1, col: usize::MAX },
                    shape: CellShape::Rows,
                });
            }
            _ => self.set_selection(Selection::range(a, b, super::selection::Granularity::Char)),
        }
        if reveal {
            self.ensure(&b, fonts);
            self.scroll_to_caret(fonts);
        }
        Outcome::Moved
    }

    /// `element.replace`: every occurrence of plain text `find` (case-sensitive) in the codes, tags or
    /// aliases of the elements (`kind`, `ids` narrow them) becomes `replace`, the text around it kept; one step, and
    /// none when nothing matched. Answer: `{ changed }`, the elements changed.
    pub(super) fn replace_in_elements(&mut self, r: ReplaceInElements, fonts: &Fonts) -> Outcome {
        let (find, replace) = (r.find, r.replace);
        let ids: Option<Vec<ElementId>> = match r.ids {
            None => None,
            Some(v) => match v.iter().map(element_id).collect::<Result<_, _>>() {
                Ok(ids) => Some(ids),
                Err(e) => return Outcome::Refused(e),
            },
        };
        let kind = r.kind;
        let all = elements::enumerate(&self.doc, &self.index, &elements::Filter::default());
        let wanted = |e: &elements::Element| {
            ids.as_ref().is_none_or(|v| v.contains(&e.id)) && kind.is_none_or(|k| (k == ElementClass::Field) == e.props.is_none())
        };
        let mut tx = self.begin("Replace");
        let mut changed = 0usize;
        for e in all.iter().filter(|e| wanted(e)) {
            match (e.id, r.field) {
                (ElementId::Field(f), ElementField::Code) => {
                    if e.code.as_deref().is_some_and(|c| c.contains(&find)) && self.replace_in_code(&mut tx, f, &find, &replace) {
                        changed += 1;
                    }
                }
                (ElementId::Control(c), ElementField::Tag | ElementField::Title) => {
                    let Some(mut control) = self.doc.controls.get(&c).cloned() else { continue };
                    let slot = if r.field == ElementField::Tag { &mut control.props.tag } else { &mut control.props.alias };
                    let Some(v) = slot.as_ref().filter(|v| v.contains(&find)) else { continue };
                    *slot = Some(v.replace(&find, &replace));
                    if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_ok() {
                        changed += 1;
                    }
                }
                _ => {}
            }
        }
        self.made = Some(CommandResult::Changed { changed: changed as u32 });
        if changed == 0 {
            self.abort(tx);
            return Outcome::Unchanged("nothing matched");
        }
        self.follow(&tx);
        self.commit(tx, None, fonts)
    }

    /// Replaces `find` in field `f`'s own code text (the fields nested in it untouched), text inline by text inline;
    /// whether it changed.
    // shortcut: a match spanning two of the code's text inlines (a formatting change inside it) isn't found.
    fn replace_in_code(&mut self, tx: &mut super::Tx, f: FieldId, find: &str, replace: &str) -> bool {
        let Some(Where::Inline { para, first, last, .. }) = self.locate(ElementId::Field(f)) else { return false };
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return false };
        let Some(s) = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last) else { return false };
        // The code's own text inlines: those not inside a nested field.
        let mut own = Vec::new();
        let mut depth = 0;
        for k in first + 1..s.code_end() {
            match &p.inlines[k] {
                Inline::FieldBegin { .. } => depth += 1,
                Inline::FieldEnd { .. } => depth -= 1,
                Inline::Text(t, r) if depth == 0 && t.contains(find) => own.push((k, t.replace(find, replace), r.clone())),
                _ => {}
            }
        }
        let mut any = false;
        // Last first, so the earlier inlines' units stay where they are.
        for (k, t, run) in own.into_iter().rev() {
            let at = units_before(&p.inlines, k);
            let range = Unit(at)..Unit(at + p.inlines[k].units() as u32);
            let with = if t.is_empty() { Vec::new() } else { vec![Inline::Text(t, run)] };
            any |= self.step(tx, Step::Text { para, range, with, edges: Default::default() }).is_ok();
        }
        if any {
            self.refresh_field_kinds(tx, para);
        }
        any
    }

    /// Control `c` with, for a repeating section, the items inside it in `lists` (its members' membership lists):
    /// an item is only ever in a section (C3), so unwrapping one unwraps them too.
    fn with_items<'l>(&self, c: ControlId, lists: impl Iterator<Item = &'l Vec<ControlId>>) -> Vec<ControlId> {
        let mut gone = vec![c];
        if self.doc.controls.get(&c).is_some_and(|k| k.props.kind == ControlKind::RepeatingSection) {
            for list in lists {
                let after = list.iter().skip_while(|x| **x != c).skip(1);
                for x in after {
                    if self.doc.controls.get(x).is_some_and(|k| k.props.kind == ControlKind::RepeatingItem) && !gone.contains(x) {
                        gone.push(*x);
                    }
                }
            }
        }
        gone
    }

    /// `region.wrap`: whole rows of one table, or whole paragraphs (with the tables between them), in a
    /// new repeating section with one item per row, or one item over the paragraphs. A selection with part of a
    /// paragraph, rows of two tables, or one side of another control's edge is refused with its reason.
    pub(super) fn wrap_in_repeating_section(&mut self, r: RepeatingSection, fonts: &Fonts) -> Outcome {
        outcome(self.region_wrap(r, fonts))
    }

    fn region_wrap(&mut self, r: RepeatingSection, fonts: &Fonts) -> Result<Outcome, Refused> {
        self.editable_target()?;
        let section = ControlProps { kind: ControlKind::RepeatingSection, tag: r.tag, alias: r.title, ..Default::default() };
        let item = || Control::new(ControlProps { kind: ControlKind::RepeatingItem, ..Default::default() });
        let s = ControlId::next();
        let made = CommandResult::Element { id: api_id(ElementId::Control(s)) };
        // Where the new controls go in a member's list: after those around the whole selection, before the rest. A
        // control holding part of the selection and more besides crosses its edge.
        fn place(lists: &[&Vec<ControlId>], outside: &[&Vec<ControlId>]) -> Result<Vec<usize>, Refused> {
            let mut out = Vec::with_capacity(lists.len());
            let all = |x: &ControlId| lists.iter().all(|l| l.contains(x));
            // A control part of the selection holds, with members outside it too, crosses its edge.
            if lists.iter().flat_map(|l| l.iter()).any(|x| !all(x) && outside.iter().any(|l| l.contains(x))) {
                return Err(shape("region boundary"));
            }
            for l in lists {
                let mut k = 0;
                for (i, x) in l.iter().enumerate() {
                    if all(x) {
                        if i != k {
                            return Err(shape("region boundary"));
                        }
                        k += 1;
                    }
                }
                out.push(k);
            }
            Ok(out)
        }
        let rows = |e: &Editor| -> Result<Option<(crate::layout::TableId, usize, usize)>, Refused> {
            if let Selection::Cells { table, anchor, focus, .. } = e.selection {
                let n = e.index.table(&e.doc, table).map_or(0, |t| t.rows.len());
                let (a, b) = (anchor.row.min(n.saturating_sub(1)), focus.row.min(n.saturating_sub(1)));
                return Ok(Some((table, a.min(b), a.max(b))));
            }
            let (a, b) = e.selection.ordered(&e.index).ok_or_else(|| bad("target"))?;
            let (ca, cb) = (e.index.chain_of(&a), e.index.chain_of(&b));
            let k = ca.iter().zip(&cb).take_while(|(x, y)| x.table == y.table && x.row == y.row && x.cell == y.cell).count();
            match (ca.get(k), cb.get(k)) {
                (Some(x), Some(y)) if x.table == y.table => Ok(Some((x.table, x.row.min(y.row), x.row.max(y.row)))),
                (Some(_), Some(_)) => Err(shape("two tables")),
                (None, None) => Ok(None),
                // One end in a table the other isn't in: whole blocks, if the ends are paragraph edges.
                _ => Ok(None),
            }
        };
        let mut tx = self.begin("Insert Repeating Section");
        let r = rows(self)?;
        if let Some((table, r0, r1)) = r {
            let t = self.index.table(&self.doc, table).ok_or_else(no_element)?;
            let lists: Vec<&Vec<ControlId>> = t.rows[r0..=r1].iter().map(|r| &r.controls).collect();
            let outside: Vec<&Vec<ControlId>> = t.rows.iter().enumerate().filter(|(k, _)| *k < r0 || *k > r1).map(|(_, r)| &r.controls).collect();
            let at = place(&lists, &outside)?;
            let mut with: Vec<crate::docx::Row> = t.rows[r0..=r1].to_vec();
            self.pending_controls.push((s, Control::new(section)));
            for (r, k) in with.iter_mut().zip(at) {
                let i = ControlId::next();
                self.pending_controls.push((i, item()));
                r.controls.splice(k..k, [s, i]);
            }
            if self.step(&mut tx, Step::Rows { table, range: r0..r1 + 1, with, grid: None }).is_err() {
                self.abort(tx);
                return Err(bad("target"));
            }
            tx.stale_tables.push(table);
            self.made = Some(made);
            return Ok(self.commit(tx, None, fonts));
        }
        // Whole paragraphs (and the tables between them) of one container.
        let (a, b) = self.selection.ordered(&self.index).ok_or_else(|| bad("target"))?;
        let top = |e: &Editor, p: &Position| -> Option<super::index::Slot> {
            let chain = e.index.chain_of(p);
            let ca = e.index.chain_of(&a);
            let k = ca.iter().zip(&chain).take_while(|(x, y)| x.table == y.table && x.row == y.row && x.cell == y.cell).count();
            match chain.get(k) {
                Some(l) => e.index.table_slot(l.table),
                None => e.index.para_slot(p.para_offset()?.0),
            }
        };
        let (sa, sb) = (top(self, &a).ok_or_else(|| bad("target"))?, top(self, &b).ok_or_else(|| bad("target"))?);
        if sa.container != sb.container {
            self.abort(tx);
            return Err(shape("partial paragraph"));
        }
        let edge = |e: &Editor, p: &Position, start: bool| match p.para_offset() {
            Some((para, u)) if e.index.para_slot(para).is_some_and(|s| s.container == sa.container) => {
                if start {
                    u == Unit(0)
                } else {
                    e.index.para(&e.doc, para).is_some_and(|q| u == super::step::para_len(q) || u == Unit(0))
                }
            }
            _ => true,
        };
        if !edge(self, &a, true) || !edge(self, &b, false) {
            self.abort(tx);
            return Err(shape("partial paragraph"));
        }
        // A range ending at the start of a later paragraph leaves that paragraph out.
        let last = match b.para_offset() {
            Some((pb, u)) if u == Unit(0) && sb.block > sa.block && self.index.para_slot(pb).is_some_and(|s| s.container == sa.container) => {
                sb.block - 1
            }
            _ => sb.block,
        };
        let blocks = self.index.blocks(&self.doc, sa.container).ok_or_else(no_element)?;
        let lists: Vec<&Vec<ControlId>> = blocks[sa.block..=last]
            .iter()
            .map(|b| match b {
                Block::Para(p) => &p.controls,
                Block::Table(t) => &t.controls,
            })
            .collect();
        fn list_of(b: &Block) -> &Vec<ControlId> {
            match b {
                Block::Para(p) => &p.controls,
                Block::Table(t) => &t.controls,
            }
        }
        let outside: Vec<&Vec<ControlId>> = blocks.iter().enumerate().filter(|(k, _)| *k < sa.block || *k > last).map(|(_, b)| list_of(b)).collect();
        let at = place(&lists, &outside)?;
        let i = ControlId::next();
        let mut with: Vec<Block> = blocks[sa.block..=last].to_vec();
        for (b, k) in with.iter_mut().zip(at) {
            match b {
                Block::Para(p) => p.controls.splice(k..k, [s, i]),
                Block::Table(t) => t.controls.splice(k..k, [s, i]),
            };
        }
        self.pending_controls.push((s, Control::new(section)));
        self.pending_controls.push((i, item()));
        if self.step(&mut tx, Step::Blocks { container: sa.container, range: sa.block..last + 1, with }).is_err() {
            self.abort(tx);
            return Err(bad("target"));
        }
        self.made = Some(made);
        Ok(self.commit(tx, None, fonts))
    }

    /// The repeating regions design mode shows on `pages`: those around the selection and the one under the
    /// pointer, each with its box per page and a title tab above its top left.
    pub(super) fn region_views(&mut self, pages: std::ops::Range<usize>) -> Vec<super::view::RegionView> {
        if self.mode != super::Mode::Design {
            return Vec::new();
        }
        let mut ids = self.sections_at(&self.selection.clone());
        if let Some(c) = self.hover_region.filter(|c| !ids.contains(c)) {
            ids.push(c);
        }
        ids.into_iter().filter_map(|c| self.region_view(c, pages.clone())).collect()
    }

    /// The repeating sections the selection `sel` is in, innermost first.
    fn sections_at(&self, sel: &Selection) -> Vec<ControlId> {
        elements::context(&self.doc, &self.index, sel)
            .into_iter()
            .filter_map(|(x, _)| if let ElementId::Control(c) = x { Some(c) } else { None })
            .filter(|c| self.doc.controls.get(c).is_some_and(|k| k.props.kind == ControlKind::RepeatingSection))
            .collect()
    }

    /// The pointer hovered at `x`, `y` (document points): in design mode, the region under it, else the one it was
    /// over while it is just above that region, where its title tab is. Decided here, at the input,
    /// so what the editor does never depends on which frames were drawn.
    pub(super) fn set_hover(&mut self, x: f64, y: f64) {
        self.hover = Some((x, y));
        if self.mode != super::Mode::Design {
            self.hover_region = None;
            return;
        }
        let under = self.hit_laid_out(x, y).and_then(|p| self.sections_at(&Selection::Caret(p)).first().copied());
        let on_tab = |e: &mut Editor, c: ControlId| {
            let Some((page, px, py)) = e.stack().page_at(x, y) else { return false };
            // The strip the tab is in, as wide as the region: leaving the region through its top edge beside the tab
            // and going to it keeps the region.
            e.region_view(c, page..page + 1).is_some_and(|g| {
                let (p, t) = g.title_rect;
                let Some(&(_, first)) = g.rects.first() else { return false };
                p == page && first[0] <= px && px <= first[2].max(t[2]) && t[1] <= py && py <= t[3]
            })
        };
        self.hover_region = match under {
            Some(c) => Some(c),
            None => self.hover_region.filter(|&c| on_tab(self, c)),
        };
    }

    /// Region `c`'s boxes on `pages` and its title tab above the first.
    fn region_view(&mut self, c: ControlId, pages: std::ops::Range<usize>) -> Option<super::view::RegionView> {
        let w = self.locate(ElementId::Control(c))?;
        let mut rects: Vec<(usize, [f64; 4])> = Vec::new();
        let mut grow = |page: usize, r: [f64; 4]| match rects.iter_mut().find(|(p, _)| *p == page) {
            Some((_, u)) => *u = [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[2]), u[3].max(r[3])],
            None => rects.push((page, r)),
        };
        match &w {
            Where::Rows { table, range } => {
                for page in pages {
                    let Some(geo) = self.pages.page(page, &self.index) else { continue };
                    for cell in &geo.cells {
                        if let Some(&(t, row, _)) = cell.chain.last()
                            && t == *table
                            && range.contains(&row)
                        {
                            grow(page, cell.rect);
                        }
                    }
                }
            }
            _ => {
                let (a, b) = self.element_range(&w)?;
                for (page, r) in self.range_rects(a, b, pages) {
                    grow(page, r);
                }
            }
        }
        let &(page, first) = rects.first()?;
        let props = self.doc.controls.get(&c).map(|k| k.props.clone()).unwrap_or_default();
        let title = props.alias.or(props.tag).unwrap_or_else(|| "Repeating section".into());
        let width = 6.0 * title.chars().count() as f64 + 10.0;
        Some(super::view::RegionView {
            id: elements::id_str(ElementId::Control(c)),
            rects,
            title_rect: (page, [first[0], first[1] - 14.0, first[0] + width, first[1]]),
            title,
        })
    }

    /// A press on a region's title tab in design mode: the region is selected.
    pub(super) fn region_title_click(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Some((page, px, py)) = self.stack().page_at(x, y) else { return false };
        self.ensure_page(page, fonts);
        let hit = self.region_views(page..page + 1).into_iter().find(|g| {
            let (p, r) = g.title_rect;
            p == page && r[0] <= px && px <= r[2] && r[1] <= py && py <= r[3]
        });
        let Some(g) = hit else { return false };
        self.select_element(&crate::api::value::ElementId(g.id), false, fonts) == Outcome::Moved
    }

    /// How unfilled placeholders look: content controls by element, tag, tag prefix or default, and unfilled
    /// picture placeholders by key (their tag) or default. `merge` adds to those set. View only.
    pub(super) fn set_placeholder_looks(&mut self, l: PlaceholderLooks) -> Outcome {
        let mut new = Looks::default();
        let mut pictures = std::collections::HashMap::new();
        for rule in l.looks {
            let look = match rule.look {
                LookArg::Image { image, .. } => {
                    if crate::docx::image::image_type(&image.0).is_none() {
                        return Outcome::invalid("bad image");
                    }
                    Look::Image(std::rc::Rc::new(image.0))
                }
                LookArg::Drawn { fill, label, icon_svg } => {
                    let icon = icon_svg.as_deref().and_then(super::view::SvgIcon::parse).map(std::rc::Rc::new);
                    let look = super::view::PlaceholderLook { color: fill.map(|c| 0x40_00_00_00 | c.0), label, icon };
                    Look::Box(look)
                }
            };
            let key = match &rule.matches {
                PlaceholderMatch::Default(_) => Some(String::new()),
                PlaceholderMatch::Tag { tag } => Some(tag.clone()),
                _ => None,
            };
            if let (Some(k), Look::Box(b)) = (key, &look) {
                pictures.insert(k, b.clone());
            }
            match rule.matches {
                PlaceholderMatch::Default(_) => new.default = Some(look),
                PlaceholderMatch::Id { id } => match element_id(&id) {
                    Ok(id) => {
                        new.by_element.insert(id, look);
                    }
                    Err(e) => return Outcome::Refused(e),
                },
                PlaceholderMatch::Tag { tag } => new.by_tag.push((TagMatch::Exact(tag), look)),
                PlaceholderMatch::Prefix { prefix } => new.by_tag.push((TagMatch::Prefix(prefix), look)),
            }
        }
        if l.merge.unwrap_or(false) {
            self.looks.by_element.extend(new.by_element);
            for (m, l) in new.by_tag {
                self.looks.by_tag.retain(|(x, _)| *x != m);
                self.looks.by_tag.push((m, l));
            }
            if new.default.is_some() {
                self.looks.default = new.default;
            }
            self.placeholder_looks.extend(pictures);
        } else {
            self.looks = new;
            self.placeholder_looks = pictures;
        }
        Outcome::Moved
    }

    /// The looks drawn on `pages` (design mode): picture controls' boxes, rich-text controls' lines.
    pub(super) fn look_views(&mut self, pages: std::ops::Range<usize>) -> Vec<super::view::LookView> {
        if self.mode != super::Mode::Design || self.looks.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut controls: Vec<(ControlId, ControlProps)> = self
            .doc
            .controls
            .iter()
            .filter(|(_, c)| matches!(c.props.kind, ControlKind::Picture | ControlKind::RichText))
            .map(|(id, c)| (*id, c.props.clone()))
            .collect();
        // Ids in reading (and making) order: the view lists them the same way every time.
        controls.sort_by_key(|(id, _)| *id);
        for (c, props) in controls {
            let Some(look) = self.looks.find(ElementId::Control(c), props.tag.as_deref()).cloned() else { continue };
            let Some(w) = self.locate(ElementId::Control(c)) else { continue };
            if props.kind == ControlKind::Picture {
                // The picture's box on each page it is drawn on.
                let Where::Inline { para, first, last, .. } = w else { continue };
                let Some(p) = self.index.para(&self.doc, para) else { continue };
                let Some(k) = (first..last).find(|&k| matches!(p.inlines[k], Inline::Picture(..))) else { continue };
                let unit = Unit(units_before(&p.inlines, k));
                for page in pages.clone() {
                    let Some(geo) = self.pages.page(page, &self.index) else { continue };
                    let Some(rect) = self.inline_box(&geo, para, unit) else { continue };
                    out.push(match &look {
                        Look::Image(b) => super::view::LookView { page, rect, fill: None, label: None, icon: None, image: Some(b.clone()) },
                        Look::Box(l) => {
                            super::view::LookView { page, rect, fill: l.color, label: l.label.clone(), icon: l.icon.clone(), image: None }
                        }
                    });
                }
            } else {
                // Rich text: a fill behind its lines, and its label while it is empty (an image look doesn't apply).
                let Look::Box(l) = look else { continue };
                let Some((a, b)) = self.element_range(&w) else { continue };
                let empty = a.same_place(&b) || props.showing_placeholder;
                let mut rects = self.range_rects(a, b, pages.clone());
                if rects.is_empty()
                    && let Some(cv) = self.caret_at(a)
                {
                    rects.push((cv.page, [cv.x, cv.top, cv.x + 60.0, cv.bottom]));
                }
                for (k, (page, rect)) in rects.into_iter().enumerate() {
                    out.push(super::view::LookView {
                        page,
                        rect,
                        fill: l.color,
                        label: if k == 0 && empty { l.label.clone() } else { None },
                        icon: None,
                        image: None,
                    });
                }
            }
        }
        out
    }

    /// The element a design-mode activation is for: the one the selection exactly covers, or with `inside`
    /// the innermost one around it (a double-click's word); reported to the host. Whether there was one.
    pub(super) fn activate(&mut self, inside: bool) -> bool {
        let sel = match self.selection {
            Selection::Object(super::selection::ObjectRef::Inline { para, offset }) => {
                let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
                Selection::Caret(Position::text(story, para, offset))
            }
            s => s,
        };
        let object = matches!(self.selection, Selection::Object(_));
        let ctx = elements::context(&self.doc, &self.index, &sel);
        let id = match ctx.iter().find(|(_, exact)| *exact || inside || object) {
            Some((id, _)) => *id,
            // A field showing its result is one unit: the caret is beside it, not in it.
            None if inside => match self.fields_here(None, false).first() {
                Some((f, _)) => ElementId::Field(*f),
                None => return false,
            },
            None => return false,
        };
        self.events.push(super::input::Notice::Activate(id));
        true
    }

    /// `control.fill`: a picture control's image, or a text or rich-text control's content as text or a
    /// DOCX fragment (a single paragraph inline; blocks only into a block-level rich-text control). One step; the
    /// control stops showing its placeholder and keeps its properties.
    pub(super) fn fill_control(&mut self, f: FillControl, fonts: &Fonts) -> Outcome {
        // The content and the control no longer showing its placeholder are one transaction.
        self.begin_batch(None);
        let r = self.control_fill(f, fonts);
        let out = self.end_batch(r.is_ok(), fonts);
        outcome(r.map(|_| out))
    }

    fn control_fill(&mut self, f: FillControl, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (e, w) = self.element(&f.id)?;
        let ElementId::Control(c) = e else { return Err(bad("id")) };
        let control = self.doc.controls.get(&c).cloned().ok_or_else(no_element)?;
        if !control.props.lock.editable() {
            return Err(locked());
        }
        let label = "Fill Content Control";
        let out = if let Some(data) = f.image {
            let data = data.0;
            let (Some((pw, ph, dx, dy)), Some(_)) = (crate::docx::image::image_info(&data), crate::docx::image::image_type(&data)) else {
                return Err(bad("image"));
            };
            let size = (pw * 72.0 / if dx > 1.0 { dx } else { 96.0 }, ph * 72.0 / if dy > 1.0 { dy } else { 96.0 });
            let Where::Inline { para, first, last, .. } = w else { return Err(bad("id")) };
            let p = self.index.para(&self.doc, para).ok_or_else(no_element)?;
            let image = p.inlines[first..last]
                .iter()
                .find_map(|i| if let Inline::Picture(pic, _) = i { pic.image } else { None })
                .ok_or_else(|| bad("id"))?;
            self.fill_images(&[image], data, size, label, fonts)
        } else {
            let given = f.text.clone().unwrap_or_default();
            let frag = match (&f.docx, &f.text) {
                (Some(d), _) => Some(self.fragment_of(&d.0)?),
                (None, Some(_)) => None,
                (None, None) => return Err(bad("text")),
            };
            let plain = matches!(control.props.kind, ControlKind::PlainText { .. } | ControlKind::Date { .. });
            if !plain && control.props.kind != ControlKind::RichText {
                return Err(bad("id"));
            }
            match (&w, frag) {
                (Where::Inline { story, para, start, end, .. }, Some(f)) if !plain => {
                    if !(f.blocks.len() == 1 && matches!(f.blocks[0], Block::Para(_))) {
                        return Err(bad("docx"));
                    }
                    self.selection = Selection::range(
                        Position::text(*story, *para, *start),
                        Position::text(*story, *para, *end),
                        super::selection::Granularity::Char,
                    );
                    self.paste_fragment(&f, fonts)
                }
                (Where::Inline { para, start, end, first, .. }, f) => {
                    // Text (a fragment's text for a plain-text control) in the content's formatting.
                    let text = match f {
                        Some(f) => f.text(),
                        None => given.clone(),
                    };
                    if plain && text.contains('\n') && !matches!(control.props.kind, ControlKind::PlainText { multi_line: true }) {
                        return Err(bad("text"));
                    }
                    let p = self.index.para(&self.doc, *para).ok_or_else(no_element)?;
                    let run = p.inlines[*first..].iter().find_map(|i| i.run().cloned()).ok_or_else(no_element)?;
                    let mut tx = self.begin(label);
                    let with = if text.is_empty() { Vec::new() } else { vec![Inline::Text(text.replace('\n', "\u{b}"), run)] };
                    if self.step(&mut tx, Step::Text { para: *para, range: *start..*end, with, edges: Default::default() }).is_err() {
                        self.abort(tx);
                        return Err(no_element());
                    }
                    self.follow(&tx);
                    self.commit(tx, None, fonts)
                }
                (Where::Blocks { container, range }, f) if !plain => {
                    // Blocks into a block-level rich-text control: they take its place, members of it.
                    let blocks = self.index.blocks(&self.doc, *container).ok_or_else(no_element)?;
                    let lead = match &blocks[range.start] {
                        Block::Para(p) => p.controls.clone(),
                        Block::Table(t) => t.controls.clone(),
                    };
                    let mut new: Vec<Block> = match f {
                        Some(f) => {
                            let mut b: Vec<Block> = f.blocks.iter().map(super::clipboard::fresh).collect();
                            for (id, k) in super::clipboard::adopt(&mut b, &f.controls) {
                                self.pending_controls.push((id, k));
                            }
                            b
                        }
                        None => {
                            let first = blocks[range.clone()].iter().find_map(super::first_para_of).ok_or_else(no_element)?;
                            let mut p = self.split_template(first).ok_or_else(no_element)?;
                            // Its membership is the control's own, put on below.
                            p.controls.clear();
                            let run = p.mark.clone();
                            p.inlines = vec![Inline::Text(given.clone(), run)];
                            vec![Block::Para(p)]
                        }
                    };
                    for b in &mut new {
                        match b {
                            Block::Para(p) => p.controls.splice(0..0, lead.iter().copied()),
                            Block::Table(t) => t.controls.splice(0..0, lead.iter().copied()),
                        };
                    }
                    let mut tx = self.begin(label);
                    if self.step(&mut tx, Step::Blocks { container: *container, range: range.clone(), with: new }).is_err() {
                        self.abort(tx);
                        return Err(no_element());
                    }
                    self.follow(&tx);
                    self.commit(tx, None, fonts)
                }
                _ => return Err(bad("id")),
            }
        };
        if !matches!(out, Outcome::Changed(_)) {
            return Err(locked());
        }
        // No longer showing its placeholder: part of the same step.
        if let Some(mut k) = self.doc.controls.get(&c).cloned().filter(|k| k.props.showing_placeholder) {
            k.props.showing_placeholder = false;
            let mut tx = self.begin(label);
            let _ = self.step(&mut tx, Step::Control { id: c, control: Some(k) });
            self.commit(tx, None, fonts);
        }
        Ok(out)
    }

    /// A DOCX fragment: its body as a clipboard fragment, its runs resolved by its
    /// own styles (a paste keeps their look).
    fn fragment_of(&self, bytes: &[u8]) -> Result<super::clipboard::Fragment, Refused> {
        let doc = crate::docx::read_editable(bytes).map_err(|_| bad("docx"))?;
        let mut blocks: Vec<Block> = doc.sections.iter().flat_map(|s| s.blocks.iter().cloned()).collect();
        // A package's body ends with a paragraph mark; an empty last paragraph after content isn't content.
        if blocks.len() > 1 && matches!(blocks.last(), Some(Block::Para(p)) if super::step::para_len(p) == Unit(0)) {
            blocks.pop();
        }
        let controls = super::clipboard::controls_of(&blocks, &doc);
        Ok(super::clipboard::Fragment { id: 0, blocks, open_end: true, cells: None, controls })
    }

    /// Locks on an edit of `a..b` (a caret: what typing there does): an edit inside a
    /// control whose content is locked, or one taking a whole control that can't be deleted, is refused. Typing at a
    /// control's start goes into it; at its end, after it.
    pub(super) fn locked_edit(&self, a: &Position, b: &Position) -> bool {
        let check = |para: ParaId, x: u32, y: u32| -> bool {
            let Some(p) = self.index.para(&self.doc, para) else { return false };
            if p.controls.iter().any(|c| self.doc.controls.get(c).is_some_and(|k| !k.props.lock.editable())) {
                return true;
            }
            let mut at = 0u32;
            let mut open: Vec<(ControlId, u32)> = Vec::new();
            for i in &p.inlines {
                match i {
                    Inline::ControlStart(c) => open.push((*c, at)),
                    Inline::ControlEnd(c) => {
                        if let Some(k) = open.iter().rposition(|(x, _)| x == c) {
                            let (_, start) = open.remove(k);
                            let end = at;
                            if let Some(lock) = self.doc.controls.get(c).map(|k| k.props.lock) {
                                let refused = if x == y {
                                    (start <= x && x < end || start == end && x == start) && !lock.editable()
                                } else if x <= start && end <= y {
                                    !lock.deletable()
                                } else {
                                    x < end && y > start && !lock.editable()
                                };
                                if refused {
                                    return true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
                at += i.units() as u32;
            }
            false
        };
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return false };
        if pa == pb {
            return check(pa, ua.0.min(ub.0), ua.0.max(ub.0));
        }
        let len = |p: ParaId| self.index.para(&self.doc, p).map_or(0, |q| super::step::para_len(q).0);
        check(pa, ua.0, len(pa).max(ua.0 + 1)) || check(pb, 0, ub.0)
    }

    /// A text edit at the selection refused by a lock: refused `locked`.
    pub(super) fn refuse_locked(&mut self) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        if self.locked_edit(&a, &b) {
            self.refused = Some("locked");
            return true;
        }
        false
    }

    /// The data parts a namespace and item id address. Several for a namespace alone is `"ambiguous namespace"`.
    fn data_parts_at(&self, ns: &str, item: Option<&str>) -> Result<(String, Option<String>, Vec<usize>), Refused> {
        let (ns, item) = (ns.to_string(), item.map(str::to_string));
        let at: Vec<usize> = self
            .doc
            .data_parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.namespace.as_deref() == Some(ns.as_str()) && item.as_ref().is_none_or(|i| *i == p.item_id))
            .map(|(k, _)| k)
            .collect();
        if item.is_none() && at.len() > 1 {
            return Err(shape("ambiguous namespace"));
        }
        Ok((ns, item, at))
    }

    pub(super) fn write_data_part(&mut self, w: WriteDataPart, fonts: &Fonts) -> Outcome {
        outcome(self.data_write(w, fonts))
    }

    /// A part's XML (well-formed, its root in `namespace`), replacing the part it addresses or added as a new one.
    /// One step, which can join a batch. Makes `{ itemId }`.
    fn data_write(&mut self, w: WriteDataPart, fonts: &Fonts) -> Result<Outcome, Refused> {
        let xml = w.xml;
        let root_ns = {
            let doc = roxmltree::Document::parse(&xml).map_err(|_| Error::invalid("not well-formed"))?;
            doc.root_element().tag_name().namespace().map(str::to_string)
        };
        let (ns, item, at) = self.data_parts_at(&w.namespace, w.item_id.as_deref())?;
        if root_ns.as_deref() != Some(ns.as_str()) {
            return Err(bad("namespace"));
        }
        let part = match at.first() {
            Some(&k) => {
                let mut p = self.doc.data_parts[k].clone();
                if p.xml == xml {
                    self.made = Some(CommandResult::DataPart { item_id: p.item_id });
                    return Ok(Outcome::Unchanged("the same XML"));
                }
                p.xml = xml;
                p.changed = true;
                p
            }
            None => {
                let item_id = item.unwrap_or_else(|| new_item_id(&xml, self.doc.data_parts.len()));
                crate::docx::DataPart { namespace: Some(ns), item_id, xml, src: None, changed: true }
            }
        };
        let item_id = part.item_id.clone();
        let mut tx = self.begin("Change Data");
        if self.step(&mut tx, Step::DataPart { item_id: item_id.clone(), part: Some(part) }).is_err() {
            self.abort(tx);
            return Err(no_element());
        }
        self.made = Some(CommandResult::DataPart { item_id });
        Ok(self.commit(tx, None, fonts))
    }

    pub(super) fn remove_data_part(&mut self, r: DataPartRef, fonts: &Fonts) -> Outcome {
        outcome(self.data_remove(r, fonts))
    }

    /// The part a reference addresses goes. One step.
    fn data_remove(&mut self, r: DataPartRef, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (_, _, at) = self.data_parts_at(&r.namespace, r.item_id.as_deref())?;
        let &k = at.first().ok_or_else(no_element)?;
        let item_id = self.doc.data_parts[k].item_id.clone();
        let mut tx = self.begin("Remove Data");
        if self.step(&mut tx, Step::DataPart { item_id, part: None }).is_err() {
            self.abort(tx);
            return Err(no_element());
        }
        Ok(self.commit(tx, None, fonts))
    }

    /// Every field with where it is (story, paragraph, begin index, start and end units), in document order.
    fn all_fields(&self) -> Vec<(FieldId, StoryId, ParaId, usize, Unit, Unit)> {
        elements::enumerate(&self.doc, &self.index, &elements::Filter { kind: Some("field".into()), ..Default::default() })
            .iter()
            .filter_map(|e| {
                let ElementId::Field(f) = e.id else { return None };
                let Where::Inline { story, para, first, start, end, .. } = self.locate(e.id)? else { return None };
                Some((f, story, para, first, start, end))
            })
            .collect()
    }

    /// The fields an F-key acts on: with a selection, every field it touches; with a caret, the
    /// innermost field around it (or right before or after it). `nested` adds the fields inside those.
    pub(super) fn fields_here(&self, ids: Option<&[crate::api::value::ElementId]>, nested: bool) -> Vec<(FieldId, ParaId)> {
        let all = self.all_fields();
        if let Some(ids) = ids {
            let want: Vec<ElementId> = ids.iter().filter_map(|x| elements::parse_id(&x.0)).collect();
            return all.iter().filter(|f| want.contains(&ElementId::Field(f.0))).map(|f| (f.0, f.2)).collect();
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Vec::new() };
        let range = |f: &(FieldId, StoryId, ParaId, usize, Unit, Unit)| (Position::text(f.1, f.2, f.4), Position::text(f.1, f.2, f.5));
        let mut picked: Vec<(FieldId, StoryId, ParaId, usize, Unit, Unit)> = if !a.same_place(&b) {
            all.iter()
                .filter(|f| {
                    let (s, e) = range(f);
                    self.index.cmp(&s, &b) == std::cmp::Ordering::Less && self.index.cmp(&a, &e) == std::cmp::Ordering::Less
                })
                .cloned()
                .collect()
        } else {
            let Some((para, u)) = a.para_offset() else { return Vec::new() };
            let here: Vec<_> = all.iter().filter(|f| f.2 == para).cloned().collect();
            let inside = here.iter().filter(|f| f.4 < u && u < f.5).min_by_key(|f| f.5.0 - f.4.0).cloned();
            match inside.or_else(|| here.iter().find(|f| f.4 == u).cloned()).or_else(|| here.iter().find(|f| f.5 == u).cloned()) {
                Some(f) => vec![f],
                None => Vec::new(),
            }
        };
        if nested {
            let more: Vec<_> = all.iter().filter(|g| picked.iter().any(|f| f.2 == g.2 && f.4 <= g.4 && g.5 <= f.5 && f.0 != g.0)).cloned().collect();
            for g in more {
                if !picked.iter().any(|f| f.0 == g.0) {
                    picked.push(g);
                }
            }
        }
        picked.into_iter().map(|f| (f.0, f.2)).collect()
    }

    /// Sets fields' views as a view setting, not an edit (`view.field-codes`): their paragraphs laid out again, with
    /// no step and no revision.
    fn set_views(&mut self, fields: &[(FieldId, ParaId)], view: FieldView) {
        let mut hints = Vec::new();
        for (f, para) in fields {
            let Some(p) = self.index.para_mut(&mut self.doc, *para) else { continue };
            for i in p.inlines.iter_mut() {
                if let Inline::FieldBegin { id, view: v, .. } = i
                    && id == f
                    && *v != view
                {
                    *v = view;
                    hints.push(super::step::para_hint(&self.index, *para, 0));
                }
            }
        }
        if !hints.is_empty() {
            self.relayout(hints);
        }
    }

    /// Fields showing their code show their result again, as an edit (a field update): a step per paragraph.
    fn show_results(&mut self, fields: &[(FieldId, ParaId)], fonts: &Fonts) -> Outcome {
        let mut paras: Vec<ParaId> = fields.iter().map(|f| f.1).collect();
        paras.sort_unstable_by_key(|p| p.0);
        paras.dedup();
        let mut tx = self.begin("Update Fields");
        for para in paras {
            let views: Vec<(FieldId, FieldView)> = fields
                .iter()
                .filter(|f| f.1 == para && self.field_view(f.0, para) == Some(FieldView::Code))
                .map(|f| (f.0, FieldView::Result))
                .collect();
            if !views.is_empty() {
                let _ = self.step(&mut tx, Step::FieldViews { para, views });
            }
        }
        self.commit(tx, None, fonts)
    }

    /// Fields `ids` (all of them without ids) in code (`show`) or result view. The caret in a field goes to its code's
    /// end when it shows its code, to its start when it shows its result. View only.
    pub(super) fn show_field_codes(&mut self, show: bool, ids: Option<&[crate::api::value::ElementId]>) -> Outcome {
        let targets: Vec<(FieldId, ParaId)> = match ids {
            Some(_) => self.fields_here(ids, false),
            None => self.all_fields().iter().map(|f| (f.0, f.2)).collect(),
        };
        self.field_views(targets, if show { FieldView::Code } else { FieldView::Result })
    }

    /// Alt+F9 (`all`) and Shift+F9 as a command: every field, or the fields here, in the other view than they show now.
    pub(super) fn field_codes_toggle(&self, all: bool) -> Option<crate::api::command::ShowFieldCodes> {
        let targets: Vec<(FieldId, ParaId)> =
            if all { self.all_fields().iter().map(|f| (f.0, f.2)).collect() } else { self.fields_here(None, false) };
        let &(f, para) = targets.first()?;
        let codes = if all {
            targets.iter().all(|&(f, p)| self.field_view(f, p) == Some(FieldView::Code))
        } else {
            self.field_view(f, para) == Some(FieldView::Code)
        };
        let ids = (!all).then(|| targets.iter().map(|&(f, _)| api_id(ElementId::Field(f))).collect());
        Some(crate::api::command::ShowFieldCodes { show: !codes, ids })
    }

    /// The fields' views set, and the caret in one of them moved to its code's end or its start.
    fn field_views(&mut self, targets: Vec<(FieldId, ParaId)>, view: FieldView) -> Outcome {
        let here = self.fields_here(None, false);
        self.set_views(&targets, view);
        if let Some(&(f, para)) = here.first().filter(|h| targets.contains(h))
            && self.selection.is_collapsed()
            && let Some(p) = self.index.para(&self.doc, para)
            && let Some(k) = p.inlines.iter().position(|i| matches!(i, Inline::FieldBegin { id, .. } if *id == f))
            && let Some(s) = crate::docx::fields::span_at(&p.inlines, k)
        {
            let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
            let u = if view == FieldView::Code { units_before(&p.inlines, s.code_end()) } else { units_before(&p.inlines, k) };
            self.selection = Selection::Caret(Position::text(story, para, Unit(u)));
        }
        Outcome::Moved
    }

    fn field_view(&self, f: FieldId, para: ParaId) -> Option<FieldView> {
        self.index.para(&self.doc, para)?.inlines.iter().find_map(|i| match i {
            Inline::FieldBegin { id, view, .. } if *id == f => Some(*view),
            _ => None,
        })
    }

    /// Ctrl+F9: a new field showing its code, two spaces with the caret between them; over a
    /// selection, the selected text is the code and the caret stays before the field; inside a code it nests.
    fn field_new(&mut self, fonts: &Fonts) -> Result<Outcome, Refused> {
        let selected = match self.selection.ordered(&self.index) {
            Some((a, b)) if !a.same_place(&b) => {
                let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return Err(bad("target")) };
                if pa != pb {
                    return Err(shape("partial paragraph"));
                }
                let p = self.index.para(&self.doc, pa).ok_or_else(no_element)?;
                let t = super::text::UnitText::of(p);
                Some(t.text.chars().skip(ua.0 as usize).take((ub.0 - ua.0) as usize).collect::<String>())
            }
            _ => None,
        };
        let id = FieldId::next();
        let code = selected.clone().unwrap_or_else(|| "  ".into());
        let out = self.insert_inlines(
            |run| {
                let mut v = crate::docx::fields::field_inlines_with(id, &code, "", &run);
                if let Some(Inline::FieldBegin { view, .. }) = v.first_mut() {
                    *view = FieldView::Code;
                }
                v
            },
            "Insert Field",
            fonts,
        );
        if !matches!(out, Outcome::Changed(_)) {
            return Err(locked());
        }
        if let Some(Where::Inline { story, para, start, .. }) = self.locate(ElementId::Field(id)) {
            let at = if selected.is_some() { start.0 } else { start.0 + 2 };
            self.selection = Selection::Caret(Position::text(story, para, Unit(at)));
        }
        self.made = Some(CommandResult::Element { id: api_id(ElementId::Field(id)) });
        Ok(out)
    }

    /// F9: the fields `ids` (default: those here, every field with Ctrl+A), innermost first, locked ones left, go to
    /// the application in one field update notice; those showing their code show their result again, as an edit.
    pub(super) fn update_fields(&mut self, ids: Option<&[crate::api::value::ElementId]>, fonts: &Fonts) -> Outcome {
        let mut fields = self.fields_here(ids, true);
        let all = self.all_fields();
        let depth = |f: &FieldId| {
            let me = all.iter().find(|g| g.0 == *f);
            me.map_or(0, |m| all.iter().filter(|g| g.2 == m.2 && g.4 < m.4 && m.5 < g.5).count())
        };
        // Innermost first, then in document order.
        let order: Vec<FieldId> = all.iter().map(|g| g.0).collect();
        fields.sort_by_key(|(f, _)| (std::cmp::Reverse(depth(f)), order.iter().position(|g| g == f)));
        let unlocked: Vec<(FieldId, ParaId)> = fields
            .into_iter()
            .filter(|(f, para)| {
                self.index
                    .para(&self.doc, *para)
                    .is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { id, lock: false, .. } if id == f)))
            })
            .collect();
        let out = match self.show_results(&unlocked, fonts) {
            o @ Outcome::Changed(_) => o,
            _ => Outcome::Moved,
        };
        self.update_token += 1;
        let token = self.update_token;
        let fields: Vec<(ElementId, String)> = unlocked
            .iter()
            .filter_map(|(f, para)| {
                let p = self.index.para(&self.doc, *para)?;
                let k = p.inlines.iter().position(|i| matches!(i, Inline::FieldBegin { id, .. } if id == f))?;
                let s = crate::docx::fields::span_at(&p.inlines, k)?;
                Some((ElementId::Field(*f), crate::docx::fields::code_text(&p.inlines, s)))
            })
            .collect();
        if !fields.is_empty() {
            self.events.push(super::input::Notice::FieldUpdate { token, fields });
        }
        out
    }

    /// The application's results for an update, as one step; fields gone since are left out.
    // shortcut: a token doesn't join the results to an earlier step, and a formatted result is set as its text.
    pub(super) fn set_field_results(&mut self, r: FieldResults, fonts: &Fonts) -> Outcome {
        let mut tx = self.begin("Update Fields");
        for result in &r.results {
            let Some(ElementId::Field(f)) = elements::parse_id(&result.id.0) else {
                self.abort(tx);
                return Outcome::Refused(no_element());
            };
            self.set_result(&mut tx, f, &plain(&result.display));
        }
        if tx.undo.is_empty() {
            self.abort(tx);
            return Outcome::Unchanged("no results");
        }
        self.follow(&tx);
        self.commit(tx, None, fonts)
    }

    /// Field `f`'s result becomes `display` (in its result's first run, else its code's); whether the field is there.
    fn set_result(&mut self, tx: &mut super::Tx, f: FieldId, display: &str) -> bool {
        let Some(Where::Inline { para, first, last, .. }) = self.locate(ElementId::Field(f)) else { return false };
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return false };
        let Some(s) = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last) else { return false };
        let at = |k: usize| Unit(units_before(&p.inlines, k));
        let begin_run = p.inlines[first].run().cloned();
        let Some(run) = s
            .sep
            .and_then(|sep| p.inlines[sep + 1..s.end].iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.clone()) } else { None }))
            .or(begin_run.clone())
        else {
            return false;
        };
        let text: Vec<Inline> = if display.is_empty() { Vec::new() } else { vec![Inline::Text(display.into(), run.clone())] };
        let step = match s.sep {
            Some(sep) => Step::Text { para, range: Unit(at(sep).0 + 1)..at(s.end), with: text, edges: Default::default() },
            None => {
                let mut with = vec![Inline::FieldSep { id: f, run }];
                with.extend(text);
                Step::Text { para, range: at(s.end)..at(s.end), with, edges: Default::default() }
            }
        };
        self.step(tx, step).is_ok()
    }

    /// Ctrl+F11 / Ctrl+Shift+F11 and `field.lock` / `field.unlock`: the fields here and those inside
    /// them, locked or unlocked, one step.
    pub(super) fn lock_fields(&mut self, lock: bool, ids: Option<&[crate::api::value::ElementId]>, fonts: &Fonts) -> Outcome {
        let fields = self.fields_here(ids, true);
        let mut paras: Vec<ParaId> = fields.iter().map(|(_, p)| *p).collect();
        paras.dedup();
        let mut tx = self.begin(if lock { "Lock Fields" } else { "Unlock Fields" });
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut inlines = p.inlines.clone();
            let mut changed = false;
            for i in inlines.iter_mut() {
                if let Inline::FieldBegin { id, lock: l, .. } = i
                    && fields.iter().any(|(f, _)| f == id)
                    && *l != lock
                {
                    *l = lock;
                    changed = true;
                }
            }
            if changed {
                let _ = self.step(&mut tx, Step::Restyle { para, inlines });
            }
        }
        if tx.undo.is_empty() {
            self.abort(tx);
            return Outcome::Unchanged("no field to change");
        }
        self.commit(tx, None, fonts)
    }

    /// Ctrl+Shift+F9 and `field.unlink`: each field here replaced by its result's text, one step.
    pub(super) fn unlink_fields(&mut self, ids: Option<&[crate::api::value::ElementId]>, fonts: &Fonts) -> Outcome {
        let mut fields = self.fields_here(ids, false);
        // Outermost only: an outer field's result replaces those inside it too.
        let all = self.all_fields();
        let span = |f: &FieldId| all.iter().find(|g| g.0 == *f).map(|g| (g.2, g.4, g.5));
        let keep: Vec<(FieldId, ParaId)> = fields
            .iter()
            .filter(|(f, _)| {
                let Some((p, s, e)) = span(f) else { return false };
                !fields.iter().any(|(g, _)| g != f && span(g).is_some_and(|(q, gs, ge)| q == p && gs <= s && e <= ge))
            })
            .cloned()
            .collect();
        fields = keep;
        // Last first, so the earlier ones' units stay where they are.
        fields.sort_by_key(|(f, _)| std::cmp::Reverse(all.iter().position(|g| g.0 == *f)));
        let mut tx = self.begin("Unlink Fields");
        for (f, para) in fields {
            let Some(Where::Inline { first, last, start, end, .. }) = self.locate(ElementId::Field(f)) else { continue };
            let Some(p) = self.index.para(&self.doc, para).cloned() else { continue };
            let Some(s) = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last) else { continue };
            let text = crate::docx::fields::result_text(&p.inlines, s);
            let run =
                s.sep.and_then(|sep| p.inlines[sep + 1..s.end].iter().find_map(|i| i.run().cloned())).or_else(|| p.inlines[first].run().cloned());
            let with = match run {
                Some(r) if !text.is_empty() => vec![Inline::Text(text, r)],
                _ => Vec::new(),
            };
            let _ = self.step(&mut tx, Step::Text { para, range: start..end, with, edges: Default::default() });
        }
        if tx.undo.is_empty() {
            self.abort(tx);
            return Outcome::Unchanged("no field here");
        }
        self.follow(&tx);
        self.commit(tx, None, fonts)
    }

    /// A text selection with one end inside a field showing its code and the other outside covers the whole field;
    /// the end inside moves past the field on its side.
    pub(super) fn widen_over_codes(&self, a: Position, b: Position) -> (Position, Position) {
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return (a, b) };
        let codes = |para: ParaId| -> Vec<(u32, u32)> {
            let Some(p) = self.index.para(&self.doc, para) else { return Vec::new() };
            crate::docx::fields::spans(&p.inlines)
                .into_iter()
                .filter(|s| matches!(p.inlines[s.begin], Inline::FieldBegin { view: FieldView::Code, .. }))
                .map(|s| (units_before(&p.inlines, s.begin), units_before(&p.inlines, s.end)))
                .collect()
        };
        // A position inside a code: after its opening brace, up to before its closing one.
        let inside = |u: u32, (s, e): (u32, u32)| s < u && u <= e;
        let (mut ua2, mut ub2) = (ua, ub);
        for f in codes(pa) {
            let b_out = pb != pa || !inside(ub.0, f);
            if inside(ua.0, f) && b_out {
                ua2 = Unit(f.0);
            }
        }
        for f in codes(pb) {
            let a_out = pa != pb || !inside(ua.0, f);
            if inside(ub.0, f) && a_out {
                ub2 = Unit(f.1 + 1);
            }
        }
        // Positions that didn't move keep their affinity.
        let a = if ua2 == ua { a } else { Position::text(a.story, pa, ua2) };
        let b = if ub2 == ub { b } else { Position::text(b.story, pb, ub2) };
        (a, b)
    }

    /// The application's labels for empty fields, by code; `merge` adds to those it has. View only.
    pub(super) fn set_field_labels(&mut self, l: FieldLabels) -> Outcome {
        if !l.merge.unwrap_or(false) {
            self.field_labels.clear();
        }
        for (code, label) in l.labels {
            self.field_labels.insert(code.trim().to_string(), label);
        }
        self.refresh_labels(None);
        Outcome::Moved
    }

    /// Design mode's labels: every empty field of the paragraphs `steps` touched (all
    /// fields with none) shows the host's label for its code, or its first argument in «»; outside design mode none
    /// does. A view setting, not an edit: no step, never saved.
    pub(super) fn refresh_labels(&mut self, steps: Option<&[Step]>) {
        let design = self.mode == super::Mode::Design;
        let paras: Vec<ParaId> = match steps {
            Some(steps) => {
                let mut v: Vec<ParaId> = steps
                    .iter()
                    .filter_map(|s| match s {
                        Step::Text { para, .. } | Step::Restyle { para, .. } | Step::Split { para, .. } => Some(*para),
                        Step::Join { first, .. } => Some(*first),
                        _ => None,
                    })
                    .collect();
                // Blocks put back or added (undoing a deletion, a paste): their fields' paragraphs.
                if steps.iter().any(|s| matches!(s, Step::Blocks { .. } | Step::Rows { .. } | Step::Note { .. } | Step::HeaderPart { .. })) {
                    v.extend(self.elems.values().filter(|e| e.props.is_none()).filter_map(|e| e.at));
                }
                v
            }
            None => self.elems.values().filter(|e| e.props.is_none()).filter_map(|e| e.at).collect(),
        };
        let mut paras = paras;
        paras.sort_unstable_by_key(|p| p.0);
        paras.dedup();
        let mut hints = Vec::new();
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut want: Vec<(usize, Option<std::rc::Rc<str>>)> = Vec::new();
            for s in crate::docx::fields::spans(&p.inlines) {
                let Inline::FieldBegin { label, .. } = &p.inlines[s.begin] else { continue };
                let empty = s.sep.is_none_or(|sep| sep + 1 == s.end);
                let new: Option<std::rc::Rc<str>> = (design && empty).then(|| {
                    let code = crate::docx::fields::code_text(&p.inlines, s);
                    let code = code.trim();
                    match self.field_labels.get(code) {
                        Some(l) => l.as_str().into(),
                        None => format!("\u{ab}{}\u{bb}", crate::docx::fields::parse(code).args.first().map_or(code, String::as_str)).into(),
                    }
                });
                if *label != new {
                    want.push((s.begin, new));
                }
            }
            if want.is_empty() {
                continue;
            }
            let Some(p) = self.index.para_mut(&mut self.doc, para) else { continue };
            for (k, new) in want {
                if let Inline::FieldBegin { label, .. } = &mut p.inlines[k] {
                    *label = new;
                }
            }
            hints.push(super::step::para_hint(&self.index, para, 0));
        }
        if !hints.is_empty() {
            self.relayout(hints);
        }
    }

    /// A field from its code and display text at the selection; the caret after it, or at the start of its code when
    /// it shows its code. An empty code makes a new field the way Ctrl+F9 does. Makes `{ id }`.
    pub(super) fn insert_field(&mut self, f: InsertField, fonts: &Fonts) -> Outcome {
        if f.code.trim().is_empty() {
            return outcome(self.editable_target().and_then(|_| self.field_new(fonts)));
        }
        outcome(self.field_insert(&f.code, f.display.as_deref().unwrap_or(""), f.show_code.unwrap_or(false), fonts))
    }

    fn field_insert(&mut self, code: &str, display: &str, code_view: bool, fonts: &Fonts) -> Result<Outcome, Refused> {
        self.editable_target()?;
        let id = FieldId::next();
        let code = padded(code);
        let out = self.insert_inlines(
            |run| {
                let mut v = crate::docx::fields::field_inlines_with(id, &code, display, &run);
                if code_view && let Some(Inline::FieldBegin { view, .. }) = v.first_mut() {
                    *view = FieldView::Code;
                }
                v
            },
            "Insert Field",
            fonts,
        );
        let Outcome::Changed(_) = out else { return Err(locked()) };
        if code_view && let Some(Where::Inline { story, para, start, .. }) = self.locate(ElementId::Field(id)) {
            self.selection = Selection::Caret(Position::text(story, para, Unit(start.0 + 1)));
        }
        self.made = Some(CommandResult::Element { id: api_id(ElementId::Field(id)) });
        Ok(out)
    }

    pub(super) fn set_field(&mut self, f: SetField, fonts: &Fonts) -> Outcome {
        // shortcut: a formatted result is set as its text, until the keep-source fragment paste.
        let display = f.display.as_ref().map(plain);
        outcome(self.field_set(&f.id, f.code.as_deref(), display.as_deref(), fonts))
    }

    /// A field's code and/or display text, as one step; what isn't given is kept.
    pub(super) fn field_set(
        &mut self,
        id: &crate::api::value::ElementId,
        code: Option<&str>,
        display: Option<&str>,
        fonts: &Fonts,
    ) -> Result<Outcome, Refused> {
        let (e, w) = self.element(id)?;
        let (ElementId::Field(f), Where::Inline { para, first, last, .. }) = (e, w) else { return Err(bad("id")) };
        if code.is_none() && display.is_none() {
            return Ok(Outcome::Unchanged("nothing to change"));
        }
        let p = self.index.para(&self.doc, para).ok_or_else(no_element)?.clone();
        let s = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last).ok_or_else(no_element)?;
        let begin_run = p.inlines[first].run().cloned().ok_or_else(no_element)?;
        let run_in =
            |from: usize, to: usize| p.inlines[from..to].iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.clone()) } else { None });
        let at = |k: usize| Unit(units_before(&p.inlines, k));
        let mut tx = self.begin("Change Field");
        // The result first: it comes after the code, so the code's units stay where they are.
        if let Some(d) = display {
            let run = s.sep.and_then(|sep| run_in(sep + 1, s.end)).unwrap_or_else(|| begin_run.clone());
            let text: Vec<Inline> = if d.is_empty() { Vec::new() } else { vec![Inline::Text(d.into(), run)] };
            let step = match s.sep {
                Some(sep) => Step::Text { para, range: Unit(at(sep).0 + 1)..at(s.end), with: text, edges: Default::default() },
                None => {
                    let mut with = vec![Inline::FieldSep { id: f, run: begin_run.clone() }];
                    with.extend(text);
                    Step::Text { para, range: at(s.end)..at(s.end), with, edges: Default::default() }
                }
            };
            if self.step(&mut tx, step).is_err() {
                self.abort(tx);
                return Err(no_element());
            }
        }
        if let Some(c) = code {
            let run = run_in(first + 1, s.code_end()).unwrap_or(begin_run.clone());
            let range = Unit(at(first).0 + 1)..at(s.code_end());
            if self.step(&mut tx, Step::Text { para, range, with: vec![Inline::Text(padded(c), run)], edges: Default::default() }).is_err() {
                self.abort(tx);
                return Err(no_element());
            }
            self.refresh_field_kinds(&mut tx, para);
        }
        self.follow(&tx);
        Ok(self.commit(tx, None, fonts))
    }

    /// The selection carried through the transaction's steps, for commands that don't place it themselves (an
    /// element removed under it, a result made shorter). Anything but text and a caret becomes a caret at where its
    /// focus went.
    fn follow(&mut self, tx: &super::Tx) {
        let map = |p: Position| tx.maps.iter().fold(p, |p, m| m.apply(p));
        self.selection = match self.selection {
            Selection::Caret(p) => Selection::Caret(map(p)),
            Selection::Text { anchor, focus, unit } => Selection::range(map(anchor), map(focus), unit),
            other => match other.focus() {
                Some(p) => Selection::Caret(map(p)),
                None => other,
            },
        };
        let valid =
            |p: &Position| p.para_offset().is_none_or(|(para, u)| self.index.para(&self.doc, para).is_some_and(|q| u <= super::step::para_len(q)));
        if !self.selection.focus().is_some_and(|p| valid(&p)) || !self.selection.anchor().is_none_or(|p| valid(&p)) {
            // A position the steps couldn't follow: the start of the element's paragraph, else the story's start.
            self.selection = Selection::Caret(tx.before.selection.focus().map(map).filter(|p| valid(p)).unwrap_or_else(|| {
                let story = self.selection.focus().map_or(StoryId::Body, |p| p.story);
                self.nav().story_edge(story, false).unwrap_or(Position::text(StoryId::Body, ParaId(0), Unit(0)))
            }));
        }
    }

    /// Gives every field of `para` the kind its code says, after a code changed.
    pub(super) fn refresh_field_kinds(&mut self, tx: &mut super::Tx, para: ParaId) {
        let Some(p) = self.index.para(&self.doc, para) else { return };
        let mut inlines = p.inlines.clone();
        let mut changed = false;
        for s in crate::docx::fields::spans(&p.inlines) {
            let want = crate::docx::fields::kind_at(&p.inlines, s);
            if let Inline::FieldBegin { kind, .. } = &mut inlines[s.begin]
                && *kind != want
            {
                *kind = want;
                changed = true;
            }
        }
        if changed {
            let _ = self.step(tx, Step::Restyle { para, inlines });
        }
    }

    /// A field with what it holds, or a control with its content; with
    /// `keep`, a control's markup only (its content stays where it is).
    pub(super) fn remove_element(&mut self, id: &crate::api::value::ElementId, keep: bool, fonts: &Fonts) -> Outcome {
        outcome(self.element_remove(id, keep, fonts))
    }

    fn element_remove(&mut self, id: &crate::api::value::ElementId, keep: bool, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (e, w) = self.element(id)?;
        if let ElementId::Control(c) = e
            && self.doc.controls.get(&c).is_some_and(|k| !k.props.lock.deletable())
        {
            return Err(locked());
        }
        let mut tx = self.begin(if matches!(e, ElementId::Field(_)) { "Remove Field" } else { "Remove Content Control" });
        let ok = match (e, w) {
            (ElementId::Field(_), Where::Inline { para, start, end, .. }) => {
                self.step(&mut tx, Step::Text { para, range: start..end, with: Vec::new(), edges: Default::default() }).is_ok()
            }
            (ElementId::Control(c), Where::Inline { para, start, end, .. }) => {
                // The markers first (they take no units), then the content unless it is kept.
                let p = self.index.para(&self.doc, para).ok_or_else(no_element)?;
                let inlines: Vec<Inline> =
                    p.inlines.iter().filter(|i| !matches!(i, Inline::ControlStart(x) | Inline::ControlEnd(x) if *x == c)).cloned().collect();
                self.step(&mut tx, Step::Restyle { para, inlines }).is_ok()
                    && (keep
                        || start == end
                        || self.step(&mut tx, Step::Text { para, range: start..end, with: Vec::new(), edges: Default::default() }).is_ok())
            }
            (ElementId::Control(c), Where::Sym { para, at, .. }) => {
                let p = self.index.para(&self.doc, para).ok_or_else(no_element)?;
                if keep {
                    // The face stays as a symbol of its own.
                    let inlines: Vec<Inline> = p
                        .inlines
                        .iter()
                        .map(|i| match i {
                            Inline::Sym { ch, font, run, control: Some(x) } if *x == c => {
                                Inline::Sym { ch: *ch, font: font.clone(), run: run.clone(), control: None }
                            }
                            other => other.clone(),
                        })
                        .collect();
                    self.step(&mut tx, Step::Restyle { para, inlines }).is_ok()
                } else {
                    self.step(&mut tx, Step::Text { para, range: at..Unit(at.0 + 1), with: Vec::new(), edges: Default::default() }).is_ok()
                }
            }
            (ElementId::Control(c), Where::Rows { table, range }) => {
                let t = self.index.table(&self.doc, table).ok_or_else(no_element)?;
                let gone = self.with_items(c, t.rows[range.clone()].iter().map(|r| &r.controls));
                if keep {
                    let with: Vec<crate::docx::Row> = t.rows[range.clone()]
                        .iter()
                        .map(|r| {
                            let mut r = r.clone();
                            r.controls.retain(|x| !gone.contains(x));
                            r
                        })
                        .collect();
                    self.step(&mut tx, Step::Rows { table, range, with, grid: None }).is_ok()
                } else if range.len() == t.rows.len() {
                    // Every row: the table goes.
                    let slot = self.index.table_slot(table).ok_or_else(no_element)?;
                    self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() }).is_ok()
                } else {
                    self.step(&mut tx, Step::Rows { table, range, with: Vec::new(), grid: None }).is_ok()
                }
            }
            (ElementId::Control(c), Where::Blocks { container, range, .. }) => {
                let blocks = self.index.blocks(&self.doc, container).ok_or_else(no_element)?;
                let gone = self.with_items(
                    c,
                    blocks[range.clone()].iter().map(|b| match b {
                        Block::Para(p) => &p.controls,
                        Block::Table(t) => &t.controls,
                    }),
                );
                let with: Vec<Block> = if keep {
                    blocks[range.clone()].iter().map(|b| gone.iter().fold(b.clone(), |b, x| without_control(&b, *x))).collect()
                } else if range.len() == blocks.len() || (range.end == blocks.len() && matches!(blocks[range.start - 1], Block::Table(_))) {
                    // A container keeps one paragraph, and ends with one: an empty one like the last, outside the
                    // control.
                    let last =
                        blocks.iter().rev().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None }).ok_or_else(no_element)?;
                    let mut p = self.split_template(last).ok_or_else(no_element)?;
                    p.controls.retain(|x| !gone.contains(x));
                    vec![Block::Para(p)]
                } else {
                    Vec::new()
                };
                self.step(&mut tx, Step::Blocks { container, range, with }).is_ok()
            }
            _ => false,
        };
        if !ok {
            self.abort(tx);
            return Err(no_element());
        }
        self.follow(&tx);
        Ok(self.commit(tx, None, fonts))
    }

    pub(super) fn insert_control(&mut self, c: InsertControl, fonts: &Fonts) -> Outcome {
        outcome(self.control_insert(c, fonts))
    }

    /// A control of `kind` at the selection, its content its placeholder text (the text with `showing_placeholder`, no
    /// docPart); with `wrapSelection` (rich text, group), around the selection instead. Makes `{ id }`.
    fn control_insert(&mut self, c: InsertControl, fonts: &Fonts) -> Result<Outcome, Refused> {
        let w = self.cx().w;
        let esc = crate::docx::write::props::esc;
        let mut checked_now = false;
        let mut kind_xml = None;
        let kind = match (c.kind, &c.options) {
            (KindArg::PlainText, Some(ControlOptions::PlainText(o))) => ControlKind::PlainText { multi_line: o.multiline.unwrap_or(false) },
            (KindArg::PlainText, _) => ControlKind::PlainText { multi_line: false },
            (KindArg::RichText, _) => ControlKind::RichText,
            (KindArg::Picture, _) => ControlKind::Picture,
            (KindArg::Checkbox, o) => {
                checked_now = matches!(o, Some(ControlOptions::Checkbox(o)) if o.checked == Some(true));
                ControlKind::Checkbox { checked: '\u{2612}', unchecked: '\u{2610}' }
            }
            (KindArg::Date, Some(ControlOptions::Date(o))) => {
                ControlKind::Date { format: o.format.clone().unwrap_or_else(|| "M/d/yyyy".into()), lid: o.locale.clone() }
            }
            (KindArg::Date, _) => ControlKind::Date { format: "M/d/yyyy".into(), lid: None },
            (k @ (KindArg::DropDown | KindArg::ComboBox), o) => {
                let name = if k == KindArg::DropDown { "dropDownList" } else { "comboBox" };
                let items = match o {
                    Some(ControlOptions::List(l)) => l.items.clone(),
                    _ => Vec::new(),
                };
                let items: String =
                    items.iter().map(|i| format!("<{w}listItem {w}displayText=\"{}\" {w}value=\"{}\"/>", esc(&i.text), esc(&i.value))).collect();
                kind_xml = Some(format!("<{w}{name}>{items}</{w}{name}>"));
                ControlKind::DropDown
            }
            (KindArg::Group, _) => ControlKind::Group,
            (KindArg::RepeatingSection, _) => return Err(Error::invalid("repeating sections are made by wrapInRepeatingSection")),
        };
        let wrap = c.wrap_selection.unwrap_or(false);
        if wrap && !matches!(kind, ControlKind::RichText | ControlKind::Group) {
            return Err(Error::invalid("only rich text and group controls wrap the selection"));
        }
        let placeholder = match c.placeholder {
            Some(t) => Some(t),
            None if wrap => None,
            None => match kind {
                ControlKind::PlainText { .. } | ControlKind::RichText => Some("Click or tap here to enter text.".into()),
                ControlKind::Date { .. } => Some("Click or tap to enter a date.".into()),
                ControlKind::DropDown => Some("Choose an item.".into()),
                _ => None,
            },
        };
        let mut props = ControlProps {
            kind: kind.clone(),
            tag: c.tag,
            alias: c.title,
            lock: c.lock.map(lock_of).unwrap_or_default(),
            binding: c.binding.as_ref().map(|b| binding_xml(b, &w)),
            kind_xml,
            ..Default::default()
        };
        if matches!(kind, ControlKind::PlainText { .. } | ControlKind::RichText | ControlKind::Date { .. } | ControlKind::DropDown)
            && placeholder.is_some()
        {
            props.showing_placeholder = true;
        }
        self.editable_target()?;
        let id = ControlId::next();
        if wrap {
            let out = self.control_wrap(id, props, fonts)?;
            self.made = Some(CommandResult::Element { id: api_id(ElementId::Control(id)) });
            return Ok(out);
        }
        let out = match kind {
            ControlKind::Checkbox { checked, unchecked } => {
                let face = if checked_now { checked } else { unchecked };
                self.pending_controls.push((id, Control::new(props)));
                self.pending_font("MS Gothic");
                let out = self.insert_inlines(
                    |run| vec![Inline::Sym { ch: face, font: "MS Gothic".into(), run, control: Some(id) }],
                    "Insert Content Control",
                    fonts,
                );
                self.pending = None;
                out
            }
            ControlKind::Picture => {
                props.showing_placeholder = true;
                self.pending_controls.push((id, Control::new(props)));
                let image = Some(self.pending_image(PICTURE_PLACEHOLDER.to_vec()));
                let picture = crate::docx::Picture { w: 144.0, h: 108.0, image, alt: "".into(), lock_aspect: true };
                self.insert_inlines(
                    |run| vec![Inline::ControlStart(id), Inline::Picture(picture.clone(), run), Inline::ControlEnd(id)],
                    "Insert Content Control",
                    fonts,
                )
            }
            _ => {
                self.pending_controls.push((id, Control::new(props)));
                let text = placeholder.unwrap_or_default();
                self.insert_inlines(
                    |run| {
                        let mut v = vec![Inline::ControlStart(id)];
                        if !text.is_empty() {
                            v.push(Inline::Text(text.clone(), run));
                        }
                        v.push(Inline::ControlEnd(id));
                        v
                    },
                    "Insert Content Control",
                    fonts,
                )
            }
        };
        match out {
            Outcome::Changed(_) => {
                self.made = Some(CommandResult::Element { id: api_id(ElementId::Control(id)) });
                Ok(out)
            }
            _ => Err(locked()),
        }
    }

    /// A new control around the selection: inline markers around a range in one paragraph, or membership of whole
    /// paragraphs (and the tables between them) of one container.
    fn control_wrap(&mut self, id: ControlId, props: ControlProps, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (a, b) = self.selection.ordered(&self.index).ok_or_else(|| bad("target"))?;
        if a.same_place(&b) {
            return Err(bad("target"));
        }
        let ((pa, oa), (pb, ob)) =
            (a.para_offset().ok_or_else(|| shape("partial paragraph"))?, b.para_offset().ok_or_else(|| shape("partial paragraph"))?);
        if pa == pb {
            let p = self.index.para(&self.doc, pa).ok_or_else(no_element)?;
            let mut inlines = split_at(&p.inlines, &[oa.0, ob.0]);
            // After the markers at the range's start and before those at its end, as text typed there would go.
            let index_at = |inlines: &[Inline], u: u32, after: bool| {
                let mut n = 0u32;
                for (k, x) in inlines.iter().enumerate() {
                    let w = x.units() as u32;
                    if n == u && (w > 0 || !after) {
                        return k;
                    }
                    n += w;
                }
                inlines.len()
            };
            let (i, j) = (index_at(&inlines, oa.0, true), index_at(&inlines, ob.0, false));
            // The new control nests with the others and with fields: none of them has one end inside it and the other
            // outside.
            let mut open: Vec<String> = Vec::new();
            for x in &inlines[i..j.max(i)] {
                match x {
                    Inline::ControlStart(c) => open.push(format!("c{}", c.0)),
                    Inline::FieldBegin { id, .. } => open.push(format!("f{}", id.0)),
                    Inline::ControlEnd(c) if open.last() == Some(&format!("c{}", c.0)) => {
                        open.pop();
                    }
                    Inline::FieldEnd { id, .. } if open.last() == Some(&format!("f{}", id.0)) => {
                        open.pop();
                    }
                    Inline::ControlEnd(_) | Inline::FieldEnd { .. } => return Err(bad("target")),
                    _ => {}
                }
            }
            if !open.is_empty() {
                return Err(bad("target"));
            }
            inlines.insert(j.max(i), Inline::ControlEnd(id));
            inlines.insert(i, Inline::ControlStart(id));
            let mut tx = self.begin("Insert Content Control");
            self.pending_controls.push((id, Control::new(props)));
            if self.step(&mut tx, Step::Restyle { para: pa, inlines }).is_err() {
                self.abort(tx);
                return Err(bad("target"));
            }
            return Ok(self.commit(tx, None, fonts));
        }
        // Whole paragraphs: from a paragraph's start to the start of a later one, or to the end of the last.
        let (sa, sb) = (self.index.para_slot(pa).ok_or_else(no_element)?, self.index.para_slot(pb).ok_or_else(no_element)?);
        let end_len = self.index.para(&self.doc, pb).map_or(Unit(0), super::step::para_len);
        if oa != Unit(0) || sa.container != sb.container || (ob != Unit(0) && ob != end_len) {
            return Err(shape("partial paragraph"));
        }
        let last = if ob == Unit(0) { sb.block } else { sb.block + 1 };
        let blocks = self.index.blocks(&self.doc, sa.container).ok_or_else(no_element)?;
        let with: Vec<Block> = blocks[sa.block..last]
            .iter()
            .map(|b| {
                let mut b = b.clone();
                match &mut b {
                    Block::Para(p) => p.controls.push(id),
                    Block::Table(t) => t.controls.push(id),
                }
                b
            })
            .collect();
        let mut tx = self.begin("Insert Content Control");
        self.pending_controls.push((id, Control::new(props)));
        if self.step(&mut tx, Step::Blocks { container: sa.container, range: sa.block..last, with }).is_err() {
            self.abort(tx);
            return Err(bad("target"));
        }
        Ok(self.commit(tx, None, fonts))
    }

    pub(super) fn set_control(&mut self, c: SetControl, fonts: &Fonts) -> Outcome {
        outcome(self.control_set(c, fonts))
    }

    /// A control's tag, title, lock, binding, kind settings and placeholder text, as one step. A control showing its
    /// placeholder shows the new one.
    fn control_set(&mut self, set: SetControl, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (e, w) = self.element(&set.id)?;
        let ElementId::Control(c) = e else { return Err(Error::invalid("not a content control")) };
        let mut control = self.doc.controls.get(&c).cloned().ok_or_else(no_element)?;
        let prefix = self.cx().w;
        let p = &mut control.props;
        if set.tag.is_some() {
            p.tag = set.tag;
        }
        if set.title.is_some() {
            p.alias = set.title;
        }
        if let Some(l) = set.lock {
            p.lock = lock_of(l);
        }
        if let Some(b) = set.binding {
            p.binding = b.as_ref().map(|b| binding_xml(b, &prefix));
        }
        if let Some(o) = &set.options {
            match (&mut p.kind, o) {
                (ControlKind::PlainText { multi_line }, ControlOptions::PlainText(o)) => {
                    if let Some(m) = o.multiline {
                        *multi_line = m;
                    }
                }
                (ControlKind::Date { format, lid }, ControlOptions::Date(o)) => {
                    if let Some(f) = &o.format {
                        *format = f.clone();
                    }
                    if o.locale.is_some() {
                        *lid = o.locale.clone();
                    }
                }
                (ControlKind::Checkbox { .. }, ControlOptions::Checkbox(_)) => {}
                _ => return Err(Error::invalid("options for another kind of control")),
            }
            // The kind element as read no longer says what it is.
            if !matches!(p.kind, ControlKind::DropDown | ControlKind::Checkbox { .. }) {
                p.kind_xml = None;
            }
        }
        let placeholder = set.placeholder;
        let showing = p.showing_placeholder;
        let mut tx = self.begin("Change Content Control");
        if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_err() {
            self.abort(tx);
            return Err(no_element());
        }
        if let (Some(t), true, Where::Inline { para, start, end, first, .. }) = (placeholder, showing, w) {
            let run = self.index.para(&self.doc, para).and_then(|p| p.inlines[first..].iter().find_map(|i| i.run().cloned()));
            if let Some(run) = run {
                let with = if t.is_empty() { Vec::new() } else { vec![Inline::Text(t, run)] };
                let _ = self.step(&mut tx, Step::Text { para, range: start..end, with, edges: Default::default() });
            }
        }
        self.follow(&tx);
        Ok(self.commit(tx, None, fonts))
    }

    /// Checks or clears a checkbox control.
    pub(super) fn set_checkbox(&mut self, c: SetCheckbox, fonts: &Fonts) -> Outcome {
        let Ok((ElementId::Control(id), Where::Sym { story, para, at })) = self.element(&c.id) else {
            return Outcome::Refused(Error::invalid("not a checkbox"));
        };
        let Some(ControlKind::Checkbox { checked, .. }) = self.doc.controls.get(&id).map(|k| k.props.kind.clone()) else {
            return Outcome::Refused(Error::invalid("not a checkbox"));
        };
        let face = self.index.para(&self.doc, para).and_then(|p| super::stories::inline_at(p, at)).and_then(|i| match i {
            Inline::Sym { ch, .. } => Some(*ch),
            _ => None,
        });
        if (face == Some(checked)) == c.checked {
            return Outcome::Unchanged("already so");
        }
        self.toggle_checkbox(Position::text(story, para, at), fonts)
    }
}

/// A block without control `c` in its membership lists (unwrapping keeps the block).
fn without_control(b: &Block, c: ControlId) -> Block {
    let mut b = b.clone();
    match &mut b {
        Block::Para(p) => p.controls.retain(|x| *x != c),
        Block::Table(t) => t.controls.retain(|x| *x != c),
    }
    b
}

/// `inlines` with their texts split at units `cuts`, so a marker can go between any two characters.
fn split_at(inlines: &[Inline], cuts: &[u32]) -> Vec<Inline> {
    let mut out = Vec::with_capacity(inlines.len() + 2);
    let mut n = 0u32;
    for i in inlines {
        let w = i.units() as u32;
        match i {
            Inline::Text(t, r) if cuts.iter().any(|&c| n < c && c < n + w) => {
                let mut at: Vec<usize> = cuts.iter().filter(|&&c| n < c && c < n + w).map(|&c| (c - n) as usize).collect();
                at.sort_unstable();
                at.dedup();
                let chars: Vec<char> = t.chars().collect();
                let mut from = 0;
                for k in at.into_iter().chain(std::iter::once(chars.len())) {
                    out.push(Inline::Text(chars[from..k].iter().collect(), r.clone()));
                    from = k;
                }
            }
            _ => out.push(i.clone()),
        }
        n += w;
    }
    out
}

/// A new part's `ds:itemID`: Word's braced GUID form, from the XML and the parts there are (unique enough in one
/// package; the writer checks for clashes).
fn new_item_id(xml: &str, n: usize) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (xml, n, "data part").hash(&mut h);
    let a = h.finish();
    (a, "second").hash(&mut h);
    let b = h.finish();
    format!("{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}", a >> 32, (a >> 16) & 0xFFFF, a & 0xFFF, 0x8000 | (b >> 48) & 0x3FFF, b & 0xFFFF_FFFF_FFFF)
}
