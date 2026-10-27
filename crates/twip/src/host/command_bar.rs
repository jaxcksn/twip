//! The command bar: the static tabs, every item's state from [`Editor::format_state`], and carrying
//! items out. Document commands go to the editor by id ([`Command::from_host`]); dialogs, files and the clipboard come
//! back as host command names; view settings (zoom, gridlines, the border pen, the last colours) are the host's.

use std::fmt::Write as _;

use super::{Engine, json_str};
use crate::docx::props::{Jc, VertAlign};
use crate::editing::state::ObjectKind;

/// The tabs, groups and items, as hosts read them (JSON).
pub const TABS: &str = include_str!("command_bar_tabs.json");

/// Items whose UI is the host's: id → host command.
const HOST: [(&str, &str); 19] = [
    ("home.paste", "Paste"),
    ("home.cut", "Cut"),
    ("home.copy", "Copy"),
    ("home.font-dialog", "Font"),
    ("home.paragraph-dialog", "Paragraph"),
    ("layout.paragraph-dialog", "Paragraph"),
    ("home.find", "Find"),
    ("home.replace", "Replace"),
    ("insert.picture", "InsertPicture"),
    ("insert.table-dialog", "InsertTable"),
    ("insert.link", "Hyperlink"),
    ("layout.page-setup", "PageSetup"),
    ("review.accessibility", "Accessibility"),
    ("view.page-width", "ZoomPageWidth"),
    ("view.one-page", "ZoomOnePage"),
    ("table.properties", "TableProperties"),
    ("picture.size", "PictureSize"),
    ("picture.alt-text", "AltText"),
    ("review.spelling", "NextMisspelling"),
];

/// Ids deliberately not supported, with why (host/unsupported_commands.md).
pub const UNSUPPORTED: [(&str, &str); 11] = [
    ("insert.bookmark", "A new bookmark has no unit of its own for saving to place (zero-width inlines aren't written yet)."),
    ("insert.placeholder", "Designing placeholder controls belongs with the deferred template and merge work."),
    ("mail.", "Mail merge is deferred (user decision, 2026-10-24)."),
    ("mailings", "Mail merge is deferred (user decision, 2026-10-24)."),
    ("picture.wrap.inline", "Turning a floating picture inline is not done."),
    ("picture.position.", "Float alignment presets are not done."),
    ("picture.image-field", "Merge image options are part of the deferred mail merge."),
    ("layout.break.next-page", "Section breaks: saving can't add sections yet."),
    ("layout.break.continuous", "Section breaks: saving can't add sections yet."),
    ("layout.break.even-page", "Section breaks: saving can't add sections yet."),
    ("layout.break.odd-page", "Section breaks: saving can't add sections yet."),
];

/// Why `id` is not supported, if it isn't.
pub fn unsupported(id: &str) -> Option<&'static str> {
    UNSUPPORTED.iter().find(|(p, _)| if p.ends_with('.') { id.starts_with(p) } else { id == *p }).map(|(_, r)| *r)
}

const PEN_STYLES: [(&str, &str); 5] =
    [("Single line", "single"), ("Double line", "double"), ("Dotted", "dotted"), ("Dashed", "dashed"), ("Thick", "thick")];
/// Pen weights: label and eighths of a point.
const PEN_WEIGHTS: [(&str, u32); 9] =
    [("¼ pt", 2), ("½ pt", 4), ("¾ pt", 6), ("1 pt", 8), ("1½ pt", 12), ("2¼ pt", 18), ("3 pt", 24), ("4½ pt", 36), ("6 pt", 48)];
/// The Borders menu's entries, in order (`table.borders.N`).
const BORDERS: [&str; 10] = ["all", "outside", "inside", "top", "bottom", "left", "right", "inside-h", "inside-v", "none"];
const TABLE_LOOK: [(&str, &str); 6] = [
    ("header", "Header row"),
    ("total", "Total row"),
    ("banded-rows", "Banded rows"),
    ("first-col", "First column"),
    ("last-col", "Last column"),
    ("banded-cols", "Banded columns"),
];
const HIGHLIGHTS: [&str; 15] = [
    "yellow",
    "green",
    "cyan",
    "magenta",
    "blue",
    "red",
    "darkBlue",
    "darkCyan",
    "darkGreen",
    "darkMagenta",
    "darkRed",
    "darkYellow",
    "darkGray",
    "lightGray",
    "black",
];

