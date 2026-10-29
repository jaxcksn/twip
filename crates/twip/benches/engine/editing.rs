//! The editor's inputs, each timed whole (editor and layout through the caret's page) at a seeded random place;
//! background layout is drained between them, untimed. Typing is the top performance priority (the constitution's
//! Engine Constraints).

use std::cell::RefCell;

use divan::Bencher;
use twip::internal::editing::Editor;
use twip::internal::editing::commands::{self, Command, Dir, MoveBy};
use twip::internal::editing::position::{Position, StoryId};
use twip::internal::editing::selection::Selection;
use twip::internal::font::Fonts;
use twip::internal::layout::{ParaId, Unit};

use super::Rng;

/// An editor on a fixture, opened with its whole layout.
fn open(name: &str, fonts: &Fonts) -> Editor {
    let mut e = Editor::open(&super::fixture(name), fonts).expect("the fixture opens");
    e.complete(fonts);
    e
}

fn caret(e: &mut Editor, para: ParaId, unit: u32) {
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para, Unit(unit))));
}

/// The body paragraphs with text: (paragraph, length in units).
fn text_paras(e: &Editor) -> Vec<(ParaId, u32)> {
    let mut paras = Vec::new();
    e.document().sections.iter().for_each(|s| super::paragraphs(&s.blocks, 0, &mut paras));
    paras.into_iter().filter(|p| p.1 > 0).map(|p| (p.0, p.1)).collect()
}

/// `input`'s inputs.
#[derive(Clone, Copy, Debug)]
enum Input {
    Typing,
    Backspace,
    Enter,
    Down,
    Right,
    Undo,
}

impl Input {
    fn command(self) -> Command {
        match self {
            Input::Typing => Command::InsertText("x".into()),
            Input::Backspace => commands::deleting(Dir::Back, false),
            Input::Enter => Command::InsertParagraph,
            Input::Down => commands::moving(MoveBy::Line, Dir::Forward, false),
            Input::Right => commands::moving(MoveBy::Char, Dir::Forward, false),
            Input::Undo => Command::Undo,
        }
    }
}

/// One input at a random place in a text paragraph of `big.docx`, with tracked changes on or off.
fn at_random_places(bencher: Bencher, input: Input, tracking: bool) {
    let fonts = Fonts::system();
    let mut e = open("big.docx", &fonts);
    assert!(e.set_tracking(tracking), "tracking can be switched in big.docx");
    let paras = text_paras(&e);
    let e = RefCell::new(e);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    bencher
        .with_inputs(|| {
            let mut e = e.borrow_mut();
            let (para, len) = paras[rng.below(paras.len())];
            caret(&mut e, para, rng.below(len as usize + 1) as u32);
            if let Input::Undo = input {
                e.execute(Command::InsertText("x".into()), &fonts);
            }
            while e.idle(&fonts) {}
            input.command()
        })
        .bench_local_values(|command| e.borrow_mut().execute(command, &fonts));
}

#[divan::bench(args = [Input::Typing, Input::Backspace, Input::Enter, Input::Down, Input::Right, Input::Undo], sample_size = 1)]
fn input(bencher: Bencher, input: Input) {
    at_random_places(bencher, input, false);
}

/// Typing with tracked changes on, beside `input(Typing)`. Budget: within 10% of it.
#[divan::bench(sample_size = 1)]
fn typing_tracked(bencher: Bencher) {
    at_random_places(bencher, Input::Typing, true);
}

/// Typing at the end of a top-level paragraph and of the most deeply nested cell's (depth 7), the last keystroke
/// undone between samples so the paragraph keeps its length: what differs is depth. Budget: depth 7 at most 1.5
/// times depth 1.
#[divan::bench(args = ["shallowest", "deepest"], sample_size = 1)]
fn typing_nested(bencher: Bencher, which: &str) {
    let fonts = Fonts::system();
    let e = open("editor/FX-NEST-DEEP7.docx", &fonts);
    let mut paras = Vec::new();
    e.document().sections.iter().for_each(|s| super::paragraphs(&s.blocks, 0, &mut paras));
    let depth = if which == "deepest" { paras.iter().map(|p| p.2).max().unwrap_or(0) } else { 1 };
    let &(para, len, _) = paras.iter().find(|p| p.2 == depth).expect("a paragraph at that depth");
    let (e, mut typed) = (RefCell::new(e), false);
    bencher
        .with_inputs(|| {
            let mut e = e.borrow_mut();
            if std::mem::replace(&mut typed, true) {
                e.execute(Command::Undo, &fonts);
            }
            caret(&mut e, para, len);
            while e.idle(&fonts) {}
        })
        .bench_local_values(|()| e.borrow_mut().execute(Command::InsertText("x".into()), &fonts));
}

