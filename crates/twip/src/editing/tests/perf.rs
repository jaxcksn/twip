//! Interaction budgets: editor time through the caret's page, without waiting for background layout
//! (ignored by default: run them alone in a release build, `cargo test --release --lib perf -- --ignored`; under the parallel test run their timings mean nothing).

use std::time::Instant;

use serde_json::{Value, json};

use super::{fonts, open};
use crate::editing::Editor;

fn p95(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * 0.95).round() as usize]
}

/// Sends one message and returns its editor time in ms.
fn send(e: &mut Editor, seq: &mut u64, mut msg: Value) -> f64 {
    *seq += 1;
    msg["seq"] = (*seq).into();
    let t = Instant::now();
    let r = e.apply_json(&msg.to_string(), fonts());
    let ms = t.elapsed().as_secs_f64() * 1e3;
    assert!(!r.contains("\"error\""), "{msg} → {r}");
    ms
}

/// A column border of `table` on page 0, at row `row`'s middle: boundary `k`'s x and the row's middle y.
fn border(e: &mut Editor, table: usize, row: usize, k: usize) -> (f64, f64) {
    let st = crate::editing::inspect::inspect(e, "geometry", true);
    let r = st["geometry"]["rows"].as_array().unwrap().iter().find(|r| r["table"] == table && r["page"] == 0 && r["row"] == row).unwrap().clone();
    (r["edges"][k][0].as_f64().unwrap(), (r["rect"][1].as_f64().unwrap() + r["rect"][3].as_f64().unwrap()) / 2.0)
}

#[test]
#[ignore = "timing: run alone in a release build, `cargo test --release --lib perf -- --ignored`"]
fn pointer_hits_and_drag_moves_are_within_budget() {
    // A pointer event's editor time p95 ≤ 0.5 ms; a border drag's moves ≤ 16 ms.
    let mut e = open("FX-TBL-BASIC.docx");
    e.complete(fonts());
    let mut seq = 0;
    send(&mut e, &mut seq, json!({ "type": "viewport", "left": 0, "top": 0, "width": 960, "height": 675 }));
    let mut hits = Vec::new();
    for k in 0..200 {
        let (x, y) = (100.0 + (k * 37 % 400) as f64, 100.0 + (k * 53 % 600) as f64);
        hits.push(send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": 1 })));
        hits.push(send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "up", "x": x, "y": y, "clicks": 1 })));
    }
    assert!(p95(hits.clone()) <= 0.5, "pointer event p95 {:.3} ms", p95(hits));
    let (x, y) = border(&mut e, 1, 0, 1);
    send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": 1 }));
    let moves: Vec<f64> =
        (0..100).map(|k| send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "move", "x": x + k as f64 * 0.3, "y": y }))).collect();
    send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "up", "x": x + 30.0, "y": y }));
    assert!(p95(moves.clone()) <= 16.0, "drag move p95 {:.3} ms", p95(moves));
}

#[test]
#[ignore = "timing: run alone in a release build, `cargo test --release --lib perf -- --ignored`"]
fn table_structure_and_resize_commits_are_within_budget() {
    // Inserting a row and committing a column resize on FX-TBL-LARGE, p95 ≤ 50 ms each.
    let mut e = open("FX-TBL-LARGE.docx");
    e.complete(fonts());
    let mut seq = 0;
    send(&mut e, &mut seq, json!({ "type": "viewport", "left": 0, "top": 0, "width": 960, "height": 675 }));
    let mut inserts = Vec::new();
    for k in 0..20 {
        let para = super::para_id(&e, 20 + k * 8);
        e.set_caret_for_benchmark(crate::editing::selection::Selection::Caret(crate::editing::position::Position::text(
            crate::editing::position::StoryId::Body,
            para,
            crate::layout::Unit(0),
        )));
        let t = Instant::now();
        e.execute(crate::editing::commands::Command::InsertRows { below: true, n: 1 }, fonts());
        inserts.push(t.elapsed().as_secs_f64() * 1e3);
    }
    assert!(p95(inserts.clone()) <= 50.0, "insert row p95 {:.3} ms", p95(inserts));
    e.complete(fonts());
    let mut resizes = Vec::new();
    for k in 0..20 {
        let (x, y) = border(&mut e, 0, 2, 2);
        let dx = if k % 2 == 0 { 6.0 } else { -6.0 };
        send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": 1 }));
        send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "move", "x": x + dx, "y": y }));
        resizes.push(send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "up", "x": x + dx, "y": y })));
        e.complete(fonts());
    }
    assert!(p95(resizes.clone()) <= 50.0, "column resize commit p95 {:.3} ms", p95(resizes));
}
