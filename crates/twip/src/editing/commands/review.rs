//! Review: tracking, revisions and comments.

use crate::api::command::{AuthorFilter, CommentEdit, CommentReply, CommentResolve};
use crate::api::review::Body;
use crate::api::value::{CommentId, RevisionId};
use crate::docx::review;
use crate::font::Fonts;

use super::Outcome;
use crate::editing::Editor;
use crate::editing::comments::{CommentCmd, RichParagraph, RichRun, RichText};
use crate::editing::revisions::resolve::Scope;
use crate::editing::selection::{Granularity, Selection};

/// A body as the comment store keeps it.
fn rich(b: &Body) -> RichText {
    match b {
        Body::Text(t) => RichText::plain(t),
        Body::Rich(r) => RichText {
            paragraphs: r
                .paragraphs
                .iter()
                .map(|p| RichParagraph {
                    runs: p
                        .runs
                        .iter()
                        .map(|r| RichRun {
                            text: r.text.clone(),
                            bold: r.bold.unwrap_or(false),
                            italic: r.italic.unwrap_or(false),
                            underline: r.underline.unwrap_or(false),
                            strike: r.strikethrough.unwrap_or(false),
                            link: r.link.clone(),
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

/// A comment id as the document numbers it.
fn comment_id(id: &CommentId) -> Option<review::CommentId> {
    crate::api::record::parse_comment_id(id)
}

impl Editor {
    pub(in crate::editing) fn set_track_changes(&mut self, on: bool) -> Outcome {
        if on == self.tracking() {
            return Outcome::Unchanged("already so");
        }
        if self.set_tracking(on) { Outcome::Moved } else { Outcome::unavailable("tracking locked") }
    }

    pub(in crate::editing) fn resolve_one(&mut self, accept: bool, id: &RevisionId, fonts: &Fonts) -> Outcome {
        let Some(n) = crate::api::record::parse_revision_id(id) else { return Outcome::invalid("no such revision") };
        self.resolve_revisions(accept, &Scope::Revision(n), fonts)
    }

    pub(in crate::editing) fn resolve_all(&mut self, accept: bool, a: AuthorFilter, fonts: &Fonts) -> Outcome {
        let scope = match a.author {
            Some(name) => Scope::Author(name),
            None => Scope::All,
        };
        self.resolve_revisions(accept, &scope, fonts)
    }

    /// A comment command; what it makes (a new comment) is its result.
    fn comment(&mut self, c: CommentCmd, fonts: &Fonts) -> Outcome {
        self.comment_cmd(c, fonts)
    }

    pub(in crate::editing) fn add_comment_cmd(&mut self, body: &Body, fonts: &Fonts) -> Outcome {
        self.comment(CommentCmd::Add(rich(body)), fonts)
    }

    pub(in crate::editing) fn reply_to_comment(&mut self, r: CommentReply, fonts: &Fonts) -> Outcome {
        let Some(to) = comment_id(&r.to) else { return Outcome::invalid("no such comment") };
        self.comment(CommentCmd::Reply { to, body: rich(&r.body) }, fonts)
    }

    pub(in crate::editing) fn edit_comment_cmd(&mut self, e: CommentEdit, fonts: &Fonts) -> Outcome {
        let Some(id) = comment_id(&e.id) else { return Outcome::invalid("no such comment") };
        self.comment(CommentCmd::Edit { id, body: rich(&e.body) }, fonts)
    }

    pub(in crate::editing) fn delete_comment_cmd(&mut self, id: &CommentId, fonts: &Fonts) -> Outcome {
        let Some(id) = comment_id(id) else { return Outcome::invalid("no such comment") };
        self.comment(CommentCmd::Delete(id), fonts)
    }

    pub(in crate::editing) fn resolve_comment_cmd(&mut self, r: CommentResolve, fonts: &Fonts) -> Outcome {
        let Some(id) = comment_id(&r.id) else { return Outcome::invalid("no such comment") };
        self.comment(CommentCmd::Resolve { id, done: r.done }, fonts)
    }

    pub(in crate::editing) fn set_active_comment(&mut self, id: Option<&CommentId>, fonts: &Fonts) -> Outcome {
        let id = match id {
            Some(id) => match comment_id(id) {
                Some(id) => Some(id),
                None => return Outcome::invalid("no such comment"),
            },
            None => None,
        };
        self.comment(CommentCmd::Activate(id), fonts)
    }

    /// The comment's thread range selected and shown.
    pub(in crate::editing) fn select_comment(&mut self, id: &CommentId, fonts: &Fonts) -> Outcome {
        let Some(id) = comment_id(id).filter(|id| self.doc.comments.by_id.contains_key(id)) else { return Outcome::invalid("no such comment") };
        let Some((a, b)) = self.comment_range(id) else { return Outcome::unavailable("the comment has no range") };
        self.set_selection(Selection::range(a, b, Granularity::Char));
        self.ensure(&b, fonts);
        self.scroll_to_caret(fonts);
        Outcome::Moved
    }

    /// The colour an author's revisions and comments show in, by the author's name.
    pub(in crate::editing) fn set_author_color_by_name(&mut self, author: &str, color: u32) -> Outcome {
        match self.doc.people.persons.iter().position(|p| *p.name == *author) {
            Some(i) => self.set_author_color(review::AuthorId(i as u32), color),
            None => Outcome::invalid("no such author"),
        }
    }
}
