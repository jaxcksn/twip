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
    notes_doc_with(body, settings, "")
}

/// [`notes_doc`] with `extra` notes (footnotes and endnotes, each part reading its own).
fn notes_doc_with(body: &str, settings: &str, extra: &str) -> Option<crate::docx::Document> {
    let note =
        |tag: &str| -> String { (1..=9).map(|i| format!(r#"<w:{tag} w:id="{i}"><w:p><w:r><w:t>Note {i}</w:t></w:r></w:p></w:{tag}>"#)).collect() };
    let (foot, end) =
        (format!("<w:footnotes {W}>{extra}{}</w:footnotes>", note("footnote")), format!("<w:endnotes {W}>{extra}{}</w:endnotes>", note("endnote")));
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

/// Two columns of ten lines, the fifth referencing endnote 1, its endnotes at the section end, then a continuous
/// section: each page's characters.
fn section_end_endnotes_in_columns(settings: &str) -> Option<Vec<Vec<(char, f64, f64)>>> {
    let mut body: String = (1..10).map(|i| para(&format!("L{i:02}"), if i == 5 { &["e1"][..] } else { &[] })).collect();
    body += &format!(r#"<w:p><w:pPr><w:sectPr><w:cols w:num="2" w:space="720"/>{PAGE}</w:sectPr></w:pPr><w:r><w:t>L10</w:t></w:r></w:p>"#);
    body += &(para("Tail", &[]) + &sect(r#"<w:type w:val="continuous"/>"#, true));
    let doc = notes_doc(&body, &format!(r#"<w:endnotePr><w:pos w:val="sectEnd"/></w:endnotePr>{settings}"#))?;
    Some(super::page_chars(&doc))
}

#[test]
fn compat_14_section_end_endnotes_balance_with_the_columns() {
    let Some(pages) = section_end_endnotes_in_columns("") else { return };
    let top = pages[0].iter().filter(|c| c.1 > 300.0).map(|c| c.2).fold(f64::MAX, f64::min);
    let first: String = pages[0].iter().filter(|c| c.1 > 300.0 && c.2 == top).map(|c| c.0).collect();
    assert_eq!(first, "L07", "six lines in column 1, four and the endnote in column 2");
}

#[test]
fn compat_15_section_end_endnotes_start_below_the_balanced_columns() {
    let Some(pages) = section_end_endnotes_in_columns(C15) else { return };
    let note = pages[0].iter().find(|c| c.0 == 'N').expect("the endnote is on page 1");
    let last = pages[0].iter().filter(|c| c.0 == 'L').map(|c| c.2).fold(0.0, f64::max);
    assert!(note.1 < 100.0 && note.2 > last, "in column 1 under the columns: {note:?} {last}");
}

#[test]
fn each_page_footnotes_count_from_their_page() {
    let brk = r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#;
    let pr = r#"<w:footnotePr><w:numRestart w:val="eachPage"/></w:footnotePr>"#;
    let body = para("a", &["f1", "f2"]) + brk + &para("b", &["f3"]) + &sect(pr, true);
    let Some(doc) = notes_doc(&body, "") else { return };
    let pages: Vec<String> = super::page_chars(&doc).iter().map(|p| p.iter().map(|c| c.0).collect()).collect();
    assert!(pages[0].starts_with("a12") && pages[1].starts_with("b1"), "{pages:?}");
}

#[test]
fn compat_14_note_waits_for_the_next_page_behind_its_line() {
    let c15 = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;
    let body = |with: Option<usize>| {
        let mut b = String::new();
        for k in 1..=80 {
            let refs: &[&str] = if k == 1 {
                &["f1"]
            } else if Some(k) == with {
                &["f2"]
            } else {
                &[]
            };
            b += &para(&format!("P{k:02}"), refs);
        }
        b + &sect("", true)
    };
    let pages = |body: &str, settings: &str| -> Option<Vec<String>> {
        Some(super::page_chars(&notes_doc(body, settings)?).iter().map(|p| p.iter().map(|c| c.0).collect()).collect())
    };
    let Some(plain) = pages(&body(None), "") else { return };
    // The last line page 1 holds with only the first note.
    let last = (1..=80).rev().find(|k| plain[0].contains(&format!("P{k:02}"))).expect("page 1 has lines");
    let on = |pages: &[String], s: &str| pages.iter().position(|p| p.contains(s));
    let Some(c12) = pages(&body(Some(last)), "") else { return };
    assert_eq!(on(&c12, &format!("P{last:02}2")), Some(0), "the line stays: {c12:?}");
    assert_eq!(on(&c12, "Note 2"), Some(1), "its note goes whole to the next page");
    let Some(c15) = pages(&body(Some(last)), c15) else { return };
    assert_eq!(on(&c15, &format!("P{last:02}2")), Some(1), "compat 15 moves the line with its note");
}

const C15: &str = r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#;

#[test]
fn compat_15_footnote_area_follows_the_page_text() {
    let pr = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let body = para("A", &["f1"]) + &sect(pr, false) + &para("B", &[]) + &sect(&format!(r#"<w:type w:val="continuous"/>{pr}"#), true);
    let ys = |settings: &str| {
        let doc = notes_doc(&body, settings)?;
        let p = &super::page_chars(&doc)[0];
        let y = |ch: char| p.iter().find(|c| c.0 == ch).map(|c| c.2);
        Some((y('B')?, y('N')?))
    };
    let (Some((b12, n12)), Some((b15, n15))) = (ys(""), ys(C15)) else { return };
    assert!(n12 < b12, "compat 12: under S1, S2 below it: note {n12}, S2 {b12}");
    assert!(n15 > b15 && b15 < b12, "compat 15: S2 right after S1, the area after S2: note {n15}, S2 {b15}");
}

#[test]
fn compat_15_footnote_area_passes_through_columns() {
    let pr = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let next = |cols: u32| format!(r#"<w:type w:val="continuous"/>{pr}<w:cols w:num="{cols}" w:space="720"/>"#);
    let body = |refs: &[&str]| {
        para("A", refs) + &sect(pr, false) + &para("B", &[]) + &para("C", &[]) + &sect(&next(2), false) + &para("D", &[]) + &sect(&next(1), true)
    };
    let chars = |refs: &[&str]| Some(super::page_chars(&notes_doc(&body(refs), C15)?).swap_remove(0));
    let (Some(plain), Some(noted)) = (chars(&[]), chars(&["f1"])) else { return };
    let y = |p: &[(char, f64, f64)], ch: char| p.iter().find(|c| c.0 == ch).map(|c| c.2);
    assert_eq!(y(&plain, 'B'), y(&noted, 'B'), "the columns don't move for the area");
    assert!(y(&noted, 'N') > y(&noted, 'D'), "the area is drawn after the last section's text: {noted:?}");
}

#[test]
fn compat_15_footnote_area_has_the_first_reference_columns() {
    let pr = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let cols = format!(r#"{pr}<w:cols w:num="2" w:space="720"/>"#);
    let next = format!(r#"<w:type w:val="continuous"/>{pr}"#);
    let body = para("A", &["f1"]) + &para("B", &[]) + &sect(&cols, false) + &para("C", &["f2"]) + &sect(&next, true);
    let Some(doc) = notes_doc(&body, C15) else { return };
    let notes: Vec<(f64, f64)> = super::page_chars(&doc)[0].iter().filter(|c| c.0 == 'N').map(|c| (c.1, c.2)).collect();
    assert_eq!(notes.len(), 2);
    assert!((notes[1].0 - 324.0).abs() < 0.5 && (notes[1].1 - notes[0].1).abs() < 0.01, "the one-column section's note in column 2: {notes:?}");
}

#[test]
fn compat_14_continuous_break_after_a_page_bottom_note_starts_a_page() {
    let body = para("A", &["f1"]) + &sect("", false) + &para("B", &[]) + &sect(r#"<w:type w:val="continuous"/>"#, true);
    let page_of_b = |settings: &str| {
        let doc = notes_doc(&body, settings)?;
        super::page_chars(&doc).iter().position(|p| p.iter().any(|c| c.0 == 'B'))
    };
    let (Some(c12), Some(c15)) = (page_of_b(""), page_of_b(C15)) else { return };
    assert_eq!((c12, c15), (1, 0));
}

#[test]
fn compat_14_columns_before_a_page_bottom_note_break_stay_unbalanced() {
    let cols = r#"<w:cols w:num="2" w:space="720"/>"#;
    let body = para("A", &["f1"]) + &para("B", &[]) + &sect(cols, false) + &para("C", &[]) + &sect(r#"<w:type w:val="continuous"/>"#, true);
    let Some(doc) = notes_doc(&body, "") else { return };
    let x = |ch: char| super::page_chars(&doc)[0].iter().find(|c| c.0 == ch).map(|c| c.1);
    assert_eq!(x('A'), x('B'), "both paragraphs in column 1");
}

#[test]
fn compat_14_note_waits_when_its_paragraph_cant_keep_its_lines() {
    let body = |at: Option<usize>| {
        let mut b = para("P01", &["f1"]);
        let mut k = 2;
        while k <= 80 {
            if Some(k) == at {
                b += r#"<w:p><w:pPr><w:widowControl/></w:pPr><w:r><w:t>Q1</w:t></w:r><w:r><w:footnoteReference w:id="10"/></w:r><w:r><w:br/><w:t>Q2</w:t><w:br/><w:t>Q3</w:t></w:r></w:p>"#;
                k += 3;
            } else {
                b += &para(&format!("P{k:02}"), &[]);
                k += 1;
            }
        }
        b + &sect("", true)
    };
    let long = r#"<w:footnote w:id="10"><w:p><w:pPr><w:widowControl/></w:pPr><w:r><w:t>Long</w:t><w:br/><w:t>note</w:t></w:r></w:p></w:footnote>"#;
    let pages = |body: &str, settings: &str| -> Option<Vec<String>> {
        Some(super::page_chars(&notes_doc_with(body, settings, long)?).iter().map(|p| p.iter().map(|c| c.0).collect()).collect())
    };
    let Some(plain) = pages(&body(None), "") else { return };
    let last = (1..=80).rev().find(|k| plain[0].contains(&format!("P{k:02}"))).expect("page 1 has lines");
    let on = |pages: &[String], s: &str| pages.iter().position(|p| p.contains(s));
    // A 3-line paragraph ending on page 1's last line: its 2-line note fits under its first line only.
    let Some(c12) = pages(&body(Some(last - 2)), "") else { return };
    assert_eq!((on(&c12, "Q3"), on(&c12, "Long")), (Some(0), Some(1)), "the lines stay, the note waits: {c12:?}");
    let Some(c15) = pages(&body(Some(last - 2)), C15) else { return };
    assert_eq!((on(&c15, "Q1"), on(&c15, "Long")), (Some(1), Some(1)), "compat 15 moves the paragraph with its note");
}

#[test]
fn endnote_continuation_separator_spans_the_column() {
    let seps = r#"<w:endnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:endnote><w:endnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:endnote>"#;
    let long: String = (1..=70).map(|i| format!("<w:p><w:r><w:t>E{i}</w:t></w:r></w:p>")).collect();
    let extra = format!(r#"{seps}<w:endnote w:id="10">{long}</w:endnote>"#);
    let body = para("A", &["e10"]) + &sect("", true);
    let Some(doc) = notes_doc_with(&body, C15, &extra) else { return };
    let snap = crate::layout::layout(&doc, &super::fonts());
    let rules: Vec<Vec<f64>> = snap
        .pages()
        .iter()
        .map(|pg| {
            crate::layout::traverse::paint_items(pg)
                .into_iter()
                .filter_map(|i| match i {
                    crate::layout::fragment::Item::Rect { rect, .. } if rect[3] - rect[1] < 1.0 => Some(rect[2] - rect[0]),
                    _ => None,
                })
                .collect()
        })
        .collect();
    assert!(rules.len() > 1 && rules[0] == [144.0], "a new endnote's separator: {rules:?}");
    assert!(rules[1..].iter().all(|r| r.len() == 1 && r[0] > 400.0), "the continued endnote's rule spans the column: {rules:?}");
}

#[test]
fn a_separator_story_counts_only_when_the_settings_list_it() {
    let sep = |before: u32, rpr: &str| {
        format!(
            r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:before="{before}" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r>{rpr}<w:separator/></w:r></w:p></w:footnote>"#
        )
    };
    let pr = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let note_y = |settings: &str, sep: &str| {
        let body = para("Body", &["f1"]) + &sect(pr, true);
        let doc = notes_doc_with(&body, settings, sep)?;
        super::page_chars(&doc)[0].iter().find(|c| c.0 == 'N').map(|c| c.2)
    };
    let listing = r#"<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="0"/></w:footnotePr>"#;
    let (Some(built_in), Some(listed)) = (note_y("", &sep(400, "")), note_y(listing, &sep(400, ""))) else { return };
    assert!((listed - built_in - 20.0).abs() < 0.01, "{built_in} {listed}");
    // The separator's line is in its run's size.
    let (Some(small), Some(big)) = (note_y(listing, &sep(0, "")), note_y(listing, &sep(0, r#"<w:rPr><w:sz w:val="48"/></w:rPr>"#))) else { return };
    assert!(big - small > 8.0, "{small} {big}");
}

#[test]
fn compat_14_separator_line_spacing_is_the_mark_height() {
    let sep = r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:line="360" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:sz w:val="40"/></w:rPr><w:separator/></w:r></w:p></w:footnote>"#;
    let listing = r#"<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="0"/></w:footnotePr>"#;
    let pr = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let note_y = |compat: &str| {
        let doc = notes_doc_with(&(para("Body", &["f1"]) + &sect(pr, true)), &format!("{listing}{compat}"), sep)?;
        super::page_chars(&doc)[0].iter().find(|c| c.0 == 'N').map(|c| c.2)
    };
    let (Some(c12), Some(c15)) = (note_y(""), note_y(C15)) else { return };
    // Half a line more of the 20 pt run in compat 15 than of the 11 pt mark in compat 12.
    assert!(c15 - c12 > 4.0, "{c12} {c15}");
}

#[test]
fn compat_14_separator_line_is_at_least_the_mark_height() {
    let sep = |sz: u32| {
        format!(
            r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:sz w:val="{sz}"/></w:rPr><w:separator/></w:r></w:p></w:footnote>"#
        )
    };
    let listing = r#"<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="0"/></w:footnotePr>"#;
    let pr = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let note_y = |compat: &str, sz: u32| {
        let doc = notes_doc_with(&(para("Body", &["f1"]) + &sect(pr, true)), &format!("{listing}{compat}"), &sep(sz))?;
        super::page_chars(&doc)[0].iter().find(|c| c.0 == 'N').map(|c| c.2)
    };
    let (Some(small), Some(mark)) = (note_y("", 16), note_y("", 20)) else { return };
    assert!((small - mark).abs() < 0.01, "an 8 pt separator run under a 10 pt mark: {small} {mark}");
    let (Some(small), Some(mark)) = (note_y(C15, 16), note_y(C15, 20)) else { return };
    assert!(mark - small > 1.0, "compat 15 leaves the mark out: {small} {mark}");
}

#[test]
fn notes_keep_the_space_after_between_them() {
    let note = |id: u32, after: u32| format!(r#"<w:footnote w:id="{id}"><w:p><w:pPr><w:spacing w:after="{after}"/></w:pPr><w:r><w:t>Q{id}</w:t></w:r></w:p></w:footnote>"#);
    let gap = |after: u32| {
        let body = para("Body", &["f11", "f12"]) + &sect("", true);
        let doc = notes_doc_with(&body, "", &(note(11, after) + &note(12, 0)))?;
        let y: Vec<f64> = super::page_chars(&doc)[0].iter().filter(|c| c.0 == 'Q').map(|c| c.2).collect();
        Some(y[1] - y[0])
    };
    let (Some(none), Some(some)) = (gap(0), gap(200)) else { return };
    assert!((some - none - 10.0).abs() < 0.01, "{none} {some}");
}

#[test]
fn a_body_ending_with_a_table_ends_with_a_paragraph() {
    let tbl = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>T</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let pr = r#"<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>"#;
    let note_y = |body: String| {
        let doc = notes_doc(&body, "")?;
        assert!(matches!(doc.sections[0].blocks.last(), Some(Block::Para(p)) if p.inlines.is_empty()));
        super::page_chars(&doc)[0].iter().find(|c| c.0 == 'N').map(|c| c.2)
    };
    let (Some(implicit), Some(written)) = (note_y(format!("{tbl}{}", sect(pr, true))), note_y(format!("{tbl}<w:p/>{}", sect(pr, true)))) else {
        return;
    };
    assert_eq!(implicit, written, "laid out as a written empty paragraph");
}
