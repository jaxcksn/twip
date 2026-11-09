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
    /// The text around the caret and the selection in it changed: for the element with the keyboard focus, which a
    /// screen reader reads as the caret moves. It goes with the `caret` request.
    CaretText {
        /// The text and the selection.
        window: crate::api::services::CaretWindow,
    },
    /// The accessibility tree changed: the whole tree, or the changes since the last `a11y` request. The first after
    /// attaching or opening is the whole tree; while edits keep coming, one goes at most every 250 ms.
    A11y {
        /// The tree or the changes.
        update: crate::api::services::A11yUpdate,
    },
}

/// Where the host's requests go, and what it was last told.
#[derive(Default)]
struct Sink {
    deliver: Option<Box<dyn FnMut(HostRequest)>>,
    cursor: Option<Cursor>,
    caret: Option<(f64, f64, f64, bool)>,
    tooltip: Option<Tooltip>,
    /// The hovered and the selected element as last sent.
    hover: Option<crate::api::services::ElementHover>,
    selected: Option<crate::api::services::ElementSelection>,
    /// The accessibility tree the host was last sent, and the document revision it was built at.
    a11y: Option<(AccessibleNode, u64)>,
    /// The accessibility timer is armed: an update goes when it fires.
    a11y_armed: bool,
    /// The caret window the host was last sent.
    window: Option<crate::services::a11y::Window>,
}

/// The timer that sends the accessibility tree's changes after an edit (timer 1 is background layout, 2 the caret's
/// blink).
pub(super) const A11Y_TIMER: u32 = 3;

/// What the reading tree is read from: the document, and the sub-template view (which references there are, which
/// bodies show in place, which one is edited). The tree is sent again when this changes.
fn a11y_key(e: &crate::editing::Editor) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (e.revision(), e.references().revision, e.editing_sub_template()).hash(&mut h);
    let mut shown: Vec<u64> = e.references().by_field.keys().filter(|f| e.reference_expanded(**f)).map(|f| f.0).collect();
    shown.sort_unstable();
    shown.hash(&mut h);
    h.finish()
}

/// How long after a change the accessibility tree's changes go: edits within it go together.
const A11Y_MS: u32 = 250;

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

/// The colours of what the editor draws over the document ([`HostSurface::set_overlay_palette`]). Setting them wins
/// over the chrome theme's; the defaults are the light theme's with the default accent: lines and marks at least 3:1
/// against the page, and the selection a light tint black text keeps at least 4.5:1 over. The caret is always black
/// or white, whichever stands out more on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayPalette {
    /// The selection, blended under the content (multiply), so black text stays black over it. Default `#9DC7F4`,
    /// the accent's light tint.
    pub selection: Color,
    /// A selected object's outline and handles. Default `#0A74E4`, the accent.
    pub object: Color,
    /// The drop caret of a drag. Default `#62656C`.
    pub drop: Color,
    /// Formatting marks and text boundaries. Default `#0A74E4`.
    pub marks: Color,
    /// The markup pane's leader lines. Default `#62656C`.
    pub leaders: Color,
}

impl Default for OverlayPalette {
    fn default() -> OverlayPalette {
        let p = crate::render::Palette::default();
        let c = |c: skia_safe::Color| Color((u32::from(c.r()) << 16) | (u32::from(c.g()) << 8) | u32::from(c.b()));
        OverlayPalette { selection: c(p.selection), object: c(p.object), drop: c(p.drop), marks: c(p.marks), leaders: c(p.leaders) }
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
    /// A tab stop dragged far enough below the ruler that the release removes it.
    pub removing: bool,
}

/// The pointer over the rulers with no button down: where (viewport CSS px), and whether Alt is down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RulerHover {
    pub at: Point,
    pub free: bool,
}

impl RulerDrag {
    /// The moved marker and where the drag puts it (page points along its ruler), as the rulers draw it.
    fn shown(&self) -> Option<crate::render::ruler::Dragged> {
        let marker = self.marker.filter(|_| self.moved)?;
        Some(crate::render::ruler::Dragged { marker, at: self.value(), removing: self.removing })
    }

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
        let m = self.marker.filter(|_| self.moved && !self.removing)?;
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
    pub(super) ruler_hover: Option<RulerHover>,
    ruler_seen: Option<RulerView>,
    /// The popover the application was asked to show and hasn't closed.
    pub(super) popover: Option<crate::api::services::Popover>,
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
    #[cfg_attr(not(any(target_os = "emscripten", twip_window)), allow(dead_code, reason = "no GPU target in this build"))]
    serial: u64,
    /// The view settings the chrome and page are drawn with ([`Presentation::apply_look`]); `chrome_set` once the host
    /// chose a scheme (before that, a dark background gives dark chrome).
    pub(super) chrome: crate::api::services::ChromeTheme,
    pub(super) chrome_set: bool,
    pub(super) accent: Option<Color>,
    pub(super) theme_colors: crate::api::services::ThemeColors,
    pub(super) appearance: crate::api::services::DocumentAppearance,
    pub(super) detail: crate::api::services::MeasureDetail,
    pub(super) object_grid: crate::api::services::ObjectGrid,
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
            ruler_hover: None,
            ruler_seen: None,
            popover: None,
            counters: FrameCounters::default(),
            verify: false,
            verify_failures: 0,
            timers: Vec::new(),
            frame_wanted: false,
            blink: super::blink::Blink::default(),
            blink_selection: None,
            serial: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            chrome: Default::default(),
            chrome_set: false,
            accent: None,
            theme_colors: Default::default(),
            appearance: Default::default(),
            detail: Default::default(),
            object_grid: Default::default(),
        }
    }
}

