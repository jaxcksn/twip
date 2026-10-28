//! Tracked revisions, comments and people in the document model. Revisions sit on what they change: a run's insert,
//! delete and format-change slots (the three a run can carry), a paragraph mark's run, and the property changes of
//! paragraphs, sections, tables, rows and cells, each with the properties before the change. Comment and move ranges
//! are zero-unit inline markers; comments and their threads live in [`Comments`].

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use super::model::{Block, Cell, ParaStyle, Paragraph, Raw, Row, Run, RunStyle, Section, SectionPage, Table, VMerge};
use super::props::{Border, Jc, Look, Width};

/// The people who made revisions and comments, in order of first appearance: an [`AuthorId`] is an index here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct People {
    pub persons: Vec<Person>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    /// `w:author`: the name revisions and comments are attributed to.
    pub name: Rc<str>,
    /// `w:initials` of the person's comments.
    pub initials: Option<Rc<str>>,
    /// `w15:presenceInfo` in the people part.
    pub identity: Option<Identity>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Identity {
    pub provider: Rc<str>,
    pub user_id: Rc<str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AuthorId(pub u32);

impl People {
    /// The author with this name and identity, added when new. Two identities tell people apart even when their
    /// names match; otherwise the name does (revisions name their author only by name), and a person found by name
    /// takes the identity given.
    pub fn author(&mut self, name: &str, identity: Option<&Identity>) -> AuthorId {
        let found = self.persons.iter().position(|p| match (&p.identity, identity) {
            (Some(a), Some(b)) => a == b,
            _ => &*p.name == name,
        });
        let i = match found {
            Some(i) => {
                if self.persons[i].identity.is_none() {
                    self.persons[i].identity = identity.cloned();
                }
                i
            }
            None => {
                self.persons.push(Person { name: name.into(), initials: None, identity: identity.cloned() });
                self.persons.len() - 1
            }
        };
        AuthorId(i as u32)
    }

    pub fn get(&self, id: AuthorId) -> Option<&Person> {
        self.persons.get(id.0 as usize)
    }
}

/// A revision's or comment's `w:id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RevisionId(pub i64);

/// A `w:date` (ISO 8601). The text as written is kept so an unchanged date saves exactly as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub raw: Rc<str>,
    /// Seconds since 1970-01-01 UTC; `None` when the text isn't a date this reads.
    pub utc: Option<i64>,
}

impl Timestamp {
    /// A date as written. Missing or unreadable dates stay as written, never invented.
    pub fn parse(raw: &str) -> Timestamp {
        Timestamp { raw: raw.into(), utc: parse_iso(raw) }
    }

    /// A UTC time written as `YYYY-MM-DDTHH:MM:SSZ`.
    pub fn from_utc(secs: i64) -> Timestamp {
        let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
        let (y, m, d) = civil_from_days(days);
        let raw = format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60);
        Timestamp { raw: raw.into(), utc: Some(secs) }
    }
}

