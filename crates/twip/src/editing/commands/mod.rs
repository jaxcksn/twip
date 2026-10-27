//! Commands: the typed operations every UI surface invokes, with their state and outcome.
//! Keys, the command bar, menus, dialogs and tests all reach behaviour only through [`Command`], so a command gives
//! the same result whichever surface invokes it.

use super::selection::Granularity;

/// What a movement steps by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveBy {
    Char,
    Word,
    Line,
    Para,
    /// Home / End.
    LineEdge,
    /// Ctrl+Home / Ctrl+End.
    Story,
    /// Page Up / Page Down.
    Screen,
    /// Ctrl+Page Up / Ctrl+Page Down.
    Page,
    /// Ctrl+Alt+Page Up / Down.
    ScreenEdge,
    /// Alt+Home / Alt+End in a table: the row's first or last cell.
    RowEdge,
    /// Alt+Page Up / Alt+Page Down in a table: the column's first or last cell.
    ColumnEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Back,
    Forward,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Move {
        by: MoveBy,
        dir: Dir,
        extend: bool,
    },
    SelectAll,
    /// Character and paragraph formatting besides the toggles and alignment.
    Format(super::format::Format),
    /// Delete Cells, shifting the cells below up or the cells to the right left.
    DeleteCellsShift {
        up: bool,
    },
    DistributeColumns,
    DistributeRows,
    /// Split the target cell into this many rows.
    SplitRows(usize),
    /// Insert Footnote or Insert Endnote at the caret.
    InsertNote(crate::docx::NoteKind),
    /// Cell alignment and shading, header rows.
    TableFormat(super::table_format::TableFormat),
    Object(super::objects::ObjectCmd),
    /// Insert Picture: the image file's bytes (PNG, JPEG, GIF, BMP or TIFF).
    InsertPicture(Vec<u8>),
    InsertTextBox,
    /// Fill Placeholder: the selected one (`key` None) or every unfilled one with the key, with an image file.
    FillPlaceholder {
        key: Option<String>,
        data: Vec<u8>,
    },
    /// How unfilled placeholders with `key` (`""`: all others) are drawn; `None` takes the look away.
    PlaceholderLook {
        key: String,
        look: Option<super::view::PlaceholderLook>,
    },
    /// Numbering (`false`) or Bullets (`true`) on the selected paragraphs.
    ToggleList {
        bullet: bool,
    },
    /// Edit Header or Edit Footer of the caret's page: into it, made first when the page has none.
    EditHeaderFooter {
        footer: bool,
    },
    /// Esc: leave a header or footer, select the text box being edited, or leave object selection.
    Escape,
    /// Close Header and Footer (`hf.close`).
    CloseHeaderFooter,
    InsertText(String),
    /// Backspace (back) or Delete (forward), by grapheme or word.
    Delete {
        dir: Dir,
        word: bool,
    },
    Enter,
    LineBreak,
    PageBreak,
    ColumnBreak,
    InsertTab,
    /// Tab / Shift+Tab in a table.
    NextCell {
        back: bool,
    },
    Undo,
    Redo,
    /// Ctrl+Y / F4: redo, or with nothing to redo, the last repeatable action again.
    Repeat,
    /// The word, sentence or paragraph at the caret.
    Select(Granularity),
    /// Insert Rows Above / Below (`n` rows), Insert Columns Left / Right.
    InsertRows {
        below: bool,
        n: usize,
    },
    InsertColumns {
        right: bool,
    },
    DeleteRows,
    DeleteColumns,
    DeleteTable,
    MergeCells,
    SplitCell {
        cols: usize,
    },
    InsertTable {
        rows: usize,
        cols: usize,
    },
    SelectTable,
    SelectCells(super::selection::CellShape),
    /// Ctrl+Shift+Enter in a table.
    SplitTable,
    /// Shift+Alt+Home/End/Page Up/Page Down in a table.
    SelectToEdge {
        row: bool,
        end: bool,
    },
    /// Tab / Shift+Tab at a list item's start: one level down or up.
    ListLevel {
        back: bool,
    },
    /// Ctrl+B, Ctrl+I, Ctrl+U.
    Toggle(super::format::Toggle),
    /// Ctrl+L, Ctrl+E, Ctrl+R, Ctrl+J.
    Align(crate::docx::props::Jc),
    Copy,
    Cut,
    /// Paste `text` (external paste is plain text).
    PasteText(String),
    /// Section and page settings of the focus's section.
    Section(super::section::SectionCmd),
    /// Several commands as one undo step (a dialog's OK).
    Batch {
        label: &'static str,
        commands: Vec<Command>,
    },
    /// Insert Symbol: the character in a font (empty: the caret's).
    InsertSymbol {
        ch: char,
        font: String,
    },
    /// A field the layout computes (PAGE, NUMPAGES, ...) at the caret.
    InsertField(crate::docx::FieldKind),
    /// Insert Checkbox: a checkbox content control at the caret.
    InsertCheckbox {
        checked: bool,
    },
    /// Insert Hyperlink: `text` linking to `address` (a HYPERLINK field), over the selection.
    InsertHyperlink {
        text: String,
        address: String,
    },
    /// Alt+Shift+Up / Down: the target rows move up or down one row.
    MoveRows {
        up: bool,
    },
    /// Auto-fit one grid column to its content (a double-click on its right border).
    AutoFitColumn {
        table: crate::layout::TableId,
        col: usize,
    },
    /// Ctrl+Alt+5: the first floating object selected; Tab / Shift+Tab then go through them.
    ObjectFocus,
    NextObject {
        back: bool,
    },
    /// Shift+F5: the previous edit location.
    GoBack,
    /// F8: extend mode, then the word, sentence, paragraph and document.
    ExtendMode,
    /// Ctrl+Shift+F8: block selection mode.
    BlockSelect,
    /// Replace All (one undo step).
    ReplaceAll {
        find: String,
        with: String,
        case: bool,
    },
    /// Units `a..b` of a paragraph selected (find, spelling).
    SelectRange {
        para: crate::layout::ParaId,
        a: crate::layout::Unit,
        b: crate::layout::Unit,
    },
    /// Editing, template design or read-only (`view.mode`).
    SetMode(super::Mode),
    /// A merge command with its JSON argument, parsed when it runs so a bad
    /// argument is answered with its name.
    Merge {
        id: String,
        arg: serde_json::Value,
    },
}

