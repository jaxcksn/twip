//! The presentation surface a host attaches: size, scroll, focus, frames, rulers, balloons and hit testing.

use serde::{Deserialize, Serialize};

use super::{Commit, Session};
use crate::api::command::Command;
use crate::api::error::Error;
use crate::api::event::Event;
use crate::api::input::{ClipboardData, Point};
use crate::api::record::Origin;
use crate::api::review::CommentAnchor;
use crate::api::services::{AccessibleNode, ClientPoint, ClientRect, HitResult, PageRect, RulerAxis, RulerUnit, RulerView, TabSelector, Tooltip};
use crate::api::value::{Color, ElementId};
use crate::editing::commands::Outcome;
use crate::editing::elements;
use crate::editing::input::Request;
use crate::render::{Frame, Renderer};
use crate::services::ruler::{Bands, Marker};

pub use crate::editing::input::Cursor;

/// Something only the host showing the session can do, given to the sink [`HostSurface::attach`] takes. Positions
/// are content CSS px: they scroll with the document. On the wire it is the `host_request` callback, `type` tagged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostRequest {
    /// Show this pointer shape.
    Cursor {
        /// The shape.
        cursor: Cursor,
    },
    /// Send every pointer event to the view until `release` (a drag started).
    Capture,
    /// End the pointer capture.
    Release,
    /// Put this on the system clipboard: a cut or copy from the keyboard.
    Clipboard {
        /// What was copied.
        data: ClipboardData,
    },
    /// The caret moved: for the IME window and the system caret.
    Caret {
        /// Its left.
        x: f64,
        /// Its top.
        y: f64,
        /// Its height.
        height: f64,
        /// A collapsed selection shows a caret.
        visible: bool,
    },
    /// Scroll so the viewport's top-left is at this point: a zoom keeps the caret's line in place. The engine already
    /// draws from there. The content takes the size the next frame reports, so a host scrolls after that frame.
    Scroll {
        /// The left.
        x: f64,
        /// The top.
        y: f64,
    },
    /// Scroll so this rectangle shows.
    ScrollTo {
        /// Its left.
        x: f64,
        /// Its top.
        y: f64,
        /// Its width.
        width: f64,
        /// Its height.
        height: f64,
    },
    /// The accessibility tree changed: mirror it.
    A11y {
        /// The whole tree.
        tree: AccessibleNode,
    },
}

/// Where the host's requests go, and what it was last told.
#[derive(Default)]
struct Sink {
    deliver: Option<Box<dyn FnMut(HostRequest)>>,
    cursor: Option<Cursor>,
    caret: Option<(f64, f64, f64, bool)>,
    tooltip: Option<Tooltip>,
    /// The document revision the last tree was sent for.
    a11y: Option<u64>,
}

/// What a host keeps about the frame it drew: the document's size and the caret, for scrollbars and the IME.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameInfo {
    /// The page stack's width, CSS px.
    pub content_width: f64,
    /// The page stack's height, CSS px.
    pub content_height: f64,
    /// The caret's left, content CSS px.
    pub caret_x: f64,
    /// The caret's top, content CSS px.
    pub caret_y: f64,
    /// The caret's height, CSS px.
    pub caret_height: f64,
    /// A collapsed selection shows a caret.
    pub caret_visible: bool,
    /// The pages laid out so far.
    pub pages: u32,
}

/// The colours of what the editor draws over the document ([`HostSurface::set_overlay_palette`]). The defaults are at
/// least 3:1 against the page, except the selection: a light fill, black text over it at least 4.5:1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayPalette {
    /// The caret. Default black.
    pub caret: Color,
    /// The selection, blended under the content (multiply), so black text stays black over it. Default `#B3D7FF`, a system
    /// text selection's light blue.
    pub selection: Color,
    /// A selected object's outline and handles. Default `#606060`.
    pub object: Color,
    /// The drop caret of a drag. Default `#404040`.
    pub drop: Color,
    /// Formatting marks and text boundaries. Default `#2F6FB5`.
    pub marks: Color,
    /// The markup pane's leader lines. Default `#767676`.
    pub leaders: Color,
}

impl Default for OverlayPalette {
    fn default() -> OverlayPalette {
        OverlayPalette {
            caret: Color(0x000000),
            selection: Color(0xB3D7FF),
            object: Color(0x606060),
            drop: Color(0x404040),
            marks: Color(0x2F6FB5),
            leaders: Color(0x767676),
        }
    }
}

/// The memory a session keeps its pages' drawing in ([`HostSurface::set_paint_budget`]): past it, what was drawn
/// longest ago goes first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaintBudget {
    /// The pages' recordings, bytes. Default 32 MB.
    pub recordings: u64,
}

impl Default for PaintBudget {
    fn default() -> PaintBudget {
        PaintBudget { recordings: 32 << 20 }
    }
}

/// How the caret blinks ([`HostSurface::set_caret_blink`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaretBlink {
    /// A phase's length, ms (at least 100); none is a steady caret.
    pub interval_ms: Option<u32>,
    /// Blinking stops after this long without input, ms, the caret left visible; none is never.
    pub stop_after_ms: Option<u32>,
}

impl Default for CaretBlink {
    fn default() -> CaretBlink {
        CaretBlink { interval_ms: Some(530), stop_after_ms: None }
    }
}

/// What the frames have cost so far: for benchmarks and tests. Counts run from the session's start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameStats {
    /// Frames drawn.
    pub frames: u64,
    /// Page recordings made: an unchanged page is recorded once.
    pub pages_recorded: u64,
    /// Pages drawn again from their recordings.
    pub pages_reused: u64,
    /// The page recordings kept now, bytes.
    pub recording_bytes: u64,
    /// The last frame's time, ms.
    pub last_frame_ms: f64,
    /// The 95th percentile of the last 120 frames' times, ms.
    pub p95_frame_ms: f64,
}

