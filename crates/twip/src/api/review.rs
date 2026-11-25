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
    /// Show Markup shows it (its kind and its reviewer); one not shown draws as if accepted.
    #[serde(default = "shown")]
    pub shown: bool,
}

fn shown() -> bool {
    true
}

/// The most characters of commented text a [`CommentThread`] carries.
pub const ANCHOR_TEXT_LIMIT: usize = 1_000;

/// A comment thread: the first comment and its replies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentThread {
    /// The first comment's id.
    pub id: CommentId,
    /// Whether the thread is resolved.
    pub done: bool,
    /// The commented text, cut to its first [`ANCHOR_TEXT_LIMIT`] characters.
    pub anchor_text: String,
    /// The page it starts on.
    pub page: u32,
    /// The first comment, then its replies.
    pub comments: Vec<Comment>,
    /// Show Markup shows the thread (comments, and its first comment's author).
    #[serde(default = "shown")]
    pub shown: bool,
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
    /// Show Markup shows it: a reply shows exactly when its thread does.
    #[serde(default = "shown")]
    pub shown: bool,
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
    /// Show Markup shows their revisions and comments (not hidden by Specific People).
    #[serde(default = "shown")]
    pub visible: bool,
}

/// Where revisions show in All Markup (Balloons).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BalloonMode {
    /// Deletions and formatting changes in balloons beside the page, insertions in place.
    Balloons,
    /// Every revision in place; comments stay in their cards.
    Inline,
    /// Formatting changes in balloons, insertions and deletions in place.
    #[default]
    CommentsAndFormatting,
}

/// Which kinds of markup show, and whose (Show Markup).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShowMarkup {
    /// Insertions and deletions: of text, moves, rows and cells.
    pub insertions_deletions: bool,
    /// Formatting changes.
    pub formatting: bool,
    /// Comments.
    pub comments: bool,
    /// The reviewers whose markup doesn't show, by name.
    pub hidden_reviewers: Vec<String>,
}

impl Default for ShowMarkup {
    fn default() -> ShowMarkup {
        ShowMarkup { insertions_deletions: true, formatting: true, comments: true, hidden_reviewers: Vec::new() }
    }
}

/// How a revision marks its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextMark {
    /// Neither style nor colour.
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
    /// Deleted text doesn't show (deletions and moved-away text only).
    Hidden,
    /// Deleted text shows as one `^` (deletions and moved-away text only).
    Caret,
    /// Deleted text shows as one `#` (deletions and moved-away text only).
    Pound,
}

/// The colour a revision mark or cell highlight draws in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkColor {
    /// The revision's author's.
    ByAuthor,
    /// The text's own colour (a cell's own fill).
    Auto,
    /// This colour.
    Fixed(Color),
}

/// How one kind of revision draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkLook {
    /// How the text is marked.
    pub mark: TextMark,
    /// The mark's colour.
    pub color: MarkColor,
}

/// Which side of the text change bars go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BarSide {
    /// No change bars.
    None,
    /// Left of the text.
    Left,
    /// Right of the text.
    Right,
    /// The page's outside edge.
    #[default]
    Outside,
}

/// How revisions draw (Track Changes Options). A view setting: not saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    /// The change bars' colour, or `auto`: red in Simple Markup, grey in All Markup.
    pub change_bar_color: super::value::AutoOr<Color>,
    /// Inserted cells' fill.
    pub inserted_cells: MarkColor,
    /// Deleted cells' fill.
    pub deleted_cells: MarkColor,
    /// Merged cells' fill.
    pub merged_cells: MarkColor,
    /// Split cells' fill.
    pub split_cells: MarkColor,
}

impl Default for MarkupStyle {
    fn default() -> MarkupStyle {
        MarkupStyle::of(&crate::docx::review::MarkupStyle::default())
    }
}

impl MarkColor {
    fn of(c: crate::docx::review::MarkColor) -> MarkColor {
        use crate::docx::review::MarkColor as M;
        match c {
            M::ByAuthor => MarkColor::ByAuthor,
            M::Auto => MarkColor::Auto,
            M::Fixed(c) => MarkColor::Fixed(Color(c)),
        }
    }

    fn model(self) -> crate::docx::review::MarkColor {
        use crate::docx::review::MarkColor as M;
        match self {
            MarkColor::ByAuthor => M::ByAuthor,
            MarkColor::Auto => M::Auto,
            MarkColor::Fixed(c) => M::Fixed(c.0),
        }
    }
}

impl TextMark {
    fn of(m: crate::docx::review::TextMark) -> TextMark {
        use crate::docx::review::TextMark as T;
        match m {
            T::None => TextMark::None,
            T::ColorOnly => TextMark::ColorOnly,
            T::Bold => TextMark::Bold,
            T::Italic => TextMark::Italic,
            T::Underline => TextMark::Underline,
            T::DoubleUnderline => TextMark::DoubleUnderline,
            T::Strikethrough => TextMark::Strikethrough,
            T::DoubleStrikethrough => TextMark::DoubleStrikethrough,
            T::Hidden => TextMark::Hidden,
            T::Caret => TextMark::Caret,
            T::Pound => TextMark::Pound,
        }
    }

    fn model(self) -> crate::docx::review::TextMark {
        use crate::docx::review::TextMark as T;
        match self {
            TextMark::None => T::None,
            TextMark::ColorOnly => T::ColorOnly,
            TextMark::Bold => T::Bold,
            TextMark::Italic => T::Italic,
            TextMark::Underline => T::Underline,
            TextMark::DoubleUnderline => T::DoubleUnderline,
            TextMark::Strikethrough => T::Strikethrough,
            TextMark::DoubleStrikethrough => T::DoubleStrikethrough,
            TextMark::Hidden => T::Hidden,
            TextMark::Caret => T::Caret,
            TextMark::Pound => T::Pound,
        }
    }
}

