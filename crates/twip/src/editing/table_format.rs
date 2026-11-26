//! Table formatting commands

use crate::docx::props::{Border, Jc, Look, Width};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::step::{Step, TableProps};
use super::table::Grid;

/// A table's band sizes ([`Editor::table_band_sizes`]).
pub use crate::api::state::TableBands;

#[derive(Clone, Debug, PartialEq)]
pub enum TableFormat {
    CellAlign(crate::docx::VerticalAlignment),
    CellShading(Shading),
    TableShading(Shading),
    CellBorders(Edges, Border),
    CellMargins([f64; 4]),
    HeaderRow(bool),
    RowHeight(Option<(f64, crate::docx::HeightRule)>),
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
    /// The table's default cell margins: top, left, bottom, right.
    TableCellMargins([Option<f64>; 4]),
    Alt(crate::api::command::TableAltText),
    /// AutoFit to the window or to fixed widths: the layout, the table's width and its columns' widths.
    Fit {
        fixed: bool,
        width: Width,
        grid: Vec<f64>,
    },
}

/// The model's cell alignment for the API's.
pub(super) fn cell_alignment(a: crate::api::value::CellAlignment) -> crate::docx::VerticalAlignment {
    use crate::api::value::CellAlignment as A;
    use crate::docx::VerticalAlignment as V;
    match a {
        A::Top => V::Top,
        A::Center => V::Center,
        A::Bottom => V::Bottom,
    }
}

/// The API's cell alignment for the model's (`None`: top).
pub(super) fn api_cell_alignment(v: Option<crate::docx::VerticalAlignment>) -> crate::api::value::CellAlignment {
    use crate::api::value::CellAlignment as A;
    use crate::docx::VerticalAlignment as V;
    match v {
        Some(V::Center) => A::Center,
        Some(V::Bottom) => A::Bottom,
        Some(V::Top | V::Both) | None => A::Top,
    }
}

/// A `w:vAlign` element.
pub(super) fn v_align_xml(w: &str, v: crate::docx::VerticalAlignment) -> String {
    format!("<{w}vAlign {w}val=\"{}\"/>", crate::docx::section::v_align_literal(v))
}

// Typed edits of the authored properties

/// Points as whole twips, as the table commands write them.
pub(super) fn twips(v: f64) -> f64 {
    (v * 20.0).round() / 20.0
}

/// A width as the table commands write it: whole twips, or whole fiftieths of a percent.
pub(super) fn written_width(w: Width) -> Width {
    match w {
        Width::Dxa(v) => Width::Dxa(twips(v)),
        Width::Pct(f) => Width::Pct((f * 5000.0).round() / 5000.0),
        w => w,
    }
}

/// A model colour as the authored one the table commands write: none as automatic, a theme colour as itself.
fn authored_colour(rgb: Option<u32>, theme: Option<crate::docx::model::ThemeColourRef>) -> crate::format::Color {
    match (rgb, theme) {
        (None, _) => crate::format::Color::Auto,
        (Some(_), Some(r)) => crate::format::Color::Theme(r),
        (Some(v), None) => crate::format::Color::Rgb(v),
    }
}

/// A model border as the authored one the table commands write: a side of no line as nil.
pub(super) fn authored_border(b: &Border) -> crate::format::TextBorder {
    if !b.visible() {
        return crate::format::TextBorder { style: crate::docx::props::BorderStyle::Nil, width: 0.0, space: 0.0, shadow: false, color: None };
    }
    crate::format::TextBorder {
        style: b.style,
        width: b.width,
        space: b.space,
        shadow: b.shadow,
        color: Some(authored_colour(b.color, b.color_theme)),
    }
}

/// Model shading as the authored shading the table commands write.
pub(super) fn authored_shading(s: &crate::docx::props::Shd) -> crate::format::Shading {
    crate::format::Shading {
        pattern: s.pattern,
        color: Some(authored_colour(s.color, s.color_theme)),
        fill: Some(authored_colour(s.fill, s.fill_theme)),
    }
}

