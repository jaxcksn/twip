//! Authored table, row and cell formatting, independent of any file format: what one level (a table, a row's
//! exceptions to its table, a row, a cell, a table style region) sets, with unset and explicitly off kept apart.

use std::rc::Rc;

use super::{ForeignProps, Shading, TextBorder};
use crate::document::format::{Jc, Width};
use crate::document::section::{HeightRule, VerticalAlignment};
use crate::docx::props::Look;

/// Authored table properties at one level: a table's, or a row's exceptions to its table's. `None` means "not set
/// here". Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableProps {
    /// The table style.
    pub style: Option<Rc<str>>,
    /// Where a floating table sits; `None`: in the text flow.
    pub floating: Option<TableFloat>,
    /// Whether a floating table may overlap other floating tables.
    pub overlap: Option<TableOverlap>,
    /// The columns run right to left.
    pub bidi_visual: Option<bool>,
    /// How many rows a horizontal band of the table style spans.
    pub row_band: Option<i64>,
    /// How many columns a vertical band of the table style spans.
    pub col_band: Option<i64>,
    /// The preferred width.
    pub width: Option<Width>,
    /// The alignment between the margins: start, centre or end.
    pub align: Option<Jc>,
    /// The spacing between cells; a width other than points is none.
    pub cell_spacing: Option<Width>,
    /// The indent from the start margin.
    pub indent: Option<Width>,
    /// The borders, each set or not on its own.
    pub borders: TableBorders,
    /// The shading.
    pub shading: Option<Shading>,
    /// How column widths are found.
    pub layout: Option<TableLayout>,
    /// The cells' default margins, each set or not on its own.
    pub margins: CellMargins,
    /// Which conditional formats of the table style apply.
    pub look: Option<Look>,
    /// The title, for accessibility.
    pub caption: Option<Rc<str>>,
    /// The description, for accessibility.
    pub description: Option<Rc<str>>,
    /// What the source format wrote at this level that this struct does not model. Owned by the adapter that read
    /// it; never read by formatting or layout.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A floating table's position, as set: each anchor, offset, alignment and distance from text set or not on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TableFloat {
    /// What the vertical position measures from; `None`: the margin.
    pub v_anchor: Option<FloatAnchor>,
    /// What the horizontal position measures from; `None`: the text.
    pub h_anchor: Option<FloatAnchor>,
    /// The horizontal offset, points.
    pub x: Option<f64>,
    /// The horizontal alignment, over the offset.
    pub x_align: Option<FloatX>,
    /// The vertical offset, points.
    pub y: Option<f64>,
    /// The vertical alignment, over the offset.
    pub y_align: Option<FloatY>,
    /// The distance from the text above, points.
    pub top_from_text: Option<f64>,
    /// The distance from the text below, points.
    pub bottom_from_text: Option<f64>,
    /// The distance from the text at the start, points.
    pub left_from_text: Option<f64>,
    /// The distance from the text at the end, points.
    pub right_from_text: Option<f64>,
}

/// What a floating table's position measures from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatAnchor {
    /// The text: the paragraph (vertically) or the column (horizontally).
    Text,
    /// The margin.
    Margin,
    /// The page.
    Page,
}

/// A floating table's horizontal alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatX {
    /// At the left.
    Left,
    /// Centred.
    Center,
    /// At the right.
    Right,
    /// On the inside of facing pages.
    Inside,
    /// On the outside of facing pages.
    Outside,
}

/// A floating table's vertical alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatY {
    /// In line with the text.
    Inline,
    /// At the top.
    Top,
    /// Centred.
    Center,
    /// At the bottom.
    Bottom,
    /// On the inside.
    Inside,
    /// On the outside.
    Outside,
}

/// Whether a floating table may overlap other floating tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableOverlap {
    /// Never.
    Never,
    /// It may.
    Overlap,
}

/// How a table's column widths are found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableLayout {
    /// From the content.
    Autofit,
    /// From the widths set.
    Fixed,
}

