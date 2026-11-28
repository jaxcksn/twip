//! Command layer

pub(crate) mod bounds;
mod character;
mod history;
mod insert;
mod navigation;
mod objects;
mod page;
mod paragraph;
pub mod references;
mod review;
mod tables;
pub(in crate::editing) mod tabs;
mod text;

pub use crate::api::command::BreakClear;
pub use character::{LigatureFlags, OpenTypeChange};
pub(crate) use character::{THEME_COLOR, THEME_FILL, colour_attrs, highlight, highlight_color, highlight_value, rgb_value, underline, with_attr};
pub use insert::{LinkHere, LinkParts};
pub use page::{MarginSide, PageNumberAlign, PageNumberSpot, page_number_sample};
pub use tables::RowExceptionsEdit;

use crate::api::command::CommandId;

pub(crate) const UNSUPPORTED: &[CommandId] = &[];

pub use crate::api::command::Command;
use crate::api::command::CommandResult;
use crate::api::error::Error;
use crate::font::Fonts;

use super::history::Typing;
use super::selection::Selection;
use super::{Editor, extend};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveBy {
    Char,
    Word,
    Line,
    Para,
    LineEdge,
    Story,
    Screen,
    Page,
    ScreenEdge,
    RowEdge,
    ColumnEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Back,
    Forward,
}

pub fn moving(by: MoveBy, dir: Dir, extend: bool) -> Command {
    use crate::api::command::{MoveSelection, MoveUnit as U};
    let by = match by {
        MoveBy::Char => U::Character,
        MoveBy::Word => U::Word,
        MoveBy::Line => U::Line,
        MoveBy::Para => U::Paragraph,
        MoveBy::LineEdge => U::LineEdge,
        MoveBy::Story => U::Document,
        MoveBy::Screen => U::Screen,
        MoveBy::ScreenEdge => U::WindowEdge,
        MoveBy::Page => U::Page,
        MoveBy::RowEdge => U::RowEdge,
        MoveBy::ColumnEdge => U::ColumnEdge,
    };
    Command::MoveSelection(MoveSelection { by, direction: direction(dir), extend: extend.then_some(true) })
}

pub fn deleting(dir: Dir, word: bool) -> Command {
    use crate::api::command::{DeleteOptions, DeleteUnit};
    Command::Delete(DeleteOptions { direction: direction(dir), unit: word.then_some(DeleteUnit::Word) })
}

