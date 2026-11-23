//! Table formatting commands

use crate::docx::props::{Border, Jc, Look, Width};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::step::{Step, TableProps};
use super::table::Grid;

/// A table's band sizes ([`Editor::table_band_sizes`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableBands {
    /// Rows in each band, as it lays out.
    pub rows: u32,
    /// Columns in each band, as it lays out.
    pub columns: u32,
    /// Its own rows in each band, over its style's.
    pub own_rows: Option<u32>,
    /// Its own columns in each band, over its style's.
    pub own_columns: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TableFormat {
    CellAlign(String),
    CellShading(Shading),
    TableShading(Shading),
    CellBorders(Edges, Border),
    CellMargins([f64; 4]),
    HeaderRow(bool),
    RowHeight(Option<(f64, String)>),
    RowCantSplit(bool),
    ColumnWidth(f64),
    TableWidth(Width),
    TableAlign(Jc),
    TableIndent(f64),
    TableStyle(String),
    TableLook(Look),
    CellProps(crate::api::command::CellProperties),
    RowProps {
        hidden: Option<bool>,
        exceptions: Option<RowEx>,
    },
    CellSpacing(f64),
    /// The table's own rows and columns in each style band (`None` keeps one, `Some(None)` leaves it to the style).
    Bands(Option<Option<u32>>, Option<Option<u32>>),
    Alt(crate::api::command::TableAltText),
    /// AutoFit to the window or to fixed widths: the layout, the table's width and its columns' widths.
    Fit {
        fixed: bool,
        width: Width,
        grid: Vec<f64>,
    },
}

/// Sets `row`'s height (`None`: auto) on the model and in its `w:trPr`.
pub(super) fn set_height(row: &mut crate::docx::Row, h: Option<(f64, String)>, cx: &crate::docx::write::props::Cx) {
    use crate::docx::write::props::Props;
    let w = &cx.w;
    let mut p = Props::parse(row.direct.0.as_deref(), "trPr", cx);
    p.reorder(crate::docx::write::props::TRPR);
    match &h {
        Some((v, rule)) => p.set(
            "trHeight",
            format!("<{w}trHeight {w}val=\"{}\" {w}hRule=\"{}\"/>", (v * 20.0).round() as i64, crate::docx::write::props::esc(rule)),
        ),
        None => p.remove("trHeight"),
    }
    row.direct = crate::docx::Raw(p.xml().map(|x| std::rc::Rc::from(x.as_str())));
    row.height = h;
}

/// `v` as a table width element `name`.
fn width_xml(w: &str, name: &str, v: &Width) -> String {
    let (n, kind) = match v {
        Width::Auto => (0, "auto"),
        Width::Nil => (0, "nil"),
        Width::Dxa(pt) => ((pt * 20.0).round() as i64, "dxa"),
        Width::Pct(f) => ((f * 5000.0).round() as i64, "pct"),
    };
    format!("<{w}{name} {w}w=\"{n}\" {w}type=\"{kind}\"/>")
}

/// `xml` as the table's own `name` in its `w:tblPr` (empty: none).
fn set_own(props: &mut TableProps, name: &str, xml: String, cx: &crate::docx::write::props::Cx) {
    use crate::docx::write::props::{Props, TBLPR};
    let mut p = Props::parse(props.direct.0.as_deref(), "tblPr", cx);
    p.reorder(TBLPR);
    match xml.is_empty() {
        true => p.remove(name),
        false => p.set(name, xml),
    }
    let xml = p.xml().unwrap_or_else(|| format!("<{}tblPr/>", cx.w));
    props.direct = crate::docx::Raw(Some(std::rc::Rc::from(xml.as_str())));
}

/// `width` as the table's on the model and in its `w:tblPr`.
pub(super) fn set_width(props: &mut TableProps, width: Width, cx: &crate::docx::write::props::Cx) {
    props.width = width;
    set_own(props, "tblW", width_xml(&cx.w, "tblW", &width), cx);
}

