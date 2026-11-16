//! Document properties: core, extended and custom (`docProps/`)

use super::write::props::esc as esc_text;

pub const CORE: &str = "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
pub const APP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";
pub const CUSTOM: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
const CUSTOM_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
const VT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
const FMTID: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";

/// The core and extended properties the model holds, by their element names.
pub const CORE_NAMES: &[(&str, &str)] = &[
    ("title", "dc:title"),
    ("subject", "dc:subject"),
    ("creator", "dc:creator"),
    ("keywords", "cp:keywords"),
    ("description", "dc:description"),
    ("lastModifiedBy", "cp:lastModifiedBy"),
    ("revision", "cp:revision"),
    ("created", "dcterms:created"),
    ("modified", "dcterms:modified"),
    ("lastPrinted", "cp:lastPrinted"),
    ("category", "cp:category"),
    ("contentStatus", "cp:contentStatus"),
];
pub const APP_NAMES: &[&str] = &["Company", "Manager", "HyperlinkBase"];

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

fn text_of(dom: &roxmltree::Document, local: &str) -> Option<String> {
    dom.root_element().children().find(|c| c.is_element() && c.tag_name().name() == local).map(|n| n.text().unwrap_or_default().to_string())
}

/// Reads the properties from the parts' XML.
pub fn read(core: Option<&str>, app: Option<&str>, custom: Option<&str>) -> Properties {
    let mut p = Properties::default();
    if let Some(dom) = core.and_then(|x| roxmltree::Document::parse(x).ok()) {
        p.core = CORE_NAMES.iter().filter_map(|(n, _)| Some((*n, text_of(&dom, n).filter(|v| !v.is_empty())?))).collect();
    }
    if let Some(dom) = app.and_then(|x| roxmltree::Document::parse(x).ok()) {
        p.app = APP_NAMES.iter().filter_map(|n| Some((*n, text_of(&dom, n).filter(|v| !v.is_empty())?))).collect();
    }
    if let Some(dom) = custom.and_then(|x| roxmltree::Document::parse(x).ok()) {
        for prop in dom.root_element().children().filter(|c| c.tag_name().name() == "property") {
            let (Some(name), Some(v)) = (prop.attribute("name"), prop.children().find(|c| c.is_element())) else { continue };
            let t = v.text().unwrap_or_default().trim().to_string();
            let value = match v.tag_name().name() {
                "bool" => Value::Bool(matches!(t.as_str(), "true" | "1")),
                "i1" | "i2" | "i4" | "i8" | "int" | "ui1" | "ui2" | "ui4" | "ui8" | "uint" | "r4" | "r8" | "decimal" => {
                    t.parse().map_or(Value::Text(t), Value::Number)
                }
                "filetime" | "date" => Value::Date(t),
                _ => Value::Text(t),
            };
            p.custom.push((name.to_string(), value));
        }
    }
    p
}

/// `xml` with the text of its root's child `qname` set (`None`: removed), added before the root's end when missing.
fn set_child(xml: &mut String, qname: &str, attrs: &str, v: Option<&str>) {
    let local = qname.rsplit(':').next().unwrap_or(qname);
    let el = format!("<{qname}{attrs}>{}</{qname}>", esc_text(v.unwrap_or_default()));
    let dom = roxmltree::Document::parse(xml).ok();
    let at = dom.as_ref().and_then(|d| d.root_element().children().find(|c| c.is_element() && c.tag_name().name() == local)).map(|n| n.range());
    let end = dom.as_ref().map(|d| d.root_element().range().end);
    drop(dom);
    match (at, v) {
        (Some(r), Some(_)) => xml.replace_range(r, &el),
        (Some(r), None) => xml.replace_range(r, ""),
        (None, Some(_)) => {
            if let Some(close) = end.and_then(|e| xml[..e].rfind("</")) {
                xml.insert_str(close, &el);
            }
        }
        (None, None) => {}
    }
}

/// The core properties part with `p`'s values (others it holds kept).
pub fn core_xml(src: Option<&str>, p: &Properties) -> String {
    let mut xml = src.map(str::to_string).unwrap_or_else(|| {
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" "#,
            r#"xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"></cp:coreProperties>"#
        )
        .to_string()
    });
    for (name, qname) in CORE_NAMES {
        let attrs = if qname.starts_with("dcterms:") { r#" xsi:type="dcterms:W3CDTF""# } else { "" };
        set_child(&mut xml, qname, attrs, p.core(name));
    }
    xml
}

/// The extended properties part with `p`'s company, manager and hyperlink base.
pub fn app_xml(src: Option<&str>, p: &Properties) -> String {
    let mut xml = src.map(str::to_string).unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"></Properties>"#.to_string()
    });
    for name in APP_NAMES {
        set_child(&mut xml, name, "", p.app(name));
    }
    xml
}

/// The custom properties part.
pub fn custom_xml(p: &Properties) -> String {
    let props: String = p
        .custom
        .iter()
        .enumerate()
        .map(|(k, (name, v))| {
            let value = match v {
                Value::Text(s) => format!("<vt:lpwstr>{}</vt:lpwstr>", esc_text(s)),
                Value::Number(n) if n.fract() == 0.0 && n.abs() < 2e9 => format!("<vt:i4>{}</vt:i4>", *n as i64),
                Value::Number(n) => format!("<vt:r8>{n}</vt:r8>"),
                Value::Bool(b) => format!("<vt:bool>{b}</vt:bool>"),
                Value::Date(d) => format!("<vt:filetime>{}</vt:filetime>", esc_text(d)),
            };
            format!(r#"<property fmtid="{FMTID}" pid="{}" name="{}">{value}</property>"#, k + 2, esc_text(name).replace('"', "&quot;"))
        })
        .collect();
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="{CUSTOM_NS}" xmlns:vt="{VT}">{props}</Properties>"#)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_read_and_write_back() {
        let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Plan</dc:title><dc:language>en</dc:language></cp:coreProperties>"#;
        let mut p = read(Some(core), None, None);
        assert_eq!(p.lookup("TITLE").as_deref(), Some("Plan"));
        p.set_core("subject", Some("Q4 & more".into()));
        p.set_core("title", None);
        p.custom.push(("Client".into(), Value::Text("Acme".into())));
        let xml = core_xml(Some(core), &p);
        assert!(xml.contains("<dc:subject>Q4 &amp; more</dc:subject>") && !xml.contains("Plan") && xml.contains("<dc:language>en"), "{xml}");
        let again = read(Some(&xml), None, Some(&custom_xml(&p)));
        assert_eq!((again.lookup("subject"), again.lookup("client")), (Some("Q4 & more".into()), Some("Acme".into())));
    }
}
