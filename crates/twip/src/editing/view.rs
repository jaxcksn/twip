//! Frame view

use std::rc::Rc;

use crate::layout::Snapshot;

use super::position::StoryId;

pub const PAGE_GAP: f64 = 18.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageStack {
    pub pages: Vec<[f64; 4]>,
    pub width: f64,
    pub height: f64,
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
        PageStack { pages, width, height: y }
    }

    pub fn page_at(&self, x: f64, y: f64) -> Option<(usize, f64, f64)> {
        let i = self.pages.iter().position(|r| y < r[1] + r[3] + PAGE_GAP / 2.0).unwrap_or(self.pages.len().checked_sub(1)?);
        let r = self.pages[i];
        Some((i, x - r[0], y - r[1]))
    }

    pub fn to_doc(&self, page: usize, x: f64, y: f64) -> Option<(f64, f64)> {
        let r = self.pages.get(page)?;
        Some((r[0] + x, r[1] + y))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretView {
    pub page: usize,
    pub x: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SelectionView {
    None,
    Text(Vec<(usize, [f64; 4])>),
    Cells(Vec<(usize, [f64; 4])>),
    /// Each selected object's frame, turned as it's drawn, and whether it shows its handles.
    Object(Vec<Outline>, bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    pub page: usize,
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
    pub rects: Vec<(usize, [f64; 4])>,
    pub button: Option<(usize, [f64; 4])>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Measure {
    Dimension { from: [Pt; 2], to: [Pt; 2], label: String, tone: Tone },
    Chip { at: [Px; 2], label: String },
    Hint { at: Px, label: String },
    Status { at: [Pt; 2], label: String, tone: Tone },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Muted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub page: usize,
    pub rect: [f64; 4],
    pub rot: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableZone {
    InsertColumn(usize),
    InsertRow(usize),
    SelectColumn(usize),
    SelectRow(usize),
    Border(usize),
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TableMark {
    InsertColumn { x: f64, top: f64, bottom: f64 },
    InsertRow { y: f64, left: f64, right: f64 },
    SelectColumn { x0: f64, x1: f64, top: f64 },
    SelectRow { y0: f64, y1: f64, left: f64 },
    Border { x: f64, top: f64, bottom: f64 },
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
    pub guides: Vec<(usize, Guides)>,
    pub measures: Vec<(usize, Measure)>,
    pub original: Option<Outline>,
    pub table: Option<TableHandles>,
    pub table_preview: Option<(usize, [f64; 4])>,
    pub correction: Option<CorrectionView>,
    pub cursor: super::input::Cursor,
    pub spelling: Vec<(usize, [f64; 4])>,
    pub drop: Option<CaretView>,
    pub marks: Vec<MarkView>,
    pub tooltip: Option<Tooltip>,
    pub regions: Vec<RegionView>,
    pub comments: Vec<CommentView>,
    pub comment_anchors: Vec<(crate::docx::review::CommentId, usize, [f64; 2])>,
    pub formatting_marks: Vec<FormattingMark>,
    pub boundaries: Vec<(usize, [f64; 4])>,
    pub field_shading: Vec<(usize, [f64; 4])>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormattingMark {
    pub page: usize,
    pub kind: MarkKind,
    pub at: [f64; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Paragraph,
    Space,
    Tab,
    LineBreak,
    Break,
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommentView {
    pub thread: crate::docx::review::CommentId,
    pub rects: Vec<(usize, [f64; 4])>,
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
    pub rect: [f64; 4],
    pub severity: Severity,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RulerMeasure {
    Dimension { from: [f64; 2], to: [f64; 2], label: String, tone: Tone },
    Chip { at: [f64; 2], label: String },
    Hint { at: f64, label: String },
    Status { at: [f64; 2], label: String, tone: Tone },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tooltip {
    pub text: String,
    pub page: usize,
    pub rect: [f64; 4],
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
        SelectionView::Text(rs) => serde_json::json!(rs.iter().map(|(p, r)| serde_json::json!({ "page": p, "rect": rects(r) })).collect::<Vec<_>>()),
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
        "caret": v.caret.map(|c| serde_json::json!({ "page": c.page, "x": r2(c.x), "top": r2(c.top), "bottom": r2(c.bottom) })),
        "selection": selection,
        "story": format!("{:?}", v.story),
        "cursor": serde_json::to_value(v.cursor).unwrap_or_default(),
    });
    if let Some(d) = v.drop {
        out["drop"] = serde_json::json!({ "page": d.page, "x": r2(d.x), "top": r2(d.top), "bottom": r2(d.bottom) });
    }
    if !v.marks.is_empty() {
        out["marks"] = serde_json::json!(
            v.marks.iter().map(|m| serde_json::json!({ "page": m.page, "rect": rects(&m.rect), "severity": m.severity.name() })).collect::<Vec<_>>()
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
