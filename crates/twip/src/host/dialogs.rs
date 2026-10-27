//! The editor's dialogs as data: Font, Paragraph, Page setup, Insert table, Table properties and Picture size.
//! `open` builds one from [`Editor::format_state`], `update` rebuilds it while it is edited (dependent fields),
//! `apply` carries it out as one command (one undo step, a cancelled dialog runs nothing). Hosts read and send them
//! as JSON.

use crate::docx::props::{Jc, LineRule, VertAlign, Width};
use crate::editing::commands::Command;
use crate::editing::format::{FontValues, Format, ParaValues};
use crate::editing::section::SectionCmd;
use crate::editing::table_format::TableFormat;

use super::Engine;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Text,
    Number,
    Choice,
    Combo,
    Check,
    Note,
    Heading,
    List,
}

/// A dialog field (values are strings: numbers invariant, checks "true"/"false", choices the option text).
#[derive(Clone, Debug)]
pub struct Field {
    pub id: String,
    pub kind: Kind,
    pub label: String,
    pub value: Option<String>,
    pub options: Option<Vec<String>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub enabled: bool,
}

/// A dialog: rows of fields, the accept button, an error, and hidden values the host sends back (`$...`).
#[derive(Clone, Debug)]
pub struct Model {
    pub id: String,
    pub title: String,
    pub rows: Vec<Vec<Field>>,
    pub accept: String,
    pub can_cancel: bool,
    pub error: Option<String>,
    pub state: serde_json::Map<String, serde_json::Value>,
}

impl Field {
    fn json(&self) -> serde_json::Value {
        let mut o = serde_json::json!({ "id": self.id, "kind": format!("{:?}", self.kind), "label": self.label, "enabled": self.enabled });
        if let Some(v) = &self.value {
            o["value"] = v.clone().into();
        }
        if let Some(v) = &self.options {
            o["options"] = v.clone().into();
        }
        for (k, v) in [("min", self.min), ("max", self.max), ("step", self.step)] {
            if let Some(v) = v {
                o[k] = v.into();
            }
        }
        o
    }
}

impl Model {
    pub fn json(&self) -> serde_json::Value {
        let rows: Vec<Vec<serde_json::Value>> = self.rows.iter().map(|r| r.iter().map(Field::json).collect()).collect();
        let mut o = serde_json::json!({ "id": self.id, "title": self.title, "rows": rows, "accept": self.accept, "canCancel": self.can_cancel, "state": self.state });
        if let Some(e) = &self.error {
            o["error"] = e.clone().into();
        }
        o
    }
}

/// A dialog's values while it is edited; `changed` is the field just changed.
#[derive(Default, Clone)]
pub struct Values {
    v: serde_json::Map<String, serde_json::Value>,
    changed: Option<String>,
}

impl Values {
    fn of(pairs: &[(&str, String)]) -> Values {
        Values { v: pairs.iter().map(|(k, v)| (k.to_string(), serde_json::Value::String(v.clone()))).collect(), changed: None }
    }

    /// Values from a JSON object (strings; other values as written).
    pub fn parse(json: &str, changed: Option<&str>) -> Values {
        let v = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json).unwrap_or_default();
        let v = v
            .into_iter()
            .map(|(k, x)| {
                (k, serde_json::Value::String(x.as_str().map_or_else(|| if x.is_null() { String::new() } else { x.to_string() }, str::to_string)))
            })
            .collect();
        Values { v, changed: changed.map(str::to_string) }
    }

    fn get(&self, id: &str) -> &str {
        self.v.get(id).and_then(|x| x.as_str()).unwrap_or("")
    }

    fn bool(&self, id: &str) -> bool {
        self.get(id) == "true"
    }

    fn num(&self, id: &str, fallback: f64) -> f64 {
        self.get(id).trim().parse().unwrap_or(fallback)
    }

    fn set(&mut self, id: &str, value: String) {
        self.v.insert(id.to_string(), serde_json::Value::String(value));
    }

    fn changed(&self, id: &str) -> bool {
        self.changed.as_deref() == Some(id)
    }
}

