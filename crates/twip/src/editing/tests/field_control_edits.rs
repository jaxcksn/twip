//! Unlinking fields, replacing in codes and changing content controls

use super::{blank_body, fonts, open, para_id};
use crate::docx::{Block, ControlId, Inline};
use crate::editing::Editor;
use crate::editing::commands::{Command, Outcome};
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::{Granularity, Selection};
use crate::layout::Unit;

fn t(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

fn ed(body: &str) -> Editor {
    Editor::open(&blank_body(body), fonts()).expect("opens")
}

fn inlines(e: &Editor, n: usize) -> Vec<Inline> {
    let paras: Vec<&crate::docx::Paragraph> =
        e.document().sections.iter().flat_map(|s| s.blocks.iter()).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect();
    paras[n].inlines.clone()
}

fn control_marks(e: &Editor, n: usize) -> (usize, usize) {
    let v = inlines(e, n);
    (v.iter().filter(|i| matches!(i, Inline::ControlStart(_))).count(), v.iter().filter(|i| matches!(i, Inline::ControlEnd(_))).count())
}

fn saved(e: &Editor) -> Editor {
    Editor::open(&crate::docx::write::save(e.document(), e.docx()).expect("saves"), fonts()).expect("reopens")
}

#[test]
fn unlinking_a_field_keeps_its_result_as_it_is() {
    let f = format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> HYPERLINK "http://example.com" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r>{}<w:r><w:br/></w:r><w:del w:id="9" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:delText>gone</w:delText></w:r></w:del><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        t(" plain")
    );
    let mut e = ed(&format!("<w:p>{f}</w:p>"));
    e.set_caret_for_benchmark(Selection::Caret(Position::text(StoryId::Body, para_id(&e, 0), Unit(2))));
    let out = e.execute(Command::UnlinkFields(Default::default()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let v = inlines(&e, 0);
    assert!(!v.iter().any(|i| matches!(i, Inline::FieldBegin { .. } | Inline::FieldSep { .. } | Inline::FieldEnd { .. })), "no field: {v:?}");
    let texts: Vec<(&str, bool, bool)> =
        v.iter().filter_map(|i| if let Inline::Text(s, r) = i { Some((s.as_str(), r.bold, r.rev.del.is_some())) } else { None }).collect();
    assert_eq!(texts, [("Bold", true, false), (" plain", false, false), ("gone", false, true)], "each run as it was: {v:?}");
    assert!(v.iter().any(|i| matches!(i, Inline::Break(..))), "the break stays");
}

fn sym_of(e: &Editor, c: ControlId) -> Option<(char, String)> {
    e.document().sections.iter().flat_map(|s| s.blocks.iter()).find_map(|b| match b {
        Block::Para(p) => p.inlines.iter().find_map(|i| match i {
            Inline::Sym { ch, font, control: Some(k), .. } if *k == c => Some((*ch, font.clone())),
            _ => None,
        }),
        _ => None,
    })
}

fn checkbox_with(e: &Editor, symbol: char) -> Option<ControlId> {
    e.document()
        .controls
        .iter()
        .find(|(_, k)| matches!(k.props.kind, crate::docx::ControlKind::Checkbox { checked, .. } if checked == symbol))
        .map(|(c, _)| *c)
}

#[test]
fn a_checked_box_given_a_new_checked_symbol_stays_checked() {
    let mut e = open("FX-CHECKBOXES.docx");
    let c = *e
        .document()
        .controls
        .iter()
        .find(|(k, c)| matches!(c.props.kind, crate::docx::ControlKind::Checkbox { .. }) && sym_of(&e, **k).is_some_and(|s| s.0 == '\u{2612}'))
        .map(|(k, _)| k)
        .expect("a checked box");
    let id = crate::editing::elements::id_str(crate::docx::ElementId::Control(c));
    let set = serde_json::json!({ "id": id, "options": { "checkedSymbol": { "font": "Segoe UI Symbol", "character": "\u{2713}" } } });
    let out = e.execute(Command::SetControl(serde_json::from_value(set).unwrap()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(sym_of(&e, c), Some(('\u{2713}', "Segoe UI Symbol".to_string())), "it shows the new face");
    let again = saved(&e);
    let c = checkbox_with(&again, '\u{2713}').expect("it saves with the new symbol");
    assert_eq!(sym_of(&again, c).map(|s| s.0), Some('\u{2713}'), "and reads back checked");
}

#[test]
fn an_argument_holding_a_nested_field_is_not_rewritten() {
    let instr = |s: &str| format!(r#"<w:r><w:instrText xml:space="preserve">{s}</w:instrText></w:r>"#);
    let fld = |kind: &str| format!(r#"<w:r><w:fldChar w:fldCharType="{kind}"/></w:r>"#);
    let nested = format!("{}{}{}{}{}", fld("begin"), instr(" PAGE "), fld("separate"), t("1"), fld("end"));
    let f = format!("{}{}{nested}{}{}{}{}", fld("begin"), instr(" REF \"ab"), instr("cd\" "), fld("separate"), t("v"), fld("end"));
    let mut e = ed(&format!("<w:p>{f}</w:p>"));
    let before = inlines(&e, 0);
    let id = before.iter().find_map(|i| if let Inline::FieldBegin { id, .. } = i { Some(*id) } else { None }).expect("a field");
    let mut tx = e.begin("Rename");
    assert!(!e.set_code_arg(&mut tx, id, |_| Some(0), "\"x\""), "the argument spans the nested field");
    e.abort(tx);
    assert_eq!(inlines(&e, 0), before);
}

#[test]
fn a_box_whose_symbols_are_one_keeps_its_state_given_a_new_symbol() {
    let w14 = "http://schemas.microsoft.com/office/word/2010/wordml";
    let sdt = format!(
        r#"<w:p><w:sdt><w:sdtPr><w:alias w:val="B"/><w:id w:val="3"/><w14:checkbox xmlns:w14="{w14}"><w14:checked w14:val="0"/><w14:checkedState w14:val="2610" w14:font="MS Gothic"/><w14:uncheckedState w14:val="2610" w14:font="MS Gothic"/></w14:checkbox></w:sdtPr><w:sdtContent><w:r><w:rPr><w:rFonts w:ascii="MS Gothic" w:hAnsi="MS Gothic"/></w:rPr><w:t>&#x2610;</w:t></w:r></w:sdtContent></w:sdt></w:p>"#
    );
    let mut e = ed(&sdt);
    let c = *e.document().controls.iter().find(|(_, k)| k.props.alias.as_deref() == Some("B")).map(|(k, _)| k).expect("the box");
    let value = |e: &Editor| match &e.document().controls[&c].props.kind {
        crate::docx::ControlKind::Checkbox { value, .. } => *value,
        _ => None,
    };
    assert_eq!(value(&e), Some(false));
    let id = crate::editing::elements::id_str(crate::docx::ElementId::Control(c));
    let set = serde_json::json!({ "id": id, "options": { "uncheckedSymbol": { "font": "Segoe UI Symbol", "character": "\u{2610}" } } });
    let out = e.execute(Command::SetControl(serde_json::from_value(set).unwrap()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(value(&e), Some(false), "still unchecked");
    let main = crate::docx::write::main_xml(e.document(), e.docx()).expect("writes").expect("changed");
    let want = r#"<w14:checkedState w14:val="2610" w14:font="MS Gothic"/><w14:uncheckedState w14:val="2610" w14:font="Segoe UI Symbol"/>"#;
    assert!(main.contains(want), "saved with its new symbol: {main}");
}

#[test]
fn wrapping_blocks_nests_the_new_control_inside_what_they_share() {
    let x = format!(r#"<w:sdt><w:sdtPr><w:id w:val="9"/><w:richText/></w:sdtPr><w:sdtContent><w:p>{}</w:p></w:sdtContent></w:sdt>"#, t("one"));
    let mut e = ed(&format!("{x}<w:p>{}</w:p>", t("two")));
    let (a, b) = (Position::text(StoryId::Body, para_id(&e, 0), Unit(0)), Position::text(StoryId::Body, para_id(&e, 1), Unit(3)));
    e.set_caret_for_benchmark(Selection::range(a, b, Granularity::Char));
    let ins = serde_json::json!({ "kind": "richText", "wrapSelection": true, "tag": "N" });
    let out = e.execute(Command::InsertControl(serde_json::from_value(ins).unwrap()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let lists: Vec<Vec<ControlId>> = e
        .document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .filter_map(|b| if let Block::Para(p) = b { Some(p.controls.clone()) } else { None })
        .collect();
    assert_eq!(lists[0].first(), lists[1].first(), "the new control is outermost in both: {lists:?}");
    let tagged = saved(&e).document().controls.values().filter(|c| c.props.tag.as_deref() == Some("N")).count();
    assert_eq!(tagged, 1, "it saves as one control");
}

#[test]
fn wrapping_blocks_across_a_control_boundary_is_refused() {
    let x = format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="9"/><w:richText/></w:sdtPr><w:sdtContent><w:p>{}</w:p><w:p>{}</w:p></w:sdtContent></w:sdt>"#,
        t("one"),
        t("two")
    );
    let mut e = ed(&format!("{x}<w:p>{}</w:p>", t("three")));
    let (a, b) = (Position::text(StoryId::Body, para_id(&e, 1), Unit(0)), Position::text(StoryId::Body, para_id(&e, 2), Unit(5)));
    e.set_caret_for_benchmark(Selection::range(a, b, Granularity::Char));
    let ins = serde_json::json!({ "kind": "richText", "wrapSelection": true, "tag": "N" });
    let out = e.execute(Command::InsertControl(serde_json::from_value(ins).unwrap()), fonts());
    assert!(!matches!(out, Outcome::Changed(_)), "{out:?}");
}

#[test]
fn a_new_placeholder_on_a_temporary_control_keeps_the_control() {
    let mut e = ed("<w:p/>");
    let ins = serde_json::json!({ "kind": "plainText", "placeholder": "Name", "tag": "T" });
    let out = e.execute(Command::InsertControl(serde_json::from_value(ins).unwrap()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let c = *e.document().controls.keys().next().expect("a control");
    let id = crate::editing::elements::id_str(crate::docx::ElementId::Control(c));
    let set = serde_json::json!({ "id": id, "temporary": true, "placeholder": "Enter name" });
    let out = e.execute(Command::SetControl(serde_json::from_value(set).unwrap()), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    assert_eq!(control_marks(&e, 0), (1, 1), "the control stays: {:?}", inlines(&e, 0));
    assert!(e.document().controls.get(&c).is_some_and(|k| k.props.temporary));
}

#[test]
fn replacing_in_codes_finds_text_across_runs() {
    let f = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">MERGEFIELD </w:instrText></w:r><w:r><w:rPr><w:b/></w:rPr><w:instrText>FirstName</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Ada</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let mut e = ed(&format!("<w:p>{f}</w:p>"));
    let r: crate::api::command::ReplaceInElements =
        serde_json::from_value(serde_json::json!({"find": "MERGEFIELD First", "replace": "MERGEFIELD Given", "in": "code"})).unwrap();
    let out = e.execute(Command::ReplaceInElements(r), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let v = inlines(&e, 0);
    let code: Vec<(&str, bool)> = v
        .iter()
        .skip_while(|i| !matches!(i, Inline::FieldBegin { .. }))
        .take_while(|i| !matches!(i, Inline::FieldSep { .. }))
        .filter_map(|i| if let Inline::Text(s, r) = i { Some((s.as_str(), r.bold)) } else { None })
        .collect();
    assert_eq!(code, [("MERGEFIELD Given", false), ("Name", true)], "each run keeps its formatting");
}

#[test]
fn replacing_in_a_code_of_many_runs_is_linear() {
    let n = 20_000;
    let runs: String =
        (0..n).map(|k| format!("<w:r>{}<w:instrText>x</w:instrText></w:r>", if k % 2 == 0 { "<w:rPr><w:b/></w:rPr>" } else { "" })).collect();
    let f = format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve">SET </w:instrText></w:r>{runs}<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>v</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    );
    let mut e = ed(&format!("<w:p>{f}</w:p>"));
    let r: crate::api::command::ReplaceInElements = serde_json::from_value(serde_json::json!({"find": "x", "replace": "y", "in": "code"})).unwrap();
    let start = std::time::Instant::now();
    let out = e.execute(Command::ReplaceInElements(r), fonts());
    let took = start.elapsed();
    assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
    let ys = inlines(&e, 0).iter().filter(|i| matches!(i, Inline::Text(s, _) if s == "y")).count();
    assert_eq!(ys, n, "every run's match is replaced");
    assert!(took < std::time::Duration::from_secs(5), "replaced in {took:?}");
}
