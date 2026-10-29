//! The C ABI over `twip::api`. It is internal: the SDKs' private layers call it, applications never do, and it may
//! change in any release. `include/twip.h` declares it.
//!
//! - Structured values cross as UTF-8 JSON, the `twip::api` types' serde form. A response is `{"ok": value}` or
//!   `{"error": {code, message, details}}`, written to a [`TwipBuf`] the caller frees with [`twip_buf_free`]. The
//!   status returned beside it is 0, or the negated error code ([`status`]).
//! - Hot input (keys, text, pointer, frames) is typed calls with primitive arguments.
//! - The engine calls back (`kind` 1 event, 2 state, 3 request frame, 4 set timer, 5 font request, 6 host request)
//!   after each entry point's work, the state before any event of the same change, never re-entrantly. Natively the
//!   callback is the function pointer given to [`twip_engine_new`]; in the browser it is `twip-host.js`'s `twip_emit`.
//! - Every entry point runs under `catch_unwind`: a panic becomes an `internal` error and a fatal `error` event, and
//!   every later call fails `internal`, except the frees. That needs `panic = "unwind"`: under `abort` (the release
//!   profile) a panic ends the process, or the module instance in the browser. An allocation failure aborts in
//!   either profile; `twip_alloc` alone answers it, with null.
//!
//! Every call is made on the engine's thread. Pointers are checked for null; their validity is the caller's promise.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use engine::api::engine::{Engine, SessionId, SessionOptions};
use engine::api::error::{Error, ErrorCode};
use engine::api::event::{Event, HostCommand, KeyOutcome};
use engine::api::fonts::FontLibrary;
use engine::api::input::{CompositionPhase, Key, Modifiers, Point, PointerButton, PointerKind};
use engine::api::session::{ExportOptions, FrameInfo, OpenToken, Session};
use serde::Deserialize;
use serde_json::{Value, json};

mod methods;

pub use methods::Method;

/// The callback natively: `kind` and its payload, valid only during the call.
pub type TwipCallback = Option<unsafe extern "C" fn(ctx: *mut c_void, session: u32, kind: u32, ptr: *const u8, len: usize)>;

/// Callback kinds.
pub mod kind {
    /// An `api::Event` other than `stateChanged` (JSON).
    pub const EVENT: u32 = 1;
    /// An `api::State` (JSON).
    pub const STATE: u32 = 2;
    /// Draw a frame; no payload.
    pub const REQUEST_FRAME: u32 = 3;
    /// Arm a one-shot timer, then call `twip_timer_fired`: `{ timer, ms }`.
    pub const SET_TIMER: u32 = 4;
    /// An `api::FontRequest` for the SDK's resolver (JSON).
    pub const FONT_REQUEST: u32 = 5;
    /// A request for the host: `{ type, ... }` (JSON).
    pub const HOST_REQUEST: u32 = 6;
}

/// The timer that runs one slice of background layout ([`kind::SET_TIMER`]), armed while a session's
/// `pages.complete` is false.
const LAYOUT_TIMER: u32 = 1;

/// Bytes the engine made, owned by the engine until [`twip_buf_free`].
#[repr(C)]
pub struct TwipBuf {
    pub ptr: *mut u8,
    pub len: usize,
}

/// What a frame call reports: the page stack's size and the caret, CSS px.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TwipFrameInfo {
    pub content_width: f64,
    pub content_height: f64,
    pub caret_x: f64,
    pub caret_y: f64,
    pub caret_height: f64,
    /// 1 when a caret shows.
    pub caret_visible: u32,
    pub pages: u32,
}

impl From<FrameInfo> for TwipFrameInfo {
    fn from(f: FrameInfo) -> TwipFrameInfo {
        TwipFrameInfo {
            content_width: f.content_width,
            content_height: f.content_height,
            caret_x: f.caret_x,
            caret_y: f.caret_y,
            caret_height: f.caret_height,
            caret_visible: u32::from(f.caret_visible),
            pages: f.pages,
        }
    }
}

