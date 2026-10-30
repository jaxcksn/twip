//! A header or footer made by entering it (the page had none) goes away when it's left unchanged, by any way out: the
//! document, its history and its saved package are as they were. A change keeps it.

use super::super::Editor;
use super::super::commands::Command;
use super::super::input::PointerKind;
use super::super::keymap::Mods;
use super::super::position::StoryId;
use super::{fonts, open};

fn fixture() -> Editor {
    let mut e = open("FX-TEXT.docx");
    e.complete(fonts());
    e
}

/// The parts a save writes.
fn parts(e: &Editor) -> Vec<String> {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    let z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a package");
    let mut names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    names.sort();
    names
}

/// What leaving must give back: the saved parts, the changed state and both history depths.
fn state(e: &Editor) -> (Vec<String>, bool, usize, usize) {
    (parts(e), e.is_dirty(), e.history().undo_depth(), e.history().redo_depth())
}

fn headers_and_footers(e: &Editor) -> usize {
    e.document().stories.keys().filter(|k| k.contains("header") || k.contains("footer")).count()
}

fn editing_header_footer(e: &Editor) -> bool {
    matches!(e.selection().focus().map(|p| p.story), Some(StoryId::HeaderFooter(_)))
}

/// A double-click on the first body line.
fn double_click_body(e: &mut Editor) {
    let (pg, g) = e.page_geometries().into_iter().next().expect("a page");
    let l = g.lines.iter().find(|l| g.stories[l.story].story == StoryId::Body && l.stops.len() > 2).expect("a body line");
    let (x, y) = e.stack().to_doc(pg, l.stops[1].x + 1.0, (l.top + l.bottom) / 2.0).expect("on the stack");
    for clicks in [1, 2] {
        e.pointer_input(PointerKind::Down, x, y, clicks, Mods::default(), fonts());
        e.pointer_input(PointerKind::Up, x, y, clicks, Mods::default(), fonts());
    }
}

#[test]
fn provisional_header_left_unchanged_is_removed() {
    let ways: [(&str, fn(&mut Editor)); 4] = [
        ("Esc", |e| {
            e.execute(Command::Escape, fonts());
        }),
        ("Close Header and Footer", |e| {
            e.execute(Command::CloseHeaderFooter, fonts());
        }),
        ("a double-click on the body", double_click_body),
        ("Edit Footer, then Esc", |e| {
            e.execute(Command::EditFooter, fonts());
            assert!(editing_header_footer(e), "in the footer");
            assert_eq!(headers_and_footers(e), 1, "the empty header went away for the footer");
            e.execute(Command::Escape, fonts());
        }),
    ];
    for (way, leave) in ways {
        let mut e = fixture();
        let before = state(&e);
        assert_eq!(headers_and_footers(&e), 0, "FX-TEXT has no header");
        e.execute(Command::EditHeader, fonts());
        assert!(editing_header_footer(&e), "{way}: in the header");
        assert_eq!(headers_and_footers(&e), 1, "{way}: a header made");
        leave(&mut e);
        assert!(!editing_header_footer(&e), "{way}: back in the body");
        assert_eq!(headers_and_footers(&e), 0, "{way}: the header is gone");
        assert_eq!(state(&e), before, "{way}: the document, its history and its package are as they were");
    }
}

#[test]
fn provisional_double_click_keeps_the_clicked_place() {
    let mut e = fixture();
    e.execute(Command::EditHeader, fonts());
    double_click_body(&mut e);
    // Before the header the caret was at the document's start; the double-click put it past the line's first stop.
    let at = e.selection().focus().and_then(|p| p.para_offset()).expect("a caret");
    assert!(at.1.0 > 0, "the caret is where the body was clicked, not where it was before the header");
}

#[test]
fn provisional_header_typed_stays() {
    let mut e = fixture();
    e.execute(Command::EditHeader, fonts());
    e.execute(Command::InsertText("x".into()), fonts());
    e.execute(Command::Escape, fonts());
    assert_eq!(headers_and_footers(&e), 1, "the header stays");
    assert!(e.is_dirty());
    e.execute(Command::Undo, fonts());
    assert_eq!(headers_and_footers(&e), 1, "the first undo takes the typing");
    e.execute(Command::Undo, fonts());
    assert_eq!(headers_and_footers(&e), 0, "the second the header");
}

#[test]
fn provisional_header_type_then_delete_stays() {
    let mut e = fixture();
    e.execute(Command::EditHeader, fonts());
    e.execute(Command::InsertText("x".into()), fonts());
    e.execute(super::del(super::super::commands::Dir::Back, false), fonts());
    e.execute(Command::Escape, fonts());
    assert_eq!(headers_and_footers(&e), 1, "an edited header stays, even empty again");
}

#[test]
fn provisional_footer_left_unchanged_is_removed() {
    let mut e = fixture();
    let before = state(&e);
    e.execute(Command::EditFooter, fonts());
    assert_eq!(headers_and_footers(&e), 1);
    e.execute(Command::Escape, fonts());
    assert_eq!(state(&e), before);
}

#[test]
fn provisional_undo_in_the_new_header_removes_it_once() {
    let mut e = fixture();
    let before = state(&e);
    e.execute(Command::EditHeader, fonts());
    e.execute(Command::Undo, fonts());
    assert!(!editing_header_footer(&e));
    assert_eq!(headers_and_footers(&e), 0);
    let (p, dirty, undo, _) = state(&e);
    assert_eq!((p, dirty, undo), (before.0, before.1, before.2), "nothing more is undone");
}

#[test]
fn provisional_existing_header_untouched() {
    let mut e = open("FX-STORIES.docx");
    e.complete(fonts());
    let before = state(&e);
    let n = headers_and_footers(&e);
    e.execute(Command::EditHeader, fonts());
    e.execute(Command::Escape, fonts());
    assert_eq!(headers_and_footers(&e), n);
    assert_eq!(state(&e), before);
}
