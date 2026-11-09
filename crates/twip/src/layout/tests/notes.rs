//! Notes in rewound chains

use crate::docx::model::next_para_id;
use crate::docx::{Block, Inline, NoteKind};
use crate::layout::cache::LayoutCache;
use crate::layout::document::layout_records;
use crate::layout::fragment::{BoxKind, Child, PageChild, PageFragment};

use super::{fixture, fonts};

fn shows_note(page: &PageFragment, id: i64) -> bool {
    fn walk(children: &[Child], id: i64) -> bool {
        children.iter().any(|c| match c {
            Child::Box(b) => matches!(b.kind, BoxKind::Note { kind: NoteKind::Foot, id: i } if i == id) || walk(&b.children, id),
            _ => false,
        })
    }
    page.children.iter().any(|c| matches!(c, PageChild::Body(b) if walk(&b.children, id)))
}

#[test]
fn rewound_note_follows_reference() {
    const ID: i64 = 9001;
    let Some(mut doc) = fixture(r"K1\K1b_keepnext.docx") else { return };
    let blocks = &mut doc.sections[0].blocks;
    let heading = blocks.iter().position(|b| matches!(b, Block::Para(p) if p.style.keep_next)).expect("K1b has a keepNext heading");
    let Some(Block::Para(body)) = blocks.iter().find(|b| matches!(b, Block::Para(p) if !p.style.keep_next)).cloned() else { unreachable!() };
    let Block::Para(p) = &mut blocks[heading] else { unreachable!() };
    p.inlines.push(Inline::NoteRef { kind: NoteKind::Foot, id: ID, custom: false, run: p.mark.clone() });
    let mut note = body;
    note.inlines = vec![Inline::Text("The note's text.".into(), note.mark.clone())];
    note.id = next_para_id();
    doc.footnotes.insert(ID, vec![Block::Para(note)]);
    let (snapshot, records) = layout_records(&doc, &fonts(), &mut LayoutCache::new());
    let pages: Vec<usize> = snapshot.pages().iter().enumerate().filter(|(_, p)| shows_note(p, ID)).map(|(i, _)| i).collect();
    assert_eq!(pages, [1], "the note is drawn once, on page 2 with its reference");
    assert!(!records[0].effects.notes_shown.iter().any(|n| n.1 == ID), "page 1's effects don't show the rewound note");
    assert!(records[1].effects.notes_shown.iter().any(|n| (n.0, n.1) == (NoteKind::Foot, ID)), "page 2's effects show it");
}

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const PAGE: &str = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;