/// The status of a call: 0, or the error code negated, counting from 1 in [`ErrorCode`]'s order (`-1` destroyed,
/// `-2` invalid argument, ..., `-10` internal).
pub fn status(code: ErrorCode) -> i32 {
    -match code {
        ErrorCode::Destroyed => 1,
        ErrorCode::InvalidArgument => 2,
        ErrorCode::Unavailable => 3,
        ErrorCode::ReadOnly => 4,
        ErrorCode::ImportFailed => 5,
        ErrorCode::ExportFailed => 6,
        ErrorCode::Aborted => 7,
        ErrorCode::NotAllowed => 8,
        ErrorCode::Unsupported => 9,
        ErrorCode::Internal => 10,
    }
}

/// A message for the SDK, queued while the engine works and delivered after.
struct Message {
    session: u32,
    kind: u32,
    payload: Vec<u8>,
}

/// The engine behind a `TwipEngine*`.
pub struct TwipEngine {
    engine: Engine,
    callback: TwipCallback,
    context: *mut c_void,
    // Shared with the engine's listener, which queues into it during a call; delivered when the call returns.
    outbox: Rc<RefCell<VecDeque<Message>>>,
    /// Sessions a host gave a surface: they get a frame request after each change, and host requests.
    surfaces: BTreeSet<u32>,
    /// Sessions with a background layout timer armed.
    armed: BTreeSet<u32>,
    /// Documents `twip_open_prepare` read, by token, with their session.
    prepared: BTreeMap<u32, (u32, OpenToken)>,
    next_token: u32,
    /// A panic happened: every later call fails `internal`.
    faulted: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct EngineOptions {
    /// `"system"` (the default) or `"empty"`.
    fonts: Option<String>,
}

impl TwipEngine {
    fn new(options: EngineOptions, callback: TwipCallback, context: *mut c_void) -> TwipEngine {
        let fonts = match options.fonts.as_deref() {
            Some("empty") => FontLibrary::empty(),
            _ => FontLibrary::system(),
        };
        let mut engine = Engine::new(fonts);
        let outbox: Rc<RefCell<VecDeque<Message>>> = Rc::default();
        let queue = outbox.clone();
        engine.set_listener(move |session, event| {
            let kind = if matches!(event, Event::StateChanged(_)) { kind::STATE } else { kind::EVENT };
            let payload = match event {
                Event::StateChanged(state) => serde_json::to_vec(state),
                e => serde_json::to_vec(e),
            };
            if let Ok(payload) = payload {
                queue.borrow_mut().push_back(Message { session: session.0, kind, payload });
            }
        });
        TwipEngine {
            engine,
            callback,
            context,
            outbox,
            surfaces: BTreeSet::new(),
            armed: BTreeSet::new(),
            prepared: BTreeMap::new(),
            next_token: 1,
            faulted: false,
        }
    }

    fn session(&mut self, id: u32) -> Result<&mut Session, Error> {
        self.engine.session(SessionId(id)).ok_or_else(|| Error::new(ErrorCode::Destroyed, format!("session {id} is closed")))
    }

