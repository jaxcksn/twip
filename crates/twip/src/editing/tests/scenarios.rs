//! The runner of tests/scenarios/**/*.json. Steps go through `Editor::apply` as host messages; pointer targets
//! are resolved from the inspection geometry, and expectations are checked against the inspection JSON.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::super::Editor;
use super::super::inspect::inspect;
use super::fonts;

/// Paragraph `n` of a dump: the body's, then the other stories' (their numbering continues the body's).
pub(crate) fn paragraph(dump: &Value, n: usize) -> Value {
    let body = dump["paragraphs"].as_array().cloned().unwrap_or_default();
    if let Some(p) = body.get(n) {
        return p.clone();
    }
    dump["stories"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|s| s["paragraphs"].as_array().cloned().unwrap_or_default())
        .find(|p| p["index"] == json!(n))
        .unwrap_or(Value::Null)
}

pub(crate) fn scenario_files() -> Vec<PathBuf> {
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(d) else { return };
        let mut v: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        v.sort();
        for p in v {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "json") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios"), &mut out);
    out
}

pub(crate) struct Run {
    pub e: Editor,
    seq: u64,
    pub id: String,
    /// What the last copy put on the clipboard: its text and the fragment's token.
    clip: Option<(String, String)>,
    /// Called after every message (a renderer or the recorder consuming frames).
    pub frame: Option<Box<dyn FnMut(&mut Editor)>>,
    /// Where a drag in parts (`press`, `move_to`) has the pointer.
    last: Option<(f64, f64)>,
    /// The viewport's zoom (`zooms`): what `dx_px`/`dy_px` scale by.
    zoom: f64,
    /// The replies to the current step's messages (`context`, `elements`, `events`).
    replies: Vec<Value>,
    /// The id the last answer gave (`"$last"` in arguments).
    last_id: Option<Value>,
}

impl Run {
    /// Opens a fixture of tests/fixtures/editor with the scenarios' viewport.
    pub fn open(fixture: &str, id: &str) -> Run {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(fixture);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{id}: missing fixture {}: {e}", path.display()));
        Run::from_bytes(&bytes, id)
    }

    /// Opens a document with the scenarios' viewport.
    pub fn from_bytes(bytes: &[u8], id: &str) -> Run {
        let e = Editor::open(bytes, fonts()).unwrap_or_else(|err| panic!("{id}: {err}"));
        let mut r = Run { e, seq: 0, id: id.to_string(), frame: None, clip: None, last: None, zoom: 1.0, replies: Vec::new(), last_id: None };
        r.send(json!({ "type": "viewport", "left": 0, "top": 0, "width": 960, "height": 675 }));
        r
    }

    pub fn send(&mut self, mut msg: Value) {
        self.seq += 1;
        msg["seq"] = self.seq.into();
        let r = self.e.apply_json(&msg.to_string(), fonts());
        // A refused command answers `{"error": …}` inside `answer`; only a reply-level error is the runner's failure.
        let v: Value = serde_json::from_str(&r).unwrap_or(Value::Null);
        assert!(v.get("error").is_none(), "{}: {msg} → {r}", self.id);
        if let Some(id) = v["answer"].get("id") {
            self.last_id = Some(id.clone());
        }
        self.replies.push(v);
        // As the host does: background layout until done, so geometry for any page is there.
        while self.e.idle(fonts()) {}
        if let Some(f) = &mut self.frame {
            f(&mut self.e);
        }
    }

