// The C ABI driven through its `extern "C"` functions with a recording callback, as an SDK drives it. Built into the
// library's unit tests (lib.rs includes it), so the fault test can reach the test-only panicking entry point.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ffi::c_void;

use serde_json::{Value, json};

use super::*;

type Log = RefCell<Vec<(u32, u32, Value)>>;

unsafe extern "C" fn record(ctx: *mut c_void, session: u32, kind: u32, ptr: *const u8, len: usize) {
    // SAFETY: `ctx` is the `Log` the test registered, alive for the engine's life; `ptr` is valid for `len` bytes now.
    let (log, data) = unsafe { (&*(ctx as *const Log), bytes(ptr, len)) };
    let payload = if data.is_empty() { Value::Null } else { serde_json::from_slice(data).expect("callback payloads are JSON") };
    log.borrow_mut().push((session, kind, payload));
}

struct Fixture {
    engine: *mut TwipEngine,
    log: Box<Log>,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture::with(br#"{"fonts":"empty"}"#)
    }

    fn with(options: &[u8]) -> Fixture {
        let log: Box<Log> = Box::default();
        let ctx = &*log as *const Log as *mut c_void;
        // SAFETY: the options are valid; the log outlives the engine (dropped after it in `Drop`).
        let engine = unsafe { twip_engine_new(options.as_ptr(), options.len(), Some(record), ctx) };
        assert!(!engine.is_null());
        Fixture { engine, log }
    }

    fn session(&self) -> u32 {
        // SAFETY: a live engine; no options.
        unsafe { twip_session_new(self.engine, std::ptr::null(), 0) }
    }

    fn call(&self, session: u32, method: Method, req: Value) -> (i32, Value) {
        let req = req.to_string();
        let mut out = TwipBuf { ptr: std::ptr::null_mut(), len: 0 };
        // SAFETY: a live engine, a request valid for its length, and an output buffer.
        let status = unsafe { twip_call(self.engine, session, method as u32, req.as_ptr(), req.len(), &mut out) };
        (status, take(&mut out))
    }

    #[allow(clippy::mut_from_ref, reason = "tests look into the engine between calls, when no call is running")]
    fn engine_ref(&self) -> &mut TwipEngine {
        // SAFETY: a live engine, and no call is running.
        unsafe { &mut *self.engine }
    }

    fn take_log(&self) -> Vec<(u32, u32, Value)> {
        std::mem::take(&mut *self.log.borrow_mut())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: the engine came from twip_engine_new and isn't used again.
        unsafe { twip_engine_free(self.engine) };
    }
}

/// The JSON in a buffer the engine wrote, freed.
fn take(buf: &mut TwipBuf) -> Value {
    // SAFETY: the engine wrote the buffer.
    let data = unsafe { bytes(buf.ptr, buf.len) }.to_vec();
    // SAFETY: the engine wrote the buffer, and it isn't read again.
    unsafe { twip_buf_free(buf) };
    assert!(buf.ptr.is_null() && buf.len == 0);
    serde_json::from_slice(&data).unwrap_or(Value::Null)
}

#[test]
fn calls_answer_in_envelopes_and_state_comes_before_events() {
    let f = Fixture::new();
    let s = f.session();
    assert_ne!(s, 0);
    let (status, response) = f.call(s, Method::Run, json!({ "type": "insertText", "value": "Hi" }));
    assert_eq!(status, 0, "{response}");
    assert_eq!(response["ok"]["record"]["origin"], "api");
    let log = f.take_log();
    let kinds: Vec<u32> = log.iter().map(|(_, k, _)| *k).collect();
    assert_eq!(kinds, [kind::STATE, kind::EVENT], "{log:?}");
    assert_eq!(log[0].2["revision"], log[1].2["revision"]);
    assert_eq!(log[1].2["type"], "documentChanged");
    assert!(log.iter().all(|(session, ..)| *session == s));
    let (status, state) = f.call(s, Method::State, Value::Null);
    assert_eq!((status, state["ok"]["revision"].as_u64()), (0, Some(2)));
    let (status, catalog) = f.call(0, Method::Catalog, Value::Null);
    assert_eq!(status, 0);
    assert!(catalog["ok"].as_array().is_some_and(|c| c.iter().any(|i| i["id"] == "setBold")));
}

