//! `twip editlatency <doc.docx> [n]`: the editor's per-input time by category, checked against the editing
//! performance budgets. Each input is timed whole (editor and layout through the caret's page); background layout is drained after each and timed
//! separately. Seeded: the same inputs on every run.

use std::time::Instant;

use twip::internal::editing::Editor;
use twip::internal::editing::commands::{Command, Dir, MoveBy};
use twip::internal::editing::position::{Position, StoryId};
use twip::internal::editing::selection::Selection;
use twip::internal::font::Fonts;
use twip::internal::layout::{ParaId, Unit};

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % n.max(1) as u64) as usize
    }
}

fn percentiles(mut v: Vec<f64>) -> String {
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v.get(((v.len().saturating_sub(1)) as f64 * q).round() as usize).copied().unwrap_or(0.0);
    format!("p50 {:.2} p95 {:.2} max {:.2} ms (n {})", at(0.5), at(0.95), v.last().copied().unwrap_or(0.0), v.len())
}

/// Body paragraphs with text, in document order.
fn text_paras(e: &Editor) -> Vec<(ParaId, u32)> {
    fn walk(blocks: &[twip::internal::docx::Block], out: &mut Vec<(ParaId, u32)>) {
        for b in blocks {
            match b {
                twip::internal::docx::Block::Para(p) => {
                    let n = p.inlines.iter().map(|i| i.units() as u32).sum();
                    if n > 0 {
                        out.push((ParaId(p.id), n));
                    }
                }
                twip::internal::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    out
}

/// Body paragraphs with their nesting depth.
fn paras_by_depth(e: &Editor) -> Vec<(ParaId, u32, usize)> {
    fn walk(blocks: &[twip::internal::docx::Block], depth: usize, out: &mut Vec<(ParaId, u32, usize)>) {
        for b in blocks {
            match b {
                twip::internal::docx::Block::Para(p) => out.push((ParaId(p.id), p.inlines.iter().map(|i| i.units() as u32).sum(), depth)),
                twip::internal::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, depth + 1, out)),
            }
        }
    }
    let mut out = Vec::new();
    e.document().sections.iter().for_each(|s| walk(&s.blocks, 0, &mut out));
    out
}

/// `editlatency --depth <doc> [n]`: typing in the deepest cell against typing in a depth-1 cell.
pub fn run_depth(path: &str, n: usize) -> Result<(), String> {
    let fonts = Fonts::system();
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let mut e = Editor::open(&bytes, &fonts)?;
    e.complete(&fonts);
    let paras = paras_by_depth(&e);
    let deepest = paras.iter().map(|p| p.2).max().unwrap_or(0);
    let p95 = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[((v.len() - 1) as f64 * 0.95).round() as usize]
    };
    let (mut ed, mut all) = (Vec::new(), Vec::new());
    for depth in [1, deepest] {
        let &(para, len, _) = paras.iter().find(|p| p.2 == depth).ok_or(format!("no paragraph at depth {depth}"))?;
        e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(len))));
        let (mut times, mut editor) = (Vec::new(), Vec::new());
        for k in 0..n {
            twip::internal::prof::take();
            let t = Instant::now();
            e.execute(Command::InsertText("x".into()), &fonts);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            let layout: f64 = twip::internal::prof::take().iter().filter(|t| t.0 == "editor:layout").map(|t| t.1).sum();
            times.push(ms);
            editor.push(ms - layout);
            while e.idle(&fonts) {}
            // Undone every 10 characters (untimed), so both paragraphs stay their own length: what differs is depth.
            if k % 10 == 9 {
                e.execute(Command::Undo, &fonts);
                e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(len))));
                while e.idle(&fonts) {}
            }
        }
        println!("depth {depth:<3} typing {}; editor alone {}", percentiles(times.clone()), percentiles(editor.clone()));
        all.push(p95(times));
        ed.push(p95(editor));
    }
    println!("with layout, p95 ratio depth {deepest} / depth 1: {:.2}", all[1] / all[0].max(1e-6));
    let p95 = ed;
    let ratio = p95[1] / p95[0].max(1e-6);
    println!("editor time p95 ratio depth {deepest} / depth 1: {ratio:.2} (budget: at most 1.5)");
    if ratio > 1.5 { Err(format!("depth budget fails: ratio {ratio:.2}")) } else { Ok(()) }
}