    /// Delivers the queued messages, with a frame request for each changed session that has a surface, and a
    /// background layout timer for each changed session (or `session`, the call's) whose pages aren't complete.
    fn flush(&mut self, session: u32) {
        let messages: Vec<Message> = self.outbox.borrow_mut().drain(..).collect();
        let changed: BTreeSet<u32> = messages.iter().filter(|m| m.kind == kind::STATE).map(|m| m.session).collect();
        for m in &messages {
            self.emit(m.session, m.kind, &m.payload);
        }
        for s in changed.intersection(&self.surfaces) {
            self.emit(*s, kind::REQUEST_FRAME, &[]);
        }
        for s in changed.into_iter().chain([session]) {
            let pending = self.engine.session(SessionId(s)).is_some_and(|x| !x.state().pages.complete);
            if pending && self.armed.insert(s) {
                self.emit(s, kind::SET_TIMER, format!(r#"{{"timer":{LAYOUT_TIMER},"ms":0}}"#).as_bytes());
            }
        }
    }

    fn emit(&self, session: u32, kind: u32, payload: &[u8]) {
        #[cfg(target_os = "emscripten")]
        {
            let _ = (self.callback, self.context);
            // SAFETY: twip-host.js reads the bytes during the call and keeps nothing.
            unsafe { twip_emit(kind, session, payload.as_ptr(), payload.len()) };
        }
        #[cfg(not(target_os = "emscripten"))]
        if let Some(callback) = self.callback {
            // SAFETY: the SDK registered this callback with its context, and copies the payload during the call.
            unsafe { callback(self.context, session, kind, payload.as_ptr(), payload.len()) };
        }
    }
}

#[cfg(target_os = "emscripten")]
unsafe extern "C" {
    /// `twip-host.js`: hands a callback to `Module.twipDispatch`.
    fn twip_emit(kind: u32, session: u32, ptr: *const u8, len: usize);
}

/// Runs `f` on the engine behind `engine` under `catch_unwind`, then delivers the messages it queued. A panic marks the
/// engine faulted and emits a fatal `error` event for `session`.
fn guard<T>(engine: *mut TwipEngine, session: u32, f: impl FnOnce(&mut TwipEngine) -> Result<T, Error>) -> Result<T, Error> {
    // SAFETY: the SDK passes a pointer twip_engine_new returned and twip_engine_free hasn't freed, on the engine
    // thread, and no other reference to it is live.
    let Some(e) = (unsafe { engine.as_mut() }) else { return Err(Error::invalid("no engine")) };
    if e.faulted {
        return Err(Error::new(ErrorCode::Internal, "the engine failed earlier"));
    }
    let result = catch_unwind(AssertUnwindSafe(|| f(e)));
    match result {
        Ok(r) => {
            e.flush(session);
            r
        }
        Err(panic) => {
            let what = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned());
            let error = Error::new(ErrorCode::Internal, format!("the engine failed: {}", what.unwrap_or_default()));
            e.faulted = true;
            e.outbox.borrow_mut().clear();
            if let Ok(payload) = serde_json::to_vec(&Event::fault(error.clone())) {
                e.emit(session, kind::EVENT, &payload);
            }
            Err(error)
        }
    }
}

/// A byte slice from the caller, empty for a null pointer.
///
/// # Safety
/// `ptr` is null, or valid for `len` bytes for the call.
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        return &[];
    }
    // SAFETY: the caller's promise.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

/// A JSON request: `null` for none.
fn request<T: serde::de::DeserializeOwned>(json: &[u8]) -> Result<T, Error> {
    let json = if json.is_empty() { b"null".as_slice() } else { json };
    serde_json::from_slice(json).map_err(|e| Error::invalid(format!("bad request: {e}")))
}

/// Hands `data` to the caller in `out` (nothing for a null `out`).
///
/// # Safety
/// `out` is null or valid for a write.
unsafe fn put(out: *mut TwipBuf, data: Vec<u8>) {
    if out.is_null() {
        return;
    }
    let boxed = data.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed).cast::<u8>();
    // SAFETY: the caller's promise.
    unsafe { out.write(TwipBuf { ptr, len }) };
}

/// Writes the response envelope to `out` and returns the status.
///
/// # Safety
/// `out` is null or valid for a write.
unsafe fn respond(out: *mut TwipBuf, result: Result<Value, Error>) -> i32 {
    let (code, envelope) = match result {
        Ok(v) => (0, json!({ "ok": v })),
        Err(e) => (status(e.code), json!({ "error": e })),
    };
    // SAFETY: the caller's promise.
    unsafe { put(out, serde_json::to_vec(&envelope).unwrap_or_default()) };
    code
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value, Error> {
    serde_json::to_value(v).map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))
}

// ---------------- handles and buffers ----------------

/// A new engine. `options_json`: `{ "fonts": "system" | "empty" }` (or empty). `cb` is called with `ctx` for every
/// callback; null in the browser. Null on bad options.
///
/// # Safety
/// `options_json` is null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_engine_new(options_json: *const u8, len: usize, cb: TwipCallback, ctx: *mut c_void) -> *mut TwipEngine {
    // SAFETY: the caller's promise.
    let json = unsafe { bytes(options_json, len) };
    let options = if json.is_empty() { Ok(EngineOptions::default()) } else { serde_json::from_slice(json) };
    match options.ok().and_then(|o| catch_unwind(AssertUnwindSafe(|| TwipEngine::new(o, cb, ctx))).ok()) {
        Some(e) => Box::into_raw(Box::new(e)),
        None => std::ptr::null_mut(),
    }
}