#[test]
fn errors_have_their_shape() {
    let f = Fixture::new();
    let s = f.session();
    let (status, response) = f.call(s, Method::Run, json!({ "type": "undo" }));
    assert_eq!(status, status_of(ErrorCode::Unavailable));
    assert_eq!(response["error"]["code"], "unavailable");
    assert!(response["error"]["message"].is_string());
    let (status, response) = f.call(s, Method::Run, json!({ "type": "setBold", "value": "yes" }));
    assert_eq!((status, &response["error"]["code"]), (status_of(ErrorCode::InvalidArgument), &json!("invalid-argument")));
    let (status, _) = f.call(s, Method::SelectHeading, json!("h1"));
    assert_eq!(status, status_of(ErrorCode::InvalidArgument), "not a heading id");
    let mut out = TwipBuf { ptr: std::ptr::null_mut(), len: 0 };
    // SAFETY: a live engine and an output buffer; 9999 is no method.
    let status = unsafe { twip_call(f.engine, s, 9999, std::ptr::null(), 0, &mut out) };
    assert_eq!((status, take(&mut out)["error"]["code"].clone()), (status_of(ErrorCode::InvalidArgument), json!("invalid-argument")));
    // SAFETY: a live engine.
    unsafe { twip_session_free(f.engine, s) };
    let (status, response) = f.call(s, Method::State, Value::Null);
    assert_eq!((status, &response["error"]["code"]), (-1, &json!("destroyed")));
}

fn status_of(code: ErrorCode) -> i32 {
    status(code)
}

#[test]
fn keys_report_what_they_did() {
    let f = Fixture::new();
    let s = f.session();
    let ctrl = 1;
    // SAFETY: a live engine and session, for each call.
    unsafe {
        assert_eq!(twip_key(f.engine, s, Key::KeyS as u32, ctrl), 2, "Ctrl+S is the host's save");
        assert_eq!(twip_key(f.engine, s, Key::F1 as u32, 0), 2 + 12, "F1 is help");
        assert_eq!(twip_key(f.engine, s, Key::KeyB as u32, ctrl), 1, "Ctrl+B is the editor's");
        assert_eq!(twip_key(f.engine, s, 9999, 0), 0);
    }
    let log = f.take_log();
    assert!(log.iter().any(|(_, k, e)| *k == kind::EVENT && e["type"] == "hostCommand" && e["command"] == "save"), "{log:?}");
    let text = "typed";
    // SAFETY: a live engine and session; the text is valid for its length.
    unsafe { twip_text(f.engine, s, text.as_ptr(), text.len()) };
    let log = f.take_log();
    assert!(log.iter().any(|(_, _, e)| e["type"] == "documentChanged" && e["origin"] == "user"), "{log:?}");
}

#[test]
fn documents_open_and_export_as_bytes() {
    let f = Fixture::new();
    let s = f.session();
    f.call(s, Method::Run, json!({ "type": "insertText", "value": "Round trip" }));
    let opts = br#"{"format":"docx"}"#;
    let (mut file, mut meta) = (TwipBuf { ptr: std::ptr::null_mut(), len: 0 }, TwipBuf { ptr: std::ptr::null_mut(), len: 0 });
    // SAFETY: a live engine and session, options valid for their length, two output buffers.
    let status = unsafe { twip_export(f.engine, s, opts.as_ptr(), opts.len(), &mut file, &mut meta) };
    assert_eq!(status, 0);
    // SAFETY: the engine wrote the file buffer.
    let docx = unsafe { bytes(file.ptr, file.len) }.to_vec();
    // SAFETY: the engine wrote the buffer, and it isn't read again.
    unsafe { twip_buf_free(&mut file) };
    assert!(docx.starts_with(b"PK"));
    assert_eq!(take(&mut meta)["ok"]["revision"], 2);
    let mut out = TwipBuf { ptr: std::ptr::null_mut(), len: 0 };
    // SAFETY: a live engine and session, the document valid for its length, no options, an output buffer.
    let status = unsafe { twip_open(f.engine, s, docx.as_ptr(), docx.len(), std::ptr::null(), 0, &mut out) };
    let warnings = take(&mut out)["ok"].clone();
    // The fixture's library may lack the document's font; nothing else may warn.
    assert_eq!(status, 0);
    assert!(warnings.as_array().unwrap().iter().all(|w| w["code"] == "font-unresolved"), "{warnings}");
    let junk = b"not a document";
    // SAFETY: as above.
    let status = unsafe { twip_open(f.engine, s, junk.as_ptr(), junk.len(), std::ptr::null(), 0, &mut out) };
    assert_eq!((status, take(&mut out)["error"]["code"].clone()), (status_of(ErrorCode::ImportFailed), json!("import-failed")));
    // SAFETY: a live engine, bytes valid for their length, an output buffer.
    let status = unsafe { twip_font_add(f.engine, junk.as_ptr(), junk.len(), &mut out) };
    assert_eq!((status, take(&mut out)["error"]["code"].clone()), (status_of(ErrorCode::InvalidArgument), json!("invalid-argument")));
}

