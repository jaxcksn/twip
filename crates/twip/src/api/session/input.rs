//! Keyboard, text, IME and pointer input, and the clipboard. Hosts forward raw input; the engine recognises
//! shortcuts and gestures itself.

use std::collections::BTreeMap;

use super::Session;
use crate::api::command::{Command, PasteOptions, Zoom};
use crate::api::engine::SessionId;
use crate::api::error::Error;
use crate::api::event::{Event, FieldRequest, HostCommand, KeyOutcome, LinkVia};
use crate::api::input::{ClipboardData, CompositionPhase, Key, Modifiers, Point, PointerButton, PointerKind};
use crate::api::record::Origin;
use crate::api::services::{
    ClientPoint, ContextMenuEvent, DialogKind, GestureAction, GestureBinding, GestureEvent, GestureId, GestureTarget, KeyBinding, MenuVia, Shortcut,
    ShortcutKeys,
};
use crate::editing::commands::Outcome;
use crate::editing::gestures::Gesture;
use crate::editing::input::Notice;
use crate::editing::keymap::{self, Bound, KeyContext, Mods};

/// Keys whose UI is the application's: key, Ctrl, Shift, Alt, and the host command it emits.
const HOST_KEYS: [(Key, bool, bool, bool, HostCommand); 13] = [
    (Key::KeyS, true, false, false, HostCommand::Save),
    (Key::F12, false, false, false, HostCommand::SaveAs),
    (Key::KeyP, true, false, false, HostCommand::Print),
    (Key::KeyO, true, false, false, HostCommand::Open),
    (Key::KeyN, true, false, false, HostCommand::New),
    (Key::KeyF, true, false, false, HostCommand::Find),
    (Key::KeyH, true, false, false, HostCommand::Replace),
    (Key::KeyG, true, false, false, HostCommand::GoTo),
    (Key::KeyK, true, false, false, HostCommand::InsertHyperlink),
    (Key::F7, false, false, false, HostCommand::NextMisspelling),
    (Key::KeyF, true, true, false, HostCommand::FocusFont),
    (Key::KeyP, true, true, false, HostCommand::FocusFontSize),
    (Key::F1, false, false, false, HostCommand::Help),
];

/// Keys that ask for a dialog: key, Ctrl, Shift, Alt, and the dialog.
const DIALOG_KEYS: [(Key, bool, bool, bool, DialogKind); 1] = [(Key::KeyD, true, false, false, DialogKind::Font)];

/// The context-menu key and Shift+F10.
const MENU_KEYS: [Chord; 2] = [(Key::ContextMenu, false, false, false), (Key::F10, false, true, false)];

/// Every gesture, with its label for a settings screen.
const GESTURES: [(GestureId, &str); 20] = [
    (GestureId::HeaderAreaDoubleClick, "Double-click the header area"),
    (GestureId::FooterAreaDoubleClick, "Double-click the footer area"),
    (GestureId::BodyDoubleClickInHeaderFooter, "Double-click the body while editing a header or footer"),
    (GestureId::TextDoubleClick, "Double-click text"),
    (GestureId::TextTripleClick, "Triple-click text"),
    (GestureId::TextCtrlClick, "Ctrl+click text"),
    (GestureId::LinkActivate, "Ctrl+click a link"),
    (GestureId::MarginClick, "Click the left margin"),
    (GestureId::MarginDoubleClick, "Double-click the left margin"),
    (GestureId::TableBorderDoubleClick, "Double-click a table's column border"),
    (GestureId::TableBorderDrag, "Drag a table border"),
    (GestureId::ObjectDrag, "Drag a picture or text box"),
    (GestureId::TextDrag, "Drag selected text"),
    (GestureId::ElementDoubleClick, "Double-click an element in design mode"),
    (GestureId::PlaceholderClick, "Click a picture placeholder"),
    (GestureId::RulerClick, "Click the ruler"),
    (GestureId::RulerDrag, "Drag a ruler marker"),
    (GestureId::RulerDragOff, "Drag a tab stop off the ruler"),
    (GestureId::RulerDoubleClick, "Double-click the ruler"),
    (GestureId::ContextClick, "Right-click"),
];