/// What table formatting `f` set on the model, written into the table's `w:tblPr` (`props`) and the `w:trPr` of its
/// `rows` it changed (from `was`) as their own.
fn own_xml(f: &TableFormat, props: &mut TableProps, rows: &mut [crate::docx::Row], was: &[crate::docx::Row], cx: &crate::docx::write::props::Cx) {
    use crate::docx::write::props::Props;
    let w = &cx.w;
    let raw = |x: Option<String>| crate::docx::Raw(x.map(|x| std::rc::Rc::from(x.as_str())));
    let table = match f {
        TableFormat::TableAlign(_) => {
            let v = match props.jc {
                Jc::Center => "center",
                Jc::Right => "right",
                _ => "left",
            };
            Some(("jc", format!("<{w}jc {w}val=\"{v}\"/>")))
        }
        TableFormat::TableIndent(_) => Some(("tblInd", width_xml(w, "tblInd", &Width::Dxa(props.ind)))),
        TableFormat::TableWidth(_) | TableFormat::ColumnWidth(_) => Some(("tblW", width_xml(w, "tblW", &props.width))),
        // Written when zero too: without its own, the table takes its style's.
        TableFormat::CellSpacing(_) => Some(("tblCellSpacing", width_xml(w, "tblCellSpacing", &Width::Dxa(props.cell_spacing)))),
        _ => None,
    };
    if let Some((name, xml)) = table {
        set_own(props, name, xml, cx);
    }
    let row = |r: &crate::docx::Row| -> Option<(&'static str, String)> {
        match f {
            TableFormat::RowCantSplit(_) => Some(("cantSplit", format!("<{w}cantSplit {w}val=\"{}\"/>", if r.cant_split { 1 } else { 0 }))),
            TableFormat::HeaderRow(_) => Some(("tblHeader", format!("<{w}tblHeader {w}val=\"{}\"/>", if r.header { 1 } else { 0 }))),
            _ => None,
        }
    };
    for (r, old) in rows.iter_mut().zip(was) {
        if (&r.height, r.cant_split, r.header) == (&old.height, old.cant_split, old.header) {
            continue;
        }
        let Some((name, xml)) = row(r) else { continue };
        let mut p = Props::parse(r.direct.0.as_deref(), "trPr", cx);
        p.reorder(crate::docx::write::props::TRPR);
        match xml.is_empty() {
            true => p.remove(name),
            false => p.set(name, xml),
        }
        r.direct = raw(p.xml());
    }
}

use super::theme::WordColour;

/// A `w:shd` to set: its pattern (`None`: the one written there, else the one its style gives, else clear), pattern
/// colour and fill (`None`: auto).
#[derive(Clone, Debug, PartialEq)]
pub struct Shading {
    pub pattern: Option<&'static str>,
    pub color: Option<WordColour>,
    pub fill: Option<WordColour>,
}

impl Shading {
    /// A fill (or none) alone.
    pub fn fill(fill: Option<WordColour>) -> Shading {
        Shading { pattern: Some("clear"), color: None, fill }
    }

    /// What it reads as over `was`, the `w:shd` there, and `inherited`, the shading its style gives.
    pub fn shd(&self, was: Option<&str>, inherited: Option<&crate::docx::props::Shd>) -> crate::docx::props::Shd {
        let pattern = self
            .pattern
            .map(str::to_string)
            .or_else(|| was.and_then(|x| crate::docx::write::props::attr_of(x, "val")))
            .or_else(|| inherited.map(|s| s.pattern.clone()).filter(|p| p != "nil"));
        crate::docx::props::Shd {
            pattern: pattern.unwrap_or_else(|| "clear".into()),
            color: self.color.map(|c| c.rgb),
            fill: self.fill.map(|c| c.rgb),
            color_theme: self.color.and_then(|c| c.theme),
            fill_theme: self.fill.and_then(|c| c.theme),
        }
    }

    /// Its XML over `was` and `inherited`.
    pub fn xml(&self, w: &str, was: Option<&str>, inherited: Option<&crate::docx::props::Shd>) -> String {
        shd_xml(w, &self.shd(was, inherited))
    }
}

/// A `w:shd` element.
pub fn shd_xml(w: &str, s: &crate::docx::props::Shd) -> String {
    use super::commands::{THEME_COLOR, THEME_FILL, colour_attrs};
    let c = |rgb: Option<u32>, theme| rgb.map(|rgb| WordColour { rgb, theme });
    let tag = format!("<{w}shd {w}val=\"{}\"/>", crate::docx::write::props::esc(&s.pattern));
    let tag = colour_attrs(&tag, w, "color", THEME_COLOR, c(s.color, s.color_theme));
    colour_attrs(&tag, w, "fill", THEME_FILL, c(s.fill, s.fill_theme))
}

