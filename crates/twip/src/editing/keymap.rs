//! Key bindings: (key, modifiers, context) to commands, for the Windows reference keymap.
//! The bindings follow the common word-processor keys.

use super::commands::{Command, Dir, MoveBy};
use super::format::{Format, Toggle};
use crate::docx::props::Jc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Where the caret is, for bindings that depend on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyContext {
    pub in_table: bool,
    /// A collapsed caret at a list item's start.
    pub list_start: bool,
    /// A floating object selected (Tab and Shift+Tab go through the floats).
    pub float: bool,
    /// The macOS layer: Cmd for Ctrl, and the platform's text navigation keys.
    pub mac: bool,
}

/// macOS text navigation: Cmd+Left/Right to the line's ends, Cmd+Up/Down to the document's,
/// Option+Left/Right by words, Option+Up/Down by paragraphs, Option+Backspace/Delete a word. Cmd stands for Ctrl in
/// every other shortcut (the Windows table treats Meta as Ctrl).
fn mac(code: &str, m: Mods) -> Option<Command> {
    let mv = |by, dir| Some(Command::Move { by, dir, extend: m.shift });
    let dir = |back: bool| if back { Dir::Back } else { Dir::Forward };
    let cmd = m.meta && !m.alt && !m.ctrl;
    let opt = m.alt && !m.meta && !m.ctrl;
    match code {
        "ArrowLeft" | "ArrowRight" if cmd => mv(MoveBy::LineEdge, dir(code == "ArrowLeft")),
        "ArrowUp" | "ArrowDown" if cmd => mv(MoveBy::Story, dir(code == "ArrowUp")),
        "ArrowLeft" | "ArrowRight" if opt => mv(MoveBy::Word, dir(code == "ArrowLeft")),
        "ArrowUp" | "ArrowDown" if opt => mv(MoveBy::Para, dir(code == "ArrowUp")),
        "Backspace" if opt => Some(Command::Delete { dir: Dir::Back, word: true }),
        "Delete" if opt => Some(Command::Delete { dir: Dir::Forward, word: true }),
        _ => None,
    }
}

