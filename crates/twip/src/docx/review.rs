//! Tracked revisions, comments and people in the document model. Revisions sit on what they change: a run's insert,
//! delete and format-change slots (the three a run can carry), a paragraph mark's run, and the property changes of
//! paragraphs, sections, tables, rows and cells, each with the properties before the change. Comment and move ranges
//! are zero-unit inline markers; comments and their threads live in [`Comments`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use super::model::{Block, Cell, PageSetup, ParaStyle, Paragraph, Raw, Row, Run, RunStyle, Section, SectionPage, Table, VMerge};
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

/// Who a person is in an identity provider (`w15:presenceInfo`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

    /// The authors a comment or a revision in `doc` names.
    pub fn named(doc: &super::model::Document) -> std::collections::HashSet<AuthorId> {
        fn run(r: &RunStyle, out: &mut std::collections::HashSet<AuthorId>) {
            out.extend([&r.rev.ins, &r.rev.del].into_iter().flatten().map(|m| m.author));
            out.extend(r.rev.fmt.as_ref().map(|f| f.mark.author));
        }
        fn blocks(bs: &[Block], out: &mut std::collections::HashSet<AuthorId>) {
            for b in bs {
                match b {
                    Block::Para(p) => {
                        run(&p.mark, out);
                        out.extend(p.style.change.as_ref().map(|c| c.mark.author));
                        for i in &p.inlines {
                            if let Some(r) = i.run() {
                                run(r, out);
                            }
                            if let Some((_, tb)) = i.story() {
                                blocks(&tb.blocks, out);
                            }
                        }
                    }
                    Block::Table(t) => {
                        out.extend(t.change.as_ref().map(|c| c.mark.author));
                        out.extend(t.grid_change.as_ref().map(|c| c.mark.author));
                        for r in &t.rows {
                            out.extend([&r.rev.ins, &r.rev.del].into_iter().flatten().map(|m| m.author));
                            out.extend([&r.rev.change, &r.rev.ex_change].into_iter().flatten().map(|c| c.mark.author));
                            for c in &r.cells {
                                out.extend(c.rev.structural.as_ref().map(|(m, _)| m.author));
                                out.extend(c.rev.change.as_ref().map(|ch| ch.mark.author));
                                blocks(&c.blocks, out);
                            }
                        }
                    }
                }
            }
        }
        let mut out: std::collections::HashSet<AuthorId> = doc.comments.by_id.values().map(|c| c.author).collect();
        for s in &doc.sections {
            out.extend(s.change.as_ref().map(|c| c.mark.author));
            blocks(&s.blocks, &mut out);
        }
        let others = doc.stories.values().chain(doc.footnotes.values()).chain(doc.endnotes.values()).chain(doc.glossary_bodies());
        for b in others.chain(doc.comments.by_id.values().map(|c| &c.body)) {
            blocks(b, &mut out);
        }
        out
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
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date (year, month, day) `z` days after 1970-01-01 (Howard Hinnant's algorithm).
pub(crate) fn civil_from_days(z: i64) -> (i64, i64, i64) {
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
    /// The formatting before, its authored properties ([`RunStyle::direct`]) the earlier `w:rPr`.
    pub previous: Rc<RunStyle>,
}

/// `w:pPrChange`: the paragraph's properties before the change.
#[derive(Clone, Debug, PartialEq)]
pub struct ParaChange {
    pub mark: Rc<RevisionMark>,
    /// The formatting before the change; its authored properties are those the change records.
    pub previous: Rc<ParaStyle>,
    /// It records earlier properties (a `w:pPrChange`), not only a list's numbering change.
    pub properties: bool,
    /// `w:numberingChange` in the paragraph's `w:numPr`, as written.
    pub numbering_change: Raw,
}

impl ParaChange {
    /// Whether it is a list's numbering change alone, without a `w:pPrChange`.
    pub fn numbering_only(&self) -> bool {
        !self.properties && self.numbering_change.0.is_some()
    }
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
    /// The earlier authored properties, for saving.
    pub previous_direct: super::table_props::PreviousDirect,
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
    Row { height: Option<(f64, super::HeightRule)>, cant_split: bool, header: bool },
    /// `w:tcPrChange`: borders and margins top, left, bottom, right.
    Cell {
        width: Option<Width>,
        borders: [Option<Border>; 4],
        shading: Option<u32>,
        margins: [f64; 4],
        v_align: super::VerticalAlignment,
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
            (r.height, r.cant_split, r.header) = (*height, *cant_split, *header);
        }
    }

    /// Gives `c` these properties, when they are a cell's.
    pub fn restore_cell(&self, c: &mut Cell) {
        if let PreviousProps::Cell { width, borders, shading, margins, v_align, text_direction, no_wrap } = self {
            let [top, left, bottom, right] = borders.clone();
            (c.width, c.top, c.left, c.bottom, c.right, c.shading, c.margins) = (*width, top, left, bottom, right, *shading, *margins);
            (c.v_align, c.text_direction, c.no_wrap) = (*v_align, text_direction.clone(), *no_wrap);
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
    /// `w:bookmarkStart`.
    BookmarkStart,
    /// `w:bookmarkEnd`.
    BookmarkEnd,
    /// `w:permStart`: an editing permission's start.
    PermissionStart,
    /// `w:permEnd`.
    PermissionEnd,
}

/// Where a comment's, move's, bookmark's or editing permission's range starts or ends: no units.
#[derive(Clone, Debug, PartialEq)]
pub struct RangeMark {
    pub kind: RangeKind,
    /// The comment's id, or the move range's or editing permission's `w:id`; 0 for a bookmark, which is known by its
    /// name (the writer numbers bookmarks).
    pub id: i64,
    /// A move range's `w:name`, a bookmark's name.
    pub name: Option<Rc<str>>,
    /// An editing permission marker's settings: every permission marker has them, a marker read or made once (its
    /// copies share them).
    pub permission: Option<Rc<Permission>>,
}

impl RangeMark {
    /// A bookmark's start.
    pub fn bookmark_start(name: impl Into<Rc<str>>) -> RangeMark {
        RangeMark { kind: RangeKind::BookmarkStart, id: 0, name: Some(name.into()), permission: None }
    }

    /// A bookmark's end.
    pub fn bookmark_end(name: impl Into<Rc<str>>) -> RangeMark {
        RangeMark { kind: RangeKind::BookmarkEnd, id: 0, name: Some(name.into()), permission: None }
    }

    /// A bookmark marker's name, and whether it starts the bookmark.
    pub fn bookmark(&self) -> Option<(&str, bool)> {
        let start = match self.kind {
            RangeKind::BookmarkStart => true,
            RangeKind::BookmarkEnd => false,
            _ => return None,
        };
        Some((self.name.as_deref()?, start))
    }

    /// An editing permission marker: whether it starts the permission's range, and the permission's id.
    pub fn permission(&self) -> Option<(bool, i64)> {
        match self.kind {
            RangeKind::PermissionStart => Some((true, self.id)),
            RangeKind::PermissionEnd => Some((false, self.id)),
            _ => None,
        }
    }

    /// A comment's or move's range marker (not a bookmark's or permission's).
    pub fn is_review(&self) -> bool {
        !matches!(self.kind, RangeKind::BookmarkStart | RangeKind::BookmarkEnd | RangeKind::PermissionStart | RangeKind::PermissionEnd)
    }
}

/// An editing permission marker's settings (`w:permStart`, `w:permEnd`).
#[derive(Clone, Debug, PartialEq)]
pub struct Permission {
    /// Who may edit the range: a start's `w:edGrp`, else its `w:ed`.
    pub editor: Option<RangeEditor>,
    /// `w:colFirst`, `w:colLast`: the first and last table column a start's range covers.
    pub columns: (Option<u32>, Option<u32>),
    /// Where it was read from (its element as read; none for a marker made since): what is written, with the marker's
    /// id, for one read. A permission whose settings change is a new one, without it.
    pub origin: super::model::Origin,
}

/// Who an editable range lets edit it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RangeEditor {
    /// A group (`w:edGrp`): `everyone`, or `administrators`, `contributors`, `editors`, `owners`, `current` and the
    /// like, which the host says the author is in.
    Group(String),
    /// One person (`w:ed`): a name or email address.
    User(String),
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
        let roots = self.roots();
        let mut replies: HashMap<CommentId, Vec<&Comment>> = HashMap::new();
        for c in self.by_id.values() {
            if roots[&c.id] != c.id {
                replies.entry(roots[&c.id]).or_default().push(c);
            }
        }
        self.by_id
            .values()
            .filter(|c| roots[&c.id] == c.id)
            .map(|r| {
                let mut rs = replies.remove(&r.id).unwrap_or_default();
                rs.sort_by(|a, b| a.date.as_ref().and_then(|d| d.utc).cmp(&b.date.as_ref().and_then(|d| d.utc)).then(a.id.cmp(&b.id)));
                (r.id, rs.iter().map(|c| c.id).collect())
            })
            .collect()
    }

    /// How many threads there are, as [`Self::threads`] has them, without building them.
    pub fn thread_count(&self) -> usize {
        self.roots().iter().filter(|(c, r)| c == r).count()
    }

    /// The first comment of the thread holding `id`.
    pub fn root(&self, id: CommentId) -> CommentId {
        self.walk_up(id, &HashMap::new(), &mut Vec::new(), &mut HashSet::new())
    }

    /// Each comment's thread's first comment, in one pass over the reply links: a cycle's first comment is its
    /// lowest id.
    pub fn roots(&self) -> HashMap<CommentId, CommentId> {
        let mut roots = HashMap::with_capacity(self.by_id.len());
        let (mut path, mut on_path) = (Vec::new(), HashSet::new());
        for &id in self.by_id.keys() {
            if roots.contains_key(&id) {
                continue;
            }
            let root = self.walk_up(id, &roots, &mut path, &mut on_path);
            roots.extend(path.drain(..).map(|c| (c, root)));
            on_path.clear();
        }
        roots
    }

    /// Follows reply links from `id` to a comment without a parent, one in `known`, or around a cycle; `path` holds
    /// the comments passed.
    fn walk_up(
        &self,
        id: CommentId,
        known: &HashMap<CommentId, CommentId>,
        path: &mut Vec<CommentId>,
        on_path: &mut HashSet<CommentId>,
    ) -> CommentId {
        let mut at = id;
        loop {
            if let Some(&r) = known.get(&at) {
                return r;
            }
            if !on_path.insert(at) {
                let from = path.iter().position(|c| *c == at).unwrap_or(0);
                return path[from..].iter().copied().min().unwrap_or(at);
            }
            path.push(at);
            match self.by_id.get(&at).and_then(|c| c.parent).filter(|p| self.by_id.contains_key(p)) {
                Some(p) => at = p,
                None => return at,
            }
        }
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
    /// `w:formatting`: formatting is limited to the styles that aren't locked.
    pub formatting: bool,
    /// The password's hash, salt, spin count and algorithm (`w:cryptAlgorithmSid` or `w:algorithmName`).
    pub password: Option<Password>,
    pub raw: Raw,
}

/// A protection password as stored: never the password itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Password {
    /// `SHA-512`, `SHA-384`, `SHA-256`, or what the document names.
    pub algorithm: String,
    /// Base64.
    pub hash: String,
    pub salt: String,
    pub spin: u32,
}

impl Password {
    /// The stored form of `password`, as Word stores it: Word's legacy key for it, salted, hashed with SHA-512 and spun
    /// 100 000 times.
    pub fn new(password: &str) -> Password {
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).expect("the operating system provides random bytes");
        let spin = 100_000;
        let key = legacy_key(password, true);
        let hash = hash("SHA-512", &salt, &key, spin).unwrap_or_default();
        Password { algorithm: "SHA-512".into(), hash: crate::base64::base64(&hash), salt: crate::base64::base64(&salt), spin }
    }

    /// `password` is this one: hashed from Word's legacy key for it (its hex digits padded or not) or, as ECMA-376
    /// describes, from the password itself. An algorithm this doesn't compute never matches.
    pub fn matches(&self, password: &str) -> bool {
        let Some(salt) = crate::base64::from_base64(&self.salt) else { return false };
        // shortcut: the spin count is capped, so a document can't make a check take minutes; Word writes 100 000.
        let spin = self.spin.min(10_000_000);
        let tries = [legacy_key(password, true), legacy_key(password, false), password.to_string()];
        let mut seen: Vec<&str> = Vec::with_capacity(3);
        tries.iter().any(|t| {
            if seen.contains(&t.as_str()) {
                return false;
            }
            seen.push(t);
            hash(&self.algorithm, &salt, t, spin).is_some_and(|h| crate::base64::base64(&h) == self.hash)
        })
    }
}