impl BalloonMode {
    /// The model's mode.
    pub fn model(self) -> crate::docx::review::BalloonMode {
        use crate::docx::review::BalloonMode as B;
        match self {
            BalloonMode::Balloons => B::Balloons,
            BalloonMode::Inline => B::Inline,
            BalloonMode::CommentsAndFormatting => B::CommentsAndFormatting,
        }
    }

    /// The API's mode for the model's.
    pub fn of(m: crate::docx::review::BalloonMode) -> BalloonMode {
        use crate::docx::review::BalloonMode as B;
        match m {
            B::Balloons => BalloonMode::Balloons,
            B::Inline => BalloonMode::Inline,
            B::CommentsAndFormatting => BalloonMode::CommentsAndFormatting,
        }
    }
}

impl MarkupStyle {
    /// The API's style for the model's.
    pub fn of(s: &crate::docx::review::MarkupStyle) -> MarkupStyle {
        use crate::docx::review::BarSide as B;
        let look = |l: crate::docx::review::MarkLook| MarkLook { mark: TextMark::of(l.mark), color: MarkColor::of(l.color) };
        MarkupStyle {
            insertions: look(s.insertions),
            deletions: look(s.deletions),
            formatting: look(s.formatting),
            moved_from: look(s.moved_from),
            moved_to: look(s.moved_to),
            change_bars: match s.change_bars {
                B::None => BarSide::None,
                B::Left => BarSide::Left,
                B::Right => BarSide::Right,
                B::Outside => BarSide::Outside,
            },
            change_bar_color: s.change_bar_color.map_or(super::value::AutoOr::Auto, |c| super::value::AutoOr::Value(Color(c))),
            inserted_cells: MarkColor::of(s.inserted_cells),
            deleted_cells: MarkColor::of(s.deleted_cells),
            merged_cells: MarkColor::of(s.merged_cells),
            split_cells: MarkColor::of(s.split_cells),
        }
    }

    /// `style` with `change`'s fields over it, as the model's.
    pub fn changed(style: &crate::docx::review::MarkupStyle, change: &super::command::MarkupStyleChange) -> crate::docx::review::MarkupStyle {
        use crate::docx::review::BarSide as B;
        let look = |l: crate::docx::review::MarkLook, c: Option<super::command::MarkLookChange>| {
            let Some(c) = c else { return l };
            crate::docx::review::MarkLook { mark: c.mark.map_or(l.mark, TextMark::model), color: c.color.map_or(l.color, MarkColor::model) }
        };
        let cell = |was, c: Option<MarkColor>| c.map_or(was, MarkColor::model);
        crate::docx::review::MarkupStyle {
            insertions: look(style.insertions, change.insertions),
            deletions: look(style.deletions, change.deletions),
            formatting: look(style.formatting, change.formatting),
            moved_from: look(style.moved_from, change.moved_from),
            moved_to: look(style.moved_to, change.moved_to),
            change_bars: change.change_bars.map_or(style.change_bars, |b| match b {
                BarSide::None => B::None,
                BarSide::Left => B::Left,
                BarSide::Right => B::Right,
                BarSide::Outside => B::Outside,
            }),
            change_bar_color: change.change_bar_color.map_or(style.change_bar_color, |c| match c {
                super::value::AutoOr::Auto => None,
                super::value::AutoOr::Value(c) => Some(c.0),
            }),
            inserted_cells: cell(style.inserted_cells, change.inserted_cells),
            deleted_cells: cell(style.deleted_cells, change.deleted_cells),
            merged_cells: cell(style.merged_cells, change.merged_cells),
            split_cells: cell(style.split_cells, change.split_cells),
        }
    }
}

/// How many revisions of each kind, and comment threads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkupCounts {
    /// Inserted text, rows and cells.
    pub insertions: u32,
    /// Deleted text, rows and cells.
    pub deletions: u32,
    /// Both halves of each move.
    pub moves: u32,
    /// Formatting changes: of text, paragraphs, tables, sections and cells, and cell merges.
    pub formatting: u32,
    /// Comment threads.
    pub comments: u32,
}

/// The review counts: of everything, and of what Show Markup shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewCounts {
    /// Every revision and thread.
    pub all: MarkupCounts,
    /// Those Show Markup shows.
    pub shown: MarkupCounts,
}

/// Where a revision balloon goes (content coordinates, CSS px), for a host that draws balloons beside the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionAnchor {
    /// The revision (`revisions()` gives its text and description).
    pub revision: RevisionId,
    /// What changed.
    pub kind: RevisionKind,
    /// Who made it.
    pub author: String,
    /// The page (from 1).
    pub page: u32,
    /// The revision's first character, at its baseline.
    pub start: ContentPoint,
    /// The first line's top.
    pub line_top: f64,
    /// The first line's bottom.
    pub line_bottom: f64,
    /// The x where a leader line leaves the page.
    pub page_edge: f64,
    /// The x of the page's left edge, for balloons on the left.
    pub page_left: f64,
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
    /// A link's address: a web, mail, FTP or phone one, or a relative one. The run of any other is plain text.
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
    /// The x of the page's left edge, for balloons on the left.
    #[serde(default)]
    pub page_left: f64,
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