/// A key with Ctrl (or Cmd), Shift and Alt.
type Chord = (Key, bool, bool, bool);

/// What the session's input does besides the editor's defaults: rebound gestures and the application's shortcuts.
/// Engine state, so a change applies to the next input.
#[derive(Default)]
pub(super) struct Bindings {
    /// The gestures not bound to their default.
    gestures: BTreeMap<GestureId, GestureAction>,
    /// The application's shortcuts, with the keys each claims.
    shortcuts: Vec<(String, Vec<Chord>)>,
}

impl Bindings {
    /// The shortcut `chord` is, other than `except`.
    fn shortcut(&self, chord: Chord, except: Option<&str>) -> Option<&str> {
        self.shortcuts.iter().find(|(id, keys)| Some(id.as_str()) != except && keys.contains(&chord)).map(|(id, _)| id.as_str())
    }

    /// The gesture's binding when it isn't the default.
    fn rebound(&self, g: Option<Gesture>) -> Option<Gesture> {
        g.filter(|g| self.gestures.contains_key(&g.id))
    }
}

/// The input modifiers as the editor's keymap takes them.
fn internal(m: Modifiers) -> Mods {
    Mods { ctrl: m.ctrl, shift: m.shift, alt: m.alt, meta: m.meta }
}

/// The clipboard token of fragment `id` copied in session `s`.
fn token(s: SessionId, id: u64) -> String {
    format!("twip:{}:{id}", s.0)
}

fn client(at: Point) -> ClientPoint {
    ClientPoint { client_x: at.x, client_y: at.y }
}

/// A key as `Ctrl+Alt+K` or `⌘K` names it, Cmd standing for Ctrl.
fn parse_chord(keys: &str) -> Option<Chord> {
    let (code, m) = keymap::chord(keys)?;
    let key = Key::ALL.iter().copied().find(|k| k.code() == code)?;
    Some((key, m.ctrl || m.meta, m.shift, m.alt))
}

impl Session {
    /// A key press. Application shortcuts emit `shortcut`, keys the application owns `hostCommand`, dialog keys
    /// `dialogRequested`; keys bound to a command run it.
    pub fn key(&mut self, key: Key, mods: Modifiers) -> KeyOutcome {
        // Cmd on macOS stands where Ctrl does elsewhere.
        let ctrl = mods.ctrl || mods.meta;
        let chord = (key, ctrl, mods.shift, mods.alt);
        if let Some(id) = self.bindings.shortcut(chord, None).map(str::to_string) {
            self.emit(Event::Shortcut { id: id.clone() });
            return KeyOutcome::Shortcut(id);
        }
        if let Some(&(.., command)) = HOST_KEYS.iter().find(|k| (k.0, k.1, k.2, k.3) == chord) {
            self.emit(Event::HostCommand { command });
            return KeyOutcome::HostCommand(command);
        }
        // The context-menu key and Shift+F10 open the menu at the caret.
        if MENU_KEYS.contains(&chord) {
            let at = self.host().caret_client_point();
            self.open_context_menu(at, MenuVia::Keyboard);
            return KeyOutcome::Handled;
        }
        if let Some(&(.., kind)) = DIALOG_KEYS.iter().find(|k| (k.0, k.1, k.2, k.3) == chord) {
            self.emit(Event::DialogRequested { kind });
            return KeyOutcome::Handled;
        }
        let Some(command) = self.editor.key_command(key.code(), internal(mods)) else { return KeyOutcome::Unhandled };
        // A bound key is the editor's even when it can't run here: the platform's default never applies.
        let _ = self.input(command);
        // A copy asks the host to fill the clipboard without changing anything.
        self.deliver_requests();
        KeyOutcome::Handled
    }