/// The algorithm name for `w:cryptAlgorithmSid`'s number.
pub fn algorithm_name(sid: &str) -> &str {
    match sid {
        "4" => "SHA-1",
        "12" => "SHA-256",
        "13" => "SHA-384",
        "14" => "SHA-512",
        s => s,
    }
}

/// Word's legacy 32-bit password key (ECMA-376 1st edition, 17.15.1.29), its bytes reversed, as upper-case hex
/// (`pad`: two digits a byte). Empty for an empty password.
fn legacy_key(password: &str, pad: bool) -> String {
    const INITIAL: [u16; 15] =
        [0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9, 0x280C, 0xA96A, 0x4EC3];
    const MATRIX: [[u16; 7]; 15] = [
        [0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09],
        [0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF],
        [0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0],
        [0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40],
        [0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5],
        [0x6F45, 0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A],
        [0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9],
        [0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0],
        [0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC],
        [0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10],
        [0xAA51, 0x4483, 0x8906, 0x022D, 0x045A, 0x08B4, 0x1168],
        [0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C],
        [0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD],
        [0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC],
        [0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4],
    ];
    // Each character's low byte, or its high byte when that is zero; at most 15.
    let bytes: Vec<u8> = password
        .encode_utf16()
        .take(15)
        .map(|c| {
            let [lo, hi] = c.to_le_bytes();
            if lo == 0 { hi } else { lo }
        })
        .collect();
    if bytes.is_empty() {
        return String::new();
    }
    let n = bytes.len();
    let mut high = INITIAL[n - 1];
    for (i, b) in bytes.iter().enumerate() {
        for (bit, m) in MATRIX[15 - n + i].iter().enumerate() {
            if b & (1 << bit) != 0 {
                high ^= m;
            }
        }
    }
    let rotate = |v: u16| ((v >> 14) & 1) | ((v << 1) & 0x7FFF);
    let mut low = 0u16;
    for b in bytes.iter().rev() {
        low = rotate(low) ^ u16::from(*b);
    }
    low = rotate(low) ^ n as u16 ^ 0xCE4B;
    let key = (u32::from(high) << 16) | u32::from(low);
    key.to_le_bytes().iter().map(|b| if pad { format!("{b:02X}") } else { format!("{b:X}") }).collect()
}

