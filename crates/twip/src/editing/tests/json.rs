//! The alpha's JSON host messages and replies, kept for the scenario files and the tests written in them: each message
//! is parsed into an input and applied through the editor's typed calls, and the reply is built from the editor's
//! state. Hosts use the `api` session.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::docx::{Block, ControlKind, Document, ElementId, Inline, Lock};
use crate::font::Fonts;

use super::super::commands::{Command, Outcome};
use super::super::elements::{self, Changes, Element};
use super::super::index::DocIndex;
use super::super::input::{Notice, PointerKind, Request, Viewport};
use super::super::keymap::Mods;
use super::super::merge::Where;
use super::super::selection::Selection;
use super::super::text::UnitText;
use super::super::{Editor, Mode};

#[derive(Clone, Debug, PartialEq)]
enum Input {
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
    Wheel,
    Viewport(Viewport),
    /// A command, as the API types it (`{"type": "command", "command": {"type": "setBold", "value": true}}`).
    Command(Box<Command>),
    Copy,
    Cut,
    /// `token`: the id of a fragment this editor copied, pasted with its formatting.
    Paste {
        text: Option<String>,
        token: Option<String>,
    },
    Focus,
    Undo,
    Redo,
    /// `mac` switches to the macOS keymap layer.
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
        commands: Vec<Command>,
    },
    Author(String),
    Mode(Mode),
    /// The external drop caret at a point (`{ x, y, page? }`), or none (`null`).
    Drop(Value),
    /// Units `start..end` of a paragraph selected (find, spelling).
    Select {
        para: crate::layout::ParaId,
        start: crate::layout::Unit,
        end: crate::layout::Unit,
    },
    SelectElement(crate::api::value::ElementId),
    ReplaceAll {
        find: String,
        with: String,
        case: bool,
    },
}

/// A message the editor can't use.
struct BadInput(String);

fn num(v: &Value, k: &str) -> Result<f64, BadInput> {
    v.get(k).and_then(Value::as_f64).ok_or_else(|| BadInput(format!("missing number `{k}`")))
}

fn mods(v: &Value) -> Mods {
    let m = v.get("mods");
    let flag = |k| m.and_then(|m| m.get(k)).and_then(Value::as_bool).unwrap_or(false);
    Mods { ctrl: flag("ctrl"), shift: flag("shift"), alt: flag("alt"), meta: flag("meta") }
}

/// `v` as a typed value: `what` names it in the error.
fn typed<T: serde::de::DeserializeOwned>(v: Option<&Value>, what: &str) -> Result<T, BadInput> {
    serde_json::from_value(v.cloned().unwrap_or(Value::Null)).map_err(|e| BadInput(format!("bad `{what}`: {e}")))
}

/// A message's sequence number and input.
fn parse(v: &Value) -> Result<(u64, Input), BadInput> {
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
        "wheel" => {
            num(v, "dx")?;
            num(v, "dy")?;
            Input::Wheel
        }
        "viewport" => Input::Viewport(Viewport {
            left: num(v, "left")?,
            top: num(v, "top")?,
            width: num(v, "width")?,
            height: num(v, "height")?,
            zoom: v.get("zoom").and_then(Value::as_f64).unwrap_or(1.0),
            dpr: v.get("dpr").and_then(Value::as_f64).unwrap_or(1.0),
        }),
        "command" => Input::Command(Box::new(typed(v.get("command"), "command")?)),
        "copy" => Input::Copy,
        "cut" => Input::Cut,
        "paste" => Input::Paste { text: s("text"), token: s("token") },
        "focus" => Input::Focus,
        "platform" => Input::Platform { mac: v.get("mac").and_then(Value::as_bool).unwrap_or(false) },
        "undo" => Input::Undo,
        "redo" => Input::Redo,
        "query" => Input::Query { q: s("q").ok_or_else(|| BadInput("missing `q`".into()))?, args: v.clone() },
        "batch" => Input::Batch { label: s("label").unwrap_or_else(|| "Batch".into()), commands: typed(v.get("commands"), "commands")? },
        "author" => Input::Author(s("name").ok_or_else(|| BadInput("missing `name`".into()))?),
        "mode" => Input::Mode(s("mode").as_deref().and_then(Mode::parse).ok_or_else(|| BadInput("bad `mode`".into()))?),
        "drop" => Input::Drop(v.get("at").cloned().unwrap_or(Value::Null)),
        "select" => {
            let n = |k: &str| v.get(k).and_then(Value::as_u64).ok_or_else(|| BadInput(format!("missing number `{k}`")));
            Input::Select {
                para: crate::layout::ParaId(n("para")?),
                start: crate::layout::Unit(n("start")? as u32),
                end: crate::layout::Unit(n("end")? as u32),
            }
        }
        "select_element" => Input::SelectElement(crate::api::value::ElementId(s("id").ok_or_else(|| BadInput("missing `id`".into()))?)),
        "replace_all" => Input::ReplaceAll {
            find: s("find").ok_or_else(|| BadInput("missing `find`".into()))?,
            with: s("with").unwrap_or_default(),
            case: v.get("case").and_then(Value::as_bool).unwrap_or(false),
        },
        "composition" => return Err(BadInput("composition input is not supported; send `text` on compositionend".into())),
        t => return Err(BadInput(format!("unknown input type `{t}`"))),
    };
    Ok((seq, input))
}

