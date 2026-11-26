//! Typed table, row and cell properties from and to `w:tblPr`, `w:tblPrEx`, `w:trPr` and `w:tcPr`
//!
//! As for paragraph properties ([`super::para_props`]), the parse keeps in [`ForeignProps`] what the typed value does
//! not hold, so that writing the value again loses nothing of what was read: unmodelled children whole, the other
//! duplicates of a modelled child (the last one counts, but the first for those the reader reads the first of: a
//! row's division and hidden mark, a cell's hidden mark, a table's title and description), a modelled child its
//! value doesn't keep whole, unmodelled attributes of modelled children and of the element itself, modelled
//! attributes written otherwise than the writer writes their value, and the children with sides (borders, margins)
//! whole beside their typed value.
//!
//! Children another node owns are not part of the typed value: the tracked property changes (`w:tblPrChange`,
//! `w:tblPrExChange`, `w:trPrChange`, `w:tcPrChange`), a row's tracked insertion and deletion (`w:ins`, `w:del`) and
//! a cell's (`w:cellIns`, `w:cellDel`, `w:cellMerge`). The serializer writes them where the schema places them when
//! it is given them; an edit written over the element as read keeps them as they are there.

use std::fmt::{Display, Write as _};
use std::rc::Rc;

use roxmltree::Node;

use super::prop_xml::{Compare, FILL, LINE_COLOUR, Out, Sink, Twips, border, colour, colour_attrs, foreign_child, props_key};
use super::props::{self as xml_props, Look, TableAnchor, TablePos, TblProps, TcProps, ThemeColours, TrProps};
use super::write::props::{Cx, Props};
use super::xml::{W, attr, attr_twips, child, int, on_off, twips, val};
use crate::document::format::{Jc, ShadingPattern, Width};
use crate::document::line::BorderStyle;
use crate::document::section::HeightRule;
use crate::format::{
    CellEdit, CellMargins, CellProp, CellProps, CellSet, Color, FloatAnchor, FloatX, FloatY, ForeignProps, MarginSide, MergeRole, QName, RowEdit,
    RowHeight, RowProp, RowProps, RowSet, Shading, TableBorders, TableEdit, TableFloat, TableLayout, TableOverlap, TableProp, TableProps, TableSet,
    TableSide, TextBorder,
};

// Kinds

/// Where a set property is written: the child, whether it was set whole, and the side it set in part.
struct Place {
    key: &'static str,
    whole: bool,
    side: Option<Side>,
}

/// A side of a child with sides.
#[derive(Clone, Copy)]
enum Side {
    Border(TableSide),
    Margin(MarginSide),
}

/// A table properties element: its name, its children in schema order, those other nodes own, and how its typed
/// properties are read and written.
trait Kind {
    type Props: Clone + Default + PartialEq + std::fmt::Debug;
    type Set: Copy + std::fmt::Debug;
    const NAME: &'static str;
    const ORDER: &'static [&'static str];
    const OWNED: &'static [&'static str];
    /// The modelled children read from the first of their name, as the reader reads them; the others from the last.
    const FIRST: &'static [&'static str];
    /// The child holding the borders, if it has one.
    const BORDERS: &'static str;
    /// The child holding the cell margins, if it has one.
    const MARGINS: &'static str;
    fn foreign(p: &Self::Props) -> Option<&Rc<ForeignProps>>;
    fn foreign_mut(p: &mut Self::Props) -> &mut Option<Rc<ForeignProps>>;
    fn borders(p: &Self::Props) -> &TableBorders;
    fn margins(p: &Self::Props) -> &CellMargins;
    /// Reads modelled child `key` into `p`, as the XML path reads it; false when it isn't modelled.
    fn read(p: &mut Self::Props, e: Node, key: &str) -> bool;
    /// Writes the attributes of modelled child `key` of `p` to `s`; false when `p` doesn't set it.
    fn emit(p: &Self::Props, key: &str, s: &mut dyn Sink) -> bool;
    fn place(s: Self::Set) -> Place;
    /// The property set whole in child `key`, or in part on its `side`.
    #[cfg(test)]
    fn set_of(key: &str, side: Option<Side>) -> Option<Self::Set>;
}

/// `w:tblPr`.
struct Tbl;
/// `w:tblPrEx`.
struct TblEx;
/// `w:trPr`.
struct Tr;
/// `w:tcPr`.
struct Tc;

/// CT_TblPr's children in schema order.
const TBL_ORDER: [&str; 18] = [
    "tblStyle",
    "tblpPr",
    "tblOverlap",
    "bidiVisual",
    "tblStyleRowBandSize",
    "tblStyleColBandSize",
    "tblW",
    "jc",
    "tblCellSpacing",
    "tblInd",
    "tblBorders",
    "shd",
    "tblLayout",
    "tblCellMar",
    "tblLook",
    "tblCaption",
    "tblDescription",
    "tblPrChange",
];

/// CT_TblPrEx's children in schema order.
const EX_ORDER: [&str; 10] = ["tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblPrExChange"];

/// CT_TrPr's children in the order the schema lists them (any order reads).
const TR_ORDER: [&str; 15] = [
    "cnfStyle",
    "divId",
    "gridBefore",
    "gridAfter",
    "wBefore",
    "wAfter",
    "cantSplit",
    "trHeight",
    "tblHeader",
    "tblCellSpacing",
    "jc",
    "hidden",
    "ins",
    "del",
    "trPrChange",
];

/// CT_TcPr's children in schema order.
const TC_ORDER: [&str; 18] = [
    "cnfStyle",
    "tcW",
    "gridSpan",
    "hMerge",
    "vMerge",
    "tcBorders",
    "shd",
    "noWrap",
    "tcMar",
    "textDirection",
    "tcFitText",
    "vAlign",
    "hideMark",
    "headers",
    "cellIns",
    "cellDel",
    "cellMerge",
    "tcPrChange",
];

/// The longest of the orders.
const MAX_ORDER: usize = 18;

const _: () = assert!(TBL_ORDER.len() <= MAX_ORDER && EX_ORDER.len() <= MAX_ORDER && TR_ORDER.len() <= MAX_ORDER && TC_ORDER.len() <= MAX_ORDER);

/// A border element's sides in schema order, either name of the start and end sides among them.
const BORDER_ORDER: &[&str] = &["top", "start", "left", "bottom", "end", "right", "insideH", "insideV", "tl2br", "tr2bl"];

/// A margins element's sides in schema order.
const MARGIN_ORDER: &[&str] = &["top", "start", "left", "bottom", "end", "right"];

/// The name a border side is written with, and the other name it may be read under.
fn border_name(side: TableSide) -> (&'static str, Option<&'static str>) {
    match side {
        TableSide::Top => ("top", None),
        TableSide::Left => ("left", Some("start")),
        TableSide::Bottom => ("bottom", None),
        TableSide::Right => ("right", Some("end")),
        TableSide::InsideH => ("insideH", None),
        TableSide::InsideV => ("insideV", None),
        TableSide::Tl2br => ("tl2br", None),
        TableSide::Tr2bl => ("tr2bl", None),
    }
}

/// The name a margin side is written with, and the other name it may be read under.
fn margin_name(side: MarginSide) -> (&'static str, Option<&'static str>) {
    match side {
        MarginSide::Top => ("top", None),
        MarginSide::Left => ("left", Some("start")),
        MarginSide::Bottom => ("bottom", None),
        MarginSide::Right => ("right", Some("end")),
    }
}

const NO_BORDERS: TableBorders =
    TableBorders { top: None, left: None, bottom: None, right: None, inside_h: None, inside_v: None, tl2br: None, tr2bl: None };

const NO_MARGINS: CellMargins = CellMargins { top: None, left: None, bottom: None, right: None };

impl Kind for Tbl {
    type Props = TableProps;
    type Set = TableSet;
    const NAME: &'static str = "tblPr";
    const ORDER: &'static [&'static str] = &TBL_ORDER;
    const OWNED: &'static [&'static str] = &["tblPrChange"];
    const FIRST: &'static [&'static str] = &["tblCaption", "tblDescription"];
    const BORDERS: &'static str = "tblBorders";
    const MARGINS: &'static str = "tblCellMar";
    fn foreign(p: &TableProps) -> Option<&Rc<ForeignProps>> {
        p.foreign.as_ref()
    }
    fn foreign_mut(p: &mut TableProps) -> &mut Option<Rc<ForeignProps>> {
        &mut p.foreign
    }
    fn borders(p: &TableProps) -> &TableBorders {
        &p.borders
    }
    fn margins(p: &TableProps) -> &CellMargins {
        &p.margins
    }
    fn read(p: &mut TableProps, e: Node, key: &str) -> bool {
        read_table(p, e, key)
    }
    fn emit(p: &TableProps, key: &str, s: &mut dyn Sink) -> bool {
        emit_table(p, key, s)
    }
    fn place(s: TableSet) -> Place {
        table_place(s)
    }
    #[cfg(test)]
    fn set_of(key: &str, side: Option<Side>) -> Option<TableSet> {
        table_set_of(key, side)
    }
}

impl Kind for TblEx {
    type Props = TableProps;
    type Set = TableSet;
    const NAME: &'static str = "tblPrEx";
    const ORDER: &'static [&'static str] = &EX_ORDER;
    const OWNED: &'static [&'static str] = &["tblPrExChange"];
    const FIRST: &'static [&'static str] = &[];
    const BORDERS: &'static str = "tblBorders";
    const MARGINS: &'static str = "tblCellMar";
    fn foreign(p: &TableProps) -> Option<&Rc<ForeignProps>> {
        p.foreign.as_ref()
    }
    fn foreign_mut(p: &mut TableProps) -> &mut Option<Rc<ForeignProps>> {
        &mut p.foreign
    }
    fn borders(p: &TableProps) -> &TableBorders {
        &p.borders
    }
    fn margins(p: &TableProps) -> &CellMargins {
        &p.margins
    }
    fn read(p: &mut TableProps, e: Node, key: &str) -> bool {
        read_table(p, e, key)
    }
    fn emit(p: &TableProps, key: &str, s: &mut dyn Sink) -> bool {
        emit_table(p, key, s)
    }
    fn place(s: TableSet) -> Place {
        table_place(s)
    }
    #[cfg(test)]
    fn set_of(key: &str, side: Option<Side>) -> Option<TableSet> {
        table_set_of(key, side)
    }
}