/// The command a key press runs, if any (DOM `KeyboardEvent.code`).
pub fn command(code: &str, m: Mods, cx: KeyContext) -> Option<Command> {
    if cx.mac
        && let Some(c) = mac(code, m)
    {
        return Some(c);
    }
    let shift = m.shift;
    let mv = |by, dir| Some(Command::Move { by, dir, extend: shift });
    let (ctrl, alt) = (m.ctrl || m.meta, m.alt);
    match code {
        "ArrowLeft" if !alt => mv(if ctrl { MoveBy::Word } else { MoveBy::Char }, Dir::Back),
        "ArrowRight" if !alt => mv(if ctrl { MoveBy::Word } else { MoveBy::Char }, Dir::Forward),
        "ArrowUp" if !alt => mv(if ctrl { MoveBy::Para } else { MoveBy::Line }, Dir::Back),
        "ArrowDown" if !alt => mv(if ctrl { MoveBy::Para } else { MoveBy::Line }, Dir::Forward),
        "Home" if !alt => mv(if ctrl { MoveBy::Story } else { MoveBy::LineEdge }, Dir::Back),
        "End" if !alt => mv(if ctrl { MoveBy::Story } else { MoveBy::LineEdge }, Dir::Forward),
        "Home" | "End" if alt && shift && !ctrl && cx.in_table => Some(Command::SelectToEdge { row: true, end: code == "End" }),
        "PageUp" | "PageDown" if alt && shift && !ctrl && cx.in_table => Some(Command::SelectToEdge { row: false, end: code == "PageDown" }),
        "Home" | "End" if alt && !ctrl && cx.in_table => mv(MoveBy::RowEdge, if code == "End" { Dir::Forward } else { Dir::Back }),
        "PageUp" | "PageDown" if alt && !ctrl && cx.in_table => mv(MoveBy::ColumnEdge, if code == "PageDown" { Dir::Forward } else { Dir::Back }),
        "PageUp" => mv(
            if ctrl && alt {
                MoveBy::ScreenEdge
            } else if ctrl {
                MoveBy::Page
            } else {
                MoveBy::Screen
            },
            Dir::Back,
        ),
        "PageDown" => mv(
            if ctrl && alt {
                MoveBy::ScreenEdge
            } else if ctrl {
                MoveBy::Page
            } else {
                MoveBy::Screen
            },
            Dir::Forward,
        ),
        "Backspace" => Some(Command::Delete { dir: Dir::Back, word: ctrl }),
        "Delete" => Some(Command::Delete { dir: Dir::Forward, word: ctrl }),
        "Enter" | "NumpadEnter" => Some(match (ctrl, shift) {
            (true, true) if cx.in_table => Command::SplitTable,
            (true, true) => Command::ColumnBreak,
            (true, false) => Command::PageBreak,
            (false, true) => Command::LineBreak,
            (false, false) => Command::Enter,
        }),
        "Tab" if cx.float && !ctrl && !alt => Some(Command::NextObject { back: shift }),
        "Digit5" | "Numpad5" if ctrl && alt && !shift => Some(Command::ObjectFocus),
        "ArrowUp" | "ArrowDown" if alt && shift && !ctrl && cx.in_table => Some(Command::MoveRows { up: code == "ArrowUp" }),
        "Tab" if cx.in_table && ctrl && !alt && !shift => Some(Command::InsertTab),
        "Tab" if cx.in_table && !ctrl && !alt => Some(Command::NextCell { back: shift }),
        "Tab" if cx.list_start && !ctrl && !alt => Some(Command::ListLevel { back: shift }),
        // Shift+Tab in text inserts a tab too.
        "Tab" if !alt && !ctrl => Some(Command::InsertTab),
        "Escape" if !ctrl && !alt && !shift => Some(Command::Escape),
        // Ctrl+Alt+F and Ctrl+Alt+D: Insert Footnote and Insert Endnote.
        "KeyF" if ctrl && alt && !shift => Some(Command::InsertNote(crate::docx::NoteKind::Foot)),
        "KeyD" if ctrl && alt && !shift => Some(Command::InsertNote(crate::docx::NoteKind::End)),
        "KeyA" if ctrl && !alt => Some(Command::SelectAll),
        "KeyB" if ctrl && !alt && !shift => Some(Command::Toggle(Toggle::Bold)),
        "KeyI" if ctrl && !alt && !shift => Some(Command::Toggle(Toggle::Italic)),
        "KeyU" if ctrl && !alt && !shift => Some(Command::Toggle(Toggle::Underline)),
        "Equal" if ctrl && !alt => Some(Command::Toggle(if shift { Toggle::Superscript } else { Toggle::Subscript })),
        // Ctrl+Shift+> and Ctrl+Shift+<, Ctrl+Space, Ctrl+1/2/5, Ctrl+0, Ctrl+M and Ctrl+Shift+M.
        "Period" if ctrl && shift && !alt => Some(Command::Format(Format::GrowFont(true))),
        "Comma" if ctrl && shift && !alt => Some(Command::Format(Format::GrowFont(false))),
        "Space" if ctrl && !shift && !alt => Some(Command::Format(Format::ClearFormatting)),
        "Digit1" if ctrl && !shift && !alt => Some(Command::Format(Format::LineSpacing(1.0))),
        "Digit2" if ctrl && !shift && !alt => Some(Command::Format(Format::LineSpacing(2.0))),
        "Digit5" if ctrl && !shift && !alt => Some(Command::Format(Format::LineSpacing(1.5))),
        "Digit0" if ctrl && !shift && !alt => Some(Command::Format(Format::SpaceBefore)),
        "KeyM" if ctrl && !alt => Some(Command::Format(Format::Indent(!shift))),
        "KeyL" if ctrl && !alt && !shift => Some(Command::Align(Jc::Left)),
        "KeyE" if ctrl && !alt && !shift => Some(Command::Align(Jc::Center)),
        "KeyR" if ctrl && !alt && !shift => Some(Command::Align(Jc::Right)),
        "KeyJ" if ctrl && !alt && !shift => Some(Command::Align(Jc::Both)),
        "KeyZ" if ctrl && !alt => Some(Command::Undo),
        "KeyY" if ctrl && !alt => Some(Command::Repeat),
        "F4" if !ctrl && !alt && !shift => Some(Command::Repeat),
        "F8" if !ctrl && !alt && !shift => Some(Command::ExtendMode),
        "F5" if shift && !ctrl && !alt => Some(Command::GoBack),
        // Field keys: Alt+F9 all codes, Shift+F9 the field here, Ctrl+F9 a new
        // field, F9 update, Ctrl+Shift+F9 unlink, Ctrl+F11 lock, Ctrl+Shift+F11 unlock.
        "F9" if alt && !ctrl && !shift => Some(merge("view.field-codes", serde_json::json!({ "toggle": "all" }))),
        "F9" if shift && !ctrl && !alt => Some(merge("view.field-codes", serde_json::json!({ "toggle": "here" }))),
        "F9" if ctrl && shift && !alt => Some(merge("field.unlink", serde_json::Value::Null)),
        "F9" if ctrl && !shift && !alt => Some(merge("field.new", serde_json::Value::Null)),
        "F9" if !ctrl && !shift && !alt => Some(merge("field.update", serde_json::Value::Null)),
        "F11" if ctrl && !alt => Some(merge(if shift { "field.unlock" } else { "field.lock" }, serde_json::Value::Null)),
        "F8" if ctrl && shift && !alt => Some(Command::BlockSelect),
        _ => None,
    }
}

#[test]
fn mac_layer() {
    let mac = KeyContext { mac: true, ..KeyContext::default() };
    let cmd = Mods { meta: true, ..Mods::default() };
    let opt = Mods { alt: true, ..Mods::default() };
    let mv = |by, dir, extend| Some(Command::Move { by, dir, extend });
    assert_eq!(command("ArrowLeft", cmd, mac), mv(MoveBy::LineEdge, Dir::Back, false));
    assert_eq!(command("ArrowDown", Mods { shift: true, ..cmd }, mac), mv(MoveBy::Story, Dir::Forward, true));
    assert_eq!(command("ArrowRight", opt, mac), mv(MoveBy::Word, Dir::Forward, false));
    assert_eq!(command("ArrowUp", opt, mac), mv(MoveBy::Para, Dir::Back, false));
    assert_eq!(command("Backspace", opt, mac), Some(Command::Delete { dir: Dir::Back, word: true }));
    // Cmd for Ctrl in the shortcuts.
    assert_eq!(command("KeyB", cmd, mac), Some(Command::Toggle(Toggle::Bold)));
    // Windows: Alt+arrows are unbound (left to the host), Ctrl+arrows by words.
    assert_eq!(command("ArrowLeft", opt, KeyContext::default()), None);
    assert_eq!(command("ArrowLeft", Mods { ctrl: true, ..Mods::default() }, KeyContext::default()), mv(MoveBy::Word, Dir::Back, false));
}

/// A merge command bound to a key.
fn merge(id: &str, arg: serde_json::Value) -> Command {
    Command::Merge { id: id.to_string(), arg }
}
