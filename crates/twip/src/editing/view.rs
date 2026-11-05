//! The rendering-facing state of one frame: plain data built from one snapshot. Renderers draw
//! it; they never compute hit targets or hold editing state.

use std::rc::Rc;

use crate::layout::Snapshot;

use super::position::StoryId;

/// Points between pages and around them, in the page stack.
pub const PAGE_GAP: f64 = 18.0;

/// Where each page sits in the document coordinate space: pages stacked top to bottom, centred, `PAGE_GAP` apart.
/// Pointer input and every rectangle a renderer draws use these coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageStack {
    /// Each page's top-left corner and size.
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

    /// The page under a document point, and the point in that page's coordinates (the nearest page between pages).
    pub fn page_at(&self, x: f64, y: f64) -> Option<(usize, f64, f64)> {
        let i = self.pages.iter().position(|r| y < r[1] + r[3] + PAGE_GAP / 2.0).unwrap_or(self.pages.len().checked_sub(1)?);
        let r = self.pages[i];
        Some((i, x - r[0], y - r[1]))
    }

    /// A page point in document coordinates.
    pub fn to_doc(&self, page: usize, x: f64, y: f64) -> Option<(f64, f64)> {
        let r = self.pages.get(page)?;
        Some((r[0] + x, r[1] + y))
    }
}

/// The caret to draw: its page and rectangle there (x, top, bottom), page coordinates.
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
    /// One rectangle per line run, by page (page coordinates).
    Text(Vec<(usize, [f64; 4])>),
    /// A cell block: each selected cell's rectangle on each page it is drawn on (page coordinates).
    Cells(Vec<(usize, [f64; 4])>),
    /// A selected object: its box on each page it is drawn on, outlined, with handles unless it can't be resized.
    Object(Vec<(usize, [f64; 4])>, bool),
}

/// An object being moved or resized, as Word shows it: its own drawing, translucent, mapped from where it is to where
/// the drag puts it; the object itself stays until the release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    pub page: usize,
    /// A float's own content; `None`, an inline picture's (the image drawn in `from`).
    pub float: Option<crate::layout::FloatId>,
    /// Its box now and the box it is dragged to, page coordinates.
    pub from: [f64; 4],
    pub to: [f64; 4],
}

/// A selected shape's frame: its unrotated box and rotation, where its rotate handle is, and its adjust handles,
/// page coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeFrame {
    pub page: usize,
    pub rect: [f64; 4],
    /// Degrees clockwise about the box's centre.
    pub rot: f64,
    /// The rotate knob's centre, 24 px above the top's middle (turned with the shape), and that middle.
    pub rotate: [f64; 2],
    pub top: [f64; 2],
    pub adjust: Vec<[f64; 2]>,
}

/// A length in page points.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Pt(pub f64);

/// A length in CSS pixels at 100% zoom: 0.75 pt each.
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

/// Which way a guide runs: `X`, a vertical line at an x; `Y`, a horizontal line at a y.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// A line a dragged edge or centre snapped to: where it is on its axis and how far it runs along the other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideLine {
    pub axis: Axis,
    pub at: Pt,
    pub span: (Pt, Pt),
}

/// The dot grid: a dot at `origin` and every `step` from it, both ways.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub origin: [Pt; 2],
    pub step: Pt,
}

/// What an object drag shows on a page besides the object: its guides, the grid, and the dotted line from the anchor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Guides {
    pub lines: Vec<GuideLine>,
    pub grid: Option<Grid>,
    /// From the anchor's place (the point [`View::anchor`] gives) to the moving object's left edge, at the anchor
    /// mark's height: drawn dotted from the anchor mark.
    pub anchor_link: Option<[Pt; 4]>,
}

/// A correction just made: its text's line boxes (underlined), and its options button (page, box) while the caret is
/// in it or just after it. Page coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct CorrectionView {
    pub rects: Vec<(usize, [f64; 4])>,
    pub button: Option<(usize, [f64; 4])>,
}

