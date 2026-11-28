//! Markup views

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::docx::review::{AuthorPalette, MarkupView, ReviewView};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::Snapshot;
use crate::layout::fragment::Item;
use crate::layout::traverse::paint;

fn fixture(name: &str) -> Document {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review").join(format!("{name}.docx"));
    crate::docx::read(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("a review fixture")
}

/// What a test lays out: a document in a view.
trait Shown {
    fn doc(&self) -> &Document;
    fn view(&self) -> ReviewView;
}

/// A document in the view it opens in.
impl Shown for Document {
    fn doc(&self) -> &Document {
        self
    }

    fn view(&self) -> ReviewView {
        crate::layout::tests::view(self)
    }
}

/// A document in a view of its own.
struct Viewed {
    doc: Document,
    view: ReviewView,
}

impl Shown for Viewed {
    fn doc(&self) -> &Document {
        &self.doc
    }

    fn view(&self) -> ReviewView {
        self.view.clone()
    }
}

impl From<Document> for Viewed {
    fn from(doc: Document) -> Viewed {
        Viewed { view: crate::layout::tests::view(&doc), doc }
    }
}

fn in_view(d: impl Into<Viewed>, view: MarkupView) -> Viewed {
    let mut d = d.into();
    d.view.view = view;
    d
}

fn layout(d: &dyn Shown, fonts: &Fonts) -> Snapshot {
    crate::layout::layout(d.doc(), &d.view(), fonts)
}

/// Per page, each drawn line's baseline, start and text (to 0.01 pt): what a line box is, without the decorations and
/// change bars drawn beside it.
fn lines(d: &dyn Shown, fonts: &Fonts) -> Vec<Vec<(i64, i64, String)>> {
    snapshot_lines(&layout(d, fonts))
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
    let fonts = Fonts::system_recommended();
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
    assert_same_lines("FX-REVIEW-TABLES", MarkupView::Original, "FX-REVIEW-TABLES-REJECTED", &Fonts::system_recommended());
}

#[test]
fn paragraph_marks_by_view() {
    let fonts = Fonts::system_recommended();
    assert_same_lines("FX-REVIEW-MARKS", MarkupView::NoMarkup, "FX-REVIEW-MARKS-ACCEPTED", &fonts);
    assert_same_lines("FX-REVIEW-MARKS", MarkupView::Original, "FX-REVIEW-MARKS-REJECTED", &fonts);
}

#[test]
fn joined_paragraphs_share_their_line_for_editing() {
    use crate::editing::Editor;
    use crate::editing::commands::Command;
    use crate::editing::position::{Affinity, Position, StoryId};
    use crate::layout::Unit;
    let fonts = Fonts::system_recommended();
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
    let on_line: Vec<[f64; 4]> = rects.iter().map(|r| r.1.page()).filter(|r| r[1] >= l.top - 0.01 && r[3] <= l.bottom + 0.01).collect();
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
    let fonts = Fonts::system_recommended();
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
            let snapshot = layout(&d, &fonts);
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
            for s in &d.doc.sections {
                paragraphs(&s.blocks, &mut ours);
            }
            let mut at = 0;
            for p in e["paragraphs"].as_array().into_iter().flatten() {
                // Empty paragraphs can't be told apart by their text (a row's end counts as one there).
                let (Some(want), Some(prefix)) = (p["lines"].as_u64(), p["text"].as_str().filter(|t| !t.is_empty())) else { continue };
                let Some(k) = (at..ours.len()).find(|&k| shown_text(&ours[k], view).starts_with(prefix)) else {
                    panic!("{name} in {view:?}: no paragraph starting {prefix:?}");
                };
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
fn rects(d: &dyn Shown, fonts: &Fonts) -> Vec<([f64; 4], u32)> {
    let snapshot = layout(d, fonts);
    let mut out = Vec::new();
    paint(&snapshot.pages()[0], &mut |p| {
        if let Item::Rect { rect, colour, .. } = p.absolute() {
            out.push((rect, colour));
        }
    });
    out
}

fn glyph_colours(d: &dyn Shown, fonts: &Fonts) -> Vec<Option<u32>> {
    snapshot_colours(&layout(d, fonts))
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
    let auto = [crate::docx::review::ReviewView::SIMPLE_BAR, crate::docx::review::ReviewView::MARKUP_BAR];
    rs.iter().filter(|(r, c)| auto.contains(c) && (r[2] - r[0] - 0.72).abs() < 1e-9 && r[2] < left).map(|(r, _)| *r).collect()
}

#[test]
fn all_markup_draws_revision_marks_colours_and_change_bars() {
    let fonts = Fonts::system_recommended();
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
                // Word's default cell highlights: Light Blue inserted, Pink deleted.
                let tint = crate::docx::review::MarkupStyle::LIGHT_BLUE;
                assert!(all.iter().any(|(_, c)| *c == tint), "{name}: inserted rows highlighted");
                assert!(!simple.iter().any(|(_, c)| *c == tint), "{name}: no highlight in Simple markup");
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
    let fonts = Fonts::system_recommended();
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
        // Word drew the bars with its own fonts, not the test kit: the extents are compared loosely.
        assert!((g[1] - w[1]).abs() < 3.0 && (g[3] - w[2]).abs() < 3.0, "bar {g:?} expected {w:?}");
    }
}

/// The host's view and colour commands lay the document out again, and aren't edits.
#[test]
fn view_commands_relayout_without_editing() {
    use crate::editing::Editor;
    use crate::editing::commands::{Command, Outcome};
    let fonts = Fonts::system_recommended();
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
    assert_eq!(e.review().palette.color(crate::docx::review::AuthorId(0)), 0x112233);
    e.complete(&fonts);
    assert!(snapshot_colours(&e.snapshot()).contains(&Some(0x112233)), "the author's revisions in the chosen colour");
    assert!(matches!(run(&mut e, color("Nobody")), Outcome::Refused(_)));
}

/// `d` with its markup's view settings changed.
fn with_markup(d: impl Into<Viewed>, change: impl FnOnce(&mut crate::docx::review::Markup)) -> Viewed {
    let mut d = d.into();
    change(std::rc::Rc::make_mut(&mut d.view.markup));
    d
}

/// Insertions, deletions and formatting left out of Show Markup show as accepted: laid out as accepted, drawn plain,
/// with no change bars; each kind alone leaves the other's marks.
#[test]
fn show_markup_kinds_show_left_out_revisions_as_accepted() {
    let fonts = Fonts::system_recommended();
    let author = AuthorPalette::DEFAULT[0];
    let d = fixture("FX-REVIEW-RUNS");
    let left = d.sections[0].left;
    let none = with_markup(d.clone(), |m| (m.show.insertions_deletions, m.show.formatting) = (false, false));
    assert_eq!(lines(&none, &fonts), lines(&fixture("FX-REVIEW-RUNS-ACCEPTED"), &fonts), "laid out as accepted");
    assert!(!glyph_colours(&none, &fonts).contains(&Some(author)), "drawn plain");
    assert!(bars(&rects(&none, &fonts), left).is_empty(), "no change bars");
    let formatting_only = with_markup(d.clone(), |m| m.show.insertions_deletions = false);
    assert_eq!(lines(&formatting_only, &fonts), lines(&none, &fonts), "deletions gone with insertions and deletions left out");
    let ins_del = with_markup(d.clone(), |m| m.show.formatting = false);
    assert!(glyph_colours(&ins_del, &fonts).contains(&Some(author)), "insertions and deletions still marked");
    assert_eq!(lines(&ins_del, &fonts), lines(&d, &fonts), "formatting left out moves nothing");
    // Simple Markup: the kinds left out draw no change bars.
    let simple = in_view(none, MarkupView::Simple);
    assert!(bars(&rects(&simple, &fonts), left).is_empty(), "no change bars in Simple markup either");
}

/// A hidden reviewer's revisions show as accepted; everyone else's keep their marks.
#[test]
fn hidden_reviewers_show_their_revisions_as_accepted() {
    let fonts = Fonts::system_recommended();
    for name in ["FX-REVIEW-RUNS", "FX-REVIEW-TABLES", "FX-REVIEW-MOVES"] {
        let d = fixture(name);
        let everyone = (0..d.people.persons.len() as u32).map(crate::docx::review::AuthorId).collect::<Vec<_>>();
        let hidden = with_markup(d.clone(), |m| m.show.hidden_reviewers = everyone.clone());
        let accepted = in_view(d.clone(), MarkupView::NoMarkup);
        assert_eq!(lines(&hidden, &fonts), lines(&accepted, &fonts), "{name}: every reviewer hidden lays out as accepted");
        let marks = |d: &dyn Shown| {
            let palette = &d.view().palette;
            let tint = |colour: u32| {
                let m = |s: u32| ((((colour >> s) & 0xFF) + 3 * 0xFF) / 4) << s;
                m(16) | m(8) | m(0)
            };
            use crate::docx::review::MarkupStyle as S;
            let fixed = [AuthorPalette::MOVE, S::LIGHT_BLUE, S::PINK, S::LIGHT_YELLOW, S::LIGHT_ORANGE];
            let colours: Vec<u32> = everyone.iter().flat_map(|&a| [palette.color(a), tint(palette.color(a))]).chain(fixed).collect();
            let glyphs = glyph_colours(d, &fonts).into_iter().flatten().filter(|c| colours.contains(c)).count();
            glyphs + rects(d, &fonts).iter().filter(|(_, c)| colours.contains(c)).count()
        };
        assert!(marks(&d) > 0, "{name}: marked with everyone shown");
        assert_eq!(marks(&hidden), 0, "{name}: no marks with every reviewer hidden");
        assert!(bars(&rects(&hidden, &fonts), d.sections[0].left).is_empty(), "{name}: no change bars");
    }
}

/// The editor's Show Markup and reviewer settings are view settings: no edit, laid out again, refused for no reviewer.
#[test]
fn show_markup_commands_are_view_settings() {
    use crate::editing::Editor;
    use crate::editing::commands::Outcome;
    let fonts = Fonts::system_recommended();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-RUNS.docx");
    let mut e = Editor::open(&std::fs::read(p).expect("the fixture"), &fonts).expect("opens");
    assert_eq!(e.set_show_markup(false, false, true), Outcome::Moved);
    assert_eq!(e.set_show_markup(false, false, true), Outcome::Unchanged("already shown so"));
    e.complete(&fonts);
    assert_eq!(snapshot_lines(&e.snapshot()), lines(&fixture("FX-REVIEW-RUNS-ACCEPTED"), &fonts));
    assert!(!e.is_dirty() && e.history().undo_depth() == 0, "a view change is no edit");
    assert_eq!(e.set_show_markup(true, true, true), Outcome::Moved);
    let names: Vec<String> = e.document().people.persons.iter().map(|p| p.name.to_string()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(e.set_hidden_reviewers(&refs), Outcome::Moved);
    e.complete(&fonts);
    assert_eq!(snapshot_lines(&e.snapshot()), lines(&fixture("FX-REVIEW-RUNS-ACCEPTED"), &fonts));
    assert!(matches!(e.set_hidden_reviewers(&["Nobody"]), Outcome::Refused(_)));
    assert_eq!(e.set_hidden_reviewers(&[]), Outcome::Moved);
    assert!(e.review().markup.show.hidden_reviewers.is_empty());
}

/// Balloons: deletions leave the line for their balloons, insertions stay marked in place.
#[test]
fn balloon_mode_takes_deletions_out_of_the_line() {
    use crate::docx::review::BalloonMode;
    let fonts = Fonts::system_recommended();
    let author = AuthorPalette::DEFAULT[0];
    let d = fixture("FX-REVIEW-RUNS");
    let balloons = with_markup(d.clone(), |m| m.balloons = BalloonMode::Balloons);
    assert_eq!(lines(&balloons, &fonts), lines(&in_view(d.clone(), MarkupView::Simple), &fonts), "deleted text gone from the lines");
    assert!(glyph_colours(&balloons, &fonts).contains(&Some(author)), "insertions still marked");
    let inline = with_markup(d.clone(), |m| m.balloons = BalloonMode::Inline);
    assert_eq!(lines(&inline, &fonts), lines(&d, &fonts), "inline keeps deletions in place");
}

/// Track Changes Options: each kind's mark and colour, change bars' side and colour, cells' highlight.
#[test]
fn markup_style_draws_each_kind_as_set() {
    use crate::docx::review::{BarSide, MarkColor, MarkLook, TextMark};
    let fonts = Fonts::system_recommended();
    let d = fixture("FX-REVIEW-RUNS");
    let left = d.sections[0].left;
    // Colours
    let fixed = with_markup(d.clone(), |m| {
        m.style.insertions.color = MarkColor::Fixed(0x123456);
        m.style.deletions.color = MarkColor::Fixed(0x123456);
    });
    assert!(glyph_colours(&fixed, &fonts).contains(&Some(0x123456)), "a fixed colour");
    let auto = with_markup(d.clone(), |m| {
        m.style.insertions.color = MarkColor::Auto;
        m.style.deletions.color = MarkColor::Auto;
    });
    assert!(!glyph_colours(&auto, &fonts).contains(&Some(AuthorPalette::DEFAULT[0])), "automatic: the text's own colour");
    // No mark: no rules and no colour, laid out as before.
    let plain = with_markup(d.clone(), |m| {
        m.style.insertions = MarkLook { mark: TextMark::None, color: MarkColor::ByAuthor };
        m.style.deletions = MarkLook { mark: TextMark::None, color: MarkColor::ByAuthor };
    });
    assert_eq!(lines(&plain, &fonts), lines(&d, &fonts));
    assert!(!glyph_colours(&plain, &fonts).contains(&Some(AuthorPalette::DEFAULT[0])));
    // Hidden deletions leave the line; a caret stands in for each.
    let hidden = with_markup(d.clone(), |m| m.style.deletions.mark = TextMark::Hidden);
    assert_eq!(lines(&hidden, &fonts), lines(&in_view(d.clone(), MarkupView::Simple), &fonts));
    let caret = with_markup(d.clone(), |m| m.style.deletions.mark = TextMark::Caret);
    let text = |d: &dyn Shown| lines(d, &fonts).concat().into_iter().map(|l| l.2).collect::<String>();
    let carets = text(&caret).matches('^').count() - text(&d).matches('^').count();
    assert!(carets > 0, "a caret for each deletion");
    assert_eq!(text(&caret).replace('^', ""), text(&hidden).replace('^', ""), "the deleted text gone");
    // Bold insertions lay out bold.
    let bold = with_markup(d.clone(), |m| m.style.insertions.mark = TextMark::Bold);
    assert_ne!(lines(&bold, &fonts), lines(&d, &fonts), "bold insertions are wider");
    // Change bars on the right, in their colour; none.
    let right = with_markup(d.clone(), |m| (m.style.change_bars, m.style.change_bar_color) = (BarSide::Right, Some(0xFF0000)));
    let page_right = d.sections[0].page_w - d.sections[0].right;
    let red: Vec<[f64; 4]> =
        rects(&right, &fonts).into_iter().filter(|(r, c)| *c == 0xFF0000 && (r[2] - r[0] - 0.72).abs() < 1e-9).map(|(r, _)| r).collect();
    assert!(!red.is_empty() && red.iter().all(|r| r[0] > page_right), "bars right of the text: {red:?}");
    assert!(bars(&rects(&right, &fonts), left).is_empty(), "none left of it");
    // Outside: the right of an odd page with mirrored margins.
    let mut mirrored = d.clone();
    mirrored.settings.mirror_margins = true;
    let auto_bars = |d: &dyn Shown, colour: u32| {
        rects(d, &fonts).into_iter().filter(|(r, c)| *c == colour && (r[2] - r[0] - 0.72).abs() < 1e-9).map(|(r, _)| r).collect::<Vec<_>>()
    };
    let outside = auto_bars(&mirrored, crate::docx::review::ReviewView::MARKUP_BAR);
    assert!(!outside.is_empty() && outside.iter().all(|r| r[0] > page_right), "page 1's outside is its right: {outside:?}");
    // Automatic: grey in All Markup, red in Simple Markup.
    let simple = in_view(d.clone(), MarkupView::Simple);
    assert!(!auto_bars(&d, crate::docx::review::ReviewView::MARKUP_BAR).is_empty(), "grey in All Markup");
    assert!(!auto_bars(&simple, crate::docx::review::ReviewView::SIMPLE_BAR).is_empty(), "red in Simple Markup");
    assert!(auto_bars(&simple, crate::docx::review::ReviewView::MARKUP_BAR).is_empty());
    let no_bars = with_markup(d.clone(), |m| m.style.change_bars = BarSide::None);
    assert!(bars(&rects(&no_bars, &fonts), left).is_empty());
    // Cells: a fixed highlight.
    let t = with_markup(fixture("FX-REVIEW-TABLES"), |m| m.style.inserted_cells = MarkColor::Fixed(0xABCDEF));
    assert!(rects(&t, &fonts).iter().any(|(_, c)| *c == 0xABCDEF), "the inserted row highlighted");
    let t = with_markup(fixture("FX-REVIEW-TABLES"), |m| (m.style.inserted_cells, m.style.deleted_cells) = (MarkColor::Auto, MarkColor::Auto));
    let base = fixture("FX-REVIEW-TABLES");
    let by = base.people.persons.len() as u32;
    let tints: Vec<u32> = (0..by).map(|a| base.view().palette.color(crate::docx::review::AuthorId(a))).collect();
    let tint = |colour: u32| {
        let m = |s: u32| ((((colour >> s) & 0xFF) + 3 * 0xFF) / 4) << s;
        m(16) | m(8) | m(0)
    };
    let (light_blue, pink) = (crate::docx::review::MarkupStyle::LIGHT_BLUE, crate::docx::review::MarkupStyle::PINK);
    let auto = rects(&t, &fonts);
    assert!(!auto.iter().any(|(_, c)| tints.iter().any(|&a| tint(a) == *c) || *c == light_blue || *c == pink), "automatic: no highlight");
    assert!(rects(&base, &fonts).iter().any(|(_, c)| *c == light_blue), "Word's Light Blue by default");
    assert!(rects(&base, &fonts).iter().any(|(_, c)| *c == pink), "and its Pink for the deleted cells");
    let by_author = with_markup(fixture("FX-REVIEW-TABLES"), |m| m.style.inserted_cells = MarkColor::ByAuthor);
    let drawn = rects(&by_author, &fonts);
    assert!(drawn.iter().any(|(_, c)| tints.iter().any(|&a| tint(a) == *c)) && !drawn.iter().any(|(_, c)| *c == light_blue), "by author: tinted");
}

/// A tracked split cell (its vertical merge undone) fills with the split cells' colour, Word's Light Orange by
/// default; a merged one with the merged cells'.
#[test]
fn split_cells_have_their_own_highlight() {
    use crate::docx::VMerge;
    use crate::docx::review::{CellMarkKind, MarkColor, MarkupStyle as S};
    let fonts = Fonts::system_recommended();
    let merged = fixture("FX-REVIEW-CELLS");
    let has = |d: &dyn Shown, colour: u32| rects(d, &fonts).iter().any(|(_, c)| *c == colour);
    assert!(has(&merged, S::LIGHT_YELLOW) && !has(&merged, S::LIGHT_ORANGE), "a merged cell in Light Yellow");
    let mut split = merged.clone();
    let mut found = 0;
    for b in &mut split.sections[0].blocks {
        let Block::Table(t) = b else { continue };
        for c in t.rows.iter_mut().flat_map(|r| &mut r.cells) {
            if let Some((_, k @ CellMarkKind::Merge { .. })) = &mut c.rev.structural {
                *k = CellMarkKind::Merge { v_merge: Some(VMerge::Restart), v_merge_orig: Some(VMerge::Continue) };
                c.v_merge = VMerge::None;
                found += 1;
            }
        }
    }
    assert_eq!(found, 1, "the fixture's merged cell");
    assert!(has(&split, S::LIGHT_ORANGE) && !has(&split, S::LIGHT_YELLOW), "a split cell in Light Orange");
    let fixed = with_markup(split, |m| m.style.split_cells = MarkColor::Fixed(0x123456));
    assert!(has(&fixed, 0x123456), "the split cells' colour set");
}

/// Balloons beside the pages: formatting changes in the default mode, deletions too in Balloons mode, none inline or
/// outside All Markup; only the kinds and reviewers shown. The pane's counts by kind, all and shown.
#[test]
fn revision_balloons_and_counts_follow_the_view() {
    use crate::docx::review::BalloonMode;
    use crate::editing::Editor;
    use crate::editing::revisions::index::RevisionKind as K;
    let fonts = Fonts::system_recommended();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-RUNS.docx");
    let mut e = Editor::open(&std::fs::read(p).expect("the fixture"), &fonts).expect("opens");
    e.complete(&fonts);
    let pages = e.snapshot().pages().len();
    let kinds = |e: &mut Editor| e.view(0..pages).revision_balloons.iter().map(|b| b.kind).collect::<Vec<_>>();
    let list = crate::editing::revisions::index::revisions(e.document(), e.index());
    let formatting = list.iter().filter(|r| r.kind.is_formatting()).count();
    let deletions = list.iter().filter(|r| matches!(r.kind, K::Delete | K::MovedFrom | K::CellDeletion)).count();
    assert!(formatting > 0 && deletions > 0, "the fixture has both");
    let default = kinds(&mut e);
    assert_eq!(default.len(), formatting, "formatting changes in balloons by default");
    assert!(default.iter().all(|k| k.is_formatting()));
    e.set_balloon_mode(BalloonMode::Balloons);
    e.complete(&fonts);
    assert_eq!(kinds(&mut e).len(), formatting + deletions, "deletions too in Balloons mode");
    let b = e.view(0..pages).revision_balloons;
    assert!(b.iter().all(|b| b.at[1] < b.at[2] && b.page < pages), "anchored on a line: {b:?}");
    e.set_show_markup(true, false, true);
    assert_eq!(kinds(&mut e).len(), deletions, "formatting left out");
    let (all, shown) = e.review_counts();
    assert_eq!(all.revisions() as usize, list.len());
    assert_eq!(all.formatting as usize, formatting);
    assert_eq!(shown.formatting, 0);
    assert_eq!(shown.revisions(), all.revisions() - all.formatting);
    e.set_show_markup(true, true, true);
    e.set_balloon_mode(BalloonMode::Inline);
    assert!(kinds(&mut e).is_empty(), "inline: no balloons");
    e.set_balloon_mode(BalloonMode::CommentsAndFormatting);
    e.set_markup_view(MarkupView::Simple);
    assert!(kinds(&mut e).is_empty(), "Simple markup: no balloons");
    let (all, shown) = e.review_counts();
    assert_eq!(all, shown, "everything shown");
}

/// With deletions shown as carets the caret walks the text as it shows: every place has a caret and it never goes
/// back.
#[test]
fn the_caret_walks_text_with_stand_in_deletions() {
    use crate::docx::review::TextMark;
    use crate::editing::Editor;
    use crate::editing::commands::{Dir, MoveBy, moving};
    let fonts = Fonts::system_recommended();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-RUNS.docx");
    let mut e = Editor::open(&std::fs::read(p).expect("the fixture"), &fonts).expect("opens");
    let mut style = e.review().markup.style.clone();
    for mark in [TextMark::Hidden, TextMark::Caret, TextMark::Pound] {
        let mut wrong = style.clone();
        wrong.insertions.mark = mark;
        assert!(matches!(e.set_markup_style(wrong), crate::editing::commands::Outcome::Refused(_)), "{mark:?} is for deletions");
    }
    style.deletions.mark = TextMark::Caret;
    assert_eq!(e.set_markup_style(style), crate::editing::commands::Outcome::Moved);
    e.complete(&fonts);
    // The story's end, where the walk stops.
    let start = e.selection();
    e.execute(moving(MoveBy::Story, Dir::Forward, false), &fonts);
    let end = e.selection().focus();
    e.set_caret_for_benchmark(start);
    let mut last = None;
    let mut done = false;
    for _ in 0..20_000 {
        assert!(e.caret_view().is_some(), "a caret at {:?}", e.selection());
        let at = e.selection().focus();
        if at == last {
            assert_eq!(at, end, "stopped only at the end");
            done = true;
            break;
        }
        if let (Some(a), Some(b)) = (last, at) {
            assert!(e.index().cmp(&a, &b).is_lt(), "forward from {a:?} to {b:?}");
        }
        last = at;
        e.execute(moving(MoveBy::Char, Dir::Forward, false), &fonts);
    }
    assert!(done, "the walk reached the end");
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{v:?}");
}

/// A one-paragraph document whose settings part holds `settings`.
fn with_settings(settings: &str) -> Vec<u8> {
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let root = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#
    );
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/settings" Target="settings.xml"/></Relationships>"#
    );
    let body = format!(r#"<w:document xmlns:w="{W}"><w:body><w:p><w:r><w:t>Text.</w:t></w:r></w:p></w:body></w:document>"#);
    let settings = format!(r#"<w:settings xmlns:w="{W}">{settings}</w:settings>"#);
    crate::layout::tests::package_bytes(&[
        ("[Content_Types].xml", ct),
        ("_rels/.rels", &root),
        ("word/_rels/document.xml.rels", &rels),
        ("word/document.xml", &body),
        ("word/settings.xml", &settings),
    ])
    .expect("a package")
}

#[test]
fn show_markup_opens_as_the_document_saved_it() {
    let shown = |settings: &str| {
        let m = ReviewView::opening(&crate::docx::read(&with_settings(settings)).expect("reads").settings).markup.show.clone();
        (m.insertions_deletions, m.formatting, m.comments)
    };
    assert_eq!(shown(""), (true, true, true), "no revisionView: all shows");
    assert_eq!(shown(r#"<w:revisionView w:insDel="0" w:comments="false"/>"#), (false, true, false));
    assert_eq!(shown(r#"<w:revisionView w:formatting="off" w:insDel="1"/>"#), (true, false, true));
}

#[test]
fn white_space_opens_hidden_as_the_document_saved_it() {
    let fonts = crate::layout::tests::fonts();
    let open = |settings: &str| crate::editing::Editor::open(&with_settings(settings), &fonts).expect("opens");
    assert!(!open("").white_space_hidden());
    assert!(!open(r#"<w:doNotDisplayPageBoundaries w:val="0"/>"#).white_space_hidden());
    let mut e = open("<w:doNotDisplayPageBoundaries/>");
    assert!(e.white_space_hidden());
    e.set_white_space_hidden(false);
    let saved = crate::docx::write::save(e.document(), e.docx()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(saved)).expect("a package");
    let mut settings = String::new();
    std::io::Read::read_to_string(&mut z.by_name("word/settings.xml").expect("settings"), &mut settings).expect("reads");
    assert!(settings.contains("<w:doNotDisplayPageBoundaries/>"), "kept as read: {settings}");
}