/// View settings the command bar toggles that are not document content.
pub struct ViewOptions {
    pub gridlines: bool,
    pub check_spelling: bool,
    pub ruler: bool,
    /// The last font colour and highlight picked (the split buttons apply them again).
    pub font_color: Option<u32>,
    pub highlight: String,
    /// The table border pen: style (`w:val`), weight (eighths of a point), colour.
    pub pen_style: &'static str,
    pub pen_size: u32,
    pub pen_color: Option<u32>,
}

impl Default for ViewOptions {
    fn default() -> ViewOptions {
        ViewOptions {
            gridlines: true,
            check_spelling: true,
            ruler: true,
            font_color: Some(0xFF0000),
            highlight: "yellow".into(),
            pen_style: "single",
            pen_size: 4,
            pen_color: None,
        }
    }
}

/// A menu entry, as hosts read it.
pub fn entry(id: &str, label: &str, disabled: bool, checked: Option<bool>) -> String {
    let mut s = format!("{{\"id\":{},\"label\":{},\"bold\":false,\"disabled\":{disabled}", json_str(id), json_str(label));
    if let Some(c) = checked {
        let _ = write!(s, ",\"checked\":{c}");
    }
    s + "}"
}

fn sep() -> String {
    entry("-", "", false, None)
}

/// "darkBlue" → "Dark blue".
fn pretty(camel: &str) -> String {
    camel
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                c.to_uppercase().to_string()
            } else if c.is_uppercase() {
                format!(" {}", c.to_lowercase())
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Every item id of the tabs (the items, not their menus' entries), in order.
fn item_ids() -> &'static [String] {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        let tabs: serde_json::Value = serde_json::from_str(TABS).unwrap_or_default();
        let mut out = Vec::new();
        for tab in tabs.as_array().into_iter().flatten() {
            for g in tab["groups"].as_array().into_iter().flatten() {
                for c in g["commands"].as_array().into_iter().flatten() {
                    if let Some(id) = c["id"].as_str() {
                        out.push(id.to_string());
                    }
                }
            }
        }
        out
    })
}

/// One item's state: enabled, checked (toggles), value (choosers, colours), menu items.
#[derive(Clone)]
struct St {
    enabled: bool,
    checked: Option<bool>,
    value: Option<String>,
    items: Option<String>,
}

/// Table Style Options as on/off: header, total, banded rows, first column, last column, banded columns (Word's
/// default without `w:tblLook`: header row, banded rows, first column).
fn look_of(t: &crate::docx::Table) -> [bool; 6] {
    t.look.map_or([true, false, true, true, false, false], |l| [l.first_row, l.last_row, !l.no_h_band, l.first_col, l.last_col, !l.no_v_band])
}

fn hex(c: u32) -> String {
    format!("{c:06X}")
}

fn color_arg(arg: &str) -> Option<u32> {
    u32::from_str_radix(arg.trim().trim_start_matches('#'), 16).ok()
}