/// Frees an engine and its sessions.
///
/// # Safety
/// `engine` is null or from twip_engine_new, and not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_engine_free(engine: *mut TwipEngine) {
    if !engine.is_null() {
        // SAFETY: the caller's promise; dropping the sessions is not the engine's work, so a panic there is dropped.
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(engine) })));
    }
}

/// A new session with a blank document; `options_json` is an `api::SessionOptions` (or empty). 0 on failure.
///
/// # Safety
/// `engine` is from twip_engine_new; `options_json` is null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_session_new(engine: *mut TwipEngine, options_json: *const u8, len: usize) -> u32 {
    // SAFETY: the caller's promise.
    let json = unsafe { bytes(options_json, len) };
    guard(engine, 0, |e| {
        let options: SessionOptions = if json.is_empty() { SessionOptions::default() } else { request(json)? };
        Ok(e.engine.create_session(options).0)
    })
    .unwrap_or(0)
}

/// Ends a session; later calls with its id fail `destroyed`. It works after a fault too.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_session_free(engine: *mut TwipEngine, session: u32) {
    // SAFETY: as in `guard`.
    let Some(e) = (unsafe { engine.as_mut() }) else { return };
    e.surfaces.remove(&session);
    e.armed.remove(&session);
    e.prepared.retain(|_, (owner, _)| *owner != session);
    // Dropping a session is not the engine's work, so a panic there is dropped.
    let _ = catch_unwind(AssertUnwindSafe(|| e.engine.close_session(SessionId(session))));
}

/// Frees what a call wrote to `buf`, and empties it.
///
/// # Safety
/// `buf` is null, or holds what the engine wrote (or zeros).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_buf_free(buf: *mut TwipBuf) {
    // SAFETY: the caller's promise.
    let Some(b) = (unsafe { buf.as_mut() }) else { return };
    if !b.ptr.is_null() {
        // SAFETY: `put` made it from a boxed slice of this length.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(b.ptr, b.len)) });
    }
    *b = TwipBuf { ptr: std::ptr::null_mut(), len: 0 };
}

/// `len` bytes for the SDK to pass data in (at least one), or null when they can't be had. Free with `twip_free`
/// and the same length.
#[unsafe(no_mangle)]
pub extern "C" fn twip_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::new();
    if v.try_reserve_exact(len.max(1)).is_err() {
        return std::ptr::null_mut();
    }
    v.resize(len.max(1), 0u8);
    Box::into_raw(v.into_boxed_slice()).cast::<u8>()
}

/// Frees what `twip_alloc(len)` returned.
///
/// # Safety
/// `ptr` is null or from `twip_alloc(len)`, and not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        // SAFETY: the caller's promise: twip_alloc boxed this many bytes (at least one).
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len.max(1))) });
    }
}

// ---------------- structured calls ----------------

/// A structured call: `method` (a [`Method`] number) with its JSON request; the response envelope goes to `out`.
///
/// # Safety
/// `engine` is from twip_engine_new; `req` is null or valid for `len` bytes; `out` is null or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_call(engine: *mut TwipEngine, session: u32, method: u32, req: *const u8, len: usize, out: *mut TwipBuf) -> i32 {
    // SAFETY: the caller's promise.
    let json = unsafe { bytes(req, len) };
    let result = guard(engine, session, |e| {
        let method = Method::from_u32(method).ok_or_else(|| Error::invalid(format!("no method {method}")))?;
        methods::call(e, session, method, json)
    });
    // SAFETY: the caller's promise.
    unsafe { respond(out, result) }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct OpenOptions {}

/// Opens a document from `bytes` (`opts`: `{}`); `out` gets `{"ok": warnings}`.
///
/// # Safety
/// `engine` is from twip_engine_new; `bytes` and `opts` are null or valid for their lengths; `out` is null or valid
/// for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_open(
    engine: *mut TwipEngine,
    session: u32,
    bytes_ptr: *const u8,
    len: usize,
    opts: *const u8,
    olen: usize,
    out: *mut TwipBuf,
) -> i32 {
    // SAFETY: the caller's promise.
    let (data, opts) = unsafe { (bytes(bytes_ptr, len), bytes(opts, olen)) };
    let result = guard(engine, session, |e| {
        let _: OpenOptions = if opts.is_empty() { OpenOptions::default() } else { request(opts)? };
        to_value(e.session(session)?.open(data)?)
    });
    // SAFETY: the caller's promise.
    unsafe { respond(out, result) }
}