/// The frame counters and the last frames' times, for [`FrameStats`].
#[derive(Default)]
struct FrameCounters {
    stats: FrameStats,
    /// The last frames' times, ms, oldest first.
    times: std::collections::VecDeque<f64>,
}

impl FrameCounters {
    const WINDOW: usize = 120;

    fn frame(&mut self, ms: f64) {
        if self.times.len() == Self::WINDOW {
            self.times.pop_front();
        }
        self.times.push_back(ms);
        self.stats.frames += 1;
        self.stats.last_frame_ms = ms;
    }

    fn stats(&self, retained: &crate::render::retained::Retained) -> FrameStats {
        let mut sorted: Vec<f64> = self.times.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let p95 = sorted.get(((sorted.len().saturating_sub(1)) as f64 * 0.95).round() as usize).copied().unwrap_or(0.0);
        FrameStats {
            p95_frame_ms: p95,
            pages_recorded: retained.counts.recorded,
            pages_reused: retained.counts.reused,
            recording_bytes: retained.bytes() as u64,
            ..self.stats
        }
    }
}

/// A press on a ruler, until its release: the marker it moves (none: a click on the text area), the view then, where
/// it was and is (viewport CSS px), and whether it moved.
#[derive(Clone, Debug)]
pub(super) struct RulerDrag {
    pub marker: Option<Marker>,
    pub view: RulerView,
    pub start: Point,
    pub now: Point,
    pub moved: bool,
    /// Alt is down: no snapping.
    pub free: bool,
}

impl RulerDrag {
    /// Where the drag puts its marker, page points along its ruler: snapped to the unit's minor marks from the text
    /// start, unless Alt is down.
    pub fn value(&self) -> f64 {
        let (h, v) = (&self.view.model.horizontal, &self.view.model.vertical);
        let horizontal = self.marker.is_none_or(Marker::horizontal);
        let (axis, at, zero) =
            if horizontal { (self.view.horizontal, self.now.x, h.text.start.0) } else { (self.view.vertical, self.now.y, v.text.start.0) };
        let pt = (at - axis.origin) / axis.scale.max(1e-9);
        if self.free { pt } else { crate::services::ruler::snap(pt, zero, self.view.unit) }
    }

    /// The guide line over the document: along which ruler, and its client coordinate.
    fn guide(&self) -> Option<(bool, f64)> {
        let m = self.marker.filter(|_| self.moved)?;
        let axis = if m.horizontal() { self.view.horizontal } else { self.view.vertical };
        Some((m.horizontal(), axis.origin + self.value() * axis.scale))
    }
}

/// The host's view of a session: viewport size, scroll and zoom, and the renderer's caches.
pub(super) struct Presentation {
    renderer: Renderer,
    width: f64,
    height: f64,
    dpr: f64,
    scroll: (f64, f64),
    /// Zoom as a factor (1 is 100%).
    zoom: f64,
    focused: bool,
    /// Room right of the pages for comment balloons, CSS px.
    markup_pane: f64,
    /// The balloons' leader lines the engine draws ([`HostSurface::set_leaders`]): polylines, content CSS px.
    leaders: Vec<Vec<Point>>,
    sink: Sink,
    /// The built-in rulers: which show, their unit (none: the language's), what a click adds, the drag under way, and
    /// the view `rulerChanged` last reported.
    pub(super) rulers: Bands,
    ruler_unit: Option<RulerUnit>,
    pub(super) tab_selector: TabSelector,
    pub(super) ruler_drag: Option<RulerDrag>,
    ruler_seen: Option<RulerView>,
    counters: FrameCounters,
    /// Each CPU frame is drawn a second time from scratch and compared ([`HostSurface::set_verify_frames`]).
    verify: bool,
    /// Verified frames that differed.
    verify_failures: u64,
    /// Timers the session wants armed: `(timer, ms)`, taken by [`HostSurface::take_timers`].
    timers: Vec<(u32, u32)>,
    /// Something visible changed with no new state (a blink phase): the host should draw a frame.
    frame_wanted: bool,
    pub(super) blink: super::blink::Blink,
    /// The selection the blink last restarted for.
    blink_selection: Option<crate::editing::selection::Selection>,
    /// Unique on the thread: names this session's WebGL canvas or GPU window.
    #[cfg_attr(not(any(target_os = "emscripten", twip_dawn)), allow(dead_code, reason = "no GPU target in this build"))]
    serial: u64,
}

