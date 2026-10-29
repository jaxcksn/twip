//! The editor subsystem: the single owner of editing state and behaviour.
//! It reads geometry only from layout snapshots and exposes rendering-independent state through [`view`].
//!
//! Every input arrives as a typed call (the `api` session's); every change is a transaction of [`step::Step`]s, recorded for undo
//! and given to layout as edit hints. One commit path records them all, undo and redo included: it alone raises the
//! revision. Geometry-dependent input first lays out through the page it concerns.

mod cell_clipboard;
pub mod clipboard;
mod clipped;
pub mod commands;
pub mod comments;
pub mod elements;
pub mod extend;
pub mod find;
pub mod format;
pub mod geometry;
pub(crate) mod gestures;
mod goback;
pub mod history;
pub mod index;
pub mod input;
#[cfg(any(test, feature = "inspect", not(target_os = "emscripten")))]
pub mod inspect;
pub mod integrity;
pub mod interaction;
pub mod keymap;
pub mod lists;
mod merge;
pub(crate) use merge::api_id;
pub mod navigate;
mod notes;
pub mod objects;
pub mod placeholders;
pub mod pointer;
pub mod position;
mod resize;
pub mod revisions;
pub mod section;
pub mod selection;
mod smart;
pub mod state;
pub mod step;
mod stories;
pub mod table;
mod table_cells;
pub mod table_format;
pub mod tables;
pub mod text;
pub mod view;

#[cfg(test)]
pub(crate) mod tests;

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::api::record::Origin;
use crate::docx::{Block, BreakKind, Document, Inline, Paragraph, Run};
use crate::font::Fonts;
use crate::layout::{Budget, Change, DocumentLayout, ParaId, Revision, Snapshot, Step as LayoutStep, TableId, Target, Unit};

use commands::{Command, Dir, MoveBy, Outcome};
use geometry::Pages;
use history::{History, SelectionRecord, Transaction, Typing, TypingRun};
use index::{Container, DocIndex};
use input::{PointerKind, Request, Viewport};
use navigate::Nav;
use position::{Affinity, Place, Position, StoryId};
use selection::{Granularity, Selection};
use step::{Map, Step, StepError, para_len, run_of};
use text::UnitText;
use view::{CaretView, PageStack, SelectionView, View};

/// The layout budget of one idle call.
pub const IDLE_BUDGET: Duration = Duration::from_millis(4);

/// A selection drag in progress: where it started and what it extends by.
#[derive(Clone, Copy, Debug)]
struct Drag {
    anchor: Position,
    /// For word and paragraph drags: the start and end of what the press selected.
    span: Option<(Position, Position)>,
    unit: Granularity,
    /// The last pointer position, for auto-scroll.
    at: (f64, f64),
    /// The drag became a cell block (it entered another cell); coming back keeps the start cell selected.
    block: bool,
}

pub struct Editor {
    doc: Document,
    layout: DocumentLayout,
    index: DocIndex,
    history: History,
    selection: Selection,
    /// Up/Down keep the x they started from.
    goal_x: Option<f64>,
    /// Formatting for what is typed next at a collapsed caret; any caret move drops it.
    pending: Option<Run>,
    drag: Option<Drag>,
    pages: Pages,
    /// The page the caret was last shown on.
    near: usize,
    /// Counts committed transactions: the document's revision.
    revision: u64,
    /// Counts every change to the document or to how it shows: what layout and the caches built from the document
    /// key on.
    generation: u64,
    seq: u64,
    viewport: Viewport,
    /// Where Esc leaves a header or footer for: the body selection the entering double-click set.
    body_return: Option<Selection>,
    /// A float being moved or resized by the pointer.
    object_drag: Option<objects::ObjectDrag>,
    /// The host's placeholder looks by key (`""`: every placeholder without its own).
    placeholder_looks: std::collections::HashMap<String, view::PlaceholderLook>,
    /// A table border being dragged.
    border_drag: Option<resize::BorderDrag>,
    /// A cell block being dragged from a cell, row or column zone: its table, start cell and shape.
    zone_drag: Option<(TableId, selection::CellRef, selection::CellShape)>,
    /// The pointer's shape where it last moved without a drag.
    cursor: &'static str,
    /// The history's position and the tracking setting when the document was last saved: away from them, the
    /// document is modified.
    saved: (u64, bool),
    /// Who the input being run comes from: what its transactions record.
    origin: Origin,
    /// A batch being run: what its commands commit folds into one transaction.
    batch: Option<Batch>,
    /// The transactions committed since the session last took them, in order.
    commits: Vec<Committed>,
    requests: Vec<Request>,
    /// What F4 repeats: the last insertion or break; consecutive typing accumulates.
    repeat: Option<Command>,
    /// Extend mode and block mode (F8, Ctrl+Shift+F8).
    modes: extend::Modes,
    /// The macOS keymap layer (the host's platform).
    mac: bool,
    /// The last edit locations, newest last (Shift+F5), and how far back Shift+F5 has gone.
    edit_marks: Vec<Position>,
    back_step: Option<usize>,
    /// Where the caret was when the current round of Go Back started.
    back_from: Option<Position>,
    /// How positions moved through the last committed transaction.
    last_maps: Vec<step::Map>,
    /// A press inside the selection: a drag-and-drop or a click.
    text_drag: Option<interaction::TextDrag>,
    /// The last fragment copied or cut here (pasted with its formatting when the clipboard still carries its id).
    clipboard: Option<clipboard::Fragment>,
    /// The cell Tab last selected, when the selection alone doesn't say (a merged cell's continuation row).
    tab_stop: Option<(Selection, TableId, (usize, usize))>,
    /// Editing, template design or read-only.
    mode: Mode,
    /// Every field and content control as the host last saw it: what change lists compare.
    elems: std::collections::HashMap<crate::docx::ElementId, elements::Element>,
    /// The elements added, removed and changed since the last reply.
    changes: elements::Changes,
    /// Why a building block refused the running command (`locked`, `read-only`): the command is refused with it.
    refused: Option<&'static str>,
    /// Notices for the host since the last reply (activation, field updates, links).
    events: Vec<input::Notice>,
    /// What the last command made (a field's id, a comment's).
    made: Option<crate::api::command::CommandResult>,
    /// Controls a command made for content it is about to put in (new repeating items): added by the commit, in
    /// the same transaction.
    pending_controls: Vec<(crate::docx::ControlId, crate::docx::Control)>,
    /// Images a command added for pictures it is about to put in: added by the commit, in the same transaction.
    pending_images: Vec<Vec<u8>>,
    /// The host's external drop caret (`view.drop`).
    drop: Option<Position>,
    /// The host's labels for empty fields in design mode, by trimmed code (`field.labels`).
    field_labels: HashMap<String, String>,
    /// The host's element marks (`element.mark`): view only.
    marks: Vec<merge::Mark>,
    /// The host's looks for controls in design mode (`view.looks`): view only.
    looks: merge::Looks,
    /// The last F9's token: `field.results` answers it.
    update_token: u64,
    /// Where the pointer last hovered, document points (a mark's tooltip).
    hover: Option<(f64, f64)>,
    /// The region the pointer was last over in design mode: kept while the pointer is on its title tab, which is
    /// above the region.
    hover_region: Option<crate::docx::ControlId>,
    /// The active comment thread the host chose, and the threads' ranges.
    comments: comments::State,
    /// The document's revisions in order, built when navigation asks and kept until the document changes.
    review_list: revisions::index::List,
    /// The session's author (the name and identity revisions are recorded under), and who that is in the document's
    /// people once a revision needs them.
    author: (Rc<str>, Option<crate::docx::review::Identity>),
    author_id: Option<crate::docx::review::AuthorId>,
    /// A move being made: its transactions record it under this name.
    move_name: Option<Rc<str>>,
    /// The insertion and deletion the last transaction recorded (what a move is made of).
    recorded: (Option<Rc<crate::docx::review::RevisionMark>>, Option<Rc<crate::docx::review::RevisionMark>>),
    /// What was cut as a move with tracking on: the fragment and its deletion, a move once pasted.
    cut_move: Option<(u64, Rc<crate::docx::review::RevisionMark>)>,
    /// Tests: how many elements change tracking looked at (a keystroke's cost doesn't grow with the
    /// document).
    #[cfg(test)]
    pub(crate) track_work: usize,
}

/// What the host lets the user do: editing, template design (activations go to the host), or nothing
/// that changes the document.
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

/// A transaction under construction: the inverses of the steps applied so far, and their layout hints.
struct Tx {
    undo: Vec<Step>,
    hints: Vec<Option<Change>>,
    before: SelectionRecord,
    label: &'static str,
    /// Paragraphs whose context changed (a join, a move into another cell) and tables whose rows came or went (their
    /// cells' table-style regions move): resolved again before the transaction commits, so the model shows what
    /// saving and reading back gives.
    stale_paras: Vec<ParaId>,
    stale_tables: Vec<TableId>,
    /// Undeletable pictures put in less those taken out ([`crate::docx::Document::undeletable`]).
    kept: i64,
    /// How positions moved through each step (the edit locations follow them).
    maps: Vec<step::Map>,
    /// Tracking was on when it began: its steps are recorded as revisions, by `rec` once the first one is.
    tracked: bool,
    rec: Option<revisions::Recording>,
    origin: Origin,
    /// Undo and redo: the transaction replayed. It goes back in the history with the steps that reverse this one.
    replayed: Option<Transaction>,
    /// What the steps touched, for the record.
    targets: step::Targets,
    /// A step changed a control or moved pictures: the picture controls' image sets are derived again.
    pictures: bool,
}

/// A batch being run: the transaction its commands' commits fold into, its label (else its first commit's), and
/// where each nested batch began in it, with the selection then.
struct Batch {
    tx: Tx,
    label: Option<Cow<'static, str>>,
    marks: Vec<(usize, Selection)>,
}

