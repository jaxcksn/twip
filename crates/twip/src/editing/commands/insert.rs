//! Insert commands

use crate::api::command::{Hyperlink, HyperlinkEdit, Picture};
use crate::docx::{FieldId, Inline};
use crate::font::Fonts;

use super::Outcome;
use crate::editing::Editor;

pub(crate) struct LinkHere {
    pub field: FieldId,
    pub address: String,
    pub text: String,
}

impl Editor {
    pub(in crate::editing) fn insert_picture_with(&mut self, p: Picture, fonts: &Fonts) -> Outcome {
        let Some(alt) = p.alt_text.filter(|a| !a.is_empty()) else { return self.insert_picture(p.data.0, fonts) };
        self.joined("Insert Picture", fonts, |e| match e.insert_picture(p.data.0, fonts) {
            o @ Outcome::Changed(_) => match e.object_cmd(crate::editing::objects::ObjectCmd::AltText(alt, None), fonts) {
                a @ Outcome::Changed(_) => a,
                _ => o,
            },
            o => o,
        })
    }

    pub(in crate::editing) fn insert_hyperlink(&mut self, h: Hyperlink, fonts: &Fonts) -> Outcome {
        let text = match h.text.filter(|t| !t.is_empty()) {
            Some(t) => t,
            None => Some(self.selected_text()).filter(|t| !t.is_empty() && !t.contains('\n')).unwrap_or_else(|| h.address.clone()),
        };
        self.pending_props(&|p, w| {
            p.set("color", format!("<{w}color {w}val=\"0563C1\"/>"));
            p.set("u", format!("<{w}u {w}val=\"single\"/>"));
        });
        let code = link_code(&h.address);
        let out = self.insert_inlines(|run| crate::docx::fields::field_inlines(&code, &text, &run), "Insert Hyperlink", fonts);
        self.pending = None;
        out
    }

    pub(crate) fn link_here(&self) -> Option<LinkHere> {
        let (a, _) = self.selection.ordered(&self.index)?;
        self.link_at(a)
    }

    pub(crate) fn link_at(&self, a: crate::editing::position::Position) -> Option<LinkHere> {
        let (para, at) = a.para_offset()?;
        let p = self.index.para(&self.doc, para)?;
        let mut unit = 0u32;
        let starts: Vec<u32> = p
            .inlines
            .iter()
            .map(|i| {
                let u = unit;
                unit += i.units() as u32;
                u
            })
            .collect();
        let units = |k: usize| starts.get(k).copied().unwrap_or(unit);
        crate::docx::fields::spans(&p.inlines)
            .into_iter()
            .filter(|s| units(s.begin) <= at.0 && at.0 <= units(s.end))
            .filter_map(|s| {
                let code = crate::docx::fields::parse(&crate::docx::fields::code_text(&p.inlines, s));
                if !code.name.eq_ignore_ascii_case("HYPERLINK") {
                    return None;
                }
                let Inline::FieldBegin { id, .. } = &p.inlines[s.begin] else { return None };
                let mut address = code.args.first().cloned().unwrap_or_default();
                if let Some(a) = code.switches.iter().find(|s| s.flag == 'l').and_then(|s| s.arg.as_deref()) {
                    address = format!("{address}#{a}");
                }
                Some((s.end - s.begin, LinkHere { field: *id, address, text: crate::docx::fields::result_text(&p.inlines, s) }))
            })
            .min_by_key(|(n, _)| *n)
            .map(|(_, l)| l)
    }

    pub(in crate::editing) fn edit_hyperlink(&mut self, h: HyperlinkEdit, fonts: &Fonts) -> Outcome {
        let Some(link) = self.link_here() else { return Outcome::unavailable("no link at the selection") };
        let code = h.address.as_deref().map(link_code);
        let id = crate::editing::merge::api_id(crate::docx::ElementId::Field(link.field));
        match self.field_set(&id, code.as_deref(), h.text.as_deref(), fonts) {
            Ok(o) => o,
            Err(e) => Outcome::Refused(e),
        }
    }

    pub(in crate::editing) fn remove_hyperlink(&mut self, fonts: &Fonts) -> Outcome {
        let Some(link) = self.link_here() else { return Outcome::unavailable("no link at the selection") };
        let id = crate::editing::merge::api_id(crate::docx::ElementId::Field(link.field));
        self.unlink_fields(Some(&[id]), fonts)
    }
}

fn link_code(address: &str) -> String {
    format!(" HYPERLINK \"{}\" ", address.replace('"', ""))
}