/// A table's authored properties edited by `edit`.
pub(super) fn edit_table(props: &mut TableProps, edit: impl FnOnce(&mut crate::format::TableEdit)) {
    let mut e = crate::format::TableEdit::new((**props.direct.get()).clone());
    edit(&mut e);
    props.direct = props.direct.edit(e);
}

/// A row's authored properties edited by `edit`.
pub(super) fn edit_row(row: &mut crate::docx::Row, edit: impl FnOnce(&mut crate::format::RowEdit)) {
    let mut e = crate::format::RowEdit::new((**row.direct.get()).clone());
    edit(&mut e);
    row.direct = row.direct.edit(e);
}

/// A cell's authored properties edited by `edit`.
fn edit_cell(cell: &mut crate::docx::Cell, edit: impl FnOnce(&mut crate::format::CellEdit)) {
    let mut e = crate::format::CellEdit::new((**cell.direct.get()).clone());
    edit(&mut e);
    cell.direct = cell.direct.edit(e);
}

/// Sets `row`'s height (`None`: auto) on the model and as its own.
pub(super) fn set_height(row: &mut crate::docx::Row, h: Option<(f64, crate::docx::HeightRule)>) {
    edit_row(row, |e| {
        e.height = h.map(|(v, rule)| crate::format::RowHeight { value: twips(v), rule });
        e.whole(crate::format::RowProp::Height);
    });
    row.height = h;
}

/// `width` as the table's on the model and as its own.
pub(super) fn set_width(props: &mut TableProps, width: Width) {
    props.width = width;
    edit_table(props, |e| {
        e.width = Some(written_width(width));
        e.whole(crate::format::TableProp::Width);
    });
}

/// `ind` (points) as the table's indent on the model and as its own.
pub(super) fn set_indent(props: &mut TableProps, ind: f64) {
    (props.ind, props.ind_pct, props.ind_set) = (twips(ind), None, true);
    let v = props.ind;
    edit_table(props, |e| {
        e.indent = Some(Width::Dxa(v));
        e.whole(crate::format::TableProp::Indent);
    });
}

/// The table's layout (fixed or fitted to the content) as its own.
pub(super) fn set_layout(props: &mut TableProps, fixed: bool) {
    edit_table(props, |e| {
        e.layout = Some(if fixed { crate::format::TableLayout::Fixed } else { crate::format::TableLayout::Autofit });
        e.whole(crate::format::TableProp::Layout);
    });
}

