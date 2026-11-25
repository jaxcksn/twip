//! Document protection: Restrict Editing and editable ranges

use std::collections::HashMap;
use std::rc::Rc;

use crate::api::command::{Command, CommandId, DeleteOptions, DeleteUnit, DocumentProtection, EditableRange, ProtectionKind as Kind};
use crate::api::error::{Error, ErrorCode};
use crate::api::value::Direction;
use crate::docx::review::{Password, Protection, ProtectionKind};
use crate::docx::{Inline, Opaque, Raw};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::step::{Edges, Step};

/// Who an editable range lets edit it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RangeEditor {
    /// A group (`w:edGrp`): `everyone`, or `administrators`, `contributors`, `editors`, `owners`, `current` and the
    /// like, which the host says the author is in.
    Group(String),
    /// One person (`w:ed`): a name or email address.
    User(String),
}

/// The `w:edGrp` values (`ST_EdGrp`).
const GROUPS: [&str; 7] = ["none", "everyone", "administrators", "contributors", "editors", "owners", "current"];

impl RangeEditor {
    /// The same editor, comparing names as Word does within each kind: a group never matches a person.
    pub fn same(&self, other: &RangeEditor) -> bool {
        match (self, other) {
            (Self::Group(a), Self::Group(b)) | (Self::User(a), Self::User(b)) => a.eq_ignore_ascii_case(b),
            _ => false,
        }
    }

    /// The editor as it is written: a group by its `w:edGrp` value, which Word writes in lower case; `None` for a
    /// group `w:edGrp` doesn't take.
    fn canonical(self) -> Option<RangeEditor> {
        match self {
            Self::Group(g) => GROUPS.iter().find(|n| n.eq_ignore_ascii_case(&g)).map(|n| Self::Group((*n).to_string())),
            user => Some(user),
        }
    }

    /// This editor lets `author`, a member of `groups`, edit: `everyone` lets all, a group its members, a person
    /// themselves.
    fn lets(&self, author: &str, groups: &[String]) -> bool {
        match self {
            Self::Group(g) => g.eq_ignore_ascii_case("everyone") || groups.iter().any(|x| x.eq_ignore_ascii_case(g)),
            Self::User(u) => u.eq_ignore_ascii_case(author),
        }
    }

    /// The API's reference to this editor.
    pub fn to_api(self) -> crate::api::value::RangeEditorRef {
        match self {
            RangeEditor::Group(g) => crate::api::value::RangeEditorRef::Group(g),
            RangeEditor::User(u) => crate::api::value::RangeEditorRef::User(u),
        }
    }

    /// The editor an API reference names.
    pub fn of_api(r: crate::api::value::RangeEditorRef) -> RangeEditor {
        match r {
            crate::api::value::RangeEditorRef::Group(g) => RangeEditor::Group(g),
            crate::api::value::RangeEditorRef::User(u) => RangeEditor::User(u),
        }
    }
}

/// An editable range as the Restrict Editing pane lists it: the permissions on one stretch of the document, each
/// for one editor.
#[derive(Clone, Debug, PartialEq)]
pub struct EditableRangeInfo {
    /// Its permissions' ids, one per editor.
    pub ids: Vec<String>,
    /// Who may edit it.
    pub editors: Vec<RangeEditor>,
    pub start: Position,
    pub end: Position,
}

/// An editing permission marker: `w:permStart` (its id, and who: `w:edGrp` or `w:ed`) or `w:permEnd`.
fn perm(i: &Inline) -> Option<(bool, String, Option<RangeEditor>)> {
    let Inline::Opaque(o) = i else { return None };
    let x = o.xml.0.as_deref()?;
    let attr = |k: &str| {
        let at = x.find(&format!(":{k}=\""))? + k.len() + 3;
        Some(crate::docx::binding::unescape(&x[at..at + x[at..].find('"')?]))
    };
    let start = x.contains("permStart");
    if !start && !x.contains("permEnd") {
        return None;
    }
    let who = if start { attr("edGrp").map(RangeEditor::Group).or_else(|| attr("ed").map(RangeEditor::User)) } else { None };
    Some((start, attr("id")?, who))
}

/// `who` (a `w:edGrp` or `w:ed`) lets `author` edit, a member of `groups`: everyone does, a group its members, a
/// person themselves.
fn may(who: &Option<RangeEditor>, author: &str, groups: &[String]) -> bool {
    who.as_ref().is_some_and(|w| w.lets(author, groups))
}

fn api_kind(k: ProtectionKind) -> Kind {
    match k {
        ProtectionKind::None => Kind::None,
        ProtectionKind::ReadOnly => Kind::ReadOnly,
        ProtectionKind::Comments => Kind::Comments,
        ProtectionKind::TrackedChanges => Kind::TrackedChanges,
        ProtectionKind::Forms => Kind::Forms,
    }
}

/// One editing permission: its id, who it lets edit (`w:edGrp` or `w:ed`), its start and end.
struct Perm {
    id: String,
    editor: Option<RangeEditor>,
    start: Position,
    end: Position,
}

/// The editable ranges: each permission and its order keys, in the order their ends come; and the permissions over
/// the same stretch as one, in document order, with their order keys.
#[derive(Default)]
pub(super) struct EditableRanges {
    ranges: Vec<Perm>,
    keys: Vec<(Vec<usize>, Vec<usize>)>,
    list: Vec<EditableRangeInfo>,
    list_keys: Vec<(Vec<usize>, Vec<usize>)>,
}

/// Where an edit falls, for the exceptions Restrict Editing makes.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Openings {
    /// In an editable range the author may edit.
    range: bool,
    /// Both ends in the same legacy form field or content control.
    form: bool,
    /// In one section filling-in-forms protection leaves open.
    section: bool,
}

/// The permission markers' `(start, id, who)` in `inlines`, each with its unit.
fn perms(inlines: &[Inline]) -> impl Iterator<Item = (Unit, (bool, String, Option<RangeEditor>))> + '_ {
    inlines.iter().scan(0u32, |unit, i| Some((Unit(std::mem::replace(unit, *unit + i.units() as u32)), i))).filter_map(|(u, i)| Some((u, perm(i)?)))
}

