//! The `rust` contract runner: every script in `tests/contract/` against `twip::api::Engine`, directly. The step
//! kinds, the matching rules and `pending.txt` are described in `tests/contract/README.md`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use twip::api::command::{Command, MoveSelection, MoveUnit};
use twip::api::engine::{Engine, SessionId, SessionOptions};
use twip::api::error::{Error, ErrorCode};
use twip::api::event::Event;
use twip::api::fonts::FontLibrary;
use twip::api::input::{Key, Modifiers, Point, PointerButton, PointerKind};
use twip::api::review::{CommentAnchor, stack_balloons};
use twip::api::services::{FindDirection, FindQuery};
use twip::api::session::{ExportOptions, Session};
use twip::api::value::{Bytes, Direction};

const RUNNER: &str = "rust";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> PathBuf {
    root().join("crates/twip/tests/fixtures")
}

/// A listener step: run `command` once, the first time an event of `event` type arrives.
struct Listen {
    event: String,
    command: Command,
    fired: bool,
    /// The `listen` step, which a failure of `command` names.
    at: String,
}

/// How the next operation is cancelled.
#[derive(Clone, Copy, PartialEq)]
enum Abort {
    None,
    /// Before it runs: it fails `aborted`.
    Early,
    /// After it commits: ignored.
    Late,
}

struct Runner {
    fixture: Option<String>,
    engine: Engine,
    session: SessionId,
    destroyed: bool,
    /// Events since the last `expectEvents`, as their wire values (state travels apart, as in the SDKs).
    events: Rc<RefCell<Vec<Value>>>,
    /// A `documentChanged` that arrived before the state reached its revision.
    stale: Rc<RefCell<Option<String>>>,
    /// How many of `events` the listeners have seen.
    heard: usize,
    listeners: Vec<Listen>,
    abort: Abort,
    /// The last step's failure, for `expectError`, and the step it came from.
    failed: Option<(String, Error)>,
    saved: HashMap<String, Value>,
    /// The bytes of the last `docx` export, for `reopen`.
    exported: Option<Vec<u8>>,
}

type Step = Result<(), String>;

fn parse<T: DeserializeOwned>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| format!("{e} in {v}"))
}

/// `parse` for an operation: a bad value is the script's failure.
fn arg_of<T: DeserializeOwned>(v: Value) -> Result<T, Result<Error, String>> {
    parse(&v).map_err(Err)
}

fn to_json<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).expect("api values serialize")
}

/// A `twip::api` command from a command method's TypeScript arguments: one argument is the op's `value`, an options
/// object is spread into the op.
fn command_of(id: &str, args: &[Value]) -> Result<Command, String> {
    let mut op = json!({ "type": id });
    match args {
        [] => {}
        [arg] => {
            op["value"] = arg.clone();
            if let (Err(_), Value::Object(fields)) = (parse::<Command>(&op), arg) {
                op = json!({ "type": id });
                for (k, v) in fields {
                    op[k] = v.clone();
                }
            }
        }
        _ => return Err(format!("commands.{id} takes one argument, {} given", args.len())),
    }
    parse(&op)
}

impl Runner {
    fn new(script: &Value) -> Runner {
        let fonts = match script["fonts"].as_str() {
            Some("empty") => FontLibrary::empty(),
            _ => FontLibrary::system(),
        };
        let mut engine = Engine::new(fonts);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (sink, stale) = (events.clone(), Rc::new(RefCell::new(None)));
        let (report, mut revision) = (stale.clone(), 0);
        engine.set_listener(move |_, e| match e {
            Event::StateChanged(state) => revision = state.revision,
            e => {
                if let Event::DocumentChanged(record) = e
                    && record.revision != revision
                {
                    report.borrow_mut().get_or_insert(format!("documentChanged {} arrived with the state at {revision}", record.revision));
                }
                sink.borrow_mut().push(to_json(e));
            }
        });
        let session = engine.create_session(SessionOptions::default());
        Runner {
            fixture: script["fixture"].as_str().map(str::to_string),
            engine,
            session,
            destroyed: false,
            events,
            stale,
            heard: 0,
            listeners: Vec::new(),
            abort: Abort::None,
            failed: None,
            saved: HashMap::new(),
            exported: None,
        }
    }