/// A readout drawn over a page; labels are final strings.
#[derive(Clone, Debug, PartialEq)]
pub enum Measure {
    /// A dimension line with end ticks, its label over its middle.
    Dimension { from: [Pt; 2], to: [Pt; 2], label: String, tone: Tone },
    /// A dark chip, its top left at `at` (from the page's top left).
    Chip { at: [Px; 2], label: String },
    /// A ruler hover hint.
    Hint { at: Px, label: String },
    /// A table status line.
    Status { at: [Pt; 2], label: String, tone: Tone },
}

/// A measurement's colour: the accent for what's being changed, muted for what's shown alongside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Muted,
}

/// A dragged object's box and angle (degrees clockwise) when the press was: drawn dashed while it is resized or turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub page: usize,
    pub rect: [f64; 4],
    pub rot: f64,
}

/// What the pointer is over on a table showing its handles: grid boundaries and grid columns, model rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableZone {
    /// The "+" above grid boundary `k`.
    InsertColumn(usize),
    /// The "+" left of the bottom border of row `k` − 1.
    InsertRow(usize),
    /// The arrow above grid column `k`.
    SelectColumn(usize),
    /// The arrow left of row `k`.
    SelectRow(usize),
    /// Column border `k`.
    Border(usize),
    Move,
    Resize,
}

/// The table showing its handles: the one under the pointer, else the one holding the caret or selection (no zone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableHover {
    pub table: crate::layout::TableId,
    pub zone: Option<TableZone>,
}

/// A table's handles on a page, page coordinates; their sizes and offsets are CSS px.
#[derive(Clone, Debug, PartialEq)]
pub struct TableHandles {
    pub page: usize,
    /// The table's top left, when its first row is on the page: the move handle is 20 px above and left of it.
    pub corner: Option<[f64; 2]>,
    /// The table's bottom right, when its last row is on the page: the resize handle's top left.
    pub end: Option<[f64; 2]>,
    /// The affordance under the pointer.
    pub mark: Option<TableMark>,
}

/// A table affordance drawn under the pointer, page coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TableMark {
    /// A "+" above column border `x` and a dashed line down it from 4 px above `top` to 4 px below `bottom`.
    InsertColumn { x: f64, top: f64, bottom: f64 },
    /// A "+" left of row border `y` and a dashed line along it from 6 px left of `left` to `right`.
    InsertRow { y: f64, left: f64, right: f64 },
    /// An arrow pointing down, 6 px above `top`, centred between `x0` and `x1`.
    SelectColumn { x0: f64, x1: f64, top: f64 },
    /// An arrow pointing right, 6 px left of `left`, centred between `y0` and `y1`.
    SelectRow { y0: f64, y1: f64, left: f64 },
    /// A dashed line down column border `x`, from 4 px above `top` to 4 px below `bottom`.
    Border { x: f64, top: f64, bottom: f64 },
}

/// The view options that shape the drag feedback: how much is measured, when the grid shows, and the unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewSettings {
    pub detail: crate::api::services::MeasureDetail,
    pub object_grid: crate::api::services::ObjectGrid,
    pub unit: crate::api::services::RulerUnit,
}