/// A table's or a cell's borders, each set or not on its own. A side of no line (none or nil) is set: it takes the
/// place of a border a style or the table gives.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TableBorders {
    /// Above.
    pub top: Option<TextBorder>,
    /// At the start.
    pub left: Option<TextBorder>,
    /// Below.
    pub bottom: Option<TextBorder>,
    /// At the end.
    pub right: Option<TextBorder>,
    /// Between rows.
    pub inside_h: Option<TextBorder>,
    /// Between columns.
    pub inside_v: Option<TextBorder>,
    /// A cell's diagonal from its top left to its bottom right.
    pub tl2br: Option<TextBorder>,
    /// A cell's diagonal from its top right to its bottom left.
    pub tr2bl: Option<TextBorder>,
}

/// A side of a table's or a cell's borders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSide {
    /// Above.
    Top,
    /// At the start.
    Left,
    /// Below.
    Bottom,
    /// At the end.
    Right,
    /// Between rows.
    InsideH,
    /// Between columns.
    InsideV,
    /// The diagonal from the top left.
    Tl2br,
    /// The diagonal from the top right.
    Tr2bl,
}

impl TableSide {
    /// Every side, in schema order.
    pub const ALL: [TableSide; 8] = [
        TableSide::Top,
        TableSide::Left,
        TableSide::Bottom,
        TableSide::Right,
        TableSide::InsideH,
        TableSide::InsideV,
        TableSide::Tl2br,
        TableSide::Tr2bl,
    ];
}

impl TableBorders {
    /// Whether no border is set.
    pub fn is_empty(&self) -> bool {
        *self == TableBorders::default()
    }

    /// The border on `side`.
    pub fn side(&self, side: TableSide) -> &Option<TextBorder> {
        match side {
            TableSide::Top => &self.top,
            TableSide::Left => &self.left,
            TableSide::Bottom => &self.bottom,
            TableSide::Right => &self.right,
            TableSide::InsideH => &self.inside_h,
            TableSide::InsideV => &self.inside_v,
            TableSide::Tl2br => &self.tl2br,
            TableSide::Tr2bl => &self.tr2bl,
        }
    }

    /// The border on `side`, to set.
    pub fn side_mut(&mut self, side: TableSide) -> &mut Option<TextBorder> {
        match side {
            TableSide::Top => &mut self.top,
            TableSide::Left => &mut self.left,
            TableSide::Bottom => &mut self.bottom,
            TableSide::Right => &mut self.right,
            TableSide::InsideH => &mut self.inside_h,
            TableSide::InsideV => &mut self.inside_v,
            TableSide::Tl2br => &mut self.tl2br,
            TableSide::Tr2bl => &mut self.tr2bl,
        }
    }
}

/// A table's default cell margins or a cell's own, each side set or not on its own. A width other than points is a
/// margin of none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellMargins {
    /// Above.
    pub top: Option<Width>,
    /// At the start.
    pub left: Option<Width>,
    /// Below.
    pub bottom: Option<Width>,
    /// At the end.
    pub right: Option<Width>,
}

/// A side of a cell's margins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginSide {
    /// Above.
    Top,
    /// At the start.
    Left,
    /// Below.
    Bottom,
    /// At the end.
    Right,
}

impl MarginSide {
    /// Every side, in schema order.
    pub const ALL: [MarginSide; 4] = [MarginSide::Top, MarginSide::Left, MarginSide::Bottom, MarginSide::Right];
}

impl CellMargins {
    /// Whether no margin is set.
    pub fn is_empty(&self) -> bool {
        *self == CellMargins::default()
    }

    /// The margin on `side`.
    pub fn side(&self, side: MarginSide) -> &Option<Width> {
        match side {
            MarginSide::Top => &self.top,
            MarginSide::Left => &self.left,
            MarginSide::Bottom => &self.bottom,
            MarginSide::Right => &self.right,
        }
    }

    /// The margin on `side`, to set.
    pub fn side_mut(&mut self, side: MarginSide) -> &mut Option<Width> {
        match side {
            MarginSide::Top => &mut self.top,
            MarginSide::Left => &mut self.left,
            MarginSide::Bottom => &mut self.bottom,
            MarginSide::Right => &mut self.right,
        }
    }

    /// The margin on `side` in points, as set: a width other than points is none.
    pub fn points(&self, side: MarginSide) -> Option<f64> {
        self.side(side).map(Width::dxa)
    }
}

