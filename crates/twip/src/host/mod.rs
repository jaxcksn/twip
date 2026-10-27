//! The host API: one document, its editor, renderer and services (fonts, saving, PDF, spelling, find), as the SDKs
//! (`twip-ffi`) drive it: input and commands, queries (content size, caret, status, reading structure), painting,
//! saving and export, all computed from the editor's state and view. Messages to the editor are JSON internally.

use skia_safe::{Canvas, Color};

use crate::editing::Editor;
use crate::editing::position::Place;
use crate::editing::selection::Selection;
use crate::editing::text::UnitText;
use crate::font::Fonts;
use crate::render::{Frame, Renderer};

pub mod a11y;
pub mod command_bar;
pub mod dialogs;
pub mod find;
pub mod spelling;

thread_local! {
    /// The fragment last copied in any engine on this thread, with that engine's serial: a paste in another engine
    /// keeps its formatting, fields and controls.
    static SHARED_CLIP: std::cell::RefCell<Option<(u64, crate::editing::clipboard::Fragment)>> = const { std::cell::RefCell::new(None) };
}

pub struct Engine {
    /// Shared by the engines of one page.
    pub fonts: std::rc::Rc<Fonts>,
    /// Unique on the thread: names this engine's fragments in clipboard tokens (`serial.id`).
    serial: u64,
    editor: Option<Editor>,
    renderer: Renderer,
    zoom: f64,
    /// Sequence numbers for inputs the compatibility calls build (the host's own messages carry theirs).
    seq: u64,
    /// The last key's command ran (the host prevents the browser's default action then).
    pub handled: bool,
    /// The auto-scroll velocity the last pointer input asked for (CSS px per second).
    pub autoscroll: [f64; 2],
    /// Events for the application since it last took them (placeholders activated; host commands a key asked
    /// for), as JSON.
    events: Vec<serde_json::Value>,
    /// The editor's requests (scroll into view, cursor, clipboard, ...) since the host last took them, as JSON.
    requests: Vec<serde_json::Value>,
    /// The spell checker, once the host gave it a dictionary.
    speller: Option<spelling::Speller>,
    /// The view settings the command bar toggles.
    pub view: command_bar::ViewOptions,
    /// The cell context last announced.
    cell_context: a11y::CellContext,
    /// The host runs on macOS (sent to every document opened).
    mac: bool,
    /// Every message applied since the document opened, one JSON per line (test builds; `twip replay`).
    #[cfg(feature = "inspect")]
    log: Vec<String>,
    /// Test builds: the recording renderer in place of `render`: frames build their view and record it
    /// as canonical JSON instead of drawing; the count of frames recorded.
    #[cfg(feature = "inspect")]
    recording: Option<usize>,
    /// The last frames the recording renderer took, as canonical JSON.
    #[cfg(feature = "inspect")]
    frames: std::collections::VecDeque<String>,
    /// Test builds: every reply since the test last took them (scenarios' `expect.answer`, `context`,
    /// `elements`, `events`).
    #[cfg(feature = "inspect")]
    replies: Vec<serde_json::Value>,
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::with_fonts(Fonts::default())
    }
}

