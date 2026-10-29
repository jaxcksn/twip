//! Revisions, comments and the people in them, where comment cards go, and the session's settings for who edits
//! and how.

use std::collections::HashMap;

use super::Session;
use crate::api::event::Event;
use crate::api::record::{Timestamp, TransactionRecord, comment_id, revision_id};
use crate::api::review::{Comment, CommentAnchor, CommentThread, Person, RevisionInfo, RevisionKind, RichParagraph, RichRun, RichText};
use crate::api::services::ContentPoint;
use crate::api::state::State;
use crate::api::value::{Color, Story};
use crate::docx::review::AuthorId;
use crate::docx::{Block, Document, Inline, RunStyle};
use crate::editing::comments::rich;
use crate::editing::position::StoryId;
use crate::editing::revisions::index::{self, RevisionKind as Kind};

/// A person of the document, with the colour they show in.
fn person(doc: &Document, id: AuthorId) -> Person {
    let p = doc.people.get(id);
    Person {
        name: p.map_or_else(String::new, |p| p.name.to_string()),
        initials: p.and_then(|p| p.initials.as_deref().map(str::to_string)),
        identity: p.and_then(|p| p.identity.clone()),
        color: Color(doc.review.palette.color(id)),
    }
}

/// A story's public kind.
pub(super) fn story(e: &crate::editing::Editor, s: StoryId) -> Story {
    match s {
        StoryId::Body => Story::Body,
        StoryId::HeaderFooter(k) if e.index().part_name(k).is_some_and(|n| n.contains("footer")) => Story::Footer,
        StoryId::HeaderFooter(_) => Story::Header,
        StoryId::Footnote(_) => Story::Footnote,
        StoryId::Endnote(_) => Story::Endnote,
        StoryId::TextBox(_) => Story::TextBox,
    }
}

fn time(t: &Option<crate::docx::review::Timestamp>) -> Option<Timestamp> {
    t.as_ref().and_then(|t| t.utc).map(Timestamp)
}

/// What the runs carrying each revision mark hold: the inserted and deleted text, the formatting changes' names, and
/// the moves' range names, by mark id.
#[derive(Default)]
struct Marks {
    inserted: HashMap<i64, String>,
    deleted: HashMap<i64, String>,
    formatted: HashMap<i64, Vec<&'static str>>,
    moves: HashMap<i64, std::rc::Rc<str>>,
}

/// The formatting `before` and `after` differ in, by name.
fn format_names(before: &RunStyle, after: &RunStyle) -> Vec<&'static str> {
    let mut out = Vec::new();
    let diffs: [(bool, &str); 12] = [
        (before.bold != after.bold, if after.bold { "Bold" } else { "Not Bold" }),
        (before.italic != after.italic, if after.italic { "Italic" } else { "Not Italic" }),
        (before.underline != after.underline, "Underline"),
        (before.strike != after.strike, "Strikethrough"),
        (before.dstrike != after.dstrike, "Double strikethrough"),
        (before.ascii != after.ascii, "Font"),
        (before.size != after.size, "Font size"),
        (before.color != after.color, "Font color"),
        (before.highlight != after.highlight, "Highlight"),
        (before.vert_align != after.vert_align, "Position"),
        (before.caps != after.caps || before.small_caps != after.small_caps, "Caps"),
        (before.hidden != after.hidden, "Hidden"),
    ];
    out.extend(diffs.iter().filter(|d| d.0).map(|d| d.1));
    out
}

impl Marks {
    fn of(doc: &Document) -> Marks {
        let mut m = Marks::default();
        doc.sections.iter().for_each(|s| m.blocks(&s.blocks));
        doc.stories.values().for_each(|b| m.blocks(b));
        doc.footnotes.values().chain(doc.endnotes.values()).for_each(|b| m.blocks(b));
        m
    }