/// Everything a renderer needs for one frame besides the snapshot's own content.
#[derive(Clone, Debug)]
pub struct View {
    pub snapshot: Rc<Snapshot>,
    pub revision: u64,
    pub stack: PageStack,
    pub pages_visible: std::ops::Range<usize>,
    pub caret: Option<CaretView>,
    /// Why there is no caret to draw though there is one: `"clipped"`, inside text an exact row height hides.
    pub caret_hidden: Option<&'static str>,
    pub selection: SelectionView,
    /// The story being edited; headers and footers fade the body while they are.
    pub story: StoryId,
    /// Areas drawn faded: the body while a header or footer is edited, page coordinates.
    pub fade: Vec<(usize, [f64; 4])>,
    /// A border drag's guide line: its page and its two ends, page coordinates.
    pub guide: Option<(usize, [f64; 4])>,
    /// The selected float's anchor: its page and the anchor paragraph's first line's top left, page
    /// coordinates; while the float is dragged, the paragraph it would anchor to.
    pub anchor: Option<(usize, [f64; 2])>,
    /// An object move or resize in progress.
    pub ghost: Option<Ghost>,
    /// The selected shapes' rotate and adjust handles.
    pub shapes: Vec<ShapeFrame>,
    /// Object drag feedback by page: the guides and grid, the readouts, and the original box while resized or turned.
    pub guides: Vec<(usize, Guides)>,
    pub measures: Vec<(usize, Measure)>,
    pub original: Option<Outline>,
    /// The handles of the table under the pointer or holding the caret, with no drag under way.
    pub table: Option<TableHandles>,
    /// A table resize's or move's new box: dashed, with the accent's soft tint (page coordinates).
    pub table_preview: Option<(usize, [f64; 4])>,
    /// The text the last AutoCorrect step made, and its options button while the caret is at it.
    pub correction: Option<CorrectionView>,
    pub cursor: super::input::Cursor,
    /// Unfilled placeholders on the visible pages drawn the host's way: page coordinates.
    pub placeholders: Vec<PlaceholderView>,
    /// Misspellings to underline (the host's spell checker fills these): page and the words' box, page coordinates.
    pub spelling: Vec<(usize, [f64; 4])>,
    /// Where a text drag would drop: a dashed caret there.
    pub drop: Option<CaretView>,
    /// The host's element marks on the visible pages: each line's box of a marked element.
    pub marks: Vec<MarkView>,
    /// The message of the mark under the pointer: its text and that line's box.
    pub tooltip: Option<Tooltip>,
    /// In design mode, the repeating regions under the pointer and around the selection: their boundaries
    /// and title tabs.
    pub regions: Vec<RegionView>,
    /// In design mode, the host's looks on picture and rich-text controls.
    pub looks: Vec<LookView>,
    /// Commented text on the visible pages, by thread: a tint over each range, the active thread's stronger.
    pub comments: Vec<CommentView>,
    /// Where each thread's range starts, when that is on a visible page: its page and the point there (page
    /// coordinates), for the host to place the thread's card beside.
    pub comment_anchors: Vec<(crate::docx::review::CommentId, usize, [f64; 2])>,
    /// Formatting marks on the visible pages (the host's setting fills these).
    pub formatting_marks: Vec<FormattingMark>,
    /// The text areas' and table cells' boundaries on the visible pages (the host's setting fills these): page and box.
    pub boundaries: Vec<(usize, [f64; 4])>,
}

/// A formatting mark to draw over a page (formatting marks on): its page, kind, and box (page coordinates; the box's
/// height is the line's).
#[derive(Clone, Debug, PartialEq)]
pub struct FormattingMark {
    pub page: usize,
    pub kind: MarkKind,
    pub at: [f64; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// A paragraph's end: a pilcrow after its last character.
    Paragraph,
    /// A space: a dot in its middle.
    Space,
    /// A tab: an arrow across it.
    Tab,
    /// A line break: a return arrow.
    LineBreak,
    /// A page or column break: a dotted line along the rest of the line.
    Break,
    /// Hidden text: a dotted underline.
    Hidden,
}

/// A thread's range on the visible pages: each line's box, page coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentView {
    /// The thread's first comment.
    pub thread: crate::docx::review::CommentId,
    pub rects: Vec<(usize, [f64; 4])>,
    pub active: bool,
}

