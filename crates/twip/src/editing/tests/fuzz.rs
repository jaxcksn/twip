//! Seeded fuzzing: mixed typing, keys and clicks at geometry targets on FX-TEXT and
//! FX-NEST-MIXED. After every step: integrity, a caret at the nesting level of its paragraph, and (every 20 steps and
//! at the end) incremental layout equal to a full one. At the end undo-all restores the original document and redo-all
//! the final one. A failing run is minimised and written as a scenario to tests/scenarios/fuzz-regressions/, where
//! both scenario runners pick it up.
//!
//! `cargo test --release fuzz -- --ignored` (FUZZ_SEEDS, FUZZ_STEPS and FUZZ_SEED override the defaults).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use serde_json::{Value, json};

use super::super::inspect::inspect;
use super::fonts;
use super::scenarios::Run;

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

const KEYS: &[(&str, bool, bool)] = &[
    ("ArrowLeft", false, false),
    ("ArrowRight", false, false),
    ("ArrowUp", false, false),
    ("ArrowDown", false, false),
    ("ArrowLeft", true, false),
    ("ArrowRight", true, false),
    ("ArrowUp", true, false),
    ("ArrowDown", true, false),
    ("ArrowRight", false, true),
    ("ArrowDown", false, true),
    ("ArrowLeft", true, true),
    ("Home", false, false),
    ("End", false, false),
    ("End", false, true),
    ("Home", true, false),
    ("End", true, false),
    ("PageDown", false, false),
    ("PageUp", false, false),
    ("Backspace", false, false),
    ("Backspace", false, false),
    ("Delete", false, false),
    ("Backspace", true, false),
    ("Enter", false, false),
    ("Enter", false, true),
    ("Tab", false, false),
    ("Tab", false, true),
    ("KeyZ", true, false),
    ("KeyY", true, false),
    ("KeyA", true, false),
    ("KeyB", true, false),
    ("KeyI", true, false),
    ("KeyE", true, false),
    ("F4", false, false),
];

const WORDS: &[&str] = &["a", "x", "hello ", " ", "é", "👍🏽", "e\u{301}", "word.", "Ünï"];

fn dump(r: &mut Run) -> Value {
    // With where each paragraph's zero-unit markers sit, which the text dump doesn't show: undo puts them back too.
    let marks: Vec<String> = super::texts_ids(&r.e)
        .iter()
        .filter_map(|p| r.e.index().para(r.e.document(), *p))
        .map(|p| {
            let mut at = 0;
            let mut out = Vec::new();
            for i in &p.inlines {
                match i {
                    crate::docx::Inline::ControlStart(c) => out.push(format!("{at}<{}", c.0)),
                    crate::docx::Inline::ControlEnd(c) => out.push(format!("{at}{}>", c.0)),
                    crate::docx::Inline::Text(..) => {}
                    _ if i.units() == 0 => out.push(format!("{at}{:?}", std::mem::discriminant(i))),
                    _ => {}
                }
                at += i.units();
            }
            out.join(" ")
        })
        .collect();
    json!({ "dump": inspect(&mut r.e, "dump", true)["dump"].clone(), "markers": marks })
}

/// The checks after each step.
fn check(r: &mut Run, n: usize, full: bool) {
    let st = inspect(&mut r.e, "caret,integrity,dump", true);
    assert_eq!(st["integrity"], json!([]), "step {n}: integrity");
    if let Some(p) = st["caret"]["para"].as_u64() {
        let depth = super::scenarios::paragraph(&st["dump"], p as usize)["depth"].as_u64();
        let chain = st["caret"]["chain"].as_array().map(|c| c.len() as u64);
        assert_eq!(chain, depth, "step {n}: the caret's chain is its paragraph's nesting level ({})", st["caret"]);
    }
    if full {
        r.e.complete(fonts());
        let ours = crate::layout::dump::page_text(&r.e.snapshot(), r.e.document());
        let fresh = crate::layout::dump::page_text(&crate::layout::layout(r.e.document(), fonts()), r.e.document());
        assert!(ours == fresh, "step {n}: incremental layout differs from a full layout");
    }
}

