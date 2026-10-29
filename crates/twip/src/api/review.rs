//! Revisions, comments and people, as applications read them, and where comment cards go beside the pages.

use serde::{Deserialize, Serialize};

use super::record::{Identity, Timestamp};
use super::services::ContentPoint;
use super::value::{Color, CommentId, RevisionId, Story};

pub use crate::docx::review::MarkupView;

/// The kinds of revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevisionKind {
    /// Inserted text.
    Insertion,
    /// Deleted text.
    Deletion,
    /// The origin of moved text.
    MoveFrom,
    /// The destination of moved text.
    MoveTo,
    /// Changed character formatting.
    Formatting,
    /// Changed paragraph properties.
    ParagraphProperties,
    /// Changed section properties.
    SectionProperties,
    /// Changed table properties.
    TableProperties,
    /// An inserted table row.
    RowInsertion,
    /// A deleted table row.
    RowDeletion,
    /// An inserted table cell.
    CellInsertion,
    /// A deleted table cell.
    CellDeletion,
    /// Merged table cells.
    CellMerge,
}

/// One revision, for a reviewing pane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionInfo {
    /// The revision.
    pub id: RevisionId,
    /// What changed.
    pub kind: RevisionKind,
    /// Who made it.
    pub author: Person,
    /// `null` when the document gives no readable date.
    pub time: Option<Timestamp>,
    /// The inserted, deleted or moved text, when there is some.
    pub text: String,
    /// What changed, for formatting changes (`Formatted: Bold, Italic`).
    pub description: String,
    /// The page it starts on.
    pub page: u32,
    /// The story it is in.
    pub story: Story,
    /// A move's other half.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair: Option<RevisionId>,
}

/// A comment thread: the first comment and its replies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentThread {
    /// The first comment's id.
    pub id: CommentId,
    /// Whether the thread is resolved.
    pub done: bool,
    /// The commented text.
    pub anchor_text: String,
    /// The page it starts on.
    pub page: u32,
    /// The first comment, then its replies.
    pub comments: Vec<Comment>,
}

/// One comment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comment {
    /// The comment.
    pub id: CommentId,
    /// Who wrote it.
    pub author: Person,
    /// When, or `None` when the document gives no readable date.
    pub time: Option<Timestamp>,
    /// The body.
    pub body: RichText,
    /// The body as plain text.
    pub plain_text: String,
}

/// Someone who made revisions or comments, with the colour they show in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Person {
    /// The display name.
    pub name: String,
    /// The initials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    /// The identity in a directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    /// The colour the editor marks their changes with.
    pub color: Color,
}

/// Formatted text: paragraphs of runs (comment bodies, field results).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RichText {
    /// The paragraphs.
    pub paragraphs: Vec<RichParagraph>,
}

/// One paragraph of [`RichText`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RichParagraph {
    /// The runs.
    pub runs: Vec<RichRun>,
}

/// A run of [`RichText`]: text with its formatting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RichRun {
    /// The text.
    pub text: String,
    /// Bold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Italic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Underlined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Struck through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    /// A link's address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// Plain or formatted text, where a command takes either.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Body {
    /// Plain text.
    Text(String),
    /// Formatted text.
    Rich(RichText),
}

/// Where a visible comment's range is, for placing its card (content coordinates, CSS px).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentAnchor {
    /// The thread.
    pub thread: CommentId,
    /// The page.
    pub page: u32,
    /// Whether it is the active thread.
    pub active: bool,
    /// Whether the thread is resolved.
    pub done: bool,
    /// The range's first character, at its baseline.
    pub start: ContentPoint,
    /// The range's last character, at its baseline.
    pub end: ContentPoint,
    /// The first line's top, for a card level with it.
    pub line_top: f64,
    /// The first line's bottom.
    pub line_bottom: f64,
    /// The x where a leader line leaves the page.
    pub page_edge: f64,
}

/// Where the SDK placed an attached card, and its leader line (a polyline).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BalloonPlacement {
    /// The thread.
    pub thread: CommentId,
    /// The card's top.
    pub top: f64,
    /// The card's left.
    pub left: f64,
    /// The leader line, a polyline from the range to the card.
    pub leader: Vec<ContentPoint>,
}

/// Card tops that don't overlap, `gap` apart: card `i` (`heights[i]` tall) is for `anchors[i]`, in the order given.
/// The first active card sits level with its line (`line_top`); the cards before it rise above it only as far as they
/// must, and the cards after it fall below it. No card goes above 0: when the cards before the active one don't fit
/// above it, they start at 0 and push it down. With no active card, each card is level with its line or just below
/// the card before it. Pure: the SDKs run the same algorithm, checked by `tests/wire/balloons.json`.
pub fn stack_balloons(anchors: &[CommentAnchor], heights: &[f64], gap: f64) -> Vec<f64> {
    let n = anchors.len().min(heights.len());
    let mut tops: Vec<f64> = anchors[..n].iter().map(|a| a.line_top).collect();
    let active = anchors[..n].iter().position(|a| a.active).unwrap_or(0);
    for i in (0..active).rev() {
        tops[i] = tops[i].min(tops[i + 1] - gap - heights[i]);
    }
    let mut bottom = f64::NEG_INFINITY;
    for i in 0..n {
        tops[i] = tops[i].max(0.0).max(bottom + gap);
        bottom = tops[i] + heights[i];
    }
    tops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balloons_stack_without_overlap() {
        let at = |line_top| CommentAnchor { line_top, ..Default::default() };
        let tops = stack_balloons(&[at(10.0), at(20.0), at(200.0)], &[50.0, 30.0, 10.0], 4.0);
        assert_eq!(tops, vec![10.0, 64.0, 200.0]);
    }
}
