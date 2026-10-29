//! Markup views (REV-1 to REV-5): No markup lays out as every revision accepted and Original as every one rejected;
//! each view's pages and lines per paragraph match the expected layouts; All markup draws the revision marks, author
//! colours, change bars and row tints, and Simple markup the change bars alone.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::docx::review::{AuthorPalette, MarkupView};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::Snapshot;
use crate::layout::fragment::Item;
use crate::layout::traverse::paint;

fn fixture(name: &str) -> Document {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review").join(format!("{name}.docx"));
    crate::docx::read(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("a review fixture")
}

fn in_view(mut d: Document, view: MarkupView) -> Document {
    d.review.view = view;
    d
}

/// Per page, each drawn line's baseline, start and text (to 0.01 pt): what a line box is, without the decorations and
/// change bars drawn beside it.
fn lines(d: &Document, fonts: &Fonts) -> Vec<Vec<(i64, i64, String)>> {
    snapshot_lines(&crate::layout::layout(d, fonts))
}

fn snapshot_lines(snapshot: &Snapshot) -> Vec<Vec<(i64, i64, String)>> {
    let round = |v: f64| (v * 100.0).round() as i64;
    snapshot
        .pages()
        .iter()
        .map(|page| {
            let mut by_line: Vec<(u32, (i64, i64, String))> = Vec::new();
            paint(page, &mut |p| {
                if let Item::Glyphs(r) = p.absolute() {
                    let text: String = r.chars.iter().map(|c| c.0).collect();
                    match by_line.iter_mut().find(|(l, _)| *l == p.line) {
                        Some((_, (_, _, t))) => t.push_str(&text),
                        None => by_line.push((p.line, (round(r.baseline), round(r.x), text))),
                    }
                }
            });
            by_line.into_iter().map(|(_, l)| l).collect()
        })
        .collect()
}

fn assert_same_lines(name: &str, view: MarkupView, twin: &str, fonts: &Fonts) {
    let got = lines(&in_view(fixture(name), view), fonts);
    let want = lines(&fixture(twin), fonts);
    assert_eq!(got.len(), want.len(), "{name} in {view:?}: pages differ from {twin}");
    for (pi, (g, w)) in got.iter().zip(&want).enumerate() {
        for (k, (a, b)) in g.iter().zip(w).enumerate() {
            assert_eq!(a, b, "{name} in {view:?}: page {} line {k} differs from {twin}", pi + 1);
        }
        assert_eq!(g.len(), w.len(), "{name} in {view:?}: page {} has {} lines, {twin} {}", pi + 1, g.len(), w.len());
    }
}

#[test]
fn no_markup_lays_out_as_accepted_and_original_as_rejected() {
    let fonts = Fonts::system();
    for name in ["FX-REVIEW-RUNS", "FX-REVIEW-TABLES", "FX-REVIEW-SECTIONS"] {
        assert_same_lines(name, MarkupView::NoMarkup, &format!("{name}-ACCEPTED"), &fonts);
    }
    for name in ["FX-REVIEW-RUNS", "FX-REVIEW-SECTIONS"] {
        assert_same_lines(name, MarkupView::Original, &format!("{name}-REJECTED"), &fonts);
    }
}

/// Original lays a table out with its table, row and cell properties before their tracked changes.
#[test]
fn original_tables() {
    assert_same_lines("FX-REVIEW-TABLES", MarkupView::Original, "FX-REVIEW-TABLES-REJECTED", &Fonts::system());
}

/// A deleted paragraph mark joins its paragraph with the next in No markup, and an inserted one in Original (REV-6).
#[test]
fn paragraph_marks_by_view() {
    let fonts = Fonts::system();
    assert_same_lines("FX-REVIEW-MARKS", MarkupView::NoMarkup, "FX-REVIEW-MARKS-ACCEPTED", &fonts);
    assert_same_lines("FX-REVIEW-MARKS", MarkupView::Original, "FX-REVIEW-MARKS-REJECTED", &fonts);
}

/// Paragraphs the view joins (REV-6) share their line for editing: each keeps its own stops, the end of the first and
/// the start of the next show the caret at one x, a point just past the join is in the next paragraph and one just
/// before it in the first, and a selection across the join is one band, with no mark drawn where the hidden one is.
#[test]
fn joined_paragraphs_share_their_line_for_editing() {
    use crate::editing::Editor;
    use crate::editing::commands::Command;
    use crate::editing::position::{Affinity, Position, StoryId};
    use crate::layout::Unit;
    let fonts = Fonts::system();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-MARKS.docx");
    let mut e = Editor::open(&std::fs::read(p).expect("the fixture"), &fonts).expect("opens");
    e.execute(Command::SetMarkupView(MarkupView::NoMarkup), &fonts);
    e.complete(&fonts);
    let geos = e.page_geometries();
    let (page, g) = geos.first().expect("a page");
    let l = g.lines.iter().find(|l| l.paras.len() == 2).expect("a line showing two paragraphs");
    let (a, b) = (l.paras[0], l.paras[1]);
    let len = e.index().para(e.document(), a).map_or(0, |p| p.inlines.iter().map(Inline::units).sum::<usize>()) as u32;
    let end = l.stops.iter().find(|s| s.para == a && s.unit == Unit(len)).expect("the first paragraph's end on the line");
    let start = l.stops.iter().find(|s| s.para == b && s.unit == Unit(0)).expect("the next paragraph's start on the line");
    assert!((end.x - start.x).abs() < 0.01, "the paragraphs meet at one x: {} and {}", end.x, start.x);
    let caret = |para, u| g.caret(para, Unit(u), Affinity::Downstream).expect("a caret place").x;
    assert!((caret(a, len) - caret(b, 0)).abs() < 0.01);
    let y = (l.top + l.bottom) / 2.0;
    let hit = |x: f64| g.hit_text(x, y, &|s| s == StoryId::Body).map(|h| (h.para, h.unit));
    assert_eq!(hit(end.x + 1.0), Some((b, Unit(0))), "just past the join");
    assert_eq!(hit(end.x - 1.0), Some((a, Unit(len))), "just before the join");
    let rects = e.range_rects(Position::text(StoryId::Body, a, Unit(len - 3)), Position::text(StoryId::Body, b, Unit(3)), *page..*page + 1);
    let on_line: Vec<[f64; 4]> = rects.iter().map(|r| r.1).filter(|r| r[1] >= l.top - 0.01 && r[3] <= l.bottom + 0.01).collect();
    assert_eq!(on_line.len(), 2, "one band per paragraph: {on_line:?}");
    assert!((on_line[0][2] - on_line[1][0]).abs() < 0.01, "the bands meet at the join, with no mark between: {on_line:?}");
}

/// A paragraph's text as `view` shows it.
fn shown_text(p: &Paragraph, view: MarkupView) -> String {
    p.inlines.iter().filter_map(|i| if let Inline::Text(t, r) = i { (!view.hides(&r.rev)).then_some(t.as_str()) } else { None }).collect()
}

/// The body's paragraphs in document order, cells' included.
fn paragraphs(blocks: &[Block], out: &mut Vec<Paragraph>) {
    for b in blocks {
        match b {
            Block::Para(p) => out.push(p.clone()),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| paragraphs(&c.blocks, out)),
        }
    }
}

