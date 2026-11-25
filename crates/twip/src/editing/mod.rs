//! Editor

pub mod autocorrect;
mod background;
mod bindings;
pub mod bookmarks;
mod borders_dialog;
mod cell_clipboard;
mod char_code;
pub mod click_type;
pub mod clipboard;
mod clipped;
pub mod commands;
pub mod comments;
mod composition;
mod drop_caps;
mod element_feedback;
pub mod elements;
pub mod extend;
mod field_shading;
pub mod find;
mod fonts;
pub mod format;
pub mod forms;
mod frames;
pub mod geometry;
pub(crate) mod gestures;
mod goback;
pub mod history;
pub mod hover;
mod html;
pub mod import;
pub mod index;
pub mod input;
#[cfg(any(test, feature = "inspect", not(target_family = "wasm")))]
pub mod inspect;
pub mod integrity;
pub mod interaction;
pub mod keymap;
pub mod list_defs;
mod list_dialogs;
mod list_gallery;
pub mod list_labels;
pub mod lists;
mod merge;
mod move_paras;
pub mod move_to;
pub(crate) mod multi;
pub mod outline;
pub(crate) use merge::api_id;
mod marquee;
pub mod navigate;
mod notes;
pub mod object_props;
pub mod objects;
pub mod picture_controls;
mod pictures;
pub mod pointer;
pub mod position;
mod preview;
mod properties;
pub mod protection;
pub mod readback;
mod resize;
pub mod revisions;
pub mod section;
pub mod selection;
mod shapes;
mod smart;
pub(crate) mod snap;
pub mod state;
pub mod step;
mod stories;
pub mod style_defs;
mod style_dialog;
mod styles;
pub mod text_links;
pub use styles::formats_a_style;
pub mod sub_templates;
pub use crate::docx::grid as table;
pub mod table_cells;
pub mod table_convert;
pub mod table_draw;
pub mod table_format;
mod table_handles;
pub mod table_readback;
pub mod tables;
pub mod text;
pub mod theme;
mod touched;
pub mod view;
mod wrap_points;

#[cfg(test)]
pub(crate) mod tests;

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::clock::Instant;

use crate::api::record::Origin;
use crate::docx::{Block, BreakKind, Document, Inline, Paragraph, Run};
use crate::font::Fonts;
use crate::layout::{Budget, Change, DocumentLayout, ParaId, Revision, Snapshot, Step as LayoutStep, TableId, Target, Unit};

use commands::{Command, Dir, MoveBy, Outcome};
use geometry::Pages;
use history::{History, SelectionRecord, Transaction, Typing, TypingRun};
use index::{Container, DocIndex};
use input::{PointerKind, Request, Viewport};
pub use merge::{DateSettings, DateStorage, date_formats, date_settings};
use navigate::Nav;
pub use notes::NoteSeparatorInfo;
use position::{Affinity, Place, Position, StoryId};
use selection::{Granularity, Selection};
use step::{Map, Step, StepError, para_len, run_of};
use text::UnitText;
pub use touched::EditMark;
use view::{CaretView, PageStack, SelectionView, View};

pub const IDLE_BUDGET: Duration = Duration::from_millis(4);

/// The comment threads [`Editor::review_counts`] counted, all and shown, and the comment edits and Show Markup they
/// were counted at.
type CommentCounts = (u64, Rc<crate::docx::review::Markup>, (u32, u32));

#[derive(Clone, Copy, Debug)]
struct Drag {
    anchor: Position,
    span: Option<(Position, Position)>,
    unit: Granularity,
    at: (f64, f64),
    block: bool,
    /// Ctrl adds what the drag selects to the selection.
    adding: bool,
    /// Alt: a column block.
    column: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correction {
    pub para: ParaId,
    pub range: std::ops::Range<Unit>,
    pub from: String,
    /// The text the correction put in `range`.
    pub to: String,
    pub typed: usize,
    pub kind: CorrectionKind,
    /// For an indent or border line: the paragraph property (`w:ind`, `w:pBdr`) as AutoFormat wrote it, so a later change
    /// to it ends the correction.
    pub(crate) applied: Option<Option<String>>,
}

/// What an automatic change made, which its AutoCorrect Options offer to undo or stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorrectionKind {
    /// AutoCorrect replaced the text typed (`from`) in the range.
    Text,
    /// AutoFormat made the address typed (`from`) a link: the range is the link's field.
    Hyperlink,
    /// AutoFormat made the characters typed (`from`) the paragraph's bottom border; the paragraph's borders before it, as
    /// written (`w:pBdr`).
    BorderLine { borders: Option<String> },
    /// AutoFormat made the Tab typed at the paragraph's start an indent; the paragraph's indents before it, as written
    /// (`w:ind`).
    Indent { ind: Option<String> },
}

pub struct Editor {
    doc: Document,
    layout: DocumentLayout,
    index: DocIndex,
    history: History,
    selection: Selection,
    /// The page point line moves keep to: each line reads its place along it from the point.
    goal: Option<[f64; 2]>,
    pending: Option<Run>,
    /// The format painter's formatting: a run's `w:rPr`, and a paragraph's `w:pPr` when it took the paragraph's too.
    painter: Option<(Option<String>, Option<Option<String>>)>,
    /// Ctrl+Shift+V pastes the painter's formatting: since Copy Formatting, with the clipboard it first saw then (its
    /// fingerprint), until the clipboard changes.
    painter_claim: Option<Option<u64>>,
    correction: Option<Correction>,
    drag: Option<Drag>,
    pages: Pages,
    near: usize,
    revision: u64,
    generation: u64,
    seq: u64,
    viewport: Viewport,
    body_return: Option<Selection>,
    object_drag: Option<objects::ObjectDrag>,
    object_child: Option<(crate::layout::FloatId, crate::drawing::ChildPath)>,
    object_also: Option<(crate::layout::FloatId, Vec<crate::layout::FloatId>)>,
    shape_drag: Option<shapes::ShapeDrag>,
    /// The selected float's wrap points being edited.
    wrap_edit: Option<wrap_points::WrapEdit>,
    /// The text box whose text Create Link's pointer is to take on.
    linking: Option<crate::layout::FloatId>,
    /// Word's Select Objects is on.
    select_objects: bool,
    /// A Select Objects rectangle being dragged.
    marquee: Option<marquee::Marquee>,
    /// The selected float's anchor being dragged.
    anchor_drag: Option<marquee::AnchorDrag>,
    /// The number the last new object's name took (0: not yet counted), as Word numbers them.
    object_number: u32,
    /// The objects the last Ungroup made, for Regroup.
    ungrouped: Option<Vec<crate::layout::FloatId>>,
    /// The wrap a picture is inserted with (Word's "Insert/paste pictures as").
    default_picture_wrap: crate::api::value::ObjectWrap,
    snaps: snap::SnapLines,
    shape_insert: Option<shapes::ShapeInsert>,
    toggled_checkbox: Option<objects::ToggledCheckbox>,
    provisional: Option<stories::Provisional>,
    composition: Option<composition::Composition>,
    border_drag: Option<resize::BorderDrag>,
    /// The line the Draw Table pen draws.
    table_pen: crate::docx::props::Border,
    /// Tables laid out as a live drag would leave them, in place of the model's.
    table_preview: Vec<resize::Provisional>,
    table_drag: Option<table_handles::TableDrag>,
    /// A pointer press, move or release is running: its edits are where the pointer is, so they don't scroll to the
    /// caret.
    pointing: bool,
    /// Edits leave the caret's page unlaid until the last of several undone or redone in one go.
    deferring: bool,
    /// The sections section commands act on, over the selection's.
    section_scope: Option<std::ops::RangeInclusive<usize>>,
    table_hover: Option<view::TableHover>,
    zone_drag: Option<(TableId, selection::CellRef, selection::CellShape)>,
    cursor: input::Cursor,
    saved: (u64, [bool; 3]),
    origin: Origin,
    batch: Option<Batch>,
    commits: Vec<Committed>,
    requests: Vec<Request>,
    repeat: Option<Command>,
    modes: extend::Modes,
    /// The other ranges of a discontiguous selection.
    multi: multi::Multi,
    /// The other ranges an edit that replaces the selection set aside, to put back should the edit change nothing.
    set_aside: Option<multi::Multi>,
    /// The ranges selected before a Ctrl click or drag that adds one, while its clicks go on.
    adding: Option<multi::Adding>,
    mac: bool,
    edit_marks: Vec<Position>,
    back_step: Option<usize>,
    back_from: Option<Position>,
    last_maps: Vec<step::Map>,
    text_drag: Option<interaction::TextDrag>,
    /// A drag out of the editor: the selection, the revision and the blocks it carried.
    drag_out: Option<(Selection, u64, Vec<crate::docx::Block>)>,
    text_drag_off: bool,
    mark_options: view::MarkOptions,
    click_and_type_off: bool,
    white_space_hidden: bool,
    click_and_type_style: Option<String>,
    clipboard: Option<clipboard::Fragment>,
    /// The document's identity among those open: what tells a fragment copied from it from one copied elsewhere.
    key: u64,
    /// The find match selected last, with the selection that selected it.
    found: Option<(Selection, ParaId, Unit, Unit)>,
    tab_stop: Option<(Selection, TableId, (usize, usize))>,
    mode: Mode,
    pub live_layout: bool,
    /// AutoFormat's "set left- and first-indent with tabs and backspaces": Tab and Backspace at a paragraph's start
    /// change its indents.
    pub indent_keys: bool,
    /// F2 or Shift+F2 waiting for its place.
    move_to: Option<move_to::MoveTo>,
    /// Where the open drop menu of a right-button drag drops the text, the selection it drags and the revision it
    /// opened at.
    drop_menu: Option<(Position, Selection, u64)>,
    /// The highlighter's colour, while it is on.
    highlighter: Option<crate::api::value::HighlightColor>,
    pub view_settings: view::ViewSettings,
    elems: std::collections::HashMap<crate::docx::ElementId, elements::Element>,
    changes: elements::Changes,
    refused: Option<&'static str>,
    events: Vec<input::Notice>,
    made: Option<crate::api::command::CommandResult>,
    pending_controls: Vec<(crate::docx::ControlId, crate::docx::Control)>,
    pending_images: Vec<Vec<u8>>,
    /// The editable ranges as of a [`Editor::perm_generation`] ([`Editor::editable_ranges`]).
    editable_cache: std::cell::RefCell<Option<(u64, Rc<protection::EditableRanges>)>>,
    /// Counts the steps that can have changed the editable ranges.
    perm_generation: u64,
    /// The groups the author is in, for editable ranges ([`Editor::set_editing_groups`]).
    editing_groups: Vec<String>,
    drop: Option<Position>,
    field_labels: HashMap<String, String>,
    sub_template_rules: Vec<sub_templates::Rule>,
    touched: touched::Touched,
    /// Counts the transactions that touched revision marks, tables, sections or sub-templates, and those that
    /// touched paragraphs that hold marks, are in cells or went.
    revision_marks: (u64, u64),
    /// Counts the transactions that touched comments or their marks.
    comment_edits: u64,
    /// The application's field types, upper case.
    application_field_types: Vec<String>,
    /// Tag prefixes whose picture controls keep their pictures.
    protected_prefixes: Vec<String>,
    /// A fill or a control's settings are changing pictures: protected ones may change.
    picture_writes: bool,
    /// The clicks of the press under way.
    press_clicks: u8,
    /// The picture control the last single click activated.
    click_activated: Option<crate::docx::ControlId>,
    /// The press selects a picture control without activating it: its gesture is rebound.
    activation_suppressed: bool,
    references: sub_templates::References,
    collapsed: std::collections::HashSet<crate::docx::FieldId>,
    warned: std::collections::HashSet<(crate::docx::diagnostics::WarningCode, String)>,
    expansions: Option<Rc<crate::layout::expansion::Expansions>>,
    expansions_stale: bool,
    surface: Option<sub_templates::Surface>,
    marks: Vec<merge::Mark>,
    update_token: u64,
    hover: Option<(f64, f64)>,
    /// The [`Editor::generation`] when the pointer last moved: a tooltip hides once the document changes.
    hover_generation: u64,
    /// The last hover tooltip looked up, and what for.
    hover_cache: Option<(hover::HoverKey, Option<hover::Hovered>)>,
    #[cfg(test)]
    hover_lookups: usize,
    #[cfg(test)]
    balloon_lookups: usize,
    #[cfg(test)]
    mark_builds: usize,
    /// A style's XML that formatting commands edit instead of the selection ([`Editor::format_style`]).
    style_target: Option<String>,
    /// The table style part formatting commands edit, when they edit a table style.
    style_part: Option<&'static str>,
    /// The list numbers a click selected.
    list_label: Option<list_labels::LabelHold>,
    /// A list level's number formatting that character formatting commands edit instead of the selection
    /// ([`Editor::format_list_label`]).
    label_target: Option<Box<list_labels::LabelTarget>>,
    /// The style target is the document defaults: formatted alone.
    defaults_target: bool,
    /// The lists applied or defined in this session, newest first, by gallery.
    recent_lists: Vec<(crate::api::services::ListGalleryKind, Vec<crate::api::command::ListLevel>)>,
    hover_region: Option<crate::docx::ElementId>,
    /// Controls and shown sub-templates are labelled while selected or hovered (`setViewOptions`'s `controlLabels`).
    pub control_labels: bool,
    comments: comments::State,
    review_list: revisions::index::List,
    /// The revision marks by paragraph as of a [`Editor::revision_marks`] and the sub-template edited, followed edit
    /// by edit: what balloons and the review counts are worked out from.
    review_marks: Option<((u64, u64), Option<u32>, revisions::marks::Marks)>,
    /// The visible pages' paragraphs, the blank paragraphs next to them (with the paragraph their balloons anchor at
    /// and whether at its end) and the revisions starting in them, kept for the next frame's balloons.
    balloon_scratch: (Vec<ParaId>, Vec<(ParaId, ParaId, bool)>, Vec<revisions::marks::Start>),
    /// The comment threads counted.
    counted: Option<CommentCounts>,
    author: (Rc<str>, Option<crate::docx::review::Identity>),
    author_id: Option<crate::docx::review::AuthorId>,
    move_name: Option<Rc<str>>,
    recorded: (Vec<Rc<crate::docx::review::RevisionMark>>, Vec<Rc<crate::docx::review::RevisionMark>>),
    cut_move: Option<(u64, Vec<Rc<crate::docx::review::RevisionMark>>)>,
    #[cfg(test)]
    pub(crate) track_work: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Editing,
    Design,
    ReadOnly,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "editing" => Some(Mode::Editing),
            "design" => Some(Mode::Design),
            "readonly" | "read-only" => Some(Mode::ReadOnly),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Editing => "editing",
            Mode::Design => "design",
            Mode::ReadOnly => "readonly",
        }
    }
}

struct Tx {
    undo: Vec<Step>,
    hints: Vec<Option<Change>>,
    before: SelectionRecord,
    label: &'static str,
    stale_paras: Vec<ParaId>,
    stale_tables: Vec<TableId>,
    kept: i64,
    maps: Vec<step::Map>,
    tracked: bool,
    rec: Option<revisions::Recording>,
    origin: Origin,
    replayed: Option<Transaction>,
    targets: step::Targets,
    pictures: bool,
    /// A text step put in an inline holding a story (a text box, an inline shape's text).
    boxes: bool,
    /// List definitions changed: every label is made again, not only those whose text changed.
    relabel: bool,
    /// The control whose placeholder the selection replaces: its paragraph and unit, where what is put in goes inside it.
    placeholder: Option<(ParaId, Unit, crate::docx::ControlId)>,
}

struct Batch {
    tx: Tx,
    label: Option<Cow<'static, str>>,
    marks: Vec<(usize, Selection)>,
}

#[derive(Clone, Debug)]
pub struct Committed {
    pub revision: u64,
    pub origin: Origin,
    pub label: Cow<'static, str>,
    pub targets: step::Targets,
}

impl Editor {
    pub fn open(bytes: &[u8], fonts: &Fonts) -> Result<Editor, String> {
        let doc = crate::docx::read_editable(bytes)?;
        Ok(Editor::with_document(doc, fonts))
    }

