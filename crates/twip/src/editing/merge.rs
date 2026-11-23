//! Merge designer operations

use crate::api::command::{
    CommandResult, DataPartRef, ElementClass, ElementField, FieldLabels, FieldResults, FillControl, InsertControl, InsertField, MarkElements,
    RepeatingSection, ReplaceInElements, SetCheckbox, SetControl, SetField, Severity as SeverityArg, WriteDataPart,
};
use crate::api::error::Error;
use crate::api::review::Body;
use crate::api::value::{ControlKind as KindArg, ControlLock, ControlOptions, DataBinding};
use crate::docx::{Block, Control, ControlId, ControlKind, ControlProps, ElementId, FieldId, FieldKind, FieldView, Inline, Lock};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::elements;
use super::index::{Container, Loc};
use super::position::{Position, StoryId};
use super::selection::Selection;
use super::step::Step;

type Refused = Error;

#[derive(Clone, Debug)]
pub(super) struct Mark {
    pub id: ElementId,
    pub severity: super::view::Severity,
    pub message: String,
}

const PICTURE_PLACEHOLDER: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65,
    84, 120, 156, 99, 120, 241, 226, 5, 0, 5, 214, 2, 233, 151, 52, 29, 132, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[derive(Clone, Debug)]
pub(super) enum Where {
    Inline { story: StoryId, para: ParaId, first: usize, last: usize, start: Unit, end: Unit },
    Sym { story: StoryId, para: ParaId, at: Unit },
    Blocks { container: Container, range: std::ops::Range<usize> },
    Rows { table: crate::layout::TableId, range: std::ops::Range<usize> },
}

fn units_before(inlines: &[Inline], k: usize) -> u32 {
    inlines[..k].iter().map(|i| i.units() as u32).sum()
}

/// The first (or last) outermost field in `inlines` that starts after (or before) unit `from`, when given: its units.
fn outer_field(inlines: &[Inline], first: bool, from: Option<u32>) -> Option<(Unit, Unit)> {
    let (mut depth, mut at, mut pick) = (0usize, 0u32, None);
    for (k, i) in inlines.iter().enumerate() {
        match i {
            Inline::FieldBegin { .. } => {
                if depth == 0 && from.is_none_or(|u| if first { at > u } else { at < u }) {
                    pick = Some((k, at));
                    if first {
                        break;
                    }
                }
                depth += 1;
            }
            Inline::FieldEnd { .. } => depth = depth.saturating_sub(1),
            _ => {}
        }
        at += i.units() as u32;
    }
    let (k, start) = pick?;
    let s = crate::docx::fields::span_at(inlines, k)?;
    Some((Unit(start), Unit(units_before(inlines, s.end) + 1)))
}

fn bad(name: &str) -> Refused {
    Error::invalid(format!("bad {name}"))
}

fn no_element() -> Refused {
    Error::invalid("no such element")
}

fn locked() -> Refused {
    Error::unavailable("locked")
}

fn shape(why: &str) -> Refused {
    Error::unavailable(why)
}

pub(super) fn outcome(r: Result<Outcome, Refused>) -> Outcome {
    r.unwrap_or_else(Outcome::Refused)
}

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

fn element_id(id: &crate::api::value::ElementId) -> Result<ElementId, Refused> {
    elements::parse_id(&id.0).ok_or_else(no_element)
}

pub(crate) fn api_id(e: ElementId) -> crate::api::value::ElementId {
    crate::api::value::ElementId(elements::id_str(e))
}

impl Editor {
    // Elements
    pub(super) fn element(&self, id: &crate::api::value::ElementId) -> Result<(ElementId, Where), Refused> {
        let e = element_id(id)?;
        let w = self.locate(e).ok_or_else(no_element)?;
        Ok((e, w))
    }