#[test]
fn frames_draw_into_the_callers_pixels() {
    let f = Fixture::new();
    let s = f.session();
    // SAFETY: a live engine and session.
    unsafe { twip_surface(f.engine, s, 200.0, 100.0, 1.0) };
    assert!(f.take_log().iter().any(|(_, k, _)| *k == kind::REQUEST_FRAME));
    let (w, h) = (200u32, 100u32);
    let pixels = twip_alloc((w * h * 4) as usize);
    let mut info = TwipFrameInfo::default();
    // SAFETY: a live engine and session, pixels for w x h, and an info to write.
    let status = unsafe { twip_frame_rgba(f.engine, s, pixels, w, h, &mut info) };
    assert_eq!(status, 0);
    assert!(info.content_width > 0.0 && info.pages >= 1, "{info:?}");
    // SAFETY: twip_alloc made it with this length.
    unsafe { twip_free(pixels, (w * h * 4) as usize) };
    f.call(s, Method::Run, json!({ "type": "insertText", "value": "x" }));
    assert_eq!(f.take_log().last().map(|(_, k, _)| *k), Some(kind::REQUEST_FRAME), "a change asks for a frame");
}

#[test]
fn a_panic_faults_the_engine() {
    let f = Fixture::new();
    let s = f.session();
    // SAFETY: a live engine and session.
    let status = unsafe { twip_test_panic(f.engine, s) };
    assert_eq!(status, status_of(ErrorCode::Internal));
    let log = f.take_log();
    let faults: Vec<&Value> = log.iter().filter(|(_, k, e)| *k == kind::EVENT && e["type"] == "error").map(|(_, _, e)| e).collect();
    assert_eq!(faults.len(), 1, "{log:?}");
    assert_eq!((faults[0]["code"].clone(), faults[0]["fatal"].clone()), (json!("internal"), json!(true)));
    let (status, response) = f.call(s, Method::State, Value::Null);
    assert_eq!((status, &response["error"]["code"]), (status_of(ErrorCode::Internal), &json!("internal")));
    // SAFETY: a live engine.
    assert_eq!(unsafe { twip_session_new(f.engine, std::ptr::null(), 0) }, 0);
}

#[test]
fn the_header_declares_the_abi() {
    let header = include_str!("../include/twip.h");
    let lib = include_str!("../src/lib.rs");
    let name = |line: &str| line.split("twip_").nth(1).and_then(|rest| rest.split('(').next()).map(|n| format!("twip_{n}"));
    let exported: BTreeSet<String> = lib.lines().filter(|l| l.starts_with("pub ") && l.contains("extern \"C\" fn twip_")).filter_map(name).collect();
    let declared: BTreeSet<String> = header.lines().filter(|l| !l.starts_with("typedef") && l.trim_end().ends_with(");")).filter_map(name).collect();
    assert_eq!(exported, declared);
    for m in Method::ALL {
        let mut screaming = String::new();
        for (i, c) in format!("{m:?}").chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                screaming.push('_');
            }
            screaming.push(c.to_ascii_uppercase());
        }
        assert!(
            header.contains(&format!("TWIP_{screaming} = {},", *m as u32)) || header.contains(&format!("TWIP_{screaming} = {} ", *m as u32)),
            "{m:?}"
        );
    }
}

#[test]
fn method_numbers_are_unique_and_stable() {
    let numbers: Vec<u32> = Method::ALL.iter().map(|m| *m as u32).collect();
    assert_eq!(numbers, (1..=Method::ALL.len() as u32).collect::<Vec<_>>());
    assert_eq!((Method::Run as u32, Method::Batch as u32, Method::State as u32, Method::Catalog as u32, Method::NewDocument as u32), (1, 2, 3, 4, 5));
}

