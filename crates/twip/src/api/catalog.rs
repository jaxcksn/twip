//! The command catalog: every command's id, label, group, shortcuts and undo grouping, for building toolbars, menus
//! and shortcut help.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::command::CommandId;

/// Where a command belongs in a UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandGroup {
    /// Typing, breaks and symbols.
    Text,
    /// Cut, copy and paste.
    Clipboard,
    /// Undo and redo.
    History,
    /// Selecting and moving through the document.
    Selection,
    /// Character formatting.
    Character,
    /// Paragraph formatting.
    Paragraph,
    /// Tab stops.
    Tabs,
    /// Page setup: margins, orientation, paper size and columns.
    Page,
    /// Headers and footers.
    HeaderFooter,
    /// Inserting tables, pictures, text boxes, notes, hyperlinks and fields.
    Insert,
    /// Tables.
    Table,
    /// Pictures and other drawing objects.
    Object,
    /// Fields.
    Field,
    /// Content controls, repeating sections, elements and placeholders.
    Control,
    /// Custom XML data parts.
    Data,
    /// Revisions and comments.
    Review,
    /// Zoom, view options and mode.
    View,
}

/// How a command's changes undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UndoGrouping {
    /// One undo step.
    Step,
    /// Consecutive typing in one paragraph joins one step.
    Typing,
    /// Not an edit: no undo step and no revision.
    View,
}

/// A command's keys, as display strings (`Ctrl+B`, `⌘B`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandShortcuts {
    /// The Windows keys.
    pub windows: Vec<String>,
    /// The Mac keys.
    pub mac: Vec<String>,
}

/// One catalog entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandInfo {
    /// The command.
    pub id: CommandId,
    /// English.
    pub label: String,
    /// The group it belongs to, for building menus and ribbons.
    pub group: CommandGroup,
    /// Its keys.
    pub shortcuts: CommandShortcuts,
    /// Whether it takes arguments.
    pub takes_arguments: bool,
    /// Whether it changes the document.
    pub mutates: bool,
    /// Its undo grouping.
    pub undo: UndoGrouping,
}