impl Presentation {
    /// The renderer's theme and page appearance from the view settings.
    pub(super) fn apply_look(&mut self) {
        use crate::api::services::{ChromeTheme, DocumentAppearance};
        use crate::render::appearance::{Appearance, PageMode};
        use crate::render::theme::{AZURE, Scheme, SystemColours, Theme};
        let c = |c: Color| crate::render::rgb(c.0);
        let scheme = match self.chrome {
            ChromeTheme::Light => Scheme::Light,
            ChromeTheme::Dark => Scheme::Dark,
            ChromeTheme::HighContrast(s) => Scheme::HighContrast(SystemColours {
                canvas: c(s.canvas),
                text: c(s.canvas_text),
                highlight: c(s.highlight),
                highlight_text: c(s.highlight_text),
                gray_text: c(s.gray_text),
            }),
        };
        let mut theme = Theme::new(scheme, self.accent.map_or(AZURE, c));
        let own = match scheme {
            Scheme::Light => self.theme_colors.light,
            Scheme::Dark => self.theme_colors.dark,
            Scheme::HighContrast(_) => None,
        };
        if let Some(o) = own {
            let t = &mut theme.tokens;
            for (to, from) in [
                (&mut t.desk, o.desk),
                (&mut t.paper, o.surface),
                (&mut t.ink, o.text),
                (&mut t.ink2, o.secondary_text),
                (&mut t.ink3, o.faint_text),
                (&mut t.line, o.line),
            ] {
                if let Some(v) = from {
                    *to = c(v);
                }
            }
        }
        self.renderer.set_theme(theme);
        self.renderer.appearance = match self.appearance {
            DocumentAppearance::Light => Appearance::Light,
            DocumentAppearance::Dark => Appearance::Dark { page: PageMode::Dark },
            DocumentAppearance::DarkWithLightPage => Appearance::Dark { page: PageMode::Light },
        };
        self.frame_wanted = true;
    }
}

impl Presentation {
    /// Asks the host for a frame: something visible changed with no new state.
    pub(super) fn want_frame(&mut self) {
        self.frame_wanted = true;
    }

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

    /// The accessibility timer went off: the tree's changes since it was last sent go to the host.
    // The tree is built (paragraphs whose reading didn't change come from the tree cache) and compared with the last
    // one, at most every A11Y_MS.
    pub(super) fn a11y_fired(s: &mut Session) {
        s.presentation.sink.a11y_armed = false;
        if s.presentation.sink.deliver.is_none() {
            return;
        }
        let revision = a11y_key(&s.editor);
        let tree = s.accessible_tree();
        let update = s.presentation.sink.a11y.as_ref().map_or_else(
            || Some(crate::api::services::A11yUpdate::Full(Box::new(tree.clone()))),
            |(sent, _)| crate::services::a11y::diff(sent, &tree),
        );
        s.presentation.sink.a11y = Some((tree, revision));
        if let (Some(update), Some(deliver)) = (update, s.presentation.sink.deliver.as_mut()) {
            deliver(HostRequest::A11y { update });
        }
    }

    fn px_per_pt(&self) -> f64 {
        self.zoom * 96.0 / 72.0
    }

    /// The zoom, as a factor (1 is 100%).
    pub(super) fn zoom(&self) -> f64 {
        self.zoom
    }

    /// How the canvas draws the page (light, dark).
    pub(super) fn appearance(&self) -> crate::render::appearance::Appearance {
        self.renderer.appearance
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
        let bg = self.renderer.background.unwrap_or(self.renderer.theme.tokens.desk);
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
    {
        use crate::api::command::CommandId as C;
        use crate::api::state::{CommandState, CommandValue};
        let stops = !state.ruler.horizontal.tabs.is_empty();
        state.commands.insert(C::SetRulerTabType, CommandState { enabled: true, checked: None, value: Some(CommandValue::TabType(p.tab_selector)) });
        state.commands.insert(C::ShowTabStop, CommandState { enabled: stops, checked: None, value: None });
        state.commands.insert(C::ClosePopover, CommandState { enabled: true, checked: None, value: None });
        let corrected = s.editor.correction().is_some();
        for id in [C::RevertAutoCorrect, C::NeverAutoCorrect, C::ShowAutoCorrectOptions] {
            state.commands.insert(id, CommandState { enabled: corrected, checked: None, value: None });
        }
        state.commands.insert(C::SetAutoCorrect, CommandState { enabled: true, checked: None, value: None });
    }
    let a = &s.autocorrect;
    state.auto_correct = crate::api::services::AutoCorrectOptions {
        two_initial_capitals: a.two_initial_capitals,
        sentence_capitals: a.sentence_capitals,
        cell_capitals: a.cell_capitals,
        day_names: a.day_names,
        replace_text: a.replace_text,
        smart_quotes: a.smart_quotes,
        ordinals: a.ordinals,
        fractions: a.fractions,
        dashes: a.dashes,
        numbered_lists: a.numbered_lists,
        bulleted_lists: a.bulleted_lists,
        never: a.never.clone(),
    };
    state.view.formatting_marks = s.formatting_marks;
    state.view.text_boundaries = s.text_boundaries;
    state.view.field_shading = s.field_shading;
    state.view.cursor_overrides.clone_from(&s.cursor_overrides);
    state.view.live_layout = s.editor.live_layout;
    let (t, rgb) = (&p.renderer.theme, |c: skia_safe::Color| Color((u32::from(c.r()) << 16) | (u32::from(c.g()) << 8) | u32::from(c.b())));
    state.view.background = rgb(p.renderer.background.unwrap_or(t.tokens.desk));
    state.view.theme = p.chrome;
    state.view.theme_colors = p.theme_colors;
    state.view.accent = rgb(t.accent.solid);
    state.view.accent_on = rgb(t.accent.on);
    state.view.appearance = p.appearance;
    state.view.detail = p.detail;
    state.view.object_grid = p.object_grid;
    state.view.sub_template_rules = s.editor.sub_template_rules();
    state.editing = match s.editor.editing_sub_template().and_then(|k| s.editor.document().glossary.as_ref()?.parts.get(k as usize)) {
        Some(b) => crate::api::state::EditingTarget::SubTemplate { name: b.name.clone(), category: b.category.clone() },
        None => crate::api::state::EditingTarget::Template,
    };
}

impl Session {
    /// Hands the editor the view options its drag feedback follows: the detail level, the object grid and the unit.
    pub(super) fn sync_view_settings(&mut self) {
        let p = &self.presentation;
        self.editor.view_settings = crate::editing::view::ViewSettings { detail: p.detail, object_grid: p.object_grid, unit: p.unit(&self.language) };
    }

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