/// The table commands a random step runs.
fn table_command(k: usize) -> Value {
    [
        json!({ "type": "insertRows", "where": "below" }),
        json!({ "type": "insertRows", "where": "above" }),
        json!({ "type": "insertColumns", "where": "left" }),
        json!({ "type": "insertColumns", "where": "right" }),
        json!({ "type": "deleteRows" }),
        json!({ "type": "deleteColumns" }),
        json!({ "type": "mergeCells" }),
        json!({ "type": "splitCells", "columns": 2 }),
        json!({ "type": "insertTable", "rows": 2, "columns": 2 }),
        json!({ "type": "deleteTable" }),
    ][k]
        .clone()
}

/// A random step, its pointer target taken from the current geometry.
fn generate(r: &mut Run, rng: &mut Rng) -> Value {
    if rng.below(25) == 0 {
        return json!({ "command": table_command(rng.below(10)) });
    }
    if rng.below(20) == 0 {
        let c = ["copy", "cut", "paste", "paste"][rng.below(4)];
        return json!({ "clipboard": c });
    }
    if rng.below(18) == 0 {
        return merge_step(r, rng);
    }
    if rng.below(15) == 0 {
        // A table border drag (column or row) or a float drag (move, or a corner when it is selected), any modifiers.
        let g = inspect(&mut r.e, "geometry", true);
        let d = |rng: &mut Rng| rng.below(81) as f64 - 40.0;
        let mods = json!({ "shift": rng.below(3) == 0, "ctrl": rng.below(3) == 0 });
        let rows: Vec<&Value> = g["geometry"]["rows"].as_array().into_iter().flatten().collect();
        let floats: Vec<&Value> = g["geometry"]["floats"].as_array().into_iter().flatten().collect();
        if !floats.is_empty() && (rows.is_empty() || rng.below(2) == 0) {
            let f = floats[rng.below(floats.len())]["float"].clone();
            let at = ["", "bottom-right", "top-left", "right"][rng.below(4)];
            let (dx, dy) = (d(rng), d(rng));
            return json!({ "drag": { "from": { "float": f, "at": at }, "to": { "float": f, "at": at, "dx": dx, "dy": dy } }, "mods": mods });
        }
        if !rows.is_empty() {
            let row = rows[rng.below(rows.len())];
            let edges = row["edges"].as_array().cloned().unwrap_or_default();
            let border = if edges.len() > 1 && rng.below(3) > 0 { edges[1 + rng.below(edges.len() - 1)][1].clone() } else { json!("bottom") };
            let t = |dx: f64, dy: f64| json!({ "table": row["table"], "row": row["row"], "border": border, "dx": dx, "dy": dy });
            let (dx, dy) = (d(rng), d(rng));
            return json!({ "drag": { "from": t(0.0, 0.0), "to": t(dx, dy) }, "mods": mods });
        }
    }
    match rng.below(10) {
        0..=2 => json!({ "type": WORDS[rng.below(WORDS.len())] }),
        3..=7 => {
            let (code, ctrl, shift) = KEYS[rng.below(KEYS.len())];
            json!({ "key": code, "mods": { "ctrl": ctrl, "shift": shift } })
        }
        _ => {
            let g = inspect(&mut r.e, "geometry", true);
            let lines: Vec<&Value> =
                g["geometry"]["lines"].as_array().into_iter().flatten().filter(|l| l["story"] == "body" && l["para"].is_u64()).collect();
            if lines.is_empty() {
                return json!({ "key": "ArrowRight" });
            }
            let l = lines[rng.below(lines.len())];
            let stops = l["stops"].as_array().cloned().unwrap_or_default();
            let u = stops.get(rng.below(stops.len())).map_or(0, |s| s[1].as_u64().unwrap_or(0));
            let clicks = [1, 1, 1, 2, 3][rng.below(5)];
            json!({ "click": { "para": l["para"], "offset": u }, "clicks": clicks, "shift": rng.below(6) == 0 })
        }
    }
}