/// `YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH:MM]`; no zone is read as UTC.
fn parse_iso(s: &str) -> Option<i64> {
    let num = |a: usize, b: usize| s.get(a..b).filter(|t| t.bytes().all(|c| c.is_ascii_digit())).and_then(|t| t.parse::<i64>().ok());
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    if s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let (mut secs, mut rest) = (days_from_civil(y, mo, d) * 86_400, &s[10..]);
    if let Some(t) = rest.strip_prefix('T') {
        let n = |a: usize, b: usize| t.get(a..b).and_then(|x| x.parse::<i64>().ok());
        let (h, mi) = (n(0, 2)?, n(3, 5)?);
        let sec = if t.as_bytes().get(5) == Some(&b':') { n(6, 8)? } else { 0 };
        secs += h * 3600 + mi * 60 + sec;
        let end = t.find(['Z', '+', '-']).unwrap_or(t.len());
        rest = &t[end..];
    }
    match rest.as_bytes().first() {
        None | Some(b'Z') => Some(secs),
        Some(&sign @ (b'+' | b'-')) => {
            let n = |a: usize, b: usize| rest.get(a..b).and_then(|x| x.parse::<i64>().ok());
            let off = n(1, 3)? * 3600 + n(4, 6).unwrap_or(0) * 60;
            Some(if sign == b'+' { secs - off } else { secs + off })
        }
        _ => None,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkKind {
    Insert,
    Delete,
    MoveFrom,
    MoveTo,
    /// A change of properties (`w:rPrChange`, `w:pPrChange`, a table, row, section or cell change, a merge): the
    /// element holding it says what changed.
    Format,
}

/// One tracked change's identity: shared by every run, mark or row it covers.
#[derive(Clone, Debug, PartialEq)]
pub struct RevisionMark {
    pub id: RevisionId,
    pub author: AuthorId,
    pub date: Option<Timestamp>,
    pub kind: MarkKind,
    /// The move range a `MoveFrom` / `MoveTo` belongs to: its two ends share it.
    pub move_name: Option<Rc<str>>,
}

/// A run's revisions: inserted (`Insert` or `MoveTo`), deleted (`Delete` or `MoveFrom`), both (another author deleted
/// an insertion), and a formatting change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunRevisions {
    pub ins: Option<Rc<RevisionMark>>,
    pub del: Option<Rc<RevisionMark>>,
    pub fmt: Option<Rc<FormatChange>>,
}

impl RunRevisions {
    pub fn is_empty(&self) -> bool {
        self.ins.is_none() && self.del.is_none() && self.fmt.is_none()
    }
}

/// `w:rPrChange`: the run's formatting before the change.
#[derive(Clone, Debug, PartialEq)]
pub struct FormatChange {
    pub mark: Rc<RevisionMark>,
    pub previous: Rc<RunStyle>,
    /// The earlier `w:rPr` as written, for saving.
    pub previous_raw: Raw,
}

/// `w:pPrChange`: the paragraph's properties before the change.
#[derive(Clone, Debug, PartialEq)]
pub struct ParaChange {
    pub mark: Rc<RevisionMark>,
    pub previous: Rc<ParaStyle>,
    pub previous_raw: Raw,
    /// `w:numberingChange` in the paragraph's `w:numPr`, as written.
    pub numbering_change: Raw,
}

/// `w:sectPrChange`: the section's page before the change.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionChange {
    pub mark: Rc<RevisionMark>,
    pub previous: SectionPage,
    pub previous_raw: Raw,
}

/// `w:tblPrChange`, `w:tblPrExChange`, `w:trPrChange` or `w:tcPrChange`: the properties before the change.
#[derive(Clone, Debug, PartialEq)]
pub struct TableChange {
    pub mark: Rc<RevisionMark>,
    /// The earlier properties as written, for saving.
    pub previous_raw: Raw,
    /// The earlier properties resolved as the table, row or cell holding the change is: what Original shows and
    /// rejecting restores. `None` for a row's table exceptions (`w:tblPrExChange`), which the model doesn't hold.
    pub previous: Option<PreviousProps>,
}

/// A table's, row's or cell's properties before a tracked change of them, resolved.
#[derive(Clone, Debug, PartialEq)]
pub enum PreviousProps {
    /// `w:tblPrChange`: the table's own properties.
    Table {
        style_id: Option<String>,
        width: Width,
        jc: Jc,
        ind: f64,
        borders: [Option<Border>; 6],
        fixed: bool,
        cell_spacing: f64,
        look: Option<Look>,
        bands: (i64, i64),
    },
    /// `w:trPrChange`.
    Row { height: Option<(f64, String)>, cant_split: bool, header: bool },
    /// `w:tcPrChange`: borders and margins top, left, bottom, right.
    Cell {
        width: Option<Width>,
        borders: [Option<Border>; 4],
        shading: Option<u32>,
        margins: [f64; 4],
        v_align: String,
        text_direction: String,
        no_wrap: bool,
    },
}

impl PreviousProps {
    /// The table's own properties as they are.
    pub fn of_table(t: &Table) -> PreviousProps {
        PreviousProps::Table {
            style_id: t.style_id.clone(),
            width: t.width,
            jc: t.jc,
            ind: t.ind,
            borders: t.borders.clone(),
            fixed: t.fixed,
            cell_spacing: t.cell_spacing,
            look: t.look,
            bands: t.bands,
        }
    }

    /// Gives `t` these properties, when they are a table's.
    pub fn restore_table(&self, t: &mut Table) {
        if let PreviousProps::Table { style_id, width, jc, ind, borders, fixed, cell_spacing, look, bands } = self {
            (t.style_id, t.width, t.jc, t.ind, t.borders) = (style_id.clone(), *width, *jc, *ind, borders.clone());
            (t.fixed, t.cell_spacing, t.look, t.bands) = (*fixed, *cell_spacing, *look, *bands);
        }
    }

    /// Gives `r` these properties, when they are a row's.
    pub fn restore_row(&self, r: &mut Row) {
        if let PreviousProps::Row { height, cant_split, header } = self {
            (r.height, r.cant_split, r.header) = (height.clone(), *cant_split, *header);
        }
    }

