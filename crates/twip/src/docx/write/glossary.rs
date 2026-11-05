//! The glossary part on save: building blocks edited since reading patched into the part as read, ones made
//! appended, ones removed taken out, renamed ones' names changed. Building blocks no edit touched keep their bytes.
//! A document without a glossary part gets one when its first building block is made.

use super::*;

/// The glossary part's main XML type and relationship.
const GLOSSARY_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml";
const GLOSSARY_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/glossaryDocument";

/// The glossary part's new XML (with its relationship and content type when the part is new), when a building block
/// changed. Pushes onto `parts`.
pub(super) fn write(doc: &Document, src: &Source, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let Some(g) = &doc.glossary else { return Ok(()) };
    match &src.glossary {
        Some(gs) => {
            if let Some(xml) = patched(doc, g, gs)? {
                parts.push((gs.part.clone(), g.map.to_glossary_xml(&xml).into_owned().into_bytes()));
            }
        }
        None => {
            let made: Vec<&BuildingBlock> = g.parts.iter().filter(|b| b.state == PartState::Added).collect();
            if made.is_empty() {
                return Ok(());
            }
            let shell = format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:glossaryDocument xmlns:w="{W}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:docParts></w:docParts></w:glossaryDocument>"#
            );
            let mut text = String::new();
            for (n, b) in made.iter().enumerate() {
                text.push_str(&part_xml_of("w:", b, n, &new_blocks_xml(&shell, &b.body, &doc.people)?));
            }
            let xml = shell.replace("<w:docParts></w:docParts>", &format!("<w:docParts>{text}</w:docParts>"));
            let name = "word/glossary/document.xml";
            package::set(parts, name, g.map.to_glossary_xml(&xml).into_owned().into_bytes());
            package::add_part(&src.package, parts, &src.main, name, GLOSSARY_TYPE, GLOSSARY_REL)?;
        }
    }
    Ok(())
}

/// A building block's `w:docPart` with body XML `body`; `n` makes its guid when it has none.
fn part_xml_of(w: &str, b: &BuildingBlock, n: usize, body: &str) -> String {
    let esc = props::esc;
    let guid = b.guid.clone().unwrap_or_else(|| {
        let h = b
            .name
            .bytes()
            .chain(b.category.bytes())
            .fold(0xcbf2_9ce4_8422_2325u64 ^ n as u64, |h, c| (h ^ u64::from(c)).wrapping_mul(0x100_0000_01b3));
        format!(
            "{{{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}}}",
            h >> 32,
            (h >> 16) & 0xFFFF,
            (h >> 4) & 0xFFF,
            h & 0xFFF,
            h.rotate_left(17) & 0xFFFF_FFFF_FFFF
        )
    });
    format!(
        "<{w}docPart><{w}docPartPr><{w}name {w}val=\"{}\"/><{w}category><{w}name {w}val=\"{}\"/><{w}gallery {w}val=\"{}\"/></{w}category>\
         <{w}behaviors><{w}behavior {w}val=\"content\"/></{w}behaviors><{w}guid {w}val=\"{}\"/></{w}docPartPr><{w}docPartBody>{body}</{w}docPartBody></{w}docPart>",
        esc(&b.name),
        esc(&b.category),
        esc(&b.gallery.xml()),
        esc(&guid)
    )
}