impl Command {
    /// Whether it changes the document: what a read-only document disables. (Every edit is refused in
    /// `commit` too, so a command listed wrongly here can't change a read-only document.)
    pub fn mutates(&self) -> bool {
        match self {
            Command::Move { .. }
            | Command::SelectAll
            | Command::Escape
            | Command::CloseHeaderFooter
            | Command::EditHeaderFooter { .. }
            | Command::NextCell { .. }
            | Command::Select(_)
            | Command::SelectTable
            | Command::SelectCells(_)
            | Command::SelectToEdge { .. }
            | Command::Copy
            | Command::ObjectFocus
            | Command::NextObject { .. }
            | Command::GoBack
            | Command::ExtendMode
            | Command::BlockSelect
            | Command::SelectRange { .. }
            | Command::PlaceholderLook { .. }
            | Command::SetMode(_) => false,
            Command::Merge { id, .. } => !matches!(
                id.as_str(),
                "view.drop" | "field.labels" | "element.mark" | "element.select" | "view.looks" | "view.field-codes" | "field.update"
            ),
            Command::Format(_)
            | Command::DeleteCellsShift { .. }
            | Command::DistributeColumns
            | Command::DistributeRows
            | Command::SplitRows(_)
            | Command::InsertNote(_)
            | Command::TableFormat(_)
            | Command::Object(_)
            | Command::InsertPicture(_)
            | Command::InsertTextBox
            | Command::FillPlaceholder { .. }
            | Command::ToggleList { .. }
            | Command::InsertText(_)
            | Command::Delete { .. }
            | Command::Enter
            | Command::LineBreak
            | Command::PageBreak
            | Command::ColumnBreak
            | Command::InsertTab
            | Command::Undo
            | Command::Redo
            | Command::Repeat
            | Command::InsertRows { .. }
            | Command::InsertColumns { .. }
            | Command::DeleteRows
            | Command::DeleteColumns
            | Command::DeleteTable
            | Command::MergeCells
            | Command::SplitCell { .. }
            | Command::InsertTable { .. }
            | Command::SplitTable
            | Command::ListLevel { .. }
            | Command::Toggle(_)
            | Command::Align(_)
            | Command::Cut
            | Command::PasteText(_)
            | Command::Section(_)
            | Command::InsertSymbol { .. }
            | Command::InsertField(_)
            | Command::InsertCheckbox { .. }
            | Command::InsertHyperlink { .. }
            | Command::MoveRows { .. }
            | Command::AutoFitColumn { .. }
            | Command::ReplaceAll { .. } => true,
            Command::Batch { commands, .. } => commands.iter().any(Command::mutates),
        }
    }
}

