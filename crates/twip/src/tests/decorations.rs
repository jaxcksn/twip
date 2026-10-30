//! Underline and strikethrough over tabs and spaces (FONT-15): a tab is decorated across its width wherever it falls
//! (a signature line), the spaces that end a line aren't (unless `w:ulTrailSpace`), and `words` skips spaces and tabs.

use std::path::Path;

use crate::layout::fragment::Item;
use crate::layout::traverse::paint;

/// One drawn line: its text, its glyph runs' extent and baseline, and its decoration rectangles.
#[derive(Debug, Default)]
struct Drawn {
    text: String,
    left: f64,
    right: f64,
    baseline: f64,
    rects: Vec<[f64; 4]>,
}

/// FX-UNDERLINE-TABS (or its `w:ulTrailSpace` twin)'s lines in order: the heading, then one line per case.
fn lines(name: &str) -> Vec<Drawn> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    let doc = crate::docx::read(&std::fs::read(&p).expect("the fixture")).expect("reads");
    let snapshot = crate::layout::layout(&doc, crate::editing::tests::fonts());
    let mut out: Vec<(u32, Drawn)> = Vec::new();
    paint(&snapshot.pages()[0], &mut |p| {
        let k = match out.iter().position(|(l, _)| *l == p.line) {
            Some(k) => k,
            None => {
                out.push((p.line, Drawn { left: f64::MAX, right: f64::MIN, ..Drawn::default() }));
                out.len() - 1
            }
        };
        let d = &mut out[k].1;
        match p.absolute() {
            Item::Glyphs(r) if !r.chars.is_empty() => {
                d.text.extend(r.chars.iter().map(|c| c.0));
                let end = r.glyphs.last().map_or(r.x, |&(g, x)| x + r.face.advance(g) * r.face.scale(r.size));
                d.left = d.left.min(r.x);
                d.right = d.right.max(end);
                d.baseline = r.baseline;
            }
            Item::Rect { rect, .. } => d.rects.push(rect),
            _ => {}
        }
    });
    out.into_iter().map(|(_, d)| d).collect()
}

/// The decorations' combined extent, and whether every one is below the baseline (underline) or above (strike).
fn extent(d: &Drawn) -> Option<(f64, f64)> {
    d.rects.iter().map(|r| (r[0], r[2])).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
}

/// The right tab stop at 6in from the left margin.
const STOP: f64 = 72.0 + 432.0;

#[test]
fn decorations_cover_tabs_and_skip_trailing_spaces() {
    let l = lines("FX-UNDERLINE-TABS.docx");
    assert!(l[0].text.starts_with("FX-UNDERLINE-TABS"), "the heading first: {:?}", l[0].text);
    let near = |a: f64, b: f64| (a - b).abs() < 1.0;
    // 1. A signature line: "Name: " then an underlined tab to the stop.
    let (_, r) = extent(&l[1]).expect("1: the tab is underlined");
    assert!(near(r, STOP), "1: the underline reaches the stop: {r}");
    assert!(l[1].rects.iter().all(|x| x[1] > l[1].baseline), "1: an underline, below the baseline");
    // 2. Underlined text, tab, text: one line from the first word to the last.
    let (a, b) = extent(&l[2]).expect("2");
    assert!(near(a, l[2].left) && near(b, l[2].right), "2: from {a} to {b}, text {}..{}", l[2].left, l[2].right);
    assert_eq!(l[2].rects.len(), 1, "2: joined across the tab");
    // 3. Only a tab.
    assert!(extent(&l[3]).is_some_and(|(a, b)| near(a, 72.0) && near(b, STOP)), "3: {:?}", extent(&l[3]));
    // 4. A dot-leader tab, underlined: the leader and the underline.
    assert!(l[4].text.contains("...."), "4: the leader draws: {:?}", l[4].text);
    assert!(extent(&l[4]).is_some_and(|(_, b)| near(b, STOP)), "4: underlined to the stop");
    // 5. Underlined trailing spaces: the underline stops at the last word, short of the three spaces (`right` is where
    // the drawn spaces end).
    assert!(extent(&l[5]).is_some_and(|(_, b)| b < l[5].right - 3.0), "5: {:?} vs {}", extent(&l[5]), l[5].right);
    // 6. Only spaces: nothing.
    assert!(l[6].rects.is_empty(), "6: {:?}", l[6].rects);
    // 7. Words only: under a, b and c, not the space or the tab.
    assert_eq!(l[7].rects.len(), 3, "7: three words: {:?}", l[7].rects);
    // 8 to 10: strikethrough, the same extents.
    assert!(extent(&l[8]).is_some_and(|(_, b)| near(b, STOP)) && l[8].rects.iter().all(|x| x[3] < l[8].baseline), "8: struck to the stop");
    assert!(extent(&l[9]).is_some_and(|(_, b)| b < l[9].right - 3.0), "9: no strike over trailing spaces");
    assert_eq!(l[10].rects.len(), 1, "10: struck through the tab");
}

#[test]
fn decorations_ul_trail_space_underlines_trailing_spaces() {
    let l = lines("FX-UNDERLINE-TABS-TRAIL.docx");
    let near = |a: f64, b: f64| (a - b).abs() < 1.0;
    assert!(extent(&l[5]).is_some_and(|(_, b)| near(b, l[5].right)), "5: the trailing spaces are underlined: {:?} vs {}", extent(&l[5]), l[5].right);
    assert!(!l[6].rects.is_empty(), "6: a line of spaces is underlined");
    assert!(extent(&l[9]).is_some_and(|(_, b)| b < l[9].right - 3.0), "9: strikethrough still stops at the last word");
}