    /// Gives `c` these properties, when they are a cell's.
    pub fn restore_cell(&self, c: &mut Cell) {
        if let PreviousProps::Cell { width, borders, shading, margins, v_align, text_direction, no_wrap } = self {
            let [top, left, bottom, right] = borders.clone();
            (c.width, c.top, c.left, c.bottom, c.right, c.shading, c.margins) = (*width, top, left, bottom, right, *shading, *margins);
            (c.v_align, c.text_direction, c.no_wrap) = (v_align.clone(), text_direction.clone(), *no_wrap);
        }
    }
}

/// `w:tblGridChange`: the column widths before the change, points.
#[derive(Clone, Debug, PartialEq)]
pub struct GridChange {
    pub mark: Rc<RevisionMark>,
    pub previous: Vec<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowRevisions {
    /// `w:trPr/w:ins`, `w:trPr/w:del`: the row was inserted or deleted.
    pub ins: Option<Rc<RevisionMark>>,
    pub del: Option<Rc<RevisionMark>>,
    pub change: Option<Rc<TableChange>>,
    /// `w:tblPrEx/w:tblPrExChange`.
    pub ex_change: Option<Rc<TableChange>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellMarkKind {
    Insert,
    Delete,
    /// `w:cellMerge`: a vertical merge changed (`w:vMerge`, `w:vMergeOrig`).
    Merge {
        v_merge: Option<VMerge>,
        v_merge_orig: Option<VMerge>,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellRevisions {
    pub structural: Option<(Rc<RevisionMark>, CellMarkKind)>,
    pub change: Option<Rc<TableChange>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RangeKind {
    CommentStart,
    CommentEnd,
    MoveFromStart,
    MoveFromEnd,
    MoveToStart,
    MoveToEnd,
}

/// Where a comment's or move's range starts or ends: no units.
#[derive(Clone, Debug, PartialEq)]
pub struct RangeMark {
    pub kind: RangeKind,
    /// The comment's id, or the move range's `w:id`.
    pub id: i64,
    /// A move range's `w:name`.
    pub name: Option<Rc<str>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommentId(pub i64);

#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub id: CommentId,
    pub author: AuthorId,
    pub initials: Option<Rc<str>>,
    pub date: Option<Timestamp>,
    /// `w16cex:dateUtc` in the extensible comments part.
    pub date_utc: Option<Timestamp>,
    /// The comment's text: paragraphs and runs, formatted.
    pub body: Vec<Block>,
    /// `w14:paraId` of the body's last paragraph: what threads and the done flag refer to.
    pub para_id: u32,
    /// `w16cid:durableId`.
    pub durable_id: Option<u32>,
    /// The comment this one replies to (`w15:paraIdParent`).
    pub parent: Option<CommentId>,
    /// `w15:done`: the thread is resolved (kept on the first comment).
    pub done: bool,
    /// The comment's `w16cex:commentExtensible` element as written.
    pub extensible: Raw,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comments {
    pub by_id: BTreeMap<CommentId, Comment>,
}

impl Comments {
    /// Each thread's first comment, then its replies, by date then id.
    pub fn threads(&self) -> Vec<(CommentId, Vec<CommentId>)> {
        let mut replies: HashMap<CommentId, Vec<&Comment>> = HashMap::new();
        for c in self.by_id.values() {
            if let Some(p) = c.parent.filter(|p| self.by_id.contains_key(p)) {
                replies.entry(self.root(p)).or_default().push(c);
            }
        }
        let roots = self.by_id.values().filter(|c| c.parent.is_none_or(|p| !self.by_id.contains_key(&p)));
        roots
            .map(|r| {
                let mut rs = replies.remove(&r.id).unwrap_or_default();
                rs.sort_by(|a, b| a.date.as_ref().and_then(|d| d.utc).cmp(&b.date.as_ref().and_then(|d| d.utc)).then(a.id.cmp(&b.id)));
                (r.id, rs.iter().map(|c| c.id).collect())
            })
            .collect()
    }

    /// The first comment of the thread holding `id` (cycles stop at the first repeat).
    pub fn root(&self, id: CommentId) -> CommentId {
        let (mut at, mut seen) = (id, Vec::new());
        while let Some(p) = self.by_id.get(&at).and_then(|c| c.parent).filter(|p| self.by_id.contains_key(p)) {
            if seen.contains(&p) {
                break;
            }
            seen.push(at);
            at = p;
        }
        at
    }
}

/// What `w:documentProtection` allows (`w:edit`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProtectionKind {
    #[default]
    None,
    ReadOnly,
    Comments,
    /// Every edit is tracked, and tracking can't be turned off.
    TrackedChanges,
    Forms,
}

/// `w:documentProtection` in the settings part: what it allows, whether it is enforced, and the element as written
/// (with its password hash) for saving.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Protection {
    pub edit: ProtectionKind,
    pub enforced: bool,
    pub raw: Raw,
}

impl Protection {
    /// Tracking is on and stays on.
    pub fn forces_tracking(&self) -> bool {
        self.enforced && self.edit == ProtectionKind::TrackedChanges
    }
}

/// The markup view: how revisions show. A view setting, never saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MarkupView {
    /// Insertions and deletions shown in place, in their author's colour, with change bars.
    #[default]
    All,
    /// The changes made, with change bars.
    Simple,
    /// The changes made, with no markup.
    NoMarkup,
    /// The document before the changes.
    Original,
}

impl MarkupView {
    /// Whether the view hides content inserted (`ins`) or deleted (`del`), moves included (REV-1).
    fn hides_change(self, ins: bool, del: bool) -> bool {
        match self {
            MarkupView::All => false,
            MarkupView::Simple | MarkupView::NoMarkup => del,
            MarkupView::Original => ins,
        }
    }

    /// Whether the view hides a run with these revisions (REV-1). Hidden text keeps its units.
    pub fn hides(self, rev: &RunRevisions) -> bool {
        self.hides_change(rev.ins.is_some(), rev.del.is_some())
    }

    /// Whether the view hides an inserted or deleted table row (REV-1).
    pub fn hides_row(self, rev: &RowRevisions) -> bool {
        self.hides_change(rev.ins.is_some(), rev.del.is_some())
    }

    /// Whether the view hides an inserted or deleted table cell (REV-1).
    pub fn hides_cell(self, rev: &CellRevisions) -> bool {
        match &rev.structural {
            Some((_, CellMarkKind::Insert)) => self.hides_change(true, false),
            Some((_, CellMarkKind::Delete)) => self.hides_change(false, true),
            _ => false,
        }
    }

    /// Whether lines with revisions get change bars (REV-4).
    pub fn change_bars(self) -> bool {
        matches!(self, MarkupView::All | MarkupView::Simple)
    }
}

/// A run's formatting in `view`: the formatting before its tracked change in Original (REV-2).
pub fn run_in(run: &Run, view: MarkupView) -> &Run {
    match (&run.rev.fmt, view) {
        (Some(f), MarkupView::Original) => &f.previous,
        _ => run,
    }
}

impl Paragraph {
    /// The paragraph's properties in `view`: those before their tracked change in Original (REV-2).
    pub fn para_style_in(&self, view: MarkupView) -> &Rc<ParaStyle> {
        match (&self.style.change, view) {
            (Some(c), MarkupView::Original) => &c.previous,
            _ => &self.style,
        }
    }

    /// The paragraph mark's formatting in `view` (REV-2).
    pub fn mark_in(&self, view: MarkupView) -> &Run {
        run_in(&self.mark, view)
    }
}

impl Section {
    /// The section's page in `view`: the page before its tracked change in Original (REV-2).
    pub fn section_in(&self, view: MarkupView) -> SectionPage {
        match (&self.change, view) {
            (Some(c), MarkupView::Original) => c.previous.clone(),
            _ => self.page(),
        }
    }
}

impl Cell {
    /// The cell's vertical merge in `view`: the merge before its tracked change in Original (REV-2).
    pub fn v_merge_in(&self, view: MarkupView) -> VMerge {
        match (&self.rev.structural, view) {
            (Some((_, CellMarkKind::Merge { v_merge_orig, .. })), MarkupView::Original) => v_merge_orig.unwrap_or(VMerge::None),
            _ => self.v_merge,
        }
    }
}

impl Table {
    /// The column grid in `view`: the grid before its tracked change in Original (REV-2).
    pub fn table_grid_in(&self, view: MarkupView) -> &[f64] {
        match (&self.grid_change, view) {
            (Some(c), MarkupView::Original) => &c.previous,
            _ => &self.grid,
        }
    }
}

/// Author colours: the default palette by author, unless the host chose one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuthorPalette {
    pub chosen: HashMap<AuthorId, u32>,
}

impl AuthorPalette {
    /// Colours for authors in order of first appearance (0xRRGGBB).
    pub const DEFAULT: [u32; 12] =
        [0xD13438, 0x0078D4, 0x5C2E91, 0x498205, 0xCC3595, 0x7160E8, 0x038387, 0x6D5700, 0xCF0F1F, 0x4E6AED, 0xB146C2, 0x394146];
    /// Moves, whoever made them.
    pub const MOVE: u32 = 0x00A050;

    pub fn color(&self, author: AuthorId) -> u32 {
        self.chosen.get(&author).copied().unwrap_or(Self::DEFAULT[author.0 as usize % Self::DEFAULT.len()])
    }
}

/// How the document shows its review: the markup view and the author colours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewView {
    pub view: MarkupView,
    pub palette: Rc<AuthorPalette>,
}

/// New revision, comment, paragraph and durable ids for this document, past every id it was read with.
#[derive(Clone, Debug, PartialEq)]
pub struct IdCounter {
    next_revision: i64,
    next_para: u32,
}

impl Default for IdCounter {
    fn default() -> IdCounter {
        IdCounter { next_revision: 1, next_para: 1 }
    }
}

impl IdCounter {
    /// Keeps new ids past a `w:id` read from the document.
    pub fn saw_revision(&mut self, id: i64) {
        self.next_revision = self.next_revision.max(id.saturating_add(1));
    }