    /// The options of the correction just made, at its options button (or its text), viewport CSS px.
    pub(super) fn auto_correct_popover(&mut self) -> Option<crate::api::services::Popover> {
        let from = self.editor.correction()?.from.clone();
        let to = self.editor.correction_text()?;
        let pages = 0..self.editor.stack().pages.len();
        let view = self.editor.correction_view(pages)?;
        let (page, r) = view.button.or_else(|| view.rects.last().copied())?;
        let stack = self.editor.stack();
        let [px, py, ..] = *stack.pages.get(page)?;
        let p = &self.presentation;
        let k = p.px_per_pt();
        let anchor = crate::api::services::ClientRect {
            left: (px + r[0]) * k - p.scroll.0,
            top: (py + r[1]) * k - p.scroll.1,
            width: (r[2] - r[0]) * k,
            height: (r[3] - r[1]) * k,
        };
        Some(crate::api::services::Popover::AutoCorrect { anchor, from, to })
    }

    /// Whether document point (`x`, `y`) is on the correction mark's options button.
    pub(super) fn on_correction_button(&mut self, x: f64, y: f64) -> bool {
        let pages = 0..self.editor.stack().pages.len();
        let Some((page, [x0, y0, x1, y1])) = self.editor.correction_view(pages).and_then(|v| v.button) else { return false };
        let stack = self.editor.stack();
        let Some(&[px, py, ..]) = stack.pages.get(page) else { return false };
        (px + x0..=px + x1).contains(&x) && (py + y0..=py + y1).contains(&y)
    }

    /// Asks the application for `popover`, closing the one open.
    /// The events follow the state they go with.
    pub(super) fn show_popover(&mut self, popover: crate::api::services::Popover) {
        let mut events = Vec::new();
        if self.presentation.popover.take().is_some() {
            events.push(Event::PopoverClosed);
        }
        self.presentation.popover = Some(popover.clone());
        self.presentation.want_frame();
        events.push(Event::PopoverRequested(popover));
        self.changed(events);
    }

    /// Dismisses the open popover, if there is one, with `popoverClosed` (after the state).
    pub(super) fn close_popover(&mut self) -> Option<crate::api::services::Popover> {
        let open = self.presentation.popover.take()?;
        self.presentation.want_frame();
        self.changed(vec![Event::PopoverClosed]);
        Some(open)
    }

    /// Closes the tab stop popover when its stop is gone.
    pub(super) fn popover_follow(&mut self) {
        use crate::api::services::Popover;
        if let Some(Popover::TabStop { position, .. }) = &self.presentation.popover
            && !self.state.ruler.horizontal.tabs.iter().any(|t| (t.position.0 - position.0).abs() < 0.5)
        {
            self.close_popover();
        }
    }

    /// The tab type picker by the box in the rulers' corner.
    pub(super) fn tab_types_popover(&self) -> crate::api::services::Popover {
        use crate::api::catalog::ruler::{TAB_TYPES, tab_type};
        let choice = |value| {
            let (name, description) = tab_type(value);
            crate::api::services::TabTypeChoice { value, name: name.into(), description: description.into() }
        };
        let choices = TAB_TYPES.into_iter().map(choice).collect();
        crate::api::services::Popover::TabType { anchor: crate::services::ruler::SELECTOR, current: self.presentation.tab_selector, choices }
    }

    /// The options of the caret's paragraph's stop at `position` (within half a point), by its mark on the ruler.
    pub(super) fn tab_stop_popover(&self, position: f64) -> Option<crate::api::services::Popover> {
        let view = self.ruler_view();
        let t = view.model.horizontal.tabs.iter().find(|t| (t.position.0 - position).abs() < 0.5)?;
        let anchor = crate::services::ruler::tab_anchor(&view, t.position.0);
        Some(crate::api::services::Popover::TabStop { anchor, position: t.position, alignment: t.alignment, leader: t.leader })
    }

    /// The position of the caret's paragraph's stop nearest viewport x `x`.
    pub(super) fn nearest_tab_stop(&self, x: f64) -> Option<f64> {
        let view = self.ruler_view();
        let centre = |p: f64| crate::services::ruler::tab_anchor(&view, p).left + 7.0;
        let tabs = view.model.horizontal.tabs.iter().map(|t| t.position.0);
        tabs.min_by(|a, b| (centre(*a) - x).abs().total_cmp(&(centre(*b) - x).abs()))
    }

    /// What the rulers measure now: the drag under way's dimension line or chip, else the hovered marker's hint (none
    /// while a popover is open).
    pub(super) fn ruler_measures(&self) -> Vec<crate::editing::view::RulerMeasure> {
        use crate::services::ruler::{drag_measures, hint, hit};
        let p = &self.presentation;
        if let Some(d) = &p.ruler_drag {
            return match d.marker.filter(|_| d.moved) {
                Some(m) => drag_measures(&d.view, m, d.value(), d.removing, p.detail, d.free),
                None => Vec::new(),
            };
        }
        let Some(h) = p.ruler_hover.filter(|_| p.popover.is_none()) else { return Vec::new() };
        let view = self.ruler_view();
        let Some(on) = hit(&view, p.rulers, h.at.x, h.at.y) else { return Vec::new() };
        hint(&view, on, p.tab_selector, h.free, p.width).into_iter().collect()
    }

    /// Where a click on the empty ruler under the pointer adds a tab stop of the box's type (page points), drawn as
    /// its ghost.
    fn ruler_ghost(&self, view: &RulerView) -> Option<f64> {
        use crate::services::ruler::{Hit, ghost, hit};
        let p = &self.presentation;
        let h = p.ruler_hover.filter(|_| p.popover.is_none() && p.ruler_drag.is_none())?;
        if matches!(p.tab_selector, TabSelector::FirstLineIndent | TabSelector::HangingIndent) {
            return None;
        }
        match hit(view, p.rulers, h.at.x, h.at.y)? {
            Hit::Text(pt) => ghost(view, pt, h.free),
            Hit::Selector | Hit::Marker(_) | Hit::Ruler => None,
        }
    }