impl Kind for Tr {
    type Props = RowProps;
    type Set = RowSet;
    const NAME: &'static str = "trPr";
    const ORDER: &'static [&'static str] = &TR_ORDER;
    const OWNED: &'static [&'static str] = &["ins", "del", "trPrChange"];
    const FIRST: &'static [&'static str] = &["divId", "hidden"];
    const BORDERS: &'static str = "";
    const MARGINS: &'static str = "";
    fn foreign(p: &RowProps) -> Option<&Rc<ForeignProps>> {
        p.foreign.as_ref()
    }
    fn foreign_mut(p: &mut RowProps) -> &mut Option<Rc<ForeignProps>> {
        &mut p.foreign
    }
    fn borders(_: &RowProps) -> &TableBorders {
        &NO_BORDERS
    }
    fn margins(_: &RowProps) -> &CellMargins {
        &NO_MARGINS
    }
    fn read(p: &mut RowProps, e: Node, key: &str) -> bool {
        match key {
            "divId" => p.division = int(val(e)),
            "gridBefore" => p.grid_before = int(val(e)),
            "gridAfter" => p.grid_after = int(val(e)),
            "wBefore" => p.width_before = Some(xml_props::width(e)),
            "wAfter" => p.width_after = Some(xml_props::width(e)),
            "cantSplit" => p.cant_split = Some(on_off(e)),
            "trHeight" => {
                let rule = attr(e, "hRule").map_or(HeightRule::AtLeast, super::section::height_rule);
                p.height = Some(RowHeight { value: twips(val(e)).unwrap_or(0.0), rule });
            }
            "tblHeader" => p.header = Some(on_off(e)),
            "tblCellSpacing" => p.cell_spacing = Some(xml_props::width(e)),
            "jc" => p.align = Some(table_jc(val(e))),
            "hidden" => p.hidden = Some(on_off(e)),
            _ => return false,
        }
        true
    }
    fn emit(p: &RowProps, key: &str, s: &mut dyn Sink) -> bool {
        match key {
            "divId" => p.division.map(|v| s.attr(W, "val", &v)).is_some(),
            "gridBefore" => p.grid_before.map(|v| s.attr(W, "val", &v)).is_some(),
            "gridAfter" => p.grid_after.map(|v| s.attr(W, "val", &v)).is_some(),
            "wBefore" => p.width_before.map(|w| width_attrs(s, w)).is_some(),
            "wAfter" => p.width_after.map(|w| width_attrs(s, w)).is_some(),
            "cantSplit" => on_off_attrs(s, p.cant_split),
            "trHeight" => p
                .height
                .map(|h| {
                    s.attr(W, "val", &Twips(h.value));
                    s.attr(W, "hRule", &super::section::height_rule_literal(h.rule));
                })
                .is_some(),
            "tblHeader" => on_off_attrs(s, p.header),
            "tblCellSpacing" => p.cell_spacing.map(|w| width_attrs(s, w)).is_some(),
            "jc" => p.align.map(|j| s.attr(W, "val", &jc_val(j))).is_some(),
            "hidden" => on_off_attrs(s, p.hidden),
            _ => false,
        }
    }
    fn place(s: RowSet) -> Place {
        let key = match s.prop {
            RowProp::Division => "divId",
            RowProp::GridBefore => "gridBefore",
            RowProp::GridAfter => "gridAfter",
            RowProp::WidthBefore => "wBefore",
            RowProp::WidthAfter => "wAfter",
            RowProp::CantSplit => "cantSplit",
            RowProp::Height => "trHeight",
            RowProp::Header => "tblHeader",
            RowProp::CellSpacing => "tblCellSpacing",
            RowProp::Align => "jc",
            RowProp::Hidden => "hidden",
        };
        Place { key, whole: s.whole, side: None }
    }
    #[cfg(test)]
    fn set_of(key: &str, _: Option<Side>) -> Option<RowSet> {
        use RowProp::*;
        [Division, GridBefore, GridAfter, WidthBefore, WidthAfter, CantSplit, Height, Header, CellSpacing, Align, Hidden]
            .into_iter()
            .map(|prop| RowSet { prop, whole: true })
            .find(|s| Self::place(*s).key == key)
    }
}

impl Kind for Tc {
    type Props = CellProps;
    type Set = CellSet;
    const NAME: &'static str = "tcPr";
    const ORDER: &'static [&'static str] = &TC_ORDER;
    const OWNED: &'static [&'static str] = &["cellIns", "cellDel", "cellMerge", "tcPrChange"];
    const FIRST: &'static [&'static str] = &["hideMark"];
    const BORDERS: &'static str = "tcBorders";
    const MARGINS: &'static str = "tcMar";
    fn foreign(p: &CellProps) -> Option<&Rc<ForeignProps>> {
        p.foreign.as_ref()
    }
    fn foreign_mut(p: &mut CellProps) -> &mut Option<Rc<ForeignProps>> {
        &mut p.foreign
    }
    fn borders(p: &CellProps) -> &TableBorders {
        &p.borders
    }
    fn margins(p: &CellProps) -> &CellMargins {
        &p.margins
    }
    fn read(p: &mut CellProps, e: Node, key: &str) -> bool {
        let merge = |e: Node| Some(if val(e) == Some("restart") { MergeRole::Restart } else { MergeRole::Continue });
        match key {
            "tcW" => p.width = Some(xml_props::width(e)),
            "gridSpan" => p.grid_span = int(val(e)),
            "hMerge" => p.h_merge = merge(e),
            "vMerge" => p.v_merge = merge(e),
            "tcBorders" => p.borders = borders(e),
            "shd" => p.shading = Some(shading(e)),
            "noWrap" => p.no_wrap = Some(on_off(e)),
            "tcMar" => p.margins = margins(e),
            "textDirection" => p.text_flow = val(e).map(super::model::text_flow),
            "tcFitText" => p.fit_text = Some(on_off(e)),
            "vAlign" => p.v_align = val(e).map(super::section::cell_v_align),
            "hideMark" => p.hide_mark = Some(on_off(e)),
            _ => return false,
        }
        true
    }
    fn emit(p: &CellProps, key: &str, s: &mut dyn Sink) -> bool {
        let merge = |s: &mut dyn Sink, m: Option<MergeRole>| {
            m.map(|m| match m {
                MergeRole::Restart => s.attr(W, "val", &"restart"),
                MergeRole::Continue => s.implied("val", "continue"),
            })
            .is_some()
        };
        match key {
            "tcW" => p.width.map(|w| width_attrs(s, w)).is_some(),
            "gridSpan" => p.grid_span.map(|v| s.attr(W, "val", &v)).is_some(),
            "hMerge" => merge(s, p.h_merge),
            "vMerge" => merge(s, p.v_merge),
            "shd" => p.shading.map(|sh| shading_attrs(s, &sh)).is_some(),
            "noWrap" => on_off_attrs(s, p.no_wrap),
            "textDirection" => p.text_flow.map(|c| s.attr(W, "val", &flow_literal(c))).is_some(),
            "tcFitText" => on_off_attrs(s, p.fit_text),
            "vAlign" => p.v_align.map(|v| s.attr(W, "val", &super::section::v_align_literal(v))).is_some(),
            "hideMark" => on_off_attrs(s, p.hide_mark),
            _ => false,
        }
    }
    fn place(s: CellSet) -> Place {
        let (key, side) = match s.prop {
            CellProp::Width => ("tcW", None),
            CellProp::GridSpan => ("gridSpan", None),
            CellProp::HMerge => ("hMerge", None),
            CellProp::VMerge => ("vMerge", None),
            CellProp::Borders => ("tcBorders", None),
            CellProp::Border(b) => ("tcBorders", Some(Side::Border(b))),
            CellProp::Shading => ("shd", None),
            CellProp::NoWrap => ("noWrap", None),
            CellProp::Margins => ("tcMar", None),
            CellProp::Margin(m) => ("tcMar", Some(Side::Margin(m))),
            CellProp::TextFlow => ("textDirection", None),
            CellProp::FitText => ("tcFitText", None),
            CellProp::VAlign => ("vAlign", None),
            CellProp::HideMark => ("hideMark", None),
        };
        Place { key, whole: s.whole, side }
    }
    #[cfg(test)]
    fn set_of(key: &str, side: Option<Side>) -> Option<CellSet> {
        use CellProp::*;
        let prop = match (key, side) {
            ("tcBorders", Some(Side::Border(b))) => Border(b),
            ("tcMar", Some(Side::Margin(m))) => Margin(m),
            _ => [Width, GridSpan, HMerge, VMerge, Borders, Shading, NoWrap, Margins, TextFlow, FitText, VAlign, HideMark]
                .into_iter()
                .find(|p| Self::place(CellSet { prop: *p, whole: true }).key == key)?,
        };
        Some(CellSet { prop, whole: side.is_none() })
    }
}

#[cfg(test)]
fn table_set_of(key: &str, side: Option<Side>) -> Option<TableSet> {
    use TableProp::*;
    let prop = match (key, side) {
        ("tblBorders", Some(Side::Border(b))) => Border(b),
        ("tblCellMar", Some(Side::Margin(m))) => Margin(m),
        _ => [
            Style,
            Floating,
            Overlap,
            BidiVisual,
            RowBand,
            ColBand,
            Width,
            Align,
            CellSpacing,
            Indent,
            Borders,
            Shading,
            Layout,
            Margins,
            Look,
            Caption,
            Description,
        ]
        .into_iter()
        .find(|p| table_place(TableSet { prop: *p, whole: true }).key == key)?,
    };
    Some(TableSet { prop, whole: side.is_none() })
}

fn table_place(s: TableSet) -> Place {
    let (key, side) = match s.prop {
        TableProp::Style => ("tblStyle", None),
        TableProp::Floating => ("tblpPr", None),
        TableProp::Overlap => ("tblOverlap", None),
        TableProp::BidiVisual => ("bidiVisual", None),
        TableProp::RowBand => ("tblStyleRowBandSize", None),
        TableProp::ColBand => ("tblStyleColBandSize", None),
        TableProp::Width => ("tblW", None),
        TableProp::Align => ("jc", None),
        TableProp::CellSpacing => ("tblCellSpacing", None),
        TableProp::Indent => ("tblInd", None),
        TableProp::Borders => ("tblBorders", None),
        TableProp::Border(b) => ("tblBorders", Some(Side::Border(b))),
        TableProp::Shading => ("shd", None),
        TableProp::Layout => ("tblLayout", None),
        TableProp::Margins => ("tblCellMar", None),
        TableProp::Margin(m) => ("tblCellMar", Some(Side::Margin(m))),
        TableProp::Look => ("tblLook", None),
        TableProp::Caption => ("tblCaption", None),
        TableProp::Description => ("tblDescription", None),
    };
    Place { key, whole: s.whole, side }
}

// Parse

/// A table's or a row's exceptions' child into `p`, as the XML path reads it; false when it isn't modelled.
fn read_table(p: &mut TableProps, e: Node, key: &str) -> bool {
    match key {
        "tblStyle" => p.style = val(e).map(Rc::from),
        "tblpPr" => p.floating = Some(float(e)),
        "tblOverlap" => p.overlap = Some(if val(e) == Some("never") { TableOverlap::Never } else { TableOverlap::Overlap }),
        "bidiVisual" => p.bidi_visual = Some(on_off(e)),
        "tblStyleRowBandSize" => p.row_band = int(val(e)),
        "tblStyleColBandSize" => p.col_band = int(val(e)),
        "tblW" => p.width = Some(xml_props::width(e)),
        "jc" => p.align = Some(table_jc(val(e))),
        "tblCellSpacing" => p.cell_spacing = Some(xml_props::width(e)),
        "tblInd" => p.indent = Some(xml_props::width(e)),
        "tblBorders" => p.borders = borders(e),
        "shd" => p.shading = Some(shading(e)),
        "tblLayout" => p.layout = Some(if attr(e, "type") == Some("fixed") { TableLayout::Fixed } else { TableLayout::Autofit }),
        "tblCellMar" => p.margins = margins(e),
        "tblLook" => p.look = Some(look(e)),
        "tblCaption" => p.caption = val(e).map(Rc::from),
        "tblDescription" => p.description = val(e).map(Rc::from),
        _ => return false,
    }
    true
}

/// A table's or a row's alignment: start, centre or end.
fn table_jc(v: Option<&str>) -> Jc {
    match v {
        Some("center") => Jc::Center,
        Some("right" | "end") => Jc::Right,
        _ => Jc::Left,
    }
}

fn shading(e: Node) -> Shading {
    let pattern = val(e).map_or(ShadingPattern::Clear, super::line_style::shading_pattern);
    Shading { pattern, color: colour(e, LINE_COLOUR), fill: colour(e, FILL) }
}