/// A paragraph of `text` referencing footnotes (`f<id>`, `c<id>` with a custom mark `*`) and endnotes (`e<id>`).
fn para(text: &str, refs: &[&str]) -> String {
    let mut s = format!(r#"<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#);
    for r in refs {
        let (kind, id) = r.split_at(1);
        s += &match kind {
            "f" => format!(r#"<w:r><w:footnoteReference w:id="{id}"/></w:r>"#),
            "c" => format!(r#"<w:r><w:footnoteReference w:customMarkFollows="1" w:id="{id}"/><w:t>*</w:t></w:r>"#),
            _ => format!(r#"<w:r><w:endnoteReference w:id="{id}"/></w:r>"#),
        };
    }
    s + "</w:p>"
}

/// A section's closing `w:sectPr` (in a paragraph when `last` is false) with `extra` properties.
fn sect(extra: &str, last: bool) -> String {
    let s = format!("<w:sectPr>{extra}{PAGE}</w:sectPr>");
    if last { s } else { format!("<w:p><w:pPr>{s}</w:pPr></w:p>") }
}

/// A document of `body`, with `settings` children and notes 1..=9 of each kind.
fn notes_doc(body: &str, settings: &str) -> Option<crate::docx::Document> {
    let note =
        |tag: &str| -> String { (1..=9).map(|i| format!(r#"<w:{tag} w:id="{i}"><w:p><w:r><w:t>Note {i}</w:t></w:r></w:p></w:{tag}>"#)).collect() };
    let (foot, end) = (format!("<w:footnotes {W}>{}</w:footnotes>", note("footnote")), format!("<w:endnotes {W}>{}</w:endnotes>", note("endnote")));
    let ct = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/><Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    super::package(&[
        ("[Content_Types].xml", ct),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", &format!("<w:document {W}><w:body>{body}</w:body></w:document>")),
        ("word/settings.xml", &format!("<w:settings {W}>{settings}</w:settings>")),
        ("word/footnotes.xml", &foot),
        ("word/endnotes.xml", &end),
    ])
}

fn numbers(doc: &crate::docx::Document, kind: NoteKind, ids: &[i64]) -> Vec<String> {
    let notes = crate::layout::document::JobNotes::new(doc);
    ids.iter().map(|id| notes.numbers.get(&(kind, *id)).cloned().unwrap_or_default()).collect()
}

#[test]
fn note_numbers_come_from_the_referencing_section() {
    let fmt = |f: &str, extra: &str| format!(r#"<w:footnotePr><w:numFmt w:val="{f}"/>{extra}</w:footnotePr>"#);
    let body = para("a", &["f1", "f2"]) + &sect(&fmt("decimal", ""), false) + &para("b", &["f3", "f4"]) + &sect(&fmt("lowerRoman", ""), true);
    let settings = r#"<w:footnotePr><w:numFmt w:val="upperLetter"/><w:numStart w:val="5"/></w:footnotePr>"#;
    let Some(doc) = notes_doc(&body, settings) else { return };
    assert_eq!(numbers(&doc, NoteKind::Foot, &[1, 2, 3, 4]), ["1", "2", "iii", "iv"], "settings ignored; S2 counts on in its own format");
    let restart = r#"<w:numStart w:val="10"/><w:numRestart w:val="eachSect"/>"#;
    let body = para("a", &["f1", "f2"]) + &sect("", false) + &para("b", &["f3", "f4"]) + &sect(&fmt("decimal", restart), true);
    let Some(doc) = notes_doc(&body, "") else { return };
    assert_eq!(numbers(&doc, NoteKind::Foot, &[1, 2, 3, 4]), ["1", "2", "10", "11"]);
    let body = para("a", &["f1", "c2", "f3"]) + &sect(&fmt("chicago", ""), true);
    let Some(doc) = notes_doc(&body, "") else { return };
    assert_eq!(numbers(&doc, NoteKind::Foot, &[1, 2, 3]), ["*", "\u{2020}", "\u{2020}"], "a custom mark takes no number; its note shows the next");
    let body = para("a", &["e1", "e2"]) + &sect(r#"<w:endnotePr><w:numFmt w:val="decimal"/><w:numStart w:val="4"/></w:endnotePr>"#, true);
    let Some(doc) = notes_doc(&body, r#"<w:endnotePr><w:numFmt w:val="upperRoman"/></w:endnotePr>"#) else { return };
    assert_eq!(numbers(&doc, NoteKind::End, &[1, 2]), ["4", "5"]);
}

#[test]
fn beneath_text_footnotes_follow_the_text() {
    let note_y = |pos: &str, settings: &str| {
        let pr = format!(r#"<w:footnotePr><w:pos w:val="{pos}"/></w:footnotePr>"#);
        let body = para("Body", &["f1"]) + &sect(&pr, true);
        let doc = notes_doc(&body, settings)?;
        super::page_chars(&doc)[0].iter().find(|c| c.0 == 'N').map(|c| c.2)
    };
    let settings = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let (Some(beneath), Some(bottom)) = (note_y("beneathText", ""), note_y("pageBottom", settings)) else { return };
    assert!(beneath < 150.0, "under the one body line: {beneath}");
    assert!(bottom > 650.0, "settings' position is ignored: {bottom}");
}

#[test]
fn section_end_endnotes_wait_for_a_section_with_its_own() {
    let body = para("S1", &["e1"])
        + &sect("<w:noEndnote/>", false)
        + &para("S2", &["e2"])
        + &sect("", false)
        + &para("S3", &["e3"])
        + &sect(r#"<w:endnotePr><w:pos w:val="docEnd"/></w:endnotePr>"#, true);
    let pages = |settings: &str| -> Option<Vec<String>> {
        let doc = notes_doc(&body, settings)?;
        Some(super::page_chars(&doc).iter().map(|p| p.iter().map(|c| c.0).collect()).collect())
    };
    let Some(sect_end) = pages(r#"<w:endnotePr><w:pos w:val="sectEnd"/></w:endnotePr>"#) else { return };
    let on = |pages: &[String], n: &str| pages.iter().position(|p| p.contains(&format!("Note {n}")));
    assert_eq!([on(&sect_end, "1"), on(&sect_end, "2"), on(&sect_end, "3")], [Some(1), Some(1), Some(2)], "{sect_end:?}");
    let Some(doc_end) = pages("") else { return };
    assert_eq!([on(&doc_end, "1"), on(&doc_end, "3")], [Some(2), Some(2)], "the section's pos is ignored: {doc_end:?}");
}
