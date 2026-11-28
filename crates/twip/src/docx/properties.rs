//! Document properties: core, extended and custom (`docProps/`)

use super::write::props::esc as esc_text;

pub use crate::document::properties::*;

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
        p.custom = custom_props(&dom).into_iter().map(|(n, v, _)| (n, v)).collect();
    }
    p
}

/// The custom part's properties: name, value and element.
fn custom_props(dom: &roxmltree::Document) -> Vec<(String, Value, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    for prop in dom.root_element().children().filter(|c| c.tag_name().name() == "property") {
        let (Some(name), Some(v)) = (prop.attribute("name"), prop.children().find(|c| c.is_element())) else { continue };
        let t = v.text().unwrap_or_default().trim().to_string();
        let value = match v.tag_name().name() {
            "bool" => Value::Bool(matches!(t.as_str(), "true" | "1")),
            "i1" | "i2" | "i4" | "i8" | "int" | "ui1" | "ui2" | "ui4" | "ui8" | "uint" | "r4" | "r8" | "decimal" => {
                // `INF` and `NaN` as their text: a number is finite.
                t.parse().ok().filter(|n: &f64| n.is_finite()).map_or(Value::Text(t), Value::Number)
            }
            "filetime" | "date" => Value::Date(t),
            _ => Value::Text(t),
        };
        out.push((name.to_string(), value, prop.range()));
    }
    out
}

/// The namespaces of the prefixes the properties parts are written with (none: the extended properties').
fn uri(prefix: &str) -> &'static str {
    match prefix {
        "dc" => "http://purl.org/dc/elements/1.1/",
        "cp" => "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
        "dcterms" => "http://purl.org/dc/terms/",
        "xsi" => "http://www.w3.org/2001/XMLSchema-instance",
        _ => "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
    }
}