fn direction(dir: Dir) -> crate::api::value::Direction {
    if dir == Dir::Forward { crate::api::value::Direction::Forward } else { crate::api::value::Direction::Backward }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Changed(u64),
    Moved,
    Unchanged(&'static str),
    Refused(Error),
}

impl Outcome {
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

/// Whether Repeat runs `c` again: an edit of the document, undo, redo and protection (its password) aside.
fn repeats(c: &Command) -> bool {
    use crate::api::catalog::{CommandGroup, UndoGrouping, info};
    !matches!(c, Command::SetDocumentProtection(_))
        && info(c.id()).is_some_and(|i| i.mutates && i.undo != UndoGrouping::View && i.group != CommandGroup::History)
}

impl Editor {
    pub fn execute(&mut self, cmd: Command, fonts: &Fonts) -> Outcome {
        self.sync_multi();
        self.set_aside = None;
        self.made = None;
        self.refused = None;
        if self.mode == super::Mode::ReadOnly && cmd.mutates() {
            return self.refuse_read_only();
        }
        // F2's Enter is checked where it drops the text, not at the selection.
        let move_to_enter = self.move_to.is_some() && matches!(cmd, Command::InsertParagraph);
        if cmd.mutates()
            && !move_to_enter
            && let Some(e) = self.protection_refusal(&cmd)
        {
            return Outcome::Refused(e);
        }
        if let Err(e) = bounds::bounds(&cmd) {
            return Outcome::Refused(e);
        }
        if self.is_discontiguous() && super::multi::single_only(cmd.id()) {
            return Outcome::unavailable("not available for multiple selections");
        }
        let typed = match &cmd {
            Command::InsertText(t) => t.chars().count(),
            _ => 0,
        };
        let revision = self.revision();
        let out = self.execute_editable(cmd, fonts);
        self.pending_images = Default::default();
        self.settle_provisional(fonts);
        // The correction mark stays through typing after it and caret moves onto it; anything else drops it.
        self.follow_correction(typed, self.revision() != revision);
        out
    }

    fn execute_editable(&mut self, cmd: Command, fonts: &Fonts) -> Outcome {
        // Columns where the Columns dialog applies them
        let mut cmd = match cmd {
            Command::SetColumns(mut c) if c.apply_to.is_some() || c.start_new_column.is_some() => {
                use super::section::ColumnsScope as S;
                use crate::api::command::ColumnsApplyTo as A;
                let scope = match c.apply_to.take().unwrap_or_default() {
                    A::Section => S::Sections,
                    A::WholeDocument => S::WholeDocument,
                    A::ThisPointForward => S::ThisPointForward,
                    A::SelectedText => S::SelectedText,
                };
                let new_column = c.start_new_column.take().unwrap_or(false);
                return self.set_columns_in(c, scope, new_column, fonts);
            }
            cmd => cmd,
        };
        // Page setup for every section or from the selection on: the command for those sections, in one step.
        if let Some(scope) = cmd.take_apply_to() {
            let label = crate::api::catalog::info(cmd.id()).map_or("Page Setup", |i| i.label.as_str());
            return self.in_section_scope(scope, label, |e| e.execute_editable(cmd, fonts), fonts);
        }
        if let Some(out) = self.move_to_command(&cmd, fonts) {
            return out;
        }
        if let Some(out) = self.list_label_command(&cmd, fonts) {
            // Repeat doesn't format a list's number
            if matches!(out, Outcome::Changed(_)) {
                self.repeat = None;
            }
            return out;
        }
        // A built-in style the document doesn't define yet joins it first, in the same undo step.
        if let Some(b) = self.built_in_named(&cmd) {
            if matches!(cmd, Command::SetTableStyle(_)) && self.table_target().is_none() {
                return Outcome::Unchanged("not in a table");
            }
            let label = crate::api::catalog::info(cmd.id()).map_or("Style", |i| i.label.as_str());
            let pending = self.pending.clone();
            self.begin_batch(Some(label.into()));
            let out = match self.add_built_in(b, fonts) {
                o @ Outcome::Refused(_) => o,
                _ if self.built_in_named(&cmd).is_some() => Outcome::Unchanged("the style couldn't be added"),
                _ => self.execute_editable(cmd, fonts),
            };
            let applied = matches!(out, Outcome::Changed(_) | Outcome::Moved)
                || match (&self.pending, &pending) {
                    (Some(a), Some(b)) => !a.layout_eq(b),
                    (a, b) => a.is_some() != b.is_some(),
                };
            let end = self.end_batch(applied, fonts);
            return if matches!(end, Outcome::Changed(_)) { end } else { out };
        }
        let typing = match &cmd {
            Command::InsertText(_) | Command::InsertTab => Some(Typing::Insert),
            Command::Delete(d) if d.unit != Some(crate::api::command::DeleteUnit::Word) => {
                Some(if d.direction == crate::api::value::Direction::Backward { Typing::Backspace } else { Typing::Delete })
            }
            _ => None,
        };
        let continues = typing.is_some_and(|kind| {
            self.selection.is_collapsed() && self.caret().and_then(|p| p.para_offset()).is_some_and(|(p, u)| self.history.continues(p, u, kind))
        });
        // Typing anywhere but where the last typing left the caret starts an undo step, however the caret got there.
        if typing.is_some() && !continues {
            self.history.break_typing();
        }
        let repeatable = match &cmd {
            Command::InsertText(t) => match (&self.repeat, continues) {
                (Some(Command::InsertText(prev)), true) => Some(Command::InsertText(format!("{prev}{t}"))),
                _ => Some(cmd.clone()),
            },
            c if repeats(c) => Some(c.clone()),
            _ => None,
        };
        if self.modes.extend > 0
            && let Command::InsertText(t) = &cmd
            && t.chars().count() == 1
        {
            return self.extend_to_char(t.chars().next().unwrap_or(' '));
        }
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
        // Any navigation ends a number selection, the caret moved or not.
        if moving {
            self.list_label = None;
        }
        if !matches!(cmd, Command::GoBack) {
            self.back_step = None;
        }
        let out = self.run(cmd, fonts);
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
        if matches!(out, Outcome::Changed(_)) && self.selection.is_collapsed() {
            self.modes = extend::Modes::default();
        }
        out
    }

    pub fn take_made(&mut self) -> Option<CommandResult> {
        self.made.take()
    }

    pub(in crate::editing) fn run(&mut self, cmd: Command, fonts: &Fonts) -> Outcome {
        use crate::api::command::{DeleteUnit, MoveUnit};
        let vertical = matches!(&cmd, Command::MoveSelection(m) if matches!(m.by, MoveUnit::Line | MoveUnit::Screen | MoveUnit::WindowEdge));
        if !vertical {
            self.goal = None;
        }
        let typing = match &cmd {
            Command::InsertText(_) | Command::InsertTab => true,
            Command::Delete(d) => d.unit != Some(DeleteUnit::Word),
            _ => false,
        };
        if !typing {
            self.history.break_typing();
        }
        // An edit by any other command puts the table tool away
        if self.table_tool() != crate::api::command::TableTool::None
            && !matches!(cmd, Command::SetTableTool(_) | Command::SetTablePen(_) | Command::DrawTableStroke(_) | Command::EraseTableBorder(_))
            && crate::api::catalog::info(cmd.id()).is_some_and(|i| {
                i.mutates && i.undo != crate::api::catalog::UndoGrouping::View && i.group != crate::api::catalog::CommandGroup::History
            })
        {
            self.set_table_tool(crate::api::command::TableTool::None);
        }
        match cmd {
            // Text
            Command::InsertText(t) => self.insert_text(&t, fonts),
            Command::InsertParagraph => self.enter(fonts),
            Command::InsertLineBreak(b) => self.line_break_clearing(b.clear, fonts),
            Command::InsertPageBreak => self.page_break(fonts),
            Command::InsertColumnBreak => self.column_break(fonts),
            Command::InsertSectionBreak(kind) => self.section_break(kind, fonts),
            Command::InsertStyleSeparator => self.insert_style_separator(fonts),
            Command::InsertTab => self.tab(fonts),
            Command::InsertSymbol(s) => self.symbol(s, fonts),
            Command::ToggleCharacterCode => self.toggle_character_code(fonts),
            Command::Delete(d) => self.delete_by(d, fonts),
            // Clipboard and history
            Command::Cut => self.cut(fonts),
            Command::Copy => self.copy_selection(fonts),
            Command::Paste(p) => self.paste_held(p, fonts),
            Command::PasteText(t) => self.paste_text(&t, fonts),
            Command::Repaste(choice) => self.repaste_as(choice, fonts),
            Command::DismissPasteOptions => {
                self.dismiss_paste_options();
                Outcome::Moved
            }
            Command::SetPasteOptions(c) => self.set_paste_options(&c),
            Command::InsertFile(f) => self.insert_file(&f.from.0, fonts),
            Command::MoveText => self.begin_move_to(false),
            Command::CopyText => self.begin_move_to(true),
            Command::Undo(n) => self.undo_many(true, n.count.unwrap_or(1) as usize, fonts),
            Command::Redo(n) => self.undo_many(false, n.count.unwrap_or(1) as usize, fonts),
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
            Command::SelectField(o) => self.select_field(o, fonts),
            Command::SetSelection(s) => self.set_selection_pieces(&s),
            Command::ShrinkSelection => self.shrink_selection(),
            Command::SelectSimilarFormatting => self.select_similar_formatting(),
            Command::SelectStyleInstances(id) => self.select_style_instances(&id.0),
            Command::GoToPage(page) => self.go_to_page(page, fonts),
            Command::GoToBookmark(name) => self.go_to_bookmark(&name, true, fonts),
            // Nothing to go to is unavailable, as Word's Go To says so.
            Command::GoTo(g) => match self.go_to(g.what.into(), g.target.into(), g.name.as_deref(), fonts) {
                Outcome::Unchanged(why) => Outcome::unavailable(why),
                out => out,
            },
            // Character formatting
            Command::SetBold(on) => self.set_toggle(super::format::Toggle::Bold, on, fonts),
            Command::SetItalic(on) => self.set_toggle(super::format::Toggle::Italic, on, fonts),
            Command::SetUnderline(style) => self.set_underline(style, fonts),
            Command::SetStrikethrough(on) => self.set_toggle(super::format::Toggle::Strike, on, fonts),
            Command::SetDoubleStrikethrough(on) => self.set_flag("Double Strikethrough", character::Flag::DoubleStrike, on, fonts),
            Command::SetSubscript(on) => self.set_toggle(super::format::Toggle::Subscript, on, fonts),
            Command::SetSuperscript(on) => self.set_toggle(super::format::Toggle::Superscript, on, fonts),
            Command::SetSmallCaps(on) => self.set_flag("Small Caps", character::Flag::Toggle(crate::format::Toggle::SMALL_CAPS), on, fonts),
            Command::SetAllCaps(on) => self.set_flag("All Caps", character::Flag::Toggle(crate::format::Toggle::CAPS), on, fonts),
            Command::SetHidden(on) => self.set_flag("Hidden", character::Flag::Toggle(crate::format::Toggle::HIDDEN), on, fonts),
            Command::SetFont(crate::api::command::FontChoice::Name(family)) => self.format(&super::format::Format::FontFamily(family), fonts),
            Command::SetFont(crate::api::command::FontChoice::Theme { theme }) => self.set_theme_font(
                match theme {
                    crate::api::value::ThemeFont::Headings => crate::docx::ThemeFont::Major,
                    crate::api::value::ThemeFont::Body => crate::docx::ThemeFont::Minor,
                },
                fonts,
            ),
            Command::SetFontSize(size) => self.format(&super::format::Format::FontSize(size.0), fonts),
            Command::GrowFont => self.format(&super::format::Format::GrowFont(true), fonts),
            Command::ShrinkFont => self.format(&super::format::Format::GrowFont(false), fonts),
            Command::GrowFontOnePoint => self.step_font_size(true, fonts),
            Command::ShrinkFontOnePoint => self.step_font_size(false, fonts),
            Command::SetFontColor(c) => self.set_font_colour(c, fonts),
            // With nothing selected, Highlight turns the pointer into the highlighter, as Word's button does.
            Command::SetHighlight(h) if matches!(self.selection, Selection::Caret(_)) && !self.is_discontiguous() => self.toggle_highlighter(h),
            Command::SetHighlight(h) => self.set_highlight(h, fonts),
            Command::SetCharacterSpacing(s) => self.set_character_spacing(s, fonts),
            Command::SetCharacterPosition(p) => self.set_character_position(p, fonts),
            Command::ApplyCharacterStyle(id) => self.apply_character_style(id, fonts),
            Command::SetTypography(mut t) => {
                let more = OpenTypeChange { ligatures: t.ligature_flags.take(), stylistic_sets: t.stylistic_sets.take() };
                self.set_typography_with(t, &more, fonts)
            }
            Command::SetFitText(w) => self.set_fit_text(w, fonts),
            Command::SetLanguage(l) => self.set_language(l, fonts),
            Command::SetCharacterEffects(e) => self.set_character_effects(e, fonts),
            Command::SetTextEffects(e) => self.set_text_effects(e, fonts),
            Command::SetUnderlineColor(c) => self.set_underline_colour(c, fonts),
            Command::ClearFormatting => self.format(&super::format::Format::ClearFormatting, fonts),
            Command::ClearAllFormatting => self.clear_all_formatting(fonts),
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
            Command::IncreaseHangingIndent => self.hanging_indent(true, fonts),
            Command::DecreaseHangingIndent => self.hanging_indent(false, fonts),
            Command::ClearParagraphFormatting => self.clear_paragraph_formatting(fonts),
            Command::MoveParagraphs(d) => self.move_paragraphs(d == crate::api::command::VerticalDirection::Up, fonts),
            Command::SetParagraphStyle(id) => self.set_paragraph_style(id, fonts),
            Command::UpdateStyleToSelection(id) => self.update_style_to_selection(&id.0, fonts),
            Command::DeleteStyle(id) => self.delete_style(&id.0, fonts),
            Command::SetParagraphBorderLines(b) => self.set_paragraph_border_lines(b, fonts),
            Command::SetShading(s) => self.set_shading(s, fonts),
            Command::SetTextBorder(b) => self.set_text_border(b, fonts),
            Command::SetPageBorders(b) => self.set_page_borders(b, fonts),
            Command::SetFrame(p) => {
                let allow = p.allow_overlap;
                self.set_frame_with_overlap(p, allow, fonts)
            }
            Command::RemoveFrame => self.remove_frame(fonts),
            Command::SetDropCap(d) => self.set_drop_cap(d, fonts),
            Command::SetStyleMeta(m) => self.set_style_meta(m, fonts),
            Command::SetDocumentDefaults(d) => self.set_document_defaults(d, fonts),
            Command::SetDefaultFormatting(c) => self.set_default_formatting(c, fonts),
            Command::CreateStyle(n) => {
                let (out, id) = self.create_style(n, fonts);
                if matches!(out, Outcome::Changed(_)) {
                    self.made = id.map(|style_id| CommandResult::Style { style_id });
                }
                out
            }
            Command::ModifyStyle(m) => self.modify_style(&m.id, m.change, fonts),
            Command::SetList(kind) => self.set_list(kind, fonts),
            Command::SetListDefinition(d) => self.set_list_definition(d, fonts),
            Command::SetNumberingValue(v) => self.set_numbering_value(v, fonts),
            Command::RestartNumbering => self.set_numbering_value(
                crate::api::command::NumberingValue { mode: crate::api::command::NumberingMode::StartNew, value: 0, advance: false },
                fonts,
            ),
            Command::ContinueNumbering => self.set_numbering_value(
                crate::api::command::NumberingValue { mode: crate::api::command::NumberingMode::Continue, value: 0, advance: false },
                fonts,
            ),
            Command::AdjustListIndents(i) => self.adjust_list_indents(i, fonts),
            Command::ApplyListEntry(id) => self.apply_list_entry(&id, fonts),
            Command::SetListLevel(c) => self.list_level(c == crate::api::command::LevelChange::Decrease, fonts),
            Command::SetParagraphFlow(f) => self.set_paragraph_flow(f, fonts),
            Command::SetOutline(o) => self.set_outline(o, fonts),
            Command::PromoteOutline(o) => self.change_outline(super::outline::OutlineChange::Promote, o.subtopics.unwrap_or(false), fonts),
            Command::DemoteOutline(o) => self.change_outline(super::outline::OutlineChange::Demote, o.subtopics.unwrap_or(false), fonts),
            Command::PromoteToHeading1(o) => self.change_outline(super::outline::OutlineChange::Heading1, o.subtopics.unwrap_or(false), fonts),
            Command::DemoteToBodyText => self.change_outline(super::outline::OutlineChange::BodyText, false, fonts),
            Command::PromoteHeading(h) => self
                .heading_para(&h.heading)
                .map_or_else(|o| o, |p| self.change_heading_outline(p, super::outline::OutlineChange::Promote, h.subtopics.unwrap_or(true), fonts)),
            Command::DemoteHeading(h) => self
                .heading_para(&h.heading)
                .map_or_else(|o| o, |p| self.change_heading_outline(p, super::outline::OutlineChange::Demote, h.subtopics.unwrap_or(true), fonts)),
            Command::MoveHeading(m) => match (self.heading_para(&m.heading), self.heading_para(&m.target)) {
                (Ok(h), Ok(t)) => {
                    let side = if m.side == crate::api::command::HeadingSide::Before {
                        super::outline::Beside::Before
                    } else {
                        super::outline::Beside::After
                    };
                    self.move_heading(h, t, side, m.level, fonts)
                }
                (Err(o), _) | (_, Err(o)) => o,
            },
            Command::DeleteHeading(h) => match self.heading_para(&h) {
                Ok(p) => self.delete_heading(p, fonts),
                Err(o) => o,
            },
            Command::InsertHeadingBefore(h) => {
                self.heading_para(&h).map_or_else(|o| o, |p| self.insert_heading(p, super::outline::NewHeading::Before, fonts))
            }
            Command::InsertHeadingAfter(h) => {
                self.heading_para(&h).map_or_else(|o| o, |p| self.insert_heading(p, super::outline::NewHeading::After, fonts))
            }
            Command::InsertSubheading(h) => {
                self.heading_para(&h).map_or_else(|o| o, |p| self.insert_heading(p, super::outline::NewHeading::Subheading, fonts))
            }
            Command::SelectHeadingContent(h) => self.heading_para(&h).map_or_else(|o| o, |p| self.select_heading_content(p)),
            Command::SetParagraphTextDirection(d) => self.set_paragraph_text_direction(d, fonts),
            Command::SetParagraphBorders(b) => self.set_paragraph_borders(b, fonts),
            Command::SetParagraphShading(c) => match c.map(|c| self.word_colour(&c)).transpose() {
                Ok(c) => self.format(&super::format::Format::ParaShading(c), fonts),
                Err(why) => Outcome::invalid(&why),
            },
            // Tab stops
            Command::AddTabStop(a) => self.add_tab_stop(a, fonts),
            Command::MoveTabStop(m) => self.move_tab_stop(m, fonts),
            Command::DeleteTabStop(at) => self.delete_tab_stop(at, fonts),
            Command::ClearTabStops => self.clear_tab_stops(fonts),
            Command::SetDefaultTabStop(v) => self.set_default_tab_stop(v, fonts),
            Command::SetAutoHyphenation(on) => self.set_hyphenation_options("Automatic Hyphenation", |h| h.auto = on, fonts),
            Command::SetHyphenationZone(z) if !(0.0..=1584.0).contains(&z.0) => Outcome::invalid("the zone must be from 0 to 1584"),
            Command::SetHyphenationZone(z) => self.set_hyphenation_options("Hyphenation Zone", |h| h.zone = (z.0 * 20.0).round() / 20.0, fonts),
            Command::SetConsecutiveHyphenLimit(n) if n > 32767 => Outcome::invalid("the limit must be from 0 to 32767"),
            Command::SetConsecutiveHyphenLimit(n) => self.set_hyphenation_options("Limit Consecutive Hyphens", |h| h.limit = i64::from(n), fonts),
            Command::SetHyphenateCaps(on) => self.set_hyphenation_options("Hyphenate Words in CAPS", |h| h.caps = !on, fonts),
            // Page layout, headers and footers
            Command::SetMargins(m) => self.set_margins(m, fonts),
            Command::SetOrientation(o) => {
                self.section_cmd(super::section::SectionCmd::Orientation(o.value == crate::api::value::Orientation::Landscape), fonts)
            }
            Command::SetPaperSize(s) => self.section_cmd(super::section::SectionCmd::Size(s.width.0, s.height.0), fonts),
            Command::SetColumns(c) => self.set_columns(c, fonts),
            Command::SetHeaderFooterDistance(d) => self.set_header_footer_distance(d, fonts),
            Command::SetDifferentFirstPage(on) => self.set_page("Different First Page", |p| p.title_pg = on, fonts),
            Command::SetDifferentOddEvenPages(on) => self.set_page_settings("Different Odd and Even Pages", |p| p.even_and_odd = on, fonts),
            Command::SetPageNumbering(n) => self.set_page_numbering(n, fonts),
            Command::SetPageVerticalAlignment(a) => self.set_page_vertical_alignment(a.value, fonts),
            Command::SetLineNumbering(l) => self.set_line_numbering(l.value, fonts),
            Command::SetSectionBreakType(k) => self.set_section_break_type(k.value, fonts),
            Command::SetSectionTextDirection(d) => self.set_section_text_direction(d.value, fonts),
            Command::SetMultiplePages(m) => self.set_multiple_pages(m, fonts),
            Command::SetHeaderFooterLinked(l) => self.set_header_footer_linked(l, fonts),
            Command::RemoveHeaderFooter(h) => self.remove_header_footer(h, fonts),
            Command::InsertAlignmentTab(t) => self.insert_alignment_tab(t, fonts),
            Command::SetPageColor(v) => self.set_page_color(v, fonts),
            Command::SetWatermark(w) => self.set_watermark(w, fonts),
            Command::RemoveWatermark => self.remove_watermark(fonts),
            Command::MoveColumnBoundary(m) => self.move_column_boundary(m.column as usize, m.to.0, fonts),
            Command::EditHeader => self.edit_header_footer(None, false, fonts),
            Command::EditFooter => self.edit_header_footer(None, true, fonts),
            Command::CloseHeaderFooter => self.close_header_footer(),
            Command::InsertPageNumber(p) => self.insert_page_number(p, fonts),
            // Insert
            Command::InsertTable(t) => self.insert_table_captioned(t.rows as usize, t.columns as usize, fonts),
            Command::InsertPicture(p) => self.insert_picture_with(p, fonts),
            Command::InsertTextBox => self.insert_text_box(fonts),
            Command::InsertFootnote => self.insert_note(crate::docx::NoteKind::Foot, None, fonts),
            Command::InsertEndnote => self.insert_note(crate::docx::NoteKind::End, None, fonts),
            Command::InsertNote(n) => {
                let kind = if n.kind == crate::api::command::NoteType::Footnote { crate::docx::NoteKind::Foot } else { crate::docx::NoteKind::End };
                self.insert_note(kind, n.custom_mark.as_deref(), fonts)
            }
            Command::SetNoteProperties(p) => self.set_note_properties(p, fonts),
            Command::ConvertNotes(c) => self.convert_notes(c, fonts),
            Command::SetNoteSeparator(c) => {
                let kind = if c.kind == crate::api::command::NoteType::Footnote { crate::docx::NoteKind::Foot } else { crate::docx::NoteKind::End };
                let part = match c.part {
                    crate::api::value::NoteSeparatorPart::Separator => crate::docx::NoteSeparator::Separator,
                    crate::api::value::NoteSeparatorPart::ContinuationSeparator => crate::docx::NoteSeparator::Continuation,
                    crate::api::value::NoteSeparatorPart::ContinuationNotice => crate::docx::NoteSeparator::Notice,
                };
                self.set_note_separator(kind, part, c.text.as_deref(), fonts)
            }
            Command::InsertHyperlink(h) => self.insert_hyperlink(h, fonts),
            Command::InsertBookmark(name) => self.insert_bookmark(&name, fonts),
            Command::SetDocumentProtection(p) => self.document_protection(p, fonts),
            Command::SetEditableRangeEditor(r) => self.set_range_editor(super::protection::RangeEditor::from(r.editor), r.allowed, fonts),
            Command::SelectNextEditableRange(o) => self.select_next_editable_range(o == crate::api::value::Order::Next),
            Command::SelectEditableRanges => self.select_all_editable_ranges(),
            Command::SetEditingGroups(groups) => {
                self.set_editing_groups(groups);
                Outcome::Moved
            }
            Command::SelectNextFormField(o) => self.select_next_form_field(o == crate::api::value::Order::Next),
            Command::AddEditableRange(r) => self.add_editable_range(r, fonts),
            Command::RemoveEditableRanges => self.remove_editable_ranges(fonts),
            Command::InsertFormField(t) => self.insert_form_field(t, fonts),
            Command::SetFormField(c) => self.set_form_field(c, fonts),
            Command::FillFormField(c) => self.fill_form_field(c, fonts),
            Command::SetFormFieldShading(on) => self.set_form_shading(on, fonts),
            Command::SetDocumentProperties(p) => self.set_document_properties(p, fonts),
            Command::SetDocumentVariable(v) => self.set_document_variable(v, fonts),
            Command::SetFileName(n) => self.set_file_name(n),
            Command::DeleteBookmark(name) => self.delete_bookmark(&name, fonts),
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
            Command::ConvertTextToTable(t) => self.convert_text_to_table(&t, fonts),
            Command::ConvertTableToText(t) => self.convert_table_to_text(&t, fonts),
            Command::DistributeRows => self.distribute_rows(fonts),
            Command::DistributeColumns => self.distribute_columns(fonts),
            Command::MoveRows(d) => self.move_rows(d == crate::api::command::VerticalDirection::Up, fonts),
            Command::AutoFitColumns(to) => self.auto_fit_columns(to, fonts),
            Command::SetCellAlignment(a) => self.set_cell_alignment(a, fonts),
            Command::SetCellShading(c) => self.set_cell_shading(c, fonts),
            Command::SetCellBorders(b) => self.set_cell_borders(b, fonts),
            Command::SetCellMargins(m) => self.set_cell_margins(m, fonts),
            Command::SetCellProperties(p) => self.table_format(&super::table_format::TableFormat::CellProps(p), fonts),
            Command::SetRowProperties(p) => self.set_row_properties(p, fonts),
            Command::SetTableBandSizes(b) => self.set_table_band_sizes(b.rows, b.columns, fonts),
            Command::SetTableCellSpacing(s) => self.table_format(&super::table_format::TableFormat::CellSpacing(s.map_or(0.0, |s| s.0)), fonts),
            Command::SetTableAltText(a) => self.table_format(&super::table_format::TableFormat::Alt(a), fonts),
            Command::SetHeaderRow(on) => self.set_header_row(on, fonts),
            Command::SetRowHeight(h) => self.set_row_height(h, fonts),
            Command::SetRowCanBreak(on) => self.set_row_can_break(on, fonts),
            Command::SetColumnWidth(w) => self.table_format(&super::table_format::TableFormat::ColumnWidth(w.0), fonts),
            Command::MoveTableColumnBoundary(b) => self.move_table_column_boundary(b.index as usize, b.to.0, fonts),
            Command::MoveTableRowBoundary(b) => self.move_table_row_boundary(b.index as usize, b.to.0, fonts),
            Command::SetTableWidth(w) => self.set_table_width(w, fonts),
            Command::SetTableAlignment(a) => self.set_table_alignment(a, fonts),
            Command::SetTableIndent(i) => self.table_format(&super::table_format::TableFormat::TableIndent(i.0), fonts),
            Command::SetTableStyle(id) => self.set_table_style(id, fonts),
            Command::SetTableStyleOptions(o) => self.set_table_style_options(o, fonts),
            Command::SetDefaultTableStyle(s) => self.set_default_table_style(s.map(|s| s.0), fonts),
            Command::SetTablePosition(p) => self.set_table_position(p, fonts),
            Command::SetTableCellMargins(m) => self.set_table_cell_margins([m.top, m.left, m.bottom, m.right], fonts),
            Command::InsertCells(shift) => self.insert_cells(
                match shift {
                    crate::api::command::InsertCellShift::Right => super::table_cells::InsertShift::Right,
                    crate::api::command::InsertCellShift::Down => super::table_cells::InsertShift::Down,
                },
                fonts,
            ),
            Command::SetTableTool(t) => {
                self.set_table_tool(t);
                Outcome::Moved
            }
            Command::SetTablePen(b) => self.set_table_pen_line(&b),
            Command::DrawTableStroke(s) => {
                let stroke = super::table_draw::Stroke { page: s.page as usize, from: s.from.map(|v| v.0), to: s.to.map(|v| v.0) };
                self.draw_table_stroke(stroke, fonts)
            }
            Command::EraseTableBorder(p) => self.erase_table_border(p.page as usize, p.at.map(|v| v.0), fonts),
            // Objects
            Command::SetObjectSize(s) => self.set_object_size(s, fonts),
            Command::ResetObjectSize => self.object_cmd(super::objects::ObjectCmd::ResetSize, fonts),
            Command::SetObjectWrap(w) => self.set_object_wrap(w, fonts),
            Command::SetAnchorLocked(on) => self.set_anchor_locked(on, fonts),
            Command::SetObjectPosition(p) => self.set_object_position(p, fonts),
            Command::SetObjectWrapOptions(w) => self.set_object_wrap_options(w, fonts),
            Command::SetAltText(a) => self.alt_text(a, fonts),
            Command::SetPictureCrop(c) => self.set_picture_crop(c, fonts),
            Command::AlignObjects(a) => self.align_objects(a, fonts),
            Command::ChangePicture(data) => self.change_picture(data.0, fonts),
            Command::DeleteObject => self.delete_selected_object(fonts),
            c @ (Command::InsertShape(_)
            | Command::SetShapePreset(_)
            | Command::SetShapeAdjust(_)
            | Command::SetShapeFill(_)
            | Command::SetShapeLine(_)
            | Command::SetShapeEffects(_)
            | Command::SetShapeText(_)
            | Command::SetPictureAdjust(_)
            | Command::SetObjectRotation(_)
            | Command::FlipObject(_)
            | Command::ArrangeObject(_)
            | Command::GroupObjects
            | Command::UngroupObjects
            | Command::SelectGroupChild(_)) => self.shape_command(c, fonts),
            Command::RegroupObjects => self.regroup_objects(fonts),
            Command::EditWrapPoints => self.edit_wrap_points(),
            Command::SetObjectName(n) => self.set_object_name(&n, fonts),
            Command::SetObjectTitle(t) => self.set_object_title(&t, fonts),
            Command::SetObjectHidden(h) => match h.object.float_id() {
                Some(f) => self.set_object_hidden(f, &h.object.path, h.hidden, fonts),
                None => Outcome::unavailable("no such object"),
            },
            Command::SelectObject(s) => match s.object.float_id() {
                Some(f) => self.select_listed(f, &s.object.path, s.extend.unwrap_or(false)),
                None => Outcome::unavailable("no such object"),
            },
            Command::RestackObject(r) => {
                let target = r.in_front_of.as_ref().map(|t| t.float_id().filter(|_| t.path.is_empty()));
                match (r.object.float_id().filter(|_| r.object.path.is_empty()), target) {
                    (Some(f), None) => self.restack_object(f, None, fonts),
                    (Some(f), Some(Some(t))) => self.restack_object(f, Some(t), fonts),
                    _ => Outcome::unavailable("no such object"),
                }
            }
            Command::SetLockAspectRatio(on) => self.set_lock_aspect(on, fonts),
            Command::SetPictureRecolor(r) => match r.to_model() {
                Ok(r) => self.set_picture_recolor(r, fonts),
                Err(e) => Outcome::invalid(&e),
            },
            Command::ResetPicture(r) => self.reset_picture(r.size.unwrap_or(false), fonts),
            Command::SetWrapPoints(p) => {
                let points: Vec<(f64, f64)> = p.iter().map(|p| (p.x, p.y)).collect();
                self.set_wrap_points(&points, fonts)
            }
            Command::ResetWrapPoints => self.reset_wrap_points(fonts),
            Command::LinkTextBox(id) => match id.float_id().filter(|_| id.path.is_empty()) {
                Some(f) => self.link_text_box(f, fonts),
                None => Outcome::unavailable("no such text box"),
            },
            Command::BeginTextBoxLink => self.begin_linking(),
            Command::UnlinkTextBox => self.unlink_text_box(fonts),
            // Fields
            Command::SetField(f) => self.set_field(f, fonts),
            Command::SetFieldResults(r) => self.set_field_results(r, fonts),
            Command::UpdateFields(ids) => self.update_fields(ids.ids.as_deref(), fonts),
            // References
            Command::InsertTableOfContents(c) => self.insert_toc_api(&c, fonts),
            Command::InsertTableOfFigures(c) => self.insert_tof_api(&c, fonts),
            Command::InsertManualTableOfContents => self.insert_manual_table_of_contents(fonts),
            Command::UpdateTableOfContents(u) => match self.toc_by_index(u.index) {
                Ok(t) => self.update_toc(&t, references::toc_update(u.mode), fonts),
                Err(out) => out,
            },
            Command::UpdateAllTablesOfContents(mode) => {
                let n = self.update_all_tables(references::toc_update(mode), fonts);
                self.made = Some(crate::api::command::CommandResult::Changed { changed: u32::try_from(n).unwrap_or(u32::MAX) });
                if n == 0 { Outcome::Unchanged("no table to update") } else { Outcome::Changed(self.revision) }
            }
            Command::RemoveTableOfContents(r) => match self.toc_by_index(r.index) {
                Ok(t) => self.remove_table_of_contents(&t, fonts),
                Err(out) => out,
            },
            Command::MarkTableOfContentsEntry(m) => self.mark_toc_entry_api(&m, fonts),
            Command::InsertCaption(c) => self.insert_caption_api(&c, fonts),
            Command::SetAutoCaption(c) => self.set_auto_caption_api(c.as_ref()),
            Command::InsertCrossReference(c) => self.insert_cross_reference_api(&c, fonts),
            Command::SetFieldsLocked(l) => self.lock_fields(l.locked, l.ids.as_deref(), fonts),
            Command::UnlinkFields(ids) => self.unlink_fields(ids.ids.as_deref(), fonts),
            Command::ShowFieldCodes(s) => self.show_field_codes(s.show, s.ids.as_deref(), fonts),
            Command::SetFieldLabels(l) => self.set_field_labels(l),
            Command::SetSubTemplateRules(r) => self.set_sub_template_rules(r),
            Command::SetApplicationFieldTypes(t) => self.set_application_field_types(t),
            Command::SetSubTemplateView(v) => self.set_sub_template_view(v),
            Command::EditSubTemplate(n) => self.edit_sub_template(&n, fonts),
            Command::CreateSubTemplate(c) => self.create_sub_template(c, fonts),
            Command::ReplaceBody(r) => self.replace_body(&r.from.0, fonts),
            Command::RenameSubTemplate(r) => self.rename_sub_template(r, fonts),
            Command::RemoveSubTemplate(n) => self.remove_sub_template(&n, fonts),
            Command::CloseSubTemplate => {
                let out = self.close_sub_template_to(None);
                if let Outcome::Moved = out {
                    self.ensure_current(fonts);
                    self.scroll_to_caret(fonts);
                }
                out
            }
            // Content controls and regions
            Command::InsertControl(c) => self.insert_control(c, fonts),
            Command::SetControl(c) => self.set_control(c, fonts),
            Command::RemoveControl(r) => self.remove_element(&r.id, r.keep_content.unwrap_or(false), fonts),
            Command::FillControl(f) => self.fill_control(f, fonts),
            Command::SetCheckbox(c) => self.set_checkbox(c, fonts),
            Command::WrapInRepeatingSection(r) => self.wrap_in_repeating_section(r, fonts),
            Command::AddRepeatingItem(a) => self.add_repeating_item(&a.id, a.item.as_ref(), a.before.unwrap_or(false), fonts),
            Command::RemoveElement(id) => self.remove_element(&id, false, fonts),
            Command::ReplaceInElements(r) => self.replace_in_elements(r, fonts),
            Command::ReplaceFont(r) => self.replace_font(&r, fonts),
            Command::MarkElements(m) => self.mark_elements(m),
            Command::SetProtectedPictureControls(p) => self.set_protected_picture_controls(p),
            Command::ActivatePictureControl(a) => self.activate_picture_control_cmd(a.element.as_ref()),
            // Data parts
            Command::WriteDataPart(w) => self.write_data_part(w, fonts),
            Command::RemoveDataPart(r) => self.remove_data_part(r, fonts),
            // Review
            Command::SetTrackChanges(on) => self.set_track_changes(on),
            Command::SetMarkupView(v) => self.set_markup_view(v),
            Command::SetShowMarkup(c) => {
                let s = &self.review.markup.show;
                let (i, f, k) = (
                    c.insertions_deletions.unwrap_or(s.insertions_deletions),
                    c.formatting.unwrap_or(s.formatting),
                    c.comments.unwrap_or(s.comments),
                );
                self.set_show_markup(i, f, k)
            }
            Command::SetHiddenReviewers(r) => self.set_hidden_reviewers(&r.hidden.iter().map(String::as_str).collect::<Vec<_>>()),
            Command::SetBalloonMode(m) => self.set_balloon_mode(m.model()),
            Command::SetMarkupStyle(c) => self.set_markup_style(crate::api::review::MarkupStyle::changed(&self.review.markup.style, &c)),
            Command::SetTrackOptions(t) => {
                let (moves, formatting) = self.track_options();
                self.set_track_options(t.moves.unwrap_or(moves), t.formatting.unwrap_or(formatting))
            }
            Command::AcceptAllAndStopTracking => self.accept_all_and_stop_tracking(fonts),
            Command::DeleteAllComments(f) => self.delete_all_comments(f.shown_only.unwrap_or(false), fonts),
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
            Command::SetSelectObjects(on) => {
                self.set_select_objects(on);
                Outcome::Moved
            }
            Command::SetZoom(_)
            | Command::SetViewOptions(_)
            | Command::SetRulerTabType(_)
            | Command::ShowTabTypes
            | Command::ShowTabStop
            | Command::SetAutoCorrect(_)
            | Command::ShowAutoCorrectOptions
            | Command::ClosePopover => Outcome::Moved,
            Command::RevertAutoCorrect | Command::NeverAutoCorrect => self.revert_correction(fonts),
            Command::DescribeFormatting => Outcome::Unchanged("described"),
            Command::SetMode(m) => self.set_mode(match m {
                crate::api::state::Mode::Editing => super::Mode::Editing,
                crate::api::state::Mode::Design => super::Mode::Design,
            }),
        }
    }

    /// The paragraph a navigation pane's heading id names.
    fn heading_para(&self, id: &crate::api::value::HeadingId) -> Result<crate::layout::ParaId, Outcome> {
        let para = id.0.parse::<u64>().map(crate::layout::ParaId).map_err(|_| Outcome::invalid("no such heading"))?;
        match self.index.para(&self.doc, para) {
            Some(_) => Ok(para),
            None => Err(Outcome::invalid("no such heading")),
        }
    }

    pub fn joined(&mut self, label: &'static str, fonts: &Fonts, f: impl FnOnce(&mut Editor) -> Outcome) -> Outcome {
        self.begin_batch(Some(label.into()));
        let out = f(self);
        let end = self.end_batch(!matches!(out, Outcome::Refused(_)), fonts);
        if matches!(end, Outcome::Changed(_)) { end } else { out }
    }
}
