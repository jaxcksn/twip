//! Merge commands: fields and content controls the host inserts, changes and
//! removes, the external drop caret, and the `hit` query. Arguments are JSON; a refusal is the reply's
//! `answer.error`, a short fixed reason, and changes nothing.

use serde_json::{Value, json};

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

/// A refusal's reason.
type Refused = String;

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

fn bad(name: &str) -> Refused {
    format!("bad estimate: {name}")
}

fn text_arg<'a>(arg: &'a Value, name: &str) -> Result<Option<&'a str>, Refused> {
    match arg.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(bad(name)),
    }
}

/// A field code as Word stores it: one space on each side.
fn padded(code: &str) -> String {
    format!(" {} ", code.trim())
}

fn lock_of(v: &Value) -> Result<Lock, Refused> {
    match v.as_str() {
        Some("none") => Ok(Lock::None),
        Some("control") => Ok(Lock::ControlLocked),
        Some("content") => Ok(Lock::ContentLocked),
        Some("both") => Ok(Lock::Both),
        _ => Err(bad("lock")),
    }
}

impl Editor {
    /// Runs merge command `id` with its JSON argument; a refusal is answered with its reason.
    pub(super) fn merge_command(&mut self, id: &str, arg: &Value, fonts: &Fonts) -> Outcome {
        let r = match id {
            "field.insert" => self.field_insert(arg, fonts),
            "field.set" => self.field_set(arg, fonts),
            "element.remove" => self.element_remove(arg, false, fonts),
            "control.insert" => self.control_insert(arg, fonts),
            "control.set" => self.control_set(arg, fonts),
            "control.remove" => self.element_remove(arg, arg.get("keep_content").and_then(Value::as_bool).unwrap_or(false), fonts),
            "view.drop" => self.view_drop(arg, fonts),
            "field.labels" => self.field_labels(arg),
            "element.mark" => self.element_mark(arg),
            "element.select" => self.element_select(arg, fonts),
            "element.replace" => self.element_replace(arg, fonts),
            "region.wrap" => self.region_wrap(arg, fonts),
            "control.fill" => self.control_fill(arg, fonts),
            "data.write" => self.data_write(arg, fonts),
            "view.field-codes" => self.field_codes(arg),
            "field.new" => self.field_new(fonts),
            "field.update" => self.field_update(arg),
            "field.results" => self.field_results(arg, fonts),
            "field.lock" => self.field_lock(arg, true, fonts),
            "field.unlock" => self.field_lock(arg, false, fonts),
            "field.unlink" => self.field_unlink(arg, fonts),
            "data.remove" => self.data_remove(arg, fonts),
            "view.looks" => self.view_looks(arg),
            _ => Err("unknown command".to_string()),
        };
        match r {
            Ok((out, answer)) => {
                if answer.is_some() {
                    self.answer = answer;
                }
                out
            }
            Err(e) => {
                self.answer = Some(json!({ "error": e }));
                Outcome::Unchanged("refused")
            }
        }
    }

