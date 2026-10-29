//! Key bindings: a key press in its context to a command of the command set, for the Windows reference keymap and
//! its macOS layer. The catalog's shortcuts name these keys. A shortcut that sets a value takes it from the
//! selection's state: Ctrl+B on mixed or plain text sets bold on, on bold text off.

use crate::api::command::{
    Command, DeleteOptions, DeleteUnit, ElementIds, FieldsLocked, InsertField, LevelChange, MoveSelection, MoveUnit, SelectionMode, VerticalDirection,
};
use crate::api::value::{Direction, LineSpacing, Order, StyleId};

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

/// A setter whose value a key takes from the selection's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Bold,
    Italic,
    Underline,
    Subscript,
    Superscript,
    SmallCaps,
    AllCaps,
    Hidden,
    /// Ctrl+E, Ctrl+R and Ctrl+J: left again when the paragraphs already have it.
    Align(crate::api::value::Alignment),
    /// Ctrl+0: 12 pt before, or none when the first paragraph has some.
    SpaceBefore,
    TrackChanges,
    /// Alt+F9 (every field) and Shift+F9 (the fields here): codes, or results when they show their codes.
    FieldCodes {
        all: bool,
    },
}

/// What a key is bound to.
#[allow(clippy::large_enum_variant, reason = "made once per key press and consumed at once")]
#[derive(Clone, Debug, PartialEq)]
pub enum Bound {
    Run(Command),
    Toggle(Toggle),
}

fn moving(by: MoveUnit, forward: bool, extend: bool) -> Option<Bound> {
    let direction = if forward { Direction::Forward } else { Direction::Backward };
    Some(Bound::Run(Command::MoveSelection(MoveSelection { by, direction, extend: extend.then_some(true) })))
}

fn deleting(forward: bool, word: bool) -> Option<Bound> {
    let direction = if forward { Direction::Forward } else { Direction::Backward };
    Some(Bound::Run(Command::Delete(DeleteOptions { direction, unit: word.then_some(DeleteUnit::Word) })))
}

fn run(c: Command) -> Option<Bound> {
    Some(Bound::Run(c))
}

/// macOS text navigation: Cmd+Left/Right to the line's ends, Cmd+Up/Down to the document's,
/// Option+Left/Right by words, Option+Up/Down by paragraphs, Option+Backspace/Delete a word. Cmd stands for Ctrl in
/// every other shortcut (the Windows table treats Meta as Ctrl).
fn mac(code: &str, m: Mods) -> Option<Bound> {
    let cmd = m.meta && !m.alt && !m.ctrl;
    let opt = m.alt && !m.meta && !m.ctrl;
    match code {
        "ArrowLeft" | "ArrowRight" if cmd => moving(MoveUnit::LineEdge, code == "ArrowRight", m.shift),
        "ArrowUp" | "ArrowDown" if cmd => moving(MoveUnit::Document, code == "ArrowDown", m.shift),
        "ArrowLeft" | "ArrowRight" if opt => moving(MoveUnit::Word, code == "ArrowRight", m.shift),
        "ArrowUp" | "ArrowDown" if opt => moving(MoveUnit::Paragraph, code == "ArrowDown", m.shift),
        "Backspace" if opt => deleting(false, true),
        "Delete" if opt => deleting(true, true),
        _ => None,
    }
}