/// Row exceptions to set: `reset` drops the rows' own first.
#[derive(Clone, Debug, PartialEq)]
pub struct RowEx {
    pub reset: bool,
    pub jc: Option<Jc>,
    pub cell_spacing: Option<f64>,
    pub shading: Option<WordColour>,
    pub borders: Option<(Edges, Border)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edges {
    All,
    Outside,
    Inside,
    InsideH,
    InsideV,
    Top,
    Bottom,
    Left,
    Right,
    DiagonalDown,
    DiagonalUp,
    None,
}

fn nil() -> Border {
    Border { style: "nil".into(), width: 0.0, space: 0.0, shadow: false, color: None, color_theme: None }
}

pub(super) fn border_xml(w: &str, name: &str, b: &Border) -> String {
    if !b.visible() {
        return format!("<{w}{name} {w}val=\"nil\"/>");
    }
    let tag = format!(
        "<{w}{name} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"{}\"/>",
        crate::docx::write::props::esc(&b.style),
        (b.width * 8.0).round() as i64,
        b.space.round() as i64
    );
    let c = b.color.map(|rgb| WordColour { rgb, theme: b.color_theme });
    super::commands::colour_attrs(&tag, w, "color", super::commands::THEME_COLOR, c)
}

/// A cell's `w:tcBorders` as written, with the sides in `sides` (bits: top, left, bottom, right, then the two
/// diagonals) set from its model borders: the others stay as they were, a style's staying its style's.
fn tc_borders(cx: &crate::docx::write::props::Cx, cell: &crate::docx::Cell, sides: u8) -> String {
    use crate::docx::write::props::{Props, TC_BORDERS};
    let w = &cx.w;
    let tcpr = Props::parse(cell.direct.0.as_deref(), "tcPr", cx);
    let mut p = Props::parse(tcpr.get("tcBorders"), "tcBorders", cx);
    p.reorder(TC_BORDERS);
    let [tl2br, tr2bl] = cell.diagonals.as_deref().cloned().unwrap_or_default();
    let all = [
        ("top", None, &cell.top),
        ("left", Some("start"), &cell.left),
        ("bottom", None, &cell.bottom),
        ("right", Some("end"), &cell.right),
        ("tl2br", None, &tl2br),
        ("tr2bl", None, &tr2bl),
    ];
    for (bit, (name, bidi, b)) in all.into_iter().enumerate() {
        if sides & (1 << bit) == 0 {
            continue;
        }
        let name = bidi.filter(|n| p.get(n).is_some()).unwrap_or(name);
        match b {
            Some(b) => p.set(name, border_xml(w, name, b)),
            None => p.remove(name),
        }
    }
    p.xml().unwrap_or_default()
}

/// Every character of the row is hidden: with its end mark, the row hides.
fn row_text_hidden(row: &crate::docx::Row) -> bool {
    row.cells.iter().all(|c| {
        c.blocks.iter().all(|b| match b {
            crate::docx::Block::Para(p) => p.mark.hidden && p.inlines.iter().all(|i| i.run().is_none_or(|r| r.hidden)),
            crate::docx::Block::Table(_) => false,
        })
    })
}

/// Sets a row's exceptions (`w:tblPrEx`) and what the model reads from them.
fn set_exceptions(row: &mut crate::docx::Row, ex: &RowEx, table: &[Option<Border>; 6], cx: &crate::docx::write::props::Cx) {
    let w = &cx.w;
    let mut p = crate::docx::write::props::Props::parse(if ex.reset { None } else { row.exceptions.0.as_deref() }, "tblPrEx", cx);
    // CT_TblPrEx's children are CT_TblPr's, in its order.
    p.reorder(crate::docx::write::props::TBLPR);
    if ex.reset {
        (row.shading, row.hatch, row.cell_spacing, row.borders) = (None, None, None, None);
    }
    if let Some(j) = ex.jc {
        let v = match j {
            Jc::Center => "center",
            Jc::Right => "right",
            _ => "left",
        };
        p.set("jc", format!("<{w}jc {w}val=\"{v}\"/>"));
        row.jc = Some(j);
    }
    if let Some(s) = ex.cell_spacing {
        let s = (s.max(0.0) * 20.0).round() / 20.0;
        p.set("tblCellSpacing", format!("<{w}tblCellSpacing {w}w=\"{}\" {w}type=\"dxa\"/>", (s * 20.0).round() as i64));
        row.cell_spacing = Some(s);
    }
    if let Some(fill) = ex.shading {
        p.set("shd", Shading { pattern: Some("clear"), color: None, fill: Some(fill) }.xml(w, None, None));
        row.shading = Some(fill.rgb);
        row.hatch = None;
    }
    if let Some((edges, b)) = &ex.borders {
        let mut now = row.borders.as_deref().cloned().unwrap_or_else(|| table.clone());
        let value = if *edges == Edges::None { nil() } else { b.clone() };
        let picks: &[usize] = match edges {
            Edges::All | Edges::None => &[0, 1, 2, 3, 4, 5],
            Edges::Outside => &[0, 1, 2, 3],
            Edges::Inside => &[4, 5],
            Edges::InsideH => &[4],
            Edges::InsideV => &[5],
            Edges::Top => &[0],
            Edges::Left => &[1],
            Edges::Bottom => &[2],
            Edges::Right => &[3],
            Edges::DiagonalDown | Edges::DiagonalUp => &[],
        };
        for &k in picks {
            now[k] = Some(value.clone());
        }
        let names = ["top", "left", "bottom", "right", "insideH", "insideV"];
        let x: String = now.iter().zip(names).filter_map(|(b, n)| b.as_ref().map(|b| border_xml(w, n, b))).collect();
        p.set("tblBorders", format!("<{w}tblBorders>{x}</{w}tblBorders>"));
        row.borders = Some(std::rc::Rc::new(now));
    }
    row.exceptions = crate::docx::Raw(p.xml().map(|x| std::rc::Rc::from(x.as_str())));
}

impl TableFormat {
    /// Whether it formats cells, each selected cell alike.
    pub(super) fn of_cells(&self) -> bool {
        matches!(
            self,
            TableFormat::CellAlign(_)
                | TableFormat::CellShading(_)
                | TableFormat::CellBorders(..)
                | TableFormat::CellMargins(_)
                | TableFormat::CellProps(_)
        )
    }
}

impl Editor {
    /// Sets the selection's table's own band sizes, over its style's: rows and columns in each band (`None` keeps
    /// one, `Some(None)` leaves it to the style), 1 to 32767.
    pub fn set_table_band_sizes(&mut self, rows: Option<Option<u32>>, columns: Option<Option<u32>>, fonts: &Fonts) -> Outcome {
        for n in [rows, columns].into_iter().flatten().flatten() {
            if let Err(e) = super::commands::bounds::within("the band size", n as f64, 1.0, 32767.0) {
                return Outcome::Refused(e);
            }
        }
        if let Some(out) = self.refuse_like(&crate::api::command::Command::SetTableAlignment(Default::default())) {
            return out;
        }
        if self.enforced_protection().is_some_and(|p| p.formatting) {
            return Outcome::Refused(crate::api::error::Error::new(crate::api::error::ErrorCode::NotAllowed, "formatting is restricted"));
        }
        self.table_format(&TableFormat::Bands(rows, columns), fonts)
    }