impl Default for Presentation {
    fn default() -> Presentation {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Presentation {
            renderer: Renderer::retaining(),
            width: 0.0,
            height: 0.0,
            dpr: 1.0,
            scroll: (0.0, 0.0),
            zoom: 1.0,
            focused: false,
            markup_pane: 0.0,
            leaders: Vec::new(),
            sink: Sink::default(),
            rulers: Bands::default(),
            ruler_unit: None,
            tab_selector: TabSelector::default(),
            ruler_drag: None,
            ruler_seen: None,
            counters: FrameCounters::default(),
            verify: false,
            verify_failures: 0,
            timers: Vec::new(),
            frame_wanted: false,
            blink: super::blink::Blink::default(),
            blink_selection: None,
            serial: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl Presentation {
    /// Arms the blink timer and asks for a frame, as a blink step says.
    fn blinked(&mut self, step: super::blink::Step) {
        if let Some(ms) = step.arm {
            self.timers.push((super::blink::TIMER, ms));
        }
        self.frame_wanted |= step.frame;
    }

    /// Forgets what the last document's frames cached.
    pub(super) fn clear(&mut self) {
        self.renderer.clear_images();
        self.renderer.retained.clear();
        self.sink.a11y = None;
    }

    fn px_per_pt(&self) -> f64 {
        self.zoom * 96.0 / 72.0
    }

    /// The zoom, as a factor (1 is 100%).
    pub(super) fn zoom(&self) -> f64 {
        self.zoom
    }

    /// A viewport point, CSS px, in document points.
    pub(super) fn to_document(&self, at: Point) -> (f64, f64) {
        let k = self.px_per_pt();
        ((at.x + self.scroll.0) / k, (at.y + self.scroll.1) / k)
    }

    /// The viewport's size, CSS px.
    pub(super) fn viewport(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    /// The frame for a viewport of `width` x `height` device pixels.
    fn frame(&self, width: f64, height: f64) -> Frame {
        let caret = self.focused && self.blink.visible();
        Frame { origin_x: -self.scroll.0, origin_y: -self.scroll.1, width, height, dpr: self.dpr, px_per_pt: self.px_per_pt(), caret }
    }

    /// The unit the rulers show.
    fn unit(&self, language: &str) -> RulerUnit {
        self.ruler_unit.unwrap_or_else(|| crate::services::ruler::default_unit(language))
    }

    /// The markup pane's colour: the background a shade towards the middle grey.
    fn pane_color(&self) -> skia_safe::Color {
        let bg = self.renderer.background.unwrap_or(skia_safe::Color::from_rgb(0xf3, 0xf3, 0xf3));
        let shade = |c: u8| if bg.r() as u32 + bg.g() as u32 + bg.b() as u32 > 3 * 128 { c.saturating_sub(10) } else { c.saturating_add(16) };
        skia_safe::Color::from_rgb(shade(bg.r()), shade(bg.g()), shade(bg.b()))
    }
}

/// The rulers' settings in the state.
pub(super) fn fill_state(s: &Session, state: &mut crate::api::state::State) {
    let p = &s.presentation;
    state.view.rulers = crate::api::state::RulerVisibility { horizontal: p.rulers.horizontal, vertical: p.rulers.vertical };
    state.view.ruler_unit = p.unit(&s.language);
    state.view.tab_selector = p.tab_selector;
    state.view.formatting_marks = s.formatting_marks;
    state.view.text_boundaries = s.text_boundaries;
}

impl Session {
    /// Sets the zoom (a factor, 1 is 100%) about the caret's line, or the viewport's centre when the caret isn't in
    /// view: that point keeps its place in the viewport. The scroll moves now, and the host is asked to follow.
    pub(super) fn set_zoom(&mut self, factor: f64) {
        let k = self.presentation.px_per_pt();
        let stack = self.editor.stack();
        let caret = self.editor.caret_view().and_then(|c| stack.to_doc(c.page, c.x, c.top));
        let p = &mut self.presentation;
        let ((sx, sy), (w, h)) = (p.scroll, (p.width, p.height));
        let shows = |&(x, y): &(f64, f64)| (0.0..w).contains(&(x * k - sx)) && (0.0..h).contains(&(y * k - sy));
        let (ax, ay) = caret.filter(shows).unwrap_or(((sx + w / 2.0) / k, (sy + h / 2.0) / k));
        let (on_x, on_y) = (ax * k - sx, ay * k - sy);
        p.zoom = factor;
        let k = p.px_per_pt();
        let (cw, ch) = (stack.width * k + p.markup_pane, stack.height * k);
        // The SDKs centre content narrower than the viewport.
        let x = if cw < w { -((w - cw) / 2.0).floor() } else { (ax * k - on_x).clamp(0.0, cw - w) };
        let y = (ay * k - on_y).clamp(0.0, (ch - h).max(0.0));
        p.scroll = (x, y);
        self.request(HostRequest::Scroll { x, y });
    }

    /// The ruler model with its viewport mapping: the caret's page's left and top edges in the viewport.
    pub(super) fn ruler_view(&self) -> RulerView {
        let p = &self.presentation;
        let k = p.px_per_pt();
        let page = self.state.pages.current.max(1);
        let [x, y, ..] = self.editor.stack().pages.get(page as usize - 1).copied().unwrap_or_default();
        RulerView {
            model: self.state.ruler.clone(),
            page,
            horizontal: RulerAxis { origin: x * k - p.scroll.0, scale: k },
            vertical: RulerAxis { origin: y * k - p.scroll.1, scale: k },
            unit: p.unit(&self.language),
        }
    }

    /// An edit or a caret move restarts the blink from visible.
    pub(super) fn blink_follow(&mut self) {
        let selection = self.editor.selection();
        let p = &mut self.presentation;
        if p.blink_selection.as_ref() == Some(&selection) {
            return;
        }
        p.blink_selection = Some(selection);
        if p.focused {
            let step = p.blink.input(std::time::Instant::now());
            p.blinked(step);
        }
    }

    /// Sends the host one request now.
    pub(super) fn request(&mut self, r: HostRequest) {
        if let Some(deliver) = self.presentation.sink.deliver.as_mut() {
            deliver(r);
        }
    }

    /// Hands the attached host what changed for it: the editor's requests, the cursor, caret, tooltip and
    /// accessibility tree. Without a host the editor's requests are dropped.
    pub(super) fn deliver_requests(&mut self) {
        let requests = self.editor.take_requests();
        if self.presentation.sink.deliver.is_none() {
            return;
        }
        let k = self.presentation.px_per_pt();
        let stack = self.editor.stack();
        let mut out = Vec::new();
        for r in requests {
            match r {
                Request::CapturePointer => out.push(HostRequest::Capture),
                Request::ReleasePointer => out.push(HostRequest::Release),
                // A keyboard copy: the engine's clipboard too, so a paste in any session keeps its formatting.
                Request::Clipboard { text, html } => {
                    let data = match self.editor.clipboard().cloned() {
                        Some(f) => self.hold(f),
                        None => ClipboardData { text, html, internal: None },
                    };
                    out.push(HostRequest::Clipboard { data });
                }
                Request::ScrollIntoView { page, rect: [x0, y0, x1, y1] } => {
                    if let Some((x, y)) = stack.to_doc(page, x0, y0) {
                        out.push(HostRequest::ScrollTo { x: x * k, y: y * k, width: (x1 - x0) * k, height: (y1 - y0) * k });
                    }
                }
                // shortcut: hosts autoscroll a drag themselves, and placeholder activation is an event the
                // pointer work emits; neither reaches the host from here.
                Request::Autoscroll { .. } | Request::Placeholder(_) => {}
            }
        }
        let view = self.editor.view(0..0);
        let hovered = self.editor.hover_tooltip();
        let cursor = view.cursor;
        let s = &mut self.presentation.sink;
        if s.cursor != Some(cursor) {
            s.cursor = Some(cursor);
            out.push(HostRequest::Cursor { cursor });
        }
        let info = self.host().caret_info();
        let caret = (info.caret_x, info.caret_y, info.caret_height, info.caret_visible);
        let (scroll, s) = (self.presentation.scroll, &mut self.presentation.sink);
        if s.caret != Some(caret) {
            s.caret = Some(caret);
            out.push(HostRequest::Caret { x: caret.0, y: caret.1, height: caret.2, visible: caret.3 });
        }
        let tooltip = hovered.and_then(|(kind, t)| {
            let (x, y) = stack.to_doc(t.page, t.rect[0], t.rect[1])?;
            let (left, top) = (x * k - scroll.0, y * k - scroll.1);
            let (width, height) = ((t.rect[2] - t.rect[0]) * k, (t.rect[3] - t.rect[1]) * k);
            let rect = ClientRect { left, top, width, height };
            Some(Tooltip { kind, text: t.text, at: ClientPoint { client_x: left, client_y: top + height }, rect })
        });
        let tooltip_changed = s.tooltip != tooltip;
        s.tooltip.clone_from(&tooltip);
        let revision = self.editor.revision();
        if self.presentation.sink.a11y != Some(revision) {
            // shortcut: the whole tree goes again after each document change; send the changed blocks when
            // documents of hundreds of pages make typing slow.
            self.presentation.sink.a11y = Some(revision);
            out.push(HostRequest::A11y { tree: self.accessible_tree() });
        }
        if tooltip_changed {
            self.emit(Event::Tooltip(tooltip));
        }
        if let Some(deliver) = self.presentation.sink.deliver.as_mut() {
            out.into_iter().for_each(deliver);
        }
    }
}

/// The WebGL state of this session's canvas goes with it.
#[cfg(target_os = "emscripten")]
impl Drop for Presentation {
    fn drop(&mut self) {
        crate::render::web::detach_gl(self.serial);
    }
}

/// So does the swap chain on its window.
#[cfg(twip_dawn)]
impl Drop for Presentation {
    fn drop(&mut self) {
        crate::render::window::detach(self.serial);
    }
}

impl Session {
    /// The presentation surface: what a host that shows the document calls.
    pub fn host(&mut self) -> HostSurface<'_> {
        HostSurface { session: self }
    }
}

/// A session's presentation, borrowed for one host call.
pub struct HostSurface<'a> {
    session: &'a mut Session,
}

impl HostSurface<'_> {
    /// A host shows the session: `sink` gets its [`HostRequest`]s from now on, starting with the cursor, caret
    /// and accessibility tree. Replaces an earlier host.
    pub fn attach(&mut self, sink: impl FnMut(HostRequest) + 'static) {
        self.session.presentation.sink = Sink { deliver: Some(Box::new(sink)), ..Sink::default() };
        self.session.deliver_requests();
    }

    /// The host is gone: requests are dropped until the next [`HostSurface::attach`].
    pub fn detach(&mut self) {
        self.session.presentation.sink = Sink::default();
    }

    /// The viewport's size in CSS px, and device pixels per CSS pixel.
    pub fn set_size(&mut self, width: f64, height: f64, dpr: f64) {
        let p = &mut self.session.presentation;
        (p.width, p.height, p.dpr) = (width.max(0.0), height.max(0.0), if dpr > 0.0 { dpr } else { 1.0 });
    }

    /// The scroll position: the viewport's top-left in content CSS px.
    pub fn set_scroll(&mut self, x: f64, y: f64) {
        self.session.presentation.scroll = (x, y);
    }

    /// Whether the host's view has the keyboard focus.
    pub fn set_focus(&mut self, focused: bool) {
        let p = &mut self.session.presentation;
        if p.focused == focused {
            return;
        }
        p.focused = focused;
        // The caret comes or goes; focus back restarts its blink from visible.
        p.frame_wanted = true;
        if focused {
            let step = p.blink.input(std::time::Instant::now());
            p.blinked(step);
        }
    }

    /// How the caret blinks: `interval` is a phase's length (none: a steady caret; at least 100 ms), and blinking
    /// stops, the caret visible, after `stop_after` without input (none: never). Default: 530 ms, never.
    pub fn set_caret_blink(&mut self, blink: CaretBlink) {
        let ms = |v: Option<u32>| v.map(|m| std::time::Duration::from_millis(u64::from(m)));
        let p = &mut self.session.presentation;
        let step = p.blink.set(ms(blink.interval_ms), ms(blink.stop_after_ms), std::time::Instant::now());
        p.blinked(step);
        p.frame_wanted = true;
    }

    /// Whether the host's view has the keyboard focus.
    pub fn focused(&self) -> bool {
        self.session.presentation.focused
    }

    /// Draws the viewport into `pixels`: `width` x `height` device pixels, RGBA, 8 bits each, premultiplied, rows
    /// packed. Fails `invalid-argument` when the buffer is too small.
    pub fn frame_rgba(&mut self, pixels: &mut [u8], width: u32, height: u32) -> Result<FrameInfo, Error> {
        let row = width as usize * 4;
        if width == 0 || height == 0 || pixels.len() < row * height as usize {
            return Err(Error::invalid("the pixel buffer is smaller than width x height x 4"));
        }
        let (w, h) = (i32::try_from(width).map_err(|_| Error::invalid("width"))?, i32::try_from(height).map_err(|_| Error::invalid("height"))?);
        let info = skia_safe::ImageInfo::new((w, h), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None);
        let Some(mut surface) = skia_safe::surfaces::wrap_pixels(&info, pixels, row, None) else {
            return Err(Error::invalid("the pixel buffer can't be drawn into"));
        };
        self.draw(surface.canvas(), f64::from(width), f64::from(height));
        drop(surface);
        if self.session.presentation.verify {
            let mut again = vec![0u8; pixels.len()];
            if let Some(mut direct) = skia_safe::surfaces::wrap_pixels(&info, &mut again, row, None) {
                self.session.presentation.renderer.direct = true;
                self.draw_frame(direct.canvas(), f64::from(width), f64::from(height));
                self.session.presentation.renderer.direct = false;
            }
            if again[..] != pixels[..row * height as usize] {
                let p = &mut self.session.presentation;
                p.verify_failures += 1;
                let diffs = again.iter().zip(&pixels[..row * height as usize]).map(|(a, b)| a.abs_diff(*b));
                let (n, worst) = diffs.fold((0usize, 0u8), |(n, w), d| (n + usize::from(d > 0), w.max(d)));
                let at: Vec<usize> =
                    again.iter().zip(&pixels[..row * height as usize]).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i / 4).collect();
                let (xs, ys): (Vec<usize>, Vec<usize>) = at.iter().map(|i| (i % width as usize, i / width as usize)).unzip();
                eprintln!(
                    "verify: frame {} differs in {n} bytes, by at most {worst}, x {:?}..{:?} y {:?}..{:?}",
                    p.counters.stats.frames,
                    xs.iter().min(),
                    xs.iter().max(),
                    ys.iter().min(),
                    ys.iter().max()
                );
            }
        }
        Ok(self.frame_info())
    }