impl Editor {
    /// Parses and applies a JSON message (or an array of them), returning the reply JSON.
    pub(crate) fn apply_json(&mut self, json: &str, fonts: &Fonts) -> String {
        // The host this answers takes no records: only the last message's are kept.
        self.commits.clear();
        let v: Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return json!({ "error": e.to_string() }).to_string(),
        };
        let msgs: Vec<&Value> = match &v {
            Value::Array(a) => a.iter().collect(),
            one => vec![one],
        };
        let mut answer = None;
        for m in msgs {
            match parse(m).and_then(|(seq, input)| self.apply_input(seq, input, fonts)) {
                Ok(a) => answer = a.or(answer),
                Err(BadInput(e)) => return json!({ "error": e, "seq": m.get("seq") }).to_string(),
            }
        }
        self.reply(answer).to_string()
    }

    /// Applies one input, returning its answer. A non-increasing `seq` is rejected unapplied.
    fn apply_input(&mut self, seq: u64, input: Input, fonts: &Fonts) -> Result<Option<Value>, BadInput> {
        if seq <= self.seq && self.seq > 0 {
            return Err(BadInput(format!("seq {seq} after {}", self.seq)));
        }
        self.seq = seq;
        let answer = match input {
            Input::Key { code, mods } => {
                if let Some(cmd) = self.key_command(&code, mods) {
                    self.execute(cmd, fonts);
                }
                None
            }
            Input::Text(t) => {
                self.execute(Command::InsertText(t), fonts);
                None
            }
            Input::Pointer { kind, x, y, clicks, mods } => {
                self.pointer_input(kind, x, y, clicks, mods, fonts);
                None
            }
            Input::Hover { x, y } => {
                self.set_hover(x, y);
                None
            }
            Input::Wheel | Input::Focus => None,
            Input::Viewport(v) => {
                self.viewport = v;
                None
            }
            Input::Platform { mac } => {
                self.mac = mac;
                None
            }
            // A refused command is answered with why; what it made is the answer.
            Input::Command(cmd) => {
                let out = self.execute(*cmd, fonts);
                self.answer_of(out)
            }
            Input::Copy => {
                self.execute(Command::Copy, fonts);
                None
            }
            Input::Cut => {
                self.execute(Command::Cut, fonts);
                None
            }
            Input::Paste { text, token } => {
                // The token names our fragment; the text must match too (another editor's copy can carry the same id).
                let ours = self.clipboard.clone().filter(|f| {
                    token.as_deref().and_then(|t| t.parse::<u64>().ok()) == Some(f.id)
                        && text.as_ref().is_none_or(|t| t.replace("\r\n", "\n") == f.text().replace("\r\n", "\n"))
                });
                match (ours, text) {
                    (Some(_), _) if self.mode == Mode::ReadOnly => {
                        self.refuse_read_only();
                    }
                    (Some(f), _) => {
                        self.history.break_typing();
                        // A paste recorded as a move is one undo step with its move.
                        self.begin_batch(None);
                        let out = self.paste_fragment(&f, fonts);
                        self.paste_moved(&f, &out, fonts);
                        self.end_batch(true, fonts);
                    }
                    (None, Some(t)) => {
                        self.execute(Command::PasteText(t), fonts);
                    }
                    (None, None) => {}
                }
                None
            }
            Input::Undo => {
                self.execute(Command::Undo, fonts);
                None
            }
            Input::Redo => {
                self.execute(Command::Redo, fonts);
                None
            }
            Input::Query { q, args } => Some(self.query(&q, &args, fonts)),
            Input::Batch { label, commands } => self.batch(label, commands, fonts),
            Input::Author(name) => {
                self.set_author(&name, None);
                None
            }
            Input::Mode(m) => {
                self.set_mode(m);
                None
            }
            Input::Drop(at) => {
                let at = self.doc_point(&at);
                self.set_drop(at, fonts);
                None
            }
            Input::Select { para, start, end } => {
                self.select_range(para, start, end, fonts);
                None
            }
            Input::SelectElement(id) => {
                let out = self.select_element(&id, true, fonts);
                self.answer_of(out)
            }
            Input::ReplaceAll { .. } if self.mode == Mode::ReadOnly => {
                self.refuse_read_only();
                None
            }
            Input::ReplaceAll { find, with, case } => {
                self.history.break_typing();
                self.replace_all(&find, &with, case, fonts);
                None
            }
        };
        // A building block's refusal outside a command (a locked drag, a read-only paste) is the answer too.
        let refused = self.refused.take().map(|why| json!({ "error": why }));
        Ok(answer.or(refused))
    }

    /// The answer for a command's outcome: why it was refused, else what it made.
    fn answer_of(&mut self, out: Outcome) -> Option<Value> {
        match out {
            Outcome::Refused(e) => Some(json!({ "error": e.message })),
            _ => self.made.take().and_then(|made| serde_json::to_value(made).ok()),
        }
    }

    /// Commands as one transaction and one undo step. A refused command undoes the ones before it, and the answer
    /// says which.
    fn batch(&mut self, label: String, commands: Vec<Command>, fonts: &Fonts) -> Option<Value> {
        self.begin_batch(Some(label.into()));
        let mut answer = None;
        for (k, cmd) in commands.into_iter().enumerate() {
            let out = self.execute(cmd, fonts);
            answer = self.answer_of(out);
            if let Some(e) = answer.as_ref().and_then(|a| a.get("error")).cloned() {
                self.end_batch(false, fonts);
                return Some(json!({ "error": e, "index": k }));
            }
        }
        self.end_batch(true, fonts);
        answer
    }

    /// The reply after a batch of messages.
    fn reply(&mut self, answer: Option<Value>) -> Value {
        let snapshot = self.layout.snapshot();
        let status = match snapshot.status() {
            crate::layout::LayoutStatus::Pending { .. } => "pending",
            crate::layout::LayoutStatus::Converged => "converged",
            crate::layout::LayoutStatus::Stabilized { .. } => "stabilized",
            crate::layout::LayoutStatus::Inconsistent { .. } => "inconsistent",
        };
        let requests: Vec<Value> = self.take_requests().iter().map(request).collect();
        let mut r = json!({
            "seq_applied": self.seq,
            "revision": self.revision,
            "layout": { "status": status, "pages": snapshot.pages().len(),
                        "provisional_from": snapshot.info().iter().position(|i| i.provisional) },
            "dirty": self.is_dirty(),
            "requests": requests,
            "context": context(&self.doc, &self.index, &self.selection, &self.elems),
            "mode": self.mode.name(),
        });
        if !self.changes.is_empty() {
            r["elements"] = changes(&std::mem::take(&mut self.changes), &self.index);
        }
        if let Some(a) = answer {
            r["answer"] = a;
        }
        if !self.events.is_empty() {
            r["events"] = std::mem::take(&mut self.events).iter().map(notice).collect();
        }
        r
    }

    /// Answers a host query; nothing changes.
    fn query(&mut self, q: &str, args: &Value, fonts: &Fonts) -> Value {
        let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
        match q {
            "elements" => {
                let matching = args.get("match").and_then(|m| Some((m.get("in")?.as_str()?.to_string(), m.get("text")?.as_str()?.to_string())));
                let f = elements::Filter { kind: s("kind"), matching, story: s("story") };
                json!(elements::enumerate(&self.doc, &self.index, &f).iter().map(|e| element(e, &self.index)).collect::<Vec<_>>())
            }
            "element" => {
                let Some(id) = s("id").as_deref().and_then(elements::parse_id) else { return json!({ "error": "bad estimate: id" }) };
                match self.elems.get(&id) {
                    Some(e) => {
                        let mut v = element(e, &self.index);
                        let children: Vec<String> = self.elems.values().filter(|c| c.parent == Some(id)).map(|c| elements::id_str(c.id)).collect();
                        v["children"] = json!(children);
                        v
                    }
                    None => json!({ "error": "no such element" }),
                }
            }
            "context" => context(&self.doc, &self.index, &self.selection, &self.elems),
            "structure" if args.get("id").is_some() => self.structure_query(args),
            "structure" => {
                let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(8) as usize;
                structure(&self.doc.sections.iter().flat_map(|s| s.blocks.iter().cloned()).collect::<Vec<_>>(), depth)
            }
            "hit" => {
                // The position at a point, the innermost element there, and whether it can be edited.
                let hit = self.doc_point(args).and_then(|(x, y)| self.hit_test(x, y, fonts));
                let Some(((para, offset), element, editable)) = hit.and_then(|(p, e, ok)| Some((p.para_offset()?, e, ok))) else {
                    return json!({ "error": "bad estimate: x" });
                };
                json!({ "position": { "para": para.0, "offset": offset.0 }, "element": element.map(elements::id_str), "editable": editable })
            }
            "data" => {
                let ns = s("namespace");
                let item = s("item_id");
                json!(
                    self.doc
                        .data_parts
                        .iter()
                        .filter(|p| ns.is_none() || p.namespace == ns)
                        .filter(|p| item.as_ref().is_none_or(|i| *i == p.item_id))
                        .map(|p| json!({ "namespace": p.namespace, "item_id": p.item_id, "xml": p.xml }))
                        .collect::<Vec<_>>()
                )
            }
            _ => json!({ "error": "unknown query" }),
        }
    }

    /// A document point from `{ x, y, page? }` (page coordinates when `page` is given).
    fn doc_point(&self, at: &Value) -> Option<(f64, f64)> {
        let (x, y) = (at.get("x")?.as_f64()?, at.get("y")?.as_f64()?);
        match at.get("page").and_then(Value::as_u64) {
            Some(page) => self.stack().to_doc(page as usize, x, y),
            None => Some((x, y)),
        }
    }

    /// The `structure` query for one element: a region's rows as a table of just those rows, or a block
    /// control's blocks.
    fn structure_query(&mut self, args: &Value) -> Value {
        let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(8) as usize;
        let id = crate::api::value::ElementId(args.get("id").and_then(Value::as_str).unwrap_or("").to_string());
        let Ok((_, w)) = self.element(&id) else { return json!({ "error": "no such element" }) };
        match w {
            Where::Rows { table, range } => {
                let Some(t) = self.index.table(&self.doc, table) else { return json!({ "error": "no such element" }) };
                let mut t = t.clone();
                t.rows = t.rows[range].to_vec();
                structure(&[Block::Table(t)], depth)
            }
            Where::Blocks { container, range } => match self.index.blocks(&self.doc, container) {
                Some(b) => structure(&b[range], depth),
                None => json!({ "error": "no such element" }),
            },
            _ => json!({ "error": "bad estimate: id" }),
        }
    }
}

