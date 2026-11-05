//! Table handles

use std::ops::Range;
use std::rc::Rc;

use crate::api::catalog::table as words;
use crate::api::services::MeasureDetail;
use crate::font::Fonts;
use crate::layout::TableId;
use crate::services::ruler::{change, length, number};

use super::Editor;
use super::geometry::{PageGeometry, Zone};
use super::input::Request;
use super::keymap::Mods;
use super::objects::{Dragged, size_label};
use super::resize::Border;
use super::selection::{CellShape, Selection};
use super::snap::{SnapKind, SnapLine, Snapping};
use super::table::Grid;
use super::tables::table_on;
use super::view::{Axis, GuideLine, Guides, Measure, Pt, Px, TableHandles, TableHover, TableMark, TableZone, Tone};

#[derive(Clone, Copy, Debug)]
pub(super) struct TableDrag {
    pub table: TableId,
    pub page: usize,
    pub start: (f64, f64),
    pub now: (f64, f64),
    pub rect: [f64; 4],
    pub snap: [f64; 2],
    pub snapped: [Option<SnapLine>; 2],
}

impl TableDrag {
    pub fn to(&self) -> [f64; 4] {
        let (dx, dy) = (self.now.0 - self.start.0 + self.snap[0], self.now.1 - self.start.1 + self.snap[1]);
        let [x0, y0, x1, y1] = self.rect;
        [x0 + dx, y0 + dy, x1 + dx, y1 + dy]
    }
}

pub(super) type TableFeedback = (Option<TableHandles>, Option<(usize, [f64; 4])>);

impl Editor {
    pub(super) fn shown_table(&self) -> Option<TableHover> {
        self.table_hover.or_else(|| self.table_target().map(|t| TableHover { table: t.table, zone: None }))
    }

    pub(crate) fn table_hover(&self) -> Option<TableHover> {
        self.table_hover
    }

    pub(super) fn table_hover_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<TableHover> {
        if let Some((_, table, border, ..)) = self.border_at(x, y, fonts) {
            let zone = match border {
                Border::Column(k) => Some(TableZone::Border(k)),
                Border::Row(_) => None,
                Border::Handle => Some(TableZone::Resize),
            };
            return Some(TableHover { table, zone });
        }
        if let Some(z) = self.zone_hit(x, y, fonts) {
            let (table, zone) = match z {
                Zone::Cell(t, ..) => (t, None),
                Zone::Row(t, r) => (t, Some(TableZone::SelectRow(r))),
                Zone::Column(t, r, c) => (t, self.index.table(&self.doc, t).and_then(|tb| Grid::of(tb).col_of(r, c)).map(TableZone::SelectColumn)),
                Zone::Table(t) => (t, Some(TableZone::Move)),
                Zone::InsertColumn(t, k) => (t, Some(TableZone::InsertColumn(k))),
                Zone::InsertRow(t, k) => (t, Some(TableZone::InsertRow(k))),
            };
            return Some(TableHover { table, zone });
        }
        let (page, px, py) = self.stack().page_at(x, y)?;
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        geo.rows
            .iter()
            .filter(|r| super::stories::reachable(ctx, geo.stories[r.story].host))
            .filter(|r| px >= r.rect[0] && px <= r.rect[2] && py >= r.rect[1] && py <= r.rect[3])
            .max_by_key(|r| r.outer.len())
            .map(|r| TableHover { table: r.table, zone: None })
    }

    pub(super) fn table_press(&mut self, table: TableId, x: f64, y: f64) {
        let Some((page, ..)) = self.stack().page_at(x, y) else { return };
        let Some(rect) = self.pages.page(page, &self.index).and_then(|g| table_on(&g, table)) else { return };
        self.table_drag = Some(TableDrag { table, page, start: (x, y), now: (x, y), rect, snap: [0.0; 2], snapped: [None; 2] });
        self.build_snaps(page, Dragged::Table(table));
        self.requests.push(Request::CapturePointer);
    }

    pub(super) fn table_drag_move(&mut self, x: f64, y: f64, mods: Mods) {
        let Some(mut d) = self.table_drag else { return };
        d.now = (x, y);
        (d.snap, d.snapped) = ([0.0; 2], [None; 2]);
        if !mods.alt {
            (d.snap, d.snapped) = super::snap::snap(d.to(), &self.snaps, self.snap_threshold());
        }
        self.table_drag = Some(d);
    }