/// A control drawn the host's way, page coordinates: a picture control's box with a fill, label and icon or
/// with the host's image (scaled into the box keeping its aspect ratio), or a rich-text control's lines with a fill
/// behind them and its label while it is empty.
#[derive(Clone, Debug, PartialEq)]
pub struct LookView {
    pub page: usize,
    pub rect: [f64; 4],
    pub fill: Option<u32>,
    pub label: Option<String>,
    pub icon: Option<std::rc::Rc<SvgIcon>>,
    /// The host's image (PNG, JPEG, ...), shared by every box it is drawn in.
    pub image: Option<std::rc::Rc<Vec<u8>>>,
}

/// A repeating region drawn in design mode: its box on each page and its title tab (page coordinates).
#[derive(Clone, Debug, PartialEq)]
pub struct RegionView {
    pub id: String,
    pub rects: Vec<(usize, [f64; 4])>,
    pub title_rect: (usize, [f64; 4]),
    pub title: String,
}

/// How bad a host's mark says an element is: drawn in its own colour.
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

/// One line's part of a marked element: page coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkView {
    pub page: usize,
    pub rect: [f64; 4],
    pub severity: Severity,
}

/// A ruler's measurement or hint, drawn over the viewport while a ruler marker is dragged or hovered. Labels are final
/// text: the value in the ruler unit, its decimals and suffix, and any change from where the drag started.
#[derive(Clone, Debug, PartialEq)]
pub enum RulerMeasure {
    /// A dimension line with end ticks and its label in the middle, from one point to another (page points).
    Dimension { from: [f64; 2], to: [f64; 2], label: String, tone: Tone },
    /// A dark chip whose top-left corner is at a viewport point (CSS px).
    Chip { at: [f64; 2], label: String },
    /// A ruler hover hint under the horizontal ruler, its left edge at a viewport x (CSS px).
    Hint { at: f64, label: String },
    /// A status line whose top-left corner is at a page point.
    Status { at: [f64; 2], label: String, tone: Tone },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tooltip {
    pub text: String,
    pub page: usize,
    pub rect: [f64; 4],
}

/// How the host has an unfilled placeholder drawn: a fill colour (ARGB), a label and a vector icon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaceholderLook {
    pub color: Option<u32>,
    pub label: Option<String>,
    pub icon: Option<std::rc::Rc<SvgIcon>>,
}

/// An SVG icon as its shapes (drawn as vector paths, no image): the viewBox (x, y, width, height) and each shape's
/// path data with its fill (ARGB; `None`: the placeholder's colour).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SvgIcon {
    pub view_box: [f32; 4],
    pub paths: Vec<(String, Option<u32>)>,
}

impl SvgIcon {
    /// The shapes of an SVG document: `path`, `circle`, `rect` and `polygon` elements with their `fill` (`none` ones
    /// are skipped; `currentColor` or none given: the placeholder's colour). `None` when it has no shape.
    // shortcut: no transforms, strokes, gradients or groups' fills; icon sets' single-colour shapes don't need them.
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

/// One drawn placeholder: its page and box, colour (ARGB), label and icon.
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

/// A view as canonical JSON (serde_json's maps are sorted).
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
        // The snapshot the frame was built from: its layout revision, page count and status.
        "layout": { "revision": v.snapshot.revision().0, "pages": v.snapshot.pages().len(), "status": format!("{:?}", v.snapshot.status()).split_whitespace().next().unwrap_or("").trim_end_matches(['{', '(']).to_string() },
        "pages": v.stack.pages.iter().map(rects).collect::<Vec<_>>(),
        "visible": [v.pages_visible.start, v.pages_visible.end],
        "caret": v.caret.map(|c| serde_json::json!({ "page": c.page, "x": r2(c.x), "top": r2(c.top), "bottom": r2(c.bottom) })),
        "selection": selection,
        "story": format!("{:?}", v.story),
        "cursor": serde_json::to_value(v.cursor).unwrap_or_default(),
    });
    // A drop caret, marks and a tooltip only while there are some, so views recorded before
    // them read the same.
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