    /// Test builds and tools: each CPU frame ([`HostSurface::frame_rgba`]) is drawn a second time from scratch,
    /// past the retained page content, and compared pixel for pixel.
    #[doc(hidden)]
    pub fn set_verify_frames(&mut self, on: bool) {
        self.session.presentation.verify = on;
    }

    /// Tools: draw frames from scratch (`false`), past the retained page content, to time them.
    #[doc(hidden)]
    pub fn set_retained(&mut self, on: bool) {
        self.session.presentation.renderer.retain = on;
    }

    /// The verified frames that differed from their from-scratch drawing.
    #[doc(hidden)]
    pub fn verify_failures(&self) -> u64 {
        self.session.presentation.verify_failures
    }

    /// Adopts the current WebGL 2 context (the page made it current) for this session's canvas.
    #[cfg(target_os = "emscripten")]
    pub fn attach_webgl(&mut self) -> bool {
        crate::render::web::attach_gl(self.session.presentation.serial)
    }

    /// Draws the viewport into this session's WebGL canvas, `width` x `height` device pixels.
    #[cfg(target_os = "emscripten")]
    pub fn frame_webgl(&mut self, width: u32, height: u32) -> Result<FrameInfo, Error> {
        let key = self.session.presentation.serial;
        let (w, h) = (i32::try_from(width).map_err(|_| Error::invalid("width"))?, i32::try_from(height).map_err(|_| Error::invalid("height"))?);
        if !crate::render::web::gl_frame(key, w, h, |c| self.draw(c, f64::from(width), f64::from(height))) {
            return Err(Error::unavailable("no WebGL context is attached"));
        }
        Ok(self.frame_info())
    }