    /// The selection's table's band sizes as it lays out, and its own ones (none: its style's).
    pub fn table_band_sizes(&self) -> Option<TableBands> {
        let tg = self.table_target()?;
        let t = self.index.table(&self.doc, tg.table)?;
        let res = self.doc.resolver.as_ref()?;
        let cx = self.cx();
        let own = t
            .direct
            .0
            .as_deref()
            .and_then(|raw| cx.parse(raw, |_, n| crate::docx::props::TblProps::parse(Some(n), &res.styles.colours)))
            .unwrap_or_default();
        let size = |v: i64| v.clamp(1, u32::MAX as i64) as u32;
        Some(TableBands { rows: size(t.bands.0), columns: size(t.bands.1), own_rows: own.row_band.map(size), own_columns: own.col_band.map(size) })
    }

    pub(super) fn table_format(&mut self, f: &TableFormat, fonts: &Fonts) -> Outcome {
        if f.of_cells() && self.is_discontiguous() {
            return self.table_format_each(f, fonts);
        }
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        self.table_format_on(tg, f, fonts)
    }

    /// Cell formatting `f` of a discontiguous selection: each block of cells, and the cells each range is in, in one
    /// undo step.
    fn table_format_each(&mut self, f: &TableFormat, fonts: &Fonts) -> Outcome {
        let mut targets: Vec<super::tables::Target> = Vec::new();
        for t in self.each_piece(|e| e.table_target()).into_iter().flatten() {
            if !targets.contains(&t) {
                targets.push(t);
            }
        }
        if targets.is_empty() {
            return Outcome::Unchanged("not in a table");
        }
        self.begin_batch(Some("Table Properties".into()));
        for t in targets {
            self.table_format_on(t, f, fonts);
        }
        self.end_batch(true, fonts)
    }

