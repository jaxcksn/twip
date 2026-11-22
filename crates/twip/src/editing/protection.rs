//! Document protection: Restrict Editing, editable ranges and the gate every edit passes

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

/// An editing permission marker: `w:permStart` (its id, and who: `w:edGrp` or `w:ed`) or `w:permEnd`.
fn perm(i: &Inline) -> Option<(bool, String, Option<String>)> {
    let Inline::Opaque(o) = i else { return None };
    let x = o.xml.0.as_deref()?;
    let attr = |k: &str| {
        let at = x.find(&format!(":{k}=\""))? + k.len() + 3;
        Some(x[at..at + x[at..].find('"')?].to_string())
    };
    let start = x.contains("permStart");
    if !start && !x.contains("permEnd") {
        return None;
    }
    let who = if start { attr("edGrp").or_else(|| attr("ed")) } else { None };
    Some((start, attr("id")?, who))
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

/// The editable ranges: who may edit each (`w:edGrp` or `w:ed`), its start and end, and their order keys.
#[derive(Default)]
pub(super) struct EditableRanges {
    ranges: Vec<(Option<String>, Position, Position)>,
    keys: Vec<(Vec<usize>, Vec<usize>)>,
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
fn perms(inlines: &[Inline]) -> impl Iterator<Item = (Unit, (bool, String, Option<String>))> + '_ {
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
        let mut open: HashMap<String, (Position, Option<String>)> = HashMap::new();
        let mut ranges = Vec::new();
        for para in self.paras_in_order() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let Some(slot) = self.index.para_slot(para) else { continue };
            let story = self.index.story(slot.container);
            for (unit, (start, id, who)) in perms(&p.inlines) {
                let at = Position::text(story, para, unit);
                if start {
                    open.insert(id, (at, who));
                } else if let Some((from, who)) = open.remove(&id) {
                    ranges.push((who, from, at));
                }
            }
        }
        let keys = ranges.iter().map(|(_, a, b)| (self.index.order_key(a), self.index.order_key(b))).collect();
        let out = Rc::new(EditableRanges { ranges, keys });
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
            | Step::SectionBreak { .. } => true,
            _ => false,
        }
    }

    /// `a..b` is in a range `author` may edit.
    fn in_editable_range_at(&self, a: &Position, b: &Position, author: &str) -> bool {
        let ranges = self.editable_ranges();
        if ranges.ranges.is_empty() {
            return false;
        }
        let may = |w: &Option<String>| w.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("everyone") || w.eq_ignore_ascii_case(author));
        let (ka, kb) = (self.index.order_key(a), self.index.order_key(b));
        ranges.ranges.iter().zip(&ranges.keys).any(|((w, ..), (s, e))| may(w) && *s <= ka && kb <= *e)
    }

    /// The selection is in a range `author` may edit.
    pub fn in_editable_range(&self, author: &str) -> bool {
        self.selection.ordered(&self.index).is_some_and(|(a, b)| self.in_editable_range_at(&a, &b, author))
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

    fn selection_openings(&self) -> Openings {
        self.selection_span().map_or_else(Openings::default, |(a, b)| self.openings(&a, &b))
    }

    /// Where an edit of the selection falls: its ends, a selected object's anchor, selected cells' table.
    fn selection_span(&self) -> Option<(Position, Position)> {
        use super::selection::{ObjectRef, Selection};
        let at = |para: ParaId, u: Unit| self.index.para_slot(para).map(|s| Position::text(self.index.story(s.container), para, u));
        let p = match self.selection {
            Selection::Object(ObjectRef::Inline { para, offset }) => at(para, offset)?,
            Selection::Object(ObjectRef::Floating(f)) => at(self.index.float_anchor(f)?, Unit(0))?,
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
        refusal(p, id, || self.table_point(t).map_or_else(Openings::default, |at| self.openings(&at, &at))).is_some()
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
        refusal(p, command.id(), || match command {
            Command::Delete(d) if self.selection.is_collapsed() => self.delete_span(d).map_or_else(Openings::default, |(a, b)| self.openings(&a, &b)),
            _ => self.selection_openings(),
        })
    }

    /// Why document protection refuses an edit that command `id` would make at `a..b`: a gesture's, where it lands.
    pub(super) fn protection_refuses_at(&self, id: CommandId, a: &Position, b: &Position) -> Option<Error> {
        refusal(self.enforced_protection()?, id, || self.openings(a, b))
    }

    /// Whether an edit that command `id` would make at `a..b`, which isn't the command's own (a correction, a
    /// gesture's), is kept from it: by document protection, a locked control or a shown sub-template's body.
    pub(super) fn kept(&self, id: CommandId, a: &Position, b: &Position) -> bool {
        self.protection_refuses_at(id, a, b).is_some() || self.locked_edit(a, b) || self.expanded_in(a, b).is_some()
    }

    /// Why document protection refuses an edit that command `id` would make at the selection: a dialog's or a
    /// gesture's, which isn't a command.
    pub fn protection_refuses(&self, id: CommandId) -> Option<Error> {
        refusal(self.enforced_protection()?, id, || self.selection_openings())
    }

    /// Whether document protection refuses each command at the selection, for the state: `None` when it's not
    /// enforced.
    pub(super) fn protection_gate(&self) -> Option<impl Fn(CommandId) -> bool + '_> {
        let p = self.enforced_protection()?;
        let at = self.selection_openings();
        Some(move |id| refusal(p, id, || at).is_some())
    }

    /// What a delete at the caret removes: the grapheme or word beside it, or the paragraph mark it joins across.
    fn delete_span(&self, d: &DeleteOptions) -> Option<(Position, Position)> {
        let caret = self.selection.focus()?;
        let Some((para, u)) = caret.para_offset() else { return Some((caret, caret)) };
        let p = self.index.para(&self.doc, para)?;
        let t = super::text::UnitText::of(p);
        let word = d.unit == Some(DeleteUnit::Word);
        let slot = self.index.para_slot(para)?;
        let blocks = self.index.blocks(&self.doc, slot.container);
        let beside = |k: Option<usize>| match k.and_then(|k| blocks?.get(k)) {
            Some(crate::docx::Block::Para(q)) => Some((ParaId(q.id), super::step::para_len(q))),
            _ => None,
        };
        let at = |para, u| Position::text(caret.story, para, u);
        Some(match d.direction {
            Direction::Backward if u > Unit(0) => (at(para, if word { t.prev_word(u) } else { t.prev_grapheme(u) }), caret),
            Direction::Backward => beside(slot.block.checked_sub(1)).map_or((caret, caret), |(q, len)| (at(q, len), caret)),
            Direction::Forward if u < t.len() => (caret, at(para, if word { t.next_word(u) } else { t.next_grapheme(u) })),
            Direction::Forward => beside(Some(slot.block + 1)).map_or((caret, caret), |(q, _)| (caret, at(q, Unit(0)))),
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
        })
    }

    /// The sections filling-in-forms protection leaves open.
    fn unprotected_sections(&self) -> impl Iterator<Item = u32> + '_ {
        self.doc.sections.iter().enumerate().filter(|(_, s)| !s.form_protected).map(|(k, _)| k as u32)
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

    pub(in crate::editing) fn add_editable_range(&mut self, r: EditableRange, fonts: &Fonts) -> Outcome {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no selection") };
        let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset()) else { return Outcome::unavailable("no text range") };
        let ids: Vec<i64> = self
            .index
            .para_ids()
            .filter_map(|p| self.index.para(&self.doc, p))
            .flat_map(|p| p.inlines.iter().filter_map(perm))
            .filter_map(|(_, id, _)| id.parse().ok())
            .collect();
        let id = ids.iter().max().map_or(1, |m| m + 1);
        let w = self.cx().w;
        let esc = crate::docx::write::props::esc;
        let who = r.editor.as_deref().map_or_else(|| format!("{w}edGrp=\"everyone\""), |e| format!("{w}ed=\"{}\"", esc(e)));
        let marker = |x: String| Inline::Opaque(Rc::new(Opaque { xml: Raw(Some(Rc::from(x.as_str()))) }));
        let (start, end) = (marker(format!("<{w}permStart {w}id=\"{id}\" {who}/>")), marker(format!("<{w}permEnd {w}id=\"{id}\"/>")));
        let steps = if pa == pb && oa == ob {
            vec![Step::Text { para: pa, range: oa..oa, with: vec![start, end], edges: Edges::default() }]
        } else {
            vec![
                Step::Text { para: pb, range: ob..ob, with: vec![end], edges: Edges::default() },
                Step::Text { para: pa, range: oa..oa, with: vec![start], edges: Edges::default() },
            ]
        };
        let mut tx = self.begin("Editable Range");
        for s in steps {
            if self.step(&mut tx, s).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale selection");
            }
        }
        self.commit(tx, None, fonts)
    }

    pub(in crate::editing) fn remove_editable_ranges(&mut self, fonts: &Fonts) -> Outcome {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no selection") };
        // The ids of the ranges around the selection, whoever may edit them.
        let mut open: HashMap<String, Position> = HashMap::new();
        let mut gone: std::collections::HashSet<String> = std::collections::HashSet::new();
        for para in self.paras_in_order() {
            let (Some(p), Some(slot)) = (self.index.para(&self.doc, para), self.index.para_slot(para)) else { continue };
            let story = self.index.story(slot.container);
            for (unit, (start, id, _)) in perms(&p.inlines) {
                let at = Position::text(story, para, unit);
                if start {
                    open.insert(id, at);
                } else if let Some(from) = open.remove(&id)
                    && self.index.cmp(&from, &b).is_le()
                    && self.index.cmp(&a, &at).is_le()
                {
                    gone.insert(id);
                }
            }
        }
        if gone.is_empty() {
            return Outcome::Unchanged("no editable range here");
        }
        let edits: Vec<(ParaId, Vec<Inline>)> = self
            .index
            .para_ids()
            .filter_map(|para| {
                let p = self.index.para(&self.doc, para)?;
                let keep = |i: &Inline| !perm(i).is_some_and(|(_, id, _)| gone.contains(&id));
                (!p.inlines.iter().all(keep)).then(|| (para, p.inlines.iter().filter(|i| keep(i)).cloned().collect()))
            })
            .collect();
        let mut tx = self.begin("Remove Editable Range");
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
            | C::DeleteStyle
            | C::SetStyleMeta
            | C::UpdateStyleToSelection
            | C::SetPageBorders
            | C::SetListDefinition
            | C::SetDefaultTabStop
            | C::SetDefaultTableStyle
    );
    let here = matches!(info.group, G::Text | G::Clipboard | G::Character | G::Paragraph | G::Tabs)
        || matches!(id, C::InsertHyperlink | C::EditHyperlink | C::RemoveHyperlink | C::InsertPicture | C::InsertField | C::InsertSymbol)
        || (section && matches!(info.group, G::Table | G::Object));
    here && !document
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
            matches!(id, C::AddComment | C::ReplyToComment | C::EditComment | C::DeleteComment | C::ResolveComment)
        }
        ProtectionKind::Forms => matches!(id, C::FillFormField | C::FillControl | C::SetCheckbox),
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
        )
}

/// Why an enforced protection `p` refuses command `id`, where `at` says the edit falls: formatting protection, a
/// tracked-changes protection's revisions, then what Restrict Editing allows and where.
fn refusal(p: &Protection, id: CommandId, at: impl FnOnce() -> Openings) -> Option<Error> {
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
    );
    if p.forces_tracking() && resolves {
        return Some(Error::unavailable("unavailable"));
    }
    if allows(p, id) {
        return None;
    }
    let at = at();
    let range = matches!(p.edit, ProtectionKind::ReadOnly | ProtectionKind::Comments) && edits_selection(id, false) && at.range;
    let form = p.edit == ProtectionKind::Forms && matches!(id, C::InsertText | C::Delete | C::PasteText) && at.form;
    let section = p.edit == ProtectionKind::Forms && edits_selection(id, true) && at.section;
    (!range && !form && !section).then(|| Error::new(ErrorCode::NotAllowed, "the document is protected"))
}
