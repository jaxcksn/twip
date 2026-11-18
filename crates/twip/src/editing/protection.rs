//! Document protection: Restrict Editing and editable ranges

use std::rc::Rc;

use crate::api::command::{DocumentProtection, EditableRange, ProtectionKind as Kind};
use crate::api::error::{Error, ErrorCode};
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

impl Editor {
    /// Every editable range, in document order: who may edit it, its start and end. Found once per revision: the
    /// state asks on every change of selection.
    fn editable_ranges(&self) -> Rc<Vec<(Option<String>, Position, Position)>> {
        let mut cache = self.editable_cache.borrow_mut();
        if let Some((rev, r)) = cache.as_ref()
            && *rev == self.revision()
        {
            return r.clone();
        }
        let mut open: Vec<(String, Position, Option<String>)> = Vec::new();
        let mut out = Vec::new();
        for para in self.paras_in_order() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let Some(slot) = self.index.para_slot(para) else { continue };
            let story = self.index.story(slot.container);
            let mut unit = 0u32;
            for i in &p.inlines {
                if let Some((start, id, who)) = perm(i) {
                    let at = Position::text(story, para, Unit(unit));
                    if start {
                        open.push((id, at, who));
                    } else if let Some(k) = open.iter().position(|(o, ..)| *o == id) {
                        let (_, from, who) = open.remove(k);
                        out.push((who, from, at));
                    }
                }
                unit += i.units() as u32;
            }
        }
        let out = Rc::new(out);
        *cache = Some((self.revision(), out.clone()));
        out
    }

    /// The selection is in a range `author` may edit.
    pub fn in_editable_range(&self, author: &str) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let may = |w: &Option<String>| w.as_deref().is_some_and(|w| w.eq_ignore_ascii_case("everyone") || w.eq_ignore_ascii_case(author));
        self.editable_ranges().iter().any(|(w, s, e)| may(w) && self.index.cmp(s, &a).is_le() && self.index.cmp(&b, e).is_le())
    }

    /// Both ends of the selection are in the same legacy form field or content control.
    pub fn in_form_field(&self) -> bool {
        let (Some(focus), anchor) = (self.selection.focus(), self.selection.anchor()) else { return false };
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
        let here = fields(focus);
        let there = anchor.filter(|a| *a != focus).map(fields);
        here.iter().any(|e| there.as_ref().is_none_or(|t| t.contains(e)))
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

    /// The selection is in a section filling-in-forms protection leaves open.
    pub fn in_unprotected_section(&self) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let open = |p: &Position| self.section_at(p).and_then(|k| self.doc.sections.get(k)).is_some_and(|s| !s.form_protected);
        open(&a) && open(&b) && self.section_at(&a) == self.section_at(&b)
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
        let mut open: Vec<(String, Position)> = Vec::new();
        let mut gone: Vec<String> = Vec::new();
        for para in self.paras_in_order() {
            let (Some(p), Some(slot)) = (self.index.para(&self.doc, para), self.index.para_slot(para)) else { continue };
            let story = self.index.story(slot.container);
            let mut unit = 0u32;
            for i in &p.inlines {
                if let Some((start, id, _)) = perm(i) {
                    let at = Position::text(story, para, Unit(unit));
                    if start {
                        open.push((id, at));
                    } else if let Some(k) = open.iter().position(|(o, _)| *o == id) {
                        let (id, from) = open.remove(k);
                        if self.index.cmp(&from, &b).is_le() && self.index.cmp(&a, &at).is_le() {
                            gone.push(id);
                        }
                    }
                }
                unit += i.units() as u32;
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