    /// Emits `contextMenu` with the menu for the selection, to open at `at`.
    pub(super) fn open_context_menu(&mut self, at: ClientPoint, via: MenuVia) {
        let menu = self.context_menu();
        self.emit(Event::ContextMenu(ContextMenuEvent { menu, at, via }));
    }

    /// Typed text (after composition).
    pub fn text(&mut self, text: &str) {
        if !text.is_empty() {
            let _ = self.input(Command::InsertText(text.to_string()));
        }
    }

    /// A command from input: checked as the API's are, and recorded as the user's.
    fn input(&mut self, command: Command) -> Result<super::Commit, crate::api::error::Error> {
        self.check(&command)?;
        if let Some(done) = self.run_view(&command) {
            return done;
        }
        self.apply(command, Origin::User)
    }

    /// The editor's notices since the last change, as events.
    pub(super) fn notices(&mut self) -> Vec<Event> {
        let id = crate::editing::api_id;
        self.editor
            .take_notices()
            .into_iter()
            .map(|n| match n {
                Notice::Activate(e) => Event::ElementActivated { id: id(e) },
                Notice::FieldUpdate { token, fields } => {
                    Event::FieldsUpdateRequested { token, fields: fields.into_iter().map(|(e, code)| FieldRequest { id: id(e), code }).collect() }
                }
                Notice::Link { address, ctrl } => Event::LinkActivated { address, via: if ctrl { LinkVia::CtrlClick } else { LinkVia::Click } },
                Notice::Placeholder(key) => Event::PlaceholderActivated(crate::api::services::PlaceholderInfo { key, element: None }),
            })
            .collect()
    }

    /// An IME composition step: `text` is the composition so far, with the cursor at `cursor` characters. The end
    /// commits its text (none: cancelled), as typed text.
    // shortcut: the interim text doesn't show until the end; showing it needs the composition drawn at the caret
    // without editing the document (or an edit kept out of the history and the records).
    pub fn composition(&mut self, phase: CompositionPhase, text: &str, cursor: u32) {
        let _ = cursor;
        if phase == CompositionPhase::End {
            self.text(text);
        }
    }

    /// Pointer input at a point in the viewport, with the button pressed or released and the click count. The
    /// engine recognises the gestures (a secondary press is `contextClick`) and runs each one's binding.
    pub fn pointer(&mut self, kind: PointerKind, at: Point, button: PointerButton, clicks: u8, mods: Modifiers) {
        if self.ruler_pointer(kind, at, button, clicks, mods) {
            return;
        }
        let (x, y) = self.presentation.to_document(at);
        let mods = internal(mods);
        match (kind, button) {
            (PointerKind::Down, PointerButton::Secondary) => self.context_click(at, x, y),
            // The middle button, and the secondary one's release, do nothing.
            (PointerKind::Down | PointerKind::Up, PointerButton::Secondary | PointerButton::Middle) => {}
            (PointerKind::Leave, _) => self.editor.pointer_leave(),
            (PointerKind::Down, PointerButton::Primary) => {
                let pressed = self.editor.press_gesture(x, y, clicks, mods, &self.fonts.fonts());
                match self.bindings.rebound(pressed) {
                    Some(g) => self.run_binding(g, at),
                    None => self.drive(kind, x, y, clicks, mods),
                }
            }
            (PointerKind::Up, PointerButton::Primary) => match self.bindings.rebound(self.editor.release_gesture(x, y)) {
                // A rebound drag ends with nothing moved.
                Some(g) => {
                    self.drive(PointerKind::Cancel, x, y, clicks, mods);
                    self.run_binding(g, at);
                }
                None => self.drive(kind, x, y, clicks, mods),
            },
            (PointerKind::Move | PointerKind::Cancel, _) => self.drive(kind, x, y, clicks, mods),
        }
        // Hovering changes the cursor and tooltip with no other change.
        self.deliver_requests();
    }