    fn blocks(&mut self, list: &[Block]) {
        for b in list {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        let Some(r) = i.run() else { continue };
                        let text = match i {
                            Inline::Text(s, _) => s.as_str(),
                            Inline::Tab(_) => "\t",
                            Inline::Break(..) => "\n",
                            _ => "",
                        };
                        for (mark, out) in [(&r.rev.ins, &mut self.inserted), (&r.rev.del, &mut self.deleted)] {
                            if let Some(mark) = mark {
                                out.entry(mark.id.0).or_default().push_str(text);
                                if let Some(name) = &mark.move_name {
                                    self.moves.insert(mark.id.0, name.clone());
                                }
                            }
                        }
                        if let Some(f) = &r.rev.fmt {
                            let names = self.formatted.entry(f.mark.id.0).or_default();
                            for n in format_names(&f.previous, r) {
                                if !names.contains(&n) {
                                    names.push(n);
                                }
                            }
                        }
                    }
                    // An inserted or deleted paragraph mark ends a line of the revision's text.
                    for (mark, out) in [(&p.mark.rev.ins, &mut self.inserted), (&p.mark.rev.del, &mut self.deleted)] {
                        if let Some(mark) = mark {
                            out.entry(mark.id.0).or_default().push('\n');
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| self.blocks(&c.blocks)),
            }
        }
    }

    /// The text of the marks `ids` in `of`, trailing paragraph marks left out.
    fn text(of: &HashMap<i64, String>, ids: &[crate::docx::review::RevisionId]) -> String {
        let s: String = ids.iter().filter_map(|id| of.get(&id.0)).map(String::as_str).collect();
        s.trim_end_matches('\n').to_string()
    }
}