    pub(super) fn table_format_on(&mut self, tg: super::tables::Target, f: &TableFormat, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let mut rows = t.rows.clone();
        let mut grid = None;
        let mut props = TableProps::of(t);
        let mut restyle = false;
        let cx = self.cx();
        let w = cx.w.clone();
        let set_direct = |cell: &mut crate::docx::Cell, local: &str, xml: String| {
            let mut p = crate::docx::write::props::Props::parse(cell.direct.0.as_deref(), "tcPr", &cx);
            p.set(local, xml);
            cell.direct = crate::docx::Raw(Some(std::rc::Rc::from(p.xml().unwrap_or_else(|| format!("<{w}tcPr/>")).as_str())));
        };
        let in_block = |r: usize, c: usize, s: usize| (tg.rows.0..=tg.rows.1).contains(&r) && c + s.max(1) > tg.cols.0 && c <= tg.cols.1;
        let (r0, r1) = (tg.rows.0, tg.rows.1.min(rows.len().saturating_sub(1)));
        match f {
            TableFormat::CellAlign(_) | TableFormat::CellShading(_) | TableFormat::CellMargins(_) => {
                for (r, row) in rows.iter_mut().enumerate() {
                    for (k, cell) in row.cells.iter_mut().enumerate() {
                        let Some(&(c, s, _)) = g.rows[r].get(k) else { continue };
                        if !in_block(r, c, s) {
                            continue;
                        }
                        match f {
                            TableFormat::CellAlign(v) => {
                                cell.v_align = v.clone();
                                set_direct(cell, "vAlign", format!("<{w}vAlign {w}val=\"{}\"/>", crate::docx::write::props::esc(v)));
                            }
                            TableFormat::CellShading(sh) => {
                                let p = crate::docx::write::props::Props::parse(cell.direct.0.as_deref(), "tcPr", &cx);
                                let inherited = if sh.pattern.is_none() { self.cell_shd(t, (r, k), cell) } else { None };
                                let shd = sh.shd(p.get("shd"), inherited.as_ref());
                                (cell.shading, cell.hatch) = (shd.flat(), shd.hatch());
                                set_direct(cell, "shd", shd_xml(&w, &shd));
                            }
                            TableFormat::CellMargins(m) => {
                                cell.margins = *m;
                                let tw = |v: f64| (v * 20.0).round() as i64;
                                set_direct(
                                    cell,
                                    "tcMar",
                                    format!(
                                        "<{w}tcMar><{w}top {w}w=\"{}\" {w}type=\"dxa\"/><{w}left {w}w=\"{}\" {w}type=\"dxa\"/><{w}bottom {w}w=\"{}\" {w}type=\"dxa\"/><{w}right {w}w=\"{}\" {w}type=\"dxa\"/></{w}tcMar>",
                                        tw(m[0]),
                                        tw(m[1]),
                                        tw(m[2]),
                                        tw(m[3])
                                    ),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            TableFormat::CellBorders(edges @ (Edges::DiagonalDown | Edges::DiagonalUp), b) => {
                let k = usize::from(*edges == Edges::DiagonalUp);
                for (r, row) in rows.iter_mut().enumerate() {
                    for (j, cell) in row.cells.iter_mut().enumerate() {
                        let Some(&(c, s, _)) = g.rows[r].get(j) else { continue };
                        if !in_block(r, c, s) {
                            continue;
                        }
                        let mut d = cell.diagonals.as_deref().cloned().unwrap_or_default();
                        d[k] = Some(b.clone());
                        cell.diagonals = Some(std::rc::Rc::new(d));
                        let xml = tc_borders(&cx, cell, 16 << k);
                        set_direct(cell, "tcBorders", xml);
                    }
                }
            }
            TableFormat::CellBorders(edges, b) => {
                let (c0, c1) = tg.cols;
                let want = |outside: bool, horizontal: bool, side: Edges| match edges {
                    Edges::All | Edges::None => true,
                    Edges::Outside => outside,
                    Edges::Inside => !outside,
                    Edges::InsideH => !outside && horizontal,
                    Edges::InsideV => !outside && !horizontal,
                    Edges::DiagonalDown | Edges::DiagonalUp => false,
                    e => outside && *e == side,
                };
                let value = if *edges == Edges::None { nil() } else { b.clone() };
                let mut shared: Vec<(usize, usize, usize)> = Vec::new();
                let mut touched: Vec<(usize, usize, u8)> = Vec::new();
                for r in r0..=r1 {
                    for (k, &(c, s, _)) in g.rows[r].iter().enumerate() {
                        if !in_block(r, c, s) {
                            continue;
                        }
                        let last = c + s.max(1) - 1;
                        let sides = [
                            (r == r0, true, Edges::Top),
                            (c <= c0, false, Edges::Left),
                            (r == r1, true, Edges::Bottom),
                            (last >= c1, false, Edges::Right),
                        ];
                        let cell = &mut rows[r].cells[k];
                        let mut bits: u8 = if *edges == Edges::None { 48 } else { 0 };
                        for (i, (outside, horizontal, side)) in sides.into_iter().enumerate() {
                            if !want(outside, horizontal, side) {
                                continue;
                            }
                            bits |= 1 << i;
                            *[&mut cell.top, &mut cell.left, &mut cell.bottom, &mut cell.right][i] = Some(value.clone());
                            if outside {
                                let overlaps = |row: usize| {
                                    g.rows
                                        .get(row)
                                        .into_iter()
                                        .flatten()
                                        .enumerate()
                                        .filter(move |(_, (oc, os, _))| *oc + (*os).max(1) > c && *oc <= last)
                                        .map(|(j, _)| j)
                                };
                                match i {
                                    0 if r > 0 => shared.extend(overlaps(r - 1).map(|j| (r - 1, j, 2))),
                                    2 => shared.extend(overlaps(r + 1).map(|j| (r + 1, j, 0))),
                                    1 if k > 0 => shared.push((r, k - 1, 3)),
                                    3 if k + 1 < g.rows[r].len() => shared.push((r, k + 1, 1)),
                                    _ => {}
                                }
                            }
                        }
                        touched.push((r, k, bits));
                    }
                }
                for (r, k, side) in shared {
                    let cell = &mut rows[r].cells[k];
                    *[&mut cell.top, &mut cell.left, &mut cell.bottom, &mut cell.right][side] = Some(value.clone());
                    touched.push((r, k, 1 << side));
                }
                touched.sort_unstable();
                touched.dedup_by(|b, a| {
                    let same = (a.0, a.1) == (b.0, b.1);
                    if same {
                        a.2 |= b.2;
                    }
                    same
                });
                for (r, k, bits) in touched {
                    let cell = &mut rows[r].cells[k];
                    // No Border clears the selected cells' diagonals too.
                    if *edges == Edges::None && in_block(r, g.rows[r][k].0, g.rows[r][k].1) {
                        cell.diagonals = None;
                    }
                    let xml = tc_borders(&cx, cell, bits);
                    set_direct(cell, "tcBorders", xml);
                }
            }
            TableFormat::CellProps(p) => {
                use crate::api::value::TableWidth as W;
                for (r, row) in rows.iter_mut().enumerate() {
                    for (k, cell) in row.cells.iter_mut().enumerate() {
                        let Some(&(c, s, _)) = g.rows[r].get(k) else { continue };
                        if !in_block(r, c, s) {
                            continue;
                        }
                        if let Some(v) = p.width {
                            let width = match v {
                                W::Auto => Width::Auto,
                                W::Points { value } => Width::Dxa(value.0),
                                W::Percent { value } => Width::Pct(value / 100.0),
                            };
                            cell.width = Some(width);
                        }
                        if let Some(d) = p.text_direction {
                            let v = serde_json::to_value(d).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "lrTb".into());
                            set_direct(cell, "textDirection", format!("<{w}textDirection {w}val=\"{v}\"/>"));
                            cell.text_direction = v;
                        }
                        let flag = |name: &str, on: bool| if on { format!("<{w}{name}/>") } else { String::new() };
                        if let Some(wrap) = p.wrap_text {
                            cell.no_wrap = !wrap;
                            set_direct(cell, "noWrap", flag("noWrap", !wrap));
                        }
                        if let Some(on) = p.fit_text {
                            cell.fit_text = on;
                            set_direct(cell, "tcFitText", flag("tcFitText", on));
                        }
                        if let Some(on) = p.hide_mark {
                            cell.hide_mark = on;
                            set_direct(cell, "hideMark", flag("hideMark", on));
                        }
                    }
                }
            }
            TableFormat::RowProps { hidden, exceptions } => {
                for row in &mut rows[r0..=r1] {
                    if let Some(h) = *hidden {
                        let mut p = crate::docx::write::props::Props::parse(row.direct.0.as_deref(), "trPr", &cx);
                        p.set("hidden", if h { format!("<{w}hidden/>") } else { String::new() });
                        row.direct = crate::docx::Raw(p.xml().map(|x| std::rc::Rc::from(x.as_str())));
                        row.hidden = h;
                        row.vanished = h && row_text_hidden(row);
                    }
                    if let Some(ex) = exceptions {
                        set_exceptions(row, ex, &t.borders, &cx);
                    }
                }
            }
            TableFormat::TableShading(sh) => {
                // Written as the table's own: its style's shading stays its style's.
                let mut p = crate::docx::write::props::Props::parse(t.direct.0.as_deref(), "tblPr", &cx);
                p.reorder(crate::docx::write::props::TBLPR);
                let shd = sh.shd(p.get("shd"), t.shd.as_ref());
                p.set("shd", shd_xml(&w, &shd));
                props.direct = crate::docx::Raw(Some(std::rc::Rc::from(p.xml().unwrap_or_default().as_str())));
                props.shd = Some(shd);
                // The whole table takes it, rows read from tables joined to it too.
                rows.iter_mut().for_each(|r| r.table_shading = None);
            }
            &TableFormat::CellSpacing(v) => {
                props.cell_spacing = ((v.max(0.0)) * 20.0).round() / 20.0;
                if props.cell_spacing == t.cell_spacing {
                    return Outcome::Unchanged("already so");
                }
            }
            TableFormat::Alt(a) => {
                let mut alt = t.alt.as_deref().cloned().unwrap_or_default();
                if let Some(v) = &a.title {
                    alt.title = v.trim().to_string();
                }
                if let Some(v) = &a.description {
                    alt.description = v.trim().to_string();
                }
                props.alt = (alt != crate::docx::TableAlt::default()).then(|| std::rc::Rc::new(alt));
            }
            &TableFormat::HeaderRow(on) => {
                rows[..=r1].iter_mut().for_each(|r| r.header = on);
                if !on {
                    rows.iter_mut().for_each(|r| r.header = false);
                }
            }
            TableFormat::RowHeight(h) => {
                let h = h.as_ref().map(|(v, rule)| ((v * 20.0).round() / 20.0, rule.clone()));
                rows[r0..=r1].iter_mut().for_each(|r| set_height(r, h.clone(), &cx));
            }
            &TableFormat::RowCantSplit(on) => {
                rows[r0..=r1].iter_mut().for_each(|r| r.cant_split = on);
            }
            TableFormat::ColumnWidth(v) => {
                let v = ((v.max(1.0)) * 20.0).round() / 20.0;
                let mut gr = t.grid.clone();
                for c in tg.cols.0..=tg.cols.1.min(gr.len().saturating_sub(1)) {
                    gr[c] = v;
                }
                for (r, row) in rows.iter_mut().enumerate() {
                    for (k, cell) in row.cells.iter_mut().enumerate() {
                        if let Some(&(c, s, _)) = g.rows[r].get(k) {
                            cell.width = Some(Width::Dxa((c..c + s.max(1)).filter_map(|i| gr.get(i)).sum()));
                        }
                    }
                }
                if matches!(props.width, Width::Dxa(_)) {
                    props.width = Width::Dxa(gr.iter().sum());
                }
                grid = Some(gr);
            }
            TableFormat::Fit { fixed, width, grid: widths } => {
                let gr: Vec<f64> = widths.iter().map(|v| (v.max(0.0) * 20.0).round() / 20.0).collect();
                for (r, row) in rows.iter_mut().enumerate() {
                    for (k, cell) in row.cells.iter_mut().enumerate() {
                        if let Some(&(c, s, _)) = g.rows[r].get(k) {
                            cell.width = Some(Width::Dxa((c..c + s.max(1)).filter_map(|i| gr.get(i)).sum()));
                        }
                    }
                }
                // As its own too: a style's layout or width doesn't come back when it is resolved again.
                props.fixed = *fixed;
                set_width(&mut props, *width, &cx);
                let layout = if *fixed { "fixed" } else { "autofit" };
                set_own(&mut props, "tblLayout", format!("<{w}tblLayout {w}type=\"{layout}\"/>"), &cx);
                grid = Some(gr);
            }
            TableFormat::Bands(rows_n, cols_n) => {
                let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
                let mut p = crate::docx::write::props::Props::parse(t.direct.0.as_deref(), "tblPr", &cx);
                p.reorder(crate::docx::write::props::TBLPR);
                for (n, name) in [(rows_n, "tblStyleRowBandSize"), (cols_n, "tblStyleColBandSize")] {
                    match n {
                        Some(Some(n)) => p.set(name, format!("<{w}{name} {w}val=\"{n}\"/>")),
                        Some(None) => p.remove(name),
                        None => {}
                    }
                }
                props.direct = crate::docx::Raw(p.xml().map(|x| std::rc::Rc::from(x.as_str())));
                let mut tbl = res.styles.table_region(res.styles.table_style(t.style_id.as_deref()), &[]).tbl.clone();
                let direct = props
                    .direct
                    .0
                    .as_deref()
                    .and_then(|raw| cx.parse(raw, |_, n| crate::docx::props::TblProps::parse(Some(n), &res.styles.colours)))
                    .unwrap_or_default();
                tbl.merge(&direct);
                props.bands = (tbl.row_band.unwrap_or(0), tbl.col_band.unwrap_or(0));
                restyle = true;
            }
            TableFormat::TableWidth(v) => props.width = *v,
            TableFormat::TableAlign(j) => props.jc = *j,
            TableFormat::TableIndent(v) => (props.ind, props.ind_pct, props.ind_set) = ((v * 20.0).round() / 20.0, None, true),
            TableFormat::TableStyle(_) | TableFormat::TableLook(_) => {
                let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
                if let TableFormat::TableStyle(id) = f {
                    if !res.styles.by_id.get(id).is_some_and(|s| s.kind == crate::docx::styles::StyleKind::Table) {
                        return Outcome::Unchanged("no such table style");
                    }
                    props.style_id = Some(id.clone());
                    // Named in its properties too: re-resolving takes the style from them.
                    let mut p = crate::docx::write::props::Props::parse(t.direct.0.as_deref(), "tblPr", &cx);
                    p.reorder(crate::docx::write::props::TBLPR);
                    p.set("tblStyle", format!("<{w}tblStyle {w}val=\"{}\"/>", crate::docx::write::props::esc(id)));
                    props.direct = crate::docx::Raw(p.xml().map(|x| std::rc::Rc::from(x.as_str())));
                    let mut tbl = res.styles.table_region(Some(id), &[]).tbl.clone();
                    let direct = t
                        .direct
                        .0
                        .as_deref()
                        .and_then(|raw| cx.parse(raw, |_, n| crate::docx::props::TblProps::parse(Some(n), &res.styles.colours)))
                        .unwrap_or_default();
                    tbl.merge(&direct);
                    let b = &tbl.borders;
                    props.borders = [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()];
                    props.bands = (tbl.row_band.unwrap_or(0), tbl.col_band.unwrap_or(0));
                    props.shd = tbl.shd.clone().filter(|s| s.flat().is_some() || s.hatch().is_some());
                    if direct.look.is_none() && props.look.is_none() {
                        props.look = tbl.look;
                    }
                }
                if let TableFormat::TableLook(l) = f {
                    props.look = Some(*l);
                }
                restyle = true;
            }
        }
        // The table's and rows' own properties as written too: re-resolving after a style edit takes them from there.
        own_xml(f, &mut props, &mut rows, &t.rows, &cx);
        // `Raw` compares equal whatever it holds: the properties as written count too.
        let same_raw = rows.iter().zip(&t.rows).all(|(a, b)| {
            a.direct.0 == b.direct.0 && a.exceptions.0 == b.exceptions.0 && a.cells.iter().zip(&b.cells).all(|(c, d)| c.direct.0 == d.direct.0)
        });
        let rows_changed = rows != t.rows || !same_raw || grid.is_some();
        let props_changed = props != TableProps::of(t) || props.direct.0 != t.direct.0;
        if !rows_changed && !props_changed {
            return Outcome::Unchanged("already so");
        }
        let n = rows.len();
        let mut tx = self.begin("Table Properties");
        let ok = (!props_changed || self.step(&mut tx, Step::TableProps { table: tg.table, props }).is_ok())
            && (!rows_changed || self.step(&mut tx, Step::Rows { table: tg.table, range: 0..n, with: rows, grid }).is_ok());
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("table edit failed");
        }
        if restyle
            || matches!(
                f,
                TableFormat::CellShading(_)
                    | TableFormat::CellAlign(_)
                    | TableFormat::CellBorders(..)
                    | TableFormat::CellMargins(_)
                    | TableFormat::CellProps(_)
            )
        {
            tx.stale_tables.push(tg.table);
        }
        self.commit(tx, None, fonts)
    }
}