fn notice(n: &Notice) -> Value {
    let id = |e: &ElementId| elements::id_str(*e);
    match n {
        Notice::Activate(e) => json!({ "type": "activate", "id": id(e) }),
        Notice::FieldUpdate { token, fields } => {
            let fields: Vec<Value> = fields.iter().map(|(e, code)| json!({ "id": id(e), "code": code })).collect();
            json!({ "type": "field_update", "token": token, "fields": fields })
        }
        Notice::Link { address, ctrl } => json!({ "type": "link", "address": address, "ctrl": ctrl }),
        Notice::Placeholder(key) => json!({ "type": "placeholder_activated", "key": key }),
    }
}

fn request(r: &Request) -> Value {
    match r {
        Request::ScrollIntoView { page, rect } => json!({ "type": "scroll_into_view", "page": page, "rect": rect }),
        Request::Autoscroll { vx, vy } => json!({ "type": "autoscroll", "vx": vx, "vy": vy }),
        Request::CapturePointer => json!({ "type": "capture_pointer" }),
        Request::ReleasePointer => json!({ "type": "release_pointer" }),
        Request::Clipboard { text, html } => json!({ "type": "clipboard", "text": text, "html": html }),
        Request::Placeholder(p) => json!({
            "type": "placeholder",
            "placeholder": { "key": p.key, "label": p.label, "width": p.width, "height": p.height, "merge": p.merge },
        }),
    }
}

