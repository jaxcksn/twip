//! Paste options, Paste Special, pictures from the clipboard and Text from File

use super::{blank_body, fonts, para_id, texts};
use crate::docx::{Block, Inline};
use crate::editing::Editor;
use crate::editing::commands::{Command, Outcome};
use crate::editing::paste::{Clip, PasteChoice, PasteDefaults};
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::{CellRef, CellShape, Granularity, Selection};
use crate::layout::Unit;

fn t(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

fn ed(body: &str) -> Editor {
    Editor::open(&blank_body(body), fonts()).expect("opens")
}

/// A document with `styles` (`w:style` elements) added to the blank one's.
fn with_styles(body: &str, styles: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let src = blank_body(body);
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).expect("a zip");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("an entry");
        let name = f.name().expect("a name").to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).expect("reads");
        if name == "word/styles.xml" {
            data = String::from_utf8(data).expect("utf-8").replacen("</w:styles>", &format!("{styles}</w:styles>"), 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).expect("starts");
        out.write_all(&data).expect("writes");
    }
    out.finish().expect("finishes").into_inner()
}

fn select(e: &mut Editor, a: (usize, u32), b: (usize, u32)) {
    let p = |e: &Editor, (n, u): (usize, u32)| Position::text(StoryId::Body, para_id(e, n), Unit(u));
    let (pa, pb) = (p(e, a), p(e, b));
    e.set_caret_for_benchmark(Selection::range(pa, pb, Granularity::Char));
}

fn caret(e: &mut Editor, a: (usize, u32)) {
    let p = Position::text(StoryId::Body, para_id(e, a.0), Unit(a.1));
    e.set_caret_for_benchmark(Selection::Caret(p));
}

fn png(w: u32, h: u32) -> Vec<u8> {
    crate::editing::wrap_points::tests::png(w, h, &vec![200u8; (w * h * 4) as usize])
}

fn base64(data: &[u8]) -> String {
    crate::api::value::base64(data)
}

fn pictures(e: &Editor) -> Vec<(f64, f64, Option<usize>)> {
    let mut out = Vec::new();
    for b in &e.document().sections[0].blocks {
        if let Block::Para(p) = b {
            out.extend(p.inlines.iter().filter_map(|i| if let Inline::Picture(p, _) = i { Some((p.w, p.h, p.image)) } else { None }));
        }
    }
    out
}

fn paras(e: &Editor) -> Vec<&crate::docx::Paragraph> {
    e.document().sections[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect()
}

fn changed(o: &Outcome) -> bool {
    matches!(o, Outcome::Changed(_))
}

fn image_clip(data: Vec<u8>) -> Clip {
    Clip { image: Some(data), ..Clip::default() }
}

#[test]
fn a_picture_on_the_clipboard_pastes_in_line_over_the_selection_as_one_step() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("abc def")));
    select(&mut e, (0, 4), (0, 7));
    let out = e.paste_clip(image_clip(png(96, 48)), None, fonts());
    assert!(changed(&out), "{out:?}");
    assert_eq!(pictures(&e), [(72.0, 36.0, Some(0))], "its own size, 96 px a 72 pt inch");
    assert_eq!(texts(&e)[0].chars().filter(|c| *c != crate::editing::text::OBJECT).collect::<String>(), "abc ", "the selection is replaced");
    assert_eq!(e.history().next_undo(), Some("Paste"));
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), ["abc def"], "one undo takes it back");
    assert!(pictures(&e).is_empty());
}

#[test]
fn a_picture_pasted_over_a_selected_picture_takes_its_place() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    caret(&mut e, (0, 1));
    assert!(changed(&e.paste_clip(image_clip(png(10, 10)), None, fonts())));
    let pic = crate::editing::selection::ObjectRef::Inline { para: para_id(&e, 0), offset: Unit(1) };
    e.set_selection(Selection::Object(pic));
    assert!(changed(&e.paste_clip(image_clip(png(20, 10)), None, fonts())));
    assert_eq!(pictures(&e).len(), 1, "{:?}", pictures(&e));
    assert_eq!(pictures(&e)[0].0, 15.0, "the new picture");
    assert_eq!(e.history().next_undo(), Some("Paste"));
}

#[test]
fn a_pasted_picture_leaves_the_caret_after_it_and_its_paste_options() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    caret(&mut e, (0, 1));
    assert!(changed(&e.paste_clip(image_clip(png(10, 10)), None, fonts())));
    assert_eq!(e.selection(), Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(2))), "the caret after it, as Word leaves it");
    let v = e.paste_options_view(0..1).expect("the Paste Options button");
    assert_eq!((v.current, v.choices.as_slice()), (PasteChoice::Picture, &[PasteChoice::Picture][..]));
    assert!(changed(&e.paste_clip(image_clip(png(10, 10)), None, fonts())));
    assert_eq!(pictures(&e).len(), 2, "pasted twice, two pictures");
    // Floating, as the default wrap makes it: the caret at its anchor
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    e.set_default_picture_wrap(crate::api::value::ObjectWrap::Square);
    caret(&mut e, (0, 1));
    assert!(changed(&e.paste_clip(image_clip(png(10, 10)), None, fonts())));
    assert!(matches!(e.selection(), Selection::Caret(_)), "{:?}", e.selection());
    assert!(e.paste_options_view(0..1).is_some());
}

#[test]
fn html_naming_only_a_remote_picture_pastes_the_clipboard_s_picture() {
    for html in [r#"<img src="https://x.test/p.png">"#, r#"<html><body><img src="https://x.test/p.png" alt="a cat"></body></html>"#] {
        let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
        caret(&mut e, (0, 1));
        let clip = Clip { html: Some(html.into()), image: Some(png(8, 8)), ..Clip::default() };
        assert_eq!(e.default_paste(&clip), Some(PasteChoice::Picture), "{html}");
        assert!(changed(&e.paste_clip(clip, None, fonts())));
        assert_eq!(pictures(&e).len(), 1, "{html}");
        assert_eq!(texts(&e)[0].chars().filter(|c| *c != crate::editing::text::OBJECT).collect::<String>(), "x", "no alternative text");
        assert_eq!(e.last_paste().map(|l| l.choice), Some(PasteChoice::Picture));
    }
    // Without a picture: the alternative text, as HTML
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    let clip = Clip { html: Some(r#"<img src="https://x.test/p.png" alt="cat">"#.into()), ..Clip::default() };
    assert_eq!(e.default_paste(&clip), Some(PasteChoice::KeepSource));
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    assert_eq!(texts(&e)[0], "x cat");
}

#[test]
fn html_that_pastes_nothing_gives_way_to_the_picture_or_the_text() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    let clip = Clip { html: Some("<html><body></body></html>".into()), text: "plain".into(), ..Clip::default() };
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    assert_eq!(texts(&e)[0], "x plain");
    assert_eq!(e.last_paste().map(|l| l.choice), Some(PasteChoice::Unformatted), "the choice taken");
    assert_eq!(e.history().next_undo(), Some("Paste"));
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), ["x"], "one undo step");
    // A remote picture alone, no alternative text, a picture and text on the clipboard: the picture
    let clip = Clip { html: Some(r#"<p><img src="https://x.test/p.png"></p><p>words</p>"#.into()), image: Some(png(8, 8)), ..Clip::default() };
    assert_eq!(e.default_paste(&clip), Some(PasteChoice::KeepSource), "it has text of its own");
    let clip = Clip { html: Some("<p></p>".into()), text: "t".into(), image: Some(png(8, 8)), ..Clip::default() };
    assert!(changed(&e.paste_clip(clip.clone(), None, fonts())));
    assert_eq!((pictures(&e).len(), e.last_paste().map(|l| l.choice)), (1, Some(PasteChoice::Picture)));
    // Told to paste the HTML, it pastes that or nothing
    e.execute(Command::Undo, fonts());
    assert!(matches!(e.paste_clip(clip, Some(PasteChoice::KeepSource), fonts()), Outcome::Unchanged(_)));
}

#[test]
fn a_wide_picture_is_scaled_to_the_text_width_and_a_cell_s_width() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    e.paste_clip(image_clip(png(2000, 1000)), None, fonts());
    let (w, h, _) = pictures(&e)[0];
    assert!((w - 468.0).abs() < 0.1 && (h - 234.0).abs() < 0.1, "{w} x {h}");
}

