//! The styles table edited in place against the same styles read again

use super::super::Editor;
use super::super::commands::Command;
use super::fonts;
use crate::docx::styles::Styles;
use crate::docx::{Block, Inline};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The blank package with `styles_inner` for its styles part's content and `body` for its body.
pub fn pkg(styles_inner: &str, body: &str) -> Vec<u8> {
    let doc = format!(
        r#"<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let styles = format!(r#"<w:styles xmlns:w="{W}">{styles_inner}</w:styles>"#);
    let parts = vec![("word/document.xml".to_string(), doc.into_bytes()), ("word/styles.xml".to_string(), styles.into_bytes())];
    crate::docx::write::repackage(&crate::docx::blank::blank_docx(), &parts, zip::CompressionMethod::Deflated).expect("repackages")
}

pub fn open_pkg(bytes: &[u8]) -> Editor {
    let mut e = Editor::open(bytes, fonts()).expect("opens");
    e.complete(fonts());
    e
}

pub fn reopened(e: &Editor) -> Editor {
    open_pkg(&crate::docx::write::save(e.document()).expect("saves"))
}

fn first_para(e: &Editor) -> &crate::docx::Paragraph {
    e.document().sections[0].blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).expect("a paragraph")
}

fn styles(e: &Editor) -> &Styles {
    &e.document().resolver.as_ref().expect("styles").styles
}

fn colours() -> crate::docx::props::ThemeColours {
    Default::default()
}

#[test]
fn an_empty_paragraph_default_survives_setting_the_character_defaults() {
    let st = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>"#;
    let mut e = open_pkg(&pkg(st, "<w:p><w:r><w:t>x</w:t></w:r></w:p>"));
    let d = crate::api::command::DocumentDefaults { size: Some(crate::api::value::Points(14.0)), ..Default::default() };
    e.execute(Command::SetDocumentDefaults(d), fonts());
    let again = reopened(&e);
    assert_eq!(format!("{:?}", styles(&again).doc_ppr), format!("{:?}", styles(&e).doc_ppr), "pPr defaults after reopen");
}

#[test]
fn an_empty_character_default_survives_setting_the_paragraph_defaults() {
    use crate::api::value::{AutoOr, Points};
    let st = r#"<w:docDefaults><w:rPrDefault/><w:pPrDefault><w:pPr><w:spacing w:after="0"/></w:pPr></w:pPrDefault></w:docDefaults>"#;
    let mut e = open_pkg(&pkg(st, "<w:p><w:r><w:t>x</w:t></w:r></w:p>"));
    let d = crate::api::command::DocumentDefaults {
        spacing: Some(crate::api::command::ParagraphSpacing { after: Some(AutoOr::Value(Points(6.0))), ..Default::default() }),
        ..Default::default()
    };
    e.execute(Command::SetDocumentDefaults(d), fonts());
    let again = reopened(&e);
    assert_eq!(format!("{:?}", styles(&again).doc_rpr), format!("{:?}", styles(&e).doc_rpr), "rPr defaults after reopen");
}

const JOIN_STYLES: &str = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:left="720"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>"#;

fn joined_runs(e: &Editor) -> (String, f64) {
    let p = first_para(e);
    let runs: Vec<String> =
        p.inlines.iter().filter_map(|i| if let Inline::Text(_, r) = i { Some(format!("{}/{}", r.size, r.bold)) } else { None }).collect();
    (runs.join(" "), p.style.ind_left)
}

fn joined() -> Editor {
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:rPr><w:vanish/></w:rPr></w:pPr><w:r><w:t>Head </w:t></w:r></w:p><w:p><w:r><w:t>Body</w:t></w:r></w:p>"#;
    let e = open_pkg(&pkg(JOIN_STYLES, body));
    assert_eq!(first_para(&e).src.as_ref().map(|s| s.ranges.len()), Some(2), "joined");
    e
}