pub fn run(path: &str, n: usize) -> Result<(), String> {
    measure(path, n, false).map(|_| ())
}

/// `editlatency --tracking <doc> [n]`: the same inputs with tracking off, then on, three times over; typing side by
/// side, each mode by its best round (budget: tracking on within 10% of off). Alternating rounds keep load from
/// other processes from landing on one mode only.
pub fn run_tracking(path: &str, n: usize) -> Result<(), String> {
    let p = |mut v: Vec<f64>, q: f64| {
        v.sort_by(f64::total_cmp);
        v.get(((v.len().saturating_sub(1)) as f64 * q).round() as usize).copied().unwrap_or(0.0)
    };
    let (mut off, mut on) = ((f64::MAX, f64::MAX), (f64::MAX, f64::MAX));
    for round in 1..=3 {
        for (tracking, best) in [(false, &mut off), (true, &mut on)] {
            println!("-- round {round}, tracking {}", if tracking { "on" } else { "off" });
            let v = measure(path, n, tracking)?;
            *best = (best.0.min(p(v.clone(), 0.5)), best.1.min(p(v, 0.95)));
        }
    }
    println!("typing p50 {:.2} ms off, {:.2} ms on; p95 {:.2} ms off, {:.2} ms on (best of 3)", off.0, on.0, off.1, on.1);
    let ratio = on.1 / off.1.max(1e-6);
    println!("typing p95 tracking on / off: {ratio:.2} (budget: at most 1.10)");
    if ratio > 1.1 { Err(format!("tracking budget fails: ratio {ratio:.2}")) } else { Ok(()) }
}

