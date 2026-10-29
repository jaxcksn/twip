//! A session: one document being edited, with its selection, history, pipeline, services and presentation. Each
//! submodule holds one area's methods.
#![warn(missing_docs)]

mod commands;
mod documents;
mod edit;
mod elements;
mod fonts;
mod host;
mod input;
mod review;
mod services;

use std::rc::Rc;

pub use documents::{Export, ExportFormat, ExportOptions, OpenToken};
pub use edit::Commit;
pub use host::{Cursor, FrameInfo, HostRequest, HostSurface};
pub use input::{Gestures, Shortcuts};
pub use services::Spelling;

use super::engine::{SessionId, SessionOptions, Shared};
use super::event::Event;
use super::fonts::FontLibrary;
use super::record::Author;
use super::state::{Mode, State};
use crate::editing::Editor;

/// One document being edited. Every change runs as a transaction; after each one the state is rebuilt, then the
/// engine's listener hears about it.
pub struct Session {
    id: SessionId,
    editor: Editor,
    fonts: FontLibrary,
    shared: Rc<Shared>,
    state: State,
    author: Author,
    read_only: bool,
    mode: Mode,
    language: String,
    track_changes: bool,
    /// The last transaction record's id.
    records: u64,
    /// Table gridlines show where cells have no borders.
    gridlines: bool,
    presentation: host::Presentation,
    watch: fonts::Watch,
    services: crate::services::Services,
    /// Gesture bindings and application shortcuts.
    bindings: input::Bindings,
}

impl Session {
    pub(crate) fn new(id: SessionId, options: SessionOptions, fonts: FontLibrary, shared: Rc<Shared>) -> Session {
        let editor = documents::blank(&fonts);
        let mut s = Session {
            id,
            editor,
            fonts,
            shared,
            state: State::default(),
            author: options.author,
            read_only: options.read_only,
            mode: options.mode,
            language: options.language,
            track_changes: options.track_changes,
            records: 0,
            gridlines: false,
            presentation: host::Presentation::default(),
            watch: fonts::Watch::default(),
            services: crate::services::Services::default(),
            bindings: input::Bindings::default(),
        };
        for d in &options.dictionaries {
            // shortcut: a dictionary that doesn't parse is left out silently; report it as a warning event if hosts
            // need to know at creation.
            let _ = s.services.spelling.add_dictionary(&d.language, &d.aff.0, &d.dic.0);
        }
        s.configure();
        s.fonts_opened();
        s.state = s.build_state();
        s
    }

    /// Runs `f` on the editor, then rebuilds the state as after a change: for the crate's tests to read and set up
    /// what the API doesn't reach.
    #[cfg(test)]
    pub(crate) fn with_editor<R>(&mut self, f: impl FnOnce(&mut Editor) -> R) -> R {
        let out = f(&mut self.editor);
        self.changed(Vec::new());
        out
    }

    /// This session's id in its engine.
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// The state at the last change.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Makes the editor follow the session's settings (after creating it, and after each open).
    fn configure(&mut self) {
        let mode = self.editor_mode();
        self.editor.set_mode(mode);
        self.services.document_replaced(self.editor.document());
        self.editor.set_author(&self.author.name, self.author.identity.clone());
        if self.track_changes {
            self.editor.set_tracking(true);
            // Turning tracking on for the session isn't an edit of the document just opened.
            self.editor.mark_saved();
        }
    }

    /// The editor's mode for the session's settings: read only wins over the mode.
    fn editor_mode(&self) -> crate::editing::Mode {
        match (self.read_only, self.mode) {
            (true, _) => crate::editing::Mode::ReadOnly,
            (false, Mode::Editing) => crate::editing::Mode::Editing,
            (false, Mode::Design) => crate::editing::Mode::Design,
        }
    }

    /// Rebuilds the state after an observable change and tells the listener, the state first.
    fn changed(&mut self, events: Vec<Event>) {
        let revisions = self.state.review.revisions;
        let selection = std::mem::take(&mut self.state.selection);
        self.state = self.build_state();
        self.emit(Event::StateChanged(self.state.clone()));
        for e in events {
            let review = match &e {
                Event::DocumentChanged(record) => self.review_events(record, revisions),
                _ => Vec::new(),
            };
            self.emit(e);
            review.into_iter().for_each(|r| self.emit(r));
        }
        if let Some(text) = self.services.announcer.next(&mut self.editor) {
            self.emit(Event::Announcement { text });
        }
        if self.state.selection != selection {
            self.emit(Event::SelectionChanged(self.state.selection.clone()));
        }
        self.deliver_requests();
    }

    fn emit(&self, event: Event) {
        self.shared.emit(self.id, &event);
    }

    /// Lays out the rest of the document: for the answers that give pages.
    fn complete_layout(&mut self) {
        let fonts = self.fonts.fonts();
        self.editor.complete(&fonts);
    }

    /// The state now, one sequence after the last.
    fn build_state(&mut self) -> State {
        let fonts = self.fonts.fonts();
        let flags = super::state::Flags {
            sequence: self.state.sequence + 1,
            mode: self.mode,
            read_only: self.read_only,
            language: &self.language,
            zoom: self.presentation.zoom(),
            gridlines: self.gridlines,
            fonts: &fonts,
        };
        let mut state = super::state::build(&mut self.editor, flags);
        drop(fonts);
        state.fonts = self.watch.state.clone();
        services::fill_state(self, &mut state);
        review::fill_state(self, &mut state);
        host::fill_state(self, &mut state);
        state
    }
}

#[cfg(test)]
mod tests;