    fn session(&mut self) -> Result<&mut Session, Error> {
        if self.destroyed {
            return Err(Error::new(ErrorCode::Destroyed, "the session was destroyed"));
        }
        self.engine.session(self.session).ok_or_else(|| Error::new(ErrorCode::Destroyed, "no such session"))
    }

    fn read(file: &str) -> Result<Vec<u8>, String> {
        let path = fixtures().join(file);
        std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// `v` with every `$file` and `$saved` replaced by its value: a file is its bytes in their wire form (base64).
    fn resolve(&self, v: &Value) -> Result<Value, String> {
        Ok(match v {
            Value::Object(o) if o.contains_key("$file") => to_json(Bytes(Runner::read(o["$file"].as_str().unwrap_or(""))?)),
            Value::Object(o) if o.contains_key("$saved") => {
                let name = o["$saved"].as_str().unwrap_or("");
                let mut at = self.saved.get(name).ok_or_else(|| format!("nothing saved as {name}"))?.clone();
                for step in o.get("path").and_then(Value::as_array).into_iter().flatten() {
                    at = match step {
                        Value::String(k) => at.get(k).cloned(),
                        Value::Number(n) => n.as_u64().and_then(|i| at.get(i as usize).cloned()),
                        m => items(&at).into_iter().find(|e| matches(m, e, "").is_ok()).cloned(),
                    }
                    .ok_or_else(|| format!("${name}: no {step} in {at}"))?;
                }
                at
            }
            Value::Object(o) => Value::Object(o.iter().map(|(k, x)| Ok((k.clone(), self.resolve(x)?))).collect::<Result<_, String>>()?),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.resolve(x)).collect::<Result<_, _>>()?),
            v => v.clone(),
        })
    }

    /// Runs one operation step. `Err(Ok(e))` is the operation's failure, `Err(Err(why))` the script's.
    fn operate(&mut self, op: &str, step: &Value) -> Result<(), Result<Error, String>> {
        // Only opens and exports take a cancellation, as in the SDKs; another operation leaves `abort` for the next.
        let abort =
            if matches!(op, "open" | "openBytes" | "reopen" | "export") { std::mem::replace(&mut self.abort, Abort::None) } else { Abort::None };
        let aborted = |what: &str| Err(Ok(Error::new(ErrorCode::Aborted, format!("the {what} was aborted"))));
        match op {
            "open" | "openBytes" | "reopen" => {
                let bytes = match op {
                    "open" => {
                        Runner::read(self.fixture.as_deref().ok_or_else(|| Err("`open` in a script with no fixture".to_string()))?).map_err(Err)?
                    }
                    "reopen" => self.exported.clone().ok_or_else(|| Err("`reopen` before any docx export".to_string()))?,
                    _ => parse::<Bytes>(&step["base64"]).map_err(Err)?.0,
                };
                let s = self.session().map_err(Ok)?;
                let token = s.open_prepare(&bytes).map_err(Ok)?;
                if abort == Abort::Early {
                    s.open_discard(token);
                    return aborted("open");
                }
                let warnings = s.open_commit(token).map_err(Ok)?;
                if let (Some(want), "open") = (step.get("match"), op) {
                    matches(want, &json!({ "warnings": warnings }), "open").map_err(Err)?;
                }
                Ok(())
            }
            "export" => {
                let options: ExportOptions = parse(&json!({ "format": step["format"] })).map_err(Err)?;
                let s = self.session().map_err(Ok)?;
                if abort == Abort::Early {
                    return aborted("export");
                }
                let export = s.export(&options).map_err(Ok)?;
                let now = s.state().revision;
                if export.revision != now {
                    return Err(Err(format!("the export reports revision {}, the state is at {now}", export.revision)));
                }
                let head: String = export.bytes.0.iter().take(4).map(|b| char::from(*b)).collect();
                if let Some(want) = step.get("match") {
                    matches(want, &json!({ "revision": export.revision, "warnings": export.warnings, "head": head }), "export").map_err(Err)?;
                }
                if options.format == twip::api::session::ExportFormat::Docx {
                    self.exported = Some(export.bytes.0);
                }
                Ok(())
            }
            "command" | "batch" => {
                let s = self.session().map_err(Ok)?;
                if op == "command" {
                    s.run(parse(&step["command"]).map_err(Err)?).map(drop).map_err(Ok)
                } else {
                    s.batch(parse(&step["commands"]).map_err(Err)?, step["label"].as_str().map(str::to_string)).map(drop).map_err(Ok)
                }
            }
            "call" => {
                let args = match step.get("args") {
                    Some(a) => parse::<Vec<Value>>(a).map_err(Err)?,
                    None => Vec::new(),
                };
                let method = step["method"].as_str().unwrap_or("");
                let result = self.call(method, &args)?;
                if let Some(want) = step.get("result") {
                    matches(want, &result, "result").map_err(Err)?;
                }
                if let Some(name) = step["save"].as_str() {
                    self.saved.insert(name.to_string(), result);
                }
                Ok(())
            }
            "key" => {
                let key = key_of(step["key"].as_str().unwrap_or("")).map_err(Err)?;
                let mods = mods_of(step).map_err(Err)?;
                self.session().map_err(Ok)?.key(key, mods);
                Ok(())
            }
            "text" => {
                let text = step["text"].as_str().ok_or_else(|| Err("`text` needs text".to_string()))?.to_string();
                self.session().map_err(Ok)?.text(&text);
                Ok(())
            }
            "click" => {
                let text = step["text"].as_str().ok_or_else(|| Err("`click` needs text".to_string()))?;
                let mods = mods_of(step).map_err(Err)?;
                let s = self.session().map_err(Ok)?;
                let at = middle_of(s, text).ok_or_else(|| Err(format!("no `{text}` to click")))?;
                s.pointer(PointerKind::Down, at, PointerButton::Primary, 1, mods);
                s.pointer(PointerKind::Up, at, PointerButton::Primary, 1, mods);
                Ok(())
            }
            "font" => {
                let bytes = Runner::read(step["file"].as_str().unwrap_or("")).map_err(Err)?;
                let added = self.engine.fonts().add(&bytes).map(drop).map_err(Ok);
                self.engine.sync_fonts();
                added
            }
            "destroy" => {
                self.engine.close_session(self.session);
                self.destroyed = true;
                Ok(())
            }
            other => Err(Err(format!("unknown step `{other}`"))),
        }
    }

    /// A `call` step: a session member by its TypeScript path, with its result as JSON.
    fn call(&mut self, method: &str, args: &[Value]) -> Result<Value, Result<Error, String>> {
        let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Null);
        if let Some(name) = method.strip_prefix('@') {
            let (name, member) = name.split_once('.').ok_or_else(|| Err(format!("no member in {method}")))?;
            let model = self.saved.get(name).cloned().ok_or_else(|| Err(format!("nothing saved as {name}")))?;
            // A patch is the changed values, sent tagged with the dialog's kind, as the SDKs send it.
            let mut patch = arg(0);
            if let Value::Object(fields) = &mut patch {
                fields.insert("kind".into(), model["kind"].clone());
            }
            let s = self.session().map_err(Ok)?;
            return match member {
                "update" => Ok(to_json(s.dialog_update(arg_of(model)?, arg_of(patch)?).map_err(Ok)?)),
                "apply" => s.dialog_apply(arg_of(model)?).map(|_| Value::Null).map_err(Ok),
                "cancel" => Ok(Value::Null),
                _ => Err(Err(format!("no dialog member {member}"))),
            };
        }
        match method {
            "catalog" => return Ok(to_json(Session::catalog())),
            "fonts.substitute" => {
                let rules: Vec<Value> = match arg(0) {
                    Value::Object(o) => o.into_iter().map(|(from, to)| json!({ "from": from, "to": to })).collect(),
                    v => arg_of(v)?,
                };
                self.engine.fonts().set_substitutions(arg_of(Value::Array(rules))?);
                self.engine.sync_fonts();
                return Ok(Value::Null);
            }
            _ => {}
        }
        let s = self.session().map_err(Ok)?;
        let none = Value::Null;
        Ok(match method {
            "setReadOnly" => {
                s.set_read_only(arg_of(arg(0))?);
                none
            }
            "setAuthor" => {
                s.set_author(arg_of(arg(0))?);
                none
            }
            "find.next" => to_json(s.find(&arg_of(arg(0))?, FindDirection::Next)),
            "find.previous" => to_json(s.find(&arg_of(arg(0))?, FindDirection::Previous)),
            "find.all" => to_json(s.find_all(&arg_of(arg(0))?)),
            "find.select" => to_json(s.select_match(arg_of(arg(0))?).map_err(Ok)?),
            "find.replace" => to_json(s.replace(&arg_of(arg(0))?, &arg_of::<String>(arg(1))?).map_err(Ok)?),
            "find.replaceAll" => json!({ "count": s.replace_all(&arg_of(arg(0))?, &arg_of::<String>(arg(1))?).map_err(Ok)? }),
            "dialogs.open" => to_json(s.dialog_open(arg_of(arg(0))?).map_err(Ok)?),
            "menus.contextMenu" => to_json(s.context_menu()),
            "menus.invoke" => {
                s.invoke(arg_of(arg(0))?).map_err(Ok)?;
                none
            }
            "spelling.addDictionary" => to_json(s.spelling().add_dictionary(arg_of(arg(0))?).map_err(Ok)?),
            "spelling.setEnabled" => {
                s.spelling().set_enabled(arg_of(arg(0))?);
                none
            }
            "spelling.setLanguage" => {
                s.spelling().set_language(&arg_of::<String>(arg(0))?);
                none
            }
            "spelling.ignore" => {
                s.spelling().ignore(&arg_of::<String>(arg(0))?);
                none
            }
            "spelling.add" => {
                s.spelling().add(&arg_of::<String>(arg(0))?);
                none
            }
            "spelling.loadUserWords" => {
                s.spelling().load_user_words(&arg_of::<String>(arg(0))?, &arg_of::<Vec<String>>(arg(1))?);
                none
            }
            "spelling.suggestions" => to_json(s.spelling().suggestions(&arg_of::<String>(arg(0))?, arg_of::<Option<String>>(arg(1))?.as_deref())),
            "spelling.next" => to_json(s.spelling().next_misspelling()),
            "navigation.headings" => to_json(s.headings()),
            "navigation.select" => to_json(s.select_heading(arg_of(arg(0))?).map_err(Ok)?),
            "styles.list" => to_json(s.styles(arg_of(arg(0).get("type").cloned().unwrap_or(Value::Null))?)),
            "review.revisions" => to_json(s.revisions()),
            "review.comments" => to_json(s.comments()),
            m => match m.strip_prefix("commands.") {
                Some(id) => to_json(s.run(command_of(id, args).map_err(Err)?).map_err(Ok)?.value),
                None => return Err(Err(format!("no call method {m}"))),
            },
        })
    }

    /// Runs the listeners whose event arrived, each once, after the operation that caused it. A listener's command
    /// that fails fails the script.
    fn run_listeners(&mut self) -> Step {
        loop {
            let seen: Vec<String> = self.events.borrow()[self.heard..].iter().filter_map(|e| e["type"].as_str().map(str::to_string)).collect();
            self.heard = self.events.borrow().len();
            let mut commands = Vec::new();
            for l in self.listeners.iter_mut().filter(|l| !l.fired && seen.contains(&l.event)) {
                l.fired = true;
                commands.push((l.at.clone(), l.command.clone()));
            }
            if commands.is_empty() {
                return Ok(());
            }
            for (at, command) in commands {
                self.session().and_then(|s| s.run(command)).map_err(|e| format!("{at} (listener): {e}"))?;
            }
        }
    }

    fn step(&mut self, at: &str, step: &Value) -> Step {
        self.step_inner(at, step)?;
        match self.stale.borrow_mut().take() {
            Some(why) => Err(format!("{at}: {why}")),
            None => Ok(()),
        }
    }

    fn step_inner(&mut self, at: &str, step: &Value) -> Step {
        let op = step["op"].as_str().unwrap_or("");
        if op != "expectError"
            && let Some((at, e)) = self.failed.take()
        {
            return Err(format!("{at}: failed unexpectedly: {e}"));
        }
        // A repeated step resolves on each run, as `$saved` may name what an earlier run saved.
        let step = &if op == "repeat" { step.clone() } else { self.resolve(step).map_err(|e| format!("{at}: {e}"))? };
        let fail = |why: String| format!("{at}: {why}");
        // Idle before an expectation: background layout runs to the end, as the SDKs' timers run it.
        if op.starts_with("expect")
            && let Ok(s) = self.session()
        {
            while s.host().idle() {}
        }
        match op {
            "expectState" => {
                let state = match self.session() {
                    Ok(s) => to_json(s.state()),
                    Err(e) => return Err(fail(e.to_string())),
                };
                matches(&step["match"], &state, "state").map_err(fail)
            }
            "expectEvents" => {
                let events = std::mem::take(&mut *self.events.borrow_mut());
                self.heard = 0;
                subsequence(step["match"].as_array().map(Vec::as_slice).unwrap_or_default(), &events)
                    .map_err(|why| fail(format!("{why}; the events were {}", Value::Array(events))))
            }
            "expectError" => {
                let want: ErrorCode = parse(&step["code"]).map_err(fail)?;
                match self.failed.take() {
                    Some((_, e)) if e.code == want => Ok(()),
                    Some((_, e)) => Err(fail(format!("expected {want:?}, the step failed with {e}"))),
                    None => Err(fail(format!("expected {want:?}, the step succeeded"))),
                }
            }
            "abort" => {
                self.abort = if step["late"].as_bool() == Some(true) { Abort::Late } else { Abort::Early };
                Ok(())
            }
            "listen" => {
                let event = step["type"].as_str().unwrap_or("").to_string();
                let command = parse(&step["command"]).map_err(fail)?;
                self.listeners.push(Listen { event, command, fired: false, at: at.to_string() });
                Ok(())
            }
            "concurrent" => {
                // Operations submitted together run in submission order (the direct engine has one queue, the
                // caller's), and listeners' commands after them. The step fails with the first that fails.
                for (i, s) in step["steps"].as_array().into_iter().flatten().enumerate() {
                    match self.operate(s["op"].as_str().unwrap_or(""), s) {
                        Ok(()) => {}
                        Err(Ok(e)) => {
                            self.failed.get_or_insert((at.to_string(), e));
                        }
                        Err(Err(why)) => return Err(format!("{at}.{i}: {why}")),
                    }
                }
                self.run_listeners()
            }
            "repeat" => {
                for i in 0..step["count"].as_u64().unwrap_or(0) {
                    self.step(&format!("{at}, repeat {i}"), &step["step"])?;
                }
                Ok(())
            }
            _ => {
                match self.operate(op, step) {
                    Ok(()) => {}
                    Err(Ok(e)) => self.failed = Some((at.to_string(), e)),
                    Err(Err(why)) => return Err(fail(why)),
                }
                self.run_listeners()
            }
        }
    }
}