    /// Keeps new paragraph and durable ids past one read (`w14:paraId`, `w16cid:durableId`).
    pub fn saw_para(&mut self, id: u32) {
        self.next_para = self.next_para.max((id & 0x7FFF_FFFF).saturating_add(1));
    }

    /// A revision or comment id: one sequence for both, as `w:id`s share one space in a document.
    pub fn next_revision_id(&mut self) -> i64 {
        let id = self.next_revision;
        self.next_revision += 1;
        id
    }

    /// A `w14:paraId` or `w16cid:durableId`: unique in the document and below 0x80000000.
    pub fn next_para_id(&mut self) -> u32 {
        let id = self.next_para;
        self.next_para = if id >= 0x7FFF_FFFE { 1 } else { id + 1 };
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_and_write() {
        let t = Timestamp::parse("2026-10-27T10:30:15Z");
        assert_eq!(t.utc, Some(1_793_097_015));
        assert_eq!(Timestamp::from_utc(1_793_097_015).raw.as_ref(), "2026-10-27T10:30:15Z");
        assert_eq!(Timestamp::parse("2026-10-27T12:30:15+02:00").utc, t.utc);
        assert_eq!(Timestamp::parse("2026-10-27T10:30Z").utc, Some(1_793_097_000));
        assert_eq!(Timestamp::parse("1970-01-01").utc, Some(0));
        let bad = Timestamp::parse("yesterday");
        assert_eq!((bad.utc, bad.raw.as_ref()), (None, "yesterday"));
    }

    #[test]
    fn authors_are_told_apart_by_identity_then_name() {
        let mut p = People::default();
        let a = p.author("Author A", None);
        assert_eq!(p.author("Author A", None), a);
        let id = Identity { provider: "AD".into(), user_id: "a@x".into() };
        assert_eq!(p.author("Author A", Some(&id)), a);
        assert_eq!(p.get(a).and_then(|x| x.identity.clone()), Some(id.clone()));
        assert_eq!(p.author("Renamed", Some(&id)), a);
        let other = Identity { provider: "AD".into(), user_id: "b@x".into() };
        assert_ne!(p.author("Author A", Some(&other)), a);
    }

    #[test]
    fn ids_stay_past_those_read() {
        let mut ids = IdCounter::default();
        ids.saw_revision(41);
        ids.saw_para(0x8000_0010);
        assert_eq!(ids.next_revision_id(), 42);
        assert_eq!(ids.next_revision_id(), 43);
        assert_eq!(ids.next_para_id(), 0x11);
        assert!(ids.next_para_id() < 0x8000_0000);
    }

    #[test]
    fn threads_group_replies_under_their_root() {
        let c = |id: i64, parent: Option<i64>, date: &str| Comment {
            id: CommentId(id),
            author: AuthorId(0),
            initials: None,
            date: Some(Timestamp::parse(date)),
            date_utc: None,
            body: Vec::new(),
            para_id: id as u32,
            durable_id: None,
            parent: parent.map(CommentId),
            done: false,
            extensible: Raw::default(),
        };
        let mut cs = Comments::default();
        for x in [
            c(1, None, "2026-01-01T00:00:00Z"),
            c(3, Some(1), "2026-01-03T00:00:00Z"),
            c(2, Some(3), "2026-01-02T00:00:00Z"),
            c(4, None, "2026-01-01T00:00:00Z"),
        ] {
            cs.by_id.insert(x.id, x);
        }
        assert_eq!(cs.threads(), vec![(CommentId(1), vec![CommentId(2), CommentId(3)]), (CommentId(4), vec![])]);
    }
}