impl Editor {
    /// Every editable range, in document order. Found again only after a step that can move, add or remove a
    /// permission marker ([`Editor::moves_perms`]): the state asks on every change.
    pub(super) fn editable_ranges(&self) -> Rc<EditableRanges> {
        let mut cache = self.editable_cache.borrow_mut();
        if let Some((generation, r)) = cache.as_ref()
            && *generation == self.perm_generation
        {
            return r.clone();
        }
        // Each open `w:permStart` by id; a later start with the same id replaces it.
        let mut open: HashMap<String, (Position, Option<RangeEditor>)> = HashMap::new();
        let mut ranges = Vec::new();
        for para in self.paras_in_order() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let Some(slot) = self.index.para_slot(para) else { continue };
            let story = self.index.story(slot.container);
            let mut unit = 0u32;
            for i in &p.inlines {
                if let Some((start, id, who)) = perm(i) {
                    let at = Position::text(story, para, Unit(unit));
                    if start {
                        open.insert(id, (at, who));
                    } else if let Some((from, editor)) = open.remove(&id) {
                        ranges.push(Perm { id, editor, start: from, end: at });
                    }
                }
                unit += i.units() as u32;
            }
        }
        let keys: Vec<_> = ranges.iter().map(|r| (self.index.order_key(&r.start), self.index.order_key(&r.end))).collect();
        // Permissions over the same stretch, side by side
        let mut order: Vec<usize> = (0..ranges.len()).collect();
        order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
        let (mut list, mut list_keys): (Vec<EditableRangeInfo>, Vec<_>) = (Vec::new(), Vec::new());
        for k in order {
            let r = &ranges[k];
            if !list.last().is_some_and(|l| l.start == r.start && l.end == r.end) {
                list.push(EditableRangeInfo { ids: Vec::new(), editors: Vec::new(), start: r.start, end: r.end });
                list_keys.push(keys[k].clone());
            }
            let Some(l) = list.last_mut() else { continue };
            l.ids.push(r.id.clone());
            // One editor, however many permissions name them
            if let Some(e) = &r.editor
                && !l.editors.iter().any(|x| x.same(e))
            {
                l.editors.push(e.clone());
            }
        }
        let out = Rc::new(EditableRanges { ranges, keys, list, list_keys });
        *cache = Some((self.perm_generation, out.clone()));
        out
    }

    /// Whether a step, applied or undone, can move, add or remove a permission marker, so that the editable ranges are
    /// found again: typing in a paragraph with none can't.
    pub(super) fn moves_perms(&self, s: &Step) -> bool {
        match s {
            Step::Text { para, with, .. } | Step::Restyle { para, inlines: with } => {
                with.iter().any(|i| !matches!(i, Inline::Text(..)))
                    || self.index.para(&self.doc, *para).is_some_and(|p| perms(&p.inlines).next().is_some())
            }
            Step::Split { .. }
            | Step::Join { .. }
            | Step::Blocks { .. }
            | Step::Rows { .. }
            | Step::HeaderPart { .. }
            | Step::BuildingBlock { .. }
            | Step::Note { .. }
            | Step::Comment { .. }
            | Step::SectionBreak { .. }
            | Step::Sections { .. } => true,
            _ => false,
        }
    }

    /// `a..b` is in a range `author` may edit.
    fn in_editable_range_at(&self, a: &Position, b: &Position, author: &str) -> bool {
        let ranges = self.editable_ranges();
        if ranges.ranges.is_empty() {
            return false;
        }
        let (ka, kb) = (self.index.order_key(a), self.index.order_key(b));
        let own = || ranges.ranges.iter().zip(&ranges.keys).filter(|(r, _)| may(&r.editor, author, &self.editing_groups)).map(|(_, k)| k);
        if own().any(|(s, e)| *s <= ka && kb <= *e) {
            return true;
        }
        // Ranges that touch or overlap cover what is between them together
        let mut spans: Vec<&(Vec<usize>, Vec<usize>)> = own().collect();
        spans.sort_unstable_by(|x, y| x.0.cmp(&y.0));
        let (mut reach, mut covered) = (&ka, false);
        for (s, e) in spans {
            if s > reach {
                break;
            }
            if e >= reach {
                (reach, covered) = (e, true);
            }
        }
        covered && *reach >= kb
    }

    /// The selection is in a range `author` may edit.
    pub fn in_editable_range(&self, author: &str) -> bool {
        self.selection.ordered(&self.index).is_some_and(|(a, b)| self.in_editable_range_at(&a, &b, author))
    }

    /// Under enforced protection, text put in at the end of a range the author may edit goes before its `w:permEnd`
    /// (the end is in the range), unless a range they may edit starts after that end: then after its start, and
    /// before its end too when it ends there.
    pub(super) fn inside_editable_end(&self, s: Step) -> Step {
        let Step::Text { para, range, with, edges } = s else { return s };
        let enforced = self.doc.settings.protection.as_ref().is_some_and(|p| p.enforced && p.edit != ProtectionKind::None);
        let inserts = range.start == range.end && edges == Edges::default() && with.iter().any(|i| i.units() > 0);
        let under = (enforced && inserts).then(|| self.index.para(&self.doc, para)).flatten().map(|p| self.under_editable_end(p, range.start));
        Step::Text { para, range, with, edges: Edges { under: under.unwrap_or(edges.under), ..edges } }
    }

    /// How many of the markers at unit `u` of `p` text put in there goes before: the last place among them inside a
    /// range the author may edit (before the first `w:permEnd` of one, after a `w:permStart` of one, before the
    /// `w:permEnd` of a range started there).
    fn under_editable_end(&self, p: &crate::docx::Paragraph, u: Unit) -> u32 {
        let author: &str = &self.author.0;
        let (mut at, mut markers) = (0u32, 0u32);
        let mut inside: Option<u32> = None;
        let mut started: Option<String> = None;
        for i in &p.inlines {
            let w = i.units() as u32;
            if at > u.0 || (at == u.0 && w > 0) {
                break;
            }
            if at == u.0 && !matches!(i, Inline::Text(..)) {
                match perm(i) {
                    Some((false, id, _)) if started.as_deref() == Some(id.as_str()) => inside = Some(markers),
                    Some((false, id, _)) if inside.is_none() && may(&self.range_who(&id), author, &self.editing_groups) => inside = Some(markers),
                    Some((true, id, who)) if may(&who, author, &self.editing_groups) => {
                        inside = Some(markers + 1);
                        started = Some(id);
                    }
                    _ => {}
                }
                markers += 1;
            }
            at += w;
        }
        inside.map_or(0, |k| markers - k)
    }

    /// Who may edit range `id`: its `w:permStart`'s `w:edGrp` or `w:ed`. Not from [`Self::editable_ranges`], which is
    /// found once per revision and a transaction's steps move.
    fn range_who(&self, id: &str) -> Option<RangeEditor> {
        self.paras_in_order().into_iter().find_map(|para| {
            self.index.para(&self.doc, para)?.inlines.iter().find_map(|i| perm(i).filter(|(start, o, _)| *start && o == id).and_then(|(.., w)| w))
        })
    }

    /// `a` and `b` are in the same legacy form field or content control.
    fn in_form_field_at(&self, a: &Position, b: &Position) -> bool {
        let fields = |at: Position| -> Vec<crate::docx::ElementId> {
            super::elements::context(&self.doc, &self.index, &super::selection::Selection::Caret(at))
                .into_iter()
                .map(|(e, _)| e)
                .filter(|e| match e {
                    crate::docx::ElementId::Control(_) => true,
                    crate::docx::ElementId::Field(_) => self.element_context_of(*e, false).form.is_some(),
                })
                .collect()
        };
        let here = fields(*b);
        let there = (a != b).then(|| fields(*a));
        here.iter().any(|e| there.as_ref().is_none_or(|t| t.contains(e)))
    }

    /// Both ends of the selection are in the same legacy form field or content control.
    pub fn in_form_field(&self) -> bool {
        let Some(focus) = self.selection.focus() else { return false };
        self.in_form_field_at(&self.selection.anchor().unwrap_or(focus), &focus)
    }

    /// `a..b` is in one section filling-in-forms protection leaves open.
    fn in_unprotected_section_at(&self, a: &Position, b: &Position) -> bool {
        let open = |p: &Position| self.section_at(p).and_then(|k| self.doc.sections.get(k)).is_some_and(|s| !s.form_protected);
        open(a) && open(b) && self.section_at(a) == self.section_at(b)
    }

    /// The selection is in a section filling-in-forms protection leaves open.
    pub fn in_unprotected_section(&self) -> bool {
        self.selection.ordered(&self.index).is_some_and(|(a, b)| self.in_unprotected_section_at(&a, &b))
    }

    fn openings(&self, a: &Position, b: &Position) -> Openings {
        Openings {
            range: self.in_editable_range_at(a, b, &self.author.0),
            form: self.in_form_field_at(a, b),
            section: self.in_unprotected_section_at(a, b),
        }
    }

    /// [`Editor::openings`] of each place the selection edits, apart: every range of a discontiguous one (its own
    /// range alone for command `id` when that replaces only it), the table of each block of cells, each float selected
    /// together.
    fn selection_openings(&self, id: CommandId) -> Vec<Openings> {
        // A block of cells: each paragraph of each cell in it
        let cells = |s: &super::selection::Selection, out: &mut Vec<Openings>| {
            let super::selection::Selection::Cells { table, anchor, focus, .. } = *s else { return };
            let story = self.index.table_slot(table).map_or(super::position::StoryId::Body, |t| self.index.story(t.container));
            let pieces = self.cell_ranges(table, anchor, focus);
            if pieces.is_empty() {
                out.push(self.table_point(table).map_or_else(Openings::default, |p| self.openings(&p, &p)));
            }
            out.extend(pieces.into_iter().map(|(para, a, b)| self.openings(&Position::text(story, para, a), &Position::text(story, para, b))));
        };
        let mut out = Vec::new();
        match self.selection {
            s @ super::selection::Selection::Cells { .. } => cells(&s, &mut out),
            _ => out.push(self.selection_span().map_or_else(Openings::default, |(a, b)| self.openings(&a, &b))),
        }
        if !replaces_primary(id) {
            out.extend(self.other_ranges().iter().map(|(a, b)| self.openings(a, b)));
            for c in self.other_cells() {
                cells(c, &mut out);
            }
        }
        out.extend(self.selected_floats().into_iter().skip(1).map(|f| self.float_point(f).map_or_else(Openings::default, |p| self.openings(&p, &p))));
        out
    }

    /// Where float `f` is anchored.
    pub(super) fn float_point(&self, f: crate::layout::FloatId) -> Option<Position> {
        let para = self.index.float_anchor(f)?;
        Some(Position::text(self.index.story(self.index.para_slot(para)?.container), para, Unit(0)))
    }

    /// Where an edit of the selection falls: its ends, a selected object's anchor, selected cells' table.
    fn selection_span(&self) -> Option<(Position, Position)> {
        use super::selection::{ObjectRef, Selection};
        let at = |para: ParaId, u: Unit| self.index.para_slot(para).map(|s| Position::text(self.index.story(s.container), para, u));
        let p = match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => at(para, offset)?,
            Selection::Object(ObjectRef::Floating(f)) => self.float_point(f)?,
            Selection::Cells { table, .. } => self.table_point(table)?,
            s => return s.ordered(&self.index),
        };
        Some((p, p))
    }

    /// The start of a table's first cell.
    pub(super) fn table_point(&self, t: crate::layout::TableId) -> Option<Position> {
        let mut table = self.index.table(&self.doc, t)?;
        // Down first cells, as deep as the tables nest
        let para = loop {
            match table.rows.first()?.cells.first()?.blocks.first()? {
                crate::docx::Block::Para(p) => break ParaId(p.id),
                crate::docx::Block::Table(inner) => table = inner,
            }
        };
        Some(Position::text(self.index.story(self.index.para_slot(para)?.container), para, Unit(0)))
    }

    /// Whether a gesture's edit, command `id`'s at the selection, is kept from the document: read only, or document
    /// protection refuses it.
    pub(super) fn gesture_kept(&self, id: CommandId) -> bool {
        self.mode == super::Mode::ReadOnly || self.protection_refuses(id).is_some()
    }

    /// [`Editor::gesture_kept`] for a gesture on table `t`.
    pub(super) fn table_gesture_kept(&self, t: crate::layout::TableId, id: CommandId) -> bool {
        if self.mode == super::Mode::ReadOnly {
            return true;
        }
        let Some(p) = self.enforced_protection() else { return false };
        refusal(p, id, || [self.table_point(t).map_or_else(Openings::default, |at| self.openings(&at, &at))]).is_some()
    }

    /// Why a tool's edit of table `t`, command `id`'s, is refused where it lands (the table under the pointer, not the
    /// selection): read only, document protection there or a locked control around it.
    pub(super) fn table_edit_refusal(&self, t: crate::layout::TableId, id: CommandId) -> Option<Error> {
        match self.table_point(t) {
            Some(at) => self.edit_refusal_at(id, &at),
            None if self.mode == super::Mode::ReadOnly => Some(Error::new(ErrorCode::ReadOnly, "the document is read only")),
            None => refusal(self.enforced_protection()?, id, || [Openings::default()]),
        }
    }

    /// Why an edit, command `id`'s, at `at`, which isn't the selection, is refused: read only, document protection
    /// there or a locked control.
    pub(super) fn edit_refusal_at(&self, id: CommandId, at: &Position) -> Option<Error> {
        if self.mode == super::Mode::ReadOnly {
            return Some(Error::new(ErrorCode::ReadOnly, "the document is read only"));
        }
        self.protection_refuses_at(id, at, at).or_else(|| (self.kept(id, at, at) || !self.editable(at)).then(|| Error::unavailable("locked")))
    }

    /// The protection, when it's enforced.
    pub(super) fn enforced_protection(&self) -> Option<&Protection> {
        self.doc.settings.protection.as_ref().filter(|p| p.enforced)
    }

    /// Why document protection refuses `command` where it runs: the gate every command passes, against the selection
    /// it runs on (for a delete at a caret, what it removes).
    pub fn protection_refusal(&self, command: &Command) -> Option<Error> {
        let p = self.enforced_protection()?;
        if p.formatting
            && let Some(s) = applied_style(command)
            && self.doc.resolver.as_ref().and_then(|r| r.styles.get(&s.0)).is_some_and(|st| st.meta.locked)
        {
            return Some(Error::new(ErrorCode::NotAllowed, "the style is restricted"));
        }
        // The table tools' edits are refused where they land, not at the selection; Update All skips the tables it
        // may not change
        if matches!(command, Command::DrawTableStroke(_) | Command::EraseTableBorder(_) | Command::UpdateAllTablesOfContents(_)) {
            return None;
        }
        if p.formatting
            && references_styles(command).iter().any(|s| self.doc.resolver.as_ref().and_then(|r| r.styles.get(s)).is_some_and(|st| st.meta.locked))
        {
            return Some(Error::new(ErrorCode::NotAllowed, "the style is restricted"));
        }
        // A table of contents' update or removal changes the table, wherever the selection is
        let table = match command {
            Command::UpdateTableOfContents(u) => self.toc_by_index(u.index).ok().map(|t| (t, false)),
            Command::RemoveTableOfContents(r) => self.toc_by_index(r.index).ok().map(|t| (t, true)),
            _ => None,
        };
        if let Some((t, whole)) = table {
            return refusal(p, command.id(), || [self.toc_range(&t, whole).map_or_else(Openings::default, |(a, b)| self.openings(&a, &b))]);
        }
        // Space or X on a selected legacy check box fills it in
        let id = match command {
            Command::InsertText(t) if matches!(t.as_str(), " " | "x" | "X") && self.selected_form_check_box().is_some() => CommandId::FillFormField,
            _ => command.id(),
        };
        refusal(p, id, || match command {
            Command::Delete(d) if self.selection.is_collapsed() => {
                vec![self.delete_span(d).map_or_else(Openings::default, |(a, b)| self.openings(&a, &b))]
            }
            _ => self.selection_openings(command.id()),
        })
    }

    /// Why document protection refuses an edit that command `id` would make at `a..b`: a gesture's, where it lands.
    pub(super) fn protection_refuses_at(&self, id: CommandId, a: &Position, b: &Position) -> Option<Error> {
        refusal(self.enforced_protection()?, id, || [self.openings(a, b)])
    }

    /// Whether an edit that command `id` would make at `a..b`, which isn't the command's own (a correction, a
    /// gesture's), is kept from it: by document protection, a locked control or a shown sub-template's body.
    pub(super) fn kept(&self, id: CommandId, a: &Position, b: &Position) -> bool {
        self.protection_refuses_at(id, a, b).is_some() || self.locked_edit(a, b) || self.expanded_in(a, b).is_some()
    }

    /// Why document protection refuses an edit that command `id` would make at the selection: a dialog's or a
    /// gesture's, which isn't a command.
    pub fn protection_refuses(&self, id: CommandId) -> Option<Error> {
        refusal(self.enforced_protection()?, id, || self.selection_openings(id))
    }

    /// Whether document protection refuses each command at the selection, for the state: `None` when it's not
    /// enforced.
    pub(super) fn protection_gate(&self) -> Option<impl Fn(CommandId) -> bool + '_> {
        let p = self.enforced_protection()?;
        let all = self.selection_openings(CommandId::Delete);
        let own = if self.is_discontiguous() { self.selection_openings(CommandId::InsertText) } else { all.clone() };
        // The table tools, and the tables of contents' updates and removal, are refused where they land (finding the
        // table is a walk of the body, too much for each state)
        Some(move |id| {
            !matches!(
                id,
                CommandId::DrawTableStroke
                    | CommandId::EraseTableBorder
                    | CommandId::UpdateTableOfContents
                    | CommandId::UpdateAllTablesOfContents
                    | CommandId::RemoveTableOfContents
            ) && refusal(p, id, || if replaces_primary(id) { &own[..] } else { &all[..] }).is_some()
        })
    }

    /// What a delete at the caret removes, as [`Editor::delete`] finds it: the grapheme or word beside it in the
    /// markup view, or the paragraph mark it joins across, into the section beside or a table's first cell.
    fn delete_span(&self, d: &DeleteOptions) -> Option<(Position, Position)> {
        use crate::docx::Block;
        let caret = self.selection.focus()?;
        let Some((para, u)) = caret.para_offset() else { return Some((caret, caret)) };
        let p = self.index.para(&self.doc, para)?;
        let t = super::text::UnitText::in_review(p, &self.doc.review);
        let word = d.unit == Some(DeleteUnit::Word);
        let slot = self.index.para_slot(para)?;
        let blocks = self.index.blocks(&self.doc, slot.container).map_or(&[][..], Vec::as_slice);
        let section = match slot.container {
            super::index::Container::Section(k) => Some(k),
            _ => None,
        };
        let at = |para, u| Position::text(caret.story, para, u);
        Some(match d.direction {
            Direction::Backward if u > Unit(0) => (at(para, if word { t.prev_word(u) } else { t.prev_grapheme(u) }), caret),
            Direction::Backward => {
                let before = match slot.block.checked_sub(1) {
                    Some(k) => blocks.get(k),
                    None => section.and_then(|k| self.doc.sections.get(k.checked_sub(1)?)?.blocks.last()),
                };
                match before {
                    Some(Block::Para(q)) => (at(ParaId(q.id), super::step::para_len(q)), caret),
                    _ => (caret, caret),
                }
            }
            Direction::Forward if u < t.len() => (caret, at(para, if word { t.next_word(u) } else { t.next_grapheme(u) })),
            Direction::Forward => {
                let after = blocks.get(slot.block + 1).or_else(|| section.and_then(|k| self.doc.sections.get(k + 1)?.blocks.first()));
                let first = |t: &crate::docx::Table| t.rows.first()?.cells.first()?.blocks.iter().find_map(super::first_para_of);
                match after {
                    Some(Block::Para(q)) => (caret, at(ParaId(q.id), Unit(0))),
                    Some(Block::Table(t)) => first(t).map_or((caret, caret), |q| (caret, at(q, Unit(0)))),
                    None => (caret, caret),
                }
            }
        })
    }

    pub fn protection_state(&self, author: &str) -> Option<crate::api::state::ProtectionState> {
        let p = self.doc.settings.protection.as_ref()?;
        Some(crate::api::state::ProtectionState {
            kind: api_kind(p.edit),
            enforced: p.enforced,
            formatting: p.formatting,
            password: p.password.is_some(),
            in_editable_range: self.in_editable_range(author),
            unprotected_sections: self.unprotected_sections().collect(),
            exceptions_allowed: restriction_options(api_kind(p.edit)).exceptions,
            sections_allowed: restriction_options(api_kind(p.edit)).sections,
            auto_format_override: self.doc.settings.auto_format_override,
            block_theme_switching: self.doc.settings.style_lock_theme,
            block_style_set_switching: self.doc.settings.style_lock_style_set,
            editors_here: self.editors_here().into_iter().map(RangeEditor::to_api).collect(),
            range_count: self.editable_ranges().list.len() as u32,
        })
    }

    /// The sections filling-in-forms protection leaves open.
    fn unprotected_sections(&self) -> impl Iterator<Item = u32> + '_ {
        self.doc.sections.iter().enumerate().filter(|(_, s)| !s.form_protected).map(|(k, _)| k as u32)
    }

    /// Sets the formatting restrictions' settings (what is `None` stays): AutoFormat may override them, the theme
    /// and the Quick Style Set can't change. Refused while the protection is enforced. One undo step.
    pub fn set_formatting_restrictions(
        &mut self,
        auto_format_override: Option<bool>,
        lock_theme: Option<bool>,
        lock_style_set: Option<bool>,
        fonts: &Fonts,
    ) -> Outcome {
        if self.enforced_protection().is_some() {
            return Outcome::Refused(Error::new(ErrorCode::NotAllowed, "stop the protection before changing it"));
        }
        let mut p = super::step::PageSettings::of(&self.doc.settings);
        let was = p;
        p.auto_format_override = auto_format_override.unwrap_or(p.auto_format_override);
        p.style_lock_theme = lock_theme.unwrap_or(p.style_lock_theme);
        p.style_lock_style_set = lock_style_set.unwrap_or(p.style_lock_style_set);
        if p == was {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin("Formatting Restrictions");
        if self.step(&mut tx, Step::PageSettings(p)).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the setting can't change");
        }
        self.commit(tx, None, fonts)
    }

    /// The styles Word's Formatting Restrictions leave allowed with Recommended Minimum: each kind's default style,
    /// the styles the Recommended list shows (not hidden), and those the document uses; by id, sorted.
    pub fn recommended_minimum_styles(&self) -> Vec<String> {
        use crate::docx::Block;
        let Some(res) = self.doc.resolver.as_ref() else { return Vec::new() };
        fn walk(blocks: &[Block], used: &mut std::collections::HashSet<String>) {
            for b in blocks {
                match b {
                    Block::Para(p) => {
                        used.extend(p.style.style_id.clone());
                        for i in &p.inlines {
                            if let Some(id) = i.run().and_then(|r| r.style_id.clone()) {
                                used.insert(id);
                            }
                            if let Some((_, tb)) = i.story() {
                                walk(&tb.blocks, used);
                            }
                        }
                    }
                    Block::Table(t) => {
                        used.extend(t.style_id.clone());
                        t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, used));
                    }
                }
            }
        }
        let mut used = std::collections::HashSet::new();
        let d = &self.doc;
        for b in d.sections.iter().map(|s| &s.blocks).chain(d.stories.values()).chain(d.footnotes.values()).chain(d.endnotes.values()) {
            walk(b, &mut used);
        }
        let mut out: Vec<String> =
            res.styles.by_id.values().filter(|st| st.default || !st.hidden() || used.contains(&st.id)).map(|st| st.id.clone()).collect();
        out.sort_unstable();
        out
    }

    /// Enforced formatting restrictions keep the theme (`true`) or the Quick Style Set from changing.
    pub fn style_switch_blocked(&self, theme: bool) -> bool {
        let s = &self.doc.settings;
        self.enforced_protection().is_some_and(|p| p.formatting) && if theme { s.style_lock_theme } else { s.style_lock_style_set }
    }

    /// [`Editor::kept`] for the formatting an AutoFormat as you type change makes at `a..b` as command `id` would:
    /// formatting restrictions don't keep it from the document when it lets AutoFormat override them.
    pub(super) fn auto_format_kept(&self, id: CommandId, a: &Position, b: &Position) -> bool {
        let Some(p) = self.enforced_protection().filter(|p| p.formatting && self.doc.settings.auto_format_override) else {
            return self.kept(id, a, b);
        };
        let open = Protection { formatting: false, ..p.clone() };
        refusal(&open, id, || [self.openings(a, b)]).is_some() || self.locked_edit(a, b) || self.expanded_in(a, b).is_some()
    }

    /// Starts enforcing the protection set, as Word's Start Enforcement Protection does: the password typed twice
    /// (none when both are empty), refused when the two differ or when there is nothing to enforce.
    pub fn start_protection(&mut self, password: &str, confirm: &str, fonts: &Fonts) -> Outcome {
        if password != confirm {
            return Outcome::invalid("the passwords don't match");
        }
        let restricts = self.doc.settings.protection.as_ref().is_some_and(|p| p.edit != ProtectionKind::None || p.formatting);
        if !restricts {
            return Outcome::unavailable("no editing or formatting restriction to enforce");
        }
        let d = DocumentProtection { enforce: Some(true), password: Some(password.to_string()), ..DocumentProtection::default() };
        self.set_document_protection(d, fonts)
    }

    pub(in crate::editing) fn set_document_protection(&mut self, d: DocumentProtection, fonts: &Fonts) -> Outcome {
        let was = self.doc.settings.protection.clone();
        let mut p = was.clone().unwrap_or_default();
        let enforced = p.enforced;
        if enforced && d.enforce != Some(false) && (d.kind.is_some() || d.formatting.is_some() || d.unprotected_sections.is_some()) {
            return Outcome::Refused(Error::new(ErrorCode::NotAllowed, "stop the protection before changing it"));
        }
        if d.enforce == Some(false) && enforced {
            if let Some(pw) = &p.password
                && !pw.matches(d.password.as_deref().unwrap_or_default())
            {
                return Outcome::Refused(Error::new(ErrorCode::NotAllowed, "the password is wrong"));
            }
            p.password = None;
        }
        if let Some(k) = d.kind {
            p.edit = match k {
                Kind::None => ProtectionKind::None,
                Kind::ReadOnly => ProtectionKind::ReadOnly,
                Kind::Comments => ProtectionKind::Comments,
                Kind::TrackedChanges => ProtectionKind::TrackedChanges,
                Kind::Forms => ProtectionKind::Forms,
            };
        }
        if let Some(f) = d.formatting {
            p.formatting = f;
        }
        if d.enforce == Some(true) && !enforced {
            p.password = d.password.as_deref().filter(|s| !s.is_empty()).map(Password::new);
        }
        if let Some(e) = d.enforce {
            p.enforced = e;
        }
        let now = (p.edit != ProtectionKind::None || p.formatting || p.enforced).then_some(p);
        let sections: Vec<(usize, crate::docx::SectionPage)> = d.unprotected_sections.map_or_else(Vec::new, |open| {
            let open: std::collections::HashSet<u32> = open.into_iter().collect();
            self.doc
                .sections
                .iter()
                .enumerate()
                .filter(|(k, s)| s.form_protected == open.contains(&(*k as u32)))
                .map(|(k, s)| (k, crate::docx::SectionPage { form_protected: !s.form_protected, ..s.page() }))
                .collect()
        });
        if sections.is_empty()
            && now.as_ref().map(|p| (p.edit, p.enforced, p.formatting, p.password.clone()))
                == was.as_ref().map(|p| (p.edit, p.enforced, p.formatting, p.password.clone()))
        {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin("Restrict Editing");
        for (section, page) in sections {
            let change = self.doc.sections[section].change.clone();
            if self.step(&mut tx, Step::SectionPage { section, page: Box::new(page), change }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("no section");
            }
        }
        if now.as_ref().map(|p| (p.edit, p.enforced, p.formatting, &p.password))
            != was.as_ref().map(|p| (p.edit, p.enforced, p.formatting, &p.password))
            && self.step(&mut tx, Step::Protection(now.map(Box::new))).is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("protection failed");
        }
        self.commit(tx, None, fonts)
    }

    /// The selection's ranges, each of a discontiguous one (its blocks of cells aside), in document order.
    fn text_ranges(&mut self) -> Vec<(Position, Position)> {
        if self.is_discontiguous() { self.selection_ranges() } else { self.selection.ordered(&self.index).into_iter().collect() }
    }

    /// An editable range on each range of the selection, each with an id of its own, in one step.
    pub(in crate::editing) fn add_editable_range(&mut self, r: EditableRange, fonts: &Fonts) -> Outcome {
        let who = match (r.editor, r.group) {
            (Some(_), Some(_)) => return Outcome::invalid("an editor or a group"),
            (Some(u), None) => RangeEditor::User(u),
            (None, g) => RangeEditor::Group(g.unwrap_or_else(|| "everyone".into())),
        };
        let Some(who) = who.canonical() else { return Outcome::invalid("not a group Word's editable ranges know") };
        let ranges = self.text_ranges();
        self.add_permission(who, &ranges, fonts)
    }

    /// `setDocumentProtection`: the formatting restrictions' settings and the protection, in one step; starting
    /// enforcement checks the password typed twice and that something is restricted.
    pub(in crate::editing) fn document_protection(&mut self, mut d: DocumentProtection, fonts: &Fonts) -> Outcome {
        let confirm = d.confirm_password.take();
        let restrictions = (d.auto_format_override.take(), d.block_theme_switching.take(), d.block_style_set_switching.take());
        let was = self.doc.settings.protection.as_ref();
        let enforced = was.is_some_and(|p| p.enforced);
        if d.enforce == Some(true) && !enforced {
            if confirm.as_ref().is_some_and(|c| *c != d.password.clone().unwrap_or_default()) {
                return Outcome::invalid("the passwords don't match");
            }
            let edits = d.kind.map_or_else(|| was.is_some_and(|p| p.edit != ProtectionKind::None), |k| k != Kind::None);
            if !edits && !d.formatting.unwrap_or_else(|| was.is_some_and(|p| p.formatting)) {
                return Outcome::unavailable("no editing or formatting restriction to enforce");
            }
        }
        if restrictions == (None, None, None) {
            return self.set_document_protection(d, fonts);
        }
        let rest = d != DocumentProtection::default();
        let stopping = d.enforce == Some(false) && enforced;
        self.joined("Restrict Editing", fonts, |e| {
            let protect = |e: &mut Editor| if rest { e.set_document_protection(d, fonts) } else { Outcome::Unchanged("already so") };
            let (first, second) = if stopping {
                let p = protect(e);
                if matches!(p, Outcome::Refused(_)) {
                    return p;
                }
                (p, e.set_formatting_restrictions(restrictions.0, restrictions.1, restrictions.2, fonts))
            } else {
                let r = e.set_formatting_restrictions(restrictions.0, restrictions.1, restrictions.2, fonts);
                if matches!(r, Outcome::Refused(_)) {
                    return r;
                }
                (r, protect(e))
            };
            match (first, second) {
                (_, o @ Outcome::Refused(_)) => o,
                (o @ Outcome::Changed(_), _) | (_, o) => o,
            }
        })
    }

    /// Adds a permission for `who` over each of `ranges`.
    fn add_permission(&mut self, who: RangeEditor, ranges: &[(Position, Position)], fonts: &Fonts) -> Outcome {
        if ranges.is_empty() {
            return Outcome::Unchanged("no selection");
        }
        let Some(ends) = ranges.iter().map(|(a, b)| Some((a.para_offset()?, b.para_offset()?))).collect::<Option<Vec<_>>>() else {
            return Outcome::unavailable("no text range");
        };
        let used: std::collections::HashSet<i64> = self
            .index
            .para_ids()
            .filter_map(|p| self.index.para(&self.doc, p))
            .flat_map(|p| p.inlines.iter().filter_map(perm))
            .filter_map(|(_, id, _)| id.parse().ok())
            .collect();
        // Ids Word reads (a signed 32-bit number): on from the greatest, else the first unused
        const MAX: i64 = i32::MAX as i64;
        let n = ends.len() as i64;
        let top = used.iter().copied().filter(|id| (0..=MAX).contains(id)).max().unwrap_or(0);
        let fresh: Vec<i64> =
            if top + n <= MAX { (top + 1..=top + n).collect() } else { (1..=MAX).filter(|id| !used.contains(id)).take(ends.len()).collect() };
        if fresh.len() < ends.len() {
            return Outcome::unavailable("no free permission id");
        }
        let w = self.cx().w;
        let esc = crate::docx::write::props::esc;
        let who = match &who {
            RangeEditor::Group(g) => format!("{w}edGrp=\"{}\"", esc(g)),
            RangeEditor::User(u) => format!("{w}ed=\"{}\"", esc(u)),
        };
        let marker = |x: String| Inline::Opaque(Rc::new(Opaque { xml: Raw(Some(Rc::from(x.as_str()))), field: None, src: None, run: None }));
        let mut tx = self.begin("Editable Range");
        // Ranges, last first
        for (k, &((pa, oa), (pb, ob))) in ends.iter().enumerate().rev() {
            let id = fresh[k];
            let (start, end) = (marker(format!("<{w}permStart {w}id=\"{id}\" {who}/>")), marker(format!("<{w}permEnd {w}id=\"{id}\"/>")));
            let steps = if pa == pb && oa == ob {
                vec![Step::Text { para: pa, range: oa..oa, with: vec![start, end], edges: Edges::default() }]
            } else {
                vec![
                    Step::Text { para: pb, range: ob..ob, with: vec![end], edges: Edges::default() },
                    Step::Text { para: pa, range: oa..oa, with: vec![start], edges: Edges::default() },
                ]
            };
            for s in steps {
                if self.step(&mut tx, s).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("stale selection");
                }
            }
        }
        self.commit(tx, None, fonts)
    }

    /// The groups the author is in, which editable ranges for them let edit (the host knows them; everyone is
    /// always one).
    pub fn set_editing_groups(&mut self, groups: Vec<String>) {
        self.editing_groups = groups;
    }

    /// The editable ranges, in document order: those over the same stretch as one, with each of their editors.
    pub fn editable_range_list(&self) -> Vec<EditableRangeInfo> {
        self.editable_ranges().list.clone()
    }

    /// Who may edit the editable ranges holding the selection, as the Restrict Editing pane ticks them.
    pub fn editors_here(&self) -> Vec<RangeEditor> {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Vec::new() };
        self.editors_over(&a, &b)
    }

    /// Who may edit the editable ranges holding `a..b`.
    fn editors_over(&self, a: &Position, b: &Position) -> Vec<RangeEditor> {
        let ranges = self.editable_ranges();
        if ranges.list.is_empty() {
            return Vec::new();
        }
        let (ka, kb) = (self.index.order_key(a), self.index.order_key(b));
        let mut out: Vec<RangeEditor> = Vec::new();
        for (r, (s, e)) in ranges.list.iter().zip(&ranges.list_keys) {
            if *s <= ka && kb <= *e {
                for e in &r.editors {
                    if !out.iter().any(|o| o.same(e)) {
                        out.push(e.clone());
                    }
                }
            }
        }
        out
    }

    /// The editable ranges the author may edit.
    fn own_ranges(&self) -> Vec<(Position, Position)> {
        let (author, groups) = (&*self.author.0, &self.editing_groups);
        self.editable_ranges().list.iter().filter(|r| r.editors.iter().any(|e| e.lets(author, groups))).map(|r| (r.start, r.end)).collect()
    }

    /// Selects the next (or previous) editable range the author may edit after the selection, round to the first
    /// (last): Word's Find Next Region I Can Edit.
    pub fn select_next_editable_range(&mut self, forward: bool) -> Outcome {
        let ranges = self.own_ranges();
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no selection") };
        let pick = if forward {
            ranges.iter().find(|r| self.index.cmp(&r.0, &b).is_ge() && !(r.0 == a && r.1 == b)).or(ranges.first())
        } else {
            ranges.iter().rev().find(|r| self.index.cmp(&r.1, &a).is_le() && !(r.0 == a && r.1 == b)).or(ranges.last())
        };
        let Some(&(s, e)) = pick else { return Outcome::Unchanged("no region you can edit") };
        let before = self.selection;
        self.multi = super::multi::Multi::default();
        self.selection = super::selection::Selection::range(s, e, super::selection::Granularity::Char);
        if self.selection == before { Outcome::Unchanged("already there") } else { Outcome::Moved }
    }

    /// Selects every editable range the author may edit, of the selection's story: Word's Show All Regions I Can
    /// Edit.
    pub fn select_all_editable_ranges(&mut self) -> Outcome {
        let story = self.selection.focus().map_or(super::position::StoryId::Body, |p| p.story);
        let ranges: Vec<(Position, Position)> = self.own_ranges().into_iter().filter(|r| r.0.story == story && !r.0.same_place(&r.1)).collect();
        if ranges.is_empty() {
            return Outcome::Unchanged("no region you can edit");
        }
        self.select_ranges(&ranges)
    }

    /// What Word's Highlight the regions I can edit draws on `pages`: each editable range the author may edit
    /// shaded, one band per line part, and bracketed at its ends, in page points.
    pub fn editable_region_views(&mut self, pages: std::ops::Range<usize>) -> (Vec<(usize, super::view::Band)>, Vec<super::view::FormattingMark>) {
        use super::view::{FormattingMark, MarkKind};
        let (mut bands, mut marks) = (Vec::new(), Vec::new());
        let near = self.near;
        for (a, b) in self.own_ranges() {
            let rects = self.range_rects(a, b, pages.clone());
            // A bracket where the range's end is: not at the edge of the pages asked for
            let (first, last) = (rects.first(), rects.last());
            let (start_page, end_page) = (first.and_then(|_| self.page_showing(a)), last.and_then(|_| self.page_showing(b)));
            if let Some(first) = first.filter(|f| start_page == Some(f.0)) {
                let s = first.1.rect;
                marks.push(FormattingMark {
                    page: first.0,
                    kind: MarkKind::BookmarkStart,
                    at: super::view::Band { rect: [s[0], s[1], s[0], s[3]], turn: first.1.turn },
                });
            }
            if let Some(last) = last.filter(|l| end_page == Some(l.0)) {
                let e = last.1.rect;
                marks.push(FormattingMark {
                    page: last.0,
                    kind: MarkKind::BookmarkEnd,
                    at: super::view::Band { rect: [e[2], e[1], e[2], e[3]], turn: last.1.turn },
                });
            }
            bands.extend(rects);
        }
        self.near = near;
        (bands, marks)
    }

    /// Lets `editor` edit the selection (`allowed`), adding a permission for them over it, or stops them editing the
    /// editable ranges over it, as ticking or clearing them in the Restrict Editing pane does. One undo step.
    pub fn set_range_editor(&mut self, editor: RangeEditor, allowed: bool, fonts: &Fonts) -> Outcome {
        // The gates of the commands it stands for: enforced protection keeps its exceptions as they are
        let like = if allowed { Command::AddEditableRange(EditableRange::default()) } else { Command::RemoveEditableRanges };
        if let Some(out) = self.refuse_like(&like) {
            return out;
        }
        let ranges = self.text_ranges();
        if allowed {
            let Some(editor) = editor.canonical() else { return Outcome::invalid("not a group Word's editable ranges know") };
            // Only the spans this editor can't edit yet get a permission.
            let missing: Vec<(Position, Position)> =
                ranges.iter().filter(|(a, b)| !self.editors_over(a, b).iter().any(|e| e.same(&editor))).copied().collect();
            if missing.is_empty() {
                return Outcome::Unchanged("already so");
            }
            return self.add_permission(editor, &missing, fonts);
        }
        // The editor's own permissions reaching the selection
        let theirs: std::collections::HashSet<String> = self
            .editable_ranges()
            .ranges
            .iter()
            .filter(|r| r.editor.as_ref().is_some_and(|e| e.same(&editor)))
            .filter(|r| ranges.iter().any(|(a, b)| self.index.cmp(&r.start, b).is_le() && self.index.cmp(a, &r.end).is_le()))
            .map(|r| r.id.clone())
            .collect();
        if theirs.is_empty() {
            return Outcome::Unchanged("not an editor here");
        }
        self.drop_permissions(&theirs, "Remove Editable Range", fonts)
    }

    /// Removes the editable ranges that reach any range of the selection.
    pub(in crate::editing) fn remove_editable_ranges(&mut self, fonts: &Fonts) -> Outcome {
        let ranges = self.text_ranges();
        if ranges.is_empty() {
            return Outcome::Unchanged("no selection");
        }
        // The ids of the ranges around the selection, whoever may edit them.
        let gone: std::collections::HashSet<String> = self
            .editable_ranges()
            .ranges
            .iter()
            .filter(|r| {
                ranges.get(ranges.partition_point(|x| self.index.cmp(&x.1, &r.start).is_lt())).is_some_and(|(a, _)| self.index.cmp(a, &r.end).is_le())
            })
            .map(|r| r.id.clone())
            .collect();
        if gone.is_empty() {
            return Outcome::Unchanged("no editable range here");
        }
        self.drop_permissions(&gone, "Remove Editable Range", fonts)
    }

    /// Takes the permissions `gone` (by id) out, both their markers.
    fn drop_permissions(&mut self, gone: &std::collections::HashSet<String>, label: &'static str, fonts: &Fonts) -> Outcome {
        let edits: Vec<(ParaId, Vec<Inline>)> = self
            .index
            .para_ids()
            .filter_map(|para| {
                let p = self.index.para(&self.doc, para)?;
                let keep = |i: &Inline| !perm(i).is_some_and(|(_, id, _)| gone.contains(&id));
                (!p.inlines.iter().all(keep)).then(|| (para, p.inlines.iter().filter(|i| keep(i)).cloned().collect()))
            })
            .collect();
        let mut tx = self.begin(label);
        for (para, inlines) in edits {
            if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale range");
            }
        }
        self.commit(tx, None, fonts)
    }
}

