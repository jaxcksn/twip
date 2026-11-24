//! Keyboard, text, IME and pointer input, and the clipboard. Hosts forward raw input; the engine recognises
//! shortcuts and gestures itself.

use std::collections::BTreeMap;

use super::Session;
use crate::api::command::{Command, PasteAs, PasteOptions, Zoom};
use crate::api::engine::SessionId;
use crate::api::error::Error;
use crate::api::event::{Event, FieldRequest, HostCommand, KeyOutcome, LinkVia};
use crate::api::input::{ClipboardData, ClipboardImage, CompositionPhase, Key, Modifiers, Point, PointerButton, PointerKind};
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
const HOST_KEYS: [(Key, bool, bool, bool, HostCommand); 23] = [
    (Key::KeyS, true, false, false, HostCommand::Save),
    (Key::F12, false, false, false, HostCommand::SaveAs),
    (Key::KeyP, true, false, false, HostCommand::Print),
    (Key::KeyO, true, false, false, HostCommand::Open),
    (Key::KeyN, true, false, false, HostCommand::New),
    (Key::KeyF, true, false, false, HostCommand::Find),
    (Key::KeyH, true, false, false, HostCommand::Replace),
    (Key::KeyG, true, false, false, HostCommand::GoTo),
    (Key::KeyK, true, false, false, HostCommand::InsertHyperlink),
    (Key::F7, false, false, true, HostCommand::NextMisspelling),
    (Key::KeyF, true, true, false, HostCommand::FocusFont),
    (Key::KeyP, true, true, false, HostCommand::FocusFontSize),
    (Key::F1, false, false, false, HostCommand::Help),
    (Key::KeyS, true, true, false, HostCommand::ApplyStyles),
    (Key::KeyS, true, true, true, HostCommand::StylesPane),
    (Key::KeyG, true, true, false, HostCommand::WordCount),
    (Key::F5, true, true, false, HostCommand::Bookmark),
    (Key::KeyV, true, false, true, HostCommand::PasteSpecial),
    (Key::F7, false, true, false, HostCommand::Thesaurus),
    (Key::F7, false, false, false, HostCommand::Spelling),
    (Key::KeyW, true, false, false, HostCommand::Close),
    (Key::F2, true, false, false, HostCommand::PrintPreview),
    (Key::KeyO, false, true, true, HostCommand::MarkTableOfContentsEntry),
];

/// Keys that ask for a dialog: key, Ctrl, Shift, Alt, and the dialog.
const DIALOG_KEYS: [(Key, bool, bool, bool, DialogKind); 1] = [(Key::KeyD, true, false, false, DialogKind::Font)];

/// The context-menu key and Shift+F10.
const MENU_KEYS: [Chord; 2] = [(Key::ContextMenu, false, false, false), (Key::F10, false, true, false)];