#[test]
fn text_on_the_clipboard_comes_before_its_picture_and_a_bad_picture_is_refused() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    let clip = Clip { text: "words".into(), image: Some(png(4, 4)), ..Clip::default() };
    assert_eq!(e.default_paste(&clip), Some(PasteChoice::TextOnly));
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    assert!(pictures(&e).is_empty() && texts(&e)[0] == "x words", "{:?}", texts(&e));
    let out = e.paste_clip(image_clip(b"\x89PNG\r\n\x1a\nbroken".to_vec()), None, fonts());
    assert!(matches!(out, Outcome::Refused(_)), "{out:?}");
    assert!(
        e.events.iter().any(|n| matches!(n, crate::editing::input::Notice::Warning(crate::docx::diagnostics::WarningCode::ImageUndecodable, _))),
        "the host is told"
    );
    let out = e.paste_clip(Clip { text: "t".into(), ..Clip::default() }, Some(PasteChoice::Picture), fonts());
    assert!(matches!(out, Outcome::Refused(_)), "no picture to paste: {out:?}");
}

#[test]
fn html_pictures_come_in_line_and_remote_ones_as_their_text() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    let html = format!(
        r#"<p>a<img src="data:image/png;base64,{}" width="40" height="20" alt="dot">b<img src="https://x.test/p.png" alt="far"></p>"#,
        base64(&png(8, 8))
    );
    assert!(changed(&e.paste_html(&html, fonts())));
    assert_eq!(pictures(&e), [(30.0, 15.0, Some(0))], "its given size, CSS px");
    assert_eq!(e.document().images.len(), 1, "its bytes are the document's");
    let text: String = texts(&e)[0].chars().filter(|c| *c != crate::editing::text::OBJECT).collect();
    assert_eq!(text, "x abfar", "a remote picture is never fetched: its alternative text stands in");
    let alt = paras(&e)[0].inlines.iter().find_map(|i| if let Inline::Picture(p, _) = i { Some(p.alt.to_string()) } else { None });
    assert_eq!(alt.as_deref(), Some("dot"));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(again.document().images.len(), 1, "the picture saves");
}

#[test]
fn a_data_url_that_is_not_a_picture_is_left_out() {
    let mut e = ed("<w:p/>");
    for src in ["data:text/html;base64,PGI+", "data:image/png;base64,!!!!", "data:image/png,rawbytes", "javascript:alert(1)"] {
        e.paste_html(&format!(r#"<p>a<img src="{src}">b</p>"#), fonts());
    }
    assert!(pictures(&e).is_empty());
    assert!(e.document().images.is_empty());
}

#[test]
fn merge_formatting_takes_the_destination_look_and_keeps_partial_emphasis() {
    let styles = r#"<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:i/></w:rPr></w:style>"#;
    let body = format!(
        r#"<w:p><w:r><w:rPr><w:sz w:val="40"/></w:rPr><w:t xml:space="preserve">big </w:t></w:r><w:r><w:rPr><w:b/><w:sz w:val="40"/></w:rPr><w:t>bold</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr>{}</w:p>"#,
        t("q")
    );
    let mut e = Editor::open(&with_styles(&body, styles), fonts()).expect("opens");
    select(&mut e, (0, 0), (0, 8));
    let f = e.copy().expect("copies");
    caret(&mut e, (1, 1));
    let clip = Clip { fragment: Some(f), ..Clip::default() };
    assert!(changed(&e.paste_clip(clip, Some(PasteChoice::Merge), fonts())));
    let p = paras(&e)[1];
    let runs: Vec<(String, bool, f64)> =
        p.inlines.iter().filter_map(|i| if let Inline::Text(t, r) = i { Some((t.clone(), r.bold, r.size)) } else { None }).collect();
    assert!(runs.iter().all(|r| (r.2 - 11.0).abs() < 0.01), "the size goes: {runs:?}");
    assert!(runs.iter().any(|r| r.0.contains("bold") && r.1), "the bold on part of it stays: {runs:?}");
    assert!(runs.iter().all(|r| r.0.contains("bold") || !r.1), "{runs:?}");
}

#[test]
fn keep_text_only_keeps_numbers_as_text_unless_told_not_to() {
    let mut e = ed("<w:p/>");
    let html = "<ol><li>one</li><li>two</li></ol><table><tr><td>a</td><td>b</td></tr></table>";
    let clip = Clip { html: Some(html.into()), text: "one\ntwo".into(), ..Clip::default() };
    assert!(changed(&e.paste_clip(clip.clone(), Some(PasteChoice::TextOnly), fonts())));
    assert_eq!(&texts(&e)[..3], ["1.\tone", "2.\ttwo", "a\tb"]);
    assert!(e.document().sections[0].blocks.iter().all(|b| matches!(b, Block::Para(_))), "no table");
    let mut e = ed("<w:p/>");
    e.set_paste_defaults(PasteDefaults { keep_bullets: false, ..PasteDefaults::default() });
    e.paste_clip(clip, Some(PasteChoice::TextOnly), fonts());
    assert_eq!(&texts(&e)[..2], ["one", "two"]);
}

#[test]
fn the_paste_options_paste_again_another_way_as_one_undo_step() {
    let mut e = ed(&format!("<w:p>{}</w:p><w:p>{}</w:p>", t("src"), t("dst")));
    let clip = Clip { html: Some("<p><b>bold</b></p>".into()), text: "bold".into(), ..Clip::default() };
    caret(&mut e, (1, 3));
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    let v = e.paste_options_view(0..1).expect("the Paste Options button");
    assert_eq!(v.current, PasteChoice::KeepSource);
    assert_eq!(v.choices, [PasteChoice::KeepSource, PasteChoice::Merge, PasteChoice::TextOnly]);
    assert!(v.button[0] >= v.end[2] - 0.01 && v.button[1] >= v.end[3] - 0.01, "below the end of the paste: {v:?}");
    let bold = |e: &Editor| paras(e)[1].inlines.iter().any(|i| matches!(i, Inline::Text(t, r) if t.contains("bold") && r.bold));
    assert!(bold(&e));
    assert!(changed(&e.repaste(PasteChoice::TextOnly, fonts())));
    assert!(!bold(&e), "as text");
    assert_eq!(texts(&e)[1], "dst bold");
    assert_eq!(e.paste_options_view(0..1).map(|v| v.current), Some(PasteChoice::TextOnly), "the button stays");
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), ["src", "dst"], "one undo takes the paste back");
    assert!(e.paste_options_view(0..1).is_none(), "nothing pasted now");
}

#[test]
fn typing_after_a_paste_takes_its_options_away() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    e.paste_clip(Clip { text: "y".into(), ..Clip::default() }, None, fonts());
    assert!(e.last_paste().is_some());
    e.execute(Command::InsertText("z".into()), fonts());
    assert!(e.last_paste().is_none());
    assert!(matches!(e.repaste(PasteChoice::TextOnly, fonts()), Outcome::Refused(_)));
}

#[test]
fn the_paste_defaults_choose_how_a_paste_takes_the_engine_s_copy() {
    let mut e = ed(&format!("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>b</w:t></w:r>{}</w:p>", t("x")));
    select(&mut e, (0, 0), (0, 1));
    let f = e.copy().expect("copies");
    caret(&mut e, (0, 2));
    assert!(!e.set_paste_defaults(PasteDefaults { within: PasteChoice::Picture, ..PasteDefaults::default() }), "not a choice it offers");
    assert!(e.set_paste_defaults(PasteDefaults { within: PasteChoice::TextOnly, ..PasteDefaults::default() }));
    e.paste_clip(Clip { fragment: Some(f), text: "b".into(), ..Clip::default() }, None, fonts());
    let runs: Vec<bool> = paras(&e)[0].inlines.iter().filter_map(|i| i.run().map(|r| r.bold)).collect();
    assert_eq!(texts(&e)[0], "bx b");
    assert_eq!(runs.last(), Some(&false), "pasted as text: {runs:?}");
}