impl Engine {
    pub fn command_bar_tabs(&self) -> &'static str {
        TABS
    }

    /// Visible tabs, the tab to switch to and every item's state, as JSON.
    pub fn command_bar_state(&mut self) -> String {
        let zoom = self.zoom;
        let spelling = self.speller.is_some() && self.view.check_spelling;
        let Some(e) = &mut self.editor else { return "{}".into() };
        let Some(f) = e.format_state() else { return "{}".into() };
        let doc = e.document();
        let mut st: Vec<(&str, St)> =
            item_ids().iter().map(|id| (id.as_str(), St { enabled: unsupported(id).is_none(), checked: None, value: None, items: None })).collect();
        let mut set = |id: &str, checked: Option<bool>, enabled: Option<bool>, value: Option<String>, items: Option<String>| {
            let Some((_, x)) = st.iter_mut().find(|(i, _)| *i == id) else { return };
            x.checked = checked.or(x.checked.take());
            x.enabled = enabled.unwrap_or(true) && x.enabled;
            x.value = value.or(x.value.take());
            x.items = items.or(x.items.take());
        };
        let r = &f.run;
        let hf = matches!(f.story, crate::editing::position::StoryId::HeaderFooter(_));
        set("home.undo", None, Some(f.can_undo), None, None);
        set("home.redo", None, Some(f.can_redo), None, None);
        set("home.cut", None, Some(f.has_selection), None, None);
        set("home.copy", None, Some(f.has_selection), None, None);
        set("home.font", None, None, Some(r.ascii.clone()), None);
        set("home.font-size", None, None, Some(format!("{}", (r.size * 10.0).round() / 10.0)), None);
        let styles: Vec<String> = quick_styles(doc).iter().map(|(id, name)| entry(id, name, false, None)).collect();
        set("home.style", None, None, Some(f.para.style_id.clone().unwrap_or_else(|| "Normal".into())), Some(styles.join(",")));
        for (id, on) in ["home.bold", "home.italic", "home.underline", "home.strike", "home.subscript", "home.superscript"].iter().zip(f.toggles) {
            set(id, Some(on), None, None, None);
        }
        set("home.font-color", None, None, self.view.font_color.map(hex), None);
        let mut hl: Vec<String> = HIGHLIGHTS.iter().map(|h| entry(&format!("home.highlight.{h}"), &pretty(h), false, None)).collect();
        hl.push(sep());
        hl.push(entry("home.highlight.none", "No color", false, None));
        set("home.highlight", None, None, Some(self.view.highlight.clone()), Some(hl.join(",")));
        set("home.bullets", Some(f.list == Some(true)), None, None, None);
        set("home.numbering", Some(f.list == Some(false)), None, None, None);
        let jc = f.para.jc;
        set("home.align-left", Some(jc == Jc::Left), None, None, None);
        set("home.align-center", Some(jc == Jc::Center), None, None, None);
        set("home.align-right", Some(jc == Jc::Right), None, None, None);
        set("home.align-justify", Some(matches!(jc, Jc::Both | Jc::Distribute)), None, None, None);
        set("layout.keep-with-next", Some(f.para.keep_next), None, None, None);
        set("review.spelling", None, Some(spelling), None, None);
        set("review.check-as-you-type", Some(self.view.check_spelling), None, None, None);
        set("view.ruler", Some(self.view.ruler), None, None, None);
        set("view.zoom-100", None, None, Some(format!("{}%", (zoom * 100.0).round())), None);
        set("table.gridlines", Some(self.view.gridlines), None, None, None);
        let table = f.table.filter(|_| !hf).and_then(|t| e.index().table(e.document(), t));
        if let Some(t) = table {
            set("table.styles", None, None, t.style_id.clone(), None);
            let on = look_of(t);
            let items: Vec<String> =
                TABLE_LOOK.iter().zip(on).map(|((id, label), c)| entry(&format!("table.look.{id}"), label, false, Some(c))).collect();
            set("table.style-options", None, None, None, Some(items.join(",")));
        }
        let items: Vec<String> = PEN_STYLES
            .iter()
            .enumerate()
            .map(|(i, (l, v))| entry(&format!("table.pen-style.{i}"), l, false, Some(*v == self.view.pen_style)))
            .collect();
        set("table.pen-style", None, None, None, Some(items.join(",")));
        let items: Vec<String> = PEN_WEIGHTS
            .iter()
            .enumerate()
            .map(|(i, (l, v))| entry(&format!("table.pen-weight.{i}"), l, false, Some(*v == self.view.pen_size)))
            .collect();
        set("table.pen-weight", None, None, None, Some(items.join(",")));
        set("table.pen-color", None, None, self.view.pen_color.map(hex), None);
        if hf && let Some(s) = doc.sections.get(f.section) {
            set("hf.different-first", Some(s.title_pg), None, None, None);
        }
        let _ = VertAlign::Baseline;
        // A read-only document disables what would change it: ids parsed with no argument or a
        // sample one, and the host's edits and editing dialogs.
        if e.mode() == crate::editing::Mode::ReadOnly {
            use crate::editing::commands::Command;
            const VIEWING: [&str; 6] = ["home.copy", "home.find", "review.accessibility", "view.page-width", "view.one-page", "review.spelling"];
            for (id, x) in st.iter_mut() {
                let parsed = [None, Some(""), Some("12"), Some("2,2")]
                    .into_iter()
                    .find_map(|a| Command::from_host(id, a.map(serde_json::Value::from).as_ref()).ok());
                let host_edit = HOST.iter().any(|(h, _)| h == id) && !VIEWING.contains(id);
                if host_edit || parsed.is_some_and(|c| c.mutates()) {
                    x.enabled = false;
                }
            }
        }

        let mut tabs = vec!["home", "insert", "layout", "mailings", "review", "view"];
        if table.is_some() {
            tabs.extend(["table-design", "table-layout"]);
        }
        if !hf && matches!(f.object, Some(ObjectKind::Picture | ObjectKind::Float)) {
            tabs.push("picture");
        }
        if hf {
            tabs.push("header-footer");
        }
        let activate = if hf { ",\"activate\":\"header-footer\"" } else { "" };
        let mut out = format!("{{\"tabs\":[{}]{activate},\"commands\":{{", tabs.iter().map(|t| json_str(t)).collect::<Vec<_>>().join(","));
        for (k, (id, x)) in st.iter().enumerate() {
            if k > 0 {
                out.push(',');
            }
            let _ = write!(out, "{}:{{\"enabled\":{}", json_str(id), x.enabled);
            if let Some(c) = x.checked {
                let _ = write!(out, ",\"checked\":{c}");
            }
            if let Some(v) = &x.value {
                let _ = write!(out, ",\"value\":{}", json_str(v));
            }
            if let Some(i) = &x.items {
                let _ = write!(out, ",\"items\":[{i}]");
            }
            out.push('}');
        }
        out + "}}"
    }

    /// Carries out an item or menu entry (`arg`: a chooser's value, a colour, "rows,cols", a symbol, a style id):
    /// JSON `{ command }` for what the host must do, or `{}`.
    pub fn command_bar_execute(&mut self, id: &str, arg: Option<&str>) -> String {
        let host = |c: &str| format!("{{\"command\":{}}}", json_str(c));
        if let Some((_, c)) = HOST.iter().find(|(i, _)| *i == id) {
            if *c == "NextMisspelling" {
                return host(&self.spelling_step());
            }
            return host(c);
        }
        if unsupported(id).is_some() {
            return "{}".into();
        }
        let index = |prefix: &str| id.strip_prefix(prefix).and_then(|k| k.parse::<usize>().ok());
        let pen =
            |v: &super::command_bar::ViewOptions| format!("{},{},{}", v.pen_style, v.pen_size as f64 / 8.0, v.pen_color.map_or("auto".into(), hex));
        match id {
            "view.zoom-out" => self.zoom = (((self.zoom - 0.1) * 10.0).round() / 10.0).max(0.25),
            "view.zoom-in" => self.zoom = (((self.zoom + 0.1) * 10.0).round() / 10.0).min(4.0),
            "view.zoom-100" => self.zoom = 1.0,
            "view.ruler" => self.view.ruler = !self.view.ruler,
            "table.gridlines" => self.view.gridlines = !self.view.gridlines,
            "review.check-as-you-type" => self.view.check_spelling = !self.view.check_spelling,
            "home.font-color" => {
                if let Some(a) = arg {
                    self.view.font_color = color_arg(a);
                }
                let c = self.view.font_color.map_or("auto".to_string(), hex);
                self.command("home.font-color", Some(&c));
            }
            "home.highlight" => {
                let h = arg.filter(|a| !a.is_empty()).map_or(self.view.highlight.clone(), str::to_string);
                self.command("home.highlight", Some(&h));
            }
            "home.highlight.none" => {
                self.command("home.highlight", Some("none"));
            }
            h if h.starts_with("home.highlight.") => {
                self.view.highlight = h["home.highlight.".len()..].to_string();
                let v = self.view.highlight.clone();
                self.command("home.highlight", Some(&v));
            }
            p if p.starts_with("table.pen-style.") => {
                if let Some(k) = index("table.pen-style.").filter(|&k| k < PEN_STYLES.len()) {
                    self.view.pen_style = PEN_STYLES[k].1;
                }
            }
            p if p.starts_with("table.pen-weight.") => {
                if let Some(k) = index("table.pen-weight.").filter(|&k| k < PEN_WEIGHTS.len()) {
                    self.view.pen_size = PEN_WEIGHTS[k].1;
                }
            }
            "table.pen-color" => self.view.pen_color = arg.and_then(color_arg),
            b if b.starts_with("table.borders.") && index("table.borders.").is_some() => {
                if let Some(edge) = index("table.borders.").and_then(|k| BORDERS.get(k)) {
                    let p = pen(&self.view);
                    self.command(&format!("table.borders.{edge}"), Some(&p));
                }
            }
            "table.styles" => {
                if let Some(a) = arg.filter(|a| !a.is_empty()) {
                    self.command("table.style", Some(a));
                }
            }
            o if o.starts_with("table.look.") => {
                let Some(e) = &mut self.editor else { return "{}".into() };
                let Some(t) = e.format_state().and_then(|f| f.table).and_then(|t| e.index().table(e.document(), t)) else { return "{}".into() };
                let mut on = look_of(t);
                if let Some(k) = TABLE_LOOK.iter().position(|(i, _)| *i == &o["table.look.".len()..]) {
                    on[k] = !on[k];
                }
                let names = ["header-row", "total-row", "banded-rows", "first-column", "last-column", "banded-columns"];
                let arg: Vec<&str> = names.iter().zip(on).filter(|(_, c)| *c).map(|(n, _)| *n).collect();
                self.command("table.look", Some(&arg.join(",")));
            }
            w if w.starts_with("picture.wrap.") => {
                let v = match &w["picture.wrap.".len()..] {
                    "square" => "square",
                    "top-bottom" => "topAndBottom",
                    "behind" => "behind",
                    "front" => "front",
                    _ => return "{}".into(),
                };
                self.command("picture.wrap", Some(v));
            }
            _ => {
                self.command(id, arg);
            }
        }
        "{}".into()
    }

    /// Installed font families for the font chooser, as a JSON array.
    pub fn font_families(&self) -> String {
        format!("[{}]", self.fonts.families().iter().map(|f| json_str(f)).collect::<Vec<_>>().join(","))
    }
}