    /// The document point of a target (see tests/scenarios/README.md).
    fn point(&mut self, t: &Value, dx: f64, dy: f64) -> (f64, f64) {
        // A target's own `dx`/`dy` (points) move it from where it names; `dx_px`/`dy_px` by pixels (0.75 pt at 100%) at the zoom.
        let k = 0.75 / self.zoom;
        let (dx, dy) = (
            dx + t["dx"].as_f64().unwrap_or(0.0) + t["dx_px"].as_f64().unwrap_or(0.0) * k,
            dy + t["dy"].as_f64().unwrap_or(0.0) + t["dy_px"].as_f64().unwrap_or(0.0) * k,
        );
        let st = inspect(&mut self.e, "geometry,dump", true);
        if let Some(tb) = t.get("table") {
            // A table border: `{"table": t, "row": r, "border": k}` is grid boundary k at row r's middle;
            // `"border": "bottom"` the row's bottom border at its middle.
            // `page`: the row's part on that page (a row split across pages).
            let row = st["geometry"]["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| &r["table"] == tb && r["row"] == t["row"] && (t["page"].is_null() || r["page"] == t["page"]));
            let row = row.unwrap_or_else(|| panic!("{}: no drawn row {t}", self.id));
            let r: Vec<f64> = row["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap_or(0.0)).collect();
            // `"border": "top"` with `col`: just inside the row's top at that grid column's middle (the column zone).
            if t["border"] == "top" {
                let e = row["edges"].as_array().cloned().unwrap_or_default();
                let k = t["col"].as_u64().unwrap_or(0);
                let at = |k: u64| e.iter().find(|x| x[1] == json!(k)).and_then(|x| x[0].as_f64());
                let x = match (at(k), at(k + 1)) {
                    (Some(a), Some(b)) => (a + b) / 2.0,
                    _ => (r[0] + r[2]) / 2.0,
                };
                return (x + dx, r[1] + 0.6 + dy);
            }
            if t["border"] == "bottom" {
                // 1 px (at 100%) below the line: inside its zone (the line to 3 px below) however the pixel rounds.
                return ((r[0] + r[2]) / 2.0 + dx, r[3] + 0.75 + dy);
            }
            // The table resize handle, 3 pt past the row's lower right corner (the table's last row).
            if t["border"] == "handle" {
                return (r[2] + 3.0 + dx, r[3] + 3.0 + dy);
            }
            let e = row["edges"].as_array().into_iter().flatten().find(|e| e[1] == t["border"]);
            let x = e.unwrap_or_else(|| panic!("{}: no border {t}", self.id))[0].as_f64().unwrap_or(0.0);
            return (x + dx, (r[1] + r[3]) / 2.0 + dy);
        }
        if t.get("float").is_some() || t.get("picture").is_some() {
            // A float's box (`float`: its index) or an inline picture's (`picture`: its index among those drawn): its
            // centre, or `at` ("top-left", "top", "left" …): that edge's middle or corner, inside.
            let f = match t.get("float") {
                Some(n) => st["geometry"]["floats"].as_array().into_iter().flatten().find(|f| &f["float"] == n),
                None => st["geometry"]["pictures"].as_array().and_then(|a| a.get(t["picture"].as_u64().unwrap_or(0) as usize)),
            };
            let n = &t["float"];
            let r: Vec<f64> = f.unwrap_or_else(|| panic!("{}: no drawn float {n}", self.id))["rect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0))
                .collect();
            let at = t["at"].as_str().unwrap_or("");
            let x = if at.contains("left") {
                r[0] + 1.0
            } else if at.contains("right") {
                r[2] - 1.0
            } else {
                (r[0] + r[2]) / 2.0
            };
            let y = if at.contains("top") {
                r[1] + 1.0
            } else if at.contains("bottom") {
                r[3] - 1.0
            } else {
                (r[1] + r[3]) / 2.0
            };
            return (x + dx, y + dy);
        }
        let para = t["para"].as_u64().unwrap_or_else(|| panic!("{}: target needs `para`", self.id));
        let lines: Vec<&Value> = st["geometry"]["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["para"] == json!(para) && (t["page"].is_null() && !l["repeat"].as_bool().unwrap_or(false) || l["page"] == t["page"]))
            .collect();
        assert!(!lines.is_empty(), "{}: no drawn line for paragraph {para}", self.id);
        let len = paragraph(&st["dump"], para as usize)["text"].as_str().map_or(0, |t| t.chars().count() as u64);
        let u = match &t["offset"] {
            Value::String(s) if s == "end" => len,
            v => v.as_u64().unwrap_or(0),
        };
        let within = lines.iter().find(|l| l["first"].as_u64() <= Some(u) && Some(u) <= l["last"].as_u64());
        let line = if t["line"] == "last" { lines[lines.len() - 1] } else { within.copied().unwrap_or(lines[lines.len() - 1]) };
        let stops = line["stops"].as_array().cloned().unwrap_or_default();
        let stop =
            stops.iter().filter(|s| s[1].as_u64() <= Some(u)).max_by_key(|s| s[1].as_u64()).or(stops.first()).cloned().unwrap_or(json!([0.0, 0]));
        let r = &line["rect"];
        let y = (r[1].as_f64().unwrap_or(0.0) + r[3].as_f64().unwrap_or(0.0)) / 2.0;
        // A line's first stop is aimed 40% into its first character: at a cell's left edge it is in the cell-selection
        // zone (up to 8 px), which a click on the text means to miss.
        let inside = match stops.get(1) {
            Some(s1) if stops.first() == Some(&stop) => 0.4 * (s1[0].as_f64().unwrap_or(0.0) - stop[0].as_f64().unwrap_or(0.0)),
            _ => 0.0,
        };
        (stop[0].as_f64().unwrap_or(0.0) + inside + dx, y + dy)
    }

    /// A command's or query's argument with its references resolved (README.md): `"$last"` is the last answer's id,
    /// `"$code:<code>"` and `"$tag:<tag>"` the first element with that code (trimmed) or tag, and a pointer target
    /// (`{"para", "offset", ...}`) the document point it names.
    fn resolve(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) if s == "$last" => self.last_id.clone().unwrap_or(Value::Null),
            Value::String(s) if s.starts_with("$code:") || s.starts_with("$tag:") => {
                let (by, want) = s[1..].split_once(':').unwrap();
                self.seq += 1;
                let r = self.e.apply_json(&json!({ "type": "query", "seq": self.seq, "q": "elements" }).to_string(), fonts());
                let all: Value = serde_json::from_str(&r).unwrap_or(Value::Null);
                let found =
                    all["answer"].as_array().into_iter().flatten().find(|x| x[by].as_str().map(str::trim) == Some(want)).map(|x| x["id"].clone());
                found.unwrap_or_else(|| panic!("{}: no element with {by} {want:?}", self.id))
            }
            Value::Object(o) if o.contains_key("para") && o.contains_key("offset") => {
                let (x, y) = self.point(v, v["dx"].as_f64().unwrap_or(0.0), v["dy"].as_f64().unwrap_or(0.0));
                json!({ "x": x, "y": y })
            }
            Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), self.resolve(x))).collect()),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.resolve(x)).collect()),
            other => other.clone(),
        }
    }

    /// The last reply's answer (debugging).
    #[allow(dead_code)]
    pub fn last_answer(&self) -> Option<Value> {
        self.replies.last().and_then(|r| r.get("answer").cloned())
    }

    /// Runs one step's input (README.md); its `expect` is checked by the caller.
    pub fn step(&mut self, step: &Value) {
        let step = &self.resolve_step(step);
        let r = self;
        r.replies.clear();
        let mods = step.get("mods").cloned().unwrap_or(json!({}));
        if let Some(q) = step.get("query") {
            // `{"query": {"q": name, ...args}}`.
            let mut m = q.clone();
            m["type"] = json!("query");
            r.send(m);
        } else if let Some(b) = step.get("batch") {
            // `{"batch": {"label", "commands": [{"command": id, "arg": …}]}}`: one input holding the commands.
            let inputs: Vec<Value> = b["commands"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| json!({ "type": "command", "id": c["command"], "arg": c.get("arg") }))
                .collect();
            r.send(json!({ "type": "batch", "label": b["label"], "inputs": inputs }));
        } else if let Some(code) = step.get("key").and_then(Value::as_str) {
            r.send(json!({ "type": "key", "code": code, "mods": mods }));
        } else if let Some(t) = step.get("ime").and_then(Value::as_str) {
            // `ime`: what the host sends for a committed composition.
            r.send(json!({ "type": "text", "text": t }));
        } else if let Some(t) = step.get("type").and_then(Value::as_str) {
            for c in t.chars() {
                r.send(json!({ "type": "text", "text": c.to_string() }));
            }
        } else if let Some(t) = step.get("click") {
            let (x, y) = r.point(t, step["dx"].as_f64().unwrap_or(0.0), step["dy"].as_f64().unwrap_or(0.0));
            let clicks = step["clicks"].as_u64().unwrap_or(1);
            let mut m = mods.clone();
            if let Some(s) = step["shift"].as_bool() {
                m["shift"] = json!(s);
            }
            // A double or triple click arrives as presses with growing click counts, as from a host.
            for c in 1..=clicks {
                r.send(json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": c, "mods": m }));
                r.send(json!({ "type": "pointer", "kind": "up", "x": x, "y": y, "clicks": c }));
            }
        } else if let Some(d) = step.get("drag") {
            let dx = step["dx"].as_f64().unwrap_or(0.0);
            let (a, b) = (r.point(&d["from"], dx, 0.0), r.point(&d["to"], dx, 0.0));
            let m = step.get("mods").cloned().unwrap_or(json!({}));
            r.send(json!({ "type": "pointer", "kind": "down", "x": a.0, "y": a.1, "clicks": 1, "mods": m }));
            for k in 1..=12 {
                let t = k as f64 / 12.0;
                r.send(json!({ "type": "pointer", "kind": "move", "x": a.0 + (b.0 - a.0) * t, "y": a.1 + (b.1 - a.1) * t }));
            }
            r.send(json!({ "type": "pointer", "kind": "up", "x": b.0, "y": b.1 }));
        } else if let Some(t) = step.get("press") {
            let (dx, dy) = (step["dx"].as_f64().unwrap_or(0.0), step["dy"].as_f64().unwrap_or(0.0));
            let a = r.point(t, dx, dy);
            r.send(
                json!({ "type": "pointer", "kind": "down", "x": a.0, "y": a.1, "clicks": 1, "mods": step.get("mods").cloned().unwrap_or(json!({})) }),
            );
            r.last = Some(a);
        } else if let Some(t) = step.get("move_to") {
            let (dx, dy) = (step["dx"].as_f64().unwrap_or(0.0), step["dy"].as_f64().unwrap_or(0.0));
            let b = r.point(t, dx, dy);
            let a = r.last.unwrap_or(b);
            for k in 1..=12 {
                let f = k as f64 / 12.0;
                r.send(json!({ "type": "pointer", "kind": "move", "x": a.0 + (b.0 - a.0) * f, "y": a.1 + (b.1 - a.1) * f }));
            }
            r.last = Some(b);
        } else if let Some(t) = step.get("release") {
            // `outside`: far up and left of the pages, as a release over the host's command bar is.
            let b =
                if t == "outside" { (-1000.0, -1000.0) } else { r.point(t, step["dx"].as_f64().unwrap_or(0.0), step["dy"].as_f64().unwrap_or(0.0)) };
            r.send(json!({ "type": "pointer", "kind": "move", "x": b.0, "y": b.1 }));
            r.send(json!({ "type": "pointer", "kind": "up", "x": b.0, "y": b.1 }));
            r.last = None;
        } else if let Some(c) = step.get("clipboard").and_then(Value::as_str) {
            // As the host does: copy and cut take the text and the fragment's token; paste gives them back.
            if c == "paste" {
                let (text, token) = r.clip.clone().unwrap_or_default();
                r.send(json!({ "type": "paste", "text": text, "token": token }));
            } else if c == "paste-text" {
                // Paste as text only (Ctrl+Shift+V): the copy's text without its fragment.
                let (text, _) = r.clip.clone().unwrap_or_default();
                r.send(json!({ "type": "paste", "text": text }));
            } else {
                r.send(json!({ "type": c }));
                r.clip = r.e.clipboard().map(|f| (f.text(), f.id.to_string()));
            }
        } else if let Some(t) = step.get("paste_text").and_then(Value::as_str) {
            r.send(json!({ "type": "paste", "text": t }));
        } else if let Some(id) = step.get("command").and_then(Value::as_str) {
            r.send(json!({ "type": "command", "id": id, "arg": step.get("arg") }));
        }
    }

    /// The step with references in its command, query and batch arguments resolved ([`Run::resolve`]).
    fn resolve_step(&mut self, step: &Value) -> Value {
        let mut s = step.clone();
        if let Some(a) = step.get("arg").filter(|a| !a.is_string()) {
            s["arg"] = self.resolve(a);
        }
        if let Some(q) = step.get("query") {
            s["query"] = self.resolve(q);
        }
        if let Some(cmds) = step["batch"]["commands"].as_array() {
            let cmds: Vec<Value> = cmds.iter().map(|c| self.resolve(c)).collect();
            s["batch"]["commands"] = json!(cmds);
        }
        s
    }

    /// The expectations on the step's replies and on the view (tests/scenarios/README.md).
    fn check_replies(&mut self, want: &Value, where_: &str) {
        let last = self.replies.last().cloned().unwrap_or(Value::Null);
        for key in ["answer", "mode"] {
            if let Some(w) = want.get(key) {
                assert!(partial(&last[key], w), "{where_}: {key}: {} for {w}", last[key]);
            }
        }
        if let Some(w) = want.get("context").and_then(Value::as_array) {
            let got = last["context"].as_array().cloned().unwrap_or_default();
            assert!(got.len() == w.len() && got.iter().zip(w).all(|(g, x)| partial(g, x)), "{where_}: context: {got:?} for {w:?}");
        }
        if let Some(w) = want.get("elements") {
            // The step's change lists, merged over its replies.
            let mut all = json!({ "added": [], "removed": [], "changed": [] });
            for r in &self.replies {
                for k in ["added", "removed", "changed"] {
                    if let (Some(dst), Some(src)) = (all[k].as_array_mut(), r["elements"][k].as_array()) {
                        dst.extend(src.iter().cloned());
                    }
                }
            }
            for k in ["added", "removed", "changed"] {
                match &w[k] {
                    Value::Null => {}
                    Value::Number(n) => {
                        assert_eq!(all[k].as_array().map_or(0, Vec::len) as u64, n.as_u64().unwrap_or(0), "{where_}: elements.{k}: {}", all[k])
                    }
                    x => assert!(partial(&all[k], x), "{where_}: elements.{k}: {} for {x}", all[k]),
                }
            }
        }
        if let Some(w) = want.get("events").and_then(Value::as_array) {
            // Each expected event, in order, among the step's events.
            let got: Vec<Value> = self.replies.iter().flat_map(|r| r["events"].as_array().cloned().unwrap_or_default()).collect();
            let mut it = got.iter();
            for x in w {
                assert!(it.any(|g| partial(g, x)), "{where_}: event {x} not in {got:?}");
            }
        }
        if let Some(w) = want.get("view") {
            let got = inspect(&mut self.e, "view", true)["view"].clone();
            assert!(partial(&got, w), "{where_}: view: {got} for {w}");
        }
    }

    pub fn check(&mut self, want: &Value, where_: &str) {
        self.check_replies(want, where_);
        let st = inspect(&mut self.e, "caret,selection,undo,dump,integrity", true);
        assert_eq!(st["integrity"], json!([]), "{where_}: integrity");
        if let Some(c) = want.get("caret").and_then(Value::as_object) {
            for (k, v) in c {
                let got = match k.as_str() {
                    "depth" => json!(st["caret"]["chain"].as_array().map_or(0, Vec::len)),
                    "page" => st["caret"]["rect"]["page"].clone(),
                    _ => st["caret"][k].clone(),
                };
                assert_eq!(&got, v, "{where_}: caret.{k} (caret {})", st["caret"]);
            }
        }
        if let Some(s) = want.get("selection") {
            if let Some(kind) = s.get("kind") {
                assert_eq!(&st["selection"]["kind"], kind, "{where_}: selection kind ({})", st["selection"]);
            }
            if let Some(t) = s.get("table") {
                assert_eq!(&st["selection"]["table"], t, "{where_}: selection table ({})", st["selection"]);
            }
            for end in ["anchor", "focus"] {
                // A cell block's ends are [row, column].
                if let Some(a) = s.get(end).filter(|v| v.is_array()) {
                    assert_eq!(&st["selection"][end], a, "{where_}: selection.{end} ({})", st["selection"]);
                }
                if let Some(o) = s.get(end).and_then(Value::as_object) {
                    for (k, v) in o {
                        assert_eq!(&st["selection"][end][k], v, "{where_}: selection.{end}.{k} ({})", st["selection"]);
                    }
                }
            }
        }
        if let Some(t) = want.get("text").and_then(Value::as_object) {
            for (i, v) in t {
                let got = paragraph(&st["dump"], i.parse::<usize>().unwrap_or(0))["text"].as_str().unwrap_or("").to_string();
                match v {
                    Value::String(s) => assert_eq!(&got, s, "{where_}: text of paragraph {i}"),
                    o => {
                        let p = o["starts"].as_str().unwrap_or("");
                        assert!(got.starts_with(p), "{where_}: paragraph {i} starts with {p:?}: {got:?}");
                    }
                }
            }
        }
        for key in ["runs", "jc", "style", "label", "sizes", "colors"] {
            if let Some(m) = want.get(key).and_then(Value::as_object) {
                for (i, v) in m {
                    let got = &paragraph(&st["dump"], i.parse::<usize>().unwrap_or(0))[key];
                    assert!(near(got, v), "{where_}: {key} of paragraph {i}: {got} for {v}");
                }
            }
        }
        // `para`: the paragraph metrics given (ind_left, ind_first, before, after, line, keep_next).
        if let Some(m) = want.get("para").and_then(Value::as_object) {
            for (i, v) in m {
                let got = &paragraph(&st["dump"], i.parse::<usize>().unwrap_or(0))["para"];
                for (k, x) in v.as_object().into_iter().flatten() {
                    assert!(near(&got[k], x), "{where_}: {k} of paragraph {i}: {} for {x}", got[k]);
                }
            }
        }
        if let Some(n) = want.get("paragraphs") {
            assert_eq!(&json!(st["dump"]["paragraphs"].as_array().map_or(0, Vec::len)), n, "{where_}: paragraph count");
        }
        if let Some(n) = want.get("undo_depth") {
            assert_eq!(&st["undo"]["undo_depth"], n, "{where_}: undo depth");
        }
        if let Some(n) = want.get("redo_depth") {
            assert_eq!(&st["undo"]["redo_depth"], n, "{where_}: redo depth");
        }
        if let Some(ts) = want.get("tables").and_then(Value::as_array) {
            for (i, t) in ts.iter().enumerate() {
                if let Some(rows) = t.get("rows") {
                    assert_eq!(&st["dump"]["tables"][i]["rows"], rows, "{where_}: rows of table {i}");
                }
                if let Some(cells) = t.get("cells") {
                    assert_eq!(&st["dump"]["tables"][i]["cells"], cells, "{where_}: cells per row of table {i}");
                }
                // Column widths and row heights within half a point (a pointer drag lands on whole pixels).
                for key in ["grid", "heights", "header_rows", "cell_fmt"] {
                    if let Some(w) = t.get(key) {
                        let got = &st["dump"]["tables"][i][key];
                        assert!(near(got, w), "{where_}: {key} of table {i}: {got} for {w}");
                    }
                }
            }
        }
        if let Some(fs) = want.get("floats").and_then(Value::as_array) {
            let got = st["dump"]["floats"].as_array().cloned().unwrap_or_default();
            assert_eq!(got.len(), fs.len(), "{where_}: floats ({got:?})");
            for (i, f) in fs.iter().enumerate() {
                for (k, v) in f.as_object().into_iter().flatten() {
                    // Points within half a point: a pointer drag lands on whole pixels.
                    match (got[i][k].as_f64(), v.as_f64()) {
                        (Some(a), Some(b)) => assert!((a - b).abs() <= 0.5, "{where_}: float {i} {k}: {a} for {b}"),
                        _ => assert_eq!(&got[i][k], v, "{where_}: float {i} {k}"),
                    }
                }
            }
        }
    }
}