/// A `w:tblBorders` or `w:tcBorders`: each side the first of its name, a start or end side the first under either.
fn borders(e: Node) -> TableBorders {
    let side = |name| child(e, name).map(border);
    TableBorders {
        top: side("top"),
        left: side("left").or_else(|| side("start")),
        bottom: side("bottom"),
        right: side("right").or_else(|| side("end")),
        inside_h: side("insideH"),
        inside_v: side("insideV"),
        tl2br: side("tl2br"),
        tr2bl: side("tr2bl"),
    }
}

/// A `w:tblCellMar` or `w:tcMar`, as [`borders`] reads its sides.
fn margins(e: Node) -> CellMargins {
    let side = |name| child(e, name).map(xml_props::width);
    CellMargins {
        top: side("top"),
        left: side("left").or_else(|| side("start")),
        bottom: side("bottom"),
        right: side("right").or_else(|| side("end")),
    }
}

/// The conditional formats `w:tblLook`'s bits give, each attribute that names one over the bit.
const LOOK_BITS: [(&str, i64); 6] =
    [("firstRow", 0x20), ("lastRow", 0x40), ("firstColumn", 0x80), ("lastColumn", 0x100), ("noHBand", 0x200), ("noVBand", 0x400)];

fn look(e: Node) -> Look {
    let bits = i64::from_str_radix(val(e).unwrap_or("0"), 16).unwrap_or(0);
    let [first_row, last_row, first_col, last_col, no_h_band, no_v_band] = LOOK_BITS.map(|(name, mask)| match attr(e, name) {
        Some(v) => super::xml::on_off_str(Some(v)),
        None => bits & mask != 0,
    });
    Look { first_row, last_row, first_col, last_col, no_h_band, no_v_band }
}

fn look_flags(l: &Look) -> [bool; 6] {
    [l.first_row, l.last_row, l.first_col, l.last_col, l.no_h_band, l.no_v_band]
}

/// A `w:tblpPr`, each attribute as set; an unknown anchor or alignment reads as the XML path takes it.
fn float(e: Node) -> TableFloat {
    TableFloat {
        v_anchor: attr(e, "vertAnchor").map(|v| match v {
            "page" => FloatAnchor::Page,
            "text" => FloatAnchor::Text,
            _ => FloatAnchor::Margin,
        }),
        h_anchor: attr(e, "horzAnchor").map(|v| match v {
            "page" => FloatAnchor::Page,
            "margin" => FloatAnchor::Margin,
            _ => FloatAnchor::Text,
        }),
        x: attr_twips(e, "tblpX"),
        x_align: attr(e, "tblpXSpec").map(|v| match v {
            "center" => FloatX::Center,
            "right" => FloatX::Right,
            "inside" => FloatX::Inside,
            "outside" => FloatX::Outside,
            _ => FloatX::Left,
        }),
        y: attr_twips(e, "tblpY"),
        y_align: attr(e, "tblpYSpec").map(|v| match v {
            "inline" => FloatY::Inline,
            "center" => FloatY::Center,
            "bottom" => FloatY::Bottom,
            "inside" => FloatY::Inside,
            "outside" => FloatY::Outside,
            _ => FloatY::Top,
        }),
        top_from_text: attr_twips(e, "topFromText"),
        bottom_from_text: attr_twips(e, "bottomFromText"),
        left_from_text: attr_twips(e, "leftFromText"),
        right_from_text: attr_twips(e, "rightFromText"),
    }
}

/// The place in `K`'s order of an element's name, if it has one.
fn rank_of<K: Kind>(ns: &str, local: &str) -> Option<usize> {
    if ns != W {
        return None;
    }
    K::ORDER.iter().position(|k| *k == local)
}

/// A properties element of kind `K` as typed properties, without the children other nodes own.
fn parse<K: Kind>(el: Node) -> K::Props {
    let mut p = K::Props::default();
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    Compare::new(el, &mut scratch).finish(None, &mut f.attributes);
    let elements = || el.children().filter(|c| c.is_element());
    // The child each name is read from: the last, but the first of those other nodes own and of those read first.
    let mut pick = [None::<usize>; MAX_ORDER];
    for (i, e) in elements().enumerate() {
        if let Some(k) = rank_of::<K>(e.tag_name().namespace().unwrap_or_default(), e.tag_name().name())
            && (!(K::OWNED.contains(&K::ORDER[k]) || K::FIRST.contains(&K::ORDER[k])) || pick[k].is_none())
        {
            pick[k] = Some(i);
        }
    }
    for (i, e) in elements().enumerate() {
        let (ns, name) = (e.tag_name().namespace().unwrap_or_default(), e.tag_name().name());
        let Some(key) = rank_of::<K>(ns, name).filter(|k| pick[*k] == Some(i)).map(|k| K::ORDER[k]) else {
            f.children.push(foreign_child(e));
            continue;
        };
        if K::OWNED.contains(&key) {
            continue;
        }
        if !K::read(&mut p, e, key) {
            f.children.push(foreign_child(e));
            continue;
        }
        if key == K::BORDERS || key == K::MARGINS {
            f.kept.push(foreign_child(e));
            continue;
        }
        let mut cmp = Compare::new(e, &mut scratch);
        if !K::emit(&p, key, &mut cmp) {
            f.children.push(foreign_child(e));
            continue;
        }
        cmp.finish(Some(QName { ns: ns.into(), local: name.into() }), &mut f.attributes);
        if e.children().any(|c| c.is_element() || c.text().is_some_and(|t| !t.trim().is_empty())) {
            f.kept.push(foreign_child(e));
        }
    }
    if f != ForeignProps::default() {
        *K::foreign_mut(&mut p) = Some(Rc::new(f));
    }
    p
}

// The attributes a modelled child is written with

/// Writes the attributes of modelled child `key` of `p`, a child with sides among them; false when `p` doesn't set it.
fn emit<K: Kind>(p: &K::Props, key: &str, s: &mut dyn Sink) -> bool {
    if key == K::BORDERS {
        return !K::borders(p).is_empty();
    }
    if key == K::MARGINS {
        return !K::margins(p).is_empty();
    }
    K::emit(p, key, s)
}

fn emit_table(p: &TableProps, key: &str, s: &mut dyn Sink) -> bool {
    let val = |s: &mut dyn Sink, v: &dyn Display| s.attr(W, "val", v);
    match key {
        "tblStyle" => p.style.as_deref().map(|v| val(s, &v)).is_some(),
        "tblpPr" => p.floating.map(|f| float_attrs(s, &f)).is_some(),
        "tblOverlap" => p
            .overlap
            .map(|o| {
                val(
                    s,
                    &match o {
                        TableOverlap::Never => "never",
                        TableOverlap::Overlap => "overlap",
                    },
                )
            })
            .is_some(),
        "bidiVisual" => on_off_attrs(s, p.bidi_visual),
        "tblStyleRowBandSize" => p.row_band.map(|v| val(s, &v)).is_some(),
        "tblStyleColBandSize" => p.col_band.map(|v| val(s, &v)).is_some(),
        "tblW" => p.width.map(|w| width_attrs(s, w)).is_some(),
        "jc" => p.align.map(|j| val(s, &jc_val(j))).is_some(),
        "tblCellSpacing" => p.cell_spacing.map(|w| width_attrs(s, w)).is_some(),
        "tblInd" => p.indent.map(|w| width_attrs(s, w)).is_some(),
        "shd" => p.shading.map(|sh| shading_attrs(s, &sh)).is_some(),
        "tblLayout" => p
            .layout
            .map(|l| {
                s.attr(
                    W,
                    "type",
                    &match l {
                        TableLayout::Fixed => "fixed",
                        TableLayout::Autofit => "autofit",
                    },
                )
            })
            .is_some(),
        "tblLook" => p
            .look
            .map(|l| {
                let flags = look_flags(&l);
                let bits: i64 = LOOK_BITS.iter().zip(flags).filter(|(_, on)| *on).map(|((_, m), _)| m).sum();
                val(s, &format_args!("{bits:04X}"));
                for ((name, _), on) in LOOK_BITS.iter().zip(flags) {
                    s.attr(W, name, &if on { "1" } else { "0" });
                }
            })
            .is_some(),
        "tblCaption" => p.caption.as_deref().map(|v| val(s, &v)).is_some(),
        "tblDescription" => p.description.as_deref().map(|v| val(s, &v)).is_some(),
        _ => false,
    }
}

/// An on/off child: on as its value left out, off as `0`.
fn on_off_attrs(s: &mut dyn Sink, v: Option<bool>) -> bool {
    match v {
        Some(true) => s.implied("val", "1"),
        Some(false) => s.attr(W, "val", &"0"),
        None => return false,
    }
    true
}

fn jc_val(j: Jc) -> &'static str {
    match j {
        Jc::Center => "center",
        Jc::Right => "right",
        _ => "left",
    }
}

/// A fraction as fiftieths of a percent, or as a percentage when they can't hold it exactly.
struct Pct(f64);

impl Display for Pct {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let n = self.0 * 5000.0;
        if (n - n.round()).abs() < 1e-6 { write!(f, "{}", n.round()) } else { write!(f, "{}%", self.0 * 100.0) }
    }
}

/// A table width's attributes.
fn width_attrs(s: &mut dyn Sink, w: Width) {
    match w {
        Width::Auto => {
            s.attr(W, "w", &0);
            s.attr(W, "type", &"auto");
        }
        Width::Nil => {
            s.attr(W, "w", &0);
            s.attr(W, "type", &"nil");
        }
        Width::Dxa(v) => {
            s.attr(W, "w", &Twips(v));
            s.attr(W, "type", &"dxa");
        }
        Width::Pct(v) => {
            s.attr(W, "w", &Pct(v));
            s.attr(W, "type", &"pct");
        }
    }
}

fn shading_attrs(s: &mut dyn Sink, sh: &Shading) {
    s.attr(W, "val", &super::line_style::shading_val(sh.pattern));
    if let Some(c) = sh.color {
        colour_attrs(s, c, LINE_COLOUR);
    }
    if let Some(c) = sh.fill {
        colour_attrs(s, c, FILL);
    }
}

/// A table or cell border side's attributes, as the table commands write them: whole eighths of a point and points, a
/// side of no line written as its line alone.
fn side_attrs(s: &mut dyn Sink, b: &TextBorder) {
    s.attr(W, "val", &super::line_style::border_val(b.style));
    let lined = !matches!(b.style, BorderStyle::None | BorderStyle::Nil);
    if lined || b.width != 0.0 {
        s.attr(W, "sz", &((b.width * 8.0).round() as i64));
    } else {
        s.implied("sz", "0");
    }
    if lined || b.space != 0.0 {
        s.attr(W, "space", &(b.space.round() as i64));
    } else {
        s.implied("space", "0");
    }
    if b.shadow {
        s.attr(W, "shadow", &1);
    } else {
        s.implied("shadow", "0");
    }
    if let Some(c) = b.color {
        colour_attrs(s, c, LINE_COLOUR);
    }
}

/// A floating table's attributes, in the order Word writes them.
fn float_attrs(s: &mut dyn Sink, f: &TableFloat) {
    let length = |s: &mut dyn Sink, local: &'static str, v: Option<f64>| {
        if let Some(v) = v {
            s.attr(W, local, &Twips(v));
        }
    };
    let anchor = |a: FloatAnchor| match a {
        FloatAnchor::Text => "text",
        FloatAnchor::Margin => "margin",
        FloatAnchor::Page => "page",
    };
    length(s, "leftFromText", f.left_from_text);
    length(s, "rightFromText", f.right_from_text);
    length(s, "topFromText", f.top_from_text);
    length(s, "bottomFromText", f.bottom_from_text);
    if let Some(a) = f.v_anchor {
        s.attr(W, "vertAnchor", &anchor(a));
    }
    if let Some(a) = f.h_anchor {
        s.attr(W, "horzAnchor", &anchor(a));
    }
    if let Some(x) = f.x_align {
        let v = match x {
            FloatX::Left => "left",
            FloatX::Center => "center",
            FloatX::Right => "right",
            FloatX::Inside => "inside",
            FloatX::Outside => "outside",
        };
        s.attr(W, "tblpXSpec", &v);
    }
    length(s, "tblpX", f.x);
    if let Some(y) = f.y_align {
        let v = match y {
            FloatY::Inline => "inline",
            FloatY::Top => "top",
            FloatY::Center => "center",
            FloatY::Bottom => "bottom",
            FloatY::Inside => "inside",
            FloatY::Outside => "outside",
        };
        s.attr(W, "tblpYSpec", &v);
    }
    length(s, "tblpY", f.y);
}