/// The physical key for a DOM `KeyboardEvent.key` value.
/// The middle of the first occurrence of `text` (of its first line, when it wraps), viewport px. The caret is left
/// at its start.
fn middle_of(s: &mut Session, text: &str) -> Option<Point> {
    if !s.find(&FindQuery { text: text.into(), ..FindQuery::default() }, FindDirection::Next).found {
        return None;
    }
    let end = s.host().frame_info();
    let back = MoveSelection { by: MoveUnit::Character, direction: Direction::Backward, extend: None };
    s.run(Command::MoveSelection(back)).ok()?;
    let start = s.host().frame_info();
    let x = if start.caret_y == end.caret_y { (start.caret_x + end.caret_x) / 2.0 } else { start.caret_x + 2.0 };
    Some(Point { x, y: start.caret_y + start.caret_height / 2.0 })
}

/// A step's `mods`.
fn mods_of(step: &Value) -> Result<Modifiers, String> {
    let mut mods = Modifiers::default();
    for m in step["mods"].as_array().into_iter().flatten() {
        match m.as_str() {
            Some("ctrl") => mods.ctrl = true,
            Some("shift") => mods.shift = true,
            Some("alt") => mods.alt = true,
            Some("meta") => mods.meta = true,
            _ => return Err(format!("unknown modifier {m}")),
        }
    }
    Ok(mods)
}

