//! The binding surface. Everything the SDKs expose is declared here; `moon run twip-ffi:generate` writes the .NET
//! and TypeScript code from it.

// Diplomat's conventions: list types expose `len` (no `is_empty`), borrowed returns need named lifetimes, and a
// command's settings are parameters (Diplomat structs can't hold strings).
#![allow(clippy::len_without_is_empty, clippy::needless_lifetimes, clippy::too_many_arguments)]

#[diplomat::bridge]
pub mod ffi {

    use diplomat_runtime::DiplomatWrite;
    use std::fmt::Write;
    use std::rc::Rc;

    /// Why a document didn't open.
    pub enum OpenError {
        /// No bytes.
        Empty,
        /// Not a .docx package Twip can read.
        Invalid,
    }

    /// Why a document didn't save.
    pub enum SaveError {
        NoDocument,
        Failed,
    }

    /// The font catalog documents are laid out with. Create one and pass it to every editor; add fonts before creating
    /// editors (an editor keeps the catalog it was created with).
    #[diplomat::opaque_mut]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct Fonts(Rc<twip::font::Fonts>);

    impl Fonts {
        /// The fonts installed on this system (none in the browser: add them with `add_font`).
        #[diplomat::attr(auto, named_constructor = "system")]
        pub fn system() -> Box<Fonts> {
            Box::new(Fonts(Rc::new(twip::font::Fonts::system())))
        }

        /// An empty catalog.
        #[diplomat::attr(auto, constructor)]
        pub fn create() -> Box<Fonts> {
            Box::new(Fonts(Rc::new(twip::font::Fonts::default())))
        }

        /// Adds a font file (TrueType, OpenType or a collection); false if it isn't one.
        pub fn add_font(&mut self, bytes: &[u8]) -> bool {
            if bytes.len() < 12 {
                return false;
            }
            Rc::make_mut(&mut self.0).add_bytes(bytes.to_vec());
            true
        }
    }

    /// Bytes the engine produced (a saved .docx, a PDF).
    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct Bytes(Vec<u8>);