    /// The rulers' gestures: true when the pointer is on a ruler, or dragging from one, and the input was theirs. A
    /// drag shows a guide line and commits nothing until its release.
    // shortcut: the dragged marker stays where it was until the release (the guide line shows where it goes).
    fn ruler_pointer(&mut self, kind: PointerKind, at: Point, button: PointerButton, clicks: u8, mods: Modifiers) -> bool {
        use crate::services::ruler::{Hit, hit};
        if let Some(mut d) = self.presentation.ruler_drag.take() {
            match kind {
                PointerKind::Move => {
                    d.moved |= (at.x - d.start.x).abs().max((at.y - d.start.y).abs()) > 2.0;
                    (d.now, d.free) = (at, mods.alt);
                    let guide = d.moved;
                    self.presentation.ruler_drag = Some(d);
                    // The guide line moved: a frame shows it.
                    if guide {
                        self.changed(Vec::new());
                    }
                }
                PointerKind::Up => {
                    (d.now, d.free) = (at, mods.alt);
                    self.request(super::HostRequest::Release);
                    self.ruler_release(&d, at);
                }
                _ => self.request(super::HostRequest::Release),
            }
            return true;
        }
        let view = self.ruler_view();
        let Some(on) = hit(&view, self.presentation.rulers, at.x, at.y) else { return false };
        if (kind, button) != (PointerKind::Down, PointerButton::Primary) {
            return true;
        }
        if clicks >= 2 && on != Hit::Selector {
            self.ruler_gesture(GestureId::RulerDoubleClick, view.page, at, |s| s.emit(Event::DialogRequested { kind: DialogKind::Tabs }));
            return true;
        }
        let marker = match on {
            Hit::Selector => {
                use crate::api::services::TabSelector as T;
                const CYCLE: [T; 7] = [T::Left, T::Center, T::Right, T::Decimal, T::Bar, T::FirstLineIndent, T::HangingIndent];
                let k = CYCLE.iter().position(|&t| t == self.presentation.tab_selector).map_or(0, |k| (k + 1) % CYCLE.len());
                self.host().set_tab_selector(CYCLE[k]);
                return true;
            }
            Hit::Ruler => return true,
            Hit::Marker(m) => Some(m),
            Hit::Text(_) => None,
        };
        let drag = super::host::RulerDrag { marker, view, start: at, now: at, moved: false, free: mods.alt };
        self.presentation.ruler_drag = Some(drag);
        self.request(super::HostRequest::Capture);
        true
    }