/// A comment body as the API gives it: runs with only the formatting they have.
fn rich_text(r: &rich::RichText) -> RichText {
    let on = |b: bool| b.then_some(true);
    RichText {
        paragraphs: r
            .paragraphs
            .iter()
            .map(|p| RichParagraph {
                runs: p
                    .runs
                    .iter()
                    .map(|r| RichRun {
                        text: r.text.clone(),
                        bold: on(r.bold),
                        italic: on(r.italic),
                        underline: on(r.underline),
                        strikethrough: on(r.strike),
                        link: r.link.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The paragraph each comment's range starts in, by comment id.
fn comment_starts(doc: &Document) -> HashMap<i64, crate::layout::ParaId> {
    fn walk(list: &[Block], out: &mut HashMap<i64, crate::layout::ParaId>) {
        for b in list {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        if let Inline::Range(m) = i
                            && m.kind == crate::docx::review::RangeKind::CommentStart
                        {
                            out.entry(m.id).or_insert(crate::layout::ParaId(p.id));
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = HashMap::new();
    doc.sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    doc.stories.values().chain(doc.footnotes.values()).chain(doc.endnotes.values()).for_each(|b| walk(b, &mut out));
    out
}

impl Session {
    /// The document's revisions, in document order.
    pub fn revisions(&mut self) -> Vec<RevisionInfo> {
        self.complete_layout();
        let pages = crate::services::para_pages(&mut self.editor);
        let e = &self.editor;
        let doc = e.document();
        let list = index::revisions(doc, e.index());
        let marks = Marks::of(doc);
        let first_id = |r: &index::Revision| revision_id(r.ids.first().copied().unwrap_or(crate::docx::review::RevisionId(0)));
        list.iter()
            .map(|r| {
                let (kind, text, description) = match r.kind {
                    Kind::Insert => (RevisionKind::Insertion, Marks::text(&marks.inserted, &r.ids), "Inserted".to_string()),
                    Kind::Delete => (RevisionKind::Deletion, Marks::text(&marks.deleted, &r.ids), "Deleted".to_string()),
                    Kind::MovedTo => (RevisionKind::MoveTo, Marks::text(&marks.inserted, &r.ids), "Moved".to_string()),
                    Kind::MovedFrom => (RevisionKind::MoveFrom, Marks::text(&marks.deleted, &r.ids), "Moved".to_string()),
                    Kind::Property => {
                        let mut names: Vec<&str> = Vec::new();
                        for n in r.ids.iter().filter_map(|id| marks.formatted.get(&id.0)).flatten() {
                            if !names.contains(n) {
                                names.push(n);
                            }
                        }
                        (RevisionKind::Formatting, String::new(), format!("Formatted: {}", names.join(", ")))
                    }
                    Kind::ParagraphProperty => (RevisionKind::ParagraphProperties, String::new(), "Formatted: Paragraph".into()),
                    Kind::TableProperty => (RevisionKind::TableProperties, String::new(), "Formatted Table".into()),
                    Kind::SectionProperty => (RevisionKind::SectionProperties, String::new(), "Formatted: Section".into()),
                    Kind::CellInsertion => (RevisionKind::CellInsertion, String::new(), "Inserted Cells".into()),
                    Kind::CellDeletion => (RevisionKind::CellDeletion, String::new(), "Deleted Cells".into()),
                    Kind::CellMerge => (RevisionKind::CellMerge, String::new(), "Merged Cells".into()),
                };
                // A move's other half: the revision of the opposite kind in the same move range.
                let name = r.ids.iter().find_map(|id| marks.moves.get(&id.0));
                let pair = name.and_then(|name| {
                    let other = if r.kind == Kind::MovedFrom { Kind::MovedTo } else { Kind::MovedFrom };
                    list.iter().find(|o| o.kind == other && o.ids.iter().any(|id| marks.moves.get(&id.0) == Some(name))).map(first_id)
                });
                let page = r.start.para_offset().and_then(|(p, _)| pages.get(&p).copied()).unwrap_or(1);
                RevisionInfo {
                    id: first_id(r),
                    kind,
                    author: person(doc, r.author),
                    time: time(&r.date),
                    text,
                    description,
                    page,
                    story: story(e, r.story),
                    pair: pair.filter(|_| matches!(r.kind, Kind::MovedFrom | Kind::MovedTo)),
                }
            })
            .collect()
    }

    /// The document's comment threads, in document order (those without a range last).
    pub fn comments(&mut self) -> Vec<CommentThread> {
        self.complete_layout();
        let pages = crate::services::para_pages(&mut self.editor);
        let threads = self.editor.comment_threads();
        let doc = self.editor.document();
        let starts = comment_starts(doc);
        let comment = |id: crate::docx::review::CommentId| {
            let c = doc.comments.by_id.get(&id)?;
            let body = rich::RichText::from_blocks(&c.body);
            Some(Comment {
                id: comment_id(id),
                author: person(doc, c.author),
                time: time(&c.date_utc).or_else(|| time(&c.date)),
                plain_text: body.to_plain(),
                body: rich_text(&body),
            })
        };
        threads
            .into_iter()
            .map(|t| CommentThread {
                id: comment_id(t.root),
                done: doc.comments.by_id.get(&t.root).is_some_and(|c| c.done),
                anchor_text: t.scope.unwrap_or_default(),
                page: starts.get(&t.root.0).and_then(|p| pages.get(p).copied()).unwrap_or(1),
                comments: std::iter::once(t.root).chain(t.replies).filter_map(comment).collect(),
            })
            .collect()
    }

    /// Everyone with revisions or comments in the document.
    pub fn people(&self) -> Vec<Person> {
        let doc = self.editor.document();
        (0..doc.people.persons.len() as u32).map(|i| person(doc, AuthorId(i))).collect()
    }

    /// The events a committed transaction's review changes call for: `revisionsChanged`, `commentsChanged`.
    pub(super) fn review_events(&mut self, record: &TransactionRecord, revisions_before: u32) -> Vec<Event> {
        let s = &record.summary;
        let doc = self.editor.document();
        // The summary names the revisions a transaction touched; the count catches the rest. Comments are compared with
        // the last seen too.
        let revisions = !s.revisions.is_empty() || self.state.review.revisions != revisions_before;
        let comments = !s.comments.is_empty() || doc.comments != self.services.comments_seen;
        if comments {
            self.services.comments_seen = doc.comments.clone();
        }
        let mut out = Vec::new();
        if revisions {
            out.push(Event::RevisionsChanged {});
        }
        if comments {
            out.push(Event::CommentsChanged {});
        }
        out
    }

    /// Records the anchors of the comments a frame shows, and emits `commentAnchors` when they changed.
    pub(super) fn set_comment_anchors(&mut self, view: &crate::editing::view::View, stack: &crate::editing::view::PageStack, px_per_pt: f64) {
        let doc = self.editor.document();
        let to = |page: usize, x: f64, y: f64| stack.to_doc(page, x, y).map(|(x, y)| ContentPoint { x: x * px_per_pt, y: y * px_per_pt });
        let anchors: Vec<CommentAnchor> = view
            .comments
            .iter()
            .filter_map(|c| {
                let (&(page, first), &(last_page, last)) = (c.rects.first()?, c.rects.last()?);
                // The line boxes' bottoms stand for the baselines.
                let (top, start) = (to(page, first[0], first[1])?, to(page, first[0], first[3])?);
                let id = crate::docx::review::CommentId(c.thread.0);
                Some(CommentAnchor {
                    thread: comment_id(crate::docx::review::CommentId(c.thread.0)),
                    page: page as u32 + 1,
                    active: c.active,
                    done: doc.comments.by_id.get(&id).is_some_and(|c| c.done),
                    line_top: top.y,
                    line_bottom: start.y,
                    start,
                    end: to(last_page, last[2], last[3])?,
                    page_edge: stack.pages.get(page).map_or(0.0, |r| (r[0] + r[2]) * px_per_pt),
                })
            })
            .collect();
        if anchors != self.services.anchors {
            self.services.anchors = anchors.clone();
            self.emit(Event::CommentAnchors(anchors));
        }
    }
}

/// The review values of the state: tracking, whether protection keeps it on, and the revision and comment counts.
pub(super) fn fill_state(s: &mut Session, state: &mut State) {
    let e = &mut s.editor;
    let r = &mut state.review;
    r.tracking = e.tracking();
    r.tracking_locked = e.document().settings.protection.as_ref().is_some_and(|p| p.forces_tracking());
    r.markup_view = e.document().review.view;
    let revision = e.revision();
    r.revisions = match s.services.revision_count {
        Some((at, n)) if at == revision => n,
        _ => {
            let n = index::revisions(e.document(), e.index()).len() as u32;
            s.services.revision_count = Some((revision, n));
            n
        }
    };
    r.comments = e.document().comments.threads().len() as u32;
    r.active_comment = e.active_comment().map(comment_id);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::api::command::{AuthorFilter, Command, CommandResult, CommentReply, CommentResolve};
    use crate::api::engine::{Engine, SessionOptions};
    use crate::api::event::Event;
    use crate::api::fonts::FontLibrary;
    use crate::api::record::Author;
    use crate::api::review::{Body, RevisionKind};
    use crate::api::services::FindQuery;
    use crate::api::value::Order;

    type Events = Rc<RefCell<Vec<Event>>>;

    /// Whether an event since the last call matches.
    fn heard(events: &Events, f: fn(&Event) -> bool) -> bool {
        std::mem::take(&mut *events.borrow_mut()).iter().any(f)
    }

    #[test]
    fn revisions_and_comments_list_and_report() {
        let mut engine = Engine::new(FontLibrary::system());
        let events: Events = Rc::default();
        let sink = events.clone();
        engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-mixed-format.docx");
        s.open(&std::fs::read(path).expect("fixture")).expect("opens");
        s.set_author(Author { name: "Ada".into(), ..Author::default() });
        s.run(Command::SetTrackChanges(true)).expect("tracks");
        assert!(s.state().review.tracking);
        let q = |t: &str| FindQuery { text: t.into(), ..FindQuery::default() };
        assert!(s.find(&q("bold words"), Order::Next).found);
        s.run(Command::InsertText("new".into())).expect("types");
        assert!(heard(&events, |e| matches!(e, Event::RevisionsChanged {})));
        let list = s.revisions();
        let kinds: Vec<_> = list.iter().map(|r| (r.kind, r.text.as_str(), r.author.name.as_str(), r.page)).collect();
        assert!(kinds.contains(&(RevisionKind::Insertion, "new", "Ada", 1)), "{kinds:?}");
        assert!(kinds.contains(&(RevisionKind::Deletion, "bold words", "Ada", 1)), "{kinds:?}");
        assert!(list.iter().all(|r| r.time.is_some()));
        assert_eq!(s.state().review.revisions, 2);
        let insertion = list.iter().find(|r| r.kind == RevisionKind::Insertion).map(|r| r.id.clone()).expect("one");
        s.run(Command::AcceptRevision(insertion)).expect("accepts");
        assert_eq!(s.state().review.revisions, 1);
        s.run(Command::RejectAllRevisions(AuthorFilter { author: Some("Ada".into()) })).expect("rejects");
        assert_eq!(s.state().review.revisions, 0);
        assert_eq!(s.people().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Ada"]);

        s.run(Command::SetTrackChanges(false)).expect("stops");
        assert!(s.find(&q("Arial words"), Order::Next).found);
        events.borrow_mut().clear();
        let made = s.run(Command::AddComment(Body::Text("Check this.".into()))).expect("comments");
        let Some(CommandResult::Comment { id: root }) = made.value else { panic!("{made:?}") };
        assert!(heard(&events, |e| matches!(e, Event::CommentsChanged {})));
        s.run(Command::ReplyToComment(CommentReply { to: root.clone(), body: Body::Text("Agreed.".into()) })).expect("replies");
        s.run(Command::ResolveComment(CommentResolve { id: root.clone(), done: true })).expect("resolves");
        assert!(heard(&events, |e| matches!(e, Event::CommentsChanged {})));
        let threads = s.comments();
        assert_eq!(threads.len(), 1);
        let t = &threads[0];
        assert_eq!((t.id.clone(), t.done, t.anchor_text.as_str(), t.page), (root.clone(), true, "Arial words", 1));
        assert_eq!(t.comments.iter().map(|c| c.plain_text.as_str()).collect::<Vec<_>>(), ["Check this.", "Agreed."]);
        assert_eq!(t.comments[0].body.paragraphs[0].runs[0].text, "Check this.");
        assert_eq!(s.state().review.comments, 1);
        s.run(Command::DeleteComment(root)).expect("deletes");
        assert_eq!((s.state().review.comments, s.state().review.active_comment.clone()), (0, None));
    }
}