#[test]
fn smart_cut_and_paste_can_be_turned_off() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("one two")));
    caret(&mut e, (0, 3));
    e.execute(Command::PasteText("x".into()), fonts());
    assert_eq!(texts(&e)[0], "one x two", "a space keeps the words apart");
    let mut e = ed(&format!("<w:p>{}</w:p>", t("one two")));
    e.set_paste_defaults(PasteDefaults { smart: false, ..PasteDefaults::default() });
    caret(&mut e, (0, 3));
    e.execute(Command::PasteText("x".into()), fonts());
    assert_eq!(texts(&e)[0], "onex two");
}

fn table(rows: &[&[&str]]) -> String {
    let grid: String = (0..rows[0].len()).map(|_| "<w:gridCol w:w=\"2000\"/>").collect();
    let body: String = rows
        .iter()
        .map(|r| {
            format!(
                "<w:tr>{}</w:tr>",
                r.iter()
                    .map(|c| format!("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p>{}</w:p></w:tc>", t(c)))
                    .collect::<String>()
            )
        })
        .collect();
    format!("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl>")
}

fn cell_texts(e: &Editor) -> Vec<Vec<String>> {
    let Some(Block::Table(t)) = e.document().sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))) else { return Vec::new() };
    t.rows
        .iter()
        .map(|r| {
            r.cells
                .iter()
                .map(|c| {
                    c.blocks
                        .iter()
                        .map(|b| if let Block::Para(p) = b { crate::editing::text::UnitText::of(p).text } else { "[table]".into() })
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect()
        })
        .collect()
}

/// Copies the first row of the second table and puts the caret in the first table's second row.
fn cells_editor() -> (Editor, Clip) {
    let body = format!("{}<w:p/>{}<w:p/>", table(&[&["a1", "a2"], &["b1", "b2"]]), table(&[&["x", "y"]]));
    let mut e = ed(&body);
    let ids = super::texts_ids(&e);
    let tables: Vec<u64> = e.document().sections[0].blocks.iter().filter_map(|b| if let Block::Table(t) = b { Some(t.id) } else { None }).collect();
    let (table, anchor, focus) = (crate::layout::TableId(tables[1]), CellRef { row: 0, col: 0 }, CellRef { row: 0, col: 1 });
    e.set_selection(Selection::Cells { table, anchor, focus, shape: CellShape::Rows });
    let f = e.copy().expect("copies the row");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, ids[2], Unit(1))));
    (e, Clip { fragment: Some(f), text: "x\ty".into(), ..Clip::default() })
}

#[test]
fn copied_rows_at_a_paragraph_after_a_nested_table_report_the_rows_they_add_to_it() {
    let outer =
        format!("<w:tbl><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc>{}<w:p>{}</w:p></w:tc></w:tr></w:tbl>", table(&[&["n"]]), t("z"));
    let mut e = ed(&format!("{outer}<w:p/>{}<w:p/>", table(&[&["x", "y"]])));
    let ids = super::texts_ids(&e);
    let Some(Block::Table(src)) = e.document().sections[0].blocks.get(2) else { panic!("the copied table") };
    let (table, anchor, focus) = (crate::layout::TableId(src.id), CellRef { row: 0, col: 0 }, CellRef { row: 0, col: 1 });
    e.set_selection(Selection::Cells { table, anchor, focus, shape: CellShape::Rows });
    let f = e.copy().expect("copies the row");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, ids[1], Unit(1))));
    let clip = Clip { fragment: Some(f), text: "x\ty".into(), ..Clip::default() };
    assert_eq!(e.default_paste(&clip), Some(PasteChoice::InsertRows));
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    let Some(Block::Table(outer)) = e.document().sections[0].blocks.first() else { panic!("a table") };
    let Some(Block::Table(nested)) = outer.rows[0].cells[0].blocks.first() else { panic!("the nested table") };
    assert_eq!((outer.rows.len(), nested.rows.len()), (1, 2), "the rows go at the nested table's end");
    assert_eq!(e.last_paste().map(|l| l.choice), Some(PasteChoice::InsertRows));
    assert!(changed(&e.repaste(PasteChoice::MergeTable, fonts())));
    let Some(Block::Table(outer)) = e.document().sections[0].blocks.first() else { panic!("a table") };
    let Some(Block::Table(nested)) = outer.rows[0].cells[0].blocks.first() else { panic!("the nested table") };
    assert_eq!((outer.rows.len(), nested.rows.len()), (1, 2), "merged into the nested table too");
}

#[test]
fn copied_rows_at_the_paragraph_after_a_table_go_at_its_end_and_offer_the_table_choices() {
    let mut e = ed(&format!("{}<w:p/>{}<w:p/>", table(&[&["a1", "a2"]]), table(&[&["x", "y"]])));
    let ids = super::texts_ids(&e);
    let Some(Block::Table(src)) = e.document().sections[0].blocks.get(2) else { panic!("the copied table") };
    let (id, anchor, focus) = (crate::layout::TableId(src.id), CellRef { row: 0, col: 0 }, CellRef { row: 0, col: 1 });
    e.set_selection(Selection::Cells { table: id, anchor, focus, shape: CellShape::Rows });
    let f = e.copy().expect("copies the row");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, ids[2], Unit(0))));
    let clip = Clip { fragment: Some(f), text: "x\ty".into(), ..Clip::default() };
    assert_eq!(e.default_paste(&clip), Some(PasteChoice::InsertRows));
    let choices = e.paste_choices(&clip);
    assert_eq!(
        &choices[..4],
        [PasteChoice::NestTable, PasteChoice::MergeTable, PasteChoice::InsertRows, PasteChoice::TextOnly],
        "no cells to overwrite"
    );
    assert!(changed(&e.paste_clip(clip.clone(), None, fonts())));
    assert_eq!(cell_texts(&e), [vec!["a1", "a2"], vec!["x", "y"]]);
    assert_eq!(e.last_paste().map(|l| l.choice), Some(PasteChoice::InsertRows));
    assert!(changed(&e.repaste(PasteChoice::MergeTable, fonts())));
    assert_eq!(cell_texts(&e), [vec!["a1", "a2"], vec!["x", "y"]], "merged into the same table");
    assert_eq!(e.document().sections[0].blocks.iter().filter(|b| matches!(b, Block::Table(_))).count(), 2);
    // Smart table paste off: the rows are a table of their own at the caret
    let mut e2 = ed(&format!("{}<w:p/>", table(&[&["a1", "a2"]])));
    let ids = super::texts_ids(&e2);
    e2.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, ids[2], Unit(0))));
    let mut d = e2.paste_defaults();
    d.smart_tables = false;
    e2.set_paste_defaults(d);
    assert_eq!(e2.default_paste(&clip), Some(PasteChoice::NestTable));
    assert!(changed(&e2.paste_clip(clip, None, fonts())));
    assert_eq!(cell_texts(&e2), [vec!["a1", "a2"]], "the table above is as it was");
    assert_eq!(e2.document().sections[0].blocks.iter().filter(|b| matches!(b, Block::Table(_))).count(), 2);
}

#[test]
fn copied_cells_in_a_table_offer_the_table_choices() {
    let (e, clip) = cells_editor();
    let choices = e.paste_choices(&clip);
    assert_eq!(
        &choices[..5],
        [PasteChoice::NestTable, PasteChoice::MergeTable, PasteChoice::InsertRows, PasteChoice::OverwriteCells, PasteChoice::TextOnly]
    );
}

#[test]
fn insert_as_new_rows_and_merge_table_put_the_rows_above_the_caret_s() {
    for choice in [PasteChoice::InsertRows, PasteChoice::MergeTable] {
        let (mut e, clip) = cells_editor();
        assert!(changed(&e.paste_clip(clip, Some(choice), fonts())), "{choice:?}");
        assert_eq!(cell_texts(&e), [vec!["a1", "a2"], vec!["x", "y"], vec!["b1", "b2"]], "{choice:?}");
    }
}

#[test]
fn overwrite_cells_writes_over_the_cells_from_the_caret_s() {
    let (mut e, clip) = cells_editor();
    assert!(changed(&e.paste_clip(clip, Some(PasteChoice::OverwriteCells), fonts())));
    assert_eq!(cell_texts(&e), [vec!["a1", "a2"], vec!["x", "y"]]);
}

