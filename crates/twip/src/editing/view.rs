//! Frame view

use std::rc::Rc;

use crate::layout::Snapshot;

use super::position::StoryId;

pub const PAGE_GAP: f64 = 18.0;

/// The pages one above the other: each page's place (left, top, width, height), and with white space hidden the part
/// of each page that shows (from its body's top to its bottom, page coordinates), the pages' shown parts touching.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageStack {
    pub pages: Vec<[f64; 4]>,
    pub width: f64,
    pub height: f64,
    /// With white space hidden, each page's shown part (top, bottom, page coordinates); empty when shown.
    pub bands: Vec<[f64; 2]>,
}

impl PageStack {
    pub fn of(snapshot: &Snapshot) -> PageStack {
        let width = snapshot.pages().iter().map(|p| p.w).fold(0.0, f64::max) + 2.0 * PAGE_GAP;
        let mut y = PAGE_GAP;
        let pages = snapshot
            .pages()
            .iter()
            .map(|p| {
                let r = [(width - p.w) / 2.0, y, p.w, p.h];
                y += p.h + PAGE_GAP;
                r
            })
            .collect();
        PageStack { pages, width, height: y, bands: Vec::new() }
    }

    /// The pages with the white space between them hidden: each shows only what is between the heights `margins`
    /// gives for the page's index (from its top, from its bottom), the headers and footers above and below left out.
    pub fn hiding_white_space(snapshot: &Snapshot, margins: impl Fn(usize) -> (f64, f64)) -> PageStack {
        let width = snapshot.pages().iter().map(|p| p.w).fold(0.0, f64::max) + 2.0 * PAGE_GAP;
        let mut y = PAGE_GAP;
        let (mut pages, mut bands) = (Vec::with_capacity(snapshot.pages().len()), Vec::with_capacity(snapshot.pages().len()));
        for (i, p) in snapshot.pages().iter().enumerate() {
            let (top, bottom) = margins(i);
            let (top, bottom) = if top >= 0.0 && top < p.h - bottom { (top, p.h - bottom.max(0.0)) } else { (0.0, p.h) };
            pages.push([(width - p.w) / 2.0, y - top, p.w, p.h]);
            bands.push([top, bottom]);
            y += bottom - top;
        }
        PageStack { pages, width, height: y + PAGE_GAP, bands }
    }

    /// Page `i`'s part that shows, in the stack (left, top, width, height).
    pub fn shown(&self, i: usize) -> Option<[f64; 4]> {
        let [x, y, w, h] = *self.pages.get(i)?;
        Some(match self.bands.get(i) {
            Some(&[top, bottom]) => [x, y + top, w, bottom - top],
            None => [x, y, w, h],
        })
    }

    /// Whether height `y` is between pages, or above the first or below the last: on no page's shown part (the
    /// gap runs the stack's whole width, as Word's does).
    pub fn in_gap(&self, y: f64) -> bool {
        (0..self.pages.len()).filter_map(|i| self.shown(i)).all(|[_, top, _, h]| y < top || y > top + h)
    }

    /// Where a double-click shows or hides the white space between pages: between two pages, above the first or below
    /// the last; with it hidden, within `tol` of the line where two pages meet, or of the first page's top or the last
    /// page's bottom (the only targets of a one-page document). Any `x` across the stack.
    pub fn toggles_white_space(&self, y: f64, tol: f64) -> bool {
        if self.bands.is_empty() {
            return self.in_gap(y);
        }
        let seam = (1..self.pages.len()).filter_map(|i| self.shown(i)).any(|[_, top, ..]| (y - top).abs() <= tol);
        let first = self.shown(0).is_some_and(|[_, top, ..]| (y - top).abs() <= tol);
        let last = self.pages.len().checked_sub(1).and_then(|i| self.shown(i)).is_some_and(|[_, top, _, h]| (y - (top + h)).abs() <= tol);
        seam || first || last
    }

    pub fn page_at(&self, x: f64, y: f64) -> Option<(usize, f64, f64)> {
        let gap = if self.bands.is_empty() { PAGE_GAP / 2.0 } else { 0.0 };
        let i = (0..self.pages.len()).position(|i| self.shown(i).is_some_and(|r| y < r[1] + r[3] + gap)).unwrap_or(self.pages.len().checked_sub(1)?);
        let r = self.pages[i];
        Some((i, x - r[0], y - r[1]))
    }