    /// The ruler marker under the pointer, tinted while hovered.
    fn ruler_hovered(&self, view: &RulerView) -> Option<Marker> {
        use crate::services::ruler::{Hit, hit};
        let p = &self.presentation;
        let h = p.ruler_hover.filter(|_| p.ruler_drag.is_none())?;
        match hit(view, p.rulers, h.at.x, h.at.y)? {
            Hit::Marker(m) => Some(m),
            Hit::Selector | Hit::Text(_) | Hit::Ruler => None,
        }
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
                Request::Autoscroll { .. } => {}
            }
        }
        self.element_feedback();
        let view = self.editor.view(0..0);
        let hovered = self.editor.hover_tooltip();
        let cursor = self.cursor_override().unwrap_or(view.cursor);
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
        // The window keeps its last text while the selection holds no text position (an object).
        if let Some(w) = crate::services::a11y::window(&mut self.editor, &self.services.spelling, &self.language, self.read_only) {
            let sink = &mut self.presentation.sink;
            if sink.window.as_ref().map(|w| &w.window) != Some(&w.window) {
                out.push(HostRequest::CaretText { window: w.window.clone() });
                sink.window = Some(w);
            }
        }
        let s = &mut self.presentation.sink;
        let tooltip = hovered.and_then(|(kind, t)| {
            let (x, y) = stack.to_doc(t.page, t.rect[0], t.rect[1])?;
            let (left, top) = (x * k - scroll.0, y * k - scroll.1);
            let (width, height) = ((t.rect[2] - t.rect[0]) * k, (t.rect[3] - t.rect[1]) * k);
            let rect = ClientRect { left, top, width, height };
            Some(Tooltip { kind, text: t.text, at: ClientPoint { client_x: left, client_y: top + height }, rect })
        });
        let tooltip_changed = s.tooltip != tooltip;
        s.tooltip.clone_from(&tooltip);
        let revision = a11y_key(&self.editor);
        match &self.presentation.sink.a11y {
            None => {
                let tree = self.accessible_tree();
                out.push(HostRequest::A11y { update: crate::api::services::A11yUpdate::Full(Box::new(tree.clone())) });
                self.presentation.sink.a11y = Some((tree, revision));
            }
            // Throttled: the first change arms the timer and later ones don't re-arm it, so typing still gets an
            // update every A11Y_MS.
            Some((_, sent)) if *sent != revision && !self.presentation.sink.a11y_armed => {
                self.presentation.sink.a11y_armed = true;
                self.presentation.timers.push((A11Y_TIMER, A11Y_MS));
            }
            Some(_) => {}
        }
        if tooltip_changed {
            self.emit(Event::Tooltip(tooltip));
        }
        if let Some(deliver) = self.presentation.sink.deliver.as_mut() {
            out.into_iter().for_each(deliver);
        }
    }
}

impl Session {
    /// The pages on screen.
    fn pages_on_screen(&self) -> std::ops::Range<usize> {
        let p = &self.presentation;
        let k = p.px_per_pt();
        let stack = self.editor.stack();
        let on = |&[_, y, _, h]: &[f64; 4]| y * k - p.scroll.1 < p.height && (y + h) * k - p.scroll.1 > 0.0;
        let first = stack.pages.iter().position(on).unwrap_or(0);
        let end = stack.pages.iter().rposition(on).map_or(first, |i| i + 1);
        first..end
    }

    /// An element's boxes on screen, in client px.
    pub(super) fn element_boxes_on_screen(&mut self, e: crate::docx::ElementId) -> Vec<ClientRect> {
        let pages = self.pages_on_screen();
        let (k, scroll) = (self.presentation.px_per_pt(), self.presentation.scroll);
        let stack = self.editor.stack();
        self.editor
            .element_rects(e, pages)
            .into_iter()
            .filter_map(|(page, [x0, y0, x1, y1])| {
                let (x, y) = stack.to_doc(page, x0, y0)?;
                Some(ClientRect { left: x * k - scroll.0, top: y * k - scroll.1, width: (x1 - x0) * k, height: (y1 - y0) * k })
            })
            .collect()
    }

    /// The hovered and the selected element: sent when either, or its boxes, changed.
    fn element_feedback(&mut self) {
        use crate::api::services::{ElementHover, ElementSelection};
        let under = {
            let fonts = self.fonts.fonts();
            self.editor.hover_point().and_then(|(x, y)| self.editor.element_under(x, y, self.pointer_mods, &fonts))
        };
        let hover = match under {
            Some((e, action)) => {
                Some(ElementHover { element: self.editor.element_context_of(e, false), boxes: self.element_boxes_on_screen(e), action })
            }
            None => None,
        };
        if self.presentation.sink.hover != hover {
            self.presentation.sink.hover.clone_from(&hover);
            self.emit(Event::ElementHover(hover));
        }
        let selected = match self.editor.selected_element() {
            Some((e, exact)) => Some(ElementSelection { element: self.editor.element_context_of(e, exact), boxes: self.element_boxes_on_screen(e) }),
            None => None,
        };
        if self.presentation.sink.selected != selected {
            self.presentation.sink.selected.clone_from(&selected);
            self.emit(Event::ElementSelect(selected));
        }
    }

    /// The application's cursor for the hovered element, if one of its overrides matches it.
    fn cursor_override(&self) -> Option<Cursor> {
        use crate::api::services::CursorTarget;
        let h = self.presentation.sink.hover.as_ref()?;
        let matches = |t: &CursorTarget| match t {
            CursorTarget::Kind(k) => *k == h.element.kind,
            CursorTarget::Action(a) => *a == h.action,
        };
        self.cursor_overrides.iter().find(|o| matches(&o.on)).map(|o| o.cursor)
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
#[cfg(twip_window)]
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
        crate::prof::time("frame", || self.draw_rgba(pixels, width, height))
    }

