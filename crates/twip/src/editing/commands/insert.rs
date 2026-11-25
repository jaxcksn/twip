//! Insert commands

use crate::api::command::{Hyperlink, HyperlinkEdit, Picture};
use crate::api::value::ObjectWrap;
use crate::docx::{FieldId, Inline};
use crate::font::Fonts;

use super::Outcome;
use crate::editing::Editor;

pub struct LinkHere {
    pub field: FieldId,
    /// Where it goes: the target, then `#` and the bookmark when it names one.
    pub address: String,
    /// The target without its bookmark: empty for a place in this document.
    pub target: String,
    /// The bookmark it goes to in its target.
    pub bookmark: Option<String>,
    /// The ScreenTip shown when hovered.
    pub tip: Option<String>,
    /// The window or frame it opens in.
    pub frame: Option<String>,
    pub text: String,
    /// A cross-reference that links to its bookmark rather than a hyperlink.
    pub reference: bool,
}

/// A link's parts as a host sets them: [`LinkHere`] without the field and text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkParts {
    pub target: String,
    pub bookmark: Option<String>,
    pub tip: Option<String>,
    pub frame: Option<String>,
}

impl LinkParts {
    /// The parts of an address written `target#bookmark`.
    pub fn of_address(address: &str) -> LinkParts {
        let (target, bookmark) = match address.split_once('#') {
            Some((t, b)) if !b.is_empty() => (t.to_string(), Some(b.to_string())),
            Some((t, _)) => (t.to_string(), None),
            None => (address.to_string(), None),
        };
        LinkParts { target, bookmark, ..LinkParts::default() }
    }

    /// The `HYPERLINK` field code that goes there.
    pub fn code(&self) -> String {
        use crate::docx::fields::quote;
        let mut code = String::from(" HYPERLINK ");
        if !self.target.is_empty() {
            code.push_str(&quote(&self.target));
            code.push(' ');
        }
        for (flag, v) in [("l", &self.bookmark), ("o", &self.tip), ("t", &self.frame)] {
            if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
                code.push_str(&format!("\\{flag} {} ", quote(v)));
            }
        }
        code
    }
}

impl Editor {
    pub(in crate::editing) fn insert_picture_with(&mut self, p: Picture, fonts: &Fonts) -> Outcome {
        let alt = p.alt_text.filter(|a| !a.is_empty());
        let wrap = self.default_picture_wrap;
        if alt.is_none() && wrap == ObjectWrap::Inline {
            return self.insert_picture(p.data.0, fonts);
        }
        self.joined("Insert Picture", fonts, |e| {
            let mut out = e.insert_picture(p.data.0, fonts);
            if !matches!(out, Outcome::Changed(_)) {
                return out;
            }
            if let Some(alt) = alt
                && let a @ Outcome::Changed(_) = e.object_cmd(crate::editing::objects::ObjectCmd::AltText(alt, None), fonts)
            {
                out = a;
            }
            if wrap != ObjectWrap::Inline
                && let w @ Outcome::Changed(_) = e.set_object_wrap(wrap, fonts)
            {
                out = w;
            }
            out
        })
    }

    /// Sets the wrap pictures are inserted with (Word's "Insert/paste pictures as", and Wrap Text > Set as Default
    /// Layout): in line with the text by default. An editor setting, not saved, no undo step. In a footnote or
    /// endnote, which hold no floating objects, a picture is inserted in line whatever the default.
    pub fn set_default_picture_wrap(&mut self, wrap: ObjectWrap) {
        self.default_picture_wrap = wrap;
    }

    /// The wrap pictures are inserted with.
    pub fn default_picture_wrap(&self) -> ObjectWrap {
        self.default_picture_wrap
    }

    pub(in crate::editing) fn insert_hyperlink(&mut self, h: Hyperlink, fonts: &Fonts) -> Outcome {
        self.insert_link(LinkParts::of_address(&h.address), h.text, fonts)
    }