/// Whether a command changes only what the selection holds: what an editable range, or a section that
/// filling-in-forms protection leaves open, lets run (a whole open section also its tables and objects).
pub fn edits_selection(id: crate::api::command::CommandId, section: bool) -> bool {
    use crate::api::catalog::CommandGroup as G;
    use crate::api::command::CommandId as C;
    let Some(info) = crate::api::catalog::info(id) else { return false };
    // What changes the document beyond the selection: its sections, styles, list definitions, defaults and fonts.
    let document = matches!(
        id,
        C::InsertSectionBreak
            | C::ReplaceFont
            | C::SetDocumentDefaults
            | C::SetDefaultFormatting
            | C::CreateStyle
            | C::ModifyStyle
            | C::DeleteStyle
            | C::SetStyleMeta
            | C::UpdateStyleToSelection
            | C::SetPageBorders
            | C::SetListDefinition
            | C::SetDefaultTabStop
            | C::SetDefaultTableStyle
    );
    let here = matches!(info.group, G::Text | G::Clipboard | G::Character | G::Paragraph | G::Tabs)
        || matches!(
            id,
            C::InsertHyperlink | C::EditHyperlink | C::RemoveHyperlink | C::InsertPicture | C::InsertField | C::InsertSymbol | C::InsertFile
        )
        || matches!(
            id,
            C::InsertTableOfContents
                | C::InsertTableOfFigures
                | C::InsertManualTableOfContents
                | C::UpdateTableOfContents
                | C::UpdateAllTablesOfContents
                | C::RemoveTableOfContents
                | C::MarkTableOfContentsEntry
                | C::InsertCaption
                | C::InsertCrossReference
        )
        || (section && matches!(info.group, G::Table | G::Object));
    here && !document
}