/// H(salt + password as UTF-16LE), then `spin` rounds of H(previous + round number as 32-bit little endian).
fn hash(algorithm: &str, salt: &[u8], password: &str, spin: u32) -> Option<Vec<u8>> {
    use sha2::Digest;
    // One digest buffer for every round.
    fn run<D: Digest + sha2::digest::FixedOutputReset>(salt: &[u8], pw: &[u8], spin: u32) -> Vec<u8> {
        let mut d = D::new();
        Digest::update(&mut d, salt);
        Digest::update(&mut d, pw);
        let mut h = d.finalize_reset();
        for i in 0..spin {
            Digest::update(&mut d, &h);
            Digest::update(&mut d, i.to_le_bytes());
            Digest::finalize_into_reset(&mut d, &mut h);
        }
        h.to_vec()
    }
    let pw: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
    Some(match algorithm_name(&algorithm.to_ascii_uppercase()) {
        "SHA-512" => run::<sha2::Sha512>(salt, &pw, spin),
        "SHA-384" => run::<sha2::Sha384>(salt, &pw, spin),
        "SHA-256" => run::<sha2::Sha256>(salt, &pw, spin),
        "SHA-1" => run::<sha1::Sha1>(salt, &pw, spin),
        _ => return None,
    })
}

impl Protection {
    /// The `w:documentProtection` element (`w`: the prefix with its colon).
    pub fn xml(&self, w: &str) -> String {
        let edit = match self.edit {
            ProtectionKind::None => "none",
            ProtectionKind::ReadOnly => "readOnly",
            ProtectionKind::Comments => "comments",
            ProtectionKind::TrackedChanges => "trackedChanges",
            ProtectionKind::Forms => "forms",
        };
        let flag = |b: bool| if b { "1" } else { "0" };
        let pw = self.password.as_ref().map_or(String::new(), |p| {
            let esc = crate::docx::write::props::esc;
            format!(
                " {w}algorithmName=\"{}\" {w}hashValue=\"{}\" {w}saltValue=\"{}\" {w}spinCount=\"{}\"",
                esc(&p.algorithm),
                esc(&p.hash),
                esc(&p.salt),
                p.spin
            )
        });
        let formatting = if self.formatting { format!(" {w}formatting=\"1\"") } else { String::new() };
        format!("<{w}documentProtection {w}edit=\"{edit}\"{formatting} {w}enforcement=\"{}\"{pw}/>", flag(self.enforced))
    }