/// A committed transaction, as the session records it.
#[derive(Clone, Debug)]
pub struct Committed {
    /// The revision it made.
    pub revision: u64,
    pub origin: Origin,
    /// Its undo label.
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
        let mut e = Editor {
            doc,
            layout: DocumentLayout::new(),
            index,
            history: History::default(),
            selection: Selection::Caret(Position::text(StoryId::Body, ParaId(0), Unit(0))),
            goal_x: None,
            pending: None,
            drag: None,
            pages: Pages::default(),
            near: 0,
            revision: 1,
            generation: 1,
            seq: 0,
            viewport: Viewport::default(),
            body_return: None,
            object_drag: None,
            placeholder_looks: std::collections::HashMap::new(),
            border_drag: None,
            zone_drag: None,
            cursor: "text",
            saved: (0, false),
            origin: Origin::User,
            batch: None,
            commits: Vec::new(),
            requests: Vec::new(),
            repeat: None,
            modes: extend::Modes::default(),
            mac: false,
            edit_marks: Vec::new(),
            back_step: None,
            back_from: None,
            last_maps: Vec::new(),
            text_drag: None,
            tab_stop: None,
            mode: Mode::Editing,
            elems: std::collections::HashMap::new(),
            changes: elements::Changes::default(),
            refused: None,
            events: Vec::new(),
            made: None,
            pending_controls: Vec::new(),
            pending_images: Vec::new(),
            drop: None,
            field_labels: HashMap::new(),
            marks: Vec::new(),
            looks: merge::Looks::default(),
            update_token: 0,
            hover: None,
            hover_region: None,
            comments: comments::State::default(),
            review_list: Default::default(),
            author: ("Author".into(), None),
            author_id: None,
            move_name: None,
            recorded: (None, None),
            cut_move: None,
            #[cfg(test)]
            track_work: 0,
            clipboard: None,
        };
        e.saved.1 = e.doc.settings.track_revisions;
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

    pub fn seq_applied(&self) -> u64 {
        self.seq
    }

    /// Whether the document differs from its last save: the history is away from the saved position, or tracking
    /// was turned on or off since. Undoing back to it clears this.
    pub fn is_dirty(&self) -> bool {
        (self.history.position(), self.doc.settings.track_revisions) != self.saved
    }

    /// A save wrote the document as it is now: its revision, at this history position. Typing after it starts a new
    /// undo step, so undo comes back to what was saved.
    pub fn mark_saved(&mut self) {
        self.history.break_typing();
        self.saved = (self.history.position(), self.doc.settings.track_revisions);
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Shows revisions in `view` (REV-1): a full relayout, not an edit (no undo step, the document stays as it is).
    pub fn set_markup_view(&mut self, view: crate::docx::review::MarkupView) -> Outcome {
        if self.doc.review.view == view {
            return Outcome::Unchanged("already in that view");
        }
        self.doc.review.view = view;
        self.relayout_review();
        Outcome::Moved
    }

    /// Draws `author`'s revisions in `color` (0xRRGGBB, REV-3): a full relayout, not an edit.
    pub fn set_author_color(&mut self, author: crate::docx::review::AuthorId, color: u32) -> Outcome {
        if self.doc.review.palette.color(author) == color {
            return Outcome::Unchanged("already that colour");
        }
        Rc::make_mut(&mut self.doc.review.palette).chosen.insert(author, color);
        self.relayout_review();
        Outcome::Moved
    }

    fn relayout_review(&mut self) {
        self.relayout([None]);
    }

    /// Lays out again what `hints` say changed (`None`: layout compares), as a new generation: a view change, or a
    /// transaction's steps.
    fn relayout(&mut self, hints: impl IntoIterator<Item = Option<Change>>) {
        self.generation += 1;
        for h in hints {
            self.layout.edit(Revision(self.generation), h);
        }
    }

    /// Runs `f` as input from `origin`: what the transactions it commits record (the editor's own input is the
    /// user's).
    pub fn with_origin<T>(&mut self, origin: Origin, f: impl FnOnce(&mut Editor) -> T) -> T {
        let was = std::mem::replace(&mut self.origin, origin);
        let out = f(self);
        self.origin = was;
        out
    }

    /// The transactions committed since the last call, in order: one record each.
    pub fn take_commits(&mut self) -> Vec<Committed> {
        std::mem::take(&mut self.commits)
    }

    /// The fonts changed: the whole document lays out again. Not an edit: the revision stays.
    pub fn fonts_changed(&mut self) {
        self.relayout([None]);
    }

    /// The index a picture the command is about to put in takes for `data`: the image is added by the commit, in the
    /// same transaction.
    fn pending_image(&mut self, data: Vec<u8>) -> usize {
        self.pending_images.push(data);
        self.doc.images.len() + self.pending_images.len() - 1
    }

    /// Refuses an edit a read-only document asked for, before it runs.
    fn refuse_read_only(&mut self) -> Outcome {
        self.refused = Some("read-only");
        Outcome::Refused(crate::api::error::Error::new(crate::api::error::ErrorCode::ReadOnly, "the document is read only"))
    }

    pub fn snapshot(&self) -> Rc<Snapshot> {
        self.layout.snapshot()
    }

    pub fn stack(&self) -> PageStack {
        PageStack::of(&self.layout.snapshot())
    }

    /// Places the selection directly, ending any typing run (benchmarks and tests choose places without input).
    pub fn set_caret_for_benchmark(&mut self, s: Selection) {
        self.history.break_typing();
        self.goal_x = None;
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

    // ---------------- layout coordination ----------------

    /// Lays out through the page showing `pos` (it may be provisional), then syncs geometry.
    fn ensure(&mut self, pos: &Position, fonts: &Fonts) {
        if let Some((para, unit)) = pos.para_offset() {
            crate::prof::time("editor:layout", || self.layout.through(&self.doc, fonts, Target::Position { para, unit }));
        }
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
    }

    /// Lays out through page `page` (the host calls it for the visible pages before painting).
    pub fn ensure_page(&mut self, page: usize, fonts: &Fonts) {
        let snapshot = self.layout.snapshot();
        // A layout from before the last change is provisional throughout.
        let stale = snapshot.revision() != self.layout.revision();
        let pending = if stale { Some(0) } else { snapshot.info().iter().position(|i| i.provisional) };
        if pending.is_some_and(|p| page >= p) {
            self.layout.through(&self.doc, fonts, Target::Page(page));
        }
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
    }

    /// Background layout for one idle slice; true while more is pending.
    pub fn idle(&mut self, fonts: &Fonts) -> bool {
        let t = Instant::now();
        let step = self.layout.run(&self.doc, fonts, Budget::Until(t + IDLE_BUDGET));
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        step == LayoutStep::Yielded || snapshot.info().iter().any(|i| i.provisional)
    }

    /// Lays out through the selection's focus when the published layout predates the document's last change: a
    /// relayout no edit laid out (fonts arriving, the markup view), whose old layout would otherwise answer the
    /// caret, hit tests and frames.
    pub fn ensure_current(&mut self, fonts: &Fonts) {
        if self.layout.snapshot().revision() == self.layout.revision() {
            return;
        }
        match self.selection.focus().filter(|p| p.para_offset().is_some()) {
            Some(p) => self.ensure(&p, fonts),
            None => self.ensure_page(0, fonts),
        }
    }

    /// Whether the published layout is the document's as it is now, to its end.
    pub fn layout_complete(&self) -> bool {
        let snapshot = self.layout.snapshot();
        snapshot.revision() == self.layout.revision() && !matches!(snapshot.status(), crate::layout::LayoutStatus::Pending { .. })
    }

    /// Lays out the whole document (tests, PDF export).
    pub fn complete(&mut self, fonts: &Fonts) {
        self.layout.complete(&self.doc, fonts);
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
    }

    // ---------------- inputs ----------------

    /// Units `a..b` of a paragraph selected and revealed (find, spelling).
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

    /// Editing, template design or read-only: design mode shows field labels.
    pub fn set_mode(&mut self, m: Mode) -> Outcome {
        let design = (self.mode == Mode::Design) != (m == Mode::Design);
        self.mode = m;
        if design {
            self.refresh_labels(None);
        }
        Outcome::Moved
    }

    /// The notices since they were last taken.
    pub fn take_notices(&mut self) -> Vec<input::Notice> {
        std::mem::take(&mut self.events)
    }

    /// The editing mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether edits are recorded as revisions (Track Changes): the document's setting, and always under
    /// tracked-changes protection.
    pub fn tracking(&self) -> bool {
        self.doc.settings.track_revisions || self.tracking_locked()
    }

    fn tracking_locked(&self) -> bool {
        self.doc.settings.protection.as_ref().is_some_and(|p| p.forces_tracking())
    }

    /// Turns tracking on or off: the document's setting (saved with it), outside the history as in Word, so undo and
    /// redo stay exact whatever tracking is set to. Refused, `false`, while protection keeps it on.
    pub fn set_tracking(&mut self, on: bool) -> bool {
        if !on && self.tracking_locked() {
            return false;
        }
        self.doc.settings.track_revisions = on;
        true
    }

    /// The session's author: who revisions recorded from now on are by ("Author" until set).
    pub fn set_author(&mut self, name: &str, identity: Option<crate::docx::review::Identity>) {
        self.author = (name.into(), identity);
        self.author_id = None;
    }

    /// The session's author among the document's people, added on first use.
    fn session_author(&mut self) -> crate::docx::review::AuthorId {
        if let Some(a) = self.author_id {
            return a;
        }
        let a = self.doc.people.author(&self.author.0, self.author.1.as_ref());
        self.author_id = Some(a);
        a
    }

    /// Updates the elements the host knows after a transaction or its undo (`inverses`: the steps that undo it), and
    /// what the next reply lists as changed.
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
                // In document order, so change lists come in that order.
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

    /// The normalizer: both ends of a selection on grapheme boundaries. Geometry has a stop per
    /// character, so a click or a vertical move can land inside a cluster.
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

    /// A cell block in the table's grid, widened to whole merged cells: clamped to the grid, then
    /// grown until no horizontal or vertical merge crosses its edge. `usize::MAX` stands for the last row or column.
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
                    // A column selection is the cells its columns cross (a spanning cell among them whole); it
                    // doesn't widen to the spanning cells' other columns, which would take in the next merges' too.
                    if shape != selection::CellShape::Columns {
                        c0 = c0.min(c);
                        c1 = c1.max(e);
                    }
                    // The vertical merge region holding this cell: from its start down while the next row continues.
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
        // The anchor keeps its corner.
        let pick = |lo: usize, hi: usize, anchor_low: bool| if anchor_low { (lo, hi) } else { (hi, lo) };
        let (ar, fr) = pick(r0, r1, a.row <= f.row);
        let (ac, fc) = pick(c0, c1, a.col <= f.col);
        Selection::Cells { table, anchor: selection::CellRef { row: ar, col: ac }, focus: selection::CellRef { row: fr, col: fc }, shape }
    }

    /// The grid cell (row, grid column) of `table` nearest a document point, on the page under it.
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
            .min_by(|a, b| dist(&a.rect, px, py).total_cmp(&dist(&b.rect, px, py)))?;
        let &(_, row, _) = c.chain.last()?;
        Some((row, c.col))
    }