/// Whether command `id` replaces the selection (typing, paste, a break or an insert): of a discontiguous selection,
/// only the selection's own range, the others deselected.
pub(super) fn replaces_primary(id: CommandId) -> bool {
    use CommandId as C;
    matches!(
        id,
        C::InsertText
            | C::InsertParagraph
            | C::InsertLineBreak
            | C::InsertPageBreak
            | C::InsertColumnBreak
            | C::InsertSectionBreak
            | C::InsertTab
            | C::InsertAlignmentTab
            | C::InsertSymbol
            | C::InsertField
            | C::InsertFormField
            | C::InsertHyperlink
            | C::InsertPicture
            | C::InsertPageNumber
            | C::InsertNote
            | C::InsertFootnote
            | C::InsertEndnote
            | C::InsertShape
            | C::InsertTextBox
            | C::InsertTable
            | C::Paste
            | C::PasteText
            | C::Repaste
            | C::InsertFile
            | C::InsertTableOfContents
            | C::InsertTableOfFigures
            | C::InsertManualTableOfContents
            | C::InsertCrossReference
    )
}

/// Whether a protection lets a command run: comments in a comments-only document, filling in forms in a forms one.
pub fn allows(p: &Protection, id: crate::api::command::CommandId) -> bool {
    use crate::api::command::CommandId as C;
    if !p.enforced || matches!(id, C::SetDocumentProtection | C::Undo | C::Redo) {
        return true;
    }
    match p.edit {
        ProtectionKind::None | ProtectionKind::TrackedChanges => true,
        ProtectionKind::ReadOnly => false,
        ProtectionKind::Comments => {
            matches!(id, C::AddComment | C::ReplyToComment | C::EditComment | C::DeleteComment | C::DeleteAllComments | C::ResolveComment)
        }
        ProtectionKind::Forms => matches!(id, C::FillFormField | C::FillControl | C::SetCheckbox),
    }
}