/// Authored row properties at one level. `None` means "not set here". Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowProps {
    /// The HTML division the row is in.
    pub division: Option<i64>,
    /// The grid columns before its first cell.
    pub grid_before: Option<i64>,
    /// The grid columns after its last cell.
    pub grid_after: Option<i64>,
    /// The preferred width of the grid columns before its first cell.
    pub width_before: Option<Width>,
    /// The preferred width of the grid columns after its last cell.
    pub width_after: Option<Width>,
    /// The row is kept on one page.
    pub cant_split: Option<bool>,
    /// The height and how it is measured.
    pub height: Option<RowHeight>,
    /// The row repeats at the top of each page as a header.
    pub header: Option<bool>,
    /// The spacing between its cells; a width other than points is none.
    pub cell_spacing: Option<Width>,
    /// Its alignment between the margins: start, centre or end.
    pub align: Option<Jc>,
    /// Its end mark is hidden.
    pub hidden: Option<bool>,
    /// What the source format wrote at this level that this struct does not model. Owned by the adapter that read
    /// it; never read by formatting or layout.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A row's height and how it is measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowHeight {
    /// The height, points.
    pub value: f64,
    /// How it is measured.
    pub rule: HeightRule,
}

/// Authored cell properties at one level. `None` means "not set here". Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellProps {
    /// The preferred width.
    pub width: Option<Width>,
    /// The grid columns it spans.
    pub grid_span: Option<i64>,
    /// Its part in cells merged across a row.
    pub h_merge: Option<MergeRole>,
    /// Its part in cells merged down a column.
    pub v_merge: Option<MergeRole>,
    /// The borders, each set or not on its own.
    pub borders: TableBorders,
    /// The shading.
    pub shading: Option<Shading>,
    /// Its text does not wrap.
    pub no_wrap: Option<bool>,
    /// Its margins, each set or not on its own.
    pub margins: CellMargins,
    /// The text flow: 0 left to right and top to bottom, else the flow's code (`docx::model::text_flow`).
    pub text_flow: Option<u8>,
    /// Its text is fitted to its width.
    pub fit_text: Option<bool>,
    /// The vertical alignment of its content.
    pub v_align: Option<VerticalAlignment>,
    /// Its end mark takes no height when the cell is empty.
    pub hide_mark: Option<bool>,
    /// What the source format wrote at this level that this struct does not model. Owned by the adapter that read
    /// it; never read by formatting or layout.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A cell's part in merged cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeRole {
    /// It starts them.
    Restart,
    /// It continues them.
    Continue,
}

/// A table property an edit sets, as a source format writes it: one written element, or one side of one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableProp {
    /// The style.
    Style,
    /// The floating position.
    Floating,
    /// Overlap.
    Overlap,
    /// Right-to-left columns.
    BidiVisual,
    /// The row band size.
    RowBand,
    /// The column band size.
    ColBand,
    /// The width.
    Width,
    /// The alignment.
    Align,
    /// The cell spacing.
    CellSpacing,
    /// The indent.
    Indent,
    /// The borders.
    Borders,
    /// One border, among the others as written.
    Border(TableSide),
    /// The shading.
    Shading,
    /// The layout.
    Layout,
    /// The default cell margins.
    Margins,
    /// One default cell margin, among the others as written.
    Margin(MarginSide),
    /// The look.
    Look,
    /// The title.
    Caption,
    /// The description.
    Description,
}

/// A table property an edit set: whole (written anew, whatever it was), or in part (among what is written with it,
/// which stays as written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableSet {
    /// The property.
    pub prop: TableProp,
    /// Set whole.
    pub whole: bool,
}

/// A row property an edit sets, as a source format writes it: one written element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowProp {
    /// The HTML division.
    Division,
    /// The grid columns before.
    GridBefore,
    /// The grid columns after.
    GridAfter,
    /// The width before.
    WidthBefore,
    /// The width after.
    WidthAfter,
    /// Kept on one page.
    CantSplit,
    /// The height.
    Height,
    /// Header row.
    Header,
    /// The cell spacing.
    CellSpacing,
    /// The alignment.
    Align,
    /// Hidden end mark.
    Hidden,
}