    /// A ruler press released: a click on the text area adds what the tab selector says (`rulerClick`); a moved
    /// marker runs its command (`rulerDrag`), or a tab stop dragged off the ruler is deleted (`rulerDragOff`).
    fn ruler_release(&mut self, d: &super::host::RulerDrag, at: Point) {
        use crate::api::command::{AddTabStop, BoundaryMove, ColumnBoundaryMove, Indents, Margins, MoveTabStop};
        use crate::api::services::TabSelector as T;
        use crate::api::value::{Points, TabAlignment};
        use crate::services::ruler::{BAND, Marker as M};
        let (h, v) = (&d.view.model.horizontal, &d.view.model.vertical);
        let (start, end) = (h.text.start.0, h.text.end.0);
        let (left, first) = (h.indents.left.0, h.indents.first_line.0);
        let value = d.value();
        // The first line where it is, relative to a new left indent.
        let indents = |left: Option<f64>, first: f64| Indents {
            left: left.map(Points),
            first_line: (first >= 0.0).then_some(Points(first)),
            hanging: (first < 0.0).then_some(Points(-first)),
            ..Indents::default()
        };
        let (id, command) = match d.marker {
            None if d.moved => return,
            None => {
                let at = (value - start).max(0.0);
                let tab = |a| Command::AddTabStop(AddTabStop { position: Points(at), alignment: Some(a), leader: None });
                let command = match self.presentation.tab_selector {
                    T::Left => tab(TabAlignment::Left),
                    T::Center => tab(TabAlignment::Center),
                    T::Right => tab(TabAlignment::Right),
                    T::Decimal => tab(TabAlignment::Decimal),
                    T::Bar => tab(TabAlignment::Bar),
                    T::FirstLineIndent => Command::SetIndents(indents(None, at - left)),
                    T::HangingIndent => Command::SetIndents(indents(Some(at), left + first - at)),
                };
                (GestureId::RulerClick, command)
            }
            Some(_) if !d.moved => return,
            Some(M::Tab(pos)) if !(-BAND..2.0 * BAND).contains(&d.now.y) => (GestureId::RulerDragOff, Command::DeleteTabStop(Points(pos))),
            Some(m) => {
                let margins = &self.state.page.margins;
                let command = match m {
                    M::Tab(pos) => Command::MoveTabStop(MoveTabStop { from: Points(pos), to: Points((value - start).max(0.0)) }),
                    M::FirstLine => Command::SetIndents(indents(None, value - start - left)),
                    M::Hanging => Command::SetIndents(indents(Some(value - start), start + left + first - value)),
                    M::Left => Command::SetIndents(Indents { left: Some(Points(value - start)), ..Indents::default() }),
                    M::Right => Command::SetIndents(Indents { right: Some(Points(end - value)), ..Indents::default() }),
                    M::TextStart => Command::SetMargins(Margins { left: Some(Points(margins.left.0 + value - start)), ..Margins::default() }),
                    M::TextEnd => Command::SetMargins(Margins { right: Some(Points(margins.right.0 + end - value)), ..Margins::default() }),
                    M::Top => Command::SetMargins(Margins { top: Some(Points(value)), ..Margins::default() }),
                    M::Bottom => Command::SetMargins(Margins { bottom: Some(Points(v.page_height.0 - value)), ..Margins::default() }),
                    M::Column(column) => Command::MoveColumnBoundary(ColumnBoundaryMove { column, to: Points(value) }),
                    M::TableColumn(index) => Command::MoveTableColumnBoundary(BoundaryMove { index, to: Points(value) }),
                    M::TableRow(index) => Command::MoveTableRowBoundary(BoundaryMove { index, to: Points(value) }),
                };
                (GestureId::RulerDrag, command)
            }
        };
        self.ruler_gesture(id, d.view.page, at, |s| {
            let _ = s.input(command);
        });
    }

    /// A ruler gesture: its default, unless the application bound it to something else.
    fn ruler_gesture(&mut self, id: GestureId, page: u32, at: Point, default: impl FnOnce(&mut Session)) {
        if self.bindings.gestures.contains_key(&id) {
            let target = GestureTarget::Area { kind: crate::api::services::AreaKind::Ruler };
            self.run_binding(Gesture { id, page, target }, at);
        } else {
            default(self);
        }
    }

    /// Pointer input to the editor's own gestures, recorded as the user's.
    // shortcut: the editor's hit tolerances and auto-scroll assume 100% zoom; hand it the presentation's viewport
    // when hits at other zooms need to be exact to the pixel.
    fn drive(&mut self, kind: PointerKind, x: f64, y: f64, clicks: u8, mods: Mods) {
        use crate::editing::input::PointerKind as K;
        let kind = match kind {
            PointerKind::Down => K::Down,
            PointerKind::Move => K::Move,
            PointerKind::Up => K::Up,
            PointerKind::Cancel | PointerKind::Leave => K::Cancel,
        };
        let _ = self.apply_with(Origin::User, |e, fonts| {
            e.pointer_input(kind, x, y, clicks, mods, fonts);
            Outcome::Unchanged("pointer input")
        });
    }

    /// A secondary press. By default the selection goes under the pointer, unless the press is inside it, and
    /// `contextMenu` opens there.
    fn context_click(&mut self, at: Point, x: f64, y: f64) {
        if !self.bindings.gestures.contains_key(&GestureId::ContextClick) {
            let _ = self.apply_with(Origin::User, |e, fonts| {
                e.context_press(x, y, fonts);
                Outcome::Unchanged("context press")
            });
            self.open_context_menu(client(at), MenuVia::Pointer);
            return;
        }
        let page = self.editor.stack().page_at(x, y).map_or(self.state.pages.current, |(p, ..)| p as u32 + 1);
        let target = GestureTarget::Menu(self.context_menu().target);
        self.run_binding(Gesture { id: GestureId::ContextClick, page, target }, at);
    }

