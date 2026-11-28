//! Document properties and variables

use crate::api::command::{CustomProperty, CustomPropertyKind, DocumentPropertiesChange, DocumentVariable};
use crate::docx::properties::{Properties, Value};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::step::Step;

impl Editor {
    pub(in crate::editing) fn set_document_properties(&mut self, c: DocumentPropertiesChange, fonts: &Fonts) -> Outcome {
        let mut p = self.doc.properties.clone();
        for (name, v) in [
            ("title", c.title),
            ("subject", c.subject),
            ("creator", c.author),
            ("keywords", c.keywords),
            ("description", c.comments),
            ("category", c.category),
            ("contentStatus", c.status),
            ("lastModifiedBy", c.last_modified_by),
        ] {
            if let Some(v) = v {
                p.set_core(name, Some(v));
            }
        }
        for (name, v) in [("Company", c.company), ("Manager", c.manager), ("HyperlinkBase", c.hyperlink_base)] {
            if let Some(v) = v {
                p.set_app(name, Some(v));
            }
        }
        if let Some(custom) = c.custom {
            p.custom = custom
                .into_iter()
                .map(|c| {
                    let v = match c.kind {
                        CustomPropertyKind::Text => Value::Text(c.value),
                        CustomPropertyKind::Number => c.value.trim().parse().map_or(Value::Text(c.value), Value::Number),
                        CustomPropertyKind::YesNo => Value::Bool(matches!(c.value.trim(), "true" | "1" | "yes")),
                        CustomPropertyKind::Date => Value::Date(c.value),
                    };
                    (c.name, v)
                })
                .collect();
        }
        self.set_info(p, self.doc.settings.variables.clone(), "Properties", fonts)
    }

    pub(in crate::editing) fn set_document_variable(&mut self, v: DocumentVariable, fonts: &Fonts) -> Outcome {
        let mut vars = self.doc.settings.variables.clone();
        match (vars.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(&v.name)), v.value) {
            (Some(slot), Some(value)) => slot.1 = value,
            (None, Some(value)) => vars.push((v.name, value)),
            (_, None) => vars.retain(|(n, _)| !n.eq_ignore_ascii_case(&v.name)),
        }
        self.set_info(self.doc.properties.clone(), vars, "Document Variable", fonts)
    }

    /// The name FILENAME shows: its fields show it now, as layout does, without an undo step.
    pub(in crate::editing) fn set_file_name(&mut self, name: String) -> Outcome {
        if self.file_name.as_deref() == Some(name.as_str()) {
            return Outcome::Unchanged("already so");
        }
        let shown = crate::docx::fields::show_file_name(&mut self.doc, &name);
        self.file_name = Some(name);
        if shown {
            self.changed_everywhere();
            self.relayout([None]);
        }
        Outcome::Moved
    }

    fn set_info(&mut self, p: Properties, vars: Vec<(String, String)>, label: &'static str, fonts: &Fonts) -> Outcome {
        if p == self.doc.properties && vars == self.doc.settings.variables {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin(label);
        if self.step(&mut tx, Step::Info(Box::new((p, vars)))).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("properties failed");
        }
        self.commit(tx, None, fonts)
    }

    /// The document's properties and variables.
    pub fn document_properties(&self) -> crate::api::services::DocumentProperties {
        let p = &self.doc.properties;
        let core = |n: &str| p.core(n).map(str::to_string);
        let app = |n: &str| p.app(n).map(str::to_string);
        crate::api::services::DocumentProperties {
            title: core("title"),
            subject: core("subject"),
            author: core("creator"),
            keywords: core("keywords"),
            comments: core("description"),
            category: core("category"),
            status: core("contentStatus"),
            last_modified_by: core("lastModifiedBy"),
            revision: core("revision"),
            created: core("created"),
            modified: core("modified"),
            last_printed: core("lastPrinted"),
            company: app("Company"),
            manager: app("Manager"),
            hyperlink_base: app("HyperlinkBase"),
            custom: p
                .custom
                .iter()
                .map(|(name, v)| {
                    let (kind, value) = match v {
                        Value::Text(s) => (CustomPropertyKind::Text, s.clone()),
                        Value::Number(_) => (CustomPropertyKind::Number, v.display()),
                        Value::Bool(b) => (CustomPropertyKind::YesNo, b.to_string()),
                        Value::Date(d) => (CustomPropertyKind::Date, d.clone()),
                    };
                    CustomProperty { name: name.clone(), kind, value }
                })
                .collect(),
            variables: self.doc.settings.variables.iter().map(|(n, v)| DocumentVariable { name: n.clone(), value: Some(v.clone()) }).collect(),
        }
    }
}
