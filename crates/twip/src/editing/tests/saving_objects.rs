//! Saved drawings, text boxes and table content

use super::super::Editor;
use super::super::commands::Command;
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::{fonts, para_id};
use crate::docx::{Block, Inline};
use crate::layout::Unit;
use crate::layout::tests::shapes::{anchor, bytes, sp};

fn editor(body: &str) -> Editor {
    let mut e = Editor::open(&bytes(body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    e
}

fn click(e: &mut Editor, x: f64, y: f64) {
    let (dx, dy) = e.stack().to_doc(0, x, y).expect("on the stack");
    e.pointer_input(PointerKind::Down, dx, dy, 1, Mods::default(), fonts());
    e.pointer_input(PointerKind::Up, dx, dy, 0, Mods::default(), fonts());
}

fn reopen(e: &Editor) -> (Editor, Vec<u8>) {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    (Editor::open(&bytes, fonts()).expect("reopens"), bytes)
}

fn part(bytes: Vec<u8>, name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut z.by_name(name).expect("part"), &mut xml).expect("utf8");
    xml
}

fn box_texts(e: &Editor) -> Vec<String> {
    let mut v = Vec::new();
    for b in &e.document().sections[0].blocks {
        if let Block::Para(p) = b {
            for i in &p.inlines {
                if let Inline::Float(f, _) = i
                    && let Some(tb) = &f.text_box
                {
                    v.push(
                        tb.blocks
                            .iter()
                            .filter_map(|b| if let Block::Para(q) = b { Some(q) } else { None })
                            .flat_map(|q| q.inlines.iter())
                            .filter_map(|i| if let Inline::Text(t, _) = i { Some(t.as_str()) } else { None })
                            .collect(),
                    );
                }
            }
        }
    }
    v
}

#[test]
fn a_pasted_text_box_is_saved_with_its_own_text() {
    let rect = sp(0, 0, 150, 60, "rect", "", "FFFFFF");
    let boxed =
        rect.replace("<wps:bodyPr/>", r#"<wps:txbx><w:txbxContent><w:p><w:r><w:t>A</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/>"#);
    let mut e = editor(&format!(
        "<w:p><w:r><w:t>Pre</w:t></w:r></w:p><w:p><w:r><w:t>Te</w:t></w:r>{}<w:r><w:t>xt</w:t></w:r></w:p><w:p><w:r><w:t>End</w:t></w:r></w:p>",
        anchor(72, 300, 150, 60, 1, false, 1, &boxed)
    ));
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::Copy, fonts());
    let p = para_id(&e, 2);
    e.select_range(p, Unit(3), Unit(3), fonts());
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(box_texts(&e).len(), 2, "{:?}", box_texts(&e));
    click(&mut e, 100.0, 330.0);
    click(&mut e, 100.0, 330.0);
    e.execute(Command::InsertText("B".into()), fonts());
    let model = box_texts(&e);
    assert!(model.iter().any(|t| t.contains('B')), "typed into a box: {model:?}");
    let (again, _) = reopen(&e);
    let mut want = model;
    let mut got = box_texts(&again);
    want.sort();
    got.sort();
    assert_eq!(got, want);
}

#[test]
fn a_pasted_group_keeps_its_parts_text() {
    use crate::layout::tests::shapes::PT;
    let boxed = sp(0, 0, 80, 80, "rect", "", "FFFFFF")
        .replace("<wps:bodyPr/>", r#"<wps:txbx><w:txbxContent><w:p><w:r><w:t>G</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/>"#);
    let group = format!(
        r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="0" y="0"/><a:chExt cx="{w}" cy="{h}"/></a:xfrm></wpg:grpSpPr>{boxed}{}</wpg:wgp>"#,
        sp(120, 0, 80, 80, "rect", "", "00FF00"),
        w = 200 * PT,
        h = 100 * PT,
    );
    let mut e = editor(&format!(
        "<w:p><w:r><w:t>Te</w:t></w:r>{}<w:r><w:t>xt</w:t></w:r></w:p><w:p><w:r><w:t>End</w:t></w:r></w:p>",
        anchor(72, 300, 200, 100, 1, false, 1, &group)
    ));
    let parts = |e: &Editor| -> Vec<String> {
        let mut v = Vec::new();
        for b in &e.document().sections[0].blocks {
            let Block::Para(p) = b else { continue };
            for i in &p.inlines {
                let Inline::Float(f, _) = i else { continue };
                for tb in f.parts.iter().filter_map(|g| g.text_box.as_ref()) {
                    v.push(
                        tb.blocks
                            .iter()
                            .filter_map(|b| if let Block::Para(q) = b { Some(q) } else { None })
                            .flat_map(|q| q.inlines.iter())
                            .filter_map(|i| if let Inline::Text(t, _) = i { Some(t.as_str()) } else { None })
                            .collect(),
                    );
                }
            }
        }
        v
    };
    assert_eq!(parts(&e), ["G"]);
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::Copy, fonts());
    let p = para_id(&e, 1);
    e.select_range(p, Unit(3), Unit(3), fonts());
    e.execute(Command::Paste(Default::default()), fonts());
    assert_eq!(parts(&e), ["G", "G"]);
    let (again, _) = reopen(&e);
    assert_eq!(parts(&again), parts(&e));
}

fn inline_text_shape() -> Editor {
    use crate::layout::tests::shapes::PT;
    let shape = r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="W" cy="H"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="DDEEFF"/></a:solidFill></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr lIns="91440" tIns="45720" rIns="91440" bIns="45720"/></wps:wsp>"#
        .replace('W', &(144 * PT).to_string())
        .replace('H', &(72 * PT).to_string());
    editor(&format!(
        r#"<w:p><w:r><w:t xml:space="preserve">Before </w:t></w:r><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{w}" cy="{h}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="1" name="Text Box 1"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{shape}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:r><w:t xml:space="preserve"> after</w:t></w:r></w:p>"#,
        w = 144 * PT,
        h = 72 * PT
    ))
}

fn inline_box(e: &Editor) -> Option<String> {
    e.document().sections[0].blocks.iter().find_map(|b| match b {
        Block::Para(p) => p.inlines.iter().find_map(|i| match i {
            Inline::Picture(pic, _) => pic.text.as_ref().map(|t| {
                t.text_box
                    .blocks
                    .iter()
                    .filter_map(|b| if let Block::Para(q) = b { Some(q) } else { None })
                    .flat_map(|q| q.inlines.iter())
                    .filter_map(|i| if let Inline::Text(t, _) = i { Some(t.as_str()) } else { None })
                    .collect()
            }),
            _ => None,
        }),
        _ => None,
    })
}

#[test]
fn an_inline_text_box_edit_survives_an_edit_of_its_paragraph() {
    let mut e = inline_text_shape();
    let snap = e.snapshot();
    let (x, y) = crate::layout::traverse::paint_items(&snap.pages()[0])
        .iter()
        .find_map(|it| match it {
            crate::layout::fragment::Item::Glyphs(g) if g.chars.iter().map(|c| c.0).collect::<String>().contains("Inside") => {
                Some((g.x + 2.0, g.baseline - 4.0))
            }
            _ => None,
        })
        .expect("the shape's text is drawn");
    click(&mut e, x, y);
    click(&mut e, x, y);
    e.execute(Command::InsertText("X".into()), fonts());
    let box_now = inline_box(&e).expect("box");
    assert!(box_now.contains('X'), "{box_now}");
    let p = para_id(&e, 0);
    e.select_range(p, Unit(0), Unit(0), fonts());
    e.execute(Command::InsertText("y".into()), fonts());
    assert_eq!(inline_box(&e).as_deref(), Some(box_now.as_str()));
    let (again, _) = reopen(&e);
    assert_eq!(inline_box(&again).as_deref(), Some(box_now.as_str()));
}

#[test]
fn a_cell_content_control_survives_inserting_a_column() {
    use crate::api::command::{HorizontalSide, InsertColumns};
    let body = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:sdt><w:sdtPr><w:tag w:val="t"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>v</w:t></w:r></w:p></w:sdtContent></w:sdt></w:tc></w:tr></w:tbl><w:p/>"#;
    let mut e = editor(body);
    let p = para_id(&e, 0);
    e.select_range(p, Unit(0), Unit(0), fonts());
    e.execute(Command::InsertColumns(InsertColumns { side: HorizontalSide::Left, count: None }), fonts());
    let (again, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    assert!(xml.contains(r#"<w:tag w:val="t""#), "the control's tag is kept: {xml}");
    assert!(again.document().controls.values().any(|c| c.props.tag.as_deref() == Some("t")), "{xml}");
}

fn count(xml: &str, what: &str) -> usize {
    xml.matches(what).count()
}

#[test]
fn markup_between_rows_and_paragraphs_survives_rewriting_the_rows() {
    use crate::api::command::{HorizontalSide, InsertColumns};
    let cell = |x: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{x}</w:tc>");
    let p = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let row1 = format!("<w:tr>{}<w:bookmarkStart w:id=\"4\" w:name=\"row\"/>{}<w:bookmarkEnd w:id=\"4\"/></w:tr>", cell(&p("a")), cell(&p("b")));
    let row2 = format!(
        "<w:tr>{}{}</w:tr>",
        cell(&p("c")),
        cell(&format!("{}<w:bookmarkStart w:id=\"3\" w:name=\"mid\"/>{}<w:bookmarkEnd w:id=\"3\"/>", p("d"), p("e")))
    );
    let body = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:permStart w:id=\"7\" w:edGrp=\"everyone\"/>{row1}<w:permEnd w:id=\"7\"/>{row2}</w:tbl><w:p/>"
    );
    let mut e = editor(&body);
    let p = para_id(&e, 0);
    e.select_range(p, Unit(0), Unit(0), fonts());
    e.execute(Command::InsertColumns(InsertColumns { side: HorizontalSide::Left, count: None }), fonts());
    let (again, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    for m in [
        "<w:permStart w:id=\"7\"",
        "<w:permEnd w:id=\"7\"",
        "w:name=\"mid\"",
        "<w:bookmarkEnd w:id=\"3\"",
        "w:name=\"row\"",
        "<w:bookmarkEnd w:id=\"4\"",
    ] {
        assert_eq!(count(&xml, m), 1, "{m}: {xml}");
    }
    assert!(
        xml.find("w:name=\"mid\"") > xml.find(">d<") && xml.find("w:name=\"mid\"") < xml.find(">e<"),
        "the bookmark stays between d and e: {xml}"
    );
    assert_eq!(super::texts(&again).len(), super::texts(&e).len());
}

#[test]
fn a_bookmark_read_between_paragraphs_is_written_once() {
    // Another bookmark read from outside the paragraph deleted: the paragraph is written from the model.
    let body = concat!(
        r#"<w:bookmarkStart w:id="1" w:name="b"/><w:bookmarkStart w:id="2" w:name="c"/><w:p><w:r><w:t>Text</w:t></w:r></w:p>"#,
        r#"<w:bookmarkEnd w:id="2"/><w:bookmarkEnd w:id="1"/><w:p><w:r><w:t>After</w:t></w:r></w:p>"#
    );
    let mut e = editor(body);
    let p = para_id(&e, 0);
    e.select_range(p, Unit(4), Unit(4), fonts());
    e.execute(Command::InsertText("!".into()), fonts());
    assert!(matches!(e.execute(Command::DeleteBookmark("c".into()), fonts()), crate::editing::Outcome::Changed(_)));
    let (again, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    assert_eq!(count(&xml, "w:name=\"b\""), 1, "{xml}");
    assert_eq!(super::texts(&again)[0], "Text!");
}

#[test]
fn a_bookmark_in_a_new_text_box_takes_an_id_of_its_own() {
    let mut e = editor(r#"<w:p><w:bookmarkStart w:id="0" w:name="_GoBack"/><w:r><w:t>Body</w:t></w:r><w:bookmarkEnd w:id="0"/></w:p>"#);
    let p = para_id(&e, 0);
    e.select_range(p, Unit(2), Unit(2), fonts());
    assert!(matches!(e.execute(Command::InsertTextBox, fonts()), crate::editing::Outcome::Changed(_)));
    e.execute(Command::InsertText("Boxed".into()), fonts());
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(e.execute(Command::InsertBookmark("inbox".into()), fonts()), crate::editing::Outcome::Changed(_)));
    let (_, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    let d = roxmltree::Document::parse(&xml).unwrap();
    let ids: Vec<(&str, &str)> = d
        .descendants()
        .filter(|n| n.tag_name().name() == "bookmarkStart")
        .map(|n| (n.attribute((crate::docx::xml::W, "name")).unwrap(), n.attribute((crate::docx::xml::W, "id")).unwrap()))
        .collect();
    assert!(ids.iter().any(|(n, _)| *n == "inbox"), "{xml}");
    let mut only: Vec<&str> = ids.iter().map(|(_, id)| *id).collect();
    only.sort_unstable();
    only.dedup();
    assert_eq!(only.len(), ids.len(), "bookmark ids are the part's own: {ids:?}");
}

#[test]
fn drawing_ids_stay_unique_across_parts() {
    use crate::drawing::NewShape;
    use crate::drawing::geom::Preset;
    let mut e = Editor::open(&super::blank_with("Text"), fonts()).expect("opens");
    e.complete(fonts());
    e.insert_shape_sized(NewShape::Shape(Preset::Rect), [72.0, 72.0], fonts());
    e.execute(Command::EditHeader, fonts());
    assert!(matches!(e.insert_shape_sized(NewShape::Shape(Preset::Rect), [72.0, 72.0], fonts()), crate::editing::Outcome::Changed(_)));
    e.execute(Command::CloseHeaderFooter, fonts());
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).filter(|n| n.ends_with(".xml")).collect();
    let mut ids = Vec::new();
    for n in names {
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut z.by_name(&n).unwrap(), &mut xml).unwrap();
        ids.extend(
            xml.match_indices("<wp:docPr ").map(|(i, _)| xml[i..].split("id=\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string()),
        );
    }
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn drawing_ids_stay_unique_across_saves() {
    use crate::drawing::NewShape;
    use crate::drawing::geom::Preset;
    let mut e = Editor::open(&super::blank_with("Text"), fonts()).expect("opens");
    e.complete(fonts());
    e.insert_shape_sized(NewShape::Shape(Preset::Rect), [72.0, 72.0], fonts());
    let (mut e, _) = reopen(&e);
    e.complete(fonts());
    e.insert_shape_sized(NewShape::Shape(Preset::Rect), [200.0, 200.0], fonts());
    let (_, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    let mut v: Vec<String> = xml
        .match_indices("<wp:docPr ")
        .map(|(i, _)| xml[i..].split("id=\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string())
        .collect();
    assert!(v.len() >= 2, "{v:?}");
    let n = v.len();
    v.sort();
    v.dedup();
    assert_eq!(v.len(), n, "docPr ids are unique: {xml}");
}

#[test]
fn bookmark_ids_stay_unique_across_parts() {
    let mut e = Editor::open(&super::blank_with("Text"), fonts()).expect("opens");
    e.complete(fonts());
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(e.execute(Command::InsertBookmark("body".into()), fonts()), crate::editing::Outcome::Changed(_)));
    e.execute(Command::EditHeader, fonts());
    e.execute(Command::InsertText("Head".into()), fonts());
    e.execute(Command::SelectAll, fonts());
    assert!(matches!(e.execute(Command::InsertBookmark("head".into()), fonts()), crate::editing::Outcome::Changed(_)));
    e.execute(Command::CloseHeaderFooter, fonts());
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).filter(|n| n.ends_with(".xml")).collect();
    let mut starts = Vec::new();
    for n in names {
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut z.by_name(&n).unwrap(), &mut xml).unwrap();
        let Ok(d) = roxmltree::Document::parse(&xml) else { continue };
        let ids = |el: &str| -> Vec<String> {
            d.descendants()
                .filter(|x| x.tag_name().name() == el)
                .filter_map(|x| x.attribute((crate::docx::xml::W, "id")).map(str::to_string))
                .collect()
        };
        let (s, mut e) = (ids("bookmarkStart"), ids("bookmarkEnd"));
        let mut s2 = s.clone();
        s2.sort();
        e.sort();
        assert_eq!(s2, e, "{n}: each start has its end");
        starts.extend(s);
    }
    assert_eq!(starts.len(), 2, "{starts:?}");
    assert_ne!(starts[0], starts[1]);
}

fn cell_bookmark_table() -> Editor {
    let cell = |x: &str| format!("<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{x}</w:tc>");
    let p = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let row = format!(
        "<w:tr>{}{}</w:tr>",
        cell(&p("c")),
        cell(&format!("{}<w:bookmarkStart w:id=\"3\" w:name=\"mid\"/>{}<w:bookmarkEnd w:id=\"3\"/>", p("d"), p("e")))
    );
    editor(&format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>{row}</w:tbl><w:p/>"
    ))
}

fn insert_column(e: &mut Editor) {
    use crate::api::command::{HorizontalSide, InsertColumns};
    let p = para_id(e, 0);
    e.select_range(p, Unit(0), Unit(0), fonts());
    e.execute(Command::InsertColumns(InsertColumns { side: HorizontalSide::Left, count: None }), fonts());
}

#[test]
fn a_cell_bookmark_deleted_before_its_row_is_rewritten_stays_deleted() {
    let mut e = cell_bookmark_table();
    assert!(matches!(e.execute(Command::DeleteBookmark("mid".into()), fonts()), crate::editing::Outcome::Changed(_)));
    insert_column(&mut e);
    let (again, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    assert_eq!(count(&xml, "w:name=\"mid\""), 0, "{xml}");
    assert_eq!(count(&xml, "<w:bookmarkEnd w:id=\"3\""), 0, "{xml}");
    assert_eq!(super::texts(&again).len(), super::texts(&e).len());
}

#[test]
fn a_cell_bookmark_joined_into_the_paragraph_before_is_written_once() {
    use crate::api::command::DeleteOptions;
    use crate::api::value::Direction;
    let mut e = cell_bookmark_table();
    let p = para_id(&e, 2);
    e.select_range(p, Unit(0), Unit(0), fonts());
    e.execute(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts());
    assert_eq!(super::texts(&e)[1], "de");
    insert_column(&mut e);
    let (_, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    assert_eq!(count(&xml, "w:name=\"mid\""), 1, "{xml}");
    assert_eq!(count(&xml, "<w:bookmarkEnd w:id=\"3\""), 1, "{xml}");
}

#[test]
fn a_body_bookmark_read_between_paragraphs_can_be_deleted() {
    let body =
        r#"<w:p><w:r><w:t>a</w:t></w:r></w:p><w:bookmarkStart w:id="1" w:name="mid"/><w:p><w:r><w:t>b</w:t></w:r></w:p><w:bookmarkEnd w:id="1"/>"#;
    let mut e = editor(body);
    assert!(matches!(e.execute(Command::DeleteBookmark("mid".into()), fonts()), crate::editing::Outcome::Changed(_)));
    let (_, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    assert_eq!(count(&xml, "w:name=\"mid\""), 0, "{xml}");
    assert_eq!(count(&xml, "<w:bookmarkEnd w:id=\"1\""), 0, "{xml}");
}

#[test]
fn a_bookmark_whose_start_paragraph_went_keeps_both_ends() {
    use crate::api::command::DeleteOptions;
    use crate::api::value::Direction;
    let mut e = editor(
        r#"<w:bookmarkStart w:id="8" w:name="Around"/><w:p><w:r><w:t>Previous</w:t></w:r></w:p><w:p><w:r><w:t>Text</w:t></w:r></w:p><w:bookmarkEnd w:id="8"/><w:p/>"#,
    );
    let (p, k) = (para_id(&e, 0), para_id(&e, 1));
    e.select_between((p, Unit(0)), (k, Unit(0)), fonts());
    e.execute(Command::Delete(DeleteOptions { direction: Direction::Backward, unit: None }), fonts());
    let (_, bytes) = reopen(&e);
    let xml = part(bytes, "word/document.xml");
    assert_eq!(count(&xml, "w:name=\"Around\""), 1, "{xml}");
    assert_eq!(count(&xml, "<w:bookmarkEnd w:id=\"8\""), 1, "{xml}");
}
