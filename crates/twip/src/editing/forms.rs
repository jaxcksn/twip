//! Legacy form fields: insert, set and fill

use std::rc::Rc;

use crate::api::command::{FormFieldChange, FormFieldFill, FormFieldType, TextInputType};
use crate::docx::forms::{FormData, FormKind};
use crate::docx::{ElementId, FieldId, FieldKind, FormField, Inline, Raw};
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Editor;
use super::commands::Outcome;
use super::merge::Where;
use super::step::Step;

/// The model's text input type for the API's.
fn input_name(t: TextInputType) -> &'static str {
    match t {
        TextInputType::Regular => "regular",
        TextInputType::Number => "number",
        TextInputType::Date => "date",
        TextInputType::CurrentDate => "currentDate",
        TextInputType::CurrentTime => "currentTime",
        TextInputType::Calculation => "calculated",
    }
}

/// A form field's options and value, for the state.
pub(super) fn info(d: &FormData) -> crate::api::state::FormFieldInfo {
    let mut i = crate::api::state::FormFieldInfo {
        kind: FormFieldType::Text,
        name: d.name.clone(),
        enabled: d.enabled,
        calculate_on_exit: d.calc_on_exit,
        help_text: d.help.1.clone(),
        status_text: d.status.1.clone(),
        input_type: None,
        default_text: None,
        max_length: None,
        checked: d.checked(),
        entries: Vec::new(),
        entry: None,
    };
    match &d.kind {
        FormKind::Text { input, default, max_length, .. } => {
            i.input_type =
                serde_json::from_value(serde_json::Value::String(if input == "calculated" { "calculation".into() } else { input.clone() })).ok();
            (i.default_text, i.max_length) = (Some(default.clone()), Some(*max_length));
        }
        FormKind::CheckBox { .. } => i.kind = FormFieldType::CheckBox,
        FormKind::DropDown { entries, default, result } => {
            (i.kind, i.entries, i.entry) = (FormFieldType::DropDown, entries.clone(), Some(result.unwrap_or(*default)));
        }
    }
    i
}

/// What layout shows for the data: a check box's state, a drop-down's entry.
fn shown(begin: &mut Inline) {
    let Inline::FieldBegin { kind, value, form: Some(f), .. } = begin else { return };
    match &f.data.kind {
        FormKind::CheckBox { size, .. } => *kind = Some(FieldKind::CheckBox { size: *size, checked: f.data.checked().unwrap_or(false) }),
        FormKind::DropDown { .. } => (*kind, *value) = (Some(FieldKind::DropDown), f.data.shown()),
        FormKind::Text { .. } => {}
    }
}

/// Where form field `f`'s span is in `inlines`.
fn form_span(inlines: &[Inline], f: FieldId) -> Option<crate::docx::fields::FieldSpan> {
    crate::docx::fields::spans(inlines).into_iter().find(|s| matches!(&inlines[s.begin], Inline::FieldBegin { id, .. } if *id == f))
}

/// The positions of form field `f`'s bookmark `name` in its paragraph's `inlines`: a start and the end after it, the
/// pair overlapping the field.
fn owned_bookmark(inlines: &[Inline], f: FieldId, name: &str) -> Option<(usize, usize)> {
    if name.is_empty() {
        return None;
    }
    let s = form_span(inlines, f)?;
    let named = |i: &Inline, end: bool| {
        let Inline::Bookmark(n) = i else { return false };
        n.strip_prefix('/').map_or(!end && n.eq_ignore_ascii_case(name), |n| end && n.eq_ignore_ascii_case(name))
    };
    let start = inlines[..s.end].iter().rposition(|i| named(i, false))?;
    let end = start + 1 + inlines[start + 1..].iter().position(|i| named(i, true))?;
    (end > s.begin).then_some((start, end))
}

/// Renames form field `f`'s bookmark (`owned`, its positions in `inlines`) to `new`: a field without one gets one
/// around it, and an empty name removes it.
fn rename_form_bookmark(f: FieldId, inlines: &mut Vec<Inline>, owned: Option<(usize, usize)>, new: &str) {
    match owned {
        Some((start, end)) if new.is_empty() => {
            inlines.remove(end);
            inlines.remove(start);
        }
        Some((start, end)) => {
            inlines[start] = Inline::Bookmark(new.to_string());
            inlines[end] = Inline::Bookmark(format!("/{new}"));
        }
        None if new.is_empty() => {}
        None => {
            if let Some(s) = form_span(inlines, f) {
                inlines.insert(s.end + 1, Inline::Bookmark(format!("/{new}")));
                inlines.insert(s.begin, Inline::Bookmark(new.to_string()));
            }
        }
    }
}

