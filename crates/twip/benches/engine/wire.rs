//! The SDK wire's JSON, serde_json against twip-wire: each samples' encode and decode, whole.
//!
//! - `State`: `big.docx`'s, as every change sends it.
//! - `Command`: one of every variant (the wire fixture), as `run` reads them.
//! - `DialogModel`: every dialog opened on `big.docx`, as `dialogUpdate` reads and writes them.
//! - `Event`: one of every variant (the wire fixture).
//!
//! `encode_serde_value` is the C ABI's old response path: a `serde_json::Value` first, then its bytes.

use divan::{Bencher, black_box};
use serde::Serialize;
use serde::de::DeserializeOwned;
use twip::api::command::Command;
use twip::api::engine::{Engine, SessionOptions};
use twip::api::event::Event;
use twip::api::fonts::FontLibrary;
use twip::api::services::{DialogKind, DialogModel};
use twip::api::state::State;

/// A wire type and the values it's timed on.
trait Sample: Serialize + DeserializeOwned + twip_wire::Encode + twip_wire::Decode {
    fn samples() -> Vec<Self>;
}

fn committed<T: DeserializeOwned>(name: &str) -> Vec<T> {
    let path = format!("{}/tests/wire/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("the wire fixture")).expect("the fixture reads")
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
        with_big(|s| committed::<DialogKind>("dialog-kind").into_iter().filter_map(|k| s.dialog_open(k).ok()).collect())
    }
}

impl Sample for Event {
    fn samples() -> Vec<Event> {
        committed("event")
    }
}

fn encoded<T: Sample>() -> Vec<Vec<u8>> {
    T::samples().iter().map(|v| serde_json::to_vec(v).expect("serializes")).collect()
}

#[divan::bench(types = [State, Command, DialogModel, Event])]
fn encode_serde<T: Sample>(bencher: Bencher) {
    let samples = T::samples();
    bencher.bench_local(|| samples.iter().map(|v| serde_json::to_vec(v).expect("serializes").len()).sum::<usize>());
}

#[divan::bench(types = [State, Command, DialogModel, Event])]
fn encode_serde_value<T: Sample>(bencher: Bencher) {
    let samples = T::samples();
    bencher.bench_local(|| {
        samples.iter().map(|v| serde_json::to_vec(&serde_json::to_value(v).expect("serializes")).expect("serializes").len()).sum::<usize>()
    });
}

#[divan::bench(types = [State, Command, DialogModel, Event])]
fn encode_wire<T: Sample>(bencher: Bencher) {
    let samples = T::samples();
    bencher.bench_local(|| samples.iter().map(|v| twip_wire::to_vec(v).len()).sum::<usize>());
}

#[divan::bench(types = [State, Command, DialogModel, Event])]
fn decode_serde<T: Sample>(bencher: Bencher) {
    let inputs = encoded::<T>();
    bencher.bench_local(|| {
        for json in &inputs {
            black_box(serde_json::from_slice::<T>(json).expect("reads"));
        }
    });
}

#[divan::bench(types = [State, Command, DialogModel, Event])]
fn decode_wire<T: Sample>(bencher: Bencher) {
    let inputs = encoded::<T>();
    bencher.bench_local(|| {
        for json in &inputs {
            black_box(twip_wire::from_slice::<T>(json).expect("reads"));
        }
    });
}