/// Each view's page count and lines per paragraph equal the expected layout's, for the paragraphs it maps.
#[test]
fn views_match_expected_layouts() {
    let fonts = Fonts::system();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/review/expected");
    for name in ["FX-REVIEW-RUNS", "FX-REVIEW-TABLES", "FX-REVIEW-SECTIONS", "FX-REVIEW-MOVES", "FX-REVIEW-MARKS"] {
        let Ok(text) = std::fs::read_to_string(dir.join(format!("views-{name}.json"))) else {
            eprintln!("skipped: no expected layout views-{name}.json");
            continue;
        };
        let expected: Value = serde_json::from_str(&text).expect("expected layout JSON");
        for (key, view) in [
            ("all-markup", MarkupView::All),
            ("simple-markup", MarkupView::Simple),
            ("no-markup", MarkupView::NoMarkup),
            ("original", MarkupView::Original),
        ] {
            let e = &expected["views"][key];
            let Some(pages) = e["pages"].as_u64() else { continue };
            let d = in_view(fixture(name), view);
            let snapshot = crate::layout::layout(&d, &fonts);
            assert_eq!(snapshot.pages().len() as u64, pages, "{name} in {view:?}: pages");
            let mut lines: HashMap<u64, std::collections::HashSet<(usize, u32)>> = HashMap::new();
            for (pi, page) in snapshot.pages().iter().enumerate() {
                paint(page, &mut |p| {
                    if let Item::Glyphs(r) = p.item
                        && let Some(para) = r.para
                    {
                        lines.entry(para.0).or_default().insert((pi, p.line));
                    }
                });
            }
            let mut ours = Vec::new();
            for s in &d.sections {
                paragraphs(&s.blocks, &mut ours);
            }
            let mut at = 0;
            for p in e["paragraphs"].as_array().into_iter().flatten() {
                // Empty paragraphs can't be told apart by their text (a row's end counts as one there).
                let (Some(want), Some(prefix)) = (p["lines"].as_u64(), p["text"].as_str().filter(|t| !t.is_empty())) else { continue };
                let Some(k) = (at..ours.len()).find(|&k| shown_text(&ours[k], view).starts_with(prefix)) else {
                    panic!("{name} in {view:?}: no paragraph starting {prefix:?}");
                };
                // A paragraph whose mark the view hides lays out with the next one (REV-6): their lines count once.
                let mut end = k;
                while end + 1 < ours.len() && view.hides(&ours[end].mark.rev) {
                    end += 1;
                }
                at = end + 1;
                let joined: std::collections::HashSet<_> = ours[k..=end].iter().filter_map(|p| lines.get(&p.id)).flatten().collect();
                let got = joined.len() as u64;
                assert_eq!(got, want, "{name} in {view:?}: lines of the paragraph starting {prefix:?}");
            }
        }
    }
}

