//! Legacy form fields (`w:ffData`)

use roxmltree::Node;

use super::write::props::esc;
use super::xml::{attr, child, children, int, val};

mod data;

pub use data::*;

fn on(n: Option<Node>) -> Option<bool> {
    n.map(|n| !matches!(attr(n, "val"), Some("0" | "false" | "off")))
}

/// A legacy form field's settings and value from its `w:ffData` element `ff`.
pub fn parse(ff: Node) -> FormData {
    let s = |name: &str| child(ff, name).and_then(val).unwrap_or_default().to_string();
    let text = |name: &str| {
        child(ff, name).map_or((false, String::new()), |n| (attr(n, "type") == Some("autoText"), val(n).unwrap_or_default().to_string()))
    };
    let kind = if let Some(cb) = child(ff, "checkBox") {
        let size = child(cb, "size").filter(|_| child(cb, "sizeAuto").is_none()).and_then(|s| int(val(s))).map(|h| h as f64 / 2.0);
        FormKind::CheckBox { size, default: on(child(cb, "default")).unwrap_or(false), checked: on(child(cb, "checked")) }
    } else if let Some(dd) = child(ff, "ddList") {
        let entries = children(dd, "listEntry").map(|e| val(e).unwrap_or_default().to_string()).collect();
        let n = |name: &str| child(dd, name).and_then(|r| int(val(r))).and_then(|v| u32::try_from(v).ok());
        FormKind::DropDown { entries, default: n("default").unwrap_or(0), result: n("result") }
    } else {
        let t = child(ff, "textInput");
        let s = |name: &str| t.and_then(|t| child(t, name)).and_then(val).unwrap_or_default().to_string();
        FormKind::Text {
            input: Some(s("type")).filter(|v| !v.is_empty()).unwrap_or_else(|| "regular".into()),
            default: s("default"),
            max_length: s("maxLength").parse().unwrap_or(0),
            format: s("format"),
        }
    };
    FormData {
        name: s("name"),
        label: child(ff, "label").and_then(|n| int(val(n))),
        tab_index: child(ff, "tabIndex").and_then(|n| int(val(n))),
        enabled: on(child(ff, "enabled")).unwrap_or(true),
        calc_on_exit: on(child(ff, "calcOnExit")).unwrap_or(false),
        entry_macro: s("entryMacro"),
        exit_macro: s("exitMacro"),
        help: text("helpText"),
        status: text("statusText"),
        kind,
    }
}

/// `d`'s `w:ffData` element (`w`: the prefix with its colon).
pub fn xml(d: &FormData, w: &str) -> String {
    let attr_el = |name: &str, v: &str| if v.is_empty() { String::new() } else { format!("<{w}{name} {w}val=\"{}\"/>", esc(v)) };
    let help = |name: &str, (auto, t): &(bool, String)| {
        if t.is_empty() {
            String::new()
        } else {
            format!("<{w}{name} {w}type=\"{}\" {w}val=\"{}\"/>", if *auto { "autoText" } else { "text" }, esc(t))
        }
    };
    let flag = |b: bool| if b { "1" } else { "0" };
    let mut s = format!("<{w}ffData>{}", attr_el("name", &d.name));
    for (name, v) in [("label", d.label), ("tabIndex", d.tab_index)] {
        if let Some(v) = v {
            s.push_str(&format!("<{w}{name} {w}val=\"{v}\"/>"));
        }
    }
    s.push_str(&if d.enabled { format!("<{w}enabled/>") } else { format!("<{w}enabled {w}val=\"0\"/>") });
    s.push_str(&format!("<{w}calcOnExit {w}val=\"{}\"/>", flag(d.calc_on_exit)));
    s.push_str(&attr_el("entryMacro", &d.entry_macro));
    s.push_str(&attr_el("exitMacro", &d.exit_macro));
    s.push_str(&help("helpText", &d.help));
    s.push_str(&help("statusText", &d.status));
    match &d.kind {
        FormKind::Text { input, default, max_length, format } => {
            s.push_str(&format!("<{w}textInput>"));
            if input != "regular" {
                s.push_str(&attr_el("type", input));
            }
            s.push_str(&attr_el("default", default));
            if *max_length > 0 {
                s.push_str(&format!("<{w}maxLength {w}val=\"{max_length}\"/>"));
            }
            s.push_str(&attr_el("format", format));
            s.push_str(&format!("</{w}textInput>"));
        }
        FormKind::CheckBox { size, default, checked } => {
            let size = match size {
                Some(pt) => format!("<{w}size {w}val=\"{}\"/>", (pt * 2.0).round() as i64),
                None => format!("<{w}sizeAuto/>"),
            };
            let checked = checked.map_or(String::new(), |c| format!("<{w}checked {w}val=\"{}\"/>", flag(c)));
            s.push_str(&format!("<{w}checkBox>{size}<{w}default {w}val=\"{}\"/>{checked}</{w}checkBox>", flag(*default)));
        }
        FormKind::DropDown { entries, default, result } => {
            s.push_str(&format!("<{w}ddList>"));
            if let Some(r) = result {
                s.push_str(&format!("<{w}result {w}val=\"{r}\"/>"));
            }
            if *default > 0 {
                s.push_str(&format!("<{w}default {w}val=\"{default}\"/>"));
            }
            for e in entries {
                s.push_str(&format!("<{w}listEntry {w}val=\"{}\"/>", esc(e)));
            }
            s.push_str(&format!("</{w}ddList>"));
        }
    }
    s.push_str(&format!("</{w}ffData>"));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_data_reads_and_writes_back() {
        let xml = r#"<w:ffData xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:name w:val="Agree"/><w:label w:val="3"/><w:tabIndex w:val="7"/><w:enabled/><w:calcOnExit w:val="0"/><w:statusText w:type="text" w:val="Tick it"/><w:checkBox><w:size w:val="24"/><w:default w:val="0"/><w:checked/></w:checkBox></w:ffData>"#;
        let d = roxmltree::Document::parse(xml).expect("parses");
        let f = parse(d.root_element());
        assert_eq!((f.name.as_str(), f.checked(), f.status.1.as_str()), ("Agree", Some(true), "Tick it"));
        assert!(matches!(f.kind, FormKind::CheckBox { size: Some(12.0), .. }));
        assert_eq!((f.label, f.tab_index), (Some(3), Some(7)));
        let again = format!(r#"<x xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{}</x>"#, super::xml(&f, "w:"));
        let d = roxmltree::Document::parse(&again).expect("parses");
        assert_eq!(parse(d.root_element().first_element_child().expect("ffData")), f);
    }
}