/// The per-input times, with tracking on or off; returns typing's.
fn measure(path: &str, n: usize, tracking: bool) -> Result<Vec<f64>, String> {
    let fonts = Fonts::system();
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let t = Instant::now();
    let mut e = Editor::open(&bytes, &fonts)?;
    if !e.set_tracking(tracking) {
        return Err("tracking can't be turned off in this document".into());
    }
    println!("open {:.1} ms, {} pages", t.elapsed().as_secs_f64() * 1e3, e.snapshot().pages().len());
    let t = Instant::now();
    while e.idle(&fonts) {}
    println!("background layout after open {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let paras = text_paras(&e);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut typing, mut back, mut enter, mut down, mut right, mut undo, mut idle) = (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    twip::internal::prof::take();
    // Where typing's time goes (the phase timers during typing only).
    let mut typing_phases: Vec<(&'static str, f64, u64)> = Vec::new();
    let mut timed = |e: &mut Editor, cmd: Command, out: &mut Vec<f64>, idle: &mut Vec<f64>| {
        let is_typing = matches!(cmd, Command::InsertText(_));
        twip::internal::prof::take();
        let t = Instant::now();
        e.execute(cmd, &fonts);
        out.push(t.elapsed().as_secs_f64() * 1e3);
        if is_typing {
            for (name, ms, calls) in twip::internal::prof::take() {
                match typing_phases.iter_mut().find(|p| p.0 == name) {
                    Some(p) => {
                        p.1 += ms;
                        p.2 += calls;
                    }
                    None => typing_phases.push((name, ms, calls)),
                }
            }
        }
        let t = Instant::now();
        while e.idle(&fonts) {}
        idle.push(t.elapsed().as_secs_f64() * 1e3);
    };
    for k in 0..n {
        // A place: a random text paragraph, a random offset in it.
        let (p, len) = paras[rng.below(paras.len())];
        let caret = Position::text(StoryId::Body, p, Unit(rng.below(len as usize + 1) as u32));
        e.set_caret_for_benchmark(Selection::Caret(caret));
        for _ in 0..5 {
            timed(&mut e, Command::InsertText("x".into()), &mut typing, &mut idle);
        }
        timed(&mut e, twip::internal::editing::commands::deleting(Dir::Back, false), &mut back, &mut idle);
        timed(&mut e, twip::internal::editing::commands::moving(MoveBy::Line, Dir::Forward, false), &mut down, &mut idle);
        timed(&mut e, twip::internal::editing::commands::moving(MoveBy::Char, Dir::Forward, false), &mut right, &mut idle);
        if k % 10 == 0 {
            timed(&mut e, Command::InsertParagraph, &mut enter, &mut idle);
        }
        if k % 5 == 0 {
            timed(&mut e, Command::Undo, &mut undo, &mut idle);
        }
    }
    let typed = typing.len().max(1) as f64;
    let typing_ms = typing.clone();
    println!("typing    {}", percentiles(typing));
    typing_phases.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (name, ms, calls) in &typing_phases {
        println!("  typing phase {name:<16} {:.3} ms per keystroke ({calls} calls)", ms / typed);
    }
    println!("backspace {}", percentiles(back));
    println!("enter     {}", percentiles(enter));
    println!("down      {}", percentiles(down));
    println!("right     {}", percentiles(right));
    println!("undo      {}", percentiles(undo));
    println!("idle      {}", percentiles(idle));
    let full = twip::internal::layout::layout(e.document(), &fonts);
    e.complete(&fonts);
    let same = twip::internal::layout::dump::page_text(&e.snapshot(), e.document()) == twip::internal::layout::dump::page_text(&full, e.document());
    println!("matches a full layout: {same}");
    Ok(typing_ms)
}

/// `editlatency --merge <doc> [n]`: a field insert and drop, the `elements` list (budget: under 0.2 s on 2,000
/// elements) and a rename over 500 references (budget: under 0.5 s, one undo step), through the editor's typed calls.
pub fn run_merge(path: &str, n: usize) -> Result<(), String> {
    use twip::internal::editing::elements::{self, Filter};
    use twip::internal::editing::input::PointerKind;
    use twip::internal::editing::keymap::Mods;
    let fonts = Fonts::system();
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let mut e = Editor::open(&bytes, &fonts)?;
    e.complete(&fonts);
    // A command as the API's JSON gives it, timed; background layout after it isn't.
    let run = |e: &mut Editor, v: serde_json::Value| -> Result<f64, String> {
        let cmd: Command = serde_json::from_value(v).map_err(|err| err.to_string())?;
        let t = Instant::now();
        e.execute(cmd, &fonts);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        while e.idle(&fonts) {}
        Ok(ms)
    };
    let mut list = Vec::new();
    let mut count = 0;
    for _ in 0..20 {
        let t = Instant::now();
        count = elements::enumerate(e.document(), e.index(), &Filter::default()).len();
        list.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("elements     {} over {count} elements (budget: under 200 ms)", percentiles(list.clone()));
    // The rename: the code prefix with the most references (`MERGEFIELD Group.`).
    let fields = elements::enumerate(e.document(), e.index(), &Filter { kind: Some("field".into()), ..Filter::default() });
    let mut groups: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in &fields {
        let code = f.code.as_deref().unwrap_or("").trim();
        if let Some(g) = code.strip_prefix("MERGEFIELD ").and_then(|c| c.split_once('.')).map(|(g, _)| g.to_string()) {
            *groups.entry(g).or_default() += 1;
        }
    }
    let (group, refs) = groups.into_iter().max_by_key(|(g, c)| (*c, std::cmp::Reverse(g.clone()))).ok_or("no MERGEFIELD codes")?;
    let depth = e.history().undo_depth();
    let rename = run(
        &mut e,
        serde_json::json!({ "type": "replaceInElements", "find": format!("{group}."), "replace": format!("{group}Renamed."), "in": "code" }),
    )?;
    let changed = match e.take_made() {
        Some(twip::api::command::CommandResult::Changed { changed }) => changed,
        _ => 0,
    };
    let steps = e.history().undo_depth() - depth;
    let undo = run(&mut e, serde_json::json!({ "type": "undo" }))?;
    println!(
        "element.replace {rename:.1} ms over {changed} of {refs} references in \"{group}.\", {steps} undo step; undo {undo:.1} ms (budget: under 500 ms, one step)"
    );
    // After the rename: a random place can be inside a field's code, and a field inserted there splits it.
    let paras = text_paras(&e);
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let (mut insert, mut drop) = (Vec::new(), Vec::new());
    for k in 0..n {
        let (p, len) = paras[rng.below(paras.len())];
        e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, p, Unit(rng.below(len as usize + 1) as u32))));
        let field = serde_json::json!({ "type": "insertField", "code": format!("MERGEFIELD Timing.F{k}") });
        if k % 2 == 0 {
            insert.push(run(&mut e, field)?);
        } else {
            // A drop at the caret's place: a click at its point in document coordinates, then the field there.
            let at = e.caret_view().and_then(|c| e.stack().to_doc(c.page, c.x, (c.top + c.bottom) / 2.0));
            let Some((x, y)) = at else { continue };
            let mut clicked = 0.0;
            for kind in [PointerKind::Down, PointerKind::Up] {
                let t = Instant::now();
                e.pointer_input(kind, x, y, 1, Mods::default(), &fonts);
                clicked += t.elapsed().as_secs_f64() * 1e3;
                while e.idle(&fonts) {}
            }
            drop.push(clicked + run(&mut e, field)?);
        }
    }
    println!("field.insert {} (budget: under 100 ms)", percentiles(insert.clone()));
    println!("drop         {} (budget: under 100 ms)", percentiles(drop.clone()));
    let mut fails = Vec::new();
    let p_max = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
    if p_max(&insert) > 100.0 || p_max(&drop) > 100.0 {
        fails.push("field insert and drop");
    }
    if p_max(&list) > 200.0 || count < 2000 {
        fails.push("elements");
    }
    if rename > 500.0 || steps != 1 || changed < 500 {
        fails.push("rename");
    }
    if fails.is_empty() { Ok(()) } else { Err(format!("{} not met", fails.join(", "))) }
}