/// Reads a document from `bytes` (`opts`: `{}`) without replacing the open one; `out` gets `{"ok": token}`. Then
/// `OpenCommit` with the token swaps it in, or `OpenDiscard` drops it. Freeing the session drops its tokens.
///
/// # Safety
/// As for twip_open.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_open_prepare(
    engine: *mut TwipEngine,
    session: u32,
    bytes_ptr: *const u8,
    len: usize,
    opts: *const u8,
    olen: usize,
    out: *mut TwipBuf,
) -> i32 {
    // SAFETY: the caller's promise.
    let (data, opts) = unsafe { (bytes(bytes_ptr, len), bytes(opts, olen)) };
    let result = guard(engine, session, |e| {
        let _: OpenOptions = if opts.is_empty() { OpenOptions::default() } else { request(opts)? };
        let prepared = e.session(session)?.open_prepare(data)?;
        let token = e.next_token;
        e.next_token += 1;
        e.prepared.insert(token, (session, prepared));
        Ok(json!(token))
    });
    // SAFETY: the caller's promise.
    unsafe { respond(out, result) }
}

/// Exports the document (`opts`: an `api::ExportOptions`). The file goes to `bytes`, raw; `meta` gets
/// `{"ok": {revision, warnings}}` or the error.
///
/// # Safety
/// `engine` is from twip_engine_new; `opts` is null or valid for `olen` bytes; `bytes` and `meta` are null or valid
/// for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_export(
    engine: *mut TwipEngine,
    session: u32,
    opts: *const u8,
    olen: usize,
    bytes_out: *mut TwipBuf,
    meta: *mut TwipBuf,
) -> i32 {
    // SAFETY: the caller's promise.
    let opts = unsafe { bytes(opts, olen) };
    let result = guard(engine, session, |e| {
        let options: ExportOptions = if opts.is_empty() { ExportOptions::default() } else { request(opts)? };
        e.session(session)?.export(&options)
    });
    let result = result.map(|export| {
        // SAFETY: the caller's promise.
        unsafe { put(bytes_out, export.bytes.0) };
        json!({ "revision": export.revision, "warnings": export.warnings })
    });
    // SAFETY: the caller's promise.
    unsafe { respond(meta, result) }
}

/// Adds a font file to the engine's library; `out` gets `{"ok": [FaceInfo]}`.
///
/// # Safety
/// `engine` is from twip_engine_new; `bytes` is null or valid for `len` bytes; `out` is null or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_font_add(engine: *mut TwipEngine, bytes_ptr: *const u8, len: usize, out: *mut TwipBuf) -> i32 {
    // SAFETY: the caller's promise.
    let data = unsafe { bytes(bytes_ptr, len) };
    let result = guard(engine, 0, |e| {
        let faces = to_value(e.engine.fonts().add(data)?)?;
        // Every session lays out with the library: what shows may have changed.
        e.engine.sync_fonts();
        for s in &e.surfaces {
            e.emit(*s, kind::REQUEST_FRAME, &[]);
        }
        Ok(faces)
    });
    // SAFETY: the caller's promise.
    unsafe { respond(out, result) }
}

// ---------------- hot input ----------------

/// A key press: `key` from the key table (`KeyboardEvent.code`), `mods` Ctrl 1, Shift 2, Alt 4, Meta 8. Returns 0
/// unhandled (the platform keeps it), 1 handled, or 2 + n for host command n (`HostCommand` order: save, saveAs,
/// print, open, new, find, replace, goTo, insertHyperlink, nextMisspelling, focusFont, focusFontSize, help).
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_key(engine: *mut TwipEngine, session: u32, key: u32, mods: u32) -> u32 {
    guard(engine, session, |e| {
        let Some(key) = Key::from_u32(key) else { return Ok(0) };
        Ok(match e.session(session)?.key(key, Modifiers::from_bits(mods)) {
            KeyOutcome::Unhandled => 0,
            KeyOutcome::Handled | KeyOutcome::Shortcut(_) => 1,
            KeyOutcome::HostCommand(h) => 2 + HostCommand::ALL.iter().position(|c| *c == h).unwrap_or(0) as u32,
        })
    })
    .unwrap_or(0)
}