/// A text flow code's `ST_TextDirection` value, as Word writes it.
pub fn flow_literal(code: u8) -> &'static str {
    super::para_props::FLOWS.iter().find(|(_, c)| *c == code).map_or("lrTb", |(n, _)| n)
}

// Serialize

/// `p` as an element of kind `K` in the part `cx` describes, with the children other nodes own (`owned`, by name) in
/// their places; a theme colour's value is written as `colours` resolve it.
fn serialize<K: Kind>(p: &K::Props, owned: &[(&str, &str)], colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let mut decls = String::new();
    let foreign = K::foreign(p).map(|f| &**f);
    out.declarations(foreign, &mut decls);
    let (w, name) = (cx.w.clone(), K::NAME);
    let _ = write!(out.s, "<{w}{name}{decls}");
    out.free_attrs(foreign.map_or(&[][..], |f| &f.attributes), None, &[]);
    out.s.push('>');
    let ranks: Vec<Option<usize>> = foreign.map_or_else(Vec::new, |f| f.children.iter().map(|c| rank_of::<K>(&c.name.ns, &c.name.local)).collect());
    let first_owned = K::ORDER.iter().position(|k| K::OWNED.contains(k));
    for (i, &key) in K::ORDER.iter().enumerate() {
        if Some(i) == first_owned {
            foreign_children(&mut out, foreign, &ranks, None);
        }
        if K::OWNED.contains(&key) {
            if let Some((_, x)) = owned.iter().find(|(n, _)| *n == key) {
                out.s.push_str(x);
            }
            foreign_children(&mut out, foreign, &ranks, Some(i));
        } else {
            foreign_children(&mut out, foreign, &ranks, Some(i));
            modelled_child::<K>(&mut out, p, key);
        }
    }
    let _ = write!(out.s, "</{w}{name}>");
    out.s
}

fn foreign_children(out: &mut Out, foreign: Option<&ForeignProps>, ranks: &[Option<usize>], at: Option<usize>) {
    let Some(f) = foreign else { return };
    for (c, _) in f.children.iter().zip(ranks).filter(|(_, r)| **r == at) {
        out.payload(c);
    }
}

/// Modelled child `key` of `p`: as written when it was kept so and its value is still the one read from it.
fn modelled_child<K: Kind>(out: &mut Out, p: &K::Props, key: &'static str) {
    let kept = K::foreign(p).and_then(|f| f.kept.iter().rev().find(|c| rank_of::<K>(&c.name.ns, &c.name.local).map(|k| K::ORDER[k]) == Some(key)));
    if let Some(c) = kept {
        let at = out.s.len();
        typed_child::<K>(out, p, key);
        let now = out.s.split_off(at);
        let same = read_as::<K>(c).is_some_and(|q| {
            let at = out.s.len();
            typed_child::<K>(out, &q, key);
            out.s.split_off(at) == now
        });
        if same {
            out.payload(c);
        } else {
            out.s.push_str(&now);
        }
        return;
    }
    typed_child::<K>(out, p, key);
}

/// The typed value read from kept child `c` alone.
fn read_as<K: Kind>(c: &crate::format::ForeignChild) -> Option<K::Props> {
    let mut wrapper = String::from("<x");
    for (p, u) in c.bindings.iter() {
        wrapper.push_str(if p.is_empty() { " xmlns" } else { " xmlns:" });
        wrapper.push_str(p);
        wrapper.push_str("=\"");
        super::prop_xml::push_esc(&mut wrapper, u);
        wrapper.push('"');
    }
    let _ = write!(wrapper, ">{}</x>", c.payload);
    let doc = roxmltree::Document::parse(&wrapper).ok()?;
    Some(parse::<K>(doc.root_element()))
}

/// Modelled child `key` of `p` from its typed value.
fn typed_child<K: Kind>(out: &mut Out, p: &K::Props, key: &'static str) {
    let foreign = K::foreign(p).map_or(&[][..], |f| &f.attributes);
    let at = out.s.len();
    let w = out.cx.w.clone();
    let _ = write!(out.s, "<{w}{key}");
    let mut present = false;
    let written = out.attrs(W, foreign, Some((W, key)), |s| present = emit::<K>(p, key, s));
    if !present {
        out.s.truncate(at);
        return;
    }
    out.free_attrs(foreign, Some((W, key)), &written);
    if key == K::BORDERS {
        out.s.push('>');
        let b = K::borders(p);
        for side in TableSide::ALL {
            if let Some(v) = b.side(side) {
                border_side(out, border_name(side).0, v);
            }
        }
        let _ = write!(out.s, "</{w}{key}>");
    } else if key == K::MARGINS {
        out.s.push('>');
        let m = K::margins(p);
        for side in MarginSide::ALL {
            if let Some(v) = m.side(side) {
                margin_side(out, margin_name(side).0, *v);
            }
        }
        let _ = write!(out.s, "</{w}{key}>");
    } else {
        out.s.push_str("/>");
    }
}

fn border_side(out: &mut Out, name: &str, b: &TextBorder) {
    let w = out.cx.w.clone();
    let _ = write!(out.s, "<{w}{name}");
    let _ = out.attrs(W, &[], None, |s| side_attrs(s, b));
    out.s.push_str("/>");
}

fn margin_side(out: &mut Out, name: &str, m: Width) {
    let w = out.cx.w.clone();
    let _ = write!(out.s, "<{w}{name}");
    let _ = out.attrs(W, &[], None, |s| width_attrs(s, m));
    out.s.push_str("/>");
}

// Written over the XML read

/// `p` as an element of kind `K` in the part `cx` describes, written as an edit of `origin` (the element the
/// properties it holds were read from, and those properties) would: each child the change leaves alone as written
/// there, each it changes in its place, a new one in schema order, one it drops removed, the children other nodes own
/// as they are there; with no origin, as an edit of no element. A changed child is written whole in its place, or,
/// when `set` sets one side of it, as that side set among the others as written. With nothing changed, the element
/// as read. `None` when nothing is left.
fn written_over<K: Kind>(p: &K::Props, colours: &ThemeColours, cx: &Cx, origin: Option<(&K::Props, &str)>, set: &[K::Set]) -> Option<String> {
    let foreign = |q: &K::Props| K::foreign(q).is_some_and(|f| !f.children.is_empty() || !f.attributes.is_empty() || !f.kept.is_empty());
    if origin.is_none() && foreign(p) {
        return Some(serialize::<K>(p, &[], colours, cx));
    }
    let empty = K::Props::default();
    let (read, xml) = origin.unwrap_or((&empty, ""));
    let mut props = Props::parse(origin.map(|_| xml), K::NAME, cx);
    props.reorder(K::ORDER);
    let mut out = Out::new(cx, colours);
    let mut changed = false;
    let places: Vec<Place> = set.iter().map(|s| K::place(*s)).collect();
    for &key in K::ORDER.iter().filter(|k| !K::OWNED.contains(k)) {
        let mut render = |q: &K::Props| {
            modelled_child::<K>(&mut out, q, key);
            std::mem::take(&mut out.s)
        };
        let (was, now) = (render(read), render(p));
        let whole = places.iter().any(|s| s.key == key && (s.whole || s.side.is_none()));
        let sides: Vec<Side> = places.iter().filter(|s| s.key == key && !s.whole).filter_map(|s| s.side).collect();
        if was == now && !whole && sides.is_empty() {
            continue;
        }
        changed = true;
        if now.is_empty() {
            props.remove(key);
            continue;
        }
        if !whole && !sides.is_empty() {
            match sides_over::<K>(props.get(key), key, p, &sides, &mut out) {
                Some(x) => props.set(key, x),
                None => props.remove(key),
            }
            continue;
        }
        props.set(key, now);
    }
    let children = |q: &K::Props| {
        K::foreign(q).map_or_else(Vec::new, |f| f.children.iter().map(|c| (props_key(&c.name), c.payload.clone())).collect::<Vec<_>>())
    };
    let (had, mut want) = (children(read), children(p));
    if had != want {
        changed = true;
        for (key, text) in had {
            match want.iter().position(|w| w.0 == key && w.1 == text) {
                Some(k) => {
                    want.remove(k);
                }
                None => {
                    if let Some(k) = props.kids.iter().position(|(n, x)| *n == key && **x == *text) {
                        props.kids.remove(k);
                    }
                }
            }
        }
        for (key, text) in want {
            props.add(&key, text.to_string());
        }
    }
    if !changed && origin.is_some() {
        return Some(xml.to_string());
    }
    props.xml()
}

/// Child `key` with sides as written (`was`) with the sides `sides` written from `p`, or removed, the others as they
/// are there; a start or end side written under its other name stays so.
fn sides_over<K: Kind>(was: Option<&str>, key: &str, p: &K::Props, sides: &[Side], out: &mut Out) -> Option<String> {
    let mut el = Props::parse(was, key, out.cx);
    el.reorder(if key == K::BORDERS { BORDER_ORDER } else { MARGIN_ORDER });
    for side in sides {
        let (name, other) = match *side {
            Side::Border(s) => border_name(s),
            Side::Margin(s) => margin_name(s),
        };
        let at = out.s.len();
        match *side {
            Side::Border(s) => {
                if let Some(b) = K::borders(p).side(s) {
                    border_side(out, name, b);
                }
            }
            Side::Margin(s) => {
                if let Some(m) = K::margins(p).side(s) {
                    margin_side(out, name, *m);
                }
            }
        }
        let xml = out.s.split_off(at);
        if xml.is_empty() {
            el.remove(name);
            if let Some(o) = other {
                el.remove(o);
            }
            continue;
        }
        // The name it is read under
        match other.filter(|o| el.get(name).is_none() && el.get(o).is_some()) {
            Some(o) => {
                let renamed = format!("<{}{o}{}", out.cx.w, &xml[1 + out.cx.w.len() + name.len()..]);
                el.set(o, renamed);
            }
            None => el.set(name, xml),
        }
    }
    el.xml()
}

/// What `p` keeps beside each modelled child `set` sets whole, or that it changes and doesn't set in part: nothing, as
/// writing the child anew keeps nothing of it as written; beside one set in part, no value written otherwise.
fn drop_replaced<K: Kind>(was: &K::Props, p: &mut K::Props, set: &[K::Set]) {
    let cx = Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\">"));
    let colours = ThemeColours::default();
    let mut out = Out::new(&cx, &colours);
    let mut render = |q: &K::Props, key: &'static str| {
        modelled_child::<K>(&mut out, q, key);
        std::mem::take(&mut out.s)
    };
    let places: Vec<Place> = set.iter().map(|s| K::place(*s)).collect();
    let replaced: Vec<&str> = K::ORDER
        .iter()
        .filter(|k| {
            let whole = places.iter().any(|s| s.key == **k && (s.whole || s.side.is_none()));
            let part = places.iter().any(|s| s.key == **k && !s.whole && s.side.is_some());
            !K::OWNED.contains(k) && (whole || !part && render(was, k) != render(p, k))
        })
        .copied()
        .collect();
    if replaced.is_empty() {
        return;
    }
    let Some(f) = K::foreign_mut(p).as_mut() else { return };
    let f = Rc::make_mut(f);
    let of = |q: &QName| &*q.ns == W && replaced.contains(&&*q.local);
    // What it held as read goes; what the edit gave it stays
    let had = K::foreign(was).map(|f| &**f);
    f.attributes.retain(|a| !a.on.as_ref().is_some_and(of) || !had.is_some_and(|h| h.attributes.contains(a)));
    f.kept.retain(|c| !of(&c.name) || !had.is_some_and(|h| h.kept.contains(c)));
    if *f == ForeignProps::default() {
        *K::foreign_mut(p) = None;
    }
}