    pub fn with_document(doc: Document, fonts: &Fonts) -> Editor {
        let index = DocIndex::build(&doc);
        let doc_hides_white_space = doc.hides_white_space;
        let mut e = Editor {
            doc,
            layout: DocumentLayout::new(),
            index,
            history: History::default(),
            selection: Selection::Caret(Position::text(StoryId::Body, ParaId(0), Unit(0))),
            goal: None,
            pending: None,
            painter: None,
            painter_claim: None,
            correction: None,
            drag: None,
            pages: Pages::default(),
            near: 0,
            revision: 1,
            generation: 1,
            seq: 0,
            viewport: Viewport::default(),
            body_return: None,
            object_drag: None,
            object_child: None,
            object_also: None,
            shape_drag: None,
            wrap_edit: None,
            object_number: 0,
            linking: None,
            select_objects: false,
            marquee: None,
            anchor_drag: None,
            ungrouped: None,
            default_picture_wrap: crate::api::value::ObjectWrap::Inline,
            snaps: snap::SnapLines::default(),
            shape_insert: None,
            toggled_checkbox: None,
            provisional: None,
            composition: None,
            border_drag: None,
            table_pen: table_draw::default_pen(),
            table_preview: Vec::new(),
            table_drag: None,
            pointing: false,
            deferring: false,
            section_scope: None,
            table_hover: None,
            zone_drag: None,
            cursor: input::Cursor::Text,
            saved: (0, [false; 3]),
            origin: Origin::User,
            batch: None,
            commits: Vec::new(),
            requests: Vec::new(),
            repeat: None,
            modes: extend::Modes::default(),
            multi: multi::Multi::default(),
            set_aside: None,
            adding: None,
            mac: false,
            edit_marks: Vec::new(),
            back_step: None,
            back_from: None,
            last_maps: Vec::new(),
            text_drag: None,
            drag_out: None,
            text_drag_off: false,
            mark_options: view::MarkOptions::default(),
            click_and_type_off: false,
            white_space_hidden: doc_hides_white_space,
            click_and_type_style: None,
            tab_stop: None,
            mode: Mode::Editing,
            live_layout: true,
            indent_keys: true,
            move_to: None,
            drop_menu: None,
            highlighter: None,
            view_settings: view::ViewSettings::default(),
            elems: std::collections::HashMap::new(),
            changes: elements::Changes::default(),
            refused: None,
            events: Vec::new(),
            made: None,
            pending_controls: Vec::new(),
            pending_images: Vec::new(),
            editable_cache: Default::default(),
            perm_generation: 0,
            editing_groups: Vec::new(),
            touched: touched::Touched::new(),
            revision_marks: (0, 0),
            comment_edits: 0,
            drop: None,
            field_labels: HashMap::new(),
            sub_template_rules: Vec::new(),
            application_field_types: Vec::new(),
            protected_prefixes: Vec::new(),
            picture_writes: false,
            press_clicks: 0,
            click_activated: None,
            activation_suppressed: false,
            references: sub_templates::References::default(),
            collapsed: std::collections::HashSet::new(),
            warned: std::collections::HashSet::new(),
            expansions: None,
            expansions_stale: false,
            surface: None,
            marks: Vec::new(),
            update_token: 0,
            hover: None,
            hover_generation: 0,
            hover_cache: None,
            #[cfg(test)]
            hover_lookups: 0,
            #[cfg(test)]
            balloon_lookups: 0,
            #[cfg(test)]
            mark_builds: 0,
            style_target: None,
            style_part: None,
            list_label: None,
            label_target: None,
            defaults_target: false,
            recent_lists: Vec::new(),
            hover_region: None,
            control_labels: true,
            comments: comments::State::default(),
            review_list: Default::default(),
            counted: None,
            review_marks: None,
            balloon_scratch: Default::default(),
            author: ("Author".into(), None),
            author_id: None,
            move_name: None,
            recorded: (Vec::new(), Vec::new()),
            cut_move: None,
            #[cfg(test)]
            track_work: 0,
            clipboard: None,
            key: import::next_key(),
            found: None,
        };
        e.saved.1 = e.tracking_settings();
        e.elems = elements::by_id(&e.doc, &e.index);
        let start = e.nav().story_edge(StoryId::Body, false).unwrap_or(Position::text(StoryId::Body, ParaId(0), Unit(0)));
        e.selection = Selection::Caret(start);
        e.layout.edit(Revision(e.generation), None);
        e.ensure(&start, fonts);
        e
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn index(&self) -> &DocIndex {
        &self.index
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Where the log of touched paragraphs is now.
    pub fn edit_mark(&self) -> EditMark {
        self.touched.mark()
    }

    /// The paragraphs edited, or whose text was otherwise changed, since `mark` (repeats included); `None` when the
    /// log no longer reaches back to it.
    pub fn touched_since(&self, mark: EditMark) -> Option<&[ParaId]> {
        self.touched.since(mark)
    }

    /// Two counts: the first changes whenever a transaction touches revision marks, tables, sections or
    /// sub-templates (until then, a document without revisions has none); the second whenever one touches what can
    /// join or part revisions, paragraphs that hold marks, are in cells or went (until then, the revisions are as many).
    pub fn revision_marks(&self) -> (u64, u64) {
        self.revision_marks
    }

    pub fn seq_applied(&self) -> u64 {
        self.seq
    }

    pub fn is_dirty(&self) -> bool {
        (self.history.position(), self.tracking_settings()) != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.history.break_typing();
        self.saved = (self.history.position(), self.tracking_settings());
    }

    /// The saved settings outside the history: tracking, and whether moves and formatting are tracked.
    fn tracking_settings(&self) -> [bool; 3] {
        let s = &self.doc.settings;
        [s.track_revisions, s.do_not_track_moves, s.do_not_track_formatting]
    }

    /// Whether tracking records moves as moves (Track moves) and formatting changes (Track formatting): document
    /// settings, saved, outside the history as tracking is.
    pub fn track_options(&self) -> (bool, bool) {
        let s = &self.doc.settings;
        (!s.do_not_track_moves, !s.do_not_track_formatting)
    }

    /// Sets Track moves and Track formatting; turning either off is refused while protection forces tracking.
    pub fn set_track_options(&mut self, moves: bool, formatting: bool) -> Outcome {
        if !(moves && formatting) && self.tracking_locked() {
            return Outcome::unavailable("protection forces tracking");
        }
        if self.track_options() == (moves, formatting) {
            return Outcome::Unchanged("already so");
        }
        let s = &mut self.doc.settings;
        (s.do_not_track_moves, s.do_not_track_formatting) = (!moves, !formatting);
        Outcome::Moved
    }

    /// The track options recording follows: both on while protection forces tracking, whatever the settings say.
    pub(crate) fn recorded_options(&self) -> (bool, bool) {
        if self.tracking_locked() { (true, true) } else { self.track_options() }
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn set_markup_view(&mut self, view: crate::docx::review::MarkupView) -> Outcome {
        if self.doc.review.view == view {
            return Outcome::Unchanged("already in that view");
        }
        self.doc.review.view = view;
        self.relayout_review();
        Outcome::Moved
    }

    /// Sets the markup's view settings past the markup view, laying the document out again when they change.
    fn set_markup(&mut self, change: impl FnOnce(&mut crate::docx::review::Markup), unchanged: &'static str) -> Outcome {
        let mut m = (*self.doc.review.markup).clone();
        change(&mut m);
        if m == *self.doc.review.markup {
            return Outcome::Unchanged(unchanged);
        }
        self.doc.review.markup = Rc::new(m);
        self.relayout_review();
        Outcome::Moved
    }

    /// Which kinds of markup show (Word's Show Markup: Insertions and Deletions, Formatting, Comments). Revisions and
    /// comments of a kind left out show as if accepted, and are left out of what the review pane lists as shown.
    pub fn set_show_markup(&mut self, insertions_deletions: bool, formatting: bool, comments: bool) -> Outcome {
        self.set_markup(
            |m| (m.show.insertions_deletions, m.show.formatting, m.show.comments) = (insertions_deletions, formatting, comments),
            "already shown so",
        )
    }

    /// The reviewers whose markup does not show (Show Markup, Specific People), by name: every author of that name
    /// the document has now (one added later under that name shows). Empty: all reviewers show. A name no author of
    /// the document has is refused.
    pub fn set_hidden_reviewers(&mut self, names: &[&str]) -> Outcome {
        let persons = &self.doc.people.persons;
        if names.iter().any(|n| !persons.iter().any(|p| *p.name == **n)) {
            return Outcome::invalid("no such reviewer");
        }
        let hidden: Vec<crate::docx::review::AuthorId> = (0..persons.len() as u32)
            .filter(|&i| names.iter().any(|n| *persons[i as usize].name == **n))
            .map(crate::docx::review::AuthorId)
            .collect();
        self.set_markup(|m| m.show.hidden_reviewers = hidden, "already shown so")
    }

    /// Where revisions show in All Markup: in balloons beside the page or in place.
    pub fn set_balloon_mode(&mut self, mode: crate::docx::review::BalloonMode) -> Outcome {
        self.set_markup(|m| m.balloons = mode, "already in that mode")
    }

    /// How revisions draw: each kind's mark and colour, change bars and cell highlights. Hidden, caret and pound
    /// marks are for deleted and moved-away text only (as Word offers them): elsewhere refused.
    pub fn set_markup_style(&mut self, style: crate::docx::review::MarkupStyle) -> Outcome {
        if [style.insertions, style.formatting, style.moved_to].iter().any(|l| l.mark.hides_text()) {
            return Outcome::invalid("that mark is for deletions");
        }
        self.set_markup(|m| m.style = style, "already drawn so")
    }

    /// Which formatting marks show. Showing hidden text or optional hyphens lays the document out again with them.
    pub fn set_mark_options(&mut self, o: view::MarkOptions) -> Outcome {
        if self.mark_options == o {
            return Outcome::Unchanged("already shown");
        }
        self.mark_options = o;
        let (hidden, hyphens) = (o.shows_hidden_text(), o.shows_optional_hyphens());
        if (self.doc.review.show_hidden, self.doc.review.show_optional_hyphens) != (hidden, hyphens) {
            (self.doc.review.show_hidden, self.doc.review.show_optional_hyphens) = (hidden, hyphens);
            self.relayout_review();
        }
        Outcome::Moved
    }

    /// Whether the editor's own layout is the document as printed: no markup, hidden text and optional hyphens as
    /// `hidden_text` (Word's Print hidden text option) has them, no shown sub-templates.
    pub fn lays_out_as_printed(&self, hidden_text: bool) -> bool {
        let r = &self.doc.review;
        r.view == crate::docx::review::MarkupView::NoMarkup
            && r.show_hidden == hidden_text
            && !r.show_optional_hyphens
            && !self.shows_expansions()
            && self.editing_sub_template().is_none()
            && self.table_preview.is_empty()
    }

    /// The document as printed, to lay out on its own when [`Editor::lays_out_as_printed`] is false: no markup, hidden
    /// text only with `hidden_text`, optional hyphens only where a line breaks at them.
    pub fn printed_document(&self, hidden_text: bool) -> crate::docx::Document {
        let mut doc = self.doc.clone();
        doc.review.view = crate::docx::review::MarkupView::NoMarkup;
        (doc.review.show_hidden, doc.review.show_optional_hyphens) = (hidden_text, false);
        doc
    }

    /// Which formatting marks show.
    pub fn mark_options(&self) -> view::MarkOptions {
        self.mark_options
    }

    /// The formatting marks on `pages` that the mark options show.
    pub fn shown_marks(&mut self, pages: std::ops::Range<usize>) -> Vec<view::FormattingMark> {
        self.formatting_marks_with(pages, self.mark_options)
    }

    pub fn set_author_color(&mut self, author: crate::docx::review::AuthorId, color: u32) -> Outcome {
        if self.doc.review.palette.color(author) == color {
            return Outcome::Unchanged("already that colour");
        }
        Rc::make_mut(&mut self.doc.review.palette).chosen.insert(author, color);
        self.relayout_review();
        Outcome::Moved
    }

    /// The document changed outside steps, anywhere: what was worked out from the edits so far starts again.
    fn changed_everywhere(&mut self) {
        self.touched.everything();
        self.revision_marks.0 += 1;
    }

    fn relayout_review(&mut self) {
        self.relayout([None]);
    }

    fn preview_layout(&mut self, f: crate::layout::FloatId, preview: Option<crate::layout::Preview>, before: Option<ParaId>) {
        let mut paras: Vec<ParaId> = [self.index.float_anchor(f), before, preview.as_ref().map(|p| p.anchor)].into_iter().flatten().collect();
        paras.dedup();
        let hints: Vec<Option<Change>> = paras.into_iter().map(|p| step::para_hint(&self.index, p, 0)).collect();
        if self.layout.preview(Revision(self.generation + 1), preview, &hints) {
            self.generation += 1;
        }
    }

    /// Lays `tables` out in place of the model's tables they copy, until the next edit or preview; the ones laid out
    /// so before are laid out from the model again.
    fn set_table_preview(&mut self, tables: Vec<resize::Provisional>) {
        if self.table_preview == tables {
            return;
        }
        let mut parts = false;
        let mut hints: Vec<Option<Change>> = Vec::new();
        for p in self.table_preview.iter().chain(&tables) {
            match step::table_hint(&self.index, p.id, p.row) {
                None if self.index.table_slot(p.id).is_some_and(|s| step::in_header_footer(&self.index, s.container)) => parts = true,
                h => hints.push(h),
            }
        }
        hints.dedup();
        self.table_preview = tables;
        self.relayout_parts(hints, parts);
    }

    /// Swaps the previewed tables with the model's, in (outermost first) or back out (`back`, innermost first).
    fn swap_table_preview(&mut self, back: bool) {
        let mut preview = std::mem::take(&mut self.table_preview);
        let mut swap = |p: &mut resize::Provisional| {
            if let Some(t) = self.index.table_mut(&mut self.doc, p.id) {
                std::mem::swap(t, &mut p.table);
            }
        };
        match back {
            false => preview.iter_mut().for_each(&mut swap),
            true => preview.iter_mut().rev().for_each(&mut swap),
        }
        self.table_preview = preview;
    }

    fn relayout(&mut self, hints: impl IntoIterator<Item = Option<Change>>) {
        self.relayout_parts(hints, false);
    }

    /// [`Editor::relayout`], with header or footer text changed too (`parts`).
    fn relayout_parts(&mut self, hints: impl IntoIterator<Item = Option<Change>>, parts: bool) {
        self.generation += 1;
        // A transaction that lays nothing out (a font rename's tables) can still change what layout reads.
        self.layout.document_changed();
        for h in hints {
            self.layout.edit(Revision(self.generation), h);
        }
        if parts {
            self.layout.edit_parts(Revision(self.generation));
        }
    }

    pub fn with_origin<T>(&mut self, origin: Origin, f: impl FnOnce(&mut Editor) -> T) -> T {
        let was = std::mem::replace(&mut self.origin, origin);
        let out = f(self);
        self.origin = was;
        out
    }

    pub fn take_commits(&mut self) -> Vec<Committed> {
        std::mem::take(&mut self.commits)
    }

    pub fn fonts_changed(&mut self) {
        self.relayout_environment();
    }

    /// The hyphenation patterns automatic hyphenation uses.
    pub fn set_hyphenation(&mut self, h: Option<crate::layout::hyphenate::Hyphenation>) {
        if self.doc.hyphenation != h {
            self.doc.hyphenation = h;
            self.relayout_environment();
        }
    }

    fn pending_image(&mut self, data: Vec<u8>) -> usize {
        self.pending_images.push(data);
        self.doc.images.len() + self.pending_images.len() - 1
    }

    fn refuse_read_only(&mut self) -> Outcome {
        self.refused = Some("read-only");
        Outcome::Refused(crate::api::error::Error::new(crate::api::error::ErrorCode::ReadOnly, "the document is read only"))
    }

    pub fn snapshot(&self) -> Rc<Snapshot> {
        self.layout.snapshot()
    }

    pub fn stack(&self) -> PageStack {
        let snapshot = self.layout.snapshot();
        if !self.white_space_hidden {
            return PageStack::of(&snapshot);
        }
        let (info, pages) = (snapshot.info(), snapshot.pages());
        let margins = |i: usize| {
            let Some(s) = info.get(i).and_then(|p| self.doc.sections.get(p.section)) else { return (0.0, 0.0) };
            match pages.get(i).and_then(|p| Some((p.h, p.body_area?))) {
                Some((h, (top, bottom))) if crate::layout::vertical::of_section(s.text_flow).is_none() => (top, h - bottom),
                _ => (s.top.abs(), s.bottom.abs()),
            }
        };
        PageStack::hiding_white_space(&snapshot, margins)
    }

    /// Hides or shows the white space between pages: above and below each page's body, with the headers and footers.
    pub fn set_white_space_hidden(&mut self, hidden: bool) -> Outcome {
        if self.white_space_hidden == hidden {
            return Outcome::Unchanged("already so");
        }
        if hidden {
            self.close_header_footer();
        }
        self.white_space_hidden = hidden;
        self.hover_cache = None;
        Outcome::Moved
    }

    /// Whether the white space between pages is hidden.
    pub fn white_space_hidden(&self) -> bool {
        self.white_space_hidden
    }

    /// Puts back a selection [`Editor::selection`] gave, as it was before an edit that was refused.
    pub fn restore_selection(&mut self, s: Selection) {
        self.set_selection(s);
    }

    pub fn set_caret_for_benchmark(&mut self, s: Selection) {
        self.history.break_typing();
        self.goal = None;
        self.selection = s;
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    fn nav(&mut self) -> Nav<'_> {
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        Nav { doc: &self.doc, idx: &self.index, pages: &mut self.pages, near: self.near }
    }

    // Layout coordination

    fn ensure(&mut self, pos: &Position, fonts: &Fonts) {
        self.sync_expansions(fonts);
        if let Some((para, unit)) = pos.para_offset() {
            // Body text, a text box in it included, by its block; headers, footers and notes by the caret's page.
            let slot = self.index.para_slot(para);
            let target = slot
                .and_then(|s| self.index.top_block(s.container, s.block))
                .map_or(Target::Page(self.near), |(section, block, row)| Target::Block { section, block, row });
            crate::prof::time("editor:layout", || self.with_flow(|l, d| l.through(d, fonts, target)));
            let in_note = slot.is_some_and(|s| matches!(self.index.host(self.index.story(s.container)), StoryId::Footnote(_) | StoryId::Endnote(_)));
            if let Target::Page(mut page) = target
                && in_note
            {
                // A note moves with its reference's line and continues onto the next page.
                while !self.laid_out_shows(para, unit, pos.affinity(), page)
                    && matches!(self.layout.snapshot().status(), crate::layout::LayoutStatus::Pending { .. })
                {
                    page += 1;
                    crate::prof::time("editor:layout", || self.with_flow(|l, d| l.through(d, fonts, Target::Page(page))));
                }
            }
        }
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
    }

    /// Whether a page laid out through `last` has the line holding `u` of `para`: from the one before `near` on, then
    /// the pages before that, where a note chosen from far on has its line.
    fn laid_out_shows(&mut self, para: ParaId, u: Unit, affinity: Affinity, last: usize) -> bool {
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        let from = self.near.saturating_sub(1);
        (from..=last).chain(0..from).any(|i| {
            self.pages
                .page(i, &self.index)
                .is_some_and(|g| g.line_at(para, u, affinity).is_some_and(|l| g.lines[l].last_of(para).is_some_and(|end| u <= end)))
        })
    }

    pub fn ensure_page(&mut self, page: usize, fonts: &Fonts) {
        self.sync_expansions(fonts);
        let snapshot = self.layout.snapshot();
        let stale = snapshot.revision() != self.layout.revision();
        let pending = if stale { Some(0) } else { snapshot.info().iter().position(|i| i.provisional) };
        if pending.is_some_and(|p| page >= p) {
            self.with_flow(|l, d| l.through(d, fonts, Target::Page(page)));
        }
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
    }

    pub fn idle(&mut self, fonts: &Fonts) -> bool {
        self.sync_expansions(fonts);
        let t = Instant::now();
        let step = self.with_flow(|l, d| l.run(d, fonts, Budget::Until(t + IDLE_BUDGET)));
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        step == LayoutStep::Yielded || snapshot.info().iter().any(|i| i.provisional)
    }

    pub fn ensure_current(&mut self, fonts: &Fonts) {
        if self.layout.snapshot().revision() == self.layout.revision() {
            return;
        }
        match self.selection.focus().filter(|p| p.para_offset().is_some()) {
            Some(p) => self.ensure(&p, fonts),
            None => self.ensure_page(0, fonts),
        }
    }

    pub fn layout_complete(&self) -> bool {
        let snapshot = self.layout.snapshot();
        snapshot.revision() == self.layout.revision() && !matches!(snapshot.status(), crate::layout::LayoutStatus::Pending { .. })
    }

    pub fn complete(&mut self, fonts: &Fonts) {
        self.sync_expansions(fonts);
        self.with_flow(|l, d| l.complete(d, fonts));
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
    }

    // Inputs

    pub fn select_between(&mut self, anchor: (ParaId, Unit), focus: (ParaId, Unit), fonts: &Fonts) -> Outcome {
        let place = |(para, u): (ParaId, Unit)| {
            let slot = self.index.para_slot(para)?;
            let len = self.index.para(&self.doc, para).map_or(Unit(0), step::para_len);
            Some(Position::text(self.index.story(slot.container), para, Unit(u.0.min(len.0))))
        };
        let (Some(a), Some(f)) = (place(anchor), place(focus)) else { return Outcome::Unchanged("no paragraph") };
        if a.story != f.story {
            return Outcome::Unchanged("not in one story");
        }
        self.history.break_typing();
        self.set_selection(Selection::range(a, f, selection::Granularity::Char));
        if let Some(p) = self.caret() {
            self.ensure(&p, fonts);
            self.scroll_to_caret(fonts);
        }
        Outcome::Moved
    }

    pub fn select_range(&mut self, para: ParaId, a: Unit, b: Unit, fonts: &Fonts) -> Outcome {
        let Some(slot) = self.index.para_slot(para) else { return Outcome::Unchanged("no paragraph") };
        let story = self.index.story(slot.container);
        let len = self.index.para(&self.doc, para).map_or(Unit(0), step::para_len);
        let (a, b) = (Unit(a.0.min(len.0)), Unit(b.0.min(len.0)));
        self.history.break_typing();
        self.set_selection(Selection::range(Position::text(story, para, a), Position::text(story, para, b), selection::Granularity::Char));
        if let Some(p) = self.caret() {
            self.ensure(&p, fonts);
            self.scroll_to_caret(fonts);
        }
        Outcome::Moved
    }

    /// Selects a floating object.
    pub fn select_float(&mut self, f: crate::layout::FloatId) -> Outcome {
        if self.float_of(f).is_none() {
            return Outcome::Unchanged("no such object");
        }
        self.history.break_typing();
        self.set_selection(Selection::Object(selection::ObjectRef::Floating(f)));
        Outcome::Moved
    }

    /// The keys follow macOS (see [`super::keymap::command`]).
    pub fn set_mac(&mut self, mac: bool) {
        self.mac = mac;
    }

    pub fn set_mode(&mut self, m: Mode) -> Outcome {
        let design = (self.mode == Mode::Design) != (m == Mode::Design);
        self.mode = m;
        if design {
            self.refresh_labels(None);
        }
        Outcome::Moved
    }

    pub fn take_notices(&mut self) -> Vec<input::Notice> {
        std::mem::take(&mut self.events)
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn tracking(&self) -> bool {
        self.doc.settings.track_revisions || self.tracking_locked()
    }

    fn tracking_locked(&self) -> bool {
        self.doc.settings.protection.as_ref().is_some_and(|p| p.forces_tracking())
    }

    pub fn set_tracking(&mut self, on: bool) -> bool {
        if !on && self.tracking_locked() {
            return false;
        }
        self.doc.settings.track_revisions = on;
        true
    }

    pub fn set_author(&mut self, name: &str, identity: Option<crate::docx::review::Identity>) {
        self.author = (name.into(), identity);
        self.author_id = None;
    }

    fn session_author(&mut self) -> crate::docx::review::AuthorId {
        if let Some(a) = self.author_id {
            return a;
        }
        let a = self.doc.people.author(&self.author.0, self.author.1.as_ref());
        self.author_id = Some(a);
        a
    }

    fn track_elements(&mut self, inverses: &[Step]) {
        use std::collections::HashSet;
        let mut now: Vec<elements::Element> = Vec::new();
        let mut gone: Vec<crate::docx::ElementId> = Vec::new();
        match elements::touched(inverses) {
            elements::Touched::Paras(paras, removed) => {
                if paras.is_empty() && removed.is_empty() {
                    return;
                }
                for p in paras {
                    now.extend(elements::in_para(&self.doc, &self.index, p));
                }
                let here: HashSet<crate::docx::ElementId> = now.iter().map(|e| e.id).collect();
                gone.extend(removed.into_iter().filter(|id| !here.contains(id) && self.index.element(*id).is_none()));
            }
            elements::Touched::All => {
                let all = elements::enumerate(&self.doc, &self.index, &elements::Filter::default());
                let here: HashSet<crate::docx::ElementId> = all.iter().map(|e| e.id).collect();
                gone.extend(self.elems.keys().filter(|k| !here.contains(k)).copied());
                now.extend(all);
            }
        }
        #[cfg(test)]
        {
            self.track_work += now.len() + gone.len();
        }
        let mut c = elements::Changes::default();
        for e in now {
            match self.elems.get(&e.id) {
                None => c.added.push(e.clone()),
                Some(old) if old.signature() != e.signature() => c.changed.push(e.clone()),
                Some(_) => {}
            }
            self.elems.insert(e.id, e);
        }
        for id in gone {
            if self.elems.remove(&id).is_some() {
                c.removed.push(id);
            }
        }
        self.changes.then(c);
    }

    fn focus_chain_depth(&self) -> usize {
        self.selection.focus().map_or(0, |p| self.index.chain_of(&p).len())
    }

    fn normalize(&self, s: Selection) -> Selection {
        let snap = |p: Position, end: bool| match p.place {
            Place::Text { para, offset, affinity } => match self.index.para(&self.doc, para).map(UnitText::of) {
                Some(t) => Position { place: Place::Text { para, offset: t.widen(offset, end), affinity }, ..p },
                None => p,
            },
            Place::RowEnd { .. } => p,
        };
        match s {
            Selection::Caret(p) => Selection::Caret(snap(p, false)),
            Selection::Text { anchor, focus, unit } => {
                let forward = self.index.cmp(&anchor, &focus) != std::cmp::Ordering::Greater;
                let (anchor, focus) = (snap(anchor, !forward), snap(focus, forward));
                let (a, b) = if forward { (anchor, focus) } else { (focus, anchor) };
                let (a, b) = self.widen_over_codes(a, b);
                if forward { Selection::range(a, b, unit) } else { Selection::range(b, a, unit) }
            }
            Selection::Cells { table, anchor, focus, shape } => self.whole_merges(table, anchor, focus, shape),
            s => s,
        }
    }

    #[allow(clippy::mut_range_bound, reason = "widening the bounds inside the loop is deliberate: the outer loop repeats until they stop changing")]
    fn whole_merges(&self, table: TableId, anchor: selection::CellRef, focus: selection::CellRef, shape: selection::CellShape) -> Selection {
        let Some(t) = self.index.table(&self.doc, table) else { return Selection::Cells { table, anchor, focus, shape } };
        let g = table::Grid::of(t);
        let (nr, nc) = (t.rows.len().saturating_sub(1), g.cols.saturating_sub(1));
        let clamp = |c: selection::CellRef| selection::CellRef { row: c.row.min(nr), col: c.col.min(nc) };
        let (a, f) = (clamp(anchor), clamp(focus));
        let (mut r0, mut r1, mut c0, mut c1) = (a.row.min(f.row), a.row.max(f.row), a.col.min(f.col), a.col.max(f.col));
        loop {
            let before = (r0, r1, c0, c1);
            for r in r0..=r1 {
                for (k, &(c, s, _)) in g.rows[r].iter().enumerate() {
                    let e = c + s.max(1) - 1;
                    if e < c0 || c > c1 {
                        continue;
                    }
                    if shape != selection::CellShape::Columns {
                        c0 = c0.min(c);
                        c1 = c1.max(e);
                    }
                    let (sr, _) = g.merge_start(r, k);
                    r0 = r0.min(sr);
                    let mut end = r;
                    while end < nr && g.cell_at(end + 1, c).is_some_and(|kk| g.continues(end + 1, kk)) {
                        end += 1;
                    }
                    r1 = r1.max(end);
                }
            }
            if (r0, r1, c0, c1) == before {
                break;
            }
        }
        let pick = |lo: usize, hi: usize, anchor_low: bool| if anchor_low { (lo, hi) } else { (hi, lo) };
        let (ar, fr) = pick(r0, r1, a.row <= f.row);
        let (ac, fc) = pick(c0, c1, a.col <= f.col);
        Selection::Cells { table, anchor: selection::CellRef { row: ar, col: ac }, focus: selection::CellRef { row: fr, col: fc }, shape }
    }

    fn grid_cell_at(&mut self, table: TableId, x: f64, y: f64, fonts: &Fonts) -> Option<(usize, usize)> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let dist = |r: &[f64; 4], a: f64, b: f64| (r[0] - a).max(a - r[2]).max(0.0) + (r[1] - b).max(b - r[3]).max(0.0);
        let c = geo
            .cells
            .iter()
            .filter(|c| c.chain.last().is_some_and(|k| k.0 == table))
            .min_by(|a, b| dist(&a.page_rect(), px, py).total_cmp(&dist(&b.page_rect(), px, py)))?;
        let &(_, row, _) = c.chain.last()?;
        Some((row, c.col))
    }

    fn set_selection(&mut self, s: Selection) {
        let s = self.normalize(s);
        if s != self.selection {
            self.pending = None;
            self.history.break_typing();
        }
        // A number selection ends once its caret is left: coming back doesn't restore it.
        if self.list_label.as_ref().is_some_and(|h| !h.holds(&self.selection) || !h.holds(&s)) {
            self.list_label = None;
        }
        self.selection = s;
    }

    fn caret(&self) -> Option<Position> {
        self.selection.focus()
    }

    // Transactions

    fn record(&self) -> SelectionRecord {
        SelectionRecord { selection: self.selection, chain: self.selection.focus().map(|p| self.index.chain_of(&p)).unwrap_or_default() }
    }

    fn begin(&self, label: &'static str) -> Tx {
        Tx {
            undo: Vec::new(),
            hints: Vec::new(),
            before: self.record(),
            label,
            stale_paras: Vec::new(),
            stale_tables: Vec::new(),
            kept: 0,
            maps: Vec::new(),
            tracked: self.tracking(),
            rec: None,
            origin: self.origin,
            replayed: None,
            targets: step::Targets::default(),
            pictures: false,
            boxes: false,
            relabel: false,
            placeholder: None,
        }
    }

    fn step(&mut self, tx: &mut Tx, s: Step) -> Result<Map, StepError> {
        if self.mode == Mode::ReadOnly {
            return Err(StepError::ReadOnly);
        }
        if let Step::Split { para, at, .. } = &s
            && let Some(p) = self.index.para(&self.doc, *para)
            && splits_a_control(p, *at)
        {
            return Err(StepError::OutOfRange);
        }
        let s = self.inside_editable_end(s);
        let s = self.inside_placeholder(tx, s);
        if !tx.tracked {
            return self.apply_step(tx, s);
        }
        if tx.rec.is_none() {
            let author = self.session_author();
            let secs = crate::clock::unix_secs();
            let mut rec = revisions::Recording::new(author, crate::docx::review::Timestamp::from_utc(secs), self.move_name.clone(), tx.label);
            rec.track_formatting = self.recorded_options().1;
            tx.rec = Some(rec);
        }
        let Some(rec) = tx.rec.as_mut() else { return self.apply_step(tx, s) };
        let mut ids = std::mem::take(&mut self.doc.review_ids);
        let steps = rec.record(s, &self.doc, &self.index, &mut ids);
        self.doc.review_ids = ids;
        let mut map = Map::Same;
        for s in steps {
            map = self.apply_step(tx, s)?;
        }
        Ok(map)
    }

    fn apply_step(&mut self, tx: &mut Tx, s: Step) -> Result<Map, StepError> {
        if self.mode == Mode::ReadOnly && !matches!(tx.origin, Origin::Undo | Origin::Redo) {
            return Err(StepError::ReadOnly);
        }
        if !self.table_preview.is_empty() {
            self.set_table_preview(Vec::new());
        }
        self.border_stepped(&s);
        let added = if self.doc.undeletable.is_empty() { 0 } else { s.pictures_of(&self.doc.undeletable) };
        tx.targets.add(&s);
        tx.pictures |= moves_picture_controls(&s);
        tx.boxes |= matches!(&s, Step::Text { with, .. } if with.iter().any(|i| i.story().is_some()));
        // Editable ranges, once found
        let cached = self.editable_cache.get_mut().is_some();
        let perms = cached && self.moves_perms(&s);
        let a = s.apply(&mut self.doc, &mut self.index)?;
        if perms || (cached && self.moves_perms(&a.inverse)) {
            self.perm_generation += 1;
        }
        if !self.doc.undeletable.is_empty() {
            tx.kept += added - a.inverse.pictures_of(&self.doc.undeletable);
        }
        tx.targets.add(&a.inverse);
        tx.pictures |= moves_picture_controls(&a.inverse);
        tx.undo.push(a.inverse);
        tx.hints.push(a.hint);
        tx.maps.push(a.map.clone());
        Ok(a.map)
    }

    fn abort(&mut self, tx: Tx) {
        self.selection = tx.before.selection;
        self.restore_aside();
        self.pending_controls.clear();
        self.pending_images.clear();
        self.perm_generation += 1;
        for s in tx.undo.into_iter().rev() {
            let _ = s.apply(&mut self.doc, &mut self.index);
        }
    }

    fn commit(&mut self, mut tx: Tx, typing: Option<TypingRun>, fonts: &Fonts) -> Outcome {
        if tx.replayed.is_none()
            && let Err(why) = crate::prof::time("edit:settle", || self.settle(&mut tx))
        {
            self.abort(tx);
            return Outcome::Unchanged(why);
        }
        if tx.undo.is_empty() {
            self.restore_aside();
            return Outcome::Unchanged("no change");
        }
        self.set_aside = None;
        self.repair_selection();
        crate::prof::time("edit:effects", || self.effects(&mut tx));
        if tx.replayed.is_none()
            && let Some(b) = &mut self.batch
        {
            b.label.get_or_insert(Cow::Borrowed(tx.label));
            b.tx.undo.append(&mut tx.undo);
            b.tx.targets.extend(tx.targets);
            self.after_edit(tx.before.selection, fonts);
            return Outcome::Changed(self.revision + 1);
        }
        let label = tx.replayed.as_ref().map_or(Cow::Borrowed(tx.label), |done| done.label.clone());
        crate::prof::time("edit:finish", || self.finish(tx, label, typing, fonts))
    }

    /// Puts a selection whose paragraph the transaction removed at the start of its story, or of the body when the
    /// story went too.
    fn repair_selection(&mut self) {
        let valid = |p: &Position| p.para_offset().is_none_or(|(para, u)| self.index.para(&self.doc, para).is_some_and(|q| u <= para_len(q)));
        let s = self.selection;
        let ok = match s {
            Selection::Cells { table, .. } => self.index.table(&self.doc, table).is_some(),
            Selection::Object(_) => true,
            _ => s.focus().is_some_and(|p| valid(&p)) && s.anchor().is_none_or(|p| valid(&p)),
        };
        if ok {
            return;
        }
        let story = s.focus().map_or(StoryId::Body, |p| p.story);
        let edge = self.nav().story_edge(story, false).or_else(|| self.nav().story_edge(StoryId::Body, false));
        self.selection = Selection::Caret(edge.unwrap_or(Position::text(StoryId::Body, ParaId(0), Unit(0))));
    }

    fn settle(&mut self, tx: &mut Tx) -> Result<(), &'static str> {
        let tracked = std::mem::replace(&mut tx.tracked, false);
        self.recorded = tx.rec.as_ref().map(revisions::Recording::marks).unwrap_or_default();
        for (id, c) in std::mem::take(&mut self.pending_controls) {
            let _ = self.step(tx, Step::Control { id, control: Some(c) });
        }
        for data in std::mem::take(&mut self.pending_images) {
            let index = self.doc.images.len();
            let _ = self.apply_step(tx, Step::AddImage { index, data: Some(data) });
        }
        let mut edited: Vec<ParaId> = tx.undo.iter().filter_map(|s| if let Step::Text { para, .. } = s { Some(*para) } else { None }).collect();
        let mut seen = std::collections::HashSet::with_capacity(edited.len());
        edited.retain(|p| seen.insert(*p));
        for para in edited {
            if self.index.para(&self.doc, para).is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { .. }))) {
                self.refresh_field_kinds(tx, para);
            }
        }
        self.drop_orphan_notes(tx);
        if tx.replayed.is_none() {
            if let Some((para, _, c)) = tx.placeholder.take() {
                if self.left_empty(tx, para, c) {
                    return Err("a placeholder");
                }
                self.placeholder_off(tx, c);
            }
            self.drop_temporary_controls(tx);
            self.sync_dates(tx);
            self.sync_bindings(tx);
        }
        self.drop_orphan_controls(tx);
        self.drop_orphan_comments(tx);
        // Formatting a style or a list level gives isn't a tracked change of the paragraphs in it.
        tx.tracked = false;
        crate::prof::time("edit:reresolve", || self.reresolve_stale(tx));
        tx.tracked = tracked;
        if tx.kept < 0 {
            return Err("a content control that can't be deleted");
        }
        if self.nests_too_deep(tx) {
            return Err("tables nested deeper than a saved document can be opened");
        }
        if tx.pictures && !self.picture_writes && self.loses_protected_picture() {
            return Err("a protected picture");
        }
        if tx.undo.iter().any(|s| !matches!(s, Step::Text { .. } | Step::Restyle { .. } | Step::AddImage { .. })) {
            crate::prof::time("edit:renumber", || self.renumber(tx));
        }
        Ok(())
    }

    /// Whether the blocks, rows or text boxes the transaction put in nest deeper than [`MAX_TABLE_DEPTH`], counting
    /// the cells and text boxes they went into.
    fn nests_too_deep(&self, tx: &Tx) -> bool {
        let cells = |rows: &[crate::docx::Row]| rows.iter().flat_map(|r| &r.cells).map(|c| table_depth(&c.blocks)).max().unwrap_or(0);
        tx.undo.iter().any(|s| match s {
            // The inverse's range is where the new blocks or rows are.
            Step::Blocks { container, range, .. } => self
                .index
                .blocks(&self.doc, *container)
                .and_then(|b| b.get(range.clone()))
                .is_some_and(|b| self.nesting(*container) + table_depth(b) > MAX_TABLE_DEPTH),
            Step::Rows { table, range, .. } => self.index.table_slot(*table).is_some_and(|slot| {
                let Some(Block::Table(t)) = self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(slot.block)) else { return false };
                let rows = t.rows.get(range.clone()).unwrap_or_default();
                self.nesting(slot.container) + 1 + cells(rows) > MAX_TABLE_DEPTH
            }),
            Step::Text { para, .. } if tx.boxes => self.index.para_slot(*para).is_some_and(|slot| {
                let Some(Block::Para(p)) = self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(slot.block)) else { return false };
                self.nesting(slot.container) + para_depth(p) > MAX_TABLE_DEPTH
            }),
            _ => false,
        })
    }

    /// How many levels hold the blocks of `container`: a table level for each cell, [`TEXT_BOX_LEVELS`] for each text
    /// box.
    fn nesting(&self, mut c: index::Container) -> usize {
        let mut n = 0;
        while n <= MAX_TABLE_DEPTH {
            let up = match c {
                index::Container::Cell { table, .. } => {
                    n += 1;
                    self.index.table_slot(table)
                }
                index::Container::Story(StoryId::TextBox(f)) => {
                    n += TEXT_BOX_LEVELS;
                    self.index.float_anchor(f).and_then(|p| self.index.para_slot(p))
                }
                _ => None,
            };
            let Some(slot) = up else { break };
            c = slot.container;
        }
        n
    }

    fn effects(&mut self, tx: &mut Tx) {
        let was = self.revision_marks;
        let mut parts = false;
        let mut hints: Vec<Option<Change>> = Vec::with_capacity(tx.hints.len());
        for (h, s) in tx.hints.iter().zip(&tx.undo).filter(|(_, s)| s.lays_out()) {
            match h {
                None if step::changes_header_footer(&self.index, s) => parts = true,
                h => hints.push(*h),
            }
        }
        crate::prof::time("edit:relayout", || self.relayout_parts(hints, parts));
        self.touched.note(tx.targets.paras.iter().copied());
        // Table, row and cell marks, and a section's or sub-template's, aren't in the targets.
        if !tx.targets.revisions.is_empty()
            || !tx.targets.tables.is_empty()
            || tx.undo.iter().any(|s| matches!(s, Step::SectionBreak { .. } | Step::BuildingBlock { .. }))
        {
            self.revision_marks.0 += 1;
        }
        // A paragraph outside cells without marks parts the marks around it, edited or not.
        let regroups = tx.targets.paras.iter().any(|&p| match self.index.para_slot(p) {
            Some(s) => matches!(s.container, Container::Cell { .. }) || self.index.para(&self.doc, p).is_some_and(revisions::index::holds_marks),
            None => true,
        });
        if regroups {
            self.revision_marks.1 += 1;
        }
        self.follow_marks(was, tx);
        if !tx.targets.comments.is_empty() {
            self.comment_edits += 1;
        }
        let maps = std::mem::take(&mut tx.maps);
        self.map_marks(&maps);
        self.map_multi(&maps);
        if !matches!(tx.origin, Origin::Undo | Origin::Redo) {
            self.mark_edit();
            self.last_maps = maps;
        }
        crate::prof::time("edit:elements", || self.track_elements(&tx.undo));
        crate::prof::time("edit:references", || self.refresh_references(&tx.undo));
        self.note_template_edits(&tx.undo);
        crate::prof::time("edit:labels", || self.refresh_labels(Some(&tx.undo)));
        if tx.pictures {
            self.doc.derive_image_sets();
        }
    }

    fn finish(&mut self, mut tx: Tx, label: Cow<'static, str>, typing: Option<TypingRun>, fonts: &Fonts) -> Outcome {
        let before = tx.before.selection;
        self.revision += 1;
        let undo = std::mem::take(&mut tx.undo);
        let bytes = undo.iter().map(history::step_bytes).sum();
        // Starting or stopping enforced protection fences the history
        let enforced = self.enforced_protection().is_some();
        let enforces = undo.iter().any(|s| matches!(s, Step::Protection(was) if was.as_ref().is_some_and(|p| p.enforced) != enforced));
        match tx.replayed.take() {
            Some(done) if tx.origin == Origin::Undo => self.history.push_redo(Transaction { undo, bytes, ..done }),
            Some(done) => self.history.push_undo_keep_redo(Transaction { undo, bytes, ..done }),
            None => {
                let after = self.record();
                self.history.record(Transaction { undo, before: tx.before, after, label: label.clone(), bytes, id: 0 }, typing);
            }
        }
        if enforces {
            self.history.fence();
        }
        self.commits.push(Committed { revision: self.revision, origin: tx.origin, label, targets: tx.targets });
        self.after_edit(before, fonts);
        Outcome::Changed(self.revision)
    }

    /// `before`: the selection the edit started from. The view follows the caret only when the edit moved it, and
    /// never for a pointer's edit, which is where the user is looking.
    fn after_edit(&mut self, before: Selection, fonts: &Fonts) {
        #[cfg(test)]
        {
            let mut fresh = DocIndex::build_with_parts(&self.doc, self.index.parts());
            fresh.flow = self.index.flow;
            assert_eq!(self.index, fresh, "the incremental index differs from a fresh build");
            let v = integrity::check(&self.doc, &self.index, &self.selection);
            let text =
                self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p)).map(|p| UnitText::of(p).text);
            assert!(v.is_empty(), "integrity: {v:?} ({:?} in {text:?})", self.selection);
            let labels = self.labels_changed(|_| true);
            assert!(labels.is_empty(), "integrity: 9 list labels: {} differ from a fresh numbering, first {:?}", labels.len(), labels.first());
        }
        if let Some(p) = self.caret().filter(|_| !self.deferring) {
            self.ensure(&p, fonts);
            if !self.pointing && before.focus() != Some(p) {
                self.scroll_to_caret(fonts);
            }
        }
        #[cfg(test)]
        self.pages.assert_memo_fresh(&self.index);
    }

    fn undo(&mut self, undo: bool, fonts: &Fonts) -> Outcome {
        if self.mode == Mode::ReadOnly {
            return Outcome::Unchanged("read only");
        }
        self.step_history(undo, fonts)
    }

    /// Undoes (or redoes) the last `count` actions in one go: the Undo and Redo lists' choice of an action takes back
    /// (or does again) it and everything after it. Changed when at least one was.
    pub fn undo_many(&mut self, undo: bool, count: usize, fonts: &Fonts) -> Outcome {
        let available = if undo { self.history.undo_depth() } else { self.history.redo_depth() };
        let count = count.min(available);
        if count == 0 {
            return Outcome::Unchanged(if undo { "nothing to undo" } else { "nothing to redo" });
        }
        if count <= 1 {
            return self.undo(undo, fonts);
        }
        let before = self.selection;
        self.deferring = true;
        let mut out = Outcome::Unchanged(if undo { "nothing to undo" } else { "nothing to redo" });
        for _ in 0..count {
            match self.undo(undo, fonts) {
                o @ Outcome::Changed(_) => out = o,
                o => {
                    if !matches!(out, Outcome::Changed(_)) {
                        out = o;
                    }
                    break;
                }
            }
        }
        // Scrolled to once, from where the caret was before the first
        self.deferring = false;
        self.after_edit(before, fonts);
        out
    }

    /// Takes back transaction `tx`, the last one, unrecorded: an edit that was only ever provisional, whatever the
    /// mode is now. Whether it did.
    fn retract(&mut self, tx: u64, fonts: &Fonts) -> bool {
        let done = self.history.position() == tx && matches!(self.step_history(true, fonts), Outcome::Changed(_));
        if done {
            self.history.pop_redo();
        }
        done
    }

    fn step_history(&mut self, undo: bool, fonts: &Fonts) -> Outcome {
        if self.batch.is_some() {
            return Outcome::Unchanged("in a batch");
        }
        let done = if undo { self.history.pop_undo() } else { self.history.pop_redo() };
        let Some(mut done) = done else { return Outcome::Unchanged(if undo { "nothing to undo" } else { "nothing to redo" }) };
        let mut tx = self.begin("");
        tx.origin = if undo { Origin::Undo } else { Origin::Redo };
        self.replay(&mut tx, std::mem::take(&mut done.undo));
        self.selection = if undo { done.before.selection } else { done.after.selection };
        self.pending = None;
        tx.replayed = Some(done);
        let out = self.commit(tx, None, fonts);
        self.follow_selection();
        out
    }

    fn replay(&mut self, tx: &mut Tx, steps: Vec<Step>) {
        for s in steps.into_iter().rev() {
            if self.apply_step(tx, s).is_err() {
                self.relayout([None]);
                break;
            }
        }
    }

    // Batches

    pub fn begin_batch(&mut self, label: Option<Cow<'static, str>>) {
        let selection = self.selection;
        if let Some(b) = &mut self.batch {
            b.marks.push((b.tx.undo.len(), selection));
            return;
        }
        self.history.break_typing();
        let tx = self.begin("");
        self.batch = Some(Batch { tx, label, marks: vec![(0, selection)] });
    }

    pub fn end_batch(&mut self, keep: bool, fonts: &Fonts) -> Outcome {
        let Some((mark, selection)) = self.batch.as_mut().and_then(|b| b.marks.pop()) else { return Outcome::Unchanged("no batch") };
        if !keep {
            self.revert_batch(mark, selection, fonts);
        }
        let outer = self.batch.as_ref().is_some_and(|b| b.marks.is_empty());
        let changed = self.batch.as_ref().is_some_and(|b| b.tx.undo.len() > mark);
        let batch = if outer { self.batch.take() } else { None };
        match batch {
            Some(b) if changed => self.finish(b.tx, b.label.unwrap_or_default(), None, fonts),
            _ if changed => Outcome::Changed(self.revision + 1),
            _ if self.selection != selection => Outcome::Moved,
            _ => Outcome::Unchanged(if keep { "nothing to do" } else { "refused" }),
        }
    }

    fn revert_batch(&mut self, mark: usize, selection: Selection, fonts: &Fonts) {
        let steps = self.batch.as_mut().map(|b| b.tx.undo.split_off(mark)).unwrap_or_default();
        let mut tx = self.begin("");
        tx.origin = Origin::Undo;
        self.replay(&mut tx, steps);
        self.selection = selection;
        if tx.undo.is_empty() {
            return;
        }
        self.effects(&mut tx);
        self.after_edit(tx.before.selection, fonts);
    }

    // Typing

    fn run_at(&self, pos: &Position) -> Option<Run> {
        if let Some(r) = &self.pending {
            return Some(r.clone());
        }
        let (para, off) = pos.para_offset()?;
        let p = self.index.para(&self.doc, para)?;
        let mut at = 0u32;
        let mut before: Option<&Run> = None;
        for i in &p.inlines {
            let n = i.units() as u32;
            if at >= off.0 && n > 0 {
                return Some(before.or_else(|| run_of(i)).unwrap_or(&p.mark).clone());
            }
            if let Some(r) = run_of(i) {
                before = Some(r);
            }
            at += n;
        }
        Some(before.unwrap_or(&p.mark).clone())
    }

    fn delete_range(&mut self, tx: &mut Tx) -> Result<Option<Position>, StepError> {
        self.delete_range_joining(tx, true)
    }

    fn delete_range_joining(&mut self, tx: &mut Tx, join: bool) -> Result<Option<Position>, StepError> {
        self.take_placeholder(tx)?;
        let end = self.selection.ordered(&self.index).map(|(_, b)| b).filter(|_| tx.tracked && matches!(self.selection, Selection::Text { .. }));
        let from = tx.maps.len();
        let at = self.delete_range_untracked(tx, join)?;
        Ok(match (at, end) {
            (Some(_), Some(b)) => Some(tx.maps[from..].iter().fold(b, |p, m| m.apply(p))),
            _ => at,
        })
    }

    /// Merges section `section` into the next, which keeps its page, headers and footers. Its own headers and
    /// footers, which no other section shows, go with it.
    fn merge_section(&mut self, tx: &mut Tx, section: usize) -> Result<(), StepError> {
        let secs = &self.doc.sections;
        let s = secs.get(section).ok_or(StepError::Missing)?;
        let elsewhere =
            |name: &str| secs.iter().enumerate().any(|(k, o)| k != section && o.headers.iter().chain(&o.footers).flatten().any(|n| n == name));
        let mut own: Vec<(String, bool, usize)> = Vec::new();
        for (footer, refs) in [(false, &s.headers), (true, &s.footers)] {
            for (slot, name) in refs.iter().enumerate() {
                if let Some(name) = name
                    && self.doc.stories.contains_key(name)
                    && !elsewhere(name)
                    && !own.iter().any(|(n, ..)| n == name)
                {
                    own.push((name.clone(), footer, slot));
                }
            }
        }
        for (name, footer, slot) in own {
            self.step(tx, Step::HeaderPart { name, section, footer, slot, blocks: None })?;
        }
        self.step(tx, Step::SectionBreak { section, back: None }).map(|_| ())
    }

    fn delete_range_untracked(&mut self, tx: &mut Tx, join: bool) -> Result<Option<Position>, StepError> {
        if let Selection::Cells { table, anchor, .. } = self.selection
            && let Some(t) = self.index.table(&self.doc, table)
            && let Some(cell) = table::Grid::of(t).cell_at(anchor.row, anchor.col)
            && let Some(s) = self.cell_content(table, anchor.row, cell)
        {
            self.selection = s;
        }
        if let Selection::Block { anchor, .. } = self.selection {
            let pieces = self.block_pieces();
            for (_, _, para, a, b) in pieces.iter().rev() {
                self.delete_text(tx, *para, *a..*b)?;
            }
            return Ok(Some(match pieces.first() {
                Some((_, _, para, a, _)) => Position::text(anchor.story, *para, *a),
                None => anchor,
            }));
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Ok(None) };
        if a.same_place(&b) {
            return Ok(Some(a));
        }
        self.remove_comments_within(tx, a, b)?;
        let section_of = |e: &Editor, p: &Position| {
            let s = match p.place {
                Place::Text { para, .. } => e.index.para_slot(para)?,
                Place::RowEnd { table, .. } => e.index.table_slot(table)?,
            };
            e.index.top_block(s.container, s.block).map(|t| t.0)
        };
        if let (Some(i), Some(j)) = (section_of(self, &a), section_of(self, &b))
            && i < j
        {
            if !tx.tracked {
                for _ in i..j {
                    self.merge_section(tx, i)?;
                }
                return self.delete_range_untracked(tx, join);
            }
            let mut caret = None;
            for k in (i..=j).rev() {
                let blocks = &self.doc.sections.get(k).ok_or(StepError::Missing)?.blocks;
                let start = blocks.first().and_then(first_para_of).map(|p| Position::text(a.story, p, Unit(0)));
                let end = blocks.last().and_then(last_para_of).map(|(p, u)| Position::text(a.story, p, u));
                let (from, to) = (if k == i { Some(a) } else { start }, if k == j { Some(b) } else { end });
                self.selection = Selection::range(from.ok_or(StepError::Missing)?, to.ok_or(StepError::Missing)?, Granularity::Char);
                caret = self.delete_range_untracked(tx, join)?;
            }
            return Ok(caret);
        }
        if let Some(p) = self.delete_across_tables(tx, a, b)? {
            return Ok(Some(p));
        }
        let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset()) else {
            return self.delete_paragraph_texts(tx, a, b).map(|()| Some(a));
        };
        if pa == pb {
            self.delete_text(tx, pa, oa..ob)?;
            return Ok(Some(Position::text(a.story, pa, oa)));
        }
        let (sa, sb) = (self.index.para_slot(pa).ok_or(StepError::Missing)?, self.index.para_slot(pb).ok_or(StepError::Missing)?);
        if sa.container == sb.container && oa == Unit(0) && ob == Unit(0) {
            self.step(tx, Step::Blocks { container: sa.container, range: sa.block..sb.block, with: Vec::new() })?;
            return Ok(Some(Position::text(a.story, pb, Unit(0))));
        }
        if sa.container == sb.container {
            let table_between =
                self.index.blocks(&self.doc, sa.container).is_some_and(|bl| bl[sa.block + 1..sb.block].iter().any(|b| matches!(b, Block::Table(_))));
            let len_a = para_len(self.index.para(&self.doc, pa).ok_or(StepError::Missing)?);
            self.delete_text(tx, pb, Unit(0)..ob)?;
            self.delete_text(tx, pa, oa..len_a)?;
            if sb.block > sa.block + 1 {
                self.step(tx, Step::Blocks { container: sa.container, range: sa.block + 1..sb.block, with: Vec::new() })?;
            }
            if table_between && !join {
                if oa == Unit(0) {
                    self.step(tx, Step::Blocks { container: sa.container, range: sa.block..sa.block + 1, with: Vec::new() })?;
                    return Ok(Some(Position::text(a.story, pb, Unit(0))));
                }
                return Ok(Some(Position::text(a.story, pa, oa)));
            }
            let controls = |e: &Editor, p: ParaId| e.index.para(&e.doc, p).map(|x| x.controls.clone());
            if controls(self, pa) != controls(self, pb) {
                return Ok(Some(Position::text(a.story, pa, oa)));
            }
            self.step(tx, Step::Join { first: pa, second: pb })?;
            tx.stale_paras.push(pa);
            return Ok(Some(Position::text(a.story, pa, oa)));
        }
        self.delete_paragraph_texts(tx, a, b)?;
        Ok(Some(a.with_affinity(Affinity::Downstream)))
    }

    fn delete_across_tables(&mut self, tx: &mut Tx, a: Position, b: Position) -> Result<Option<Position>, StepError> {
        let (ca, cb) = (self.index.chain_of(&a), self.index.chain_of(&b));
        let k = ca.iter().zip(&cb).take_while(|(x, y)| x.table == y.table && x.row == y.row && x.cell == y.cell).count();
        let top = |e: &Editor, p: &Position, chain: &[index::Level]| -> Option<(index::Slot, Option<(TableId, usize)>)> {
            match chain.get(k) {
                Some(l) => Some((e.index.table_slot(l.table)?, Some((l.table, l.row)))),
                None => Some((e.index.para_slot(p.para_offset()?.0)?, None)),
            }
        };
        let (Some((sa, ta)), Some((sb, tb))) = (top(self, &a, &ca), top(self, &b, &cb)) else { return Ok(None) };
        if sa.container != sb.container || (ta.is_none() && tb.is_none()) || (ta.is_some() && ta.map(|t| t.0) == tb.map(|t| t.0)) {
            return Ok(None);
        }
        let story = a.story;
        let mut moved_into: Option<ParaId> = None;
        match tb {
            Some((table, rb)) => {
                let n = self.index.table(&self.doc, table).map_or(0, |t| t.rows.len());
                if rb + 1 >= n {
                    self.step(tx, Step::Blocks { container: sb.container, range: sb.block..sb.block + 1, with: Vec::new() })?;
                } else {
                    let kept = self.index.table(&self.doc, table).and_then(|t| {
                        let g = table::Grid::of(t);
                        let mut r = t.rows.get(rb + 1)?.clone();
                        let mut changed = false;
                        for (k, c) in r.cells.iter_mut().enumerate() {
                            if c.v_merge == crate::docx::VMerge::Continue && g.merge_start(rb + 1, k).0 <= rb {
                                c.v_merge = crate::docx::VMerge::Restart;
                                changed = true;
                            }
                        }
                        changed.then_some(r)
                    });
                    match kept {
                        Some(r) => self.step(tx, Step::Rows { table, range: 0..rb + 2, with: vec![r], grid: None })?,
                        None => self.step(tx, Step::Rows { table, range: 0..rb + 1, with: Vec::new(), grid: None })?,
                    };
                    tx.stale_tables.push(table);
                    moved_into =
                        self.index.table(&self.doc, table).and_then(|t| t.rows.first()?.cells.first()?.blocks.iter().find_map(first_para_of));
                }
            }
            None => {
                let (pb, ob) = b.para_offset().ok_or(StepError::Missing)?;
                self.delete_text(tx, pb, Unit(0)..ob)?;
            }
        }
        if sb.block > sa.block + 1 {
            self.step(tx, Step::Blocks { container: sa.container, range: sa.block + 1..sb.block, with: Vec::new() })?;
        }
        match ta {
            Some((table, ra)) => {
                let n = self.index.table(&self.doc, table).map_or(0, |t| t.rows.len());
                if ra == 0 {
                    self.step(tx, Step::Blocks { container: sa.container, range: sa.block..sa.block + 1, with: Vec::new() })?;
                } else {
                    self.step(tx, Step::Rows { table, range: ra..n, with: Vec::new(), grid: None })?;
                    tx.stale_tables.push(table);
                }
                let next = self
                    .index
                    .blocks(&self.doc, sa.container)
                    .and_then(|bl| bl.get(if ra == 0 { sa.block } else { sa.block + 1 }))
                    .and_then(first_para_of);
                Ok(next.map(|p| Position::text(story, p, Unit(0))))
            }
            None => {
                let (pa, oa) = a.para_offset().ok_or(StepError::Missing)?;
                let len = para_len(self.index.para(&self.doc, pa).ok_or(StepError::Missing)?);
                self.delete_text(tx, pa, oa..len)?;
                if let Some(target) = moved_into {
                    let n = self.move_para_into(tx, pa, target)?;
                    return Ok(Some(Position::text(story, target, Unit(n))));
                }
                Ok(Some(Position::text(story, pa, oa)))
            }
        }
    }

    /// Moves `para`'s content to the start of `target` and removes it; returns the units moved. Untracked, every
    /// inline moves (anchored objects, bookmarks, comment and control edges); tracked, the deleted paragraph keeps
    /// its markers and only its text is inserted.
    fn move_para_into(&mut self, tx: &mut Tx, para: ParaId, target: ParaId) -> Result<u32, StepError> {
        let p = self.index.para(&self.doc, para).ok_or(StepError::Missing)?;
        let inlines: Vec<Inline> = if tx.tracked {
            p.inlines.iter().filter(|i| i.units() > 0).cloned().collect()
        } else {
            p.inlines.iter().filter(|i| !matches!(i, Inline::Text(t, _) if t.is_empty())).cloned().collect()
        };
        let n: u32 = inlines.iter().map(|i| i.units() as u32).sum();
        let slot = self.index.para_slot(para).ok_or(StepError::Missing)?;
        self.step(tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() })?;
        if !inlines.is_empty() {
            self.step(tx, Step::Text { para: target, range: Unit(0)..Unit(0), with: inlines, edges: Default::default() })?;
        }
        tx.stale_paras.push(target);
        Ok(n)
    }

    fn delete_text(&mut self, tx: &mut Tx, para: ParaId, range: std::ops::Range<Unit>) -> Result<(), StepError> {
        let p = self.index.para(&self.doc, para).ok_or(StepError::Missing)?;
        let mut at = 0u32;
        let mut marks: Vec<(u32, crate::docx::ControlId, bool)> = Vec::new();
        for i in &p.inlines {
            match i {
                Inline::ControlStart(c) => marks.push((at, *c, true)),
                Inline::ControlEnd(c) => marks.push((at, *c, false)),
                _ => {}
            }
            at += i.units() as u32;
        }
        let (a, b) = (range.start.0, range.end.0);
        let mut cuts: Vec<u32> = Vec::new();
        for &(u, c, start) in &marks {
            let other = marks.iter().find(|(_, d, s)| *d == c && *s != start).map(|m| m.0);
            let Some(o) = other else { continue };
            let (cs, ce) = if start { (u, o) } else { (o, u) };
            let whole = a <= cs && ce <= b && (a < cs || ce < b || a < b);
            let drop_down = self.doc.controls.get(&c).is_some_and(|k| k.props.kind == crate::docx::ControlKind::DropDown);
            if drop_down && !whole && a < ce && b > cs {
                return Err(StepError::OutOfRange);
            }
            if a < u && u < b && !(a <= cs && ce <= b) {
                cuts.push(u);
            }
        }
        let (comment_cuts, emptied) = comments::deletion_cuts(p, a, b);
        cuts.extend(comment_cuts);
        cuts.extend(clipboard::severed_markers(p, range.start, range.end));
        cuts.sort();
        cuts.dedup();
        if !emptied.is_empty() {
            self.remove_comments(tx, &emptied)?;
        }
        let mut lone: Vec<(crate::docx::ControlId, bool)> = Vec::new();
        for &(u, c, start) in &marks {
            let Some(o) = marks.iter().find(|(_, d, s)| *d == c && *s != start).map(|m| m.0) else { continue };
            let inside = |x: u32| a < x && x < b;
            if (u == a || u == b) && inside(o) {
                lone.push((c, start));
            }
        }
        // Bookmarks and editing permissions on the range's edges go with it when the rest of them does (comments go
        // with `remove_comments`).
        let bound = clipboard::bound_markers(self.index.para(&self.doc, para).ok_or(StepError::Missing)?, range.start, range.end, false);
        if !lone.is_empty() || !bound.is_empty() {
            let p = self.index.para(&self.doc, para).ok_or(StepError::Missing)?;
            let inlines: Vec<Inline> = p
                .inlines
                .iter()
                .enumerate()
                .filter(|(k, i)| {
                    !bound.contains(k)
                        && !matches!(i, Inline::ControlStart(c) if lone.contains(&(*c, true)))
                        && !matches!(i, Inline::ControlEnd(c) if lone.contains(&(*c, false)))
                })
                .map(|(_, i)| i.clone())
                .collect();
            self.step(tx, Step::Restyle { para, inlines })?;
        }
        let mut ends: Vec<u32> = cuts.clone();
        ends.push(b);
        let mut starts = vec![a];
        starts.extend(cuts);
        for (s, e) in starts.into_iter().zip(ends).rev() {
            if e > s {
                self.step(tx, Step::Text { para, range: Unit(s)..Unit(e), with: Vec::new(), edges: Default::default() })?;
            }
        }
        Ok(())
    }

    fn delete_paragraph_texts(&mut self, tx: &mut Tx, a: Position, b: Position) -> Result<(), StepError> {
        let mut p = a;
        let mut guard = 0;
        while self.index.cmp(&p, &b) == std::cmp::Ordering::Less && guard < 1_000_000 {
            guard += 1;
            if let Some((para, off)) = p.para_offset() {
                let len = para_len(self.index.para(&self.doc, para).ok_or(StepError::Missing)?);
                let end = match b.para_offset() {
                    Some((pb, ob)) if pb == para => ob,
                    _ => len,
                };
                if end > off {
                    self.delete_text(tx, para, off..end)?;
                }
                p = Position::text(p.story, para, off.max(Unit(0)));
                let at_end = Position::text(p.story, para, para_len(self.index.para(&self.doc, para).ok_or(StepError::Missing)?));
                let next = self.nav().right(at_end, false);
                if next == at_end {
                    break;
                }
                p = next;
            } else {
                let next = self.nav().right(p, false);
                if next == p {
                    break;
                }
                p = next;
            }
        }
        Ok(())
    }

    fn insert_text(&mut self, text: &str, fonts: &Fonts) -> Outcome {
        let text = &*crate::docx::xml::legal(text);
        if text.is_empty() {
            return Outcome::Unchanged("empty text");
        }
        if text == " " && self.mode != Mode::Design && self.selected_picture_unfilled() && self.activate_picture_control() {
            return Outcome::Unchanged("placeholder activated");
        }
        if (text == " " || ((text == "x" || text == "X") && self.selected_form_check_box().is_some()))
            && let Some(p) = self.selected_checkbox()
        {
            return if self.mode == Mode::Design { Outcome::Unchanged("design mode") } else { self.toggle_checkbox(p, fonts) };
        }
        self.shape_story_for_typing(fonts);
        self.object_to_caret();
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        let mut tx = self.begin("Typing");
        match self.type_into(&mut tx, text) {
            Ok((caret, single)) => {
                self.selection = Selection::Caret(caret);
                let typing = caret.para_offset().filter(|_| single).map(|(para, at)| TypingRun { para, at, kind: Typing::Insert });
                self.commit(tx, typing, fonts)
            }
            Err(why) => {
                if why != "no caret" {
                    self.abort(tx);
                }
                self.restore_aside();
                Outcome::Unchanged(why)
            }
        }
    }

    /// Types `text` over the selection in `tx`: the selection goes, then the text, a paragraph for each line and a tab
    /// for each tab character. Where the caret ends, and whether the text was one line; on an error, `tx` is to be
    /// aborted (except "no caret", where nothing changed).
    fn type_into(&mut self, tx: &mut Tx, text: &str) -> Result<(Position, bool), &'static str> {
        let caret = match self.delete_range(tx) {
            Ok(Some(p)) => p,
            Ok(None) => return Err("no caret"),
            Err(_) => return Err("stale selection"),
        };
        let caret = self.past_row_end(caret);
        let Place::Text { para, offset, .. } = caret.place else { return Err("caret at a row end") };
        let Some(run) = self.run_at(&caret) else { return Err("no paragraph") };
        let (mut para, mut at) = (para, offset);
        let mut single = true;
        for (k, line) in text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).enumerate() {
            if k > 0 {
                single = false;
                let Some(second) = self.split_template(para) else { break };
                let id = ParaId(second.id);
                if self.step(tx, Step::Split { para, at, second, kept: Vec::new() }).is_err() {
                    return Err("split failed");
                }
                para = id;
                at = Unit(0);
            }
            let mut inlines = Vec::new();
            for (j, seg) in line.split('\t').enumerate() {
                if j > 0 {
                    inlines.push(Inline::Tab(run.clone()));
                }
                if !seg.is_empty() {
                    inlines.push(Inline::Text(seg.to_string(), run.clone()));
                }
            }
            let n: u32 = inlines.iter().map(|i| i.units() as u32).sum();
            if n > 0 && self.step(tx, Step::Text { para, range: at..at, with: inlines, edges: Default::default() }).is_err() {
                return Err("insert failed");
            }
            at = Unit(at.0 + n);
        }
        // Caret past the typed cluster
        let plain = |c: char| c.is_ascii_graphic() || c == ' ';
        let typed = text.rsplit('\n').next().and_then(|l| l.strip_suffix('\r').unwrap_or(l).chars().next_back());
        let p = self.index.para(&self.doc, para);
        let ends = typed.is_some_and(plain) && p.is_some_and(|p| text::char_at(p, at).is_none_or(plain));
        let at = match p {
            Some(p) if !ends => {
                let t = UnitText::of(p);
                if t.is_boundary(at) { at } else { t.next_grapheme(at) }
            }
            _ => at,
        };
        Ok((Position::text(caret.story, para, at), single))
    }

    fn past_row_end(&mut self, mut p: Position) -> Position {
        for _ in 0..64 {
            if p.para_offset().is_some() {
                break;
            }
            let next = self.nav().right(p, false);
            if next == p {
                break;
            }
            p = next;
        }
        p
    }

    fn column_break(&mut self, fonts: &Fonts) -> Outcome {
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        let at_start = self.selection.is_collapsed()
            && self.caret().and_then(|c| c.para_offset()).is_some_and(|(p, off)| {
                off == Unit(0) || self.index.para(&self.doc, p).is_some_and(|q| UnitText::of(q).text.chars().nth(off.0 as usize - 1) == Some('\u{c}'))
            });
        if at_start {
            return self.insert_inline(|run| Inline::Break(BreakKind::Column, run), "Column Break", fonts);
        }
        let mut tx = self.begin("Column Break");
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => self.past_row_end(p),
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let (Some((para, at)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let Some(second) = self.split_template(para) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let id = ParaId(second.id);
        if self.step(&mut tx, Step::Split { para, at, second, kept: Vec::new() }).is_err()
            || self
                .step(
                    &mut tx,
                    Step::Text { para: id, range: Unit(0)..Unit(0), with: vec![Inline::Break(BreakKind::Column, run)], edges: Default::default() },
                )
                .is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("break failed");
        }
        self.selection = Selection::Caret(Position::text(caret.story, id, Unit(1)));
        self.commit(tx, None, fonts)
    }

    fn page_break(&mut self, fonts: &Fonts) -> Outcome {
        let s = &self.doc.settings;
        if s.compat() == crate::docx::Compat::Legacy && s.has("splitPgBreakAndParaMark") {
            return self.insert_inline(|run| Inline::Break(BreakKind::Page, run), "Page Break", fonts);
        }
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        let mut tx = self.begin("Page Break");
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => self.past_row_end(p),
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let (Some((para, at)), Some(run), Some(second)) =
            (caret.para_offset(), self.run_at(&caret), caret.para_offset().and_then(|(p, _)| self.split_template(p)))
        else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let id = ParaId(second.id);
        let with = vec![Inline::Break(BreakKind::Page, run)];
        if self.step(&mut tx, Step::Text { para, range: at..at, with, edges: Default::default() }).is_err()
            || self.step(&mut tx, Step::Split { para, at: Unit(at.0 + 1), second, kept: Vec::new() }).is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("break failed");
        }
        self.selection = Selection::Caret(Position::text(caret.story, id, Unit(0)));
        self.commit(tx, None, fonts)
    }

    fn split_template(&self, para: ParaId) -> Option<Paragraph> {
        let p = self.index.para(&self.doc, para)?;
        Some(Paragraph {
            style: p.style.clone(),
            mark: p.mark.clone(),
            list: p.list.clone(),
            inlines: Vec::new(),
            src: None,
            id: crate::docx::next_para_id(),
            div: None,
            controls: p.controls.clone(),
        })
    }

    fn enter(&mut self, fonts: &Fonts) -> Outcome {
        if self.mode == Mode::Design && self.activate(false) {
            return Outcome::Unchanged("activated");
        }
        if self.mode != Mode::Design && self.activate_picture_control() {
            return Outcome::Unchanged("picture control activated");
        }
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        if let Some(o) = self.list_enter(fonts) {
            return o;
        }
        self.split_at_caret(true, fonts).0
    }

    /// Enter's split: the selection replaced, the caret's paragraph split there (the part after it in the style that
    /// follows at a paragraph's end), the caret at the second part's start. With `enter`, Enter's own cases too: a row
    /// added at a row end, a paragraph before a table that starts its story. Answers the first part when it split.
    fn split_at_caret(&mut self, enter: bool, fonts: &Fonts) -> (Outcome, Option<ParaId>) {
        let mut tx = self.begin("Enter");
        let collapsed = matches!(self.selection, Selection::Caret(_));
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => p,
            _ => {
                self.abort(tx);
                return (Outcome::Unchanged("no caret"), None);
            }
        };
        if let (true, Place::RowEnd { table, row }) = (enter, caret.place) {
            self.abort(tx);
            return (self.add_row(table, row, caret.story, fonts), None);
        }
        let Some((para, at)) = caret.para_offset() else {
            self.abort(tx);
            return (Outcome::Unchanged("caret at a row end"), None);
        };
        if enter
            && at == Unit(0)
            && let Some(slot) = self.index.para_slot(para)
            && slot.block == 0
            && let Some(table_slot) = self.table_needing_paragraph_before(slot.container)
        {
            let template = self.split_template(para).map(|mut p| {
                p.list = None;
                p
            });
            let Some(mut p) = template else {
                self.abort(tx);
                return (Outcome::Unchanged("no paragraph"), None);
            };
            p.inlines = self.run_at(&caret).map(|r| vec![Inline::Text(String::new(), r)]).unwrap_or_default();
            let new_id = ParaId(p.id);
            if self
                .step(
                    &mut tx,
                    Step::Blocks { container: table_slot.container, range: table_slot.block..table_slot.block, with: vec![Block::Para(p)] },
                )
                .is_err()
            {
                self.abort(tx);
                return (Outcome::Unchanged("insert failed"), None);
            }
            tx.stale_paras.push(new_id);
            self.selection = Selection::Caret(if collapsed { Position::text(caret.story, new_id, Unit(0)) } else { caret });
            return (self.commit(tx, None, fonts), None);
        }
        let Some(mut second) = self.split_template(para) else {
            self.abort(tx);
            return (Outcome::Unchanged("no paragraph"), None);
        };
        let at_end = self.index.para(&self.doc, para).is_some_and(|p| para_len(p) == at);
        if at_end && let Some(s) = self.next_style_paragraph(para, second.clone()) {
            second = s;
            tx.stale_paras.push(ParaId(second.id));
        }
        let id = ParaId(second.id);
        if self.step(&mut tx, Step::Split { para, at, second, kept: Vec::new() }).is_err() {
            self.abort(tx);
            return (Outcome::Unchanged("split failed"), None);
        }
        self.selection = Selection::Caret(Position::text(caret.story, id, Unit(0)));
        (self.commit(tx, None, fonts), Some(para))
    }

    fn table_needing_paragraph_before(&self, container: Container) -> Option<index::Slot> {
        let Container::Cell { table, row: 0, cell: 0 } = container else { return None };
        let s = self.index.table_slot(table)?;
        (s.block == 0).then_some(s)
    }

    fn insert_inline(&mut self, make: impl Fn(Run) -> Inline, label: &'static str, fonts: &Fonts) -> Outcome {
        self.insert_inlines(|run| vec![make(run)], label, fonts)
    }

    fn insert_inlines(&mut self, make: impl Fn(Run) -> Vec<Inline>, label: &'static str, fonts: &Fonts) -> Outcome {
        self.insert_inlines_typed(make, label, false, fonts)
    }

    /// `typing`: the insert joins the typing around it in one undo step.
    fn insert_inlines_typed(&mut self, make: impl Fn(Run) -> Vec<Inline>, label: &'static str, typing: bool, fonts: &Fonts) -> Outcome {
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        let mut tx = self.begin(label);
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => p,
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let caret = self.past_row_end(caret);
        let (Some((para, at)), Some(run)) = (caret.para_offset(), self.run_at(&caret)) else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let with = make(run);
        let n = with.iter().map(Inline::units).sum::<usize>() as u32;
        if self.step(&mut tx, Step::Text { para, range: at..at, with, edges: Default::default() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("insert failed");
        }
        let at = Unit(at.0 + n);
        self.selection = Selection::Caret(Position::text(caret.story, para, at));
        self.commit(tx, typing.then_some(TypingRun { para, at, kind: Typing::Insert }), fonts)
    }

    fn delete(&mut self, dir: Dir, word: bool, fonts: &Fonts) -> Outcome {
        if let Some(o) = self.delete_object(fonts) {
            return o;
        }
        // A placeholder deleted shows again
        // Backspace at its start
        if !self.is_discontiguous()
            && let Some((_, at, _, s, _)) = self.placeholder_here()
            && !(dir == Dir::Back && self.selection.is_collapsed() && at.para_offset().is_some_and(|(_, u)| u.0 == s))
        {
            return Outcome::Unchanged("a placeholder");
        }
        if !self.is_discontiguous()
            && let Some(o) = self.delete_cells(dir == Dir::Forward, fonts)
        {
            return o;
        }
        if dir == Dir::Back
            && let Some(o) = self.list_backspace(fonts)
        {
            return o;
        }
        if self.selection.is_collapsed()
            && let Some(p) = self.caret()
            && let Some((para, u)) = p.para_offset()
            && let Some(q) = self.index.para(&self.doc, para)
        {
            let unit = if dir == Dir::Back { u.0.checked_sub(1) } else { Some(u.0) };
            let f = unit.and_then(|k| match stories::inline_at(q, Unit(k)) {
                Some(Inline::FieldEnd { id, .. }) | Some(Inline::FieldBegin { id, .. }) => Some(*id),
                _ => None,
            });
            if let Some(f) = f
                && let Some(merge::Where::Inline { story, para, start, end, .. }) = self.locate(crate::docx::ElementId::Field(f))
            {
                self.set_selection(Selection::range(Position::text(story, para, start), Position::text(story, para, end), Granularity::Char));
                return Outcome::Moved;
            }
        }
        if self.selection.is_collapsed() {
            if let Some(p) = self.caret()
                && let Some((para, u)) = p.para_offset()
            {
                let (a, b) = if dir == Dir::Back { (Unit(u.0.saturating_sub(1)), u) } else { (u, Unit(u.0 + 1)) };
                if a != b && self.locked_edit(&Position::text(p.story, para, a), &Position::text(p.story, para, b)) {
                    self.refused = Some("locked");
                    return Outcome::Unchanged("locked");
                }
            }
        } else if self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        if self.is_discontiguous() {
            return self.multi_delete(dir, fonts);
        }
        let mut tx = self.begin(if dir == Dir::Back { "Backspace" } else { "Delete" });
        if !self.selection.is_collapsed() {
            let caret = match self.delete_range_joining(&mut tx, false) {
                Ok(Some(p)) => p,
                _ => {
                    self.abort(tx);
                    return Outcome::Unchanged("stale selection");
                }
            };
            self.selection = Selection::Caret(caret);
            return self.commit(tx, None, fonts);
        }
        let Some(caret) = self.caret() else { return Outcome::Unchanged("no caret") };
        let Some((para, off)) = caret.para_offset() else {
            if dir == Dir::Back {
                let p = self.nav().left(caret, false);
                self.set_selection(Selection::Caret(p));
                return Outcome::Moved;
            }
            return Outcome::Unchanged("row end");
        };
        let Some(t) = self.index.para(&self.doc, para).map(|q| UnitText::in_review(q, &self.doc.review)) else {
            return Outcome::Unchanged("no paragraph");
        };
        let typing = |at| TypingRun { para, at, kind: if dir == Dir::Back { Typing::Backspace } else { Typing::Delete } };
        if dir == Dir::Back && off > Unit(0) {
            let from = if word { t.prev_word(off) } else { t.prev_grapheme(off) };
            if tx.tracked && !word && self.index.para(&self.doc, para).is_some_and(|p| revisions::record::all_deleted(p, from, off)) {
                self.set_selection(Selection::Caret(Position::text(caret.story, para, from)));
                return Outcome::Moved;
            }
            if self.delete_text(&mut tx, para, from..off).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("delete failed");
            }
            self.selection = Selection::Caret(Position::text(caret.story, para, from));
            return self.commit(tx, Some(typing(from)), fonts);
        }
        if dir == Dir::Forward && off < t.len() {
            let to = if word { t.next_word(off) } else { t.next_grapheme(off) };
            let from = tx.maps.len();
            if self.delete_text(&mut tx, para, off..to).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("delete failed");
            }
            let end = if tx.tracked { tx.maps[from..].iter().fold(Position::text(caret.story, para, to), |p, m| m.apply(p)) } else { caret };
            let at = end.para_offset().map_or(off, |(_, u)| u);
            self.selection = Selection::Caret(end);
            return self.commit(tx, Some(typing(at)), fonts);
        }
        if !tx.tracked
            && let Some(slot) = self.index.para_slot(para)
            && let Container::Section(k) = slot.container
        {
            let at_edge = match dir {
                Dir::Back if slot.block == 0 && k > 0 => Some((k - 1, self.doc.sections[k - 1].blocks.last())),
                Dir::Forward if slot.block + 1 == self.doc.sections[k].blocks.len() => self.doc.sections.get(k + 1).map(|s| (k, s.blocks.first())),
                _ => None,
            };
            let controls = self.index.para(&self.doc, para).map(|x| &x.controls);
            if let Some((section, Some(Block::Para(q)))) = at_edge
                && Some(&q.controls) == controls
                && self.merge_section(&mut tx, section).is_err()
            {
                self.abort(tx);
                return Outcome::Unchanged("section break");
            }
        }
        let Some(slot) = self.index.para_slot(para) else { return Outcome::Unchanged("no paragraph") };
        let blocks = self.index.blocks(&self.doc, slot.container).map(|b| b.len()).unwrap_or(0);
        let sibling = |k: usize| match self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(k)) {
            Some(Block::Para(p)) => Some(Ok((ParaId(p.id), para_len(p)))),
            Some(Block::Table(t)) => Some(Err(TableId(t.id))),
            None => None,
        };
        let controls = |p: ParaId| self.index.para(&self.doc, p).map(|x| x.controls.clone());
        match dir {
            Dir::Back => match slot.block.checked_sub(1).and_then(sibling) {
                Some(Ok((prev, _))) if controls(prev) != controls(para) => Outcome::Unchanged("a block control's edge"),
                Some(Ok((prev, len))) => {
                    if self.step(&mut tx, Step::Join { first: prev, second: para }).is_err() {
                        self.abort(tx);
                        return Outcome::Unchanged("join failed");
                    }
                    tx.stale_paras.push(prev);
                    self.selection = Selection::Caret(Position::text(caret.story, prev, len));
                    self.commit(tx, None, fonts)
                }
                Some(Err(_)) => Outcome::Unchanged("after a table"),
                None => Outcome::Unchanged("start of a cell or story"),
            },
            Dir::Forward => match (slot.block + 1 < blocks).then(|| sibling(slot.block + 1)).flatten() {
                Some(Ok((next, _))) if controls(next) != controls(para) => Outcome::Unchanged("a block control's edge"),
                Some(Ok((next, _))) => {
                    let from = tx.maps.len();
                    if self.step(&mut tx, Step::Join { first: para, second: next }).is_err() {
                        self.abort(tx);
                        return Outcome::Unchanged("join failed");
                    }
                    tx.stale_paras.push(para);
                    let end = match tx.tracked {
                        true => tx.maps[from..].iter().fold(Position::text(caret.story, next, Unit(0)), |p, m| m.apply(p)),
                        false => Position::text(caret.story, para, off),
                    };
                    self.selection = Selection::Caret(end);
                    self.commit(tx, None, fonts)
                }
                Some(Err(table)) => {
                    let Some(target) = self
                        .index
                        .table(&self.doc, table)
                        .and_then(|t| t.rows.first())
                        .and_then(|r| r.cells.first())
                        .and_then(|c| c.blocks.iter().find_map(first_para_of))
                    else {
                        return Outcome::Unchanged("no first cell");
                    };
                    let n = match self.move_para_into(&mut tx, para, target) {
                        Ok(n) => n,
                        Err(_) => {
                            self.abort(tx);
                            return Outcome::Unchanged("move failed");
                        }
                    };
                    self.selection = Selection::Caret(Position::text(caret.story, target, Unit(n)));
                    self.commit(tx, None, fonts)
                }
                _ => Outcome::Unchanged("end of a cell or story"),
            },
        }
    }

    fn next_cell(&mut self, back: bool, fonts: &Fonts) -> Outcome {
        let (table, here, story) = match self.selection {
            Selection::Cells { table, .. } => {
                if !back {
                    return self.select_cell(table, 0, 0);
                }
                let Some(slot) = self.index.table_slot(table) else { return Outcome::Unchanged("no table") };
                let Container::Cell { table: outer, row, cell } = slot.container else { return Outcome::Unchanged("not nested") };
                (outer, (row, cell), self.index.story(slot.container))
            }
            _ => {
                let Some(focus) = self.selection.focus().or_else(|| self.selection.anchor()) else { return Outcome::Unchanged("no caret") };
                let Some(level) = self.index.chain_of(&focus).last().copied() else { return Outcome::Unchanged("not in a table") };
                let here = match self.tab_stop {
                    Some((sel, t, rc)) if sel == self.selection && t == level.table => rc,
                    _ => (level.row, level.cell),
                };
                (level.table, here, focus.story)
            }
        };
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let order: Vec<(usize, usize)> = t.rows.iter().enumerate().flat_map(|(r, row)| (0..row.cells.len()).map(move |c| (r, c))).collect();
        let here = (here.0, here.1.min(t.rows.get(here.0).map_or(0, |r| r.cells.len().saturating_sub(1))));
        let Some(i) = order.iter().position(|&c| c == here) else { return Outcome::Unchanged("cell not found") };
        let next = if back { i.checked_sub(1).and_then(|j| order.get(j)) } else { order.get(i + 1) };
        match next {
            Some(&(r, c)) => self.select_cell(table, r, c),
            None if back => Outcome::Unchanged("first cell"),
            // Past the last cell: a new row
            None if self.mode == Mode::ReadOnly => self.refuse_read_only(),
            None => {
                if let Some(e) = self.protection_refuses(crate::api::command::CommandId::InsertRows) {
                    return Outcome::Refused(e);
                }
                let last = t.rows.len() - 1;
                self.add_row(table, last, story, fonts)
            }
        }
    }

    fn select_cell(&mut self, table: TableId, row: usize, cell: usize) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let g = table::Grid::of(t);
        let (r, c) = g.merge_start(row, cell);
        if let Some(Block::Table(inner)) = t.rows.get(r).and_then(|x| x.cells.get(c)).and_then(|x| x.blocks.first()) {
            let ig = table::Grid::of(inner);
            let last = inner.rows.len().saturating_sub(1);
            let s = Selection::Cells {
                table: TableId(inner.id),
                anchor: selection::CellRef { row: 0, col: 0 },
                focus: selection::CellRef { row: last, col: ig.cols.saturating_sub(1) },
                shape: selection::CellShape::Rect,
            };
            self.set_selection(s);
            self.tab_stop = None;
            return Outcome::Moved;
        }
        let Some(s) = self.cell_content(table, r, c) else { return Outcome::Unchanged("empty cell") };
        self.set_selection(s);
        self.tab_stop = Some((self.selection, table, (row, cell)));
        Outcome::Moved
    }

    fn cell_content(&self, table: TableId, row: usize, cell: usize) -> Option<Selection> {
        let t = self.index.table(&self.doc, table)?;
        let story = self.index.table_slot(table).map_or(self.editing(), |s| self.index.story(s.container));
        let blocks = &t.rows.get(row)?.cells.get(cell)?.blocks;
        let a = blocks.iter().find_map(first_para_of)?;
        let (lp, len) = blocks.iter().rev().find_map(last_para_of)?;
        Some(Selection::range(Position::text(story, a, Unit(0)), Position::text(story, lp, len), Granularity::Char))
    }

    fn add_row(&mut self, table: TableId, after: usize, story: StoryId, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let Some(mut row) = t.rows.get(after).cloned() else { return Outcome::Unchanged("no row") };
        row.src = None;
        for cell in &mut row.cells {
            let Some(p) = cell.blocks.iter().find_map(|b| match b {
                Block::Para(p) => Some(p),
                _ => None,
            }) else {
                return Outcome::Unchanged("a cell without a paragraph");
            };
            let p = Paragraph {
                style: p.style.clone(),
                mark: p.mark.clone(),
                list: None,
                inlines: vec![Inline::Text(String::new(), p.mark.clone())],
                src: None,
                id: crate::docx::next_para_id(),
                div: None,
                controls: Vec::new(),
            };
            cell.blocks = vec![Block::Para(p)];
            cell.v_merge = crate::docx::VMerge::None;
        }
        let first_para = row.cells.first().and_then(|c| c.blocks.first()).and_then(first_para_of);
        let mut tx = self.begin("Insert Row");
        if self.step(&mut tx, Step::Rows { table, range: after + 1..after + 1, with: vec![row], grid: None }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("insert failed");
        }
        tx.stale_tables.push(table);
        if let Some(p) = first_para {
            self.selection = Selection::Caret(Position::text(story, p, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }

    // Movement

    fn move_caret(&mut self, by: MoveBy, dir: Dir, extend: bool, fonts: &Fonts) -> Outcome {
        let forward = dir == Dir::Forward;
        let was = self.selection;
        if extend
            && let Selection::Cells { table, anchor, focus, shape: selection::CellShape::Rect } = self.selection
            && matches!(by, MoveBy::Line | MoveBy::Char)
        {
            let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
            let g = table::Grid::of(t);
            let mut f = focus;
            match by {
                MoveBy::Line if forward => f.row = (f.row + 1).min(t.rows.len().saturating_sub(1)),
                MoveBy::Line => f.row = f.row.saturating_sub(1),
                _ if forward => f.col = (f.col + 1).min(g.cols.saturating_sub(1)),
                _ => f.col = f.col.saturating_sub(1),
            }
            self.set_selection(Selection::Cells { table, anchor, focus: f, shape: selection::CellShape::Rect });
            return Outcome::Moved;
        }
        if !extend
            && !self.selection.is_collapsed()
            && let Some((a, b)) = self.selection.ordered(&self.index).or_else(|| self.selection_edges())
        {
            let edge = Selection::Caret(if forward { b } else { a });
            if matches!(by, MoveBy::Char) {
                self.set_selection(edge);
                return Outcome::Moved;
            }
            if self.selection.focus().is_none() {
                self.selection = edge;
            }
        }
        let Some(focus) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
        if matches!(by, MoveBy::Line | MoveBy::LineEdge | MoveBy::Screen | MoveBy::Page | MoveBy::ScreenEdge) {
            self.ensure(&focus, fonts);
        }
        let target = match by {
            MoveBy::Char | MoveBy::Word => {
                let word = by == MoveBy::Word;
                let t = if forward { self.nav().right(focus, word) } else { self.nav().left(focus, word) };
                if t.same_place(&focus) { self.next_note_edge(focus.story, forward).or(Some(t)) } else { Some(t) }
            }
            MoveBy::Para => Some(if forward { self.nav().next_para(focus) } else { self.nav().prev_para(focus) }),
            MoveBy::Story => match focus.story {
                StoryId::Footnote(_) | StoryId::Endnote(_) | StoryId::TextBox(_) => self.nav().story_edge(StoryId::Body, forward),
                s => self.nav().story_edge(s, forward),
            },
            MoveBy::RowEdge => self.nav().table_edge(focus, true, forward),
            MoveBy::ColumnEdge => self.nav().table_edge(focus, false, forward),
            MoveBy::LineEdge => self.nav().line_edge(focus, forward).or_else(|| self.clipped_line_edge(focus, forward)),
            MoveBy::Line => {
                let goal = match self.goal {
                    Some(g) => g,
                    None => {
                        let g = match self.nav().caret_point(focus) {
                            Some(g) => g,
                            None => self.clipped_x(focus).unwrap_or([0.0; 2]),
                        };
                        self.goal = Some(g);
                        g
                    }
                };
                match self.clipped_vertical(focus, goal, forward) {
                    Some(t) => Some(t),
                    None => {
                        let t = self.nav().vertical(focus, goal, forward);
                        t.or_else(|| self.cross_vertical(focus, goal, forward, fonts)).or_else(|| self.nav().story_edge(focus.story, forward))
                    }
                }
            }
            MoveBy::Screen | MoveBy::Page if matches!(focus.story, StoryId::HeaderFooter(_)) => self.page_header_footer(focus, forward, fonts),
            MoveBy::Screen | MoveBy::Page | MoveBy::ScreenEdge => self.page_move(by, forward, focus, fonts),
        };
        let Some(target) = target else {
            self.selection = was;
            return Outcome::Unchanged("no place to move to");
        };
        let goal = self.goal;
        let s = if extend {
            let anchor = self.selection.anchor().unwrap_or(focus);
            match self.drag_across_cells(anchor, target, false) {
                Some((s, _)) => s,
                None => Selection::range(anchor, target, Granularity::Char),
            }
        } else {
            Selection::Caret(target)
        };
        self.set_selection(s);
        if matches!(by, MoveBy::Line | MoveBy::Screen) {
            self.goal = goal;
        }
        self.ensure(&target, fonts);
        match self.caret_view() {
            // Ctrl+Home: the top of the first page.
            Some(c) if by == MoveBy::Story && !forward && target.story == StoryId::Body => {
                self.requests.push(Request::ScrollIntoView { page: c.page, rect: [c.x, -view::PAGE_GAP, c.x + 1.0, c.bottom], at_top: true });
            }
            _ => self.scroll_to_caret(fonts),
        }
        Outcome::Moved
    }

    /// The first and last text places of a cell range or an inline object.
    fn selection_edges(&self) -> Option<(Position, Position)> {
        match self.selection {
            Selection::Object(selection::ObjectRef::Inline { para, offset }) => {
                let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
                Some((Position::text(story, para, offset), Position::text(story, para, Unit(offset.0 + 1))))
            }
            Selection::Cells { table, anchor, focus, .. } => {
                let t = self.index.table(&self.doc, table)?;
                let g = table::Grid::of(t);
                let at = |r: usize, c: usize| {
                    let cell = g.cell_at(r, c)?;
                    self.cell_content(table, r, cell)?.ordered(&self.index)
                };
                let (a, _) = at(anchor.row.min(focus.row), anchor.col.min(focus.col))?;
                let (_, b) = at(anchor.row.max(focus.row), anchor.col.max(focus.col))?;
                Some((a, b))
            }
            _ => None,
        }
    }

    fn page_move(&mut self, by: MoveBy, forward: bool, focus: Position, fonts: &Fonts) -> Option<Position> {
        let stack = self.stack();
        let caret = self.caret_view()?;
        let (cx, cy) = stack.to_doc(caret.page, caret.x, caret.top)?;
        let x = self.goal.get_or_insert([caret.x, (caret.top + caret.bottom) / 2.0])[0];
        let (tx, ty) = match by {
            MoveBy::Screen => {
                let dy = if forward { self.viewport.height } else { -self.viewport.height };
                self.viewport.top = (self.viewport.top + dy).clamp(0.0, (stack.height - self.viewport.height).max(0.0));
                (cx, cy + dy)
            }
            MoveBy::Page => {
                let page = if forward { (caret.page + 1).min(stack.pages.len() - 1) } else { caret.page.saturating_sub(1) };
                let r = stack.pages[page];
                let top = self.snapshot().pages()[page].body_area.map_or(72.0, |a| a.0);
                (r[0] + x, r[1] + top + 1.0)
            }
            _ => {
                let y = if forward { self.viewport.top + self.viewport.height - 4.0 } else { self.viewport.top + 4.0 };
                (cx, y)
            }
        };
        let (page, px, py) = stack.page_at(tx, ty.clamp(0.0, stack.height))?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let want = focus.story;
        let hit = geo.hit_text(px, py, &|s| s == want)?;
        Some(Position { story: hit.story, place: Place::Text { para: hit.para, offset: hit.unit, affinity: hit.affinity } })
    }

    // Pointer

    fn hit(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<Position> {
        let (p, _) = self.hit_full(x, y, fonts)?;
        Some(p)
    }

    fn hit_laid_out(&mut self, x: f64, y: f64) -> Option<Position> {
        let (page, px, py) = self.stack().page_at(x, y)?;
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        let hit = geo.hit_text(px, py, &|s| stories::reachable(ctx, s))?;
        Some(Position { story: hit.story, place: Place::Text { para: hit.para, offset: hit.unit, affinity: hit.affinity } })
    }

    fn hit_full(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(Position, bool)> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        let hit = geo.hit_text(px, py, &|s| stories::reachable(ctx, s))?;
        self.near = page;
        Some((Position { story: hit.story, place: Place::Text { para: hit.para, offset: hit.unit, affinity: hit.affinity } }, hit.repeat))
    }

    fn bar_hit(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(Position, Position)> {
        let ctx = self.editing();
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let section = self.snapshot().info().get(page).map_or(0, |i| i.section);
        // Lines' start margin
        let start = |s: &crate::docx::Section| if crate::layout::vertical::of_section(s.text_flow).is_some() { s.top } else { s.left };
        let left = self.doc.sections.get(section).map_or(72.0, start) - 1.5 / self.viewport.zoom.max(0.1);
        let i = geo.selection_bar(px, py, left, &|s| stories::reachable(ctx, s) && !matches!(s, StoryId::BuildingBlock(_)))?;
        self.near = page;
        self.line_span_of(&geo, i)
    }

    fn line_span_of(&mut self, geo: &geometry::PageGeometry, i: usize) -> Option<(Position, Position)> {
        let l = &geo.lines[i];
        let story = geo.stories[l.story].story;
        let (first, last) = (l.stops.first()?, l.stops.last()?);
        let a = Position::text(story, first.para, first.unit);
        let len = self.index.para(&self.doc, last.para).map_or(Unit(0), para_len);
        let b = if last.unit >= len {
            self.after_mark(Position::text(story, last.para, len))
        } else {
            Position::text(story, last.para, last.unit).with_affinity(Affinity::Upstream)
        };
        Some((a, b))
    }

    fn line_span(&mut self, p: Position) -> (Position, Position) {
        match self.nav().line_of(p) {
            Some(at) => self.line_span_of(&at.geo, at.line).unwrap_or((p, p)),
            None => (p, p),
        }
    }

    fn after_mark(&mut self, end: Position) -> Position {
        let Some((para, _)) = end.para_offset() else { return end };
        let next = self.nav().right(end, false);
        match (next.para_offset(), self.index.para_slot(para)) {
            (Some((q, Unit(0))), Some(s)) if self.index.para_slot(q).is_some_and(|t| t.container == s.container && t.block == s.block + 1) => next,
            _ => end,
        }
    }

    fn span(&mut self, p: Position, unit: Granularity) -> (Position, Position) {
        match unit {
            Granularity::Word => self.word_span(p),
            Granularity::Sentence => self.sentence_span(p),
            Granularity::Paragraph => {
                let (a, b) = self.para_span(p);
                (a, self.after_mark(b))
            }
            Granularity::Line => self.line_span(p),
            Granularity::Char => (p, p),
        }
    }

    fn zone_hit(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<geometry::Zone> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let hovered = self.table_hover.map(|h| h.table);
        let z = geo.zone(px, py, 0.75 / self.viewport.zoom.max(0.1), hovered)?;
        let table = match z {
            geometry::Zone::Cell(t, ..)
            | geometry::Zone::Row(t, _)
            | geometry::Zone::Column(t, ..)
            | geometry::Zone::Table(t)
            | geometry::Zone::InsertColumn(t, _)
            | geometry::Zone::InsertRow(t, _) => t,
        };
        let story = self.index.table_slot(table).map(|s| self.index.host(self.index.story(s.container)));
        (story.map(|s| stories::reachable(self.editing(), s)) == Some(true)).then_some(z)
    }

    fn pointer(&mut self, kind: PointerKind, x: f64, y: f64, clicks: u8, mods: keymap::Mods, fonts: &Fonts) {
        if self.shape_insert_pointer(kind, x, y, mods.shift, fonts) {
            return;
        }
        match kind {
            PointerKind::Down if self.move_to_press(x, y, fonts) => {}
            PointerKind::Down => {
                let mods = if self.modes.extend > 0 && clicks == 1 { keymap::Mods { shift: true, ..mods } } else { mods };
                self.goal = None;
                self.history.break_typing();
                if !mods.shift && self.wrap_points_press(x, y, mods.ctrl, fonts) {
                    return;
                }
                if self.linking_press(x, y, fonts) {
                    return;
                }
                if !mods.shift && self.anchor_press(x, y) {
                    return;
                }
                if !mods.shift && !mods.ctrl && self.marquee_press(x, y, fonts) {
                    return;
                }
                if self.region_title_click(x, y, fonts) {
                    return;
                }
                if !mods.shift && self.expansion_click(x, y, clicks, fonts) {
                    return;
                }
                if clicks >= 2 && !mods.shift && self.stack().toggles_white_space(y, 3.0 / self.viewport.zoom.max(0.1)) {
                    self.drag = None;
                    if clicks == 2 {
                        self.set_white_space_hidden(!self.white_space_hidden);
                    }
                    return;
                }
                if self.story_click(x, y, clicks, fonts) {
                    return;
                }
                if !mods.ctrl && self.shape_press(x, y) {
                    return;
                }
                if (mods.shift || mods.ctrl) && self.object_toggle(x, y, fonts) {
                    return;
                }
                if !mods.shift && self.object_press(x, y, mods.ctrl, fonts) {
                    return;
                }
                if !mods.shift && self.checkbox_press(x, y, fonts) {
                    return;
                }
                if clicks == 1
                    && !mods.shift
                    && (mods.ctrl || mods.meta || self.mode == Mode::ReadOnly)
                    && let Some(address) = self.link_under(x, y, fonts)
                {
                    match address.strip_prefix('#') {
                        Some(name) => _ = self.go_to_bookmark(name, false, fonts),
                        None => self.events.push(input::Notice::Link { address, ctrl: mods.ctrl || mods.meta }),
                    }
                    return;
                }
                if clicks == 2
                    && let Some((_, table, resize::Border::Column(k), ..)) = self.border_at(x, y, fonts)
                {
                    self.border_cancel();
                    self.history.break_typing();
                    if !self.table_gesture_kept(table, crate::api::command::CommandId::AutoFitColumns) {
                        self.auto_fit_column(table, k.saturating_sub(1), fonts);
                    }
                    return;
                }
                if clicks < 2 && self.border_press(x, y, mods.shift, mods.ctrl, fonts) {
                    return;
                }
                if let Some(z) = self.zone_hit(x, y, fonts) {
                    let (table, row, cell, shape) = match z {
                        geometry::Zone::Cell(t, r, c) if clicks < 2 => (t, r, c, selection::CellShape::Rect),
                        geometry::Zone::Cell(t, r, c) => (t, r, c, selection::CellShape::Rows),
                        geometry::Zone::Row(t, r) => (t, r, 0, selection::CellShape::Rows),
                        geometry::Zone::Column(t, r, c) => (t, r, c, selection::CellShape::Columns),
                        geometry::Zone::Table(t) => {
                            self.set_selection(Selection::Cells {
                                table: t,
                                anchor: selection::CellRef { row: 0, col: 0 },
                                focus: selection::CellRef { row: usize::MAX, col: usize::MAX },
                                shape: selection::CellShape::Table,
                            });
                            self.drag = None;
                            if !self.table_gesture_kept(t, crate::api::command::CommandId::SetTablePosition) {
                                self.table_press(t, x, y);
                            }
                            return;
                        }
                        geometry::Zone::InsertColumn(t, k) => {
                            if !self.table_gesture_kept(t, crate::api::command::CommandId::InsertColumns) {
                                self.insert_column_at(t, k, fonts);
                            }
                            return;
                        }
                        geometry::Zone::InsertRow(t, k) => {
                            if !self.table_gesture_kept(t, crate::api::command::CommandId::InsertRows) {
                                self.insert_row_at(t, k, fonts);
                            }
                            return;
                        }
                    };
                    if let Some(t) = self.index.table(&self.doc, table) {
                        let g = table::Grid::of(t);
                        let col = g.col_of(row, cell).unwrap_or(0);
                        let span = t.rows.get(row).and_then(|r| r.cells.get(cell)).map_or(1, |c| c.span.max(1));
                        let (a, f) = match shape {
                            selection::CellShape::Rows => ((row, 0), (row, g.cols.saturating_sub(1))),
                            selection::CellShape::Columns => ((0, col), (t.rows.len().saturating_sub(1), col + span - 1)),
                            _ => ((row, col), (row, col + span - 1)),
                        };
                        let adding = self.zone_press_adds(table, mods);
                        self.set_selection(Selection::Cells {
                            table,
                            anchor: selection::CellRef { row: a.0, col: a.1 },
                            focus: selection::CellRef { row: f.0, col: f.1 },
                            shape,
                        });
                        self.zone_added(adding);
                        self.drag = None;
                        self.zone_drag = Some((table, selection::CellRef { row: a.0, col: a.1 }, shape));
                        self.requests.push(Request::CapturePointer);
                        return;
                    }
                }
                if let Some((a, b)) = self.bar_hit(x, y, fonts) {
                    let (unit, (a, b)) = match clicks {
                        _ if mods.ctrl || clicks >= 3 => {
                            // Story selection
                            self.selection = Selection::Caret(a);
                            self.select_all();
                            return;
                        }
                        2 => (Granularity::Paragraph, self.span(a, Granularity::Paragraph)),
                        _ => (Granularity::Line, (a, b)),
                    };
                    self.set_selection(Selection::range(a, b, unit));
                    self.drag = Some(Drag { anchor: a, span: Some((a, b)), unit, at: (x, y), block: false, adding: false, column: false });
                    self.requests.push(Request::CapturePointer);
                    return;
                }
                if clicks == 2
                    && !mods.shift
                    && !mods.ctrl
                    && let Some(t) = self.click_and_type_at(x, y, fonts)
                {
                    self.drag = None;
                    self.click_and_type(t, fonts);
                    return;
                }
                if clicks == 1 && !mods.shift && !mods.ctrl && !mods.meta && !mods.alt && self.label_press(x, y, fonts) {
                    return;
                }
                let Some(p) = self.hit(x, y, fonts) else { return };
                if clicks == 2 && self.note_jump(p) {
                    return;
                }
                if clicks == 1
                    && !mods.shift
                    && !self.text_drag_off
                    && !mods.alt
                    && self.modes == extend::Modes::default()
                    && self.mode != Mode::ReadOnly
                    && self.highlighter.is_none()
                    && (self.inside_selection(&p) || !(mods.ctrl || (self.mac && mods.meta)) && self.inside_any_range(&p))
                {
                    self.text_drag = Some(interaction::TextDrag { start: (x, y), press: p, moved: false, copy: mods.ctrl, drop: None, menu: false });
                    self.ctrl_press_on_selection(mods);
                    self.requests.push(Request::CapturePointer);
                    return;
                }
                if self.add_press(p, x, y, clicks, mods) {
                    return;
                }
                // The double-click that opens a selected text box puts the caret there.
                let opening = matches!(self.selection, Selection::Object(_)) && matches!(p.story, StoryId::TextBox(_));
                let unit = match clicks {
                    2 if opening => Granularity::Char,
                    2 => Granularity::Word,
                    3.. => Granularity::Paragraph,
                    _ if mods.ctrl && !mods.shift => Granularity::Sentence,
                    _ => Granularity::Char,
                };
                if unit == Granularity::Char
                    && mods.shift
                    && let Some((sa, sb)) = self.selection.ordered(&self.index)
                {
                    let before = self.index.cmp(&p, &sa) == std::cmp::Ordering::Less;
                    let (s, unit) = match self.selection {
                        Selection::Text { unit: Granularity::Word, .. } => {
                            let (pa, pb) = self.word_span(p);
                            (
                                if before { Selection::range(sb, pa, Granularity::Word) } else { Selection::range(sa, pb, Granularity::Word) },
                                Granularity::Word,
                            )
                        }
                        _ => {
                            let anchor = self.selection.anchor().unwrap_or(p);
                            let (wa, wb) = self.word_span(anchor);
                            let inside =
                                self.index.cmp(&p, &wa) != std::cmp::Ordering::Less && self.index.cmp(&p, &wb) != std::cmp::Ordering::Greater;
                            let forward = self.index.cmp(&p, &anchor) != std::cmp::Ordering::Less;
                            let anchor = if inside {
                                anchor
                            } else if forward {
                                wa
                            } else {
                                wb
                            };
                            (Selection::range(anchor, p, Granularity::Char), Granularity::Char)
                        }
                    };
                    let span = (unit == Granularity::Word).then_some((sa, sb));
                    let anchor = s.anchor().unwrap_or(p);
                    self.set_selection(s);
                    self.drag = Some(Drag { anchor, span, unit, at: (x, y), block: false, adding: false, column: false });
                } else if unit == Granularity::Char {
                    self.set_selection(Selection::Caret(p));
                    self.drag = Some(Drag { anchor: p, span: None, unit, at: (x, y), block: false, adding: false, column: false });
                } else {
                    let (a, b) = self.span(p, unit);
                    self.set_selection(Selection::range(a, b, unit));
                    self.drag = Some(Drag { anchor: a, span: Some((a, b)), unit, at: (x, y), block: false, adding: false, column: false });
                }
                self.requests.push(Request::CapturePointer);
            }
            PointerKind::Move => {
                if self.wrap_point_move(x, y) || self.marquee_move(x, y) || self.anchor_move(x, y) {
                    return;
                }
                if self.text_drag.is_some() {
                    self.text_drag_move(x, y, mods.ctrl, fonts);
                    return;
                }
                if self.object_drag.is_some() {
                    self.object_move(x, y, mods, fonts);
                    return;
                }
                if self.shape_drag.is_some() {
                    self.shape_move(x, y, mods);
                    return;
                }
                if let Some(d) = self.border_drag.as_mut() {
                    d.now = (x, y);
                    d.snapping = if mods.alt { snap::Snapping::Free } else { snap::Snapping::On };
                    self.border_moved(fonts);
                    return;
                }
                if self.table_drag.is_some() {
                    self.table_drag_move(x, y, mods);
                    return;
                }
                if let Some((table, anchor, shape)) = self.zone_drag {
                    if let Some((row, col)) = self.grid_cell_at(table, x, y, fonts) {
                        let focus = match shape {
                            selection::CellShape::Rows => selection::CellRef { row, col: usize::MAX },
                            selection::CellShape::Columns => selection::CellRef { row: usize::MAX, col },
                            _ => selection::CellRef { row, col },
                        };
                        let a = match shape {
                            selection::CellShape::Rows => selection::CellRef { row: anchor.row, col: 0 },
                            selection::CellShape::Columns => selection::CellRef { row: 0, col: anchor.col },
                            _ => anchor,
                        };
                        self.set_selection(Selection::Cells { table, anchor: a, focus, shape });
                        if self.adding.is_some() {
                            self.multi.primary = Some(self.selection);
                        }
                    }
                    return;
                }
                let Some(mut d) = self.drag else {
                    self.cursor = self.hover(x, y, mods, fonts);
                    self.set_hover(x, y);
                    self.table_hover = self.table_hover_at(x, y, fonts);
                    return;
                };
                d.at = (x, y);
                self.drag = Some(d);
                self.autoscroll(x, y);
                let Some(p) = self.hit(x, y, fonts) else { return };
                if p.story != d.anchor.story {
                    return;
                }
                if d.column {
                    self.selection = if p.same_place(&d.anchor) { Selection::Caret(p) } else { Selection::Block { anchor: d.anchor, focus: p } };
                    self.pending = None;
                    return;
                }
                let crossed = if d.unit == Granularity::Char { self.drag_across_cells(d.anchor, p, d.block) } else { None };
                if let Some((_, true)) = crossed
                    && let Some(dd) = self.drag.as_mut()
                {
                    dd.block = true;
                }
                let s = match crossed {
                    // Over whole merged cells, as a plain drag's selection is
                    Some((s, _)) => self.normalize(s),
                    None => self.dragged_selection(d, p),
                };
                if d.adding && self.removing() && !self.sweeps_anew(&s) {
                    return;
                }
                self.selection = s;
                if d.adding {
                    self.multi.primary = Some(self.selection);
                    if self.removing() {
                        self.show_removal(p);
                    }
                }
                self.pending = None;
            }
            PointerKind::Cancel if self.wrap_point_cancel() => {}
            PointerKind::Cancel if self.marquee_cancel() => {}
            PointerKind::Up if self.marquee.is_some() => {
                self.marquee_release(fonts);
            }
            PointerKind::Up if self.anchor_drag.is_some() => {
                self.anchor_release(fonts);
            }
            PointerKind::Up if self.wrap_point_dragging() => {
                self.wrap_point_release(fonts);
            }
            PointerKind::Cancel if self.object_cancel() => {}
            PointerKind::Cancel if self.shape_cancel() => {}
            PointerKind::Up if self.shape_drag.is_some() => {
                self.shape_release(fonts);
            }
            PointerKind::Cancel if self.border_cancel() => self.requests.push(Request::ReleasePointer),
            PointerKind::Up if self.border_drag.is_some() => {
                self.border_release(fonts);
            }
            PointerKind::Cancel if self.table_drag.take().is_some() => self.requests.push(Request::ReleasePointer),
            PointerKind::Up if self.table_drag.is_some() => {
                self.table_drag_release(fonts);
            }
            PointerKind::Up if self.object_drag.is_some() => {
                self.object_release(fonts);
            }
            PointerKind::Up if self.text_drag.is_some_and(|d| !d.moved) && self.removing() => {
                if let Some(d) = self.text_drag.take() {
                    self.requests.push(Request::ReleasePointer);
                    self.add_release(d.press, false);
                }
            }
            PointerKind::Up if self.text_drag.is_some() => {
                self.adding = None;
                self.text_drag_release(fonts);
            }
            PointerKind::Cancel if self.text_drag.take().is_some() => {
                self.adding = None;
                self.requests.push(Request::ReleasePointer);
            }
            PointerKind::Up | PointerKind::Cancel if self.zone_drag.take().is_some() => {
                self.requests.push(Request::ReleasePointer);
                if self.adding.is_some()
                    && let Some(at) = self.cells_start(&self.selection)
                {
                    self.zone_release(at, kind == PointerKind::Cancel);
                }
                self.adding = None;
            }
            PointerKind::Up | PointerKind::Cancel => {
                if let Some(d) = self.drag.filter(|d| d.adding) {
                    self.add_release(d.anchor, kind == PointerKind::Cancel);
                }
                if self.drag.take().is_some() {
                    self.requests.push(Request::ReleasePointer);
                    self.requests.push(Request::Autoscroll { vx: 0.0, vy: 0.0 });
                }
                if kind == PointerKind::Up {
                    self.highlighter_release(fonts);
                }
            }
        }
    }

    /// A selecting drag's selection from its press to `p` within text: by words once it leaves the press's word, by
    /// its unit for a drag from a double or triple click.
    fn dragged_selection(&mut self, d: Drag, p: Position) -> Selection {
        let s = match (d.unit, d.span) {
            (Granularity::Char, _) | (_, None) => {
                let (wa, wb) = self.word_span(d.anchor);
                let beyond = !(self.index.cmp(&p, &wa) != std::cmp::Ordering::Less && self.index.cmp(&p, &wb) != std::cmp::Ordering::Greater);
                if beyond {
                    let (pa, pb) = self.word_span(p);
                    if self.index.cmp(&p, &d.anchor) == std::cmp::Ordering::Less {
                        Selection::range(wb, pa, Granularity::Char)
                    } else {
                        let pb = if pb.para_offset() == p.para_offset() && self.at_para_end(p) { self.after_mark(p) } else { pb };
                        Selection::range(wa, pb, Granularity::Char)
                    }
                } else {
                    Selection::range(d.anchor, p, Granularity::Char)
                }
            }
            (unit, Some((a, b))) => {
                let (pa, pb) = self.span(p, unit);
                if self.index.cmp(&p, &a) == std::cmp::Ordering::Less { Selection::range(b, pa, unit) } else { Selection::range(a, pb, unit) }
            }
        };
        self.normalize(s)
    }

    fn drag_across_cells(&mut self, anchor: Position, p: Position, was_block: bool) -> Option<(Selection, bool)> {
        let a_chain = self.index.chain_of(&anchor);
        let p_chain = self.index.chain_of(&p);
        let forward = self.index.cmp(&p, &anchor) != std::cmp::Ordering::Less;
        let cell_ref = |e: &Editor, table: TableId, row: usize, cell: usize| -> Option<selection::CellRef> {
            let t = e.index.table(&e.doc, table)?;
            Some(selection::CellRef { row, col: table::Grid::of(t).col_of(row, cell.min(t.rows.get(row)?.cells.len().checked_sub(1)?))? })
        };
        if let Some(k) = (0..a_chain.len().saturating_sub(1)).rev().find(|&k| p_chain.get(k).is_some_and(|l| l.table == a_chain[k].table))
            && a_chain.len() > 1
            && p_chain.iter().all(|l| l.table != a_chain[a_chain.len() - 1].table)
        {
            let (al, pl) = (a_chain[k], p_chain[k]);
            if (pl.row, pl.cell) != (al.row, al.cell) {
                let (Some(ac), Some(pc)) = (cell_ref(self, al.table, al.row, al.cell), cell_ref(self, pl.table, pl.row, pl.cell)) else {
                    return None;
                };
                return Some((Selection::Cells { table: al.table, anchor: ac, focus: pc, shape: selection::CellShape::Rect }, true));
            }
        }
        let shared = a_chain.iter().zip(&p_chain).take_while(|(a, b)| a.table == b.table && (a.row, a.cell) == (b.row, b.cell)).count();
        if shared == a_chain.len() && p_chain.len() > shared && !a_chain.is_empty() {
            let pl = p_chain[shared];
            let t = self.index.table(&self.doc, pl.table)?;
            let to = if forward {
                Position::row_end(p.story, pl.table, pl.row)
            } else {
                t.rows
                    .get(pl.row)?
                    .cells
                    .first()
                    .and_then(|c| c.blocks.iter().find_map(first_para_of))
                    .map(|pp| Position::text(p.story, pp, Unit(0)))?
            };
            return Some((Selection::range(anchor, to, Granularity::Char), false));
        }
        if let Some(al) = a_chain.last().copied() {
            if let Some(pl) = p_chain.iter().find(|l| l.table == al.table).copied() {
                let cell = |r: usize, c: usize| (r, c);
                if cell(pl.row, pl.cell) == cell(al.row, al.cell) && pl.cell != usize::MAX {
                    if !was_block {
                        return None;
                    }
                    let c = cell_ref(self, al.table, al.row, al.cell)?;
                    return Some((Selection::Cells { table: al.table, anchor: c, focus: c, shape: selection::CellShape::Rect }, true));
                }
                let (Some(ac), Some(pc)) = (cell_ref(self, al.table, al.row, al.cell), cell_ref(self, pl.table, pl.row, pl.cell)) else {
                    return None;
                };
                return Some((Selection::Cells { table: al.table, anchor: ac, focus: pc, shape: selection::CellShape::Rect }, true));
            }
            let t = self.index.table(&self.doc, al.table)?;
            let story = anchor.story;
            let from = if forward {
                t.rows
                    .get(al.row)?
                    .cells
                    .first()
                    .and_then(|c| c.blocks.iter().find_map(first_para_of))
                    .map(|pp| Position::text(story, pp, Unit(0)))?
            } else {
                Position::row_end(story, al.table, al.row)
            };
            return Some((Selection::range(from, p, Granularity::Char), false));
        }
        let pl = p_chain.first().copied()?;
        let t = self.index.table(&self.doc, pl.table)?;
        let to = if forward {
            Position::row_end(p.story, pl.table, pl.row)
        } else {
            t.rows.get(pl.row)?.cells.first().and_then(|c| c.blocks.iter().find_map(first_para_of)).map(|pp| Position::text(p.story, pp, Unit(0)))?
        };
        Some((Selection::range(anchor, to, Granularity::Char), false))
    }

    fn at_para_end(&self, p: Position) -> bool {
        p.para_offset().is_some_and(|(para, u)| self.index.para(&self.doc, para).is_some_and(|q| para_len(q) == u))
    }

    fn autoscroll(&mut self, x: f64, y: f64) {
        let v = self.viewport;
        let speed = |outside: f64| {
            let d = outside * v.zoom * 96.0 / 72.0;
            if d <= 4.0 { 0.0 } else { (40.0 + 12.0 * d).min(2400.0) }
        };
        let vy = if y > v.top + v.height {
            speed(y - v.top - v.height)
        } else if y < v.top {
            -speed(v.top - y)
        } else {
            0.0
        };
        let vx = if x > v.left + v.width {
            speed(x - v.left - v.width)
        } else if x < v.left {
            -speed(v.left - x)
        } else {
            0.0
        };
        self.requests.push(Request::Autoscroll { vx, vy });
    }

    fn word_span(&self, p: Position) -> (Position, Position) {
        let Some((para, off)) = p.para_offset() else { return (p, p) };
        let Some(t) = self.index.para(&self.doc, para).map(|q| UnitText::in_review(q, &self.doc.review)) else { return (p, p) };
        let (a, b) = t.word_at(off);
        (Position::text(p.story, para, a), Position::text(p.story, para, b))
    }

    fn sentence_span(&self, p: Position) -> (Position, Position) {
        let Some((para, off)) = p.para_offset() else { return (p, p) };
        let Some(t) = self.index.para(&self.doc, para).map(|q| UnitText::in_review(q, &self.doc.review)) else { return (p, p) };
        let (a, b) = t.sentence_at(off);
        (Position::text(p.story, para, a), Position::text(p.story, para, b))
    }

    fn para_span(&self, p: Position) -> (Position, Position) {
        let Some((para, _)) = p.para_offset() else { return (p, p) };
        let len = self.index.para(&self.doc, para).map_or(Unit(0), para_len);
        (Position::text(p.story, para, Unit(0)), Position::text(p.story, para, len))
    }

    // Geometry for the view

    /// The part of the document the host shows, in document points, and its zoom.
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Sets the part of the document the host shows: what a screen move pages by, and what pointer tolerances scale
    /// with.
    pub fn set_viewport(&mut self, v: Viewport) {
        if v.zoom != self.viewport.zoom {
            self.hover_cache = None;
        }
        self.viewport = v;
    }

    pub fn caret_view(&mut self) -> Option<CaretView> {
        let focus = self.selection.focus()?;
        self.caret_at(focus)
    }

    /// How the arrow keys move from the selection's focus: along its line's text flow.
    pub fn caret_arrows(&mut self) -> Option<keymap::Arrows> {
        let focus = self.selection.focus()?;
        let at = self.nav().line_of(focus)?;
        keymap::Arrows::of(at.geo.lines[at.line].axes)
    }

    /// The page showing `p`, once laid out.
    pub fn page_showing(&mut self, p: Position) -> Option<usize> {
        self.caret_at(p).map(|c| c.page)
    }

    fn caret_at(&mut self, focus: Position) -> Option<CaretView> {
        let (para, unit) = focus.para_offset()?;
        let affinity = focus.affinity();
        let near = self.near;
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        if let Some((page, geo, i)) = self.pages.line_showing(para, unit, affinity, near, &self.index) {
            let l = &geo.lines[i];
            let x = l.x_of(para, unit)?;
            self.near = page;
            return Some(CaretView::of(page, geometry::CaretRect::turned(l.turn, x, l.top, l.bottom)));
        }
        for page in self.pages.pages_of(para, near, &self.index) {
            let geo = self.pages.page(page, &self.index)?;
            if let Some(c) = geo.caret(para, unit, affinity) {
                self.near = page;
                return Some(CaretView::of(page, c));
            }
        }
        None
    }

    fn scroll_to_caret(&mut self, _fonts: &Fonts) {
        if let Some(c) = self.caret_view() {
            self.requests.push(Request::ScrollIntoView { page: c.page, rect: c.rect(), at_top: false });
        }
    }

    /// After a jump (a link, a bookmark, a page): the caret's line at the top of the view, as Word puts it.
    fn jump_to_caret(&mut self) {
        if let Some(c) = self.caret_view() {
            self.requests.push(Request::ScrollIntoView { page: c.page, rect: c.rect(), at_top: true });
        }
    }

    /// Where the selection is on `pages`: nothing for a caret.
    pub fn selection_bounds(&mut self, pages: std::ops::Range<usize>) -> view::SelectionBounds {
        let mut out = view::SelectionBounds::default();
        match self.selection_view(pages) {
            SelectionView::None => {}
            SelectionView::Text(r) => out.rects = r.into_iter().map(|(page, b)| (page, b.page())).collect(),
            SelectionView::Cells(r) => out.rects = r,
            SelectionView::Object(f, _) => out.frames = f,
        }
        let boxes = out.rects.iter().copied().chain(out.frames.iter().map(|f| (f.page, f.turned_box())));
        for (page, r) in boxes {
            match out.bounds.iter_mut().find(|(p, _)| *p == page) {
                Some((_, b)) => *b = [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[2]), b[3].max(r[3])],
                None => out.bounds.push((page, r)),
            }
        }
        out
    }

    fn selection_view(&mut self, pages: std::ops::Range<usize>) -> SelectionView {
        if let Selection::Object(o) = self.selection {
            let mut out = Vec::new();
            for page in pages {
                let Some(geo) = self.pages.page(page, &self.index) else { continue };
                match o {
                    _ if let Some(d) = self.object_drag.filter(|d| d.obj == o && !d.live) => {
                        if page == d.over {
                            out.push(view::Outline { page, rect: d.on_over(d.rect(self.object_locked(o))), rot: d.rot });
                        }
                    }
                    selection::ObjectRef::Floating(_) => {
                        for f in self.selected_floats() {
                            if geo.stories.iter().any(|s| s.float == Some(f))
                                && let Some((rect, rot)) = self.float_frame(f, page)
                            {
                                out.push(view::Outline { page, rect, rot });
                            }
                        }
                    }
                    selection::ObjectRef::Inline { para, offset } => {
                        // Turned live by a rotate drag: an inline object isn't laid out until the drop.
                        let turn = match &self.shape_drag {
                            Some(d) if d.obj == o && d.handle == shapes::ShapeHandle::Rotate => d.angle(),
                            _ => self.inline_turn(para, offset),
                        };
                        out.extend(self.inline_frame(&geo, para, offset).map(|(rect, rot)| view::Outline { page, rect, rot: rot + turn }))
                    }
                }
            }
            let dragged = self.object_drag.is_some_and(|d| d.now != d.start) || self.shape_drag.is_some();
            return SelectionView::Object(out, !self.object_fixed(o) && !dragged);
        }
        if self.is_discontiguous() {
            return SelectionView::Text(self.multi_rects(pages));
        }
        if let Selection::Block { .. } = self.selection {
            return SelectionView::Text(
                self.block_pieces().into_iter().filter(|(page, ..)| pages.contains(page)).map(|(page, r, ..)| (page, r)).collect(),
            );
        }
        if let Selection::Cells { .. } = self.selection {
            return SelectionView::Cells(self.block_rects(self.selection, pages));
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return SelectionView::None };
        if a.same_place(&b) {
            return SelectionView::None;
        }
        SelectionView::Text(self.range_rects(a, b, pages))
    }

    /// The cells of block `s` on `pages`.
    pub(super) fn block_rects(&mut self, s: Selection, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let Selection::Cells { table, anchor, focus, .. } = s else { return Vec::new() };
        let (r0, r1) = (anchor.row.min(focus.row), anchor.row.max(focus.row));
        let (c0, c1) = (anchor.col.min(focus.col), anchor.col.max(focus.col));
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for c in &geo.cells {
                let Some(&(t, row, _)) = c.chain.last() else { continue };
                if t == table && (r0..=r1).contains(&row) && c.col <= c1 && c.col + c.span.max(1) > c0 {
                    out.push((page, c.page_rect()));
                }
            }
        }
        out
    }

    pub(super) fn range_rects(&mut self, a: Position, b: Position, pages: std::ops::Range<usize>) -> Vec<(usize, view::Band)> {
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for (l, para) in geo.lines.iter().flat_map(|l| l.paras.iter().map(move |&p| (l, p))) {
                if geo.stories[l.story].story == a.story
                    && let Some(band) = range_band(&self.index, l, para, &a, &b)
                {
                    out.push((page, band));
                }
            }
        }
        out
    }

    /// The highlight of `ranges` (in order of their starts, all of one story) on `pages`: the lines are walked once,
    /// each meeting only the ranges that reach it.
    pub(super) fn ranges_rects(&mut self, ranges: &[(Position, Position)], pages: std::ops::Range<usize>) -> Vec<(usize, view::Band)> {
        let mut out = Vec::new();
        let Some(story) = ranges.first().map(|r| r.0.story) else { return out };
        // Overlapping ranges (the selection's own, mid-drag) as one, for their ends to come in order
        let mut union: Vec<(Position, Position)> = Vec::with_capacity(ranges.len());
        for &(a, b) in ranges {
            match union.last_mut() {
                Some(last) if self.index.cmp(&a, &last.1).is_lt() => {
                    if self.index.cmp(&b, &last.1).is_gt() {
                        last.1 = b;
                    }
                }
                _ => union.push((a, b)),
            }
        }
        let ranges = &union[..];
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for (l, para) in geo.lines.iter().flat_map(|l| l.paras.iter().map(move |&p| (l, p))) {
                if geo.stories[l.story].story != story {
                    continue;
                }
                let (Some(lf), Some(ll)) = (l.first_of(para), l.last_of(para)) else { continue };
                let (first, last) = (Position::text(story, para, lf), Position::text(story, para, ll));
                let from = ranges.partition_point(|r| self.index.cmp(&r.1, &first) == std::cmp::Ordering::Less);
                for (a, b) in &ranges[from..] {
                    if self.index.cmp(a, &last) == std::cmp::Ordering::Greater {
                        break;
                    }
                    if let Some(band) = range_band(&self.index, l, para, a, b) {
                        out.push((page, band));
                    }
                }
            }
        }
        out
    }

    pub fn view(&mut self, visible: std::ops::Range<usize>) -> View {
        let caret = self.caret_view();
        let caret_hidden = if caret.is_none() { self.caret_hidden() } else { None };
        let selection = self.selection_view(visible.clone());
        let story = self.editing();
        let anchor = match self.selection {
            _ if self.anchor_drag.is_some() => self.anchor_drag_view(),
            Selection::Object(selection::ObjectRef::Floating(f)) => {
                let locked = self.selected_object().and_then(|o| o.locked) == Some(true);
                let moving = self.object_drag.filter(|d| d.handle.is_none() && d.now != d.start && !locked);
                let to = moving.and_then(|d| {
                    let (para, ..) = self.anchor_at(d.over, self.index.float_anchor(f)?, d.on_over(d.placed(d.rect(false))))?;
                    Some((d.over, para))
                });
                match to {
                    Some((page, para)) => self.anchor_point(page, para).map(|p| (page, p)),
                    None => self.index.float_anchor(f).and_then(|para| {
                        let page = *self.pages.pages_of(para, self.near, &self.index).first()?;
                        self.anchor_point(page, para).map(|p| (page, p))
                    }),
                }
            }
            _ => None,
        };
        let ghost = self.object_drag.filter(|d| d.now != d.start && !d.live).map(|d| view::Ghost {
            page: d.over,
            source: d.page,
            float: match d.obj {
                selection::ObjectRef::Floating(f) => Some(f),
                selection::ObjectRef::Inline { .. } => None,
            },
            from: d.rect,
            to: d.on_over(d.rect(self.object_locked(d.obj))),
            rot: d.rot,
        });
        let (mut guides, mut measures, original) = self.object_feedback(visible.clone(), anchor);
        let (table, table_preview) = self.table_feedback(visible.clone(), &mut guides, &mut measures);
        let visible_pages = visible.clone();
        let (marks, tooltip) = self.mark_views(visible.clone());
        let (comments, comment_anchors) = self.comment_views(visible.clone());
        let revision_balloons = self.revision_balloons(visible.clone());
        let mut fade = Vec::new();
        if matches!(story, StoryId::HeaderFooter(_)) {
            for page in visible.clone() {
                if let Some(geo) = self.pages.page(page, &self.index)
                    && let Some((top, bottom)) = geo.body_area
                {
                    fade.push((page, [0.0, top, geo.w, bottom]));
                }
            }
        }
        View {
            snapshot: self.layout.snapshot(),
            revision: self.revision,
            stack: self.stack(),
            pages_visible: visible,
            caret,
            caret_hidden,
            selection,
            story,
            fade,
            guide: self.border_drag.as_ref().map(|d| (d.page, d.page_guide())),
            anchor: anchor.filter(|_| self.mark_options.all || self.mark_options.anchors),
            marquee: self.marquee_rect(),
            ghost,
            shapes: self.shape_frames(visible_pages.clone()),
            wrap_points: self.wrap_points(visible_pages.clone()),
            guides,
            measures,
            original,
            table,
            table_preview,
            correction: self.correction_view(visible_pages.clone()),
            composition: self.composition_rects(visible_pages.clone()),
            marks,
            tooltip,
            regions: self.region_views(visible_pages.clone()),
            comments,
            comment_anchors,
            revision_balloons,
            spelling: Vec::new(),
            formatting_marks: Vec::new(),
            boundaries: Vec::new(),
            field_shading: Vec::new(),
            editable_regions: Vec::new(),
            list_labels: self.list_label_rects(visible_pages.clone()),
            drop: self
                .text_drag
                .filter(|d| d.moved)
                .and_then(|d| d.drop)
                .or(self.drop)
                .or(self.drop_menu())
                .or(self.move_to.and_then(|m| m.target))
                .and_then(|p| self.caret_at(p)),
            cursor: self.drag_cursor().unwrap_or(self.cursor),
        }
    }

    pub fn selection_contains(&mut self, x: f64, y: f64) -> bool {
        let Some((page, px, py)) = self.stack().page_at(x, y) else { return false };
        match self.selection_view(page..page + 1) {
            SelectionView::Text(r) => r.iter().any(|(p, b)| *p == page && b.contains(px, py)),
            SelectionView::Cells(r) => r.iter().any(|(p, [x0, y0, x1, y1])| *p == page && px >= *x0 && px <= *x1 && py >= *y0 && py <= *y1),
            SelectionView::Object(r, _) => r.iter().any(|o| {
                let [x0, y0, x1, y1] = o.rect;
                let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                let (ux, uy) = objects::turned((px - cx, py - cy), -o.rot);
                o.page == page && cx + ux >= x0 && cx + ux <= x1 && cy + uy >= y0 && cy + uy <= y1
            }),
            SelectionView::None => false,
        }
    }

    pub fn formatting_marks(&mut self, pages: std::ops::Range<usize>) -> Vec<view::FormattingMark> {
        self.formatting_marks_with(pages, view::MarkOptions::ALL)
    }

    /// The formatting marks `o` shows on `pages`, as Word draws them: spaces, tabs, breaks with their labels, the
    /// paragraph, cell and row marks and hidden text's underline.
    pub fn formatting_marks_with(&mut self, pages: std::ops::Range<usize>, o: view::MarkOptions) -> Vec<view::FormattingMark> {
        use view::{FormattingMark, MarkKind};
        let mut out = Vec::new();
        if !o.any() {
            return out;
        }
        let (spaces, tabs, paragraphs) = (o.all || o.spaces, o.all || o.tabs, o.all || o.paragraphs);
        let mut texts: HashMap<ParaId, UnitText> = HashMap::new();
        let snapshot = self.snapshot();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            // Sections ending on the page
            let info = snapshot.info();
            let sections = info.get(page).map_or(0, |i| i.section)..info.get(page + 1).map_or(self.doc.sections.len(), |i| i.section + 1);
            for line in &geo.lines {
                for &para in line.paras.iter() {
                    let text = match texts.entry(para) {
                        std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                        std::collections::hash_map::Entry::Vacant(e) => {
                            let Some(p) = self.index.para(&self.doc, para) else { continue };
                            e.insert(UnitText::of_review(p, &self.doc.review))
                        }
                    };
                    let stops: Vec<&geometry::Stop> = line.stops.iter().filter(|s| s.para == para).collect();
                    let right = if line.turn == crate::layout::vertical::Turn::NONE {
                        geo.stories[line.story].rect[2]
                    } else {
                        line.turn.inverse().rect([0.0, 0.0, geo.w, geo.h])[2]
                    };
                    for (k, a) in stops.iter().enumerate() {
                        // Break at a line's end
                        let b = match stops.get(k + 1) {
                            Some(b) if b.unit.0 == a.unit.0 + 1 => **b,
                            Some(_) => continue,
                            None if matches!(text.char_at(a.unit), Some('\u{c}' | '\u{2028}')) => geometry::Stop { x: a.x, ..**a },
                            None => continue,
                        };
                        let mut mark = |kind, x0: f64, x1: f64| out.push(FormattingMark { page, kind, at: line.strip(x0, x1) });
                        if o.shows_hidden_text() && text.is_hidden(a.unit) {
                            mark(MarkKind::Hidden, a.x, b.x);
                        }
                        let brk = || {
                            let q = self.index.para(&self.doc, para)?;
                            match stories::inline_at(q, a.unit)? {
                                crate::docx::Inline::Break(crate::docx::BreakKind::Page, _) => Some(MarkKind::PageBreak),
                                crate::docx::Inline::Break(crate::docx::BreakKind::Column, _) => Some(MarkKind::ColumnBreak),
                                _ => None,
                            }
                        };
                        // A deletion's stand-in (`^`, `#`) is blanked in the text, not a space
                        let stand_in = |u: Unit| {
                            text.is_hidden(u)
                                && self
                                    .index
                                    .para(&self.doc, para)
                                    .and_then(|q| stories::inline_at(q, u))
                                    .and_then(crate::docx::Inline::run)
                                    .is_some_and(|r| self.doc.review.hides(&r.rev))
                        };
                        match text.char_at(a.unit) {
                            Some(' ') if spaces && !stand_in(a.unit) => mark(MarkKind::Space, a.x, b.x),
                            Some('\u{a0}') if spaces => mark(MarkKind::NonBreakingSpace, a.x, b.x),
                            Some('\u{2011}') if o.all => mark(MarkKind::NonBreakingHyphen, a.x, b.x),
                            Some('\t') if tabs => mark(MarkKind::Tab, a.x, b.x),
                            Some('\u{2028}') if paragraphs => mark(MarkKind::LineBreak, a.x, b.x),
                            Some('\u{c}') if paragraphs => mark(brk().unwrap_or(MarkKind::Break), a.x, right.max(b.x)),
                            _ => {}
                        }
                    }
                    if !paragraphs {
                        continue;
                    }
                    if let Some(last) = stops.last().filter(|s| s.unit != text.len() && self.doc.review.show_hidden && o.shows_hidden_text())
                        && self.index.para(&self.doc, para).is_some_and(|q| shown_join_at(q, last.unit))
                    {
                        out.push(FormattingMark { page, kind: MarkKind::HiddenParagraph, at: line.strip(last.x, last.x) });
                    }
                    // Paragraph mark after a closing break
                    let after_break = stops.last().filter(|s| s.unit.0 + 1 == text.len().0 && text.char_at(s.unit) == Some('\u{c}'));
                    let end = match stops.iter().find(|s| s.unit == text.len()) {
                        Some(s) => s.x,
                        None if after_break.is_some() => right,
                        None => continue,
                    };
                    let slot = self.index.para_slot(para);
                    let kind = match slot.map(|s| s.container) {
                        Some(index::Container::Cell { table, row, cell })
                            if self.index.table(&self.doc, table).and_then(|t| t.rows.get(row)?.cells.get(cell)).map(|c| c.blocks.len())
                                == slot.map(|s| s.block + 1) =>
                        {
                            MarkKind::CellEnd
                        }
                        // A hidden mark shown apart
                        _ if self.doc.review.show_hidden
                            && o.shows_hidden_text()
                            && self.index.para(&self.doc, para).is_some_and(|q| q.mark.hidden) =>
                        {
                            MarkKind::HiddenParagraph
                        }
                        _ => MarkKind::Paragraph,
                    };
                    out.push(FormattingMark { page, kind, at: line.strip(end, end) });
                    if let Some(index::Container::Section(k)) = slot.map(|s| s.container)
                        && sections.contains(&k)
                        && slot.is_some_and(|s| s.block + 1 == self.doc.sections[k].blocks.len())
                        && let Some(next) = self.doc.sections.get(k + 1)
                    {
                        out.push(FormattingMark { page, kind: MarkKind::SectionBreak(next.break_kind), at: line.strip(end, right.max(end)) });
                    }
                }
            }
            if paragraphs {
                // End-of-row marks
                let mut first: std::collections::HashMap<(TableId, usize), (f64, f64)> = std::collections::HashMap::new();
                for l in &geo.lines {
                    if let Some(c) = l.chain.last() {
                        let e = first.entry((c.0, c.1)).or_insert((l.top, l.bottom));
                        if l.top < e.0 {
                            *e = (l.top, l.bottom);
                        }
                    }
                }
                for r in geo.rows.iter().filter(|r| !r.repeat) {
                    if let Some(&(top, bottom)) = first.get(&(r.table, r.row)) {
                        out.push(FormattingMark {
                            page,
                            kind: MarkKind::RowEnd,
                            at: view::Band { rect: [r.rect[2], top, r.rect[2], bottom], turn: r.turn },
                        });
                    }
                }
            }
        }
        out
    }

    pub fn text_boundaries(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let snapshot = self.layout.snapshot();
        let mut out = Vec::new();
        for page in pages {
            if let Some(fragment) = snapshot.pages().get(page) {
                for child in fragment.children.iter() {
                    let b = match child {
                        crate::layout::fragment::PageChild::Body(b) => &**b,
                        crate::layout::fragment::PageChild::Header(b) | crate::layout::fragment::PageChild::Footer(b) => b,
                    };
                    let ((x, y), (w, h)) = (b.offset, b.size);
                    out.push((page, [x, y, x + w, y + h]));
                }
            }
            if let Some(geo) = self.pages.page(page, &self.index) {
                out.extend(geo.cells.iter().map(|c| (page, c.page_rect())));
            }
        }
        out
    }

    pub fn cell_rects(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let mut out = Vec::new();
        for page in pages {
            if let Some(geo) = self.pages.page(page, &self.index) {
                out.extend(geo.cells.iter().map(|c| (page, c.page_rect())));
            }
        }
        out
    }

    pub fn paras_on(&mut self, pages: std::ops::Range<usize>) -> Vec<ParaId> {
        let mut out: Vec<ParaId> = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for p in geo.lines.iter().filter(|l| !l.repeat).flat_map(|l| l.paras.iter()) {
                if !out.contains(p) {
                    out.push(*p);
                }
            }
        }
        out
    }

    pub fn unit_rects(&mut self, para: ParaId, a: Unit, b: Unit, pages: std::ops::Range<usize>) -> Vec<(usize, view::Band)> {
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for (_, l) in geo.lines_of(para) {
                let (Some(first), Some(last)) = (l.first_of(para), l.last_of(para)) else { continue };
                let (lo, hi) = (a.max(first), b.min(last));
                if lo >= hi {
                    continue;
                }
                if let (Some(x0), Some(x1)) = (l.x_of(para, lo), l.x_of(para, hi)) {
                    out.push((page, l.strip(x0.min(x1), x0.max(x1))));
                }
            }
        }
        out
    }

    pub fn line_starts(&mut self, para: ParaId) -> Vec<Unit> {
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        let mut out = Vec::new();
        for page in self.pages.pages_of(para, self.near, &self.index) {
            if let Some(g) = self.pages.page(page, &self.index) {
                out.extend(g.lines_of(para).filter_map(|(_, l)| l.first_of(para)));
            }
        }
        out.dedup();
        if out.first() != Some(&Unit(0)) {
            out.insert(0, Unit(0));
        }
        out
    }

    pub fn selects_story(&mut self) -> bool {
        let Some(story) = self.selection.focus().map(|p| p.story) else { return false };
        let nav = self.nav();
        let (Some(a), Some(b)) = (nav.story_edge(story, false), nav.story_edge(story, true)) else { return false };
        a != b && self.selection.ordered(&self.index) == Some((a, b))
    }

    pub fn caret_editable(&self) -> bool {
        self.selection.focus().is_none_or(|p| self.editable(&p))
    }

    pub fn page_geometries(&mut self) -> Vec<(usize, Rc<geometry::PageGeometry>)> {
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        (0..self.pages.len()).filter_map(|i| self.pages.page(i, &self.index).map(|g| (i, g))).collect()
    }

    pub fn clipboard(&self) -> Option<&clipboard::Fragment> {
        self.clipboard.as_ref()
    }

    pub fn adopt_clipboard(&mut self, mut f: clipboard::Fragment) -> u64 {
        f.id = self.clipboard.as_ref().map_or(1, |c| c.id + 1);
        let id = f.id;
        self.clipboard = Some(f);
        id
    }

    pub fn clipboard_id(&self) -> Option<u64> {
        self.clipboard.as_ref().map(|f| f.id)
    }

    pub fn copy(&mut self) -> Option<clipboard::Fragment> {
        let f = self.clip()?;
        // The clipboard changes: Ctrl+Shift+V pastes it, not the painter's formatting.
        self.painter_claim = None;
        self.requests.push(Request::Clipboard { text: f.text(), html: Some(f.html()) });
        self.clipboard = Some(f.clone());
        Some(f)
    }

    pub fn selected_text(&mut self) -> String {
        self.selected_text_within(usize::MAX).unwrap_or_default()
    }

    /// Units `a..b` of a paragraph as they read: what the markup view shows without hidden text, fields as their
    /// results; breaks as line ends and objects as spaces.
    #[cfg(test)]
    pub(crate) fn read_text(&self, para: ParaId, a: Unit, b: Unit) -> String {
        let Some(p) = self.index.para(&self.doc, para) else { return String::new() };
        seen_text(p, &self.doc.review).text_in(a, b).map(plain_char).collect()
    }

    pub fn selected_text_within(&mut self, limit: usize) -> Option<String> {
        let (text, whole) = self.text_within(limit, &mut HashMap::new());
        whole.then_some(text)
    }

    /// The selection's text cut to its first `limit` characters, and whether that is all of it. `texts` holds the
    /// paragraphs' texts found so far, for selections in turn.
    pub(super) fn text_within(&mut self, limit: usize, texts: &mut HashMap<ParaId, text::Seen>) -> (String, bool) {
        if let Selection::Block { .. } = self.selection {
            return self.block_text(limit);
        }
        if self.is_discontiguous() {
            return self.multi_text(limit);
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return (String::new(), true) };
        let mut out = String::new();
        let mut chars = 0usize;
        let mut p = a;
        let mut guard = 0;
        while self.index.cmp(&p, &b) == std::cmp::Ordering::Less && guard < 1_000_000 {
            guard += 1;
            match p.para_offset() {
                Some((para, off)) => {
                    let Some(len) = self.index.para(&self.doc, para).map(step::para_len) else { break };
                    let seen = match texts.entry(para) {
                        std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                        std::collections::hash_map::Entry::Vacant(v) => {
                            let Some(p) = self.index.para(&self.doc, para) else { break };
                            v.insert(seen_text(p, &self.doc.review))
                        }
                    };
                    let end = match b.para_offset() {
                        Some((pb, ob)) if pb == para => ob,
                        _ => len,
                    };
                    let mut text = seen.text_in(off, end);
                    let mut take = 0;
                    out.extend(text.by_ref().take(limit - chars).inspect(|_| take += 1).map(plain_char));
                    if text.next().is_some() {
                        return (out, false);
                    }
                    chars += take;
                    let at_end = Position::text(p.story, para, len);
                    let next = self.nav().right(at_end, false);
                    if next == at_end || self.index.cmp(&next, &b) == std::cmp::Ordering::Greater {
                        let reached = b.para_offset().is_some_and(|(pb, _)| pb == para);
                        return (out, reached || next != at_end);
                    }
                    if next.para_offset().is_some() {
                        if chars == limit {
                            return (out, false);
                        }
                        out.push('\n');
                        chars += 1;
                    }
                    p = next;
                }
                None => {
                    let next = self.nav().right(p, false);
                    if next == p {
                        break;
                    }
                    p = next;
                }
            }
        }
        (out, self.index.cmp(&p, &b) != std::cmp::Ordering::Less)
    }
}