    /// The same settings, the element as read aside.
    pub fn same(&self, o: &Protection) -> bool {
        (self.edit, self.enforced, self.formatting, &self.password) == (o.edit, o.enforced, o.formatting, &o.password)
    }

    /// Formatting is limited to the unlocked styles, and direct formatting is refused.
    pub fn limits_formatting(&self) -> bool {
        self.enforced && self.formatting
    }

    /// Tracking is on and stays on.
    pub fn forces_tracking(&self) -> bool {
        self.enforced && self.edit == ProtectionKind::TrackedChanges
    }
}

/// The markup view: how revisions show. A view setting, never saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    fn hides_change(self, ins: bool, del: bool) -> bool {
        match self {
            MarkupView::All => false,
            MarkupView::Simple | MarkupView::NoMarkup => del,
            MarkupView::Original => ins,
        }
    }

    /// Whether the view alone, every kind and reviewer shown, hides a run's text.
    pub fn hides(self, rev: &RunRevisions) -> bool {
        self.hides_change(rev.ins.is_some(), rev.del.is_some())
    }

    /// Whether the view draws change bars.
    pub fn has_change_bars(self) -> bool {
        matches!(self, MarkupView::All | MarkupView::Simple)
    }
}

/// Which markup shows: Word's Show Markup kinds and reviewers. A revision or comment it leaves out still exists and
/// is still accepted, rejected or deleted, but shows as if accepted: its text unmarked, a deletion's text gone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShowMarkup {
    /// Insertions and deletions: of text, moves, rows and cells.
    pub insertions_deletions: bool,
    /// Formatting changes: of text, paragraphs, sections, tables, rows and cells, and cell merges.
    pub formatting: bool,
    /// Comments.
    pub comments: bool,
    /// The reviewers whose revisions and comments do not show, in id order.
    pub hidden_reviewers: Vec<AuthorId>,
}