fn key_of(name: &str) -> Result<Key, String> {
    let code = match name {
        " " => "Space".to_string(),
        "-" => "Minus".into(),
        "=" => "Equal".into(),
        "[" => "BracketLeft".into(),
        "]" => "BracketRight".into(),
        "\\" => "Backslash".into(),
        ";" => "Semicolon".into(),
        "'" => "Quote".into(),
        "`" => "Backquote".into(),
        "," => "Comma".into(),
        "." => "Period".into(),
        "/" => "Slash".into(),
        n if n.len() == 1 && n.as_bytes()[0].is_ascii_alphabetic() => format!("Key{}", n.to_ascii_uppercase()),
        n if n.len() == 1 && n.as_bytes()[0].is_ascii_digit() => format!("Digit{n}"),
        n => n.into(),
    };
    parse(&Value::String(code))
}

/// The elements a `$has` or a `$saved` path searches: an array's elements, and those of their nested `items` arrays.
fn items(v: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for e in v.as_array().into_iter().flatten() {
        out.push(e);
        if let Some(nested) = e.get("items") {
            out.extend(items(nested));
        }
    }
    out
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Exact equality, numbers by value, and a missing key equal to null.
fn equal(a: &Value, b: &Value) -> bool {
    let at = |o: &serde_json::Map<String, Value>, k: &String| o.get(k).cloned().unwrap_or(Value::Null);
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| equal(x, y)),
        (Value::Object(x), Value::Object(y)) => x.keys().chain(y.keys()).all(|k| equal(&at(x, k), &at(y, k))),
        (a, b) => a == b,
    }
}

