//! Style edits: one undo step, and the same formatting as the document read again

use super::super::Editor;
use super::super::commands::Command;
use super::{fonts, open, texts, texts_ids};
use crate::docx::{Block, Inline};

fn fixture() -> Editor {
    let mut e = open("FX-STYLES.docx");
    e.complete(fonts());
    e
}

/// Every body paragraph's resolved paragraph formatting and its runs' sizes, colours and emphasis.
fn formatting(e: &Editor) -> Vec<String> {
    let ids = texts_ids(e);
    texts(e)
        .into_iter()
        .zip(ids)
        .map(|(t, id)| {
            let p = e.index().para(e.document(), id).expect("a paragraph");
            let runs: Vec<String> = p
                .inlines
                .iter()
                .filter_map(|i| if let Inline::Text(_, r) = i { Some(format!("{}/{:?}/{}/{}", r.size, r.color, r.bold, r.italic)) } else { None })
                .collect();
            format!("{t:.20} {:?} {:?} {:?} {}", p.style.style_id, p.style.space_before, p.style.jc, runs.join(" "))
        })
        .collect()
}

fn read_again(e: &Editor) -> Editor {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let mut again = Editor::open(&bytes, fonts()).expect("reads again");
    again.complete(fonts());
    again
}

#[test]
fn a_style_edit_formats_its_users_as_a_fresh_read_does_and_undoes_in_one_step() {
    let mut e = fixture();
    let before = formatting(&e);
    let h1 = e.document().resolver.as_ref().expect("styles").styles.get("Heading1").expect("Heading 1").raw.clone();
    let bigger = h1.replace("<w:sz w:val=\"32\"/>", "<w:sz w:val=\"40\"/>").replace("<w:b/>", "<w:b/><w:color w:val=\"C00000\"/>");
    let out = e.edit_styles("Modify Style", |s| s.set_xml(bigger), fonts());
    assert!(matches!(out, super::super::commands::Outcome::Changed(_)), "{out:?}");
    let after = formatting(&e);
    assert_ne!(after, before);
    assert_eq!(after, formatting(&read_again(&e)), "the same formatting as the saved document read again");
    let changed: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
    assert!(
        changed.iter().all(|&i| ["FX-STYLES", "Heading two", "Heading three"].iter().any(|t| after[i].starts_with(t))),
        "only Heading 1 and its descendants: {changed:?}"
    );
    e.execute(Command::Undo, fonts());
    assert_eq!(formatting(&e), before, "one undo step restores it");
    e.execute(Command::Redo, fonts());
    assert_eq!(formatting(&e), after);
}

#[test]
fn deleting_a_character_style_drops_it_from_its_runs_formatting() {
    let mut e = fixture();
    e.edit_styles("Delete Style", |s| s.delete("Emph"), fonts());
    let again = read_again(&e);
    assert_eq!(formatting(&e), formatting(&again));
    let red = |e: &Editor| {
        e.document().sections[0].blocks.iter().any(|b| match b {
            Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::Text(t, r) if t.contains("emphasised") && r.italic)),
            Block::Table(_) => false,
        })
    };
    assert!(!red(&e), "the run lost the style's italic");
}

#[test]
fn formatting_commands_edit_a_style_and_leave_the_document_alone() {
    use crate::api::command::{AddTabStop, Command as C, Indents};
    use crate::api::value::{Alignment, Points};
    let mut e = fixture();
    let (revision, before) = (e.revision(), formatting(&e));
    let raw = e.document().resolver.as_ref().expect("styles").styles.get("Heading1").expect("Heading 1").raw.clone();
    let commands = vec![
        C::SetFontSize(Points(20.0)),
        C::SetItalic(true),
        C::SetIndents(Indents { left: Some(Points(36.0)), right: None, first_line: None, hanging: None }),
        C::SetAlignment(Alignment::Center),
        C::AddTabStop(AddTabStop { position: Points(144.0), alignment: None, leader: None }),
    ];
    let xml = e.format_style(&raw, commands, fonts()).expect("formats the style");
    for part in [
        "<w:sz w:val=\"40\"/>",
        "<w:i/>",
        "w:left=\"720\"",
        "<w:jc w:val=\"center\"/>",
        "w:pos=\"2880\"",
        "<w:keepNext/>",
        "<w:outlineLvl w:val=\"0\"/>",
    ] {
        assert!(xml.contains(part), "{part} in {xml}");
    }
    assert_eq!((e.revision(), formatting(&e)), (revision, before), "the document didn't change");
    assert!(e.format_style(&raw, vec![C::InsertText("x".into())], fonts()).is_err(), "only formatting commands");
    e.edit_styles("Modify Style", |s| s.set_xml(xml), fonts());
    let heading = formatting(&e).into_iter().find(|f| f.starts_with("FX-STYLES")).expect("the heading");
    assert!(heading.contains("Center") && heading.contains("20/None/true/true"), "{heading}");
}

#[test]
fn a_character_style_takes_character_formatting() {
    use crate::api::command::Command as C;
    let mut e = fixture();
    let raw = e.document().resolver.as_ref().expect("styles").styles.get("Emph").expect("Emph").raw.clone();
    let xml = e.format_style(&raw, vec![C::SetBold(true), C::SetItalic(false)], fonts()).expect("formats it");
    assert!(xml.contains("<w:b/>") && xml.contains("<w:i w:val=\"0\"/>"), "{xml}");
}