#[test]
fn nest_table_puts_a_table_in_the_cell() {
    let (mut e, clip) = cells_editor();
    assert!(changed(&e.paste_clip(clip, Some(PasteChoice::NestTable), fonts())));
    let rows = cell_texts(&e);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows[1][0].contains("[table]"), "{rows:?}");
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{:?}", v.iter().map(|v| v.detail.clone()).collect::<Vec<_>>());
}

const QUOTE_A: &str = r#"<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:pPr><w:jc w:val="center"/><w:spacing w:before="480"/></w:pPr></w:style>"#;
const QUOTE_B: &str = r#"<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:pPr><w:jc w:val="right"/></w:pPr></w:style>"#;

fn quote_paste(choice: Option<PasteChoice>) -> crate::docx::ParaStyle {
    let mut a = Editor::open(&with_styles(&format!(r#"<w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr>{}</w:p><w:p/>"#, t("quoted")), QUOTE_A), fonts())
        .expect("opens");
    select(&mut a, (0, 0), (1, 0));
    let f = a.copy().expect("copies");
    let mut b = Editor::open(&with_styles(&format!("<w:p>{}</w:p>", t("dst")), QUOTE_B), fonts()).expect("opens");
    caret(&mut b, (0, 0));
    let clip = Clip { fragment: Some(f), text: "quoted\n".into(), ..Clip::default() };
    if choice.is_none() {
        assert_eq!(b.default_paste(&clip), Some(PasteChoice::DestinationStyles), "the styles conflict");
    }
    assert!(changed(&b.paste_clip(clip, choice, fonts())));
    let b = Editor::open(&crate::docx::write::save(b.document()).expect("saves"), fonts()).expect("reopens");
    let p = paras(&b).into_iter().find(|p| crate::editing::text::UnitText::of(p).text == "quoted").expect("the pasted paragraph");
    (*p.style).clone()
}

#[test]
fn keep_source_formatting_keeps_a_conflicting_style_s_look_and_destination_styles_take_this_one_s() {
    use crate::docx::props::Jc;
    let kept = quote_paste(Some(PasteChoice::KeepSource));
    assert_eq!((kept.jc, kept.space_before, kept.style_id.as_deref()), (Jc::Center, 24.0, Some("Quote")), "its own look, saved");
    let dest = quote_paste(None);
    assert_eq!((dest.jc, dest.space_before, dest.style_id.as_deref()), (Jc::Right, 0.0, Some("Quote")), "this document's Quote");
}

#[test]
fn text_from_file_inserts_another_document_s_body_with_its_styles() {
    let other = with_styles(&format!(r#"<w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr>{}</w:p><w:p>{}</w:p>"#, t("first"), t("second")), QUOTE_A);
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    caret(&mut e, (0, 1));
    let out = e.insert_file(&other, fonts());
    assert!(changed(&out), "{out:?}");
    assert_eq!(texts(&e), ["afirst", "secondb"]);
    assert!(e.document().resolver.as_ref().is_some_and(|r| r.styles.get("Quote").is_some()), "its style comes");
    assert_eq!(e.history().next_undo(), Some("Insert File"));
    e.execute(Command::Undo, fonts());
    assert_eq!(texts(&e), ["ab"]);
    assert!(matches!(e.insert_file(b"PK\x03\x04not a document", fonts()), Outcome::Refused(_)));
}

/// A document with `body` and a header part `word/header1.xml` (relationship `rIdHdr`) holding `header`.
fn with_header(body: &str, header: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(blank_body(body))).expect("a zip");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("an entry");
        let name = f.name().expect("a name").to_string();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).expect("reads");
        let text = String::from_utf8(bytes.clone()).expect("utf-8");
        if name == "word/_rels/document.xml.rels" {
            let rel = r#"<Relationship Id="rIdHdr" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
            bytes = text.replacen("</Relationships>", rel, 1).into_bytes();
        } else if name == "[Content_Types].xml" {
            let ct = r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#;
            bytes = text.replacen("</Types>", ct, 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).expect("starts");
        out.write_all(&bytes).expect("writes");
    }
    out.start_file("word/header1.xml", zip::write::SimpleFileOptions::default()).expect("starts");
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:p>{}</w:p></w:hdr>"#,
        t(header)
    );
    out.write_all(xml.as_bytes()).expect("writes");
    out.finish().expect("finishes").into_inner()
}

/// Each body section's paragraphs' text.
fn section_texts(e: &Editor) -> Vec<Vec<String>> {
    let text = |b: &Block| if let Block::Para(p) = b { Some(crate::editing::text::UnitText::of(p).text) } else { None };
    e.document().sections.iter().map(|s| s.blocks.iter().filter_map(text).collect()).collect()
}

#[test]
fn text_from_file_brings_its_section_breaks_and_their_page_setup() {
    let landscape = r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdHdr"/><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="360" w:footer="360" w:gutter="0"/></w:sectPr>"#;
    let other = with_header(&format!("<w:p><w:pPr>{landscape}</w:pPr>{}</w:p><w:p>{}</w:p>", t("land"), t("port")), "head");
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    let page = e.document().sections[0].page_w;
    caret(&mut e, (0, 1));
    assert!(changed(&e.insert_file(&other, fonts())));
    assert_eq!(section_texts(&e), [vec!["aland".to_string()], vec!["portb".to_string()]], "a section break after its first section");
    let s = &e.document().sections;
    assert_eq!((s[0].page_w, s[0].left, s[1].page_w), (792.0, 36.0, page), "the first its page, the last this document's");
    let header = s[0].headers.iter().flatten().next().expect("its header comes");
    let hb = e.document().stories.get(header).expect("the header's story");
    assert!(hb.iter().any(|b| matches!(b, Block::Para(p) if crate::editing::text::UnitText::of(p).text == "head")), "{hb:?}");
    assert!(s[1].headers.iter().all(Option::is_none), "the last section keeps this document's (none)");
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{:?}", v.iter().map(|v| v.detail.clone()).collect::<Vec<_>>());
    assert_eq!(e.history().next_undo(), Some("Insert File"));
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(section_texts(&again), section_texts(&e), "saved with its section break");
    assert!(again.document().sections[0].headers.iter().flatten().next().is_some(), "and its header");
    e.execute(Command::Undo, fonts());
    assert_eq!(section_texts(&e), [vec!["ab".to_string()]], "one undo step");
    assert!(e.document().stories.is_empty());
    // In a cell, no section breaks: its sections' text alone
    let cell = format!("<w:tbl><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl><w:p/>", t("ab"));
    let mut e = ed(&cell);
    let Some(Block::Table(tb)) = e.document().sections[0].blocks.first() else { panic!("a table") };
    let p = crate::layout::ParaId(match &tb.rows[0].cells[0].blocks[0] {
        Block::Para(p) => p.id,
        _ => panic!("a paragraph"),
    });
    let story = e.index().para_slot(p).map(|s| e.index().story(s.container)).expect("a story");
    e.set_caret_for_benchmark(Selection::Caret(Position::text(story, p, Unit(1))));
    assert!(changed(&e.insert_file(&other, fonts())));
    assert_eq!(e.document().sections.len(), 1);
}

#[test]
fn many_file_sections_sharing_a_header_bring_it_in_once_in_linear_time() {
    // Before: each section took its own copy, n x h header paragraphs
    let (n, h) = (2_000, 1_000);
    // First-page and even-page slots, which no page shows here: the import alone, not drawing the header on each page
    let sect =
        r#"<w:pPr><w:sectPr><w:headerReference w:type="first" r:id="rIdHdr"/><w:headerReference w:type="even" r:id="rIdHdr"/></w:sectPr></w:pPr>"#;
    let body: String = (0..n).map(|_| format!("<w:p>{sect}{}</w:p>", t("s"))).collect::<String>() + &format!("<w:p>{}</w:p>", t("end"));
    let other = with_header(&body, &vec!["h"; h].join("</w:t></w:r></w:p><w:p><w:r><w:t>"));
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    caret(&mut e, (0, 1));
    let start = std::time::Instant::now();
    assert!(changed(&e.insert_file(&other, fonts())));
    assert!(start.elapsed().as_secs() < 20, "{:?}", start.elapsed());
    let d = e.document();
    assert_eq!(d.sections.len(), n + 1);
    assert_eq!(d.stories.len(), 1, "one story for the one part");
    let (name, blocks) = d.stories.iter().next().expect("a story");
    assert_eq!(blocks.len(), h);
    assert!(d.sections[..n].iter().all(|s| s.headers[1].as_ref() == Some(name) && s.headers[2].as_ref() == Some(name)));
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{:?}", v.iter().take(3).map(|v| v.detail.clone()).collect::<Vec<_>>());
    e.execute(Command::Undo, fonts());
    assert_eq!(section_texts(&e), [vec!["ab".to_string()]]);
    assert!(e.document().stories.is_empty());
    e.execute(Command::Redo, fonts());
    assert_eq!((e.document().sections.len(), e.document().stories.len()), (n + 1, 1));
}

#[test]
fn a_file_ending_in_a_section_break_keeps_it() {
    let landscape = r#"<w:sectPr><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/></w:sectPr>"#;
    let other = blank_body(&format!("<w:p>{}</w:p><w:p><w:pPr>{landscape}</w:pPr>{}</w:p>", t("one"), t("land")));
    let r = crate::docx::read_editable(&other).expect("reads");
    assert_eq!(r.sections.iter().map(|s| s.blocks.len()).collect::<Vec<_>>(), [2, 1], "the reader closes the last section");
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    let page = e.document().sections[0].page_w;
    caret(&mut e, (0, 1));
    assert!(changed(&e.insert_file(&other, fonts())));
    assert_eq!(section_texts(&e), [vec!["aone".to_string(), "land".to_string()], vec!["b".to_string()]]);
    let s = &e.document().sections;
    assert_eq!((s[0].page_w, s[1].page_w), (792.0, page));
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{:?}", v.iter().map(|v| v.detail.clone()).collect::<Vec<_>>());
}

/// A PNG's size and its pixels, straight RGBA: 8-bit RGBA without filters, as the clipboard's pictures are written.
fn unpng(png: &[u8]) -> ([u32; 2], Vec<u8>) {
    use std::io::Read;
    let (mut at, mut size, mut z) = (8, [0u32; 2], Vec::new());
    while at + 8 <= png.len() {
        let n = u32::from_be_bytes(png[at..at + 4].try_into().expect("4 bytes")) as usize;
        let (kind, body) = (&png[at + 4..at + 8], &png[at + 8..at + 8 + n]);
        if kind == b"IHDR" {
            size = [u32::from_be_bytes(body[..4].try_into().expect("4")), u32::from_be_bytes(body[4..8].try_into().expect("4"))];
        } else if kind == b"IDAT" {
            z.extend_from_slice(body);
        }
        at += 12 + n;
    }
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(&z[..]).read_to_end(&mut raw).expect("inflates");
    let row = size[0] as usize * 4;
    (size, raw.chunks(row + 1).flat_map(|r| r[1..].to_vec()).collect())
}

/// The picture a copy puts on the clipboard.
fn drawn(f: &crate::editing::clipboard::Fragment) -> Option<crate::editing::clipboard::ClipPicture> {
    crate::services::clipboard::image(f, fonts())
}

fn pixel(png: &[u8], x: u32, y: u32) -> [u8; 4] {
    let (size, px) = unpng(png);
    let k = (y * size[0] + x) as usize * 4;
    px[k..k + 4].try_into().expect("a pixel")
}

#[test]
fn copying_a_selected_picture_puts_it_on_the_clipboard_as_it_is_drawn() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    caret(&mut e, (0, 1));
    let data = crate::editing::wrap_points::tests::png(96, 48, &[40u8, 120, 200, 255].repeat(96 * 48));
    assert!(changed(&e.paste_clip(image_clip(data), None, fonts())));
    e.set_selection(Selection::Object(crate::editing::selection::ObjectRef::Inline { para: para_id(&e, 0), offset: Unit(1) }));
    e.execute(Command::Copy, fonts());
    let f = e.clipboard().cloned().expect("copied");
    let image = drawn(&f).expect("a picture on the clipboard");
    assert_eq!(unpng(&image.png).0, [96, 48], "96 dpi, the size it is on the page");
    assert_eq!(pixel(&image.png, 48, 24), [40, 120, 200, 255]);
    let html = crate::services::clipboard::html(&e, &f, fonts());
    assert!(html.contains("<img src=\"data:image/png;base64,") && html.contains("width=\"96\" height=\"48\""), "{html}");
    // Pasted as a picture, it is the same picture
    caret(&mut e, (0, 0));
    let clip = image_clip(image.png.clone());
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    assert_eq!(pictures(&e).len(), 2);
    assert_eq!(pictures(&e)[0].0, 72.0);
}

#[test]
fn a_mixed_selection_s_html_embeds_its_pictures_and_puts_no_picture_on_the_clipboard() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    caret(&mut e, (0, 1));
    e.paste_clip(image_clip(png(8, 8)), None, fonts());
    select(&mut e, (0, 0), (0, 3));
    e.execute(Command::Copy, fonts());
    let f = e.clipboard().expect("copied");
    assert!(f.image.is_none(), "text with a picture: HTML and text only");
    assert!(f.drawn.is_empty(), "drawn when its HTML is asked for");
    let html = crate::services::clipboard::html(&e, f, fonts());
    let (a, img, b) = (html.find(">a<").expect("a"), html.find("<img").expect("the picture"), html.find(">b<").expect("b"));
    assert!(a < img && img < b, "{html}");
    // Pasted back as HTML, the picture comes too
    let mut other = ed("<w:p/>");
    assert!(changed(&other.paste_html(&html, fonts())));
    assert_eq!(pictures(&other).len(), 1);
}