/// Whether each of `wants` matches a different one of `got`, in any order.
fn assign(wants: &[Value], got: &[&Value], used: &mut Vec<bool>) -> bool {
    let Some((first, rest)) = wants.split_first() else { return true };
    for i in 0..got.len() {
        if !used[i] && matches(first, got[i], "").is_ok() {
            used[i] = true;
            if assign(rest, got, used) {
                return true;
            }
            used[i] = false;
        }
    }
    false
}

/// A partial deep match (README "Matching"): objects compare the keys `want` names (a missing key is null), arrays
/// element by element (`want` may be shorter), numbers by value; `$exact`, `$has` and `$type` test differently.
fn matches(want: &Value, got: &Value, path: &str) -> Result<(), String> {
    if let Value::Object(w) = want
        && w.len() == 1
    {
        if let Some(exact) = w.get("$exact") {
            return if equal(exact, got) { Ok(()) } else { Err(format!("{path} is {got}, exactly {exact} expected")) };
        }
        if let Some(Value::String(t)) = w.get("$type") {
            return if type_name(got) == t { Ok(()) } else { Err(format!("{path} is {got}, a {t} expected")) };
        }
        if let Some(Value::Array(has)) = w.get("$has") {
            let all = items(got);
            if !got.is_array() || !assign(has, &all, &mut vec![false; all.len()]) {
                return Err(format!("{path} is {got}, which doesn't have each of {}", Value::Array(has.clone())));
            }
            return Ok(());
        }
    }
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => {
            w.iter().try_for_each(|(k, wv)| matches(wv, g.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}")))
        }
        (Value::Array(w), Value::Array(g)) => {
            if w.len() > g.len() {
                return Err(format!("{path} has {} items, {} expected: {got}", g.len(), w.len()));
            }
            w.iter().zip(g).enumerate().try_for_each(|(i, (wv, gv))| matches(wv, gv, &format!("{path}[{i}]")))
        }
        (Value::Number(w), Value::Number(g)) if w.as_f64() == g.as_f64() => Ok(()),
        (w, g) if w == g => Ok(()),
        (w, g) => Err(format!("{path} is {g}, {w} expected")),
    }
}