/// The Styles gallery: the built-in quick styles (with the document's names for them) and the document's own quick
/// paragraph styles.
pub fn quick_styles(doc: &crate::docx::Document) -> Vec<(String, String)> {
    const BUILTIN: [(&str, &str); 10] = [
        ("Normal", "Normal"),
        ("NoSpacing", "No Spacing"),
        ("Title", "Title"),
        ("Subtitle", "Subtitle"),
        ("Heading1", "Heading 1"),
        ("Heading2", "Heading 2"),
        ("Heading3", "Heading 3"),
        ("Heading4", "Heading 4"),
        ("Quote", "Quote"),
        ("ListParagraph", "List Paragraph"),
    ];
    let Some(res) = doc.resolver.as_ref() else { return Vec::new() };
    let styles = &res.styles;
    let pretty = |n: &str| {
        let mut c = n.chars();
        c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
    };
    let mut out: Vec<(String, String)> = BUILTIN
        .iter()
        .map(|(id, name)| (id.to_string(), styles.by_id.get(*id).filter(|s| !s.name.is_empty()).map_or(name.to_string(), |s| pretty(&s.name))))
        .collect();
    for id in &styles.paragraph_order {
        let Some(s) = styles.by_id.get(id) else { continue };
        if s.quick && !BUILTIN.iter().any(|b| b.0 == id) {
            out.push((id.clone(), pretty(if s.name.is_empty() { id } else { &s.name })));
        }
    }
    out
}