impl Default for ShowMarkup {
    fn default() -> ShowMarkup {
        ShowMarkup { insertions_deletions: true, formatting: true, comments: true, hidden_reviewers: Vec::new() }
    }
}

impl ShowMarkup {
    /// Whether `author`'s markup shows.
    pub fn shows_reviewer(&self, author: AuthorId) -> bool {
        self.hidden_reviewers.binary_search(&author).is_err()
    }

    /// Whether a revision's markup shows: its kind and its reviewer.
    pub fn shows(&self, m: &RevisionMark) -> bool {
        self.shows_change(m.kind == MarkKind::Format, m.author)
    }

    /// Whether a change by `author` shows: a formatting change, or an insertion or deletion.
    pub fn shows_change(&self, formatting: bool, author: AuthorId) -> bool {
        (if formatting { self.formatting } else { self.insertions_deletions }) && self.shows_reviewer(author)
    }

    /// Whether a comment by `author` shows.
    pub fn shows_comment(&self, author: AuthorId) -> bool {
        self.comments && self.shows_reviewer(author)
    }
}

/// Where revisions show in All Markup: Word's Balloons setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BalloonMode {
    /// Deletions and formatting changes in balloons beside the page, insertions in place.
    Balloons,
    /// Every revision in place. Comments stay in their margin cards.
    Inline,
    /// Formatting changes in balloons, insertions and deletions in place.
    #[default]
    CommentsAndFormatting,
}

/// How a revision marks its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextMark {
    /// Not marked: neither style nor colour.
    None,
    /// In the mark's colour only.
    ColorOnly,
    /// Bold.
    Bold,
    /// Italic.
    Italic,
    /// Underlined.
    Underline,
    /// Double underlined.
    DoubleUnderline,
    /// Struck through.
    Strikethrough,
    /// Double struck through.
    DoubleStrikethrough,
    /// A deletion's text does not show.
    Hidden,
    /// A deletion's text shows as one `^`.
    Caret,
    /// A deletion's text shows as one `#`.
    Pound,
}

