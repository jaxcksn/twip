//! The command layer: one handler per [`Command`], one file per group of the command set. A handler applies its
//! command at the selection through the editor's transactions and steps, using the document operations (text,
//! formatting, tables, objects, fields, controls, review) as building blocks. The merge designer's fields, content
//! controls and data parts are handled by their operations in `editing/merge.rs`, and the view commands by the session.
//! Keys, menus, dialogs, sessions and tests all reach behaviour through [`Editor::execute`], so a command does the
//! same whichever surface runs it.

mod character;
mod history;
mod insert;
mod navigation;
mod objects;
mod page;
mod paragraph;
mod review;
mod tables;
mod tabs;
mod text;

pub(crate) use character::highlight_value;

use crate::api::command::CommandId;

/// The commands the engine doesn't do yet: never available, and refused as unsupported.
// shortcut: section breaks (saving can't add sections), the format painter, odd and even pages, and boundary moves by
// command are refused until they're built.
pub(crate) const UNSUPPORTED: &[CommandId] = &[
    CommandId::InsertSectionBreak,
    CommandId::CopyFormatting,
    CommandId::PasteFormatting,
    CommandId::SetDifferentOddEvenPages,
    CommandId::MoveColumnBoundary,
    CommandId::MoveTableColumnBoundary,
    CommandId::MoveTableRowBoundary,
];

pub use crate::api::command::Command;
use crate::api::command::CommandResult;
use crate::api::error::Error;
use crate::font::Fonts;

use super::history::Typing;
use super::selection::Selection;
use super::{Editor, extend};

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

/// The command that moves the caret (or with `extend`, extends the selection) by `by`, in the editor's units.
pub fn moving(by: MoveBy, dir: Dir, extend: bool) -> Command {
    use crate::api::command::{MoveSelection, MoveUnit as U};
    let by = match by {
        MoveBy::Char => U::Character,
        MoveBy::Word => U::Word,
        MoveBy::Line => U::Line,
        MoveBy::Para => U::Paragraph,
        MoveBy::LineEdge => U::LineEdge,
        MoveBy::Story => U::Document,
        MoveBy::Screen | MoveBy::ScreenEdge => U::Screen,
        MoveBy::Page => U::Page,
        MoveBy::RowEdge => U::RowEdge,
        MoveBy::ColumnEdge => U::ColumnEdge,
    };
    Command::MoveSelection(MoveSelection { by, direction: direction(dir), extend: extend.then_some(true) })
}

/// Backspace (`Dir::Back`) or Delete, by a character or a word.
pub fn deleting(dir: Dir, word: bool) -> Command {
    use crate::api::command::{DeleteOptions, DeleteUnit};
    Command::Delete(DeleteOptions { direction: direction(dir), unit: word.then_some(DeleteUnit::Word) })
}

fn direction(dir: Dir) -> crate::api::value::Direction {
    if dir == Dir::Forward { crate::api::value::Direction::Forward } else { crate::api::value::Direction::Backward }
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
    /// The command can't run here: the error says why, and nothing changed.
    Refused(Error),
}

impl Outcome {
    /// A command the engine doesn't do yet.
    pub(crate) fn unsupported(what: &str) -> Outcome {
        Outcome::Refused(Error::unsupported(what))
    }

    pub(crate) fn unavailable(why: &str) -> Outcome {
        Outcome::Refused(Error::unavailable(why))
    }

    pub(crate) fn invalid(why: &str) -> Outcome {
        Outcome::Refused(Error::invalid(why))
    }
}

impl Editor {
    /// Runs `cmd` at the selection. What it made (a field's id, a comment's) is [`Editor::take_made`]'s.
    pub fn execute(&mut self, cmd: Command, fonts: &Fonts) -> Outcome {
        self.made = None;
        self.refused = None;
        // Read only refuses an edit before it runs, by the catalog's `mutates`.
        if self.mode == super::Mode::ReadOnly && cmd.mutates() {
            return self.refuse_read_only();
        }
        let out = self.execute_editable(cmd, fonts);
        // Images for pictures that never went in.
        self.pending_images.clear();
        out
    }

