//! The renderer contract: every scenario runs with no frames, with the Skia
//! renderer drawing a frame after every input (as the host does), and with a recorder that lags and drops frames. The
//! inspection state after every step is the same in all three: editing state doesn't depend on rendering.

use std::path::Path;

use serde_json::Value;

use crate::editing::inspect::inspect;
use crate::editing::tests::recorder::{Recorder, canonical};
use crate::editing::tests::scenarios::{Run, scenario_files};
use crate::render::{Frame, Renderer};

fn states(path: &Path, mode: usize) -> Vec<Value> {
    let s: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("readable scenario")).expect("json");
    let mut r = Run::open(s["fixture"].as_str().unwrap_or(""), s["id"].as_str().unwrap_or("?"));
    match mode {
        1 => {
            let mut renderer = Renderer::default();
            let mut surface = skia_safe::surfaces::raster_n32_premul((480, 360)).expect("a raster surface");
            r.frame = Some(Box::new(move |e| {
                let f = Frame { origin_x: 0.0, origin_y: 0.0, width: 480.0, height: 360.0, dpr: 1.0, px_per_pt: 4.0 / 3.0 };
                let probe = e.view(0..0);
                let visible = Renderer::visible(&probe, &f);
                if visible.end > 0 {
                    e.ensure_page(visible.end - 1, crate::editing::tests::fonts());
                }
                let view = e.view(visible);
                renderer.draw(surface.canvas(), &view, &e.document().images, &f);
            }));
        }
        2 => {
            let mut rec = Recorder::new(2, 3);
            r.frame = Some(Box::new(move |e| rec.frame(e)));
        }
        _ => {}
    }
    let mut out = Vec::new();
    for step in s["steps"].as_array().into_iter().flatten() {
        r.step(step);
        out.push(inspect(&mut r.e, "caret,selection,undo,dump,integrity", true));
    }
    out
}

#[test]
fn editing_state_is_the_same_with_any_renderer_or_none() {
    let files = scenario_files();
    assert!(!files.is_empty());
    for f in &files {
        let plain = states(f, 0);
        for (mode, what) in [(1, "drawing every frame with Skia"), (2, "a lagging, dropping recorder")] {
            let got = states(f, mode);
            if let Some(k) = (0..plain.len()).find(|&k| got.get(k) != plain.get(k)) {
                let keys: Vec<&String> =
                    plain[k].as_object().map(|o| o.keys().filter(|key| got[k][key.as_str()] != plain[k][key.as_str()]).collect()).unwrap_or_default();
                let _ = std::fs::write(std::env::temp_dir().join("contract-got.json"), serde_json::to_string_pretty(&got[k]).unwrap_or_default());
                let _ = std::fs::write(std::env::temp_dir().join("contract-plain.json"), serde_json::to_string_pretty(&plain[k]).unwrap_or_default());
                panic!(
                    "{}: {what} changed editing state at step {k} in {keys:?}: {} vs {}",
                    f.display(),
                    keys.first().map_or(String::new(), |key| got[k][key.as_str()].to_string().chars().take(300).collect()),
                    keys.first().map_or(String::new(), |key| plain[k][key.as_str()].to_string().chars().take(300).collect())
                );
            }
        }
    }
}

#[test]
fn recorded_frames_are_canonical_and_follow_the_editor() {
    let path = scenario_files().into_iter().find(|f| f.to_string_lossy().contains("E2E-EDIT-01")).expect("E2E-EDIT-01");
    let s: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("json");
    let mut r = Run::open(s["fixture"].as_str().unwrap_or(""), "recorder");
    let rec = std::rc::Rc::new(std::cell::RefCell::new(Recorder::new(0, 0)));
    let sink = rec.clone();
    r.frame = Some(Box::new(move |e| sink.borrow_mut().frame(e)));
    for step in s["steps"].as_array().into_iter().flatten() {
        r.step(step);
    }
    let frames = rec.borrow().frames.clone();
    assert!(!frames.is_empty());
    // The last frame is the editor's view now, and building the same view twice gives the same text.
    let pages = r.e.stack().pages.len().min(2);
    let now = canonical(&r.e.view(0..pages));
    assert_eq!(frames.last(), Some(&now));
    assert_eq!(now, canonical(&r.e.view(0..pages)));
}

#[test]
fn editing_state_is_the_same_on_a_second_run() {
    // Ids the process hands out (paragraphs, controls) have moved on by the second run: nothing in the state may
    // depend on them. New repeating-section items here get fresh control ids.
    let f = scenario_files().into_iter().find(|f| f.to_string_lossy().contains("FUZZ-FX-MERGE-REGIONS-41")).expect("the scenario");
    assert_eq!(states(&f, 0), states(&f, 0));
}
