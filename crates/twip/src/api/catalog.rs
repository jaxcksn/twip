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
    /// Tables of contents and of figures, captions and cross-references.
    References,
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
        (
            C::InsertText,
            "Type",
            G::Text,
            &["Ctrl+Shift+Space", "Ctrl+Shift+-", "Ctrl+-", "Ctrl+Alt+Num-", "Ctrl+Num-", "Ctrl+Alt+C", "Ctrl+Alt+R", "Ctrl+Alt+T", "Ctrl+Alt+."],
            &["⇧⌘Space", "⇧⌘-", "⌘-", "⌥⌘Num-", "⌘Num-", "⌥⌘C", "⌥⌘R", "⌥⌘T", "⌥⌘."],
            true,
            true,
            U::Typing,
        ),
        (C::InsertParagraph, "New Paragraph", G::Text, &["Enter"], &["Enter"], false, true, U::Step),
        (C::InsertLineBreak, "Insert Line Break", G::Text, &["Shift+Enter"], &["⇧Enter"], true, true, U::Step),
        (C::InsertPageBreak, "Insert Page Break", G::Text, &["Ctrl+Enter"], &["⌘Enter"], false, true, U::Step),
        (C::InsertColumnBreak, "Insert Column Break", G::Text, &["Ctrl+Shift+Enter"], &["⇧⌘Enter"], false, true, U::Step),
        (C::InsertSectionBreak, "Insert Section Break", G::Text, &[], &[], true, true, U::Step),
        (C::InsertStyleSeparator, "Style Separator", G::Paragraph, &["Ctrl+Alt+Enter"], &["⌥⌘Enter"], false, true, U::Step),
        (C::InsertTab, "Insert Tab", G::Text, &["Tab"], &["Tab"], false, true, U::Typing),
        (C::InsertSymbol, "Symbol", G::Text, &[], &[], true, true, U::Step),
        (C::ToggleCharacterCode, "Toggle Character Code", G::Text, &["Alt+X"], &[], false, true, U::Step),
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
        (C::Repaste, "Paste Options", G::Clipboard, &[], &[], true, true, U::Step),
        (C::DismissPasteOptions, "Dismiss Paste Options", G::Clipboard, &[], &[], false, false, U::View),
        (C::SetPasteOptions, "Cut, Copy and Paste Options", G::Clipboard, &[], &[], true, false, U::View),
        (C::InsertFile, "Text from File", G::Insert, &[], &[], true, true, U::Step),
        (C::MoveText, "Move Text", G::Clipboard, &["F2"], &["F2"], false, false, U::View),
        (C::CopyText, "Copy Text", G::Clipboard, &["Shift+F2"], &["⇧F2"], false, false, U::View),
        (C::Undo, "Undo", G::History, &["Ctrl+Z"], &["⌘Z"], true, true, U::Step),
        (C::Redo, "Redo", G::History, &["Ctrl+Shift+Z"], &["⇧⌘Z"], true, true, U::Step),
        (C::Repeat, "Repeat", G::History, &["Ctrl+Y", "F4"], &["⌘Y", "F4"], false, true, U::Step),
        (C::SelectAll, "Select All", G::Selection, &["Ctrl+A"], &["⌘A"], false, false, U::View),
        (C::Select, "Select", G::Selection, &[], &[], true, false, U::View),
        (C::MoveSelection, "Move Selection", G::Selection, &[], &[], true, false, U::View),
        (C::MoveToCell, "Next Cell", G::Selection, &["Tab", "Shift+Tab"], &["Tab", "⇧Tab"], true, false, U::Step),
        (C::SetSelectionMode, "Selection Mode", G::Selection, &["F8", "Ctrl+Shift+F8"], &["F8", "⇧⌘F8"], true, false, U::View),
        (C::GoBack, "Go Back", G::Selection, &["Shift+F5"], &["⇧F5"], false, false, U::View),
        (C::Escape, "Escape", G::Selection, &["Esc"], &["Esc"], false, false, U::View),
        (C::SelectNextObject, "Select Next Object", G::Selection, &["Ctrl+Alt+5"], &["⌥⌘5"], true, false, U::View),
        (C::SelectField, "Select Field", G::Selection, &["F11", "Shift+F11"], &["F11", "⇧F11"], true, false, U::View),
        (C::SetSelection, "Set Selection", G::Selection, &[], &[], true, false, U::View),
        (C::ShrinkSelection, "Shrink Selection", G::Selection, &["Shift+F8"], &["⇧F8"], false, false, U::View),
        (C::SelectSimilarFormatting, "Select Text with Similar Formatting", G::Selection, &[], &[], false, false, U::View),
        (C::SelectStyleInstances, "Select All Instances", G::Selection, &[], &[], true, false, U::View),
        (C::GoToPage, "Go To Page", G::Selection, &[], &[], true, false, U::View),
        (C::GoToBookmark, "Go To Bookmark", G::Selection, &[], &[], true, false, U::View),
        (C::GoTo, "Go To", G::Selection, &[], &[], true, false, U::View),
        (C::SetBold, "Bold", G::Character, &["Ctrl+B"], &["⌘B"], true, true, U::Step),
        (C::SetItalic, "Italic", G::Character, &["Ctrl+I"], &["⌘I"], true, true, U::Step),
        (C::SetUnderline, "Underline", G::Character, &["Ctrl+U", "Ctrl+Shift+D", "Ctrl+Shift+W"], &["⌘U", "⇧⌘D", "⇧⌘W"], true, true, U::Step),
        (C::SetStrikethrough, "Strikethrough", G::Character, &[], &[], true, true, U::Step),
        (C::SetDoubleStrikethrough, "Double Strikethrough", G::Character, &[], &[], true, true, U::Step),
        (C::SetSubscript, "Subscript", G::Character, &["Ctrl+="], &["⌘="], true, true, U::Step),
        (C::SetSuperscript, "Superscript", G::Character, &["Ctrl+Shift+="], &["⇧⌘="], true, true, U::Step),
        (C::SetSmallCaps, "Small Caps", G::Character, &["Ctrl+Shift+K"], &["⇧⌘K"], true, true, U::Step),
        (C::SetAllCaps, "All Caps", G::Character, &["Ctrl+Shift+A"], &["⇧⌘A"], true, true, U::Step),
        (C::SetHidden, "Hidden", G::Character, &["Ctrl+Shift+H"], &["⇧⌘H"], true, true, U::Step),
        (C::SetFont, "Font", G::Character, &["Ctrl+Shift+Q"], &["⇧⌘Q"], true, true, U::Step),
        (C::SetFontSize, "Font Size", G::Character, &[], &[], true, true, U::Step),
        (C::GrowFont, "Grow Font", G::Character, &["Ctrl+Shift+>"], &["⇧⌘>"], false, true, U::Step),
        (C::ShrinkFont, "Shrink Font", G::Character, &["Ctrl+Shift+<"], &["⇧⌘<"], false, true, U::Step),
        (C::GrowFontOnePoint, "Grow Font One Point", G::Character, &["Ctrl+]"], &["⌘]"], false, true, U::Step),
        (C::ShrinkFontOnePoint, "Shrink Font One Point", G::Character, &["Ctrl+["], &["⌘["], false, true, U::Step),
        (C::SetFontColor, "Font Color", G::Character, &[], &[], true, true, U::Step),
        (C::SetHighlight, "Highlight", G::Character, &[], &[], true, true, U::Step),
        (C::SetCharacterSpacing, "Character Spacing", G::Character, &[], &[], true, true, U::Step),
        (C::SetCharacterPosition, "Position", G::Character, &[], &[], true, true, U::Step),
        (C::ApplyCharacterStyle, "Character Style", G::Character, &[], &[], true, true, U::Step),
        (C::SetTypography, "Advanced Font", G::Character, &[], &[], true, true, U::Step),
        (C::SetFitText, "Fit Text", G::Character, &[], &[], true, true, U::Step),
        (C::SetLanguage, "Language", G::Character, &[], &[], true, true, U::Step),
        (C::SetCharacterEffects, "Font Effects", G::Character, &[], &[], true, true, U::Step),
        (C::SetTextEffects, "Text Effects", G::Character, &[], &[], true, true, U::Step),
        (C::SetUnderlineColor, "Underline Color", G::Character, &[], &[], true, true, U::Step),
        (C::ClearFormatting, "Clear Formatting", G::Character, &["Ctrl+Space"], &["⌘Space"], false, true, U::Step),
        (C::ClearAllFormatting, "Clear All Formatting", G::Character, &[], &[], false, true, U::Step),
        (C::ChangeCase, "Change Case", G::Character, &["Shift+F3"], &["⇧F3"], true, true, U::Step),
        (C::CopyFormatting, "Copy Formatting", G::Character, &["Ctrl+Shift+C"], &["⇧⌘C"], false, false, U::View),
        (C::PasteFormatting, "Paste Formatting", G::Character, &["Ctrl+Shift+V"], &["⇧⌘V"], false, true, U::Step),
        (C::SetAlignment, "Alignment", G::Paragraph, &["Ctrl+L", "Ctrl+E", "Ctrl+R", "Ctrl+J"], &["⌘L", "⌘E", "⌘R", "⌘J"], true, true, U::Step),
        (C::SetLineSpacing, "Line Spacing", G::Paragraph, &["Ctrl+1", "Ctrl+2", "Ctrl+5"], &["⌘1", "⌘2", "⌘5"], true, true, U::Step),
        (C::SetParagraphSpacing, "Paragraph Spacing", G::Paragraph, &["Ctrl+0"], &["⌘0"], true, true, U::Step),
        (C::SetIndents, "Indents", G::Paragraph, &[], &[], true, true, U::Step),
        (C::IncreaseIndent, "Increase Indent", G::Paragraph, &["Ctrl+M"], &["⌘M"], false, true, U::Step),
        (C::DecreaseIndent, "Decrease Indent", G::Paragraph, &["Ctrl+Shift+M"], &["⇧⌘M"], false, true, U::Step),
        (C::IncreaseHangingIndent, "Hanging Indent", G::Paragraph, &["Ctrl+T"], &["⌘T"], false, true, U::Step),
        (C::DecreaseHangingIndent, "Remove Hanging Indent", G::Paragraph, &["Ctrl+Shift+T"], &["⇧⌘T"], false, true, U::Step),
        (C::ClearParagraphFormatting, "Reset Paragraph", G::Paragraph, &["Ctrl+Q"], &["⌥⌘Q"], false, true, U::Step),
        (C::MoveParagraphs, "Move Paragraphs", G::Paragraph, &["Alt+Shift+Up", "Alt+Shift+Down"], &["⌃⇧Up", "⌃⇧Down"], true, true, U::Step),
        (
            C::SetParagraphStyle,
            "Style",
            G::Paragraph,
            &["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3", "Ctrl+Shift+N", "Ctrl+Shift+L"],
            &["⌥⌘1", "⌥⌘2", "⌥⌘3", "⇧⌘N", "⇧⌘L"],
            true,
            true,
            U::Step,
        ),
        (C::UpdateStyleToSelection, "Update Style to Match Selection", G::Paragraph, &[], &[], true, true, U::Step),
        (C::DeleteStyle, "Delete Style", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetStyleMeta, "Style Options", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetDocumentDefaults, "Set Defaults", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetDefaultFormatting, "Set Defaults", G::Paragraph, &[], &[], true, true, U::Step),
        (C::CreateStyle, "New Style", G::Paragraph, &[], &[], true, true, U::Step),
        (C::ModifyStyle, "Modify Style", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetList, "List", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetListLevel, "List Level", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetListDefinition, "Define List", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetNumberingValue, "Set Numbering Value", G::Paragraph, &[], &[], true, true, U::Step),
        (C::ApplyListEntry, "Bullets and Numbering", G::Paragraph, &[], &[], true, true, U::Step),
        (C::RestartNumbering, "Restart at 1", G::Paragraph, &[], &[], false, true, U::Step),
        (C::ContinueNumbering, "Continue Numbering", G::Paragraph, &[], &[], false, true, U::Step),
        (C::AdjustListIndents, "Adjust List Indents", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetParagraphFlow, "Line and Page Breaks", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetOutline, "Outline Level", G::Paragraph, &[], &[], true, true, U::Step),
        (C::PromoteOutline, "Promote", G::Paragraph, &["Alt+Shift+Left"], &["⌃⇧Left"], true, true, U::Step),
        (C::DemoteOutline, "Demote", G::Paragraph, &["Alt+Shift+Right"], &["⌃⇧Right"], true, true, U::Step),
        (C::PromoteToHeading1, "Promote to Heading 1", G::Paragraph, &[], &[], true, true, U::Step),
        (C::DemoteToBodyText, "Demote to Body Text", G::Paragraph, &[], &[], false, true, U::Step),
        (C::PromoteHeading, "Promote", G::Paragraph, &[], &[], true, true, U::Step),
        (C::DemoteHeading, "Demote", G::Paragraph, &[], &[], true, true, U::Step),
        (C::MoveHeading, "Move Heading", G::Paragraph, &[], &[], true, true, U::Step),
        (C::DeleteHeading, "Delete Heading", G::Paragraph, &[], &[], true, true, U::Step),
        (C::InsertHeadingBefore, "New Heading Before", G::Paragraph, &[], &[], true, true, U::Step),
        (C::InsertHeadingAfter, "New Heading After", G::Paragraph, &[], &[], true, true, U::Step),
        (C::InsertSubheading, "New Subheading", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SelectHeadingContent, "Select Heading and Content", G::Selection, &[], &[], true, false, U::View),
        (C::SetParagraphTextDirection, "Text Direction", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetParagraphBorders, "Borders", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetParagraphShading, "Shading", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetParagraphBorderLines, "Borders", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetShading, "Shading", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetTextBorder, "Borders", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetPageBorders, "Page Borders", G::Paragraph, &[], &[], true, true, U::Step),
        (C::SetFrame, "Frame", G::Paragraph, &[], &[], true, true, U::Step),
        (C::RemoveFrame, "Remove Frame", G::Paragraph, &[], &[], false, true, U::Step),
        (C::SetDropCap, "Drop Cap", G::Paragraph, &[], &[], true, true, U::Step),
        (C::AddTabStop, "Add Tab Stop", G::Tabs, &[], &[], true, true, U::Step),
        (C::MoveTabStop, "Move Tab Stop", G::Tabs, &[], &[], true, true, U::Step),
        (C::DeleteTabStop, "Delete Tab Stop", G::Tabs, &[], &[], true, true, U::Step),
        (C::ClearTabStops, "Clear Tab Stops", G::Tabs, &[], &[], false, true, U::Step),
        (C::SetDefaultTabStop, "Default Tab Stops", G::Tabs, &[], &[], true, true, U::Step),
        (C::SetAutoHyphenation, "Automatic Hyphenation", G::Page, &[], &[], true, true, U::Step),
        (C::SetHyphenationZone, "Hyphenation Zone", G::Page, &[], &[], true, true, U::Step),
        (C::SetConsecutiveHyphenLimit, "Limit Consecutive Hyphens", G::Page, &[], &[], true, true, U::Step),
        (C::SetHyphenateCaps, "Hyphenate Words in CAPS", G::Page, &[], &[], true, true, U::Step),
        (C::SetMargins, "Margins", G::Page, &[], &[], true, true, U::Step),
        (C::SetOrientation, "Orientation", G::Page, &[], &[], true, true, U::Step),
        (C::SetPaperSize, "Size", G::Page, &[], &[], true, true, U::Step),
        (C::SetColumns, "Columns", G::Page, &[], &[], true, true, U::Step),
        (C::SetHeaderFooterDistance, "Header and Footer Position", G::Page, &[], &[], true, true, U::Step),
        (C::SetDifferentFirstPage, "Different First Page", G::Page, &[], &[], true, true, U::Step),
        (C::SetDifferentOddEvenPages, "Different Odd and Even Pages", G::Page, &[], &[], true, true, U::Step),
        (C::SetPageNumbering, "Format Page Numbers", G::Page, &[], &[], true, true, U::Step),
        (C::SetPageVerticalAlignment, "Vertical Alignment", G::Page, &[], &[], true, true, U::Step),
        (C::SetLineNumbering, "Line Numbers", G::Page, &[], &[], true, true, U::Step),
        (C::SetSectionBreakType, "Section Start", G::Page, &[], &[], true, true, U::Step),
        (C::SetSectionTextDirection, "Section Text Direction", G::Page, &[], &[], true, true, U::Step),
        (C::SetMultiplePages, "Multiple Pages", G::Page, &[], &[], true, true, U::Step),
        (C::MoveColumnBoundary, "Move Column Boundary", G::Page, &[], &[], true, true, U::Step),
        (C::EditHeader, "Edit Header", G::HeaderFooter, &[], &[], false, true, U::Step),
        (C::EditFooter, "Edit Footer", G::HeaderFooter, &[], &[], false, true, U::Step),
        (C::CloseHeaderFooter, "Close Header and Footer", G::HeaderFooter, &[], &[], false, false, U::View),
        (C::InsertPageNumber, "Page Number", G::HeaderFooter, &[], &[], true, true, U::Step),
        (C::SetHeaderFooterLinked, "Link to Previous", G::HeaderFooter, &[], &[], true, true, U::Step),
        (C::RemoveHeaderFooter, "Remove Header or Footer", G::HeaderFooter, &[], &[], true, true, U::Step),
        (C::InsertAlignmentTab, "Insert Alignment Tab", G::HeaderFooter, &[], &[], true, true, U::Step),
        (C::SetPageColor, "Page Color", G::Page, &[], &[], true, true, U::Step),
        (C::SetWatermark, "Watermark", G::Page, &[], &[], true, true, U::Step),
        (C::RemoveWatermark, "Remove Watermark", G::Page, &[], &[], false, true, U::Step),
        (C::InsertTable, "Table", G::Insert, &[], &[], true, true, U::Step),
        (C::InsertPicture, "Picture", G::Insert, &[], &[], true, true, U::Step),
        (C::InsertTextBox, "Text Box", G::Insert, &[], &[], false, true, U::Step),
        (C::InsertShape, "Shape", G::Insert, &[], &[], true, true, U::Step),
        (C::InsertFootnote, "Footnote", G::Insert, &["Ctrl+Alt+F"], &["⌥⌘F"], false, true, U::Step),
        (C::InsertEndnote, "Endnote", G::Insert, &["Ctrl+Alt+D"], &["⌥⌘D"], false, true, U::Step),
        (C::InsertNote, "Footnote and Endnote", G::Insert, &[], &[], true, true, U::Step),
        (C::SetNoteProperties, "Note Options", G::Insert, &[], &[], true, true, U::Step),
        (C::ConvertNotes, "Convert Notes", G::Insert, &[], &[], true, true, U::Step),
        (C::SetNoteSeparator, "Note Separator", G::Insert, &[], &[], true, true, U::Step),
        (C::InsertHyperlink, "Link", G::Insert, &[], &[], true, true, U::Step),
        (C::EditHyperlink, "Edit Link", G::Insert, &[], &[], true, true, U::Step),
        (C::RemoveHyperlink, "Remove Link", G::Insert, &[], &[], false, true, U::Step),
        (C::InsertBookmark, "Bookmark", G::Insert, &[], &[], true, true, U::Step),
        (C::DeleteBookmark, "Delete Bookmark", G::Insert, &[], &[], true, true, U::Step),
        (C::SetDocumentProtection, "Restrict Editing", G::Review, &[], &[], true, true, U::Step),
        (C::AddEditableRange, "Editable Range", G::Review, &[], &[], true, true, U::Step),
        (C::RemoveEditableRanges, "Remove Editable Range", G::Review, &[], &[], false, true, U::Step),
        (C::SetEditableRangeEditor, "Editable Range Editor", G::Review, &[], &[], true, true, U::Step),
        (C::SelectNextEditableRange, "Find Next Region I Can Edit", G::Review, &[], &[], true, false, U::View),
        (C::SelectEditableRanges, "Show All Regions I Can Edit", G::Review, &[], &[], false, false, U::View),
        (C::SetEditingGroups, "Editing Groups", G::Review, &[], &[], true, false, U::View),
        (C::InsertFormField, "Legacy Form Field", G::Insert, &[], &[], true, true, U::Step),
        (C::SetFormField, "Form Field Options", G::Insert, &[], &[], true, true, U::Step),
        (C::FillFormField, "Fill In Form Field", G::Insert, &[], &[], true, true, U::Step),
        (C::SelectNextFormField, "Next Form Field", G::Selection, &[], &[], true, false, U::View),
        (C::SetFormFieldShading, "Form Field Shading", G::Insert, &[], &[], true, true, U::Step),
        (C::SetDocumentProperties, "Properties", G::Insert, &[], &[], true, true, U::Step),
        (C::SetDocumentVariable, "Document Variable", G::Insert, &[], &[], true, true, U::Step),
        (C::SetFileName, "File Name", G::Field, &[], &[], true, false, U::View),
        (
            C::InsertField,
            "Field",
            G::Insert,
            &["Ctrl+F9", "Alt+Shift+D", "Alt+Shift+T", "Alt+Shift+P", "Ctrl+Alt+L"],
            &["⌘F9", "⌃⇧D", "⌃⇧T", "⌃⇧P", "⌥⌘L"],
            true,
            true,
            U::Step,
        ),
        (C::InsertRows, "Insert Rows", G::Table, &[], &[], true, true, U::Step),
        (C::InsertColumns, "Insert Columns", G::Table, &[], &[], true, true, U::Step),
        (C::DeleteRows, "Delete Rows", G::Table, &[], &[], false, true, U::Step),
        (C::DeleteColumns, "Delete Columns", G::Table, &[], &[], false, true, U::Step),
        (C::DeleteTable, "Delete Table", G::Table, &[], &[], false, true, U::Step),
        (C::DeleteCells, "Delete Cells", G::Table, &[], &[], true, true, U::Step),
        (C::MergeCells, "Merge Cells", G::Table, &[], &[], false, true, U::Step),
        (C::SplitCells, "Split Cells", G::Table, &[], &[], true, true, U::Step),
        (C::SplitTable, "Split Table", G::Table, &["Ctrl+Shift+Enter"], &["⇧⌘Enter"], false, true, U::Step),
        (C::ConvertTextToTable, "Convert Text to Table", G::Table, &[], &[], true, true, U::Step),
        (C::ConvertTableToText, "Convert Table to Text", G::Table, &[], &[], true, true, U::Step),
        (C::DistributeRows, "Distribute Rows", G::Table, &[], &[], false, true, U::Step),
        (C::DistributeColumns, "Distribute Columns", G::Table, &[], &[], false, true, U::Step),
        (C::MoveRows, "Move Rows", G::Table, &["Alt+Shift+Up", "Alt+Shift+Down"], &["⌃⇧Up", "⌃⇧Down"], true, true, U::Step),
        (C::AutoFitColumns, "AutoFit", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellAlignment, "Cell Alignment", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellShading, "Cell Shading", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellBorders, "Cell Borders", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellMargins, "Cell Margins", G::Table, &[], &[], true, true, U::Step),
        (C::SetCellProperties, "Cell Properties", G::Table, &[], &[], true, true, U::Step),
        (C::SetRowProperties, "Row Properties", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableBandSizes, "Table Properties", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableCellSpacing, "Cell Spacing", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableAltText, "Table Alt Text", G::Table, &[], &[], true, true, U::Step),
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
        (C::SetDefaultTableStyle, "Set as Default", G::Table, &[], &[], true, true, U::Step),
        (C::SetTablePosition, "Table Position", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableCellMargins, "Table Properties", G::Table, &[], &[], true, true, U::Step),
        (C::InsertCells, "Insert Cells", G::Table, &[], &[], true, true, U::Step),
        (C::SetTableTool, "Table Tool", G::Table, &[], &[], true, false, U::View),
        (C::SetTablePen, "Pen", G::Table, &[], &[], true, false, U::View),
        (C::DrawTableStroke, "Draw Table", G::Table, &[], &[], true, true, U::Step),
        (C::EraseTableBorder, "Eraser", G::Table, &[], &[], true, true, U::Step),
        (C::SetObjectSize, "Size", G::Object, &[], &[], true, true, U::Step),
        (C::ResetObjectSize, "Reset Size", G::Object, &[], &[], false, true, U::Step),
        (C::SetObjectWrap, "Wrap Text", G::Object, &[], &[], true, true, U::Step),
        (C::SetAnchorLocked, "Lock Anchor", G::Object, &[], &[], true, true, U::Step),
        (C::SetObjectPosition, "Position", G::Object, &[], &[], true, true, U::Step),
        (C::SetObjectWrapOptions, "Wrap Options", G::Object, &[], &[], true, true, U::Step),
        (C::SetAltText, "Alt Text", G::Object, &[], &[], true, true, U::Step),
        (C::DeleteObject, "Delete", G::Object, &[], &[], false, true, U::Step),
        (C::SetShapePreset, "Change Shape", G::Object, &[], &[], true, true, U::Step),
        (C::SetShapeAdjust, "Adjust Shape", G::Object, &[], &[], true, true, U::Step),
        (C::SetShapeFill, "Shape Fill", G::Object, &[], &[], true, true, U::Step),
        (C::SetShapeLine, "Shape Outline", G::Object, &[], &[], true, true, U::Step),
        (C::SetShapeEffects, "Shape Effects", G::Object, &[], &[], true, true, U::Step),
        (C::SetShapeText, "Text Box Options", G::Object, &[], &[], true, true, U::Step),
        (C::SetPictureAdjust, "Picture Corrections", G::Object, &[], &[], true, true, U::Step),
        (C::SetPictureCrop, "Crop", G::Object, &[], &[], true, true, U::Step),
        (C::ChangePicture, "Change Picture", G::Object, &[], &[], true, true, U::Step),
        (C::SetObjectRotation, "Rotation", G::Object, &[], &[], true, true, U::Step),
        (C::FlipObject, "Flip", G::Object, &[], &[], true, true, U::Step),
        (C::ArrangeObject, "Arrange", G::Object, &[], &[], true, true, U::Step),
        (C::AlignObjects, "Align", G::Object, &[], &[], true, true, U::Step),
        (C::GroupObjects, "Group", G::Object, &[], &[], false, true, U::Step),
        (C::UngroupObjects, "Ungroup", G::Object, &[], &[], false, true, U::Step),
        (C::RegroupObjects, "Regroup", G::Object, &[], &[], false, true, U::Step),
        (C::EditWrapPoints, "Edit Wrap Points", G::Object, &[], &[], false, false, U::View),
        (C::SelectGroupChild, "Select Group Child", G::Object, &[], &[], true, false, U::View),
        (C::SetObjectName, "Rename", G::Object, &[], &[], true, true, U::Step),
        (C::SetObjectTitle, "Alt Text", G::Object, &[], &[], true, true, U::Step),
        (C::SetObjectHidden, "Hide", G::Object, &[], &[], true, true, U::Step),
        (C::SelectObject, "Select Object", G::Object, &[], &[], true, false, U::View),
        (C::RestackObject, "Reorder", G::Object, &[], &[], true, true, U::Step),
        (C::SetLockAspectRatio, "Lock Aspect Ratio", G::Object, &[], &[], true, true, U::Step),
        (C::SetPictureRecolor, "Recolor", G::Object, &[], &[], true, true, U::Step),
        (C::ResetPicture, "Reset Picture", G::Object, &[], &[], true, true, U::Step),
        (C::SetWrapPoints, "Edit Wrap Points", G::Object, &[], &[], true, true, U::Step),
        (C::ResetWrapPoints, "Reset Wrap Points", G::Object, &[], &[], false, true, U::Step),
        (C::LinkTextBox, "Create Link", G::Object, &[], &[], true, true, U::Step),
        (C::BeginTextBoxLink, "Create Link", G::Object, &[], &[], false, false, U::View),
        (C::UnlinkTextBox, "Break Link", G::Object, &[], &[], false, true, U::Step),
        (C::SetField, "Edit Field", G::Field, &[], &[], true, true, U::Step),
        (C::SetFieldResults, "Field Results", G::Field, &[], &[], true, true, U::Step),
        (C::UpdateFields, "Update Fields", G::Field, &["F9"], &["F9"], true, true, U::Step),
        (C::SetFieldsLocked, "Lock Fields", G::Field, &["Ctrl+F11", "Ctrl+Shift+F11"], &["⌘F11", "⇧⌘F11"], true, true, U::Step),
        (C::UnlinkFields, "Unlink Fields", G::Field, &["Ctrl+Shift+F9"], &["⇧⌘F9"], true, true, U::Step),
        (C::ShowFieldCodes, "Field Codes", G::Field, &["Alt+F9", "Shift+F9"], &["⌥F9", "⇧F9"], true, false, U::View),
        (C::SetFieldLabels, "Field Labels", G::Field, &[], &[], true, false, U::View),
        (C::SetSubTemplateRules, "Sub-template Rules", G::Field, &[], &[], true, false, U::View),
        (C::SetApplicationFieldTypes, "Application Field Types", G::Field, &[], &[], true, false, U::View),
        (C::SetSubTemplateView, "Show Sub-templates", G::Field, &[], &[], true, false, U::View),
        (C::EditSubTemplate, "Edit Sub-template", G::Field, &[], &[], true, false, U::View),
        (C::CloseSubTemplate, "Close Sub-template", G::Field, &[], &[], false, false, U::View),
        (C::CreateSubTemplate, "New Sub-template", G::Field, &[], &[], true, true, U::Step),
        (C::RenameSubTemplate, "Rename Sub-template", G::Field, &[], &[], true, true, U::Step),
        (C::RemoveSubTemplate, "Remove Sub-template", G::Field, &[], &[], true, true, U::Step),
        (C::ReplaceBody, "Replace Body", G::Field, &[], &[], true, true, U::Step),
        (C::InsertTableOfContents, "Table of Contents", G::References, &[], &[], true, true, U::Step),
        (C::InsertTableOfFigures, "Insert Table of Figures", G::References, &[], &[], true, true, U::Step),
        (C::InsertManualTableOfContents, "Manual Table", G::References, &[], &[], false, true, U::Step),
        (C::UpdateTableOfContents, "Update Table", G::References, &[], &[], true, true, U::Step),
        (C::UpdateAllTablesOfContents, "Update All Tables", G::References, &[], &[], true, true, U::Step),
        (C::RemoveTableOfContents, "Remove Table of Contents", G::References, &[], &[], true, true, U::Step),
        (C::MarkTableOfContentsEntry, "Mark Entry", G::References, &[], &[], true, true, U::Step),
        (C::InsertCaption, "Insert Caption", G::References, &[], &[], true, true, U::Step),
        (C::SetAutoCaption, "AutoCaption", G::References, &[], &[], true, false, U::View),
        (C::InsertCrossReference, "Cross-reference", G::References, &[], &[], true, true, U::Step),
        (C::InsertControl, "Content Control", G::Control, &[], &[], true, true, U::Step),
        (C::SetControl, "Control Properties", G::Control, &[], &[], true, true, U::Step),
        (C::RemoveControl, "Remove Content Control", G::Control, &[], &[], true, true, U::Step),
        (C::FillControl, "Fill Content Control", G::Control, &[], &[], true, true, U::Step),
        (C::SetCheckbox, "Check Box", G::Control, &[], &[], true, true, U::Step),
        (C::WrapInRepeatingSection, "Repeating Section", G::Control, &[], &[], true, true, U::Step),
        (C::AddRepeatingItem, "Insert Item", G::Control, &[], &[], true, true, U::Step),
        (C::RemoveElement, "Remove", G::Control, &[], &[], true, true, U::Step),
        (C::ReplaceInElements, "Replace in Fields and Controls", G::Control, &[], &[], true, true, U::Step),
        (C::ReplaceFont, "Replace Font", G::Character, &[], &[], true, true, U::Step),
        (C::MarkElements, "Mark", G::Control, &[], &[], true, false, U::View),
        (C::SetProtectedPictureControls, "Protected Picture Controls", G::Control, &[], &[], true, false, U::View),
        (C::ActivatePictureControl, "Activate Picture Control", G::Control, &[], &[], true, false, U::View),
        (C::WriteDataPart, "Write Data", G::Data, &[], &[], true, true, U::Step),
        (C::RemoveDataPart, "Remove Data", G::Data, &[], &[], true, true, U::Step),
        (C::SetTrackChanges, "Track Changes", G::Review, &["Ctrl+Shift+E"], &["⇧⌘E"], true, true, U::Step),
        (C::SetMarkupView, "Markup", G::Review, &[], &[], true, false, U::View),
        (C::SetShowMarkup, "Show Markup", G::Review, &[], &[], true, false, U::View),
        (C::SetHiddenReviewers, "Show Markup: Specific People", G::Review, &[], &[], true, false, U::View),
        (C::SetBalloonMode, "Balloons", G::Review, &[], &[], true, false, U::View),
        (C::SetMarkupStyle, "Track Changes Options", G::Review, &[], &[], true, false, U::View),
        (C::SetTrackOptions, "Track Moves/Formatting", G::Review, &[], &[], true, true, U::View),
        (C::AcceptRevision, "Accept", G::Review, &[], &[], true, true, U::Step),
        (C::RejectRevision, "Reject", G::Review, &[], &[], true, true, U::Step),
        (C::AcceptRevisionsInSelection, "Accept Selected Changes", G::Review, &[], &[], false, true, U::Step),
        (C::RejectRevisionsInSelection, "Reject Selected Changes", G::Review, &[], &[], false, true, U::Step),
        (C::AcceptAllRevisions, "Accept All Changes", G::Review, &[], &[], true, true, U::Step),
        (C::RejectAllRevisions, "Reject All Changes", G::Review, &[], &[], true, true, U::Step),
        (C::AcceptAllAndStopTracking, "Accept All Changes and Stop Tracking", G::Review, &[], &[], false, true, U::Step),
        (C::SelectRevision, "Next Change", G::Review, &[], &[], true, false, U::View),
        (C::AddComment, "New Comment", G::Review, &["Ctrl+Alt+M"], &["⌥⌘M"], true, true, U::Step),
        (C::ReplyToComment, "Reply", G::Review, &[], &[], true, true, U::Step),
        (C::EditComment, "Edit Comment", G::Review, &[], &[], true, true, U::Step),
        (C::DeleteComment, "Delete Comment", G::Review, &[], &[], true, true, U::Step),
        (C::DeleteAllComments, "Delete All Comments", G::Review, &[], &[], true, true, U::Step),
        (C::ResolveComment, "Resolve", G::Review, &[], &[], true, true, U::Step),
        (C::SetActiveComment, "Active Comment", G::Review, &[], &[], true, false, U::View),
        (C::SelectComment, "Go To Comment", G::Review, &[], &[], true, false, U::View),
        (C::SetAuthorColor, "Author Color", G::Review, &[], &[], true, false, U::View),
        (C::SetZoom, "Zoom", G::View, &[], &[], true, false, U::View),
        (C::SetViewOptions, "Show", G::View, &["Ctrl+Shift+8"], &["⌘8"], true, false, U::View),
        (C::SetMode, "Mode", G::View, &[], &[], true, false, U::View),
        (C::SetSelectObjects, "Select Objects", G::View, &[], &[], true, false, U::View),
        (C::SetRulerTabType, "Tab Type", G::Tabs, &[], &[], true, false, U::View),
        (C::ShowTabTypes, "Tab Types", G::Tabs, &[], &[], false, false, U::View),
        (C::ShowTabStop, "Tab Stop Options", G::Tabs, &[], &[], false, false, U::View),
        (C::SetAutoCorrect, "AutoCorrect Options", G::Text, &[], &[], true, false, U::View),
        (C::RevertAutoCorrect, "Undo AutoCorrect", G::Text, &[], &[], false, true, U::Step),
        (C::NeverAutoCorrect, "Stop Correcting", G::Text, &[], &[], false, true, U::Step),
        (C::ShowAutoCorrectOptions, "AutoCorrect Options Button", G::Text, &["Alt+Shift+F10"], &["⌥⇧F10"], false, false, U::View),
        (C::ClosePopover, "Close Popover", G::View, &[], &[], false, false, U::View),
        (C::DescribeFormatting, "Describe Formatting", G::View, &["Shift+F1"], &["⇧F1"], false, false, U::View),
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

