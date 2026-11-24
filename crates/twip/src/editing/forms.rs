//! Legacy form fields: insert, set and fill

use std::rc::Rc;

use crate::api::command::{FormFieldChange, FormFieldFill, FormFieldType, TextInputType};
use crate::docx::forms::{FormData, FormKind};
use crate::docx::{ElementId, FieldId, FieldKind, FormField, Inline, Raw};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

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

    /// The settings and value of the form field `id` names, else of the one at the selection: every option
    /// `setFormField` sets, its text format, macros, check box size and default and drop-down default among them.
    pub fn form_data(&self, id: Option<&crate::api::value::ElementId>) -> Option<&FormData> {
        let (f, para) = self.form_field(id)?;
        self.index.para(&self.doc, para)?.inlines.iter().find_map(|i| match i {
            Inline::FieldBegin { id, form: Some(form), .. } if *id == f => Some(&form.data),
            _ => None,
        })
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

/// What a press on a legacy form field asks the host for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormPress {
    /// Open a drop-down's list: a click on one in a document protected for filling in forms.
    OpenList,
    /// Open the field's options: a double-click on one in a document that isn't.
    Options,
}

impl Editor {
    /// Filling-in-forms protection holds at `p`: it is enforced and `p`'s section is one it protects.
    pub fn fills_forms_at(&self, p: &crate::editing::position::Position) -> bool {
        self.enforced_protection().is_some_and(|x| x.edit == crate::docx::review::ProtectionKind::Forms)
            && self.section_at(p).and_then(|k| self.doc.sections.get(k)).is_some_and(|s| s.form_protected)
    }

    /// Filling-in-forms protection holds at the selection: Tab and Shift+Tab move between the fields to fill in.
    pub fn tab_moves_between_form_fields(&self) -> bool {
        self.mode == super::Mode::Editing && self.selection.focus().is_some_and(|p| self.fills_forms_at(&p))
    }

    /// The enabled legacy drop-down holding the selection where forms are filled in, in editing mode: the one F4 or
    /// Alt+Down opens the list of.
    pub fn selected_form_drop_down(&self) -> Option<FieldId> {
        if self.mode != super::Mode::Editing {
            return None;
        }
        let (a, b) = self.selection.ordered(&self.index)?;
        let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
        if pa != pb || !self.fills_forms_at(&a) {
            return None;
        }
        let p = self.index.para(&self.doc, pa)?;
        crate::docx::fields::spans(&p.inlines).into_iter().find_map(|s| match &p.inlines[s.begin] {
            Inline::FieldBegin { id, form: Some(form), .. } if form.data.enabled && matches!(form.data.kind, FormKind::DropDown { .. }) => {
                let (start, end, ..) = self.form_field_range(*id, pa)?;
                (start <= ua && ub <= end).then_some(*id)
            }
            _ => None,
        })
    }