    fn execute_editable(&mut self, cmd: Command, fonts: &Fonts) -> Outcome {
        // Typing that continues a run extends what F4 repeats.
        let continues = matches!(cmd, Command::InsertText(_))
            && self.selection.is_collapsed()
            && self.caret().and_then(|p| p.para_offset()).is_some_and(|(p, u)| self.history.continues(p, u, Typing::Insert));
        let repeatable = match &cmd {
            Command::InsertText(t) => match (&self.repeat, continues) {
                (Some(Command::InsertText(prev)), true) => Some(Command::InsertText(format!("{prev}{t}"))),
                _ => Some(cmd.clone()),
            },
            Command::InsertParagraph | Command::InsertLineBreak | Command::InsertPageBreak | Command::InsertColumnBreak | Command::InsertTab => {
                Some(cmd.clone())
            }
            _ => None,
        };
        // In extend mode a typed character extends the selection to its next occurrence.
        if self.modes.extend > 0
            && let Command::InsertText(t) = &cmd
            && t.chars().count() == 1
        {
            return self.extend_to_char(t.chars().next().unwrap_or(' '));
        }
        // In extend or block mode movement extends the selection.
        let cmd = match cmd {
            Command::MoveSelection(mut m)
                if m.extend != Some(true)
                    && self.modes != extend::Modes::default()
                    && !matches!(self.selection, Selection::Object(_) | Selection::Cells { .. }) =>
            {
                m.extend = Some(true);
                Command::MoveSelection(m)
            }
            c => c,
        };
        let moving = matches!(cmd, Command::MoveSelection(_));
        if !matches!(cmd, Command::GoBack) {
            self.back_step = None;
        }
        let out = self.run(cmd, fonts);
        // A building block that refuses answers why: the command is refused.
        let out = match (out, self.refused) {
            (Outcome::Unchanged(_), Some("read-only")) => {
                Outcome::Refused(Error::new(crate::api::error::ErrorCode::ReadOnly, "the document is read only"))
            }
            (Outcome::Unchanged(_), Some(why)) => Outcome::Refused(Error::unavailable(why)),
            (out, _) => out,
        };
        if repeatable.is_some() && matches!(out, Outcome::Changed(_)) {
            self.repeat = repeatable;
        }
        if moving && self.modes.block {
            self.selection = self.as_block(self.selection);
        }
        // An edit that leaves a caret (typing over, deleting) ends the modes; formatting keeps them.
        if matches!(out, Outcome::Changed(_)) && self.selection.is_collapsed() {
            self.modes = extend::Modes::default();
        }
        out
    }

    /// What the last command made, for the commands that create something.
    pub fn take_made(&mut self) -> Option<CommandResult> {
        self.made.take()
    }

