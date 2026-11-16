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
    /// The editable ranges `author` may edit, start and end, in document order.
    fn editable_ranges(&self, author: &str) -> Vec<(Position, Position)> {
        let mut open: Vec<(String, Position, bool)> = Vec::new();
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
                        let ok = who.is_some_and(|w| w.eq_ignore_ascii_case("everyone") || w.eq_ignore_ascii_case(author));
                        open.push((id, at, ok));
                    } else if let Some(k) = open.iter().position(|(o, ..)| *o == id) {
                        let (_, from, ok) = open.remove(k);
                        if ok {
                            out.push((from, at));
                        }
                    }
                }
                unit += i.units() as u32;
            }
        }
        out
    }

    /// The selection is in a range `author` may edit.
    pub fn in_editable_range(&self, author: &str) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        self.editable_ranges(author).iter().any(|(s, e)| self.index.cmp(s, &a).is_le() && self.index.cmp(&b, e).is_le())
    }

    pub fn protection_state(&self, author: &str) -> Option<crate::api::state::ProtectionState> {
        let p = self.doc.settings.protection.as_ref()?;
        Some(crate::api::state::ProtectionState {
            kind: api_kind(p.edit),
            enforced: p.enforced,
            formatting: p.formatting,
            password: p.password.is_some(),
            in_editable_range: self.in_editable_range(author),
        })
    }

    pub(in crate::editing) fn set_document_protection(&mut self, d: DocumentProtection, fonts: &Fonts) -> Outcome {
        let was = self.doc.settings.protection.clone();
        let mut p = was.clone().unwrap_or_default();
        let enforced = p.enforced;
        if enforced && d.enforce != Some(false) && (d.kind.is_some() || d.formatting.is_some()) {
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
        if now.as_ref().map(|p| (p.edit, p.enforced, p.formatting, p.password.clone()))
            == was.as_ref().map(|p| (p.edit, p.enforced, p.formatting, p.password.clone()))
        {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin("Restrict Editing");
        if self.step(&mut tx, Step::Protection(now.map(Box::new))).is_err() {
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