/// The rulers' words: the markers' names, what hovering one says it does, and the tab types' names and descriptions.
pub(crate) mod ruler {
    use crate::api::services::TabSelector;

    pub const FIRST_LINE: &str = "First line";
    pub const HANGING: &str = "Hanging indent";
    pub const LEFT_INDENT: &str = "Left indent";
    pub const RIGHT_INDENT: &str = "Right indent";
    pub const LEFT_MARGIN: &str = "Left margin";
    pub const RIGHT_MARGIN: &str = "Right margin";
    pub const TOP_MARGIN: &str = "Top margin";
    pub const BOTTOM_MARGIN: &str = "Bottom margin";
    pub const COLUMN_WIDTH: &str = "Column width";
    pub const ROW_HEIGHT: &str = "Row height";
    /// After a tab type's name: `Left tab`.
    pub const TAB: &str = "tab";

    pub const FIRST_LINE_DOES: &str = "indents only the first line";
    pub const HANGING_DOES: &str = "where wrapped lines start";
    pub const LEFT_INDENT_DOES: &str = "moves the first line with it";
    pub const TAB_DOES: &str = "click for options, drag down to remove";
    pub const MARGIN_DOES: &str = "drag to change";
    pub const COLUMN_BORDER: &str = "Column border — drag to resize";
    pub const COLUMN_SPACING: &str = "Column spacing — drag to resize";
    pub const ROW_BORDER: &str = "Row border — drag to resize";