/// A blue square and a red one beside it, floating.
fn two_squares() -> Editor {
    use crate::layout::tests::shapes::{anchor, bytes, sp};
    let a = anchor(100, 300, 60, 30, 1, false, 5, &sp(0, 0, 60, 30, "rect", "", "0000FF"));
    let b = anchor(200, 300, 60, 30, 2, false, 6, &sp(0, 0, 60, 30, "rect", "", "FF0000"));
    let mut e = Editor::open(&bytes(&format!("<w:p><w:r><w:t>Text</w:t></w:r>{a}{b}</w:p>")).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn floats(e: &Editor) -> Vec<crate::layout::FloatId> {
    let mut out = Vec::new();
    super::super::paste::tests_each_float(&e.document().sections[0].blocks, &mut |f| out.push(crate::layout::FloatId(f)));
    out
}

#[test]
fn cutting_a_shape_puts_its_drawing_on_the_clipboard() {
    let mut e = two_squares();
    let f = floats(&e);
    e.set_selection(Selection::Object(crate::editing::selection::ObjectRef::Floating(f[0])));
    e.execute(Command::Cut, fonts());
    let clip = e.clipboard().expect("cut");
    let image = drawn(clip).expect("its drawing");
    assert!((image.size[0] - 60.0).abs() < 2.0 && (image.size[1] - 30.0).abs() < 2.0, "the shape and its outline: {:?}", image.size);
    let [w, h] = unpng(&image.png).0;
    let [r, _, b, a] = pixel(&image.png, w / 2, h / 2);
    assert!(a == 255 && b > 200 && r < 50, "the blue square: {:?}", pixel(&image.png, w / 2, h / 2));
    assert!(pixel(&image.png, 0, 0)[3] > 100, "the outline at its corner");
    assert_eq!(floats(&e).len(), 1, "cut");
}

#[test]
fn selected_floats_are_drawn_together_where_they_are() {
    let mut e = two_squares();
    let f = floats(&e);
    e.set_selection(Selection::Object(crate::editing::selection::ObjectRef::Floating(f[0])));
    e.object_also = Some((f[0], vec![f[1]]));
    assert_eq!(e.selected_floats().len(), 2);
    e.execute(Command::Copy, fonts());
    let both = e.clipboard().and_then(drawn).expect("both");
    assert!((both.size[0] - 160.0).abs() < 2.0 && (both.size[1] - 30.0).abs() < 2.0, "side by side, 40 pt apart: {:?}", both.size);
    let [w, h] = unpng(&both.png).0;
    assert_eq!(pixel(&both.png, w / 2, h / 2)[3], 0, "the space between them is clear");
    assert!(pixel(&both.png, w - 10, h / 2)[0] > 200, "the red one on the right");
}

#[test]
fn a_copied_float_pastes_as_a_float() {
    let mut e = two_squares();
    let f = floats(&e);
    e.set_selection(Selection::Object(crate::editing::selection::ObjectRef::Floating(f[1])));
    e.execute(Command::Copy, fonts());
    caret(&mut e, (0, 2));
    assert!(changed(&e.execute(Command::Paste(Default::default()), fonts())));
    assert_eq!(floats(&e).len(), 3);
    assert_eq!(texts(&e)[0].chars().filter(|c| *c != crate::editing::text::OBJECT).collect::<String>(), "Text");
}

#[test]
fn a_copied_group_child_is_a_float_of_its_own_where_it_is_drawn() {
    use crate::layout::tests::shapes::{PT, anchor, bytes, sp};
    let group = format!(
        r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="0" y="0"/><a:chExt cx="{w}" cy="{h}"/></a:xfrm></wpg:grpSpPr>{}{}</wpg:wgp>"#,
        sp(0, 0, 80, 80, "rect", "", "0000FF"),
        sp(120, 0, 80, 80, "rect", "", "00FF00"),
        w = 200 * PT,
        h = 100 * PT,
    );
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", anchor(72, 300, 200, 100, 1, false, 1, &group));
    let mut e = Editor::open(&bytes(&body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    let g = floats(&e)[0];
    e.set_selection(Selection::Object(crate::editing::selection::ObjectRef::Floating(g)));
    e.object_child = Some((g, vec![1]));
    e.execute(Command::Copy, fonts());
    let clip = e.clipboard().cloned().expect("copied");
    let image = drawn(&clip).expect("its drawing");
    assert!((image.size[0] - 80.0).abs() < 2.0 && (image.size[1] - 80.0).abs() < 2.0, "the child alone: {:?}", image.size);
    let [w, h] = unpng(&image.png).0;
    let [r, gr, b, _] = pixel(&image.png, w / 2, h / 2);
    assert!(gr > 200 && r < 50 && b < 50, "the green one");
    caret(&mut e, (0, 4));
    assert!(changed(&e.execute(Command::Paste(Default::default()), fonts())));
    let pasted = e.document().sections[0].blocks.iter().find_map(|b| match b {
        Block::Para(p) => p.inlines.iter().find_map(|i| match i {
            Inline::Float(f, _) if crate::layout::FloatId(f.id) != g => Some(f.clone()),
            _ => None,
        }),
        _ => None,
    });
    let pasted = pasted.expect("a float of its own");
    assert_eq!((pasted.w, pasted.h), (80.0, 80.0));
    assert!(
        (pasted.h_offset - 192.0).abs() < 0.5 && (pasted.v_offset - 300.0).abs() < 0.5,
        "where it was drawn: {} {}",
        pasted.h_offset,
        pasted.v_offset
    );
}

/// A document with `body`, whose `w:altChunk r:id="rIdChunk"` imports `part` (under `word/`) holding `data`.
fn with_chunk(body: &str, part: &str, data: &[u8]) -> Vec<u8> {
    use std::io::{Read, Write};
    let src = blank_body(body);
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).expect("a zip");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("an entry");
        let name = f.name().expect("a name").to_string();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).expect("reads");
        if name == "word/_rels/document.xml.rels" {
            let rel = format!(
                r#"<Relationship Id="rIdChunk" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/aFChunk" Target="{part}"/></Relationships>"#
            );
            bytes = String::from_utf8(bytes).expect("utf-8").replacen("</Relationships>", &rel, 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).expect("starts");
        out.write_all(&bytes).expect("writes");
    }
    out.start_file(format!("word/{part}"), zip::write::SimpleFileOptions::default()).expect("starts");
    out.write_all(data).expect("writes");
    out.finish().expect("finishes").into_inner()
}

fn main_part(docx: &[u8]) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(docx)).expect("a zip");
    let mut s = String::new();
    z.by_name("word/document.xml").expect("the main part").read_to_string(&mut s).expect("reads");
    s
}