fn n(d: f64) -> String {
    let s = format!("{:.3}", d);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn b(v: bool) -> String {
    (if v { "true" } else { "false" }).into()
}

fn field(id: &str, kind: Kind, label: &str, v: &Values) -> Field {
    Field {
        id: id.into(),
        kind,
        label: label.into(),
        value: Some(v.get(id).to_string()),
        options: None,
        min: None,
        max: None,
        step: None,
        enabled: true,
    }
}

fn num(id: &str, label: &str, v: &Values, min: f64, max: f64, step: f64, enabled: bool) -> Field {
    Field { min: Some(min), max: Some(max), step: Some(step), enabled, ..field(id, Kind::Number, label, v) }
}

fn choice(id: &str, label: &str, v: &Values, options: &[&str]) -> Field {
    Field { options: Some(options.iter().map(|s| s.to_string()).collect()), ..field(id, Kind::Choice, label, v) }
}

fn combo(id: &str, label: &str, v: &Values, options: Vec<String>) -> Field {
    Field { options: Some(options), ..field(id, Kind::Combo, label, v) }
}

fn check(id: &str, label: &str, v: &Values) -> Field {
    field(id, Kind::Check, label, v)
}

fn heading(t: &str) -> Field {
    Field {
        id: format!("heading:{t}"),
        kind: Kind::Heading,
        label: t.into(),
        value: None,
        options: None,
        min: None,
        max: None,
        step: None,
        enabled: true,
    }
}

/// A dialog with its hidden values (`$...`).
fn model(v: &Values, id: &str, title: &str, rows: Vec<Vec<Field>>, accept: &str) -> Model {
    Model {
        id: id.into(),
        title: title.into(),
        rows,
        accept: accept.into(),
        can_cancel: true,
        error: None,
        state: v.v.iter().filter(|(k, _)| k.starts_with('$')).map(|(k, x)| (k.clone(), x.clone())).collect(),
    }
}

const IN: f64 = 72.0;
const UNDERLINES: [(&str, &str); 9] = [
    ("None", "none"),
    ("Single", "single"),
    ("Words only", "words"),
    ("Double", "double"),
    ("Thick", "thick"),
    ("Dotted", "dotted"),
    ("Dash", "dash"),
    ("Dot dash", "dotDash"),
    ("Wave", "wave"),
];
const ALIGNMENTS: [&str; 4] = ["Left", "Centered", "Right", "Justified"];
const SPECIALS: [&str; 3] = ["(none)", "First line", "Hanging"];
const LINE_RULES: [&str; 3] = ["Multiple", "At least", "Exactly"];
const TABLE_ALIGNMENTS: [&str; 3] = ["Left", "Center", "Right"];
const WIDTH_KINDS: [&str; 3] = ["Automatic", "Weight", "Percent of page"];
const HEIGHT_RULES: [&str; 2] = ["At least", "Exactly"];

/// The dialogs this module builds.
const DIALOGS: [&str; 9] =
    ["Font", "Paragraph", "PageSetup", "InsertTable", "TableProperties", "PictureSize", "AltText", "Hyperlink", "Accessibility"];

/// An accessibility issue: what is wrong, where (a paragraph and, for a picture, its unit), and whether it is a tip.
struct Issue {
    text: String,
    para: crate::layout::ParaId,
    unit: Option<u32>,
    tip: bool,
}

/// Check Accessibility: pictures without alt text (placeholders aside), tables without a header row.
fn issues(doc: &crate::docx::Document) -> Vec<Issue> {
    use crate::docx::{Block, Inline};
    fn walk(doc: &crate::docx::Document, blocks: &[Block], out: &mut Vec<Issue>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let mut at = 0u32;
                    for i in &p.inlines {
                        if let Inline::Picture(pic, _) = i
                            && pic.alt.is_empty()
                            && !pic.image.is_some_and(|k| doc.placeholders.contains_key(&k) || doc.placeholder_keys.contains_key(&k))
                        {
                            out.push(Issue {
                                text: "Picture without alt text: add a description.".into(),
                                para: crate::layout::ParaId(p.id),
                                unit: Some(at),
                                tip: false,
                            });
                        }
                        at += i.units() as u32;
                    }
                }
                Block::Table(t) => {
                    let first = t
                        .rows
                        .first()
                        .and_then(|r| r.cells.first())
                        .and_then(|c| c.blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p.id) } else { None }));
                    if let Some(id) = first.filter(|_| !t.rows.first().is_some_and(|r| r.header)) {
                        out.push(Issue {
                            text: "Table without a header row: mark its first row as a header row (Table Layout → Header row).".into(),
                            para: crate::layout::ParaId(id),
                            unit: None,
                            tip: true,
                        });
                    }
                    for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                        walk(doc, &c.blocks, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for s in &doc.sections {
        walk(doc, &s.blocks, &mut out);
    }
    out
}