#[test]
fn a_paragraph_joined_by_a_hidden_mark_keeps_its_first_part_through_a_style_edit() {
    let mut e = joined();
    let before = joined_runs(&e);
    let raw = styles(&e).get("Normal").expect("Normal").raw.clone();
    let xml = raw.replace("</w:style>", "<w:rPr><w:sz w:val=\"40\"/></w:rPr></w:style>");
    e.edit_styles("Modify Style", |s| s.set_xml(xml), fonts());
    let live = joined_runs(&e);
    assert_eq!(live.1, before.1, "indent kept");
    assert_eq!(live, joined_runs(&reopened(&e)), "same as reopen");
}

#[test]
fn a_paragraph_joined_by_a_hidden_mark_keeps_its_first_part_through_italic() {
    let mut e = joined();
    e.execute(Command::SelectAll, fonts());
    e.execute(Command::SetItalic(true), fonts());
    assert_eq!(joined_runs(&e), joined_runs(&reopened(&e)), "same as reopen");
}

#[test]
fn editing_a_character_style_only_a_mark_uses_resolves_the_mark_again() {
    let st = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="character" w:styleId="Big"><w:name w:val="Big"/><w:rPr><w:sz w:val="48"/></w:rPr></w:style>"#;
    let body = r#"<w:p><w:pPr><w:rPr><w:rStyle w:val="Big"/></w:rPr></w:pPr></w:p>"#;
    let mut e = open_pkg(&pkg(st, body));
    let raw = styles(&e).get("Big").expect("Big").raw.replace("w:val=\"48\"", "w:val=\"96\"");
    e.edit_styles("Modify Style", |s| s.set_xml(raw), fonts());
    assert_eq!(first_para(&e).mark.size, 48.0);
    assert_eq!(first_para(&e).mark.size, first_para(&reopened(&e)).mark.size);
}

#[test]
fn ruby_text_follows_a_paragraph_style_edit() {
    let st = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;
    let body = r#"<w:p><w:r><w:ruby><w:rubyPr><w:rubyAlign w:val="center"/><w:hps w:val="10"/><w:hpsRaise w:val="18"/><w:hpsBaseText w:val="20"/><w:lid w:val="ja-JP"/></w:rubyPr><w:rt><w:r><w:t>kan</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>X</w:t></w:r></w:rubyBase></w:ruby></w:r></w:p>"#;
    let mut e = open_pkg(&pkg(st, body));
    let sizes = |e: &Editor| {
        first_para(e)
            .inlines
            .iter()
            .find_map(|i| {
                if let Inline::Ruby(r, _) = i {
                    Some((r.base.iter().map(|x| x.1.size).collect::<Vec<_>>(), r.text.iter().map(|x| x.1.size).collect::<Vec<_>>()))
                } else {
                    None
                }
            })
            .expect("a ruby")
    };
    let raw = styles(&e).get("Normal").expect("Normal").raw.replace("</w:style>", "<w:rPr><w:sz w:val=\"60\"/></w:rPr></w:style>");
    e.edit_styles("Modify Style", |s| s.set_xml(raw), fonts());
    assert_eq!(sizes(&e), (vec![30.0], vec![30.0]));
    assert_eq!(sizes(&e), sizes(&reopened(&e)));
}

const TN: &str = r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/></w:tblPr></w:style>"#;
const TG: &str = r#"<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:basedOn w:val="TableNormal"/></w:style>"#;

