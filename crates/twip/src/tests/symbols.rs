//! Symbol fonts (FONT-13): where Symbol and Wingdings aren't installed, their bullets and symbols draw the characters
//! their codes show, in a face that has them (the shipped fallback when nothing installed does), and the page's text
//! carries those characters.

use std::path::Path;
use std::rc::Rc;

use crate::font::{Face, Fonts};
use crate::layout::fragment::Item;
use crate::layout::traverse::paint;

/// The characters FX-BULLETS' bullets show, in its order: Symbol F0B7, Courier New o, Wingdings A7, 76, D8, FC, 75,
/// 6E and E8, then the two `w:sym` runs (Symbol B7 and Wingdings FC).
const SHOWN: [char; 9] = ['•', 'o', '▪', '❖', '➢', '✓', '◆', '■', '➔'];

fn bullets() -> crate::docx::Document {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-BULLETS.docx");
    crate::docx::read(&std::fs::read(&p).expect("FX-BULLETS")).expect("reads")
}

/// Every drawn character with its face and glyph, page by page.
fn drawn(fonts: &Fonts) -> Vec<(char, Rc<Face>, u32)> {
    let snapshot = crate::layout::layout(&bullets(), fonts);
    let mut out = Vec::new();
    for page in snapshot.pages() {
        paint(page, &mut |p| {
            if let Item::Glyphs(r) = p.absolute() {
                for (k, c) in r.chars.iter().enumerate() {
                    out.push((c.0, r.face.clone(), r.glyphs.get(k).map_or(u32::MAX, |g| g.0)));
                }
            }
        });
    }
    out
}

/// A catalog with one text face and no symbol fonts: its box glyphs cover printable ASCII only.
fn without_symbol_fonts() -> Fonts {
    let mut f = Fonts::default();
    f.add_bytes(std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/fonts/stand-in-serif.ttf")).expect("the font"))
        .expect("adds");
    f
}

#[test]
fn symbol_bullets_draw_without_symbol_fonts() {
    let d = drawn(&without_symbol_fonts());
    for c in SHOWN {
        let (_, face, glyph) = d.iter().find(|(x, ..)| *x == c).unwrap_or_else(|| panic!("{c} is drawn"));
        assert!(*glyph != 0 && face.has_glyph(c), "{c} draws a real glyph in {}", face.family);
    }
    assert!(d.iter().all(|(_, _, g)| *g != 0), "no .notdef anywhere");
}

#[test]
fn symbol_page_text_is_unicode() {
    let d = drawn(&without_symbol_fonts());
    assert!(d.iter().all(|(c, ..)| !('\u{F000}'..='\u{F0FF}').contains(c)), "no private-use codes in the page's text");
    let syms = d.iter().filter(|(c, ..)| *c == '•' || *c == '✓').count();
    assert!(syms >= 4, "the bullets and both w:sym runs carry • and ✓");
}

#[test]
fn symbol_installed_fonts_draw_their_own_glyphs() {
    // Machine-local: Windows has Symbol and Wingdings.
    let fonts = crate::editing::tests::fonts();
    if !fonts.is_installed("Symbol") || !fonts.is_installed("Wingdings") {
        return;
    }
    let d = drawn(fonts);
    for c in SHOWN.into_iter().filter(|&c| c != 'o') {
        let (_, face, _) = d.iter().find(|(x, ..)| *x == c).unwrap_or_else(|| panic!("{c} is drawn"));
        assert!(matches!(face.family.as_str(), "Symbol" | "Wingdings"), "{c} draws in the installed {}", face.family);
    }
}