/// The end-of-run checks: redo-all reaches the history's top; then undo-all gives the original document, and redo-all
/// the top again.
fn unwind(r: &mut Run, original: &Value) {
    let depth = |r: &mut Run, k: &str| inspect(&mut r.e, "undo", true)["undo"][k].as_u64().unwrap_or(0);
    let redo_all = |r: &mut Run| {
        for _ in 0..10_000 {
            if depth(r, "redo_depth") == 0 {
                break;
            }
            r.step(&json!({ "key": "KeyY", "mods": { "ctrl": true } }));
        }
    };
    redo_all(r);
    let top = dump(r);
    for _ in 0..10_000 {
        if depth(r, "undo_depth") == 0 {
            break;
        }
        r.step(&json!({ "key": "KeyZ", "mods": { "ctrl": true } }));
    }
    assert_eq!(&dump(r), original, "undo-all restores the original document");
    redo_all(r);
    assert_eq!(dump(r), top, "redo-all restores the final document");
    check(r, usize::MAX, true);
}

/// Generates and runs `n` steps; the steps run so far, and the failure if one happened.
fn fuzz(fixture: &str, seed: u64, n: usize) -> (Vec<Value>, Result<(), String>) {
    let mut steps = Vec::new();
    let res = catch_unwind(AssertUnwindSafe(|| {
        let mut r = Run::open(fixture, &format!("fuzz {fixture} {seed}"));
        let original = dump(&mut r);
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        for k in 0..n {
            let s = generate(&mut r, &mut rng);
            steps.push(s.clone());
            r.step(&s);
            check(&mut r, k, k % 20 == 19);
        }
        unwind(&mut r, &original);
    }));
    (steps, res.map_err(panic_text))
}

/// Replays fixed steps with the same checks.
fn replay(fixture: &str, steps: &[Value]) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(|| {
        let mut r = Run::open(fixture, "fuzz replay");
        let original = dump(&mut r);
        for (k, s) in steps.iter().enumerate() {
            r.step(s);
            check(&mut r, k, k % 20 == 19 || k + 1 == steps.len());
        }
        unwind(&mut r, &original);
    }))
    .map_err(panic_text)
}

fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

/// The kind of a failure: its message up to the first detail (so a smaller run must fail the same way).
fn kind(why: &str) -> String {
    why.split(['(', '[', ':']).take(2).collect::<Vec<_>>().join(":").chars().take(60).collect()
}

/// Drops steps one at a time (last first) while the run still fails the same way.
fn minimise(fixture: &str, mut steps: Vec<Value>, why: &str) -> Vec<Value> {
    let want = kind(why);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut i = steps.len();
    let mut tries = 0;
    while i > 0 && tries < 400 {
        i -= 1;
        tries += 1;
        let mut fewer = steps.clone();
        fewer.remove(i);
        if replay(fixture, &fewer).is_err_and(|w| kind(&w) == want) {
            steps = fewer;
        }
    }
    std::panic::set_hook(hook);
    steps
}

fn env(k: &str, default: u64) -> u64 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
#[ignore = "long: cargo test --release fuzz -- --ignored"]
fn fuzz_editing() {
    let steps = env("FUZZ_STEPS", 300) as usize;
    let seeds: Vec<u64> = match std::env::var("FUZZ_SEED").ok().and_then(|v| v.parse().ok()) {
        Some(s) => vec![s],
        None => (1..=env("FUZZ_SEEDS", 12)).collect(),
    };
    let mut failures = Vec::new();
    for fixture in [
        "FX-TEXT.docx",
        "FX-NEST-MIXED.docx",
        "FX-LISTS.docx",
        "FX-OBJ.docx",
        "FX-TBL-MERGED.docx",
        "FX-STORIES.docx",
        "FX-MERGE-FIELDS.docx",
        "FX-MERGE-CONTROLS.docx",
        "FX-MERGE-REGIONS.docx",
        "FX-MERGE-DATA.docx",
    ] {
        for &seed in &seeds {
            let (run, res) = fuzz(fixture, seed, steps);
            let Err(why) = res else { continue };
            let small = minimise(fixture, run, &why);
            let id = format!("FUZZ-{}-{seed}", fixture.trim_end_matches(".docx"));
            let scenario = json!({
                "id": id, "title": format!("Fuzz regression: {}", why.lines().next().unwrap_or("")), "fixture": fixture,
                "methods": ["K", "M"],
                "steps": small.iter().map(|s| { let mut s = s.clone(); s["expect"] = json!({}); s }).collect::<Vec<_>>(),
            });
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/fuzz-regressions");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join(format!("{id}.json")), serde_json::to_string_pretty(&scenario).unwrap_or_default() + "\n");
            failures.push(format!("{id} ({} steps after minimising): {why}", small.len()));
        }
    }
    assert!(failures.is_empty(), "fuzz failures:\n{}", failures.join("\n"));
}