    /// Inserts a link to `parts` showing `text`: the selected text when `None` (replaced by the link), else the
    /// address.
    pub fn insert_link(&mut self, parts: LinkParts, text: Option<String>, fonts: &Fonts) -> Outcome {
        let address = match &parts.bookmark {
            Some(b) => format!("{}#{b}", parts.target),
            None => parts.target.clone(),
        };
        let text = match text.filter(|t| !t.is_empty()) {
            Some(t) => t,
            None => Some(self.selected_text()).filter(|t| !t.is_empty() && !t.contains('\n')).unwrap_or(address),
        };
        self.pending_props(&|p, w| {
            p.set("color", format!("<{w}color {w}val=\"0563C1\"/>"));
            p.set("u", format!("<{w}u {w}val=\"single\"/>"));
        });
        let code = parts.code();
        let out = self.insert_inlines(|run| crate::docx::fields::field_inlines(&code, &text, &run), "Insert Hyperlink", fonts);
        self.pending = None;
        out
    }

    /// The hyperlink at the selection's start.
    pub fn link_here(&self) -> Option<LinkHere> {
        let (a, _) = self.selection.ordered(&self.index)?;
        self.innermost_link(a, false)
    }

    /// The innermost link at `a`: a hyperlink, or a cross-reference that links to its bookmark.
    pub fn link_at(&self, a: crate::editing::position::Position) -> Option<LinkHere> {
        self.innermost_link(a, true)
    }

    /// [`Editor::link_at`], cross-references left out unless `references`.
    fn innermost_link(&self, a: crate::editing::position::Position, references: bool) -> Option<LinkHere> {
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
                let switch = |f: char| code.switches.iter().find(|s| s.flag == f).and_then(|s| s.arg.clone()).filter(|a| !a.is_empty());
                let reference = code.name != "HYPERLINK";
                if reference && !references {
                    return None;
                }
                let parts = match code.name.as_str() {
                    "HYPERLINK" => LinkParts {
                        target: code.args.first().cloned().unwrap_or_default(),
                        bookmark: switch('l'),
                        tip: switch('o'),
                        frame: switch('t'),
                    },
                    // Cross-references made links by `\h`
                    "REF" | "PAGEREF" | "NOTEREF" if code.switches.iter().any(|s| s.flag.eq_ignore_ascii_case(&'h')) => {
                        LinkParts { bookmark: Some(code.args.first()?.clone()), ..LinkParts::default() }
                    }
                    _ => return None,
                };
                let Inline::FieldBegin { id, .. } = &p.inlines[s.begin] else { return None };
                let address = match &parts.bookmark {
                    Some(b) => format!("{}#{b}", parts.target),
                    None => parts.target.clone(),
                };
                let LinkParts { target, bookmark, tip, frame } = parts;
                let text = crate::docx::fields::result_text(&p.inlines, s);
                Some((s.end - s.begin, LinkHere { field: *id, address, target, bookmark, tip, frame, text, reference }))
            })
            .min_by_key(|(n, _)| *n)
            .map(|(_, l)| l)
    }

    pub(in crate::editing) fn edit_hyperlink(&mut self, h: HyperlinkEdit, fonts: &Fonts) -> Outcome {
        let address = h.address;
        let change = address.as_deref().map(|a| {
            let to = LinkParts::of_address(a);
            move |p: &mut LinkParts| {
                p.target = to.target;
                p.bookmark = to.bookmark;
            }
        });
        self.edit_link(change, h.text, fonts)
    }

    /// Changes the link at the selection: its parts by `change` (a ScreenTip and frame kept unless it changes
    /// them), its shown text to `text`.
    pub fn edit_link(&mut self, change: Option<impl FnOnce(&mut LinkParts)>, text: Option<String>, fonts: &Fonts) -> Outcome {
        let Some(link) = self.link_here() else { return Outcome::unavailable("no link at the selection") };
        let code = change.map(|f| {
            let mut parts = LinkParts { target: link.target, bookmark: link.bookmark, tip: link.tip, frame: link.frame };
            f(&mut parts);
            parts.code()
        });
        let id = crate::editing::merge::api_id(crate::docx::ElementId::Field(link.field));
        match self.field_set(&id, code.as_deref(), text.map(crate::api::review::Body::Text).as_ref(), fonts) {
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