    /// A gesture's binding other than its default: an event, nothing, or a command as one transaction.
    fn run_binding(&mut self, g: Gesture, at: Point) {
        match self.bindings.gestures.get(&g.id).cloned() {
            Some(GestureAction::Emit) => self.emit(Event::Gesture(GestureEvent { gesture: g.id, at: client(at), page: g.page, target: g.target })),
            Some(GestureAction::Op(command)) => {
                let _ = self.input(command);
            }
            Some(GestureAction::None | GestureAction::Default) | None => {}
        }
    }

    /// A wheel or trackpad scroll, CSS px. With Ctrl (Cmd) it zooms, 10% a notch of 100 px (a pinch sends small
    /// deltas); scrolling is the host's.
    pub fn wheel(&mut self, dx: f64, dy: f64, mods: Modifiers) {
        let _ = dx;
        if (mods.ctrl || mods.meta) && dy != 0.0 {
            let percent = (self.presentation.zoom() * 100.0 * 1.1f64.powf(-dy / 100.0)).clamp(10.0, 500.0);
            let _ = self.input(Command::SetZoom(Zoom::Percent(percent)));
        }
    }

    /// Copies (or cuts) the selection: what to put on the system clipboard. `None` when nothing is selected.
    pub fn copy(&mut self, cut: bool) -> Option<ClipboardData> {
        let command = if cut { Command::Cut } else { Command::Copy };
        self.check(&command).ok()?;
        // The copy is answered here, not asked of the host.
        let mut copied = false;
        let _ = self.apply_with(Origin::User, |e, fonts| {
            let out = e.execute(command, fonts);
            copied = e.take_clipboard_request();
            out
        });
        let f = self.editor.clipboard().filter(|_| copied)?.clone();
        Some(self.hold(f))
    }

    /// Makes `f`, copied here, the engine's clipboard: what goes on the system clipboard, its token naming it.
    pub(super) fn hold(&mut self, f: crate::editing::clipboard::Fragment) -> ClipboardData {
        let data = ClipboardData { text: f.text(), html: Some(f.html()), internal: Some(token(self.id, f.id)) };
        *self.shared.clipboard.borrow_mut() = Some((self.id, f));
        data
    }

    /// Pastes clipboard data as one transaction: the engine's own copy (`internal`, from any of its sessions) with
    /// its formatting, else the text; `as_text` pastes the text.
    pub fn paste(&mut self, data: &ClipboardData, as_text: bool) {
        let lines = |t: &str| t.replace("\r\n", "\n");
        // The token names the engine's copy; the text must match too (another engine's copy can carry the same token).
        let held =
            self.shared.clipboard.borrow().clone().filter(|(from, f)| {
                !as_text && data.internal.as_deref() == Some(token(*from, f.id).as_str()) && lines(&data.text) == lines(&f.text())
            });
        // Checked as the text: the held fragment's paste reads as unavailable until the state sees it adopted.
        let text = Command::PasteText(data.text.clone());
        if self.check(&text).is_err() {
            return;
        }
        let command = match held {
            Some((from, f)) => {
                if from != self.id || self.editor.clipboard_id() != Some(f.id) {
                    self.editor.adopt_clipboard(f);
                }
                Command::Paste(PasteOptions::default())
            }
            // shortcut: there is no HTML import, so HTML from elsewhere pastes as its text in the insertion point's
            // formatting; a converter from HTML to a fragment would paste it formatted.
            None => text,
        };
        let _ = self.apply(command, Origin::User);
        self.deliver_requests();
    }