    /// Draws this session's frames into the window `hwnd` with the GPU from now on: the backend (1 D3D12, 2 D3D11,
    /// 3 Vulkan), or `None` when there is no usable GPU or this build has none (draw with [`HostSurface::frame_rgba`]).
    /// # Safety
    /// `hwnd` is a live window that outlives the attach ([`HostSurface::detach_window`], or the session's end).
    pub unsafe fn attach_window(&mut self, hwnd: *mut std::ffi::c_void) -> Option<u32> {
        // SAFETY: the caller's promise.
        #[cfg(twip_dawn)]
        return unsafe { crate::render::window::attach(self.session.presentation.serial, hwnd) }.map(|b| b as u32);
        #[cfg(not(twip_dawn))]
        {
            let _ = hwnd;
            None
        }
    }

    /// Stops drawing into the window [`HostSurface::attach_window`] gave.
    pub fn detach_window(&mut self) {
        #[cfg(twip_dawn)]
        crate::render::window::detach(self.session.presentation.serial);
    }

    /// Testing: loses the GPU device as a driver reset would. Window frames fail `unavailable` until the next
    /// [`HostSurface::attach_window`], which makes a new device. Nothing without a GPU build.
    #[doc(hidden)]
    pub fn lose_gpu_device(&mut self) {
        #[cfg(twip_dawn)]
        crate::render::window::lose_device();
    }

    /// Draws the viewport into the attached window, `width` x `height` device pixels, and presents it. Fails
    /// `unavailable` when no window is attached or the GPU can't draw now (the device was lost): draw with
    /// [`HostSurface::frame_rgba`] then.
    pub fn frame_window(&mut self, width: u32, height: u32) -> Result<FrameInfo, Error> {
        #[cfg(twip_dawn)]
        {
            let key = self.session.presentation.serial;
            if crate::render::window::frame(key, width, height, |c| self.draw(c, f64::from(width), f64::from(height))) {
                return Ok(self.frame_info());
            }
        }
        let _ = (width, height);
        Err(Error::unavailable("no GPU window is attached"))
    }

    /// Draws the viewport on `canvas`, `width` x `height` device pixels: the GPU paths hand their canvas here.
    #[doc(hidden)]
    pub fn draw(&mut self, canvas: &skia_safe::Canvas, width: f64, height: f64) {
        let started = std::time::Instant::now();
        self.draw_frame(canvas, width, height);
        let ms = started.elapsed().as_secs_f64() * 1e3;
        self.session.presentation.counters.frame(ms);
    }