/// `editlatency --api <doc> [n]`: typing through `api::Session` (the state rebuilt and every event encoded as JSON,
/// as the C ABI sends them) against typing on the editor directly, the same keystrokes at the same places: `n`
/// rounds of five keystrokes, a few lines further down each round (untimed). Neither drains background layout (a
/// session runs it on the engine's timers, which nothing arms here). Three runs; the API's p95 overhead is the median
/// of the three (budget: under 1 ms).
pub fn run_api(path: &str, n: usize) -> Result<(), String> {
    use std::cell::Cell;
    use std::rc::Rc;
    use twip::api::engine::{Engine, SessionOptions};
    use twip::api::event::Event;
    use twip::api::fonts::FontLibrary;
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let p95 = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.get(((v.len().saturating_sub(1)) as f64 * 0.95).round() as usize).copied().unwrap_or(0.0)
    };
    // The lines each round moves down: the same in both, so both type at the same places.
    let downs: Vec<usize> = {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        (0..n).map(|_| 1 + rng.below(40)).collect()
    };
    let down = || twip::internal::editing::commands::moving(MoveBy::Line, Dir::Forward, false);
    let (mut direct, mut api, mut overhead) = (Vec::new(), Vec::new(), Vec::new());
    for run in 1..=3 {
        let fonts = Fonts::system();
        let mut e = Editor::open(&bytes, &fonts)?;
        let mut times = Vec::new();
        for &d in &downs {
            for _ in 0..5 {
                let t = Instant::now();
                e.execute(Command::InsertText("x".into()), &fonts);
                times.push(t.elapsed().as_secs_f64() * 1e3);
                e.take_commits();
            }
            for _ in 0..d {
                e.execute(down(), &fonts);
            }
            e.take_commits();
        }
        drop(e);
        let mut engine = Engine::new(FontLibrary::system());
        let encoded = Rc::new(Cell::new(0usize));
        let sink = encoded.clone();
        engine.set_listener(move |_, event| {
            let json = match event {
                Event::StateChanged(state) => serde_json::to_vec(state),
                e => serde_json::to_vec(e),
            };
            sink.set(sink.get() + json.map_or(0, |j| j.len()));
        });
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).ok_or("no session")?;
        s.open(&bytes).map_err(|e| e.message)?;
        let mut through = Vec::new();
        for &d in &downs {
            for _ in 0..5 {
                let t = Instant::now();
                s.text("x");
                through.push(t.elapsed().as_secs_f64() * 1e3);
            }
            for _ in 0..d {
                let _ = s.run(down());
            }
        }
        let (a, b) = (p95(times.clone()), p95(through.clone()));
        println!(
            "run {run}: engine {}; api {}; p95 overhead {:.3} ms ({} KB of JSON a keystroke)",
            percentiles(times),
            percentiles(through),
            b - a,
            encoded.get() / 1024 / (5 * n).max(1)
        );
        direct.push(a);
        api.push(b);
        overhead.push(b - a);
    }
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[1]
    };
    let (a, b, o) = (median(direct), median(api), median(overhead));
    println!("typing p95 engine {a:.3} ms, api {b:.3} ms; api overhead {o:.3} ms (median of 3; budget: under 1 ms)");
    if o < 1.0 { Ok(()) } else { Err(format!("api budget fails: {o:.3} ms")) }
}