impl Editor {
    /// The form field `id` names, else the one at the selection: its id and paragraph.
    fn form_field(&self, id: Option<&crate::api::value::ElementId>) -> Option<(FieldId, ParaId)> {
        let (f, para) = match id {
            Some(id) => match super::elements::parse_id(&id.0)? {
                ElementId::Field(f) => match self.locate(ElementId::Field(f))? {
                    Where::Inline { para, .. } => (f, para),
                    _ => return None,
                },
                _ => return None,
            },
            None => *self.fields_here(None, false).first()?,
        };
        let p = self.index.para(&self.doc, para)?;
        p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { id, form: Some(_), .. } if *id == f)).then_some((f, para))
    }

    /// Applies `edit` to a form field's data; `false` from it changes nothing.
    fn edit_form(&mut self, f: FieldId, para: ParaId, label: &'static str, edit: impl FnOnce(&mut FormData) -> bool, fonts: &Fonts) -> Outcome {
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let mut inlines = p.inlines.clone();
        let Some(begin) = inlines.iter_mut().find(|i| matches!(i, Inline::FieldBegin { id, .. } if *id == f)) else {
            return Outcome::Unchanged("no field");
        };
        let Inline::FieldBegin { form: Some(form), .. } = begin else { return Outcome::unavailable("not a form field") };
        let mut data = form.data.clone();
        if !edit(&mut data) || data == form.data {
            return Outcome::Unchanged("already so");
        }
        let (old, new) = (form.data.name.clone(), data.name.clone());
        Rc::make_mut(form).data = data;
        shown(begin);
        if old != new {
            let owned = owned_bookmark(&inlines, f, &old);
            if let Err(why) = self.form_name_free(f, owned.map(|_| old.as_str()), &new) {
                return Outcome::invalid(why);
            }
            rename_form_bookmark(f, &mut inlines, owned, &new);
        }
        let mut tx = self.begin(label);
        if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale field");
        }
        self.commit(tx, None, fonts)
    }

    /// Whether `new` can name form field `f`: no other form field has it, nor a bookmark but its own (`own`).
    fn form_name_free(&self, f: FieldId, own: Option<&str>, new: &str) -> Result<(), &'static str> {
        for p in self.index.para_ids().filter_map(|para| self.index.para(&self.doc, para)) {
            for i in &p.inlines {
                match i {
                    Inline::FieldBegin { id, form: Some(form), .. } if *id != f && form.data.name.eq_ignore_ascii_case(new) => {
                        return Err("another form field has the name");
                    }
                    Inline::Bookmark(n) if n.eq_ignore_ascii_case(new) && !own.is_some_and(|o| n.eq_ignore_ascii_case(o)) => {
                        return Err("a bookmark has the name");
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub(in crate::editing) fn insert_form_field(&mut self, t: FormFieldType, fonts: &Fonts) -> Outcome {
        let (code, stem, display) = match t {
            FormFieldType::Text => ("FORMTEXT", "Text", "\u{2002}".repeat(5)),
            FormFieldType::CheckBox => ("FORMCHECKBOX", "Check", String::new()),
            FormFieldType::DropDown => ("FORMDROPDOWN", "Dropdown", String::new()),
        };
        // Word's names: the kind and the first number no form field or bookmark of the document has.
        let mut names = std::collections::HashSet::new();
        for para in self.index.para_ids() {
            if let Some(p) = self.index.para(&self.doc, para) {
                names.extend(p.inlines.iter().filter_map(|i| match i {
                    Inline::FieldBegin { form: Some(f), .. } => Some(f.data.name.to_ascii_lowercase()),
                    Inline::Bookmark(n) if !n.starts_with('/') => Some(n.to_ascii_lowercase()),
                    _ => None,
                }));
            }
        }
        let n = (1..).find(|n| !names.contains(&format!("{stem}{n}").to_ascii_lowercase())).unwrap_or(1);
        let name = format!("{stem}{n}");
        let kind = match t {
            FormFieldType::Text => FormKind::default(),
            FormFieldType::CheckBox => FormKind::CheckBox { size: None, default: false, checked: None },
            FormFieldType::DropDown => FormKind::DropDown { entries: Vec::new(), default: 0, result: None },
        };
        let data = FormData { name: name.clone(), enabled: true, kind, ..FormData::default() };
        let id = FieldId::next();
        let out = self.insert_inlines(
            |run| {
                let mut v = crate::docx::fields::field_inlines_with(id, &format!(" {code} "), &display, &run);
                if let Some(Inline::FieldBegin { form, .. }) = v.first_mut() {
                    *form = Some(Rc::new(FormField { xml: Raw(None), data: data.clone(), read: data.clone() }));
                }
                if let Some(b) = v.first_mut() {
                    shown(b);
                }
                v.insert(0, Inline::Bookmark(name.clone()));
                v.push(Inline::Bookmark(format!("/{name}")));
                v
            },
            "Insert Form Field",
            fonts,
        );
        if matches!(out, Outcome::Changed(_)) {
            self.made = Some(crate::api::command::CommandResult::Element { id: super::merge::api_id(ElementId::Field(id)) });
        }
        out
    }

    pub(in crate::editing) fn set_form_field(&mut self, c: FormFieldChange, fonts: &Fonts) -> Outcome {
        let Some((f, para)) = self.form_field(c.id.as_ref()) else { return Outcome::unavailable("no form field") };
        self.edit_form(
            f,
            para,
            "Form Field Options",
            |d| {
                if let Some(v) = c.name {
                    d.name = v;
                }
                if let Some(v) = c.enabled {
                    d.enabled = v;
                }
                if let Some(v) = c.calculate_on_exit {
                    d.calc_on_exit = v;
                }
                if let Some(v) = c.entry_macro {
                    d.entry_macro = v;
                }
                if let Some(v) = c.exit_macro {
                    d.exit_macro = v;
                }
                if let Some(v) = c.help_text {
                    d.help = (false, v);
                }
                if let Some(v) = c.status_text {
                    d.status = (false, v);
                }
                match &mut d.kind {
                    FormKind::Text { input, default, max_length, format } => {
                        let Some(t) = c.text else { return true };
                        if let Some(v) = t.input_type {
                            *input = input_name(v).into();
                        }
                        if let Some(v) = t.default {
                            *default = v;
                        }
                        if let Some(v) = t.max_length {
                            *max_length = v;
                        }
                        if let Some(v) = t.format {
                            *format = v;
                        }
                    }
                    FormKind::CheckBox { size, default, .. } => {
                        let Some(b) = c.check_box else { return true };
                        if let Some(v) = b.size {
                            *size = match v {
                                crate::api::value::AutoOr::Auto => None,
                                crate::api::value::AutoOr::Value(p) => Some(p.0),
                            };
                        }
                        if let Some(v) = b.checked_by_default {
                            *default = v;
                        }
                    }
                    FormKind::DropDown { entries, default, result } => {
                        let Some(dd) = c.drop_down else { return true };
                        if let Some(v) = dd.entries {
                            *entries = v;
                            *result = None;
                        }
                        *default = dd.default.unwrap_or(*default).min(entries.len().saturating_sub(1) as u32);
                    }
                }
                true
            },
            fonts,
        )
    }

    pub(in crate::editing) fn fill_form_field(&mut self, c: FormFieldFill, fonts: &Fonts) -> Outcome {
        let Some((f, para)) = self.form_field(c.id.as_ref()) else { return Outcome::unavailable("no form field") };
        let enabled = self
            .index
            .para(&self.doc, para)
            .is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { id, form: Some(form), .. } if *id == f && form.data.enabled)));
        if !enabled {
            return Outcome::unavailable("the field is not enabled");
        }
        if let Some(text) = c.text {
            let max = self.index.para(&self.doc, para).and_then(|p| {
                p.inlines.iter().find_map(|i| match i {
                    Inline::FieldBegin { id, form: Some(form), .. } if *id == f => match &form.data.kind {
                        FormKind::Text { max_length, .. } => Some(*max_length),
                        _ => None,
                    },
                    _ => None,
                })
            });
            let Some(max) = max else { return Outcome::unavailable("not a text form field") };
            let text: String = if max > 0 { text.chars().take(max as usize).collect() } else { text };
            let mut tx = self.begin("Fill In");
            if !self.set_result(&mut tx, f, &crate::api::review::Body::Text(text)) {
                self.abort(tx);
                return Outcome::Unchanged("stale field");
            }
            return self.commit(tx, None, fonts);
        }
        self.edit_form(
            f,
            para,
            "Fill In",
            |d| match &mut d.kind {
                FormKind::CheckBox { checked, .. } => c.checked.map(|v| *checked = Some(v)).is_some(),
                FormKind::DropDown { entries, result, .. } => c.entry.filter(|e| (*e as usize) < entries.len()).map(|e| *result = Some(e)).is_some(),
                FormKind::Text { .. } => false,
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_form_shading(&mut self, on: bool, fonts: &Fonts) -> Outcome {
        let mut p = super::step::PageSettings::of(&self.doc.settings);
        if p.no_form_shading == !on {
            return Outcome::Unchanged("already so");
        }
        p.no_form_shading = !on;
        let mut tx = self.begin("Form Field Shading");
        if self.step(&mut tx, Step::PageSettings(p)).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("setting failed");
        }
        self.commit(tx, None, fonts)
    }
}