impl Engine {
    /// The table style gallery: the document's table styles.
    // shortcut: built-in gallery styles aren't offered: applying one needs its definition
    // added to the styles part; add when the gallery should show more than the document's own.
    pub fn table_style_gallery(&self) -> String {
        let Some(res) = self.editor.as_ref().and_then(|e| e.document().resolver.clone()) else { return "[]".into() };
        let mut v: Vec<(&String, &crate::docx::styles::Style)> =
            res.styles.by_id.iter().filter(|(id, s)| s.kind == crate::docx::styles::StyleKind::Table && *id != "TableNormal").collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        let out: Vec<serde_json::Value> = v
            .iter()
            .map(|(id, s)| serde_json::json!({ "id": id, "name": if s.name.is_empty() { id.as_str() } else { s.name.as_str() }, "family": "In this document", "insideH": true, "insideV": true }))
            .collect();
        serde_json::Value::Array(out).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id of the tabs (items and their menus' entries), with whether it is a menu that only opens.
    fn all_ids() -> Vec<(String, bool)> {
        let tabs: serde_json::Value = serde_json::from_str(TABS).unwrap();
        let mut out = Vec::new();
        for tab in tabs.as_array().unwrap() {
            for g in tab["groups"].as_array().unwrap() {
                for c in g["commands"].as_array().unwrap() {
                    let menu = c["kind"] == "Menu";
                    out.push((c["id"].as_str().unwrap().to_string(), menu));
                    for i in c["items"].as_array().into_iter().flatten() {
                        let id = i["id"].as_str().unwrap();
                        if id != "-" {
                            out.push((id.to_string(), false));
                        }
                    }
                }
            }
        }
        out
    }

    #[test]
    fn every_id_is_mapped() {
        // Ids the host carries out itself (view settings, the pen, colours remembered, menus built from state).
        let host_side = |id: &str| {
            id.starts_with("view.")
                || id.starts_with("table.pen-")
                || id.starts_with("table.borders.")
                || id.starts_with("table.look.")
                || id.starts_with("home.highlight")
                || id.starts_with("picture.wrap.")
                || matches!(
                    id,
                    "table.styles" | "table.style-options" | "table.gridlines" | "review.check-as-you-type" | "home.font-color" | "table.borders"
                )
        };
        // A sample argument for ids that take one.
        let arg = |id: &str| -> Option<serde_json::Value> {
            Some(serde_json::Value::String(
                match id {
                    "insert.table" => "2,2",
                    "home.font" => "Arial",
                    "home.font-size" => "12",
                    "home.style" => "Normal",
                    "insert.symbol" => "§",
                    "picture.size" => "10,10",
                    _ => "",
                }
                .into(),
            ))
        };
        let mut missing = Vec::new();
        for (id, menu) in all_ids() {
            let ok = menu
                || HOST.iter().any(|(i, _)| *i == id)
                || unsupported(&id).is_some()
                || host_side(&id)
                || crate::editing::commands::Command::from_host(&id, arg(&id).as_ref()).is_ok();
            if !ok {
                missing.push(id);
            }
        }
        assert!(missing.is_empty(), "command-bar ids without a mapping: {missing:?}");
    }

    #[test]
    fn state_and_execute() {
        let mut e = Engine::with_fonts(crate::font::Fonts::system());
        e.new_document();
        e.text("Hello world");
        e.key("KeyA", true, false, false);
        let st = |e: &mut Engine| serde_json::from_str::<serde_json::Value>(&e.command_bar_state()).unwrap();
        assert_eq!(st(&mut e)["commands"]["home.bold"]["checked"], false);
        assert_eq!(e.command_bar_execute("home.bold", None), "{}");
        assert_eq!(st(&mut e)["commands"]["home.bold"]["checked"], true);
        assert_eq!(st(&mut e)["commands"]["home.copy"]["enabled"], true);
        assert_eq!(e.command_bar_execute("home.find", None), "{\"command\":\"Find\"}");
        assert_eq!(st(&mut e)["commands"]["mail.recipients"]["enabled"], false, "unsupported ids are disabled");
        // Read-only: what edits is disabled, copy and find aren't.
        e.command("view.mode", Some("readonly"));
        let s = st(&mut e);
        for (id, on) in [
            ("home.bold", false),
            ("home.font-size", false),
            ("insert.table", false),
            ("home.paste", false),
            ("home.copy", true),
            ("home.find", true),
            ("view.ruler", true),
        ] {
            assert_eq!(s["commands"][id]["enabled"], on, "{id} in read-only");
        }
        e.command("view.mode", Some("editing"));
        // Page X of Y: one undo step, fields saved and read back.
        e.key("End", true, false, false);
        let depth = e.editor.as_ref().unwrap().history().undo_depth();
        e.command_bar_execute("insert.page-number.page-of", None);
        assert_eq!(e.editor.as_ref().unwrap().history().undo_depth(), depth + 1);
        let fields = |d: &crate::docx::Document| -> usize {
            d.sections
                .iter()
                .flat_map(|s| s.blocks.iter())
                .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
                .flat_map(|p| p.inlines.iter())
                .filter(|i| matches!(i, crate::docx::Inline::FieldBegin { .. }))
                .count()
        };
        assert_eq!(fields(e.editor.as_ref().unwrap().document()), 2);
        let saved = e.save().unwrap();
        let again = crate::editing::Editor::open(&saved, &e.fonts).unwrap();
        assert_eq!(fields(again.document()), 2, "the fields save as fields");
        // Landscape and two columns, saved.
        e.command_bar_execute("layout.orientation.landscape", None);
        e.command_bar_execute("layout.columns.2", None);
        let again = crate::editing::Editor::open(&e.save().unwrap(), &e.fonts).unwrap();
        let s = &again.document().sections[0];
        assert!(s.page_w > s.page_h);
        assert_eq!(s.columns.num, 2);
        // The font chooser and the gallery are JSON arrays.
        assert!(e.font_families().starts_with('['));
        assert!(e.table_style_gallery().starts_with('['));
    }

    #[test]
    fn checkbox_inserts_a_control() {
        let mut e = Engine::with_fonts(crate::font::Fonts::system());
        e.new_document();
        e.text("Agree ");
        e.command_bar_execute("insert.checkbox", None);
        e.text(" yes");
        let saved = e.save().unwrap();
        let xml = {
            use std::io::Read;
            let mut z = zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).unwrap();
            let mut s = String::new();
            z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
            s
        };
        assert!(xml.contains("w14:checkbox") && xml.contains('\u{2610}') && xml.contains("Agree ") && xml.contains(" yes"), "{xml}");
        let again = crate::editing::Editor::open(&saved, &e.fonts).unwrap();
        let p = match &again.document().sections[0].blocks[0] {
            crate::docx::Block::Para(p) => p.clone(),
            _ => panic!(),
        };
        assert!(p.inlines.iter().any(|i| matches!(i, crate::docx::Inline::Sym { control: Some(_), .. })), "read back as a checkbox control");
        assert_eq!(crate::editing::text::UnitText::of(&p).text, "Agree \u{2610} yes");
    }

    #[test]
    fn gridlines_show_and_hide() {
        // View → Gridlines: a borderless table's cell edges drawn dashed light blue, gone when turned off.
        let blue = |on: bool| {
            let mut e = Engine::with_fonts(crate::font::Fonts::system());
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-TBL-NOBORDER.docx");
            e.open(&std::fs::read(path).unwrap());
            if !on {
                e.command_bar_execute("table.gridlines", None);
            }
            let info = skia_safe::ImageInfo::new_n32_premul((900, 600), None);
            let mut surface = skia_safe::surfaces::raster(&info, None, None).unwrap();
            e.draw(surface.canvas(), 0.0, 0.0, 900.0, 600.0, 1.0);
            let mut px = vec![0u8; 900 * 600 * 4];
            assert!(surface.read_pixels(&info, &mut px, 900 * 4, (0, 0)));
            // N32 is BGRA here: the gridline blue (9CC2E5) is high in blue, lower in red.
            px.chunks(4).filter(|p| p[0] > 200 && p[2] < 190 && p[2] > 120).count()
        };
        assert!(blue(true) > 50, "gridlines drawn");
        assert_eq!(blue(false), 0, "no gridlines when off");
    }
}