/// Every filled rectangle drawn on page 1.
fn rects(d: &Document, fonts: &Fonts) -> Vec<([f64; 4], u32)> {
    let snapshot = crate::layout::layout(d, fonts);
    let mut out = Vec::new();
    paint(&snapshot.pages()[0], &mut |p| {
        if let Item::Rect { rect, colour } = p.absolute() {
            out.push((rect, colour));
        }
    });
    out
}

fn glyph_colours(d: &Document, fonts: &Fonts) -> Vec<Option<u32>> {
    snapshot_colours(&crate::layout::layout(d, fonts))
}

fn snapshot_colours(snapshot: &Snapshot) -> Vec<Option<u32>> {
    let mut out = Vec::new();
    paint(&snapshot.pages()[0], &mut |p| {
        if let Item::Glyphs(r) = p.item {
            out.push(r.colour);
        }
    });
    out
}

/// Change bars: 0.72 wide, black, `min(36, left margin / 2)` left of the body.
fn bars(rs: &[([f64; 4], u32)], left: f64) -> Vec<[f64; 4]> {
    rs.iter().filter(|(r, c)| *c == 0 && (r[2] - r[0] - 0.72).abs() < 1e-9 && r[2] < left).map(|(r, _)| *r).collect()
}

#[test]
fn all_markup_draws_revision_marks_colours_and_change_bars() {
    let fonts = Fonts::system();
    let author = AuthorPalette::DEFAULT[0];
    for name in ["FX-REVIEW-RUNS", "FX-REVIEW-TABLES", "FX-REVIEW-MOVES"] {
        let d = fixture(name);
        let left = d.sections[0].left;
        let all = rects(&in_view(d.clone(), MarkupView::All), &fonts);
        let simple = rects(&in_view(d.clone(), MarkupView::Simple), &fonts);
        let x = left - (left / 2.0).min(36.0);
        for (view, b) in [("All", bars(&all, left)), ("Simple", bars(&simple, left))] {
            assert!(!b.is_empty(), "{name}: change bars in {view} markup");
            assert!(b.iter().all(|r| (r[0] - x).abs() < 1e-9), "{name}: bars at {x}: {b:?}");
            assert!(b.windows(2).all(|w| w[1][1] > w[0][3] + 0.01), "{name}: consecutive bars join: {b:?}");
        }
        let none = rects(&in_view(d.clone(), MarkupView::NoMarkup), &fonts);
        assert!(bars(&none, left).is_empty(), "{name}: no change bars in No markup");
        let colours = glyph_colours(&in_view(d.clone(), MarkupView::All), &fonts);
        let plain = glyph_colours(&in_view(d.clone(), MarkupView::Simple), &fonts);
        match name {
            "FX-REVIEW-MOVES" => {
                assert!(colours.contains(&Some(AuthorPalette::MOVE)), "{name}: moves in the move colour");
                // Double underline and double strike: four rules per moved run, in the move colour.
                assert!(all.iter().filter(|(_, c)| *c == AuthorPalette::MOVE).count() >= 4, "{name}: move marks");
            }
            "FX-REVIEW-TABLES" => {
                // A quarter of the inserted row's author's colour over white.
                let Some(Block::Table(t)) = d.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))) else { panic!("{name}: a table") };
                let by = t.rows.iter().find_map(|r| r.rev.ins.as_ref()).map(|m| m.author).expect("an inserted row");
                let colour = d.review.palette.color(by);
                let tint = |s: u32| ((((colour >> s) & 0xFF) + 3 * 0xFF) / 4) << s;
                let tint = tint(16) | tint(8) | tint(0);
                assert!(all.iter().any(|(_, c)| *c == tint), "{name}: inserted and deleted rows tinted");
                assert!(!simple.iter().any(|(_, c)| *c == tint), "{name}: no tint in Simple markup");
            }
            _ => {
                assert!(colours.contains(&Some(author)), "{name}: revised text in its author's colour");
                assert!(all.iter().filter(|(_, c)| *c == author).count() >= 2, "{name}: underlines and strikethrough");
            }
        }
        assert!(!plain.iter().any(|c| *c == Some(author) || *c == Some(AuthorPalette::MOVE)), "{name}: Simple markup draws text plain");
        assert!(!simple.iter().any(|(_, c)| *c == author || *c == AuthorPalette::MOVE), "{name}: Simple markup draws no revision marks");
    }
}