    /// The legacy form field drawn at `(x, y)`: its id and paragraph.
    pub(super) fn form_field_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(FieldId, ParaId, crate::editing::position::Position)> {
        let (e, p, _) = self.element_at(x, y, fonts)?;
        let ElementId::Field(f) = e else { return None };
        let para = p.para_offset()?.0;
        let q = self.index.para(&self.doc, para)?;
        q.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { id, form: Some(_), .. } if *id == f)).then_some((f, para, p))
    }

    /// What a press of `clicks` at `(x, y)` on a legacy form field asks the host for, in editing mode: a click on a
    /// drop-down where forms are filled in opens its list; a double-click on any where they aren't, its options.
    pub fn form_field_press_at(&mut self, x: f64, y: f64, clicks: u8, fonts: &Fonts) -> Option<(FieldId, FormPress)> {
        if self.mode != super::Mode::Editing {
            return None;
        }
        let (f, para, p) = self.form_field_at(x, y, fonts)?;
        let data = self.index.para(&self.doc, para)?.inlines.iter().find_map(|i| match i {
            Inline::FieldBegin { id, form: Some(form), .. } if *id == f => Some(&form.data),
            _ => None,
        })?;
        match (self.fills_forms_at(&p), clicks) {
            (true, 1) if matches!(data.kind, FormKind::DropDown { .. }) && data.enabled => Some((f, FormPress::OpenList)),
            (false, 2) => Some((f, FormPress::Options)),
            _ => None,
        }
    }

    /// Where legacy form field `f` of `para` is: its start and end, and its result's.
    fn form_field_range(&self, f: FieldId, para: ParaId) -> Option<(Unit, Unit, Unit, Unit)> {
        let p = self.index.para(&self.doc, para)?;
        let s = form_span(&p.inlines, f)?;
        let at = |k: usize| Unit(p.inlines[..k].iter().map(|i| i.units() as u32).sum());
        let result = s.sep.map_or(at(s.end), |k| at(k + 1));
        Some((at(s.begin), Unit(at(s.end).0 + 1), result, at(s.end)))
    }

    /// Selects legacy form field `f` of `para` as Tab does: a check box whole, a text field's or drop-down's result.
    fn select_form_field(&mut self, f: FieldId, para: ParaId, check_box: bool) -> bool {
        let Some((start, end, a, b)) = self.form_field_range(f, para) else { return false };
        let Some(slot) = self.index.para_slot(para) else { return false };
        let story = self.index.story(slot.container);
        let (a, b) = if check_box { (start, end) } else { (a, b) };
        self.selection = super::selection::Selection::range(
            crate::editing::position::Position::text(story, para, a),
            crate::editing::position::Position::text(story, para, b),
            super::selection::Granularity::Char,
        );
        true
    }

    /// Moves to the next (or previous) field to fill in, as Tab (Shift+Tab) does where forms are filled in: an
    /// enabled legacy form field or a content control whose content can be edited, in the selection's story,
    /// round to the first (last) after the last (first). Selects a check box whole, a text field's or drop-down's
    /// result, a control's content.
    pub fn select_next_form_field(&mut self, forward: bool) -> Outcome {
        use crate::docx::ControlKind;
        let Some(focus) = self.selection.focus() else { return Outcome::Unchanged("no selection") };
        // Going on from the selection's far end, back from its near one: a paragraph and a unit of the same place.
        let (a, b) = self.selection.ordered(&self.index).unwrap_or((focus, focus));
        let Some((here, start)) = (if forward { b } else { a }).para_offset() else { return Outcome::Unchanged("no paragraph") };
        let story = focus.story;
        // Every place to fill in, in order: (paragraph, unit, what)
        enum Stop {
            Form(FieldId, bool),
            Control(crate::docx::ControlId),
        }
        let mut stops: Vec<(usize, ParaId, Unit, Stop)> = Vec::new();
        let mut mine = 0;
        for (k, para) in self.paras_of(story).into_iter().enumerate() {
            if para == here {
                mine = k;
            }
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut u = 0u32;
            for i in &p.inlines {
                match i {
                    Inline::FieldBegin { id, form: Some(form), .. } if form.data.enabled => {
                        stops.push((k, para, Unit(u), Stop::Form(*id, matches!(form.data.kind, FormKind::CheckBox { .. }))));
                    }
                    Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. }
                        if self.doc.controls.get(c).is_some_and(|k| {
                            k.props.lock.editable()
                                && !matches!(k.props.kind, ControlKind::Group | ControlKind::RepeatingSection | ControlKind::RepeatingItem)
                        }) =>
                    {
                        stops.push((k, para, Unit(u), Stop::Control(*c)));
                    }
                    _ => {}
                }
                u += i.units() as u32;
            }
        }
        if stops.is_empty() {
            return Outcome::Unchanged("no form fields");
        }
        // The stop holding the selection counts as passed
        let key = |s: &(usize, ParaId, Unit, Stop)| (s.0, s.2);
        let pick = if forward {
            stops.iter().find(|s| key(s) > (mine, start)).or(stops.first())
        } else {
            // Not the one the selection is in
            let holds = |s: &(usize, ParaId, Unit, Stop)| {
                s.0 == mine
                    && match &s.3 {
                        Stop::Form(f, _) => self.form_field_range(*f, s.1).is_some_and(|(a, b, ..)| a <= start && start < b),
                        Stop::Control(c) => self
                            .locate(ElementId::Control(*c))
                            .and_then(|w| self.element_range(&w))
                            .and_then(|(a, b)| Some((a.para_offset()?.1, b.para_offset()?.1)))
                            .is_some_and(|(a, b)| a <= start && start <= b),
                    }
            };
            stops.iter().rev().find(|s| key(s) < (mine, start) && !holds(s)).or(stops.last())
        };
        let Some((_, para, _, stop)) = pick else { return Outcome::Unchanged("no form fields") };
        let before = self.selection;
        match stop {
            Stop::Form(f, check_box) => {
                self.select_form_field(*f, *para, *check_box);
            }
            Stop::Control(c) => {
                let Some(range) = self.locate(ElementId::Control(*c)).and_then(|w| self.element_range(&w)) else {
                    return Outcome::Unchanged("stale control");
                };
                self.selection = super::selection::Selection::range(range.0, range.1, super::selection::Granularity::Char);
            }
        }
        if self.selection == before { Outcome::Unchanged("already there") } else { Outcome::Moved }
    }

    /// The legacy check box the selection is on where forms are filled in: the whole field selected, as clicking or
    /// Tab leaves it.
    pub(super) fn selected_form_check_box(&self) -> Option<crate::editing::position::Position> {
        let (a, b) = self.selection.ordered(&self.index)?;
        let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
        if pa != pb {
            return None;
        }
        let p = self.index.para(&self.doc, pa)?;
        let f = crate::docx::fields::spans(&p.inlines).into_iter().find_map(|s| match &p.inlines[s.begin] {
            Inline::FieldBegin { id, form: Some(form), .. } if matches!(form.data.kind, FormKind::CheckBox { .. }) => {
                let (start, end, ..) = self.form_field_range(*id, pa)?;
                (start == ua && end == ub).then_some(start)
            }
            _ => None,
        })?;
        Some(crate::editing::position::Position::text(a.story, pa, f)).filter(|p| self.fills_forms_at(p))
    }

    /// Toggles the legacy check box whose field starts at `p`, as a click, Space or X does where forms are filled
    /// in, leaving it selected.
    pub(super) fn toggle_form_check_box(&mut self, p: crate::editing::position::Position, fonts: &Fonts) -> Outcome {
        let Some((para, u)) = p.para_offset() else { return Outcome::Unchanged("no check box") };
        let Some(Inline::FieldBegin { id, form: Some(form), .. }) =
            self.index.para(&self.doc, para).and_then(|q| super::stories::inline_at(q, u)).cloned()
        else {
            return Outcome::Unchanged("no check box");
        };
        let FormKind::CheckBox { .. } = form.data.kind else { return Outcome::Unchanged("no check box") };
        let now = form.data.checked().unwrap_or(false);
        let fill = FormFieldFill { id: Some(super::merge::api_id(ElementId::Field(id))), text: None, checked: Some(!now), entry: None };
        let out = self.fill_form_field(fill, fonts);
        if matches!(out, Outcome::Changed(_)) {
            self.select_form_field(id, para, true);
        }
        out
    }
}