/// `wants`, in order, against a subsequence of `events`.
fn subsequence(wants: &[Value], events: &[Value]) -> Result<(), String> {
    let mut next = 0;
    for (i, want) in wants.iter().enumerate() {
        match events[next..].iter().position(|e| matches(want, e, "").is_ok()) {
            Some(at) => next += at + 1,
            None => return Err(format!("events[{i}]: no later event matches {want}")),
        }
    }
    Ok(())
}

/// Runs one script; `Err` names the step that failed and why.
fn run(script: &Value) -> Step {
    let mut runner = Runner::new(script);
    for (i, step) in script["steps"].as_array().into_iter().flatten().enumerate() {
        let at = format!("step {i} ({})", step["op"].as_str().unwrap_or("?"));
        catch_unwind(AssertUnwindSafe(|| runner.step(&at, step))).unwrap_or_else(|_| Err(format!("{at}: the engine panicked")))?;
    }
    match runner.failed.take() {
        Some((at, e)) => Err(format!("{at}: failed unexpectedly: {e}")),
        None => Ok(()),
    }
}

#[test]
fn contract_scripts() {
    let dir = root().join("tests/contract");
    let pending: Vec<String> = std::fs::read_to_string(dir.join("pending.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    scripts.sort();
    assert!(!scripts.is_empty(), "no contract scripts in {}", dir.display());
    let (mut passed, mut skipped, mut problems) = (0, 0, Vec::new());
    for path in scripts {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let script: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{RUNNER}: {name}: {e}"));
        let result = catch_unwind(AssertUnwindSafe(|| run(&script))).unwrap_or_else(|_| Err("the engine panicked outside a step".into()));
        match (result, pending.contains(&name)) {
            (Ok(()), false) => passed += 1,
            (Err(e), true) => {
                println!("{RUNNER}: {name}: skipped (pending): {e}");
                skipped += 1;
            }
            (Ok(()), true) => problems.push(format!("{RUNNER}: {name}: passes, but pending.txt lists it: delete its line")),
            (Err(why), false) => problems.push(format!("{RUNNER}: {name}: {why}")),
        }
    }
    println!("{RUNNER}: {passed} passed, {skipped} skipped (pending)");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// `stack_balloons` gives every case's tops in `tests/wire/balloons.json`; the SDKs check the same cases.
#[test]
fn balloon_stacking() {
    let path = root().join("crates/twip/tests/wire/balloons.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let cases: Vec<Value> = serde_json::from_str(&text).expect("balloons.json parses");
    assert!(!cases.is_empty(), "no cases in {}", path.display());
    for case in &cases {
        let name = &case["name"];
        let anchors: Vec<CommentAnchor> = parse(&case["anchors"]).unwrap_or_else(|e| panic!("{RUNNER}: balloons {name}: {e}"));
        let heights: Vec<f64> = parse(&case["heights"]).unwrap_or_else(|e| panic!("{RUNNER}: balloons {name}: {e}"));
        let want: Vec<f64> = parse(&case["tops"]).unwrap_or_else(|e| panic!("{RUNNER}: balloons {name}: {e}"));
        let tops = stack_balloons(&anchors, &heights, case["gap"].as_f64().expect("a gap"));
        assert_eq!(tops, want, "{RUNNER}: balloons {name}");
    }
}

#[test]
fn partial_matches() {
    let got = json!({ "a": 1, "b": [1, 2, 3], "c": { "d": "x", "e": null }, "m": [{ "k": 1, "items": [{ "k": 2 }] }] });
    assert!(matches(&json!({ "a": 1.0, "b": [1, 2], "c": { "e": null, "f": null } }), &got, "v").is_ok());
    assert!(matches(&json!({ "b": [2] }), &got, "v").is_err());
    assert!(matches(&json!({ "f": 1 }), &got, "v").is_err());
    assert!(matches(&json!({ "b": [1, 2, 3, 4] }), &got, "v").is_err());
    assert!(matches(&json!({ "b": { "$exact": [1, 2, 3] } }), &got, "v").is_ok());
    assert!(matches(&json!({ "b": { "$exact": [1, 2] } }), &got, "v").is_err());
    assert!(matches(&json!({ "c": { "$exact": { "d": "x" } } }), &got, "v").is_ok());
    assert!(matches(&json!({ "c": { "$exact": { "d": "x", "e": 1 } } }), &got, "v").is_err());
    assert!(matches(&json!({ "b": { "$has": [3, 1] } }), &got, "v").is_ok());
    assert!(matches(&json!({ "b": { "$has": [1, 1] } }), &got, "v").is_err());
    assert!(matches(&json!({ "m": { "$has": [{ "k": 2 }, { "k": 1 }] } }), &got, "v").is_ok());
    assert!(matches(&json!({ "c": { "d": { "$type": "string" } } }), &got, "v").is_ok());
    assert!(matches(&json!({ "a": { "$type": "string" } }), &got, "v").is_err());
    let events = [json!({ "type": "a" }), json!({ "type": "b" }), json!({ "type": "c" })];
    assert!(subsequence(&[json!({ "type": "a" }), json!({ "type": "c" })], &events).is_ok());
    assert!(subsequence(&[json!({ "type": "c" }), json!({ "type": "a" })], &events).is_err());
}