    pub fn to_doc(&self, page: usize, x: f64, y: f64) -> Option<(f64, f64)> {
        let r = self.pages.get(page)?;
        Some((r[0] + x, r[1] + y))
    }
}

/// A box over text: `rect` as its line is laid out, drawn on the page by `turn`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// The box as the line is laid out.
    pub rect: [f64; 4],
    /// Where the line is drawn on the page.
    pub turn: crate::layout::vertical::Turn,
}

impl Band {
    /// A box drawn where it is laid out.
    pub fn upright(rect: [f64; 4]) -> Band {
        Band { rect, turn: crate::layout::vertical::Turn::NONE }
    }

    /// The box on the page that holds it.
    pub fn page(&self) -> [f64; 4] {
        self.turn.rect(self.rect)
    }

    /// Whether the page point `x`, `y` lies in it, edges included.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let [lx, ly] = self.turn.back([x, y]);
        let [x0, y0, x1, y1] = self.rect;
        lx >= x0 - 1e-9 && lx <= x1 + 1e-9 && ly >= y0 - 1e-9 && ly <= y1 + 1e-9
    }
}

/// A caret on page `page`: a bar across a line running `dir`, in the box from `x`, `top` to `right`, `bottom`
/// (`right` is `x` for an upright horizontal line's, `bottom` is `top` for an upright vertical one's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretView {
    /// The page it is on.
    pub page: usize,
    /// The box's left.
    pub x: f64,
    /// The box's top.
    pub top: f64,
    /// The box's bottom.
    pub bottom: f64,
    /// The box's right.
    pub right: f64,
    /// The way the line runs.
    pub dir: super::geometry::LineDir,
    /// The bar's ends: on the line's top side, then on its bottom side.
    pub ends: [[f64; 2]; 2],
}

impl CaretView {
    /// Caret `c` on page `page`.
    pub fn of(page: usize, c: super::geometry::CaretRect) -> CaretView {
        CaretView { page, x: c.x, top: c.top, bottom: c.bottom, right: c.right, dir: c.dir, ends: c.ends }
    }

    /// Whether the bar runs along neither of the page's axes: across a line turned by other than quarter turns.
    pub fn slanted(&self) -> bool {
        let [[x0, y0], [x1, y1]] = self.ends;
        (x1 - x0).abs() > 1e-9 && (y1 - y0).abs() > 1e-9
    }

    /// The box the bar fills, at least a point thick.
    pub fn rect(&self) -> [f64; 4] {
        [self.x, self.top, self.right.max(self.x + 1.0), self.bottom.max(self.top + 1.0)]
    }