/// Typed text, UTF-8.
///
/// # Safety
/// `engine` is from twip_engine_new; `utf8` is null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_text(engine: *mut TwipEngine, session: u32, utf8: *const u8, len: usize) {
    // SAFETY: the caller's promise.
    let text = String::from_utf8_lossy(unsafe { bytes(utf8, len) }).into_owned();
    let _ = guard(engine, session, |e| {
        e.session(session)?.text(&text);
        Ok(())
    });
}

/// An IME composition step: `phase` start 0, update 1, end 2; the composition so far, UTF-8.
///
/// # Safety
/// `engine` is from twip_engine_new; `utf8` is null or valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_composition(engine: *mut TwipEngine, session: u32, phase: u32, utf8: *const u8, len: usize) {
    // SAFETY: the caller's promise.
    let text = String::from_utf8_lossy(unsafe { bytes(utf8, len) }).into_owned();
    let phase = match phase {
        0 => CompositionPhase::Start,
        1 => CompositionPhase::Update,
        _ => CompositionPhase::End,
    };
    let _ = guard(engine, session, |e| {
        let cursor = text.chars().count() as u32;
        e.session(session)?.composition(phase, &text, cursor);
        Ok(())
    });
}

/// Pointer input: `kind` down 0, move 1, up 2, cancel 3, leave 4; a viewport point in CSS px; the button pressed or
/// released (primary 0, secondary 1, middle 2); the click count; modifiers.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_pointer(engine: *mut TwipEngine, session: u32, kind: u32, x: f64, y: f64, button: u32, clicks: u32, mods: u32) {
    let kind = match kind {
        0 => PointerKind::Down,
        1 => PointerKind::Move,
        2 => PointerKind::Up,
        4 => PointerKind::Leave,
        _ => PointerKind::Cancel,
    };
    let button = match button {
        1 => PointerButton::Secondary,
        2 => PointerButton::Middle,
        _ => PointerButton::Primary,
    };
    let _ = guard(engine, session, |e| {
        e.session(session)?.pointer(kind, Point { x, y }, button, clicks.min(255) as u8, Modifiers::from_bits(mods));
        Ok(())
    });
}

/// A wheel or trackpad scroll, CSS px.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_wheel(engine: *mut TwipEngine, session: u32, dx: f64, dy: f64, mods: u32) {
    let _ = guard(engine, session, |e| {
        e.session(session)?.wheel(dx, dy, Modifiers::from_bits(mods));
        Ok(())
    });
}

/// Whether the host's view has the focus (0 or 1).
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_focus(engine: *mut TwipEngine, session: u32, focused: u32) {
    let _ = guard(engine, session, |e| {
        e.session(session)?.host().set_focus(focused != 0);
        Ok(())
    });
}

// ---------------- presentation ----------------

/// The viewport's size in CSS px and its device pixel ratio. A session with a surface gets frame requests.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_surface(engine: *mut TwipEngine, session: u32, width: f64, height: f64, dpr: f64) {
    let _ = guard(engine, session, |e| {
        e.session(session)?.host().set_size(width, height, dpr);
        let shown = width > 0.0 && height > 0.0;
        if shown && e.surfaces.insert(session) {
            let queue = e.outbox.clone();
            e.session(session)?.host().attach(move |r| {
                if let Ok(payload) = serde_json::to_vec(&r) {
                    queue.borrow_mut().push_back(Message { session, kind: kind::HOST_REQUEST, payload });
                }
            });
        } else if !shown && e.surfaces.remove(&session) {
            e.session(session)?.host().detach();
        }
        if shown {
            e.emit(session, kind::REQUEST_FRAME, &[]);
        }
        Ok(())
    });
}

/// The scroll position: the viewport's top-left in content CSS px.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_scroll(engine: *mut TwipEngine, session: u32, x: f64, y: f64) {
    let _ = guard(engine, session, |e| {
        e.session(session)?.host().set_scroll(x, y);
        if e.surfaces.contains(&session) {
            e.emit(session, kind::REQUEST_FRAME, &[]);
        }
        Ok(())
    });
}

