//! What a reader and the spelling checker take a paragraph's text to be

use super::{blank_body, fonts};
use crate::editing::Editor;
use crate::layout::Unit;
use crate::services::a11y::window;
use crate::services::spelling::{Speller, Spellers};

fn dictionary() -> Option<Speller> {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictionaries");
    Speller::new("en-US", &std::fs::read(d.join("en_US.aff")).ok()?, &std::fs::read(d.join("en_US.dic")).ok()?).ok()
}

#[test]
fn a_combining_mark_is_part_of_its_word() {
    let sp = dictionary().expect("the en_US fixture dictionary");
    let nfd: Vec<char> = "nai\u{308}ve".chars().collect();
    let errors = sp.errors(&nfd);
    assert!(errors.iter().all(|e| e.end() == nfd.len() || nfd.get(e.end()).is_some_and(|c| !('\u{300}'..='\u{36f}').contains(c))), "{errors:?}");
    let nfc: Vec<char> = "na\u{ef}ve".chars().collect();
    assert_eq!(errors.len(), sp.errors(&nfc).len(), "decomposed and composed spell the word alike");
    assert_eq!(errors.first().map(|e| (e.start, e.len)), Some((0, 6)), "the whole word, its mark included");
}

/// How long building the caret window takes over a paragraph of `chars` characters, in milliseconds: the fastest of
/// five builds, so a busy machine slows none of the ones counted.
fn window_ms(chars: usize) -> f64 {
    let word = "lorem ipsum dolor sit amet ";
    let body: String = word.repeat(chars / word.len() + 1);
    let mut e = Editor::open(&blank_body(&format!("<w:p><w:r><w:t xml:space=\"preserve\">{body}</w:t></w:r></w:p>")), fonts()).expect("opens");
    e.complete(fonts());
    let para = super::para_id(&e, 0);
    e.select_range(para, Unit(5), Unit(5), fonts());
    let _ = window(&mut e, &Spellers::default(), "en-US", false);
    (0..5)
        .map(|_| {
            let t = crate::clock::Instant::now();
            assert!(window(&mut e, &Spellers::default(), "en-US", false).is_some());
            t.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn the_caret_window_costs_in_line_with_its_paragraph() {
    let small = window_ms(10_000).max(0.25);
    let big = window_ms(80_000);
    // Eight times the text: linear is about 8x; a per-unit scan of the line starts is about 64x. It runs alone
    // (.config/nextest.toml).
    assert!(big / small < 20.0, "small {small} ms, 8x the text {big} ms: {:.1}x", big / small);
}
