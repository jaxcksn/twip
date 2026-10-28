//! Constraint spaces: every non-content input an algorithm's result depends on, compared by equality
//! wherever results are reused.

use crate::docx::props::Border;

use crate::docx::review::MarkupView;

use super::env::TextEnv;

/// What shaping a paragraph depends on besides its content: the resolved text of its page-dependent fields, in
/// inline order (`None`: the field shows its cached result), and the numbers of its note references and marks.
/// Empty for most paragraphs, which therefore shape once for every page they appear on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeSpace {
    pub fields: Vec<Option<String>>,
    pub notes: Vec<String>,
    /// [`super::env::TextEnv::balance_spaces`] and [`super::env::TextEnv::compress_punctuation`].
    pub balance: bool,
    pub compress: bool,
    /// The markup view: which revised text shapes at all.
    pub view: MarkupView,
    /// Where a joined paragraph's units come from (REV-6): its pieces name their own paragraphs once drawn.
    pub join: Option<std::rc::Rc<super::join::JoinMap>>,
}

/// A paragraph's border settings, which make consecutive paragraphs share one box when identical (PARA-7):
/// top, left, bottom, right, between. `None` when the paragraph has no border; shared, so a line space stays small.
pub type BorderKey = Option<std::rc::Rc<[Option<Border>; 5]>>;

/// What breaking a paragraph into lines depends on besides its shaped pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct ParaSpace {
    /// Column (or cell content) width.
    pub width: f64,
    /// The piece the lines start at: 0, or where a paragraph continued from an earlier column starts (re-broken at
    /// this column's width, without first-line indent or top border).
    pub start: usize,
    /// The neighbours' border keys (PARA-7): the previous paragraph's (the paragraph's own for a continuation) and the
    /// next one's; both `None` for a paragraph without borders.
    pub borders: (BorderKey, BorderKey),
    /// The x the lines are drawn at: the column's left edge for body text, 0 for stories laid out as units (cells,
    /// headers, notes, text boxes), which are drawn at 0 and moved into place. TAB-9's tab leaders sit on a grid from
    /// this origin, so drawn lines depend on it.
    pub x0: f64,
    pub env: TextEnv,
}

/// One column of a page body (COL-1): where it is and how far content may go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnSpace {
    pub x: f64,
    pub width: f64,
    pub top: f64,
    pub bottom: f64,
    /// Balancing (COL-5): a column's last space-after counts toward its height.
    pub balance: bool,
}

/// A page's body area (HDR-4 to HDR-6, HDR-13): left edge and width after gutter and mirroring, top and bottom after
/// the header and footer heights the page gets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyArea {
    pub left: f64,
    pub width: f64,
    pub top: f64,
    pub bottom: f64,
}

/// What a page body depends on besides its section and token. No page number: parity and number enter only through
/// the values they produce (the margins, the header and footer heights, [`BodyFields`]).
#[derive(Clone, Debug, PartialEq)]
pub struct BodySpace {
    pub section: usize,
    pub page_w: f64,
    pub page_h: f64,
    /// Left, right and top margins after gutter and mirroring (floats position against them, FLOAT-6).
    pub margins: (f64, f64, f64),
    pub area: BodyArea,
    /// Column boxes (COL-1), from the body's top.
    pub columns: Vec<ColumnSpace>,
    /// The body starts below an earlier section's on the same page (a continuous section break).
    pub mid_page: bool,
    /// The section ends with a continuous break and several columns: its last page is balanced (COL-5).
    pub balance: bool,
    /// The next section starts on the same page (continuous): the body isn't vertically aligned (SETUP-1).
    pub next_continuous: bool,
    /// The header and footer heights the area was computed from (`None`: the page shows no such story), as HDR-8 caps
    /// them: what convergence compares with the composed ones.
    pub hf: HfHeights,
}

/// A page's header and footer heights (HDR-4 to HDR-6): `None` when it shows no such story.
pub type HfHeights = [Option<f64>; 2];

/// The values of the body's page-dependent fields on its page (FIELD-1).
pub type BodyFields = super::text::FieldValues;