// Holders

/// Typed properties with the element they were read from (or last written as) beside them, and what edits since
/// reading set.
struct Holder<K: Kind> {
    props: Rc<K::Props>,
    origin: Option<(Rc<K::Props>, Rc<str>)>,
    set: Vec<K::Set>,
}

impl<K: Kind> Clone for Holder<K> {
    fn clone(&self) -> Self {
        Holder { props: self.props.clone(), origin: self.origin.clone(), set: self.set.clone() }
    }
}

impl<K: Kind> Default for Holder<K> {
    fn default() -> Self {
        Holder { props: Rc::new(K::Props::default()), origin: None, set: Vec::new() }
    }
}

impl<K: Kind> std::fmt::Debug for Holder<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct(K::NAME).field("props", &self.props).field("origin", &self.origin.as_ref().map(|o| &o.1)).field("set", &self.set).finish()
    }
}

impl<K: Kind> Holder<K> {
    fn read(props: K::Props, xml: Rc<str>) -> Self {
        let props = Rc::new(props);
        Holder { origin: Some((props.clone(), xml)), props, set: Vec::new() }
    }

    fn from_xml(xml: Option<&str>, cx: &Cx) -> Self {
        let Some(x) = xml else { return Holder::default() };
        Holder::read(cx.parse(x, |_, el| parse::<K>(el)).unwrap_or_default(), x.into())
    }

    fn as_read(&self) -> Option<&Rc<str>> {
        let (read, xml) = self.origin.as_ref()?;
        (Rc::ptr_eq(read, &self.props) || **read == *self.props).then_some(xml)
    }

    fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Option<std::borrow::Cow<'_, str>> {
        if let Some(x) = self.as_read() {
            return Some(std::borrow::Cow::Borrowed(x));
        }
        let origin = self.origin.as_ref().map(|(read, xml)| (&**read, &**xml));
        if origin.is_none() && *self.props == K::Props::default() {
            return None;
        }
        written_over::<K>(&self.props, colours, cx, origin, &self.set).map(std::borrow::Cow::Owned)
    }

    fn edited(&self, mut props: K::Props, set: &[K::Set]) -> Self {
        let mut all = self.set.clone();
        all.extend_from_slice(set);
        if K::foreign(&props).is_some() {
            drop_replaced::<K>(&self.props, &mut props, &all);
        }
        Holder { props: Rc::new(props), origin: self.origin.clone(), set: all }
    }

    fn moved_to(&self, cx: &Cx) -> Option<Self> {
        let (read, xml) = self.origin.as_ref()?;
        if cx.parse(xml, |_, el| parse::<K>(el)).is_some_and(|p| p == **read) {
            return None;
        }
        Some(Holder { props: self.props.clone(), origin: None, set: self.set.clone() })
    }

    fn read_from(&self) -> Option<(&Rc<K::Props>, &Rc<str>)> {
        let xml = self.as_read()?;
        Some((&self.props, xml))
    }

    fn rewritten(&self, xml: Option<&str>, colours: &ThemeColours, cx: &Cx) -> Self {
        let next = Holder::from_xml(xml, cx);
        #[cfg(test)]
        written_as(self, &next, xml, colours, cx);
        #[cfg(not(test))]
        let _ = colours;
        next
    }
}

/// What an edit of a properties element of kind `K` as XML set, as typed properties' record of it: each child that
/// differs between `was` and `now` (both in the part `cx` describes), set whole when `now` writes it as `p` does or
/// has no sides, else each side it changed, in part.
#[cfg(test)]
fn inferred_set<K: Kind>(p: &K::Props, was: &str, now: &str, colours: &ThemeColours, cx: &Cx) -> Vec<K::Set> {
    let (old, new) = (Props::parse(Some(was), K::NAME, cx), Props::parse(Some(now), K::NAME, cx));
    let mut out = Out::new(cx, colours);
    let mut set = Vec::new();
    for &key in K::ORDER.iter().filter(|k| !K::OWNED.contains(k)) {
        let (a, b) = (old.get(key), new.get(key));
        if a == b {
            continue;
        }
        modelled_child::<K>(&mut out, p, key);
        let typed = std::mem::take(&mut out.s);
        let sides = key == K::BORDERS || key == K::MARGINS;
        let (Some(a), Some(b)) = (a, b) else {
            set.extend(K::set_of(key, None));
            continue;
        };
        if b == typed || !sides {
            set.extend(K::set_of(key, None));
            continue;
        }
        let (sa, sb) = (Props::parse(Some(a), key, cx), Props::parse(Some(b), key, cx));
        let names: Vec<(Side, (&str, Option<&str>))> = if key == K::BORDERS {
            TableSide::ALL.iter().map(|s| (Side::Border(*s), border_name(*s))).collect()
        } else {
            MarginSide::ALL.iter().map(|s| (Side::Margin(*s), margin_name(*s))).collect()
        };
        for (side, (name, other)) in names {
            let get = |p: &Props| p.get(name).or_else(|| other.and_then(|o| p.get(o))).map(str::to_string);
            if get(&sa) != get(&sb) {
                set.extend(K::set_of(key, Some(side)));
            }
        }
    }
    set
}

/// What an edit of a properties element of kind `K` as XML wrote (`now`, over `was` as read) is what writing its typed
/// properties over the same writes, byte for byte, but for two differences the typed writer makes: it places a child
/// it writes in schema order (the cell commands place one after the others), and it keeps an element with no
/// children that nothing changed. `TWIP_TABLE_BYTES_OUT` names a file to list each one that isn't in, in place of
/// failing.
#[cfg(test)]
fn written_as<K: Kind>(was: &Holder<K>, next: &Holder<K>, now: Option<&str>, colours: &ThemeColours, cx: &Cx) {
    let Some((read, xml)) = was.read_from() else { return };
    let set = now.map_or_else(Vec::new, |now| inferred_set::<K>(&next.props, xml, now, colours, cx));
    let got = written_over::<K>(&next.props, colours, cx, Some((read, xml)), &set);
    let kids = |x: &str| {
        let mut k = Props::parse(Some(x), K::NAME, cx).kids;
        k.sort();
        (x[..x.find('>').unwrap_or(x.len())].to_string(), k)
    };
    let same = match (got.as_deref(), now) {
        (Some(a), Some(b)) => a == b || kids(a) == kids(b),
        (Some(a), None) => kids(a).1.is_empty(),
        (None, None) => true,
        (None, Some(_)) => false,
    };
    if same {
        return;
    }
    let what = format!("over {xml:?}\n  wrote {now:?}\n  typed {got:?}\n");
    match std::env::var_os("TWIP_TABLE_BYTES_OUT") {
        Some(path) => {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("the report");
            let test = std::thread::current().name().unwrap_or_default().to_string();
            let _ = writeln!(f, "{test} {what}");
        }
        None => panic!("an edit's {} is not what its typed properties write: {what}", K::NAME),
    }
}

/// A holder of one kind's typed properties, with its parse, serializer and writer over the XML read.
macro_rules! holder {
    ($(#[$doc:meta])* $name:ident, $kind:ty, $props:ty, $set:ty, $edit:ty, $el:literal) => {
        $(#[$doc])*
        ///
        /// The written form stands for the properties only while they are the ones it was read as. Its `==` is
        /// always true, as a container's derived equality (layout equivalence) does not compare authored
        /// properties: use [`Self::authored_eq`] for that.
        #[derive(Clone, Debug, Default)]
        pub struct $name(Holder<$kind>);

        impl $name {
            #[doc = concat!("Properties read from `xml`, the `", $el, "` they were parsed from.")]
            pub fn read(props: $props, xml: Rc<str>) -> $name {
                $name(Holder::read(props, xml))
            }

            #[doc = concat!("Properties read from `xml`, shared with other elements read alike.")]
            pub fn shared(props: Rc<$props>, xml: Rc<str>) -> $name {
                $name(Holder { origin: Some((props.clone(), xml)), props, set: Vec::new() })
            }

            #[doc = concat!("The `", $el, "` they were read from, while they are the properties read from it, with those properties.")]
            pub fn read_from(&self) -> Option<(&Rc<$props>, &Rc<str>)> {
                self.0.read_from()
            }

            #[doc = concat!("The properties `xml` holds (a `", $el, "` in the part `cx` describes, an edit of these properties' as XML), the XML kept beside them.")]
            pub fn rewritten(&self, xml: Option<&str>, colours: &ThemeColours, cx: &Cx) -> $name {
                $name(self.0.rewritten(xml, colours, cx))
            }

            /// Typed properties with no written form.
            pub fn typed(props: Rc<$props>) -> $name {
                $name(Holder { props, origin: None, set: Vec::new() })
            }

            #[doc = concat!("The properties `xml` (a `", $el, "` in the part `cx` describes) holds; none for no XML, and nothing typed (the XML kept as written) for XML that doesn't parse.")]
            pub fn from_xml(xml: Option<&str>, cx: &Cx) -> $name {
                $name(Holder::from_xml(xml, cx))
            }

            /// The typed properties.
            pub fn get(&self) -> &Rc<$props> {
                &self.0.props
            }

            #[doc = concat!("The `", $el, "` as read, while the properties are the ones read from it.")]
            pub fn as_read(&self) -> Option<&Rc<str>> {
                self.0.as_read()
            }

            #[doc = concat!("The `", $el, "` they were read from (or last written as), whatever they are now.")]
            pub fn origin_xml(&self) -> Option<&Rc<str>> {
                self.0.origin.as_ref().map(|(_, x)| x)
            }

            #[doc = concat!("These properties with `xml` in place of the `", $el, "` they were read from: the same element, written otherwise (its namespaces declared on it).")]
            pub fn with_origin_xml(&self, xml: Rc<str>) -> $name {
                let origin = self.0.origin.as_ref().map(|(p, _)| (p.clone(), xml));
                $name(Holder { props: self.0.props.clone(), origin, set: self.0.set.clone() })
            }

            #[doc = concat!("The `", $el, "` to write in the part `cx` describes: as read while the properties are those, else written over it from them (a theme colour's value as `colours` resolve it), the children other nodes own as they were read; `None` when nothing is left.")]
            pub fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Option<std::borrow::Cow<'_, str>> {
                self.0.xml(colours, cx)
            }

            #[doc = concat!("`props` in place of these properties, set by an edit that set `set`, in order: the `", $el, "` they were read from kept to write them over, what was set written again.")]
            pub fn edited(&self, props: $props, set: &[$set]) -> $name {
                $name(self.0.edited(props, set))
            }

            /// An edit's result as these properties' successor.
            pub fn edit(&self, e: $edit) -> $name {
                self.edited(e.props, &e.set)
            }

            #[doc = concat!("These properties where `cx` describes the part they are written in: `None` while the `", $el, "` they were read from reads the same there, else without it, to be written from the properties.")]
            pub fn moved_to(&self, cx: &Cx) -> Option<$name> {
                self.0.moved_to(cx).map($name)
            }

            /// Authored equality: the typed properties.
            pub fn authored_eq(&self, o: &$name) -> bool {
                Rc::ptr_eq(&self.0.props, &o.0.props) || self.0.props == o.0.props
            }

            #[doc = concat!("A `", $el, "` as typed properties, without the children other nodes own.")]
            pub fn parse(el: Node) -> $props {
                parse::<$kind>(el)
            }

            #[doc = concat!("`p` as a `", $el, "` in the part `cx` describes, with the children other nodes own (`owned`, by local name, as written) in their places; a theme colour's value is written as `colours` resolve it.")]
            pub fn serialize(p: &$props, owned: &[(&str, &str)], colours: &ThemeColours, cx: &Cx) -> String {
                serialize::<$kind>(p, owned, colours, cx)
            }

            #[doc = concat!("`p` as a `", $el, "` written over `origin` (the `", $el, "` the properties it holds were read from, and those properties) by an edit that set `set`: each child the edit leaves alone as written there, each it changes in its place, one side set in part among the others as written, the children other nodes own as they are there; with nothing changed, the element as read. `None` when nothing is left.")]
            pub fn written_over(p: &$props, colours: &ThemeColours, cx: &Cx, origin: Option<(&$props, &str)>, set: &[$set]) -> Option<String> {
                written_over::<$kind>(p, colours, cx, origin, set)
            }
        }
    };
}

holder!(
    /// A table's authored properties: typed, with the `w:tblPr` they were read from beside them.
    TableDirect, Tbl, TableProps, TableSet, TableEdit, "w:tblPr"
);
holder!(
    /// A row's exceptions to its table's properties: typed, with the `w:tblPrEx` they were read from beside them.
    ExceptionsDirect, TblEx, TableProps, TableSet, TableEdit, "w:tblPrEx"
);
holder!(
    /// A row's authored properties: typed, with the `w:trPr` they were read from beside them.
    RowDirect, Tr, RowProps, RowSet, RowEdit, "w:trPr"
);
holder!(
    /// A cell's authored properties: typed, with the `w:tcPr` they were read from beside them.
    CellDirect, Tc, CellProps, CellSet, CellEdit, "w:tcPr"
);

impl PartialEq for TableDirect {
    fn eq(&self, _: &TableDirect) -> bool {
        true
    }
}

impl PartialEq for ExceptionsDirect {
    fn eq(&self, _: &ExceptionsDirect) -> bool {
        true
    }
}

impl PartialEq for RowDirect {
    fn eq(&self, _: &RowDirect) -> bool {
        true
    }
}

impl PartialEq for CellDirect {
    fn eq(&self, _: &CellDirect) -> bool {
        true
    }
}

impl TableDirect {
    /// The properties of `xml`, a `w:tblPr` with the main namespace bound to `w`.
    #[cfg(test)]
    pub(crate) fn in_w(xml: &str) -> TableDirect {
        TableDirect::from_xml(Some(xml), &Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\">")))
    }
}