fn wrap(inner: &str) -> String {
    format!(r#"<w:styles xmlns:w="{W}">{inner}</w:styles>"#)
}

#[test]
fn editing_normal_table_keeps_words_fixed_definition() {
    let mut live = Styles::parse(Some(&wrap(&format!("{TN}{TG}"))), colours());
    let edited = TN.replace("</w:style>", "<w:tcPr><w:vAlign w:val=\"center\"/></w:tcPr><w:rPr><w:b/></w:rPr></w:style>");
    live.set_xml(edited.clone()).expect("sets");
    let fresh = Styles::parse(Some(&wrap(&format!("{edited}{TG}"))), colours());
    assert_eq!(format!("{:?}", live.get("TableNormal").unwrap().base), format!("{:?}", fresh.get("TableNormal").unwrap().base));
}

#[test]
fn editing_a_table_style_read_before_its_base_keeps_it_frozen() {
    let t = r#"<w:style w:type="table" w:styleId="T"><w:name w:val="T"/><w:basedOn w:val="TN"/><w:rPr><w:b/></w:rPr><w:tblStylePr w:type="firstRow"><w:tcPr><w:vAlign w:val="center"/></w:tcPr></w:tblStylePr></w:style>"#;
    let tn = r#"<w:style w:type="table" w:default="1" w:styleId="TN"><w:name w:val="TN"/><w:rPr><w:i/></w:rPr></w:style>"#;
    let xml = wrap(&format!("{t}{tn}"));
    let mut live = Styles::parse(Some(&xml), colours());
    live.set_xml(t.to_string()).expect("sets");
    let fresh = Styles::parse(Some(&xml), colours());
    assert!(fresh.get("T").unwrap().frozen, "fixture is frozen on read");
    assert_eq!(live.chain(Some("T")).len(), fresh.chain(Some("T")).len());
    assert_eq!(format!("{:?}", live.get("T").unwrap().base), format!("{:?}", fresh.get("T").unwrap().base));
}

#[test]
fn the_default_styles_follow_a_style_edit() {
    let normal = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;
    let mut live = Styles::parse(Some(&wrap(normal)), colours());
    let as_char = normal.replace("w:type=\"paragraph\"", "w:type=\"character\"");
    live.set_xml(as_char.clone()).expect("sets");
    let fresh = Styles::parse(Some(&wrap(&as_char)), colours());
    assert_eq!(live.default_paragraph, fresh.default_paragraph, "default paragraph");
    assert_eq!(live.paragraph_order, fresh.paragraph_order);
}

#[test]
fn deleting_the_default_table_style_clears_it() {
    let t = r#"<w:style w:type="table" w:default="1" w:customStyle="1" w:styleId="Cust"><w:name w:val="Cust"/></w:style>"#;
    let mut live = Styles::parse(Some(&wrap(t)), colours());
    assert_eq!(live.default_table.as_deref(), Some("Cust"));
    live.delete("Cust").expect("deletes");
    assert_eq!(live.default_table, None);
}

#[test]
fn a_style_edit_takes_one_style_element_alone() {
    let a = r#"<w:style w:type="paragraph" w:styleId="A"><w:name w:val="A"/></w:style>"#;
    let b = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;
    let mut live = Styles::parse(Some(&wrap(b)), colours());
    assert!(live.set_xml(format!("{a}{b}")).is_err(), "two w:style elements");
    assert!(live.set_xml(format!("{a}text")).is_err(), "text beside it");
    assert!(live.set_xml(a.to_string()).is_ok());
}

#[test]
fn deleting_a_custom_table_style_falls_back_to_the_default_one() {
    let normal = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;
    let cust = r#"<w:style w:type="table" w:customStyle="1" w:styleId="Cust"><w:name w:val="Cust"/><w:basedOn w:val="TableNormal"/></w:style>"#;
    let st = format!("{normal}{TN}{TG}{cust}");
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="Cust"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let mut e = open_pkg(&pkg(&st, body));
    let style_id = |e: &Editor| e.index().table_ids().next().and_then(|t| e.index().table(e.document(), t).and_then(|t| t.style_id.clone()));
    assert_eq!(style_id(&e).as_deref(), Some("Cust"));
    e.edit_styles("Delete Style", |s| s.delete("Cust"), fonts());
    assert_eq!(style_id(&e), style_id(&reopened(&e)));
}

#[test]
fn finding_a_style_edits_users_is_linear_in_the_styles() {
    const N: usize = 30_000;
    let mut st = String::from(r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#);
    for i in 0..N {
        st.push_str(&format!(r#"<w:style w:type="paragraph" w:styleId="Z{i:06}"><w:name w:val="Z{i:06}"/></w:style>"#));
    }
    // The same document twice: its paragraphs use the first changed id, then the last one; only the membership test differs.
    let time = |id: usize| {
        let body: String = (0..N).map(|_| format!(r#"<w:p><w:pPr><w:pStyle w:val="Z{id:06}"/></w:pPr></w:p>"#)).collect();
        let mut e = open_pkg(&pkg(&st, &body));
        let t = std::time::Instant::now();
        let d = crate::api::command::DocumentDefaults { size: Some(crate::api::value::Points(14.0)), ..Default::default() };
        e.execute(Command::SetDocumentDefaults(d), fonts());
        t.elapsed().as_secs_f64()
    };
    let (first, last) = (time(0), time(N - 1));
    assert!(last < first * 2.0 + 0.5, "first {first}s, last {last}s");
}

#[test]
fn deleting_a_style_takes_it_out_of_the_paragraphs_and_runs_naming_it() {
    let st = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:customStyle="1" w:styleId="Note"><w:name w:val="Note"/><w:basedOn w:val="Normal"/><w:rPr><w:sz w:val="40"/></w:rPr></w:style>
<w:style w:type="character" w:customStyle="1" w:styleId="Mark"><w:name w:val="Mark"/><w:rPr><w:b/></w:rPr></w:style>"#;
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Note"/><w:jc w:val="center"/><w:rPr><w:rStyle w:val="Mark"/></w:rPr></w:pPr><w:r><w:rPr><w:rStyle w:val="Mark"/><w:i/></w:rPr><w:t>a</w:t></w:r></w:p>"#;
    let mut e = open_pkg(&pkg(st, body));
    let id = |n: &str| crate::api::value::StyleId(n.into());
    e.execute(Command::DeleteStyle(id("Note")), fonts());
    e.execute(Command::DeleteStyle(id("Mark")), fonts());
    let doc = {
        use std::io::Read;
        let bytes = crate::docx::write::save(e.document()).expect("saves");
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a package");
        let mut s = String::new();
        z.by_name("word/document.xml").expect("the body").read_to_string(&mut s).expect("text");
        s
    };
    assert!(!doc.contains("Note") && !doc.contains("Mark"), "{doc}");
    assert!(doc.contains("<w:jc w:val=\"center\"/>") && doc.contains("<w:i/>"), "the rest stays: {doc}");
    let p = first_para(&e);
    let run = p.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.clone()) } else { None }).expect("a run");
    assert_eq!((p.style.style_id.as_deref(), run.style_id.as_deref(), run.bold, run.italic, run.size), (Some("Normal"), None, false, true, 12.0));
    let again = reopened(&e);
    assert_eq!(first_para(&again).style.style_id, p.style.style_id);
    // A style made later under the id isn't theirs.
    let note = r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="Note"><w:name w:val="Note"/><w:rPr><w:sz w:val="60"/></w:rPr></w:style>"#;
    e.edit_styles("New Style", |s| s.set_xml(note.into()), fonts());
    assert_eq!(first_para(&e).style.style_id.as_deref(), Some("Normal"));
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    e.execute(Command::Undo, fonts());
    assert_eq!(first_para(&e).style.style_id.as_deref(), Some("Note"), "one undo step each");
}

const LINKED: &str = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:link w:val="Heading1Char"/><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
<w:style w:type="character" w:customStyle="1" w:styleId="Heading1Char"><w:name w:val="Heading 1 Char"/><w:basedOn w:val="DefaultParagraphFont"/><w:link w:val="Heading1"/><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
<w:style w:type="character" w:customStyle="1" w:styleId="FooChar"><w:name w:val="Foo Char"/><w:rPr><w:i/></w:rPr></w:style>
<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/></w:style>
<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:basedOn w:val="TableNormal"/></w:style>"#;

fn linked() -> Editor {
    open_pkg(&pkg(LINKED, r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Head</w:t></w:r></w:p>"#))
}

fn dialog(e: &Editor, open: crate::api::services::StyleOpen) -> crate::api::services::StyleDialog {
    e.style_dialog_open(open).expect("opens")
}

#[test]
fn a_new_style_made_a_character_style_takes_a_character_base() {
    use crate::api::services::{StyleDialogType, StyleOpen};
    let mut e = linked();
    let was = dialog(&e, StyleOpen::Create);
    assert_eq!(was.based_on.as_ref().map(|b| b.0.as_str()), Some("Heading1"), "the selection's paragraph style");
    let mut now = was.clone();
    now.kind = StyleDialogType::Character;
    now.name = "Accent".into();
    let v = e.style_dialog_update(&was, now, fonts()).expect("updates");
    assert_eq!(v.based_on.as_ref().map(|b| b.0.as_str()), Some("DefaultParagraphFont"));
    e.style_dialog_apply(v, fonts());
    let accent = styles(&e).get("Accent").expect("made");
    assert_eq!((accent.kind, accent.based_on.as_deref()), (crate::docx::styles::StyleKind::Character, Some("DefaultParagraphFont")));
}

#[test]
fn a_style_dialog_refuses_a_base_of_another_type_or_another_style_to_edit() {
    use crate::api::services::StyleOpen;
    use crate::api::value::StyleId;
    let mut e = linked();
    let mut v = dialog(&e, StyleOpen::Create);
    v.name = "Body".into();
    v.pending = v.pending.replace("w:val=\"Heading1\"", "w:val=\"FooChar\"");
    assert!(matches!(e.style_dialog_apply(v, fonts()), super::super::commands::Outcome::Refused(_)), "based on a character style");
    let mut v = dialog(&e, StyleOpen::Modify(StyleId("Heading1".into())));
    v.pending = v.pending.replace("w:styleId=\"Heading1\"", "w:styleId=\"Normal\"");
    assert!(matches!(e.style_dialog_apply(v, fonts()), super::super::commands::Outcome::Refused(_)), "another style's id");
    assert!(styles(&e).get("Normal").is_some_and(|s| s.name == "Normal"));
}

#[test]
fn a_new_linked_style_takes_a_character_id_no_style_has() {
    use crate::api::services::{StyleDialogType, StyleOpen};
    let mut e = linked();
    let mut v = dialog(&e, StyleOpen::Create);
    (v.name, v.kind) = ("Foo".into(), StyleDialogType::Linked);
    e.style_dialog_apply(v, fonts());
    assert_eq!(styles(&e).get("Foo").and_then(|s| s.link.as_deref()), Some("FooChar1"));
    assert_eq!(styles(&e).get("FooChar").map(|s| s.name.as_str()), Some("Foo Char"), "the other style stays");
    assert!(styles(&e).get("FooChar").is_some_and(|s| s.raw.contains("<w:i/>")));
}

#[test]
fn modifying_a_linked_style_formats_its_character_half() {
    use crate::api::services::StyleOpen;
    use crate::api::value::{Points, StyleId};
    let mut e = linked();
    let was = dialog(&e, StyleOpen::Modify(StyleId("Heading1".into())));
    let mut now = was.clone();
    now.size = Points(20.0);
    let v = e.style_dialog_update(&was, now, fonts()).expect("updates");
    e.style_dialog_apply(v, fonts());
    let size = |id: &str| styles(&e).get(id).and_then(|s| s.base.rpr.size);
    assert_eq!((size("Heading1"), size("Heading1Char")), (Some(20.0), Some(20.0)));
}

#[test]
fn only_a_paragraph_style_applies_to_paragraphs() {
    use crate::api::value::StyleId;
    let mut e = linked();
    let out = e.execute(Command::SetParagraphStyle(StyleId("TableGrid".into())), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Refused(_)));
    assert_eq!(first_para(&e).style.style_id.as_deref(), Some("Heading1"));
}

const GRID: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/></w:style>
<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:basedOn w:val="TableNormal"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPr></w:style>"#;

/// The first table's borders, band sizes and each cell's fill and margins.
fn table_look(e: &Editor) -> String {
    let t = e.index().table_ids().next().and_then(|t| e.index().table(e.document(), t)).expect("a table");
    let cells: Vec<String> = t.rows.iter().flat_map(|r| &r.cells).map(|c| format!("{:?} {:?} {:?}", c.shading, c.margins, c.top)).collect();
    format!("{:?} {:?} {:?} {}", t.style_id, t.borders, t.bands, cells.join(" | "))
}

#[test]
fn a_table_style_edit_formats_its_tables_as_a_fresh_read_does() {
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let mut e = open_pkg(&pkg(GRID, body));
    let raw = styles(&e).get("TableGrid").expect("TableGrid").raw.replace("w:sz=\"4\"", "w:sz=\"18\"").replace(
        "</w:tblBorders></w:tblPr>",
        r#"</w:tblBorders><w:tblStyleRowBandSize w:val="2"/><w:tblCellMar><w:left w:w="216" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="FFFF00"/></w:tcPr>"#,
    );
    let before = table_look(&e);
    e.edit_styles("Modify Style", |s| s.set_xml(raw), fonts());
    let live = table_look(&e);
    assert_ne!(live, before, "the style changed the table");
    assert_eq!(live, table_look(&reopened(&e)));
    e.execute(Command::Undo, fonts());
    assert_eq!(table_look(&e), before);
}

#[test]
fn an_inserted_table_takes_its_style_as_a_fresh_read_does() {
    let mut e = open_pkg(&pkg(GRID, "<w:p/>"));
    e.execute(Command::InsertTable(crate::api::command::TableSize { rows: 2, columns: 2 }), fonts());
    assert!(table_look(&e).starts_with("Some(\"TableGrid\")"), "{}", table_look(&e));
    assert_eq!(table_look(&e), table_look(&reopened(&e)));
    // A document without Table Grid: the table's own lines and margins.
    let mut e = open_pkg(&pkg(r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#, "<w:p/>"));
    e.execute(Command::InsertTable(crate::api::command::TableSize { rows: 1, columns: 2 }), fonts());
    assert!(table_look(&e).contains("5.4"), "{}", table_look(&e));
    assert_eq!(table_look(&e), table_look(&reopened(&e)));
}

#[test]
fn a_style_edit_with_track_changes_on_records_no_revisions() {
    let st = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;
    let mut e = open_pkg(&pkg(st, "<w:p><w:r><w:t>x</w:t></w:r></w:p>"));
    e.execute(Command::SetTrackChanges(true), fonts());
    let raw = styles(&e)
        .get("Normal")
        .expect("Normal")
        .raw
        .replace("</w:style>", "<w:pPr><w:jc w:val=\"center\"/></w:pPr><w:rPr><w:sz w:val=\"40\"/></w:rPr></w:style>");
    e.edit_styles("Modify Style", |s| s.set_xml(raw), fonts());
    let p = first_para(&e);
    assert_eq!((p.style.jc, p.mark.size), (crate::docx::props::Jc::Center, 20.0), "the style applies");
    assert!(p.style.change.is_none() && p.mark.rev.fmt.is_none(), "no formatting revision");
    assert!(p.inlines.iter().filter_map(|i| i.run()).all(|r| r.rev.fmt.is_none()));
}

#[test]
fn a_built_in_style_the_document_lacks_is_listed_applied_and_added_in_one_step() {
    use crate::api::value::StyleId;
    let mut e =
        Editor::open(&super::blank_body("<w:p><w:r><w:t>Head</w:t></w:r></w:p><w:p><w:r><w:t>body</w:t></w:r></w:p>"), fonts()).expect("opens");
    e.complete(fonts());
    assert!(styles(&e).get("Heading1").is_none(), "a blank document defines no headings");
    let listed = crate::services::styles::list(e.document(), None, false);
    assert!(listed.iter().any(|s| s.id.0 == "Heading1" && s.name == "Heading 1" && s.preview.size.0 == 20.0), "the gallery offers it");
    let undo = e.history().undo_depth();
    let out = e.execute(Command::SetParagraphStyle(StyleId("Heading1".into())), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let p = first_para(&e);
    assert_eq!((p.style.style_id.as_deref(), p.style.keep_next, p.style.outline_lvl), (Some("Heading1"), true, Some(0)));
    assert!(p.inlines.iter().filter_map(|i| i.run()).all(|r| r.size == 20.0));
    assert_eq!(e.history().undo_depth(), undo + 1, "one undo step");
    assert_eq!(listed.len(), crate::services::styles::list(e.document(), None, false).len(), "listed once, defined now");
    let again = reopened(&e);
    assert_eq!(first_para(&again).style.style_id.as_deref(), Some("Heading1"));
    e.execute(Command::Undo, fonts());
    assert!(styles(&e).get("Heading1").is_none(), "undone with it");
    e.execute(Command::Select(crate::api::command::SelectUnit::Paragraph), fonts());
    e.execute(Command::ApplyCharacterStyle(Some(StyleId("Strong".into()))), fonts());
    assert!(first_para(&e).inlines.iter().filter_map(|i| i.run()).any(|r| r.bold && r.style_id.as_deref() == Some("Strong")));
    assert!(matches!(e.execute(Command::SetParagraphStyle(StyleId("Strong".into())), fonts()), super::super::commands::Outcome::Refused(_)));
}

#[test]
fn a_built_in_style_the_document_lacks_opens_in_the_style_dialog_and_saves_with_it() {
    use crate::api::services::StyleOpen;
    use crate::api::value::{Points, StyleId};
    let mut e = Editor::open(&super::blank_body("<w:p><w:r><w:t>x</w:t></w:r></w:p>"), fonts()).expect("opens");
    e.complete(fonts());
    let was = e.style_dialog_open(StyleOpen::Modify(StyleId("Heading4".into()))).expect("opens on the built-in");
    let mut now = was.clone();
    now.size = Points(30.0);
    let v = e.style_dialog_update(&was, now, fonts()).expect("updates");
    e.style_dialog_apply(v, fonts());
    let h4 = styles(&e).get("Heading4").expect("added");
    assert_eq!((h4.name.as_str(), h4.base.rpr.size, h4.based_on.as_deref()), ("heading 4", Some(30.0), Some("Normal")));
}

#[test]
fn a_session_takes_a_built_in_style_the_document_lacks() {
    use crate::api::engine::{Engine, SessionOptions};
    use crate::api::value::StyleId;
    let mut engine = Engine::new(crate::api::fonts::FontLibrary::empty());
    let id = engine.create_session(SessionOptions::default());
    let s = engine.session(id).expect("a session");
    s.run(Command::InsertText("Head".into())).expect("types");
    s.run(Command::SetParagraphStyle(StyleId("Heading2".into()))).expect("applies a built-in");
    assert!(s.run(Command::SetParagraphStyle(StyleId("NoSuchStyle".into()))).is_err());
}

#[test]
fn a_table_style_edit_keeps_the_tables_own_alignment_and_row_heights() {
    use crate::api::value::{AutoOr, HeightRule, HorizontalAlignment, Points, RowHeight};
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let style = GRID
        .replace("<w:tblPr><w:tblBorders>", r#"<w:tblPr><w:jc w:val="right"/><w:tblBorders>"#)
        .replace("</w:tblPr></w:style>", r#"</w:tblPr><w:trPr><w:cantSplit/></w:trPr></w:style>"#);
    let mut e = open_pkg(&pkg(&style, body));
    let look = |e: &Editor| {
        let t = e.index().table_ids().next().and_then(|t| e.index().table(e.document(), t)).expect("a table");
        (t.jc, t.rows[0].height.clone(), t.rows[0].cant_split)
    };
    assert_eq!(look(&e), (crate::docx::props::Jc::Right, None, true), "the style's");
    e.execute(Command::SetTableAlignment(HorizontalAlignment::Center), fonts());
    e.execute(Command::SetRowHeight(AutoOr::Value(RowHeight { height: Points(30.0), rule: HeightRule::Exact })), fonts());
    e.execute(Command::SetRowCanBreak(true), fonts());
    let own = look(&e);
    assert_eq!(own, (crate::docx::props::Jc::Center, Some((30.0, "exact".to_string())), false));
    let raw = styles(&e).get("TableGrid").expect("TableGrid").raw.replace("w:sz=\"4\"", "w:sz=\"12\"");
    e.edit_styles("Modify Style", |s| s.set_xml(raw), fonts());
    assert_eq!(look(&e), own, "the table's own stay");
    assert_eq!(look(&reopened(&e)), own);
    let xml = {
        use std::io::Read;
        let bytes = crate::docx::write::save(e.document()).expect("saves");
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a package");
        let mut s = String::new();
        z.by_name("word/document.xml").expect("the body").read_to_string(&mut s).expect("text");
        s
    };
    assert_eq!(
        (xml.matches("<w:trHeight").count(), xml.matches("<w:jc w:val=\"center\"/>").count(), xml.matches("<w:cantSplit").count()),
        (1, 1, 1),
        "{xml}"
    );
    assert_eq!(table_look(&e), table_look(&reopened(&e)));
    // Taken out of the style, the style's alignment goes from the tables that took it.
    let mut e = open_pkg(&pkg(&style, body));
    let raw = styles(&e).get("TableGrid").expect("TableGrid").raw.replace(r#"<w:jc w:val="right"/>"#, "").replace("<w:cantSplit/>", "");
    e.edit_styles("Modify Style", |s| s.set_xml(raw), fonts());
    assert_eq!(look(&e), (crate::docx::props::Jc::Left, None, false));
    assert_eq!(look(&e), look(&reopened(&e)));
}

fn bigger_normal(e: &mut Editor) {
    let raw = styles(e).get("Normal").expect("Normal").raw.clone();
    let xml = raw.replace("</w:style>", "<w:rPr><w:sz w:val=\"40\"/></w:rPr></w:style>");
    e.edit_styles("Modify Style", |s| s.set_xml(xml), fonts());
}

fn delete(e: &mut Editor) {
    let d = crate::api::command::DeleteOptions { direction: crate::api::value::Direction::Forward, unit: None };
    e.execute(Command::Delete(d), fonts());
}

#[test]
fn a_paragraph_joined_by_a_hidden_mark_keeps_its_parts_after_its_first_part_is_edited() {
    let mut e = joined();
    for _ in 0..4 {
        delete(&mut e);
    }
    bigger_normal(&mut e);
    assert_eq!(joined_runs(&e), joined_runs(&reopened(&e)));
}

#[test]
fn a_paragraph_joined_by_a_hidden_mark_keeps_its_parts_through_an_undone_delete_across_them() {
    let ends = |e: &Editor| first_para(e).src.as_ref().map(|s| s.joined.iter().map(|j| j.end).collect::<Vec<_>>());
    let mut e = joined();
    let read = ends(&e);
    let para = crate::layout::ParaId(first_para(&e).id);
    e.select_range(para, crate::layout::Unit(3), crate::layout::Unit(7), fonts());
    delete(&mut e);
    assert_eq!(ends(&e), Some(vec![3]), "\"Hea\" and \"dy\"");
    e.execute(Command::Undo, fonts());
    assert_eq!(ends(&e), read);
    e.execute(Command::Redo, fonts());
    assert_eq!(ends(&e), Some(vec![3]));
    e.execute(Command::Undo, fonts());
    bigger_normal(&mut e);
    assert_eq!(joined_runs(&e), joined_runs(&reopened(&e)));
}

#[test]
fn a_table_style_dialog_refuses_an_indent_or_cell_margin_out_of_range() {
    use crate::api::services::{DialogValues, StyleOpen};
    let mut e = open_pkg(&pkg(GRID, "<w:p/>"));
    let v = dialog(&e, StyleOpen::Modify(crate::api::value::StyleId("TableGrid".into())));
    let start = e.style_table_properties(&v.pending).expect("table properties");
    for (indent, margin) in [(1e300, 0.0), (0.0, 1e300), (0.0, -1.0)] {
        let mut now = start.clone();
        now.indent = crate::api::value::Points(indent);
        now.cell_margins.left = crate::api::value::Points(margin);
        let got = e.style_dialog_table_format(&v, &DialogValues::TableProperties(start.clone()), &DialogValues::TableProperties(now));
        assert!(got.is_err(), "indent {indent}, margin {margin}");
    }
}