    fn draw_rgba(&mut self, pixels: &mut [u8], width: u32, height: u32) -> Result<FrameInfo, Error> {
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

    /// Draws this session's frames into `window` with the GPU from now on (an `HWND` on Windows, a `CAMetalLayer` on
    /// macOS): the backend (1 D3D12, 2 D3D11, 3 Vulkan, 4 Metal), or `None` when there is no usable GPU or this build
    /// has none (draw with [`HostSurface::frame_rgba`]).
    /// # Safety
    /// `window` is a live window or layer that outlives the attach ([`HostSurface::detach_window`], or the session's
    /// end).
    pub unsafe fn attach_window(&mut self, window: *mut std::ffi::c_void) -> Option<u32> {
        // SAFETY: the caller's promise.
        #[cfg(twip_window)]
        return unsafe { crate::render::window::attach(self.session.presentation.serial, window) }.map(|b| b as u32);
        #[cfg(not(twip_window))]
        {
            let _ = window;
            None
        }
    }

    /// Stops drawing into the window [`HostSurface::attach_window`] gave.
    pub fn detach_window(&mut self) {
        #[cfg(twip_window)]
        crate::render::window::detach(self.session.presentation.serial);
    }

    /// Testing: loses the GPU device as a driver reset would. Window frames fail `unavailable` until the next
    /// [`HostSurface::attach_window`], which makes a new device. Nothing without a GPU build.
    #[doc(hidden)]
    pub fn lose_gpu_device(&mut self) {
        #[cfg(twip_window)]
        crate::render::window::lose_device();
    }

    /// Draws the viewport into the attached window, `width` x `height` device pixels, and presents it. Fails
    /// `unavailable` when no window is attached or the GPU can't draw now (the device was lost): draw with
    /// [`HostSurface::frame_rgba`] then.
    pub fn frame_window(&mut self, width: u32, height: u32) -> Result<FrameInfo, Error> {
        #[cfg(twip_window)]
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
        self.session.sync_view_settings();
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
        let field_shading = s.editor.field_shading(visible.clone(), s.field_shading);
        let mut view = s.editor.view(visible);
        view.spelling = marks;
        view.formatting_marks = formatting_marks;
        view.boundaries = boundaries;
        view.field_shading = field_shading;
        p.renderer.gridlines = s.gridlines;
        p.renderer.page_colour = s.editor.document().page_colour;
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
            line.set_color(p.renderer.palette().leaders);
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
            let measures = s.ruler_measures();
            let frame = crate::render::ruler::RulerFrame {
                bands: p.rulers,
                width: width / f.dpr,
                height: height / f.dpr,
                dpr: f.dpr,
                selector: p.tab_selector,
                guide: p.ruler_drag.as_ref().and_then(RulerDrag::guide),
                drag: p.ruler_drag.as_ref().and_then(RulerDrag::shown),
                ghost: s.ruler_ghost(&ruler),
                hover: s.ruler_hovered(&ruler),
                popover: p.popover.as_ref(),
                measures: &measures,
                page: p.renderer.page_theme(),
            };
            crate::render::ruler::paint(&ruler, &p.renderer.theme, canvas, &frame);
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

    /// The screen reader moved the caret in the focused element (as it does reading continuously): the selection goes
    /// to those places of the last caret window, and the next window follows it. `invalid-argument` when a place is
    /// outside that window.
    pub fn caret_window_select(
        &mut self,
        anchor: crate::api::services::WindowPoint,
        focus: crate::api::services::WindowPoint,
    ) -> Result<(), crate::api::error::Error> {
        let s = &mut *self.session;
        let w = s.presentation.sink.window.as_ref().ok_or_else(|| crate::api::error::Error::invalid("no caret window was sent"))?;
        let (Some(a), Some(f)) = (w.place(anchor), w.place(focus)) else {
            return Err(crate::api::error::Error::invalid("the place is outside the caret window"));
        };
        s.apply_with(crate::api::record::Origin::User, |e, fonts| e.select_between(a, f, fonts)).map(|_| ())
    }

    /// A timer the engine asked for (`set_timer`) went off: the C ABI arms one while background layout is pending,
    /// and each runs one slice of it ([`HostSurface::idle`]).
    pub fn timer_fired(&mut self, timer: u32) {
        if timer == A11Y_TIMER {
            Presentation::a11y_fired(self.session);
            return;
        }
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
        let r = &mut self.session.presentation.renderer;
        r.overlay_colours = Some(crate::render::Palette {
            selection: c(palette.selection),
            object: c(palette.object),
            drop: c(palette.drop),
            marks: c(palette.marks),
            leaders: c(palette.leaders),
        });
        self.session.presentation.frame_wanted = true;
    }

    /// The overlay colours back to the chrome theme's: they follow it again.
    pub fn reset_overlay_palette(&mut self) {
        let r = &mut self.session.presentation.renderer;
        r.overlay_colours = None;
        self.session.presentation.frame_wanted = true;
    }

    /// The background around the pages. Until the host sets the chrome's theme, a dark background gives dark chrome.
    pub fn set_background(&mut self, color: Color) {
        self.background(Some(crate::render::rgb(color.0)));
    }

    /// The background back to the chrome theme's desk (and, until the host sets the theme, the light chrome).
    pub fn reset_background(&mut self) {
        self.background(None);
    }

    fn background(&mut self, bg: Option<skia_safe::Color>) {
        use crate::api::services::ChromeTheme;
        let p = &mut self.session.presentation;
        p.renderer.background = bg;
        if !p.chrome_set {
            let dark = bg.is_some_and(|bg| crate::render::theme::luminance(bg) < 0.2);
            p.chrome = if dark { ChromeTheme::Dark } else { ChromeTheme::Light };
            p.apply_look();
        }
        p.frame_wanted = true;
        self.session.changed(Vec::new());
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
        self.session.presentation.ruler_hover = None;
        self.session.changed(Vec::new());
    }

    /// The unit the rulers show (by default the language's: inches for en-US, centimetres otherwise).
    pub fn set_ruler_unit(&mut self, unit: RulerUnit) {
        self.session.presentation.ruler_unit = Some(unit);
        self.session.changed(Vec::new());
    }

    /// What a click on the ruler adds; the tab type picker the box in the rulers' corner opens sets it too.
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
        // The corner box asks for the tab type picker rather than cycling; a double-click asks for the Tabs dialog.
        press(s, K::Down, 5.0, 5.0, 1);
        assert_eq!(s.state().view.tab_selector, TabSelector::Left);
        press(s, K::Down, x(300.0), 10.0, 2);
        let events = events.borrow();
        assert!(events.iter().any(|e| matches!(e, Event::DialogRequested { kind: DialogKind::Tabs })));
        assert_eq!(events.iter().filter(|e| matches!(e, Event::DocumentChanged(_))).count(), 4, "typing and three tab commands");
    }

    mod ruler {
        use super::*;
        use crate::api::command::AddTabStop;
        use crate::api::event::{Event, KeyOutcome};
        use crate::api::input::{Key, Modifiers, PointerButton as B, PointerKind as K};
        use crate::api::services::{ClientRect, MeasureDetail, Popover, TabSelector};
        use crate::api::session::Session;
        use crate::api::session::host::RulerDrag;
        use crate::api::value::{Points, TabAlignment, TabLeader};
        use crate::editing::view::RulerMeasure;
        use crate::render::ruler::Dragged;
        use crate::services::ruler::Marker;

        type Events = std::rc::Rc<std::cell::RefCell<Vec<Event>>>;

        /// A session that typed "Hello", in an 800 by 600 view with both rulers, and the events it raises; with no fonts,
        /// or `fonts`.
        fn ruled() -> (Engine, crate::api::engine::SessionId, Events) {
            ruled_with(FontLibrary::empty())
        }

        fn ruled_with(fonts: FontLibrary) -> (Engine, crate::api::engine::SessionId, Events) {
            let mut engine = Engine::new(fonts);
            let events = Events::default();
            let sink = events.clone();
            engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
            let id = engine.create_session(SessionOptions::default());
            let s = engine.session(id).expect("a session");
            s.run(Command::InsertText("Hello".into())).expect("types");
            s.host().set_size(800.0, 600.0, 1.0);
            s.host().set_rulers(true, true);
            events.borrow_mut().clear();
            (engine, id, events)
        }

        /// The client x of a point `pt` from the text start: the page's left edge is 18 pt in, its text an inch
        /// further, and a CSS px is 3/4 pt.
        fn x(pt: f64) -> f64 {
            24.0 + (72.0 + pt) / 0.75
        }

        fn at(s: &mut Session, kind: K, x: f64, y: f64, alt: bool) {
            let clicks = if kind == K::Down { 1 } else { 0 };
            s.pointer(kind, Point { x, y }, B::Primary, clicks, Modifiers { alt, ..Modifiers::default() });
        }

        fn click(s: &mut Session, x: f64, y: f64) {
            at(s, K::Down, x, y, false);
            at(s, K::Up, x, y, false);
        }

        fn labels(s: &Session) -> Vec<String> {
            s.ruler_measures()
                .into_iter()
                .map(|m| match m {
                    RulerMeasure::Dimension { label, .. }
                    | RulerMeasure::Chip { label, .. }
                    | RulerMeasure::Hint { label, .. }
                    | RulerMeasure::Status { label, .. } => label,
                })
                .collect()
        }

        fn tabs(s: &Session) -> Vec<(f64, TabAlignment, TabLeader)> {
            s.state().ruler.horizontal.tabs.iter().map(|t| (t.position.0, t.alignment, t.leader)).collect()
        }

        fn popovers(events: &Events) -> Vec<Option<Popover>> {
            let events = events.borrow();
            let shown = events.iter().filter_map(|e| match e {
                Event::PopoverRequested(p) => Some(Some(p.clone())),
                Event::PopoverClosed => Some(None),
                _ => None,
            });
            shown.collect()
        }

        #[test]
        fn a_first_line_drag_follows_the_pointer_snapped_to_the_sixteenth_inch_unless_alt_is_down() {
            let (mut engine, id, _) = ruled();
            let s = engine.session(id).expect("a session");
            // 0.27 in is 19.44 pt: it snaps to 0.25 in.
            at(s, K::Down, x(0.0), 5.0, false);
            at(s, K::Move, x(19.44), 5.0, false);
            let shown = s.presentation.ruler_drag.as_ref().and_then(RulerDrag::shown);
            assert_eq!(shown, Some(Dragged { marker: Marker::FirstLine, at: 72.0 + 18.0, removing: false }), "the marker follows");
            assert!(s.presentation.ruler_drag.as_ref().and_then(RulerDrag::guide).is_some_and(|(h, gx)| h && (gx - x(18.0)).abs() < 1e-9));
            assert_eq!(labels(s), ["First line 0.25″"]);
            assert_eq!(s.state().ruler.horizontal.indents.first_line, Points(0.0), "nothing commits during the drag");
            // Alt pressed mid-drag runs it again where the pointer is: no snapping, and the change shows.
            s.modifiers(Modifiers { alt: true, ..Modifiers::default() });
            assert_eq!(labels(s), ["First line 0.27″  Δ +0.27″"]);
            s.modifiers(Modifiers::default());
            assert_eq!(labels(s), ["First line 0.25″"]);
            at(s, K::Up, x(19.44), 5.0, false);
            assert_eq!(s.state().ruler.horizontal.indents.first_line, Points(18.0));
            // With Alt it lands where the pointer is.
            at(s, K::Down, x(18.0), 5.0, true);
            at(s, K::Move, x(37.44), 5.0, true);
            at(s, K::Up, x(37.44), 5.0, true);
            // Indents are kept in twentieths of a point; snapped, it would be 36 pt.
            assert!((s.state().ruler.horizontal.indents.first_line.0 - 37.44).abs() < 0.05, "{:?}", s.state().ruler.horizontal.indents);
        }

        #[test]
        fn the_detail_level_picks_a_chip_a_dimension_line_or_the_change_too() {
            let (mut engine, id, _) = ruled();
            let s = engine.session(id).expect("a session");
            let detail = |s: &mut Session, d| {
                s.run(Command::SetViewOptions(crate::api::command::ViewOptions { detail: Some(d), ..Default::default() })).expect("sets");
            };
            at(s, K::Down, x(0.0), 5.0, false);
            at(s, K::Move, x(36.0), 5.0, false);
            assert!(matches!(&s.ruler_measures()[..], [RulerMeasure::Dimension { .. }]));
            assert_eq!(labels(s), ["First line 0.50″"]);
            detail(s, MeasureDetail::Full);
            assert_eq!(labels(s), ["First line 0.50″  Δ +0.50″"]);
            detail(s, MeasureDetail::Quiet);
            assert!(matches!(&s.ruler_measures()[..], [RulerMeasure::Chip { .. }]));
            assert_eq!(labels(s), ["First line 0.50″"]);
        }

        #[test]
        fn hovering_the_ruler_hints_and_shows_the_ghost_stop_a_click_adds() {
            let (mut engine, id, _) = ruled();
            let s = engine.session(id).expect("a session");
            at(s, K::Move, x(0.0), 5.0, false);
            assert_eq!(
                s.ruler_measures(),
                [RulerMeasure::Hint { at: x(0.0) - 6.0, label: "First line · 0.00″ — indents only the first line".into() }]
            );
            at(s, K::Move, 5.0, 5.0, false);
            assert_eq!(labels(s), ["Tab type: Left — click to change"]);
            at(s, K::Move, x(468.0), 5.0, false);
            assert_eq!(labels(s), ["Right margin · 1.00″ — drag to change"]);
            assert_eq!(s.ruler_hovered(&s.ruler_view()), Some(Marker::TextEnd), "the margin's handle tints");
            // Over the empty ruler, the ghost of the stop sits where the click adds it.
            at(s, K::Move, x(146.0), 10.0, false);
            assert_eq!(s.ruler_ghost(&s.ruler_view()), Some(72.0 + 144.0));
            assert_eq!(labels(s), ["Click to add a left tab · 2.00″"]);
            click(s, x(146.0), 10.0);
            assert_eq!(tabs(s), [(144.0, TabAlignment::Left, TabLeader::None)]);
            // Off the ruler, nothing shows.
            at(s, K::Move, 300.0, 300.0, false);
            assert!(s.ruler_measures().is_empty() && s.ruler_ghost(&s.ruler_view()).is_none());
        }

        #[test]
        fn a_stop_is_removed_only_more_than_30_px_below_the_band() {
            let (mut engine, id, _) = ruled();
            let s = engine.session(id).expect("a session");
            click(s, x(144.0), 10.0);
            // 29 px below the band, it moves.
            at(s, K::Down, x(144.0), 15.0, false);
            at(s, K::Move, x(162.0), 18.0 + 29.0, false);
            assert_eq!(labels(s), ["Left tab 2.25″"]);
            at(s, K::Up, x(162.0), 18.0 + 29.0, false);
            assert_eq!(tabs(s).iter().map(|t| t.0).collect::<Vec<_>>(), [162.0]);
            // Further down it fades and says so; dragged back up in the same drag, it stays.
            at(s, K::Down, x(162.0), 15.0, false);
            at(s, K::Move, x(162.0), 18.0 + 31.0, false);
            assert_eq!(labels(s), ["Release to remove tab stop"]);
            assert!(s.presentation.ruler_drag.as_ref().is_some_and(|d| d.removing && d.guide().is_none()));
            at(s, K::Move, x(162.0), 15.0, false);
            at(s, K::Up, x(162.0), 15.0, false);
            assert_eq!(tabs(s).len(), 1);
            // Released down there, it goes.
            at(s, K::Down, x(162.0), 15.0, false);
            at(s, K::Move, x(162.0), 18.0 + 31.0, false);
            at(s, K::Up, x(162.0), 18.0 + 31.0, false);
            assert!(tabs(s).is_empty());
            assert_eq!(s.state().history.undo_label.as_deref(), Some("Delete Tab Stop"));
        }

        #[test]
        fn the_tab_type_box_asks_for_the_picker_and_its_answer_sets_the_type() {
            let (mut engine, id, events) = ruled();
            let s = engine.session(id).expect("a session");
            click(s, 5.0, 5.0);
            let shown = popovers(&events);
            let Some(Some(Popover::TabType { anchor, current, choices })) = shown.first() else { panic!("{shown:?}") };
            assert_eq!((*anchor, *current), (ClientRect { left: 2.0, top: 2.0, width: 14.0, height: 14.0 }, TabSelector::Left));
            let named: Vec<(TabSelector, &str, &str)> = choices.iter().map(|c| (c.value, c.name.as_str(), c.description.as_str())).collect();
            assert_eq!(
                named,
                [
                    (TabSelector::Left, "Left", "Text starts at the stop"),
                    (TabSelector::Center, "Center", "Text centers on the stop"),
                    (TabSelector::Right, "Right", "Text ends at the stop"),
                    (TabSelector::Decimal, "Decimal", "Numbers line up on the decimal point"),
                    (TabSelector::Bar, "Bar", "A vertical line at the stop"),
                    (TabSelector::FirstLineIndent, "First line indent", "A click sets where the first line starts"),
                    (TabSelector::HangingIndent, "Hanging indent", "A click sets where wrapped lines start"),
                ]
            );
            // A second press on the box closes it.
            click(s, 5.0, 5.0);
            assert_eq!(popovers(&events).len(), 2);
            assert_eq!(popovers(&events)[1], None);
            // The picker's answer, and the command's state.
            s.run(Command::SetRulerTabType(TabSelector::Decimal)).expect("sets");
            assert_eq!(s.state().view.tab_selector, TabSelector::Decimal);
            let value = s.command_state(crate::api::command::CommandId::SetRulerTabType).and_then(|c| c.value.clone());
            assert_eq!(value, Some(crate::api::state::CommandValue::TabType(TabSelector::Decimal)));
            click(s, x(146.0), 10.0);
            assert_eq!(tabs(s), [(144.0, TabAlignment::Decimal, TabLeader::None)]);
            // From the keyboard; Esc dismisses it.
            events.borrow_mut().clear();
            s.run(Command::ShowTabTypes).expect("shows");
            assert!(matches!(&popovers(&events)[..], [Some(Popover::TabType { current: TabSelector::Decimal, .. })]));
            assert_eq!(s.key(Key::Escape, Modifiers::default()), KeyOutcome::Handled);
            assert_eq!(popovers(&events).last(), Some(&None));
            // The picker's answer closes it, and says so.
            s.run(Command::ShowTabTypes).expect("shows");
            events.borrow_mut().clear();
            s.run(Command::SetRulerTabType(TabSelector::Left)).expect("sets");
            assert_eq!(popovers(&events), [None]);
            // The application closes it itself: once; with none open, nothing happens.
            s.run(Command::ShowTabTypes).expect("shows");
            events.borrow_mut().clear();
            s.run(Command::ClosePopover).expect("closes");
            s.run(Command::ClosePopover).expect("closes nothing");
            assert_eq!(popovers(&events), [None]);
        }

        #[test]
        fn the_overlay_colours_and_background_go_back_to_the_themes() {
            use crate::api::command::ViewOptions;
            use crate::api::services::ChromeTheme;
            use crate::api::session::OverlayPalette;
            use crate::api::value::Color;
            let (mut engine, id, _) = ruled();
            let s = engine.session(id).expect("a session");
            let desk = s.state().view.background;
            let theme = s.presentation.renderer.palette();
            s.host().set_background(Color(0x1E1E1E));
            assert_eq!(s.state().view.background, Color(0x1E1E1E));
            assert_eq!(s.state().view.theme, ChromeTheme::Dark, "a dark background darkens the chrome");
            s.host().reset_background();
            assert_eq!((s.state().view.background, s.state().view.theme), (desk, ChromeTheme::Light));
            s.host().set_overlay_palette(OverlayPalette { marks: Color(0xFF0000), ..OverlayPalette::default() });
            assert_ne!(s.presentation.renderer.palette(), theme);
            s.host().reset_overlay_palette();
            assert_eq!(s.presentation.renderer.palette(), theme);
            // Reset, they follow the theme again.
            s.run(Command::SetViewOptions(ViewOptions { theme: Some(ChromeTheme::Dark), ..ViewOptions::default() })).expect("sets");
            assert_eq!(s.presentation.renderer.palette(), s.presentation.renderer.page_theme().palette());
            assert_ne!(s.state().view.background, desk);
        }

        #[test]
        fn the_hosts_colours_replace_each_schemes_own() {
            use crate::api::command::ViewOptions;
            use crate::api::services::{ChromeColors, ChromeTheme, SystemColors, ThemeColors};
            use crate::api::value::Color;
            let (mut engine, id, _) = ruled();
            let s = engine.session(id).expect("a session");
            let set = |s: &mut Session, theme: Option<ChromeTheme>, colors: Option<ThemeColors>| {
                s.run(Command::SetViewOptions(ViewOptions { theme, theme_colors: colors, ..ViewOptions::default() })).expect("sets");
            };
            let ink = |s: &Session| s.presentation.renderer.theme.tokens.ink;
            let built_in = ink(s);
            let dark = ChromeColors { desk: Some(Color(0x1B1F27)), text: Some(Color(0xE6EDF3)), ..ChromeColors::default() };
            set(s, None, Some(ThemeColors { light: None, dark: Some(dark) }));
            assert_eq!(ink(s), built_in, "the light scheme keeps its own");
            set(s, Some(ChromeTheme::Dark), None);
            assert_eq!((s.state().view.background, ink(s)), (Color(0x1B1F27), crate::render::rgb(0xE6EDF3)));
            assert_eq!(s.state().view.theme_colors.dark, Some(dark));
            // The caret stays black on the white page.
            assert_eq!(s.presentation.renderer.caret(), skia_safe::Color::BLACK);
            // High contrast takes the platform's.
            let hc = SystemColors {
                canvas: Color(0),
                canvas_text: Color(0xFFFFFF),
                highlight: Color(0x1AEBFF),
                highlight_text: Color(0),
                gray_text: Color(0x3FF23F),
            };
            set(s, Some(ChromeTheme::HighContrast(hc)), None);
            assert_eq!(s.state().view.background, Color(0));
            // `{}` puts the scheme's own back.
            set(s, Some(ChromeTheme::Dark), Some(ThemeColors { light: None, dark: Some(ChromeColors::default()) }));
            assert_ne!(s.state().view.background, Color(0x1B1F27));
        }

        #[test]
        fn a_click_on_a_stop_asks_for_its_options_and_each_answer_is_one_step() {
            let (mut engine, id, events) = ruled();
            let s = engine.session(id).expect("a session");
            assert!(!s.command_state(crate::api::command::CommandId::ShowTabStop).is_some_and(|c| c.enabled), "no stops");
            click(s, x(144.0), 10.0);
            events.borrow_mut().clear();
            click(s, x(144.0), 15.0);
            let request = Popover::TabStop {
                anchor: ClientRect { left: x(144.0) - 7.0, top: 10.0, width: 14.0, height: 14.0 },
                position: Points(144.0),
                alignment: TabAlignment::Left,
                leader: TabLeader::None,
            };
            assert_eq!(popovers(&events), [Some(request.clone())]);
            // Changing the alignment and leader keeps it open; undo puts the stop back.
            let change = AddTabStop { position: Points(144.0), alignment: Some(TabAlignment::Right), leader: Some(TabLeader::Dots) };
            s.run(Command::AddTabStop(change)).expect("changes");
            assert_eq!(tabs(s), [(144.0, TabAlignment::Right, TabLeader::Dots)]);
            assert_eq!(s.state().history.undo_label.as_deref(), Some("Add Tab Stop"));
            assert_eq!(popovers(&events).len(), 1, "still open");
            s.run(Command::Undo).expect("undoes");
            assert_eq!(tabs(s), [(144.0, TabAlignment::Left, TabLeader::None)]);
            // Clearing it is one step, and its popover closes with it.
            s.run(Command::DeleteTabStop(Points(144.0))).expect("clears");
            assert!(tabs(s).is_empty());
            assert_eq!(popovers(&events), [Some(request), None]);
            s.run(Command::Undo).expect("undoes");
            assert_eq!(tabs(s).len(), 1);
            // From the keyboard: the stop nearest the caret.
            click(s, x(400.0), 10.0);
            events.borrow_mut().clear();
            assert!(s.command_state(crate::api::command::CommandId::ShowTabStop).is_some_and(|c| c.enabled));
            s.run(Command::ShowTabStop).expect("shows");
            assert!(matches!(&popovers(&events)[..], [Some(Popover::TabStop { position: Points(144.0), .. })]), "{:?}", popovers(&events));
            // A press in the document dismisses it.
            click(s, 300.0, 300.0);
            assert_eq!(popovers(&events).last(), Some(&None));
        }

        #[test]
        fn in_a_table_the_ruler_measures_the_columns_either_side_of_a_dragged_border() {
            let (mut engine, id, _) = ruled_with(FontLibrary::system_recommended());
            let s = engine.session(id).expect("a session");
            s.run(Command::InsertTable(crate::api::command::TableSize { rows: 1, columns: 3 })).expect("inserts");
            // A frame lays the table out; the next state has its column boundaries.
            let mut pixels = vec![0u8; 800 * 600 * 4];
            s.host().frame_rgba(&mut pixels, 800, 600).expect("draws");
            s.host().set_rulers(true, true);
            let view = s.ruler_view();
            let b: Vec<f64> = view.model.horizontal.table_columns.iter().map(|b| b.0).collect();
            assert_eq!(b.len(), 4, "{b:?}");
            let client = |pt: f64| view.horizontal.origin + pt * view.horizontal.scale;
            at(s, K::Down, client(b[1]), 12.0, false);
            at(s, K::Move, client(b[1] + 36.0), 12.0, true);
            assert!(matches!(&s.ruler_measures()[..], [RulerMeasure::Dimension { .. }, RulerMeasure::Dimension { .. }]));
            // The border was 2.17 in from the left edge: 2.67 in and 1.67 in either side of it now.
            assert_eq!(labels(s), ["2.67", "1.67"], "{b:?}");
            assert!(s.presentation.ruler_drag.as_ref().and_then(RulerDrag::shown).is_some_and(|d| d.marker == Marker::TableColumn(1)));
        }
    }

    #[test]
    fn a_drop_runs_its_command_at_the_hit_position() {
        let mut engine = Engine::new(FontLibrary::system_recommended());
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