/// The styles a table of contents or caption command gives what it inserts, which formatting protection allows only
/// when none is locked.
fn references_styles(c: &Command) -> Vec<String> {
    let tocs = || (1..=9).map(|n| format!("TOC{n}")).chain(["TOCHeading".to_string()]);
    match c {
        Command::InsertTableOfContents(t) if t.options.as_ref().is_some_and(|o| o.captions.is_some()) => {
            vec!["TableofFigures".into(), "TOCHeading".into()]
        }
        Command::InsertTableOfContents(_) | Command::InsertManualTableOfContents => tocs().collect(),
        Command::InsertTableOfFigures(_) => vec!["TableofFigures".into(), "TOCHeading".into()],
        Command::InsertCaption(_) => vec!["Caption".into()],
        _ => Vec::new(),
    }
}

/// The style a command applies, which formatting protection allows only when it isn't locked.
fn applied_style(c: &Command) -> Option<&crate::api::value::StyleId> {
    match c {
        Command::SetParagraphStyle(s)
        | Command::SetTableStyle(s)
        | Command::ApplyCharacterStyle(Some(s))
        | Command::SetDefaultTableStyle(Some(s)) => Some(s),
        _ => None,
    }
}

/// Whether formatting protection refuses a command: direct formatting, of characters, paragraphs, lists and cells,
/// and the edits of styles and defaults that restyle text the author didn't select.
pub fn restricts_formatting(id: CommandId) -> bool {
    use CommandId as C;
    super::styles::formats_a_style_id(id)
        || matches!(
            id,
            C::ReplaceFont
                | C::SetDocumentDefaults
                | C::SetDefaultFormatting
                | C::CreateStyle
                | C::ModifyStyle
                | C::SetFitText
                | C::SetTableBandSizes
                | C::DeleteStyle
                | C::UpdateStyleToSelection
                | C::SetList
                | C::SetListLevel
                | C::SetListDefinition
                | C::ApplyListEntry
                | C::SetNumberingValue
                | C::AdjustListIndents
                | C::SetCellShading
                | C::SetCellBorders
                | C::PasteFormatting
        )
}