    fn set_selection(&mut self, s: Selection) {
        let s = self.normalize(s);
        if s != self.selection {
            self.pending = None;
            self.history.break_typing();
        }
        self.selection = s;
    }

    /// Where typing goes: the caret, or the start of a text selection (after deleting it).
    fn caret(&self) -> Option<Position> {
        self.selection.focus()
    }

    // ---------------- transactions ----------------

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
        }
    }

    fn step(&mut self, tx: &mut Tx, s: Step) -> Result<Map, StepError> {
        // Read only refuses edits before they run: no step reaches a read-only document.
        if self.mode == Mode::ReadOnly {
            return Err(StepError::ReadOnly);
        }
        // An inline content control stays in one paragraph: a split inside one is refused, so Enter and
        // pastes of several paragraphs do nothing there.
        if let Step::Split { para, at, .. } = &s
            && let Some(p) = self.index.para(&self.doc, *para)
            && splits_a_control(p, *at)
        {
            return Err(StepError::OutOfRange);
        }
        if !tx.tracked {
            return self.apply_step(tx, s);
        }
        // Tracking: the steps recording the edit apply in its place, by the session's author at the transaction's time.
        if tx.rec.is_none() {
            let author = self.session_author();
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
            tx.rec = Some(revisions::Recording::new(author, crate::docx::review::Timestamp::from_utc(secs), self.move_name.clone(), tx.label));
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

    /// Applies one step to the document as it is, recording its inverse in the transaction.
    fn apply_step(&mut self, tx: &mut Tx, s: Step) -> Result<Map, StepError> {
        if self.mode == Mode::ReadOnly {
            return Err(StepError::ReadOnly);
        }
        let added = if self.doc.undeletable.is_empty() { 0 } else { s.pictures_of(&self.doc.undeletable) };
        tx.targets.add(&s);
        tx.pictures |= moves_picture_controls(&s);
        let a = s.apply(&mut self.doc, &mut self.index)?;
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

    /// Undoes a transaction's steps (a command that failed half way).
    fn abort(&mut self, tx: Tx) {
        self.pending_controls.clear();
        self.pending_images.clear();
        for s in tx.undo.into_iter().rev() {
            let _ = s.apply(&mut self.doc, &mut self.index);
        }
    }

    /// The one commit path, for every edit, undo and redo: records the transaction as one undo step (undo and redo
    /// put theirs back in the history), raises the revision, lays out and keeps the record. Nothing changed records
    /// nothing. In a batch, an edit joins the batch's transaction instead, committed when the batch ends.
    fn commit(&mut self, mut tx: Tx, typing: Option<TypingRun>, fonts: &Fonts) -> Outcome {
        if tx.replayed.is_none()
            && let Err(why) = self.settle(&mut tx)
        {
            self.selection = tx.before.selection;
            self.abort(tx);
            return Outcome::Unchanged(why);
        }
        if tx.undo.is_empty() {
            return Outcome::Unchanged("no change");
        }
        self.effects(&mut tx);
        if tx.replayed.is_none()
            && let Some(b) = &mut self.batch
        {
            b.label.get_or_insert(Cow::Borrowed(tx.label));
            b.tx.undo.append(&mut tx.undo);
            b.tx.targets.extend(tx.targets);
            self.after_edit(fonts);
            return Outcome::Changed(self.revision + 1);
        }
        let label = tx.replayed.as_ref().map_or(Cow::Borrowed(tx.label), |done| done.label.clone());
        self.finish(tx, label, typing, fonts)
    }

    /// What a forward edit leaves to do, as steps of its transaction: the controls and images it made, field kinds,
    /// orphans, stale paragraphs and list labels. Refused when it would delete a picture control that can't be.
    fn settle(&mut self, tx: &mut Tx) -> Result<(), &'static str> {
        // Field kinds and orphans follow what the edit did: never recorded as revisions. Resolving paragraphs again
        // and numbering are, so a recorded property change keeps what it had.
        let tracked = std::mem::replace(&mut tx.tracked, false);
        self.recorded = tx.rec.as_ref().map(revisions::Recording::marks).unwrap_or_default();
        for (id, c) in std::mem::take(&mut self.pending_controls) {
            let _ = self.step(tx, Step::Control { id, control: Some(c) });
        }
        for data in std::mem::take(&mut self.pending_images) {
            let index = self.doc.images.len();
            let _ = self.apply_step(tx, Step::AddImage { index, data: Some(data) });
        }
        // A field code edited in place: its kind is its code's again.
        let edited: Vec<ParaId> = tx.undo.iter().filter_map(|s| if let Step::Text { para, .. } = s { Some(*para) } else { None }).collect();
        for para in edited {
            if self.index.para(&self.doc, para).is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { .. }))) {
                self.refresh_field_kinds(tx, para);
            }
        }
        self.drop_orphan_notes(tx);
        self.drop_orphan_controls(tx);
        self.drop_orphan_comments(tx);
        tx.tracked = tracked;
        self.reresolve_stale(tx);
        // An edit that would delete an undeletable picture control ("Content control cannot be deleted") does
        // nothing.
        if tx.kept < 0 {
            return Err("a content control that can't be deleted");
        }
        // Paragraphs came, went or changed their properties: list labels follow (invariant 9).
        if tx.undo.iter().any(|s| !matches!(s, Step::Text { .. } | Step::Restyle { .. } | Step::AddImage { .. })) {
            self.renumber(tx);
        }
        Ok(())
    }

    /// What follows a transaction's steps: layout, the edit locations, the elements the host knows, the design labels
    /// and the picture controls' image sets.
    fn effects(&mut self, tx: &mut Tx) {
        let hints: Vec<Option<Change>> = tx.hints.iter().zip(&tx.undo).filter(|(_, s)| s.lays_out()).map(|(h, _)| *h).collect();
        self.relayout(hints);
        let maps = std::mem::take(&mut tx.maps);
        self.map_marks(&maps);
        // Undo and redo go back to edits already marked.
        if !matches!(tx.origin, Origin::Undo | Origin::Redo) {
            self.mark_edit();
            self.last_maps = maps;
        }
        self.track_elements(&tx.undo);
        self.refresh_labels(Some(&tx.undo));
        if tx.pictures {
            self.doc.derive_image_sets();
        }
    }

    /// Records a transaction whose steps applied and laid out: the next revision, its place in the history, and its
    /// record for the session.
    fn finish(&mut self, mut tx: Tx, label: Cow<'static, str>, typing: Option<TypingRun>, fonts: &Fonts) -> Outcome {
        self.revision += 1;
        let undo = std::mem::take(&mut tx.undo);
        let bytes = undo.iter().map(history::step_bytes).sum();
        match tx.replayed.take() {
            // The steps that reverse this one take the replayed transaction's place: undone, it can be redone; redone,
            // undone again.
            Some(done) if tx.origin == Origin::Undo => self.history.push_redo(Transaction { undo, bytes, ..done }),
            Some(done) => self.history.push_undo_keep_redo(Transaction { undo, bytes, ..done }),
            None => {
                let after = self.record();
                self.history.record(Transaction { undo, before: tx.before, after, label: label.clone(), bytes, id: 0 }, typing);
            }
        }
        self.commits.push(Committed { revision: self.revision, origin: tx.origin, label, targets: tx.targets });
        self.after_edit(fonts);
        Outcome::Changed(self.revision)
    }

    fn after_edit(&mut self, fonts: &Fonts) {
        // Tests check every transaction: the incremental index equals a fresh build, and the integrity
        // invariants hold.
        #[cfg(test)]
        {
            assert_eq!(self.index, DocIndex::build_with_parts(&self.doc, self.index.parts()), "the incremental index differs from a fresh build");
            let v = integrity::check(&self.doc, &self.index, &self.selection);
            let text =
                self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p)).map(|p| UnitText::of(p).text);
            assert!(v.is_empty(), "integrity: {v:?} ({:?} in {text:?})", self.selection);
            let labels = self.label_changes();
            assert!(labels.is_empty(), "integrity: 9 list labels: {} differ from a fresh numbering, first {:?}", labels.len(), labels.first());
        }
        if let Some(p) = self.caret() {
            self.ensure(&p, fonts);
            self.scroll_to_caret(fonts);
        }
    }

    /// Undo (or redo): the last transaction's steps reversed, committed as a transaction of their own from
    /// [`Origin::Undo`] (or [`Origin::Redo`]).
    fn undo(&mut self, undo: bool, fonts: &Fonts) -> Outcome {
        // A batch is one transaction: undo can't take a part of it.
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
        self.commit(tx, None, fonts)
    }

    /// Applies recorded steps last to first (undoing what they record) into `tx`, which then holds their inverses.
    fn replay(&mut self, tx: &mut Tx, steps: Vec<Step>) {
        for s in steps.into_iter().rev() {
            if self.apply_step(tx, s).is_err() {
                // A history that doesn't fit the document is a bug; lay out fully so the screen stays right.
                self.relayout([None]);
                break;
            }
        }
    }

    // ---------------- batches ----------------

    /// Starts a batch: until it ends, what commands commit folds into one transaction, labelled `label` (else its
    /// first commit's label). A batch begun inside another joins it.
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

    /// Ends the batch begun last. With `keep`, the outermost commits as one transaction (when it changed anything);
    /// without, what the batch did is undone and the selection is as it was.
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

    /// Undoes what the batch's commits since `mark` did (they leave its transaction), back to `selection`.
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
        self.after_edit(fonts);
    }

    // ---------------- typing ----------------

    /// The run typed text takes at `pos`: the pending format, the text before the caret, a neighbour's,
    /// or the paragraph mark's.
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

    /// Deletes a text selection and returns where the caret goes: its start.
    fn delete_range(&mut self, tx: &mut Tx) -> Result<Option<Position>, StepError> {
        self.delete_range_joining(tx, true)
    }

    /// [`Editor::delete_range`]; `join`: the two ends of a range across a table become one paragraph (typing over it
    /// joins them; Delete and Backspace keep them apart). With tracking the deleted text stays, and the caret goes to
    /// the range's end, after it.
    fn delete_range_joining(&mut self, tx: &mut Tx, join: bool) -> Result<Option<Position>, StepError> {
        let end = self.selection.ordered(&self.index).map(|(_, b)| b).filter(|_| tx.tracked && matches!(self.selection, Selection::Text { .. }));
        let from = tx.maps.len();
        let at = self.delete_range_untracked(tx, join)?;
        Ok(match (at, end) {
            (Some(_), Some(b)) => Some(tx.maps[from..].iter().fold(b, |p, m| m.apply(p))),
            _ => at,
        })
    }

    /// [`Editor::delete_range_joining`] with the caret at the range's start.
    fn delete_range_untracked(&mut self, tx: &mut Tx, join: bool) -> Result<Option<Position>, StepError> {
        // Over a cell block, typing replaces the first cell's text only.
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
        // Across a table's rows (also from a drag into or out of a table, which ends at a row's end mark).
        if let Some(p) = self.delete_across_tables(tx, a, b)? {
            return Ok(Some(p));
        }
        let (Some((pa, oa)), Some((pb, ob))) = (a.para_offset(), b.para_offset()) else {
            // A row end mark at either end: delete the text of the paragraphs between.
            return self.delete_paragraph_texts(tx, a, b).map(|()| Some(a));
        };
        if pa == pb {
            self.delete_text(tx, pa, oa..ob)?;
            return Ok(Some(Position::text(a.story, pa, oa)));
        }
        let (sa, sb) = (self.index.para_slot(pa).ok_or(StepError::Missing)?, self.index.para_slot(pb).ok_or(StepError::Missing)?);
        if sa.container == sb.container && oa == Unit(0) && ob == Unit(0) {
            // Whole paragraphs with their marks: they go, and the paragraph after keeps its own properties.
            self.step(tx, Step::Blocks { container: sa.container, range: sa.block..sb.block, with: Vec::new() })?;
            return Ok(Some(Position::text(a.story, pb, Unit(0))));
        }
        if sa.container == sb.container {
            // Siblings: drop what lies between (tables too), then join the two ends; with a table between, Delete keeps
            // them apart.
            let table_between =
                self.index.blocks(&self.doc, sa.container).is_some_and(|bl| bl[sa.block + 1..sb.block].iter().any(|b| matches!(b, Block::Table(_))));
            let len_a = para_len(self.index.para(&self.doc, pa).ok_or(StepError::Missing)?);
            self.delete_text(tx, pb, Unit(0)..ob)?;
            self.delete_text(tx, pa, oa..len_a)?;
            if sb.block > sa.block + 1 {
                self.step(tx, Step::Blocks { container: sa.container, range: sa.block + 1..sb.block, with: Vec::new() })?;
            }
            if table_between && !join {
                return Ok(Some(Position::text(a.story, pa, oa)));
            }
            // A block-level control's edge: the text goes, the paragraphs stay apart.
            let controls = |e: &Editor, p: ParaId| e.index.para(&e.doc, p).map(|x| x.controls.clone());
            if controls(self, pa) != controls(self, pb) {
                return Ok(Some(Position::text(a.story, pa, oa)));
            }
            self.step(tx, Step::Join { first: pa, second: pb })?;
            tx.stale_paras.push(pa);
            return Ok(Some(Position::text(a.story, pa, oa)));
        }
        // Across cells of one table: only the text goes; cells never merge.
        self.delete_paragraph_texts(tx, a, b)?;
        Ok(Some(a.with_affinity(Affinity::Downstream)))
    }

    /// A range from outside a table into it, out of it, or from one table to another at the same level: the
    /// rows it touches go whole; text left before a table whose rows remain moves into its first cell. `None` when
    /// both ends are in the same table at their shared level (cells of one table: only text goes).
    fn delete_across_tables(&mut self, tx: &mut Tx, a: Position, b: Position) -> Result<Option<Position>, StepError> {
        let (ca, cb) = (self.index.chain_of(&a), self.index.chain_of(&b));
        let k = ca.iter().zip(&cb).take_while(|(x, y)| x.table == y.table && x.row == y.row && x.cell == y.cell).count();
        // The ends' blocks in the container both are in: a paragraph, or a table at that level (with the row).
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
        // The b side first (later in the container), so the a side's indices hold.
        let mut moved_into: Option<ParaId> = None;
        match tb {
            Some((table, rb)) => {
                let n = self.index.table(&self.doc, table).map_or(0, |t| t.rows.len());
                if rb + 1 >= n {
                    self.step(tx, Step::Blocks { container: sb.container, range: sb.block..sb.block + 1, with: Vec::new() })?;
                } else {
                    // A vertical merge that started in a row going on restarts in the first row kept.
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
                // The caret: what followed, the b side's paragraph or table.
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
                // The rest of the paragraph before joins the remaining table's first cell ("Bef" + "c31").
                if let Some(target) = moved_into {
                    let inlines: Vec<Inline> =
                        self.index.para(&self.doc, pa).map(|p| p.inlines.iter().filter(|i| i.units() > 0).cloned().collect()).unwrap_or_default();
                    let n: u32 = inlines.iter().map(|i| i.units() as u32).sum();
                    if !inlines.is_empty() {
                        self.step(tx, Step::Text { para: target, range: Unit(0)..Unit(0), with: inlines, edges: Default::default() })?;
                    }
                    let slot = self.index.para_slot(pa).ok_or(StepError::Missing)?;
                    self.step(tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() })?;
                    tx.stale_paras.push(target);
                    return Ok(Some(Position::text(story, target, Unit(n))));
                }
                Ok(Some(Position::text(story, pa, oa)))
            }
        }
    }

    /// Deletes units `range` of `para`, keeping content controls' edges: the range is
    /// cut at each control marker whose partner it leaves out, so that marker stays (and the control with what is
    /// left of it); a range touching a drop-down list's content without covering the whole control is refused.
    /// Comment markers stay the same way, except those of a comment whose text the range takes whole: it goes too.
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
            // Strictly inside the range, its partner outside: the range is cut there.
            if a < u && u < b && !(a <= cs && ce <= b) {
                cuts.push(u);
            }
        }
        let (comment_cuts, emptied) = comments::deletion_cuts(p, a, b);
        cuts.extend(comment_cuts);
        cuts.sort();
        cuts.dedup();
        if !emptied.is_empty() {
            self.remove_comments(tx, &emptied)?;
        }
        // A control the range takes whole whose marker sits on the range's edge (the other inside it): that marker
        // leaves first, so the control goes whole. Removed before the deletion, it comes back with the paragraph's
        // other inlines, in place, on undo (a restyle keeps every inline).
        let mut lone: Vec<(crate::docx::ControlId, bool)> = Vec::new();
        for &(u, c, start) in &marks {
            let Some(o) = marks.iter().find(|(_, d, s)| *d == c && *s != start).map(|m| m.0) else { continue };
            let inside = |x: u32| a < x && x < b;
            if (u == a || u == b) && inside(o) {
                lone.push((c, start));
            }
        }
        if !lone.is_empty() {
            let p = self.index.para(&self.doc, para).ok_or(StepError::Missing)?;
            let inlines: Vec<Inline> = p
                .inlines
                .iter()
                .filter(|i| {
                    !matches!(i, Inline::ControlStart(c) if lone.contains(&(*c, true)))
                        && !matches!(i, Inline::ControlEnd(c) if lone.contains(&(*c, false)))
                })
                .cloned()
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

    /// Deletes the selected text of every paragraph from `a` to `b` without joining any.
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
                // On to the next paragraph's start.
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
        if text.is_empty() {
            return Outcome::Unchanged("empty text");
        }
        // Space with an unfilled placeholder selected activates it.
        if text == " " && self.activate_placeholder() {
            return Outcome::Unchanged("placeholder activated");
        }
        // Space with a checkbox selected toggles it; in design mode it changes nothing (the checkbox
        // doesn't toggle, and the space doesn't type over it).
        if text == " "
            && let Some(p) = self.selected_checkbox()
        {
            return if self.mode == Mode::Design { Outcome::Unchanged("design mode") } else { self.toggle_checkbox(p, fonts) };
        }
        self.object_to_caret();
        if self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let mut tx = self.begin("Typing");
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => p,
            Ok(None) => return Outcome::Unchanged("no caret"),
            Err(_) => {
                self.abort(tx);
                return Outcome::Unchanged("stale selection");
            }
        };
        // At a row's end mark, text goes to the start of what follows.
        let caret = self.past_row_end(caret);
        let Place::Text { para, offset, .. } = caret.place else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let Some(run) = self.run_at(&caret) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let (mut para, mut at) = (para, offset);
        let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
        for (k, line) in lines.iter().enumerate() {
            if k > 0 {
                let Some(second) = self.split_template(para) else { break };
                let id = ParaId(second.id);
                if self.step(&mut tx, Step::Split { para, at, second }).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("split failed");
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
            if n > 0 && self.step(&mut tx, Step::Text { para, range: at..at, with: inlines, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("insert failed");
            }
            at = Unit(at.0 + n);
        }
        // Text typed before a combining mark joins its cluster: the caret goes after the cluster.
        let at = self.index.para(&self.doc, para).map(UnitText::of).map_or(at, |t| if t.is_boundary(at) { at } else { t.next_grapheme(at) });
        self.selection = Selection::Caret(Position::text(caret.story, para, at));
        // Typing over a selection starts a typing run; what follows joins it.
        let single = lines.len() == 1;
        let typing = single.then_some(TypingRun { para, at, kind: Typing::Insert });
        self.commit(tx, typing, fonts)
    }

    /// From a row's end mark, the first text place after it; other places are themselves.
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

    /// Ctrl+Shift+Enter in text: a column break that starts a new paragraph (the text before the caret keeps
    /// its paragraph); right after a break or at a paragraph's start, only the break.
    fn column_break(&mut self, fonts: &Fonts) -> Outcome {
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
        if self.step(&mut tx, Step::Split { para, at, second }).is_err()
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

    /// A new paragraph after `para` with its properties (Enter).
    fn split_template(&self, para: ParaId) -> Option<Paragraph> {
        let p = self.index.para(&self.doc, para)?;
        Some(Paragraph {
            style: p.style.clone(),
            mark: p.mark.clone(),
            list: p.list.clone(),
            inlines: Vec::new(),
            src: None,
            id: crate::docx::next_para_id(),
            // The new paragraph is in the same block-level controls.
            controls: p.controls.clone(),
        })
    }

    fn enter(&mut self, fonts: &Fonts) -> Outcome {
        // Design mode: Enter with a field or control selected reports it to the host and does nothing else.
        if self.mode == Mode::Design && self.activate(false) {
            return Outcome::Unchanged("activated");
        }
        // Enter with an unfilled placeholder selected activates it.
        if self.activate_placeholder() {
            return Outcome::Unchanged("placeholder activated");
        }
        if let Some(o) = self.list_enter(fonts) {
            return o;
        }
        let mut tx = self.begin("Enter");
        let collapsed = matches!(self.selection, Selection::Caret(_));
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => p,
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        // Enter at a row's end mark adds an empty row below.
        if let Place::RowEnd { table, row } = caret.place {
            self.abort(tx);
            return self.add_row(table, row, caret.story, fonts);
        }
        let Some((para, at)) = caret.para_offset() else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        // At the start of a table's first cell with nothing before the table in its container, a paragraph
        // goes before the table.
        if at == Unit(0)
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
                return Outcome::Unchanged("no paragraph");
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
                return Outcome::Unchanged("insert failed");
            }
            tx.stale_paras.push(new_id);
            // From a caret, the caret goes into the new paragraph; over a selection it stays at the cell's
            // start (E2E-EDIT-11).
            self.selection = Selection::Caret(if collapsed { Position::text(caret.story, new_id, Unit(0)) } else { caret });
            return self.commit(tx, None, fonts);
        }
        let Some(mut second) = self.split_template(para) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        // At a paragraph's end, the new one is in the style's next style (a heading's: Normal), resolved again there.
        let at_end = self.index.para(&self.doc, para).is_some_and(|p| para_len(p) == at);
        if at_end && let Some(s) = self.next_style_paragraph(para, second.clone()) {
            second = s;
            tx.stale_paras.push(ParaId(second.id));
        }
        let id = ParaId(second.id);
        if self.step(&mut tx, Step::Split { para, at, second }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("split failed");
        }
        self.selection = Selection::Caret(Position::text(caret.story, id, Unit(0)));
        self.commit(tx, None, fonts)
    }

    /// When `container` is the first cell of a table that is the first block of its own container, that
    /// table's slot.
    fn table_needing_paragraph_before(&self, container: Container) -> Option<index::Slot> {
        let Container::Cell { table, row: 0, cell: 0 } = container else { return None };
        let s = self.index.table_slot(table)?;
        (s.block == 0).then_some(s)
    }

    fn insert_inline(&mut self, make: impl Fn(Run) -> Inline, label: &'static str, fonts: &Fonts) -> Outcome {
        self.insert_inlines(|run| vec![make(run)], label, fonts)
    }

    /// Inlines at the caret (replacing the selection) as one step; the caret after them.
    fn insert_inlines(&mut self, make: impl Fn(Run) -> Vec<Inline>, label: &'static str, fonts: &Fonts) -> Outcome {
        if self.refuse_locked() {
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
        self.selection = Selection::Caret(Position::text(caret.story, para, Unit(at.0 + n)));
        self.commit(tx, None, fonts)
    }

    /// Backspace and Delete.
    fn delete(&mut self, dir: Dir, word: bool, fonts: &Fonts) -> Outcome {
        if let Some(o) = self.delete_object(fonts) {
            return o;
        }
        if let Some(o) = self.delete_cells(dir == Dir::Forward, fonts) {
            return o;
        }
        if dir == Dir::Back
            && let Some(o) = self.list_backspace(fonts)
        {
            return o;
        }
        // A field's brace or edge: the first press selects the whole field, the next deletes it.
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
        // Locks: the selection, or the unit Backspace or Delete would take.
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
            // At a row end mark, Backspace moves into the row's last cell; Delete does nothing.
            if dir == Dir::Back {
                let p = self.nav().left(caret, false);
                self.set_selection(Selection::Caret(p));
                return Outcome::Moved;
            }
            return Outcome::Unchanged("row end");
        };
        let Some(t) = self.index.para(&self.doc, para).map(UnitText::of) else { return Outcome::Unchanged("no paragraph") };
        let typing = |at| TypingRun { para, at, kind: if dir == Dir::Back { Typing::Backspace } else { Typing::Delete } };
        if dir == Dir::Back && off > Unit(0) {
            let from = if word { t.prev_word(off) } else { t.prev_grapheme(off) };
            // With tracking, Backspace over deleted text moves the caret into it and changes nothing.
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
            // With tracking the deleted text stays: the caret goes past it.
            let end = if tx.tracked { tx.maps[from..].iter().fold(Position::text(caret.story, para, to), |p, m| m.apply(p)) } else { caret };
            let at = end.para_offset().map_or(off, |(_, u)| u);
            self.selection = Selection::Caret(end);
            return self.commit(tx, Some(typing(at)), fonts);
        }
        // At a paragraph boundary: join with the sibling paragraph; cells and tables never merge.
        let Some(slot) = self.index.para_slot(para) else { return Outcome::Unchanged("no paragraph") };
        let blocks = self.index.blocks(&self.doc, slot.container).map(|b| b.len()).unwrap_or(0);
        let sibling = |k: usize| match self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(k)) {
            Some(Block::Para(p)) => Some(Ok((ParaId(p.id), para_len(p)))),
            Some(Block::Table(t)) => Some(Err(TableId(t.id))),
            None => None,
        };
        // Paragraphs in different block-level controls stay apart.
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
                // After a table: nothing, even in an empty paragraph.
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
                    // With tracking the deleted mark stays: the caret goes past it, to the next paragraph's start.
                    let end = match tx.tracked {
                        true => tx.maps[from..].iter().fold(Position::text(caret.story, next, Unit(0)), |p, m| m.apply(p)),
                        false => Position::text(caret.story, para, off),
                    };
                    self.selection = Selection::Caret(end);
                    self.commit(tx, None, fonts)
                }
                // Before a table: the paragraph's text moves into the start of the first cell.
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
                    let Some(inlines) =
                        self.index.para(&self.doc, para).map(|p| p.inlines.iter().filter(|i| i.units() > 0).cloned().collect::<Vec<_>>())
                    else {
                        return Outcome::Unchanged("no paragraph");
                    };
                    let n: u32 = inlines.iter().map(|i| i.units() as u32).sum();
                    if (!inlines.is_empty()
                        && self
                            .step(&mut tx, Step::Text { para: target, range: Unit(0)..Unit(0), with: inlines, edges: Default::default() })
                            .is_err())
                        || self
                            .step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() })
                            .is_err()
                    {
                        self.abort(tx);
                        return Outcome::Unchanged("move failed");
                    }
                    tx.stale_paras.push(target);
                    self.selection = Selection::Caret(Position::text(caret.story, target, Unit(n)));
                    self.commit(tx, None, fonts)
                }
                _ => Outcome::Unchanged("end of a cell or story"),
            },
        }
    }

    /// Tab / Shift+Tab in a table: the next (previous) cell's content is selected, in
    /// row order, a vertically merged cell once per row it spans; a cell starting with a nested table selects that
    /// table's cells. Tab in the last cell of the caret's innermost table adds a row to that table only.
    fn next_cell(&mut self, back: bool, fonts: &Fonts) -> Outcome {
        // From a nested table's cell block: Tab goes into its first cell, Shift+Tab before its container cell.
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
                    // A continuation row's stop: the selection Tab made there.
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
            None => {
                let last = t.rows.len() - 1;
                self.add_row(table, last, story, fonts)
            }
        }
    }

    /// Tab's selection of cell (`row`, `cell`): its content (a merge continuation's is its region's first cell's),
    /// or the cells of the nested table it starts with.
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

    /// The text range of a cell's whole content.
    fn cell_content(&self, table: TableId, row: usize, cell: usize) -> Option<Selection> {
        let t = self.index.table(&self.doc, table)?;
        let story = self.index.table_slot(table).map_or(self.editing(), |s| self.index.story(s.container));
        let blocks = &t.rows.get(row)?.cells.get(cell)?.blocks;
        let a = blocks.iter().find_map(first_para_of)?;
        let (lp, len) = blocks.iter().rev().find_map(last_para_of)?;
        Some(Selection::range(Position::text(story, a, Unit(0)), Position::text(story, lp, len), Granularity::Char))
    }

    /// Adds an empty row like row `after` below it, in this table only, with the caret in its first cell.
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

    // ---------------- movement ----------------

    fn move_caret(&mut self, by: MoveBy, dir: Dir, extend: bool, fonts: &Fonts) -> Outcome {
        let forward = dir == Dir::Forward;
        // Shift+arrows on a cell block: its focus cell moves a row or a column.
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
        // Left/Right without Shift collapse a selection to its start or end.
        if !extend
            && matches!(by, MoveBy::Char)
            && !self.selection.is_collapsed()
            && let Some((a, b)) = self.selection.ordered(&self.index)
        {
            self.set_selection(Selection::Caret(if forward { b } else { a }));
            return Outcome::Moved;
        }
        let Some(focus) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
        if matches!(by, MoveBy::Line | MoveBy::LineEdge | MoveBy::Screen | MoveBy::Page | MoveBy::ScreenEdge) {
            self.ensure(&focus, fonts);
        }
        let target = match by {
            MoveBy::Char | MoveBy::Word => {
                let word = by == MoveBy::Word;
                let t = if forward { self.nav().right(focus, word) } else { self.nav().left(focus, word) };
                // All notes of a kind are one story: past a note's edge, the next or previous note.
                if t.same_place(&focus) { self.next_note_edge(focus.story, forward).or(Some(t)) } else { Some(t) }
            }
            MoveBy::Para => Some(if forward { self.nav().next_para(focus) } else { self.nav().prev_para(focus) }),
            // From notes and text boxes, Ctrl+Home/End go to the main document's start or end.
            MoveBy::Story => match focus.story {
                StoryId::Footnote(_) | StoryId::Endnote(_) | StoryId::TextBox(_) => self.nav().story_edge(StoryId::Body, forward),
                s => self.nav().story_edge(s, forward),
            },
            MoveBy::RowEdge => self.nav().table_edge(focus, true, forward),
            MoveBy::ColumnEdge => self.nav().table_edge(focus, false, forward),
            MoveBy::LineEdge => self.nav().line_edge(focus, forward).or_else(|| self.clipped_line_edge(focus, forward)),
            MoveBy::Line => {
                let x = match self.goal_x {
                    Some(x) => x,
                    None => {
                        let x = match self.nav().caret_x(focus) {
                            Some(x) => x,
                            None => self.clipped_x(focus).unwrap_or(0.0),
                        };
                        self.goal_x = Some(x);
                        x
                    }
                };
                // Clipped text has no lines: moves into, through and out of it first.
                match self.clipped_vertical(focus, x, forward) {
                    Some(t) => Some(t),
                    None => {
                        let t = self.nav().vertical(focus, x, forward);
                        // Past the story's first or last line: the next header, footer, note or body line where moves
                        // cross stories, else the story's start or end.
                        t.or_else(|| self.cross_vertical(focus, x, forward, fonts)).or_else(|| self.nav().story_edge(focus.story, forward))
                    }
                }
            }
            MoveBy::Screen | MoveBy::Page if matches!(focus.story, StoryId::HeaderFooter(_)) => self.page_header_footer(focus, forward, fonts),
            MoveBy::Screen | MoveBy::Page | MoveBy::ScreenEdge => self.page_move(by, forward, focus, fonts),
        };
        let Some(target) = target else { return Outcome::Unchanged("no place to move to") };
        let goal = self.goal_x;
        let s = if extend {
            let anchor = self.selection.anchor().unwrap_or(focus);
            // Across cells, the shapes a drag gives (Shift+Down into the next cell is a cell block).
            match self.drag_across_cells(anchor, target, false) {
                Some((s, _)) => s,
                None => Selection::range(anchor, target, Granularity::Char),
            }
        } else {
            Selection::Caret(target)
        };
        self.set_selection(s);
        if matches!(by, MoveBy::Line | MoveBy::Screen) {
            self.goal_x = goal;
        }
        self.ensure(&target, fonts);
        self.scroll_to_caret(fonts);
        Outcome::Moved
    }

    /// Page Up/Down keep the caret's place on screen; Ctrl+Page Up/Down go to a page's top; Ctrl+Alt to the screen's
    /// top or bottom.
    fn page_move(&mut self, by: MoveBy, forward: bool, focus: Position, fonts: &Fonts) -> Option<Position> {
        let stack = self.stack();
        let caret = self.caret_view()?;
        let (cx, cy) = stack.to_doc(caret.page, caret.x, caret.top)?;
        let x = *self.goal_x.get_or_insert(caret.x);
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

    // ---------------- pointer ----------------

    fn hit(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<Position> {
        let (p, _) = self.hit_full(x, y, fonts)?;
        Some(p)
    }

    /// [`Editor::hit`] on pages already laid out (nothing is laid out for it): for the view, which has no fonts.
    fn hit_laid_out(&mut self, x: f64, y: f64) -> Option<Position> {
        let (page, px, py) = self.stack().page_at(x, y)?;
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        let hit = geo.hit_text(px, py, &|s| stories::reachable(ctx, s))?;
        Some(Position { story: hit.story, place: Place::Text { para: hit.para, offset: hit.unit, affinity: hit.affinity } })
    }

    /// [`Editor::hit`], and whether the point is in a repeated header row's copy.
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

    /// The selection bar under a point: the page and line beside it.
    fn bar_hit(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(Position, Position)> {
        if self.editing() != StoryId::Body {
            return None;
        }
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        // To 2 px left of the text column.
        let section = self.snapshot().info().get(page).map_or(0, |i| i.section);
        let left = self.doc.sections.get(section).map_or(72.0, |s| s.left) - 1.5 / self.viewport.zoom.max(0.1);
        let i = geo.selection_bar(px, py, left)?;
        self.near = page;
        self.line_span_of(&geo, i)
    }

    /// A line's start and end; a paragraph's last line ends after its mark.
    fn line_span_of(&mut self, geo: &geometry::PageGeometry, i: usize) -> Option<(Position, Position)> {
        let l = &geo.lines[i];
        let story = geo.stories[l.story].story;
        // From its first stop to its last, which belong to different paragraphs where the view joins them (REV-6).
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

    /// The line showing `p`.
    fn line_span(&mut self, p: Position) -> (Position, Position) {
        match self.nav().line_of(p) {
            Some(at) => self.line_span_of(&at.geo, at.line).unwrap_or((p, p)),
            None => (p, p),
        }
    }

    /// The place after a paragraph's mark: the next paragraph's start when it is a sibling paragraph, else the end
    /// (the last paragraph of a story or cell, or one before a table).
    fn after_mark(&mut self, end: Position) -> Position {
        let Some((para, _)) = end.para_offset() else { return end };
        let next = self.nav().right(end, false);
        match (next.para_offset(), self.index.para_slot(para)) {
            (Some((q, Unit(0))), Some(s)) if self.index.para_slot(q).is_some_and(|t| t.container == s.container && t.block == s.block + 1) => next,
            _ => end,
        }
    }

    /// What a gesture of `unit` selects at `p`.
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

    /// The table zone under a point, on the editing story.
    fn zone_hit(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<geometry::Zone> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let z = geo.zone(px, py, 0.75 / self.viewport.zoom.max(0.1))?;
        let table = match z {
            geometry::Zone::Cell(t, ..) | geometry::Zone::Row(t, _) | geometry::Zone::Column(t, ..) | geometry::Zone::Table(t) => t,
        };
        let story = self.index.table_slot(table).map(|s| self.index.story(s.container));
        (story.map(|s| stories::reachable(self.editing(), s)) == Some(true)).then_some(z)
    }

    fn pointer(&mut self, kind: PointerKind, x: f64, y: f64, clicks: u8, mods: keymap::Mods, fonts: &Fonts) {
        match kind {
            PointerKind::Down => {
                // In extend mode a click extends the selection.
                let mods = if self.modes.extend > 0 && clicks == 1 { keymap::Mods { shift: true, ..mods } } else { mods };
                self.goal_x = None;
                self.history.break_typing();
                // Design mode: a region's title tab selects the region.
                if self.mode == Mode::Design && self.region_title_click(x, y, fonts) {
                    return;
                }
                if self.story_click(x, y, clicks, fonts) {
                    return;
                }
                if !mods.shift && self.object_press(x, y, mods.ctrl, fonts) {
                    return;
                }
                if !mods.shift && clicks == 1 && self.checkbox_press(x, y, fonts) {
                    return;
                }
                // A link: Ctrl+click (a click when read only) activates it, for the application to open.
                if clicks == 1
                    && !mods.shift
                    && (mods.ctrl || mods.meta || self.mode == Mode::ReadOnly)
                    && let Some(address) = self.link_under(x, y, fonts)
                {
                    self.events.push(input::Notice::Link { address, ctrl: mods.ctrl || mods.meta });
                    return;
                }
                // A double-click on a column border auto-fits the column to its left.
                if clicks == 2
                    && let Some((_, table, resize::Border::Column(k), ..)) = self.border_at(x, y, fonts)
                    && k > 0
                {
                    self.border_drag = None;
                    self.history.break_typing();
                    self.auto_fit_column(table, k - 1, fonts);
                    return;
                }
                if clicks < 2 && self.border_press(x, y, mods.shift, mods.ctrl, fonts) {
                    return;
                }
                if let Some(z) = self.zone_hit(x, y, fonts) {
                    // A cell, row or column zone: a click selects it; a double-click in a cell or row zone,
                    // the row.
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
                        self.set_selection(Selection::Cells {
                            table,
                            anchor: selection::CellRef { row: a.0, col: a.1 },
                            focus: selection::CellRef { row: f.0, col: f.1 },
                            shape,
                        });
                        self.drag = None;
                        // A drag from the zone extends the block cell by cell, keeping its shape.
                        self.zone_drag = Some((table, selection::CellRef { row: a.0, col: a.1 }, shape));
                        self.requests.push(Request::CapturePointer);
                        return;
                    }
                }
                if let Some((a, b)) = self.bar_hit(x, y, fonts) {
                    // The selection bar: a line, a paragraph, the document.
                    let (unit, (a, b)) = match clicks {
                        _ if mods.ctrl || clicks >= 3 => {
                            self.select_all();
                            return;
                        }
                        2 => (Granularity::Paragraph, self.span(a, Granularity::Paragraph)),
                        _ => (Granularity::Line, (a, b)),
                    };
                    self.set_selection(Selection::range(a, b, unit));
                    self.drag = Some(Drag { anchor: a, span: Some((a, b)), unit, at: (x, y), block: false });
                    self.requests.push(Request::CapturePointer);
                    return;
                }
                let Some(p) = self.hit(x, y, fonts) else { return };
                if clicks == 2 && self.note_jump(p) {
                    return;
                }
                // A press inside the selection: drag-and-drop, or a click if it doesn't move.
                if clicks == 1 && !mods.shift && self.modes == extend::Modes::default() && self.mode != Mode::ReadOnly && self.inside_selection(&p) {
                    self.text_drag = Some(interaction::TextDrag { start: (x, y), press: p, moved: false, copy: mods.ctrl, drop: None });
                    self.requests.push(Request::CapturePointer);
                    return;
                }
                let unit = match clicks {
                    2 => Granularity::Word,
                    3.. => Granularity::Paragraph,
                    _ if mods.ctrl && !mods.shift => Granularity::Sentence,
                    _ => Granularity::Char,
                };
                if unit == Granularity::Char
                    && mods.shift
                    && let Some((sa, sb)) = self.selection.ordered(&self.index)
                {
                    // Shift+click: after a double-click, by whole words; otherwise from the anchor, which snaps
                    // to its word's edge once the click is outside that word (automatic word selection).
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
                    self.drag = Some(Drag { anchor, span, unit, at: (x, y), block: false });
                } else if unit == Granularity::Char {
                    self.set_selection(Selection::Caret(p));
                    self.drag = Some(Drag { anchor: p, span: None, unit, at: (x, y), block: false });
                } else {
                    let (a, b) = self.span(p, unit);
                    self.set_selection(Selection::range(a, b, unit));
                    self.drag = Some(Drag { anchor: a, span: Some((a, b)), unit, at: (x, y), block: false });
                }
                self.requests.push(Request::CapturePointer);
            }
            PointerKind::Move => {
                if self.text_drag.is_some() {
                    self.text_drag_move(x, y, mods.ctrl, fonts);
                    return;
                }
                if let Some(d) = self.object_drag.as_mut() {
                    d.now = (x, y);
                    return;
                }
                if let Some(d) = self.border_drag.as_mut() {
                    d.now = (x, y);
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
                    }
                    return;
                }
                let Some(mut d) = self.drag else {
                    self.cursor = self.hover(x, y, fonts);
                    self.set_hover(x, y);
                    return;
                };
                d.at = (x, y);
                self.drag = Some(d);
                self.autoscroll(x, y);
                let Some(p) = self.hit(x, y, fonts) else { return };
                if p.story != d.anchor.story {
                    return;
                }
                if d.unit == Granularity::Char
                    && let Some((s, block)) = self.drag_across_cells(d.anchor, p, d.block)
                {
                    if let Some(dd) = self.drag.as_mut() {
                        dd.block |= block;
                    }
                    self.selection = s;
                    self.pending = None;
                    return;
                }
                let s = match (d.unit, d.span) {
                    (Granularity::Char, _) | (_, None) => {
                        // Beyond the start word, a drag extends by whole words (automatic word selection).
                        let (wa, wb) = self.word_span(d.anchor);
                        let beyond = !(self.index.cmp(&p, &wa) != std::cmp::Ordering::Less && self.index.cmp(&p, &wb) != std::cmp::Ordering::Greater);
                        if beyond {
                            let (pa, pb) = self.word_span(p);
                            if self.index.cmp(&p, &d.anchor) == std::cmp::Ordering::Less {
                                Selection::range(wb, pa, Granularity::Char)
                            } else {
                                // Reaching a paragraph's end takes its mark (smart paragraph selection).
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
                self.selection = self.normalize(s);
                self.pending = None;
            }
            PointerKind::Cancel if self.object_drag.take().is_some() || self.border_drag.take().is_some() => {
                self.requests.push(Request::ReleasePointer)
            }
            PointerKind::Up if self.border_drag.is_some() => {
                self.border_release(fonts);
            }
            PointerKind::Up if self.object_drag.is_some() => {
                self.object_release(fonts);
            }
            PointerKind::Up if self.text_drag.is_some() => {
                self.text_drag_release(fonts);
            }
            PointerKind::Cancel if self.text_drag.take().is_some() => self.requests.push(Request::ReleasePointer),
            PointerKind::Up | PointerKind::Cancel if self.zone_drag.take().is_some() => self.requests.push(Request::ReleasePointer),
            PointerKind::Up | PointerKind::Cancel => {
                if self.drag.take().is_some() {
                    self.requests.push(Request::ReleasePointer);
                    self.requests.push(Request::Autoscroll { vx: 0.0, vy: 0.0 });
                }
            }
        }
    }

    /// A character drag's selection when it crosses table structure, and whether it is a cell block: into
    /// another cell of the start cell's table a block of cells; back in the start cell after that, the start cell;
    /// out of the table, whole rows from the start row to the edge crossed, plus the text; into a table from outside,
    /// whole rows of it. `None`: an ordinary text drag.
    fn drag_across_cells(&mut self, anchor: Position, p: Position, was_block: bool) -> Option<(Selection, bool)> {
        let a_chain = self.index.chain_of(&anchor);
        let p_chain = self.index.chain_of(&p);
        let forward = self.index.cmp(&p, &anchor) != std::cmp::Ordering::Less;
        let cell_ref = |e: &Editor, table: TableId, row: usize, cell: usize| -> Option<selection::CellRef> {
            let t = e.index.table(&e.doc, table)?;
            Some(selection::CellRef { row, col: table::Grid::of(t).col_of(row, cell.min(t.rows.get(row)?.cells.len().checked_sub(1)?))? })
        };
        // A table of the anchor's, further out than its innermost, that the pointer is in too, in another cell: a cell
        // block of that table (from an inner cell into another outer cell).
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
        // From a cell's text into a table inside that cell: whole rows of the inner table, as into a table from outside.
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
        // The start cell's table, at the deepest level both ends share or the anchor's innermost.
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
            // Out of the table: whole rows from the start row, plus the text up to the pointer.
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
            // Leaving a nested table to its containing cell keeps the inner rows; leaving an outer table the
            // same rule applies at its level.
            return Some((Selection::range(from, p, Granularity::Char), false));
        }
        // Into a table from outside: whole rows of it (the table at the anchor's level).
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

    /// Auto-scroll while dragging outside the viewport: `min(40 + 12·d, 2400)` px/s per axis, where d is the
    /// distance outside in px.
    fn autoscroll(&mut self, x: f64, y: f64) {
        let v = self.viewport;
        let speed = |outside: f64| {
            // Points to px: 96/72 at 100%, times the zoom.
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
        let Some(t) = self.index.para(&self.doc, para).map(UnitText::of) else { return (p, p) };
        let (a, b) = t.word_at(off);
        (Position::text(p.story, para, a), Position::text(p.story, para, b))
    }

    fn sentence_span(&self, p: Position) -> (Position, Position) {
        let Some((para, off)) = p.para_offset() else { return (p, p) };
        let Some(t) = self.index.para(&self.doc, para).map(UnitText::of) else { return (p, p) };
        let (a, b) = t.sentence_at(off);
        (Position::text(p.story, para, a), Position::text(p.story, para, b))
    }

    fn para_span(&self, p: Position) -> (Position, Position) {
        let Some((para, _)) = p.para_offset() else { return (p, p) };
        let len = self.index.para(&self.doc, para).map_or(Unit(0), para_len);
        (Position::text(p.story, para, Unit(0)), Position::text(p.story, para, len))
    }

    // ---------------- geometry for the view ----------------

    /// The caret's page and rectangle, from the current snapshot.
    pub fn caret_view(&mut self) -> Option<CaretView> {
        let focus = self.selection.focus()?;
        self.caret_at(focus)
    }

    /// Where a caret at `focus` shows.
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
            return Some(CaretView { page, x, top: l.top, bottom: l.bottom });
        }
        // A closing paragraph after a nested table has a caret place but no line.
        for page in self.pages.pages_of(para, near, &self.index) {
            let geo = self.pages.page(page, &self.index)?;
            if let Some(c) = geo.caret(para, unit, affinity) {
                self.near = page;
                return Some(CaretView { page, x: c.x, top: c.top, bottom: c.bottom });
            }
        }
        None
    }

    fn scroll_to_caret(&mut self, _fonts: &Fonts) {
        if let Some(c) = self.caret_view() {
            self.requests.push(Request::ScrollIntoView { page: c.page, rect: [c.x, c.top, c.x + 1.0, c.bottom] });
        }
    }

    /// Selection rectangles per page and line.
    /// The selection's highlight on `pages`.
    fn selection_view(&mut self, pages: std::ops::Range<usize>) -> SelectionView {
        if let Selection::Object(o) = self.selection {
            let mut out = Vec::new();
            for page in pages {
                let Some(geo) = self.pages.page(page, &self.index) else { continue };
                match o {
                    // While dragged, where the drag puts it (the preview).
                    _ if let Some(d) = self.object_drag.filter(|d| d.obj == o) => {
                        if page == d.page {
                            out.push((page, d.rect(self.object_locked(o))));
                        }
                    }
                    selection::ObjectRef::Floating(f) => out.extend(geo.stories.iter().filter(|s| s.float == Some(f)).map(|s| (page, s.rect))),
                    selection::ObjectRef::Inline { para, offset } => out.extend(self.inline_box(&geo, para, offset).map(|r| (page, r))),
                }
            }
            return SelectionView::Object(out, !self.object_fixed(o));
        }
        if let Selection::Block { .. } = self.selection {
            return SelectionView::Text(
                self.block_pieces().into_iter().filter(|(page, ..)| pages.contains(page)).map(|(page, r, ..)| (page, r)).collect(),
            );
        }
        if let Selection::Cells { table, anchor, focus, .. } = self.selection {
            let (r0, r1) = (anchor.row.min(focus.row), anchor.row.max(focus.row));
            let (c0, c1) = (anchor.col.min(focus.col), anchor.col.max(focus.col));
            let mut out = Vec::new();
            for page in pages {
                let Some(geo) = self.pages.page(page, &self.index) else { continue };
                for c in &geo.cells {
                    let Some(&(t, row, _)) = c.chain.last() else { continue };
                    if t == table && (r0..=r1).contains(&row) && c.col <= c1 && c.col + c.span.max(1) > c0 {
                        out.push((page, c.rect));
                    }
                }
            }
            return SelectionView::Cells(out);
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return SelectionView::None };
        if a.same_place(&b) {
            return SelectionView::None;
        }
        SelectionView::Text(self.range_rects(a, b, pages))
    }

    /// Each drawn line's box of the text from `a` to `b` (in one story) on `pages`: page coordinates.
    pub(super) fn range_rects(&mut self, a: Position, b: Position, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            // Each paragraph's part of each line (a line shows several where the view joins them, REV-6).
            for (l, para) in geo.lines.iter().flat_map(|l| l.paras.iter().map(move |&p| (l, p))) {
                if geo.stories[l.story].story != a.story {
                    continue;
                }
                let (Some(lf), Some(ll)) = (l.first_of(para), l.last_of(para)) else { continue };
                let first = Position::text(a.story, para, lf);
                let last = Position::text(a.story, para, ll);
                if self.index.cmp(&last, &a) == std::cmp::Ordering::Less || self.index.cmp(&first, &b) == std::cmp::Ordering::Greater {
                    continue;
                }
                let lo = if self.index.cmp(&first, &a) == std::cmp::Ordering::Less { a.para_offset().map(|(_, u)| u) } else { Some(lf) };
                let hi = if self.index.cmp(&last, &b) == std::cmp::Ordering::Greater { b.para_offset().map(|(_, u)| u) } else { Some(ll) };
                let (Some(lo), Some(hi)) = (lo, hi) else { continue };
                let (Some(x0), Some(x1)) = (l.x_of(para, lo), l.x_of(para, hi)) else { continue };
                // A selection running past the paragraph's end shows its mark as a space's width (a mark the view hides
                // between joined paragraphs isn't on the line's end, and shows none).
                let shown_end = l.stops.last().is_some_and(|s| s.end && s.para == para);
                let mark = if hi == ll && self.index.cmp(&last, &b) == std::cmp::Ordering::Less && shown_end { 4.0 } else { 0.0 };
                if x1 + mark > x0 + 0.01 {
                    out.push((page, [x0, l.top, x1 + mark, l.bottom]));
                }
            }
        }
        out
    }

    /// The rendering-facing state of a frame for the visible pages.
    pub fn view(&mut self, visible: std::ops::Range<usize>) -> View {
        let caret = self.caret_view();
        let caret_hidden = if caret.is_none() { self.caret_hidden() } else { None };
        let selection = self.selection_view(visible.clone());
        let story = self.editing();
        let anchor = match self.selection {
            Selection::Object(selection::ObjectRef::Floating(f)) => self.index.float_anchor(f).and_then(|para| {
                (0..self.pages.len()).find_map(|page| {
                    let geo = self.pages.page(page, &self.index)?;
                    geo.lines.iter().find_map(|l| l.span_of(para).map(|(x0, _)| (page, [x0, l.top])))
                })
            }),
            _ => None,
        };
        let visible_pages = visible.clone();
        let (marks, tooltip) = self.mark_views(visible.clone());
        let (comments, comment_anchors) = self.comment_views(visible.clone());
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
            guide: self.border_drag.map(|d| (d.page, d.guide())),
            anchor,
            placeholders: self.placeholder_views(&visible_pages),
            marks,
            tooltip,
            regions: self.region_views(visible_pages.clone()),
            looks: self.look_views(visible_pages.clone()),
            comments,
            comment_anchors,
            spelling: Vec::new(),
            formatting_marks: Vec::new(),
            boundaries: Vec::new(),
            drop: self.text_drag.filter(|d| d.moved).and_then(|d| d.drop).or(self.drop).and_then(|p| self.caret_at(p)),
            cursor: match self.border_drag.map(|d| d.border) {
                Some(resize::Border::Column(_)) => "col-resize",
                Some(resize::Border::Row(_)) => "row-resize",
                Some(resize::Border::Handle) => "nwse-resize",
                None if self.object_drag.is_some() => "move",
                None if self.text_drag.is_some_and(|d| d.moved) => {
                    if self.text_drag.is_some_and(|d| d.copy) {
                        "copy"
                    } else {
                        "default"
                    }
                }
                None => self.cursor,
            },
        }
    }

    /// Whether a document point is on the selection's highlight (a right-click there keeps the selection).
    pub fn selection_contains(&mut self, x: f64, y: f64) -> bool {
        let Some((page, px, py)) = self.stack().page_at(x, y) else { return false };
        match self.selection_view(page..page + 1) {
            SelectionView::Text(r) | SelectionView::Cells(r) | SelectionView::Object(r, _) => {
                r.iter().any(|(p, [x0, y0, x1, y1])| *p == page && px >= *x0 && px <= *x1 && py >= *y0 && py <= *y1)
            }
            SelectionView::None => false,
        }
    }

    /// The formatting marks on `pages`: each paragraph end, space, tab, break and hidden run there, from the lines'
    /// caret stops and the paragraphs' units.
    pub fn formatting_marks(&mut self, pages: std::ops::Range<usize>) -> Vec<view::FormattingMark> {
        use view::{FormattingMark, MarkKind};
        let mut out = Vec::new();
        let mut texts: HashMap<ParaId, UnitText> = HashMap::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for line in &geo.lines {
                for &para in &line.paras {
                    let text = match texts.entry(para) {
                        std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                        std::collections::hash_map::Entry::Vacant(e) => {
                            let Some(p) = self.index.para(&self.doc, para) else { continue };
                            e.insert(UnitText::of_view(p, self.doc.review.view))
                        }
                    };
                    let stops: Vec<&geometry::Stop> = line.stops.iter().filter(|s| s.para == para).collect();
                    let (top, bottom) = (line.top, line.bottom);
                    for w in stops.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        if b.unit.0 != a.unit.0 + 1 {
                            continue;
                        }
                        let mut mark = |kind, x0: f64, x1: f64| out.push(FormattingMark { page, kind, at: [x0, top, x1, bottom] });
                        if text.is_hidden(a.unit) {
                            mark(MarkKind::Hidden, a.x, b.x);
                        }
                        match text.char_at(a.unit) {
                            Some(' ') => mark(MarkKind::Space, a.x, b.x),
                            Some('\t') => mark(MarkKind::Tab, a.x, b.x),
                            Some('\u{2028}') => mark(MarkKind::LineBreak, a.x, b.x),
                            Some('\u{c}') => mark(MarkKind::Break, a.x, geo.w),
                            _ => {}
                        }
                    }
                    if let Some(end) = stops.iter().find(|s| s.unit == text.len()) {
                        out.push(FormattingMark { page, kind: MarkKind::Paragraph, at: [end.x, top, end.x, bottom] });
                    }
                }
            }
        }
        out
    }

    /// The text areas' and table cells' boundaries on `pages`: (page, box), page coordinates.
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
                out.extend(geo.cells.iter().map(|c| (page, c.rect)));
            }
        }
        out
    }

    /// Every table cell's box on `pages` (gridlines): (page, rect), page coordinates.
    pub fn cell_rects(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let mut out = Vec::new();
        for page in pages {
            if let Some(geo) = self.pages.page(page, &self.index) {
                out.extend(geo.cells.iter().map(|c| (page, c.rect)));
            }
        }
        out
    }

    /// The paragraphs with lines on `pages`, in order, each once (spelling marks the host draws).
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

    /// The rectangles units `a..b` of `para` cover on `pages`, one per line: (page, [x0, top, x1, bottom]).
    pub fn unit_rects(&mut self, para: ParaId, a: Unit, b: Unit, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
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
                    out.push((page, [x0.min(x1), l.top, x0.max(x1), l.bottom]));
                }
            }
        }
        out
    }

    /// Every laid-out (non-provisional) page's geometry, in page order.
    pub fn page_geometries(&mut self) -> Vec<(usize, Rc<geometry::PageGeometry>)> {
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        (0..self.pages.len()).filter_map(|i| self.pages.page(i, &self.index).map(|g| (i, g))).collect()
    }

    /// The fragment last copied or cut here.
    pub fn clipboard(&self) -> Option<&clipboard::Fragment> {
        self.clipboard.as_ref()
    }

    /// Takes another editor's fragment as this one's last copy (a paste between instances); its id
    /// here, counted on from this editor's own.
    pub fn adopt_clipboard(&mut self, mut f: clipboard::Fragment) -> u64 {
        f.id = self.clipboard.as_ref().map_or(1, |c| c.id + 1);
        let id = f.id;
        self.clipboard = Some(f);
        id
    }

    /// The id of the fragment last copied here (what a host's clipboard token names).
    pub fn clipboard_id(&self) -> Option<u64> {
        self.clipboard.as_ref().map(|f| f.id)
    }

    /// Copies the selection, its revisions accepted in every markup view: kept as the internal fragment, and asked of
    /// the host as text and HTML with its id.
    pub fn copy(&mut self) -> Option<clipboard::Fragment> {
        let mut f = self.copy_fragment()?.accepted();
        // The copy's controls travel with it; markers its range cut off don't.
        clipboard::drop_unpaired(&mut f.blocks);
        clipboard::drop_partial(&mut f.blocks, &self.index);
        f.controls = clipboard::controls_of(&f.blocks, &self.doc);
        self.requests.push(Request::Clipboard { text: f.text(), html: Some(f.html()) });
        self.clipboard = Some(f.clone());
        Some(f)
    }

    /// The selected text, paragraphs separated by newlines (plain-text copy).
    pub fn selected_text(&mut self) -> String {
        self.selected_text_within(usize::MAX).unwrap_or_default()
    }

    /// The selected text, as [`Editor::selected_text`], or `None` once it is longer than `limit` characters: the
    /// walk stops there.
    pub fn selected_text_within(&mut self, limit: usize) -> Option<String> {
        if let Selection::Block { .. } = self.selection {
            return Some(self.block_text()).filter(|t| t.chars().count() <= limit);
        }
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Some(String::new()) };
        let mut out = String::new();
        // Characters so far, each paragraph's newline counted.
        let mut chars = 0usize;
        let mut p = a;
        let mut guard = 0;
        while self.index.cmp(&p, &b) == std::cmp::Ordering::Less && guard < 1_000_000 {
            guard += 1;
            match p.para_offset() {
                Some((para, off)) => {
                    let Some(t) = self.index.para(&self.doc, para).map(UnitText::of) else { break };
                    let end = match b.para_offset() {
                        Some((pb, ob)) if pb == para => ob,
                        _ => t.len(),
                    };
                    let take = end.0.saturating_sub(off.0) as usize;
                    chars += take + 1;
                    if chars > limit.saturating_add(1) {
                        return None;
                    }
                    out.extend(t.text.chars().skip(off.0 as usize).take(take).map(|c| match c {
                        '\u{2028}' => '\n',
                        '\u{c}' | '\u{FFFC}' => ' ',
                        c => c,
                    }));
                    let at_end = Position::text(p.story, para, t.len());
                    let next = self.nav().right(at_end, false);
                    if next == at_end || self.index.cmp(&next, &b) == std::cmp::Ordering::Greater {
                        break;
                    }
                    if next.para_offset().is_some() {
                        out.push('\n');
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
        Some(out).filter(|t| t.chars().count() <= limit)
    }
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

/// Whether a split of `p` at `at` would leave an inline control's start in one paragraph and its end in the other:
/// `at` from the control's start up to (not including) its end.
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

/// Whether a step can change which pictures picture controls hold: it changes a control, or puts
/// pictures in (its inverse, pictures taken out).
fn moves_picture_controls(s: &Step) -> bool {
    matches!(s, Step::Control { .. }) || s.pictures_where(&|_| true) > 0
}
