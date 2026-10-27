//! Host inputs and replies: typed values, parsed from JSON at the boundary. Nothing a
//! host sends can panic the editor: malformed messages are reported and change nothing.

use serde_json::{Value, json};

use super::keymap::Mods;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    Cancel,
}

/// The visible part of the document, in document points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
    pub dpr: f64,
}

impl Default for Viewport {
    fn default() -> Viewport {
        Viewport { left: 0.0, top: 0.0, width: 816.0, height: 1056.0, zoom: 1.0, dpr: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Key {
        code: String,
        mods: Mods,
    },
    Text(String),
    /// Document coordinates (points in the page stack).
    Pointer {
        kind: PointerKind,
        x: f64,
        y: f64,
        clicks: u8,
        mods: Mods,
    },
    Hover {
        x: f64,
        y: f64,
    },
    Wheel {
        dx: f64,
        dy: f64,
    },
    Viewport(Viewport),
    Command {
        id: String,
        arg: Option<Value>,
    },
    Copy,
    Cut,
    /// `token`: the id of a fragment this editor copied (the host's own clipboard type), pasted with its formatting.
    Paste {
        text: Option<String>,
        html: Option<String>,
        token: Option<String>,
    },
    Focus(bool),
    Undo,
    Redo,
    /// The host's platform: `mac` switches to the macOS keymap layer.
    Platform {
        mac: bool,
    },
    /// A question about the document: answered in the reply, nothing changes.
    Query {
        q: String,
        args: Value,
    },
    /// Commands applied as one undo step labelled `label`; a refused one rolls them all back.
    Batch {
        label: String,
        inputs: Vec<Value>,
    },
}

/// One input with its sequence number.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub seq: u64,
    pub input: Input,
}

/// A message the editor can't use.
#[derive(Clone, Debug, PartialEq)]
pub struct BadInput(pub String);

fn num(v: &Value, k: &str) -> Result<f64, BadInput> {
    v.get(k).and_then(Value::as_f64).ok_or_else(|| BadInput(format!("missing number `{k}`")))
}

fn mods(v: &Value) -> Mods {
    let m = v.get("mods");
    let flag = |k| m.and_then(|m| m.get(k)).and_then(Value::as_bool).unwrap_or(false);
    Mods { ctrl: flag("ctrl"), shift: flag("shift"), alt: flag("alt"), meta: flag("meta") }
}

impl Message {
    pub fn parse(v: &Value) -> Result<Message, BadInput> {
        let seq = v.get("seq").and_then(Value::as_u64).ok_or_else(|| BadInput("missing `seq`".into()))?;
        let ty = v.get("type").and_then(Value::as_str).ok_or_else(|| BadInput("missing `type`".into()))?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let input = match ty {
            "key" => Input::Key { code: s("code").ok_or_else(|| BadInput("missing `code`".into()))?, mods: mods(v) },
            "text" => Input::Text(s("text").ok_or_else(|| BadInput("missing `text`".into()))?),
            "pointer" => Input::Pointer {
                kind: match s("kind").as_deref() {
                    Some("down") => PointerKind::Down,
                    Some("move") => PointerKind::Move,
                    Some("up") => PointerKind::Up,
                    Some("cancel") => PointerKind::Cancel,
                    k => return Err(BadInput(format!("pointer kind {k:?}"))),
                },
                x: num(v, "x")?,
                y: num(v, "y")?,
                clicks: v.get("clicks").and_then(Value::as_u64).unwrap_or(1).clamp(1, 3) as u8,
                mods: mods(v),
            },
            "hover" => Input::Hover { x: num(v, "x")?, y: num(v, "y")? },
            "wheel" => Input::Wheel { dx: num(v, "dx")?, dy: num(v, "dy")? },
            "viewport" => Input::Viewport(Viewport {
                left: num(v, "left")?,
                top: num(v, "top")?,
                width: num(v, "width")?,
                height: num(v, "height")?,
                zoom: v.get("zoom").and_then(Value::as_f64).unwrap_or(1.0),
                dpr: v.get("dpr").and_then(Value::as_f64).unwrap_or(1.0),
            }),
            "command" => Input::Command { id: s("id").ok_or_else(|| BadInput("missing `id`".into()))?, arg: v.get("arg").cloned() },
            "copy" => Input::Copy,
            "cut" => Input::Cut,
            "paste" => Input::Paste { text: s("text"), html: s("html"), token: s("token") },
            "focus" => Input::Focus(v.get("has_focus").and_then(Value::as_bool).unwrap_or(true)),
            "platform" => Input::Platform { mac: v.get("mac").and_then(Value::as_bool).unwrap_or(false) },
            "undo" => Input::Undo,
            "redo" => Input::Redo,
            "query" => Input::Query { q: s("q").ok_or_else(|| BadInput("missing `q`".into()))?, args: v.clone() },
            "batch" => Input::Batch {
                label: s("label").unwrap_or_else(|| "Batch".into()),
                inputs: v.get("inputs").and_then(Value::as_array).cloned().ok_or_else(|| BadInput("missing `inputs`".into()))?,
            },
            // IME composition is deferred: hosts send composed text as `text`.
            "composition" => return Err(BadInput("composition input is not supported; send `text` on compositionend".into())),
            t => return Err(BadInput(format!("unknown input type `{t}`"))),
        };
        Ok(Message { seq, input })
    }
}

/// A request for the host.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    ScrollIntoView {
        page: usize,
        rect: [f64; 4],
    },
    Autoscroll {
        vx: f64,
        vy: f64,
    },
    CapturePointer,
    ReleasePointer,
    Cursor(&'static str),
    Clipboard {
        text: String,
        html: Option<String>,
    },
    /// An unfilled placeholder was activated: the host supplies its content.
    Placeholder(super::placeholders::PlaceholderInfo),
}

impl Request {
    pub fn to_json(&self) -> Value {
        match self {
            Request::ScrollIntoView { page, rect } => json!({ "type": "scroll_into_view", "page": page, "rect": rect }),
            Request::Autoscroll { vx, vy } => json!({ "type": "autoscroll", "vx": vx, "vy": vy }),
            Request::CapturePointer => json!({ "type": "capture_pointer" }),
            Request::ReleasePointer => json!({ "type": "release_pointer" }),
            Request::Cursor(c) => json!({ "type": "cursor", "value": c }),
            Request::Clipboard { text, html } => json!({ "type": "clipboard", "text": text, "html": html }),
            Request::Placeholder(p) => json!({ "type": "placeholder", "placeholder": p.to_json() }),
        }
    }
}