/// Every saved regression, replayed with the fuzz checks (the scenario runners check only their expectations).
#[test]
fn fuzz_regressions() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/fuzz-regressions");
    // `SCENARIO`: only the regressions whose file name holds it (as the scenario runner filters).
    let only = std::env::var("SCENARIO").ok();
    let files: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json") && only.as_ref().is_none_or(|o| p.to_string_lossy().contains(o.as_str())))
        .collect();
    let failed: Vec<String> = super::par_map(&files, |f| {
        let s: Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap_or_default()).unwrap_or_default();
        let steps: Vec<Value> = s["steps"].as_array().cloned().unwrap_or_default();
        replay(s["fixture"].as_str().unwrap_or(""), &steps)
            .err()
            .map(|why| format!("{}: {}", f.display(), why.lines().take(3).collect::<Vec<_>>().join(" ")))
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A command: an insert at the caret (or around the selection), or a change to an element named by its code
/// or tag (so a saved regression names the same element when replayed), or a mode switch.
fn merge_step(r: &mut Run, rng: &mut Rng) -> Value {
    let all = crate::editing::elements::enumerate(r.e.document(), r.e.index(), &Default::default());
    let pick = |rng: &mut Rng, field: bool| -> Option<String> {
        let v: Vec<String> = all
            .iter()
            .filter_map(|e| match (&e.code, e.props.as_ref().and_then(|p| p.tag.clone())) {
                (Some(c), _) if field => Some(format!("$code:{}", c.trim())),
                (None, Some(t)) if !field => Some(format!("$tag:{t}")),
                _ => None,
            })
            .collect();
        (!v.is_empty()).then(|| v[rng.below(v.len())].clone())
    };
    let cmd = |op: Value| json!({ "command": op });
    let mode = |m: &str| json!({ "mode": m });
    match rng.below(15) {
        14 => match pick(rng, false) {
            Some(id) => cmd(json!({ "type": "fillControl", "id": id, "text": (["Filled", "", "a b"][rng.below(3)]) })),
            None => json!({ "key": "ArrowUp" }),
        },
        13 => cmd(json!({ "type": "wrapInRepeatingSection", "tag": format!("Fuzz/R{}", rng.below(3)) })),
        10 => cmd(json!({
            "type": "replaceInElements",
            "find": (["Fuzz", "MERGEFIELD", "Name", "e"][rng.below(4)]),
            "replace": (["Zz", "", "MERGEFIELD"][rng.below(3)]),
            "in": (["code", "tag", "title"][rng.below(3)])
        })),
        11 => {
            let field = rng.below(2) == 0;
            match pick(rng, field) {
                Some(id) => json!({ "select_element": id }),
                None => json!({ "key": "End" }),
            }
        }
        12 => match pick(rng, true) {
            Some(id) => cmd(json!({ "type": "markElements", "marks": [{ "id": id, "severity": "error", "message": "m" }] })),
            None => json!({ "key": "Home" }),
        },
        0 => cmd(json!({ "type": "insertField", "code": format!("MERGEFIELD Fuzz.F{}", rng.below(5)), "display": (["", "«F»"][rng.below(2)]) })),
        1 => {
            let kind = ["plainText", "richText", "checkbox", "date", "picture"][rng.below(5)];
            cmd(json!({ "type": "insertControl", "kind": kind, "tag": format!("Fuzz/C{}", rng.below(5)) }))
        }
        2 => cmd(json!({ "type": "insertControl", "kind": (["richText", "group"][rng.below(2)]), "tag": "Fuzz/Wrap", "wrapSelection": true })),
        3 => match pick(rng, true) {
            Some(id) => cmd(json!({ "type": "setField", "id": id, "code": format!("MERGEFIELD Fuzz.S{}", rng.below(3)) })),
            None => mode("design"),
        },
        4 => match pick(rng, true) {
            Some(id) => cmd(json!({ "type": "setField", "id": id, "display": (["", "X", "a b"][rng.below(3)]) })),
            None => mode("editing"),
        },
        5 => {
            let field = rng.below(2) == 0;
            match pick(rng, field) {
                Some(id) => cmd(json!({ "type": "removeElement", "value": id })),
                None => json!({ "key": "ArrowLeft" }),
            }
        }
        6 => match pick(rng, false) {
            Some(id) => cmd(json!({ "type": "removeControl", "id": id, "keepContent": rng.below(2) == 0 })),
            None => json!({ "key": "ArrowRight" }),
        },
        7 => match pick(rng, false) {
            Some(id) => cmd(json!({
                "type": "setControl",
                "id": id,
                "tag": format!("Fuzz/T{}", rng.below(3)),
                "lock": (["none", "content", "control"][rng.below(3)])
            })),
            None => json!({ "key": "Home" }),
        },
        _ => mode(["design", "editing"][rng.below(2)]),
    }
}

/// Incremental layout while typing in long tables (an edit inside a table's rows lays out only those rows, the
/// others taken from the baseline's layout of the table): after every input, the same pages as a full layout. The
/// inputs are the typing benchmark's (`editing::input`); FX-TBL-LARGE-NESTED caught a row's text taken from a page
/// record older than the baseline. Release build: `cargo test --release --lib fuzz_editing -- --ignored`.
#[test]
#[ignore = "slow: run in a release build with the fuzz"]
fn fuzz_editing_long_tables() {
    for fixture in ["FX-TBL-LARGE.docx", "FX-TBL-LARGE-NESTED.docx"] {
        use crate::editing::commands::{Command, Dir, MoveBy};
        use crate::editing::position::{Position, StoryId};
        use crate::editing::selection::Selection;
        use crate::layout::Unit;
        let f = super::fonts();
        let mut e = super::open(fixture);
        e.complete(f);
        let mut paras = Vec::new();
        fn walk(blocks: &[crate::docx::Block], out: &mut Vec<(crate::layout::ParaId, u32)>) {
            for b in blocks {
                match b {
                    crate::docx::Block::Para(p) => {
                        let n: u32 = p.inlines.iter().map(|i| i.units() as u32).sum();
                        if n > 0 {
                            out.push((crate::layout::ParaId(p.id), n));
                        }
                    }
                    crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut paras));
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut below = |n: usize| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % n.max(1) as u64) as usize
        };
        for k in 0..60 {
            let (p, len) = paras[below(paras.len())];
            let at = below(len as usize + 1) as u32;
            e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, Unit(at))));
            let mut cmds = vec![Command::InsertText("x".into()); 5];
            cmds.push(crate::editing::tests::del(Dir::Back, false));
            cmds.push(crate::editing::tests::mv(MoveBy::Line, Dir::Forward, false));
            cmds.push(crate::editing::tests::mv(MoveBy::Char, Dir::Forward, false));
            if k % 10 == 0 {
                cmds.push(Command::InsertParagraph);
            }
            if k % 5 == 0 {
                cmds.push(Command::Undo);
            }
            for (j, c) in cmds.into_iter().enumerate() {
                let name = format!("{c:?}");
                e.execute(c, f);
                while e.idle(f) {}
                e.complete(f);
                let full = crate::layout::layout(e.document(), f);
                let a = crate::layout::dump::page_text(&e.snapshot(), e.document());
                let b = crate::layout::dump::page_text(&full, e.document());
                if a != b {
                    let i = a.lines().zip(b.lines()).position(|(x, y)| x != y);
                    panic!(
                        "{fixture} k {k} step {j} {name} para {p:?} at {at}: first differing line {i:?}\n{:?}\n{:?}",
                        i.and_then(|i| a.lines().nth(i)),
                        i.and_then(|i| b.lines().nth(i))
                    );
                }
            }
        }
    }
}
