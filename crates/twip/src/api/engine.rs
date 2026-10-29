//! The engine: the font library, the shared clipboard and the sessions of one thread, and the listener every
//! session's events go to.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::event::Event;
use super::fonts::FontLibrary;
use super::record::Author;
use super::session::Session;
use super::state::Mode;
use super::value::Bytes;

/// A session's id in its engine. Ids are never reused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub u32);

/// A Hunspell dictionary.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dictionary {
    /// BCP 47 (`en-US`).
    pub language: String,
    /// The affix file (`.aff`).
    pub aff: Bytes,
    /// The word list (`.dic`).
    pub dic: Bytes,
}

/// How a session starts. Every field has a default, so `{}` is a valid wire value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct SessionOptions {
    /// Who edits: tracked changes and comments are attributed to them.
    pub author: Author,
    /// Starts read only: commands that change the document fail with `read-only`.
    pub read_only: bool,
    /// Editing (the default) or template design.
    pub mode: Mode,
    /// BCP 47; text with no language is checked in it.
    pub language: String,
    /// Spelling dictionaries to load, one per language.
    pub dictionaries: Vec<Dictionary>,
    /// Turns tracking on after each open, unless the document's protection decides.
    pub track_changes: bool,
}

impl Default for SessionOptions {
    fn default() -> SessionOptions {
        SessionOptions {
            author: Author::default(),
            read_only: false,
            mode: Mode::Editing,
            language: "en-US".into(),
            dictionaries: Vec::new(),
            track_changes: false,
        }
    }
}

/// Receives every session's events, in order.
pub(crate) type Listener = Box<dyn FnMut(SessionId, &Event)>;

/// What the sessions of one engine share: the listener, and the fragment copied last in any of them.
#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) listener: RefCell<Option<Listener>>,
    pub(crate) clipboard: RefCell<Option<(SessionId, crate::editing::clipboard::Fragment)>>,
}

impl Shared {
    /// Delivers `event` to the listener. A listener never reaches the engine, so this never re-enters; were it to,
    /// the event would be dropped rather than delivered out of order.
    pub(crate) fn emit(&self, session: SessionId, event: &Event) {
        if let Ok(mut listener) = self.listener.try_borrow_mut()
            && let Some(f) = listener.as_mut()
        {
            f(session, event);
        }
    }
}

/// The engine of one thread: its font library, its sessions and their shared clipboard.
pub struct Engine {
    fonts: FontLibrary,
    // Every session holds it: their events reach the one listener, and a copy in one pastes in another.
    shared: Rc<Shared>,
    sessions: BTreeMap<SessionId, Session>,
    next: u32,
}

impl Engine {
    /// An engine laying out with `fonts`.
    pub fn new(fonts: FontLibrary) -> Engine {
        Engine { fonts, shared: Rc::default(), sessions: BTreeMap::new(), next: 1 }
    }

    /// The listener for every session's events, replacing an earlier one. It must not call the engine: work it
    /// starts is queued and run after it returns.
    pub fn set_listener(&mut self, f: impl FnMut(SessionId, &Event) + 'static) {
        *self.shared.listener.borrow_mut() = Some(Box::new(f));
    }

    /// A new session with a blank document.
    pub fn create_session(&mut self, options: SessionOptions) -> SessionId {
        let id = SessionId(self.next);
        self.next += 1;
        self.sessions.insert(id, Session::new(id, options, self.fonts.clone(), self.shared.clone()));
        id
    }

    /// The session `id`, unless it was closed. It has caught up with changes to the font library.
    pub fn session(&mut self, id: SessionId) -> Option<&mut Session> {
        let s = self.sessions.get_mut(&id)?;
        s.sync_fonts();
        Some(s)
    }

    /// Ends the session; its id stays unused. Closing a closed session does nothing.
    pub fn close_session(&mut self, id: SessionId) {
        self.sessions.remove(&id);
    }

    /// The font library every session lays out with.
    pub fn fonts(&self) -> &FontLibrary {
        &self.fonts
    }

    /// After a change to the font library: every session lays out again and reports its fonts' state.
    pub fn sync_fonts(&mut self) {
        self.sessions.values_mut().for_each(Session::sync_fonts);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::{Engine, SessionOptions};
    use crate::api::command::Command;
    use crate::api::event::Event;
    use crate::api::fonts::FontLibrary;

    #[test]
    fn listener_hears_each_commit_in_order_after_the_state() {
        let mut engine = Engine::new(FontLibrary::system());
        let heard: Rc<RefCell<Vec<(&str, u64)>>> = Rc::default();
        let sink = heard.clone();
        engine.set_listener(move |_, e| match e {
            Event::StateChanged(s) => sink.borrow_mut().push(("state", s.revision)),
            Event::DocumentChanged(r) => sink.borrow_mut().push(("commit", r.revision)),
            _ => {}
        });
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        let mut last = 0;
        for c in [Command::InsertText("a".into()), Command::InsertText("b".into()), Command::Undo, Command::Redo] {
            heard.borrow_mut().clear();
            s.run(c).expect("runs");
            // Heard before `run` returned: the state rebuilt for this commit, then the commit itself.
            let rev = s.state().revision;
            assert_eq!(*heard.borrow(), [("state", rev), ("commit", rev)]);
            assert_ne!(rev, last);
            last = rev;
        }
    }
}