impl Engine {
    /// The dialog for a host command: JSON `{ dialog }`, or "null" when it has none.
    pub fn dialog_open(&mut self, command: &str) -> String {
        if !DIALOGS.contains(&command) {
            return "null".into();
        }
        match self.build(command, None) {
            Some(m) => serde_json::json!({ "dialog": m.json() }).to_string(),
            None => "null".into(),
        }
    }

    /// The dialog rebuilt from the values being edited (`changed`: the field the user just changed).
    pub fn dialog_update(&mut self, id: &str, values: &str, changed: Option<&str>) -> String {
        let v = Values::parse(values, changed);
        match self.build(id, Some(v)) {
            Some(m) => m.json().to_string(),
            None => "null".into(),
        }
    }

    /// Carries out a dialog: `{ close }` (the document changed or not), or the dialog again with an error.
    pub fn dialog_apply(&mut self, id: &str, values: &str) -> String {
        let v = Values::parse(values, None);
        if id == "Accessibility" {
            // Going to an issue: the picture selected, or the caret in the table's first cell.
            let found = self.editor.as_ref().map(|e| issues(e.document())).unwrap_or_default();
            if let Some(i) = v.get("issue").trim().parse::<usize>().ok().and_then(|k| found.get(k)) {
                let (a, b) = i.unit.map_or((0, 0), |u| (u, u + 1));
                self.command("edit.select-range", Some(&format!("{},{a},{b}", i.para.0)));
            }
            return "{\"close\":true}".into();
        }
        let Some(cmd) = self.apply(id, &v) else { return "{\"close\":true}".into() };
        self.run_command(cmd);
        "{\"close\":true}".into()
    }