/// The command a key press runs, if any (DOM `KeyboardEvent.code`).
// shortcut: Ctrl+Alt+Page Up / Down (the screen's top or bottom) has no command in the command set, so it's unbound.
pub fn command(code: &str, m: Mods, cx: KeyContext) -> Option<Bound> {
    if cx.mac
        && let Some(c) = mac(code, m)
    {
        return Some(c);
    }
    let shift = m.shift;
    let (ctrl, alt) = (m.ctrl || m.meta, m.alt);
    let plain = !ctrl && !alt && !shift;
    let style = |id: &str| run(Command::SetParagraphStyle(StyleId(id.into())));
    let toggle = |t| Some(Bound::Toggle(t));
    match code {
        "ArrowLeft" if !alt => moving(if ctrl { MoveUnit::Word } else { MoveUnit::Character }, false, shift),
        "ArrowRight" if !alt => moving(if ctrl { MoveUnit::Word } else { MoveUnit::Character }, true, shift),
        "ArrowUp" if !alt => moving(if ctrl { MoveUnit::Paragraph } else { MoveUnit::Line }, false, shift),
        "ArrowDown" if !alt => moving(if ctrl { MoveUnit::Paragraph } else { MoveUnit::Line }, true, shift),
        "Home" if !alt => moving(if ctrl { MoveUnit::Document } else { MoveUnit::LineEdge }, false, shift),
        "End" if !alt => moving(if ctrl { MoveUnit::Document } else { MoveUnit::LineEdge }, true, shift),
        "Home" | "End" if alt && !ctrl && cx.in_table => moving(MoveUnit::RowEdge, code == "End", shift),
        "PageUp" | "PageDown" if alt && !ctrl && cx.in_table => moving(MoveUnit::ColumnEdge, code == "PageDown", shift),
        "PageUp" | "PageDown" if ctrl && alt => None,
        "PageUp" | "PageDown" => moving(if ctrl { MoveUnit::Page } else { MoveUnit::Screen }, code == "PageDown", shift),
        "Backspace" => deleting(false, ctrl),
        "Delete" => deleting(true, ctrl),
        "Enter" | "NumpadEnter" => run(match (ctrl, shift) {
            (true, true) if cx.in_table => Command::SplitTable,
            (true, true) => Command::InsertColumnBreak,
            (true, false) => Command::InsertPageBreak,
            (false, true) => Command::InsertLineBreak,
            (false, false) => Command::InsertParagraph,
        }),
        "Tab" if cx.float && !ctrl && !alt => run(Command::SelectNextObject(if shift { Order::Previous } else { Order::Next })),
        "Digit5" | "Numpad5" if ctrl && alt && !shift => run(Command::SelectNextObject(Order::Next)),
        "ArrowUp" | "ArrowDown" if alt && shift && !ctrl && cx.in_table => {
            run(Command::MoveRows(if code == "ArrowUp" { VerticalDirection::Up } else { VerticalDirection::Down }))
        }
        "Tab" if cx.in_table && ctrl && !alt && !shift => run(Command::InsertTab),
        "Tab" if cx.in_table && !ctrl && !alt => run(Command::MoveToCell(if shift { Order::Previous } else { Order::Next })),
        "Tab" if cx.list_start && !ctrl && !alt => run(Command::SetListLevel(if shift { LevelChange::Decrease } else { LevelChange::Increase })),
        // Shift+Tab in text inserts a tab too.
        "Tab" if !alt && !ctrl => run(Command::InsertTab),
        "Escape" if plain => run(Command::Escape),
        "KeyF" if ctrl && alt && !shift => run(Command::InsertFootnote),
        "KeyD" if ctrl && alt && !shift => run(Command::InsertEndnote),
        "KeyA" if ctrl && !alt && !shift => run(Command::SelectAll),
        "KeyB" if ctrl && !alt && !shift => toggle(Toggle::Bold),
        "KeyI" if ctrl && !alt && !shift => toggle(Toggle::Italic),
        "KeyU" if ctrl && !alt && !shift => toggle(Toggle::Underline),
        "Equal" if ctrl && !alt => toggle(if shift { Toggle::Superscript } else { Toggle::Subscript }),
        "KeyK" if ctrl && shift && !alt => toggle(Toggle::SmallCaps),
        "KeyA" if ctrl && shift && !alt => toggle(Toggle::AllCaps),
        "KeyH" if ctrl && shift && !alt => toggle(Toggle::Hidden),
        "KeyE" if ctrl && shift && !alt => toggle(Toggle::TrackChanges),
        "Period" if ctrl && shift && !alt => run(Command::GrowFont),
        "Comma" if ctrl && shift && !alt => run(Command::ShrinkFont),
        "Space" if ctrl && !shift && !alt => run(Command::ClearFormatting),
        "Digit1" if ctrl && !shift && !alt => run(Command::SetLineSpacing(LineSpacing::Multiple { lines: 1.0 })),
        "Digit2" if ctrl && !shift && !alt => run(Command::SetLineSpacing(LineSpacing::Multiple { lines: 2.0 })),
        "Digit5" if ctrl && !shift && !alt => run(Command::SetLineSpacing(LineSpacing::Multiple { lines: 1.5 })),
        "Digit0" if ctrl && !shift && !alt => toggle(Toggle::SpaceBefore),
        "Digit1" if ctrl && alt && !shift => style("Heading1"),
        "Digit2" if ctrl && alt && !shift => style("Heading2"),
        "Digit3" if ctrl && alt && !shift => style("Heading3"),
        "KeyN" if ctrl && shift && !alt => style("Normal"),
        "KeyM" if ctrl && !alt => run(if shift { Command::DecreaseIndent } else { Command::IncreaseIndent }),
        "KeyL" if ctrl && !alt && !shift => run(Command::SetAlignment(crate::api::value::Alignment::Left)),
        "KeyE" if ctrl && !alt && !shift => toggle(Toggle::Align(crate::api::value::Alignment::Center)),
        "KeyR" if ctrl && !alt && !shift => toggle(Toggle::Align(crate::api::value::Alignment::Right)),
        "KeyJ" if ctrl && !alt && !shift => toggle(Toggle::Align(crate::api::value::Alignment::Justify)),
        "KeyZ" if ctrl && !alt && !shift => run(Command::Undo),
        "KeyZ" if ctrl && !alt && shift => run(Command::Redo),
        "KeyY" if ctrl && !alt && !shift => run(Command::Redo),
        "F4" if plain => run(Command::Repeat),
        "F3" if shift && !ctrl && !alt => run(Command::ChangeCase(crate::api::command::CaseChange::Toggle)),
        "F8" if plain => run(Command::SetSelectionMode(SelectionMode::Extend)),
        "F8" if ctrl && shift && !alt => run(Command::SetSelectionMode(SelectionMode::Block)),
        "F5" if shift && !ctrl && !alt => run(Command::GoBack),
        // Field keys: Alt+F9 all codes, Shift+F9 the field here, Ctrl+F9 a new field, F9 update, Ctrl+Shift+F9 unlink,
        // Ctrl+F11 lock, Ctrl+Shift+F11 unlock.
        "F9" if alt && !ctrl && !shift => toggle(Toggle::FieldCodes { all: true }),
        "F9" if shift && !ctrl && !alt => toggle(Toggle::FieldCodes { all: false }),
        "F9" if ctrl && shift && !alt => run(Command::UnlinkFields(ElementIds::default())),
        "F9" if ctrl && !shift && !alt => run(Command::InsertField(InsertField { code: String::new(), display: None, show_code: None })),
        "F9" if plain => run(Command::UpdateFields(ElementIds::default())),
        "F11" if ctrl && !alt => run(Command::SetFieldsLocked(FieldsLocked { locked: !shift, ids: None })),
        _ => None,
    }
}

