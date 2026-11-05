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
    Object(Vec<(usize, [f64; 4])>, bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    pub page: usize,
    pub float: Option<crate::layout::FloatId>,
    pub from: [f64; 4],
    pub to: [f64; 4],
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewSettings {
    pub detail: crate::api::services::MeasureDetail,
    pub object_grid: crate::api::services::ObjectGrid,
    pub unit: crate::api::services::RulerUnit,
    /// Placeholders are drawn as editor UI.
    pub placeholder_ui: bool,
    /// CSS px per point at the view's zoom (0: not yet known, taken as 100%): the affordances' tabs and bars are
    /// sized in px.
    pub px_per_pt: f64,
}

impl ViewSettings {
    /// Points per CSS px.
    pub fn u(&self) -> f64 {
        if self.px_per_pt > 0.0 { 1.0 / self.px_per_pt } else { 0.75 }
    }
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
    pub placeholders: Vec<PlaceholderView>,
    pub spelling: Vec<(usize, [f64; 4])>,
    pub drop: Option<CaretView>,
    pub marks: Vec<MarkView>,
    pub tooltip: Option<Tooltip>,
    pub regions: Vec<RegionView>,
    pub looks: Vec<LookView>,
    pub affordances: Vec<super::affordances::AffordanceView>,
    pub comments: Vec<CommentView>,
    pub comment_anchors: Vec<(crate::docx::review::CommentId, usize, [f64; 2])>,
    pub formatting_marks: Vec<FormattingMark>,
    pub boundaries: Vec<(usize, [f64; 4])>,
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
pub struct LookView {
    pub page: usize,
    pub rect: [f64; 4],
    pub fill: Option<u32>,
    pub label: Option<String>,
    pub icon: Option<std::rc::Rc<SvgIcon>>,
    pub image: Option<std::rc::Rc<Vec<u8>>>,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaceholderLook {
    pub color: Option<u32>,
    pub label: Option<String>,
    pub icon: Option<std::rc::Rc<SvgIcon>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SvgIcon {
    pub view_box: [f32; 4],
    pub paths: Vec<(String, Option<u32>)>,
}

impl SvgIcon {
    pub fn parse(svg: &str) -> Option<SvgIcon> {
        let attr = |tag: &str, name: &str| -> Option<String> {
            let k = tag.find(&format!(" {name}=\""))? + name.len() + 3;
            Some(tag[k..].split('"').next()?.to_string())
        };
        let num = |tag: &str, name: &str| attr(tag, name).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok());
        let root = svg.find("<svg").map(|k| &svg[k..svg[k..].find('>').map_or(svg.len(), |e| k + e)])?;
        let view_box = match attr(root, "viewBox")
            .map(|v| v.split([' ', ',']).filter(|s| !s.is_empty()).filter_map(|s| s.parse::<f32>().ok()).collect::<Vec<_>>())
        {
            Some(v) if v.len() == 4 => [v[0], v[1], v[2], v[3]],
            _ => [0.0, 0.0, num(root, "width").unwrap_or(24.0), num(root, "height").unwrap_or(24.0)],
        };
        let fill = |tag: &str| -> Option<Option<u32>> {
            match attr(tag, "fill").as_deref() {
                Some("none") => None,
                Some(c) if c.starts_with('#') && c.len() == 7 => Some(u32::from_str_radix(&c[1..], 16).ok().map(|n| 0xFF00_0000 | n)),
                _ => Some(None),
            }
        };
        let mut paths = Vec::new();
        let mut rest = svg;
        while let Some(k) = rest.find('<') {
            let end = rest[k..].find('>').map_or(rest.len(), |e| k + e + 1);
            let tag = &rest[k..end];
            let name = tag[1..].split(|c: char| c.is_whitespace() || c == '>' || c == '/').next().unwrap_or("");
            let d = match name {
                "path" => attr(tag, "d"),
                "circle" => {
                    let (cx, cy, r) = (num(tag, "cx").unwrap_or(0.0), num(tag, "cy").unwrap_or(0.0), num(tag, "r").unwrap_or(0.0));
                    Some(format!("M{} {}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 {} 0Z", cx - r, cy, 2.0 * r, -2.0 * r))
                }
                "rect" => {
                    let (x, y, w, h) = (
                        num(tag, "x").unwrap_or(0.0),
                        num(tag, "y").unwrap_or(0.0),
                        num(tag, "width").unwrap_or(0.0),
                        num(tag, "height").unwrap_or(0.0),
                    );
                    Some(format!("M{x} {y}h{w}v{h}h{}Z", -w))
                }
                "polygon" => attr(tag, "points").map(|p| format!("M{}Z", p.trim())),
                _ => None,
            };
            if let (Some(d), Some(f)) = (d, fill(tag)) {
                paths.push((d, f));
            }
            rest = &rest[end..];
        }
        (!paths.is_empty()).then_some(SvgIcon { view_box, paths })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaceholderView {
    pub page: usize,
    pub rect: [f64; 4],
    pub color: u32,
    pub label: String,
    pub icon: Option<std::rc::Rc<SvgIcon>>,
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
            serde_json::json!({ "object": rs.iter().map(|(p, r)| serde_json::json!({ "page": p, "rect": rects(r) })).collect::<Vec<_>>(), "handles": handles })
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
    if !v.looks.is_empty() {
        out["looks"] =
            serde_json::json!(v.looks.iter().map(|l| serde_json::json!({
            "page": l.page, "rect": rects(&l.rect), "image": l.image.is_some(), "label": l.label, "fill": l.fill.map(|f| format!("{f:08X}")),
        })).collect::<Vec<_>>());
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

/// The UI face's advance, in ems: every character of the monospaced face is this wide.
pub const MONO_ADVANCE: f64 = 0.6;

/// How wide `text` is in the UI face at `size`.
pub fn mono_width(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * MONO_ADVANCE * size
}

/// A placeholder the affordances draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Field(crate::docx::FieldId),
    Control(crate::docx::ControlId),
    RepeatingSection(crate::docx::ControlId),
    /// A sub-template reference shown as a card or as its body.
    SubTemplate(crate::docx::FieldId),
    /// A picture placeholder, by the content control it's in.
    InputBlock(crate::docx::ControlId),
    /// An IF field shown as its result.
    IfField(crate::docx::FieldId),
}

impl Target {
    pub fn element(self) -> crate::docx::ElementId {
        match self {
            Target::Field(f) => crate::docx::ElementId::Field(f),
            Target::Control(c) | Target::RepeatingSection(c) => crate::docx::ElementId::Control(c),
            Target::SubTemplate(f) => crate::docx::ElementId::Field(f),
            Target::InputBlock(c) => crate::docx::ElementId::Control(c),
            Target::IfField(f) => crate::docx::ElementId::Field(f),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldState {
    Rest,
    Hover,
    Selected,
    /// A merge field the data source lacks (`setUnmappedFields`), at rest or hovered.
    Unmapped,
}

/// One line of a field mark, in page points. `round_*`: the field starts or ends here; `gap_*`: another field's mark
/// touches this edge, so the two are drawn apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub rect: [f64; 4],
    pub round_left: bool,
    pub round_right: bool,
    pub gap_left: bool,
    pub gap_right: bool,
}

/// A field mark's caption: `{ }`, the key and the type text.
#[derive(Clone, Debug, PartialEq)]
pub struct Caption {
    pub key: String,
    pub type_text: String,
}

/// What render draws, per page.
#[derive(Clone, Debug, PartialEq)]
pub enum AffordanceView {
    /// A field, or a control showing placeholder text: a tint behind its text, one segment per line, and a caption
    /// under its last segment while hovered or selected.
    Field { page: usize, target: Target, segments: Vec<Segment>, state: FieldState, caption: Option<Caption>, on_tint: bool },
    /// A repeating section: its bar in the gutter (8 px further out per enclosing section), its tab on its first page,
    /// and its rows' tint.
    Region { page: usize, target: Target, bar: [f64; 4], tab: Option<Tab>, rows: Vec<[f64; 4]>, state: FieldState },
    /// A collapsed sub-template reference, or one whose body can't show: a card in its place, with Expand when it
    /// can show.
    Card {
        page: usize,
        target: Target,
        rect: [f64; 4],
        name: String,
        line: String,
        resolution: CardState,
        expand: Option<[f64; 4]>,
        state: FieldState,
    },
    /// A sub-template's body shown in its reference's place: a dashed bar in the gutter and its tab, always.
    BodyMark { page: usize, target: Target, bar: [f64; 4], tab: Tab, parts: Vec<(Part, [f64; 4])> },
    /// An IF field's result: a tint over it, one rect per line, and its tab (condition and else) while hovered or
    /// selected.
    IfRegion { page: usize, target: Target, rects: Vec<[f64; 4]>, tab: Option<Tab>, state: FieldState },
    /// A picture placeholder: a tinted block in its base colour with a label chip, the hint on hover, the buttons
    /// when selected; filled, the picture, with a tab on hover.
    Block(Box<Block>),
}

/// An input block, page points.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub page: usize,
    pub target: Target,
    pub rect: [f64; 4],
    /// The rule's fill as RGB (none: the accent).
    pub base: Option<u32>,
    pub label: String,
    pub icon: Option<std::rc::Rc<SvgIcon>>,
    /// The rule's image look, drawn instead of the tint and chip.
    pub image: Option<std::rc::Rc<Vec<u8>>>,
    pub hint: String,
    pub buttons: Vec<Button>,
    pub state: BlockState,
    /// Hovered when filled: its label, the date it was filled and Replace.
    pub done: Option<Tab>,
}

/// A button on a selected input block.
#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub id: String,
    pub label: String,
    pub primary: bool,
    pub rect: [f64; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockState {
    Rest,
    Hover,
    Selected,
    /// Filled.
    Done,
}

/// What a card's sub-template reference resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardState {
    Resolved,
    Missing,
    NearMatch,
    Cycle,
    TooDeep,
}

/// What a press on part of an affordance does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Show the card's body.
    Expand,
    /// Show the reference as a card again (the tab's name).
    Collapse,
    /// Edit the sub-template.
    Open,
    /// The newer version: the application updates it.
    Update,
    /// A filled input block's Replace.
    Replace,
}

/// A tab's box (page points) and what it says.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub rect: [f64; 4],
    pub icon: Option<Icon>,
    pub text: String,
}

/// The glyphs tabs draw as paths: the UI face has neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Repeats: an open circle with an arrowhead.
    Repeat,
    /// A copy: two overlapping squares.
    Copy,
}

impl AffordanceView {
    pub fn page(&self) -> usize {
        match self {
            AffordanceView::Field { page, .. }
            | AffordanceView::Region { page, .. }
            | AffordanceView::Card { page, .. }
            | AffordanceView::BodyMark { page, .. }
            | AffordanceView::IfRegion { page, .. } => *page,
            AffordanceView::Block(b) => b.page,
        }
    }
}