/// A row property an edit set: whole (written anew, whatever it was), or in part (among what is written with it,
/// which stays as written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowSet {
    /// The property.
    pub prop: RowProp,
    /// Set whole.
    pub whole: bool,
}

/// A cell property an edit sets, as a source format writes it: one written element, or one side of one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellProp {
    /// The width.
    Width,
    /// The grid span.
    GridSpan,
    /// The horizontal merge.
    HMerge,
    /// The vertical merge.
    VMerge,
    /// The borders.
    Borders,
    /// One border, among the others as written.
    Border(TableSide),
    /// The shading.
    Shading,
    /// No wrap.
    NoWrap,
    /// The margins.
    Margins,
    /// One margin, among the others as written.
    Margin(MarginSide),
    /// The text flow.
    TextFlow,
    /// Fit text.
    FitText,
    /// The vertical alignment.
    VAlign,
    /// The hidden end mark.
    HideMark,
}

/// A cell property an edit set: whole (written anew, whatever it was), or in part (among what is written with it,
/// which stays as written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSet {
    /// The property.
    pub prop: CellProp,
    /// Set whole.
    pub whole: bool,
}

/// Authored properties of one kind being edited, with what the edit set, in the order it set it: a source format
/// writes what was set again, the rest as it was written.
#[derive(Clone, Debug, Default)]
pub struct PropsEdit<P, S> {
    /// The properties.
    pub props: P,
    /// What was set, in order.
    pub set: Vec<S>,
}

/// Table properties being edited.
pub type TableEdit = PropsEdit<TableProps, TableSet>;

/// Row properties being edited.
pub type RowEdit = PropsEdit<RowProps, RowSet>;

/// Cell properties being edited.
pub type CellEdit = PropsEdit<CellProps, CellSet>;

impl<P, S> PropsEdit<P, S> {
    /// An edit of `props`.
    pub fn new(props: P) -> PropsEdit<P, S> {
        PropsEdit { props, set: Vec::new() }
    }
}

impl TableEdit {
    /// Notes `prop` set whole.
    pub fn whole(&mut self, prop: TableProp) {
        self.set.push(TableSet { prop, whole: true });
    }

    /// Notes `prop` set in part.
    pub fn part(&mut self, prop: TableProp) {
        self.set.push(TableSet { prop, whole: false });
    }

    /// Sets the border on `side`, among the others as written.
    pub fn border(&mut self, side: TableSide, b: Option<TextBorder>) {
        *self.props.borders.side_mut(side) = b;
        self.part(TableProp::Border(side));
    }

    /// Sets the default cell margin on `side`, among the others as written.
    pub fn margin(&mut self, side: MarginSide, m: Option<Width>) {
        *self.props.margins.side_mut(side) = m;
        self.part(TableProp::Margin(side));
    }
}

impl RowEdit {
    /// Notes `prop` set whole.
    pub fn whole(&mut self, prop: RowProp) {
        self.set.push(RowSet { prop, whole: true });
    }
}

impl CellEdit {
    /// Notes `prop` set whole.
    pub fn whole(&mut self, prop: CellProp) {
        self.set.push(CellSet { prop, whole: true });
    }

    /// Notes `prop` set in part.
    pub fn part(&mut self, prop: CellProp) {
        self.set.push(CellSet { prop, whole: false });
    }

    /// Sets the border on `side`, among the others as written.
    pub fn border(&mut self, side: TableSide, b: Option<TextBorder>) {
        *self.props.borders.side_mut(side) = b;
        self.part(CellProp::Border(side));
    }

    /// Sets the margin on `side`, among the others as written.
    pub fn margin(&mut self, side: MarginSide, m: Option<Width>) {
        *self.props.margins.side_mut(side) = m;
        self.part(CellProp::Margin(side));
    }
}

impl<P, S> std::ops::Deref for PropsEdit<P, S> {
    type Target = P;
    fn deref(&self) -> &P {
        &self.props
    }
}

impl<P, S> std::ops::DerefMut for PropsEdit<P, S> {
    fn deref_mut(&mut self) -> &mut P {
        &mut self.props
    }
}