impl TextMark {
    /// Whether a deletion's own text does not lay out.
    pub fn hides_text(self) -> bool {
        matches!(self, TextMark::Hidden | TextMark::Caret | TextMark::Pound)
    }

    /// The character a deletion's text shows as.
    pub fn stand_in(self) -> Option<char> {
        match self {
            TextMark::Caret => Some('^'),
            TextMark::Pound => Some('#'),
            _ => None,
        }
    }
}

/// The colour a mark draws in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkColor {
    /// The revision's author's.
    ByAuthor,
    /// The text's own colour.
    Auto,
    /// This colour (0xRRGGBB).
    Fixed(u32),
}

/// How one kind of revision draws: its mark and the mark's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkLook {
    /// How the text is marked.
    pub mark: TextMark,
    /// The mark's colour.
    pub color: MarkColor,
}

/// Which side of the text change bars go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BarSide {
    /// No change bars.
    None,
    /// Left of the text.
    Left,
    /// Right of the text.
    Right,
    /// The page's outside edge: the right on odd pages with mirrored margins, else the left.
    #[default]
    Outside,
}

/// How revisions draw: Word's Track Changes Options. View state: never saved.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkupStyle {
    /// Inserted text.
    pub insertions: MarkLook,
    /// Deleted text.
    pub deletions: MarkLook,
    /// Text with formatting changes.
    pub formatting: MarkLook,
    /// Text moved away.
    pub moved_from: MarkLook,
    /// Text moved here.
    pub moved_to: MarkLook,
    /// Where change bars go.
    pub change_bars: BarSide,
    /// `None`: automatic ([`ReviewView::change_bar_color`]).
    pub change_bar_color: Option<u32>,
    /// Inserted, deleted, merged and split cells' fill: `ByAuthor` tints the cell with the author's colour, `Auto`
    /// leaves its own fill.
    pub inserted_cells: MarkColor,
    /// Deleted cells' fill.
    pub deleted_cells: MarkColor,
    /// Merged cells' fill.
    pub merged_cells: MarkColor,
    /// Split cells' fill: a cell whose vertical merge was undone.
    pub split_cells: MarkColor,
}

impl Default for MarkupStyle {
    fn default() -> MarkupStyle {
        let look = |mark, color| MarkLook { mark, color };
        MarkupStyle {
            insertions: look(TextMark::Underline, MarkColor::ByAuthor),
            deletions: look(TextMark::Strikethrough, MarkColor::ByAuthor),
            formatting: look(TextMark::None, MarkColor::ByAuthor),
            moved_from: look(TextMark::DoubleStrikethrough, MarkColor::Fixed(AuthorPalette::MOVE)),
            moved_to: look(TextMark::DoubleUnderline, MarkColor::Fixed(AuthorPalette::MOVE)),
            change_bars: BarSide::Outside,
            change_bar_color: None,
            inserted_cells: MarkColor::Fixed(MarkupStyle::LIGHT_BLUE),
            deleted_cells: MarkColor::Fixed(MarkupStyle::PINK),
            merged_cells: MarkColor::Fixed(MarkupStyle::LIGHT_YELLOW),
            split_cells: MarkColor::Fixed(MarkupStyle::LIGHT_ORANGE),
        }
    }
}

impl MarkupStyle {
    /// Word's Light Blue cell highlight, inserted cells' by default.
    pub const LIGHT_BLUE: u32 = 0x99CCFF;
    /// Word's Pink cell highlight, deleted cells' by default.
    pub const PINK: u32 = 0xFF99CC;
    /// Word's Light Yellow cell highlight, merged cells' by default.
    pub const LIGHT_YELLOW: u32 = 0xFFFF99;
    /// Word's Light Orange cell highlight, split cells' by default.
    pub const LIGHT_ORANGE: u32 = 0xFFCC99;

    /// How a revision mark of `kind` draws.
    pub fn look(&self, kind: MarkKind) -> MarkLook {
        match kind {
            MarkKind::Insert => self.insertions,
            MarkKind::Delete => self.deletions,
            MarkKind::MoveFrom => self.moved_from,
            MarkKind::MoveTo => self.moved_to,
            MarkKind::Format => self.formatting,
        }
    }
}

/// The parts of the review view that change how revisions lay out and draw beyond the markup view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Markup {
    /// Which markup shows.
    pub show: ShowMarkup,
    /// Where revisions show.
    pub balloons: BalloonMode,
    /// How revisions draw.
    pub style: MarkupStyle,
}