const CHUNK: &str = r#"<w:altChunk r:id="rIdChunk"/>"#;

#[test]
fn an_alt_chunk_s_html_is_content_once_opened_and_saves_as_it() {
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p>{}</w:p>", t("before"), t("after"));
    let html = "<html><body><p>one <b>bold</b></p><ul><li>item</li></ul></body></html>";
    let mut e = Editor::open(&with_chunk(&body, "chunk.html", html.as_bytes()), fonts()).expect("opens");
    assert_eq!(texts(&e), ["before", "one bold", "item", "after"]);
    assert!(paras(&e)[1].inlines.iter().any(|i| matches!(i, Inline::Text(t, r) if t == "bold" && r.bold)));
    assert!(paras(&e)[2].style.num.is_some(), "a list item");
    assert_eq!(e.history().next_undo(), None, "opening isn't an edit");
    assert!(e.document().alt_chunks.is_empty());
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = main_part(&saved);
    assert!(!xml.contains("altChunk") && xml.contains("bold"), "{xml}");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(texts(&again), ["before", "one bold", "item", "after"]);
    // Edited next to it, it is written once
    caret(&mut e, (3, 0));
    e.execute(Command::InsertText("x".into()), fonts());
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(texts(&again), ["before", "one bold", "item", "xafter"]);
}