    impl Bytes {
        #[diplomat::attr(auto, getter)]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        // The .NET backend can't return a borrowed slice: it copies into the caller's buffer instead
        // (`Bytes.ToArray()` in bindings/dotnet/Twip wraps both).
        #[diplomat::attr(dotnet, disable)]
        pub fn as_slice<'a>(&'a self) -> &'a [u8] {
            &self.0
        }

        /// Copies the bytes into `out` (at most its length); the count copied.
        #[diplomat::attr(not(dotnet), disable)]
        pub fn copy_to(&self, out: &mut [u8]) -> usize {
            let n = out.len().min(self.0.len());
            out[..n].copy_from_slice(&self.0[..n]);
            n
        }
    }

    /// Modifier keys held during a key press or pointer event.
    pub struct Modifiers {
        pub ctrl: bool,
        pub shift: bool,
        pub alt: bool,
        pub meta: bool,
    }

    pub enum PointerKind {
        Down,
        Move,
        Up,
        Cancel,
    }

    /// A copied or cut selection: plain text and HTML for other applications, and a token that pastes the exact
    /// content back into any Twip editor of this process.
    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct Clipboard {
        text: String,
        html: String,
        token: String,
    }

    impl Clipboard {
        pub fn text(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(&self.text);
        }

        pub fn html(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(&self.html);
        }

        pub fn token(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(&self.token);
        }
    }

    /// A rectangle in points (page coordinates) or pixels (content coordinates), as the method says.
    pub struct Rect {
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
    }

    /// The caret in content pixels; `visible` is false while text is selected.
    pub struct CaretPosition {
        pub x: f64,
        pub y: f64,
        pub height: f64,
        pub visible: bool,
    }

    /// What the editor asks of the host after an input.
    pub enum RequestKind {
        /// Scroll so that `rect` on `page` is visible.
        ScrollIntoView,
        /// Keep scrolling at `velocity_x` / `velocity_y` pixels per second while the pointer is held outside the view.
        Autoscroll,
        CapturePointer,
        ReleasePointer,
        /// Show the `cursor` (a CSS cursor name: `text`, `default`, `move`, `col-resize`, ...).
        Cursor,
        /// Put `text` and `html` on the system clipboard (a copy or cut from the keyboard).
        Clipboard,
        /// An unfilled placeholder was activated: supply its content (`fill_placeholder`).
        Placeholder,
        Unknown,
    }

    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct Request(serde_json::Value);

    impl Request {
        #[diplomat::attr(auto, getter)]
        pub fn kind(&self) -> RequestKind {
            match self.0["type"].as_str().unwrap_or("") {
                "scroll_into_view" => RequestKind::ScrollIntoView,
                "autoscroll" => RequestKind::Autoscroll,
                "capture_pointer" => RequestKind::CapturePointer,
                "release_pointer" => RequestKind::ReleasePointer,
                "cursor" => RequestKind::Cursor,
                "clipboard" => RequestKind::Clipboard,
                "placeholder" => RequestKind::Placeholder,
                _ => RequestKind::Unknown,
            }
        }

        /// ScrollIntoView: the page (from 0).
        #[diplomat::attr(auto, getter)]
        pub fn page(&self) -> u32 {
            self.0["page"].as_u64().unwrap_or(0) as u32
        }

        /// ScrollIntoView: the rectangle on the page, points.
        #[diplomat::attr(auto, getter)]
        pub fn rect(&self) -> Rect {
            let r = |i: usize| self.0["rect"][i].as_f64().unwrap_or(0.0);
            Rect { x: r(0), y: r(1), width: r(2), height: r(3) }
        }

        #[diplomat::attr(auto, getter)]
        pub fn velocity_x(&self) -> f64 {
            self.0["vx"].as_f64().unwrap_or(0.0)
        }

        #[diplomat::attr(auto, getter)]
        pub fn velocity_y(&self) -> f64 {
            self.0["vy"].as_f64().unwrap_or(0.0)
        }

        /// Cursor: the CSS cursor name.
        pub fn cursor(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["value"].as_str().unwrap_or("default"));
        }

        /// Clipboard: the plain text.
        pub fn text(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["text"].as_str().unwrap_or(""));
        }

        /// Clipboard: the HTML (empty if none).
        pub fn html(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["html"].as_str().unwrap_or(""));
        }

        /// Placeholder: its key (what to fill it with).
        pub fn placeholder_key(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["placeholder"]["key"].as_str().unwrap_or(""));
        }

        /// Placeholder: its label.
        pub fn placeholder_label(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["placeholder"]["label"].as_str().unwrap_or(""));
        }
    }

    /// Requests since the host last took them, in order.
    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct RequestList(Vec<serde_json::Value>);

    impl RequestList {
        #[diplomat::attr(auto, getter)]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        pub fn get(&self, index: usize) -> Option<Box<Request>> {
            self.0.get(index).map(|v| Box::new(Request(v.clone())))
        }
    }

    /// What happened in the document that the application may act on.
    pub enum EventKind {
        /// An unfilled placeholder was activated (see `Request.placeholder_key`).
        Placeholder,
        /// A field or control was activated in design mode: `id`.
        Activate,
        /// The user asked to update fields: compute each field's result and pass it back with `token`.
        FieldUpdate,
        /// A key asked for an application command (`name`: Find, Replace, Hyperlink, Font, ...).
        Command,
        Unknown,
    }

    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct Event(serde_json::Value);

    impl Event {
        #[diplomat::attr(auto, getter)]
        pub fn kind(&self) -> EventKind {
            match self.0["type"].as_str().unwrap_or("") {
                "placeholder" => EventKind::Placeholder,
                "activate" => EventKind::Activate,
                "field_update" => EventKind::FieldUpdate,
                "command" => EventKind::Command,
                _ => EventKind::Unknown,
            }
        }

        /// Activate: the element's id.
        pub fn id(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["id"].as_str().unwrap_or(""));
        }

        /// Command: the application command's name.
        pub fn name(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["name"].as_str().unwrap_or(""));
        }

        /// FieldUpdate: the token to pass back with the results.
        #[diplomat::attr(auto, getter)]
        pub fn token(&self) -> u32 {
            self.0["token"].as_u64().unwrap_or(0) as u32
        }

        /// FieldUpdate: how many fields to compute.
        #[diplomat::attr(auto, getter)]
        pub fn field_count(&self) -> usize {
            self.0["fields"].as_array().map_or(0, Vec::len)
        }

        /// FieldUpdate: the field's id.
        pub fn field_id(&self, index: usize, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["fields"][index]["id"].as_str().unwrap_or(""));
        }

        /// FieldUpdate: the field's code (`MERGEFIELD Name`, ...).
        pub fn field_code(&self, index: usize, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["fields"][index]["code"].as_str().unwrap_or(""));
        }
    }

    /// Events since the application last took them, in order.
    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct EventList(Vec<serde_json::Value>);

    impl EventList {
        #[diplomat::attr(auto, getter)]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        pub fn get(&self, index: usize) -> Option<Box<Event>> {
            self.0.get(index).map(|v| Box::new(Event(v.clone())))
        }
    }

    /// Where a new row goes relative to the selected rows.
    pub enum RowSide {
        Above,
        Below,
    }

    /// Where a new column goes relative to the selected columns.
    pub enum ColumnSide {
        Left,
        Right,
    }

    /// Which way the remaining cells move when cells are deleted.
    pub enum CellShift {
        /// The cells to the right move left.
        Left,
        /// The cells below move up.
        Up,
    }

    /// What part of the table to select.
    pub enum TableSelection {
        /// The cells under the selection.
        Cell,
        /// The selected rows.
        Row,
        /// The selected columns.
        Column,
        /// The whole table.
        Table,
    }

    /// Vertical alignment of text in a cell.
    pub enum CellAlignment {
        Top,
        Center,
        Bottom,
    }

    /// Which cell edges a border applies to.
    pub enum BorderEdges {
        /// Every edge.
        All,
        /// The outer edges of the selection.
        Outside,
        /// The inner edges, horizontal and vertical.
        Inside,
        /// The inner horizontal edges.
        InsideHorizontal,
        /// The inner vertical edges.
        InsideVertical,
        Top,
        Bottom,
        Left,
        Right,
        /// Removes the borders.
        Clear,
    }

    /// The line style of a border.
    pub enum BorderStyle {
        Single,
        Double,
        Dotted,
        Dashed,
        Thick,
    }

    /// How a row height is applied.
    pub enum RowHeightRule {
        /// The row fits its content.
        Auto,
        /// The row is at least the given height.
        AtLeast,
        /// The row is exactly the given height.
        Exact,
    }

    /// How a table width is measured.
    pub enum TableWidthUnit {
        /// The table fits its content.
        Auto,
        Points,
        /// A percentage of the available width.
        Percent,
    }

    /// Horizontal alignment of a table on the page.
    pub enum TableAlignment {
        Left,
        Center,
        Right,
    }

    /// How text wraps around a floating picture.
    pub enum PictureWrap {
        Square,
        Tight,
        Through,
        TopAndBottom,
        /// The picture is behind the text.
        Behind,
        /// The picture is in front of the text.
        InFront,
    }

    /// The kind of content control.
    pub enum ControlKind {
        PlainText,
        RichText,
        Picture,
        Checkbox,
        Date,
        Group,
    }

    /// What a content control's lock prevents.
    pub enum ControlLock {
        /// Keeps the current lock (no lock for a new control).
        Unchanged,
        /// Nothing is locked.
        Unlocked,
        /// The control can't be deleted.
        Control,
        /// The content can't be edited.
        Content,
        /// Neither the control can be deleted nor its content edited.
        Both,
    }

    /// How serious a mark on an element is.
    pub enum MarkSeverity {
        Error,
        Warning,
        Info,
    }

    /// Which text of an element to search.
    pub enum ElementText {
        /// Field codes.
        Code,
        /// Control tags.
        Tag,
        /// Control titles.
        Alias,
    }

    /// Which elements to include.
    pub enum ElementKind {
        /// Fields and controls.
        Any,
        Field,
        Control,
    }

    /// Paragraph alignment.
    pub enum Alignment {
        /// Lines start at the left margin.
        Left,
        /// Lines are centred.
        Center,
        /// Lines end at the right margin.
        Right,
        /// Lines fill the width, except a paragraph's last.
        Justify,
    }

    /// A text highlight colour.
    pub enum Highlight {
        Yellow,
        Green,
        Cyan,
        Magenta,
        Blue,
        Red,
        DarkBlue,
        DarkCyan,
        DarkGreen,
        DarkMagenta,
        DarkRed,
        DarkYellow,
        DarkGray,
        LightGray,
        Black,
        /// No highlight.
        None,
    }

    /// Borders around paragraphs.
    pub enum ParagraphBorders {
        /// A box around the paragraphs.
        Box,
        /// A line under the paragraphs.
        Bottom,
        /// No borders.
        None,
    }

    /// Page margin presets.
    pub enum MarginPreset {
        /// 1 in on every side.
        Normal,
        /// 0.5 in on every side.
        Narrow,
        /// 1 in top and bottom, 0.75 in left and right.
        Moderate,
        /// 1 in top and bottom, 2 in left and right.
        Wide,
    }

    /// Page orientation.
    pub enum Orientation {
        /// Taller than wide.
        Portrait,
        /// Wider than tall.
        Landscape,
    }

    /// Paper sizes.
    pub enum PaperSize {
        /// 8.5 x 11 in.
        Letter,
        /// 8.5 x 14 in.
        Legal,
        /// 7.25 x 10.5 in.
        Executive,
        /// 11 x 17 in.
        Tabloid,
        /// 297 x 420 mm.
        A3,
        /// 210 x 297 mm.
        A4,
        /// 148 x 210 mm.
        A5,
    }

    /// A page number to insert.
    pub enum PageNumber {
        /// The current page's number.
        Page,
        /// The number of pages.
        PageCount,
        /// "Page X of Y".
        PageOfCount,
    }

    /// A unit of text around the caret.
    pub enum TextUnit {
        Word,
        Sentence,
        Paragraph,
    }

    /// What the editor lets the user do.
    pub enum EditorMode {
        /// Normal editing.
        Editing,
        /// Template design: fields and controls show their names.
        Design,
        /// Viewing only: nothing can change the document.
        ReadOnly,
    }

    /// The status bar's numbers.
    pub struct DocumentStatus {
        /// The caret's page, from 1.
        pub page: u32,
        /// Pages laid out so far.
        pub pages: u32,
        pub words: u32,
        pub zoom: f64,
    }

    pub enum SelectionKind {
        /// No document.
        None,
        Caret,
        Text,
        /// A block of table cells.
        Cells,
        /// A picture or text box.
        Object,
        /// A vertical block of text.
        Block,
    }

    /// The selection. For a caret, text or a block: the paragraph ids and character offsets of its anchor (where it
    /// started) and focus (where it ends now); pass them to `select_range`. Zero for cells and objects.
    pub struct SelectionInfo {
        pub kind: SelectionKind,
        pub anchor_paragraph: u64,
        pub anchor_offset: u32,
        pub focus_paragraph: u64,
        pub focus_offset: u32,
    }

    /// A node of the document's reading structure (headings, paragraphs, lists, tables, rows, cells), for screen
    /// readers and other assistive technology.
    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct AccessibleNode(serde_json::Value);

    impl AccessibleNode {
        /// The role: `document`, `heading`, `paragraph`, `list`, `listitem`, `table`, `row`, `cell`, ...
        pub fn role(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["r"].as_str().unwrap_or(""));
        }

        /// The heading level (0 when not a heading).
        #[diplomat::attr(auto, getter)]
        pub fn level(&self) -> u32 {
            self.0["l"].as_u64().unwrap_or(0) as u32
        }

        /// The accessible name (a list item's label, a cell's position, ...), empty if none.
        pub fn label(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["a"].as_str().unwrap_or(""));
        }

        /// How many drawn lines the node's own text has.
        #[diplomat::attr(auto, getter)]
        pub fn line_count(&self) -> usize {
            self.0["t"].as_array().map_or(0, Vec::len)
        }

        /// A line's text.
        pub fn line_text(&self, index: usize, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["t"][index]["s"].as_str().unwrap_or(""));
        }

        /// A line's box, content pixels.
        pub fn line_rect(&self, index: usize) -> Rect {
            let l = &self.0["t"][index];
            let n = |k: &str| l[k].as_f64().unwrap_or(0.0);
            Rect { x: n("x"), y: n("y"), width: n("w"), height: n("h") }
        }

        #[diplomat::attr(auto, getter)]
        pub fn child_count(&self) -> usize {
            self.0["c"].as_array().map_or(0, Vec::len)
        }

        pub fn child(&self, index: usize) -> Option<Box<AccessibleNode>> {
            self.0["c"].get(index).map(|c| Box::new(AccessibleNode(c.clone())))
        }
    }

    /// A field or content control in the document.
    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct Element(serde_json::Value);

    impl Element {
        /// Its id, for the element methods (`select_element`, `set_field`, ...).
        pub fn id(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["id"].as_str().unwrap_or(""));
        }

        /// `field`, or the control's kind (`plain_text`, `rich_text`, `checkbox`, `date`, `dropdown`, `picture`, ...).
        pub fn kind(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["kind"].as_str().unwrap_or(""));
        }

        /// What the document shows for it.
        pub fn display(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["display"].as_str().unwrap_or(""));
        }

        /// A field's code (`MERGEFIELD Name`), empty for a control.
        pub fn code(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["code"].as_str().unwrap_or(""));
        }

        /// A control's tag, empty for a field.
        pub fn tag(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["tag"].as_str().unwrap_or(""));
        }

        /// A control's title, empty for a field.
        pub fn alias(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["alias"].as_str().unwrap_or(""));
        }

        /// The id of the element it's nested in, empty at the top.
        pub fn parent(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["parent"].as_str().unwrap_or(""));
        }

        /// The story it's in: `body`, `header`, `footer`, `footnote`, `endnote`, `textbox`.
        pub fn story(&self, out: &mut DiplomatWrite) {
            let _ = out.write_str(self.0["story"].as_str().unwrap_or(""));
        }

        #[diplomat::attr(auto, getter)]
        pub fn locked(&self) -> bool {
            self.0["locked"].as_bool().unwrap_or(false)
        }
    }

    /// Elements in document order.
    #[diplomat::opaque]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct ElementList(Vec<serde_json::Value>);

    impl ElementList {
        #[diplomat::attr(auto, getter)]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        pub fn get(&self, index: usize) -> Option<Box<Element>> {
            self.0.get(index).map(|v| Box::new(Element(v.clone())))
        }
    }

    /// An editor for one document: input, commands, saving and drawing.
    #[diplomat::opaque_mut]
    #[diplomat::attr(dotnet, manually_disposable)]
    pub struct Editor(twip::host::Engine);

    impl Editor {
        /// An editor holding a new, empty US Letter document.
        #[diplomat::attr(auto, constructor)]
        pub fn create(fonts: &Fonts) -> Box<Editor> {
            let mut e = twip::host::Engine::default();
            e.fonts = Rc::clone(&fonts.0);
            e.new_document();
            Box::new(Editor(e))
        }

        /// Opens .docx bytes in place of the current document; on an error the current one stays.
        pub fn open(&mut self, bytes: &[u8]) -> Result<(), OpenError> {
            if bytes.is_empty() {
                return Err(OpenError::Empty);
            }
            self.0.try_open(bytes).map_err(|_| OpenError::Invalid)
        }

        /// The document as .docx bytes.
        pub fn save(&mut self) -> Result<Box<Bytes>, SaveError> {
            if self.0.editor().is_none() {
                return Err(SaveError::NoDocument);
            }
            self.0.save().map(|b| Box::new(Bytes(b))).map_err(|_| SaveError::Failed)
        }

        /// The document as PDF, laid out completely first.
        pub fn export_pdf(&mut self, title: &str) -> Box<Bytes> {
            Box::new(Bytes(self.0.export_pdf(title)))
        }

        /// Pages laid out so far (layout continues in the background: see `idle`).
        #[diplomat::attr(auto, getter)]
        pub fn page_count(&self) -> u32 {
            self.0.page_sizes().len() as u32
        }

        /// A summary for logs: pages and whether there are unsaved changes.
        pub fn describe(&self, out: &mut DiplomatWrite) {
            let _ = write!(out, "{} page(s)", self.0.page_sizes().len());
        }

        /// Runs background layout for a while; true while there is more to do.
        pub fn idle(&mut self) -> bool {
            self.0.idle()
        }

        /// A key press (DOM `KeyboardEvent.code` names, e.g. `KeyB`, `ArrowLeft`); true if the editor handled it.
        pub fn key(&mut self, code: &str, mods: Modifiers) -> bool {
            self.0.key(code, mods.ctrl, mods.shift, mods.alt)
        }

        /// Text typed or composed (an IME's committed text).
        pub fn text(&mut self, text: &str) {
            self.0.text(text);
        }

        /// A pointer event at (`x`, `y`) content pixels; true if the editor wants the pointer captured.
        pub fn pointer(&mut self, kind: PointerKind, x: f64, y: f64, clicks: i32, mods: Modifiers) -> bool {
            let kind = match kind {
                PointerKind::Down => "down",
                PointerKind::Move => "move",
                PointerKind::Up => "up",
                PointerKind::Cancel => "cancel",
            };
            let mods = twip::editing::keymap::Mods { ctrl: mods.ctrl, shift: mods.shift, alt: mods.alt, meta: mods.meta };
            self.0.pointer(kind, x, y, clicks, mods)
        }

        /// The visible part of the content, in content pixels, and the device pixel ratio.
        pub fn set_viewport(&mut self, left: f64, top: f64, width: f64, height: f64, dpr: f64) {
            self.0.viewport(left, top, width, height, dpr);
        }

        /// Zoom: 1.0 = 100% (a point is 96/72 pixels).
        pub fn set_zoom(&mut self, zoom: f64) {
            self.0.set_zoom(zoom);
        }

        pub fn undo(&mut self) {
            self.0.run_command(twip::editing::commands::Command::Undo);
        }

        pub fn redo(&mut self) {
            self.0.run_command(twip::editing::commands::Command::Redo);
        }

        pub fn select_all(&mut self) {
            self.0.run_command(twip::editing::commands::Command::SelectAll);
        }

        /// Copies the selection; `cut` deletes it too. `None` when nothing is selected.
        // shortcut: reads the engine's JSON reply; a typed `Engine::copy` replaces it when host/ is reworked.
        pub fn copy(&mut self, cut: bool) -> Option<Box<Clipboard>> {
            let json: serde_json::Value = serde_json::from_str(&self.0.copy(cut)).ok()?;
            let s = |k: &str| json[k].as_str().unwrap_or_default().to_string();
            json.get("token").map(|_| Box::new(Clipboard { text: s("text"), html: s("html"), token: s("token") }))
        }

        /// Pastes: the content `token` names when it came from a Twip editor (`Clipboard.token`), else `text` as
        /// plain text.
        pub fn paste(&mut self, text: &str, token: &str) {
            self.0.paste(text, token);
        }

        /// Inserts a row above or below the selected rows.
        pub fn insert_row(&mut self, side: RowSide) {
            let id = match side {
                RowSide::Above => "table.insert-above",
                RowSide::Below => "table.insert-below",
            };
            self.0.command(id, None);
        }

        /// Inserts a column left or right of the selected columns.
        pub fn insert_column(&mut self, side: ColumnSide) {
            let id = match side {
                ColumnSide::Left => "table.insert-left",
                ColumnSide::Right => "table.insert-right",
            };
            self.0.command(id, None);
        }

        /// Deletes the selected rows.
        pub fn delete_rows(&mut self) {
            self.0.command("table.delete.row", None);
        }

        /// Deletes the selected columns.
        pub fn delete_columns(&mut self) {
            self.0.command("table.delete.column", None);
        }

        /// Deletes the table around the selection.
        pub fn delete_table(&mut self) {
            self.0.command("table.delete.table", None);
        }

        /// Deletes the selected cells, moving the remaining cells left or up.
        pub fn delete_cells(&mut self, shift: CellShift) {
            let id = match shift {
                CellShift::Left => "table.delete.cells-left",
                CellShift::Up => "table.delete.cells-up",
            };
            self.0.command(id, None);
        }

        /// Merges the selected cells into one.
        pub fn merge_cells(&mut self) {
            self.0.command("table.merge", None);
        }

        /// Splits the cell into `columns` cells side by side.
        pub fn split_cell(&mut self, columns: u32) {
            self.0.command("table.split", Some(&columns.to_string()));
        }

        /// Splits the cell into `rows` cells stacked vertically.
        pub fn split_cell_into_rows(&mut self, rows: u32) {
            self.0.command("table.split-rows", Some(&rows.to_string()));
        }

        /// Selects the current cells, rows, columns or the whole table.
        pub fn select_in_table(&mut self, part: TableSelection) {
            let id = match part {
                TableSelection::Cell => "table.select.cell",
                TableSelection::Row => "table.select.row",
                TableSelection::Column => "table.select.column",
                TableSelection::Table => "table.select.table",
            };
            self.0.command(id, None);
        }

        /// Gives the selected columns equal widths.
        pub fn distribute_columns(&mut self) {
            self.0.command("table.distribute.columns", None);
        }

        /// Gives the selected rows equal heights.
        pub fn distribute_rows(&mut self) {
            self.0.command("table.distribute.rows", None);
        }

        /// Sets the vertical alignment of text in the selected cells.
        pub fn set_cell_alignment(&mut self, alignment: CellAlignment) {
            let id = match alignment {
                CellAlignment::Top => "table.cell-align.top",
                CellAlignment::Center => "table.cell-align.center",
                CellAlignment::Bottom => "table.cell-align.bottom",
            };
            self.0.command(id, None);
        }

        /// Turns repeating the header rows at the top of each page on or off for the selected rows.
        pub fn toggle_header_row(&mut self) {
            self.0.command("table.header-row", None);
        }

        /// Sets the borders on the given edges of the selected cells, with `width` in points and `color` as 0xRRGGBB (above 0xFFFFFF for automatic).
        pub fn set_cell_borders(&mut self, edges: BorderEdges, style: BorderStyle, width: f64, color: u32) {
            let edge = match edges {
                BorderEdges::All => "all",
                BorderEdges::Outside => "outside",
                BorderEdges::Inside => "inside",
                BorderEdges::InsideHorizontal => "inside-h",
                BorderEdges::InsideVertical => "inside-v",
                BorderEdges::Top => "top",
                BorderEdges::Bottom => "bottom",
                BorderEdges::Left => "left",
                BorderEdges::Right => "right",
                BorderEdges::Clear => "none",
            };
            let style = match style {
                BorderStyle::Single => "single",
                BorderStyle::Double => "double",
                BorderStyle::Dotted => "dotted",
                BorderStyle::Dashed => "dashed",
                BorderStyle::Thick => "thick",
            };
            let color = if color > 0xFF_FFFF { "auto".to_string() } else { format!("{color:06X}") };
            let arg = format!("{style},{width},{color}");
            self.0.command(&format!("table.borders.{edge}"), Some(&arg));
        }

        /// Sets the inner margins of the selected cells, in points.
        pub fn set_cell_margins(&mut self, top: f64, left: f64, bottom: f64, right: f64) {
            self.0.command("table.cell-margins", Some(&format!("{top},{left},{bottom},{right}")));
        }

        /// Sets the height of the selected rows, in points (ignored for `Auto`).
        pub fn set_row_height(&mut self, height: f64, rule: RowHeightRule) {
            let arg = match rule {
                RowHeightRule::Auto => "auto".to_string(),
                RowHeightRule::AtLeast => format!("{height},atLeast"),
                RowHeightRule::Exact => format!("{height},exact"),
            };
            self.0.command("table.row-height", Some(&arg));
        }

        /// Turns allowing the selected rows to break across pages on or off.
        pub fn toggle_row_break_across_pages(&mut self) {
            self.0.command("table.row-cant-split", None);
        }

        /// Sets the width of the selected columns, in points.
        pub fn set_column_width(&mut self, width: f64) {
            self.0.command("table.column-width", Some(&width.to_string()));
        }

        /// Sets the table's preferred width, in points or as a percentage (ignored for `Auto`).
        pub fn set_table_width(&mut self, width: f64, unit: TableWidthUnit) {
            let arg = match unit {
                TableWidthUnit::Auto => "auto".to_string(),
                TableWidthUnit::Points => width.to_string(),
                TableWidthUnit::Percent => format!("{width}%"),
            };
            self.0.command("table.width", Some(&arg));
        }

        /// Aligns the table horizontally on the page.
        pub fn set_table_alignment(&mut self, alignment: TableAlignment) {
            let id = match alignment {
                TableAlignment::Left => "table.align.left",
                TableAlignment::Center => "table.align.center",
                TableAlignment::Right => "table.align.right",
            };
            self.0.command(id, None);
        }

        /// Sets the table's indent from the margin, in points.
        pub fn set_table_indent(&mut self, indent: f64) {
            self.0.command("table.indent", Some(&indent.to_string()));
        }

        /// Applies the table style with this style id to the table.
        pub fn set_table_style(&mut self, style_id: &str) {
            self.0.command("table.style", Some(style_id));
        }

        /// Sets which parts of the table get the table style's special formatting.
        pub fn set_table_style_options(
            &mut self,
            header_row: bool,
            total_row: bool,
            first_column: bool,
            last_column: bool,
            banded_rows: bool,
            banded_columns: bool,
        ) {
            let on = [
                (header_row, "header-row"),
                (total_row, "total-row"),
                (first_column, "first-column"),
                (last_column, "last-column"),
                (banded_rows, "banded-rows"),
                (banded_columns, "banded-columns"),
            ];
            let arg = on.iter().filter(|(b, _)| *b).map(|(_, n)| *n).collect::<Vec<_>>().join(",");
            self.0.command("table.look", Some(&arg));
        }

        /// Sets the fill colour of the selected cells as 0xRRGGBB, or removes it for a value above 0xFFFFFF.
        pub fn set_cell_shading(&mut self, color: u32) {
            let arg = if color > 0xFF_FFFF { String::new() } else { format!("{color:06X}") };
            self.0.command("table.shading", Some(&arg));
        }

        /// Shows or hides gridlines on table cells that have no borders.
        pub fn set_table_gridlines(&mut self, show: bool) {
            self.0.view.gridlines = show;
        }

        /// Deletes the selected picture.
        pub fn delete_picture(&mut self) {
            self.0.command("picture.delete", None);
        }

        /// Sets how text wraps around the selected floating picture.
        pub fn set_picture_wrap(&mut self, wrap: PictureWrap) {
            let arg = match wrap {
                PictureWrap::Square => "square",
                PictureWrap::Tight => "tight",
                PictureWrap::Through => "through",
                PictureWrap::TopAndBottom => "topAndBottom",
                PictureWrap::Behind => "behind",
                PictureWrap::InFront => "front",
            };
            self.0.command("picture.wrap", Some(arg));
        }

        /// Locks or unlocks the selected floating picture's anchor.
        pub fn toggle_picture_anchor_lock(&mut self) {
            self.0.command("picture.lock-anchor", None);
        }

        /// Returns the selected picture to its original size.
        pub fn reset_picture_size(&mut self) {
            self.0.command("picture.reset-size", None);
        }

        /// Sets the alternative text of the selected picture.
        pub fn set_picture_alt_text(&mut self, text: &str) {
            self.0.command("picture.alt-text.set", Some(text));
        }

        /// Sets the size of the selected picture, in points (both must be positive).
        pub fn set_picture_size(&mut self, width: f64, height: f64) {
            self.0.command("picture.size", Some(&format!("{width},{height}")));
        }

        /// Inserts a field with this code and result text at the caret, or right after element `after` (empty for the caret).
        pub fn insert_field(&mut self, code: &str, display: &str, show_code: bool, after: &str) {
            let mut arg = serde_json::json!({ "code": code, "display": display, "view": if show_code { "code" } else { "result" } });
            if !after.is_empty() {
                arg["target"] = serde_json::json!({ "id": after });
            }
            self.0.command_value("field.insert", arg);
        }

        /// Changes a field's code and result text; an empty string keeps that part as it is.
        pub fn set_field(&mut self, id: &str, code: &str, display: &str) {
            let mut arg = serde_json::json!({ "id": id });
            if !code.is_empty() {
                arg["code"] = code.into();
            }
            if !display.is_empty() {
                arg["display"] = display.into();
            }
            self.0.command_value("field.set", arg);
        }

        /// Inserts an empty field showing its code with the caret inside it, or turns the selected text into a field code.
        pub fn insert_empty_field(&mut self) {
            self.0.command("field.new", None);
        }

        /// Asks the host to update field `id` and the fields inside it, or the fields at the selection when `id` is empty.
        pub fn update_fields(&mut self, id: &str) {
            let arg = if id.is_empty() { serde_json::json!({}) } else { serde_json::json!({ "ids": [id] }) };
            self.0.command_value("field.update", arg);
        }

        /// Sets the result text of field `id` after the host computed it.
        pub fn set_field_result(&mut self, id: &str, display: &str) {
            let arg = serde_json::json!({ "results": [{ "id": id, "display": display }] });
            self.0.command_value("field.results", arg);
        }

        /// Locks or unlocks field `id` and the fields inside it, or the fields at the selection when `id` is empty.
        pub fn set_fields_locked(&mut self, id: &str, locked: bool) {
            let arg = if id.is_empty() { serde_json::json!({}) } else { serde_json::json!({ "ids": [id] }) };
            let cmd = if locked { "field.lock" } else { "field.unlock" };
            self.0.command_value(cmd, arg);
        }

        /// Replaces field `id`, or the fields at the selection when `id` is empty, with its result text.
        pub fn unlink_fields(&mut self, id: &str) {
            let arg = if id.is_empty() { serde_json::json!({}) } else { serde_json::json!({ "ids": [id] }) };
            self.0.command_value("field.unlink", arg);
        }

        /// Sets the label shown in design mode for empty fields with this code; `replace_all` drops the other labels first, and an empty `code` with `replace_all` clears them all.
        pub fn set_field_label(&mut self, code: &str, label: &str, replace_all: bool) {
            let mut labels = serde_json::Map::new();
            if !code.is_empty() {
                labels.insert(code.to_string(), label.into());
            }
            let arg = serde_json::json!({ "labels": labels, "merge": !replace_all });
            self.0.command_value("field.labels", arg);
        }

        /// Removes a field or content control together with its content.
        pub fn remove_element(&mut self, id: &str) {
            let arg = serde_json::json!({ "id": id });
            self.0.command_value("element.remove", arg);
        }

        /// Selects a field or content control, scrolling it into view when `reveal` is true.
        pub fn select_element(&mut self, id: &str, reveal: bool) {
            let arg = serde_json::json!({ "id": id, "reveal": reveal });
            self.0.command_value("element.select", arg);
        }

        /// Marks an element with a severity and a tooltip message; `clear_others` removes the other marks first, and an empty `id` with it clears all marks.
        pub fn mark_element(&mut self, id: &str, severity: MarkSeverity, message: &str, clear_others: bool) {
            let severity = match severity {
                MarkSeverity::Error => "error",
                MarkSeverity::Warning => "warning",
                MarkSeverity::Info => "info",
            };
            let marks =
                if id.is_empty() { serde_json::json!([]) } else { serde_json::json!([{ "id": id, "severity": severity, "message": message }]) };
            let arg = serde_json::json!({ "marks": marks, "clear": clear_others });
            self.0.command_value("element.mark", arg);
        }

        /// Replaces text in field codes or control tags or titles, in element `id` only or in all of them when `id` is empty.
        pub fn replace_in_elements(&mut self, find: &str, replace: &str, within: ElementText, kind: ElementKind, id: &str) {
            let within = match within {
                ElementText::Code => "code",
                ElementText::Tag => "tag",
                ElementText::Alias => "alias",
            };
            let mut arg = serde_json::json!({ "find": find, "replace": replace, "in": within });
            match kind {
                ElementKind::Any => {}
                ElementKind::Field => arg["kind"] = "field".into(),
                ElementKind::Control => arg["kind"] = "control".into(),
            }
            if !id.is_empty() {
                arg["ids"] = serde_json::json!([id]);
            }
            self.0.command_value("element.replace", arg);
        }

        /// Inserts a content control at the caret or after element `after`, or around the selection with `wrap_selection` (rich text and group only); empty strings use the defaults.
        pub fn insert_control(
            &mut self,
            kind: ControlKind,
            tag: &str,
            alias: &str,
            placeholder: &str,
            lock: ControlLock,
            binding: &str,
            wrap_selection: bool,
            multi_line: bool,
            checked: bool,
            date_format: &str,
            after: &str,
        ) {
            let kind = match kind {
                ControlKind::PlainText => "plain_text",
                ControlKind::RichText => "rich_text",
                ControlKind::Picture => "picture",
                ControlKind::Checkbox => "checkbox",
                ControlKind::Date => "date",
                ControlKind::Group => "group",
            };
            let lock = match lock {
                ControlLock::Unchanged => None,
                ControlLock::Unlocked => Some("none"),
                ControlLock::Control => Some("control"),
                ControlLock::Content => Some("content"),
                ControlLock::Both => Some("both"),
            };
            let mut props = serde_json::json!({ "multi_line": multi_line, "value": checked });
            if !date_format.is_empty() {
                props["format"] = date_format.into();
            }
            let mut arg = serde_json::json!({ "kind": kind, "lock": lock, "wrap": wrap_selection, "props": props });
            if !tag.is_empty() {
                arg["tag"] = tag.into();
            }
            if !alias.is_empty() {
                arg["alias"] = alias.into();
            }
            if !placeholder.is_empty() {
                arg["placeholder"] = placeholder.into();
            }
            if !binding.is_empty() {
                arg["binding"] = binding.into();
            }
            if !after.is_empty() {
                arg["target"] = serde_json::json!({ "id": after });
            }
            self.0.command_value("control.insert", arg);
        }

        /// Changes a content control's tag, title, data binding, placeholder text and lock; empty strings and `Unchanged` keep what it has.
        pub fn set_control(&mut self, id: &str, tag: &str, alias: &str, binding: &str, placeholder: &str, lock: ControlLock) {
            let lock = match lock {
                ControlLock::Unchanged => None,
                ControlLock::Unlocked => Some("none"),
                ControlLock::Control => Some("control"),
                ControlLock::Content => Some("content"),
                ControlLock::Both => Some("both"),
            };
            let mut arg = serde_json::json!({ "id": id });
            if let Some(l) = lock {
                arg["lock"] = l.into();
            }
            if !tag.is_empty() {
                arg["tag"] = tag.into();
            }
            if !alias.is_empty() {
                arg["alias"] = alias.into();
            }
            if !binding.is_empty() {
                arg["binding"] = binding.into();
            }
            if !placeholder.is_empty() {
                arg["placeholder"] = placeholder.into();
            }
            self.0.command_value("control.set", arg);
        }

        /// Removes a content control, keeping its content in the document when `keep_content` is true.
        pub fn remove_control(&mut self, id: &str, keep_content: bool) {
            let arg = serde_json::json!({ "id": id, "keep_content": keep_content });
            self.0.command_value("control.remove", arg);
        }

        /// Replaces the content of a plain-text, date or rich-text control with this text.
        pub fn fill_control(&mut self, id: &str, text: &str) {
            let arg = serde_json::json!({ "id": id, "text": text });
            self.0.command_value("control.fill", arg);
        }

        /// Wraps the selected table rows or paragraphs in a new repeating section, with an optional tag and title (empty for none).
        pub fn wrap_in_repeating_section(&mut self, tag: &str, alias: &str) {
            let mut arg = serde_json::json!({});
            if !tag.is_empty() {
                arg["tag"] = tag.into();
            }
            if !alias.is_empty() {
                arg["alias"] = alias.into();
            }
            self.0.command_value("region.wrap", arg);
        }

        /// Writes an XML data part whose root is in `namespace`, replacing the part with that namespace (and `item_id` when not empty) or adding a new one.
        pub fn write_data_part(&mut self, namespace: &str, item_id: &str, xml: &str) {
            let mut arg = serde_json::json!({ "namespace": namespace, "xml": xml });
            if !item_id.is_empty() {
                arg["item_id"] = item_id.into();
            }
            self.0.command_value("data.write", arg);
        }

        /// Removes the XML data part with this namespace (and `item_id` when not empty).
        pub fn remove_data_part(&mut self, namespace: &str, item_id: &str) {
            let mut arg = serde_json::json!({ "namespace": namespace });
            if !item_id.is_empty() {
                arg["item_id"] = item_id.into();
            }
            self.0.command_value("data.remove", arg);
        }

        /// Toggles bold on the selection (or the next typed text).
        pub fn bold(&mut self) {
            self.0.command("home.bold", None);
        }

        /// Toggles italic on the selection (or the next typed text).
        pub fn italic(&mut self) {
            self.0.command("home.italic", None);
        }

        /// Toggles a single underline on the selection (or the next typed text).
        pub fn underline(&mut self) {
            self.0.command("home.underline", None);
        }

        /// Toggles strikethrough on the selection (or the next typed text).
        pub fn strikethrough(&mut self) {
            self.0.command("home.strike", None);
        }

        /// Toggles subscript on the selection (or the next typed text).
        pub fn subscript(&mut self) {
            self.0.command("home.subscript", None);
        }

        /// Toggles superscript on the selection (or the next typed text).
        pub fn superscript(&mut self) {
            self.0.command("home.superscript", None);
        }

        /// Sets the font family of the selection (or the next typed text).
        pub fn set_font(&mut self, family: &str) {
            self.0.command("home.font", Some(family));
        }

        /// Sets the font size of the selection (or the next typed text), rounded to the nearest half point.
        pub fn set_font_size(&mut self, points: f64) {
            self.0.command("home.font-size", Some(&points.to_string()));
        }

        /// Makes the selected text one step larger in the standard font size list.
        pub fn grow_font(&mut self) {
            self.0.command("home.grow-font", None);
        }

        /// Makes the selected text one step smaller in the standard font size list.
        pub fn shrink_font(&mut self) {
            self.0.command("home.shrink-font", None);
        }

        /// Sets the text colour of the selection (or the next typed text).
        pub fn set_font_color(&mut self, color: u32) {
            self.0.command("home.font-color", Some(&format!("{:06X}", color & 0xFF_FFFF)));
        }

        /// Gives the selection (or the next typed text) the automatic text colour.
        pub fn set_font_color_automatic(&mut self) {
            self.0.command("home.font-color", Some("auto"));
        }

        /// Highlights the selection (or the next typed text) in a colour, or removes the highlight.
        pub fn set_highlight(&mut self, color: Highlight) {
            let id = match color {
                Highlight::Yellow => "yellow",
                Highlight::Green => "green",
                Highlight::Cyan => "cyan",
                Highlight::Magenta => "magenta",
                Highlight::Blue => "blue",
                Highlight::Red => "red",
                Highlight::DarkBlue => "darkBlue",
                Highlight::DarkCyan => "darkCyan",
                Highlight::DarkGreen => "darkGreen",
                Highlight::DarkMagenta => "darkMagenta",
                Highlight::DarkRed => "darkRed",
                Highlight::DarkYellow => "darkYellow",
                Highlight::DarkGray => "darkGray",
                Highlight::LightGray => "lightGray",
                Highlight::Black => "black",
                Highlight::None => "none",
            };
            self.0.command("home.highlight", Some(id));
        }

        /// Removes the direct character formatting from the selection.
        pub fn clear_formatting(&mut self) {
            self.0.command("home.clear-formatting", None);
        }

        /// Applies a paragraph style to the selected paragraphs; nothing happens if the document has no such style.
        pub fn set_style(&mut self, style_id: &str) {
            self.0.command("home.style", Some(style_id));
        }

        /// Toggles a bulleted list on the selected paragraphs.
        pub fn bullets(&mut self) {
            self.0.command("home.bullets", None);
        }

        /// Toggles a numbered list on the selected paragraphs.
        pub fn numbering(&mut self) {
            self.0.command("home.numbering", None);
        }

        /// Moves the selected paragraphs' left indent in by half an inch.
        pub fn increase_indent(&mut self) {
            self.0.command("home.indent-increase", None);
        }

        /// Moves the selected paragraphs' left indent out by half an inch, not below zero.
        pub fn decrease_indent(&mut self) {
            self.0.command("home.indent-decrease", None);
        }

        /// Aligns the selected paragraphs.
        pub fn set_alignment(&mut self, alignment: Alignment) {
            let id = match alignment {
                Alignment::Left => "home.align-left",
                Alignment::Center => "home.align-center",
                Alignment::Right => "home.align-right",
                Alignment::Justify => "home.align-justify",
            };
            self.0.command(id, None);
        }

        /// Sets the selected paragraphs' line spacing as a multiple of single spacing.
        pub fn set_line_spacing(&mut self, multiple: f64) {
            self.0.command(&format!("home.line-spacing.{multiple}"), None);
        }

        /// Adds 12 points of space before the selected paragraphs, or removes it when the first one already has some.
        pub fn toggle_space_before(&mut self) {
            self.0.command("home.space-before", None);
        }

        /// Removes the space after the selected paragraphs.
        pub fn remove_space_after(&mut self) {
            self.0.command("home.space-after-remove", None);
        }

        /// Toggles keeping the selected paragraphs on the same page as the next one.
        pub fn keep_with_next(&mut self) {
            self.0.command("layout.keep-with-next", None);
        }

        /// Sets the borders of the selected paragraphs.
        pub fn set_paragraph_borders(&mut self, borders: ParagraphBorders) {
            let id = match borders {
                ParagraphBorders::Box => "layout.borders.box",
                ParagraphBorders::Bottom => "layout.borders.bottom",
                ParagraphBorders::None => "layout.borders.none",
            };
            self.0.command(id, None);
        }

        /// Fills the background of the selected paragraphs with a colour.
        pub fn set_paragraph_shading(&mut self, color: u32) {
            self.0.command("layout.shading", Some(&format!("{:06X}", color & 0xFF_FFFF)));
        }

        /// Removes the background colour of the selected paragraphs.
        pub fn clear_paragraph_shading(&mut self) {
            self.0.command("layout.shading", Some("none"));
        }

        /// Ends the paragraph at the caret and starts a new one, as pressing Enter does.
        pub fn insert_paragraph(&mut self) {
            self.0.command("edit.enter", None);
        }

        /// Inserts a tab character at the caret.
        pub fn insert_tab(&mut self) {
            self.0.command("edit.tab", None);
        }

        /// Redoes the last undone change, or else repeats the last edit at the selection.
        pub fn repeat(&mut self) {
            self.0.command("edit.repeat", None);
        }

        /// Selects the word, sentence or paragraph at the caret.
        pub fn select(&mut self, unit: TextUnit) {
            let id = match unit {
                TextUnit::Word => "edit.select_word",
                TextUnit::Sentence => "edit.select_sentence",
                TextUnit::Paragraph => "edit.select_paragraph",
            };
            self.0.command(id, None);
        }

        /// Selects text within one paragraph and scrolls it into view.
        pub fn select_range(&mut self, paragraph: u64, start: u32, end: u32) {
            self.0.command("edit.select-range", Some(&format!("{paragraph},{start},{end}")));
        }

        /// Replaces every occurrence of a text in the document body, as one undo step.
        pub fn replace_all(&mut self, find: &str, replacement: &str, match_case: bool) {
            let arg = serde_json::json!({ "find": find, "with": replacement, "case": match_case }).to_string();
            self.0.command("edit.replace-all", Some(&arg));
        }

        /// Inserts an empty table at the caret.
        pub fn insert_table(&mut self, rows: u32, cols: u32) {
            self.0.command("insert.table", Some(&format!("{rows},{cols}")));
        }

        /// Inserts a picture at the caret at its own size, scaled down to the text width if wider, and selects it.
        pub fn insert_picture(&mut self, image: &[u8]) {
            self.0.command("insert.picture", Some(&twip::host::base64(image)));
        }

        /// Inserts a 2 x 1 inch text box in front of the text at the caret and puts the caret in it.
        pub fn insert_text_box(&mut self) {
            self.0.command("insert.text-box", None);
        }

        /// Inserts an unchecked checkbox content control at the caret.
        pub fn insert_checkbox(&mut self) {
            self.0.command("insert.checkbox", None);
        }

        /// Inserts a symbol character at the caret, optionally in a given font.
        pub fn insert_symbol(&mut self, symbol: &str, font: &str) {
            self.0.command("insert.symbol", Some(&format!("{symbol}\n{font}")));
        }

        /// Inserts a footnote reference at the caret and moves the caret to the new note's text.
        pub fn insert_footnote(&mut self) {
            self.0.command("insert.footnote", None);
        }

        /// Inserts an endnote reference at the caret and moves the caret to the new note's text.
        pub fn insert_endnote(&mut self) {
            self.0.command("insert.endnote", None);
        }

        /// Inserts a page number field at the caret (in a header, footer or the body).
        pub fn insert_page_number(&mut self, kind: PageNumber) {
            let id = match kind {
                PageNumber::Page => "insert.page-number.page",
                PageNumber::PageCount => "insert.page-number.total",
                PageNumber::PageOfCount => "insert.page-number.page-of",
            };
            self.0.command(id, None);
        }

        /// Inserts a page break at the caret.
        pub fn insert_page_break(&mut self) {
            self.0.command("layout.break.page", None);
        }

        /// Inserts a column break at the caret.
        pub fn insert_column_break(&mut self) {
            self.0.command("layout.break.column", None);
        }

        /// Inserts a line break at the caret, starting a new line in the same paragraph.
        pub fn insert_line_break(&mut self) {
            self.0.command("layout.break.line", None);
        }

        /// Sets the page margins of the current section to a preset.
        pub fn set_margins(&mut self, preset: MarginPreset) {
            let id = match preset {
                MarginPreset::Normal => "layout.margins.normal",
                MarginPreset::Narrow => "layout.margins.narrow",
                MarginPreset::Moderate => "layout.margins.moderate",
                MarginPreset::Wide => "layout.margins.wide",
            };
            self.0.command(id, None);
        }

        /// Turns the current section's pages to portrait or landscape, turning the margins with them.
        pub fn set_orientation(&mut self, orientation: Orientation) {
            let id = match orientation {
                Orientation::Portrait => "layout.orientation.portrait",
                Orientation::Landscape => "layout.orientation.landscape",
            };
            self.0.command(id, None);
        }

        /// Sets the paper size of the current section, keeping its orientation.
        pub fn set_paper_size(&mut self, size: PaperSize) {
            let id = match size {
                PaperSize::Letter => "layout.size.0",
                PaperSize::Legal => "layout.size.1",
                PaperSize::Executive => "layout.size.2",
                PaperSize::Tabloid => "layout.size.3",
                PaperSize::A3 => "layout.size.4",
                PaperSize::A4 => "layout.size.5",
                PaperSize::A5 => "layout.size.6",
            };
            self.0.command(id, None);
        }

        /// Lays out the current section in equal-width text columns.
        pub fn set_columns(&mut self, count: u32) {
            self.0.command(&format!("layout.columns.{count}"), None);
        }

        /// Moves the caret into the current page's header, creating an empty one if there is none.
        pub fn edit_header(&mut self) {
            self.0.command("insert.header", None);
        }

        /// Moves the caret into the current page's footer, creating an empty one if there is none.
        pub fn edit_footer(&mut self) {
            self.0.command("insert.footer", None);
        }

        /// Leaves the header or footer and returns the caret to the body.
        pub fn close_header_footer(&mut self) {
            self.0.command("hf.close", None);
        }

        /// Toggles a different header and footer on the first page of the current section.
        pub fn different_first_page(&mut self) {
            self.0.command("hf.different-first", None);
        }

        /// Switches between editing, template design and read-only viewing.
        pub fn set_mode(&mut self, mode: EditorMode) {
            let id = match mode {
                EditorMode::Editing => "editing",
                EditorMode::Design => "design",
                EditorMode::ReadOnly => "readonly",
            };
            self.0.command("view.mode", Some(id));
        }

        /// Zooms in by 10%, up to 400%.
        pub fn zoom_in(&mut self) {
            self.0.command_bar_execute("view.zoom-in", None);
        }

        /// Zooms out by 10%, down to 25%.
        pub fn zoom_out(&mut self) {
            self.0.command_bar_execute("view.zoom-out", None);
        }

        /// Sets how placeholders with a given key are drawn: a fill colour, a label and an icon.
        pub fn set_placeholder_look(&mut self, key: &str, color: u32, label: &str, icon_svg: &str) {
            let mut v = serde_json::json!({ "key": key, "color": format!("#{color:08X}") });
            if !label.is_empty() {
                v["label"] = label.into();
            }
            if !icon_svg.is_empty() {
                v["icon"] = icon_svg.into();
            }
            self.0.command("view.placeholder-look", Some(&v.to_string()));
        }

        /// Restores the default look of placeholders with a given key.
        pub fn clear_placeholder_look(&mut self, key: &str) {
            self.0.command("view.placeholder-look", Some(&serde_json::json!({ "key": key }).to_string()));
        }

        /// Toggles code view for every field (`all`) or for the fields at the caret.
        pub fn toggle_field_codes(&mut self, all: bool) {
            self.0.command_value("view.field-codes", serde_json::json!({ "toggle": if all { "all" } else { "here" } }));
        }

        /// Shows every field's code (`true`) or its result (`false`).
        pub fn show_field_codes(&mut self, show: bool) {
            self.0.command_value("view.field-codes", serde_json::json!({ "show": show }));
        }

        /// Shows the drop caret where a dragged item would land, at (`x`, `y`) content pixels.
        pub fn set_drop_caret(&mut self, x: f64, y: f64) {
            let k = self.0.px_per_pt();
            self.0.command_value("view.drop", serde_json::json!({ "x": x / k, "y": y / k }));
        }

        /// Hides the drop caret.
        pub fn clear_drop_caret(&mut self) {
            self.0.command_value("view.drop", serde_json::Value::Null);
        }

        /// The status bar's numbers.
        pub fn status(&mut self) -> DocumentStatus {
            let [page, pages, words] = self.0.status_numbers();
            DocumentStatus { page: page as u32, pages: pages as u32, words: words as u32, zoom: self.0.zoom() }
        }

        /// The selection.
        pub fn selection(&mut self) -> SelectionInfo {
            use twip::editing::selection::Selection as S;
            let none = SelectionInfo { kind: SelectionKind::None, anchor_paragraph: 0, anchor_offset: 0, focus_paragraph: 0, focus_offset: 0 };
            let Some(e) = self.0.editor() else { return none };
            let at = |p: &twip::editing::position::Position| p.para_offset().map_or((0, 0), |(para, unit)| (para.0, unit.0));
            let (kind, a, f) = match e.selection() {
                S::Caret(p) => (SelectionKind::Caret, at(&p), at(&p)),
                S::Text { anchor, focus, .. } => (SelectionKind::Text, at(&anchor), at(&focus)),
                S::Block { anchor, focus } => (SelectionKind::Block, at(&anchor), at(&focus)),
                S::Cells { .. } => (SelectionKind::Cells, (0, 0), (0, 0)),
                S::Object(_) => (SelectionKind::Object, (0, 0), (0, 0)),
            };
            SelectionInfo { kind, anchor_paragraph: a.0, anchor_offset: a.1, focus_paragraph: f.0, focus_offset: f.1 }
        }

        /// The document's reading structure, for assistive technology.
        // shortcut: built from the engine's JSON tree; a typed tree replaces it when host/ is reworked.
        pub fn accessible_tree(&mut self) -> Box<AccessibleNode> {
            Box::new(AccessibleNode(serde_json::from_str(&self.0.accessible_tree()).unwrap_or_default()))
        }

        /// The fields and content controls in document order; `kind` filters (`field`, `plain_text`, ...; empty for
        /// all).
        pub fn elements(&mut self, kind: &str) -> Box<ElementList> {
            let mut q = serde_json::json!({ "type": "query", "q": "elements" });
            if !kind.is_empty() {
                q["kind"] = kind.into();
            }
            let reply: serde_json::Value = serde_json::from_str(&self.0.send_json(&q.to_string())).unwrap_or_default();
            Box::new(ElementList(reply["answer"].as_array().cloned().unwrap_or_default()))
        }

        /// What the editor asked of the host since the last call (scroll, cursor, clipboard, ...). Call after inputs.
        pub fn take_requests(&mut self) -> Box<RequestList> {
            Box::new(RequestList(self.0.take_request_values()))
        }

        /// What happened that the application may act on since the last call.
        pub fn take_events(&mut self) -> Box<EventList> {
            Box::new(EventList(self.0.take_event_values()))
        }

        /// Draws the pages under the viewport into `pixels`: `width` x `height` device pixels, RGBA, 8 bits each,
        /// premultiplied alpha, rows packed (`width * height * 4` bytes). The page stack's top-left sits at
        /// (`origin_x`, `origin_y`) content pixels; `dpr` is the device pixel ratio. False if the buffer is too small.
        #[diplomat::attr(not(dotnet), disable)]
        pub fn paint(&mut self, pixels: &mut [u8], width: u32, height: u32, origin_x: f64, origin_y: f64, dpr: f64) -> bool {
            self.0.paint_rgba(pixels, width, height, origin_x, origin_y, dpr)
        }

        /// As `paint`, into a new buffer (JavaScript can't lend one). `None` for an empty size.
        // shortcut: a copy per frame; browsers paint through WebGPU/WebGL instead (A4), so this serves Node and tests.
        #[diplomat::attr(dotnet, disable)]
        #[diplomat::attr(js, rename = "paint")]
        pub fn paint_to_bytes(&mut self, width: u32, height: u32, origin_x: f64, origin_y: f64, dpr: f64) -> Option<Box<Bytes>> {
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            self.0.paint_rgba(&mut pixels, width, height, origin_x, origin_y, dpr).then(|| Box::new(Bytes(pixels)))
        }

        /// Browser: a WebGPU instance handle for `Module.WebGPU.importJsDevice` / `importJsQueue` (see the web
        /// package's `attachWebGpu` helper).
        #[diplomat::attr(not(js), disable)]
        pub fn webgpu_instance() -> usize {
            #[cfg(target_os = "emscripten")]
            return twip::render::web::webgpu_instance();
            #[cfg(not(target_os = "emscripten"))]
            0
        }

        /// Browser: adopts the page's WebGPU device for every editor (imported device and queue handles, and the
        /// instance from `webgpu_instance`). False when it can't be used: paint with WebGL instead.
        #[diplomat::attr(not(js), disable)]
        pub fn attach_webgpu(instance: usize, device: usize, queue: usize) -> bool {
            #[cfg(target_os = "emscripten")]
            // SAFETY: the web package's helper passes live handles it just imported.
            return unsafe { twip::render::web::attach_webgpu(instance, device, queue) };
            #[cfg(not(target_os = "emscripten"))]
            {
                let _ = (instance, device, queue);
                false
            }
        }

        /// Browser: paints the pages under the viewport into a WebGPU canvas texture (a handle from
        /// `importJsTexture`, released here); `height` is the texture's height in pixels. False if nothing was drawn.
        #[diplomat::attr(not(js), disable)]
        pub fn paint_webgpu(&mut self, texture: usize, bgra: bool, origin_x: f64, origin_y: f64, height: u32, dpr: f64) -> bool {
            #[cfg(target_os = "emscripten")]
            // SAFETY: the web package's helper imports the canvas's current texture for this one call.
            return unsafe { self.0.paint_webgpu(texture, bgra, origin_x, origin_y, height, dpr) };
            #[cfg(not(target_os = "emscripten"))]
            {
                let _ = (texture, bgra, origin_x, origin_y, height, dpr);
                false
            }
        }

        /// Browser: adopts the current WebGL 2 context (made current with `Module.GL.makeContextCurrent`) for this
        /// editor's canvas.
        #[diplomat::attr(not(js), disable)]
        pub fn attach_webgl(&mut self) -> bool {
            #[cfg(target_os = "emscripten")]
            return self.0.attach_webgl();
            #[cfg(not(target_os = "emscripten"))]
            false
        }

        /// Browser: paints the pages under the viewport into this editor's WebGL canvas (`width` x `height` device
        /// pixels, its context current).
        #[diplomat::attr(not(js), disable)]
        pub fn paint_webgl(&mut self, origin_x: f64, origin_y: f64, width: u32, height: u32, dpr: f64) -> bool {
            #[cfg(target_os = "emscripten")]
            return self.0.paint_webgl(origin_x, origin_y, width, height, dpr);
            #[cfg(not(target_os = "emscripten"))]
            {
                let _ = (origin_x, origin_y, width, height, dpr);
                false
            }
        }

        /// The caret, content pixels.
        pub fn caret(&mut self) -> CaretPosition {
            let [x, y, height, visible] = self.0.caret();
            CaretPosition { x, y, height, visible: visible > 0.0 }
        }

        /// The size of all pages stacked with their gaps, content pixels (what the host scrolls).
        pub fn content_size(&self) -> Rect {
            let [width, height] = self.0.content_size();
            Rect { x: 0.0, y: 0.0, width, height }
        }

        /// Fills the selected placeholder (`key` empty) or every unfilled one with `key`, with an image file's bytes;
        /// how many were filled.
        pub fn fill_placeholder(&mut self, key: &str, image: &[u8]) -> u32 {
            self.0.fill_placeholder(key, image) as u32
        }
    }
}
