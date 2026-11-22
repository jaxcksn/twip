//! Interaction budgets

use crate::clock::Instant;

use serde_json::{Value, json};

use super::{fonts, open};
use crate::editing::Editor;

fn p95(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * 0.95).round() as usize]
}

fn send(e: &mut Editor, seq: &mut u64, mut msg: Value) -> f64 {
    *seq += 1;
    msg["seq"] = (*seq).into();
    let t = Instant::now();
    let r = e.apply_json(&msg.to_string(), fonts());
    let ms = t.elapsed().as_secs_f64() * 1e3;
    assert!(!r.contains("\"error\""), "{msg} → {r}");
    ms
}

fn border(e: &mut Editor, table: usize, row: usize, k: usize) -> (f64, f64) {
    let st = crate::editing::inspect::inspect(e, "geometry", true);
    let r = st["geometry"]["rows"].as_array().unwrap().iter().find(|r| r["table"] == table && r["page"] == 0 && r["row"] == row).unwrap().clone();
    (r["edges"][k][0].as_f64().unwrap(), (r["rect"][1].as_f64().unwrap() + r["rect"][3].as_f64().unwrap()) / 2.0)
}

#[test]
#[ignore = "timing: run alone in a release build, `cargo nextest run -p twip --release --lib --run-ignored only --test-threads 1 perf`"]
fn pointer_hits_and_drag_moves_are_within_budget() {
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
#[ignore = "timing: run alone in a release build, `cargo nextest run -p twip --release --lib --run-ignored only --test-threads 1 perf`"]
fn live_float_drag_moves_are_within_budget() {
    let mut e = open("FX-OBJ.docx");
    e.complete(fonts());
    let mut seq = 0;
    send(&mut e, &mut seq, json!({ "type": "viewport", "left": 0, "top": 0, "width": 960, "height": 675 }));
    let st = crate::editing::inspect::inspect(&mut e, "geometry", true);
    let f = st["geometry"]["floats"].as_array().unwrap().iter().find(|f| f["float"] == 1).unwrap()["rect"].clone();
    let (x, y) = ((f[0].as_f64().unwrap() + f[2].as_f64().unwrap()) / 2.0, (f[1].as_f64().unwrap() + f[3].as_f64().unwrap()) / 2.0);
    send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "clicks": 1 }));
    let moves: Vec<f64> = (0..100)
        .map(|k| {
            let ms = send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "move", "x": x + k as f64 * 0.5, "y": y - k as f64 * 1.5 }));
            let t = Instant::now();
            e.ensure_page(0, fonts());
            ms + t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    send(&mut e, &mut seq, json!({ "type": "pointer", "kind": "up", "x": x + 50.0, "y": y - 150.0 }));
    assert!(p95(moves.clone()) <= 16.0, "live drag move p95 {:.3} ms", p95(moves));
}

#[test]
#[ignore = "timing: run alone in a release build, `cargo nextest run -p twip --release --lib --run-ignored only --test-threads 1 perf`"]
fn table_structure_and_resize_commits_are_within_budget() {
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
        e.execute(
            crate::editing::commands::Command::InsertRows(crate::api::command::InsertRows {
                side: if true { crate::api::command::VerticalSide::Below } else { crate::api::command::VerticalSide::Above },
                count: Some(1),
            }),
            fonts(),
        );
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

/// The fastest of three enumerations of the elements of `xml`'s document.
fn enumeration_time(xml: &str) -> f64 {
    let e = Editor::open(&super::blank_body(xml), super::fonts()).expect("opens");
    (0..3)
        .map(|_| {
            let t = Instant::now();
            crate::editing::elements::enumerate(e.document(), e.index(), &crate::editing::elements::Filter::default());
            t.elapsed().as_secs_f64()
        })
        .fold(f64::MAX, f64::min)
}

#[test]
fn inline_controls_enumerate_in_linear_time() {
    let para = |n: usize| {
        format!("<w:p>{}</w:p>", "<w:sdt><w:sdtPr><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>x</w:t></w:r></w:sdtContent></w:sdt>".repeat(n))
    };
    let (a, b) = (enumeration_time(&para(1000)), enumeration_time(&para(4000)));
    assert!(b < a * 8.0 + 0.1, "4x the controls took {:.2}x as long ({a:.4}s -> {b:.4}s)", b / a);
}

#[test]
fn a_block_control_of_empty_paragraphs_enumerates_in_linear_time() {
    let body = |n: usize| format!("<w:sdt><w:sdtPr><w:richText/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>", "<w:p/>".repeat(n));
    let (a, b) = (enumeration_time(&body(10000)), enumeration_time(&body(40000)));
    assert!(b < a * 8.0 + 0.1, "4x the paragraphs took {:.2}x as long ({a:.4}s -> {b:.4}s)", b / a);
}

#[test]
fn a_paragraphs_elements_track_in_linear_time() {
    let time = |n: usize| {
        let body = format!(
            "<w:p>{}{}</w:p>",
            "<w:fldSimple w:instr=\" PAGEREF x \"><w:r><w:t xml:space=\"preserve\">5 </w:t></w:r></w:fldSimple>".repeat(n),
            "<w:sdt><w:sdtPr><w:text/></w:sdtPr><w:sdtContent><w:r><w:t xml:space=\"preserve\">x </w:t></w:r></w:sdtContent></w:sdt>".repeat(n)
        );
        let e = Editor::open(&super::blank_body(&body), super::fonts()).expect("opens");
        let para = super::para_id(&e, 0);
        (0..3)
            .map(|_| {
                let t = Instant::now();
                assert_eq!(crate::editing::elements::in_para(e.document(), e.index(), para).len(), 2 * n);
                t.elapsed().as_secs_f64()
            })
            .fold(f64::MAX, f64::min)
    };
    let (a, b) = (time(500), time(2000));
    assert!(b < a * 8.0 + 0.1, "4x the fields and controls took {:.2}x as long ({a:.4}s -> {b:.4}s)", b / a);
}