/// The earlier properties a tracked change of a table's, a row's exceptions', a row's or a cell's properties records.
/// Its `==` is always true, as its holders' is: use [`PreviousDirect::authored_eq`] for authored equality.
#[derive(Clone, Debug)]
pub enum PreviousDirect {
    /// A `w:tblPrChange`'s `w:tblPr`.
    Table(TableDirect),
    /// A `w:tblPrExChange`'s `w:tblPrEx`.
    Exceptions(ExceptionsDirect),
    /// A `w:trPrChange`'s `w:trPr`.
    Row(RowDirect),
    /// A `w:tcPrChange`'s `w:tcPr`.
    Cell(CellDirect),
}

impl PartialEq for PreviousDirect {
    fn eq(&self, _: &PreviousDirect) -> bool {
        true
    }
}

impl PreviousDirect {
    /// The earlier properties of the change element `name` (`w:tblPrChange`, `w:tblPrExChange`, `w:trPrChange` or
    /// `w:tcPrChange`) from `xml`, the properties element it holds, as written in the part `cx` describes.
    pub fn from_xml(name: &str, xml: Option<&str>, cx: &Cx) -> PreviousDirect {
        match name {
            "tblPrChange" => PreviousDirect::Table(TableDirect::from_xml(xml, cx)),
            "tblPrExChange" => PreviousDirect::Exceptions(ExceptionsDirect::from_xml(xml, cx)),
            "trPrChange" => PreviousDirect::Row(RowDirect::from_xml(xml, cx)),
            _ => PreviousDirect::Cell(CellDirect::from_xml(xml, cx)),
        }
    }

    /// The properties element to write in the part `cx` describes, as its holder writes it.
    pub fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Option<std::borrow::Cow<'_, str>> {
        match self {
            PreviousDirect::Table(d) => d.xml(colours, cx),
            PreviousDirect::Exceptions(d) => d.xml(colours, cx),
            PreviousDirect::Row(d) => d.xml(colours, cx),
            PreviousDirect::Cell(d) => d.xml(colours, cx),
        }
    }

    /// The properties element they were read from (or last written as), whatever they are now.
    pub fn origin_xml(&self) -> Option<&Rc<str>> {
        match self {
            PreviousDirect::Table(d) => d.origin_xml(),
            PreviousDirect::Exceptions(d) => d.origin_xml(),
            PreviousDirect::Row(d) => d.origin_xml(),
            PreviousDirect::Cell(d) => d.origin_xml(),
        }
    }

    /// These properties with `xml` in place of the element they were read from.
    pub fn with_origin_xml(&self, xml: Rc<str>) -> PreviousDirect {
        match self {
            PreviousDirect::Table(d) => PreviousDirect::Table(d.with_origin_xml(xml)),
            PreviousDirect::Exceptions(d) => PreviousDirect::Exceptions(d.with_origin_xml(xml)),
            PreviousDirect::Row(d) => PreviousDirect::Row(d.with_origin_xml(xml)),
            PreviousDirect::Cell(d) => PreviousDirect::Cell(d.with_origin_xml(xml)),
        }
    }

    /// The properties element as read, while the properties are the ones read from it.
    pub fn as_read(&self) -> Option<&Rc<str>> {
        match self {
            PreviousDirect::Table(d) => d.as_read(),
            PreviousDirect::Exceptions(d) => d.as_read(),
            PreviousDirect::Row(d) => d.as_read(),
            PreviousDirect::Cell(d) => d.as_read(),
        }
    }

    /// Authored equality: the same kind, with the same typed properties.
    pub fn authored_eq(&self, o: &PreviousDirect) -> bool {
        match (self, o) {
            (PreviousDirect::Table(a), PreviousDirect::Table(b)) => a.authored_eq(b),
            (PreviousDirect::Exceptions(a), PreviousDirect::Exceptions(b)) => a.authored_eq(b),
            (PreviousDirect::Row(a), PreviousDirect::Row(b)) => a.authored_eq(b),
            (PreviousDirect::Cell(a), PreviousDirect::Cell(b)) => a.authored_eq(b),
            _ => false,
        }
    }
}

// Conversion from and to the XML path's properties

fn authored_colour(rgb: Option<u32>, theme: Option<super::model::ThemeColourRef>) -> Option<Color> {
    theme.map(Color::Theme).or(rgb.map(Color::Rgb))
}

/// An authored colour as the XML path holds it: its value (a theme colour's as `colours` resolve it) and its theme
/// colour.
fn xml_colour(c: Option<Color>, colours: &ThemeColours) -> (Option<u32>, Option<super::model::ThemeColourRef>) {
    match c {
        Some(Color::Rgb(v)) => (Some(v), None),
        Some(Color::Theme(r)) => (Some(colours.resolve(r)), Some(r)),
        Some(Color::Auto) | None => (None, None),
    }
}

fn authored_borders(b: &xml_props::Borders) -> TableBorders {
    let side = |b: &Option<xml_props::Border>| {
        b.as_ref().map(|b| TextBorder {
            style: b.style,
            width: b.width,
            space: b.space,
            shadow: b.shadow,
            color: authored_colour(b.color, b.color_theme),
        })
    };
    TableBorders {
        top: side(&b.top),
        left: side(&b.left),
        bottom: side(&b.bottom),
        right: side(&b.right),
        inside_h: side(&b.inside_h),
        inside_v: side(&b.inside_v),
        tl2br: side(&b.tl2br),
        tr2bl: side(&b.tr2bl),
    }
}

fn xml_borders(b: &TableBorders, colours: &ThemeColours) -> xml_props::Borders {
    let side = |b: &Option<TextBorder>| {
        b.as_ref().map(|b| {
            let (color, color_theme) = xml_colour(b.color, colours);
            xml_props::Border { style: b.style, width: b.width, space: b.space, shadow: b.shadow, color, color_theme }
        })
    };
    xml_props::Borders {
        top: side(&b.top),
        left: side(&b.left),
        bottom: side(&b.bottom),
        right: side(&b.right),
        inside_h: side(&b.inside_h),
        inside_v: side(&b.inside_v),
        tl2br: side(&b.tl2br),
        tr2bl: side(&b.tr2bl),
    }
}

fn authored_margins(m: &xml_props::Margins) -> CellMargins {
    let side = |v: Option<f64>| v.map(Width::Dxa);
    CellMargins { top: side(m.top), left: side(m.left), bottom: side(m.bottom), right: side(m.right) }
}

/// Margins as the XML path holds them: a width other than points is none.
fn xml_margins(m: &CellMargins) -> xml_props::Margins {
    let side = |v: Option<Width>| v.map(Width::dxa);
    xml_props::Margins { top: side(m.top), left: side(m.left), bottom: side(m.bottom), right: side(m.right) }
}

fn authored_shading(s: &xml_props::Shd) -> Shading {
    Shading { pattern: s.pattern, color: authored_colour(s.color, s.color_theme), fill: authored_colour(s.fill, s.fill_theme) }
}

fn xml_shd(s: &Shading, colours: &ThemeColours) -> xml_props::Shd {
    let (color, color_theme) = xml_colour(s.color, colours);
    let (fill, fill_theme) = xml_colour(s.fill, colours);
    xml_props::Shd { pattern: s.pattern, color, fill, color_theme, fill_theme }
}

/// The offset (twentieths of a point) a vertical alignment stands for under a text anchor.
fn y_code(y: Option<FloatY>) -> f64 {
    match y {
        Some(FloatY::Center) => -8.0,
        Some(FloatY::Bottom) => -12.0,
        Some(FloatY::Inside) => -16.0,
        Some(FloatY::Outside) => -20.0,
        _ => -4.0,
    }
}

/// A floating position as the XML path folds it, with the overlap set beside it.
fn table_pos(f: &TableFloat, overlap: Option<TableOverlap>) -> TablePos {
    let v_anchor = match f.v_anchor {
        Some(FloatAnchor::Page) => TableAnchor::Page,
        Some(FloatAnchor::Text) => TableAnchor::Text,
        _ => TableAnchor::Margin,
    };
    let y = match (v_anchor, f.y_align) {
        (TableAnchor::Text, Some(_)) => y_code(f.y_align) / 20.0,
        _ => f.y.unwrap_or(0.0),
    };
    TablePos {
        v_anchor,
        y,
        x: f.x.unwrap_or(0.0),
        x_spec: f.x_align.map(|x| match x {
            FloatX::Center => Jc::Center,
            FloatX::Right | FloatX::Outside => Jc::Right,
            FloatX::Left | FloatX::Inside => Jc::Left,
        }),
        h_page: f.h_anchor == Some(FloatAnchor::Page),
        h_text: !matches!(f.h_anchor, Some(FloatAnchor::Margin | FloatAnchor::Page)),
        y_spec: f.y_align.map(|y| match y {
            FloatY::Center => Jc::Center,
            FloatY::Bottom | FloatY::Outside => Jc::Right,
            FloatY::Inline | FloatY::Top | FloatY::Inside => Jc::Left,
        }),
        dist: [f.top_from_text, f.bottom_from_text, f.left_from_text, f.right_from_text].map(|d| d.unwrap_or(0.0).max(0.0)),
        never_overlap: overlap == Some(TableOverlap::Never),
    }
}