/// A key's code and modifiers from a catalog shortcut (`Ctrl+Shift+B`, `⌘B`), when it is one chord.
pub fn chord(s: &str) -> Option<(String, Mods)> {
    let mut m = Mods::default();
    let mut rest = s;
    while let Some(c) = rest.chars().next().filter(|c| "⌘⇧⌥⌃".contains(*c)) {
        rest = &rest[c.len_utf8()..];
        match c {
            '⌘' => m.meta = true,
            '⇧' => m.shift = true,
            '⌥' => m.alt = true,
            _ => m.ctrl = true,
        }
    }
    let parts: Vec<&str> = rest.split('+').collect();
    let (key, mods) = parts.split_last()?;
    for p in mods {
        match *p {
            "Ctrl" => m.ctrl = true,
            "Shift" => m.shift = true,
            "Alt" => m.alt = true,
            _ => return None,
        }
    }
    let code = match *key {
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_uppercase()) => format!("Key{k}"),
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) => format!("Digit{k}"),
        "=" => "Equal".into(),
        ">" => "Period".into(),
        "<" => "Comma".into(),
        "Space" => "Space".into(),
        "Enter" => "Enter".into(),
        "Esc" => "Escape".into(),
        "Tab" => "Tab".into(),
        k if k.starts_with('F') && k[1..].parse::<u8>().is_ok() => k.into(),
        _ => return None,
    };
    Some((code, m))
}