    pub const RELEASE_TO_REMOVE: &str = "Release to remove tab stop";
    pub const REMOVE: &str = "Remove tab stop";
    /// Before a tab type's name in lower case: `Click to add a left tab`.
    pub const CLICK_TO_ADD: &str = "Click to add a";
    /// Before an indent type's name in lower case: `Click to set the hanging indent`.
    pub const CLICK_TO_SET: &str = "Click to set the";
    /// Before the box's type: `Tab type: Left`.
    pub const TAB_TYPE: &str = "Tab type:";
    pub const TAB_TYPE_DOES: &str = "click to change";

    /// Every tab type, in the picker's order.
    pub const TAB_TYPES: [TabSelector; 7] = [
        TabSelector::Left,
        TabSelector::Center,
        TabSelector::Right,
        TabSelector::Decimal,
        TabSelector::Bar,
        TabSelector::FirstLineIndent,
        TabSelector::HangingIndent,
    ];

    /// A tab type's name and what it does.
    pub fn tab_type(t: TabSelector) -> (&'static str, &'static str) {
        match t {
            TabSelector::Left => ("Left", "Text starts at the stop"),
            TabSelector::Center => ("Center", "Text centers on the stop"),
            TabSelector::Right => ("Right", "Text ends at the stop"),
            TabSelector::Decimal => ("Decimal", "Numbers line up on the decimal point"),
            TabSelector::Bar => ("Bar", "A vertical line at the stop"),
            TabSelector::FirstLineIndent => ("First line indent", "A click sets where the first line starts"),
            TabSelector::HangingIndent => ("Hanging indent", "A click sets where wrapped lines start"),
        }
    }
}

/// The table readouts' words: what a status line names, and what a key does.
pub(crate) mod table {
    pub const COLUMN: &str = "col";
    pub const ROW: &str = "row";
    pub const TABLE: &str = "table";
    pub const CELLS: &str = "cells";
    /// Backspace deletes what is selected.
    pub const DELETE: &str = "⌫ delete";
    /// Alt held: a drag moves freely; without it, the key that does.
    pub const FREE: &str = "free";
    pub const ALT_FREE: &str = "⌥ free";
}