/// `got` has everything `want` gives: objects by the keys given, arrays element by element (same length), numbers
/// within half a point.
fn partial(got: &Value, want: &Value) -> bool {
    match (got, want) {
        (Value::Object(g), Value::Object(w)) => w.iter().all(|(k, x)| partial(g.get(k).unwrap_or(&Value::Null), x)),
        (Value::Array(g), Value::Array(w)) => g.len() == w.len() && g.iter().zip(w).all(|(a, b)| partial(a, b)),
        _ => near(got, want),
    }
}

/// Equal, numbers within half a point.
fn near(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap_or(0.0) - y.as_f64().unwrap_or(0.0)).abs() <= 0.5,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| near(p, q)),
        _ => a == b,
    }
}

fn run(path: &Path) {
    let s: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("readable scenario")).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let id = s["id"].as_str().unwrap_or("?").to_string();
    // `zooms`: the whole scenario once per zoom; targets are document points, so only the pixel-sized zones change.
    let zooms: Vec<f64> = s["zooms"].as_array().map_or(vec![1.0], |z| z.iter().filter_map(Value::as_f64).collect());
    for zoom in zooms {
        let mut r = Run::open(s["fixture"].as_str().unwrap_or(""), &id);
        if zoom != 1.0 {
            r.send(json!({ "type": "viewport", "left": 0, "top": 0, "width": 960, "height": 675, "zoom": zoom }));
            r.zoom = zoom;
        }
        for (i, step) in s["steps"].as_array().into_iter().flatten().enumerate() {
            let where_ = format!("{id} @{}% step {i}", (zoom * 100.0).round());
            r.step(step);
            if let Some(w) = step.get("expect") {
                r.check(w, &where_);
            }
        }
    }
}

#[test]
fn scenarios() {
    let files = scenario_files();
    assert!(!files.is_empty(), "no scenarios found in tests/scenarios");
    let only = std::env::var("SCENARIO").ok();
    let failed: Vec<String> = files
        .iter()
        .filter(|f| only.as_ref().is_none_or(|o| f.to_string_lossy().contains(o.as_str())))
        .filter_map(|f| {
            std::panic::catch_unwind(|| run(f)).err().map(|e| {
                let why = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                format!("{}: {}", f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), why.lines().next().unwrap_or(""))
            })
        })
        .collect();
    assert!(
        failed.is_empty(),
        "{} scenarios failed:
{}",
        failed.len(),
        failed.join(
            "
"
        )
    );
}

#[test]
fn a_scenario_naming_a_missing_fixture_fails() {
    let dir = std::env::temp_dir().join(format!("scenario-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let p = dir.join("missing.json");
    std::fs::write(&p, r#"{"id":"MISSING","fixture":"FX-DOES-NOT-EXIST.docx","steps":[]}"#).expect("write");
    let failed = std::panic::catch_unwind(|| run(&p)).is_err();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failed, "a missing fixture must fail the scenario");
}