/// A character of text as plain text: breaks as line ends, objects as spaces.
pub(crate) fn plain_char(c: char) -> char {
    match c {
        '\u{2028}' => '\n',
        '\u{c}' | text::OBJECT => ' ',
        c => c,
    }
}

/// A paragraph's text as it reads: what the markup view shows, hidden text only while it shows, fields as their
/// results.
fn seen_text(p: &Paragraph, review: &crate::docx::review::ReviewView) -> text::Seen {
    text::Seen::of(p, |r| (review.show_hidden || !r.hidden) && !review.hides(&r.rev), true)
}

/// Whether a paragraph joined to `p` through its vanished mark ends at `u`, its parts showing on their own lines while
/// hidden text shows.
fn shown_join_at(p: &Paragraph, u: Unit) -> bool {
    let Some(src) = &p.src else { return false };
    p.style.parts.len() == src.joined.len() + 1 && src.joined.iter().any(|j| j.end == u.0)
}

/// The deepest nesting an edit may make, in table levels. The reader refuses parts nested deeper than
/// [`crate::docx::xml::MAX_DEPTH`] elements, three a table level, so a document nested deeper would save but not open.
const MAX_TABLE_DEPTH: usize = 30;

/// The element levels a text box adds (`w:p`, `w:r`, `mc:AlternateContent`, `mc:Choice`, `w:drawing`, `wp:anchor`,
/// `a:graphic`, `a:graphicData`, `wps:wsp`, `wps:txbx`, `w:txbxContent`), in table levels, rounded up.
const TEXT_BOX_LEVELS: usize = 4;