/// A folded floating position as an authored one that folds to it.
fn authored_float(t: &TablePos) -> TableFloat {
    let text = t.v_anchor == TableAnchor::Text;
    let y_align = t.y_spec.map(|j| match (text, j) {
        (true, _) => match (t.y * 20.0).round() as i64 {
            -8 => FloatY::Center,
            -12 => FloatY::Bottom,
            -16 => FloatY::Inside,
            -20 => FloatY::Outside,
            _ => FloatY::Top,
        },
        (false, Jc::Center) => FloatY::Center,
        (false, Jc::Right) => FloatY::Bottom,
        (false, _) => FloatY::Top,
    });
    TableFloat {
        v_anchor: Some(match t.v_anchor {
            TableAnchor::Text => FloatAnchor::Text,
            TableAnchor::Margin => FloatAnchor::Margin,
            TableAnchor::Page => FloatAnchor::Page,
        }),
        h_anchor: Some(if t.h_page {
            FloatAnchor::Page
        } else if t.h_text {
            FloatAnchor::Text
        } else {
            FloatAnchor::Margin
        }),
        x: Some(t.x),
        x_align: t.x_spec.map(|j| match j {
            Jc::Center => FloatX::Center,
            Jc::Right => FloatX::Right,
            _ => FloatX::Left,
        }),
        y: (!text || y_align.is_none()).then_some(t.y),
        y_align,
        top_from_text: Some(t.dist[0]),
        bottom_from_text: Some(t.dist[1]),
        left_from_text: Some(t.dist[2]),
        right_from_text: Some(t.dist[3]),
    }
}

/// The XML path's table properties (one level, or levels folded) as typed properties.
pub fn from_tbl_props(t: &TblProps) -> TableProps {
    TableProps {
        style: t.style.as_deref().map(Rc::from),
        floating: t.floating.as_ref().map(authored_float),
        overlap: t.floating.and_then(|f| f.never_overlap.then_some(TableOverlap::Never)),
        bidi_visual: t.bidi_visual,
        row_band: t.row_band,
        col_band: t.col_band,
        width: t.width,
        align: t.jc,
        cell_spacing: t.cell_spacing.map(Width::Dxa),
        indent: t.ind,
        borders: authored_borders(&t.borders),
        shading: t.shd.as_ref().map(authored_shading),
        layout: t.fixed.map(|f| if f { TableLayout::Fixed } else { TableLayout::Autofit }),
        margins: authored_margins(&t.margins),
        look: t.look,
        caption: None,
        description: None,
        foreign: None,
    }
}

/// Typed table properties as the XML path holds them, a theme colour's value as `colours` resolve it.
pub fn tbl_props(p: &TableProps, colours: &ThemeColours) -> TblProps {
    let shd = p.shading.as_ref().map(|s| xml_shd(s, colours));
    TblProps {
        style: p.style.as_deref().map(str::to_string),
        width: p.width,
        jc: p.align,
        ind: p.indent,
        borders: xml_borders(&p.borders, colours),
        margins: xml_margins(&p.margins),
        fixed: p.layout.map(|l| l == TableLayout::Fixed),
        look: p.look,
        cell_spacing: p.cell_spacing.map(Width::dxa),
        row_band: p.row_band,
        col_band: p.col_band,
        floating: p.floating.as_ref().map(|f| table_pos(f, p.overlap)),
        shading: shd.as_ref().map(xml_props::Shd::flat),
        shd,
        bidi_visual: p.bidi_visual,
    }
}

/// The XML path's row properties (one level, or levels folded) as typed properties.
pub fn from_tr_props(t: &TrProps) -> RowProps {
    RowProps {
        grid_before: t.grid_before,
        grid_after: t.grid_after,
        width_before: t.w_before,
        width_after: t.w_after,
        cant_split: t.cant_split,
        height: t.height.map(|(value, rule)| RowHeight { value, rule }),
        header: t.header,
        cell_spacing: t.cell_spacing.map(Width::Dxa),
        align: t.jc,
        ..RowProps::default()
    }
}

/// Typed row properties as the XML path holds them.
pub fn tr_props(p: &RowProps) -> TrProps {
    TrProps {
        height: p.height.map(|h| (h.value, h.rule)),
        cant_split: p.cant_split,
        header: p.header,
        jc: p.align,
        grid_before: p.grid_before,
        grid_after: p.grid_after,
        w_before: p.width_before.map(Width::sized),
        w_after: p.width_after.map(Width::sized),
        cell_spacing: p.cell_spacing.map(Width::dxa),
    }
}

/// The XML path's cell properties (one level, or levels folded) as typed properties.
pub fn from_tc_props(t: &TcProps) -> CellProps {
    let merge = |m: Option<bool>| m.map(|r| if r { MergeRole::Restart } else { MergeRole::Continue });
    CellProps {
        width: t.width,
        grid_span: t.grid_span,
        h_merge: merge(t.h_merge),
        v_merge: merge(t.v_merge),
        borders: authored_borders(&t.borders),
        shading: t.shd.as_ref().map(authored_shading),
        no_wrap: t.no_wrap,
        margins: authored_margins(&t.margins),
        text_flow: t.text_direction.as_deref().map(super::model::text_flow),
        fit_text: t.fit_text,
        v_align: t.v_align,
        hide_mark: None,
        foreign: None,
    }
}

/// Typed cell properties as the XML path holds them, a theme colour's value as `colours` resolve it; the text flow as
/// the value Word writes for it.
pub fn tc_props(p: &CellProps, colours: &ThemeColours) -> TcProps {
    let shd = p.shading.as_ref().map(|s| xml_shd(s, colours));
    TcProps {
        width: p.width,
        grid_span: p.grid_span,
        v_merge: p.v_merge.map(|m| m == MergeRole::Restart),
        h_merge: p.h_merge.map(|m| m == MergeRole::Restart),
        borders: xml_borders(&p.borders, colours),
        shading: shd.as_ref().map(xml_props::Shd::flat),
        shd,
        margins: xml_margins(&p.margins),
        v_align: p.v_align,
        text_direction: p.text_flow.map(|c| flow_literal(c).to_string()),
        no_wrap: p.no_wrap,
        fit_text: p.fit_text,
    }
}

// Resolution over the XML path's style levels

/// A table's typed authored properties `direct` over `base` (a table style region's, as the XML path holds them),
/// folded, as the XML path holds them.
pub fn table_over(base: &TblProps, direct: &TableProps, colours: &ThemeColours) -> TblProps {
    let mut t = from_tbl_props(base);
    t.merge(direct);
    tbl_props(&t, colours)
}

/// A row's typed authored properties `direct` over `base` (a table style region's), folded, as the XML path holds
/// them.
pub fn row_over(base: &TrProps, direct: &RowProps) -> TrProps {
    let mut r = from_tr_props(base);
    r.merge(direct);
    tr_props(&r)
}