impl Markup {
    /// Whether a deletion's text leaves the line in `view`: hidden by the view, not shown (shows as accepted), in a
    /// balloon, or drawn hidden or as a stand-in.
    fn hides_deletion(&self, view: MarkupView, m: &RevisionMark) -> bool {
        view == MarkupView::All && (!self.show.shows(m) || self.balloons == BalloonMode::Balloons || self.style.look(m.kind).mark.hides_text())
    }

    /// Whether a run with `rev` leaves the line in `view`.
    pub fn hides(&self, view: MarkupView, rev: &RunRevisions) -> bool {
        view.hides_change(rev.ins.is_some(), rev.del.is_some()) || rev.del.as_ref().is_some_and(|d| self.hides_deletion(view, d))
    }

    /// Whether a deletion's text shows as a stand-in character (`^`, `#`) in `view`.
    pub fn stand_in(&self, view: MarkupView, rev: &RunRevisions) -> Option<char> {
        let d = rev.del.as_ref().filter(|_| view == MarkupView::All && self.balloons != BalloonMode::Balloons)?;
        self.style.look(d.kind).mark.stand_in().filter(|_| self.show.shows(d))
    }

    /// Rows and cells stay in place in balloon mode and with any deletion mark: only filtered out, they go.
    pub fn hides_row(&self, view: MarkupView, rev: &RowRevisions) -> bool {
        view.hides_change(rev.ins.is_some(), rev.del.is_some()) || rev.del.as_ref().is_some_and(|d| view == MarkupView::All && !self.show.shows(d))
    }

    /// Whether a cell with `rev` leaves the table in `view`.
    pub fn hides_cell(&self, view: MarkupView, rev: &CellRevisions) -> bool {
        match &rev.structural {
            Some((_, CellMarkKind::Insert)) => view.hides_change(true, false),
            Some((m, CellMarkKind::Delete)) => view.hides_change(false, true) || view == MarkupView::All && !self.show.shows(m),
            _ => false,
        }
    }
}

pub fn run_in(run: &Run, view: MarkupView) -> &Run {
    match (&run.rev.fmt, view) {
        (Some(f), MarkupView::Original) => &f.previous,
        _ => run,
    }
}

impl Paragraph {
    pub fn para_style_in(&self, view: MarkupView) -> &Rc<ParaStyle> {
        match (&self.style.change, view) {
            (Some(c), MarkupView::Original) => &c.previous,
            _ => self.div.as_ref().map_or(&self.style, |d| d.over(&self.style)),
        }
    }

    pub fn mark_in(&self, view: MarkupView) -> &Run {
        run_in(&self.mark, view)
    }
}

impl Section {
    pub fn section_in(&self, view: MarkupView) -> PageSetup<'_> {
        match (&self.change, view) {
            (Some(c), MarkupView::Original) => PageSetup::from(&c.previous),
            _ => PageSetup::from(self),
        }
    }
}

impl Cell {
    pub fn v_merge_in(&self, view: MarkupView) -> VMerge {
        match (&self.rev.structural, view) {
            (Some((_, CellMarkKind::Merge { v_merge_orig, .. })), MarkupView::Original) => v_merge_orig.unwrap_or(VMerge::None),
            _ => self.v_merge,
        }
    }
}

impl Table {
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

/// How the document shows its review: the markup view, the author colours, which markup shows and how it draws; and
/// whether hidden text and optional hyphens show (laid out, as Word's display options show them). View state: never
/// saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewView {
    pub view: MarkupView,
    pub palette: Rc<AuthorPalette>,
    /// Which markup shows, where revisions go and how they draw.
    pub markup: Rc<Markup>,
    pub show_hidden: bool,
    /// Optional hyphens show as `¬` (laid out with their width).
    pub show_optional_hyphens: bool,
}

impl ReviewView {
    /// Whether a run with `rev` leaves the line.
    pub fn hides(&self, rev: &RunRevisions) -> bool {
        self.markup.hides(self.view, rev)
    }

    /// Whether a row with `rev` leaves the table.
    pub fn hides_row(&self, rev: &RowRevisions) -> bool {
        self.markup.hides_row(self.view, rev)
    }