/// What table formatting `f` set on the model, written as the table's own properties (`props`) and the own properties
/// of its `rows` it changed (from `was`).
fn own_xml(f: &TableFormat, props: &mut TableProps, rows: &mut [crate::docx::Row], was: &[crate::docx::Row]) {
    use crate::format::{RowProp, TableProp};
    match f {
        TableFormat::TableAlign(_) => {
            let jc = match props.jc {
                Jc::Center => Jc::Center,
                Jc::Right => Jc::Right,
                _ => Jc::Left,
            };
            edit_table(props, |e| {
                e.align = Some(jc);
                e.whole(TableProp::Align);
            });
        }
        TableFormat::TableIndent(_) => {
            let v = props.ind;
            edit_table(props, |e| {
                e.indent = Some(written_width(Width::Dxa(v)));
                e.whole(TableProp::Indent);
            });
        }
        TableFormat::TableWidth(_) | TableFormat::ColumnWidth(_) => {
            let v = props.width;
            edit_table(props, |e| {
                e.width = Some(written_width(v));
                e.whole(TableProp::Width);
            });
        }
        // Written when zero too: without its own, the table takes its style's.
        TableFormat::CellSpacing(_) => {
            let v = props.cell_spacing;
            edit_table(props, |e| {
                e.cell_spacing = Some(written_width(Width::Dxa(v)));
                e.whole(TableProp::CellSpacing);
            });
        }
        _ => {}
    }
    for (r, old) in rows.iter_mut().zip(was) {
        if (&r.height, r.cant_split, r.header) == (&old.height, old.cant_split, old.header) {
            continue;
        }
        let (prop, on) = match f {
            TableFormat::RowCantSplit(_) => (RowProp::CantSplit, r.cant_split),
            TableFormat::HeaderRow(_) => (RowProp::Header, r.header),
            _ => continue,
        };
        edit_row(r, |e| {
            match prop {
                RowProp::CantSplit => e.cant_split = Some(on),
                _ => e.header = Some(on),
            }
            e.whole(prop);
        });
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
        use crate::docx::line_style::shading_pattern;
        self.shd_over(was.and_then(|x| crate::docx::write::props::attr_of(x, "val")).map(|v| shading_pattern(&v)), inherited)
    }

    /// What it reads as over `was`, the pattern set there, and `inherited`, the shading its style gives.
    pub fn shd_over(&self, was: Option<crate::docx::props::ShadingPattern>, inherited: Option<&crate::docx::props::Shd>) -> crate::docx::props::Shd {
        use crate::docx::line_style::shading_pattern;
        let pattern = self
            .pattern
            .map(shading_pattern)
            .or(was)
            .or_else(|| inherited.map(|s| s.pattern).filter(|p| *p != crate::docx::props::ShadingPattern::Nil));
        crate::docx::props::Shd {
            pattern: pattern.unwrap_or(crate::docx::props::ShadingPattern::Clear),
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
    let tag = format!("<{w}shd {w}val=\"{}\"/>", crate::docx::line_style::shading_val(s.pattern));
    let tag = colour_attrs(&tag, w, "color", THEME_COLOR, c(s.color, s.color_theme));
    colour_attrs(&tag, w, "fill", THEME_FILL, c(s.fill, s.fill_theme))
}

/// Row exceptions to set: `reset` drops the rows' own first.
#[derive(Clone, Debug, PartialEq)]
pub struct RowEx {
    pub reset: bool,
    pub jc: Option<Jc>,
    pub cell_spacing: Option<f64>,
    /// The rows' indent from the table's left edge.
    pub ind: Option<f64>,
    pub shading: Option<Shading>,
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
    Border { style: crate::docx::props::BorderStyle::Nil, width: 0.0, space: 0.0, shadow: false, color: None, color_theme: None }
}

pub(super) fn border_xml(w: &str, name: &str, b: &Border) -> String {
    if !b.visible() {
        return format!("<{w}{name} {w}val=\"nil\"/>");
    }
    let tag = format!(
        "<{w}{name} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"{}\"{}/>",
        crate::docx::line_style::border_val(b.style),
        (b.width * 8.0).round() as i64,
        b.space.round() as i64,
        if b.shadow { format!(" {w}shadow=\"1\"") } else { String::new() }
    );
    let c = b.color.map(|rgb| WordColour { rgb, theme: b.color_theme });
    super::commands::colour_attrs(&tag, w, "color", super::commands::THEME_COLOR, c)
}

/// The sides in `sides` (bits: top, left, bottom, right, then the two diagonals) of a cell's own borders set from its
/// model borders: the others stay as they were, a style's staying its style's.
fn set_own_sides(cell: &mut crate::docx::Cell, sides: u8) {
    use crate::format::TableSide;
    let [tl2br, tr2bl] = cell.diagonals.as_deref().cloned().unwrap_or_default();
    let model = [cell.top.clone(), cell.left.clone(), cell.bottom.clone(), cell.right.clone(), tl2br, tr2bl];
    edit_cell(cell, |e| {
        let names = [TableSide::Top, TableSide::Left, TableSide::Bottom, TableSide::Right, TableSide::Tl2br, TableSide::Tr2bl];
        for (bit, (side, b)) in names.into_iter().zip(&model).enumerate() {
            if sides & (1 << bit) != 0 {
                e.border(side, b.as_ref().map(authored_border));
            }
        }
    });
}

// A cell's sides, as [`set_cell_sides`] takes them
pub(super) const TOP: u8 = 1;
pub(super) const LEFT: u8 = 2;
pub(super) const BOTTOM: u8 = 4;
pub(super) const RIGHT: u8 = 8;
pub(super) const DIAGONAL_DOWN: u8 = 16;
pub(super) const DIAGONAL_UP: u8 = 32;

/// Sets a cell's `sides` to `b`, in the model and in its `w:tcBorders`.
pub(super) fn set_cell_sides(cell: &mut crate::docx::Cell, sides: u8, b: &Border) {
    for (i, side) in [&mut cell.top, &mut cell.left, &mut cell.bottom, &mut cell.right].into_iter().enumerate() {
        if sides & (1 << i) != 0 {
            *side = Some(b.clone());
        }
    }
    if sides & (DIAGONAL_DOWN | DIAGONAL_UP) != 0 {
        let mut d = cell.diagonals.as_deref().cloned().unwrap_or_default();
        for (k, line) in d.iter_mut().enumerate() {
            if sides & (DIAGONAL_DOWN << k) != 0 {
                *line = Some(b.clone());
            }
        }
        cell.diagonals = Some(std::rc::Rc::new(d));
    }
    set_own_sides(cell, sides);
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
fn set_exceptions(row: &mut crate::docx::Row, ex: &RowEx, t: &crate::docx::Table, style_jc: Option<Jc>) {
    use crate::format::{TableBorders, TableLayout, TableProp};
    let table = &t.borders;
    let base = if ex.reset { crate::docx::table_props::ExceptionsDirect::default() } else { row.exceptions.clone() };
    let mut e = crate::format::TableEdit::new((**base.get()).clone());
    if ex.reset {
        (row.shading, row.hatch, row.cell_spacing, row.borders, row.ind, row.run) = (None, None, None, None, None, None);
        // Its own row properties' alignment, or its style's
        let own = row.direct.get().align;
        row.jc = own.or(style_jc);
    }
    let jc = ex.jc.map(|j| match j {
        Jc::Center => Jc::Center,
        Jc::Right => Jc::Right,
        _ => Jc::Left,
    });
    if let Some(j) = ex.jc {
        e.align = jc;
        e.whole(TableProp::Align);
        row.jc = Some(j);
    }
    let spacing = ex.cell_spacing.map(|s| twips(s.max(0.0)));
    if let Some(s) = spacing {
        e.cell_spacing = Some(Width::Dxa(s));
        e.whole(TableProp::CellSpacing);
        row.cell_spacing = Some(s);
    }
    // A row's own indent: the row starts a layout run of its own (fixed, at its width), as Word reads one.
    let ind = ex.ind.filter(|_| !t.fixed).map(twips);
    if let Some(i) = ind {
        e.indent = Some(Width::Dxa(i));
        e.whole(TableProp::Indent);
        e.layout = Some(TableLayout::Fixed);
        e.whole(TableProp::Layout);
        row.ind = Some(i);
        row.run = row.run.or(Some(t.width));
    }
    let shd = ex.shading.as_ref().map(|sh| sh.shd_over(e.shading.map(|s| s.pattern), None));
    if let Some(shd) = &shd {
        e.shading = Some(authored_shading(shd));
        e.whole(TableProp::Shading);
        (row.shading, row.hatch) = (shd.flat(), shd.hatch());
    }
    let borders = ex.borders.as_ref().map(|(edges, b)| {
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
        now
    });
    if let Some(now) = &borders {
        let side = |k: usize| now[k].as_ref().map(authored_border);
        e.borders = TableBorders {
            top: side(0),
            left: side(1),
            bottom: side(2),
            right: side(3),
            inside_h: side(4),
            inside_v: side(5),
            ..TableBorders::default()
        };
        e.whole(TableProp::Borders);
        row.borders = Some(std::rc::Rc::new(now.clone()));
    }
    row.exceptions = base.edit(e);
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
        if let Some(out) = self.refuse_like(&crate::api::command::Command::SetTableBandSizes(Default::default())) {
            return out;
        }
        self.table_format(&TableFormat::Bands(rows, columns), fonts)
    }

    /// The selection's table's band sizes as it lays out, and its own ones (none: its style's).
    pub fn table_band_sizes(&self) -> Option<TableBands> {
        let tg = self.table_target()?;
        let t = self.index.table(&self.doc, tg.table)?;
        let res = self.doc.resolver.as_ref()?;
        let _ = res;
        let own = t.direct.get();
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

    pub(super) fn table_format_on(&mut self, tg: super::tables::Target, f: &TableFormat, fonts: &Fonts) -> Outcome {
        self.table_format_blocks(&[tg], f, fonts)
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
        // One edit per table, over every block of it.
        let mut by_table: Vec<Vec<super::tables::Target>> = Vec::new();
        for t in targets {
            match by_table.iter_mut().find(|g| g[0].table == t.table) {
                Some(g) => g.push(t),
                None => by_table.push(vec![t]),
            }
        }
        self.begin_batch(Some("Table Properties".into()));
        for blocks in by_table {
            self.table_format_blocks(&blocks, f, fonts);
        }
        self.end_batch(true, fonts)
    }

    /// Formats `f` over `blocks`, blocks of cells of one table, in one edit of its rows.
    fn table_format_blocks(&mut self, blocks: &[super::tables::Target], f: &TableFormat, fonts: &Fonts) -> Outcome {
        let tg = blocks[0];
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let mut rows = t.rows.clone();
        let mut grid = None;
        let mut props = TableProps::of(t);
        let mut restyle = false;
        let colours = self.colours();
        let in_one = |tg: &super::tables::Target, r: usize, c: usize, s: usize| {
            (tg.rows.0..=tg.rows.1).contains(&r) && c + s.max(1) > tg.cols.0 && c <= tg.cols.1
        };
        let in_block = |r: usize, c: usize, s: usize| blocks.iter().any(|tg| in_one(tg, r, c, s));
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
                                cell.v_align = *v;
                                edit_cell(cell, |e| {
                                    e.v_align = Some(*v);
                                    e.whole(crate::format::CellProp::VAlign);
                                });
                            }
                            TableFormat::CellShading(sh) => {
                                let inherited = if sh.pattern.is_none() { self.cell_shd(t, (r, k), cell) } else { None };
                                let shd = sh.shd_over(cell.direct.get().shading.map(|s| s.pattern), inherited.as_ref());
                                (cell.shading, cell.hatch) = (shd.flat(), shd.hatch());
                                edit_cell(cell, |e| {
                                    e.shading = Some(authored_shading(&shd));
                                    e.whole(crate::format::CellProp::Shading);
                                });
                            }
                            TableFormat::CellMargins(m) => {
                                cell.margins = *m;
                                let side = |v: f64| Some(Width::Dxa(twips(v)));
                                let margins = crate::format::CellMargins { top: side(m[0]), left: side(m[1]), bottom: side(m[2]), right: side(m[3]) };
                                edit_cell(cell, |e| {
                                    e.margins = margins;
                                    e.whole(crate::format::CellProp::Margins);
                                });
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
                        set_own_sides(cell, 16 << k);
                    }
                }
            }
            TableFormat::CellBorders(edges, b) => {
                for &tg in blocks {
                    let (r0, r1) = (tg.rows.0, tg.rows.1.min(rows.len().saturating_sub(1)));
                    let in_block = |r: usize, c: usize, s: usize| in_one(&tg, r, c, s);
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
                        set_own_sides(cell, bits);
                    }
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
                            let flow = crate::docx::model::text_flow(&v);
                            edit_cell(cell, |e| {
                                e.text_flow = Some(flow);
                                e.whole(crate::format::CellProp::TextFlow);
                            });
                            cell.text_direction = v;
                        }
                        let on_off = |on: bool| on.then_some(true);
                        if let Some(wrap) = p.wrap_text {
                            cell.no_wrap = !wrap;
                            edit_cell(cell, |e| {
                                e.no_wrap = on_off(!wrap);
                                e.whole(crate::format::CellProp::NoWrap);
                            });
                        }
                        if let Some(on) = p.fit_text {
                            cell.fit_text = on;
                            edit_cell(cell, |e| {
                                e.fit_text = on_off(on);
                                e.whole(crate::format::CellProp::FitText);
                            });
                        }
                        if let Some(on) = p.hide_mark {
                            cell.hide_mark = on;
                            edit_cell(cell, |e| {
                                e.hide_mark = on_off(on);
                                e.whole(crate::format::CellProp::HideMark);
                            });
                        }
                    }
                }
            }
            TableFormat::RowProps { hidden, exceptions } => {
                let style_jc = match (exceptions, &self.doc.resolver) {
                    (Some(x), Some(res)) if x.reset => res.cascade().table_region(t.style_id.as_deref(), &[]).format.row.align,
                    _ => None,
                };
                for row in &mut rows[r0..=r1] {
                    if let Some(h) = *hidden {
                        edit_row(row, |e| {
                            e.hidden = h.then_some(true);
                            e.whole(crate::format::RowProp::Hidden);
                        });
                        row.hidden = h;
                        row.vanished = h && row_text_hidden(row);
                    }
                    if let Some(ex) = exceptions {
                        set_exceptions(row, ex, t, style_jc);
                    }
                }
            }
            TableFormat::TableShading(sh) => {
                // Written as the table's own: its style's shading stays its style's.
                let shd = sh.shd_over(t.direct.get().shading.map(|s| s.pattern), t.shd.as_ref());
                edit_table(&mut props, |e| {
                    e.shading = Some(authored_shading(&shd));
                    e.whole(crate::format::TableProp::Shading);
                });
                props.shd = Some(shd);
                // The whole table takes it, rows read from tables joined to it too.
                rows.iter_mut().for_each(|r| r.table_shading = None);
            }
            TableFormat::TableCellMargins(m) => {
                use crate::format::MarginSide;
                edit_table(&mut props, |e| {
                    for (side, v) in MarginSide::ALL.into_iter().zip(m) {
                        if let Some(v) = v {
                            e.margin(side, Some(Width::Dxa(twips(*v))));
                        }
                    }
                });
                restyle = true;
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
                let h = h.as_ref().map(|(v, rule)| ((v * 20.0).round() / 20.0, *rule));
                rows[r0..=r1].iter_mut().for_each(|r| set_height(r, h));
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
                set_width(&mut props, *width);
                set_layout(&mut props, *fixed);
                grid = Some(gr);
            }
            TableFormat::Bands(rows_n, cols_n) => {
                let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
                edit_table(&mut props, |e| {
                    if let Some(n) = rows_n {
                        e.row_band = n.map(i64::from);
                        e.whole(crate::format::TableProp::RowBand);
                    }
                    if let Some(n) = cols_n {
                        e.col_band = n.map(i64::from);
                        e.whole(crate::format::TableProp::ColBand);
                    }
                });
                let c = res.cascade();
                let tbl = crate::docx::table_props::table_on(
                    &c.table_region(c.table_style(t.style_id.as_deref()), &[]).format.table,
                    props.direct.get(),
                    &colours,
                );
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
                    edit_table(&mut props, |e| {
                        e.style = Some(id.as_str().into());
                        e.whole(crate::format::TableProp::Style);
                    });
                    let tbl = crate::docx::table_props::table_on(&res.cascade().table_region(Some(id), &[]).format.table, t.direct.get(), &colours);
                    let direct = t.direct.get();
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
        own_xml(f, &mut props, &mut rows, &t.rows);
        // Authored properties compare equal whatever they hold: they count too.
        let same_authored = rows.iter().zip(&t.rows).all(|(a, b)| {
            a.direct.authored_eq(&b.direct)
                && a.exceptions.authored_eq(&b.exceptions)
                && a.cells.iter().zip(&b.cells).all(|(c, d)| c.direct.authored_eq(&d.direct))
        });
        let rows_changed = rows != t.rows || !same_authored || grid.is_some();
        let props_changed = props != TableProps::of(t) || !props.direct.authored_eq(&t.direct);
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