/// The glossary part as read with the edits made since, or `None` when nothing changed.
fn patched(doc: &Document, g: &Glossary, gs: &GlossarySource) -> Result<Option<String>, String> {
    let dom = roxmltree::Document::parse(&gs.xml).map_err(|e| e.to_string())?;
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let mut splices: Vec<Splice> = Vec::new();
    let mut lists: Vec<(&[Block], &[Block])> = Vec::new();
    // Building blocks as read pair up with the glossary's first parts, in order.
    for ((range, orig), b) in gs.parts.iter().zip(&g.parts) {
        if b.state == PartState::Removed {
            splices.push((range.clone(), String::new()));
            continue;
        }
        lists.push((orig.as_slice(), b.body.as_slice()));
        let name = dom
            .descendants()
            .find(|n| n.range() == *range)
            .and_then(|n| child(n, "docPartPr"))
            .and_then(|n| child(n, "name"))
            .and_then(|n| n.attributes().find(|a| a.name() == "val" && a.namespace() == Some(W)));
        if let Some(a) = name.filter(|a| a.value() != b.name) {
            splices.push((a.range_value(), props::esc(&b.name)));
        }
    }
    let mut text = String::new();
    for (n, b) in g.parts.iter().enumerate().skip(gs.parts.len()).filter(|(_, b)| b.state == PartState::Added) {
        text.push_str(&part_xml_of(&w, b, n, &new_blocks_xml(&gs.xml, &b.body, &doc.people)?));
    }
    if !text.is_empty() {
        let list = dom.descendants().find(|n| is_w(*n, "docParts")).ok_or("a glossary without its parts list")?;
        let r = list.range();
        let close = gs.xml[..r.end].rfind("</").filter(|&c| c > r.start).ok_or("a self-closed parts list")?;
        splices.push((close..close, text));
    }
    let (orig, now): (Vec<&[Block]>, Vec<&[Block]>) = lists.iter().copied().unzip();
    part_xml_with(&gs.xml, &boxes(&orig, &now), &lists, false, &splices, &[], &doc.controls, &gone_controls(doc), &doc.people)
}

#[cfg(test)]
mod tests {
    use super::super::super::*;
    use std::io::Read;

    fn fixture() -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
        std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        (0..z.len())
            .map(|i| {
                let mut f = z.by_index(i).unwrap();
                let mut data = Vec::new();
                f.read_to_end(&mut data).unwrap();
                (f.name().unwrap().to_string(), data)
            })
            .collect()
    }

    fn doc_part<'a>(xml: &'a str, name: &str) -> &'a str {
        let at = xml.find(&format!("w:val=\"{name}\"")).unwrap();
        let start = xml[..at].rfind("<w:docPart>").unwrap();
        let end = at + xml[at..].find("</w:docPart>").unwrap() + "</w:docPart>".len();
        &xml[start..end]
    }

    #[test]
    fn an_unedited_glossary_saves_as_read() {
        let bytes = fixture();
        let doc = read_editable(&bytes).unwrap();
        assert_eq!(write::save(&doc).unwrap(), bytes);
    }

    #[test]
    fn an_edited_body_changes_only_its_building_block() {
        let bytes = fixture();
        let mut doc = read_editable(&bytes).unwrap();
        let g = doc.glossary.as_mut().unwrap();
        let Block::Para(p) = &mut g.parts[0].body[0] else { panic!("a paragraph") };
        let Some(Inline::Text(t, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..))) else { panic!("text") };
        *t = "Violation record".into();
        let saved = write::save(&doc).unwrap();
        let (before, after) = (entries(&bytes), entries(&saved));
        assert_eq!(before.len(), after.len());
        for ((n1, d1), (n2, d2)) in before.iter().zip(&after) {
            assert_eq!(n1, n2);
            if n1 != "word/glossary/document.xml" {
                assert_eq!(d1, d2, "{n1} changed");
            }
        }
        let old = String::from_utf8(before.iter().find(|(n, _)| n == "word/glossary/document.xml").unwrap().1.clone()).unwrap();
        let new = String::from_utf8(after.iter().find(|(n, _)| n == "word/glossary/document.xml").unwrap().1.clone()).unwrap();
        let violation = doc_part(&new, "Violation");
        assert!(violation.contains("Violation record"));
        assert!(violation.contains(r#"<w:pStyle w:val="Heading1"/>"#));
        // Everything outside Violation's building block is as read, ids included.
        assert_eq!(new.replace(violation, ""), old.replace(doc_part(&old, "Violation"), ""));
        assert!(!new.contains("~g~"));
        assert_eq!(doc_part(&new, "Greeting"), doc_part(&old, "Greeting"));
        // It reads back with the edit.
        let again = read_editable(&saved).unwrap();
        let Block::Para(p) = &again.glossary.as_ref().unwrap().parts[0].body[0] else { panic!("a paragraph") };
        assert!(p.inlines.iter().any(|i| matches!(i, Inline::Text(s, _) if s == "Violation record")));
    }
}