    fn build(&mut self, id: &str, given: Option<Values>) -> Option<Model> {
        let fonts = self.fonts.families();
        let e = self.editor.as_mut()?;
        let f = e.format_state()?;
        Some(match id {
            "Font" => {
                let r = &f.run;
                let ul = UNDERLINES
                    .iter()
                    .find(|(_, x)| Some(*x) == r.underline.as_deref())
                    .map_or(if r.underline.is_some() { "Single" } else { "None" }, |(l, _)| l);
                let mut v = given.unwrap_or_else(|| {
                    Values::of(&[
                        ("font", r.ascii.clone()),
                        ("size", n(r.size)),
                        ("bold", b(f.toggles[0])),
                        ("italic", b(f.toggles[1])),
                        ("underline", ul.into()),
                        ("strike", b(r.strike)),
                        ("dstrike", b(r.dstrike)),
                        ("sup", b(r.vert_align == VertAlign::Superscript)),
                        ("sub", b(r.vert_align == VertAlign::Subscript)),
                        ("smallcaps", b(r.small_caps)),
                        ("caps", b(r.caps)),
                        ("hidden", b(r.hidden)),
                        ("spacing", n(r.spacing)),
                        ("position", n(r.position)),
                    ])
                });
                // Superscript and subscript exclude each other, as do the strikethroughs and the caps styles.
                for (a, other) in
                    [("sup", "sub"), ("sub", "sup"), ("strike", "dstrike"), ("dstrike", "strike"), ("smallcaps", "caps"), ("caps", "smallcaps")]
                {
                    if v.changed(a) && v.bool(a) {
                        v.set(other, b(false));
                    }
                }
                let names: Vec<&str> = UNDERLINES.iter().map(|u| u.0).collect();
                model(
                    &v,
                    "Font",
                    "Font",
                    vec![
                        vec![combo("font", "Font", &v, fonts), num("size", "Size (pt)", &v, 1.0, 1638.0, 0.5, true)],
                        vec![check("bold", "Bold", &v), check("italic", "Italic", &v), choice("underline", "Underline", &v, &names)],
                        vec![heading("Effects")],
                        vec![check("strike", "Strikethrough", &v), check("dstrike", "Double strikethrough", &v)],
                        vec![check("sup", "Superscript", &v), check("sub", "Subscript", &v), check("hidden", "Hidden", &v)],
                        vec![check("smallcaps", "Small caps", &v), check("caps", "All caps", &v)],
                        vec![heading("Character spacing")],
                        vec![
                            num("spacing", "Spacing (pt)", &v, -100.0, 1584.0, 0.1, true),
                            num("position", "Position (pt, + raised)", &v, -1584.0, 1584.0, 0.5, true),
                        ],
                    ],
                    "OK",
                )
            }
            "Paragraph" => {
                let p = &f.para;
                let mut v = given.unwrap_or_else(|| {
                    let align = match p.jc {
                        Jc::Center => 1,
                        Jc::Right => 2,
                        Jc::Both | Jc::Distribute => 3,
                        Jc::Left => 0,
                    };
                    let rule = match p.line_rule {
                        LineRule::Auto => 0,
                        LineRule::AtLeast => 1,
                        LineRule::Exact => 2,
                    };
                    let special = if p.ind_first > 0.0 {
                        1
                    } else if p.ind_first < 0.0 {
                        2
                    } else {
                        0
                    };
                    Values::of(&[
                        ("align", ALIGNMENTS[align].into()),
                        ("left", n(p.ind_left / IN)),
                        ("right", n(p.ind_right / IN)),
                        ("special", SPECIALS[special].into()),
                        ("by", n(p.ind_first.abs() / IN)),
                        ("before", n(p.space_before)),
                        ("after", n(p.space_after)),
                        ("rule", LINE_RULES[rule].into()),
                        ("at", n(p.line)),
                        ("keepNext", b(p.keep_next)),
                        ("keepLines", b(p.keep_lines)),
                        ("pageBreak", b(p.page_break_before)),
                        ("widow", b(p.widow_control)),
                    ])
                });
                // Between multiple (lines) and an exact or minimum height (points) the field gets a sensible value.
                if v.changed("rule") {
                    v.set("at", (if v.get("rule") == LINE_RULES[0] { "1" } else { "12" }).into());
                }
                let special = v.get("special") != SPECIALS[0];
                let multiple = v.get("rule") == LINE_RULES[0];
                model(
                    &v,
                    "Paragraph",
                    "Paragraph",
                    vec![
                        vec![choice("align", "Alignment", &v, &ALIGNMENTS)],
                        vec![heading("Indentation")],
                        vec![num("left", "Left (in)", &v, -10.0, 20.0, 0.1, true), num("right", "Right (in)", &v, -10.0, 20.0, 0.1, true)],
                        vec![choice("special", "Special", &v, &SPECIALS), num("by", "By (in)", &v, 0.0, 20.0, 0.1, special)],
                        vec![heading("Spacing")],
                        vec![num("before", "Before (pt)", &v, 0.0, 1584.0, 6.0, true), num("after", "After (pt)", &v, 0.0, 1584.0, 6.0, true)],
                        vec![
                            choice("rule", "Line spacing", &v, &LINE_RULES),
                            num("at", if multiple { "At (lines)" } else { "At (pt)" }, &v, 0.06, 1584.0, if multiple { 0.25 } else { 1.0 }, true),
                        ],
                        vec![heading("Pagination")],
                        vec![check("widow", "Widow/Orphan control", &v), check("keepNext", "Keep with next", &v)],
                        vec![check("keepLines", "Keep lines together", &v), check("pageBreak", "Page break before", &v)],
                    ],
                    "OK",
                )
            }
            "PageSetup" => {
                let s = e.document().sections.get(f.section)?.page();
                let mut v = given.unwrap_or_else(|| {
                    Values::of(&[
                        ("top", n(s.top / IN)),
                        ("bottom", n(s.bottom / IN)),
                        ("left", n(s.left / IN)),
                        ("right", n(s.right / IN)),
                        ("width", n(s.page_w / IN)),
                        ("height", n(s.page_h / IN)),
                        ("landscape", b(s.page_w > s.page_h)),
                    ])
                });
                // Landscape swaps the paper's sides; typing sizes updates the orientation.
                let (w, h) = (v.num("width", 8.5), v.num("height", 11.0));
                if v.changed("landscape") && v.bool("landscape") != (w > h) {
                    v.set("width", n(h));
                    v.set("height", n(w));
                } else if v.changed("width") || v.changed("height") {
                    v.set("landscape", b(w > h));
                }
                model(
                    &v,
                    "PageSetup",
                    "Page setup",
                    vec![
                        vec![heading("Margins")],
                        vec![num("top", "Top (in)", &v, 0.0, 22.0, 0.1, true), num("bottom", "Bottom (in)", &v, 0.0, 22.0, 0.1, true)],
                        vec![num("left", "Left (in)", &v, 0.0, 22.0, 0.1, true), num("right", "Right (in)", &v, 0.0, 22.0, 0.1, true)],
                        vec![heading("Paper")],
                        vec![num("width", "Width (in)", &v, 1.0, 22.0, 0.1, true), num("height", "Height (in)", &v, 1.0, 22.0, 0.1, true)],
                        vec![check("landscape", "Landscape", &v)],
                    ],
                    "OK",
                )
            }
            "InsertTable" => {
                let v = given.unwrap_or_else(|| Values::of(&[("cols", "3".into()), ("rows", "3".into())]));
                model(
                    &v,
                    "InsertTable",
                    "Insert table",
                    vec![vec![num("cols", "Columns", &v, 1.0, 63.0, 1.0, true), num("rows", "Rows", &v, 1.0, 200.0, 1.0, true)]],
                    "Insert",
                )
            }
            "TableProperties" => {
                let t = e.index().table(e.document(), f.table?)?;
                let (row, cell) = f.cell.unwrap_or((0, 0));
                let r = t.rows.get(row)?;
                let c = r.cells.get(cell);
                let v = given.unwrap_or_else(|| {
                    let align = match t.jc {
                        Jc::Center => 1,
                        Jc::Right => 2,
                        _ => 0,
                    };
                    let (kind, width) = match t.width {
                        Width::Dxa(pt) => (1, pt),
                        Width::Pct(p) => (2, p * 100.0),
                        _ => (0, 100.0),
                    };
                    let (h, rule) = r.height.clone().map_or((0.0, 0), |(h, rule)| (h, (rule == "exact") as usize));
                    let m = c.map_or([0.0, 5.4, 0.0, 5.4], |c| c.margins);
                    Values::of(&[
                        ("align", TABLE_ALIGNMENTS[align].into()),
                        ("indent", n(t.ind)),
                        ("widthKind", WIDTH_KINDS[kind].into()),
                        ("width", n(width)),
                        ("rowHeight", n(h)),
                        ("rule", HEIGHT_RULES[rule].into()),
                        ("canBreak", b(!r.cant_split)),
                        ("top", n(m[0])),
                        ("left", n(m[1])),
                        ("bottom", n(m[2])),
                        ("right", n(m[3])),
                        ("$canBreak", b(!r.cant_split)),
                    ])
                });
                let left = v.get("align") == TABLE_ALIGNMENTS[0];
                let pct = v.get("widthKind") == WIDTH_KINDS[2];
                let auto = v.get("widthKind") == WIDTH_KINDS[0];
                model(
                    &v,
                    "TableProperties",
                    "Table properties",
                    vec![
                        vec![heading("Table")],
                        vec![
                            choice("align", "Alignment", &v, &TABLE_ALIGNMENTS),
                            num("indent", "Indent from left (pt)", &v, -720.0, 720.0, 1.0, left),
                        ],
                        vec![
                            choice("widthKind", "Preferred width", &v, &WIDTH_KINDS),
                            num("width", if pct { "Width (%)" } else { "Width (pt)" }, &v, 1.0, 2000.0, 1.0, !auto),
                        ],
                        vec![heading("Selected rows")],
                        vec![
                            num("rowHeight", "Row height (pt, 0 = automatic)", &v, 0.0, 1584.0, 1.0, true),
                            choice("rule", "Row height is", &v, &HEIGHT_RULES),
                        ],
                        vec![check("canBreak", "Allow row to break across pages", &v)],
                        vec![heading("Selected cells' margins (pt)")],
                        vec![num("top", "Top", &v, 0.0, 144.0, 1.0, true), num("bottom", "Bottom", &v, 0.0, 144.0, 1.0, true)],
                        vec![num("left", "Left", &v, 0.0, 144.0, 1.0, true), num("right", "Right", &v, 0.0, 144.0, 1.0, true)],
                    ],
                    "OK",
                )
            }
            "PictureSize" => {
                let (w, h) = f.object_size?;
                let r1 = |x: f64| (x * 10.0).round() / 10.0;
                let mut v = given.unwrap_or_else(|| Values::of(&[("width", n(r1(w))), ("height", n(r1(h))), ("lock", "true".into())]));
                // With the aspect ratio locked, one side changes the other.
                let ratio = h / w.max(1.0);
                if v.bool("lock") && v.changed("width") {
                    let x = n(r1(v.num("width", w) * ratio));
                    v.set("height", x);
                }
                if v.bool("lock") && v.changed("height") {
                    let x = n(r1(v.num("height", h) / ratio));
                    v.set("width", x);
                }
                model(
                    &v,
                    "PictureSize",
                    "Picture size",
                    vec![
                        vec![num("width", "Width (pt)", &v, 1.0, 2000.0, 1.0, true), num("height", "Height (pt)", &v, 1.0, 2000.0, 1.0, true)],
                        vec![check("lock", "Lock aspect ratio", &v)],
                    ],
                    "OK",
                )
            }
            "Accessibility" => {
                let found = issues(e.document());
                let v = given.unwrap_or_default();
                if found.is_empty() {
                    let mut m = model(
                        &v,
                        "Accessibility",
                        "Check accessibility",
                        vec![vec![Field { kind: Kind::Note, ..heading("No accessibility issues found.") }]],
                        "OK",
                    );
                    m.can_cancel = false;
                    return Some(m);
                }
                let count = found.iter().filter(|i| !i.tip).count();
                let list = Field {
                    options: Some(found.iter().map(|i| format!("{}{}", if i.tip { "Tip: " } else { "" }, i.text)).collect()),
                    ..field("issue", Kind::List, "Issues", &v)
                };
                model(
                    &v,
                    "Accessibility",
                    &format!("Check accessibility: {count} issue{}", if count == 1 { "" } else { "s" }),
                    vec![vec![Field { kind: Kind::Note, ..heading("Choose an item to go to it.") }], vec![list]],
                    "Close",
                )
            }
            "Hyperlink" => {
                let v = given.unwrap_or_else(|| {
                    // The selected text (in one paragraph) is the link's text.
                    let t = if f.has_selection { e.selected_text() } else { String::new() };
                    Values::of(&[("text", if t.contains('\n') { String::new() } else { t }), ("address", "https://".into())])
                });
                model(
                    &v,
                    "Hyperlink",
                    "Insert hyperlink",
                    vec![vec![field("text", Kind::Text, "Text to display", &v)], vec![field("address", Kind::Text, "Address", &v)]],
                    "Insert",
                )
            }
            "AltText" => {
                let alt = f.object_alt.clone()?;
                let v = given.unwrap_or_else(|| Values::of(&[("text", alt.to_string())]));
                model(
                    &v,
                    "AltText",
                    "Alt text",
                    vec![vec![Field { kind: Kind::Text, ..field("text", Kind::Text, "Describe this picture for people who can't see it", &v) }]],
                    "OK",
                )
            }
            _ => return None,
        })
    }