    pub(super) fn table_drag_release(&mut self, fonts: &Fonts) {
        let Some(d) = self.table_drag.take() else { return };
        self.requests.push(Request::ReleasePointer);
        if d.now == d.start {
            return;
        }
        let Some(pos) = self.index.table(&self.doc, d.table).map(|t| t.floating.unwrap_or(super::tables::NEW_POSITION)) else { return };
        let [x, y, ..] = d.to();
        self.position_table(d.table, pos, [Some(x), Some(y)], d.page, "Move Table", fonts);
    }

    fn table_page(&mut self, table: TableId, visible: &Range<usize>) -> Option<(usize, Rc<PageGeometry>, [f64; 4])> {
        let under = self.hover.and_then(|(x, y)| self.stack().page_at(x, y)).map(|(p, ..)| p);
        under.into_iter().chain(visible.clone()).find_map(|p| {
            let geo = self.pages.page(p, &self.index)?;
            let r = table_on(&geo, table)?;
            Some((p, geo, r))
        })
    }

    pub(super) fn table_feedback(
        &mut self,
        visible: Range<usize>,
        guides: &mut Vec<(usize, Guides)>,
        measures: &mut Vec<(usize, Measure)>,
    ) -> TableFeedback {
        let set = self.view_settings;
        let px = 0.75 / self.viewport.zoom.max(0.1);
        let full = |s: Snapping| set.detail == MeasureDetail::Full || (set.detail == MeasureDetail::Standard && s == Snapping::Free);
        let mut preview = None;
        if let Some(d) = self.border_drag.filter(|d| d.now != d.start)
            && let Some(geo) = self.pages.page(d.page, &self.index)
            && let Some(tb) = table_on(&geo, d.table)
        {
            let g = d.guide();
            match d.border {
                Border::Column(k) => {
                    let row = geo.rows.iter().find(|r| r.table == d.table && r.row == d.row && !r.repeat);
                    let edge = |j: usize| row.and_then(|r| r.edges.iter().find(|e| e.1 == j)).map(|e| e.0);
                    let x = g[0];
                    let right = (!d.shift && !d.ctrl).then(|| edge(k + 1)).flatten().map(|b| (x, b));
                    let widths = [k.checked_sub(1).and_then(edge).map(|a| (a, x)), right];
                    if set.detail == MeasureDetail::Quiet {
                        if let Some((a, b)) = widths.into_iter().flatten().next() {
                            let at = [Px::of(Pt(x + 8.0 * px)), Px::of(Pt(tb[1] - 24.0 * px))];
                            measures.push((d.page, Measure::Chip { at, label: length(b - a, set.unit) }));
                        }
                    } else {
                        let y = Pt(tb[1] - 12.0 * px);
                        for (a, b) in widths.into_iter().flatten() {
                            let label = number(b - a, set.unit);
                            measures.push((d.page, Measure::Dimension { from: [Pt(a), y], to: [Pt(b), y], label, tone: Tone::Accent }));
                        }
                        if full(d.snapping) {
                            let free = match d.snapping {
                                Snapping::On => words::ALT_FREE,
                                Snapping::Free => words::FREE,
                            };
                            let label = format!("Δ {} · {free}", change(x - d.at, set.unit));
                            let at = [Pt((x - 40.0 * px).max(tb[0])), Pt(tb[3] + 10.0 * px)];
                            measures.push((d.page, Measure::Status { at, label, tone: Tone::Accent }));
                        }
                    }
                }
                Border::Handle => {
                    let r = [d.span[0], d.row_top, g[0], g[3]];
                    preview = Some((d.page, r));
                    let label = size_label(r[2] - r[0], r[3] - r[1], set.unit);
                    let w = label.chars().count() as f64 * 6.0 * px;
                    measures.push((d.page, Measure::Status { at: [Pt(r[2] - w), Pt(r[3] + 10.0 * px)], label, tone: Tone::Accent }));
                }
                Border::Row(_) => {}
            }
        }
        if let Some(d) = self.table_drag.filter(|d| d.now != d.start) {
            preview = Some((d.page, d.to()));
            let [w, h] = self.snaps.size;
            let lines = d
                .snapped
                .iter()
                .zip([(Axis::X, (Pt(0.0), Pt(h))), (Axis::Y, (Pt(0.0), Pt(w)))])
                .filter_map(|(l, (axis, span))| l.filter(|l| l.kind != SnapKind::Grid).map(|l| GuideLine { axis, at: l.at, span }))
                .collect();
            guides.push((d.page, Guides { lines, grid: self.snaps.grid, anchor_link: None }));
        }
        let still = self.border_drag.is_none() && self.table_drag.is_none();
        if set.detail == MeasureDetail::Full
            && still
            && let Selection::Cells { table, .. } = self.selection
            && let Some((page, geo, tb)) = self.table_page(table, &visible)
            && let Some(label) = self.status_label(&geo)
        {
            measures.push((page, Measure::Status { at: [Pt(tb[0]), Pt(tb[3] + 10.0 * px)], label, tone: Tone::Accent }));
        }
        let handles = match self.shown_table() {
            Some(h) if still && self.zone_drag.is_none() && self.object_drag.is_none() => self.handles(h, &visible),
            _ => None,
        };
        (handles, preview)
    }