#[cfg(test)]
mod editing_tests {
    use crate::api::command::{Command, SubTemplateName};
    use crate::docx::{Block, Inline};
    use crate::editing::tests::{fonts, open};

    fn glossary_xml(bytes: &[u8]) -> String {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut f = z.by_name("word/glossary/document.xml").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        s
    }

    fn violation(xml: &str) -> &str {
        let at = xml.find(r#"w:val="Violation""#).unwrap();
        let start = xml[..at].rfind("<w:docPart>").unwrap();
        &xml[start..at + xml[at..].find("</w:docPart>").unwrap()]
    }

    fn edit(e: &mut crate::editing::Editor, name: &str) {
        e.execute(crate::editing::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
        e.execute(Command::EditSubTemplate(SubTemplateName { name: name.into(), category: None }), fonts());
    }

    #[test]
    fn edits_on_a_sub_template_save_into_its_building_block() {
        let mut e = open("FX-SUBTEMPLATES.docx");
        let before = crate::docx::write::save(e.document()).unwrap();
        edit(&mut e, "Violation");
        // A new paragraph at the end of the second paragraph (its comment is in the first).
        let Block::Para(p) = &e.document().glossary.as_ref().unwrap().parts[0].body[2] else { panic!("a paragraph") };
        let para = crate::layout::ParaId(p.id);
        e.select_range(para, crate::layout::Unit(100), crate::layout::Unit(100), fonts());
        e.execute(Command::InsertParagraph, fonts());
        e.execute(Command::InsertText("Added line".into()), fonts());
        e.execute(Command::CloseSubTemplate, fonts());
        let saved = crate::docx::write::save(e.document()).unwrap();
        let (old, new) = (glossary_xml(&before), glossary_xml(&saved));
        assert!(violation(&new).contains("Added line"));
        // The other building blocks are as read, and Violation's comment markers stay.
        assert_eq!(new.replace(violation(&new), ""), old.replace(violation(&old), ""));
        assert!(violation(&new).contains("commentReference"));
        let again = crate::docx::read_editable(&saved).unwrap();
        let body = &again.glossary.as_ref().unwrap().parts[0].body;
        assert!(body.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Text(s, _) if s == "Added line")))));
    }

    #[test]
    fn tracked_edits_on_a_sub_template_are_revisions_in_its_body() {
        let mut e = open("FX-SUBTEMPLATES.docx");
        edit(&mut e, "Appeals notice");
        e.execute(Command::SetTrackChanges(true), fonts());
        e.execute(Command::InsertText("Note: ".into()), fonts());
        let saved = crate::docx::write::save(e.document()).unwrap();
        let xml = glossary_xml(&saved);
        let at = xml.find(r#"w:val="Appeals notice""#).unwrap();
        let part = &xml[at..at + xml[at..].find("</w:docPart>").unwrap()];
        assert!(part.contains("<w:ins ") && part.contains("Note: "), "{part}");
    }
}

#[cfg(test)]
mod review_tests {
    use crate::api::command::{Command, SubTemplateName};
    use crate::editing::tests::{fonts, open};

    #[test]
    fn a_bodys_revisions_are_reviewed_while_it_is_edited() {
        let mut e = open("FX-SUBTEMPLATES.docx");
        e.execute(crate::editing::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
        e.execute(Command::EditSubTemplate(SubTemplateName { name: "Violation".into(), category: None }), fonts());
        e.execute(Command::AcceptAllRevisions(Default::default()), fonts());
        e.execute(Command::CloseSubTemplate, fonts());
        let saved = crate::docx::write::save(e.document()).unwrap();
        let again = crate::docx::read_editable(&saved).unwrap();
        let body = &again.glossary.as_ref().unwrap().parts[0].body;
        let tracked =
            body.iter().any(|b| matches!(b, crate::docx::Block::Para(p) if p.inlines.iter().any(|i| i.run().is_some_and(|r| r.rev.ins.is_some()))));
        assert!(!tracked, "the insertion in Violation was accepted");
        let text: String = format!("{body:?}");
        assert!(text.contains("again"), "accepting kept the inserted text");
    }
}
