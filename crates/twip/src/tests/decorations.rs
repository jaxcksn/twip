//! Tab and space decorations

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
    colours: Vec<u32>,
    shapes: Vec<std::rc::Rc<crate::layout::fragment::ShapeItem>>,
    paints: Vec<Option<std::rc::Rc<crate::layout::fragment::GlyphPaint>>>,
}

/// FX-UNDERLINE-TABS (or its `w:ulTrailSpace` twin)'s lines in order: the heading, then one line per case.
fn lines(name: &str) -> Vec<Drawn> {
    drawn_lines(&format!("editor/{name}"))
}

/// The drawn lines of fixture `name` (under `tests/fixtures`), in order.
fn drawn_lines(name: &str) -> Vec<Drawn> {
    drawn_lines_on(name, None)
}

/// [`drawn_lines`], the document's page colour set to `page_colour`.
fn drawn_lines_on(name: &str, page_colour: Option<u32>) -> Vec<Drawn> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    let mut doc = crate::docx::read(&std::fs::read(&p).expect("the fixture")).expect("reads");
    doc.page_colour = page_colour.or(doc.page_colour);
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
                d.paints.push(r.paint.clone());
            }
            Item::Rect { rect, colour } => {
                d.rects.push(rect);
                d.colours.push(colour);
            }
            Item::Shape(s) => d.shapes.push(s),
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

/// fixtures/render/underlines.docx: per style, a line with an automatic and a red underline.
#[test]
fn decorations_draw_every_underline_style() {
    let l = drawn_lines("render/underlines.docx");
    let style = |name: &str| l.iter().find(|d| d.text.starts_with(name) && d.text[name.len()..].starts_with(' ')).unwrap_or_else(|| panic!("{name}"));
    let widths = |d: &Drawn| d.rects.iter().map(|r| ((r[2] - r[0]) * 100.0).round() / 100.0).collect::<Vec<_>>();
    let height = |d: &Drawn| d.rects.iter().map(|r| r[3] - r[1]).fold(0.0, f64::max);
    // Underline colour: the second sample's line is red, the first the text's.
    let single = style("single");
    assert_eq!(single.colours, [0, 0xC0_0000], "{:?}", single.colours);
    // Word's dash lengths, cut where each run ends.
    for (name, first) in [("dotted", 1.44), ("dash", 3.84), ("dashLong", 7.92), ("dotDash", 2.88), ("dotDotDash", 2.16)] {
        let w = widths(style(name));
        assert!(w.len() > 10 && w[0] == first, "{name}: {w:?}");
        assert!(style(name).colours.contains(&0xC0_0000), "{name}: the red sample");
    }
    // Heavy forms: the same dashes, twice as thick.
    for (light, heavy) in [
        ("dotted", "dottedHeavy"),
        ("dash", "dashedHeavy"),
        ("dashLong", "dashLongHeavy"),
        ("dotDash", "dashDotHeavy"),
        ("dotDotDash", "dashDotDotHeavy"),
    ] {
        let (a, b) = (style(light), style(heavy));
        assert_eq!(widths(a), widths(b), "{heavy}: {light}'s dashes");
        assert!((height(b) - 2.0 * height(a)).abs() < 1e-9, "{heavy}: {} vs {}", height(b), height(a));
    }
    assert!((height(style("thick")) - 2.0 * height(single)).abs() < 1e-9);
    // Waves are stroked paths, one per run; wavyDouble two, wavyHeavy twice the stroke.
    let (wave, heavy, double) = (style("wave"), style("wavyHeavy"), style("wavyDouble"));
    assert!(wave.rects.is_empty() && wave.shapes.len() == 2, "{:?}", wave.rects);
    assert_eq!(double.shapes.len(), 4);
    assert_eq!(heavy.shapes[0].line.width, 2.0 * wave.shapes[0].line.width);
    assert!(wave.shapes.iter().all(|s| s.rect[1] > wave.baseline - 1.0 && s.ink), "below the baseline, in document ink");
    assert!(style("none").rects.is_empty() && style("none").shapes.is_empty());
}

/// fixtures/render/char-effects.docx: emboss, engrave, outline and shadow paint their runs and underlines.
#[test]
fn decorations_carry_character_effects() {
    use crate::layout::fragment::GlyphFill;
    let l = drawn_lines("render/char-effects.docx");
    let row = |name: &str| l.iter().find(|d| d.text.starts_with(name)).unwrap_or_else(|| panic!("{name}"));
    let effect = |d: &Drawn| d.paints.iter().flatten().next().cloned().expect("a run with an effect");
    assert!(row("emboss white").paints[0].is_none(), "the label is ordinary text");
    let emboss = effect(row("emboss white"));
    assert_eq!(emboss.fill, GlyphFill::Solid(0));
    assert_eq!(&*emboss.under, &[(0.96, 0.96, 0x99_9999), (-0.96, -0.96, 0)]);
    let engrave = effect(row("imprint shaded"));
    assert_eq!(&*engrave.under, &[(0.96, 0.96, 0), (-0.96, -0.96, 0x82_8282)], "the light tone is the shading's");
    let outline = effect(row("outline white"));
    assert!(outline.fill == GlyphFill::None && outline.outline.is_some() && outline.under.is_empty());
    let shadow = effect(row("shadow white"));
    let [(dx, dy, c)] = *shadow.under else { panic!("{:?}", shadow.under) };
    assert!((dx - 0.0436 * 11.0).abs() < 1e-9 && dx == dy && c == 0xC0_C0C0);
    // An outlined run's underline is hollow: a frame around the line, over nothing.
    let o = row("outline white");
    assert_eq!(o.rects.len(), 8, "two frames of four sides: {:?}", o.rects);
    let (top, bottom) = (o.rects[0], o.rects[1]);
    assert!(top[0] == bottom[0] && top[2] == bottom[2] && bottom[1] - top[3] > 0.5, "a hole between {top:?} and {bottom:?}");
    // An embossed underline has the run's copies under it.
    assert!(row("emboss white").colours.windows(3).any(|w| w == [0x99_9999, 0, 0]), "{:?}", row("emboss white").colours);
}

/// Emboss's light copy is a share of what is behind the text: the page colour where nothing else is.
#[test]
fn decorations_tone_effects_from_the_page_colour() {
    let l = drawn_lines_on("render/char-effects.docx", Some(0x20_3040));
    let row = |name: &str| l.iter().find(|d| d.text.starts_with(name)).unwrap_or_else(|| panic!("{name}"));
    let effect = |d: &Drawn| d.paints.iter().flatten().next().cloned().expect("a run with an effect");
    assert_eq!(effect(row("emboss white")).under[0].2, 0x13_1D26, "0.6 of the page colour");
    assert_eq!(effect(row("emboss shaded")).under[0].2, 0x82_8282, "the paragraph's shading comes first");
}