fn element(e: &Element, idx: &DocIndex) -> Value {
    let mut v = json!({
        "id": elements::id_str(e.id), "kind": e.kind(), "display": e.display, "story": elements::story_name(e.story, idx),
        "parent": e.parent.map(elements::id_str), "locked": e.locked,
    });
    if let Some(c) = &e.code {
        v["code"] = json!(c);
    }
    if let Some(p) = &e.props {
        v["tag"] = json!(p.tag);
        v["alias"] = json!(p.alias);
        v["lock"] = json!(match p.lock {
            Lock::None => "none",
            Lock::ControlLocked => "control",
            Lock::ContentLocked => "content",
            Lock::Both => "both",
        });
        v["showing_placeholder"] = json!(p.showing_placeholder);
        v["binding"] = json!(p.binding.is_some());
        match &p.kind {
            ControlKind::PlainText { multi_line } => v["props"] = json!({ "multi_line": multi_line }),
            ControlKind::Checkbox { checked, unchecked } => {
                v["props"] = json!({ "checked": checked.to_string(), "unchecked": unchecked.to_string() })
            }
            ControlKind::Date { format, lid } => v["props"] = json!({ "format": format, "lid": lid }),
            _ => {}
        }
    }
    v
}

fn changes(c: &Changes, idx: &DocIndex) -> Value {
    json!({
        "added": c.added.iter().map(|e| element(e, idx)).collect::<Vec<_>>(),
        "removed": c.removed.iter().map(|e| elements::id_str(*e)).collect::<Vec<_>>(),
        "changed": c.changed.iter().map(|e| element(e, idx)).collect::<Vec<_>>(),
    })
}