    /// The command a dialog's values carry out (None: nothing to do).
    fn apply(&mut self, id: &str, v: &Values) -> Option<Command> {
        Some(match id {
            "Font" => {
                let ul = UNDERLINES.iter().find(|(l, _)| *l == v.get("underline")).map_or("none", |u| u.1);
                let size = v.num("size", f64::NAN);
                Command::Format(Format::Font(FontValues {
                    font: v.get("font").trim().to_string(),
                    size: (!size.is_nan()).then_some(size),
                    bold: v.bool("bold"),
                    italic: v.bool("italic"),
                    underline: ul.into(),
                    strike: v.bool("strike"),
                    dstrike: v.bool("dstrike"),
                    vert: if v.bool("sup") {
                        VertAlign::Superscript
                    } else if v.bool("sub") {
                        VertAlign::Subscript
                    } else {
                        VertAlign::Baseline
                    },
                    small_caps: v.bool("smallcaps"),
                    caps: v.bool("caps"),
                    hidden: v.bool("hidden"),
                    spacing: v.num("spacing", 0.0),
                    position: v.num("position", 0.0),
                }))
            }
            "Paragraph" => {
                let rule = LINE_RULES.iter().position(|r| *r == v.get("rule")).unwrap_or(0);
                let special = SPECIALS.iter().position(|r| *r == v.get("special")).unwrap_or(0);
                let by = v.num("by", 0.0) * IN;
                Command::Format(Format::Paragraph(ParaValues {
                    jc: [Jc::Left, Jc::Center, Jc::Right, Jc::Both][ALIGNMENTS.iter().position(|a| *a == v.get("align")).unwrap_or(0)],
                    left: v.num("left", 0.0) * IN,
                    right: v.num("right", 0.0) * IN,
                    first: match special {
                        1 => by,
                        2 => -by,
                        _ => 0.0,
                    },
                    before: v.num("before", 0.0),
                    after: v.num("after", 0.0),
                    line_rule: [LineRule::Auto, LineRule::AtLeast, LineRule::Exact][rule],
                    line: v.num("at", if rule == 0 { 1.0 } else { 12.0 }),
                    keep_next: v.bool("keepNext"),
                    keep_lines: v.bool("keepLines"),
                    page_break_before: v.bool("pageBreak"),
                    widow: v.bool("widow"),
                }))
            }
            "PageSetup" => {
                let e = self.editor.as_mut()?;
                let f = e.format_state()?;
                let mut p = e.document().sections.get(f.section)?.page();
                (p.top, p.bottom, p.left, p.right) =
                    (v.num("top", 1.0) * IN, v.num("bottom", 1.0) * IN, v.num("left", 1.0) * IN, v.num("right", 1.0) * IN);
                (p.page_w, p.page_h) = (v.num("width", 8.5) * IN, v.num("height", 11.0) * IN);
                Command::Section(SectionCmd::Page(p))
            }
            "InsertTable" => {
                Command::InsertTable { rows: v.num("rows", 3.0).clamp(1.0, 200.0) as usize, cols: v.num("cols", 3.0).clamp(1.0, 63.0) as usize }
            }
            "TableProperties" => {
                let align = TABLE_ALIGNMENTS.iter().position(|a| *a == v.get("align")).unwrap_or(0);
                let kind = WIDTH_KINDS.iter().position(|a| *a == v.get("widthKind")).unwrap_or(0);
                let width = v.num("width", 100.0);
                let row_h = v.num("rowHeight", 0.0);
                let mut commands = vec![
                    Command::TableFormat(TableFormat::TableAlign([Jc::Left, Jc::Center, Jc::Right][align])),
                    Command::TableFormat(TableFormat::TableIndent(if align == 0 { v.num("indent", 0.0) } else { 0.0 })),
                    Command::TableFormat(TableFormat::TableWidth(match kind {
                        1 if width > 0.0 => Width::Dxa(width),
                        2 if width > 0.0 => Width::Pct(width / 100.0),
                        _ => Width::Auto,
                    })),
                    Command::TableFormat(TableFormat::RowHeight(
                        (row_h > 0.0).then(|| (row_h, if v.get("rule") == HEIGHT_RULES[1] { "exact" } else { "atLeast" }.to_string())),
                    )),
                    Command::TableFormat(TableFormat::CellMargins([
                        v.num("top", 0.0),
                        v.num("left", 5.4),
                        v.num("bottom", 0.0),
                        v.num("right", 5.4),
                    ])),
                ];
                // Allow Row to Break Across Pages toggles: only when the check changed.
                if v.bool("canBreak") != v.bool("$canBreak") {
                    commands.push(Command::TableFormat(TableFormat::RowCantSplit));
                }
                Command::Batch { label: "Table Properties", commands }
            }
            "Hyperlink" => {
                let address = v.get("address").trim().to_string();
                let text = v.get("text").trim().to_string();
                if address.is_empty() || address == "https://" {
                    return None;
                }
                Command::InsertHyperlink { text: if text.is_empty() { address.clone() } else { text }, address }
            }
            "AltText" => Command::Object(crate::editing::objects::ObjectCmd::AltText(v.get("text").trim().to_string())),
            "PictureSize" => Command::Object(crate::editing::objects::ObjectCmd::Size(v.num("width", 1.0).max(1.0), v.num("height", 1.0).max(1.0))),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    #[test]
    fn dialogs_open_and_apply() {
        let mut e = Engine::with_fonts(Fonts::system());
        e.new_document();
        e.text("Some text");
        e.key("KeyA", true, false, false);
        let open: serde_json::Value = serde_json::from_str(&e.dialog_open("Font")).unwrap();
        assert_eq!(open["dialog"]["id"], "Font");
        // Bold through the Font dialog: one undo step.
        let mut values = serde_json::Map::new();
        for row in open["dialog"]["rows"].as_array().unwrap() {
            for f in row.as_array().unwrap() {
                if let Some(v) = f["value"].as_str() {
                    values.insert(f["id"].as_str().unwrap().into(), v.into());
                }
            }
        }
        values.insert("bold".into(), "true".into());
        values.insert("size".into(), "14".into());
        e.dialog_apply("Font", &serde_json::Value::Object(values).to_string());
        let ed = e.editor.as_mut().unwrap();
        let f = ed.format_state().unwrap();
        assert!(f.toggles[0] && (f.run.size - 14.0).abs() < 1e-9);
        // Page setup: landscape swaps the sides as it is edited, and applies.
        let m: serde_json::Value = serde_json::from_str(&e.dialog_update(
            "PageSetup",
            r#"{"top":"1","bottom":"1","left":"1","right":"1","width":"8.5","height":"11","landscape":"true"}"#,
            Some("landscape"),
        ))
        .unwrap();
        let width = m["rows"].as_array().unwrap().iter().flat_map(|r| r.as_array().unwrap()).find(|f| f["id"] == "width").unwrap()["value"].clone();
        assert_eq!(width, "11");
        e.dialog_apply("PageSetup", r#"{"top":"0.5","bottom":"0.5","left":"0.5","right":"0.5","width":"11","height":"8.5","landscape":"true"}"#);
        let s = &e.editor.as_ref().unwrap().document().sections[0];
        assert_eq!((s.page_w, s.page_h, s.top), (792.0, 612.0, 36.0));
        let saved = e.save().unwrap();
        let again = crate::editing::Editor::open(&saved, &e.fonts).unwrap();
        assert_eq!(again.document().sections[0].page_w, 792.0, "the section's page saves");
        // Insert table, then Table properties in one undo step.
        e.dialog_apply("InsertTable", r#"{"rows":"2","cols":"2"}"#);
        assert!(e.editor.as_mut().unwrap().format_state().unwrap().table.is_some());
        assert_ne!(e.dialog_open("TableProperties"), "null");
        let undo_before = e.editor.as_ref().unwrap().history().undo_depth();
        e.dialog_apply("TableProperties", r#"{"align":"Center","indent":"0","widthKind":"Weight","width":"300","rowHeight":"30","rule":"Exactly","canBreak":"false","$canBreak":"true","top":"2","left":"4","bottom":"2","right":"4"}"#);
        assert_eq!(e.editor.as_ref().unwrap().history().undo_depth(), undo_before + 1, "one undo step");
        let ed = e.editor.as_mut().unwrap();
        let f = ed.format_state().unwrap();
        let t = ed.index().table(ed.document(), f.table.unwrap()).unwrap();
        assert_eq!((t.jc, t.rows[0].cant_split), (Jc::Center, true));
        assert_eq!(t.rows[0].height, Some((30.0, "exact".to_string())));
    }

    #[test]
    fn hyperlink_inserts_a_field() {
        let mut e = Engine::with_fonts(Fonts::system());
        e.new_document();
        e.text("Visit the store site today");
        e.key("Home", true, false, false);
        assert!(e.find_next("store site", false, false));
        let open: serde_json::Value = serde_json::from_str(&e.dialog_open("Hyperlink")).unwrap();
        let text = open["dialog"]["rows"][0][0]["value"].as_str().unwrap().to_string();
        assert_eq!(text, "store site", "the selection is the link's text");
        e.dialog_apply("Hyperlink", r#"{"text":"store site","address":"https://store.example.com"}"#);
        let saved = e.save().unwrap();
        let xml = {
            use std::io::Read;
            let mut z = zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).unwrap();
            let mut s = String::new();
            z.by_name("word/document.xml").unwrap().read_to_string(&mut s).unwrap();
            s
        };
        assert!(xml.contains("HYPERLINK \"https://store.example.com\""), "{xml}");
        assert!(xml.contains(">store site<") && xml.contains("0563C1"), "{xml}");
        let again = crate::editing::Editor::open(&saved, &e.fonts).unwrap();
        let t = match &again.document().sections[0].blocks[0] {
            crate::docx::Block::Para(p) => crate::docx::fields::text(p),
            _ => panic!(),
        };
        assert_eq!(t, "Visit the store site today");
    }

    #[test]
    fn accessibility_lists_issues_and_goes_to_them() {
        let mut e = Engine::with_fonts(Fonts::system());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-OBJ.docx");
        e.open(&std::fs::read(path).unwrap());
        let open: serde_json::Value = serde_json::from_str(&e.dialog_open("Accessibility")).unwrap();
        let list = open["dialog"]["rows"][1][0]["options"].as_array().cloned().unwrap_or_default();
        assert!(list.iter().any(|x| x.as_str().unwrap().starts_with("Picture without alt text")), "{open}");
        let k = list.iter().position(|x| x.as_str().unwrap().starts_with("Picture")).unwrap();
        e.dialog_apply("Accessibility", &format!(r#"{{"issue":"{k}"}}"#));
        assert!(e.editor.as_mut().unwrap().format_state().unwrap().has_selection, "the picture is selected");
        // With every picture described the issue goes.
        let mut e2 = Engine::with_fonts(Fonts::system());
        e2.new_document();
        e2.text("Plain");
        let open: serde_json::Value = serde_json::from_str(&e2.dialog_open("Accessibility")).unwrap();
        assert_eq!(open["dialog"]["canCancel"], false, "no issues: a message");
    }
}