/// Typing through `api::Session` as the SDKs drive it (the state rebuilt and every event encoded as JSON, as the C
/// ABI sends them), a line further down each keystroke. Budget: under 1 ms over `input(Typing)`.
#[divan::bench(sample_size = 1)]
fn typing_api(bencher: Bencher) {
    use twip::api::engine::{Engine, SessionOptions};
    use twip::api::event::Event;
    use twip::api::fonts::FontLibrary;
    let mut engine = Engine::new(FontLibrary::system());
    engine.set_listener(|_, event| {
        let json = match event {
            Event::StateChanged(state) => serde_json::to_vec(state),
            e => serde_json::to_vec(e),
        };
        divan::black_box(json.ok());
    });
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("the session was just created");
    s.open(&super::fixture("big.docx")).expect("the fixture opens");
    let s = RefCell::new(s);
    bencher
        .with_inputs(|| {
            let _ = s.borrow_mut().run(commands::moving(MoveBy::Line, Dir::Forward, false));
        })
        .bench_local_values(|()| s.borrow_mut().text("x"));
}

/// Mail merge on `FX-MERGE-LARGE.docx` (2,000 elements, 500 references in one group).
mod merge {
    use std::cell::RefCell;

    use divan::Bencher;
    use twip::internal::editing::commands::Command;
    use twip::internal::editing::elements::{self, Filter};
    use twip::internal::font::Fonts;

    use super::{Rng, caret, open, text_paras};

    const DOC: &str = "editor/FX-MERGE-LARGE.docx";

    fn command(v: serde_json::Value) -> Command {
        serde_json::from_value(v).expect("a command")
    }

    /// The element list. Budget: under 200 ms.
    #[divan::bench(sample_count = 20)]
    fn elements(bencher: Bencher) {
        let fonts = Fonts::system();
        let e = open(DOC, &fonts);
        bencher.bench_local(|| elements::enumerate(e.document(), e.index(), &Filter::default()));
    }

    /// A rename over every reference in the largest `MERGEFIELD Group.`, undone between samples. Budget: under
    /// 500 ms, one undo step.
    #[divan::bench(sample_count = 20, sample_size = 1)]
    fn rename(bencher: Bencher) {
        let fonts = Fonts::system();
        let e = open(DOC, &fonts);
        let fields = elements::enumerate(e.document(), e.index(), &Filter { kind: Some("field".into()), ..Filter::default() });
        let mut groups = std::collections::BTreeMap::<String, usize>::new();
        for f in &fields {
            if let Some((g, _)) = f.code.as_deref().unwrap_or("").trim().strip_prefix("MERGEFIELD ").and_then(|c| c.split_once('.')) {
                *groups.entry(g.to_string()).or_default() += 1;
            }
        }
        let group = groups.into_iter().max_by_key(|(_, n)| *n).expect("MERGEFIELD codes").0;
        let rename = serde_json::json!({
            "type": "replaceInElements", "find": format!("{group}."), "replace": format!("{group}Renamed."), "in": "code"
        });
        let (e, mut renamed) = (RefCell::new(e), false);
        bencher
            .with_inputs(|| {
                let mut e = e.borrow_mut();
                if std::mem::replace(&mut renamed, true) {
                    e.execute(Command::Undo, &fonts);
                }
                while e.idle(&fonts) {}
                command(rename.clone())
            })
            .bench_local_values(|rename| e.borrow_mut().execute(rename, &fonts));
    }

    /// A field inserted at a random place. Budget: under 100 ms.
    #[divan::bench(sample_size = 1)]
    fn insert_field(bencher: Bencher) {
        let fonts = Fonts::system();
        let e = open(DOC, &fonts);
        let paras = text_paras(&e);
        let (e, mut rng) = (RefCell::new(e), Rng(0x2545_f491_4f6c_dd1d));
        let field = serde_json::json!({ "type": "insertField", "code": "MERGEFIELD Timing.F" });
        bencher
            .with_inputs(|| {
                let mut e = e.borrow_mut();
                let (para, len) = paras[rng.below(paras.len())];
                caret(&mut e, para, rng.below(len as usize + 1) as u32);
                while e.idle(&fonts) {}
                command(field.clone())
            })
            .bench_local_values(|insert| e.borrow_mut().execute(insert, &fonts));
    }
}