/// How many levels deep blocks go: 0 for paragraphs, 1 for a table of paragraphs, [`TEXT_BOX_LEVELS`] more for a text
/// box.
fn table_depth(blocks: &[Block]) -> usize {
    let cells = |t: &crate::docx::Table| t.rows.iter().flat_map(|r| &r.cells).map(|c| table_depth(&c.blocks)).max().unwrap_or(0);
    blocks
        .iter()
        .map(|b| match b {
            Block::Table(t) => 1 + cells(t),
            Block::Para(p) => para_depth(p),
        })
        .max()
        .unwrap_or(0)
}

/// How many levels deep a paragraph's text boxes go, a group's parts' included.
fn para_depth(p: &Paragraph) -> usize {
    fn parts(i: &Inline) -> &[crate::docx::GroupPart] {
        match i {
            Inline::Float(f, _) => &f.parts,
            _ => &[],
        }
    }
    p.inlines
        .iter()
        .flat_map(|i| i.story().map(|(_, tb)| tb).into_iter().chain(parts(i).iter().filter_map(|g| g.text_box.as_ref())))
        .map(|tb| TEXT_BOX_LEVELS + table_depth(&tb.blocks))
        .max()
        .unwrap_or(0)
}

fn first_para_of(b: &Block) -> Option<ParaId> {
    match b {
        Block::Para(p) => Some(ParaId(p.id)),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).find_map(first_para_of),
    }
}