    /// Whether a cell with `rev` leaves the table.
    pub fn hides_cell(&self, rev: &CellRevisions) -> bool {
        self.markup.hides_cell(self.view, rev)
    }

    /// Whether a revision draws its marks: All Markup, its kind and reviewer shown.
    pub fn marks(&self, m: &RevisionMark) -> bool {
        self.view == MarkupView::All && self.markup.show.shows(m)
    }

    /// Whether the view draws change bars at all.
    pub fn change_bars(&self) -> bool {
        self.view.has_change_bars() && self.markup.style.change_bars != BarSide::None
    }

    /// The change bars' colour (0xRRGGBB): the style's, or automatic: red in Simple Markup, grey in All Markup (as
    /// Word draws them).
    pub fn change_bar_color(&self) -> u32 {
        let auto = if self.view == MarkupView::Simple { Self::SIMPLE_BAR } else { Self::MARKUP_BAR };
        self.markup.style.change_bar_color.unwrap_or(auto)
    }

    /// Simple Markup's automatic change bar colour.
    pub const SIMPLE_BAR: u32 = 0xFF0000;
    /// All Markup's automatic change bar colour.
    pub const MARKUP_BAR: u32 = 0x808080;

    /// Whether a revision puts a change bar beside its lines.
    pub fn bars(&self, m: &RevisionMark) -> bool {
        self.change_bars() && self.markup.show.shows(m)
    }

    /// The colour a revision's mark draws in, `None` for the text's own: its look's, an author's from the palette.
    pub fn mark_color(&self, m: &RevisionMark) -> Option<u32> {
        self.color_of(self.markup.style.look(m.kind).color, m.author)
    }

    /// The colour `c` names for `author`'s revision, `None` for the text's own.
    pub fn color_of(&self, c: MarkColor, author: AuthorId) -> Option<u32> {
        match c {
            MarkColor::ByAuthor => Some(self.palette.color(author)),
            MarkColor::Auto => None,
            MarkColor::Fixed(c) => Some(c),
        }
    }

    /// Whether a comment by `author` shows: comments shown, and its reviewer.
    pub fn shows_comment(&self, author: AuthorId) -> bool {
        self.markup.show.shows_comment(author)
    }
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
        self.next_revision = self.next_revision.saturating_add(1);
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
    fn passwords_hash_from_words_legacy_key() {
        // ECMA-376 1st edition's example: "Example" keys to 0x64CEED7E, written reversed.
        assert_eq!(legacy_key("Example", true), "7EEDCE64");
        assert_eq!(legacy_key("", true), "");
        let p = Password::new("Example");
        assert!(p.matches("Example") && !p.matches("example") && !p.matches(""));
        // A hash of the password itself, and Word 2007's SHA-1 (`w:cryptAlgorithmSid="4"`).
        for algorithm in ["SHA-256", "SHA-1"] {
            let h = hash(algorithm, b"0123456789abcdef", "Example", 10).expect("computed");
            let p =
                Password { algorithm: algorithm.into(), hash: crate::base64::base64(&h), salt: crate::base64::base64(b"0123456789abcdef"), spin: 10 };
            assert!(p.matches("Example") && !p.matches("Other"), "{algorithm}");
        }
        // Word 16.112 protecting a document with "Example" (`w:cryptAlgorithmSid="14"`).
        let word = Password {
            algorithm: "14".into(),
            hash: "aqKd5zF+ZLDhXamFt+PhIOVMMDHd6mvzt30mAzZQNGV9pwjlCwKexsb0sjaSOJD/nw05xlAb+FGU3TNUgKq0oA==".into(),
            salt: "mRL8HCYKqg4tYiEVGBiVZA==".into(),
            spin: 100_000,
        };
        assert!(word.matches("Example") && !word.matches("example"));
        let salt = crate::base64::from_base64(&word.salt).expect("base64");
        let h = hash("SHA-512", &salt, &legacy_key("Example", true), word.spin).expect("computed");
        assert_eq!(crate::base64::base64(&h), word.hash, "Password::new's form");
    }

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
        // Replies in a loop have no root: neither counts as a thread.
        cs.by_id.insert(CommentId(5), c(5, Some(6), "2026-01-01T00:00:00Z"));
        cs.by_id.insert(CommentId(6), c(6, Some(5), "2026-01-01T00:00:00Z"));
        assert_eq!(cs.thread_count(), cs.threads().len());
    }
}