    /// The element `arg.id` names, and where it is.
    fn element_arg(&self, arg: &Value) -> Result<(ElementId, Where), Refused> {
        let id = arg.get("id").and_then(Value::as_str).ok_or_else(|| bad("id"))?;
        let e = elements::parse_id(id).ok_or_else(|| bad("id"))?;
        let w = self.locate(e).ok_or("no such element")?;
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

    /// Puts the selection where `arg.target` says: `"caret"` (or none) keeps it; `{ id }`
    /// is the caret right after that element; `{ at: { x, y, page? } }` is the caret at a hit-tested point. Refuses
    /// a place whose content can't be edited.
    fn target(&mut self, arg: &Value, fonts: &Fonts) -> Result<(), Refused> {
        match arg.get("target") {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s == "caret" => {}
            Some(t) if t.get("id").is_some() => {
                let (_, w) = self.element_arg(t)?;
                let p = match w {
                    Where::Inline { story, para, end, .. } => Position::text(story, para, end),
                    Where::Sym { story, para, at } => Position::text(story, para, Unit(at.0 + 1)),
                    Where::Blocks { .. } | Where::Rows { .. } => return Err(bad("target")),
                };
                self.selection = Selection::Caret(p);
            }
            Some(t) if t.get("at").is_some() => {
                let (p, repeat) = self.hit_at(&t["at"], fonts).ok_or_else(|| bad("target"))?;
                if repeat {
                    return Err("locked".into());
                }
                self.selection = Selection::Caret(p);
            }
            Some(_) => return Err(bad("target")),
        }
        match self.selection.focus() {
            Some(p) if !self.editable(&p) => Err("locked".into()),
            Some(_) => Ok(()),
            // A cell block (rows for a region): no text place to check.
            None if matches!(self.selection, Selection::Cells { .. }) => Ok(()),
            None => Err(bad("target")),
        }
    }

    /// The text position at a document point (`{ x, y, page? }`: page coordinates when `page` is given), and whether
    /// it is in a repeated header row's copy.
    fn hit_at(&mut self, at: &Value, fonts: &Fonts) -> Option<(Position, bool)> {
        let (x, y) = (at.get("x")?.as_f64()?, at.get("y")?.as_f64()?);
        let (x, y) = match at.get("page").and_then(Value::as_u64) {
            Some(page) => self.stack().to_doc(page as usize, x, y)?,
            None => (x, y),
        };
        self.hit_full(x, y, fonts)
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

    /// The `hit` query: the position at a point, the innermost element there, and whether it can be edited.
    /// The selection doesn't move.
    pub(super) fn hit_query(&mut self, args: &Value, fonts: &Fonts) -> Value {
        let Some((p, repeat)) = self.hit_at(args, fonts) else { return json!({ "error": "bad estimate: x" }) };
        let Some((para, offset)) = p.para_offset() else { return json!({ "error": "bad estimate: x" }) };
        let element = elements::context(&self.doc, &self.index, &Selection::Caret(p)).first().map(|(e, _)| elements::id_str(*e));
        json!({ "position": { "para": para.0, "offset": offset.0 }, "element": element, "editable": !repeat && self.editable(&p) })
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

    /// `element.mark`: marks on elements, drawn with their severity and message; `clear` takes the others
    /// away first. View only.
    fn element_mark(&mut self, arg: &Value) -> Result<(Outcome, Option<Value>), Refused> {
        let mut new = Vec::new();
        for m in arg.get("marks").and_then(Value::as_array).into_iter().flatten() {
            let id = m.get("id").and_then(Value::as_str).and_then(elements::parse_id).ok_or_else(|| bad("marks"))?;
            if self.index.element(id).is_none() {
                return Err("no such element".into());
            }
            let severity = m.get("severity").and_then(Value::as_str).and_then(super::view::Severity::parse).ok_or_else(|| bad("severity"))?;
            new.push(Mark { id, severity, message: m.get("message").and_then(Value::as_str).unwrap_or("").to_string() });
        }
        if arg.get("clear").and_then(Value::as_bool).unwrap_or(false) {
            self.marks.clear();
        }
        self.marks.retain(|m| !new.iter().any(|n| n.id == m.id));
        self.marks.extend(new);
        Ok((Outcome::Moved, None))
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

    /// `element.select`: the element selected (a header's or footer's entered for it), shown unless
    /// `reveal` is false.
    fn element_select(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let (_, w) = self.element_arg(arg)?;
        let (a, b) = self.element_range(&w).ok_or("no such element")?;
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
        if arg.get("reveal").and_then(Value::as_bool).unwrap_or(true) {
            self.ensure(&b, fonts);
            self.scroll_to_caret(fonts);
        }
        Ok((Outcome::Moved, None))
    }

    /// `element.replace`: every occurrence of plain text `find` (case-sensitive) in the codes, tags or
    /// aliases of the elements (`kind`, `ids` narrow them) becomes `replace`, the text around it kept; one step, and
    /// none when nothing matched. Answer: `{ changed }`, the elements changed.
    fn element_replace(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let find = text_arg(arg, "find")?.filter(|f| !f.is_empty()).ok_or_else(|| bad("find"))?.to_string();
        let replace = text_arg(arg, "replace")?.ok_or_else(|| bad("replace"))?.to_string();
        let within = text_arg(arg, "in")?.unwrap_or("code");
        if !matches!(within, "code" | "tag" | "alias") {
            return Err(bad("in"));
        }
        let kind = text_arg(arg, "kind")?;
        if kind.is_some_and(|k| !matches!(k, "field" | "control")) {
            return Err(bad("kind"));
        }
        let ids: Option<Vec<ElementId>> = match arg.get("ids") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_array()
                    .ok_or_else(|| bad("ids"))?
                    .iter()
                    .map(|x| x.as_str().and_then(elements::parse_id).ok_or_else(|| bad("ids")))
                    .collect::<Result<_, _>>()?,
            ),
        };
        let all = elements::enumerate(&self.doc, &self.index, &elements::Filter::default());
        let wanted =
            |e: &elements::Element| ids.as_ref().is_none_or(|v| v.contains(&e.id)) && kind.is_none_or(|k| (k == "field") == e.props.is_none());
        let mut tx = self.begin("Replace");
        let mut changed = 0usize;
        for e in all.iter().filter(|e| wanted(e)) {
            match (e.id, within) {
                (ElementId::Field(f), "code") => {
                    if e.code.as_deref().is_some_and(|c| c.contains(&find)) && self.replace_in_code(&mut tx, f, &find, &replace) {
                        changed += 1;
                    }
                }
                (ElementId::Control(c), "tag" | "alias") => {
                    let Some(mut control) = self.doc.controls.get(&c).cloned() else { continue };
                    let slot = if within == "tag" { &mut control.props.tag } else { &mut control.props.alias };
                    let Some(v) = slot.as_ref().filter(|v| v.contains(&find)) else { continue };
                    *slot = Some(v.replace(&find, &replace));
                    if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_ok() {
                        changed += 1;
                    }
                }
                _ => {}
            }
        }
        let answer = Some(json!({ "changed": changed }));
        if changed == 0 {
            self.abort(tx);
            return Ok((Outcome::Unchanged("nothing matched"), answer));
        }
        self.follow(&tx);
        Ok((self.commit(tx, None, fonts), answer))
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
    fn region_wrap(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        self.target(arg, fonts)?;
        let section = ControlProps {
            kind: ControlKind::RepeatingSection,
            tag: text_arg(arg, "tag")?.map(str::to_string),
            alias: text_arg(arg, "alias")?.map(str::to_string),
            ..Default::default()
        };
        let item = || Control::new(ControlProps { kind: ControlKind::RepeatingItem, ..Default::default() });
        let s = ControlId::next();
        let answer = Some(json!({ "id": elements::id_str(ElementId::Control(s)) }));
        // Where the new controls go in a member's list: after those around the whole selection, before the rest. A
        // control holding part of the selection and more besides crosses its edge.
        fn place(lists: &[&Vec<ControlId>], outside: &[&Vec<ControlId>]) -> Result<Vec<usize>, Refused> {
            let mut out = Vec::with_capacity(lists.len());
            let all = |x: &ControlId| lists.iter().all(|l| l.contains(x));
            // A control part of the selection holds, with members outside it too, crosses its edge.
            if lists.iter().flat_map(|l| l.iter()).any(|x| !all(x) && outside.iter().any(|l| l.contains(x))) {
                return Err("region boundary".into());
            }
            for l in lists {
                let mut k = 0;
                for (i, x) in l.iter().enumerate() {
                    if all(x) {
                        if i != k {
                            return Err("region boundary".into());
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
                (Some(_), Some(_)) => Err("two tables".into()),
                (None, None) => Ok(None),
                // One end in a table the other isn't in: whole blocks, if the ends are paragraph edges.
                _ => Ok(None),
            }
        };
        let mut tx = self.begin("Insert Repeating Section");
        let r = rows(self)?;
        if let Some((table, r0, r1)) = r {
            let t = self.index.table(&self.doc, table).ok_or("no such element")?;
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
            return Ok((self.commit(tx, None, fonts), answer));
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
            return Err("partial paragraph".into());
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
            return Err("partial paragraph".into());
        }
        // A range ending at the start of a later paragraph leaves that paragraph out.
        let last = match b.para_offset() {
            Some((pb, u)) if u == Unit(0) && sb.block > sa.block && self.index.para_slot(pb).is_some_and(|s| s.container == sa.container) => {
                sb.block - 1
            }
            _ => sb.block,
        };
        let blocks = self.index.blocks(&self.doc, sa.container).ok_or("no such element")?;
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
        Ok((self.commit(tx, None, fonts), answer))
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
        self.element_select(&json!({ "id": g.id, "reveal": false }), fonts).is_ok()
    }

    /// The `structure` query for one element: a region's rows as a table of just those rows, or a block
    /// control's blocks.
    pub(super) fn structure_query(&mut self, args: &Value) -> Value {
        let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(8) as usize;
        let Ok((_, w)) = self.element_arg(args) else { return json!({ "error": "no such element" }) };
        match w {
            Where::Rows { table, range } => {
                let Some(t) = self.index.table(&self.doc, table) else { return json!({ "error": "no such element" }) };
                let mut t = t.clone();
                t.rows = t.rows[range].to_vec();
                elements::structure(&[Block::Table(t)], depth)
            }
            Where::Blocks { container, range } => match self.index.blocks(&self.doc, container) {
                Some(b) => elements::structure(&b[range], depth),
                None => json!({ "error": "no such element" }),
            },
            _ => json!({ "error": "bad estimate: id" }),
        }
    }

    /// `view.looks`: looks by element, tag, tag prefix or default; `merge` adds to those set. View only.
    fn view_looks(&mut self, arg: &Value) -> Result<(Outcome, Option<Value>), Refused> {
        let mut new = Looks::default();
        for entry in arg.get("looks").and_then(Value::as_array).ok_or_else(|| bad("looks"))? {
            let l = &entry["look"];
            let look = if let Some(img) = l.get("image").and_then(Value::as_str) {
                let bytes = super::commands::base64(img).filter(|b| crate::docx::image::image_type(b).is_some()).ok_or_else(|| bad("image"))?;
                Look::Image(std::rc::Rc::new(bytes))
            } else {
                let fill = match l.get("fill").and_then(Value::as_str) {
                    Some(c) => {
                        let h = c.trim_start_matches('#');
                        let n = u32::from_str_radix(h, 16).map_err(|_| bad("fill"))?;
                        Some(if h.len() == 6 { 0x40_00_00_00 | n } else { n })
                    }
                    None => None,
                };
                let icon = l.get("icon_svg").and_then(Value::as_str).and_then(super::view::SvgIcon::parse).map(std::rc::Rc::new);
                Look::Box(super::view::PlaceholderLook { color: fill, label: l.get("label").and_then(Value::as_str).map(str::to_string), icon })
            };
            match &entry["match"] {
                Value::String(s) if s == "default" => new.default = Some(look),
                m if m.get("id").is_some() => {
                    let id = m["id"].as_str().and_then(elements::parse_id).ok_or_else(|| bad("match"))?;
                    new.by_element.insert(id, look);
                }
                m if m.get("tag").is_some() => new.by_tag.push((TagMatch::Exact(m["tag"].as_str().ok_or_else(|| bad("match"))?.to_string()), look)),
                m if m.get("prefix").is_some() => {
                    new.by_tag.push((TagMatch::Prefix(m["prefix"].as_str().ok_or_else(|| bad("match"))?.to_string()), look))
                }
                _ => return Err(bad("match")),
            }
        }
        if arg.get("merge").and_then(Value::as_bool).unwrap_or(false) {
            self.looks.by_element.extend(new.by_element);
            for (m, l) in new.by_tag {
                self.looks.by_tag.retain(|(x, _)| *x != m);
                self.looks.by_tag.push((m, l));
            }
            if new.default.is_some() {
                self.looks.default = new.default;
            }
        } else {
            self.looks = new;
        }
        Ok((Outcome::Moved, None))
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
            None if inside => match self.fields_here(&json!({}), false).first() {
                Some((f, _)) => ElementId::Field(*f),
                None => return false,
            },
            None => return false,
        };
        self.events.push(json!({ "type": "activate", "id": elements::id_str(id) }));
        true
    }

    /// `control.fill`: a picture control's image, or a text or rich-text control's content as text or a
    /// DOCX fragment (a single paragraph inline; blocks only into a block-level rich-text control). One step; the
    /// control stops showing its placeholder and keeps its properties.
    fn control_fill(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let (e, w) = self.element_arg(arg)?;
        let ElementId::Control(c) = e else { return Err(bad("id")) };
        let control = self.doc.controls.get(&c).cloned().ok_or("no such element")?;
        if !control.props.lock.editable() {
            return Err("locked".into());
        }
        let before = self.history.undo_depth();
        let label = "Fill Content Control";
        let out = if let Some(img) = text_arg(arg, "image")? {
            let data = super::commands::base64(img).ok_or_else(|| bad("image"))?;
            let (Some((pw, ph, dx, dy)), Some(_)) = (crate::docx::image::image_info(&data), crate::docx::image::image_type(&data)) else {
                return Err(bad("image"));
            };
            let size = (pw * 72.0 / if dx > 1.0 { dx } else { 96.0 }, ph * 72.0 / if dy > 1.0 { dy } else { 96.0 });
            let Where::Inline { para, first, last, .. } = w else { return Err(bad("id")) };
            let p = self.index.para(&self.doc, para).ok_or("no such element")?;
            let image = p.inlines[first..last]
                .iter()
                .find_map(|i| if let Inline::Picture(pic, _) = i { pic.image } else { None })
                .ok_or_else(|| bad("id"))?;
            self.fill_images(&[image], data, size, label, fonts)
        } else {
            let frag = match (text_arg(arg, "docx")?, text_arg(arg, "text")?) {
                (Some(d), _) => Some(self.fragment_of(d)?),
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
                        None => text_arg(arg, "text")?.unwrap_or("").to_string(),
                    };
                    if plain && text.contains('\n') && !matches!(control.props.kind, ControlKind::PlainText { multi_line: true }) {
                        return Err(bad("text"));
                    }
                    let p = self.index.para(&self.doc, *para).ok_or("no such element")?;
                    let run = p.inlines[*first..].iter().find_map(|i| i.run().cloned()).ok_or("no such element")?;
                    let mut tx = self.begin(label);
                    let with = if text.is_empty() { Vec::new() } else { vec![Inline::Text(text.replace('\n', "\u{b}"), run)] };
                    if self.step(&mut tx, Step::Text { para: *para, range: *start..*end, with, edges: Default::default() }).is_err() {
                        self.abort(tx);
                        return Err("no such element".into());
                    }
                    self.follow(&tx);
                    self.commit(tx, None, fonts)
                }
                (Where::Blocks { container, range }, f) if !plain => {
                    // Blocks into a block-level rich-text control: they take its place, members of it.
                    let blocks = self.index.blocks(&self.doc, *container).ok_or("no such element")?;
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
                            let first = blocks[range.clone()].iter().find_map(super::first_para_of).ok_or("no such element")?;
                            let mut p = self.split_template(first).ok_or("no such element")?;
                            // Its membership is the control's own, put on below.
                            p.controls.clear();
                            let run = p.mark.clone();
                            p.inlines = vec![Inline::Text(text_arg(arg, "text")?.unwrap_or("").to_string(), run)];
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
                        return Err("no such element".into());
                    }
                    self.follow(&tx);
                    self.commit(tx, None, fonts)
                }
                _ => return Err(bad("id")),
            }
        };
        if !matches!(out, Outcome::Changed(_)) {
            return Err("locked".into());
        }
        // No longer showing its placeholder: part of the same step.
        if let Some(mut k) = self.doc.controls.get(&c).cloned().filter(|k| k.props.showing_placeholder) {
            k.props.showing_placeholder = false;
            let mut tx = self.begin(label);
            let _ = self.step(&mut tx, Step::Control { id: c, control: Some(k) });
            self.commit(tx, None, fonts);
        }
        let n = self.history.undo_depth().saturating_sub(before);
        if n > 1 {
            self.history.merge_last(n, label);
        }
        Ok((out, None))
    }

    /// A DOCX fragment (base64): its body as a clipboard fragment, its runs resolved by its
    /// own styles (a paste keeps their look).
    fn fragment_of(&self, b64: &str) -> Result<super::clipboard::Fragment, Refused> {
        let bytes = super::commands::base64(b64).ok_or_else(|| bad("docx"))?;
        let doc = crate::docx::read_editable(&bytes).map_err(|_| bad("docx"))?;
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

    /// A text edit at the selection refused by a lock: answered `"locked"`.
    pub(super) fn refuse_locked(&mut self) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        if self.locked_edit(&a, &b) {
            self.answer = Some(json!({ "error": "locked" }));
            return true;
        }
        false
    }

    /// The data parts `arg` addresses: by root namespace, and `item_id` when given. Several for a
    /// namespace alone is `"ambiguous namespace"`.
    fn data_parts_at(&self, arg: &Value) -> Result<(String, Option<String>, Vec<usize>), Refused> {
        let ns = text_arg(arg, "namespace")?.ok_or_else(|| bad("namespace"))?.to_string();
        let item = text_arg(arg, "item_id")?.map(str::to_string);
        let at: Vec<usize> = self
            .doc
            .data_parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.namespace.as_deref() == Some(ns.as_str()) && item.as_ref().is_none_or(|i| *i == p.item_id))
            .map(|(k, _)| k)
            .collect();
        if item.is_none() && at.len() > 1 {
            return Err("ambiguous namespace".into());
        }
        Ok((ns, item, at))
    }

    /// `data.write`: a part's XML (well-formed, its root in `namespace`), replacing the part it addresses or
    /// added as a new one. One step, which can join a batch. Answer: `{ item_id }`.
    fn data_write(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let xml = text_arg(arg, "xml")?.ok_or_else(|| bad("xml"))?.to_string();
        let root_ns = {
            let doc = roxmltree::Document::parse(&xml).map_err(|_| "not well-formed".to_string())?;
            doc.root_element().tag_name().namespace().map(str::to_string)
        };
        let (ns, item, at) = self.data_parts_at(arg)?;
        if root_ns.as_deref() != Some(ns.as_str()) {
            return Err(bad("namespace"));
        }
        let part = match at.first() {
            Some(&k) => {
                let mut p = self.doc.data_parts[k].clone();
                if p.xml == xml {
                    return Ok((Outcome::Unchanged("the same XML"), Some(json!({ "item_id": p.item_id }))));
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
            return Err("no such element".into());
        }
        Ok((self.commit(tx, None, fonts), Some(json!({ "item_id": item_id }))))
    }

    /// `data.remove`: the part `arg` addresses goes. One step.
    fn data_remove(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let (_, _, at) = self.data_parts_at(arg)?;
        let &k = at.first().ok_or("no such element")?;
        let item_id = self.doc.data_parts[k].item_id.clone();
        let mut tx = self.begin("Remove Data");
        if self.step(&mut tx, Step::DataPart { item_id, part: None }).is_err() {
            self.abort(tx);
            return Err("no such element".into());
        }
        Ok((self.commit(tx, None, fonts), None))
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
    fn fields_here(&self, arg: &Value, nested: bool) -> Vec<(FieldId, ParaId)> {
        let all = self.all_fields();
        if let Some(ids) = arg.get("ids").and_then(Value::as_array) {
            let want: Vec<ElementId> = ids.iter().filter_map(|x| x.as_str().and_then(elements::parse_id)).collect();
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

    /// Sets fields' views without a step (a view setting): their paragraphs laid out again.
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
            self.revision += 1;
            for h in hints {
                self.layout.edit(crate::layout::Revision(self.revision), h);
            }
        }
    }

    /// `view.field-codes` and Alt+F9 / Shift+F9: every field (`toggle: "all"`), the fields here
    /// (`"here"`), or `ids` with `show`, in code or result view. The caret in a field goes to its code's end when it
    /// shows its code, to its start when it shows its result. View only.
    fn field_codes(&mut self, arg: &Value) -> Result<(Outcome, Option<Value>), Refused> {
        let all = self.all_fields();
        let here = self.fields_here(&json!({}), false);
        let (targets, view): (Vec<(FieldId, ParaId)>, FieldView) = match arg.get("toggle").and_then(Value::as_str) {
            Some("all") => {
                let codes = all.iter().all(|f| self.field_view(f.0, f.2) == Some(FieldView::Code));
                (all.iter().map(|f| (f.0, f.2)).collect(), if codes { FieldView::Result } else { FieldView::Code })
            }
            Some("here") => {
                let Some(first) = here.first() else { return Ok((Outcome::Unchanged("no field here"), None)) };
                let view = if self.field_view(first.0, first.1) == Some(FieldView::Code) { FieldView::Result } else { FieldView::Code };
                (here.clone(), view)
            }
            _ => {
                let show = arg.get("show").and_then(Value::as_bool).ok_or_else(|| bad("show"))?;
                let t = if arg.get("ids").is_some() { self.fields_here(arg, false) } else { all.iter().map(|f| (f.0, f.2)).collect() };
                (t, if show { FieldView::Code } else { FieldView::Result })
            }
        };
        self.set_views(&targets, view);
        // The caret in one of them: to its code's end, or its start.
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
        Ok((Outcome::Moved, None))
    }

    fn field_view(&self, f: FieldId, para: ParaId) -> Option<FieldView> {
        self.index.para(&self.doc, para)?.inlines.iter().find_map(|i| match i {
            Inline::FieldBegin { id, view, .. } if *id == f => Some(*view),
            _ => None,
        })
    }

    /// Ctrl+F9: a new field showing its code, two spaces with the caret between them; over a
    /// selection, the selected text is the code and the caret stays before the field; inside a code it nests.
    fn field_new(&mut self, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let selected = match self.selection.ordered(&self.index) {
            Some((a, b)) if !a.same_place(&b) => {
                let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return Err(bad("target")) };
                if pa != pb {
                    return Err("partial paragraph".into());
                }
                let p = self.index.para(&self.doc, pa).ok_or("no such element")?;
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
            return Err("locked".into());
        }
        if let Some(Where::Inline { story, para, start, .. }) = self.locate(ElementId::Field(id)) {
            let at = if selected.is_some() { start.0 } else { start.0 + 2 };
            self.selection = Selection::Caret(Position::text(story, para, Unit(at)));
        }
        Ok((out, Some(json!({ "id": elements::id_str(ElementId::Field(id)) }))))
    }

    /// F9 and `field.update`: the fields here (every field with Ctrl+A), innermost first, locked ones
    /// left, go to the host in one `field_update` event; those showing their code show their result again. Answer:
    /// `{ token }`.
    fn field_update(&mut self, arg: &Value) -> Result<(Outcome, Option<Value>), Refused> {
        let mut fields = self.fields_here(arg, true);
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
        self.set_views(&unlocked, FieldView::Result);
        self.update_token += 1;
        let token = self.update_token;
        let list: Vec<Value> = unlocked
            .iter()
            .filter_map(|(f, para)| {
                let p = self.index.para(&self.doc, *para)?;
                let k = p.inlines.iter().position(|i| matches!(i, Inline::FieldBegin { id, .. } if id == f))?;
                let s = crate::docx::fields::span_at(&p.inlines, k)?;
                Some(json!({ "id": elements::id_str(ElementId::Field(*f)), "code": crate::docx::fields::code_text(&p.inlines, s) }))
            })
            .collect();
        if !list.is_empty() {
            self.events.push(json!({ "type": "field_update", "token": token, "fields": list }));
        }
        Ok((Outcome::Moved, Some(json!({ "token": token }))))
    }

    /// `field.results`: the host's results for an update, as one step; fields gone since are reported.
    fn field_results(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let results = arg.get("results").and_then(Value::as_array).ok_or_else(|| bad("results"))?.clone();
        let mut tx = self.begin("Update Fields");
        let mut skipped = Vec::new();
        for r in &results {
            let Some(id) = r.get("id").and_then(Value::as_str) else { return Err(bad("results")) };
            let display = r.get("display").and_then(Value::as_str).unwrap_or("");
            let Some(ElementId::Field(f)) = elements::parse_id(id) else { return Err(bad("results")) };
            if !self.set_result(&mut tx, f, display) {
                skipped.push(json!(id));
            }
        }
        if tx.undo.is_empty() {
            self.abort(tx);
            return Ok((Outcome::Unchanged("no results"), Some(json!({ "skipped": skipped }))));
        }
        self.follow(&tx);
        Ok((self.commit(tx, None, fonts), Some(json!({ "skipped": skipped }))))
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
    fn field_lock(&mut self, arg: &Value, lock: bool, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let fields = self.fields_here(arg, true);
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
            return Ok((Outcome::Unchanged("no field to change"), None));
        }
        Ok((self.commit(tx, None, fonts), None))
    }

    /// Ctrl+Shift+F9 and `field.unlink`: each field here replaced by its result's text, one step.
    fn field_unlink(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let mut fields = self.fields_here(arg, false);
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
            return Ok((Outcome::Unchanged("no field here"), None));
        }
        self.follow(&tx);
        Ok((self.commit(tx, None, fonts), None))
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

    /// `field.labels`: the host's labels for empty fields, by code; `merge` adds to those it has. View only.
    fn field_labels(&mut self, arg: &Value) -> Result<(Outcome, Option<Value>), Refused> {
        let labels = arg.get("labels").and_then(Value::as_object).ok_or_else(|| bad("labels"))?;
        if !arg.get("merge").and_then(Value::as_bool).unwrap_or(false) {
            self.field_labels.clear();
        }
        for (code, label) in labels {
            self.field_labels.insert(code.trim().to_string(), label.as_str().ok_or_else(|| bad("labels"))?.to_string());
        }
        self.refresh_labels(None);
        Ok((Outcome::Moved, None))
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
            self.revision += 1;
            for h in hints {
                self.layout.edit(crate::layout::Revision(self.revision), h);
            }
        }
    }

    /// `view.drop`: the external drop caret at a point, or none. View only.
    fn view_drop(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        self.drop = match arg {
            Value::Null => None,
            at => self.hit_at(at, fonts).filter(|(p, repeat)| !repeat && self.editable(p)).map(|(p, _)| p),
        };
        Ok((Outcome::Moved, None))
    }

    /// `field.insert`: a field from its code and display text at the target; the caret after it, or at the
    /// start of its code when it shows its code.
    fn field_insert(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let code = text_arg(arg, "code")?.ok_or_else(|| bad("code"))?;
        let display = text_arg(arg, "display")?.unwrap_or("");
        let code_view = match text_arg(arg, "view")? {
            None | Some("result") => false,
            Some("code") => true,
            Some(_) => return Err(bad("view")),
        };
        self.target(arg, fonts)?;
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
        let Outcome::Changed(_) = out else { return Err("locked".into()) };
        if code_view && let Some(Where::Inline { story, para, start, .. }) = self.locate(ElementId::Field(id)) {
            self.selection = Selection::Caret(Position::text(story, para, Unit(start.0 + 1)));
        }
        Ok((out, Some(json!({ "id": elements::id_str(ElementId::Field(id)) }))))
    }

    /// `field.set`: a field's code and/or display text, as one step; what isn't given is kept.
    fn field_set(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let (e, w) = self.element_arg(arg)?;
        let (ElementId::Field(f), Where::Inline { para, first, last, .. }) = (e, w) else { return Err(bad("id")) };
        if arg.get("display_docx").is_some_and(|v| !v.is_null()) {
            // shortcut: rich results come with the keep-source fragment paste; plain text until then.
            return Err(bad("display_docx"));
        }
        let (code, display) = (text_arg(arg, "code")?, text_arg(arg, "display")?);
        if code.is_none() && display.is_none() {
            return Ok((Outcome::Unchanged("nothing to change"), None));
        }
        let p = self.index.para(&self.doc, para).ok_or("no such element")?.clone();
        let s = crate::docx::fields::span_at(&p.inlines, first).filter(|s| s.end == last).ok_or("no such element")?;
        let begin_run = p.inlines[first].run().cloned().ok_or("no such element")?;
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
                return Err("no such element".into());
            }
        }
        if let Some(c) = code {
            let run = run_in(first + 1, s.code_end()).unwrap_or(begin_run.clone());
            let range = Unit(at(first).0 + 1)..at(s.code_end());
            if self.step(&mut tx, Step::Text { para, range, with: vec![Inline::Text(padded(c), run)], edges: Default::default() }).is_err() {
                self.abort(tx);
                return Err("no such element".into());
            }
            self.refresh_field_kinds(&mut tx, para);
        }
        self.follow(&tx);
        Ok((self.commit(tx, None, fonts), None))
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

    /// `element.remove` and `control.remove`: a field with what it holds, or a control with its content; with
    /// `keep`, a control's markup only (its content stays where it is).
    fn element_remove(&mut self, arg: &Value, keep: bool, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let (e, w) = self.element_arg(arg)?;
        if let ElementId::Control(c) = e
            && self.doc.controls.get(&c).is_some_and(|k| !k.props.lock.deletable())
        {
            return Err("locked".into());
        }
        let mut tx = self.begin(if matches!(e, ElementId::Field(_)) { "Remove Field" } else { "Remove Content Control" });
        let ok = match (e, w) {
            (ElementId::Field(_), Where::Inline { para, start, end, .. }) => {
                self.step(&mut tx, Step::Text { para, range: start..end, with: Vec::new(), edges: Default::default() }).is_ok()
            }
            (ElementId::Control(c), Where::Inline { para, start, end, .. }) => {
                // The markers first (they take no units), then the content unless it is kept.
                let p = self.index.para(&self.doc, para).ok_or("no such element")?;
                let inlines: Vec<Inline> =
                    p.inlines.iter().filter(|i| !matches!(i, Inline::ControlStart(x) | Inline::ControlEnd(x) if *x == c)).cloned().collect();
                self.step(&mut tx, Step::Restyle { para, inlines }).is_ok()
                    && (keep
                        || start == end
                        || self.step(&mut tx, Step::Text { para, range: start..end, with: Vec::new(), edges: Default::default() }).is_ok())
            }
            (ElementId::Control(c), Where::Sym { para, at, .. }) => {
                let p = self.index.para(&self.doc, para).ok_or("no such element")?;
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
                let t = self.index.table(&self.doc, table).ok_or("no such element")?;
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
                    let slot = self.index.table_slot(table).ok_or("no such element")?;
                    self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() }).is_ok()
                } else {
                    self.step(&mut tx, Step::Rows { table, range, with: Vec::new(), grid: None }).is_ok()
                }
            }
            (ElementId::Control(c), Where::Blocks { container, range, .. }) => {
                let blocks = self.index.blocks(&self.doc, container).ok_or("no such element")?;
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
                        blocks.iter().rev().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None }).ok_or("no such element")?;
                    let mut p = self.split_template(last).ok_or("no such element")?;
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
            return Err("no such element".into());
        }
        self.follow(&tx);
        Ok((self.commit(tx, None, fonts), None))
    }

    /// `control.insert`: a control of `kind` at the target, its content its placeholder text (the text
    /// with `showing_placeholder`, no docPart); with `wrap` (rich text, group), around the selection instead.
    fn control_insert(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let props_arg = arg.get("props").cloned().unwrap_or(Value::Null);
        let ch = |name: &str, default: char| -> Result<char, Refused> {
            match props_arg.get(name) {
                None | Some(Value::Null) => Ok(default),
                Some(v) => v.as_str().and_then(|s| s.chars().next()).ok_or_else(|| bad(name)),
            }
        };
        let kind = match text_arg(arg, "kind")?.ok_or_else(|| bad("kind"))? {
            "plain_text" => ControlKind::PlainText { multi_line: props_arg.get("multi_line").and_then(Value::as_bool).unwrap_or(false) },
            "rich_text" => ControlKind::RichText,
            "picture" => ControlKind::Picture,
            "checkbox" => ControlKind::Checkbox { checked: ch("checked", '\u{2612}')?, unchecked: ch("unchecked", '\u{2610}')? },
            "date" => ControlKind::Date {
                format: props_arg.get("format").and_then(Value::as_str).unwrap_or("M/d/yyyy").to_string(),
                lid: props_arg.get("lid").and_then(Value::as_str).map(str::to_string),
            },
            "group" => ControlKind::Group,
            // shortcut: repeating sections are made by `region.wrap`, which keeps their items' rules.
            _ => return Err(bad("kind")),
        };
        let wrap = arg.get("wrap").and_then(Value::as_bool).unwrap_or(false);
        if wrap && !matches!(kind, ControlKind::RichText | ControlKind::Group) {
            return Err(bad("wrap"));
        }
        let placeholder = match text_arg(arg, "placeholder")? {
            Some(t) => Some(t.to_string()),
            None if wrap => None,
            None => match kind {
                ControlKind::PlainText { .. } | ControlKind::RichText => Some("Click or tap here to enter text.".into()),
                ControlKind::Date { .. } => Some("Click or tap to enter a date.".into()),
                _ => None,
            },
        };
        let mut props = ControlProps {
            kind: kind.clone(),
            tag: text_arg(arg, "tag")?.map(str::to_string),
            alias: text_arg(arg, "alias")?.map(str::to_string),
            lock: arg.get("lock").filter(|v| !v.is_null()).map(lock_of).transpose()?.unwrap_or_default(),
            binding: text_arg(arg, "binding")?.map(str::to_string),
            ..Default::default()
        };
        if matches!(kind, ControlKind::PlainText { .. } | ControlKind::RichText | ControlKind::Date { .. }) && placeholder.is_some() {
            props.showing_placeholder = true;
        }
        self.target(arg, fonts)?;
        let id = ControlId::next();
        let answer = Some(json!({ "id": elements::id_str(ElementId::Control(id)) }));
        if wrap {
            return self.control_wrap(id, props, fonts).map(|o| (o, answer));
        }
        let out = match kind {
            ControlKind::Checkbox { checked, unchecked } => {
                let face = if props_arg.get("value").and_then(Value::as_bool).unwrap_or(false) { checked } else { unchecked };
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
                self.doc.images.push(PICTURE_PLACEHOLDER.to_vec());
                let image = Some(self.doc.images.len() - 1);
                let picture = crate::docx::Picture { w: 144.0, h: 108.0, image, alt: "".into(), lock_aspect: true };
                let out = self.insert_inlines(
                    |run| vec![Inline::ControlStart(id), Inline::Picture(picture.clone(), run), Inline::ControlEnd(id)],
                    "Insert Content Control",
                    fonts,
                );
                if !matches!(out, Outcome::Changed(_)) {
                    self.doc.images.pop();
                }
                out
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
            Outcome::Changed(_) => Ok((out, answer)),
            _ => Err("locked".into()),
        }
    }

    /// A new control around the selection: inline markers around a range in one paragraph, or membership of whole
    /// paragraphs (and the tables between them) of one container.
    fn control_wrap(&mut self, id: ControlId, props: ControlProps, fonts: &Fonts) -> Result<Outcome, Refused> {
        let (a, b) = self.selection.ordered(&self.index).ok_or_else(|| bad("target"))?;
        if a.same_place(&b) {
            return Err(bad("target"));
        }
        let ((pa, oa), (pb, ob)) = (a.para_offset().ok_or("partial paragraph")?, b.para_offset().ok_or("partial paragraph")?);
        if pa == pb {
            let p = self.index.para(&self.doc, pa).ok_or("no such element")?;
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
        let (sa, sb) = (self.index.para_slot(pa).ok_or("no such element")?, self.index.para_slot(pb).ok_or("no such element")?);
        let end_len = self.index.para(&self.doc, pb).map_or(Unit(0), super::step::para_len);
        if oa != Unit(0) || sa.container != sb.container || (ob != Unit(0) && ob != end_len) {
            return Err("partial paragraph".into());
        }
        let last = if ob == Unit(0) { sb.block } else { sb.block + 1 };
        let blocks = self.index.blocks(&self.doc, sa.container).ok_or("no such element")?;
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

    /// `control.set`: a control's tag, title, lock, binding, kind settings and placeholder text, as one step.
    /// A control showing its placeholder shows the new one.
    fn control_set(&mut self, arg: &Value, fonts: &Fonts) -> Result<(Outcome, Option<Value>), Refused> {
        let (e, w) = self.element_arg(arg)?;
        let ElementId::Control(c) = e else { return Err(bad("id")) };
        let mut control = self.doc.controls.get(&c).cloned().ok_or("no such element")?;
        let p = &mut control.props;
        if arg.get("tag").is_some() {
            p.tag = text_arg(arg, "tag")?.map(str::to_string);
        }
        if arg.get("alias").is_some() {
            p.alias = text_arg(arg, "alias")?.map(str::to_string);
        }
        if let Some(l) = arg.get("lock") {
            p.lock = if l.is_null() { Lock::None } else { lock_of(l)? };
        }
        if arg.get("binding").is_some() {
            p.binding = text_arg(arg, "binding")?.map(str::to_string);
        }
        if let Some(props) = arg.get("props").filter(|v| v.is_object()) {
            match &mut p.kind {
                ControlKind::PlainText { multi_line } => {
                    if let Some(m) = props.get("multi_line") {
                        *multi_line = m.as_bool().ok_or_else(|| bad("props"))?;
                    }
                }
                ControlKind::Date { format, lid } => {
                    if let Some(f) = props.get("format") {
                        *format = f.as_str().ok_or_else(|| bad("props"))?.to_string();
                    }
                    if let Some(l) = props.get("lid") {
                        *lid = l.as_str().map(str::to_string);
                    }
                }
                ControlKind::Checkbox { checked, unchecked } => {
                    for (name, slot) in [("checked", checked), ("unchecked", unchecked)] {
                        if let Some(v) = props.get(name) {
                            *slot = v.as_str().and_then(|s| s.chars().next()).ok_or_else(|| bad("props"))?;
                        }
                    }
                }
                _ => return Err(bad("props")),
            }
            // The kind element as read no longer says what it is.
            p.kind_xml = None;
        }
        let placeholder = text_arg(arg, "placeholder")?.map(str::to_string);
        let showing = p.showing_placeholder;
        let mut tx = self.begin("Change Content Control");
        if self.step(&mut tx, Step::Control { id: c, control: Some(control) }).is_err() {
            self.abort(tx);
            return Err("no such element".into());
        }
        if let (Some(t), true, Where::Inline { para, start, end, first, .. }) = (placeholder, showing, w) {
            let run = self.index.para(&self.doc, para).and_then(|p| p.inlines[first..].iter().find_map(|i| i.run().cloned()));
            if let Some(run) = run {
                let with = if t.is_empty() { Vec::new() } else { vec![Inline::Text(t, run)] };
                let _ = self.step(&mut tx, Step::Text { para, range: start..end, with, edges: Default::default() });
            }
        }
        self.follow(&tx);
        Ok((self.commit(tx, None, fonts), None))
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