    /// A picture control's picture size (inline or floating), in points.
    pub fn picture_control_size(&self, c: ControlId) -> Option<crate::api::value::Size> {
        use crate::api::value::{Points, Size};
        if self.doc.controls.get(&c)?.props.kind != ControlKind::Picture {
            return None;
        }
        let Some(Where::Inline { para, first, last, .. }) = self.locate(ElementId::Control(c)) else { return None };
        let p = self.index.para(&self.doc, para)?;
        p.inlines.get(first..last)?.iter().find_map(|i| match i {
            Inline::Picture(pic, _) => Some(Size { width: Points(pic.w), height: Points(pic.h) }),
            Inline::Float(f, _) if f.image.is_some() => Some(Size { width: Points(f.w), height: Points(f.h) }),
            _ => None,
        })
    }

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
                // Row-level control
                let rows: Vec<(crate::layout::TableId, usize)> =
                    locs.iter().filter_map(|l| if let Loc::Row(t, r) = l { Some((*t, *r)) } else { None }).collect();
                if let Some(&(table, _)) = rows.first() {
                    let at: Vec<usize> = rows.iter().filter(|(t, _)| *t == table).map(|(_, r)| *r).collect();
                    let (lo, hi) = (*at.iter().min()?, *at.iter().max()?);
                    return Some(Where::Rows { table, range: lo..hi + 1 });
                }
                // Block-level control
                let slots: Vec<super::index::Slot> = locs
                    .iter()
                    .filter_map(|l| match l {
                        Loc::Para(p) => self.index.para_slot(*p),
                        Loc::Table(t) => self.index.table_slot(*t),
                        _ => None,
                    })
                    .collect();
                let container = slots.first()?.container;
                let at: Vec<usize> = slots.iter().filter(|s| s.container == container).map(|s| s.block).collect();
                let (lo, hi) = (*at.iter().min()?, *at.iter().max()?);
                Some(Where::Blocks { container, range: lo..hi + 1 })
            }
        }
    }

    fn editable_target(&self) -> Result<(), Refused> {
        match self.selection.focus() {
            Some(p) if !self.editable(&p) => Err(locked()),
            Some(_) => Ok(()),
            None if matches!(self.selection, Selection::Cells { .. }) => Ok(()),
            None => Err(Error::unavailable("no place to insert")),
        }
    }

    pub fn hit_test(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(Position, Option<ElementId>, bool)> {
        let (p, repeat) = self.hit_full(x, y, fonts)?;
        let element = elements::context(&self.doc, &self.index, &Selection::Caret(p)).first().map(|(e, _)| *e);
        Some((p, element, !repeat && self.editable(&p)))
    }

    pub fn set_drop(&mut self, at: Option<(f64, f64)>, fonts: &Fonts) {
        self.drop = at.and_then(|(x, y)| self.hit_test(x, y, fonts)).filter(|h| h.2).map(|h| h.0);
    }

    pub(super) fn editable(&self, p: &Position) -> bool {
        if self.doc.controls.values().any(|k| matches!(k.props.kind, ControlKind::Group)) && self.in_group(p, p, false) {
            return false;
        }
        elements::context(&self.doc, &self.index, &Selection::Caret(*p)).iter().all(|(e, _)| {
            let ElementId::Control(c) = e else { return true };
            if self.doc.controls.get(c).is_none_or(|k| k.props.lock.editable()) {
                return true;
            }
            matches!(self.locate(*e), Some(Where::Inline { para, end, .. }) if p.para_offset() == Some((para, end)))
        })
    }

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

    // Element marks
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
        self.hover_cache = None;
        if m.clear.unwrap_or(false) {
            self.marks.clear();
        }
        self.marks.retain(|m| !new.iter().any(|n| n.id == m.id));
        self.marks.extend(new);
        Outcome::Moved
    }

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
                        && rect.contains(hx, hy)
                    {
                        tip = Some(super::view::Tooltip { text: m.message.clone(), page, rect: rect.page() });
                    }
                    out.push(super::view::MarkView { page, rect, severity: m.severity });
                }
            }
            kept.push(m);
        }
        self.marks = kept;
        (out, tip)
    }

    /// The hover tooltip in the kinds the API names (a note reference's has none yet).
    pub fn hover_tooltip(&mut self) -> Option<(crate::api::services::TooltipKind, super::view::Tooltip)> {
        use super::hover::HoverTip as H;
        use crate::api::services::TooltipKind as K;
        let (tip, t, _) = self.hover_tip()?;
        let (tip, t) = (*tip, t.clone());
        let kind = match tip {
            H::Mark => K::Mark,
            H::Link => K::Link,
            H::Revision => K::Revision,
            H::Comment => K::Comment,
            H::Note | H::WhiteSpace => return None,
        };
        Some((kind, t))
    }

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

    // Find and replace
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

    pub(super) fn replace_in_code(&mut self, tx: &mut super::Tx, f: FieldId, find: &str, replace: &str) -> bool {
        let Some(Where::Inline { para, first, last, .. }) = self.locate(ElementId::Field(f)) else { return false };
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return false };
        let Some(s) = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last) else { return false };
        // The code's own text, run by run (fields nested in it are theirs), in groups of runs next to each other: a
        // match can span runs of different formatting.
        let mut groups: Vec<Vec<usize>> = vec![Vec::new()];
        let mut depth = 0;
        for k in first + 1..s.code_end() {
            match &p.inlines[k] {
                Inline::FieldBegin { .. } => depth += 1,
                Inline::FieldEnd { .. } => depth -= 1,
                Inline::Text(..) if depth == 0 => {
                    groups.last_mut().expect("a group").push(k);
                    continue;
                }
                i if i.units() == 0 && depth == 0 => continue,
                _ => {}
            }
            if groups.last().is_some_and(|g| !g.is_empty()) {
                groups.push(Vec::new());
            }
        }
        let mut own: Vec<(usize, String)> = Vec::new();
        for g in groups.iter().filter(|g| !g.is_empty()) {
            let parts: Vec<&str> = g.iter().map(|&k| if let Inline::Text(t, _) = &p.inlines[k] { t.as_str() } else { "" }).collect();
            let all: String = parts.concat();
            let found: Vec<usize> = all.match_indices(find).map(|(at, _)| at).collect();
            if found.is_empty() {
                continue;
            }
            // Each run keeps its text outside the matches; a match's replacement goes in the run it starts in. The
            // matches a run holds part of start at `first`: those before it end before the run does.
            let mut at = 0;
            let mut first = 0;
            for (&k, part) in g.iter().zip(&parts) {
                let (a, b) = (at, at + part.len());
                at = b;
                let mut text = String::new();
                let mut c = a;
                while found.get(first).is_some_and(|&m| m + find.len() <= a) {
                    first += 1;
                }
                for &m in found[first..].iter().take_while(|&&m| m < b) {
                    let e = m + find.len();
                    if e <= a {
                        continue;
                    }
                    if m > c {
                        text.push_str(&all[c..m]);
                    }
                    if m >= a {
                        text.push_str(replace);
                    }
                    c = e.min(b).max(c);
                }
                text.push_str(&all[c.min(b)..b]);
                if text != *part {
                    own.push((k, text));
                }
            }
        }
        // Runs next to each other change in one step, the paragraph as it was: the steps go last to first.
        let mut spans: Vec<(std::ops::Range<u32>, Vec<Inline>)> = Vec::new();
        let (mut at, mut from) = (0u32, 0usize);
        let mut next = usize::MAX;
        for (k, t) in own {
            let Inline::Text(_, run) = &p.inlines[k] else { continue };
            at += units_before(&p.inlines[from..], k - from);
            from = k;
            let end = at + p.inlines[k].units() as u32;
            let with = (!t.is_empty()).then(|| Inline::Text(t, run.clone()));
            let adjacent = k == next;
            next = k + 1;
            match spans.last_mut() {
                Some((r, w)) if adjacent => {
                    r.end = end;
                    w.extend(with);
                }
                _ => spans.push((at..end, with.into_iter().collect())),
            }
        }
        let mut any = false;
        for (r, with) in spans.into_iter().rev() {
            any |= self.step(tx, Step::Text { para, range: Unit(r.start)..Unit(r.end), with, edges: Default::default() }).is_ok();
        }
        if any {
            self.refresh_field_kinds(tx, para);
        }
        any
    }

    /// Field `f`'s code with the argument `pick` names (from the code as parsed) set to `value`, written as an argument
    /// is; the rest of the code as it was.
    pub(super) fn set_code_arg(
        &mut self,
        tx: &mut super::Tx,
        f: FieldId,
        pick: impl Fn(&crate::docx::fields::FieldCode) -> Option<usize>,
        value: &str,
    ) -> bool {
        let Some(Where::Inline { para, first, last, .. }) = self.locate(ElementId::Field(f)) else { return false };
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return false };
        let Some(s) = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last) else { return false };
        // The code's own pieces: each inline and where its text starts
        let mut code = String::new();
        let mut pieces: Vec<(usize, usize)> = Vec::new();
        let mut depth = 0;
        for k in first + 1..s.code_end() {
            match &p.inlines[k] {
                Inline::FieldBegin { .. } => depth += 1,
                Inline::FieldEnd { .. } => depth -= 1,
                Inline::Text(t, _) if depth == 0 => {
                    pieces.push((k, code.len()));
                    code.push_str(t);
                }
                Inline::Tab(_) | Inline::PTab(..) if depth == 0 => {
                    pieces.push((k, code.len()));
                    code.push('\t');
                }
                _ => {}
            }
        }
        let mut spans = Vec::new();
        let parsed = crate::docx::fields::parse_spans(&code, Some(&mut spans));
        let Some(span) = pick(&parsed).and_then(|i| spans.get(i).cloned()) else { return false };
        let ends: Vec<usize> = pieces.iter().skip(1).map(|p| p.1).chain([code.len()]).collect();
        let over: Vec<(usize, usize, usize)> =
            pieces.iter().zip(ends).filter(|&(&(_, at), end)| at < span.end && end > span.start).map(|(&(k, at), end)| (k, at, end)).collect();
        // A field nested in the argument would be left outside the value written over it.
        if let (Some(a), Some(b)) = (over.first(), over.last())
            && p.inlines[a.0..b.0].iter().any(|i| matches!(i, Inline::FieldBegin { .. }))
        {
            return false;
        }
        let mut any = false;
        for (j, &(k, at, end)) in over.iter().enumerate().rev() {
            let mut t = code[at..span.start.max(at)].to_string();
            if j == 0 {
                t.push_str(value);
            }
            t.push_str(&code[span.end.min(end)..end]);
            let start = units_before(&p.inlines, k);
            let range = Unit(start)..Unit(start + p.inlines[k].units() as u32);
            let with = match (&p.inlines[k], t.is_empty()) {
                (_, true) => Vec::new(),
                (Inline::Text(_, run), false) => vec![Inline::Text(t, run.clone())],
                (other, false) => vec![other.clone()],
            };
            any |= self.step(tx, Step::Text { para, range, with, edges: Default::default() }).is_ok();
        }
        if any {
            self.refresh_field_kinds(tx, para);
        }
        any
    }

    /// Control `c` and, for a repeating section, its own items (those right inside it: a nested section's stay).
    fn with_items<'l>(&self, c: ControlId, lists: impl Iterator<Item = &'l Vec<ControlId>>) -> Vec<ControlId> {
        let mut gone = vec![c];
        if self.doc.controls.get(&c).is_some_and(|k| k.props.kind == ControlKind::RepeatingSection) {
            for list in lists {
                let item = list.iter().position(|x| *x == c).and_then(|k| list.get(k + 1));
                if let Some(x) = item
                    && self.doc.controls.get(x).is_some_and(|k| k.props.kind == ControlKind::RepeatingItem)
                    && !gone.contains(x)
                {
                    gone.push(*x);
                }
            }
        }
        gone
    }

    // Repeating sections
    pub(super) fn wrap_in_repeating_section(&mut self, r: RepeatingSection, fonts: &Fonts) -> Outcome {
        outcome(self.region_wrap(r, fonts))
    }

    fn region_wrap(&mut self, r: RepeatingSection, fonts: &Fonts) -> Result<Outcome, Refused> {
        self.editable_target()?;
        let section = ControlProps { kind: ControlKind::RepeatingSection, tag: r.tag, alias: r.title, ..Default::default() };
        let item = || Control::new(ControlProps { kind: ControlKind::RepeatingItem, ..Default::default() });
        let s = ControlId::next();
        let made = CommandResult::Element { id: api_id(ElementId::Control(s)) };
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
                _ => Ok(None),
            }
        };
        let mut tx = self.begin("Insert Repeating Section");
        let r = rows(self)?;
        if let Some((table, r0, r1)) = r {
            let t = self.index.table(&self.doc, table).ok_or_else(no_element)?;
            let lists: Vec<&Vec<ControlId>> = t.rows[r0..=r1].iter().map(|r| &r.controls).collect();
            let outside: Vec<&Vec<ControlId>> = t.rows.iter().enumerate().filter(|(k, _)| *k < r0 || *k > r1).map(|(_, r)| &r.controls).collect();
            let at = nest(&lists, &outside)?;
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
        let outside: Vec<&Vec<ControlId>> =
            blocks.iter().enumerate().filter(|(k, _)| *k < sa.block || *k > last).map(|(_, b)| block_controls(b)).collect();
        let at = nest(&lists, &outside)?;
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

    /// The items of repeating section `s`, in order: each item's id and the blocks (or rows) of `lists` it spans.
    fn section_items<'l>(&self, s: ControlId, lists: impl Iterator<Item = &'l Vec<ControlId>>) -> Vec<(ControlId, std::ops::Range<usize>)> {
        let mut out: Vec<(ControlId, std::ops::Range<usize>)> = Vec::new();
        for (k, list) in lists.enumerate() {
            let Some(i) = list.iter().position(|x| *x == s).and_then(|at| list.get(at + 1)) else { continue };
            match out.last_mut() {
                Some((last, r)) if last == i && r.end == k => r.end = k + 1,
                _ => out.push((*i, k..k + 1)),
            }
        }
        out
    }

    /// Adds an item to repeating section `section`: a copy of its item `item` (by default its last), content and
    /// all, as Word's + button and Insert Item Before / After do; before that item when `before`, else after it. The
    /// copy's controls, fields, bookmarks and notes are its own. Refused when the section doesn't allow adding items,
    /// its content is locked or it is bound to XML data. One undo step; the selection goes to the new item's start.
    pub fn add_repeating_item(
        &mut self,
        section: &crate::api::value::ElementId,
        item: Option<&crate::api::value::ElementId>,
        before: bool,
        fonts: &Fonts,
    ) -> Outcome {
        outcome(self.repeating_item_add(section, item, before, fonts))
    }

    fn repeating_item_add(
        &mut self,
        section: &crate::api::value::ElementId,
        item: Option<&crate::api::value::ElementId>,
        before: bool,
        fonts: &Fonts,
    ) -> Result<Outcome, Refused> {
        let (e, w) = self.element(section)?;
        let ElementId::Control(s) = e else { return Err(bad("id")) };
        let props = self.doc.controls.get(&s).map(|c| c.props.clone()).ok_or_else(no_element)?;
        if props.kind != ControlKind::RepeatingSection {
            return Err(bad("id"));
        }
        if props.kind_xml.as_deref().is_some_and(|x| x.contains("doNotAllowInsertDeleteSection")) || !props.lock.editable() {
            return Err(locked());
        }
        if props.binding.is_some() || props.rest.iter().any(|(n, _)| n == "dataBinding") {
            return Err(shape("bound repeating section"));
        }
        let want = match item {
            Some(id) => match self.element(id)?.0 {
                ElementId::Control(i) => Some(i),
                ElementId::Field(_) => return Err(bad("item")),
            },
            None => None,
        };
        let pick = |items: &[(ControlId, std::ops::Range<usize>)]| -> Result<(ControlId, std::ops::Range<usize>), Refused> {
            match want {
                Some(i) => items.iter().find(|(x, _)| *x == i).cloned().ok_or_else(|| bad("item")),
                None => items.last().cloned().ok_or_else(no_element),
            }
        };
        let new_item = ControlId::next();
        let item_props = |e: &Editor, i: ControlId| {
            let mut p =
                e.doc.controls.get(&i).map(|c| c.props.clone()).unwrap_or(ControlProps { kind: ControlKind::RepeatingItem, ..Default::default() });
            p.rest.retain(|(n, _)| n != "id");
            Control::new(p)
        };
        // The controls each copied list keeps: those around the section, the section, then the new item
        let lead = |list: &[ControlId]| -> Vec<ControlId> {
            let at = list.iter().position(|x| *x == s).map_or(0, |k| k + 1);
            let mut v = list[..at].to_vec();
            v.push(new_item);
            v
        };
        let inner = |list: &[ControlId]| -> Vec<ControlId> {
            list.iter().position(|x| *x == s).map_or_else(Vec::new, |k| list.get(k + 2..).unwrap_or(&[]).to_vec())
        };
        match w {
            Where::Blocks { container, range } => {
                let blocks = self.index.blocks(&self.doc, container).ok_or_else(no_element)?;
                let items = self.section_items(s, blocks[range.clone()].iter().map(block_controls));
                let (src, r) = pick(&items)?;
                let (r, leads): (std::ops::Range<usize>, Vec<Vec<ControlId>>) = (
                    range.start + r.start..range.start + r.end,
                    blocks[range.start + r.start..range.start + r.end].iter().map(|b| lead(block_controls(b))).collect(),
                );
                let mut copied: Vec<Block> = blocks[r.clone()].to_vec();
                for b in &mut copied {
                    let rest = inner(block_controls(b));
                    match b {
                        Block::Para(p) => p.controls = rest,
                        Block::Table(t) => t.controls = rest,
                    }
                }
                let f = self.whole_fragment(copied);
                self.pending_controls.push((new_item, item_props(self, src)));
                let mut tx = self.begin("Insert Item");
                let mut with: Vec<Block> = f.blocks.iter().map(super::clipboard::fresh).collect();
                self.take_in(&mut tx, &mut with, &f, &mut super::clipboard::Taken::default());
                for (b, l) in with.iter_mut().zip(leads) {
                    match b {
                        Block::Para(p) => p.controls.splice(0..0, l),
                        Block::Table(t) => t.controls.splice(0..0, l),
                    };
                }
                let first = with.iter().find_map(super::first_para_of);
                let at = if before { r.start } else { r.end };
                if self.step(&mut tx, Step::Blocks { container, range: at..at, with }).is_err() {
                    self.abort(tx);
                    return Err(no_element());
                }
                if let Some(p) = first {
                    self.selection = Selection::Caret(Position::text(self.index.story(container), p, Unit(0)));
                }
                self.made = Some(CommandResult::Element { id: api_id(ElementId::Control(new_item)) });
                Ok(self.commit(tx, None, fonts))
            }
            Where::Rows { table, range } => {
                let t = self.index.table(&self.doc, table).ok_or_else(no_element)?;
                let items = self.section_items(s, t.rows[range.clone()].iter().map(|r| &r.controls));
                let (src, r) = pick(&items)?;
                let r = range.start + r.start..range.start + r.end;
                let leads: Vec<Vec<ControlId>> = t.rows[r.clone()].iter().map(|row| lead(&row.controls)).collect();
                let mut shell = t.clone();
                shell.controls.clear();
                shell.rows = t.rows[r.clone()].to_vec();
                for row in &mut shell.rows {
                    row.controls = inner(&row.controls);
                }
                let f = self.whole_fragment(vec![Block::Table(shell)]);
                self.pending_controls.push((new_item, item_props(self, src)));
                let mut tx = self.begin("Insert Item");
                let mut with: Vec<Block> = f.blocks.iter().map(super::clipboard::fresh).collect();
                self.take_in(&mut tx, &mut with, &f, &mut super::clipboard::Taken::default());
                let Some(Block::Table(copy)) = with.pop() else {
                    self.abort(tx);
                    return Err(no_element());
                };
                let mut rows = copy.rows;
                for (row, l) in rows.iter_mut().zip(leads) {
                    row.controls.splice(0..0, l);
                }
                let first = rows.first().and_then(|row| row.cells.first()).and_then(|c| c.blocks.iter().find_map(super::first_para_of));
                let at = if before { r.start } else { r.end };
                if self.step(&mut tx, Step::Rows { table, range: at..at, with: rows, grid: None }).is_err() {
                    self.abort(tx);
                    return Err(no_element());
                }
                tx.stale_tables.push(table);
                if let Some(p) = first
                    && let Some(slot) = self.index.para_slot(p)
                {
                    self.selection = Selection::Caret(Position::text(self.index.story(slot.container), p, Unit(0)));
                }
                self.made = Some(CommandResult::Element { id: api_id(ElementId::Control(new_item)) });
                Ok(self.commit(tx, None, fonts))
            }
            _ => Err(shape("inline repeating section")),
        }
    }

    /// The labelled regions shown: those the selection is in and the one under the pointer. None while read-only or
    /// with `controlLabels` off.
    pub(super) fn region_views(&mut self, pages: std::ops::Range<usize>) -> Vec<super::view::RegionView> {
        if self.mode == super::Mode::ReadOnly || !self.control_labels {
            return Vec::new();
        }
        let mut ids = self.regions_at(&self.selection.clone());
        if let Some(c) = self.hover_region.filter(|c| !ids.contains(c)) {
            ids.push(c);
        }
        ids.into_iter().filter_map(|c| self.region_view(c, pages.clone())).collect()
    }

    /// The regions a selection is in, innermost first: every repeating section, the innermost other control, and a
    /// shown sub-template. A control whose `w15:appearance` is `hidden` has none, and neither do repeating items.
    fn regions_at(&self, sel: &Selection) -> Vec<ElementId> {
        let mut inner = false;
        let mut take = |x: ElementId| match x {
            ElementId::Control(c) => {
                let props = &self.doc.controls.get(&c)?.props;
                let hidden = props.rest.iter().any(|(n, xml)| n == "appearance" && xml.contains("\"hidden\""));
                match props.kind {
                    ControlKind::RepeatingSection => Some(x),
                    ControlKind::RepeatingItem | ControlKind::Other => None,
                    _ if hidden || inner => None,
                    _ => {
                        inner = true;
                        Some(x)
                    }
                }
            }
            ElementId::Field(f) => self.reference_expanded(f).then_some(x).filter(|_| self.references.by_field.contains_key(&f)),
        };
        // A selected picture is its picture control's.
        let picture = match sel {
            Selection::Object(o) => self.picture_control_at(*o).map(ElementId::Control),
            _ => None,
        };
        let context = elements::context(&self.doc, &self.index, sel).into_iter().map(|(x, _)| x);
        picture.into_iter().chain(context).filter_map(&mut take).collect()
    }

    pub(super) fn set_hover(&mut self, x: f64, y: f64) {
        self.hover = Some((x, y));
        self.hover_generation = self.generation;
        if self.mode == super::Mode::ReadOnly {
            self.hover_region = None;
            return;
        }
        let shown = self.expansion_at(x, y).map(|(f, _)| ElementId::Field(f));
        let under = shown.or_else(|| self.hit_laid_out(x, y).and_then(|p| self.regions_at(&Selection::Caret(p)).first().copied()));
        let on_tab = |e: &mut Editor, c: ElementId| {
            let Some((page, px, py)) = e.stack().page_at(x, y) else { return false };
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

    /// Which region the pointer is over, so that a move onto or off one draws a frame.
    pub fn hover_region(&self) -> Option<ElementId> {
        self.hover_region
    }

    fn region_view(&mut self, x: ElementId, pages: std::ops::Range<usize>) -> Option<super::view::RegionView> {
        let mut rects: Vec<(usize, [f64; 4])> = Vec::new();
        let mut grow = |page: usize, r: [f64; 4]| match rects.iter_mut().find(|(p, _)| *p == page) {
            Some((_, u)) => *u = [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[2]), u[3].max(r[3])],
            None => rects.push((page, r)),
        };
        let title = match x {
            ElementId::Field(f) => {
                for (page, r) in self.expansion_rects(f, pages) {
                    grow(page, r);
                }
                self.references.by_field.get(&f)?.name.clone()
            }
            ElementId::Control(c) => {
                match self.locate(x)? {
                    Where::Rows { table, range } => {
                        for page in pages {
                            let Some(geo) = self.pages.page(page, &self.index) else { continue };
                            for cell in &geo.cells {
                                if let Some(&(t, row, _)) = cell.chain.last()
                                    && t == table
                                    && range.contains(&row)
                                {
                                    grow(page, cell.page_rect());
                                }
                            }
                        }
                    }
                    w => {
                        let (a, b) = self.element_range(&w)?;
                        for (page, r) in self.range_rects(a, b, pages) {
                            grow(page, r.page());
                        }
                    }
                }
                let props = self.doc.controls.get(&c).map(|k| k.props.clone()).unwrap_or_default();
                let fallback = (props.kind == ControlKind::RepeatingSection).then(|| "Repeating section".to_string());
                props.alias.or(props.tag).or(fallback).unwrap_or_default()
            }
        };
        let &(page, first) = rects.first()?;
        // An untitled control's tab is a bare handle, as Word draws it.
        let width = 6.0 * title.chars().count() as f64 + 10.0;
        Some(super::view::RegionView {
            id: elements::id_str(x),
            rects,
            title_rect: (page, [first[0], first[1] - 14.0, first[0] + width, first[1]]),
            title,
        })
    }

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
            None if inside => match self.fields_here(None, false).first() {
                Some((f, _)) => ElementId::Field(*f),
                None => return false,
            },
            None => return false,
        };
        self.events.push(super::input::Notice::Activate(id));
        true
    }

    // Control fill
    pub(super) fn fill_control(&mut self, f: FillControl, fonts: &Fonts) -> Outcome {
        self.begin_batch(None);
        self.picture_writes = true;
        let r = self.control_fill(f, fonts);
        self.picture_writes = false;
        let out = self.end_batch(r.is_ok(), fonts);
        outcome(r.map(|_| out))
    }

    /// Fills date control `id` with the date `date` (ISO 8601: `YYYY-MM-DD`, with a time and zone or without): its
    /// text that date in the control's format, its full date that date, so a format without the day or year keeps
    /// it. One undo step.
    pub fn fill_control_date(&mut self, id: &crate::api::value::ElementId, date: &str, fonts: &Fonts) -> Outcome {
        self.begin_batch(Some("Fill Content Control".into()));
        let r = self.control_date_fill(id, date, fonts);
        let out = self.end_batch(r.is_ok(), fonts);
        outcome(r.map(|_| out))
    }

    fn control_date_fill(&mut self, id: &crate::api::value::ElementId, date: &str, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (e, _) = self.element(id)?;
        let ElementId::Control(c) = e else { return Err(bad("id")) };
        let control = self.doc.controls.get(&c).cloned().ok_or_else(no_element)?;
        if !matches!(control.props.kind, ControlKind::Date { .. }) {
            return Err(bad("id"));
        }
        if !control.props.lock.editable() {
            return Err(locked());
        }
        let secs = crate::docx::review::Timestamp::parse(date.trim()).utc.ok_or_else(|| bad("date"))?;
        let full = crate::docx::review::Timestamp::from_utc(secs).raw.to_string();
        let text = crate::docx::binding::shown(&control.props, &full);
        if let Some(xml) = crate::docx::binding::with_full_date_value(&control.props, Some(full), &self.cx().w) {
            let mut control = control;
            control.props.kind_xml = Some(xml);
            let mut tx = self.begin("Fill Content Control");
            if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_err() {
                self.abort(tx);
                return Err(no_element());
            }
            self.commit(tx, None, fonts);
        }
        self.control_fill(FillControl { id: id.clone(), text: Some(text), image: None, docx: None, building_block: None }, fonts)
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
            let image = p.inlines[first..last].iter().find_map(super::picture_controls::picture_image).ok_or_else(|| bad("id"))?;
            if self.image_shown_elsewhere(image, para, first..last) {
                self.fill_control_copy(c, para, first..last, image, data, size, label, fonts)
            } else {
                self.fill_images(&[image], data, size, label, false, fonts)
            }
        } else {
            let given = f.text.clone().unwrap_or_default();
            let gallery = control.props.gallery();
            let frag = match (&f.building_block, &f.docx, &f.text) {
                (Some(name), ..) => Some(self.building_block_fragment(name, &control.props)?),
                (None, Some(d), _) => Some(self.fragment_of(&d.0)?),
                (None, None, Some(_)) => None,
                (None, None, None) => return Err(bad("text")),
            };
            let plain = matches!(control.props.kind, ControlKind::PlainText { .. } | ControlKind::Date { .. });
            if !plain && control.props.kind != ControlKind::RichText && gallery.is_none() {
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
                (Where::Inline { para, start, end, first, last, .. }, f) => {
                    let text = match f {
                        Some(f) => f.text(),
                        None => given.clone(),
                    };
                    if plain && text.contains('\n') && !matches!(control.props.kind, ControlKind::PlainText { multi_line: true }) {
                        return Err(bad("text"));
                    }
                    let own = control.props.showing_placeholder.then(|| self.control_own_run(c, *para, *first)).flatten();
                    let p = self.index.para(&self.doc, *para).ok_or_else(no_element)?;
                    let run = own.or_else(|| p.inlines[*first..].iter().find_map(|i| i.run().cloned())).ok_or_else(no_element)?;
                    // Empty control
                    let next = p.inlines.iter().skip(*last).position(|i| i.units() > 0).map_or(p.inlines.len(), |k| last + k);
                    let under =
                        if start == end { p.inlines[*last..next].iter().filter(|i| !matches!(i, Inline::Text(..))).count() as u32 } else { 0 };
                    let edges = super::step::Edges { under, ..Default::default() };
                    let mut tx = self.begin(label);
                    let with = crate::docx::binding::text_inlines(&crate::docx::xml::legal(&text), &run);
                    if self.step(&mut tx, Step::Text { para: *para, range: *start..*end, with, edges }).is_err() {
                        self.abort(tx);
                        return Err(no_element());
                    }
                    self.follow(&tx);
                    self.commit(tx, None, fonts)
                }
                (Where::Blocks { container, range }, f) if !plain => {
                    let blocks = self.index.blocks(&self.doc, *container).ok_or_else(no_element)?;
                    // The control and the ones around it
                    let lead = match &blocks[range.start] {
                        Block::Para(p) => &p.controls,
                        Block::Table(t) => &t.controls,
                    };
                    let lead: Vec<ControlId> = lead[..lead.iter().position(|x| *x == c).map_or(lead.len(), |k| k + 1)].to_vec();
                    let mut tx = self.begin(label);
                    let mut new: Vec<Block> = match f {
                        Some(f) => {
                            let mut b: Vec<Block> = f.blocks.iter().map(super::clipboard::fresh).collect();
                            self.take_in(&mut tx, &mut b, &f, &mut super::clipboard::Taken::default());
                            b
                        }
                        None => {
                            let blocks = self.index.blocks(&self.doc, *container).ok_or_else(no_element)?;
                            let first = blocks[range.clone()].iter().find_map(super::first_para_of).ok_or_else(no_element)?;
                            let mut p = self.split_template(first).ok_or_else(no_element)?;
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
        if let Some(mut k) = self.doc.controls.get(&c).cloned().filter(|k| k.props.showing_placeholder) {
            k.props.showing_placeholder = false;
            let mut tx = self.begin(label);
            let _ = self.step(&mut tx, Step::Control { id: c, control: Some(k) });
            self.commit(tx, None, fonts);
        }
        Ok(out)
    }

    /// The live building blocks a gallery control of `gallery` and `category` (each `None`: any) offers.
    pub fn gallery_entries<'a>(
        &'a self,
        gallery: Option<&'a str>,
        category: Option<&'a str>,
    ) -> impl Iterator<Item = &'a crate::docx::BuildingBlock> + 'a {
        self.doc.glossary.iter().flat_map(|g| g.parts.iter()).filter(move |b| {
            b.state != crate::docx::PartState::Removed
                && b.editable
                && category.is_none_or(|c| b.category == c)
                && gallery.is_none_or(|g| b.gallery.named(g))
        })
    }

    /// A building block's body, to fill gallery control `props` with.
    fn building_block_fragment(&self, name: &str, props: &ControlProps) -> Result<super::clipboard::Fragment, Refused> {
        let Some(crate::docx::KeptKind::Gallery { gallery, category }) = props.kept_kind() else { return Err(bad("id")) };
        let part = self.gallery_entries(gallery.as_deref(), category.as_deref()).find(|b| b.name == name).ok_or_else(|| bad("buildingBlock"))?;
        let mut blocks = part.body.clone();
        if blocks.len() > 1 && matches!(blocks.last(), Some(Block::Para(p)) if super::step::para_len(p) == Unit(0)) {
            blocks.pop();
        }
        let controls = super::clipboard::controls_of(&blocks, &self.doc);
        Ok(super::clipboard::Fragment { id: 0, blocks, open_end: true, controls, ..Default::default() })
    }

    fn fragment_of(&self, bytes: &[u8]) -> Result<super::clipboard::Fragment, Refused> {
        let doc = crate::docx::read_editable(bytes).map_err(|_| bad("docx"))?;
        let mut blocks: Vec<Block> = doc.sections.iter().flat_map(|s| s.blocks.iter().cloned()).collect();
        if blocks.len() > 1 && matches!(blocks.last(), Some(Block::Para(p)) if super::step::para_len(p) == Unit(0)) {
            blocks.pop();
        }
        let refs = super::clipboard::refs_of(&doc, &blocks);
        let mut controls = super::clipboard::controls_of(&blocks, &doc);
        for body in refs.notes.iter().map(|n| &n.2).chain(refs.comments.iter().map(|c| &c.body)) {
            controls.extend(super::clipboard::controls_of(body, &doc));
        }
        let mut f = super::clipboard::Fragment { id: 0, blocks, open_end: true, controls, refs, ..Default::default() };
        f.origin = Some(super::import::Origin::of_document(doc, &mut f));
        Ok(f)
    }

    // Locks
    pub(super) fn locked_edit(&self, a: &Position, b: &Position) -> bool {
        self.locks(a, b, true)
    }

    fn locks(&self, a: &Position, b: &Position, deletes: bool) -> bool {
        if self.doc.controls.values().any(|k| matches!(k.props.kind, ControlKind::Group)) && self.in_group(a, b, deletes) {
            return true;
        }
        let ends = |p: ParaId| self.index.para(&self.doc, p).map_or(&[][..], |q| &q.controls[..]);
        let (pa, pb) = (a.para_offset().map(|(p, _)| p), b.para_offset().map(|(p, _)| p));
        // `whole`: a paragraph between the ends, deleted with its block controls unless one also holds an end.
        let check_with = |para: ParaId, x: u32, y: u32, whole: bool| -> bool {
            let Some(p) = self.index.para(&self.doc, para) else { return false };
            let held = |c: &ControlId| [pa, pb].into_iter().flatten().any(|e| ends(e).contains(c));
            let refused = |c: &ControlId, lock: crate::docx::Lock| if deletes && whole && !held(c) { !lock.deletable() } else { !lock.editable() };
            if p.controls.iter().any(|c| self.doc.controls.get(c).is_some_and(|k| refused(c, k.props.lock))) {
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
                                } else if deletes && x <= start && end <= y {
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
        let check = |para: ParaId, x: u32, y: u32| check_with(para, x, y, false);
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return false };
        if pa == pb {
            return check(pa, ua.0.min(ub.0), ua.0.max(ub.0));
        }
        let len = |p: ParaId| self.index.para(&self.doc, p).map_or(0, |q| super::step::para_len(q).0);
        // The paragraphs between the ends go whole.
        let inside = |p: ParaId| {
            let at = self.index.para_slot(p).map(|s| Position::text(self.index.story(s.container), p, Unit(0)));
            p != pa && p != pb && at.is_some_and(|at| self.index.cmp(a, &at).is_lt() && self.index.cmp(&at, b).is_lt())
        };
        check(pa, ua.0, len(pa).max(ua.0 + 1))
            || check(pb, 0, ub.0)
            || self.index.para_ids().filter(|&p| inside(p)).any(|p| check_with(p, 0, len(p), true))
    }

    /// Whether `a..b` edits a group control's own content, which only the controls nested in it let change: a group
    /// around an end, unless both ends are in the same control nested in it, or the edit deletes (`deletes`) the
    /// group whole.
    pub(super) fn in_group(&self, a: &Position, b: &Position, deletes: bool) -> bool {
        let collapsed = a == b;
        let is_group = |c: &ControlId| self.doc.controls.get(c).is_some_and(|k| matches!(k.props.kind, ControlKind::Group));
        // The controls around `p`, innermost first, past the inline ones a caret is at the end of
        let around = |p: &Position| -> Vec<(ControlId, Option<(Position, Position)>)> {
            elements::context(&self.doc, &self.index, &Selection::Caret(*p))
                .into_iter()
                .filter_map(|(e, _)| {
                    let ElementId::Control(c) = e else { return None };
                    let at = self.locate(e);
                    let range = at.as_ref().and_then(|w| self.element_range(w));
                    let after = collapsed && matches!(at, Some(Where::Inline { .. })) && range.is_some_and(|(_, end)| end == *p);
                    (!after).then_some((c, range))
                })
                .collect()
        };
        let whole = |r: &Option<(Position, Position)>| {
            deletes && !collapsed && r.is_some_and(|(s, e)| self.index.cmp(a, &s).is_le() && self.index.cmp(&e, b).is_le())
        };
        let at_a = around(a);
        let at_b = if collapsed { Vec::new() } else { around(b) };
        let inner_b = if collapsed { at_a.first() } else { at_b.first() };
        let nested = matches!((at_a.first(), inner_b), (Some((x, _)), Some((y, _))) if x == y && !is_group(x));
        !nested && at_a.iter().chain(&at_b).any(|(c, r)| is_group(c) && !whole(r))
    }

    pub(super) fn refuse_locked(&mut self) -> bool {
        self.refuse_lock(true)
    }

    /// [`Editor::refuse_locked`] for an edit that changes content in place and deletes nothing (formatting, case):
    /// only a lock on the content refuses it.
    pub(super) fn refuse_locked_content(&mut self) -> bool {
        self.refuse_lock(false)
    }

    /// [`Editor::refuse_expanded`] and [`Editor::refuse_locked`] for an edit that replaces the selection (typing,
    /// paste, a break or an insert): of a discontiguous selection it replaces only the selection's own range and
    /// deselects the others, so only that range is checked.
    pub(super) fn refuse_replacing(&mut self) -> bool {
        let multi = std::mem::take(&mut self.multi);
        let refused = self.refuse_expanded() || self.refuse_locked();
        if refused {
            self.multi = multi;
        }
        refused
    }

    /// Whether a lock refuses the edit of selection `s` (cells, a block, an object, a discontiguous selection): each
    /// paragraph piece it covers.
    pub(super) fn pieces_lock(&mut self, s: super::selection::Selection, deletes: bool) -> bool {
        let was = std::mem::replace(&mut self.selection, s);
        let pieces = self.selected_ranges();
        self.selection = was;
        pieces.into_iter().any(|(para, x, y)| {
            let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
            self.locks(&Position::text(story, para, x), &Position::text(story, para, y), deletes)
        })
    }

    fn refuse_lock(&mut self, deletes: bool) -> bool {
        let locked = match self.selection.ordered(&self.index) {
            Some((a, b)) => {
                self.locks(&a, &b, deletes)
                    || self.other_ranges().iter().any(|(a, b)| self.locks(a, b, deletes))
                    || self.other_cells().to_vec().into_iter().any(|c| self.pieces_lock(c, deletes))
            }
            None => self.pieces_lock(self.selection, deletes),
        };
        self.refuse_if_locked(locked)
    }

    /// [`Editor::refuse_locked_content`] for `a..b` rather than the selection.
    pub(super) fn refuse_locked_content_in(&mut self, a: &Position, b: &Position) -> bool {
        let locked = self.locks(a, b, false);
        self.refuse_if_locked(locked)
    }

    fn refuse_if_locked(&mut self, locked: bool) -> bool {
        if locked {
            self.refused = Some("locked");
            self.events.push(super::input::Notice::Refused { sub_template: None, element: None });
        }
        locked
    }

    // Data parts
    fn data_parts_at(&self, ns: &str, item: Option<&str>) -> Result<(String, Option<String>, Vec<usize>), Refused> {
        let (ns, item) = (ns.to_string(), item.map(str::to_string));
        let at: Vec<usize> = self
            .doc
            .data_parts
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.namespace.as_deref() == Some(ns.as_str()) && item.as_ref().is_none_or(|i| crate::docx::binding::same_item(i, &p.item_id))
            })
            .map(|(k, _)| k)
            .collect();
        if at.len() > 1 {
            return Err(shape(if item.is_none() { "ambiguous namespace" } else { "ambiguous item id" }));
        }
        Ok((ns, item, at))
    }

    pub(super) fn write_data_part(&mut self, w: WriteDataPart, fonts: &Fonts) -> Outcome {
        outcome(self.data_write(w, fonts))
    }

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
                let item_id = match item {
                    Some(i) => crate::docx::binding::guid(&i).ok_or_else(|| Error::invalid("an item id is a GUID"))?,
                    None => new_item_id(&xml, self.doc.data_parts.len()),
                };
                if self.doc.data_parts.iter().any(|p| crate::docx::binding::same_item(&p.item_id, &item_id)) {
                    return Err(Error::invalid("another part has the item id"));
                }
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

    // Fields
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

    pub(super) fn fields_here(&self, ids: Option<&[crate::api::value::ElementId]>, nested: bool) -> Vec<(FieldId, ParaId)> {
        let all = self.all_fields();
        if let Some(ids) = ids {
            let want: Vec<ElementId> = ids.iter().filter_map(|x| elements::parse_id(&x.0)).collect();
            return all.iter().filter(|f| want.contains(&ElementId::Field(f.0))).map(|f| (f.0, f.2)).collect();
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Vec::new() };
        let range = |f: &(FieldId, StoryId, ParaId, usize, Unit, Unit)| (Position::text(f.1, f.2, f.4), Position::text(f.1, f.2, f.5));
        // Every range of a discontiguous selection, in document order
        let mut ranges = self.other_ranges().to_vec();
        if !a.same_place(&b) {
            let k = ranges.partition_point(|r| self.index.cmp(&r.0, &a).is_lt());
            ranges.insert(k, (a, b));
        }
        let mut picked: Vec<(FieldId, StoryId, ParaId, usize, Unit, Unit)> = if !ranges.is_empty() {
            all.iter()
                .filter(|f| {
                    let (s, e) = range(f);
                    ranges.get(ranges.partition_point(|r| self.index.cmp(&r.1, &s).is_le())).is_some_and(|r| self.index.cmp(&r.0, &e).is_lt())
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

    /// F11 and Shift+F11: selects the next (or previous) field in the story, the outermost of nested ones, from where
    /// the selection starts. A walk from the caret's paragraph that stops at the first one: the model holds no field
    /// across paragraphs (reading keeps such a field's result as text), so each paragraph's nesting starts at 0.
    pub fn select_field(&mut self, order: crate::api::value::Order, fonts: &Fonts) -> Outcome {
        let Some((a, _)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no caret") };
        let next = order == crate::api::value::Order::Next;
        let nav = self.nav();
        // A row end has no text: the walk starts in the paragraph beside it, whole.
        let start = a.para_offset().map(|(p, u)| (p, Some(u.0))).or_else(|| {
            let beside = if next { nav.right(a, false) } else { nav.left(a, false) };
            beside.para_offset().map(|(p, _)| (p, None))
        });
        let Some((from, at)) = start else { return Outcome::Unchanged("no caret") };
        let found = {
            let mut para = from;
            loop {
                let Some(p) = nav.idx.para(nav.doc, para) else { break None };
                if let Some((s, e)) = outer_field(&p.inlines, next, if para == from { at } else { None }) {
                    break Some((para, s, e));
                }
                match nav.para_beside(para, next) {
                    Some(q) => para = q,
                    None => break None,
                }
            }
        };
        let Some((para, s, e)) = found else { return Outcome::Unchanged("no field there") };
        self.history.break_typing();
        self.set_selection(Selection::range(Position::text(a.story, para, s), Position::text(a.story, para, e), super::selection::Granularity::Char));
        if let Some(p) = self.caret() {
            self.ensure(&p, fonts);
            self.scroll_to_caret(fonts);
        }
        Outcome::Moved
    }

    fn set_views(&mut self, fields: &[(FieldId, ParaId)], view: FieldView) {
        let mut hints = Vec::new();
        let mut shown: Vec<Step> = Vec::new();
        for (f, para) in fields {
            let Some(p) = self.index.para_mut(&mut self.doc, *para) else { continue };
            for i in p.inlines.iter_mut() {
                if let Inline::FieldBegin { id, view: v, .. } = i
                    && id == f
                    && *v != view
                {
                    *v = view;
                    hints.push(super::step::para_hint(&self.index, *para, 0));
                    if !matches!(shown.last(), Some(Step::FieldViews { para: q, .. }) if q == para) {
                        shown.push(Step::FieldViews { para: *para, views: Vec::new() });
                    }
                }
            }
        }
        self.touched.note(fields.iter().map(|f| f.1));
        // A field shown otherwise is an element changed
        self.track_elements(&shown);
        if !hints.is_empty() {
            self.relayout(hints);
        }
    }

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
            // An updated result is current
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let stale = |i: &Inline| matches!(i, Inline::FieldBegin { id, dirty: true, .. } if fields.iter().any(|f| f.0 == *id && f.1 == para));
            if p.inlines.iter().any(stale) {
                let mut inlines = p.inlines.clone();
                for i in inlines.iter_mut() {
                    if stale(i)
                        && let Inline::FieldBegin { dirty, .. } = i
                    {
                        *dirty = false;
                    }
                }
                let _ = self.step(&mut tx, Step::Restyle { para, inlines });
            }
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn show_field_codes(&mut self, show: bool, ids: Option<&[crate::api::value::ElementId]>) -> Outcome {
        let targets: Vec<(FieldId, ParaId)> = match ids {
            Some(_) => self.fields_here(ids, false),
            None => self.all_fields().iter().map(|f| (f.0, f.2)).collect(),
        };
        self.field_views(targets, if show { FieldView::Code } else { FieldView::Result })
    }

    pub fn field_codes_toggle(&self, all: bool) -> Option<crate::api::command::ShowFieldCodes> {
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

    pub(super) fn update_fields(&mut self, ids: Option<&[crate::api::value::ElementId]>, fonts: &Fonts) -> Outcome {
        let mut fields = self.fields_here(ids, true);
        let all = self.all_fields();
        let depth = |f: &FieldId| {
            let me = all.iter().find(|g| g.0 == *f);
            me.map_or(0, |m| all.iter().filter(|g| g.2 == m.2 && g.4 < m.4 && m.5 < g.5).count())
        };
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
        // Document properties and variables: the engine's to compute.
        let own: Vec<(FieldId, String)> = unlocked
            .iter()
            .filter_map(|(f, para)| {
                let p = self.index.para(&self.doc, *para)?;
                let k = p.inlines.iter().position(|i| matches!(i, Inline::FieldBegin { id, .. } if id == f))?;
                let Inline::FieldBegin { kind: Some(kind), .. } = &p.inlines[k] else { return None };
                let code = crate::docx::fields::code_text(&p.inlines, crate::docx::fields::span_at(&p.inlines, k)?);
                if *kind == FieldKind::FileName {
                    return Some((*f, crate::docx::fields::file_name_shown(self.doc.file_name.as_deref()?, &code)));
                }
                if !matches!(kind, FieldKind::DocProperty(_) | FieldKind::DocVariable(_) | FieldKind::DocDate(_)) {
                    return None;
                }
                Some((*f, crate::docx::fields::document_value(kind, code.trim(), &self.doc.properties, &self.doc.settings.variables)))
            })
            .collect();
        let out = if own.is_empty() {
            out
        } else {
            let mut tx = self.begin("Update Fields");
            for (f, v) in &own {
                self.set_result(&mut tx, *f, &Body::Text(v.clone()));
            }
            match self.commit(tx, None, fonts) {
                o @ Outcome::Changed(_) => o,
                _ => out,
            }
        };
        self.update_token += 1;
        let token = self.update_token;
        let fields: Vec<(ElementId, String)> = unlocked
            .iter()
            .filter(|(f, _)| !own.iter().any(|(g, _)| g == f))
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

    pub(super) fn set_field_results(&mut self, r: FieldResults, fonts: &Fonts) -> Outcome {
        let mut tx = self.begin("Update Fields");
        for result in &r.results {
            let Some(ElementId::Field(f)) = elements::parse_id(&result.id.0) else {
                self.abort(tx);
                return Outcome::Refused(no_element());
            };
            self.set_result(&mut tx, f, &result.display);
        }
        if tx.undo.is_empty() {
            self.abort(tx);
            return Outcome::Unchanged("no results");
        }
        self.follow(&tx);
        self.commit(tx, None, fonts)
    }

    pub(super) fn set_result(&mut self, tx: &mut super::Tx, f: FieldId, display: &Body) -> bool {
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
        let text = self.result_inlines(display, para, at(first), &run);
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

    pub(super) fn unlink_fields(&mut self, ids: Option<&[crate::api::value::ElementId]>, fonts: &Fonts) -> Outcome {
        let mut fields = self.fields_here(ids, false);
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
        fields.sort_by_key(|(f, _)| std::cmp::Reverse(all.iter().position(|g| g.0 == *f)));
        let mut tx = self.begin("Unlink Fields");
        for (f, para) in fields {
            let Some(Where::Inline { first, last, start, end, .. }) = self.locate(ElementId::Field(f)) else { continue };
            let Some(p) = self.index.para(&self.doc, para).cloned() else { continue };
            let Some(s) = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last) else { continue };
            let spans = crate::docx::fields::spans(&p.inlines);
            let mut with = Vec::new();
            unlinked(&p.inlines, &spans, s, 0, &mut with);
            let _ = self.step(&mut tx, Step::Text { para, range: start..end, with, edges: Default::default() });
        }
        if tx.undo.is_empty() {
            self.abort(tx);
            return Outcome::Unchanged("no field here");
        }
        self.follow(&tx);
        self.commit(tx, None, fonts)
    }

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
        let a = if ua2 == ua { a } else { Position::text(a.story, pa, ua2) };
        let b = if ub2 == ub { b } else { Position::text(b.story, pb, ub2) };
        (a, b)
    }

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
            // A sub-template's paragraph is also drawn in the body, laid out apart from it.
            if self.index.para_slot(para).is_some_and(|s| matches!(self.index.story(s.container), StoryId::BuildingBlock(_))) {
                self.expansions_stale = true;
            }
            hints.push(super::step::para_hint(&self.index, para, 0));
            self.touched.note(std::iter::once(para));
        }
        if !hints.is_empty() {
            self.relayout(hints);
        }
    }

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
        outcome(self.field_set(&f.id, f.code.as_deref(), f.display.as_ref(), fonts))
    }

    /// A field result in `run`'s formatting, at unit `at` of `para`: lines and a rich display's paragraphs end in line
    /// breaks, and its runs take their bold, italic, underline, strikethrough and links over `run`.
    fn result_inlines(&self, display: &Body, para: ParaId, at: Unit, run: &crate::docx::Run) -> Vec<Inline> {
        let rich = match display {
            Body::Text(t) => return super::comments::rich::text_inlines(t, run),
            Body::Rich(r) => r,
        };
        let res = self.doc.resolver.clone();
        let cx = self.cx();
        let base = res.as_ref().and_then(|res| self.run_base(para, at, res, &cx));
        let w = &cx.w;
        let mut out = Vec::new();
        for (k, p) in rich.paragraphs.iter().enumerate() {
            if k > 0 {
                out.push(Inline::Break(crate::docx::BreakKind::Line, run.clone()));
            }
            for r in p.runs.iter().filter(|r| !r.text.is_empty()) {
                let flags = [(r.bold, "b"), (r.italic, "i"), (r.underline, "u"), (r.strikethrough, "strike")];
                let formatted = match (&res, &base) {
                    (Some(res), Some(base)) if flags.iter().any(|(on, _)| on.is_some()) => {
                        let mut rp = crate::docx::write::props::Props::parse(run.direct.0.as_deref(), "rPr", &cx);
                        for (on, name) in flags {
                            let Some(on) = on else { continue };
                            let xml = match (name, on) {
                                ("u", true) => format!("<{w}u {w}val=\"single\"/>"),
                                ("u", false) => format!("<{w}u {w}val=\"none\"/>"),
                                (_, true) => format!("<{w}{name}/>"),
                                (_, false) => format!("<{w}{name} {w}val=\"0\"/>"),
                            };
                            rp.set(name, xml);
                        }
                        res.run_from_xml(base, rp.xml().as_deref(), &cx)
                    }
                    _ => run.clone(),
                };
                // A link to follow; any other address shows as text.
                match r.link.as_deref().and_then(super::html::web_address) {
                    Some(address) => {
                        let code = crate::docx::fields::link_code(&address);
                        out.extend(crate::docx::fields::field_inlines(&code, &r.text, &formatted));
                    }
                    None => out.extend(super::comments::rich::text_inlines(&r.text, &formatted)),
                }
            }
        }
        out
    }

    pub(super) fn field_set(
        &mut self,
        id: &crate::api::value::ElementId,
        code: Option<&str>,
        display: Option<&Body>,
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
        if let Some(d) = display {
            let run = s.sep.and_then(|sep| run_in(sep + 1, s.end)).unwrap_or_else(|| begin_run.clone());
            let text = self.result_inlines(d, para, at(first), &run);
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

    pub(super) fn follow(&mut self, tx: &super::Tx) {
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
            self.selection = Selection::Caret(tx.before.selection.focus().map(map).filter(|p| valid(p)).unwrap_or_else(|| {
                let story = self.selection.focus().map_or(StoryId::Body, |p| p.story);
                self.nav().story_edge(story, false).unwrap_or(Position::text(StoryId::Body, ParaId(0), Unit(0)))
            }));
        }
    }

    pub(super) fn refresh_field_kinds(&mut self, tx: &mut super::Tx, para: ParaId) {
        let Some(p) = self.index.para(&self.doc, para) else { return };
        let mut inlines = p.inlines.clone();
        let mut changed = false;
        for s in crate::docx::fields::spans(&p.inlines) {
            let code = crate::docx::fields::parse(&crate::docx::fields::code_text(&p.inlines, s));
            let want = if self.application_field(&code) { None } else { code.kind() };
            if let Inline::FieldBegin { kind, .. } = &mut inlines[s.begin]
                && *kind != want
                && !kind.as_ref().is_some_and(FieldKind::is_not_from_code)
            {
                *kind = want;
                changed = true;
            }
        }
        if changed {
            let _ = self.step(tx, Step::Restyle { para, inlines });
        }
    }

    // Element removal
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
        // Placeholder text goes with its control
        let keep = keep && !matches!(e, ElementId::Control(c) if self.doc.controls.get(&c).is_some_and(|k| k.props.showing_placeholder));
        let mut tx = self.begin(if matches!(e, ElementId::Field(_)) { "Remove Field" } else { "Remove Content Control" });
        // Control removal keeping its contents: untracked
        if keep && matches!(e, ElementId::Control(_)) {
            tx.tracked = false;
        }
        let ok = match (e, w) {
            (ElementId::Field(_), Where::Inline { para, start, end, .. }) => {
                self.step(&mut tx, Step::Text { para, range: start..end, with: Vec::new(), edges: Default::default() }).is_ok()
            }
            (ElementId::Control(c), Where::Inline { para, start, end, .. }) => {
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
                    let last =
                        blocks.iter().rev().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None }).ok_or_else(no_element)?;
                    let mut p = self.split_template(last).ok_or_else(no_element)?;
                    if let Some(k) = p.controls.iter().position(|x| *x == c) {
                        p.controls.truncate(k);
                    }
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

    pub(super) fn insert_control(&mut self, mut c: InsertControl, fonts: &Fonts) -> Outcome {
        let Some(float) = c.float.take() else { return outcome(self.control_insert(c, fonts)) };
        if c.kind != KindArg::Picture {
            return Outcome::Refused(Error::invalid("only a picture control floats"));
        }
        if float.wrap == crate::api::value::ObjectWrap::Inline {
            return Outcome::Refused(Error::invalid("a float needs a floating wrap"));
        }
        let before: std::collections::HashSet<ControlId> = self.doc.controls.keys().copied().collect();
        self.begin_batch(Some("Insert Content Control".into()));
        let inserted = self.control_insert(c, fonts);
        let new = self.doc.controls.keys().copied().find(|k| !before.contains(k));
        let Some(o) = new.filter(|_| matches!(inserted, Ok(Outcome::Changed(_)))).and_then(|k| self.picture_of_control(k)) else {
            self.end_batch(false, fonts);
            return outcome(inserted);
        };
        self.set_selection(Selection::Object(o));
        let floated = matches!(self.set_object_wrap(float.wrap, fonts), Outcome::Changed(_));
        if floated && (float.horizontal.is_some() || float.vertical.is_some()) {
            let at = crate::api::command::ObjectPosition {
                horizontal: float.horizontal,
                vertical: float.vertical,
                allow_overlap: None,
                layout_in_cell: None,
            };
            self.set_object_position(at, fonts);
        }
        self.end_batch(floated, fonts)
    }

    // Content controls
    fn control_insert(&mut self, c: InsertControl, fonts: &Fonts) -> Result<Outcome, Refused> {
        let w = self.cx().w;
        let esc = crate::docx::write::props::esc;
        let mut checked_now = false;
        let mut kind_xml = None;
        let mut size = (144.0, 108.0);
        let placeholder_image = match &c.placeholder_image {
            Some(_) if c.kind != KindArg::Picture => return Err(Error::invalid("placeholderImage is for picture controls")),
            Some(p) => Some(placeholder_data(p)?),
            None => None,
        };
        let kind = match (c.kind, &c.options) {
            (KindArg::PlainText, Some(ControlOptions::PlainText(o))) => ControlKind::PlainText { multi_line: o.multiline.unwrap_or(false) },
            (KindArg::PlainText, _) => ControlKind::PlainText { multi_line: false },
            (KindArg::RichText, _) => ControlKind::RichText,
            (KindArg::Picture, o) => {
                if let Some(ControlOptions::Picture(o)) = o {
                    size = picture_size(o, size)?;
                }
                ControlKind::Picture
            }
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
                let image = self.pending_image(placeholder_image.unwrap_or_else(|| PICTURE_PLACEHOLDER.to_vec()));
                let emu = crate::drawing::units::Emu::from_pt;
                let picture = crate::docx::Picture {
                    w: size.0,
                    h: size.1,
                    image: Some(image),
                    alt: "".into(),
                    lock_aspect: true,
                    effect: [0.0; 4],
                    shape: Some(Box::new({
                        let mut s = crate::drawing::Shape::picture(image, emu(size.0), emu(size.1));
                        s.nv.name = self.new_object_name("Picture");
                        s
                    })),
                    text: None,
                    src: Default::default(),
                };
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

    fn control_wrap(&mut self, id: ControlId, props: ControlProps, fonts: &Fonts) -> Result<Outcome, Refused> {
        // Selected cells: their rows, or the whole table
        if let Selection::Cells { table, anchor, focus, .. } = self.selection {
            let t = self.index.table(&self.doc, table).ok_or_else(no_element)?;
            let last = t.rows.len().saturating_sub(1);
            let (r0, r1) = (anchor.row.min(focus.row).min(last), anchor.row.max(focus.row).min(last));
            let whole = r0 == 0 && r1 == last;
            let at = if whole {
                Vec::new()
            } else {
                let lists: Vec<&Vec<ControlId>> = t.rows[r0..=r1].iter().map(|r| &r.controls).collect();
                let outside: Vec<&Vec<ControlId>> = t.rows[..r0].iter().chain(&t.rows[r1 + 1..]).map(|r| &r.controls).collect();
                nest(&lists, &outside)?
            };
            let mut tx = self.begin("Insert Content Control");
            self.pending_controls.push((id, Control::new(props)));
            let ok = if whole {
                let slot = self.index.table_slot(table).ok_or_else(no_element)?;
                let mut with = t.clone();
                with.controls.push(id);
                self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: vec![Block::Table(with)] })
                    .is_ok()
            } else {
                let mut with = t.rows[r0..=r1].to_vec();
                with.iter_mut().zip(at).for_each(|(r, k)| r.controls.insert(k, id));
                tx.stale_tables.push(table);
                self.step(&mut tx, Step::Rows { table, range: r0..r1 + 1, with, grid: None }).is_ok()
            };
            if !ok {
                self.abort(tx);
                return Err(bad("target"));
            }
            return Ok(self.commit(tx, None, fonts));
        }
        let (a, b) = self.selection.ordered(&self.index).ok_or_else(|| bad("target"))?;
        if a.same_place(&b) {
            return Err(bad("target"));
        }
        if let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset())
            && pa == pb
        {
            let p = self.index.para(&self.doc, pa).ok_or_else(no_element)?;
            let mut inlines = split_at(&p.inlines, &[oa.0, ob.0]);
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
            let (i, j) = around_controls(&inlines, index_at(&inlines, oa.0, true), index_at(&inlines, ob.0, false));
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
        // Each end's block in the container both are in: its paragraph, or the table it is in (wrapped whole).
        let (ca, cb) = (self.index.chain_of(&a), self.index.chain_of(&b));
        let k = ca.iter().zip(&cb).take_while(|(x, y)| x.table == y.table && x.row == y.row && x.cell == y.cell).count();
        let top = |e: &Editor, p: &Position, chain: &[super::index::Level]| -> Option<(super::index::Slot, bool)> {
            match chain.get(k) {
                Some(l) => Some((e.index.table_slot(l.table)?, true)),
                None => Some((e.index.para_slot(p.para_offset()?.0)?, false)),
            }
        };
        let ((sa, a_table), (sb, b_table)) = (top(self, &a, &ca).ok_or_else(no_element)?, top(self, &b, &cb).ok_or_else(no_element)?);
        let starts = a_table || a.para_offset().is_some_and(|(_, u)| u == Unit(0));
        let ends = |e: &Editor| match b.para_offset() {
            Some((pb, ob)) if !b_table => e.index.para(&e.doc, pb).is_some_and(|q| ob == Unit(0) || ob == super::step::para_len(q)),
            _ => true,
        };
        if !starts || sa.container != sb.container || !ends(self) {
            return Err(shape("partial paragraph"));
        }
        let last = if !b_table && b.para_offset().is_some_and(|(_, u)| u == Unit(0)) { sb.block } else { sb.block + 1 };
        if last <= sa.block {
            return Err(bad("target"));
        }
        let blocks = self.index.blocks(&self.doc, sa.container).ok_or_else(no_element)?;
        let lists: Vec<&Vec<ControlId>> = blocks[sa.block..last].iter().map(block_controls).collect();
        let outside: Vec<&Vec<ControlId>> =
            blocks.iter().enumerate().filter(|(k, _)| *k < sa.block || *k >= last).map(|(_, b)| block_controls(b)).collect();
        let at = nest(&lists, &outside)?;
        let with: Vec<Block> = blocks[sa.block..last]
            .iter()
            .zip(at)
            .map(|(b, k)| {
                let mut b = b.clone();
                match &mut b {
                    Block::Para(p) => p.controls.insert(k, id),
                    Block::Table(t) => t.controls.insert(k, id),
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

    /// Inserts a building block gallery content control at the selection, offering the building blocks of
    /// `gallery` (Word's name for it: `Quick Parts`, `AutoText`, `Custom 1`, ...) and `category` (each `None`: any),
    /// showing Word's placeholder until one is chosen. Its id is the command's result.
    pub fn insert_gallery_control(&mut self, gallery: Option<String>, category: Option<String>, fonts: &Fonts) -> Outcome {
        outcome(self.gallery_insert(gallery, category, fonts))
    }

    fn gallery_insert(&mut self, gallery: Option<String>, category: Option<String>, fonts: &Fonts) -> Result<Outcome, Refused> {
        self.editable_target()?;
        let w = self.cx().w;
        let kind_xml = crate::docx::KeptKind::Gallery { gallery, category }.part_xml(&w);
        let props = ControlProps { kind: ControlKind::Other, kind_xml, showing_placeholder: true, ..Default::default() };
        let id = ControlId::next();
        self.pending_controls.push((id, Control::new(props)));
        let out = self.insert_inlines(
            |run| vec![Inline::ControlStart(id), Inline::Text("Choose a building block.".into(), run), Inline::ControlEnd(id)],
            "Insert Content Control",
            fonts,
        );
        let Outcome::Changed(_) = out else { return Err(locked()) };
        self.made = Some(CommandResult::Element { id: api_id(ElementId::Control(id)) });
        Ok(out)
    }

    /// Sets building block gallery control `id`'s gallery and category (`Some(None)` clears one: any); what is
    /// `None` stays. One undo step.
    pub fn set_gallery_control(
        &mut self,
        id: &crate::api::value::ElementId,
        gallery: Option<Option<String>>,
        category: Option<Option<String>>,
        fonts: &Fonts,
    ) -> Outcome {
        let r = (|| {
            let (e, _) = self.element(id)?;
            let ElementId::Control(c) = e else { return Err(bad("id")) };
            let mut control = self.doc.controls.get(&c).cloned().ok_or_else(no_element)?;
            let Some(crate::docx::KeptKind::Gallery { gallery: g, category: k }) = control.props.kept_kind() else { return Err(bad("id")) };
            let kind = crate::docx::KeptKind::Gallery { gallery: gallery.unwrap_or(g), category: category.unwrap_or(k) };
            let xml = kind.part_xml(&self.cx().w);
            if control.props.kind_xml == xml {
                return Ok(Outcome::Unchanged("already so"));
            }
            control.props.kind_xml = xml;
            let mut tx = self.begin("Content Control Properties");
            if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_err() {
                self.abort(tx);
                return Err(no_element());
            }
            Ok(self.commit(tx, None, fonts))
        })();
        outcome(r)
    }

    /// Sets date control `id`'s calendar (Word's calendar types: `gregorian`, `hijri`, `hebrew`, `taiwan`,
    /// `japan`, `thai`, `korea`, `saka`, `gregorianXlitEnglish`, `gregorianXlitFrench`, `gregorianUs`,
    /// `gregorianMeFrench`, `gregorianArabic`) and what it stores in its data (Store XML contents in the following
    /// format); what is `None` stays. One undo step.
    pub fn set_date_control_storage(
        &mut self,
        id: &crate::api::value::ElementId,
        calendar: Option<&str>,
        storage: Option<DateStorage>,
        fonts: &Fonts,
    ) -> Outcome {
        const CALENDARS: &[&str] = &[
            "gregorian",
            "hijri",
            "hebrew",
            "taiwan",
            "japan",
            "thai",
            "korea",
            "saka",
            "gregorianXlitEnglish",
            "gregorianXlitFrench",
            "gregorianUs",
            "gregorianMeFrench",
            "gregorianArabic",
        ];
        let r = (|| {
            if calendar.is_some_and(|c| !CALENDARS.contains(&c)) {
                return Err(bad("calendar"));
            }
            let (e, _) = self.element(id)?;
            let ElementId::Control(c) = e else { return Err(bad("id")) };
            let mut control = self.doc.controls.get(&c).cloned().ok_or_else(no_element)?;
            let ControlKind::Date { format, .. } = &control.props.kind else { return Err(bad("id")) };
            let w = self.cx().w;
            let esc = crate::docx::write::props::esc;
            let mut x = control.props.kind_xml.clone().unwrap_or_else(|| format!("<{w}date><{w}dateFormat {w}val=\"{}\"/></{w}date>", esc(format)));
            let after_lid = if x.contains("lid") { "lid" } else { "dateFormat" };
            if let Some(s) = storage {
                x = with_child_val(&x, "storeMappedDataAs", s.xml(), &w, Some(after_lid));
            }
            if let Some(cal) = calendar {
                let after = if x.contains("storeMappedDataAs") { "storeMappedDataAs" } else { after_lid };
                x = with_child_val(&x, "calendar", cal, &w, Some(after));
            }
            if control.props.kind_xml.as_deref() == Some(x.as_str()) {
                return Ok(Outcome::Unchanged("already so"));
            }
            control.props.kind_xml = Some(x);
            let mut tx = self.begin("Content Control Properties");
            if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_err() {
                self.abort(tx);
                return Err(no_element());
            }
            Ok(self.commit(tx, None, fonts))
        })();
        outcome(r)
    }

    pub(super) fn set_control(&mut self, c: SetControl, fonts: &Fonts) -> Outcome {
        self.picture_writes = true;
        let out = self.control_set(c, fonts);
        self.picture_writes = false;
        outcome(out)
    }

    fn control_set(&mut self, set: SetControl, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (e, w) = self.element(&set.id)?;
        let ElementId::Control(c) = e else { return Err(Error::invalid("not a content control")) };
        let mut control = self.doc.controls.get(&c).cloned().ok_or_else(no_element)?;
        let prefix = self.cx().w;
        let p = &mut control.props;
        let mut resize = None;
        // A checkbox's face for its state in its new symbols.
        let mut face: Option<(char, String)> = None;
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
                    // Only the format and locale of the `w:date` read change.
                    if let Some(x) = &mut p.kind_xml {
                        if let Some(f) = &o.format {
                            *x = with_child_val(x, "dateFormat", f, &prefix, None);
                        }
                        if let Some(l) = &o.locale {
                            *x = with_child_val(x, "lid", l, &prefix, Some("dateFormat"));
                        }
                    }
                }
                (ControlKind::DropDown, ControlOptions::List(l)) => {
                    let esc = crate::docx::write::props::esc;
                    let items: String = l
                        .items
                        .iter()
                        .map(|i| format!("<{prefix}listItem {prefix}displayText=\"{}\" {prefix}value=\"{}\"/>", esc(&i.text), esc(&i.value)))
                        .collect();
                    // The element as read (a drop-down list or a combo box, its last value) with the new entries.
                    let was = p.kind_xml.clone().unwrap_or_else(|| format!("<{prefix}dropDownList/>"));
                    let end = crate::docx::binding::tag_end(&was).ok_or_else(|| Error::invalid("a drop-down without its element"))?;
                    let head = was[..end].trim_end_matches('/');
                    let name = head.get(1..).and_then(|h| h.split(|c: char| c.is_whitespace() || c == '/').next()).unwrap_or_default();
                    p.kind_xml = Some(format!("{head}>{items}</{name}>"));
                }
                (ControlKind::Checkbox { checked, unchecked }, ControlOptions::Checkbox(o)) => {
                    if o.checked_symbol.is_some() || o.unchecked_symbol.is_some() {
                        let (was_checked, was_unchecked) = (*checked, *unchecked);
                        let font = |x: &str, name: &str| {
                            let at = x.find(&format!("{name} "))?;
                            let f = x[at..].find("font=\"")? + at + 6;
                            Some(x[f..f + x[f..].find('"')?].to_string())
                        };
                        let was = p.kind_xml.clone().unwrap_or_default();
                        let one = |s: &crate::api::value::CheckboxSymbol| s.character.chars().next();
                        if let Some(c) = o.checked_symbol.as_ref().and_then(one) {
                            *checked = c;
                        }
                        if let Some(c) = o.unchecked_symbol.as_ref().and_then(one) {
                            *unchecked = c;
                        }
                        let f_on = o
                            .checked_symbol
                            .as_ref()
                            .map(|s| s.font.clone())
                            .or_else(|| font(&was, "checkedState"))
                            .unwrap_or_else(|| "MS Gothic".into());
                        let f_off = o
                            .unchecked_symbol
                            .as_ref()
                            .map(|s| s.font.clone())
                            .or_else(|| font(&was, "uncheckedState"))
                            .unwrap_or_else(|| "MS Gothic".into());
                        // The box's state is its face: checked when it shows the checked symbol it had (its XML's
                        // when both symbols were one).
                        let on = match &w {
                            Where::Sym { para, at, .. } if was_checked != was_unchecked => self
                                .index
                                .para(&self.doc, *para)
                                .and_then(|q| super::stories::inline_at(q, *at))
                                .and_then(|i| if let Inline::Sym { ch, .. } = i { Some(*ch == was_checked) } else { None }),
                            _ => None,
                        }
                        .unwrap_or_else(|| crate::docx::xml::checkbox_checked(&was).unwrap_or(false));
                        face = Some(if on { (*checked, f_on.clone()) } else { (*unchecked, f_off.clone()) });
                        let w14 = crate::docx::xml::W14;
                        let esc = crate::docx::write::props::esc;
                        p.kind_xml = Some(format!(
                            r#"<w14:checkbox xmlns:w14="{w14}"><w14:checked w14:val="{}"/><w14:checkedState w14:val="{:04X}" w14:font="{}"/><w14:uncheckedState w14:val="{:04X}" w14:font="{}"/></w14:checkbox>"#,
                            u8::from(on),
                            *checked as u32,
                            esc(&f_on),
                            *unchecked as u32,
                            esc(&f_off)
                        ));
                    }
                }
                (ControlKind::RepeatingSection, ControlOptions::RepeatingSection(o)) => {
                    let was = p.kind_xml.clone().unwrap_or_default();
                    let title = o.section_title.clone().or_else(|| {
                        let at = was.find("sectionTitle ")?;
                        let v = was[at..].find("val=\"")? + at + 5;
                        Some(was[v..v + was[v..].find('"')?].to_string())
                    });
                    let fixed = o.allow_insert_delete.map_or(was.contains("doNotAllowInsertDeleteSection"), |a| !a);
                    let w15 = crate::docx::xml::W15;
                    let title = title
                        .filter(|t| !t.is_empty())
                        .map_or(String::new(), |t| format!(r#"<w15:sectionTitle w15:val="{}"/>"#, crate::docx::write::props::esc(&t)));
                    let fixed = if fixed { "<w15:doNotAllowInsertDeleteSection/>" } else { "" };
                    p.kind_xml = Some(format!(r#"<w15:repeatingSection xmlns:w15="{w15}">{title}{fixed}</w15:repeatingSection>"#));
                }
                (ControlKind::Picture, ControlOptions::Picture(o)) => resize = Some(o.clone()),
                _ => return Err(Error::invalid("options for another kind of control")),
            }
            if !matches!(
                p.kind,
                ControlKind::DropDown
                    | ControlKind::Checkbox { .. }
                    | ControlKind::Picture
                    | ControlKind::RepeatingSection
                    | ControlKind::Date { .. }
            ) {
                p.kind_xml = None;
            }
        }
        // Appearance, colour, temporary and the contents' style: `w:sdtPr` children by local name.
        let mut rest = |name: &str, xml: Option<String>| {
            p.rest.retain(|(n, _)| n != name);
            if let Some(x) = xml {
                p.rest.push((name.to_string(), x));
            }
        };
        let w15 = crate::docx::xml::W15;
        if let Some(a) = set.appearance {
            let v = match a {
                crate::api::value::ControlAppearance::BoundingBox => "boundingBox",
                crate::api::value::ControlAppearance::Tags => "tags",
                crate::api::value::ControlAppearance::Hidden => "hidden",
            };
            rest("appearance", Some(format!(r#"<w15:appearance xmlns:w15="{w15}" w15:val="{v}"/>"#)));
        }
        if let Some(c) = set.color {
            let xml = match c {
                crate::api::value::NoneOr::None => None,
                crate::api::value::NoneOr::Value(c) => Some(format!(r#"<w15:color xmlns:w15="{w15}" {prefix}val="{:06X}"/>"#, c.0)),
            };
            rest("color", xml);
        }
        if let Some(t) = set.temporary {
            rest("temporary", t.then(|| format!("<{prefix}temporary/>")));
        }
        if let Some(s) = &set.style {
            let xml = match s {
                crate::api::value::NoneOr::None => None,
                crate::api::value::NoneOr::Value(id) => {
                    Some(format!("<{prefix}rPr><{prefix}rStyle {prefix}val=\"{}\"/></{prefix}rPr>", crate::docx::write::props::esc(&id.0)))
                }
            };
            rest("rPr", xml);
        }
        let placeholder = set.placeholder;
        let showing = p.showing_placeholder;
        let image = match &set.placeholder_image {
            None => None,
            Some(_) if p.kind != ControlKind::Picture => return Err(Error::invalid("placeholderImage is for picture controls")),
            Some(_) if !showing => return Err(Error::unavailable("the control isn't showing its placeholder")),
            Some(i) => Some(placeholder_data(i)?),
        };
        // The control's picture, inline or floating: its new size, and its new placeholder picture (a new image, so
        // the picture is written afresh; its shape's fill draws it).
        let resized = match &w {
            _ if resize.is_none() && image.is_none() => None,
            Where::Inline { para, first, last, .. } => {
                let mut inlines = self.index.para(&self.doc, *para).ok_or_else(no_element)?.inlines.clone();
                let picture = inlines[*first..*last].iter_mut().find_map(|i| match i {
                    Inline::Picture(p, _) => Some((&mut p.w, &mut p.h, &mut p.image, &mut p.shape)),
                    Inline::Float(f, _) if f.image.is_some() => Some((&mut f.w, &mut f.h, &mut f.image, &mut f.shape)),
                    _ => None,
                });
                let (pw, ph, pi, shape) = picture.ok_or_else(|| Error::invalid("no picture in the control"))?;
                if let Some(o) = &resize {
                    (*pw, *ph) = picture_size(o, (*pw, *ph))?;
                }
                if let Some(data) = image {
                    let k = self.pending_image(data);
                    *pi = Some(k);
                    if let Some(crate::drawing::ShapeKind::Pic(p)) = shape.as_deref_mut().map(|s| &mut s.kind) {
                        p.blip.image = Some(k);
                    }
                }
                Some((*para, inlines))
            }
            _ => return Err(Error::invalid("no picture in the control")),
        };
        let mut tx = self.begin(CHANGE_CONTROL);
        if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_err() {
            self.abort(tx);
            return Err(no_element());
        }
        if let Some((para, inlines)) = resized
            && self.step(&mut tx, Step::Restyle { para, inlines }).is_err()
        {
            self.abort(tx);
            return Err(no_element());
        }
        if let (Some((ch, font)), Where::Sym { para, at, .. }) = (face, &w)
            && let Some(Inline::Sym { ch: was, font: was_font, run, control }) =
                self.index.para(&self.doc, *para).and_then(|q| super::stories::inline_at(q, *at)).cloned()
            && (was != ch || was_font != font)
        {
            let with = vec![Inline::Sym { ch, font, run, control }];
            if self.step(&mut tx, Step::Text { para: *para, range: *at..Unit(at.0 + 1), with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Err(no_element());
            }
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

/// A wrap's inline range `i..j` widened over the zero-width control markers at its edges whose partners are in it (or
/// at its other edge): a selection starting or ending at an inline control's edge, or of its whole content, holds
/// that control.
fn around_controls(inlines: &[Inline], mut i: usize, mut j: usize) -> (usize, usize) {
    let partner = |k: usize| -> Option<usize> {
        match inlines[k] {
            Inline::ControlStart(c) => inlines[k..].iter().position(|x| matches!(x, Inline::ControlEnd(d) if *d == c)).map(|e| k + e),
            Inline::ControlEnd(c) => inlines[..k].iter().rposition(|x| matches!(x, Inline::ControlStart(d) if *d == c)),
            _ => None,
        }
    };
    loop {
        let back = i.checked_sub(1).filter(|&k| inlines[k].units() == 0 && partner(k).is_some_and(|e| e > k && e <= j.max(i)));
        let ahead = (j < inlines.len() && inlines[j].units() == 0).then_some(j).filter(|&k| partner(k).is_some_and(|s| s < k && s >= i));
        match (back, ahead) {
            (Some(k), _) => i = k,
            (None, Some(k)) => j = k + 1,
            (None, None) => return (i, j),
        }
    }
}

/// Element `xml` with its child `local`'s `val` set to `val`: a child it doesn't have is added (after child `after`,
/// else first), written with prefix `w`.
fn with_child_val(xml: &str, local: &str, val: &str, w: &str, after: Option<&str>) -> String {
    let esc = crate::docx::write::props::esc;
    let find = |name: &str| {
        let mut from = 0;
        while let Some(k) = xml[from..].find('<').map(|k| from + k) {
            let n = xml[k + 1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or_default();
            if n.rsplit(':').next() == Some(name) && !n.starts_with('/') {
                return Some((k, k + crate::docx::binding::tag_end(&xml[k..])? + 1));
            }
            from = k + 1;
        }
        None
    };
    let name_end = |tag: &str| tag.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(tag.len());
    if let Some((start, end)) = find(local) {
        let tag = &xml[start..end];
        let new = match attr_value(tag, "val") {
            Some(v) => format!("{}{}{}", &tag[..v.start], esc(val), &tag[v.end..]),
            None => {
                let at = name_end(tag);
                format!("{} {w}val=\"{}\"{}", &tag[..at], esc(val), &tag[at..])
            }
        };
        return format!("{}{new}{}", &xml[..start], &xml[end..]);
    }
    let child = format!("<{w}{local} {w}val=\"{}\"/>", esc(val));
    if let Some((_, at)) = after.and_then(find) {
        return format!("{}{child}{}", &xml[..at], &xml[at..]);
    }
    let Some(at) = crate::docx::binding::tag_end(xml).map(|e| e + 1) else { return format!("{xml}{child}") };
    // A root without children opens for the child
    match xml[..at].strip_suffix("/>") {
        Some(head) => {
            let name = &xml[1..name_end(xml)];
            format!("{}>{child}</{name}>{}", head.trim_end(), &xml[at..])
        }
        None => format!("{}{child}{}", &xml[..at], &xml[at..]),
    }
}

/// Where the value of `tag`'s attribute of local name `local` is (`w:val = "x"` as well as `w:val="x"`).
fn attr_value(tag: &str, local: &str) -> Option<std::ops::Range<usize>> {
    let b = tag.as_bytes();
    let space = |i: usize| b.get(i).is_some_and(u8::is_ascii_whitespace);
    let mut i = tag.find(|c: char| c.is_whitespace() || c == '/' || c == '>')?;
    loop {
        while space(i) {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'/' | b'>') {
            i += 1;
        }
        let name = &tag[start..i];
        while space(i) {
            i += 1;
        }
        if name.is_empty() || b.get(i) != Some(&b'=') {
            return None;
        }
        i += 1;
        while space(i) {
            i += 1;
        }
        let q = *b.get(i).filter(|q| matches!(q, b'"' | b'\''))?;
        let from = i + 1;
        let to = from + tag[from..].find(char::from(q))?;
        if name.rsplit(':').next() == Some(local) {
            return Some(from..to);
        }
        i = to + 1;
    }
}

/// A block's block controls, outermost first.
fn block_controls(b: &Block) -> &Vec<ControlId> {
    match b {
        Block::Para(p) => &p.controls,
        Block::Table(t) => &t.controls,
    }
}

/// Where a control wrapping blocks (or rows) whose controls are `lists` goes in each list: inside the controls all of
/// them are in, outside the others. Refused when a control holds only some of them and blocks outside them too: the
/// new control would cross its boundary.
fn nest(lists: &[&Vec<ControlId>], outside: &[&Vec<ControlId>]) -> Result<Vec<usize>, Refused> {
    let mut out = Vec::with_capacity(lists.len());
    let all = |x: &ControlId| lists.iter().all(|l| l.contains(x));
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

/// How many fields deep unlinking goes into results: below that, a field's result is its text. A document nests
/// fields with flat begin and end marks, so the depth is the file's to choose.
const MAX_UNLINK_DEPTH: usize = 32;

/// A field's result as content of its own, as unlinking leaves it: its result's inlines (pictures, breaks and
/// formatting kept), the fields in it unlinked too; an evaluated field that shows a value (a page number, a date) as
/// that value.
fn unlinked(inlines: &[Inline], spans: &[crate::docx::fields::FieldSpan], s: crate::docx::fields::FieldSpan, depth: usize, out: &mut Vec<Inline>) {
    let Inline::FieldBegin { kind, value, run, .. } = &inlines[s.begin] else { return };
    let result_run = || s.sep.and_then(|sep| inlines[sep + 1..s.end].iter().find_map(|i| i.run().cloned())).unwrap_or_else(|| run.clone());
    if let (Some(_), Some(v)) = (kind, value) {
        if !v.is_empty() {
            out.push(Inline::Text(v.clone(), result_run()));
        }
        return;
    }
    let Some(sep) = s.sep else { return };
    if depth >= MAX_UNLINK_DEPTH {
        let text = crate::docx::fields::result_text(inlines, s);
        if !text.is_empty() {
            out.push(Inline::Text(text, result_run()));
        }
        return;
    }
    let mut k = sep + 1;
    let from = spans.partition_point(|t| t.begin <= sep);
    for t in spans[from..].iter().take_while(|t| t.begin < s.end) {
        if t.begin < k || t.end > s.end {
            continue;
        }
        out.extend_from_slice(&inlines[k..t.begin]);
        unlinked(inlines, spans, *t, depth + 1, out);
        k = t.end + 1;
    }
    out.extend_from_slice(&inlines[k..s.end]);
}

/// A placeholder picture's bytes, when they are the PNG, JPEG, GIF, BMP or SVG its MIME type says.
fn placeholder_data(p: &crate::api::value::PlaceholderImage) -> Result<Vec<u8>, Refused> {
    let want = match p.mime.trim().to_ascii_lowercase().as_str() {
        "image/png" => "image/png",
        "image/jpeg" | "image/jpg" => "image/jpeg",
        "image/gif" => "image/gif",
        "image/bmp" | "image/x-ms-bmp" => "image/bmp",
        "image/svg+xml" => "image/svg+xml",
        m => return Err(Error::invalid(format!("placeholderImage: {m} isn't PNG, JPEG, GIF, BMP or SVG"))),
    };
    match crate::docx::image::image_type(&p.data.0) {
        Some((m, _)) if m == want => Ok(p.data.0.clone()),
        _ => Err(Error::invalid(format!("placeholderImage: the data isn't {want}"))),
    }
}

/// A picture control's size from its options, each side within 1…1584 pt; a side not given keeps `was`'s.
fn picture_size(o: &crate::api::value::PictureOptions, was: (f64, f64)) -> Result<(f64, f64), Refused> {
    let side = |v: Option<crate::api::value::Points>, was: f64| match v.map(|v| v.0) {
        Some(v) if (1.0..=1584.0).contains(&v) => Ok((v * 20.0).round() / 20.0),
        Some(_) => Err(Error::invalid("picture size out of range (1…1584 pt)")),
        None => Ok(was),
    };
    Ok((side(o.width, was.0)?, side(o.height, was.1)?))
}

/// Changing a control's properties: its placeholder's text changes with them, which isn't an edit of its content.
const CHANGE_CONTROL: &str = "Change Content Control";

impl Editor {
    /// Temporary controls whose content the transaction edited go, leaving their content.
    pub(super) fn drop_temporary_controls(&mut self, tx: &mut super::Tx) {
        if tx.label == CHANGE_CONTROL {
            return;
        }
        let temporary = |c: &ControlId| self.doc.controls.get(c).is_some_and(|k| k.props.temporary());
        let mut gone: Vec<ControlId> = Vec::new();
        for s in &tx.undo {
            let Step::Text { para, range, .. } = s else { continue };
            let Some(p) = self.index.para(&self.doc, *para) else { continue };
            gone.extend(p.controls.iter().filter(|c| temporary(c)));
            // Inline ones around the edit.
            let (mut unit, mut open) = (0u32, Vec::new());
            for i in &p.inlines {
                match i {
                    Inline::ControlStart(c) => open.push((*c, unit)),
                    Inline::ControlEnd(c) => {
                        if let Some(k) = open.iter().rposition(|(o, _)| o == c) {
                            let (c, start) = open.remove(k);
                            if start <= range.start.0 && range.end.0 <= unit && temporary(&c) {
                                gone.push(c);
                            }
                        }
                    }
                    _ => {}
                }
                unit += i.units() as u32;
            }
        }
        gone.sort_unstable();
        gone.dedup();
        for c in gone {
            let step = match self.locate(ElementId::Control(c)) {
                Some(Where::Inline { para, .. }) => self.index.para(&self.doc, para).map(|p| Step::Restyle {
                    para,
                    inlines: p.inlines.iter().filter(|i| !matches!(i, Inline::ControlStart(x) | Inline::ControlEnd(x) if *x == c)).cloned().collect(),
                }),
                Some(Where::Blocks { container, range }) => self.index.blocks(&self.doc, container).map(|blocks| Step::Blocks {
                    container,
                    with: blocks[range.clone()].iter().map(|b| without_control(b, c)).collect(),
                    range,
                }),
                _ => None,
            };
            if let Some(step) = step {
                let _ = self.step(tx, step);
            }
        }
    }
}

fn without_control(b: &Block, c: ControlId) -> Block {
    let mut b = b.clone();
    match &mut b {
        Block::Para(p) => p.controls.retain(|x| *x != c),
        Block::Table(t) => t.controls.retain(|x| *x != c),
    }
    b
}

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

fn new_item_id(xml: &str, n: usize) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (xml, n, "data part").hash(&mut h);
    let a = h.finish();
    (a, "second").hash(&mut h);
    let b = h.finish();
    format!("{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}", a >> 32, (a >> 16) & 0xFFFF, a & 0xFFF, 0x8000 | (b >> 48) & 0x3FFF, b & 0xFFFF_FFFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::with_child_val;

    #[test]
    fn a_child_value_is_set_where_its_attribute_is() {
        let x = r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val = "M/d/yyyy"/></w:date>"#;
        assert_eq!(
            with_child_val(x, "dateFormat", "d MMM", "w:", None),
            r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val = "d MMM"/></w:date>"#
        );
        let x = r#"<w:date><w:dateFormat w:x="1"/></w:date>"#;
        assert_eq!(with_child_val(x, "dateFormat", "d", "w:", None), r#"<w:date><w:dateFormat w:val="d" w:x="1"/></w:date>"#);
    }

    #[test]
    fn a_child_goes_inside_a_root_without_children() {
        let x = r#"<w:date w:fullDate="2026-01-01T00:00:00Z"/>"#;
        let with = with_child_val(x, "dateFormat", "d", "w:", None);
        assert_eq!(with, r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="d"/></w:date>"#);
        assert_eq!(
            with_child_val(&with, "lid", "en-GB", "w:", Some("dateFormat")),
            r#"<w:date w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="d"/><w:lid w:val="en-GB"/></w:date>"#
        );
        assert_eq!(with_child_val("<w:date/>", "lid", "en-GB", "w:", Some("dateFormat")), r#"<w:date><w:lid w:val="en-GB"/></w:date>"#);
    }
}

/// What a date control stores in its data (`w:storeMappedDataAs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateStorage {
    /// The text it shows.
    Text,
    /// The date (`xsd:date`).
    Date,
    /// The date and time (`xsd:dateTime`), Word's default.
    DateTime,
}

impl DateStorage {
    fn xml(self) -> &'static str {
        match self {
            DateStorage::Text => "text",
            DateStorage::Date => "date",
            DateStorage::DateTime => "dateTime",
        }
    }
}

/// A date control's settings as Content Control Properties shows them: display format, locale, calendar and what
/// it stores (Word's defaults where the control doesn't say).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateSettings {
    pub format: String,
    pub locale: Option<String>,
    pub calendar: String,
    pub storage: DateStorage,
}

/// Date control `props`' settings; `None` for another kind.
pub fn date_settings(props: &ControlProps) -> Option<DateSettings> {
    let ControlKind::Date { format, lid } = &props.kind else { return None };
    let x = props.kind_xml.as_deref().unwrap_or_default();
    let val = |local: &str| {
        let at = x.find(&format!(":{local} ")).or_else(|| x.find(&format!("<{local} ")))?;
        let tag = &x[at..at + crate::docx::binding::tag_end(&x[at..])?];
        attr_value(tag, "val").map(|r| crate::docx::binding::unescape(&tag[r]))
    };
    let storage = match val("storeMappedDataAs").as_deref() {
        Some("text") => DateStorage::Text,
        Some("date") => DateStorage::Date,
        _ => DateStorage::DateTime,
    };
    Some(DateSettings {
        format: if format.is_empty() { "M/d/yyyy".into() } else { format.clone() },
        locale: lid.clone(),
        calendar: val("calendar").unwrap_or_else(|| "gregorian".into()),
        storage,
    })
}

/// The display formats a date control's Content Control Properties lists for `locale`.
pub fn date_formats(locale: &str) -> &'static [&'static str] {
    const EN_GB: &[&str] = &[
        "dd/MM/yyyy",
        "dddd, d MMMM yyyy",
        "d MMMM yyyy",
        "dd/MM/yy",
        "yyyy-MM-dd",
        "d-MMM-yy",
        "dd.MM.yyyy",
        "d MMM. yy",
        "MMMM yy",
        "MMM-yy",
        "dd/MM/yyyy HH:mm",
        "dd/MM/yyyy HH:mm:ss",
        "h:mm am/pm",
        "h:mm:ss am/pm",
        "HH:mm",
        "HH:mm:ss",
    ];
    const DE: &[&str] = &[
        "dd.MM.yyyy",
        "dddd, d. MMMM yyyy",
        "d. MMMM yyyy",
        "dd.MM.yy",
        "yyyy-MM-dd",
        "d. MMM. yy",
        "dd. MMM. yyyy",
        "MMMM yy",
        "MMM. yy",
        "dd.MM.yyyy HH:mm",
        "dd.MM.yyyy HH:mm:ss",
        "HH:mm",
        "HH:mm:ss",
    ];
    const FR: &[&str] = &[
        "dd/MM/yyyy",
        "dddd d MMMM yyyy",
        "d MMMM yyyy",
        "dd/MM/yy",
        "yyyy-MM-dd",
        "d-MMM-yy",
        "dd.MM.yy",
        "d MMM. yy",
        "MMMM yy",
        "MMM-yy",
        "dd/MM/yyyy HH:mm",
        "dd/MM/yyyy HH:mm:ss",
        "HH:mm",
        "HH:mm:ss",
    ];
    const ES: &[&str] = &[
        "dd/MM/yyyy",
        "dddd, d' de 'MMMM' de 'yyyy",
        "d' de 'MMMM' de 'yyyy",
        "dd/MM/yy",
        "yyyy-MM-dd",
        "d-MMM-yy",
        "MMMM' de 'yyyy",
        "MMM-yy",
        "dd/MM/yyyy H:mm",
        "dd/MM/yyyy H:mm:ss",
        "H:mm",
        "H:mm:ss",
    ];
    const JA: &[&str] = &[
        "yyyy/MM/dd",
        "yyyy'年'M'月'd'日'",
        "yyyy'年'M'月'd'日'dddd",
        "yy/MM/dd",
        "yyyy-MM-dd",
        "yyyy'年'M'月'",
        "M'月'd'日'",
        "yyyy/MM/dd H:mm",
        "H:mm",
        "H:mm:ss",
    ];
    const ZH: &[&str] = &[
        "yyyy/M/d",
        "yyyy'年'M'月'd'日'",
        "yyyy'年'M'月'd'日'dddd",
        "yyyy-MM-dd",
        "yyyy'年'M'月'",
        "M'月'd'日'",
        "yyyy/M/d H:mm",
        "H:mm",
        "H:mm:ss",
    ];
    match locale.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase().as_str() {
        "en" if !locale.eq_ignore_ascii_case("en-US") && !locale.eq_ignore_ascii_case("en") => EN_GB,
        "de" => DE,
        "fr" => FR,
        "es" => ES,
        "ja" => JA,
        "zh" => ZH,
        _ => crate::services::fields::DATE_PICTURES,
    }
}
