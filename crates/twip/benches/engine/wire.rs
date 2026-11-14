//! The SDK wire's JSON (twip-wire): each sample set's encode and decode, whole.
//!
//! - `State`: `big.docx`'s, as every change sends it.
//! - `Command`: one of every variant (the wire fixture), as `run` reads them.
//! - `DialogModel`: every dialog opened on `big.docx`, as `dialogUpdate` reads and writes them.
//! - `Event`: one of every variant (the wire fixture).

use divan::{Bencher, black_box};
use twip::api::command::Command;
use twip::api::engine::{Engine, SessionOptions};
use twip::api::event::Event;
use twip::api::fonts::FontLibrary;
use twip::api::services::{DialogKind, DialogModel};
use twip::api::state::State;
use twip_wire::{Decode, Encode, Named};

/// A wire type and the values it's timed on.
trait Sample: Encode + Decode {
    fn samples() -> Vec<Self>;
}

/// A committed wire fixture's examples, each re-encoded as one JSON value.
fn committed<T: Decode>(name: &str) -> Vec<T> {
    let path = format!("{}/tests/wire/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("the wire fixture");
    let items: Vec<serde_json::Value> = serde_json::from_str(&text).expect("the fixture is JSON");
    items.iter().map(|v| twip_wire::from_slice(&serde_json::to_vec(v).expect("JSON")).expect("the fixture reads")).collect()
}

/// `f` on a session with `big.docx` open.
fn with_big<T>(f: impl FnOnce(&mut twip::api::session::Session) -> T) -> T {
    let mut engine = Engine::new(FontLibrary::empty());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("the session");
    s.open(&super::fixture("big.docx")).expect("big.docx opens");
    f(s)
}

impl Sample for State {
    fn samples() -> Vec<State> {
        with_big(|s| vec![s.state().clone()])
    }
}

impl Sample for Command {
    fn samples() -> Vec<Command> {
        committed("command")
    }
}

impl Sample for DialogModel {
    fn samples() -> Vec<DialogModel> {
        with_big(|s| DialogKind::names().iter().filter_map(|n| s.dialog_open(DialogKind::from_name(n)?).ok()).collect())
    }
}

impl Sample for Event {
    fn samples() -> Vec<Event> {
        committed("event")
    }
}

#[divan::bench(types = [State, Command, DialogModel, Event])]
fn encode<T: Sample>(bencher: Bencher) {
    let samples = T::samples();
    bencher.bench_local(|| samples.iter().map(|v| twip_wire::to_vec(v).len()).sum::<usize>());
}

#[divan::bench(types = [State, Command, DialogModel, Event])]
fn decode<T: Sample>(bencher: Bencher) {
    let inputs: Vec<Vec<u8>> = T::samples().iter().map(twip_wire::to_vec).collect();
    bencher.bench_local(|| {
        for json in &inputs {
            black_box(twip_wire::from_slice::<T>(json).expect("reads"));
        }
    });
}