/// The change bars of FX-REVIEW-MOVES (a page without balloons) where the expected export draws them.
#[test]
fn change_bars_match_the_expected_export() {
    let fonts = Fonts::system();
    let Ok(text) = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/review/expected/change-bars.json")) else {
        eprintln!("skipped: no change-bars.json");
        return;
    };
    let v: Value = serde_json::from_str(&text).expect("change bars JSON");
    let f = &v["fixtures"]["FX-REVIEW-MOVES.docx"];
    let page_h = f["pages"][0]["mediabox"][3].as_f64().unwrap_or(792.0);
    let want: Vec<[f64; 3]> = f["pages"][0]["bars"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| [b["x0"].as_f64().unwrap_or(0.0), page_h - b["top"].as_f64().unwrap_or(0.0), page_h - b["bottom"].as_f64().unwrap_or(0.0)])
        .collect();
    let d = fixture("FX-REVIEW-MOVES");
    let got = bars(&rects(&d, &fonts), d.sections[0].left);
    assert_eq!(got.len(), want.len(), "bars: {got:?}, expected {want:?}");
    for (g, w) in got.iter().zip(&want) {
        assert!((g[0] - w[0]).abs() < 0.13, "bar x {} expected {}", g[0], w[0]);
        // Line heights differ with the fonts installed: the extents are compared loosely.
        assert!((g[1] - w[1]).abs() < 3.0 && (g[3] - w[2]).abs() < 3.0, "bar {g:?} expected {w:?}");
    }
}

/// The host's view and colour commands lay the document out again, and aren't edits.
#[test]
fn view_commands_relayout_without_editing() {
    use crate::editing::Editor;
    use crate::editing::commands::{Command, Outcome};
    let fonts = Fonts::system();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-RUNS.docx");
    let mut e = Editor::open(&std::fs::read(p).expect("the fixture"), &fonts).expect("opens");
    let run = |e: &mut Editor, cmd: Command| {
        assert!(!cmd.mutates(), "{cmd:?} isn't an edit");
        e.execute(cmd, &fonts)
    };
    use crate::docx::review::MarkupView;
    let color =
        |author: &str| Command::SetAuthorColor(crate::api::command::AuthorColor { author: author.into(), color: crate::api::value::Color(0x112233) });
    assert_eq!(run(&mut e, Command::SetMarkupView(MarkupView::NoMarkup)), Outcome::Moved);
    assert_eq!(run(&mut e, Command::SetMarkupView(MarkupView::NoMarkup)), Outcome::Unchanged("already in that view"));
    e.complete(&fonts);
    let accepted = lines(&fixture("FX-REVIEW-RUNS-ACCEPTED"), &fonts);
    assert_eq!(snapshot_lines(&e.snapshot()), accepted, "No markup lays out as accepted");
    assert!(!e.is_dirty() && e.history().undo_depth() == 0, "a view change is no edit");
    run(&mut e, Command::SetMarkupView(MarkupView::All));
    let name = e.document().people.persons[0].name.to_string();
    assert_eq!(run(&mut e, color(&name)), Outcome::Moved);
    assert_eq!(e.document().review.palette.color(crate::docx::review::AuthorId(0)), 0x112233);
    e.complete(&fonts);
    assert!(snapshot_colours(&e.snapshot()).contains(&Some(0x112233)), "the author's revisions in the chosen colour");
    assert!(matches!(run(&mut e, color("Nobody")), Outcome::Refused(_)));
}