    fn status_label(&self, geo: &PageGeometry) -> Option<String> {
        let Selection::Cells { table, anchor, focus, shape } = self.selection else { return None };
        let t = self.index.table(&self.doc, table)?;
        let g = Grid::of(t);
        let unit = self.view_settings.unit;
        match shape {
            CellShape::Columns if anchor.col == focus.col => {
                let c = anchor.col.min(g.cols.checked_sub(1)?);
                let drawn = geo.rows.iter().filter(|r| r.table == table && !r.repeat).find_map(|r| {
                    let at = |k: usize| r.edges.iter().find(|e| e.1 == k).map(|e| e.0);
                    Some(at(c + 1)? - at(c)?)
                });
                let width = drawn.or_else(|| t.grid.get(c).copied())?;
                let cells = (0..t.rows.len()).filter(|&r| g.cell_at(r, c).is_some_and(|k| !g.continues(r, k))).count();
                Some(format!("{} {} · {} · {cells} {} · {}", words::COLUMN, c + 1, length(width, unit), words::CELLS, words::DELETE))
            }
            CellShape::Rows if anchor.row == focus.row => {
                let row = t.rows.get(anchor.row)?;
                let delete = if row.header { String::new() } else { format!(" · {}", words::DELETE) };
                Some(format!("{} {} · {} {}{delete}", words::ROW, anchor.row + 1, row.cells.len(), words::CELLS))
            }
            CellShape::Table => Some(format!("{} · {} × {}", words::TABLE, g.cols, t.rows.len())),
            CellShape::Columns | CellShape::Rows | CellShape::Rect => None,
        }
    }

    fn handles(&mut self, h: TableHover, visible: &Range<usize>) -> Option<TableHandles> {
        let (page, geo, [x0, y0, x1, y1]) = self.table_page(h.table, visible)?;
        let rows: Vec<&super::geometry::GRow> = geo.rows.iter().filter(|r| r.table == h.table && !r.repeat).collect();
        let first = rows.iter().find(|r| r.row == 0);
        let last = rows.iter().max_by_key(|r| r.row)?;
        let live = last.outer.is_empty() || self.held_tables().contains(&h.table);
        let edge = |r: &super::geometry::GRow, k: usize| r.edges.iter().find(|e| e.1 == k).map(|e| e.0);
        let mark = match h.zone {
            Some(TableZone::InsertColumn(k)) => first.and_then(|r| edge(r, k)).map(|x| TableMark::InsertColumn { x, top: y0, bottom: y1 }),
            Some(TableZone::InsertRow(k)) => rows.iter().find(|r| r.row + 1 == k).map(|r| TableMark::InsertRow { y: r.rect[3], left: x0, right: x1 }),
            Some(TableZone::SelectColumn(c)) => first.and_then(|r| {
                let a = edge(r, c)?;
                let b = r.edges.iter().filter(|e| e.1 > c).map(|e| e.0).fold(f64::INFINITY, f64::min);
                b.is_finite().then_some(TableMark::SelectColumn { x0: a, x1: b, top: y0 })
            }),
            Some(TableZone::SelectRow(r)) => {
                rows.iter().find(|x| x.row == r).map(|x| TableMark::SelectRow { y0: x.rect[1], y1: x.rect[3], left: x0 })
            }
            Some(TableZone::Border(k)) => rows.iter().find_map(|r| edge(r, k)).map(|x| TableMark::Border { x, top: y0, bottom: y1 }),
            Some(TableZone::Move | TableZone::Resize) | None => None,
        };
        Some(TableHandles { page, corner: first.map(|r| [r.rect[0], r.rect[1]]), end: live.then_some([last.rect[2], last.rect[3]]), mark })
    }
}
