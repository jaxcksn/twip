//! tests/fixtures/docx-feature-showcase.docx against reference positions (points, from a PDF of the document): where
//! the layout differed and was fixed.

use std::path::Path;

use crate::layout::cache::LayoutCache;
use crate::layout::document::layout_records;

use super::{doc, fonts};

/// A dump item line's text field and values.
struct Glyphs {
    page: usize,
    y: f64,
    x: f64,
    color: String,
    chars: String,
}

fn field<'a>(l: &'a str, key: &str) -> Option<&'a str> {
    let at = l.find(&format!(" {key} "))? + key.len() + 2;
    l[at..].split(' ').next()
}

fn layout() -> Option<(usize, Vec<Glyphs>, Vec<(usize, [f64; 4], String)>)> {
    let d = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/docx-feature-showcase.docx")))?;
    let (snapshot, _) = layout_records(&d, &fonts(), &mut LayoutCache::new());
    let text = crate::layout::dump::page_text(&snapshot, &d);
    let (mut glyphs, mut rects, mut page) = (Vec::new(), Vec::new(), 0usize);
    let mut pages = 0;
    for l in text.lines() {
        if l.starts_with("page ") {
            pages += 1;
            page = pages - 1;
        } else if l.starts_with("G ") {
            let chars = l.split(" chars \"").nth(1).and_then(|s| s.split("\" cx").next()).unwrap_or("").to_string();
            let y = field(l, "y").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let x = field(l, "x").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let color = l.split(" color ").nth(1).and_then(|s| s.split(' ').next()).unwrap_or("").to_string();
            glyphs.push(Glyphs { page, y, x, color, chars });
        } else if let Some(r) = l.strip_prefix("R ") {
            let v: Vec<&str> = r.split(' ').collect();
            if v.len() >= 5 {
                let n = |i: usize| v[i].parse::<f64>().unwrap_or(0.0);
                rects.push((page, [n(0), n(1), n(2), n(3)], v[4].to_string()));
            }
        }
    }
    Some((pages, glyphs, rects))
}

fn find<'a>(g: &'a [Glyphs], page: usize, starts: &str) -> &'a Glyphs {
    g.iter().find(|x| x.page == page && x.chars.starts_with(starts)).unwrap_or_else(|| panic!("page {} shows {starts:?}", page + 1))
}

#[test]
fn showcase_matches_word() {
    // The reference positions hold with the document's own fonts; without them (no metric-compatible stand-ins exist
    // for these) the lines can't match, so the test only runs where they're installed.
    let installed = fonts().families();
    if let Some(missing) = ["Calibri Light", "Segoe UI", "Georgia", "Consolas"].into_iter().find(|f| !installed.iter().any(|i| i == f)) {
        eprintln!("showcase skipped: {missing} isn't installed");
        return;
    }
    let Some((pages, g, rects)) = layout() else { return };
    assert_eq!(pages, 10);
    // w:ptab right-aligned at the margin: the even header's title at the right.
    assert!((find(&g, 1, "DOCX Feature Showcase").x - 450.58).abs() < 0.3, "ptab: the header title is right-aligned");
    // A TOC's result doesn't show the Hyperlink character style (teal 0E7C86 = 949382).
    assert_ne!(find(&g, 1, "Welcome").color, "Some(949382)", "TOC entries without the Hyperlink style");
    // Heading 1's rule starts under its number (hanging indent) and is drawn above a drop cap's frame.
    assert!(rects.iter().any(|(p, r, c)| *p == 2 && c == "0e7c86" && (r[1] - 99.5).abs() < 0.5 && r[0] <= 72.01), "Welcome's rule");
    // The drop-capped paragraph and the typography table's rows (emphasis dots, raised/lowered text, vAlign center).
    assert!((find(&g, 2, "ocuments are more than text").y - 119.5).abs() < 0.3);
    assert!((find(&g, 2, "Bold").y - 315.0).abs() < 0.3, "a centred cell's content with its space-after");
    let geo = find(&g, 2, "Georgia").y;
    assert!((geo - 532.5).abs() < 0.6, "rows after the emphasis dots: {geo}");
    // Pagination: the pull quote starts page 4, as in the reference.
    let quote = g.iter().find(|x| x.page == 3 && x.chars.contains("Typography is what")).expect("the pull quote is on page 4");
    assert!((quote.y - 84.6).abs() < 0.5, "at its top");
    // Paragraph shading (the code block) and a double border (two lines a side).
    assert!(rects.iter().any(|(p, _, c)| *p == 3 && c == "f2f4f7"), "the code block's shading");
    assert!(
        rects.iter().filter(|(p, r, c)| *p == 3 && c == "e8684a" && r[3] - r[1] < 1.0).count() >= 2,
        "the boxed paragraph's double top and bottom"
    );
    // A checkbox drawn by its w:sym: ☑.
    assert!(g.iter().any(|x| x.page == 5 && x.chars == "\u{2611}"), "the checked boxes show ☑");
    // BREAK-6 beside the float: compressing the spaces fits "page" onto the third line.
    assert!(
        g.iter().any(|x| x.page == 6 && x.chars.starts_with("are structure, rhythm") && x.chars.trim_end().ends_with("page")),
        "6.1's third line"
    );
    // A next-page section's first space-before less the previous paragraph's space-after (PAGE-7).
    assert!((find(&g, 8, "The Complex Table").y - 96.62).abs() < 0.3);
    assert!((find(&g, 9, "Multi-Column").y - 94.58).abs() < 0.3);
    // Endnotes in a section of columns: below the balanced columns, in the first.
    assert!(find(&g, 9, "Endnotes live").x < 300.0, "the endnote is in column 1");
}