    /// One command's handler, without the repeat and mode bookkeeping of [`Editor::execute`].
    pub(in crate::editing) fn run(&mut self, cmd: Command, fonts: &Fonts) -> Outcome {
        use crate::api::command::{DeleteUnit, MoveUnit};
        let vertical = matches!(&cmd, Command::MoveSelection(m) if matches!(m.by, MoveUnit::Line | MoveUnit::Screen));
        if !vertical {
            self.goal_x = None;
        }
        let typing = match &cmd {
            Command::InsertText(_) => true,
            Command::Delete(d) => d.unit != Some(DeleteUnit::Word),
            _ => false,
        };
        if !typing {
            self.history.break_typing();
        }
        match cmd {
            // Text
            Command::InsertText(t) => self.insert_text(&t, fonts),
            Command::InsertParagraph => self.enter(fonts),
            Command::InsertLineBreak => self.line_break(fonts),
            Command::InsertPageBreak => self.page_break(fonts),
            Command::InsertColumnBreak => self.column_break(fonts),
            Command::InsertSectionBreak(kind) => self.section_break(kind),
            Command::InsertTab => self.tab(fonts),
            Command::InsertSymbol(s) => self.symbol(s, fonts),
            Command::Delete(d) => self.delete_by(d, fonts),
            // Clipboard and history
            Command::Cut => self.cut(fonts),
            Command::Copy => self.copy_selection(),
            Command::Paste(p) => self.paste_held(p, fonts),
            Command::PasteText(t) => self.paste_text(&t, fonts),
            Command::Undo => self.undo(true, fonts),
            Command::Redo => self.undo(false, fonts),
            Command::Repeat => self.repeat_last(fonts),
            // Selection and navigation
            Command::SelectAll => self.select_all(),
            Command::Select(unit) => self.select_unit(unit),
            Command::MoveSelection(m) => self.move_selection(m, fonts),
            Command::MoveToCell(o) => self.next_cell(o == crate::api::value::Order::Previous, fonts),
            Command::SetSelectionMode(m) => self.selection_mode(m),
            Command::GoBack => self.go_back(fonts),
            Command::Escape => self.escape(),
            Command::SelectNextObject(o) => self.next_object(o),
            Command::GoToPage(page) => self.go_to_page(page, fonts),
            // Character formatting
            Command::SetBold(on) => self.set_toggle(super::format::Toggle::Bold, on, fonts),
            Command::SetItalic(on) => self.set_toggle(super::format::Toggle::Italic, on, fonts),
            Command::SetUnderline(style) => self.set_underline(style, fonts),
            Command::SetStrikethrough(on) => self.set_toggle(super::format::Toggle::Strike, on, fonts),
            Command::SetDoubleStrikethrough(on) => self.set_flag("Double Strikethrough", "dstrike", on, fonts),
            Command::SetSubscript(on) => self.set_toggle(super::format::Toggle::Subscript, on, fonts),
            Command::SetSuperscript(on) => self.set_toggle(super::format::Toggle::Superscript, on, fonts),
            Command::SetSmallCaps(on) => self.set_flag("Small Caps", "smallCaps", on, fonts),
            Command::SetAllCaps(on) => self.set_flag("All Caps", "caps", on, fonts),
            Command::SetHidden(on) => self.set_flag("Hidden", "vanish", on, fonts),
            Command::SetFont(family) => self.format(&super::format::Format::FontFamily(family), fonts),
            Command::SetFontSize(size) => self.format(&super::format::Format::FontSize(size.0), fonts),
            Command::GrowFont => self.format(&super::format::Format::GrowFont(true), fonts),
            Command::ShrinkFont => self.format(&super::format::Format::GrowFont(false), fonts),
            Command::SetFontColor(c) => self.set_font_color(c, fonts),
            Command::SetHighlight(h) => self.set_highlight(h, fonts),
            Command::SetCharacterSpacing(s) => self.set_character_spacing(s, fonts),
            Command::SetCharacterPosition(p) => self.set_character_position(p, fonts),
            Command::ClearFormatting => self.format(&super::format::Format::ClearFormatting, fonts),
            Command::ChangeCase(to) => self.change_case(to, fonts),
            Command::CopyFormatting => self.copy_formatting(),
            Command::PasteFormatting => self.paste_formatting(fonts),
            // Paragraph formatting
            Command::SetAlignment(a) => self.set_alignment(a, fonts),
            Command::SetLineSpacing(s) => self.set_line_spacing(s, fonts),
            Command::SetParagraphSpacing(s) => self.set_paragraph_spacing(s, fonts),
            Command::SetIndents(i) => self.set_indents(i, fonts),
            Command::IncreaseIndent => self.format(&super::format::Format::Indent(true), fonts),
            Command::DecreaseIndent => self.format(&super::format::Format::Indent(false), fonts),
            Command::SetParagraphStyle(id) => self.set_paragraph_style(id, fonts),
            Command::SetList(kind) => self.set_list(kind, fonts),
            Command::SetListLevel(c) => self.list_level(c == crate::api::command::LevelChange::Decrease, fonts),
            Command::SetParagraphFlow(f) => self.set_paragraph_flow(f, fonts),
            Command::SetParagraphBorders(b) => self.set_paragraph_borders(b, fonts),
            Command::SetParagraphShading(c) => self.format(&super::format::Format::ParaShading(c.map(|c| c.0)), fonts),
            // Tab stops
            Command::AddTabStop(a) => self.add_tab_stop(a, fonts),
            Command::MoveTabStop(m) => self.move_tab_stop(m, fonts),
            Command::DeleteTabStop(at) => self.delete_tab_stop(at, fonts),
            Command::ClearTabStops => self.clear_tab_stops(fonts),
            Command::SetDefaultTabStop(v) => self.set_default_tab_stop(v, fonts),
            // Page layout, headers and footers
            Command::SetMargins(m) => self.set_margins(m, fonts),
            Command::SetOrientation(o) => {
                self.section_cmd(super::section::SectionCmd::Orientation(o == crate::api::value::Orientation::Landscape), fonts)
            }
            Command::SetPaperSize(s) => self.section_cmd(super::section::SectionCmd::Size(s.width.0, s.height.0), fonts),
            Command::SetColumns(c) => self.set_columns(c, fonts),
            Command::SetHeaderFooterDistance(d) => self.set_header_footer_distance(d, fonts),
            Command::SetDifferentFirstPage(on) => self.set_page("Different First Page", |p| p.title_pg = on, fonts),
            Command::SetDifferentOddEvenPages(_) => Outcome::unsupported("different odd and even pages"),
            Command::MoveColumnBoundary(_) => Outcome::unsupported("moving a text column boundary"),
            Command::EditHeader => self.edit_header_footer(None, false, fonts),
            Command::EditFooter => self.edit_header_footer(None, true, fonts),
            Command::CloseHeaderFooter => self.close_header_footer(),
            Command::InsertPageNumber(p) => self.insert_page_number(p, fonts),
            // Insert
            Command::InsertTable(t) => self.insert_table(t.rows as usize, t.columns as usize, fonts),
            Command::InsertPicture(p) => self.insert_picture_with(p, fonts),
            Command::InsertTextBox => self.insert_text_box(fonts),
            Command::InsertFootnote => self.insert_note(crate::docx::NoteKind::Foot, fonts),
            Command::InsertEndnote => self.insert_note(crate::docx::NoteKind::End, fonts),
            Command::InsertHyperlink(h) => self.insert_hyperlink(h, fonts),
            Command::EditHyperlink(h) => self.edit_hyperlink(h, fonts),
            Command::RemoveHyperlink => self.remove_hyperlink(fonts),
            Command::InsertField(f) => self.insert_field(f, fonts),
            // Tables
            Command::InsertRows(r) => self.insert_rows(r.side == crate::api::command::VerticalSide::Below, r.count.unwrap_or(1) as usize, fonts),
            Command::InsertColumns(c) => self.insert_columns_by(c, fonts),
            Command::DeleteRows => self.delete_rows(fonts),
            Command::DeleteColumns => self.delete_columns(fonts),
            Command::DeleteTable => self.delete_table(fonts),
            Command::DeleteCells(shift) => self.delete_cells_cmd(shift == crate::api::command::CellShift::Up, fonts),
            Command::MergeCells => self.merge_cells(fonts),
            Command::SplitCells(s) => self.split_cells(s, fonts),
            Command::SplitTable => self.split_table(fonts),
            Command::DistributeRows => self.distribute_rows(fonts),
            Command::DistributeColumns => self.distribute_columns(fonts),
            Command::MoveRows(d) => self.move_rows(d == crate::api::command::VerticalDirection::Up, fonts),
            Command::AutoFitColumns(to) => self.auto_fit_columns(to, fonts),
            Command::SetCellAlignment(a) => self.set_cell_alignment(a, fonts),
            Command::SetCellShading(c) => self.table_format(&super::table_format::TableFormat::CellShading(c.map(|c| c.0)), fonts),
            Command::SetCellBorders(b) => self.set_cell_borders(b, fonts),
            Command::SetCellMargins(m) => self.set_cell_margins(m, fonts),
            Command::SetHeaderRow(on) => self.set_header_row(on, fonts),
            Command::SetRowHeight(h) => self.set_row_height(h, fonts),
            Command::SetRowCanBreak(on) => self.set_row_can_break(on, fonts),
            Command::SetColumnWidth(w) => self.table_format(&super::table_format::TableFormat::ColumnWidth(w.0), fonts),
            Command::MoveTableColumnBoundary(_) | Command::MoveTableRowBoundary(_) => Outcome::unsupported("moving a table boundary by command"),
            Command::SetTableWidth(w) => self.set_table_width(w, fonts),
            Command::SetTableAlignment(a) => self.set_table_alignment(a, fonts),
            Command::SetTableIndent(i) => self.table_format(&super::table_format::TableFormat::TableIndent(i.0), fonts),
            Command::SetTableStyle(id) => self.set_table_style(id, fonts),
            Command::SetTableStyleOptions(o) => self.set_table_style_options(o, fonts),
            // Pictures and text boxes
            Command::SetObjectSize(s) => self.set_object_size(s, fonts),
            Command::ResetObjectSize => self.object_cmd(super::objects::ObjectCmd::ResetSize, fonts),
            Command::SetObjectWrap(w) => self.set_object_wrap(w, fonts),
            Command::SetAnchorLocked(on) => self.set_anchor_locked(on, fonts),
            Command::SetAltText(a) => self.alt_text(a, fonts),
            Command::DeleteObject => self.delete_selected_object(fonts),
            // Fields
            Command::SetField(f) => self.set_field(f, fonts),
            Command::SetFieldResults(r) => self.set_field_results(r, fonts),
            Command::UpdateFields(ids) => self.update_fields(ids.ids.as_deref(), fonts),
            Command::SetFieldsLocked(l) => self.lock_fields(l.locked, l.ids.as_deref(), fonts),
            Command::UnlinkFields(ids) => self.unlink_fields(ids.ids.as_deref(), fonts),
            Command::ShowFieldCodes(s) => self.show_field_codes(s.show, s.ids.as_deref()),
            Command::SetFieldLabels(l) => self.set_field_labels(l),
            // Content controls and regions
            Command::InsertControl(c) => self.insert_control(c, fonts),
            Command::SetControl(c) => self.set_control(c, fonts),
            Command::RemoveControl(r) => self.remove_element(&r.id, r.keep_content.unwrap_or(false), fonts),
            Command::FillControl(f) => self.fill_control(f, fonts),
            Command::SetCheckbox(c) => self.set_checkbox(c, fonts),
            Command::WrapInRepeatingSection(r) => self.wrap_in_repeating_section(r, fonts),
            Command::RemoveElement(id) => self.remove_element(&id, false, fonts),
            Command::ReplaceInElements(r) => self.replace_in_elements(r, fonts),
            Command::MarkElements(m) => self.mark_elements(m),
            Command::SetPlaceholderLooks(l) => self.set_placeholder_looks(l),
            Command::FillPlaceholder(f) => self.fill_placeholders(f.key.as_deref(), f.image.0, fonts),
            // Data parts
            Command::WriteDataPart(w) => self.write_data_part(w, fonts),
            Command::RemoveDataPart(r) => self.remove_data_part(r, fonts),
            // Review
            Command::SetTrackChanges(on) => self.set_track_changes(on),
            Command::SetMarkupView(v) => self.set_markup_view(v),
            Command::AcceptRevision(id) => self.resolve_one(true, &id, fonts),
            Command::RejectRevision(id) => self.resolve_one(false, &id, fonts),
            Command::AcceptRevisionsInSelection => self.resolve_revisions(true, &super::revisions::resolve::Scope::Selection, fonts),
            Command::RejectRevisionsInSelection => self.resolve_revisions(false, &super::revisions::resolve::Scope::Selection, fonts),
            Command::AcceptAllRevisions(a) => self.resolve_all(true, a, fonts),
            Command::RejectAllRevisions(a) => self.resolve_all(false, a, fonts),
            Command::SelectRevision(o) => self.go_to_revision(o == crate::api::value::Order::Previous, fonts),
            Command::AddComment(body) => self.add_comment_cmd(&body, fonts),
            Command::ReplyToComment(r) => self.reply_to_comment(r, fonts),
            Command::EditComment(e) => self.edit_comment_cmd(e, fonts),
            Command::DeleteComment(id) => self.delete_comment_cmd(&id, fonts),
            Command::ResolveComment(r) => self.resolve_comment_cmd(r, fonts),
            Command::SetActiveComment(id) => self.set_active_comment(id.as_ref(), fonts),
            Command::SelectComment(id) => self.select_comment(&id, fonts),
            Command::SetAuthorColor(a) => self.set_author_color_by_name(&a.author, a.color.0),
            // View
            Command::SetZoom(_) | Command::SetViewOptions(_) => Outcome::Moved,
            Command::SetMode(m) => self.set_mode(match m {
                crate::api::state::Mode::Editing => super::Mode::Editing,
                crate::api::state::Mode::Design => super::Mode::Design,
            }),
        }
    }

    /// Runs `f`, several building blocks, as one transaction labelled `label`: undone when `f` refuses.
    pub fn joined(&mut self, label: &'static str, fonts: &Fonts, f: impl FnOnce(&mut Editor) -> Outcome) -> Outcome {
        self.begin_batch(Some(label.into()));
        let out = f(self);
        let end = self.end_batch(!matches!(out, Outcome::Refused(_)), fonts);
        if matches!(end, Outcome::Changed(_)) { end } else { out }
    }
}
