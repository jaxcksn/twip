//! Raw input hosts forward: keys (by `KeyboardEvent.code`), modifiers, pointer and composition phases, and the
//! clipboard's contents. The engine recognises shortcuts and gestures from these.

use serde::{Deserialize, Serialize};

macro_rules! keys {
    ($($name:ident = $n:literal,)*) => {
        /// A physical key, named and numbered as in the C ABI's key table. The names are `KeyboardEvent.code`'s.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[repr(u32)]
        pub enum Key {
            $(#[doc = concat!("The `", stringify!($name), "` key.")] $name = $n,)*
        }

        impl Key {
            /// The key's `KeyboardEvent.code`.
            pub fn code(self) -> &'static str {
                match self {
                    $(Key::$name => stringify!($name),)*
                }
            }

            /// The key with this table number.
            pub fn from_u32(n: u32) -> Option<Key> {
                match n {
                    $($n => Some(Key::$name),)*
                    _ => None,
                }
            }

            /// Every key, by table number.
            pub const ALL: &'static [Key] = &[$(Key::$name,)*];
        }
    };
}

keys! {
    KeyA = 1, KeyB = 2, KeyC = 3, KeyD = 4, KeyE = 5, KeyF = 6, KeyG = 7, KeyH = 8, KeyI = 9, KeyJ = 10, KeyK = 11,
    KeyL = 12, KeyM = 13, KeyN = 14, KeyO = 15, KeyP = 16, KeyQ = 17, KeyR = 18, KeyS = 19, KeyT = 20, KeyU = 21,
    KeyV = 22, KeyW = 23, KeyX = 24, KeyY = 25, KeyZ = 26,
    Digit0 = 27, Digit1 = 28, Digit2 = 29, Digit3 = 30, Digit4 = 31, Digit5 = 32, Digit6 = 33, Digit7 = 34,
    Digit8 = 35, Digit9 = 36,
    F1 = 37, F2 = 38, F3 = 39, F4 = 40, F5 = 41, F6 = 42, F7 = 43, F8 = 44, F9 = 45, F10 = 46, F11 = 47, F12 = 48,
    Enter = 61, Tab = 62, Backspace = 63, Delete = 64, Escape = 65, Space = 66, Insert = 67, Home = 68, End = 69,
    PageUp = 70, PageDown = 71, ArrowLeft = 72, ArrowRight = 73, ArrowUp = 74, ArrowDown = 75, ContextMenu = 76,
    Minus = 77, Equal = 78, BracketLeft = 79, BracketRight = 80, Backslash = 81, Semicolon = 82, Quote = 83,
    Backquote = 84, Comma = 85, Period = 86, Slash = 87, IntlBackslash = 88,
    NumpadEnter = 89, NumpadAdd = 90, NumpadSubtract = 91, NumpadMultiply = 92, NumpadDivide = 93,
    NumpadDecimal = 94, Numpad0 = 95, Numpad1 = 96, Numpad2 = 97, Numpad3 = 98, Numpad4 = 99, Numpad5 = 100,
    Numpad6 = 101, Numpad7 = 102, Numpad8 = 103, Numpad9 = 104,
}

/// The modifier keys held. In the C ABI a bit set: Ctrl 1, Shift 2, Alt 4, Meta (Cmd) 8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Modifiers {
    /// Ctrl.
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt (Option on a Mac).
    pub alt: bool,
    /// Meta: Cmd on a Mac, the Windows key on Windows.
    pub meta: bool,
}

impl Modifiers {
    /// The modifiers of a C ABI bit set.
    pub fn from_bits(bits: u32) -> Modifiers {
        Modifiers { ctrl: bits & 1 != 0, shift: bits & 2 != 0, alt: bits & 4 != 0, meta: bits & 8 != 0 }
    }
}

/// A pointer event's kind. In the C ABI: down 0, move 1, up 2, cancel 3, leave 4 (the pointer left the view).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum PointerKind {
    /// A button went down.
    Down = 0,
    /// The pointer moved.
    Move = 1,
    /// A button came up.
    Up = 2,
    /// The platform cancelled the pointer (a lost capture, say).
    Cancel = 3,
    /// The pointer left the view.
    Leave = 4,
}

/// The button a pointer event pressed or released. In the C ABI: primary 0, secondary 1, middle 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum PointerButton {
    /// The main button (left, by default).
    #[default]
    Primary = 0,
    /// The context-menu button (right, by default).
    Secondary = 1,
    /// The middle button or wheel.
    Middle = 2,
}

/// An IME composition's phase. In the C ABI: start 0, update 1, end 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum CompositionPhase {
    /// A composition began.
    Start = 0,
    /// The composition's text changed.
    Update = 1,
    /// The composition ended, committed or cancelled.
    End = 2,
}

/// A point in the editor's viewport, CSS px from its top-left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    /// CSS px from the left edge.
    pub x: f64,
    /// CSS px from the top edge.
    pub y: f64,
}

/// What a copy puts on the clipboard, and what a paste takes from it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipboardData {
    /// The plain text (`text/plain`).
    pub text: String,
    /// The HTML (`text/html`), when there is any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// A token naming the engine's internal fragment: a paste in any session of the engine keeps its formatting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal: Option<String>,
}