pub fn json_str(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Keys whose UI is the host's (dialogs, files, panes): code, ctrl, shift, alt → host command.
const HOST_KEYS: [(&str, bool, bool, bool, &str); 13] = [
    ("KeyF", true, false, false, "Find"),
    ("KeyH", true, false, false, "Replace"),
    ("KeyK", true, false, false, "Hyperlink"),
    ("KeyD", true, false, false, "Font"),
    ("KeyF", true, true, false, "Font"),
    ("KeyP", true, false, false, "Print"),
    ("F12", true, true, false, "Print"),
    ("KeyS", true, false, false, "Save"),
    ("KeyO", true, false, false, "Open"),
    ("KeyN", true, false, false, "New"),
    ("ContextMenu", false, false, false, "ContextMenu"),
    ("F10", false, true, false, "ContextMenu"),
    ("F7", false, false, false, "NextMisspelling"),
];

/// The WebGL state of this editor's canvas goes with it.
#[cfg(target_os = "emscripten")]
impl Drop for Engine {
    fn drop(&mut self) {
        crate::render::web::detach_gl(self.serial);
    }
}

impl Engine {
    pub fn with_fonts(fonts: Fonts) -> Engine {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Engine {
            fonts: std::rc::Rc::new(fonts),
            serial: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            editor: None,
            renderer: Renderer::default(),
            zoom: 1.0,
            seq: 0,
            handled: false,
            autoscroll: [0.0, 0.0],
            events: Vec::new(),
            requests: Vec::new(),
            speller: None,
            view: command_bar::ViewOptions::default(),
            cell_context: a11y::CellContext::default(),
            mac: false,
            #[cfg(feature = "inspect")]
            log: Vec::new(),
            #[cfg(feature = "inspect")]
            recording: None,
            #[cfg(feature = "inspect")]
            frames: std::collections::VecDeque::new(),
            #[cfg(feature = "inspect")]
            replies: Vec::new(),
        }
    }

    /// Test builds: the recording renderer on (frames recorded, nothing drawn) or off (`render` draws again).
    #[cfg(feature = "inspect")]
    pub fn set_recording(&mut self, on: bool) {
        self.recording = on.then_some(0);
    }

    /// Test builds: the last frames the recording renderer took, a JSON array of canonical views.
    #[cfg(feature = "inspect")]
    pub fn recorded_frames(&self) -> String {
        format!("[{}]", self.frames.iter().cloned().collect::<Vec<_>>().join(","))
    }

    /// Test builds: the frames the recording renderer took (0 while it is off).
    #[cfg(feature = "inspect")]
    pub fn recorded(&self) -> usize {
        self.recording.unwrap_or(0)
    }

    pub fn add_font(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() < 12 {
            return false;
        }
        std::rc::Rc::make_mut(&mut self.fonts).add_bytes(bytes.to_vec());
        true
    }

    pub fn editor(&mut self) -> Option<&mut Editor> {
        self.editor.as_mut()
    }

    /// The host's platform: macOS switches the keymap's macOS layer on.
    pub fn set_mac(&mut self, mac: bool) {
        self.mac = mac;
        self.send(serde_json::json!({ "type": "platform", "mac": mac }));
    }

    /// File > New: an empty US Letter document.
    pub fn new_document(&mut self) -> String {
        self.open(&crate::docx::blank::blank_docx())
    }

    pub fn open(&mut self, bytes: &[u8]) -> String {
        let t = std::time::Instant::now();
        match self.try_open(bytes) {
            Ok(()) => self.info_with((t.elapsed().as_secs_f64() * 1000.0) as u64),
            Err(e) => format!("{{\"error\":{}}}", json_str(&e)),
        }
    }

    /// Opens .docx bytes in place of the current document; on an error the current one stays.
    pub fn try_open(&mut self, bytes: &[u8]) -> Result<(), String> {
        let e = Editor::open(bytes, &self.fonts)?;
        self.editor = Some(e);
        self.renderer.clear_images();
        self.seq = 0;
        #[cfg(feature = "inspect")]
        self.log.clear();
        if self.mac {
            self.send(serde_json::json!({ "type": "platform", "mac": true }));
        }
        Ok(())
    }

    /// The laid-out pages' sizes in points (the layout so far; more pages may follow while it runs).
    pub fn page_sizes(&self) -> Vec<(f64, f64)> {
        self.editor.as_ref().map_or_else(Vec::new, |e| e.snapshot().pages().iter().map(|p| (p.w, p.h)).collect())
    }

    pub fn info(&self) -> String {
        self.info_with(0)
    }

    fn info_with(&self, ms: u64) -> String {
        let Some(e) = &self.editor else { return "{\"pages\":[],\"signature\":\"\",\"ms\":0,\"dirty\":false}".into() };
        let snapshot = e.snapshot();
        let pages: Vec<String> = snapshot.pages().iter().map(|p| format!("[{},{}]", p.w, p.h)).collect();
        format!("{{\"pages\":[{}],\"signature\":\"\",\"ms\":{ms},\"dirty\":{}}}", pages.join(","), e.is_dirty())
    }

    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom.clamp(0.1, 5.0);
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn set_background(&mut self, rgb24: u32) {
        self.renderer.background = Some(crate::render::rgb(rgb24));
    }

    /// CSS pixels per point.
    pub fn px_per_pt(&self) -> f64 {
        self.zoom * 96.0 / 72.0
    }

    /// The page stack's size in CSS pixels.
    pub fn content_size(&self) -> [f64; 2] {
        let Some(e) = &self.editor else { return [0.0, 0.0] };
        let s = e.stack();
        [s.width * self.px_per_pt(), s.height * self.px_per_pt()]
    }

    /// Applies host messages (JSON) and returns the reply JSON.
    pub fn input(&mut self, json: &str) -> String {
        let Some(e) = &mut self.editor else { return "{\"error\":\"no document\"}".into() };
        #[cfg(feature = "inspect")]
        self.log.push(json.replace('\n', " "));
        e.apply_json(json, &self.fonts)
    }

    /// Builds and applies one message (the compatibility calls below), returning the reply.
    fn send(&mut self, mut msg: serde_json::Value) -> serde_json::Value {
        let Some(e) = &mut self.editor else { return serde_json::Value::Null };
        self.seq = self.seq.max(e.seq_applied()) + 1;
        msg["seq"] = self.seq.into();
        let msg = msg.to_string();
        #[cfg(feature = "inspect")]
        self.log.push(msg.clone());
        let r = e.apply_json(&msg, &self.fonts);
        let r: serde_json::Value = serde_json::from_str(&r).unwrap_or(serde_json::Value::Null);
        for q in r["requests"].as_array().into_iter().flatten() {
            if q["type"] == "placeholder" {
                self.events.push(q.clone());
            }
            self.requests.push(q.clone());
        }
        // events (a design-mode activation, F9's field update) reach the page however the input came: a
        // double-click's are in the pointer input's reply, which the page doesn't see.
        for ev in r["events"].as_array().into_iter().flatten().filter(|v| v["type"] == "activate" || v["type"] == "field_update") {
            self.events.push(ev.clone());
        }
        #[cfg(feature = "inspect")]
        self.replies.push(r.clone());
        r
    }

    /// The `keymap` query: whether `code` with `mods` is bound here (the editor's
    /// keymap, `context: "table"` for a caret in a cell, or a host key), its command, and whether the browser keeps
    /// it.
    fn keymap_query(&self, msg: &serde_json::Value) -> serde_json::Value {
        let code = msg["code"].as_str().unwrap_or("");
        let m = |k: &str| msg["mods"][k].as_bool().unwrap_or(false);
        let (ctrl, shift, alt) = (m("ctrl"), m("shift"), m("alt"));
        let reserved = ctrl && !alt && matches!((code, shift), ("Tab", false) | ("KeyT", _) | ("KeyN", false) | ("KeyW", false));
        let navigates = alt && !ctrl && matches!(code, "Home" | "ArrowLeft" | "ArrowRight");
        let cx = crate::editing::keymap::KeyContext { in_table: msg["context"] == "table", ..Default::default() };
        let host = HOST_KEYS.iter().find(|k| (k.0, k.1, k.2, k.3) == (code, ctrl, shift, alt)).map(|k| k.4.to_string());
        let command = host.or_else(|| {
            let c = crate::editing::keymap::command(code, crate::editing::keymap::Mods { ctrl, shift, alt, meta: false }, cx)?;
            let d = format!("{c:?}");
            Some(d[..d.find([' ', '(', '{']).unwrap_or(d.len())].to_string())
        });
        let mut v = serde_json::json!({ "bound": command.is_some() || navigates, "reserved": reserved });
        if let Some(c) = command {
            v["command"] = c.into();
        }
        v
    }

    /// One host message (JSON) given its sequence number here, as the compatibility calls are; the reply JSON. The
    /// application's path for commands, queries and batches.
    pub fn send_json(&mut self, json: &str) -> String {
        match serde_json::from_str::<serde_json::Value>(json) {
            Ok(msg @ serde_json::Value::Object(_)) if msg["type"] == "query" && msg["q"] == "keymap" => {
                let answer = self.keymap_query(&msg);
                let mut r = self.send(msg);
                r["answer"] = answer;
                r.to_string()
            }
            Ok(msg @ serde_json::Value::Object(_)) => self.send(msg).to_string(),
            Ok(_) => serde_json::json!({ "error": "one input object expected" }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    /// Test builds: the replies since the last call, as a JSON array.
    #[cfg(feature = "inspect")]
    pub fn take_replies(&mut self) -> String {
        serde_json::Value::Array(std::mem::take(&mut self.replies)).to_string()
    }

    /// The events since the last call, as a JSON array: `{"type": "placeholder", "placeholder": {key, label,
    /// width, height, merge}}`.
    /// The requests since the last call (JSON objects, `Request::to_json`).
    pub fn take_request_values(&mut self) -> Vec<serde_json::Value> {
        std::mem::take(&mut self.requests)
    }

    /// The events since the last call (JSON objects).
    pub fn take_event_values(&mut self) -> Vec<serde_json::Value> {
        std::mem::take(&mut self.events)
    }

    pub fn take_events(&mut self) -> String {
        serde_json::Value::Array(std::mem::take(&mut self.events)).to_string()
    }

    /// The selected unfilled placeholder as JSON (`{key, label, width, height, merge}`), or `null`.
    pub fn placeholder(&self) -> String {
        self.editor.as_ref().and_then(|e| e.selected_placeholder()).map_or("null".to_string(), |p| p.to_json().to_string())
    }

    /// Fill Placeholder: the selected one (`key` empty) or every unfilled one with `key`; how many were filled.
    pub fn fill_placeholder(&mut self, key: &str, data: &[u8]) -> usize {
        let before = self.editor.as_ref().map_or(0, |e| e.document().filled.len());
        let (id, arg) =
            if key.is_empty() { ("placeholder.fill", base64(data)) } else { ("placeholder.fill-key", format!("{key}\n{}", base64(data))) };
        self.command(id, Some(&arg));
        self.editor.as_ref().map_or(0, |e| e.document().filled.len()).saturating_sub(before)
    }

    /// A key press (DOM `KeyboardEvent.code`); true when a command ran for it.
    pub fn key(&mut self, code: &str, ctrl: bool, shift: bool, alt: bool) -> bool {
        if let Some(&(.., name)) = HOST_KEYS.iter().find(|k| (k.0, k.1, k.2, k.3) == (code, ctrl, shift, alt)) {
            let name = if name == "NextMisspelling" { self.spelling_step() } else { name.to_string() };
            self.events.push(serde_json::json!({ "type": "command", "name": name }));
            self.handled = true;
            return true;
        }
        let mods = crate::editing::keymap::Mods { ctrl, shift, alt, meta: false };
        // A bound key is the editor's even when it changes nothing visible (a pending format, Shift+Tab at level 1).
        // Browser navigation keys never leave the document (Alt+Home: the home page; Alt+Left/Right: back, forward),
        // bound or not.
        let navigates = alt && !ctrl && matches!(code, "Home" | "ArrowLeft" | "ArrowRight");
        let bound = navigates || self.editor.as_ref().is_some_and(|e| e.key_command(code, mods).is_some());
        let before = self.editor.as_ref().map(|e| (e.revision(), e.selection()));
        self.send(serde_json::json!({ "type": "key", "code": code, "mods": { "ctrl": ctrl, "shift": shift, "alt": alt } }));
        let after = self.editor.as_ref().map(|e| (e.revision(), e.selection()));
        self.handled = bound || before != after;
        self.handled
    }

    /// A typed command (dialogs): executed directly.
    // shortcut: not in the input log, so `twip replay` doesn't replay dialogs; log them as host ids if needed.
    pub fn run_command(&mut self, cmd: crate::editing::commands::Command) {
        if let Some(e) = &mut self.editor {
            e.execute(cmd, &self.fonts);
        }
    }

    /// A command-bar, menu or dialog command by its host id; ids the editor doesn't know do
    /// nothing. Returns what the page should do next (nothing yet: `{}`).
    pub fn command(&mut self, id: &str, arg: Option<&str>) -> String {
        self.send(serde_json::json!({ "type": "command", "id": id, "arg": arg }));
        "{}".into()
    }

    /// A command whose argument is a JSON value (an object, a number), not a string; as [`command`](Self::command).
    pub fn command_value(&mut self, id: &str, arg: serde_json::Value) {
        self.send(serde_json::json!({ "type": "command", "id": id, "arg": arg }));
    }

    pub fn text(&mut self, text: &str) {
        self.send(serde_json::json!({ "type": "text", "text": text }));
    }

    /// Pointer input at a content point in CSS pixels (converted to document points here).
    pub fn pointer(&mut self, kind: &str, x: f64, y: f64, clicks: i32, mods: crate::editing::keymap::Mods) -> bool {
        let k = self.px_per_pt();
        let mods = serde_json::json!({ "shift": mods.shift, "ctrl": mods.ctrl, "alt": mods.alt });
        let r = self.send(serde_json::json!({ "type": "pointer", "kind": kind, "x": x / k, "y": y / k, "clicks": clicks, "mods": mods }));
        let requests = r["requests"].as_array().cloned().unwrap_or_default();
        if let Some(a) = requests.iter().rev().find(|q| q["type"] == "autoscroll") {
            self.autoscroll = [a["vx"].as_f64().unwrap_or(0.0), a["vy"].as_f64().unwrap_or(0.0)];
        }
        requests.iter().any(|q| q["type"] == "capture_pointer")
    }

    /// The viewport (CSS pixels, content coordinates), for Page Up/Down and auto-scroll.
    pub fn viewport(&mut self, left: f64, top: f64, width: f64, height: f64, dpr: f64) {
        let k = self.px_per_pt();
        self.send(serde_json::json!({ "type": "viewport", "left": left / k, "top": top / k, "width": width / k, "height": height / k, "zoom": self.zoom, "dpr": dpr }));
    }

    /// Copy or cut: `{text, html, token}` for the system clipboard (`token` names the internal fragment).
    pub fn copy(&mut self, cut: bool) -> String {
        // Through the editor's input (so the input log replays it), then the fragment that made, if it made one.
        let before = self.editor.as_ref().and_then(|e| e.clipboard_id());
        self.send(serde_json::json!({ "type": if cut { "cut" } else { "copy" } }));
        let Some(f) = self.editor.as_ref().and_then(|e| e.clipboard()).filter(|f| Some(f.id) != before) else { return "{}".into() };
        let out = serde_json::json!({ "text": f.text(), "html": f.html(), "token": format!("{}.{}", self.serial, f.id) }).to_string();
        SHARED_CLIP.set(Some((self.serial, f.clone())));
        out
    }

    /// Paste: the internal fragment when `token` names it (this engine's, or another's on the page, adopted here),
    /// else `text` as plain text.
    pub fn paste(&mut self, text: &str, token: &str) {
        let token = match token.split_once('.') {
            Some((s, id)) if s.parse() == Ok(self.serial) => id.to_string(),
            Some((s, id)) => SHARED_CLIP
                .with_borrow(|c| c.as_ref().filter(|(o, f)| s.parse() == Ok(*o) && id.parse() == Ok(f.id)).map(|(_, f)| f.clone()))
                .and_then(|f| self.editor.as_mut().map(|e| e.adopt_clipboard(f).to_string()))
                .unwrap_or_default(),
            None => token.to_string(),
        };
        self.send(serde_json::json!({ "type": "paste", "text": text, "token": (!token.is_empty()).then_some(token) }));
    }

    pub fn idle(&mut self) -> bool {
        let Some(e) = &mut self.editor else { return false };
        e.idle(&self.fonts)
    }

    /// The caret: [x, y, height, visible] in content CSS pixels.
    pub fn caret(&mut self) -> [f64; 4] {
        let k = self.px_per_pt();
        let Some(e) = &mut self.editor else { return [0.0; 4] };
        let collapsed = e.selection().is_collapsed();
        let stack = e.stack();
        let Some(c) = e.caret_view() else { return [0.0; 4] };
        let Some((x, y)) = stack.to_doc(c.page, c.x, c.top) else { return [0.0; 4] };
        [x * k, y * k, ((c.bottom - c.top) * k).max(6.0), if collapsed { 1.0 } else { 0.0 }]
    }

    /// Status bar: JSON `{ page, pages, words, mode, zoom, template }`.
    /// The caret's page (from 1), the pages laid out, and the words in the document.
    pub fn status_numbers(&mut self) -> [usize; 3] {
        let Some(e) = &mut self.editor else { return [0; 3] };
        let page = e.caret_view().map_or(0, |c| c.page);
        // shortcut: words are counted over the whole document per call; count per changed paragraph when it shows in
        // typing profiles.
        [page + 1, e.snapshot().pages().len(), word_count(e.document())]
    }

    pub fn status(&mut self) -> String {
        let zoom = self.zoom;
        if self.editor.is_none() {
            return "{}".into();
        }
        let [page, pages, words] = self.status_numbers();
        format!("{{\"page\":{page},\"pages\":{pages},\"words\":{words},\"mode\":\"Editing\",\"zoom\":{zoom},\"template\":false}}")
    }

    /// What the focused textarea mirrors for screen readers: the focus paragraph and the selection in UTF-16 units.
    pub fn input_mirror(&mut self) -> String {
        let empty = "{\"text\":\"\",\"start\":0,\"end\":0,\"backward\":false}".to_string();
        let Some(e) = &mut self.editor else { return empty };
        let Some(focus) = e.selection().focus() else { return empty };
        let Place::Text { para, offset, .. } = focus.place else { return empty };
        let Some(p) = e.index().para(e.document(), para) else { return empty };
        let t = UnitText::of(p);
        let u16_at = |u: u32| t.text.chars().take(u as usize).map(char::len_utf16).sum::<usize>();
        let (start, end, backward) = match e.selection() {
            Selection::Text { anchor, focus, .. } => {
                let a = anchor.para_offset().filter(|(pa, _)| *pa == para).map_or(0, |(_, u)| u16_at(u.0));
                let f = u16_at(focus.para_offset().map_or(offset.0, |(_, u)| u.0));
                (a.min(f), a.max(f), f < a)
            }
            _ => (u16_at(offset.0), u16_at(offset.0), false),
        };
        let text: String = t.text.chars().map(|c| if matches!(c, '\u{2028}' | '\u{c}') { '\n' } else { c }).collect();
        format!("{{\"text\":{},\"start\":{start},\"end\":{end},\"backward\":{backward}}}", json_str(&text))
    }

    /// The reading structure for the screen-reader DOM: one paragraph node per body paragraph with its lines.

    /// The editor's semantic state for tests; `settled` is what the host knows: no unsent
    /// input and no deferred layout through the visible pages.
    /// The messages applied since the document opened, one JSON per line (`twip replay` input).
    #[cfg(feature = "inspect")]
    pub fn input_log(&self) -> String {
        self.log.join("\n")
    }

    #[cfg(feature = "inspect")]
    pub fn inspect(&mut self, what: &str) -> String {
        let Some(e) = &mut self.editor else { return "{}".into() };
        let pending = e.snapshot().info().iter().any(|i| i.provisional);
        crate::editing::inspect::inspect(e, what, !pending).to_string()
    }

    /// The edited document as .docx bytes.
    /// The document as PDF: laid out completely first.
    pub fn export_pdf(&mut self, title: &str) -> Vec<u8> {
        let fonts = &self.fonts;
        let Some(e) = self.editor.as_mut() else { return Vec::new() };
        e.complete(fonts);
        crate::pdf::export(e.document(), &e.snapshot(), title)
    }

    pub fn save(&mut self) -> Result<Vec<u8>, String> {
        let e = self.editor.as_mut().ok_or("no document")?;
        let bytes = crate::docx::write::save(e.document())?;
        e.mark_saved();
        Ok(bytes)
    }

    /// Draws the pages under the viewport: the canvas is `width` x `height` device pixels at `dpr`, and the page
    /// stack's top-left sits at (`origin_x`, `origin_y`) CSS pixels in it.
    /// Adopts the current WebGL 2 context (the page made it current) for this editor's canvas.
    #[cfg(target_os = "emscripten")]
    pub fn attach_webgl(&mut self) -> bool {
        crate::render::web::attach_gl(self.serial)
    }

    /// Paints the pages under the viewport into this editor's WebGL canvas (`width` x `height` device pixels); the
    /// page stack's top-left is at (`origin_x`, `origin_y`) CSS pixels.
    #[cfg(target_os = "emscripten")]
    pub fn paint_webgl(&mut self, origin_x: f64, origin_y: f64, width: u32, height: u32, dpr: f64) -> bool {
        let (w, h) = (width as i32, height as i32);
        crate::render::web::gl_frame(self.serial, w, h, |c| self.draw(c, origin_x, origin_y, f64::from(width), f64::from(height), dpr))
    }

    /// As [`paint_webgl`](Self::paint_webgl) into a WebGPU canvas texture (an emdawnwebgpu handle from
    /// `importJsTexture`, released here). The device must be attached ([`crate::render::web::attach_webgpu`]).
    /// # Safety
    /// `texture` must be a live emdawnwebgpu texture, not used again.
    #[cfg(target_os = "emscripten")]
    pub unsafe fn paint_webgpu(&mut self, texture: usize, bgra: bool, origin_x: f64, origin_y: f64, height: u32, dpr: f64) -> bool {
        unsafe {
            crate::render::web::webgpu_frame(texture, bgra, |c| {
                let w = f64::from(c.base_layer_size().width);
                self.draw(c, origin_x, origin_y, w, f64::from(height), dpr)
            })
        }
    }

    /// Draws into `pixels`: `width` x `height` device pixels, RGBA, 8 bits each, premultiplied, rows packed.
    /// The page stack's top-left sits at (`origin_x`, `origin_y`) CSS pixels. False if the buffer is too small.
    pub fn paint_rgba(&mut self, pixels: &mut [u8], width: u32, height: u32, origin_x: f64, origin_y: f64, dpr: f64) -> bool {
        let row = width as usize * 4;
        if width == 0 || height == 0 || pixels.len() < row * height as usize {
            return false;
        }
        let info = skia_safe::ImageInfo::new((width as i32, height as i32), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None);
        let Some(mut surface) = skia_safe::surfaces::wrap_pixels(&info, pixels, row, None) else { return false };
        self.draw(surface.canvas(), origin_x, origin_y, f64::from(width), f64::from(height), dpr);
        true
    }

    pub fn draw(&mut self, canvas: &Canvas, origin_x: f64, origin_y: f64, width: f64, height: f64, dpr: f64) {
        let f = Frame { origin_x, origin_y, width, height, dpr, px_per_pt: self.px_per_pt() };
        let Some(e) = &mut self.editor else {
            canvas.clear(self.renderer.background.unwrap_or(Color::from_rgb(0xf3, 0xf3, 0xf3)));
            return;
        };
        // Lay out the visible pages first (they may be provisional), then build the frame's view from that snapshot.
        let probe = e.view(0..0);
        let visible = Renderer::visible(&probe, &f);
        if visible.end > 0 {
            e.ensure_page(visible.end - 1, &self.fonts);
        }
        let marks = self.squiggles(visible.clone());
        let Some(e) = &mut self.editor else { return };
        let grid = if self.view.gridlines { e.cell_rects(visible.clone()) } else { Vec::new() };
        let mut view = e.view(visible);
        view.spelling = marks;
        view.gridlines = grid;
        #[cfg(feature = "inspect")]
        if let Some(n) = &mut self.recording {
            self.frames.push_back(crate::editing::view::canonical(&view));
            if self.frames.len() > 500 {
                self.frames.pop_front();
            }
            *n += 1;
            canvas.clear(self.renderer.background.unwrap_or(Color::from_rgb(0xf3, 0xf3, 0xf3)));
            return;
        }
        self.renderer.draw(canvas, &view, &e.document().images, &f);
    }
}

fn word_count(doc: &crate::docx::Document) -> usize {
    fn walk(blocks: &[crate::docx::Block], n: &mut usize) {
        for b in blocks {
            match b {
                crate::docx::Block::Para(p) => *n += UnitText::of(p).text.split_whitespace().count(),
                crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, n)),
            }
        }
    }
    let mut n = 0;
    doc.sections.iter().for_each(|s| walk(&s.blocks, &mut n));
    n
}

/// Standard base64 with padding: image data in JSON inputs (the input log replays them).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            out.push(if k <= c.len() { T[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}
