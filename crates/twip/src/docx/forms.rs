//! Legacy form fields (`w:ffData`)

use roxmltree::Node;

use super::write::props::esc;
use super::xml::{attr, child, children, int, val};

/// A legacy form field's settings and value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormData {
    pub name: String,
    pub enabled: bool,
    pub calc_on_exit: bool,
    pub entry_macro: String,
    pub exit_macro: String,
    /// The help shown on F1 (`w:helpText`) and in the status bar (`w:statusText`): own text, or an AutoText entry's.
    pub help: (bool, String),
    pub status: (bool, String),
    pub kind: FormKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormKind {
    /// FORMTEXT: `regular`, `number`, `date`, `currentDate`, `currentTime` or `calculated`; its default text, at most
    /// `max_length` characters (0: any), and its format.
    Text { input: String, default: String, max_length: u32, format: String },
    /// FORMCHECKBOX: its size in points (`None`: its text's), checked by default, checked now (`None`: as by default).
    CheckBox { size: Option<f64>, default: bool, checked: Option<bool> },
    /// FORMDROPDOWN: its entries, the one by default and the one shown (`None`: the default).
    DropDown { entries: Vec<String>, default: u32, result: Option<u32> },
}

impl Default for FormKind {
    fn default() -> FormKind {
        FormKind::Text { input: "regular".into(), default: String::new(), max_length: 0, format: String::new() }
    }
}

fn on(n: Option<Node>) -> Option<bool> {
    n.map(|n| !matches!(attr(n, "val"), Some("0" | "false" | "off")))
}

impl FormData {
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
            enabled: on(child(ff, "enabled")).unwrap_or(true),
            calc_on_exit: on(child(ff, "calcOnExit")).unwrap_or(false),
            entry_macro: s("entryMacro"),
            exit_macro: s("exitMacro"),
            help: text("helpText"),
            status: text("statusText"),
            kind,
        }
    }

    /// The `w:ffData` element (`w`: the prefix with its colon).
    pub fn xml(&self, w: &str) -> String {
        let attr_el = |name: &str, v: &str| if v.is_empty() { String::new() } else { format!("<{w}{name} {w}val=\"{}\"/>", esc(v)) };
        let help = |name: &str, (auto, t): &(bool, String)| {
            if t.is_empty() {
                String::new()
            } else {
                format!("<{w}{name} {w}type=\"{}\" {w}val=\"{}\"/>", if *auto { "autoText" } else { "text" }, esc(t))
            }
        };
        let flag = |b: bool| if b { "1" } else { "0" };
        let mut s = format!("<{w}ffData>{}", attr_el("name", &self.name));
        s.push_str(&if self.enabled { format!("<{w}enabled/>") } else { format!("<{w}enabled {w}val=\"0\"/>") });
        s.push_str(&format!("<{w}calcOnExit {w}val=\"{}\"/>", flag(self.calc_on_exit)));
        s.push_str(&attr_el("entryMacro", &self.entry_macro));
        s.push_str(&attr_el("exitMacro", &self.exit_macro));
        s.push_str(&help("helpText", &self.help));
        s.push_str(&help("statusText", &self.status));
        match &self.kind {
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

    /// A drop-down's shown entry.
    pub fn shown(&self) -> Option<String> {
        let FormKind::DropDown { entries, default, result } = &self.kind else { return None };
        let k = result.unwrap_or(*default) as usize;
        entries.get(k).or(entries.first()).cloned()
    }

    /// A check box shows checked.
    pub fn checked(&self) -> Option<bool> {
        match &self.kind {
            FormKind::CheckBox { default, checked, .. } => Some(checked.unwrap_or(*default)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_data_reads_and_writes_back() {
        let xml = r#"<w:ffData xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:name w:val="Agree"/><w:enabled/><w:calcOnExit w:val="0"/><w:statusText w:type="text" w:val="Tick it"/><w:checkBox><w:size w:val="24"/><w:default w:val="0"/><w:checked/></w:checkBox></w:ffData>"#;
        let d = roxmltree::Document::parse(xml).expect("parses");
        let f = FormData::parse(d.root_element());
        assert_eq!((f.name.as_str(), f.checked(), f.status.1.as_str()), ("Agree", Some(true), "Tick it"));
        assert!(matches!(f.kind, FormKind::CheckBox { size: Some(12.0), .. }));
        let again = format!(r#"<x xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{}</x>"#, f.xml("w:"));
        let d = roxmltree::Document::parse(&again).expect("parses");
        assert_eq!(FormData::parse(d.root_element().first_element_child().expect("ffData")), f);
    }
}
