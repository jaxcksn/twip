//! Styles created, changed and read back without a dialog

use super::fonts;
use super::style_model::{open_pkg, pkg, reopened};
use crate::api::command::{Border, BorderEdges, BorderStyle, CellBorders, Command, ListKind, ParagraphSpacing, StyleMetaChange};
use crate::api::services::{StyleVisibility, TableStylePart};
use crate::api::state::Tri;
use crate::api::value::{Alignment, AutoOr, Color, HorizontalAlignment, Points, StyleId};
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::Block;
use crate::editing::Editor;
use crate::editing::commands::Outcome;
use crate::editing::position::{Position, StoryId};
use crate::editing::selection::Selection;
use crate::editing::style_defs::{NewStyle, StyleCategory, StyleChange, StyleList, TablePartFormatting};

const STYLES: &str = concat!(
    r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>"#,
    r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>"#,
    r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>"#,
    r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="Body"><w:name w:val="Body"/><w:basedOn w:val="Normal"/></w:style>"#,
    r#"<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/></w:style>"#,
    r#"<w:style w:type="character" w:customStyle="1" w:styleId="Emph"><w:name w:val="Emph"/><w:basedOn w:val="DefaultParagraphFont"/><w:rPr><w:i/></w:rPr></w:style>"#,
    r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/></w:style>"#,
);

fn editor(body: &str) -> Editor {
    open_pkg(&pkg(STYLES, body))
}

