//! Styles: reading their flags, and editing the styles table

use std::path::Path;

use super::styles::{StyleEditError, StyleKind, Styles};
use super::*;

fn styles() -> Styles {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES.docx");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc = read_editable(&bytes).unwrap_or_else(|e| panic!("FX-STYLES: {e}"));
    doc.resolver.expect("a resolver").styles.clone()
}

#[test]
fn names_links_flags_and_latent_styles_are_read() {
    let s = styles();
    let memo = s.get("MemoBody").expect("Memo Body");
    assert_eq!((memo.name.as_str(), memo.aliases.as_slice()), ("Memo Body", ["MB".to_string()].as_slice()));
    assert_eq!(s.get("Quote").and_then(|q| q.link.as_deref()), Some("QuoteChar"));
    assert_eq!(s.get("QuoteChar").and_then(|q| q.link.as_deref()), Some("Quote"));
    let draft = s.get("Draft").expect("Draft").meta;
    assert!(draft.semi_hidden && draft.unhide_when_used && !draft.hidden);
    assert!(s.get("HiddenNote").expect("Hidden Note").meta.hidden);
    assert!(s.get("Locked").expect("Locked").meta.locked);
    assert!(s.get("Callout").expect("Callout").meta.auto_redefine);
    assert_eq!(s.latent.exceptions.len(), 7);
    assert_eq!(s.latent.priority, Some(99));
    let h2 = s.latent.exceptions.iter().find(|l| l.name == "heading 2").expect("heading 2");
    assert_eq!((h2.priority, h2.semi_hidden, h2.unhide_when_used, h2.quick), (Some(9), Some(true), Some(true), Some(true)));
    assert!(s.root.starts_with("<w:styles") && s.defaults_raw.as_deref().is_some_and(|d| d.starts_with("<w:docDefaults>")));
    assert!(s.get("Callout").expect("Callout").raw.contains("<w:rsid w:val=\"00A1B2C3\"/>"), "the element as written");
}

#[test]
fn a_style_set_from_its_xml_is_read_again_and_reports_its_dependants() {
    let mut s = styles();
    let h1 = s.get("Heading1").expect("Heading 1").raw.replace("<w:sz w:val=\"32\"/>", "<w:sz w:val=\"40\"/>");
    let changed = s.set_xml(h1).expect("sets it");
    assert_eq!(changed, ["Heading1", "Heading2", "Heading3"], "it and the styles based on it");
    assert_eq!(s.chain_rpr(Some("Heading2")).size, Some(13.0), "Heading 2 sets its own size");
    assert_eq!(s.chain_rpr(Some("Heading1")).size, Some(20.0));
    let new = "<w:style w:type=\"paragraph\" w:customStyle=\"1\" w:styleId=\"Note\"><w:name w:val=\"Note\"/><w:basedOn w:val=\"Callout\"/></w:style>";
    assert_eq!(s.set_xml(new.to_string()).expect("adds it"), ["Note"]);
    assert_eq!(s.get("Note").map(|n| n.kind), Some(StyleKind::Paragraph));
    assert!(s.paragraph_order.iter().any(|p| p == "Note"));
}

#[test]
fn a_style_edit_that_would_loop_or_is_malformed_is_refused() {
    let mut s = styles();
    let looped = s.get("Heading1").expect("Heading 1").raw.replace("<w:basedOn w:val=\"Normal\"/>", "<w:basedOn w:val=\"Heading3\"/>");
    assert_eq!(s.set_xml(looped), Err(StyleEditError::Loop));
    assert_eq!(s.set_xml("<w:style/>".into()), Err(StyleEditError::Malformed));
    assert_eq!(s.delete("Nope"), Err(StyleEditError::Missing));
}

#[test]
fn deleting_a_style_rebases_its_dependants_and_drops_references_to_it() {
    let mut s = styles();
    let changed = s.delete("Heading2").expect("deletes it");
    assert_eq!(changed, ["Heading2", "Heading3"], "it, for its users, and the style based on it");
    assert!(s.get("Heading2").is_none());
    let h3 = s.get("Heading3").expect("Heading 3");
    assert_eq!(h3.based_on.as_deref(), Some("Heading1"), "based on the deleted style's base");
    assert!(h3.raw.contains("<w:basedOn w:val=\"Heading1\"/>"), "and written so: {}", h3.raw);
    s.delete("QuoteChar").expect("deletes the linked character style");
    let quote = s.get("Quote").expect("Quote");
    assert!(quote.link.is_none() && !quote.raw.contains("w:link"), "the link goes with it");
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES.docx")).expect("FX-STYLES")
}

fn edit(doc: &mut Document, f: impl FnOnce(&mut Styles)) {
    let res = doc.resolver.as_mut().expect("a resolver");
    f(&mut std::rc::Rc::make_mut(res).styles);
}

fn styles_part(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a package");
    let mut s = String::new();
    zip.by_name("word/styles.xml").expect("a styles part").read_to_string(&mut s).expect("text");
    s
}

#[test]
fn saving_with_no_style_edits_keeps_the_package() {
    let bytes = fixture_bytes();
    let doc = read_editable(&bytes).expect("reads");
    assert_eq!(write::save(&doc).expect("saves"), bytes);
}

#[test]
fn a_changed_added_and_deleted_style_are_written_and_the_rest_kept() {
    let bytes = fixture_bytes();
    let before = styles_part(&bytes);
    let mut doc = read_editable(&bytes).expect("reads");
    edit(&mut doc, |s| {
        let callout = s.get("Callout").expect("Callout").raw.replace("<w:sz w:val=\"24\"/>", "<w:sz w:val=\"28\"/>");
        s.set_xml(callout).expect("changes Callout");
        s.set_xml("<w:style w:type=\"character\" w:customStyle=\"1\" w:styleId=\"Code\"><w:name w:val=\"Code\"/></w:style>".into())
            .expect("adds Code");
        s.delete("Draft").expect("deletes Draft");
    });
    let after = styles_part(&write::save(&doc).expect("saves"));
    assert!(after.contains("<w:sz w:val=\"28\"/>") && after.contains("<w:rsid w:val=\"00A1B2C3\"/>"), "Callout changed, its other children kept");
    assert!(after.contains("w:styleId=\"Code\"") && !after.contains("w:styleId=\"Draft\""));
    let kept = |xml: &str| xml[xml.find("w:styleId=\"Quote\"").expect("Quote")..].split("</w:style>").next().map(str::to_string);
    assert_eq!(kept(&after), kept(&before), "an untouched style is byte for byte the same");
    let again = read_editable(&write::save(&doc).expect("saves")).expect("reads again");
    let s = &again.resolver.expect("a resolver").styles;
    assert_eq!(s.chain_rpr(Some("Callout")).size, Some(14.0));
    assert!(s.get("Code").is_some() && s.get("Draft").is_none());
}