fn last_para_of(b: &Block) -> Option<(ParaId, Unit)> {
    match b {
        Block::Para(p) => Some((ParaId(p.id), para_len(p))),
        Block::Table(t) => t.rows.iter().rev().flat_map(|r| r.cells.iter().rev()).flat_map(|c| c.blocks.iter().rev()).find_map(last_para_of),
    }
}

fn splits_a_control(p: &crate::docx::Paragraph, at: Unit) -> bool {
    let mut u = 0u32;
    let mut open: Vec<(crate::docx::ControlId, u32)> = Vec::new();
    for i in &p.inlines {
        match i {
            Inline::ControlStart(c) => open.push((*c, u)),
            Inline::ControlEnd(c) => {
                if let Some(k) = open.iter().rposition(|(o, _)| o == c) {
                    let (_, s) = open.remove(k);
                    if s <= at.0 && at.0 < u && s < u {
                        return true;
                    }
                }
            }
            _ => {}
        }
        u += i.units() as u32;
    }
    false
}

fn moves_picture_controls(s: &Step) -> bool {
    matches!(s, Step::Control { .. }) || s.pictures_where(&|_| true) > 0
}

/// The band of `a..b` on line `l`'s part of paragraph `para`, if it reaches any.
fn range_band(index: &index::DocIndex, l: &geometry::GLine, para: ParaId, a: &Position, b: &Position) -> Option<view::Band> {
    use std::cmp::Ordering::{Greater, Less};
    let (lf, ll) = (l.first_of(para)?, l.last_of(para)?);
    let first = Position::text(a.story, para, lf);
    let last = Position::text(a.story, para, ll);
    if index.cmp(&last, a) == Less || index.cmp(&first, b) == Greater {
        return None;
    }
    let lo = if index.cmp(&first, a) == Less { a.para_offset().map(|(_, u)| u) } else { Some(lf) }?;
    let hi = if index.cmp(&last, b) == Greater { b.para_offset().map(|(_, u)| u) } else { Some(ll) }?;
    let (x0, x1) = (l.x_of(para, lo)?, l.x_of(para, hi)?);
    let shown_end = l.stops.last().is_some_and(|s| s.end && s.para == para);
    let mark = if hi == ll && index.cmp(&last, b) == Less && shown_end { 4.0 } else { 0.0 };
    (x1 + mark > x0 + 0.01).then(|| l.strip(x0, x1 + mark))
}