/// Why an enforced protection `p` refuses command `id`, where `at` says the edit falls: formatting protection, a
/// tracked-changes protection's revisions, then what Restrict Editing allows and where.
fn refusal<P: AsRef<[Openings]>>(p: &Protection, id: CommandId, at: impl FnOnce() -> P) -> Option<Error> {
    use CommandId as C;
    if !p.enforced || !crate::api::catalog::info(id).is_some_and(|i| i.mutates) {
        return None;
    }
    if p.formatting && restricts_formatting(id) {
        return Some(Error::new(ErrorCode::NotAllowed, "formatting is restricted"));
    }
    let resolves = matches!(
        id,
        C::AcceptRevision
            | C::RejectRevision
            | C::AcceptAllRevisions
            | C::RejectAllRevisions
            | C::AcceptRevisionsInSelection
            | C::RejectRevisionsInSelection
            | C::AcceptAllAndStopTracking
    );
    if p.forces_tracking() && resolves {
        return Some(Error::unavailable("unavailable"));
    }
    if allows(p, id) {
        return None;
    }
    // Each place the edit falls on needs an exception of its own: a form field in one and an open section in another
    // are both allowed.
    let places = at();
    let opens = |at: &Openings| {
        let range = matches!(p.edit, ProtectionKind::ReadOnly | ProtectionKind::Comments) && edits_selection(id, false) && at.range;
        let form = p.edit == ProtectionKind::Forms && matches!(id, C::InsertText | C::Delete | C::PasteText) && at.form;
        let section = p.edit == ProtectionKind::Forms && edits_selection(id, true) && at.section;
        range || form || section
    };
    let places = places.as_ref();
    let allowed = if places.is_empty() { opens(&Openings::default()) } else { places.iter().all(opens) };
    (!allowed).then(|| Error::new(ErrorCode::NotAllowed, "the document is protected"))
}

/// What Word's Restrict Editing pane offers with editing restriction `kind`: exceptions (editable ranges and who may
/// edit them) for read-only and comments, sections to leave open for filling in forms.
pub fn restriction_options(kind: crate::api::command::ProtectionKind) -> RestrictionOptions {
    use crate::api::command::ProtectionKind as K;
    RestrictionOptions { exceptions: matches!(kind, K::ReadOnly | K::Comments), sections: kind == K::Forms }
}

/// The options Word's Restrict Editing pane offers with an editing restriction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestrictionOptions {
    /// Exceptions: parts of the document, and who may edit them.
    pub exceptions: bool,
    /// The sections filling in forms leaves open.
    pub sections: bool,
}