/// Whether a command can run now, and its toggle state or value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandState {
    pub enabled: bool,
    pub checked: Option<bool>,
    pub reason: Option<&'static str>,
}

/// What executing a command did.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The document changed to this revision.
    Changed(u64),
    /// The selection or view changed, the document didn't.
    Moved,
    /// Nothing happened, and why.
    Unchanged(&'static str),
}

/// Why a host command id isn't a command.
#[derive(Debug, PartialEq)]
pub struct Unknown;

impl Command {
    /// A host command id (command bar, menus, tests) with its argument, parsed at the boundary.
    pub fn from_host(id: &str, arg: Option<&serde_json::Value>) -> Result<Command, Unknown> {
        use super::format::Format as F;
        let text = || arg.and_then(|a| a.as_str()).map(str::to_string).ok_or(Unknown);
        Ok(match id {
            "home.undo" | "edit.undo" => Command::Undo,
            "home.redo" | "edit.redo" => Command::Redo,
            "edit.select_all" => Command::SelectAll,
            "hf.close" => Command::CloseHeaderFooter,
            "insert.header" | "hf.go-header" => Command::EditHeaderFooter { footer: false },
            "insert.footer" | "hf.go-footer" => Command::EditHeaderFooter { footer: true },
            "edit.copy" => Command::Copy,
            "edit.cut" => Command::Cut,
            "edit.paste_text" => Command::PasteText(text()?),
            "edit.insert_text" => Command::InsertText(text()?),
            "edit.enter" => Command::Enter,
            "layout.break.line" => Command::LineBreak,
            "layout.break.page" | "insert.page_break" => Command::PageBreak,
            "layout.break.column" => Command::ColumnBreak,
            "edit.tab" => Command::InsertTab,
            "edit.repeat" => Command::Repeat,
            "table.insert-above" => Command::InsertRows { below: false, n: 1 },
            "table.insert-below" => Command::InsertRows { below: true, n: 1 },
            "table.insert-left" => Command::InsertColumns { right: false },
            "table.insert-right" => Command::InsertColumns { right: true },
            "table.delete.row" => Command::DeleteRows,
            "table.delete.column" => Command::DeleteColumns,
            "table.delete.table" => Command::DeleteTable,
            "table.merge" => Command::MergeCells,
            "table.split" => Command::SplitCell { cols: arg.and_then(|a| a.as_u64().or_else(|| a.as_str()?.parse().ok())).unwrap_or(2) as usize },
            "table.select.table" => Command::SelectTable,
            "table.select.cell" => Command::SelectCells(super::selection::CellShape::Rect),
            "table.select.row" => Command::SelectCells(super::selection::CellShape::Rows),
            "table.select.column" => Command::SelectCells(super::selection::CellShape::Columns),
            "insert.table" => {
                // "rows,cols" from the command bar's grid picker.
                let t = text()?;
                let mut it = t.split(',').map(|x| x.trim().parse::<usize>().ok());
                match (it.next().flatten(), it.next().flatten()) {
                    (Some(rows), Some(cols)) => Command::InsertTable { rows, cols },
                    _ => return Err(Unknown),
                }
            }
            "home.bold" => Command::Toggle(super::format::Toggle::Bold),
            "home.italic" => Command::Toggle(super::format::Toggle::Italic),
            "home.underline" => Command::Toggle(super::format::Toggle::Underline),
            "home.strike" => Command::Toggle(super::format::Toggle::Strike),
            "table.delete.cells-left" => Command::DeleteCellsShift { up: false },
            "table.delete.cells-up" => Command::DeleteCellsShift { up: true },
            "table.distribute" | "table.distribute.columns" => Command::DistributeColumns,
            "table.distribute.rows" => Command::DistributeRows,
            "table.split-rows" => Command::SplitRows(arg.and_then(|a| a.as_u64().or_else(|| a.as_str()?.parse().ok())).unwrap_or(2) as usize),
            "insert.footnote" => Command::InsertNote(crate::docx::NoteKind::Foot),
            "insert.endnote" => Command::InsertNote(crate::docx::NoteKind::End),
            "table.cell-align.top" => Command::TableFormat(super::table_format::TableFormat::CellAlign("top".into())),
            "table.cell-align.center" => Command::TableFormat(super::table_format::TableFormat::CellAlign("center".into())),
            "table.cell-align.bottom" => Command::TableFormat(super::table_format::TableFormat::CellAlign("bottom".into())),
            "table.header-row" => Command::TableFormat(super::table_format::TableFormat::HeaderRow),
            s if s.starts_with("table.borders.") => {
                use super::table_format::Edges;
                let edges = match &s["table.borders.".len()..] {
                    "all" => Edges::All,
                    "outside" => Edges::Outside,
                    "inside" => Edges::Inside,
                    "inside-h" => Edges::InsideH,
                    "inside-v" => Edges::InsideV,
                    "top" => Edges::Top,
                    "bottom" => Edges::Bottom,
                    "left" => Edges::Left,
                    "right" => Edges::Right,
                    "none" => Edges::None,
                    _ => return Err(Unknown),
                };
                // "style,width pt,RRGGBB|auto"; a single ½ pt line by default.
                let t = text().unwrap_or_default();
                let mut it = t.split(',').map(str::trim);
                let style = it.next().filter(|s| !s.is_empty()).unwrap_or("single").to_string();
                let width = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.5);
                let color = it.next().and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok());
                Command::TableFormat(super::table_format::TableFormat::CellBorders(
                    edges,
                    crate::docx::props::Border { style, width, space: 0.0, color },
                ))
            }
            "table.cell-margins" => {
                let v: Vec<f64> = text()?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                let [t, l, b, r] = v[..] else { return Err(Unknown) };
                Command::TableFormat(super::table_format::TableFormat::CellMargins([t, l, b, r]))
            }
            "table.row-height" => {
                // "auto", or "points[,atLeast|exact]".
                let t = text()?;
                let mut it = t.split(',').map(str::trim);
                let h = match it.next() {
                    Some("auto") => None,
                    Some(v) => Some((v.parse().map_err(|_| Unknown)?, it.next().unwrap_or("atLeast").to_string())),
                    None => return Err(Unknown),
                };
                Command::TableFormat(super::table_format::TableFormat::RowHeight(h))
            }
            "table.row-cant-split" => Command::TableFormat(super::table_format::TableFormat::RowCantSplit),
            "table.column-width" => Command::TableFormat(super::table_format::TableFormat::ColumnWidth(text()?.trim().parse().map_err(|_| Unknown)?)),
            "table.width" => {
                // "auto", "50%" (a fraction, as the reader stores it) or points.
                let t = text()?;
                let t = t.trim();
                let w = if t == "auto" {
                    crate::docx::props::Width::Auto
                } else if let Some(p) = t.strip_suffix('%') {
                    crate::docx::props::Width::Pct(p.trim().parse::<f64>().map_err(|_| Unknown)? / 100.0)
                } else {
                    crate::docx::props::Width::Dxa(t.parse().map_err(|_| Unknown)?)
                };
                Command::TableFormat(super::table_format::TableFormat::TableWidth(w))
            }
            "table.align.left" => Command::TableFormat(super::table_format::TableFormat::TableAlign(crate::docx::props::Jc::Left)),
            "table.align.center" => Command::TableFormat(super::table_format::TableFormat::TableAlign(crate::docx::props::Jc::Center)),
            "table.align.right" => Command::TableFormat(super::table_format::TableFormat::TableAlign(crate::docx::props::Jc::Right)),
            "table.indent" => Command::TableFormat(super::table_format::TableFormat::TableIndent(text()?.trim().parse().map_err(|_| Unknown)?)),
            "table.style" => Command::TableFormat(super::table_format::TableFormat::TableStyle(text()?.trim().to_string())),
            "table.look" => {
                // The options on, by name: header-row, total-row, first-column, last-column, banded-rows, banded-columns.
                let t = text().unwrap_or_default();
                let on: Vec<&str> = t.split(',').map(str::trim).collect();
                let has = |n: &str| on.contains(&n);
                Command::TableFormat(super::table_format::TableFormat::TableLook(crate::docx::props::Look {
                    first_row: has("header-row"),
                    last_row: has("total-row"),
                    first_col: has("first-column"),
                    last_col: has("last-column"),
                    no_h_band: !has("banded-rows"),
                    no_v_band: !has("banded-columns"),
                }))
            }
            "table.shading" => {
                let t = text().unwrap_or_default();
                let t = t.trim().trim_start_matches('#');
                Command::TableFormat(super::table_format::TableFormat::CellShading(if t.is_empty() {
                    None
                } else {
                    Some(u32::from_str_radix(t, 16).map_err(|_| Unknown)?)
                }))
            }
            "home.font-size" => Command::Format(F::FontSize(text()?.trim().parse().map_err(|_| Unknown)?)),
            "home.grow-font" => Command::Format(F::GrowFont(true)),
            "home.shrink-font" => Command::Format(F::GrowFont(false)),
            "home.font-color" => {
                let t = text()?;
                let t = t.trim().trim_start_matches('#');
                Command::Format(F::FontColor(if t.eq_ignore_ascii_case("auto") {
                    None
                } else {
                    Some(u32::from_str_radix(t, 16).map_err(|_| Unknown)?)
                }))
            }
            "home.highlight" => {
                let t = text()?;
                Command::Format(F::Highlight((!t.eq_ignore_ascii_case("none")).then(|| t.trim().to_string())))
            }
            "home.font" => Command::Format(F::FontFamily(text()?.trim().to_string())),
            "home.style" => Command::Format(F::ParaStyle(text()?.trim().to_string())),
            "home.clear-formatting" => Command::Format(F::ClearFormatting),
            "home.space-before" => Command::Format(F::SpaceBefore),
            "home.space-after-remove" => Command::Format(F::RemoveSpaceAfter),
            "home.indent-increase" => Command::Format(F::Indent(true)),
            "home.indent-decrease" => Command::Format(F::Indent(false)),
            "layout.keep-with-next" => Command::Format(F::KeepWithNext),
            s if s.starts_with("home.line-spacing.") => {
                Command::Format(F::LineSpacing(s["home.line-spacing.".len()..].parse().map_err(|_| Unknown)?))
            }
            "home.subscript" => Command::Toggle(super::format::Toggle::Subscript),
            "home.superscript" => Command::Toggle(super::format::Toggle::Superscript),
            "home.copy" => Command::Copy,
            "home.cut" => Command::Cut,
            "picture.delete" => Command::Delete { dir: Dir::Forward, word: false },
            "picture.wrap" => Command::Object(super::objects::ObjectCmd::Wrap(match text()?.trim() {
                w @ ("square" | "tight" | "through" | "behind" | "front") => w.to_string(),
                "topAndBottom" | "top-bottom" => "topAndBottom".to_string(),
                _ => return Err(Unknown),
            })),
            // The image file as base64 (inputs are JSON, so the input log replays it).
            "insert.picture" => Command::InsertPicture(base64(&text()?).ok_or(Unknown)?),
            "insert.text-box" => Command::InsertTextBox,
            // The image file as base64; `placeholder.fill-key`: "key\nbase64".
            "placeholder.fill" => Command::FillPlaceholder { key: None, data: base64(&text()?).ok_or(Unknown)? },
            "placeholder.fill-key" => {
                let t = text()?;
                let (key, data) = t.split_once('\n').ok_or(Unknown)?;
                Command::FillPlaceholder { key: Some(key.to_string()), data: base64(data).ok_or(Unknown)? }
            }
            // "editing", "design" or "readonly".
            "view.mode" => Command::SetMode(super::Mode::parse(&text()?).ok_or(Unknown)?),
            "field.insert" | "field.set" | "element.remove" | "control.insert" | "control.set" | "control.remove" | "view.drop" | "field.labels"
            | "element.mark" | "element.select" | "element.replace" | "region.wrap" | "control.fill" | "view.looks" | "data.write"
            | "data.remove" | "view.field-codes" | "field.new" | "field.update" | "field.results" | "field.lock" | "field.unlock"
            | "field.unlink" => Command::Merge { id: id.to_string(), arg: arg.cloned().unwrap_or(serde_json::Value::Null) },
            // JSON: {"key": "...", "color": "#RRGGBB" or "#AARRGGBB", "label": "...", "icon": "<svg ...>"}; without color,
            // label and icon, the key's look is taken away.
            "view.placeholder-look" => {
                let v: serde_json::Value = serde_json::from_str(&text()?).map_err(|_| Unknown)?;
                let key = v["key"].as_str().unwrap_or("").to_string();
                let color = v["color"].as_str().and_then(|c| {
                    let h = c.trim_start_matches('#');
                    let n = u32::from_str_radix(h, 16).ok()?;
                    Some(if h.len() == 6 { 0x40_00_00_00 | n } else { n })
                });
                let label = v["label"].as_str().map(str::to_string);
                let icon = v["icon"].as_str().and_then(super::view::SvgIcon::parse).map(std::rc::Rc::new);
                let look = (color.is_some() || label.is_some() || icon.is_some()).then_some(super::view::PlaceholderLook { color, label, icon });
                Command::PlaceholderLook { key, look }
            }
            "home.numbering" => Command::ToggleList { bullet: false },
            "home.bullets" => Command::ToggleList { bullet: true },
            "picture.lock-anchor" => Command::Object(super::objects::ObjectCmd::LockAnchor),
            "picture.reset-size" => Command::Object(super::objects::ObjectCmd::ResetSize),
            "insert.checkbox" => Command::InsertCheckbox { checked: false },
            "picture.alt-text.set" => Command::Object(super::objects::ObjectCmd::AltText(text().unwrap_or_default())),
            "picture.size" => {
                // "w,h" in points.
                let t = text()?;
                let mut it = t.split(',').map(|x| x.trim().parse::<f64>().ok());
                match (it.next().flatten(), it.next().flatten()) {
                    (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Command::Object(super::objects::ObjectCmd::Size(w, h)),
                    _ => return Err(Unknown),
                }
            }
            "home.align-left" => Command::Align(crate::docx::props::Jc::Left),
            "home.align-center" => Command::Align(crate::docx::props::Jc::Center),
            "home.align-right" => Command::Align(crate::docx::props::Jc::Right),
            "home.align-justify" => Command::Align(crate::docx::props::Jc::Both),
            "layout.margins.normal" => Command::Section(super::section::SectionCmd::Margins([72.0, 72.0, 72.0, 72.0])),
            "layout.margins.narrow" => Command::Section(super::section::SectionCmd::Margins([36.0, 36.0, 36.0, 36.0])),
            "layout.margins.moderate" => Command::Section(super::section::SectionCmd::Margins([72.0, 72.0, 54.0, 54.0])),
            "layout.margins.wide" => Command::Section(super::section::SectionCmd::Margins([72.0, 72.0, 144.0, 144.0])),
            "layout.orientation.portrait" => Command::Section(super::section::SectionCmd::Orientation(false)),
            "layout.orientation.landscape" => Command::Section(super::section::SectionCmd::Orientation(true)),
            s if s.starts_with("layout.size.") => {
                // Letter, Legal, Executive, Tabloid, A3, A4, A5 (portrait, points).
                const PAPER: [(f64, f64); 7] =
                    [(612.0, 792.0), (612.0, 1008.0), (522.0, 756.0), (792.0, 1224.0), (842.0, 1191.0), (595.0, 842.0), (420.0, 595.0)];
                let &(w, h) = s["layout.size.".len()..].parse::<usize>().ok().and_then(|k| PAPER.get(k)).ok_or(Unknown)?;
                Command::Section(super::section::SectionCmd::Size(w, h))
            }
            s if s.starts_with("layout.columns.") => {
                Command::Section(super::section::SectionCmd::Columns(s["layout.columns.".len()..].parse().map_err(|_| Unknown)?))
            }
            "hf.different-first" => Command::Section(super::section::SectionCmd::DifferentFirst),
            "layout.borders.box" => Command::Format(F::ParaBorders("box".into())),
            "layout.borders.bottom" => Command::Format(F::ParaBorders("bottom".into())),
            "layout.borders.none" => Command::Format(F::ParaBorders("none".into())),
            "layout.shading" => {
                let t = text().unwrap_or_default();
                let t = t.trim().trim_start_matches('#');
                Command::Format(F::ParaShading(if t.is_empty() || t.eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(u32::from_str_radix(t, 16).map_err(|_| Unknown)?)
                }))
            }
            "insert.page-break" => Command::PageBreak,
            "insert.page-number.page" | "hf.page-number.page" => Command::InsertField(crate::docx::FieldKind::Page),
            "insert.page-number.total" | "hf.page-number.total" => Command::InsertField(crate::docx::FieldKind::NumPages),
            "insert.page-number.page-of" | "hf.page-number.page-of" => Command::Batch {
                label: "Page Number",
                commands: vec![
                    Command::InsertText("Page ".into()),
                    Command::InsertField(crate::docx::FieldKind::Page),
                    Command::InsertText(" of ".into()),
                    Command::InsertField(crate::docx::FieldKind::NumPages),
                ],
            },
            "insert.symbol" => {
                // The character, or "character\nfont".
                let t = text()?;
                let (c, font) = t.split_once('\n').unwrap_or((&t, ""));
                let mut it = c.chars();
                match (it.next(), it.next()) {
                    (Some(ch), None) => Command::InsertSymbol { ch, font: font.trim().to_string() },
                    _ => return Err(Unknown),
                }
            }
            "edit.replace-all" => {
                // JSON {"find", "with", "case"}.
                let v: serde_json::Value = serde_json::from_str(&text()?).map_err(|_| Unknown)?;
                Command::ReplaceAll {
                    find: v["find"].as_str().ok_or(Unknown)?.to_string(),
                    with: v["with"].as_str().unwrap_or("").to_string(),
                    case: v["case"].as_bool().unwrap_or(false),
                }
            }
            "edit.select-range" => {
                // "paragraph id,start,end" (units).
                let t = text()?;
                let v: Vec<u64> = t.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                let [p, a, b] = v[..] else { return Err(Unknown) };
                Command::SelectRange { para: crate::layout::ParaId(p), a: crate::layout::Unit(a as u32), b: crate::layout::Unit(b as u32) }
            }
            "edit.select_word" => Command::Select(Granularity::Word),
            "edit.select_sentence" => Command::Select(Granularity::Sentence),
            "edit.select_paragraph" => Command::Select(Granularity::Paragraph),
            _ => return Err(Unknown),
        })
    }
}

/// Standard base64 (padding and whitespace allowed); `None` on any other character.
pub(crate) fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[test]
fn base64_decodes() {
    assert_eq!(base64("TWFu").as_deref(), Some(&b"Man"[..]));
    assert_eq!(base64("TWE=").as_deref(), Some(&b"Ma"[..]));
    assert_eq!(base64("TQ==").as_deref(), Some(&b"M"[..]));
    assert_eq!(base64("iVBO\nRw0K").as_deref(), Some(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A][..]));
    assert!(base64("*").is_none());
}
