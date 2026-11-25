//! A session: one document being edited, with its selection, history, pipeline, services and presentation. Each
//! submodule holds one area's methods.
#![warn(missing_docs)]

mod blink;
mod commands;
pub(crate) mod documents;
mod edit;
mod elements;
mod fonts;
mod host;
mod input;
mod review;
mod services;

use std::rc::Rc;

pub use documents::{Export, ExportFormat, ExportOptions, OpenToken, PdfOptions};
pub use edit::Commit;
pub(crate) use fonts::unopened_document_fonts;
pub use host::{CaretBlink, Cursor, FrameInfo, FrameStats, HostRequest, HostSurface, OverlayPalette, PaintBudget};
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
    /// The keys follow macOS.
    mac: bool,
    /// The editor's settings for every document: the field settings from the options and the ones commands set since
    /// (each command's last, in [`setup_rank`] order), run on each document before its first layout is seen.
    field_setup: Vec<super::command::Command>,
    /// The layout follows each edit at once (`setViewOptions`'s `liveLayout`).
    live_layout: bool,
    /// Controls and shown sub-templates are labelled (`setViewOptions`'s `controlLabels`).
    control_labels: bool,
    /// The last transaction record's id.
    records: u64,
    /// Table gridlines show where cells have no borders.
    gridlines: bool,
    /// Which formatting marks show (paragraph ends, spaces, tabs, breaks, hidden text, optional hyphens, anchors).
    marks: crate::editing::view::MarkOptions,
    /// Click and Type is on.
    click_and_type: bool,
    /// Selected text can be dragged and dropped.
    text_drags: bool,
    /// The wrap inserted pictures take.
    picture_wrap: crate::api::value::ObjectWrap,
    /// The style Click and Type's paragraphs take; `None` is the default paragraph style.
    click_style: Option<String>,
    /// The text areas' and cells' boundaries show.
    text_boundaries: bool,
    /// When fields are shaded.
    field_shading: crate::api::services::FieldShading,
    /// Bookmarks show.
    show_bookmarks: bool,
    /// Under enforced protection, the regions the author may edit are highlighted.
    show_editable_regions: bool,
    /// Pointer shapes over elements instead of the editor's own.
    cursor_overrides: Vec<crate::api::services::CursorOverride>,
    /// The modifier keys of the last pointer input.
    pointer_mods: crate::editing::keymap::Mods,
    /// The secondary button went down in the selected text: its release opens the drop menu (or, not dragged, the
    /// context menu).
    right_drag: bool,
    /// The element the primary button went down on, for the click that ends the press.
    pressed: Option<crate::docx::ElementId>,
    presentation: host::Presentation,
    watch: fonts::Watch,
    services: crate::services::Services,
    /// Gesture bindings and application shortcuts.
    bindings: input::Bindings,
    /// What typing corrects.
    autocorrect: crate::editing::autocorrect::AutoCorrect,
    /// Tab and Backspace at a paragraph's start set its indents (AutoFormat).
    tab_indents: bool,
    /// The cut, copy and paste options.
    paste_defaults: crate::editing::paste::PasteDefaults,
    /// The caption inserted tables get.
    auto_caption: Option<crate::editing::captions::CaptionOptions>,
}

/// The commands that apply `options`' field settings: rules and types first, as labels depend on them.
fn field_setup(options: &SessionOptions) -> Vec<super::command::Command> {
    use super::command::{ApplicationFieldTypes, Command, FieldLabels, SubTemplateRules};
    let mut out = Vec::new();
    if !options.sub_template_rules.is_empty() {
        out.push(Command::SetSubTemplateRules(SubTemplateRules { rules: options.sub_template_rules.clone() }));
    }
    if !options.application_field_types.is_empty() {
        out.push(Command::SetApplicationFieldTypes(ApplicationFieldTypes { types: options.application_field_types.clone() }));
    }
    if !options.field_labels.is_empty() {
        out.push(Command::SetFieldLabels(FieldLabels { labels: options.field_labels.clone(), merge: None }));
    }
    out
}

/// Where a command that sets the editor's settings for every document goes in the session's setup: rules and types
/// before labels, as labels depend on them. `None` for every other command.
pub(super) fn setup_rank(c: &super::command::Command) -> Option<u8> {
    use super::command::Command;
    match c {
        Command::SetSubTemplateRules(_) => Some(0),
        Command::SetApplicationFieldTypes(_) => Some(1),
        Command::SetFieldLabels(_) => Some(2),
        Command::SetProtectedPictureControls(_) => Some(3),
        Command::SetEditingGroups(_) => Some(4),
        _ => None,
    }
}