/// A cell's typed authored properties `direct` over `base` (its table style region's), folded, as the XML path holds
/// them.
pub fn cell_over(base: &TcProps, direct: &CellProps, colours: &ThemeColours) -> TcProps {
    let mut c = from_tc_props(base);
    c.merge(direct);
    tc_props(&c, colours)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:x="urn:x""#;

    fn cx() -> Cx {
        Cx::of_xml(&format!("<w:document {NS}>"))
    }

    fn table(x: &str) -> TableProps {
        cx().parse(x, |_, el| TableDirect::parse(el)).expect("parses")
    }

    fn cell(x: &str) -> CellProps {
        cx().parse(x, |_, el| CellDirect::parse(el)).expect("parses")
    }

    /// `x` (a `w:tcPr`) written again, with what the writing lost of it.
    fn cell_rewritten(x: &str, edit: impl FnOnce(&mut CellProps)) -> (String, Vec<String>) {
        let cx = cx();
        let mut p = cell(x);
        edit(&mut p);
        let xml = CellDirect::serialize(&p, &[], &ThemeColours::default(), &cx);
        let lost = cx.parse(x, |_, had| cx.parse(&xml, |_, has| super::super::char_props::tests::lost(had, has))).flatten().expect("parses");
        (xml, lost)
    }

    fn line(style: BorderStyle, eighths: f64) -> TextBorder {
        TextBorder { style, width: eighths / 8.0, space: 0.0, shadow: false, color: Some(Color::Auto) }
    }

    #[test]
    fn unset_off_and_explicit_stay_apart() {
        let t = table(
            r#"<w:tblPr><w:bidiVisual w:val="0"/><w:tblLayout w:type="fixed"/><w:tblBorders><w:top w:val="nil"/></w:tblBorders><w:tblCellMar><w:start w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>"#,
        );
        assert_eq!((t.bidi_visual, t.layout, t.align), (Some(false), Some(TableLayout::Fixed), None));
        assert_eq!(t.borders.top.map(|b| b.style), Some(BorderStyle::Nil), "a side of no line is set");
        assert_eq!((t.borders.left, t.margins.left, t.margins.right), (None, Some(Width::Dxa(0.0)), None));
        let c = cell(r#"<w:tcPr><w:vMerge/><w:hMerge w:val="restart"/><w:noWrap/></w:tcPr>"#);
        assert_eq!((c.v_merge, c.h_merge, c.no_wrap, c.fit_text), (Some(MergeRole::Continue), Some(MergeRole::Restart), Some(true), None));
    }

    #[test]
    fn unmodelled_children_and_attributes_are_kept_in_their_place() {
        let x = r#"<w:tcPr x:a="1"><w:cnfStyle w:val="100000000000"/><w:tcW w:w="2880" w:type="dxa" x:b="2"/><w:hideMark/><w:headers><w:header w:val="h1"/></w:headers><x:mine/></w:tcPr>"#;
        let (xml, lost) = cell_rewritten(x, |_| {});
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        let (xml, lost) = cell_rewritten(x, |c| c.v_align = Some(crate::document::section::VerticalAlignment::Center));
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        let at: Vec<usize> =
            ["w:cnfStyle", "w:tcW", "w:vAlign", "w:hideMark", "w:headers", "x:mine"].iter().map(|n| xml.find(&format!("<{n}")).expect(n)).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "schema order: {xml}");
    }

    #[test]
    fn values_written_otherwise_are_written_as_read_until_they_change() {
        let cx = cx();
        let colours = ThemeColours::default();
        let x = r#"<w:tblPr><w:tblW w:w="50%" w:type="pct"/><w:jc w:val="start"/><w:tblLook w:val="04a0"/></w:tblPr>"#;
        let mut t = table(x);
        assert_eq!((t.width, t.align), (Some(Width::Pct(0.5)), Some(Jc::Left)));
        assert_eq!(t.look.map(|l| (l.first_row, l.first_col, l.no_v_band, l.last_row)), Some((true, true, true, false)));
        let xml = TableDirect::serialize(&t, &[], &colours, &cx);
        assert!(xml.contains(r#"w:w="50%""#) && xml.contains(r#"w:val="start""#) && xml.contains(r#"w:val="04a0""#), "{xml}");
        (t.width, t.align) = (Some(Width::Dxa(144.0)), Some(Jc::Center));
        let xml = TableDirect::serialize(&t, &[], &colours, &cx);
        assert!(xml.contains(r#"<w:tblW w:w="2880" w:type="dxa"/>"#) && xml.contains(r#"<w:jc w:val="center"/>"#), "{xml}");
    }

    #[test]
    fn sides_are_written_as_read_until_they_change() {
        let x = r#"<w:tcPr><w:tcBorders><w:start w:val="single" w:sz="4" w:space="0" w:color="auto" x:q="1"/><w:bottom w:val="nil"/><w:bar w:val="single"/></w:tcBorders></w:tcPr>"#;
        let c = cell(x);
        assert_eq!((c.borders.left, c.borders.bottom.map(|b| b.style)), (Some(line(BorderStyle::Single, 4.0)), Some(BorderStyle::Nil)));
        let (xml, lost) = cell_rewritten(x, |_| {});
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        let (xml, _) = cell_rewritten(x, |c| c.borders.top = Some(line(BorderStyle::Double, 8.0)));
        assert!(xml.contains(r#"<w:tcBorders><w:top w:val="double" w:sz="8" w:space="0" w:color="auto"/><w:left w:val="single""#), "{xml}");
    }

    #[test]
    fn one_side_set_in_part_is_written_among_the_others_as_written() {
        let cx = cx();
        let colours = ThemeColours::default();
        let x = r#"<w:tcPr><w:tcW w:w="0" w:type="auto"/><w:tcBorders><w:start w:val="single" w:sz="4" w:space="0" w:color="auto" x:q="1"/><w:bottom w:val="nil"/></w:tcBorders><w:tcMar><w:end w:w="10" w:type="dxa"/></w:tcMar></w:tcPr>"#;
        let read = cell(x);
        let mut e = CellEdit::new(read.clone());
        e.border(TableSide::Top, Some(line(BorderStyle::Double, 8.0)));
        e.border(TableSide::Left, Some(line(BorderStyle::Dotted, 2.0)));
        e.border(TableSide::Bottom, None);
        e.margin(MarginSide::Right, Some(Width::Dxa(5.4)));
        let xml = CellDirect::written_over(&e.props, &colours, &cx, Some((&read, x)), &e.set).expect("properties");
        assert_eq!(
            xml,
            r#"<w:tcPr><w:tcW w:w="0" w:type="auto"/><w:tcBorders><w:top w:val="double" w:sz="8" w:space="0" w:color="auto"/><w:start w:val="dotted" w:sz="2" w:space="0" w:color="auto"/></w:tcBorders><w:tcMar><w:end w:w="108" w:type="dxa"/></w:tcMar></w:tcPr>"#
        );
        let again = cx.parse(&xml, |_, el| CellDirect::parse(el)).expect("parses");
        assert_eq!((again.borders, again.margins), (e.props.borders, e.props.margins));
    }

    #[test]
    fn the_children_other_nodes_own_go_where_the_schema_places_them() {
        let cx = Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\">"));
        let colours = ThemeColours::default();
        let row = RowProps {
            cant_split: Some(true),
            hidden: Some(true),
            height: Some(RowHeight { value: 18.0, rule: HeightRule::Exact }),
            ..Default::default()
        };
        let owned =
            [("trPrChange", "<w:trPrChange w:id=\"3\"><w:trPr/></w:trPrChange>"), ("ins", "<w:ins w:id=\"1\"/>"), ("del", "<w:del w:id=\"2\"/>")];
        assert_eq!(
            RowDirect::serialize(&row, &owned, &colours, &cx),
            r#"<w:trPr><w:cantSplit/><w:trHeight w:val="360" w:hRule="exact"/><w:hidden/><w:ins w:id="1"/><w:del w:id="2"/><w:trPrChange w:id="3"><w:trPr/></w:trPrChange></w:trPr>"#
        );
        let x = r#"<w:tcPr><w:tcPrChange w:id="5"><w:tcPr/></w:tcPrChange><w:cellMerge w:id="4" w:vMerge="rest"/><w:gridSpan w:val="2"/></w:tcPr>"#;
        let c = cell(x);
        assert_eq!(c.grid_span, Some(2));
        assert_eq!(c.foreign, None, "the owned children are not the cell's");
        let owned =
            [("cellMerge", r#"<w:cellMerge w:id="4" w:vMerge="rest"/>"#), ("tcPrChange", r#"<w:tcPrChange w:id="5"><w:tcPr/></w:tcPrChange>"#)];
        assert_eq!(
            CellDirect::serialize(&c, &owned, &colours, &cx),
            r#"<w:tcPr><w:gridSpan w:val="2"/><w:cellMerge w:id="4" w:vMerge="rest"/><w:tcPrChange w:id="5"><w:tcPr/></w:tcPrChange></w:tcPr>"#
        );
    }

    #[test]
    fn an_edit_writes_over_the_xml_it_was_read_from() {
        let cx = cx();
        let colours = ThemeColours::default();
        let x = r#"<w:tblPr> <w:tblStyle w:val="Grid"/><w:tblW w:w="0" w:type="auto"/><w:tblLook w:val="04A0"/><w:tblPrChange w:id="1"><w:tblPr/></w:tblPrChange></w:tblPr>"#;
        let held = TableDirect::from_xml(Some(x), &cx);
        assert_eq!(held.xml(&colours, &cx).as_deref(), Some(x), "as read");
        let mut e = TableEdit::new((**held.get()).clone());
        e.width = Some(Width::Dxa(144.0));
        e.whole(TableProp::Width);
        e.layout = Some(TableLayout::Fixed);
        e.whole(TableProp::Layout);
        let edited = held.edit(e);
        assert!(!edited.authored_eq(&held));
        assert_eq!(edited.as_read(), None);
        let mut props = Props::parse(Some(x), "tblPr", &cx);
        props.reorder(super::super::write::props::TBLPR);
        props.set("tblW", r#"<w:tblW w:w="2880" w:type="dxa"/>"#.into());
        props.set("tblLayout", r#"<w:tblLayout w:type="fixed"/>"#.into());
        assert_eq!(edited.xml(&colours, &cx).map(|x| x.into_owned()), props.xml(), "as the XML patch writes it");
        assert_eq!(held.moved_to(&cx).map(|_| ()), None, "reads the same in the same part");
    }

    #[test]
    fn a_child_set_whole_keeps_nothing_of_it_as_written() {
        let cx = cx();
        let colours = ThemeColours::default();
        let x = r#"<w:tblPr><w:tblW w:w="50%" w:type="pct" x:b="1"/></w:tblPr>"#;
        let held = TableDirect::from_xml(Some(x), &cx);
        let mut e = TableEdit::new((**held.get()).clone());
        e.width = Some(Width::Pct(0.5));
        e.whole(TableProp::Width);
        let edited = held.edit(e);
        assert_eq!(edited.get().foreign, None);
        assert_eq!(TableDirect::serialize(edited.get(), &[], &colours, &cx), r#"<w:tblPr><w:tblW w:w="2500" w:type="pct"/></w:tblPr>"#);
    }

    #[test]
    fn a_floating_position_folds_as_the_xml_path_reads_it() {
        let colours = ThemeColours::default();
        let x = r#"<w:tblPr><w:tblpPr w:leftFromText="-20" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="column" w:tblpXSpec="inside" w:tblpYSpec="outside" w:tblpY="100"/><w:tblOverlap w:val="never"/></w:tblPr>"#;
        let t = table(x);
        let f = t.floating.expect("floating");
        assert_eq!(
            (f.h_anchor, f.x_align, f.y_align, f.left_from_text),
            (Some(FloatAnchor::Text), Some(FloatX::Inside), Some(FloatY::Outside), Some(-1.0))
        );
        let frozen = cx().parse(x, |_, el| TblProps::parse(Some(el), &colours)).expect("parses");
        assert_eq!(tbl_props(&t, &colours), frozen);
        assert_eq!(tbl_props(&from_tbl_props(&frozen), &colours), frozen);
        let pos = frozen.floating.expect("floating");
        assert!(pos.never_overlap && pos.y == -1.0 && pos.dist[2] == 0.0, "{pos:?}");
    }

    #[test]
    fn levels_fold_as_the_xml_path_folds_them() {
        let colours = ThemeColours::default();
        let parse = |x: &str| (table(x), cx().parse(x, |_, el| TblProps::parse(Some(el), &colours)).expect("parses"));
        let (style, frozen_style) = parse(
            r#"<w:tblPr><w:tblpPr w:tblpX="100"/><w:tblOverlap w:val="never"/><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/></w:tblBorders><w:tblCellMar><w:left w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr>"#,
        );
        let (direct, frozen_direct) = parse(
            r#"<w:tblPr><w:tblpPr w:tblpX="200"/><w:tblBorders><w:left w:val="nil"/></w:tblBorders><w:tblCellMar><w:top w:w="5" w:type="pct"/></w:tblCellMar><w:shd w:val="clear" w:fill="FF0000"/></w:tblPr>"#,
        );
        let mut typed = style;
        typed.merge(&direct);
        let mut frozen = frozen_style;
        frozen.merge(&frozen_direct);
        assert_eq!(tbl_props(&typed, &colours), frozen);
        assert_eq!(typed.overlap, None, "the overlap goes with its level's position");
        let cells = |x: &str| (cell(x), cx().parse(x, |_, el| TcProps::parse(Some(el), &colours)).expect("parses"));
        let (mut typed, mut frozen) =
            cells(r#"<w:tcPr><w:tcBorders><w:top w:val="single"/><w:tl2br w:val="single"/></w:tcBorders><w:vAlign w:val="center"/></w:tcPr>"#);
        let (direct, frozen_direct) = cells(r#"<w:tcPr><w:tcBorders><w:top w:val="nil"/></w:tcBorders><w:textDirection w:val="btLr"/></w:tcPr>"#);
        typed.merge(&direct);
        frozen.merge(&frozen_direct);
        assert_eq!(tc_props(&typed, &colours), frozen);
    }

    #[test]
    fn a_strict_text_direction_reads_as_the_flow_word_writes() {
        let colours = ThemeColours::default();
        let x = r#"<w:tcPr><w:textDirection w:val="tbV"/></w:tcPr>"#;
        let c = cell(x);
        assert_eq!(c.text_flow, Some(4));
        let frozen = cx().parse(x, |_, el| TcProps::parse(Some(el), &colours)).expect("parses");
        assert_eq!((frozen.text_direction.as_deref(), tc_props(&c, &colours).text_direction.as_deref()), (Some("tbV"), Some("lrTbV")));
    }

    #[test]
    fn an_overlap_before_its_position_still_counts() {
        let colours = ThemeColours::default();
        let x = r#"<w:tblPr><w:tblOverlap w:val="never"/><w:tblpPr w:tblpX="100"/></w:tblPr>"#;
        let frozen = cx().parse(x, |_, el| TblProps::parse(Some(el), &colours)).expect("parses");
        assert_eq!(frozen.floating.map(|f| f.never_overlap), Some(false), "the XML path reads it only after the position");
        assert_eq!(tbl_props(&table(x), &colours).floating.map(|f| f.never_overlap), Some(true), "read in any order");
    }

    #[test]
    fn row_exceptions_read_only_their_own_children() {
        let cx = cx();
        let x = r#"<w:tblPrEx><w:tblStyle w:val="Grid"/><w:tblInd w:w="100" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPrEx>"#;
        let ex = cx.parse(x, |_, el| ExceptionsDirect::parse(el)).expect("parses");
        assert_eq!((ex.style.as_deref(), ex.indent, ex.layout), (None, Some(Width::Dxa(5.0)), Some(TableLayout::Fixed)));
        let xml = ExceptionsDirect::serialize(&ex, &[], &ThemeColours::default(), &cx);
        assert_eq!(xml, r#"<w:tblPrEx><w:tblInd w:w="100" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblStyle w:val="Grid"/></w:tblPrEx>"#);
    }
}