#[test]
fn an_alt_chunk_comes_in_untracked_and_its_pictures_with_it() {
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p/>", t("x"));
    let html = format!(r#"<p>pic <img src="data:image/png;base64,{}" width="16" height="16"></p>"#, base64(&png(4, 4)));
    let mut doc = crate::docx::read_editable(&with_chunk(&body, "chunk.htm", html.as_bytes())).expect("reads");
    doc.settings.track_revisions = true;
    let e = Editor::with_document(doc, fonts());
    assert!(e.document().settings.track_revisions);
    assert_eq!(pictures(&e).len(), 1);
    assert!(paras(&e).iter().all(|p| p.inlines.iter().all(|i| i.run().is_none_or(|r| r.rev.ins.is_none()))), "not a tracked insertion");
}

#[test]
fn a_document_without_a_resolver_keeps_its_alt_chunks_and_tracking() {
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p/>", t("x"));
    let mut doc = crate::docx::read_editable(&with_chunk(&body, "chunk.html", b"<p>from html</p>")).expect("reads");
    doc.settings.track_revisions = true;
    doc.resolver = None;
    let e = Editor::with_document(doc, fonts());
    assert_eq!(e.document().alt_chunks.len(), 1);
    assert!(e.document().settings.track_revisions);
    assert_eq!(e.history().next_undo(), None);
}

#[test]
fn an_alt_chunk_alone_in_a_cell_leaves_the_cell_ending_in_a_paragraph() {
    let cell = format!("<w:tbl><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc>{CHUNK}</w:tc></w:tr></w:tbl><w:p/>");
    let html = "<table><tr><td>inner</td></tr></table>";
    let e = Editor::open(&with_chunk(&cell, "c.html", html.as_bytes()), fonts()).expect("opens");
    let Some(Block::Table(t)) = e.document().sections[0].blocks.first() else { panic!("a table") };
    let blocks = &t.rows[0].cells[0].blocks;
    assert!(matches!(blocks.first(), Some(Block::Table(_))) && matches!(blocks.last(), Some(Block::Para(_))), "{blocks:?}");
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{:?}", v.iter().map(|v| v.detail.clone()).collect::<Vec<_>>());
}

#[test]
fn an_empty_alt_chunk_after_a_table_leaves_its_cell_and_the_body_ending_in_a_paragraph() {
    let tbl = |inner: &str| format!("<w:tbl><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc>{inner}</w:tc></w:tr></w:tbl>");
    let empty = b"<html><body></body></html>";
    for body in [tbl(&format!("{}{CHUNK}", tbl("<w:p/>"))) + "<w:p/>", tbl("<w:p/>") + CHUNK] {
        let e = Editor::open(&with_chunk(&body, "c.html", empty), fonts()).expect("opens");
        let blocks = &e.document().sections[0].blocks;
        assert!(matches!(blocks.last(), Some(Block::Para(_))), "{blocks:?}");
        let Some(Block::Table(t)) = blocks.first() else { panic!("a table") };
        assert!(matches!(t.rows[0].cells[0].blocks.last(), Some(Block::Para(_))));
        let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
        assert!(v.is_empty(), "{:?}", v.iter().map(|v| v.detail.clone()).collect::<Vec<_>>());
    }
}

#[test]
fn an_alt_chunk_of_another_format_or_too_much_html_is_kept_as_read() {
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p>{}</w:p>", t("a"), t("b"));
    let rtf = with_chunk(&body, "chunk.rtf", br"{\rtf1 hello}");
    let e = Editor::open(&rtf, fonts()).expect("opens");
    assert_eq!(texts(&e), ["a", "b"], "not drawn");
    assert!(main_part(&crate::docx::write::save(e.document()).expect("saves")).contains("altChunk"), "kept");
    let big = format!("<p>{}</p>", "x".repeat(9 << 20));
    let e = Editor::open(&with_chunk(&body, "big.html", big.as_bytes()), fonts()).expect("opens");
    assert_eq!(texts(&e), ["a", "b"]);
}

#[test]
fn an_alt_chunk_s_text_is_a_paragraph_a_line() {
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p>{}</w:p>", t("a"), t("b"));
    let e = Editor::open(&with_chunk(&body, "chunk.txt", b"one\r\ntwo\tthree\rfour\n"), fonts()).expect("opens");
    assert_eq!(texts(&e).len(), 5, "{:?}", texts(&e));
    assert!(paras(&e)[2].inlines.iter().any(|i| matches!(i, Inline::Tab(_))), "a tab");
    assert_eq!(e.history().next_undo(), None, "opening isn't an edit");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(!main_part(&saved).contains("altChunk"));
    let e = Editor::open(&with_chunk(&body, "chunk.txt", b"caf\xe9"), fonts()).expect("opens");
    assert_eq!(texts(&e), ["a", "caf\u{e9}", "b"], "Windows-1252 when not UTF-8");
    let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("h\u{e9}".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    let e = Editor::open(&with_chunk(&body, "chunk.txt", &utf16), fonts()).expect("opens");
    assert_eq!(texts(&e), ["a", "h\u{e9}", "b"]);
    let e = Editor::open(&with_chunk(&body, "chunk.txt", b""), fonts()).expect("opens");
    assert_eq!(texts(&e), ["a", "b"], "empty text imports nothing");
}

#[test]
fn an_alt_chunk_s_document_is_its_body_once_opened_and_saves_as_it() {
    let inner = blank_body(&format!("<w:p>{}</w:p><w:p>{}</w:p>", t("inner one"), t("inner two")));
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p>{}</w:p>", t("a"), t("b"));
    let e = Editor::open(&with_chunk(&body, "chunk.docx", &inner), fonts()).expect("opens");
    assert_eq!(texts(&e), ["a", "inner one", "inner two", "b"]);
    assert_eq!(e.history().next_undo(), None, "opening isn't an edit");
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(!main_part(&saved).contains("altChunk"));
    assert_eq!(texts(&Editor::open(&saved, fonts()).expect("reopens")), ["a", "inner one", "inner two", "b"]);
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{:?}", v.iter().map(|v| v.detail.clone()).collect::<Vec<_>>());
    let e = Editor::open(&with_chunk(&body, "chunk.docx", b"not a zip"), fonts()).expect("opens");
    assert_eq!(texts(&e), ["a", "b"], "an unreadable document is not drawn");
    assert!(main_part(&crate::docx::write::save(e.document()).expect("saves")).contains("altChunk"), "kept as read");
}

#[test]
fn alt_chunk_documents_nest_eight_deep() {
    let mut docx = blank_body(&format!("<w:p>{}</w:p>", t("L0")));
    for i in 1..=10 {
        docx = with_chunk(&format!("<w:p>{}</w:p>{CHUNK}", t(&format!("L{i}"))), "c.docx", &docx);
    }
    let e = Editor::open(&docx, fonts()).expect("opens");
    let want: Vec<String> = (2..=10).rev().map(|i| format!("L{i}")).collect();
    assert_eq!(texts(&e), want);
}

#[test]
fn a_document_read_to_view_brings_its_alt_chunks_in_through_the_editor() {
    let body = format!("<w:p>{}</w:p>{CHUNK}", t("a"));
    let docx = with_chunk(&body, "chunk.html", b"<p>from html</p>");
    let doc = crate::docx::read(&docx).expect("reads");
    assert_eq!(doc.alt_chunks.len(), 1);
    let doc = Editor::with_document(doc, fonts()).into_document();
    let text: Vec<String> = doc.sections[0]
        .blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(crate::editing::text::UnitText::of(p).text) } else { None })
        .collect();
    assert_eq!(text, ["a", "from html"]);
}

#[test]
fn a_read_only_document_copies_its_objects_but_doesn_t_cut_them() {
    let mut e = two_squares();
    let f = floats(&e);
    e.set_mode(crate::editing::Mode::ReadOnly);
    e.set_selection(Selection::Object(crate::editing::selection::ObjectRef::Floating(f[0])));
    e.execute(Command::Copy, fonts());
    assert!(e.clipboard().is_some_and(|c| c.image.is_some()), "Word copies from a read-only document");
    let before = e.clipboard().map(|c| c.id);
    e.execute(Command::Cut, fonts());
    assert_eq!(floats(&e).len(), 2, "not cut");
    assert_eq!(e.clipboard().map(|c| c.id), before);
}

#[test]
fn a_picture_pasted_in_a_cell_fits_the_cell() {
    let mut e = ed(&format!("{}<w:p/>", table(&[&["a", "b"]])));
    let ids = super::texts_ids(&e);
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, ids[0], Unit(1))));
    assert!(changed(&e.paste_clip(image_clip(png(400, 200)), None, fonts())));
    let Some(Block::Table(t)) = e.document().sections[0].blocks.first() else { panic!("a table") };
    let pic = t.rows[0].cells[0].blocks.iter().find_map(|b| match b {
        Block::Para(p) => p.inlines.iter().find_map(|i| if let Inline::Picture(p, _) = i { Some((p.w, p.h)) } else { None }),
        _ => None,
    });
    let (w, h) = pic.expect("in the cell");
    assert!(w < 100.0 && (w / h - 2.0).abs() < 0.01, "within the cell's 100 pt, as Insert Picture sizes it: {w} x {h}");
}

#[test]
fn an_alt_chunk_in_utf_16_reads_as_its_text() {
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p/>", t("a"));
    let mut data = vec![0xFF, 0xFE];
    data.extend("<p>caf\u{e9}</p>".encode_utf16().flat_map(u16::to_le_bytes));
    let e = Editor::open(&with_chunk(&body, "u.html", &data), fonts()).expect("opens");
    assert_eq!(&texts(&e)[..2], ["a", "caf\u{e9}"]);
}

/// A table whose cells are Quote paragraphs, in a document whose Quote is `styles`.
fn quote_table(styles: &str, cells: &[&str]) -> Editor {
    let tcs: String = cells
        .iter()
        .map(|c| {
            format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr>{}</w:p></w:tc>"#, t(c))
        })
        .collect();
    let grid: String = cells.iter().map(|_| "<w:gridCol w:w=\"2000\"/>").collect();
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid><w:tr>{tcs}</w:tr><w:tr>{tcs}</w:tr></w:tbl><w:p/>"
    );
    Editor::open(&with_styles(&body, styles), fonts()).expect("opens")
}

