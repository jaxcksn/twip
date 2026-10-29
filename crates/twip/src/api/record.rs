//! Transaction records: who changed what, when, and with which undo label.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::value::{CommentId, ElementId, RevisionId};

pub use crate::docx::review::Identity;

/// One committed transaction, as `documentChanged` reports it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionRecord {
    /// Unique in the session, increasing.
    pub id: u64,
    /// The document revision this transaction made.
    pub revision: u64,
    /// Who started it.
    pub origin: Origin,
    /// The session's author at the time.
    pub author: Author,
    /// When it committed.
    pub time: Timestamp,
    /// The undo label (`Typing`, `Bold`, a batch's label).
    pub label: String,
    /// What it touched.
    pub summary: ChangeSummary,
}

/// What started a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// Input: keys, text, pointer, clipboard.
    User,
    /// A command or batch the application ran.
    Api,
    /// An undo.
    Undo,
    /// A redo.
    Redo,
}

/// Who edits: the name revisions and comments are recorded under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Author {
    /// The display name.
    pub name: String,
    /// The initials shown on comments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    /// The identity in a directory, written to the document with the author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
}

impl Default for Author {
    fn default() -> Author {
        Author { name: "Author".into(), initials: None, identity: None }
    }
}

/// A UTC time in whole seconds since 1970, ISO 8601 on the wire (`2026-10-27T09:30:00Z`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Now, by the system clock.
    pub fn now() -> Timestamp {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Timestamp(i64::try_from(secs).unwrap_or(i64::MAX))
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::docx::review::Timestamp::from_utc(self.0).raw)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Timestamp, D::Error> {
        let s = String::deserialize(d)?;
        crate::docx::review::Timestamp::parse(&s).utc.map(Timestamp).ok_or_else(|| D::Error::custom(format!("`{s}` is not an ISO 8601 time")))
    }
}

/// What a transaction touched: how many paragraphs and tables, and which elements, revisions and comments.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeSummary {
    /// How many paragraphs it changed.
    pub paragraphs: u32,
    /// How many tables it changed.
    pub tables: u32,
    /// The elements it changed.
    pub elements: Vec<ElementId>,
    /// The revisions it made or changed.
    pub revisions: Vec<RevisionId>,
    /// The comments it made or changed.
    pub comments: Vec<CommentId>,
}

impl TransactionRecord {
    /// The record of a committed transaction, the session's `id`-th, made by `author` now.
    pub(crate) fn of(committed: crate::editing::Committed, id: u64, author: &Author) -> TransactionRecord {
        TransactionRecord {
            id,
            revision: committed.revision,
            origin: committed.origin,
            author: author.clone(),
            time: Timestamp::now(),
            label: committed.label.into_owned(),
            summary: ChangeSummary::of(&committed.targets),
        }
    }
}

impl ChangeSummary {
    /// What steps touched, counted and named. Ids come in order.
    fn of(t: &crate::editing::step::Targets) -> ChangeSummary {
        fn sorted<T: Ord + Copy>(v: &std::collections::HashSet<T>) -> Vec<T> {
            let mut v: Vec<T> = v.iter().copied().collect();
            v.sort_unstable();
            v
        }
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        ChangeSummary {
            paragraphs: count(t.paras.len()),
            tables: count(t.tables.len()),
            elements: sorted(&t.elements).into_iter().map(|e| ElementId(crate::editing::elements::id_str(e))).collect(),
            revisions: sorted(&t.revisions).into_iter().map(revision_id).collect(),
            comments: sorted(&t.comments).into_iter().map(comment_id).collect(),
        }
    }
}

/// A revision's id as the API names it (`r7`).
pub(crate) fn revision_id(id: crate::docx::review::RevisionId) -> RevisionId {
    RevisionId(format!("r{}", id.0))
}

/// A comment's id as the API names it (`c4`).
pub(crate) fn comment_id(id: crate::docx::review::CommentId) -> CommentId {
    CommentId(format!("c{}", id.0))
}

/// The revision an API id names, when it is one.
pub(crate) fn parse_revision_id(id: &RevisionId) -> Option<crate::docx::review::RevisionId> {
    id.0.trim().strip_prefix('r')?.parse().ok().map(crate::docx::review::RevisionId)
}

/// The comment an API id names, when it is one.
pub(crate) fn parse_comment_id(id: &CommentId) -> Option<crate::docx::review::CommentId> {
    id.0.trim().strip_prefix('c')?.parse().ok().map(crate::docx::review::CommentId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_iso_utc() {
        let t = Timestamp(1_792_000_000);
        let json = serde_json::to_string(&t).expect("serializes");
        assert_eq!(json, "\"2026-10-14T17:46:40Z\"");
        assert_eq!(serde_json::from_str::<Timestamp>(&json).ok(), Some(t));
    }
}