/// id, label, group, Windows keys, Mac keys, takes arguments, mutates, undo grouping.
type Entry = (CommandId, &'static str, CommandGroup, &'static [&'static str], &'static [&'static str], bool, bool, UndoGrouping);

/// Every command, in command set order. The Windows keys are the keymap's (`editing::keymap`, which a test checks
/// them against); a key in a context (Tab in a table) is listed with the command it runs there.
const ENTRIES: &[Entry] = {
    use CommandGroup as G;
    use CommandId as C;
    use UndoGrouping as U;
    &[
        (C::InsertText, "Type", G::Text, &[], &[], true, true, U::Typing),
        (C::InsertParagraph, "New Paragraph", G::Text, &["Enter"], &["Enter"], false, true, U::Step),
        (C::InsertLineBreak, "Insert Line Break", G::Text, &["Shift+Enter"], &["⇧Enter"], false, true, U::Step),
        (C::InsertPageBreak, "Insert Page Break", G::Text, &["Ctrl+Enter"], &["⌘Enter"], false, true, U::Step),
        (C::InsertColumnBreak, "Insert Column Break", G::Text, &["Ctrl+Shift+Enter"], &["⇧⌘Enter"], false, true, U::Step),
        (C::InsertSectionBreak, "Insert Section Break", G::Text, &[], &[], true, true, U::Step),
        (C::InsertTab, "Insert Tab", G::Text, &["Tab"], &["Tab"], false, true, U::Typing),
        (C::InsertSymbol, "Symbol", G::Text, &[], &[], true, true, U::Step),
        (
            C::Delete,
            "Delete",
            G::Text,
            &["Backspace", "Delete", "Ctrl+Backspace", "Ctrl+Delete"],
            &["Backspace", "Delete", "⌥Backspace", "⌥Delete"],
            true,
            true,
            U::Typing,
        ),
        (C::Cut, "Cut", G::Clipboard, &["Ctrl+X"], &["⌘X"], false, true, U::Step),
        (C::Copy, "Copy", G::Clipboard, &["Ctrl+C"], &["⌘C"], false, false, U::View),
        (C::Paste, "Paste", G::Clipboard, &["Ctrl+V", "Ctrl+Shift+V"], &["⌘V", "⇧⌘V"], true, true, U::Step),
        (C::PasteText, "Paste Text", G::Clipboard, &[], &[], true, true, U::Step),
        (C::Undo, "Undo", G::History, &["Ctrl+Z"], &["⌘Z"], false, true, U::Step),
        (C::Redo, "Redo", G::History, &["Ctrl+Y", "Ctrl+Shift+Z"], &["⇧⌘Z"], false, true, U::Step),
        (C::Repeat, "Repeat", G::History, &["F4"], &["F4"], false, true, U::Step),
        (C::SelectAll, "Select All", G::Selection, &["Ctrl+A"], &["⌘A"], false, false, U::View),
        (C::Select, "Select", G::Selection, &[], &[], true, false, U::View),
        (C::MoveSelection, "Move Selection", G::Selection, &[], &[], true, false, U::View),
        (C::MoveToCell, "Next Cell", G::Selection, &["Tab", "Shift+Tab"], &["Tab", "⇧Tab"], true, true, U::Step),
        (C::SetSelectionMode, "Selection Mode", G::Selection, &["F8", "Ctrl+Shift+F8"], &["F8", "⇧⌘F8"], true, false, U::View),
        (C::GoBack, "Go Back", G::Selection, &["Shift+F5"], &["⇧F5"], false, false, U::View),
        (C::Escape, "Escape", G::Selection, &["Esc"], &["Esc"], false, false, U::View),
        (C::SelectNextObject, "Select Next Object", G::Selection, &["Ctrl+Alt+5"], &["⌥⌘5"], true, false, U::View),
        (C::GoToPage, "Go To Page", G::Selection, &[], &[], true, false, U::View),
        (C::SetBold, "Bold", G::Character, &["Ctrl+B"], &["⌘B"], true, true, U::Step),
        (C::SetItalic, "Italic", G::Character, &["Ctrl+I"], &["⌘I"], true, true, U::Step),
        (C::SetUnderline, "Underline", G::Character, &["Ctrl+U"], &["⌘U"], true, true, U::Step),
        (C::SetStrikethrough, "Strikethrough", G::Character, &[], &[], true, true, U::Step),
        (C::SetDoubleStrikethrough, "Double Strikethrough", G::Character, &[], &[], true, true, U::Step),
        (C::SetSubscript, "Subscript", G::Character, &["Ctrl+="], &["⌘="], true, true, U::Step),
        (C::SetSuperscript, "Superscript", G::Character, &["Ctrl+Shift+="], &["⇧⌘="], true, true, U::Step),
        (C::SetSmallCaps, "Small Caps", G::Character, &["Ctrl+Shift+K"], &["⇧⌘K"], true, true, U::Step),
        (C::SetAllCaps, "All Caps", G::Character, &["Ctrl+Shift+A"], &["⇧⌘A"], true, true, U::Step),
        (C::SetHidden, "Hidden", G::Character, &["Ctrl+Shift+H"], &["⇧⌘H"], true, true, U::Step),
        (C::SetFont, "Font", G::Character, &[], &[], true, true, U::Step),
        (C::SetFontSize, "Font Size", G::Character, &[], &[], true, true, U::Step),
        (C::GrowFont, "Grow Font", G::Character, &["Ctrl+Shift+>"], &["⇧⌘>"], false, true, U::Step),
        (C::ShrinkFont, "Shrink Font", G::Character, &["Ctrl+Shift+<"], &["⇧⌘<"], false, true, U::Step),
        (C::SetFontColor, "Font Color", G::Character, &[], &[], true, true, U::Step),
        (C::SetHighlight, "Highlight", G::Character, &[], &[], true, true, U::Step),
        (C::SetCharacterSpacing, "Character Spacing", G::Character, &[], &[], true, true, U::Step),
        (C::SetCharacterPosition, "Position", G::Character, &[], &[], true, true, U::Step),
        (C::ClearFormatting, "Clear Formatting", G::Character, &["Ctrl+Space"], &["⌘Space"], false, true, U::Step),
        (C::ChangeCase, "Change Case", G::Character, &["Shift+F3"], &["⇧F3"], true, true, U::Step),
        (C::CopyFormatting, "Copy Formatting", G::Character, &[], &[], false, false, U::View),
        (C::PasteFormatting, "Paste Formatting", G::Character, &[], &[], false, true, U::Step),
        (C::SetAlignment, "Alignment", G::Paragraph, &["Ctrl+L", "Ctrl+E", "Ctrl+R", "Ctrl+J"], &["⌘L", "⌘E", "⌘R", "⌘J"], true, true, U::Step),
        (C::SetLineSpacing, "Line Spacing", G::Paragraph, &["Ctrl+1", "Ctrl+2", "Ctrl+5"], &["⌘1", "⌘2", "⌘5"], true, true, U::Step),
        (C::SetParagraphSpacing, "Paragraph Spacing", G::Paragraph, &["Ctrl+0"], &["⌘0"], true, true, U::Step),
        (C::SetIndents, "Indents", G::Paragraph, &[], &[], true, true, U::Step),
        (C::IncreaseIndent, "Increase Indent", G::Paragraph, &["Ctrl+M"], &["⌘M"], false, true, U::Step),
        (C::DecreaseIndent, "Decrease Indent", G::Paragraph, &["Ctrl+Shift+M"], &["⇧⌘M"], false, true, U::Step),
        (
            C::SetParagraphStyle,
            "Style",
            G::Paragraph,
            &["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3", "Ctrl+Shift+N"],
            &["⌥⌘1", "⌥⌘2", "⌥⌘3", "⇧⌘N"],
            true,
            true,
            U::Step,
        ),
        (C::SetList, "List", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetListLevel, "List Level", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetParagraphFlow, "Line and Page Breaks", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetParagraphBorders, "Borders", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetParagraphShading, "Shading", G::Paragraph, &[], &[], true, true, U::Step),
        (C::AddTabStop, "Add Tab Stop", G::Tabs, &[], &[], true, true, U::Step),
        (C::MoveTabStop, "Move Tab Stop", G::Tabs, &[], &[], true, true, U::Step),
        (C::DeleteTabStop, "Delete Tab Stop", G::Tabs, &[], &[], true, true, U::Step),
        (C::ClearTabStops, "Clear Tab Stops", G::Tabs, &[], &[], false, true, U::Step),
        (C::SetDefaultTabStop, "Default Tab Stops", G::Tabs, &[], &[], true, true, U::Step),
        (C::SetMargins, "Margins", G::Page, &[], &[], true, true, U::Step),
        (C::SetOrientation, "Orientation", G::Page, &[], &[], true, true, U::Step),
        (C::SetPaperSize, "Size", G::Page, &[], &[], true, true, U::Step),
        (C::SetColumns, "Columns", G::Page, &[], &[], true, true, U::Step),
        (C::SetHeaderFooterDistance, "Header and Footer Position", G::Page, &[], &[], true, true, U::Step),
        (C::SetDifferentFirstPage, "Different First Page", G::Page, &[], &[], true, true, U::Step),
        (C::SetDifferentOddEvenPages, "Different Odd and Even Pages", G::Page, &[], &[], true, true, U::Step),
        (C::MoveColumnBoundary, "Move Column Boundary", G::Page, &[], &[], true, true, U::Step),
        (C::EditHeader, "Edit Header", G::HeaderFooter, &[], &[], false, true, U::Step),
        (C::EditFooter, "Edit Footer", G::HeaderFooter, &[], &[], false, true, U::Step),
        (C::CloseHeaderFooter, "Close Header and Footer", G::HeaderFooter, &[], &[], false, false, U::View),
        (C::InsertPageNumber, "Page Number", G::HeaderFooter, &[], &[], true, true, U::Step),
        (C::InsertTable, "Table", G::Insert, &[], &[], true, true, U::Step),
        (C::InsertPicture, "Picture", G::Insert, &[], &[], true, true, U::Step),
        (C::InsertTextBox, "Text Box", G::Insert, &[], &[], false, true, U::Step),
        (C::InsertFootnote, "Footnote", G::Insert, &["Ctrl+Alt+F"], &["⌥⌘F"], false, true, U::Step),
        (C::InsertEndnote, "Endnote", G::Insert, &["Ctrl+Alt+D"], &["⌥⌘D"], false, true, U::Step),
        (C::InsertHyperlink, "Link", G::Insert, &[], &[], true, true, U::Step),
        (C::EditHyperlink, "Edit Link", G::Insert, &[], &[], true, true, U::Step),
        (C::RemoveHyperlink, "Remove Link", G::Insert, &[], &[], false, true, U::Step),
        (C::InsertField, "Field", G::Insert, &["Ctrl+F9"], &["⌘F9"], true, true, U::Step),
        (C::InsertRows, "Insert Rows", G::Table, &[], &[], true, true, U::Step),
        (C::InsertColumns, "Insert Columns", G::Table, &[], &[], true, true, U::Step),
        (C::DeleteRows, "Delete Rows", G::Table, &[], &[], false, true, U::Step),
        (C::DeleteColumns, "Delete Columns", G::Table, &[], &[], false, true, U::Step),
        (C::DeleteTable, "Delete Table", G::Table, &[], &[], false, true, U::Step),
        (C::DeleteCells, "Delete Cells", G::Table, &[], &[], true, true, U::Step),
        (C::MergeCells, "Merge Cells", G::Table, &[], &[], false, true, U::Step),
        (C::SplitCells, "Split Cells", G::Table, &[], &[], true, true, U::Step),
        (C::SplitTable, "Split Table", G::Table, &["Ctrl+Shift+Enter"], &["⇧⌘Enter"], false, true, U::Step),
        (C::DistributeRows, "Distribute Rows", G::Table, &[], &[], false, true, U::Step),
        (C::DistributeColumns, "Distribute Columns", G::Table, &[], &[], false, true, U::Step),
        (C::MoveRows, "Move Rows", G::Table, &["Alt+Shift+Up", "Alt+Shift+Down"], &["⌥⇧Up", "⌥⇧Down"], true, true, U::Step),
        (C::AutoFitColumns, "AutoFit", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellAlignment, "Cell Alignment", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellShading, "Cell Shading", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellBorders, "Cell Borders", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellMargins, "Cell Margins", G::Table, &[], &[], true, true, U::Step),
        (C::SetHeaderRow, "Repeat Header Rows", G::Table, &[], &[], true, true, U::Step),
        (C::SetRowHeight, "Row Height", G::Table, &[], &[], true, true, U::Step),
        (C::SetRowCanBreak, "Allow Row to Break Across Pages", G::Table, &[], &[], true, true, U::Step),
        (C::SetColumnWidth, "Column Width", G::Table, &[], &[], true, true, U::Step),
        (C::MoveTableColumnBoundary, "Move Column Border", G::Table, &[], &[], true, true, U::Step),
        (C::MoveTableRowBoundary, "Move Row Border", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableWidth, "Table Width", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableAlignment, "Table Alignment", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableIndent, "Table Indent", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableStyle, "Table Style", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableStyleOptions, "Table Style Options", G::Table, &[], &[], true, true, U::Step),
        (C::SetObjectSize, "Size", G::Object, &[], &[], true, true, U::Step),
        (C::ResetObjectSize, "Reset Size", G::Object, &[], &[], false, true, U::Step),
        (C::SetObjectWrap, "Wrap Text", G::Object, &[], &[], true, true, U::Step),
        (C::SetAnchorLocked, "Lock Anchor", G::Object, &[], &[], true, true, U::Step),
        (C::SetAltText, "Alt Text", G::Object, &[], &[], true, true, U::Step),
        (C::DeleteObject, "Delete", G::Object, &[], &[], false, true, U::Step),
        (C::SetField, "Edit Field", G::Field, &[], &[], true, true, U::Step),
        (C::SetFieldResults, "Field Results", G::Field, &[], &[], true, true, U::Step),
        (C::UpdateFields, "Update Fields", G::Field, &["F9"], &["F9"], true, true, U::Step),
        (C::SetFieldsLocked, "Lock Fields", G::Field, &["Ctrl+F11", "Ctrl+Shift+F11"], &["⌘F11", "⇧⌘F11"], true, true, U::Step),
        (C::UnlinkFields, "Unlink Fields", G::Field, &["Ctrl+Shift+F9"], &["⇧⌘F9"], true, true, U::Step),
        (C::ShowFieldCodes, "Field Codes", G::Field, &["Alt+F9", "Shift+F9"], &["⌥F9", "⇧F9"], true, false, U::View),
        (C::SetFieldLabels, "Field Labels", G::Field, &[], &[], true, false, U::View),
        (C::InsertControl, "Content Control", G::Control, &[], &[], true, true, U::Step),
        (C::SetControl, "Control Properties", G::Control, &[], &[], true, true, U::Step),
        (C::RemoveControl, "Remove Content Control", G::Control, &[], &[], true, true, U::Step),
        (C::FillControl, "Fill Content Control", G::Control, &[], &[], true, true, U::Step),
        (C::SetCheckbox, "Check Box", G::Control, &[], &[], true, true, U::Step),
        (C::WrapInRepeatingSection, "Repeating Section", G::Control, &[], &[], true, true, U::Step),
        (C::RemoveElement, "Remove", G::Control, &[], &[], true, true, U::Step),
        (C::ReplaceInElements, "Replace in Fields and Controls", G::Control, &[], &[], true, true, U::Step),
        (C::MarkElements, "Mark", G::Control, &[], &[], true, false, U::View),
        (C::SetPlaceholderLooks, "Placeholder Looks", G::Control, &[], &[], true, false, U::View),
        (C::FillPlaceholder, "Fill Placeholder", G::Control, &[], &[], true, true, U::Step),
        (C::WriteDataPart, "Write Data", G::Data, &[], &[], true, true, U::Step),
        (C::RemoveDataPart, "Remove Data", G::Data, &[], &[], true, true, U::Step),
        (C::SetTrackChanges, "Track Changes", G::Review, &["Ctrl+Shift+E"], &["⇧⌘E"], true, true, U::Step),
        (C::SetMarkupView, "Markup", G::Review, &[], &[], true, false, U::View),
        (C::AcceptRevision, "Accept", G::Review, &[], &[], true, true, U::Step),
        (C::RejectRevision, "Reject", G::Review, &[], &[], true, true, U::Step),
        (C::AcceptRevisionsInSelection, "Accept Selected Changes", G::Review, &[], &[], false, true, U::Step),
        (C::RejectRevisionsInSelection, "Reject Selected Changes", G::Review, &[], &[], false, true, U::Step),
        (C::AcceptAllRevisions, "Accept All Changes", G::Review, &[], &[], true, true, U::Step),
        (C::RejectAllRevisions, "Reject All Changes", G::Review, &[], &[], true, true, U::Step),
        (C::SelectRevision, "Next Change", G::Review, &[], &[], true, false, U::View),
        (C::AddComment, "New Comment", G::Review, &[], &[], true, true, U::Step),
        (C::ReplyToComment, "Reply", G::Review, &[], &[], true, true, U::Step),
        (C::EditComment, "Edit Comment", G::Review, &[], &[], true, true, U::Step),
        (C::DeleteComment, "Delete Comment", G::Review, &[], &[], true, true, U::Step),
        (C::ResolveComment, "Resolve", G::Review, &[], &[], true, true, U::Step),
        (C::SetActiveComment, "Active Comment", G::Review, &[], &[], true, false, U::View),
        (C::SelectComment, "Go To Comment", G::Review, &[], &[], true, false, U::View),
        (C::SetAuthorColor, "Author Color", G::Review, &[], &[], true, false, U::View),
        (C::SetZoom, "Zoom", G::View, &[], &[], true, false, U::View),
        (C::SetViewOptions, "Show", G::View, &[], &[], true, false, U::View),
        (C::SetMode, "Mode", G::View, &[], &[], true, false, U::View),
    ]
};

/// Every command, in command set order.
pub fn catalog() -> &'static [CommandInfo] {
    static CATALOG: OnceLock<Vec<CommandInfo>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let strings = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect();
        ENTRIES
            .iter()
            .map(|&(id, label, group, windows, mac, takes_arguments, mutates, undo)| CommandInfo {
                id,
                label: label.into(),
                group,
                shortcuts: CommandShortcuts { windows: strings(windows), mac: strings(mac) },
                takes_arguments,
                mutates,
                undo,
            })
            .collect()
    })
}

/// The catalog entry for `id`.
pub fn info(id: CommandId) -> Option<&'static CommandInfo> {
    // The catalog is in declaration order: an id's entry is at its index.
    catalog().get(id as usize).filter(|c| c.id == id)
}