/// Writes `info` when it isn't null.
///
/// # Safety
/// `info` is null or valid for a write.
unsafe fn put_info(info: *mut TwipFrameInfo, frame: FrameInfo) {
    if !info.is_null() {
        // SAFETY: the caller's promise.
        unsafe { info.write(frame.into()) };
    }
}

/// Draws the viewport as CPU pixels: `width` x `height` device pixels, RGBA, premultiplied, rows packed.
///
/// # Safety
/// `engine` is from twip_engine_new; `pixels` is valid for `width * height * 4` bytes; `info` is null or valid for
/// a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_frame_rgba(
    engine: *mut TwipEngine,
    session: u32,
    pixels: *mut u8,
    width: u32,
    height: u32,
    info: *mut TwipFrameInfo,
) -> i32 {
    if pixels.is_null() {
        return status(ErrorCode::InvalidArgument);
    }
    let len = width as usize * height as usize * 4;
    // SAFETY: the caller's promise.
    let pixels = unsafe { std::slice::from_raw_parts_mut(pixels, len) };
    match guard(engine, session, |e| e.session(session)?.host().frame_rgba(pixels, width, height)) {
        Ok(frame) => {
            // SAFETY: the caller's promise.
            unsafe { put_info(info, frame) };
            0
        }
        Err(e) => status(e.code),
    }
}

/// Browser: a WebGPU instance handle for `Module.WebGPU.importJsDevice` and `importJsQueue`. 0 natively.
#[unsafe(no_mangle)]
pub extern "C" fn twip_gpu_instance() -> usize {
    #[cfg(target_os = "emscripten")]
    return catch_unwind(engine::internal::render::web::webgpu_instance).unwrap_or(0);
    #[cfg(not(target_os = "emscripten"))]
    0
}

/// Browser: adopts the page's WebGPU device for every session (imported device and queue handles, and the instance
/// from `twip_gpu_instance`). Fails `unavailable` when it can't be used: draw with WebGL then.
///
/// # Safety
/// The handles are live, just imported by the SDK.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_attach_webgpu(instance: usize, device: usize, queue: usize) -> i32 {
    #[cfg(target_os = "emscripten")]
    {
        // SAFETY: the caller's promise.
        let ok = catch_unwind(|| unsafe { engine::internal::render::web::attach_webgpu(instance, device, queue) }).unwrap_or(false);
        if ok { 0 } else { status(ErrorCode::Unavailable) }
    }
    #[cfg(not(target_os = "emscripten"))]
    {
        let _ = (instance, device, queue);
        status(ErrorCode::Unavailable)
    }
}

/// Browser: draws the viewport into a WebGPU canvas texture (an emdawnwebgpu handle from `importJsTexture`,
/// released here); `height` is the texture's height in pixels, `bgra` 1 for a BGRA texture.
///
/// # Safety
/// `engine` is from twip_engine_new; `texture` is a live texture handle not used again; `info` is null or valid for
/// a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_frame_webgpu(
    engine: *mut TwipEngine,
    session: u32,
    texture: usize,
    bgra: u32,
    height: u32,
    info: *mut TwipFrameInfo,
) -> i32 {
    #[cfg(target_os = "emscripten")]
    {
        let result = guard(engine, session, |e| {
            let mut host = e.session(session)?.host();
            // SAFETY: the caller's promise: the texture was imported for this one call.
            let drawn = unsafe {
                engine::internal::render::web::webgpu_frame(texture, bgra != 0, |c| {
                    let width = f64::from(c.base_layer_size().width);
                    host.draw(c, width, f64::from(height));
                })
            };
            if drawn { Ok(host.frame_info()) } else { Err(Error::unavailable("no WebGPU device is attached")) }
        });
        match result {
            Ok(frame) => {
                // SAFETY: the caller's promise.
                unsafe { put_info(info, frame) };
                0
            }
            Err(e) => status(e.code),
        }
    }
    #[cfg(not(target_os = "emscripten"))]
    {
        let _ = (engine, session, texture, bgra, height, info);
        status(ErrorCode::Unavailable)
    }
}