/// `xml` with the text of its root's child `qname` set (`None`: removed), added before the root's end when missing,
/// `typed` with `xsi:type="dcterms:W3CDTF"`. Written with the prefixes the root gives the namespaces; a namespace the
/// root doesn't declare is declared on the element.
fn set_child(xml: &mut String, qname: &str, typed: bool, v: Option<&str>) {
    let (prefix, local) = qname.split_once(':').unwrap_or(("", qname));
    let Ok(dom) = roxmltree::Document::parse(xml) else { return };
    let root = dom.root_element();
    let mut decls = String::new();
    let mut name = |p: &str, l: &str| {
        let u = uri(p);
        let p = match root.namespaces().find(|ns| ns.uri() == u) {
            Some(ns) => ns.name().unwrap_or(""),
            None => {
                let d = if p.is_empty() { format!(" xmlns=\"{u}\"") } else { format!(" xmlns:{p}=\"{u}\"") };
                if !decls.contains(&d) {
                    decls.push_str(&d);
                }
                p
            }
        };
        if p.is_empty() { l.to_string() } else { format!("{p}:{l}") }
    };
    let el_name = name(prefix, local);
    let attrs = if typed { format!(" {}=\"{}\"", name("xsi", "type"), name("dcterms", "W3CDTF")) } else { String::new() };
    let el = format!("<{el_name}{decls}{attrs}>{}</{el_name}>", esc_text(v.unwrap_or_default()));
    let at = root.children().find(|c| c.is_element() && c.tag_name().name() == local && c.tag_name().namespace() == Some(uri(prefix)));
    let at = at.map(|n| n.range());
    let r = root.range();
    let root_name = xml[r.start + 1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or_default().to_string();
    drop(dom);
    match (at, v) {
        (Some(r), Some(_)) => xml.replace_range(r, &el),
        (Some(r), None) => xml.replace_range(r, ""),
        // An empty root written `<cp:coreProperties .../>` opens for it.
        (None, Some(_)) if xml[..r.end].ends_with("/>") => xml.replace_range(r.end - 2..r.end, &format!(">{el}</{root_name}>")),
        (None, Some(_)) => {
            if let Some(close) = xml[..r.end].rfind("</") {
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
        set_child(&mut xml, qname, qname.starts_with("dcterms:"), p.core(name));
    }
    xml
}

/// The extended properties part with `p`'s company, manager and hyperlink base.
pub fn app_xml(src: Option<&str>, p: &Properties) -> String {
    let mut xml = src.map(str::to_string).unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"></Properties>"#.to_string()
    });
    for name in APP_NAMES {
        set_child(&mut xml, name, false, p.app(name));
    }
    xml
}

/// The custom properties part. A property as read (a list, binary data or a link the model doesn't hold) is written as
/// read, renumbered, while the part's root names its namespaces as this writes them.
pub fn custom_xml(src: Option<&str>, p: &Properties) -> String {
    let dom = src.and_then(|x| roxmltree::Document::parse(x).ok());
    let kept = dom.as_ref().filter(|d| {
        let root = d.root_element();
        root.tag_name().namespace() == Some(CUSTOM_NS)
            && root.lookup_namespace_uri(None) == Some(CUSTOM_NS)
            && root.lookup_namespace_uri(Some("vt")) == Some(VT)
            && !d.input_text()[root.range()].ends_with("/>")
    });
    let read = kept.map(custom_props).unwrap_or_default();
    let props: String = p
        .custom
        .iter()
        .enumerate()
        .map(|(k, (name, v))| {
            let pid = k + 2;
            if let (Some(x), Some((.., r))) = (src, read.iter().find(|(n, w, _)| n == name && w == v)) {
                return with_pid(&x[r.clone()], pid);
            }
            let value = match v {
                Value::Text(s) => format!("<vt:lpwstr>{}</vt:lpwstr>", esc_text(s)),
                Value::Number(n) if n.fract() == 0.0 && n.abs() < 2e9 => format!("<vt:i4>{}</vt:i4>", *n as i64),
                Value::Number(n) => format!("<vt:r8>{n}</vt:r8>"),
                Value::Bool(b) => format!("<vt:bool>{b}</vt:bool>"),
                Value::Date(d) => format!("<vt:filetime>{}</vt:filetime>", esc_text(d)),
            };
            format!(r#"<property fmtid="{FMTID}" pid="{pid}" name="{}">{value}</property>"#, esc_text(name))
        })
        .collect();
    match (src, kept) {
        (Some(x), Some(d)) => {
            let r = d.root_element().range();
            let open = r.start + x[r.start..].find('>').map_or(0, |e| e + 1);
            let close = x[..r.end].rfind("</").unwrap_or(r.end);
            format!("{}{props}{}", &x[..open], &x[close..])
        }
        _ => {
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="{CUSTOM_NS}" xmlns:vt="{VT}">{props}</Properties>"#)
        }
    }
}

/// A `property` element with its `pid` set.
fn with_pid(el: &str, pid: usize) -> String {
    let tag_end = el.find('>').unwrap_or(el.len());
    match el[..tag_end].find(" pid=").map(|k| k + 6) {
        Some(at) if at < el.len() => {
            let quote = el.as_bytes()[at - 1] as char;
            let end = el[at..].find(quote).map_or(at, |e| at + e);
            format!("{}{pid}{}", &el[..at], &el[end..])
        }
        _ => el.to_string(),
    }
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
        let again = read(Some(&xml), None, Some(&custom_xml(None, &p)));
        assert_eq!((again.lookup("subject"), again.lookup("client")), (Some("Q4 & more".into()), Some("Acme".into())));
    }

    #[test]
    fn parts_without_the_prefixes_still_parse_and_unknown_values_stay() {
        // A core part declaring only `cp`, and self-closed.
        let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"/>"#;
        let mut p = Properties::default();
        p.set_core("title", Some("Plan".into()));
        p.set_core("modified", Some("2026-11-16T10:00:00Z".into()));
        let xml = core_xml(Some(core), &p);
        assert!(roxmltree::Document::parse(&xml).is_ok(), "{xml}");
        assert_eq!(read(Some(&xml), None, None).core("title"), Some("Plan"));
        assert_eq!(read(Some(&xml), None, None).core("modified"), Some("2026-11-16T10:00:00Z"));
        // An extended part with its namespace under a prefix.
        let app = r#"<ep:Properties xmlns:ep="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"></ep:Properties>"#;
        p.set_app("Company", Some("Acme".into()));
        let xml = app_xml(Some(app), &p);
        assert!(xml.contains("<ep:Company>Acme</ep:Company>"), "{xml}");
        // A list property is kept when another changes.
        let custom = r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="5" name="Tags"><vt:vector size="1" baseType="lpwstr"><vt:lpwstr>a</vt:lpwstr></vt:vector></property><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="6" name="Client"><vt:lpwstr>Acme</vt:lpwstr></property></Properties>"#;
        let mut q = read(None, None, Some(custom));
        q.custom[1].1 = Value::Text("Beta".into());
        let xml = custom_xml(Some(custom), &q);
        assert!(xml.contains(r#"pid="2" name="Tags"><vt:vector"#) && xml.contains("<vt:lpwstr>Beta</vt:lpwstr>"), "{xml}");
    }

    #[test]
    fn a_number_that_isnt_finite_reads_as_its_text_and_writes_back_as_read() {
        let custom = r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="N"><vt:r8>NaN</vt:r8></property></Properties>"#;
        let p = read(None, None, Some(custom));
        assert_eq!(p.custom, vec![("N".to_string(), Value::Text("NaN".into()))]);
        assert_eq!(p, read(None, None, Some(custom)), "the same read twice is equal");
        assert!(custom_xml(Some(custom), &p).contains("<vt:r8>NaN</vt:r8>"));
    }
}