#[test]
fn every_command_reaches_the_session() {
    let commands: Vec<Value> = serde_json::from_str(include_str!("../../twip/tests/wire/command.json")).expect("the fixture is JSON");
    let f = Fixture::new();
    let s = f.session();
    for command in &commands {
        let (status, response) = f.call(s, Method::Run, command.clone());
        let message = response["error"]["message"].as_str().unwrap_or_default();
        assert!(!message.starts_with("bad request") && status != status_of(ErrorCode::Internal), "{command}: {response}");
        let log = f.take_log();
        let state = log.iter().position(|(_, k, _)| *k == kind::STATE);
        let event = log.iter().position(|(_, k, _)| *k == kind::EVENT);
        assert!(event.is_none_or(|e| state.is_some_and(|s| s < e)), "the state comes first: {command}: {log:?}");
    }
    let (status, response) = f.call(s, Method::Batch, json!({ "commands": commands, "label": "All" }));
    assert!(!response["error"]["message"].as_str().unwrap_or_default().starts_with("bad request"), "{status}: {response}");
}

/// The host requests in a log.
fn requests(log: Vec<(u32, u32, Value)>) -> Vec<Value> {
    log.into_iter().filter(|(_, k, _)| *k == kind::HOST_REQUEST).map(|(_, _, r)| r).collect()
}