fn plain() -> Editor {
    editor(r#"<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Body"/></w:pPr><w:r><w:t>two</w:t></w:r></w:p>"#)
}

fn paras(e: &Editor) -> Vec<&crate::docx::Paragraph> {
    e.document().sections.iter().flat_map(|s| s.blocks.iter()).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect()
}

fn caret_in(e: &mut Editor, n: usize) {
    let at = Position::text(StoryId::Body, super::para_id(e, n), crate::layout::Unit(0));
    e.set_caret_for_benchmark(Selection::Caret(at));
}

fn id(s: &str) -> StyleId {
    StyleId(s.into())
}

fn refused(o: &Outcome) -> bool {
    matches!(o, Outcome::Refused(_))
}

#[test]
fn a_new_paragraph_style_is_made_applied_and_read_back() {
    let mut e = plain();
    caret_in(&mut e, 0);
    let change = StyleChange {
        name: Some("Callout".into()),
        next: Some(Some(id("Normal"))),
        formatting: vec![
            Command::SetBold(true),
            Command::SetFontSize(Points(14.0)),
            Command::SetAlignment(Alignment::Center),
            Command::SetParagraphSpacing(ParagraphSpacing { after: Some(AutoOr::Value(Points(6.0))), ..Default::default() }),
        ],
        ..Default::default()
    };
    let (out, made) = e.create_style(NewStyle { category: StyleCategory::Paragraph, apply: true, change, ..Default::default() }, fonts());
    assert!(!refused(&out), "{out:?}");
    assert_eq!(made, Some(id("Callout")));
    assert_eq!(paras(&e)[0].style.style_id.as_deref(), Some("Callout"), "applied");
    assert!(paras(&e)[0].mark.bold);
    for e in [&e, &reopened(&e)] {
        let d = e.style_definition(&id("Callout")).expect("defined");
        assert_eq!((d.category, d.built_in, d.defined, d.quick), (StyleCategory::Paragraph, false, true, true));
        assert_eq!((d.based_on, d.next), (Some(id("Normal")), Some(id("Normal"))));
        assert_eq!((d.character.bold, d.character.size), (Tri::Uniform(true), Tri::Uniform(Points(14.0))));
        assert_eq!(d.paragraph.alignment, Tri::Uniform(Alignment::Center));
        assert_eq!(d.own_character.bold, Tri::Uniform(true), "set by it");
        assert_eq!(d.own_character.italic, Tri::Unavailable, "inherited");
        assert_eq!(d.own_paragraph.space_after, Tri::Uniform(AutoOr::Value(Points(6.0))));
        assert_eq!(d.own_paragraph.indents.left, Tri::Unavailable);
        assert!(d.description.contains("Bold"), "{}", d.description);
    }
    // One undo step takes it away.
    let u = e.execute(Command::Undo(Default::default()), fonts());
    assert!(e.style_definition(&id("Callout")).is_none(), "{u:?} {:?}", e.style_definition(&id("Callout")).map(|d| d.defined));
    assert_ne!(paras(&e)[0].style.style_id.as_deref(), Some("Callout"));
}

#[test]
fn a_new_style_from_the_selection_takes_its_formatting_and_its_base() {
    let mut e = editor(r#"<w:p><w:pPr><w:pStyle w:val="Body"/><w:jc w:val="right"/></w:pPr><w:r><w:rPr><w:i/></w:rPr><w:t>one</w:t></w:r></w:p>"#);
    caret_in(&mut e, 0);
    let (out, made) = e.create_style(NewStyle { from_selection: true, ..Default::default() }, fonts());
    assert!(!refused(&out), "{out:?}");
    let d = e.style_definition(made.as_ref().expect("made")).expect("defined");
    assert_eq!((d.name.as_str(), d.based_on.clone()), ("Style1", Some(id("Body"))), "Word's first free name, the paragraph's style");
    assert_eq!((d.own_paragraph.alignment, d.own_character.italic), (Tri::Uniform(Alignment::Right), Tri::Uniform(true)));
}

#[test]
fn a_style_is_renamed_rebased_and_relinked() {
    let mut e = plain();
    let o = e.modify_style(
        &id("Body"),
        StyleChange { name: Some("Body Text Plain".into()), based_on: Some(Some(id("Heading1"))), ..Default::default() },
        fonts(),
    );
    assert!(!refused(&o), "{o:?}");
    let d = e.style_definition(&id("Body")).expect("Body");
    assert_eq!((d.name.as_str(), d.based_on.clone(), d.character.bold), ("Body Text Plain", Some(id("Heading1")), Tri::Uniform(true)));
    assert!(paras(&e)[1].mark.bold, "its paragraph lays out again from its new base");
    // A built-in style keeps its name; the new one is an alias.
    e.modify_style(&id("Heading1"), StyleChange { name: Some("Big".into()), ..Default::default() }, fonts());
    let h = e.style_definition(&id("Heading1")).expect("Heading1");
    assert_eq!((h.name.as_str(), h.aliases.clone()), ("Heading 1", vec!["Big".to_string()]));
    // Refused: a loop, a base of another type, a following character style, a taken name, paragraph formatting on a
    // character style, table formatting on a paragraph style.
    for (style, c) in [
        ("Heading1", StyleChange { based_on: Some(Some(id("Body"))), ..Default::default() }),
        ("Body", StyleChange { based_on: Some(Some(id("Emph"))), ..Default::default() }),
        ("Body", StyleChange { next: Some(Some(id("Emph"))), ..Default::default() }),
        ("Body", StyleChange { name: Some("emph".into()), ..Default::default() }),
        ("Body", StyleChange { name: Some("Heading 3".into()), ..Default::default() }),
        ("Emph", StyleChange { formatting: vec![Command::SetAlignment(Alignment::Center)], ..Default::default() }),
        ("Emph", StyleChange { next: Some(None), ..Default::default() }),
        ("Body", StyleChange { formatting: vec![Command::SetCellShading(None)], ..Default::default() }),
        ("Body", StyleChange { formatting: vec![Command::InsertText("x".into())], ..Default::default() }),
        ("Body", StyleChange { row_band: Some(2), ..Default::default() }),
    ] {
        assert!(refused(&e.modify_style(&id(style), c.clone(), fonts())), "{style}: {c:?}");
    }
    // Linked both ways; linking another unlinks the first.
    e.modify_style(&id("Body"), StyleChange { link: Some(Some(id("Emph"))), ..Default::default() }, fonts());
    assert_eq!(e.style_definition(&id("Emph")).expect("Emph").link, Some(id("Body")));
    assert_eq!(e.style_definition(&id("Body")).expect("Body").category, StyleCategory::Linked);
    e.modify_style(&id("Heading1"), StyleChange { link: Some(Some(id("Emph"))), ..Default::default() }, fonts());
    let links = |e: &Editor| ["Body", "Heading1", "Emph"].map(|s| e.style_definition(&id(s)).expect("defined").link);
    let relinked = [None, Some(id("Emph")), Some(id("Heading1"))];
    assert_eq!(links(&e), relinked, "the old partner lets go");
    // One undo step: all three sides back, then again.
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(links(&e), [Some(id("Emph")), None, Some(id("Body"))], "undone");
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(links(&e), relinked, "redone");
    e.modify_style(&id("Heading1"), StyleChange { link: Some(None), ..Default::default() }, fonts());
    assert_eq!(e.style_definition(&id("Emph")).expect("Emph").link, None);
    // Flags.
    e.modify_style(&id("Body"), StyleChange { auto_update: Some(true), quick: Some(true), ..Default::default() }, fonts());
    let d = e.style_definition(&id("Body")).expect("Body");
    assert!(d.auto_update && d.quick);
    let again = reopened(&e);
    let d = again.style_definition(&id("Body")).expect("Body");
    assert!(d.auto_update && d.quick && d.name == "Body Text Plain");
}

#[test]
fn a_linked_style_makes_its_character_half_and_shares_its_character_formatting() {
    let mut e = plain();
    let change = StyleChange { name: Some("Note".into()), formatting: vec![Command::SetItalic(true)], ..Default::default() };
    let (_, made) = e.create_style(NewStyle { category: StyleCategory::Linked, change, ..Default::default() }, fonts());
    let d = e.style_definition(made.as_ref().expect("made")).expect("Note");
    assert_eq!(d.category, StyleCategory::Linked);
    let half = d.link.expect("its character half");
    let c = e.style_definition(&half).expect("the half");
    assert_eq!((c.category, c.name.as_str(), c.own_character.italic), (StyleCategory::Character, "Note Char", Tri::Uniform(true)));
    e.modify_style(
        &id("Note"),
        StyleChange {
            formatting: vec![Command::SetFontColor(AutoOr::Value(crate::api::drawing::ColourValue::rgb(Color(0xFF0000))))],
            ..Default::default()
        },
        fonts(),
    );
    let colors = |e: &Editor| [id("Note"), half.clone()].map(|s| e.style_definition(&s).expect("defined").own_character.color);
    let red = Tri::Uniform(AutoOr::Value(Color(0xFF0000)));
    assert_eq!(colors(&e), [red, red]);
    // Both halves in one undo step.
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(colors(&e), [Tri::Unavailable, Tri::Unavailable], "undone");
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(colors(&e), [red, red], "redone");
    // Linked to nothing: refused, nothing written.
    let (depth, styles) = (e.history().undo_depth(), e.document().resolver.as_ref().expect("styles").styles.by_id.len());
    let change = StyleChange { name: Some("Loose".into()), link: Some(None), ..Default::default() };
    let (out, made) = e.create_style(NewStyle { category: StyleCategory::Linked, change, ..Default::default() }, fonts());
    assert!(refused(&out) && made.is_none(), "{out:?}");
    assert_eq!((e.history().undo_depth(), e.document().resolver.as_ref().expect("styles").styles.by_id.len()), (depth, styles));
}

#[test]
fn a_paragraph_style_numbers_with_a_list() {
    let mut e = plain();
    caret_in(&mut e, 0);
    e.execute(Command::SetList(Some(ListKind::Numbered)), fonts());
    let ListRef { list: ListId(list), level: _ } = paras(&e)[0].style.num.expect("a list");
    let o = e.modify_style(&id("Body"), StyleChange { list: Some(Some(StyleList { list, level: 1 })), ..Default::default() }, fonts());
    assert!(!refused(&o), "{o:?}");
    assert!(paras(&e)[1].list.is_some(), "Body's paragraph is numbered");
    let d = e.style_definition(&id("Body")).expect("Body");
    assert_eq!(d.list, Some(StyleList { list, level: 1 }));
    assert_eq!((d.paragraph.list, d.paragraph.list_level), (Tri::Uniform(crate::api::state::ListState::Numbered), Tri::Uniform(1)));
    assert!(refused(&e.modify_style(
        &id("Body"),
        StyleChange { list: Some(Some(StyleList { list: 999, level: 0 })), ..Default::default() },
        fonts()
    )));
    e.modify_style(&id("Body"), StyleChange { list: Some(None), ..Default::default() }, fonts());
    assert!(paras(&e)[1].list.is_none());
    // No list over a base style's.
    let normal = StyleChange { list: Some(Some(StyleList { list, level: 0 })), ..Default::default() };
    assert!(!refused(&e.modify_style(&id("Normal"), normal, fonts())));
    assert!(paras(&e)[1].list.is_none() && e.style_definition(&id("Body")).expect("Body").list.is_none(), "Body's own no list");
}

#[test]
fn a_new_base_reformats_the_styles_numbers() {
    let mut e = plain();
    caret_in(&mut e, 0);
    e.execute(Command::SetList(Some(ListKind::Numbered)), fonts());
    let ListRef { list: ListId(list), level: _ } = paras(&e)[0].style.num.expect("a list");
    let own = StyleChange { list: Some(Some(StyleList { list, level: 0 })), ..Default::default() };
    assert!(!refused(&e.modify_style(&id("Body"), own, fonts())));
    let label = |e: &Editor| paras(e)[1].list.as_ref().map(|l| (l.text.clone(), l.run.bold)).expect("a number");
    let (text, bold) = label(&e);
    assert!(!bold);
    // A bold base: its number bold, its text the same
    let rebase = StyleChange { based_on: Some(Some(id("Heading1"))), ..Default::default() };
    assert!(!refused(&e.modify_style(&id("Body"), rebase, fonts())));
    assert_eq!(label(&e), (text, true));
}

#[test]
fn a_table_style_takes_its_parts_formatting_and_bands() {
    let mut e = plain();
    let red = Color(0xFF0000);
    let line = Border { style: BorderStyle::Single, width: Points(1.0), color: AutoOr::Auto, space: None, shadow: None };
    let change = StyleChange {
        name: Some("Grid Red".into()),
        formatting: vec![
            Command::SetCellBorders(CellBorders { edges: BorderEdges::All, border: Some(line.clone()) }),
            Command::SetTableAlignment(HorizontalAlignment::Center),
            Command::SetFontSize(Points(9.0)),
        ],
        parts: vec![TablePartFormatting {
            part: TableStylePart::HeaderRow,
            clear: false,
            formatting: vec![
                Command::SetBold(true),
                Command::SetCellShading(Some(crate::api::drawing::ColourValue::rgb(red))),
                Command::SetCellAlignment(crate::api::value::CellAlignment::Center),
            ],
        }],
        row_band: Some(2),
        ..Default::default()
    };
    let (out, made) = e.create_style(NewStyle { category: StyleCategory::Table, change, ..Default::default() }, fonts());
    assert!(!refused(&out), "{out:?}");
    for e in [&e, &reopened(&e)] {
        let t = e.style_definition(made.as_ref().expect("made")).expect("defined").table.expect("a table style");
        assert_eq!((t.alignment, t.row_band, t.column_band), (HorizontalAlignment::Center, 2, 1));
        let whole = &t.parts[0];
        assert_eq!(
            (whole.part, whole.borders.top.clone(), whole.borders.inside_vertical.clone()),
            (TableStylePart::WholeTable, Some(line.clone()), Some(line.clone()))
        );
        assert_eq!(whole.character.size, Tri::Uniform(Points(9.0)));
        let head = t.parts.iter().find(|p| p.part == TableStylePart::HeaderRow).expect("the header row");
        assert!(head.defined && head.fill == Some(red) && head.character.bold == Tri::Uniform(true));
        assert_eq!((head.cell_alignment, head.character.size), (crate::api::value::CellAlignment::Center, Tri::Uniform(Points(9.0))));
        assert!(!t.parts.iter().find(|p| p.part == TableStylePart::TotalRow).expect("the total row").defined);
    }
    // The part's own formatting cleared; the whole table's alignment isn't a part's.
    let clear = StyleChange {
        parts: vec![TablePartFormatting { part: TableStylePart::HeaderRow, clear: true, formatting: Vec::new() }],
        ..Default::default()
    };
    e.modify_style(made.as_ref().expect("made"), clear, fonts());
    let t = e.style_definition(made.as_ref().expect("made")).expect("defined").table.expect("a table style");
    assert!(!t.parts.iter().find(|p| p.part == TableStylePart::HeaderRow).expect("the header row").defined);
    let wrong = StyleChange {
        parts: vec![TablePartFormatting {
            part: TableStylePart::HeaderRow,
            clear: false,
            formatting: vec![Command::SetTableAlignment(HorizontalAlignment::Right)],
        }],
        ..Default::default()
    };
    assert!(refused(&e.modify_style(made.as_ref().expect("made"), wrong, fonts())));
    assert!(refused(&e.modify_style(made.as_ref().expect("made"), StyleChange { row_band: Some(0), ..Default::default() }, fonts())));
}

#[test]
fn a_list_style_is_made_with_its_levels() {
    let mut e = plain();
    let mut levels = crate::editing::list_defs::complete(&[]);
    levels[0].format = crate::api::command::NumberFormat::UpperRoman;
    let change = StyleChange { name: Some("Outline List".into()), list_levels: Some(levels), ..Default::default() };
    let (out, made) = e.create_style(NewStyle { category: StyleCategory::List, change, ..Default::default() }, fonts());
    assert!(!refused(&out), "{out:?}");
    let d = e.style_definition(made.as_ref().expect("made")).expect("defined");
    assert_eq!((d.category, d.list_levels.len()), (StyleCategory::List, 9));
    assert_eq!(d.list_levels[0].format, crate::api::command::NumberFormat::UpperRoman);
    assert!(refused(&e.modify_style(
        made.as_ref().expect("made"),
        StyleChange { formatting: vec![Command::SetBold(true)], ..Default::default() },
        fonts()
    )));
    // Applied without levels: refused for that
    let change = StyleChange { name: Some("Bare List".into()), ..Default::default() };
    let (out, _) = e.create_style(NewStyle { category: StyleCategory::List, change, apply: true, ..Default::default() }, fonts());
    assert!(matches!(&out, Outcome::Refused(err) if err.message.contains("list_levels")), "{out:?}");
}

#[test]
fn built_in_styles_list_with_their_latent_flags_and_take_an_exception() {
    let mut e = plain();
    let all = e.built_in_styles();
    assert_eq!(all.len(), crate::services::styles::builtin::BUILT_IN.len());
    let h5 = all.iter().find(|b| b.id.0 == "Heading5").expect("heading 5");
    assert_eq!(
        (h5.defined, h5.visibility, h5.name.as_str()),
        (false, StyleVisibility::HideUntilUsed, "Heading 5"),
        "Word's own flags: no latent styles"
    );
    assert!(all.iter().find(|b| b.id.0 == "Heading1").expect("heading 1").defined);
    let d = e.style_definition(&id("Heading5")).expect("a built-in the document lacks");
    assert!(!d.defined && d.built_in && d.based_on == Some(id("Normal")));
    let o = e.execute(
        Command::SetStyleMeta(StyleMetaChange {
            id: id("Heading5"),
            visibility: Some(StyleVisibility::Show),
            priority: Some(5),
            ..Default::default()
        }),
        fonts(),
    );
    assert!(!refused(&o), "{o:?}");
    for e in [&e, &reopened(&e)] {
        let all = e.built_in_styles();
        let h5 = all.iter().find(|b| b.id.0 == "Heading5").expect("heading 5");
        assert_eq!((h5.defined, h5.visibility, h5.priority), (false, StyleVisibility::Show, 5));
        let h6 = all.iter().find(|b| b.id.0 == "Heading6").expect("heading 6");
        assert_eq!((h6.visibility, h6.priority), (StyleVisibility::HideUntilUsed, 9), "the others keep Word's");
    }
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.built_in_styles().iter().find(|b| b.id.0 == "Heading5").expect("heading 5").visibility, StyleVisibility::HideUntilUsed);
}

#[test]
fn a_built_in_style_the_document_lacks_joins_it_when_modified() {
    let mut e = plain();
    let o = e.modify_style(&id("Heading2"), StyleChange { formatting: vec![Command::SetItalic(true)], ..Default::default() }, fonts());
    assert!(!refused(&o), "{o:?}");
    let d = e.style_definition(&id("Heading2")).expect("Heading2");
    assert!(d.defined && d.built_in && d.own_character.italic == Tri::Uniform(true));
}

#[test]
fn style_edits_are_refused_while_formatting_is_restricted() {
    let mut e = plain();
    e.execute(
        Command::SetDocumentProtection(crate::api::command::DocumentProtection { formatting: Some(true), enforce: Some(true), ..Default::default() }),
        fonts(),
    );
    assert!(refused(&e.modify_style(&id("Body"), StyleChange { quick: Some(true), ..Default::default() }, fonts())));
    assert!(refused(&e.create_style(NewStyle::default(), fonts()).0));
    assert!(refused(&e.set_default_formatting(vec![Command::SetBold(true)], fonts())));
    assert!(refused(&e.set_list_levels(Default::default(), fonts())));
    assert!(refused(&e.set_table_band_sizes(Some(Some(2)), None, fonts())));
}

#[test]
fn style_edits_are_refused_under_restrict_editing() {
    let mut e = plain();
    let no_changes = crate::api::command::DocumentProtection {
        kind: Some(crate::api::command::ProtectionKind::ReadOnly),
        enforce: Some(true),
        ..Default::default()
    };
    e.execute(Command::SetDocumentProtection(no_changes), fonts());
    assert!(refused(&e.modify_style(&id("Body"), StyleChange { quick: Some(true), ..Default::default() }, fonts())));
    assert!(refused(&e.create_style(NewStyle::default(), fonts()).0));
    assert!(refused(&e.set_table_band_sizes(Some(Some(2)), None, fonts())));
    assert!(refused(&e.set_cell_shading(Some(crate::api::drawing::ColourValue::rgb(Color(0xFF0000))), fonts())));
}

#[test]
fn every_default_sets_and_reads_back_without_a_dialog() {
    use crate::api::command::{AddTabStop, Language, ParagraphFlow};
    let mut e = plain();
    let d = e.document_defaults().expect("defaults");
    assert_eq!((d.character.size, d.own_character.size), (Tri::Uniform(Points(11.0)), Tri::Uniform(Points(11.0))));
    assert_eq!(d.own_character.bold, Tri::Unavailable, "the defaults don't set it");
    let o = e.set_default_formatting(
        vec![
            Command::SetBold(true),
            Command::SetItalic(true),
            Command::SetLanguage(Language { language: Some("fr-FR".into()), ..Default::default() }),
            Command::SetParagraphFlow(ParagraphFlow { widow_control: Some(false), keep_with_next: Some(true), ..Default::default() }),
            Command::AddTabStop(AddTabStop { position: Points(72.0), alignment: None, leader: None }),
        ],
        fonts(),
    );
    assert!(!refused(&o), "{o:?}");
    for e in [&e, &reopened(&e)] {
        let d = e.document_defaults().expect("defaults");
        assert_eq!((d.own_character.bold, d.own_character.italic), (Tri::Uniform(true), Tri::Uniform(true)));
        assert_eq!(d.own_character.language, Tri::Uniform("fr-FR".to_string()));
        assert_eq!((d.paragraph.flow.widow_control, d.paragraph.flow.keep_with_next), (Tri::Uniform(false), Tri::Uniform(true)));
        assert_eq!(d.tabs.iter().map(|t| t.position).collect::<Vec<_>>(), vec![Points(72.0)]);
        assert!(paras(e)[0].mark.bold, "text in the default style takes them");
    }
    assert!(refused(&e.set_default_formatting(vec![Command::RemoveFrame], fonts())));
    // One undo step takes them all away, and redo puts them back.
    e.execute(Command::Undo(Default::default()), fonts());
    let d = e.document_defaults().expect("defaults");
    assert_eq!((d.own_character.bold, d.paragraph.flow.widow_control, d.tabs.len()), (Tri::Unavailable, Tri::Uniform(true), 0), "undone");
    assert!(!paras(&e)[0].mark.bold);
    e.execute(Command::Redo(Default::default()), fonts());
    let d = e.document_defaults().expect("defaults");
    assert_eq!((d.own_character.bold, d.paragraph.flow.widow_control, d.tabs.len()), (Tri::Uniform(true), Tri::Uniform(false), 1), "redone");
    assert!(paras(&e)[0].mark.bold);
    // Turned off again: written off, not left to toggle.
    e.set_default_formatting(vec![Command::SetBold(false)], fonts());
    assert_eq!(e.document_defaults().expect("defaults").character.bold, Tri::Uniform(false));
    assert!(!paras(&e)[0].mark.bold);
}

#[test]
fn a_table_overrides_its_styles_band_sizes() {
    let mut e = plain();
    caret_in(&mut e, 0);
    e.execute(Command::InsertTable(crate::api::command::TableSize { rows: 4, columns: 2 }), fonts());
    let b = e.table_band_sizes().expect("in a table");
    assert_eq!((b.rows, b.own_rows), (1, None), "Word's one row bands");
    let o = e.set_table_band_sizes(Some(Some(2)), Some(Some(3)), fonts());
    assert!(!refused(&o), "{o:?}");
    let b = e.table_band_sizes().expect("in a table");
    assert_eq!((b.rows, b.columns, b.own_rows, b.own_columns), (2, 3, Some(2), Some(3)));
    let doc = {
        use std::io::Read;
        let bytes = crate::docx::write::save(e.document()).expect("saves");
        let mut s = String::new();
        zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .expect("a zip")
            .by_name("word/document.xml")
            .expect("the body")
            .read_to_string(&mut s)
            .expect("text");
        s
    };
    assert!(doc.contains("w:tblStyleRowBandSize w:val=\"2\"") && doc.contains("w:tblStyleColBandSize w:val=\"3\""), "{doc}");
    e.execute(Command::Undo(Default::default()), fonts());
    assert_eq!(e.table_band_sizes().map(|b| (b.own_rows, b.own_columns)), Some((None, None)), "undone");
    e.execute(Command::Redo(Default::default()), fonts());
    assert_eq!(e.table_band_sizes().map(|b| (b.own_rows, b.own_columns)), Some((Some(2), Some(3))), "redone");
    e.set_table_band_sizes(Some(None), None, fonts());
    let b = e.table_band_sizes().expect("in a table");
    assert_eq!((b.rows, b.own_rows, b.own_columns), (1, None, Some(3)), "back to the style's");
    assert!(refused(&e.set_table_band_sizes(Some(Some(0)), None, fonts())));
}

#[test]
fn a_tracked_band_size_change_records_the_tables_previous_properties() {
    let mut e = plain();
    caret_in(&mut e, 0);
    e.execute(Command::InsertTable(crate::api::command::TableSize { rows: 4, columns: 2 }), fonts());
    e.set_author("Author A", None);
    assert!(e.set_tracking(true));
    let o = e.set_table_band_sizes(Some(Some(2)), None, fonts());
    assert!(!refused(&o), "{o:?}");
    let doc = body_xml(&e);
    // Word: a table property change, its previous w:tblPr without the band size.
    let at = doc.find("<w:tblPrChange").unwrap_or_else(|| panic!("a tracked table property change: {doc}"));
    let change = &doc[at..at + doc[at..].find("</w:tblPrChange>").expect("closed")];
    assert!(change.contains("w:author=\"Author A\"") && change.contains("<w:tblPr>"), "{change}");
    assert!(!change.contains("tblStyleRowBandSize"), "the previous properties have no band size: {change}");
    assert!(doc[..at].contains("<w:tblStyleRowBandSize w:val=\"2\"/>"), "the new one is the table's: {doc}");
    let again = reopened(&e);
    let t = again
        .document()
        .sections
        .iter()
        .flat_map(|s| s.blocks.iter())
        .find_map(|b| if let Block::Table(t) = b { Some(t) } else { None })
        .expect("a table");
    assert!(t.change.is_some(), "the change reads back");
}

fn body_xml(e: &Editor) -> String {
    use std::io::Read;
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut s = String::new();
    zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .expect("a zip")
        .by_name("word/document.xml")
        .expect("the body")
        .read_to_string(&mut s)
        .expect("text");
    s
}

#[test]
fn a_table_style_takes_shading_margins_and_row_breaking() {
    use crate::api::command::{CellMargins, ShadingChange, ShadingTarget};
    use crate::api::value::NoneOr;
    let mut e = plain();
    let shade = |target, c: u32| {
        Command::SetShading(ShadingChange {
            target,
            fill: NoneOr::Value(crate::api::drawing::ColourValue::rgb(Color(c))),
            pattern: Default::default(),
            color: AutoOr::Auto,
        })
    };
    let margins = CellMargins { top: Some(Points(2.0)), bottom: Some(Points(3.0)), left: Some(Points(4.0)), right: Some(Points(5.0)) };
    let change = StyleChange {
        name: Some("Shaded".into()),
        formatting: vec![shade(ShadingTarget::Table, 0x00FF00), Command::SetCellMargins(margins.clone()), Command::SetRowCanBreak(false)],
        parts: vec![TablePartFormatting {
            part: TableStylePart::HeaderRow,
            clear: false,
            formatting: vec![
                shade(ShadingTarget::Cell, 0x0000FF),
                Command::SetCellMargins(CellMargins { left: Some(Points(9.0)), ..Default::default() }),
                Command::SetRowCanBreak(true),
            ],
        }],
        ..Default::default()
    };
    let (out, made) = e.create_style(NewStyle { category: StyleCategory::Table, change, ..Default::default() }, fonts());
    assert!(!refused(&out), "{out:?}");
    for e in [&e, &reopened(&e)] {
        let t = e.style_definition(made.as_ref().expect("made")).expect("defined").table.expect("a table style");
        assert_eq!(t.fill, Some(Color(0x00FF00)), "the table's fill");
        let whole = &t.parts[0];
        assert_eq!((&whole.cell_margins, whole.row_can_break), (&margins, false));
        let head = t.parts.iter().find(|p| p.part == TableStylePart::HeaderRow).expect("the header row");
        assert_eq!((head.fill, head.cell_margins.left, head.row_can_break), (Some(Color(0x0000FF)), Some(Points(9.0)), true));
    }
    // Paragraph shading isn't a table's; cell shading isn't a paragraph style's.
    let wrong = StyleChange { formatting: vec![shade(ShadingTarget::Cell, 0xFF0000)], ..Default::default() };
    assert!(refused(&e.modify_style(&id("Body"), wrong, fonts())));
}

#[test]
fn a_table_style_lets_rows_break_over_a_base_that_doesnt() {
    let mut e = plain();
    let table = |name: &str, based_on: Option<StyleId>, on: bool| NewStyle {
        category: StyleCategory::Table,
        change: StyleChange {
            name: Some(name.into()),
            based_on: based_on.map(Some),
            formatting: vec![Command::SetRowCanBreak(on)],
            ..Default::default()
        },
        ..Default::default()
    };
    let (out, base) = e.create_style(table("Unbroken", None, false), fonts());
    assert!(!refused(&out), "{out:?}");
    let (out, made) = e.create_style(table("Broken", base, true), fonts());
    assert!(!refused(&out), "{out:?}");
    for e in [&e, &reopened(&e)] {
        let t = e.style_definition(made.as_ref().expect("made")).expect("defined").table.expect("a table style");
        assert!(t.parts[0].row_can_break, "over its base's");
    }
}

#[test]
fn a_table_parts_font_grows_from_its_own_size() {
    let header = r#"<w:tblStylePr w:type="firstRow"><w:rPr><w:sz w:val="28"/></w:rPr></w:tblStylePr>"#;
    let tt = format!(r#"<w:style w:type="table" w:styleId="TT"><w:name w:val="TT"/><w:rPr><w:sz w:val="20"/></w:rPr>{header}</w:style>"#);
    let mut e = open_pkg(&pkg(&format!("{STYLES}{tt}"), "<w:p/>"));
    let grow = |part: Option<TableStylePart>| match part {
        Some(part) => {
            StyleChange { parts: vec![TablePartFormatting { part, clear: false, formatting: vec![Command::GrowFont] }], ..Default::default() }
        }
        None => StyleChange { formatting: vec![Command::GrowFont], ..Default::default() },
    };
    assert!(!refused(&e.modify_style(&id("TT"), grow(Some(TableStylePart::HeaderRow)), fonts())));
    assert!(!refused(&e.modify_style(&id("TT"), grow(None), fonts())));
    let t = e.style_definition(&id("TT")).expect("defined").table.expect("a table style");
    let head = t.parts.iter().find(|p| p.part == TableStylePart::HeaderRow).expect("the header row");
    assert_eq!((t.parts[0].character.size, head.character.size), (Tri::Uniform(Points(10.5)), Tri::Uniform(Points(16.0))));
}

#[test]
fn a_table_styles_definition_lists_its_tab_stops() {
    let tt = r#"<w:style w:type="table" w:styleId="TT"><w:name w:val="TT"/><w:pPr><w:tabs><w:tab w:val="left" w:pos="1440"/><w:tab w:val="clear" w:pos="720"/></w:tabs></w:pPr></w:style>"#;
    let e = open_pkg(&pkg(&format!("{STYLES}{tt}"), "<w:p/>"));
    let d = e.style_definition(&id("TT")).expect("defined");
    assert_eq!(d.tabs.iter().map(|t| t.position).collect::<Vec<_>>(), vec![Points(72.0)], "a clear isn't a stop");
}

#[test]
fn a_document_with_many_latent_exceptions_lists_built_ins_in_one_pass() {
    let many: String = (0..20_000).map(|k| format!(r#"<w:lsdException w:name="x{k}" w:uiPriority="1"/>"#)).collect();
    let latent = format!(
        r#"<w:latentStyles w:defUIPriority="99">{many}<w:lsdException w:name="Heading 5" w:semiHidden="0" w:uiPriority="4"/></w:latentStyles>"#
    );
    let e = open_pkg(&pkg(&format!("{latent}{STYLES}"), "<w:p/>"));
    let h5 = e.built_in_styles().into_iter().find(|b| b.id.0 == "Heading5").expect("heading 5");
    assert_eq!((h5.priority, h5.visibility), (4, StyleVisibility::Show), "matched without case");
}

#[test]
fn the_defaults_formatted_as_a_style_resolve_alone() {
    // Defaults that turn a toggle on, which would turn it off again over themselves
    let styles = STYLES.replace(r#"<w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault>"#, r#"<w:rPr><w:b/><w:sz w:val="22"/></w:rPr></w:rPrDefault>"#);
    let mut e = open_pkg(&pkg(&styles, r#"<w:p><w:r><w:t>one</w:t></w:r></w:p>"#));
    let stand_in = r#"<w:style w:type="paragraph" w:styleId="DocumentDefaults"><w:name w:val="Document Defaults"/><w:rPr><w:b/><w:sz w:val="22"/></w:rPr></w:style>"#;
    let (_, over, _) = e.style_formatting(stand_in).expect("resolves");
    e.defaults_target = true;
    let alone = e.style_formatting(stand_in).expect("resolves");
    e.defaults_target = false;
    assert!(!over.bold, "a style's bold over bold defaults turns it off");
    assert!(alone.1.bold, "the defaults' own bold, not over themselves");
}