/// Every gesture, with its label for a settings screen.
const GESTURES: [(GestureId, &str); 32] = [
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
    (GestureId::PictureControlClick, "Click a picture control"),
    (GestureId::PictureControlDoubleClick, "Double-click a picture control"),
    (GestureId::RulerClick, "Click the ruler"),
    (GestureId::RulerDrag, "Drag a ruler marker"),
    (GestureId::RulerDragOff, "Drag a tab stop off the ruler"),
    (GestureId::RulerDoubleClick, "Double-click the ruler"),
    (GestureId::ContextClick, "Right-click"),
    (GestureId::ObjectDoubleClick, "Double-click a picture or shape"),
    (GestureId::ObjectResizeDrag, "Drag a sizing handle"),
    (GestureId::ShapeHandleDrag, "Drag a shape's adjustment or rotation handle"),
    (GestureId::TableMoveDrag, "Drag a table's move handle"),
    (GestureId::TableZoneClick, "Click a cell, row or column's selection area"),
    (GestureId::TableInsertClick, "Click a table's insert mark"),
    (GestureId::NoteDoubleClick, "Double-click a note reference"),
    (GestureId::ClickAndType, "Double-click blank space"),
    (GestureId::PageGapDoubleClick, "Double-click between pages"),
    (GestureId::FormFieldClick, "Click a drop-down form field"),
    (GestureId::FormFieldDoubleClick, "Double-click a form field"),
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
        // Esc dismisses an open popover first.
        if key == Key::Escape && self.close_popover().is_some() {
            return KeyOutcome::Handled;
        }
        // Cmd on macOS stands where Ctrl does elsewhere.
        let ctrl = mods.ctrl || mods.meta;
        let chord = (key, ctrl, mods.shift, mods.alt);
        if let Some(id) = self.bindings.shortcut(chord, None).map(str::to_string) {
            self.emit(Event::Shortcut { id: id.clone() });
            return KeyOutcome::Shortcut(id);
        }
        // F4 and Alt+Down open a drop-down form field's list where forms are filled in (F4 elsewhere repeats).
        if matches!(chord, (Key::F4, false, false, false) | (Key::ArrowDown, false, false, true)) && self.editor.selected_form_drop_down().is_some() {
            let command = HostCommand::OpenFormFieldList;
            self.emit(Event::HostCommand { command });
            return KeyOutcome::HostCommand(command);
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
        use crate::editing::autocorrect::{Trigger, may_trigger};
        // Enter on a picture control, or Space on an unfilled one, activates it in read only too: it changes nothing.
        let activates = match &command {
            Command::InsertParagraph => self.editor.selected_picture_control().is_some(),
            Command::InsertText(t) => t == " " && self.editor.selected_picture_unfilled(),
            _ => false,
        };
        if self.read_only && activates {
            return self.apply_with(Origin::User, |e, _| {
                e.activate_picture_control();
                Outcome::Unchanged("picture control activated")
            });
        }
        if let Err(e) = self.check(&command) {
            // An edit the user tried in a read-only document is said to be refused, as Word does; the API's aren't.
            if e.code == crate::api::error::ErrorCode::ReadOnly {
                self.announce(vec![(crate::api::services::AnnouncementKind::ReadOnly, "Read only".into())]);
            }
            return Err(e);
        }
        if let Some(done) = self.run_view(&command) {
            return done;
        }
        // Typing corrects what it finishes (AutoCorrect), as a step of its own after the typing's.
        let trigger = match &command {
            Command::InsertText(t) if t.chars().last().is_some_and(may_trigger) => Some(Trigger::Typed),
            Command::InsertTab => Some(Trigger::Typed),
            Command::InsertParagraph => Some(Trigger::ParagraphEnd),
            _ => None,
        };
        let done = self.apply(command, Origin::User)?;
        if let Some(trigger) = trigger {
            let ac = std::mem::take(&mut self.autocorrect);
            let corrected = self.apply_with(Origin::User, |e, fonts| e.auto_correct(&ac, trigger, fonts));
            self.autocorrect = ac;
            if let Ok(c) = corrected
                && c.record.is_some()
            {
                return Ok(c);
            }
        }
        Ok(done)
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
                Notice::PictureControl { control } => Event::PictureControlActivated(crate::api::services::PictureControlActivation {
                    element: id(crate::docx::ElementId::Control(control)),
                }),
                Notice::Warning(code, message) => Event::Warning(crate::docx::diagnostics::Warning { code, message, count: 1, location: None }),
                Notice::Refused { sub_template, element } => Event::EditRefused {
                    reason: match sub_template {
                        Some(name) => crate::api::event::RefusalReason::SubTemplate { name },
                        None => crate::api::event::RefusalReason::Locked,
                    },
                    element: element.map(id),
                },
            })
            .collect()
    }

    /// An IME composition step: `text` is the composition so far, with the cursor at `cursor` characters. It shows
    /// at the caret, underlined, in place of the selection, and enters neither the history nor the records; the end
    /// takes it out and commits its text (none: cancelled), as typed text.
    pub fn composition(&mut self, phase: CompositionPhase, text: &str, cursor: u32) {
        if phase == CompositionPhase::End {
            self.end_composition();
            if text.is_empty() {
                self.changed(Vec::new());
            } else {
                self.text(text);
            }
            return;
        }
        if self.check(&Command::InsertText(text.to_string())).is_err() && !text.is_empty() {
            return;
        }
        let fonts = self.fonts.fonts();
        self.editor.compose(text, cursor, &fonts);
        drop(fonts);
        self.discard_commits();
        self.changed(Vec::new());
        self.deliver_requests();
    }

    /// Takes out the composition showing, if there is one, unrecorded.
    pub(super) fn end_composition(&mut self) {
        if self.editor.composing() {
            let fonts = self.fonts.fonts();
            self.editor.end_composition(&fonts);
            drop(fonts);
            self.discard_commits();
        }
    }

    /// Forgets the editor's commits since the last: edits the host never hears of.
    fn discard_commits(&mut self) {
        let _ = (self.editor.take_commits(), self.editor.take_made());
    }

    /// Pointer input at a point in the viewport, with the button pressed or released and the click count. The
    /// engine recognises the gestures (a secondary press is `contextClick`) and runs each one's binding.
    pub fn pointer(&mut self, kind: PointerKind, at: Point, button: PointerButton, clicks: u8, mods: Modifiers) {
        if self.ruler_pointer(kind, at, button, clicks, mods) {
            // The hovered marker's tooltip.
            self.deliver_requests();
            return;
        }
        if kind == PointerKind::Down {
            // The Paste Options button stays while the paste is the last change, as Word's does.
            if !matches!(self.presentation.popover, Some(crate::api::services::Popover::PasteOptions { .. })) {
                self.close_popover();
            }
            self.end_composition();
        }
        let (x, y) = self.presentation.to_document(at);
        // The correction mark's options button opens its popover.
        if kind == PointerKind::Down && self.on_correction_button(x, y) {
            if let Some(p) = self.auto_correct_popover() {
                self.show_popover(p);
            }
            return;
        }
        let api_mods = mods;
        let mods = internal(mods);
        self.pointer_mods = mods;
        self.sync_view_settings();
        if (kind, button) == (PointerKind::Down, PointerButton::Primary) {
            self.pressed = self.editor.element_under(x, y, mods, &self.fonts.fonts()).map(|(e, _)| e);
        }
        match (kind, button) {
            (PointerKind::Down, PointerButton::Secondary) => {
                // In the selected text the press starts a right-button drag, whose release opens the drop menu.
                let mut dragging = false;
                if !self.bindings.gestures.contains_key(&GestureId::ContextClick) {
                    let _ = self.apply_with(Origin::User, |e, fonts| {
                        dragging = e.right_press(x, y, fonts);
                        Outcome::Unchanged("right press")
                    });
                }
                self.right_drag = dragging;
                if !dragging {
                    self.context_click(at, x, y);
                }
            }
            (PointerKind::Up, PointerButton::Secondary) if std::mem::take(&mut self.right_drag) => {
                let mut dropped = None;
                let _ = self.apply_with(Origin::User, |e, _| {
                    dropped = e.right_release();
                    Outcome::Unchanged("right release")
                });
                match dropped {
                    Some(_) => {
                        let menu = crate::services::menu::drop_menu(self.editor.document().file_name.is_some());
                        self.emit(Event::ContextMenu(ContextMenuEvent { menu, at: client(at), via: MenuVia::Pointer }));
                    }
                    // Not dragged, or dropped on the selection: the context menu, as Word opens it on the release.
                    None => self.open_context_menu(client(at), MenuVia::Pointer),
                }
            }
            // The middle button, and the secondary one's release, do nothing.
            (PointerKind::Down | PointerKind::Up, PointerButton::Secondary | PointerButton::Middle) => {}
            (PointerKind::Leave, _) => {
                let hovered = self.editor.table_hover().is_some();
                self.editor.pointer_leave();
                // The table's handles go with the pointer.
                if hovered && self.editor.table_hover().is_none() {
                    self.presentation.want_frame();
                }
            }
            (PointerKind::Down, PointerButton::Primary) => {
                let pressed = self.editor.press_gesture(x, y, clicks, mods, &self.fonts.fonts());
                // A form field's list or options are the host's: the press selects the field and emits the gesture.
                let form = pressed.clone().filter(|g| matches!(g.id, GestureId::FormFieldClick | GestureId::FormFieldDoubleClick));
                match self.bindings.rebound(pressed) {
                    // A picture control rebound still takes the press: it is selected, not activated.
                    Some(g) if matches!(g.id, GestureId::PictureControlClick | GestureId::PictureControlDoubleClick) => {
                        self.editor.set_activation_suppressed(true);
                        self.drive(kind, x, y, clicks, mods);
                        self.editor.set_activation_suppressed(false);
                        self.run_binding(g, at);
                    }
                    Some(g) => self.run_binding(g, at),
                    None => {
                        self.drive(kind, x, y, clicks, mods);
                        if let Some(g) = form {
                            self.emit(Event::Gesture(GestureEvent { gesture: g.id, at: client(at), page: g.page, target: g.target }));
                        }
                    }
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
        if (kind, button) == (PointerKind::Up, PointerButton::Primary)
            && let Some(e) = self.pressed.take()
            && self.editor.element_under(x, y, mods, &self.fonts.fonts()).is_some_and(|(under, _)| under == e)
        {
            let element = self.editor.element_context_of(e, false);
            self.emit(Event::ElementClick(crate::api::services::ElementClick { element, count: clicks, button, modifiers: api_mods }));
        }
        // Hovering changes the cursor and tooltip with no other change.
        self.deliver_requests();
    }

    /// The modifier keys changed with no other input (Alt pressed or released): a ruler drag under way, or the hover,
    /// runs again at the pointer's last position, so its snapping and readouts follow at once.
    pub fn modifiers(&mut self, mods: Modifiers) {
        let p = &self.presentation;
        let at = p.ruler_drag.as_ref().map(|d| d.now).or(p.ruler_hover.map(|h| h.at));
        if let Some(at) = at {
            self.ruler_pointer(PointerKind::Move, at, PointerButton::Primary, 0, mods);
            self.deliver_requests();
        }
    }

    /// The rulers' gestures: true when the pointer is on a ruler, or dragging from one, and the input was theirs. A
    /// drag moves its marker and guide line with the pointer and commits nothing until its release; hovering shows
    /// what's under the pointer.
    fn ruler_pointer(&mut self, kind: PointerKind, at: Point, button: PointerButton, clicks: u8, mods: Modifiers) -> bool {
        use crate::services::ruler::{Hit, Marker, hit, removing};
        if let Some(mut d) = self.presentation.ruler_drag.take() {
            d.moved |= (at.x - d.start.x).abs().max((at.y - d.start.y).abs()) > 2.0;
            (d.now, d.free) = (at, mods.alt);
            d.removing = d.moved && matches!(d.marker, Some(Marker::Tab(_))) && removing(if d.horizontal() { at.y } else { at.x });
            match kind {
                PointerKind::Move => {
                    // The marker, guide and readout moved: a frame shows them.
                    if d.moved {
                        self.presentation.want_frame();
                    }
                    self.presentation.ruler_drag = Some(d);
                }
                PointerKind::Up => {
                    self.request(super::HostRequest::Release);
                    self.presentation.want_frame();
                    self.ruler_release(&d, at);
                }
                PointerKind::Down | PointerKind::Cancel | PointerKind::Leave => {
                    self.request(super::HostRequest::Release);
                    self.presentation.want_frame();
                }
            }
            return true;
        }
        let view = self.ruler_view();
        let on = hit(&view, self.presentation.rulers, at.x, at.y);
        let hover = on.filter(|_| kind == PointerKind::Move).map(|_| super::host::RulerHover { at, free: mods.alt });
        if hover != self.presentation.ruler_hover {
            self.presentation.ruler_hover = hover;
            self.presentation.want_frame();
        }
        let Some(on) = on else { return false };
        if (kind, button) != (PointerKind::Down, PointerButton::Primary) {
            return true;
        }
        // A press closes the open popover; on the box, with the picker open, that's all it does.
        let open = self.close_popover();
        if clicks >= 2 && on != Hit::Selector {
            self.ruler_gesture(GestureId::RulerDoubleClick, view.page, at, |s| s.emit(Event::DialogRequested { kind: DialogKind::Tabs }));
            return true;
        }
        let marker = match on {
            Hit::Selector => {
                if !matches!(open, Some(crate::api::services::Popover::TabType { .. })) {
                    self.show_popover(self.tab_types_popover());
                }
                return true;
            }
            Hit::Ruler => return true,
            Hit::Marker(m) => Some(m),
            Hit::Text(_) => None,
        };
        let drag = super::host::RulerDrag { marker, view, start: at, now: at, moved: false, free: mods.alt, removing: false };
        self.presentation.ruler_drag = Some(drag);
        self.request(super::HostRequest::Capture);
        true
    }

    /// A ruler press released: a click on the text area adds what the tab selector says (`rulerClick`), and a click on
    /// a stop asks for its options; a moved marker runs its command (`rulerDrag`), or a tab stop dragged more than the
    /// removal distance below the ruler is deleted (`rulerDragOff`).
    fn ruler_release(&mut self, d: &super::host::RulerDrag, at: Point) {
        use crate::api::command::{AddTabStop, BoundaryMove, ColumnBoundaryMove, Indents, MoveTabStop};
        use crate::api::services::TabSelector as T;
        use crate::api::value::{Points, TabAlignment};
        use crate::services::ruler::Marker as M;
        let (h, v) = (&d.view.model.horizontal, &d.view.model.vertical);
        let a = crate::services::ruler::along(&d.view.model);
        let (left, first) = (a.indents.left.0, a.indents.first_line.0);
        let value = d.value();
        // From the line's start and to its end
        let (from_start, to_end) = (a.from_start(value), a.from_end(value));
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
                let at = from_start.max(0.0);
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
            Some(M::Tab(pos)) if !d.moved => {
                if let Some(p) = self.tab_stop_popover(pos) {
                    self.show_popover(p);
                }
                return;
            }
            Some(_) if !d.moved => return,
            Some(M::Tab(pos)) if d.removing => (GestureId::RulerDragOff, Command::DeleteTabStop(Points(pos))),
            Some(m) => {
                // A margin's edge: the section's margin it is, from the page's edge less the gutter beside it.
                let set_margin = |s: &mut Self| {
                    let section = s.editor.format_state().map_or(0, |f| f.section);
                    let page = (h.page_width.0, v.page_height.0);
                    crate::services::ruler::margins(&mut s.editor, section).and_then(|r| r.margins_at(m, value, page.0, page.1))
                };
                let command = match m {
                    M::Tab(pos) => Command::MoveTabStop(MoveTabStop { from: Points(pos), to: Points(from_start.max(0.0)) }),
                    M::FirstLine => Command::SetIndents(indents(None, from_start - left)),
                    M::Hanging => Command::SetIndents(indents(Some(from_start), left + first - from_start)),
                    M::Left => Command::SetIndents(Indents { left: Some(Points(from_start)), ..Indents::default() }),
                    M::Right => Command::SetIndents(Indents { right: Some(Points(to_end)), ..Indents::default() }),
                    M::TextStart | M::TextEnd | M::Top | M::Bottom => match set_margin(self) {
                        Some(margins) => Command::SetMargins(margins),
                        None => return,
                    },
                    M::Column(column) => {
                        // Columns stacked from the right: the boundary's distance from the stacking margin, from where
                        // the section's columns are laid out.
                        let to = match h.columns.as_slice() {
                            [c0, c1, ..] if c1.start.0 < c0.start.0 => {
                                let section = self.editor.format_state().map_or(0, |f| f.section);
                                let doc = self.editor.document();
                                let Some(s) = doc.sections.get(section) else { return };
                                let left = s.left + if doc.settings.gutter_at_top { 0.0 } else { s.gutter };
                                left + (s.page_w - s.right - value)
                            }
                            _ => value,
                        };
                        Command::MoveColumnBoundary(ColumnBoundaryMove { column, to: Points(to) })
                    }
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
    fn drive(&mut self, kind: PointerKind, x: f64, y: f64, clicks: u8, mods: Mods) {
        use crate::editing::input::PointerKind as K;
        let kind = match kind {
            PointerKind::Down => K::Down,
            PointerKind::Move => K::Move,
            PointerKind::Up => K::Up,
            PointerKind::Cancel | PointerKind::Leave => K::Cancel,
        };
        let (hovered, region) = (self.editor.table_hover(), self.editor.hover_region());
        let _ = self.apply_with(Origin::User, |e, fonts| {
            e.pointer_input(kind, x, y, clicks, mods, fonts);
            Outcome::Unchanged("pointer input")
        });
        // A border, object or table drag moves its guide with no new state, and a table's handles and a hovered
        // control's label follow the pointer: a frame shows them.
        let moved = self.editor.table_hover() != hovered || self.editor.hover_region() != region;
        if kind == K::Move && (self.editor.dragging() || moved) {
            self.presentation.want_frame();
        }
        // The Select Objects rectangle, the pen's stroke and the eraser's border are state with no edit.
        let v = &self.state.view;
        let shown = (v.object_marquee, v.table_tool_feedback, v.table_eraser_target);
        if crate::api::state::pointer_feedback(&mut self.editor, &self.fonts.fonts()) != shown {
            self.changed(Vec::new());
        }
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
        // "Cut" or "Copied", as the commands say.
        let said = self.command_said(&command);
        // The copy is answered here, not asked of the host.
        let mut copied = false;
        let _ = self.apply_with(Origin::User, |e, fonts| {
            let out = e.execute(command, fonts);
            copied = e.take_clipboard_request();
            out
        });
        let f = self.editor.clipboard().filter(|_| copied)?.clone();
        self.announce(said);
        Some(self.hold(f))
    }

    /// Makes `f`, copied here, the engine's clipboard: what goes on the system clipboard, its token naming it. A copy of
    /// an object carries its picture; the HTML gives objects as their alternative text (the HTML with them drawn is
    /// [`Session::clipboard_html`], asked when the target takes HTML).
    pub(super) fn hold(&mut self, f: crate::editing::clipboard::Fragment) -> ClipboardData {
        let image = crate::services::clipboard::image(&f, &self.fonts.fonts())
            .map(|p| ClipboardImage { mime: "image/png".into(), data: crate::api::value::Bytes(p.png) });
        let data = ClipboardData { text: f.text(), html: Some(f.html()), internal: Some(token(self.id, f.id)), image };
        *self.shared.clipboard.borrow_mut() = Some((self.id, f));
        data
    }

    /// The HTML of the copy `token` names, its objects drawn as pictures in it (as Word's copies carry them): for the
    /// host to give the clipboard when the target asks for HTML. A copy made in another session gives its HTML with
    /// the objects as their alternative text (the session that made it draws them). Unavailable once another copy
    /// replaced it.
    pub fn clipboard_html(&mut self, token_: &str) -> Result<String, Error> {
        let held = self.shared.clipboard.borrow().clone();
        match held {
            Some((from, f)) if token(from, f.id) == token_ => {
                Ok(if from == self.id { crate::services::clipboard::html(&self.editor, &f, &self.fonts.fonts()) } else { f.html() })
            }
            _ => Err(Error::unavailable("the clipboard has changed")),
        }
    }

    /// Makes the engine's clipboard the editor's, for a paste of it.
    pub(super) fn adopt_held(&mut self) {
        // An undelivered copy here is the newest.
        if self.editor.clipboard_requested()
            && let Some(f) = self.editor.clipboard().cloned()
        {
            self.hold(f);
            return;
        }
        let held = self.shared.clipboard.borrow().clone();
        if let Some((from, f)) = held
            && (from != self.id || self.editor.clipboard_id() != Some(f.id))
        {
            self.editor.adopt_clipboard(f);
        }
    }

    /// Starts a drag of the selection out of the view, to another application or editor: what the host's platform
    /// drag carries (a copy's clipboard data). `None` for a caret, or with text drag and drop off. End it with
    /// [`Session::drag_end`], or [`Session::drop_dragged`] when it comes back to this view.
    pub fn drag_out(&mut self) -> Option<crate::api::services::DragSource> {
        let d = self.editor.drag_out();
        self.deliver_requests();
        let mut d = d?;
        let can_move = d.can_move && !self.read_only;
        let fonts = self.fonts.fonts();
        self.editor.draw_objects(&mut d.fragment, &fonts);
        let html = crate::services::clipboard::html(&self.editor, &d.fragment, &fonts);
        drop(fonts);
        let mut data = self.hold(d.fragment);
        data.html = Some(html);
        Some(crate::api::services::DragSource { data, can_move })
    }

    /// A drag out of the view ended elsewhere: a move deletes the dragged text, as one undo step (a smart cut),
    /// unless the selection or the document changed since it started.
    pub fn drag_end(&mut self, effect: crate::api::services::DropEffect) {
        let moved = effect == crate::api::services::DropEffect::Move && !self.read_only;
        let _ = self.apply_with(Origin::User, |e, fonts| e.drag_out_end(moved, fonts));
        self.deliver_requests();
    }

    /// A drag out of the view came back and dropped at `at`: the text moves there (`copy`: is copied), one undo step,
    /// as a drag inside the view would. Refused where it can't go.
    pub fn drop_dragged(&mut self, at: Point, copy: bool) -> Result<super::Commit, Error> {
        let (x, y) = self.presentation.to_document(at);
        let command = if copy { Command::Copy } else { Command::Cut };
        if !copy {
            self.check_where(&command, false)?;
        }
        let done = self.apply_with(Origin::User, |e, fonts| e.drag_out_drop(x, y, copy, fonts));
        self.deliver_requests();
        done
    }

    /// Pastes clipboard data as one transaction, as the paste options say for where it came from: the engine's own
    /// copy (`internal`, from any of its sessions), the HTML, the picture or the text; `as_text` pastes the text. The
    /// first `as_text` paste (Ctrl+Shift+V) after Copy Formatting (Ctrl+Shift+C) pastes the copied formatting instead,
    /// as Word's Paste Formatting, until a copy or other clipboard data ends it; a copy in another application between
    /// the two can't be seen.
    pub fn paste(&mut self, data: &ClipboardData, as_text: bool) {
        if as_text && self.editor.paste_takes_formatting(&data.text, data.html.as_deref()) {
            let _ = self.input(Command::PasteFormatting);
            self.deliver_requests();
            return;
        }
        let _ = self.paste_with(data, as_text.then_some(PasteAs::Text));
    }

    /// What `data` holds, as a paste reads it: the held copy when its token and text are the engine's.
    fn clip_of(&self, data: &ClipboardData) -> (crate::editing::paste::Clip, bool) {
        let lines = |t: &str| t.replace("\r\n", "\n");
        // The token names the engine's copy; the text must match too (another engine's copy can carry the same token).
        let held = self
            .shared
            .clipboard
            .borrow()
            .clone()
            .filter(|(from, f)| data.internal.as_deref() == Some(token(*from, f.id).as_str()) && lines(&data.text) == lines(&f.text()));
        let clip = crate::editing::paste::Clip {
            fragment: held.as_ref().map(|(_, f)| f.clone()),
            html: data.html.clone(),
            text: data.text.clone(),
            image: data.image.as_ref().map(|i| i.data.0.clone()),
        };
        (clip, held.is_some())
    }

    /// Pastes clipboard data as one transaction, as `choice` takes it (Paste Special, a host's own paste menu), or as
    /// the paste options say for where it came from (`None`, `formatted`). Told nothing, HTML that pastes nothing
    /// gives way to the picture, else the text; told a choice, it is pasted that way or refused (unavailable). The
    /// Paste Options button (`popoverRequested` kind `pasteOptions`) then offers the other choices.
    pub fn paste_with(&mut self, data: &ClipboardData, choice: Option<PasteAs>) -> Result<(), Error> {
        use crate::editing::paste::PasteChoice as P;
        let told = choice.and_then(P::of_api);
        let (mut clip, held) = self.clip_of(data);
        let Some(asked) = told.map(|c| self.editor.landing(&clip, c)).or_else(|| self.editor.default_paste(&clip)) else { return Ok(()) };
        let mut c = asked;
        let text = Command::PasteText(data.text.clone());
        if c == P::Picture {
            let picture = clip.image.clone().unwrap_or_default();
            let command = match self.editor.selected_picture_control() {
                Some(id) => Command::FillControl(crate::api::command::FillControl {
                    id: crate::api::value::ElementId(crate::editing::elements::id_str(crate::docx::ElementId::Control(id))),
                    image: Some(crate::api::value::Bytes(picture)),
                    text: None,
                    docx: None,
                    building_block: None,
                    date: None,
                }),
                None => Command::InsertPicture(crate::api::command::Picture { data: crate::api::value::Bytes(picture), alt_text: None }),
            };
            if let Err(e) = self.check(&command) {
                // Refused as a picture, the text
                if told.is_some() || clip.text.is_empty() {
                    return Err(e);
                }
                c = P::Unformatted;
            }
        }
        if c != P::Picture {
            self.check(&text)?;
            // Text-only fallback where protection refuses the formatted paste
            let formatted = Command::Paste(PasteOptions::default());
            if !matches!(c, P::Unformatted | P::TextOnly) && self.editor.protection_refusal(&formatted).is_some() {
                c = if clip.fragment.is_some() || clip.html.is_some() { P::TextOnly } else { P::Unformatted };
            }
        }
        if held && c != P::Unformatted {
            self.adopt_held();
            clip.fragment = self.editor.clipboard().cloned();
        } else if c == P::Unformatted {
            clip.fragment = None;
        }
        // The choice the gate fell back to, else as asked
        let choice = if c == asked { told } else { Some(c) };
        let done = self.apply_with(Origin::User, |e, fonts| e.paste_clip(clip, choice, fonts));
        self.deliver_requests();
        done.map(|_| ())
    }

    /// What `data` can paste as at the selection, in Word's order (Paste Special's list), and what Ctrl+V takes (one
    /// of them). Empty where nothing can be pasted (read only, protection).
    pub fn paste_choices(&self, data: &ClipboardData) -> crate::api::services::PasteChoices {
        let (clip, _) = self.clip_of(data);
        let formatted = Command::Paste(PasteOptions::default());
        if self.read_only || (self.editor.protection_refusal(&formatted).is_some() && self.check(&Command::PasteText(data.text.clone())).is_err()) {
            return crate::api::services::PasteChoices::default();
        }
        let choices = self.editor.paste_choices(&clip).into_iter().map(|c| c.to_api()).collect();
        let default = self.editor.default_paste(&clip).map(|c| c.to_api());
        crate::api::services::PasteChoices { choices, default }
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
        // Mac chords: Ctrl as Cmd
        let mods = if self.mac { Mods { ctrl: false, shift, alt, meta: ctrl } } else { Mods { ctrl, shift, alt, meta: false } };
        let base = KeyContext { mac: self.mac, ..KeyContext::default() };
        let contexts = [base, KeyContext { in_table: true, ..base }, KeyContext { list_start: true, ..base }, KeyContext { float: true, ..base }];
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