    /// What the frames have cost so far.
    pub fn frame_stats(&self) -> FrameStats {
        let p = &self.session.presentation;
        p.counters.stats(&p.renderer.retained)
    }

    fn draw_frame(&mut self, canvas: &skia_safe::Canvas, width: f64, height: f64) {
        let s = &mut *self.session;
        let p = &mut s.presentation;
        let f = p.frame(width, height);
        let fonts = s.fonts.fonts();
        // Lay out the visible pages first (they may be provisional), then build the frame's view from that snapshot.
        let visible = Renderer::visible(&s.editor.stack(), &f);
        if visible.end > 0 {
            s.editor.ensure_page(visible.end - 1, &fonts);
        }
        let marks = s.services.spelling.squiggles(&mut s.editor, visible.clone(), &s.language);
        let formatting_marks = if s.formatting_marks { s.editor.formatting_marks(visible.clone()) } else { Vec::new() };
        let boundaries = if s.text_boundaries { s.editor.text_boundaries(visible.clone()) } else { Vec::new() };
        let mut view = s.editor.view(visible);
        view.spelling = marks;
        view.formatting_marks = formatting_marks;
        view.boundaries = boundaries;
        p.renderer.gridlines = s.gridlines;
        p.renderer.draw(canvas, &view, &s.editor.document().images, &f);
        if p.markup_pane > 0.0 {
            let left = (view.stack.width * f.px_per_pt + f.origin_x) * f.dpr;
            let r = skia_safe::Rect::from_xywh(left as f32, 0.0, (p.markup_pane * f.dpr) as f32, height as f32);
            canvas.draw_rect(r, &skia_safe::Paint::new(skia_safe::Color4f::from(p.pane_color()), None));
        }
        // The balloons' leaders, over everything: from each comment's text to its card, one device pixel wide.
        if !p.leaders.is_empty() {
            let mut line = skia_safe::Paint::default();
            line.set_anti_alias(true);
            line.set_style(skia_safe::paint::Style::Stroke);
            line.set_color(p.renderer.palette.leaders);
            line.set_stroke_width(f.dpr.round().max(1.0) as f32);
            for polyline in &p.leaders {
                let points: Vec<skia_safe::Point> = polyline
                    .iter()
                    .map(|q| skia_safe::Point::new(((q.x + f.origin_x) * f.dpr) as f32, ((q.y + f.origin_y) * f.dpr) as f32))
                    .collect();
                canvas.draw_points(skia_safe::canvas::PointMode::Polygon, &points, &line);
            }
        }
        drop(fonts);
        // The rulers sit over the viewport's top and left edges, the document scrolling under them.
        // shortcut: revealing the selection doesn't allow for them, so a caret scrolled to the top edge can sit under
        // the horizontal ruler.
        let ruler = s.ruler_view();
        let p = &s.presentation;
        if p.rulers != Bands::default() {
            let frame = crate::render::ruler::RulerFrame {
                bands: p.rulers,
                width: width / f.dpr,
                height: height / f.dpr,
                dpr: f.dpr,
                selector: p.tab_selector,
                guide: p.ruler_drag.as_ref().and_then(RulerDrag::guide),
                typeface: crate::render::ruler::typeface(&p.renderer),
            };
            let theme = crate::render::ruler::RulerTheme::of(p.renderer.background.unwrap_or(skia_safe::Color::from_rgb(0xf3, 0xf3, 0xf3)));
            crate::render::ruler::paint(&ruler, &theme, canvas, &frame);
        }
        // Coalesced per frame: the view as this frame shows it, when it changed.
        if s.presentation.ruler_seen.as_ref() != Some(&ruler) {
            s.presentation.ruler_seen = Some(ruler.clone());
            s.emit(Event::RulerChanged(ruler));
        }
        let stack = s.editor.stack();
        s.set_comment_anchors(&view, &stack, f.px_per_pt);
        // The pages just laid out may have placed the caret.
        s.deliver_requests();
    }

    /// The document's size and the caret after the last frame: `caret_visible` is whether the frame drew the caret
    /// (focused, collapsed, in the blink's visible phase).
    pub fn frame_info(&mut self) -> FrameInfo {
        let p = &self.session.presentation;
        let drawn = p.focused && p.blink.visible();
        let mut info = self.caret_info();
        info.caret_visible &= drawn;
        info
    }

    /// The document's size and the caret, `caret_visible` for a collapsed selection whatever the focus and blink:
    /// what the IME and the system caret follow.
    fn caret_info(&mut self) -> FrameInfo {
        let k = self.session.presentation.px_per_pt();
        let e = &mut self.session.editor;
        let stack = e.stack();
        let caret = e.caret_view().and_then(|c| Some((stack.to_doc(c.page, c.x, c.top)?, c.bottom - c.top)));
        let ((caret_x, caret_y), caret_height) = caret.unwrap_or(((0.0, 0.0), 0.0));
        FrameInfo {
            content_width: stack.width * k + self.session.presentation.markup_pane,
            content_height: stack.height * k,
            caret_x: caret_x * k,
            caret_y: caret_y * k,
            caret_height: caret_height * k,
            caret_visible: caret.is_some() && e.selection().is_collapsed(),
            pages: e.snapshot().pages().len() as u32,
        }
    }

    /// The caret's bottom-left in the viewport: where a menu opened from the keyboard goes.
    pub(super) fn caret_client_point(&mut self) -> crate::api::services::ClientPoint {
        let f = self.frame_info();
        let (x, y) = self.session.presentation.scroll;
        crate::api::services::ClientPoint { client_x: f.caret_x - x, client_y: f.caret_y + f.caret_height - y }
    }

    /// The one-shot timers the session wants armed since the last call, as `(timer, ms)`: the host arms each and calls
    /// [`HostSurface::timer_fired`] with its number when it goes off. Timer 1 (background layout) isn't among them:
    /// the host arms it while `pages.complete` is false.
    pub fn take_timers(&mut self) -> Vec<(u32, u32)> {
        std::mem::take(&mut self.session.presentation.timers)
    }

