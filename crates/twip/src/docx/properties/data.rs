//! A document's core, extended and custom properties

/// A custom property's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Bool(bool),
    /// An ISO 8601 date and time.
    Date(String),
}

impl Value {
    /// As a field shows it.
    pub fn display(&self) -> String {
        match self {
            Value::Text(s) | Value::Date(s) => s.clone(),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            Value::Bool(b) => if *b { "Y" } else { "N" }.into(),
        }
    }
}

/// The document's properties: core and extended by element name, custom in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties {
    pub core: Vec<(&'static str, String)>,
    pub app: Vec<(&'static str, String)>,
    pub custom: Vec<(String, Value)>,
}

impl Properties {
    pub fn core(&self, name: &str) -> Option<&str> {
        self.core.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str())
    }

    pub fn app(&self, name: &str) -> Option<&str> {
        self.app.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str())
    }

    pub fn set_core(&mut self, name: &'static str, v: Option<String>) {
        self.core.retain(|(n, _)| *n != name);
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            self.core.push((name, v));
        }
    }

    pub fn set_app(&mut self, name: &'static str, v: Option<String>) {
        self.app.retain(|(n, _)| *n != name);
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            self.app.push((name, v));
        }
    }

    /// The value DOCPROPERTY shows for `name` (compared without case): a core, extended or custom property.
    pub fn lookup(&self, name: &str) -> Option<String> {
        let core = match name.to_ascii_lowercase().as_str() {
            "title" => "title",
            "subject" => "subject",
            "author" => "creator",
            "keywords" => "keywords",
            "comments" => "description",
            "lastsavedby" => "lastModifiedBy",
            "revisionnumber" => "revision",
            "createtime" => "created",
            "lastsavedtime" => "modified",
            "lastprinted" => "lastPrinted",
            "category" => "category",
            "company" => return self.app("Company").map(str::to_string),
            "manager" => return self.app("Manager").map(str::to_string),
            "hyperlinkbase" => return self.app("HyperlinkBase").map(str::to_string),
            _ => return self.custom.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.display()),
        };
        Some(self.core(core).unwrap_or_default().to_string())
    }
}
