//! Deleting a paragraph's break into a following table

use std::io::Read;

use super::super::Editor;
use super::super::commands::{Command, Dir, MoveBy};
use super::{blank_body, del, fonts, mv, texts};

fn saved_body(e: &Editor) -> String {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a package");
    let mut s = String::new();
    z.by_name("word/document.xml").expect("document.xml").read_to_string(&mut s).expect("utf8");
    s
}

const BEFORE_A_TABLE: &str = "<w:p><w:bookmarkStart w:id=\"7\" w:name=\"mark\"/><w:r><w:t>Hello</w:t></w:r><w:bookmarkEnd w:id=\"7\"/></w:p>\
    <w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid>\
    <w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>two</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>";

fn opened() -> Editor {
    let mut e = Editor::open(&blank_body(BEFORE_A_TABLE), fonts()).expect("opens");
    e.complete(fonts());
    e.execute(mv(MoveBy::Story, Dir::Back, false), fonts());
    e
}

#[test]
fn delete_at_the_end_moves_the_paragraphs_bookmark_into_the_first_cell() {
    let mut e = opened();
    e.execute(mv(MoveBy::LineEdge, Dir::Forward, false), fonts());
    e.execute(del(Dir::Forward, false), fonts());
    assert!(texts(&e).iter().any(|t| t == "Hellocell"), "{:?}", texts(&e));
    let xml = saved_body(&e);
    assert!(xml.contains("w:name=\"mark\"") && xml.contains("bookmarkEnd"), "{xml}");
    e.execute(Command::Undo(Default::default()), fonts());
    assert!(texts(&e).iter().any(|t| t == "Hello"), "{:?}", texts(&e));
    assert!(saved_body(&e).contains("w:name=\"mark\""));
}

#[test]
fn deleting_a_range_into_the_first_row_moves_the_paragraphs_bookmark() {
    let mut e = opened();
    e.execute(mv(MoveBy::Char, Dir::Forward, false), fonts());
    for _ in 0..5 {
        e.execute(mv(MoveBy::Char, Dir::Forward, true), fonts());
    }
    e.execute(del(Dir::Back, false), fonts());
    assert!(texts(&e).iter().any(|t| t == "Htwo"), "{:?}", texts(&e));
    let xml = saved_body(&e);
    assert!(xml.contains("w:name=\"mark\""), "{xml}");
}