#[test]
fn mac_layer() {
    let mac = KeyContext { mac: true, ..KeyContext::default() };
    let cmd = Mods { meta: true, ..Mods::default() };
    let opt = Mods { alt: true, ..Mods::default() };
    assert_eq!(command("ArrowLeft", cmd, mac), moving(MoveUnit::LineEdge, false, false));
    assert_eq!(command("ArrowDown", Mods { shift: true, ..cmd }, mac), moving(MoveUnit::Document, true, true));
    assert_eq!(command("ArrowRight", opt, mac), moving(MoveUnit::Word, true, false));
    assert_eq!(command("ArrowUp", opt, mac), moving(MoveUnit::Paragraph, false, false));
    assert_eq!(command("Backspace", opt, mac), deleting(false, true));
    // Cmd for Ctrl in the shortcuts.
    assert_eq!(command("KeyB", cmd, mac), Some(Bound::Toggle(Toggle::Bold)));
    // Windows: Alt+arrows are unbound (left to the host), Ctrl+arrows by words.
    assert_eq!(command("ArrowLeft", opt, KeyContext::default()), None);
    assert_eq!(command("ArrowLeft", Mods { ctrl: true, ..Mods::default() }, KeyContext::default()), moving(MoveUnit::Word, false, false));
}

#[test]
fn catalog_shortcuts_are_the_keymap() {
    use crate::api::catalog::catalog;
    // The clipboard keys are the host's (its copy, cut and paste events), and a command's other platform's keys are
    // checked with that platform's layer.
    let host = ["Ctrl+X", "Ctrl+C", "Ctrl+V", "Ctrl+Shift+V", "⌘X", "⌘C", "⌘V", "⇧⌘V"];
    for info in catalog() {
        for (keys, cx) in [(&info.shortcuts.windows, KeyContext::default()), (&info.shortcuts.mac, KeyContext { mac: true, ..KeyContext::default() })]
        {
            for k in keys.iter().filter(|k| !host.contains(&k.as_str())) {
                let Some((code, mods)) = chord(k) else { continue };
                use crate::api::command::CommandId as C;
                let cx = KeyContext { in_table: matches!(info.id, C::SplitTable | C::MoveRows | C::MoveToCell), ..cx };
                let bound = command(&code, mods, cx);
                let id = match &bound {
                    Some(Bound::Run(c)) => Some(c.id()),
                    Some(Bound::Toggle(t)) => Some(toggle_id(*t)),
                    None => None,
                };
                assert_eq!(id, Some(info.id), "{k} for {:?}", info.id);
            }
        }
    }
}

/// The command a toggle key runs.
pub fn toggle_id(t: Toggle) -> crate::api::command::CommandId {
    use crate::api::command::CommandId as C;
    match t {
        Toggle::Bold => C::SetBold,
        Toggle::Italic => C::SetItalic,
        Toggle::Underline => C::SetUnderline,
        Toggle::Subscript => C::SetSubscript,
        Toggle::Superscript => C::SetSuperscript,
        Toggle::SmallCaps => C::SetSmallCaps,
        Toggle::AllCaps => C::SetAllCaps,
        Toggle::Hidden => C::SetHidden,
        Toggle::Align(_) => C::SetAlignment,
        Toggle::SpaceBefore => C::SetParagraphSpacing,
        Toggle::TrackChanges => C::SetTrackChanges,
        Toggle::FieldCodes { .. } => C::ShowFieldCodes,
    }
}