/// The element context as the reply carries it.
fn context(doc: &Document, idx: &DocIndex, sel: &Selection, sigs: &HashMap<ElementId, Element>) -> Value {
    Value::Array(
        elements::context(doc, idx, sel)
            .into_iter()
            .map(|(id, exact)| {
                let mut v = json!({ "id": elements::id_str(id), "exact": exact });
                match sigs.get(&id) {
                    Some(e) => {
                        v["kind"] = json!(e.kind());
                        if let Some(c) = &e.code {
                            v["code"] = json!(c);
                        }
                        if let Some(t) = e.props.as_ref().and_then(|p| p.tag.as_deref()) {
                            v["tag"] = json!(t);
                        }
                    }
                    None => v["kind"] = json!("field"),
                }
                v
            })
            .collect(),
    )
}

/// The block structure of `blocks`, to `depth` table levels.
fn structure(blocks: &[Block], depth: usize) -> Value {
    Value::Array(
        blocks
            .iter()
            .map(|b| match b {
                Block::Para(p) => {
                    let ids: Vec<String> = all_ids(p).into_iter().map(elements::id_str).collect();
                    json!({ "para": { "text_len": UnitText::of(p).len().0, "elements": ids } })
                }
                Block::Table(t) => json!({ "table": { "rows": t.rows.iter().map(|r| json!({ "cells": r.cells.iter().map(|c| {
                    let has = c.blocks.iter().any(|b| match b { Block::Para(p) => !crate::docx::fields::text(p).trim().is_empty() || p.inlines.iter().any(|i| i.units() > 0 && !matches!(i, Inline::Text(..))), Block::Table(_) => true });
                    let mut v = json!({ "span": c.span.max(1), "vmerge": format!("{:?}", c.v_merge).to_lowercase(), "has_content": has });
                    if depth > 0 { v["blocks"] = structure(&c.blocks, depth - 1); }
                    v
                }).collect::<Vec<_>>() })).collect::<Vec<_>>() } }),
            })
            .collect(),
    )
}

fn all_ids(p: &crate::docx::Paragraph) -> Vec<ElementId> {
    p.inlines
        .iter()
        .filter_map(|i| match i {
            Inline::FieldBegin { id, .. } => Some(ElementId::Field(*id)),
            Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => Some(ElementId::Control(*c)),
            _ => None,
        })
        .collect()
}