#[test]
fn a_shown_session_gets_host_requests() {
    // Text lays out with fonts: with none, there is no caret to report.
    let f = Fixture::with(br#"{"fonts":"system"}"#);
    let s = f.session();
    f.call(s, Method::Run, json!({ "type": "insertText", "value": "Hidden" }));
    assert!(requests(f.take_log()).is_empty(), "no surface, no requests");
    // SAFETY: a live engine and session.
    unsafe { twip_surface(f.engine, s, 400.0, 300.0, 1.0) };
    let first = requests(f.take_log());
    let types: Vec<&str> = first.iter().filter_map(|r| r["type"].as_str()).collect();
    assert_eq!(types, ["cursor", "caret", "a11y"], "{first:?}");
    let mut pixels = vec![0u8; 400 * 300 * 4];
    // SAFETY: a live engine and session, pixels for 400 x 300, no info.
    assert_eq!(unsafe { twip_frame_rgba(f.engine, s, pixels.as_mut_ptr(), 400, 300, std::ptr::null_mut()) }, 0);
    let caret = first.into_iter().chain(requests(f.take_log())).rfind(|r| r["type"] == "caret");
    assert!(caret.as_ref().is_some_and(|c| c["x"].as_f64().is_some_and(|x| x > 0.0) && c["visible"] == true), "the caret shows: {caret:?}");
    f.call(s, Method::Run, json!({ "type": "insertText", "value": "!" }));
    let typed = requests(f.take_log());
    assert!(typed.iter().any(|r| r["type"] == "caret") && typed.iter().any(|r| r["type"] == "a11y"), "{typed:?}");
    assert!(!typed.iter().any(|r| r["type"] == "cursor"), "an unchanged cursor isn't sent again: {typed:?}");
    f.call(s, Method::HostRevealSelection, Value::Null);
    let reveal = requests(f.take_log());
    assert!(reveal.len() == 1 && reveal[0]["type"] == "scroll_to" && reveal[0]["height"].as_f64().is_some_and(|h| h > 0.0), "{reveal:?}");
    // SAFETY: a live engine and session.
    unsafe { twip_surface(f.engine, s, 0.0, 0.0, 1.0) };
    f.call(s, Method::Run, json!({ "type": "insertText", "value": "?" }));
    assert!(requests(f.take_log()).is_empty(), "a hidden session gets none");
}

#[test]
fn the_markup_pane_widens_the_content() {
    let f = Fixture::new();
    let s = f.session();
    let (w, h) = (1200u32, 800u32);
    // SAFETY: a live engine and session.
    unsafe { twip_surface(f.engine, s, f64::from(w), f64::from(h), 1.0) };
    let frame = |f: &Fixture| {
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let mut info = TwipFrameInfo::default();
        // SAFETY: a live engine and session, pixels for w x h, and an info to write.
        assert_eq!(unsafe { twip_frame_rgba(f.engine, s, pixels.as_mut_ptr(), w, h, &mut info) }, 0);
        (info, pixels)
    };
    let (before, _) = frame(&f);
    assert_eq!(f.call(s, Method::HostSetBackground, json!("#202020")).0, 0);
    assert_eq!(f.call(s, Method::HostSetMarkupPaneWidth, json!(200.0)).0, 0);
    let (after, pixels) = frame(&f);
    assert_eq!(after.content_width, before.content_width + 200.0);
    let at = |x: f64| pixels[(400 * w as usize + x as usize) * 4..][..3].to_vec();
    assert_eq!(at(before.content_width + 100.0), [0x30, 0x30, 0x30], "the pane is the background a shade lighter");
    assert_eq!(at(after.content_width + 50.0), [0x20, 0x20, 0x20], "the background past it");
    let (status, pages) = f.call(s, Method::HostVisiblePages, Value::Null);
    assert_eq!(status, 0);
    assert_eq!(pages["ok"][0]["page"], 0, "{pages}");
    assert!(pages["ok"][0]["rect"]["width"].as_f64().is_some_and(|w| w > 500.0), "{pages}");
}

#[test]
fn an_open_is_prepared_then_committed_or_discarded() {
    let f = Fixture::new();
    let s = f.session();
    f.call(s, Method::Run, json!({ "type": "insertText", "value": "Saved" }));
    let opts = br#"{"format":"docx"}"#;
    let (mut file, mut meta) = (TwipBuf { ptr: std::ptr::null_mut(), len: 0 }, TwipBuf { ptr: std::ptr::null_mut(), len: 0 });
    // SAFETY: a live engine and session, options valid for their length, two output buffers.
    assert_eq!(unsafe { twip_export(f.engine, s, opts.as_ptr(), opts.len(), &mut file, &mut meta) }, 0);
    // SAFETY: the engine wrote the file buffer.
    let docx = unsafe { bytes(file.ptr, file.len) }.to_vec();
    // SAFETY: the engine wrote both buffers, and they aren't read again.
    unsafe {
        twip_buf_free(&mut file);
        twip_buf_free(&mut meta);
    }
    let prepare = |data: &[u8]| {
        let mut out = TwipBuf { ptr: std::ptr::null_mut(), len: 0 };
        // SAFETY: a live engine and session, the document valid for its length, no options, an output buffer.
        let status = unsafe { twip_open_prepare(f.engine, s, data.as_ptr(), data.len(), std::ptr::null(), 0, &mut out) };
        (status, take(&mut out))
    };
    let (status, junk) = prepare(b"not a document");
    assert_eq!((status, &junk["error"]["code"]), (status_of(ErrorCode::ImportFailed), &json!("import-failed")));
    f.call(s, Method::Run, json!({ "type": "insertText", "value": " and more" }));
    let revision = |f: &Fixture| f.call(s, Method::State, Value::Null).1["ok"]["revision"].clone();
    let before = revision(&f);
    let (status, token) = prepare(&docx);
    assert_eq!(status, 0, "{token}");
    assert_eq!(revision(&f), before, "preparing replaces nothing");
    assert_eq!(f.call(s, Method::OpenDiscard, token["ok"].clone()).0, 0);
    assert_eq!(f.call(s, Method::OpenCommit, token["ok"].clone()).0, status_of(ErrorCode::InvalidArgument), "a discarded token is gone");
    assert_eq!(revision(&f), before);
    let (_, token) = prepare(&docx);
    let other = f.session();
    assert_eq!(f.call(other, Method::OpenCommit, token["ok"].clone()).0, status_of(ErrorCode::InvalidArgument), "tokens are the session's");
    let (status, warnings) = f.call(s, Method::OpenCommit, token["ok"].clone());
    assert_eq!(status, 0, "{warnings}");
    // As for twip_open: only the fixture's empty library may warn.
    assert!(warnings["ok"].as_array().is_some_and(|w| w.iter().all(|w| w["code"] == "font-unresolved")), "{warnings}");
    assert_ne!(revision(&f), before, "the prepared document is open");
    prepare(&docx);
    // SAFETY: a live engine.
    unsafe { twip_session_free(f.engine, s) };
    assert!(f.engine_ref().prepared.is_empty(), "freeing the session drops its tokens");
}

#[test]
fn frees_work_after_a_fault_and_allocation_failure_is_null() {
    let f = Fixture::new();
    let s = f.session();
    // SAFETY: a live engine and session.
    assert_eq!(unsafe { twip_test_panic(f.engine, s) }, status_of(ErrorCode::Internal));
    // SAFETY: a live engine.
    unsafe { twip_session_free(f.engine, s) };
    assert!(f.engine_ref().engine.session(SessionId(s)).is_none(), "the session was freed");
    assert!(twip_alloc(usize::MAX).is_null());
}

#[test]
fn missing_faces_are_requested_once() {
    let docx = include_bytes!("../../twip/tests/fixtures/text-basics.docx");
    let mut out = TwipBuf { ptr: std::ptr::null_mut(), len: 0 };
    // With no resolver, a missing family is unresolved at once.
    let f = Fixture::new();
    let s = f.session();
    // SAFETY: a live engine and session, the document valid for its length, no options, an output buffer.
    assert_eq!(unsafe { twip_open(f.engine, s, docx.as_ptr(), docx.len(), std::ptr::null(), 0, &mut out) }, 0);
    let warnings = take(&mut out)["ok"].clone();
    assert!(warnings.as_array().is_some_and(|w| w.iter().any(|w| w["code"] == "font-unresolved")), "{warnings}");
    assert!(!f.take_log().iter().any(|(_, k, _)| *k == kind::FONT_REQUEST), "nobody to ask");
    let f = Fixture::new();
    assert_eq!(f.call(0, Method::FontSetResolver, json!(true)).0, 0);
    let s = f.session();
    // SAFETY: a live engine and session, the document valid for its length, no options, an output buffer.
    assert_eq!(unsafe { twip_open(f.engine, s, docx.as_ptr(), docx.len(), std::ptr::null(), 0, &mut out) }, 0, "{}", take(&mut out));
    take(&mut out);
    let log = f.take_log();
    let requests: Vec<&Value> = log.iter().filter(|(_, k, _)| *k == kind::FONT_REQUEST).map(|(_, _, r)| r).collect();
    assert!(requests.iter().any(|r| r["family"] == "Georgia"), "{log:?}");
    assert!(log.iter().filter(|(_, k, _)| *k == kind::FONT_REQUEST).all(|(session, ..)| *session == 0), "font requests are the engine's");
    for r in &requests {
        assert_eq!(f.call(0, Method::FontMarkUnresolved, (*r).clone()).0, 0);
    }
    let log = f.take_log();
    assert!(log.iter().any(|(_, k, e)| *k == kind::EVENT && e["type"] == "fontsChanged"), "unresolved families are reported: {log:?}");
    // SAFETY: as above.
    assert_eq!(unsafe { twip_open(f.engine, s, docx.as_ptr(), docx.len(), std::ptr::null(), 0, &mut out) }, 0);
    take(&mut out);
    assert!(!f.take_log().iter().any(|(_, k, _)| *k == kind::FONT_REQUEST), "each face is requested once");
}

#[test]
fn a_layout_timer_runs_background_layout_to_the_end() {
    let f = Fixture::new();
    let s = f.session();
    let docx = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../twip/tests/fixtures/editor/FX-outline.docx")).expect("the fixture");
    let mut out = TwipBuf { ptr: std::ptr::null_mut(), len: 0 };
    // SAFETY: a live engine and session, the document valid for its length, no options, an output buffer.
    assert_eq!(unsafe { twip_open(f.engine, s, docx.as_ptr(), docx.len(), std::ptr::null(), 0, &mut out) }, 0);
    take(&mut out);
    let timers = |log: &[(u32, u32, Value)]| log.iter().filter(|m| m.1 == kind::SET_TIMER).map(|m| m.2.clone()).collect::<Vec<_>>();
    let mut log = f.take_log();
    assert_eq!(timers(&log), [json!({ "timer": LAYOUT_TIMER, "ms": 0 })], "one timer while layout is pending");
    for _ in 0..1000 {
        if timers(&log).is_empty() {
            break;
        }
        // SAFETY: a live engine and session.
        unsafe { twip_timer_fired(f.engine, s, LAYOUT_TIMER) };
        log = f.take_log();
    }
    assert!(timers(&log).is_empty(), "the timers stop");
    let (_, state) = f.call(s, Method::State, Value::Null);
    assert_eq!(state["ok"]["pages"]["complete"], true, "{}", state["ok"]["pages"]);
    assert!(timers(&f.take_log()).is_empty(), "nothing to arm once complete");
}