    /// Application shortcuts: registering keys and looking up what a key does.
    pub fn shortcuts(&mut self) -> Shortcuts<'_> {
        Shortcuts { session: self }
    }

    /// The pointer gestures' bindings.
    pub fn gestures(&mut self) -> Gestures<'_> {
        Gestures { session: self }
    }

    /// What `chord` does now, besides shortcut `except`: a shortcut, a host command, or a command in some context.
    fn binding_of(&self, chord: Chord, except: Option<&str>) -> Option<KeyBinding> {
        if let Some(id) = self.bindings.shortcut(chord, except) {
            return Some(KeyBinding::Shortcut { id: id.to_string() });
        }
        if let Some(&(.., command)) = HOST_KEYS.iter().find(|k| (k.0, k.1, k.2, k.3) == chord) {
            return Some(KeyBinding::Host { command });
        }
        let (key, ctrl, shift, alt) = chord;
        let mods = Mods { ctrl, shift, alt, meta: false };
        let contexts = [
            KeyContext::default(),
            KeyContext { in_table: true, ..KeyContext::default() },
            KeyContext { list_start: true, ..KeyContext::default() },
            KeyContext { float: true, ..KeyContext::default() },
        ];
        let bound = contexts.into_iter().find_map(|cx| keymap::command(key.code(), mods, cx))?;
        let id = match bound {
            Bound::Run(c) => c.id(),
            Bound::Toggle(t) => keymap::toggle_id(t),
        };
        Some(KeyBinding::Command { id })
    }
}

/// A session's application shortcuts.
pub struct Shortcuts<'a> {
    session: &'a mut Session,
}

impl Shortcuts<'_> {
    /// Claims keys for the application: pressing them emits `shortcut` with its id instead of reaching the keymap.
    /// Registering an id again replaces its keys. Fails `unavailable` when a key is already a command's, a host
    /// key, another shortcut's or the editor's own (the menu and dialog keys), naming it (and `details.binding`).
    pub fn register(&mut self, shortcut: Shortcut) -> Result<(), Error> {
        let keys = match &shortcut.keys {
            ShortcutKeys::Same(k) => vec![k.as_str()],
            ShortcutKeys::PerPlatform { windows, mac } => vec![windows.as_str(), mac.as_str()],
        };
        let mut chords = Vec::new();
        for k in keys {
            let chord = parse_chord(k).ok_or_else(|| Error::invalid(format!("`{k}` isn't a key the editor knows")))?;
            let taken = match self.session.binding_of(chord, Some(&shortcut.id)) {
                Some(b) => Some((
                    match &b {
                        KeyBinding::Command { id } => format!("runs {}", super::commands::id_name(*id)),
                        KeyBinding::Shortcut { id } => format!("is the shortcut {id}"),
                        KeyBinding::Host { command } => format!("is the host key {}", serde_json::to_value(command).unwrap_or_default()),
                    },
                    serde_json::to_value(&b).ok(),
                )),
                None if MENU_KEYS.contains(&chord) => Some(("opens the context menu".to_string(), None)),
                None if DIALOG_KEYS.iter().any(|d| (d.0, d.1, d.2, d.3) == chord) => Some(("opens a dialog".to_string(), None)),
                None => None,
            };
            if let Some((what, binding)) = taken {
                let mut e = Error::unavailable(format!("{k} {what}"));
                e.details = binding.map(|b| serde_json::json!({ "binding": b }));
                return Err(e);
            }
            chords.push(chord);
        }
        let b = &mut self.session.bindings;
        b.shortcuts.retain(|(id, _)| *id != shortcut.id);
        b.shortcuts.push((shortcut.id, chords));
        Ok(())
    }

    /// Releases an application shortcut.
    pub fn unregister(&mut self, id: &str) {
        self.session.bindings.shortcuts.retain(|(s, _)| s != id);
    }

    /// What `keys` (`Ctrl+B`) do now.
    pub fn lookup(&self, keys: &str) -> Option<KeyBinding> {
        self.session.binding_of(parse_chord(keys)?, None)
    }
}

/// A session's gesture bindings.
pub struct Gestures<'a> {
    session: &'a mut Session,
}