    /// Whether the line it is across runs down or up the page.
    pub fn vertical(&self) -> bool {
        self.dir != super::geometry::LineDir::Horizontal
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionView {
    None,
    Text(Vec<(usize, Band)>),
    Cells(Vec<(usize, [f64; 4])>),
    /// Each selected object's frame, turned as it's drawn, and whether it shows its handles.
    Object(Vec<Outline>, bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    pub page: usize,
    /// The page the object is drawn on, which `from` is on.
    pub source: usize,
    pub float: Option<crate::layout::FloatId>,
    pub from: [f64; 4],
    pub to: [f64; 4],
    /// The frames' rotation: the drawing scales along the frame's own sides.
    pub rot: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapeFrame {
    pub page: usize,
    pub rect: [f64; 4],
    pub rot: f64,
    pub rotate: [f64; 2],
    pub top: [f64; 2],
    pub adjust: Vec<[f64; 2]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Pt(pub f64);

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Px(pub f64);

impl Px {
    pub fn of(p: Pt) -> Px {
        Px(p.0 / 0.75)
    }

    pub fn pt(self) -> Pt {
        Pt(self.0 * 0.75)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideLine {
    pub axis: Axis,
    pub at: Pt,
    pub span: (Pt, Pt),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub origin: [Pt; 2],
    pub step: Pt,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Guides {
    pub lines: Vec<GuideLine>,
    pub grid: Option<Grid>,
    pub anchor_link: Option<[Pt; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CorrectionView {
    /// The corrected text's boxes on the page.
    pub rects: Vec<(usize, [f64; 4])>,
    /// The corrected text's bands, to underline.
    pub bands: Vec<(usize, Band)>,
    pub button: Option<(usize, [f64; 4])>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Measure {
    Dimension { from: [Pt; 2], to: [Pt; 2], label: String, tone: Tone },
    Chip { at: [Px; 2], label: String },
    Status { at: [Pt; 2], label: String, tone: Tone },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Muted,
}

/// A float's wrap polygon while its points are edited: its page and its vertices (page points, as drawn), closed from
/// the last back to the first.
#[derive(Clone, Debug, PartialEq)]
pub struct WrapPoints {
    pub page: usize,
    pub points: Vec<[f64; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub page: usize,
    pub rect: [f64; 4],
    pub rot: f64,
}

/// Where the selection is on its pages (page coordinates, points), for placing UI beside it: the text or cell
/// pieces, each selected object's frame as drawn, turned, and per page the box around all of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionBounds {
    /// Each selected line's or cell's rect, by page.
    pub rects: Vec<(usize, [f64; 4])>,
    /// Each selected object's frame, turned as it's drawn.
    pub frames: Vec<Outline>,
    /// The union of the rects and frames on each page.
    pub bounds: Vec<(usize, [f64; 4])>,
}

impl Outline {
    /// The upright box around the frame turned by its rotation (degrees clockwise) about its center.
    pub fn turned_box(&self) -> [f64; 4] {
        let [x0, y0, x1, y1] = self.rect;
        let (cx, cy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
        let (sin, cos) = self.rot.to_radians().sin_cos();
        let (w, h) = (hw * cos.abs() + hh * sin.abs(), hw * sin.abs() + hh * cos.abs());
        [cx - w, cy - h, cx + w, cy + h]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableZone {
    InsertColumn(usize),
    InsertRow(usize),
    SelectColumn(usize),
    SelectRow(usize),
    Border(usize),
    RowBorder(usize),
    Move,
    Resize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableHover {
    pub table: crate::layout::TableId,
    pub zone: Option<TableZone>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableHandles {
    pub page: usize,
    pub corner: Option<[f64; 2]>,
    pub end: Option<[f64; 2]>,
    pub mark: Option<TableMark>,
    /// Where the table, as laid out, is drawn: the corner, end and mark are as laid out.
    pub turn: crate::layout::vertical::Turn,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TableMark {
    InsertColumn { x: f64, top: f64, bottom: f64 },
    InsertRow { y: f64, left: f64, right: f64 },
    SelectColumn { x0: f64, x1: f64, top: f64 },
    SelectRow { y0: f64, y1: f64, left: f64 },
    Border { x: f64, top: f64, bottom: f64 },
    RowBorder { y: f64, left: f64, right: f64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewSettings {
    pub detail: crate::api::services::MeasureDetail,
    pub object_grid: crate::api::services::ObjectGrid,
    pub unit: crate::api::services::RulerUnit,
}

#[derive(Clone, Debug)]
pub struct View {
    pub snapshot: Rc<Snapshot>,
    pub revision: u64,
    pub stack: PageStack,
    pub pages_visible: std::ops::Range<usize>,
    pub caret: Option<CaretView>,
    pub caret_hidden: Option<&'static str>,
    pub selection: SelectionView,
    pub story: StoryId,
    pub fade: Vec<(usize, [f64; 4])>,
    pub guide: Option<(usize, [f64; 4])>,
    pub anchor: Option<(usize, [f64; 2])>,
    pub ghost: Option<Ghost>,
    pub shapes: Vec<ShapeFrame>,
    /// The selected float's wrap polygon while its points are edited.
    pub wrap_points: Option<WrapPoints>,
    pub guides: Vec<(usize, Guides)>,
    pub measures: Vec<(usize, Measure)>,
    pub original: Option<Outline>,
    pub table: Option<TableHandles>,
    pub table_preview: Option<(usize, [f64; 4])>,
    pub correction: Option<CorrectionView>,
    /// The IME composition showing, to underline.
    pub composition: Vec<(usize, Band)>,
    pub cursor: super::input::Cursor,
    pub spelling: Vec<(usize, Band)>,
    pub drop: Option<CaretView>,
    pub marks: Vec<MarkView>,
    pub tooltip: Option<Tooltip>,
    pub regions: Vec<RegionView>,
    pub comments: Vec<CommentView>,
    pub comment_anchors: Vec<(crate::docx::review::CommentId, usize, [f64; 2])>,
    pub formatting_marks: Vec<FormattingMark>,
    pub boundaries: Vec<(usize, [f64; 4])>,
    pub field_shading: Vec<(usize, Band)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormattingMark {
    pub page: usize,
    pub kind: MarkKind,
    pub at: Band,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Paragraph,
    /// A vanished paragraph mark, shown with hidden text: a paragraph mark dotted underneath.
    HiddenParagraph,
    Space,
    Tab,
    LineBreak,
    /// A break other than a page or column break (a text-wrapping break).
    Break,
    Hidden,
    BookmarkStart,
    BookmarkEnd,
    /// A non-breaking space: a degree sign.
    NonBreakingSpace,
    /// A non-breaking hyphen: a dash over the character.
    NonBreakingHyphen,
    /// "Page Break", on a dotted line to the text's right edge.
    PageBreak,
    /// "Column Break", on a dotted line to the text's right edge.
    ColumnBreak,
    /// "Section Break (…)" after the last paragraph of a section, on a double dotted line: the next section's break.
    SectionBreak(crate::docx::SectionBreak),
    /// The end-of-cell mark, in place of the paragraph mark of a cell's last paragraph.
    CellEnd,
    /// The end-of-row mark, right of a row's last cell.
    RowEnd,
}

/// Which formatting marks show, as Word's Display options name them: all of them, or each kind on its own. Hidden
/// text and optional hyphens show laid out ([`MarkOptions::shows_hidden_text`]). By default only the selected
/// floating object's anchor shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkOptions {
    /// Every mark (Show/Hide ¶).
    pub all: bool,
    /// Tab characters, as arrows.
    pub tabs: bool,
    /// Spaces and non-breaking spaces, as dots and degree signs.
    pub spaces: bool,
    /// Paragraph marks, line, page, column and section breaks, and end-of-cell and end-of-row marks.
    pub paragraphs: bool,
    /// Hidden text, laid out and dotted underneath.
    pub hidden: bool,
    /// Optional hyphens, laid out as not signs.
    pub optional_hyphens: bool,
    /// The selected floating object's anchor.
    pub anchors: bool,
}

impl Default for MarkOptions {
    fn default() -> MarkOptions {
        MarkOptions { all: false, tabs: false, spaces: false, paragraphs: false, hidden: false, optional_hyphens: false, anchors: true }
    }
}

impl MarkOptions {
    /// Every mark.
    pub const ALL: MarkOptions =
        MarkOptions { all: true, tabs: true, spaces: true, paragraphs: true, hidden: true, optional_hyphens: true, anchors: true };

    /// Whether hidden text shows (and is laid out).
    pub fn shows_hidden_text(&self) -> bool {
        self.all || self.hidden
    }

    /// Whether optional hyphens show (and are laid out).
    pub fn shows_optional_hyphens(&self) -> bool {
        self.all || self.optional_hyphens
    }

    /// Whether any mark drawn in the text shows (the anchor is the selected object's).
    pub fn any(&self) -> bool {
        self.all || self.tabs || self.spaces || self.paragraphs || self.hidden || self.optional_hyphens
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommentView {
    pub thread: crate::docx::review::CommentId,
    /// The commented text's boxes on the page.
    pub rects: Vec<(usize, [f64; 4])>,
    /// The commented text's bands, to tint.
    pub bands: Vec<(usize, Band)>,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegionView {
    pub id: String,
    pub rects: Vec<(usize, [f64; 4])>,
    pub title_rect: (usize, [f64; 4]),
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> {
        match s {
            "error" => Some(Severity::Error),
            "warning" => Some(Severity::Warning),
            "info" => Some(Severity::Info),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkView {
    pub page: usize,
    pub rect: Band,
    pub severity: Severity,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RulerMeasure {
    Dimension { from: [f64; 2], to: [f64; 2], label: String, tone: Tone },
    Chip { at: [f64; 2], label: String },
    Status { at: [f64; 2], label: String, tone: Tone },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tooltip {
    pub text: String,
    pub page: usize,
    pub rect: [f64; 4],
}

#[cfg(any(test, feature = "inspect"))]
/// A caret's bar: a vertical line's also gives its right end and the way its line runs.
fn caret_json(c: CaretView) -> serde_json::Value {
    let mut v = serde_json::json!({ "page": c.page, "x": r2(c.x), "top": r2(c.top), "bottom": r2(c.bottom) });
    if c.vertical() {
        v["right"] = serde_json::json!(r2(c.right));
        v["line"] = serde_json::json!(format!("{:?}", c.dir));
    }
    if c.slanted() {
        v["ends"] = serde_json::json!(c.ends.map(|p| p.map(r2)));
    }
    v
}

#[cfg(any(test, feature = "inspect"))]
fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(any(test, feature = "inspect"))]
pub fn canonical(v: &View) -> String {
    let rects = |r: &[f64; 4]| serde_json::json!([r2(r[0]), r2(r[1]), r2(r[2]), r2(r[3])]);
    let selection = match &v.selection {
        SelectionView::None => serde_json::Value::Null,
        SelectionView::Text(rs) => {
            serde_json::json!(rs.iter().map(|(p, b)| serde_json::json!({ "page": p, "rect": rects(&b.page()) })).collect::<Vec<_>>())
        }
        SelectionView::Cells(rs) => {
            serde_json::json!({ "cells": rs.iter().map(|(p, r)| serde_json::json!({ "page": p, "rect": rects(r) })).collect::<Vec<_>>() })
        }
        SelectionView::Object(rs, handles) => {
            serde_json::json!({ "object": rs.iter().map(|o| serde_json::json!({ "page": o.page, "rect": rects(&o.rect) })).collect::<Vec<_>>(), "handles": handles })
        }
    };
    let mut out = serde_json::json!({
        "revision": v.revision,
        "layout": { "revision": v.snapshot.revision().0, "pages": v.snapshot.pages().len(), "status": format!("{:?}", v.snapshot.status()).split_whitespace().next().unwrap_or("").trim_end_matches(['{', '(']).to_string() },
        "pages": v.stack.pages.iter().map(rects).collect::<Vec<_>>(),
        "visible": [v.pages_visible.start, v.pages_visible.end],
        "caret": v.caret.map(caret_json),
        "selection": selection,
        "story": format!("{:?}", v.story),
        "cursor": serde_json::to_value(v.cursor).unwrap_or_default(),
    });
    if let Some(d) = v.drop {
        out["drop"] = caret_json(d);
    }
    if !v.marks.is_empty() {
        out["marks"] = serde_json::json!(
            v.marks
                .iter()
                .map(|m| serde_json::json!({ "page": m.page, "rect": rects(&m.rect.page()), "severity": m.severity.name() }))
                .collect::<Vec<_>>()
        );
    }
    if !v.regions.is_empty() {
        out["regions"] = serde_json::json!(
            v.regions
                .iter()
                .map(|g| serde_json::json!({
                    "title": g.title,
                    "rects": g.rects.iter().map(|(p, r)| serde_json::json!({ "page": p, "rect": rects(r) })).collect::<Vec<_>>(),
                    "title_rect": { "page": g.title_rect.0, "rect": rects(&g.title_rect.1) },
                }))
                .collect::<Vec<_>>()
        );
    }
    if !v.comments.is_empty() {
        out["comments"] = serde_json::json!(
            v.comments
                .iter()
                .map(|c| serde_json::json!({
                    "thread": c.thread.0,
                    "active": c.active,
                    "rects": c.rects.iter().map(|(p, r)| serde_json::json!({ "page": p, "rect": rects(r) })).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        );
    }
    if !v.comment_anchors.is_empty() {
        out["comment_anchors"] = serde_json::json!(
            v.comment_anchors
                .iter()
                .map(|(id, p, [x, y])| serde_json::json!({ "thread": id.0, "page": p, "at": [r2(*x), r2(*y)] }))
                .collect::<Vec<_>>()
        );
    }
    if let Some(t) = &v.tooltip {
        out["tooltip"] = serde_json::json!({ "text": t.text, "page": t.page, "rect": rects(&t.rect) });
    }
    out.to_string()
}