#[test]
fn cells_from_a_document_whose_styles_conflict_paste_into_a_table_by_default() {
    let mut a = quote_table(QUOTE_A, &["p", "q"]);
    let table = a.document().sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t.id) } else { None }).expect("a table");
    let (anchor, focus) = (CellRef { row: 0, col: 0 }, CellRef { row: 0, col: 1 });
    a.set_selection(Selection::Cells { table: crate::layout::TableId(table), anchor, focus, shape: CellShape::Rows });
    let f = a.copy().expect("copies the cells");
    assert!(f.cells.is_some(), "cells copied");
    let mut b = quote_table(QUOTE_B, &["x", "y"]);
    let ids = super::texts_ids(&b);
    b.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, ids[2], Unit(1))));
    let clip = Clip { fragment: Some(f), text: "p\tq".into(), ..Clip::default() };
    let d = b.default_paste(&clip).expect("a default");
    assert!(d.is_table() && b.paste_choices(&clip).contains(&d), "Use Destination Styles lands as a table choice: {d:?}");
    assert!(changed(&b.paste_clip(clip, None, fonts())), "Ctrl+V pastes");
    let last = b.last_paste().expect("the Paste Options");
    assert_eq!(last.choice, d, "the table choice it took");
    let v = b.paste_options_view(0..1).expect("the button");
    assert!(v.choices.contains(&v.current), "{v:?}");
}

#[test]
fn a_merge_default_pastes_cells_into_a_table_and_plain_text() {
    let (mut e, clip) = cells_editor();
    assert!(e.set_paste_defaults(PasteDefaults { within: PasteChoice::Merge, other: PasteChoice::Merge, ..PasteDefaults::default() }));
    let d = e.default_paste(&clip).expect("a default");
    assert!(d.is_table(), "{d:?}");
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    assert_eq!(e.last_paste().map(|l| l.choice), Some(d));
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    e.set_paste_defaults(PasteDefaults { other: PasteChoice::Merge, ..PasteDefaults::default() });
    caret(&mut e, (0, 1));
    let clip = Clip { text: "y".into(), ..Clip::default() };
    assert_eq!(e.default_paste(&clip), Some(PasteChoice::TextOnly));
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    assert_eq!(texts(&e)[0], "x y");
}

#[test]
fn without_smart_table_paste_cells_nest_in_the_cell() {
    let (mut e, clip) = cells_editor();
    e.set_paste_defaults(PasteDefaults { smart_tables: false, ..PasteDefaults::default() });
    assert_eq!(e.default_paste(&clip), Some(PasteChoice::NestTable));
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    assert!(cell_texts(&e)[1][0].contains("[table]"), "{:?}", cell_texts(&e));
}

#[test]
fn the_paste_options_keep_the_choices_of_where_it_was_pasted() {
    let (mut e, clip) = cells_editor();
    assert!(changed(&e.paste_clip(clip, Some(PasteChoice::OverwriteCells), fonts())));
    let at_paste = e.last_paste().expect("pasted").choices.clone();
    let v = e.paste_options_view(0..1).expect("the button");
    assert!(v.choices.iter().all(|c| at_paste.contains(c)) && v.choices.contains(&PasteChoice::InsertRows), "{v:?}");
    assert!(changed(&e.repaste(PasteChoice::InsertRows, fonts())));
    assert_eq!(cell_texts(&e), [vec!["a1", "a2"], vec!["x", "y"], vec!["b1", "b2"]]);
}

#[test]
fn html_with_a_picture_offers_no_picture_on_the_button() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    let clip = Clip { html: Some("<p>y</p>".into()), text: "y".into(), image: Some(png(4, 4)), ..Clip::default() };
    assert!(changed(&e.paste_clip(clip, None, fonts())));
    let v = e.paste_options_view(0..1).expect("the button");
    assert!(!v.choices.contains(&PasteChoice::Picture), "{v:?}");
    assert!(changed(&e.repaste(PasteChoice::Picture, fonts())), "Paste Special's picture is still there");
}

#[test]
fn pasted_list_items_landing_in_a_list_continue_it_unless_told_not_to() {
    let numbering = |e: &Editor| -> Vec<Option<i64>> { paras(e).iter().map(|p| p.style.num.map(|n| n.0)).collect() };
    let html = "<ul><li>new</li><li>mid</li><li>end</li></ul>";
    let mut e = ed(&format!("<w:p>{}</w:p>", t("x")));
    caret(&mut e, (0, 1));
    e.paste_clip(Clip { html: Some("<ol><li>one</li><li>two</li></ol>".into()), text: "one\ntwo".into(), ..Clip::default() }, None, fonts());
    let list = numbering(&e).into_iter().flatten().next().expect("a list");
    let n = texts(&e).iter().position(|t| t == "two").expect("two");
    for merge in [true, false] {
        let mut e2 = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
        e2.set_paste_defaults(PasteDefaults { merge_lists: merge, ..PasteDefaults::default() });
        caret(&mut e2, (n, 3));
        assert!(changed(&e2.paste_clip(Clip { html: Some(html.into()), text: "new\nmid\nend".into(), ..Clip::default() }, None, fonts())));
        let k = texts(&e2).iter().position(|t| t == "mid").expect("pasted");
        let got = numbering(&e2)[k];
        assert_eq!(got == Some(list), merge, "merged: {merge} {got:?} {list}");
    }
}

#[test]
fn text_from_file_brings_in_the_file_s_alt_chunks() {
    let body = format!("<w:p>{}</w:p>{CHUNK}", t("a"));
    let other = with_chunk(&body, "chunk.html", b"<p>from html</p>");
    let mut e = ed(&format!("<w:p>{}</w:p>", t("z")));
    caret(&mut e, (0, 1));
    assert!(changed(&e.insert_file(&other, fonts())));
    assert!(texts(&e).iter().any(|t| t.contains("from html")), "{:?}", texts(&e));
}

#[test]
fn many_alt_chunks_on_one_part_open_in_linear_time_and_stop_at_the_cap() {
    let n = 12_000;
    let body: String = CHUNK.repeat(n) + "<w:p/>";
    let docx = with_chunk(&body, "x.html", b"<p>x</p>");
    let start = std::time::Instant::now();
    let e = Editor::open(&docx, fonts()).expect("opens");
    let imported = texts(&e).iter().filter(|t| *t == "x").count();
    assert_eq!(imported, 10_000, "past the cap, kept as read");
    assert!(start.elapsed().as_secs() < 20, "{:?}", start.elapsed());
}

#[test]
fn an_alt_chunk_declared_windows_1252_reads_as_it() {
    let body = format!("<w:p>{}</w:p>{CHUNK}<w:p/>", t("a"));
    let data = b"<html><head><meta charset=\"windows-1252\"></head><body><p>caf\xe9 \x93q\x94</p></body></html>";
    let e = Editor::open(&with_chunk(&body, "w.html", data), fonts()).expect("opens");
    assert_eq!(&texts(&e)[..2], ["a", "caf\u{e9} \u{201c}q\u{201d}"]);
}

#[test]
fn a_very_tall_html_picture_keeps_its_shape() {
    let mut e = ed("<w:p/>");
    let html = format!(r#"<p><img src="data:image/png;base64,{}" width="100" height="4000"></p>"#, base64(&png(1, 40)));
    assert!(changed(&e.paste_html(&html, fonts())));
    let (w, h, _) = pictures(&e)[0];
    assert!((h - 1584.0).abs() < 0.1 && (w / h - 100.0 / 4000.0).abs() < 0.001, "{w} x {h}");
}

#[test]
fn copying_text_with_pictures_draws_nothing_until_its_html_is_asked_for() {
    let mut e = ed(&format!("<w:p>{}</w:p>", t("ab")));
    for _ in 0..3 {
        caret(&mut e, (0, 1));
        e.paste_clip(image_clip(png(64, 64)), None, fonts());
    }
    e.set_caret_for_benchmark(Selection::range(
        Position::text(StoryId::Body, para_id(&e, 0), Unit(0)),
        Position::text(StoryId::Body, para_id(&e, 0), Unit(5)),
        Granularity::Char,
    ));
    e.execute(Command::Copy, fonts());
    let f = e.clipboard().cloned().expect("copied");
    assert!(f.drawn.is_empty() && f.image.is_none());
    assert_eq!(crate::services::clipboard::html(&e, &f, fonts()).matches("<img").count(), 3);
}