/// Browser: adopts the current WebGL 2 context (made current with `Module.GL.makeContextCurrent`) for the session's
/// canvas.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_attach_webgl(engine: *mut TwipEngine, session: u32) -> i32 {
    #[cfg(target_os = "emscripten")]
    {
        match guard(engine, session, |e| Ok(e.session(session)?.host().attach_webgl())) {
            Ok(true) => 0,
            Ok(false) => status(ErrorCode::Unavailable),
            Err(e) => status(e.code),
        }
    }
    #[cfg(not(target_os = "emscripten"))]
    {
        let _ = (engine, session);
        status(ErrorCode::Unavailable)
    }
}

/// Browser: draws the viewport into the session's WebGL canvas, `width` x `height` device pixels.
///
/// # Safety
/// `engine` is from twip_engine_new; `info` is null or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_frame_webgl(engine: *mut TwipEngine, session: u32, width: u32, height: u32, info: *mut TwipFrameInfo) -> i32 {
    #[cfg(target_os = "emscripten")]
    {
        match guard(engine, session, |e| e.session(session)?.host().frame_webgl(width, height)) {
            Ok(frame) => {
                // SAFETY: the caller's promise.
                unsafe { put_info(info, frame) };
                0
            }
            Err(e) => status(e.code),
        }
    }
    #[cfg(not(target_os = "emscripten"))]
    {
        let _ = (engine, session, width, height, info);
        status(ErrorCode::Unavailable)
    }
}

/// Windows: draws the session's frames into the window `hwnd` with the GPU (Graphite on Dawn). Returns the backend,
/// 1 D3D12, 2 D3D11 or 3 Vulkan; fails `unavailable` with no usable GPU, or in a build without one: draw with
/// twip_frame_rgba then.
///
/// # Safety
/// `engine` is from twip_engine_new; `hwnd` is a live window that outlives the attach (twip_detach_window, or the
/// session's end).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_attach_window(engine: *mut TwipEngine, session: u32, hwnd: *mut std::ffi::c_void) -> i32 {
    // SAFETY: the caller's promise.
    match guard(engine, session, |e| Ok(unsafe { e.session(session)?.host().attach_window(hwnd) })) {
        Ok(Some(backend)) => backend as i32,
        Ok(None) => status(ErrorCode::Unavailable),
        Err(e) => status(e.code),
    }
}

/// Windows: stops drawing into the window twip_attach_window gave.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_detach_window(engine: *mut TwipEngine, session: u32) {
    let _ = guard(engine, session, |e| {
        e.session(session)?.host().detach_window();
        Ok(())
    });
}

/// Windows: draws the viewport into the attached window, `width` x `height` device pixels, and presents it. Fails
/// `unavailable` when the GPU can't draw now (the device was lost): draw with twip_frame_rgba then.
///
/// # Safety
/// `engine` is from twip_engine_new; `info` is null or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_frame_window(engine: *mut TwipEngine, session: u32, width: u32, height: u32, info: *mut TwipFrameInfo) -> i32 {
    match guard(engine, session, |e| e.session(session)?.host().frame_window(width, height)) {
        Ok(frame) => {
            // SAFETY: the caller's promise.
            unsafe { put_info(info, frame) };
            0
        }
        Err(e) => status(e.code),
    }
}

/// A timer the engine armed (callback kind 4) went off.
///
/// # Safety
/// `engine` is from twip_engine_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_timer_fired(engine: *mut TwipEngine, session: u32, timer: u32) {
    let _ = guard(engine, session, |e| {
        if timer == LAYOUT_TIMER {
            e.armed.remove(&session);
        }
        e.session(session)?.host().timer_fired(timer);
        Ok(())
    });
}

/// Test builds: a structured call that panics inside the engine, for the fault path.
#[cfg(test)]
unsafe extern "C" fn twip_test_panic(engine: *mut TwipEngine, session: u32) -> i32 {
    match guard(engine, session, |_| -> Result<(), Error> { panic!("injected") }) {
        Ok(()) => 0,
        Err(e) => status(e.code),
    }
}

#[cfg(test)]
#[path = "../tests/abi.rs"]
mod abi;