impl Session {
    /// Keeps `c`, a setting [`setup_rank`] ranks, for the documents opened after: it replaces the one before, and
    /// labels that merge go into the labels kept.
    pub(super) fn remember_setup(&mut self, c: super::command::Command) {
        use super::command::Command;
        let Some(rank) = setup_rank(&c) else { return };
        let at = self.field_setup.iter().position(|k| setup_rank(k) >= Some(rank));
        let kept = at.filter(|&k| setup_rank(&self.field_setup[k]) == Some(rank));
        match (kept, c) {
            (Some(k), Command::SetFieldLabels(l)) if l.merge == Some(true) => {
                if let Command::SetFieldLabels(was) = &mut self.field_setup[k] {
                    was.labels.extend(l.labels);
                }
            }
            (Some(k), c) => self.field_setup[k] = c,
            (None, c) => self.field_setup.insert(at.unwrap_or(self.field_setup.len()), c),
        }
    }
    pub(crate) fn new(id: SessionId, options: SessionOptions, fonts: FontLibrary, shared: Rc<Shared>) -> Session {
        let editor = documents::blank(&fonts);
        let field_setup = field_setup(&options);
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
            mac: options.mac,
            field_setup,
            live_layout: true,
            control_labels: true,
            records: 0,
            gridlines: false,
            marks: Default::default(),
            click_and_type: true,
            text_drags: true,
            picture_wrap: crate::api::value::ObjectWrap::Inline,
            click_style: None,
            text_boundaries: false,
            field_shading: Default::default(),
            show_bookmarks: false,
            show_editable_regions: true,
            cursor_overrides: Vec::new(),
            pointer_mods: Default::default(),
            pressed: None,
            right_drag: false,
            presentation: host::Presentation::default(),
            watch: fonts::Watch::default(),
            services: crate::services::Services::default(),
            bindings: input::Bindings::default(),
            autocorrect: Default::default(),
            tab_indents: true,
            paste_defaults: Default::default(),
            auto_caption: None,
        };
        for d in &options.dictionaries {
            // shortcut: a dictionary that doesn't parse is left out silently; report it as a warning event if hosts
            // need to know at creation.
            let _ = s.services.add_dictionary(d);
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
        self.services.spelling.forget();
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
        self.editor.set_hyphenation(self.services.hyphenation.clone());
        (self.editor.live_layout, self.editor.control_labels) = (self.live_layout, self.control_labels);
        self.editor.set_mac(self.mac);
        let _ = self.editor.set_mark_options(self.marks);
        self.editor.indent_keys = self.tab_indents;
        self.editor.set_click_and_type(self.click_and_type);
        self.editor.set_text_drag_and_drop(self.text_drags);
        self.editor.set_default_picture_wrap(self.picture_wrap);
        self.editor.set_click_and_type_style(self.click_style.clone());
        self.editor.set_paste_defaults(self.paste_defaults);
        self.editor.set_auto_caption(self.auto_caption.clone());
        let fonts = self.fonts.fonts();
        for c in &self.field_setup {
            self.editor.execute(c.clone(), &fonts);
        }
        self.services.document_replaced();
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
        let editing = std::mem::take(&mut self.state.editing);
        self.state = crate::prof::time("session:state", || self.build_state());
        self.emit(Event::StateChanged(self.state.clone()));
        for e in events {
            let review = match &e {
                Event::DocumentChanged(record) => crate::prof::time("session:review", || self.review_events(record, revisions)),
                _ => Vec::new(),
            };
            self.emit(e);
            review.into_iter().for_each(|r| self.emit(r));
        }
        self.popover_follow();
        let mut said = std::mem::take(&mut self.services.said);
        said.extend(self.services.announcer.next(&mut self.editor, &self.services.spelling, &self.language, self.read_only));
        if self.state.editing != editing {
            self.emit(Event::EditingTargetChanged(self.state.editing.clone()));
            // Screen readers hear where the editor went, first.
            let text = match &self.state.editing {
                crate::api::state::EditingTarget::SubTemplate { name, .. } => format!("Editing sub-template {name}"),
                crate::api::state::EditingTarget::Template => "Back to the template".into(),
            };
            said.insert(0, (crate::api::services::AnnouncementKind::Mode, text));
        }
        self.announce(said);
        if self.state.selection != selection {
            self.emit(Event::SelectionChanged(self.state.selection.clone()));
        }
        self.blink_follow();
        self.deliver_requests();
    }

    /// Says what a change calls for, as one announcement: the kinds the application turned off are left out, and the
    /// announcement's kind is what its first part is about.
    fn announce(&self, said: Vec<(crate::api::services::AnnouncementKind, String)>) {
        let said: Vec<_> = said.into_iter().filter(|s| !self.services.silenced.contains(&s.0)).collect();
        if let Some(&(kind, _)) = said.first() {
            let text = said.into_iter().map(|s| s.1).collect::<Vec<_>>().join(", ");
            self.emit(Event::Announcement { text, kind });
        }
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
        state.theme_colours = self.editor.theme_colours();
        state.theme_fonts = self.editor.theme_fonts();
        services::fill_state(self, &mut state);
        review::fill_state(self, &mut state);
        host::fill_state(self, &mut state);
        state
    }
}

#[cfg(test)]
mod argument_tests;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod paste_and_replace;
#[cfg(test)]
mod protection_tests;
#[cfg(test)]
mod replace_tests;
#[cfg(test)]
mod tests;