    /// Whether something visible changed since the last call with no new state (the caret's blink phase): the host
    /// draws a frame then, as after a state change.
    pub fn take_frame_request(&mut self) -> bool {
        std::mem::take(&mut self.session.presentation.frame_wanted)
    }

    /// A timer the engine asked for (`set_timer`) went off: the C ABI arms one while background layout is pending,
    /// and each runs one slice of it ([`HostSurface::idle`]).
    pub fn timer_fired(&mut self, timer: u32) {
        if timer == super::blink::TIMER {
            let s = &mut *self.session;
            let active = s.presentation.focused && s.editor.selection().is_collapsed();
            let step = s.presentation.blink.fired(active, std::time::Instant::now());
            s.presentation.blinked(step);
            return;
        }
        self.idle();
    }

    /// Background layout for one slice of a few milliseconds: true while more is pending. When the slice changes
    /// `pages.count` or `pages.complete`, a new state goes out (same revision). The SDKs run it on the engine's
    /// timers; a host of `twip::api` calls it when it has nothing else to do, until it returns false.
    pub fn idle(&mut self) -> bool {
        let s = &mut *self.session;
        s.editor.idle(&s.fonts.fonts());
        let (count, complete) = crate::api::state::pages(&s.editor);
        if (count, complete) != (s.state.pages.count, s.state.pages.complete) {
            s.changed(Vec::new());
        }
        !complete
    }

    /// The comment balloons' leader lines, polylines in content CSS px (they scroll with the document): the engine
    /// draws them over the frame in the overlay palette's leader colour, so a host drawing on the GPU draws nothing
    /// over it. An empty list draws none.
    pub fn set_leaders(&mut self, leaders: Vec<Vec<Point>>) {
        self.session.presentation.leaders = leaders;
        self.session.presentation.frame_wanted = true;
    }

    /// The memory the session keeps its pages' drawing in.
    pub fn set_paint_budget(&mut self, budget: PaintBudget) {
        self.session.presentation.renderer.retained.budget = usize::try_from(budget.recordings).unwrap_or(usize::MAX);
    }

    /// The colours of what the editor draws over the document.
    pub fn set_overlay_palette(&mut self, palette: OverlayPalette) {
        let c = |c: Color| crate::render::rgb(c.0);
        self.session.presentation.renderer.palette = crate::render::Palette {
            caret: c(palette.caret),
            selection: c(palette.selection),
            object: c(palette.object),
            drop: c(palette.drop),
            marks: c(palette.marks),
            leaders: c(palette.leaders),
        };
        self.session.presentation.frame_wanted = true;
    }

    /// The background around the pages; the paper stays white.
    pub fn set_background(&mut self, color: Color) {
        self.session.presentation.renderer.background = Some(crate::render::rgb(color.0));
    }

    /// Room right of the pages for comment balloons, CSS px: the content gets wider by it, so a host that centres
    /// the content moves the pages aside. It takes a shade of the background.
    pub fn set_markup_pane_width(&mut self, px: f64) {
        self.session.presentation.markup_pane = if px.is_finite() { px.max(0.0) } else { 0.0 };
    }

    /// Which built-in rulers show, along the viewport's top and left edges (off by default).
    pub fn set_rulers(&mut self, horizontal: bool, vertical: bool) {
        self.session.presentation.rulers = Bands { horizontal, vertical };
        self.session.presentation.ruler_drag = None;
        self.session.changed(Vec::new());
    }

    /// The unit the rulers show (by default the language's: inches for en-US, centimetres otherwise).
    pub fn set_ruler_unit(&mut self, unit: RulerUnit) {
        self.session.presentation.ruler_unit = Some(unit);
        self.session.changed(Vec::new());
    }

    /// What a click on the ruler adds; the selector box in the rulers' corner cycles it.
    pub fn set_tab_selector(&mut self, kind: TabSelector) {
        self.session.presentation.tab_selector = kind;
        self.session.changed(Vec::new());
    }

    /// The ruler model with its viewport mapping: the client x of the caret's page's left edge, its top edge's y,
    /// and CSS px per point.
    pub fn ruler_view(&self) -> RulerView {
        self.session.ruler_view()
    }

    /// The visible comments' anchors.
    pub fn comment_anchors(&self) -> Vec<CommentAnchor> {
        self.session.services.anchors.clone()
    }

    /// A viewport point in the page stack's points.
    fn doc_point(&self, at: Point) -> (f64, f64) {
        let p = &self.session.presentation;
        let k = p.px_per_pt();
        ((at.x + p.scroll.0) / k, (at.y + p.scroll.1) / k)
    }

    /// What is under a viewport point, without moving the selection. Lays out the page there.
    pub fn hit_test(&mut self, at: Point) -> HitResult {
        let (x, y) = self.doc_point(at);
        let s = &mut *self.session;
        let hit = s.editor.hit_test(x, y, &s.fonts.fonts());
        HitResult { element: hit.and_then(|h| h.1).map(|e| ElementId(elements::id_str(e))), editable: hit.is_some_and(|h| h.2) }
    }

    /// An external drag over the viewport: shows the drop caret where text can go in.
    pub fn drag_over(&mut self, at: Point) {
        let at = self.doc_point(at);
        let s = &mut *self.session;
        s.editor.set_drop(Some(at), &s.fonts.fonts());
    }

    /// The external drag left.
    pub fn drag_leave(&mut self) {
        let s = &mut *self.session;
        s.editor.set_drop(None, &s.fonts.fonts());
    }

