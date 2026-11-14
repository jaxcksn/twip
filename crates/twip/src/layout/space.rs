//! Constraint spaces

use crate::docx::props::Border;

use crate::docx::review::MarkupView;

use super::units::Lu;

use super::env::TextEnv;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeSpace {
    pub fields: Vec<Option<String>>,
    pub notes: Vec<String>,
    pub balance: Option<String>,
    /// The document's default East Asian font.
    pub default_east_asia: Option<String>,
    pub compress: bool,
    /// Compat 15: with the spaces balanced, compressed punctuation keeps the breaks after hyphens and dashes.
    pub balanced_dashes: bool,
    pub em1000: bool,
    /// The font metric options of [`super::env::TextEnv`].
    pub metric_rules: MetricRules,
    pub in_cell: bool,
    /// The text runs in a vertical section.
    pub vertical: bool,
    /// Page and column breaks in a frame break its lines.
    pub frame_breaks: bool,
    /// Fitted regions break lines with their first adjustment (compat 15).
    pub stale_fit: bool,
    /// Compat 15: a leading page or column break in a section's last paragraph, a frame or a cell doesn't start the
    /// paragraph again.
    pub breaks_continue: bool,
    /// The paragraph carries the section's break.
    pub ends_section: bool,
    /// How many pieces the document's first line holds, broken with its fitted regions unadjusted, when this
    /// paragraph holds it (compat 15 fitted text).
    pub first_line: Option<usize>,
    /// Fitted regions keep their natural widths (still never broken).
    pub unfitted: bool,
    pub view: MarkupView,
    pub join: Option<std::rc::Rc<super::join::JoinMap>>,
    pub float: Option<std::rc::Rc<crate::docx::Float>>,
    pub hide: Option<u64>,
    pub expansion: Option<std::rc::Rc<super::expansion::Expansion>>,
    /// The patterns, whether capitals are left alone, and the fewest letters after a break.
    pub hyphenate: Option<(super::hyphenate::Hyphenation, bool, usize)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MetricRules {
    pub no_leading: bool,
    pub no_raise_lower: bool,
    pub whole_point_spacing: bool,
    pub mac_small_caps: bool,
    pub wp_line_heights: bool,
    /// Compat ≥ 14: the hyphen, the figure dash and their kin break after too.
    pub dash_breaks: bool,
}

pub type BorderKey = Option<std::rc::Rc<BorderGroup>>;

/// A bordered paragraph's border group.
#[derive(Clone, Debug, PartialEq)]
pub struct BorderGroup {
    /// Top, left, bottom, right.
    pub sides: [Option<Border>; 4],
    pub between: Option<Border>,
    /// The box's left edge and right indent.
    pub edges: (f64, f64),
    pub frame: Option<crate::docx::props::FramePr>,
    /// The HTML division the paragraph is in.
    pub div: Option<i64>,
    /// The part of a paragraph continued from the previous page or column.
    pub continued: bool,
}

impl BorderGroup {
    pub fn joins(&self, o: &BorderGroup) -> bool {
        self.sides == o.sides && self.edges == o.edges && self.frame == o.frame && self.div == o.div
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParaSpace {
    pub width: f64,
    pub start: usize,
    pub borders: (BorderKey, BorderKey),
    pub x0: f64,
    pub env: TextEnv,
    pub fit: bool,
    /// The line before `start`, on an earlier page or column, broke between letters rather than at a hyphen.
    pub letter_break: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnSpace {
    pub x: f64,
    pub width: f64,
    pub top: Lu,
    pub bottom: Lu,
    pub balance: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyArea {
    pub left: f64,
    pub width: f64,
    pub top: Lu,
    pub bottom: Lu,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodySpace {
    pub section: usize,
    pub page_w: f64,
    pub page_h: f64,
    pub margins: (f64, f64, f64),
    pub area: BodyArea,
    pub columns: Vec<ColumnSpace>,
    pub mid_page: bool,
    pub balance: bool,
    pub next_continuous: bool,
    /// Compat 15: the section's footnote area passes to the next section on the page.
    pub hold_notes: bool,
    /// The body ends above an earlier section's page-bottom footnotes, which a line with a footnote reference can't
    /// join.
    pub notes_closed: bool,
    pub hf: HfHeights,
    /// The wrapping floats of the page's header and footer.
    pub hf_floats: Vec<super::float::Placed>,
    pub odd: bool,
    /// Paragraphs with `w:mirrorIndents` mirror on the page.
    pub mirror: bool,
}

pub type HfHeights = [Option<f64>; 2];

pub type BodyFields = super::text::FieldValues;