impl Gestures<'_> {
    /// Every gesture's binding.
    pub fn bindings(&self) -> Vec<GestureBinding> {
        let bound = &self.session.bindings.gestures;
        let binding = |&(gesture, label): &(GestureId, &str)| GestureBinding {
            gesture,
            action: bound.get(&gesture).cloned().unwrap_or(GestureAction::Default),
            label: label.into(),
        };
        GESTURES.iter().map(binding).collect()
    }

    /// Rebinds a gesture.
    pub fn set(&mut self, gesture: GestureId, action: GestureAction) -> Result<(), Error> {
        let bound = &mut self.session.bindings.gestures;
        match action {
            GestureAction::Default => bound.remove(&gesture),
            action => bound.insert(gesture, action),
        };
        Ok(())
    }

    /// Back to the default binding, for one gesture or all of them.
    pub fn reset(&mut self, gesture: Option<GestureId>) {
        let bound = &mut self.session.bindings.gestures;
        match gesture {
            Some(g) => {
                bound.remove(&g);
            }
            None => bound.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::api::command::CommandId;
    use crate::api::engine::{Engine, SessionOptions};
    use crate::api::error::ErrorCode;
    use crate::api::event::{Event, HostCommand, KeyOutcome};
    use crate::api::fonts::FontLibrary;
    use crate::api::input::{Key, Modifiers};
    use crate::api::services::{KeyBinding, Shortcut, ShortcutKeys};

    fn shortcut(id: &str, keys: &str) -> Shortcut {
        Shortcut { id: id.into(), keys: ShortcutKeys::Same(keys.into()) }
    }

    #[test]
    fn shortcuts_claim_free_keys_and_name_the_taken_ones() {
        let mut engine = Engine::new(FontLibrary::empty());
        let events: Rc<RefCell<Vec<Event>>> = Rc::default();
        let sink = events.clone();
        engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        let mut sc = s.shortcuts();
        sc.register(Shortcut { id: "app.quote".into(), keys: ShortcutKeys::PerPlatform { windows: "Ctrl+Alt+Q".into(), mac: "⌥⌘Q".into() } })
            .expect("a free key");
        assert_eq!(sc.lookup("Ctrl+Alt+Q"), Some(KeyBinding::Shortcut { id: "app.quote".into() }));
        assert_eq!(sc.lookup("Ctrl+B"), Some(KeyBinding::Command { id: CommandId::SetBold }));
        assert_eq!(sc.lookup("Ctrl+S"), Some(KeyBinding::Host { command: HostCommand::Save }));
        assert_eq!(sc.lookup("Ctrl+Alt+J"), None);
        for (keys, names) in [("Ctrl+B", "setBold"), ("Ctrl+S", "save"), ("Ctrl+Alt+Q", "app.quote"), ("Shift+F10", "context menu")] {
            let e = sc.register(shortcut("app.other", keys)).expect_err("taken");
            assert_eq!(e.code, ErrorCode::Unavailable);
            assert!(e.message.contains(names), "{}", e.message);
        }
        assert_eq!(sc.register(shortcut("app.other", "Ctrl+Nope")).map_err(|e| e.code), Err(ErrorCode::InvalidArgument));
        // Registering again moves the shortcut.
        sc.register(shortcut("app.quote", "Ctrl+Alt+J")).expect("its own id");
        assert_eq!(sc.lookup("Ctrl+Alt+Q"), None);
        // The key emits the shortcut instead of reaching the keymap (Cmd stands for Ctrl).
        let revision = s.state().revision;
        events.borrow_mut().clear();
        let cmd_alt = Modifiers { meta: true, alt: true, ..Modifiers::default() };
        assert_eq!(s.key(Key::KeyJ, cmd_alt), KeyOutcome::Shortcut("app.quote".into()));
        assert_eq!(*events.borrow(), [Event::Shortcut { id: "app.quote".into() }]);
        assert_eq!(s.state().revision, revision);
        s.shortcuts().unregister("app.quote");
        assert_eq!(s.key(Key::KeyJ, cmd_alt), KeyOutcome::Unhandled);
    }
}