    /// Runs `command` at the drop point, as one transaction: the caret goes there first. Fails `unavailable` where
    /// text can't go in.
    pub fn drop(&mut self, at: Point, command: Command) -> Result<Commit, Error> {
        let (x, y) = self.doc_point(at);
        self.session.check(&command)?;
        self.session.apply_with(Origin::User, |e, fonts| {
            e.set_drop(None, fonts);
            let place = e.hit_test(x, y, fonts).filter(|h| h.2).and_then(|h| h.0.para_offset());
            let Some((para, offset)) = place else { return Outcome::Refused(Error::unavailable("nothing can be dropped here")) };
            e.select_range(para, offset, offset, fonts);
            e.execute(command, fonts)
        })
    }

    /// Scrolls the selection into view: a `scroll_to` request for the selection's focus.
    pub fn reveal_selection(&mut self) {
        let k = self.session.presentation.px_per_pt();
        let e = &mut self.session.editor;
        let Some(c) = e.caret_view() else { return };
        let Some((x, y)) = e.stack().to_doc(c.page, c.x, c.top) else { return };
        let request = HostRequest::ScrollTo { x: x * k, y: y * k, width: k, height: (c.bottom - c.top) * k };
        if let Some(deliver) = self.session.presentation.sink.deliver.as_mut() {
            deliver(request);
        }
    }

    /// The pages laid out in the viewport and where they are, viewport CSS px (the host adds the view's client
    /// offset).
    pub fn visible_pages(&self) -> Vec<PageRect> {
        let p = &self.session.presentation;
        let k = p.px_per_pt();
        let stack = self.session.editor.stack();
        let pages = stack.pages.iter().enumerate().map(|(i, &[x, y, w, h])| {
            let rect = ClientRect { left: x * k - p.scroll.0, top: y * k - p.scroll.1, width: w * k, height: h * k };
            PageRect { page: i as u32, rect }
        });
        pages.filter(|r| r.rect.top < p.height && r.rect.top + r.rect.height > 0.0).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::api::command::Command;
    use crate::api::engine::{Engine, SessionOptions};
    use crate::api::fonts::FontLibrary;
    use crate::api::input::Point;

    #[test]
    fn the_rulers_add_move_and_delete_tab_stops_one_step_each() {
        use crate::api::event::Event;
        use crate::api::input::{Modifiers, PointerButton as B, PointerKind as K};
        use crate::api::services::{DialogKind, TabSelector};
        let mut engine = Engine::new(FontLibrary::empty());
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        s.run(Command::InsertText("Hello".into())).expect("types");
        s.host().set_size(800.0, 600.0, 1.0);
        s.host().set_rulers(true, true);
        assert!(s.state().view.rulers.horizontal && s.state().view.rulers.vertical);
        let mut pixels = vec![0u8; 800 * 600 * 4];
        s.host().frame_rgba(&mut pixels, 800, 600).expect("draws");
        assert!(events.borrow().iter().any(|e| matches!(e, Event::RulerChanged(v) if v.horizontal.origin == 24.0)));
        // The page's left edge is 18 pt in, its text an inch further; a CSS px is 3/4 pt. 144 pt in is x 312.
        let x = |pt: f64| 24.0 + (72.0 + pt) / 0.75;
        fn press(s: &mut crate::api::session::Session, kind: K, x: f64, y: f64, clicks: u8) {
            s.pointer(kind, Point { x, y }, B::Primary, clicks, Modifiers::default());
        }
        let tabs = |s: &crate::api::session::Session| s.state().ruler.horizontal.tabs.iter().map(|t| t.position.0).collect::<Vec<_>>();
        // A click on the text area adds a left stop, snapped to the eighth inch.
        press(s, K::Down, x(145.0), 10.0, 1);
        press(s, K::Up, x(145.0), 10.0, 1);
        assert_eq!(tabs(s), [144.0]);
        // Dragging it moves it on release only.
        press(s, K::Down, x(144.0), 15.0, 1);
        press(s, K::Move, x(181.0), 15.0, 0);
        assert_eq!(tabs(s), [144.0], "nothing commits during the drag");
        press(s, K::Up, x(181.0), 15.0, 0);
        assert_eq!(tabs(s), [180.0]);
        // Dragged off the ruler, it goes.
        press(s, K::Down, x(180.0), 15.0, 1);
        press(s, K::Move, x(180.0), 120.0, 0);
        press(s, K::Up, x(180.0), 120.0, 0);
        assert!(tabs(s).is_empty());
        assert_eq!(s.state().history.undo_label.as_deref(), Some("Delete Tab Stop"));
        // The corner box cycles the tab selector; a double-click asks for the Tabs dialog.
        press(s, K::Down, 5.0, 5.0, 1);
        assert_eq!(s.state().view.tab_selector, TabSelector::Center);
        press(s, K::Down, x(300.0), 10.0, 2);
        let events = events.borrow();
        assert!(events.iter().any(|e| matches!(e, Event::DialogRequested { kind: DialogKind::Tabs })));
        assert_eq!(events.iter().filter(|e| matches!(e, Event::DocumentChanged(_))).count(), 4, "typing and three tab commands");
    }

    #[test]
    fn a_drop_runs_its_command_at_the_hit_position() {
        let mut engine = Engine::new(FontLibrary::system());
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        s.run(Command::InsertText("Hello".into())).expect("types");
        // The first line's start: the page sits 18 pt in, its margins are an inch, a CSS px is 3/4 pt.
        let start = Point { x: (18.0 + 72.0 + 1.0) / 0.75, y: (18.0 + 72.0 + 6.0) / 0.75 };
        s.run(Command::SelectAll).expect("selects");
        let hit = s.host().hit_test(start);
        assert!(hit.editable, "{hit:?}");
        s.host().drag_over(start);
        assert!(s.editor.view(0..0).drop.is_some(), "the drop caret shows");
        s.host().drag_leave();
        assert!(s.editor.view(0..0).drop.is_none());
        s.host().drop(start, Command::InsertText("!".into())).expect("drops");
        s.run(Command::SelectAll).expect("selects");
        assert_eq!(s.editor.selected_text(), "!Hello");
    }
}
