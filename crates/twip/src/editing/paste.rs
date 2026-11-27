//! Paste options, Paste Special, and pictures to and from the clipboard

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::resolvers::{Resolvers, TypedDefs};
use crate::docx::{Block, Inline, Paragraph, Run, RunStyle};
use crate::font::Fonts;

use super::Editor;
use super::clipboard::{Fragment, Look};
use super::commands::Outcome;
use super::position::Position;
use super::selection::{ObjectRef, Selection};
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::run_xml;
use crate::format::{CharEdit, CharProps, ListChoice, ParaEdit, ParaProp, Prop};
use crate::layout::{FloatId, Unit};

/// The most bytes a pasted picture has: the most a part of an opened document may.
pub const MAX_PICTURE: usize = crate::docx::package::MAX_PART as usize;

/// What the clipboard holds, as a paste reads it: the engine's own copy, another application's HTML, the plain text
/// and a picture.
#[derive(Clone, Debug, Default)]
pub struct Clip {
    /// The engine's own copy, when the clipboard holds it.
    pub fragment: Option<Fragment>,
    /// HTML (`text/html`).
    pub html: Option<String>,
    /// The plain text.
    pub text: String,
    /// A picture: PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF.
    pub image: Option<Vec<u8>>,
}

/// How a paste takes what the clipboard holds: Word's Paste Options, Paste Special's formats and its table choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PasteChoice {
    /// Its own look: formatting, and where a style of the same name differs here, that style's formatting.
    KeepSource,
    /// The look of the text it lands in, keeping bold, italic and underline applied to part of it.
    Merge,
    /// This document's styles where it has one of the same name; formatting applied directly kept.
    DestinationStyles,
    /// Its text only, formatted as where it lands; list numbers and bullets as text when [`PasteDefaults::keep_bullets`].
    TextOnly,
    /// The clipboard's HTML, read as HTML even when it holds the engine's own copy.
    Html,
    /// The clipboard's plain text as it is.
    Unformatted,
    /// The clipboard's picture, in line.
    Picture,
    /// Copied cells as a table nested in the cell at the caret; at a paragraph after a table (not in a cell), as a
    /// table of their own there.
    NestTable,
    /// Copied cells as rows above the caret's row, made like the rows of this table.
    MergeTable,
    /// Copied cells as rows above the caret's row, as they were copied.
    InsertRows,
    /// Copied cells over the cells from the caret's (or the selected block), adding rows the table lacks.
    OverwriteCells,
}

impl PasteChoice {
    /// The choice an API [`PasteAs`](crate::api::command::PasteAs) names; `None` for `formatted` (the defaults
    /// decide).
    pub fn of_api(a: crate::api::command::PasteAs) -> Option<PasteChoice> {
        use crate::api::command::PasteAs as A;
        Some(match a {
            A::Formatted => return None,
            A::KeepSource => PasteChoice::KeepSource,
            A::Merge => PasteChoice::Merge,
            A::DestinationStyles => PasteChoice::DestinationStyles,
            A::TextOnly => PasteChoice::TextOnly,
            A::Html => PasteChoice::Html,
            A::Text => PasteChoice::Unformatted,
            A::Picture => PasteChoice::Picture,
            A::NestTable => PasteChoice::NestTable,
            A::MergeTable => PasteChoice::MergeTable,
            A::InsertRows => PasteChoice::InsertRows,
            A::OverwriteCells => PasteChoice::OverwriteCells,
        })
    }

    /// The API's name for it.
    pub fn to_api(self) -> crate::api::command::PasteAs {
        use crate::api::command::PasteAs as A;
        match self {
            PasteChoice::KeepSource => A::KeepSource,
            PasteChoice::Merge => A::Merge,
            PasteChoice::DestinationStyles => A::DestinationStyles,
            PasteChoice::TextOnly => A::TextOnly,
            PasteChoice::Html => A::Html,
            PasteChoice::Unformatted => A::Text,
            PasteChoice::Picture => A::Picture,
            PasteChoice::NestTable => A::NestTable,
            PasteChoice::MergeTable => A::MergeTable,
            PasteChoice::InsertRows => A::InsertRows,
            PasteChoice::OverwriteCells => A::OverwriteCells,
        }
    }

    /// Whether it is one of the table choices.
    pub fn is_table(self) -> bool {
        matches!(self, PasteChoice::NestTable | PasteChoice::MergeTable | PasteChoice::InsertRows | PasteChoice::OverwriteCells)
    }
}

/// Word's cut, copy and paste options: what a paste does when it isn't told, and what goes with it. A host's
/// setting; Word's defaults otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteDefaults {
    /// Pasting within the same document.
    pub within: PasteChoice,
    /// Pasting between documents.
    pub between: PasteChoice,
    /// Pasting between documents when style definitions conflict.
    pub conflict: PasteChoice,
    /// Pasting from other programs.
    pub other: PasteChoice,
    /// Keep bullets and numbers when pasting text with Keep Text Only.
    pub keep_bullets: bool,
    /// Show the Paste Options button when content is pasted.
    pub show_button: bool,
    /// Use smart cut and paste: the adjustments below are made only while it is on.
    pub smart: bool,
    /// Adjust sentence and word spacing automatically: a space added or taken away to keep words apart.
    pub smart_spacing: bool,
    /// Adjust paragraph spacing on paste.
    pub smart_paragraphs: bool,
    /// Adjust table formatting and alignment on paste: copied cells landing in a table go into it (as rows, or over
    /// its cells) as the selection suggests, rather than as a table nested in the cell.
    pub smart_tables: bool,
    /// Smart style behavior.
    pub smart_styles: bool,
    /// Merge pasted lists with surrounding lists: list items pasted into a list item continue its list, in its format.
    pub merge_lists: bool,
    /// Merge formatting when pasting from PowerPoint.
    pub from_powerpoint: bool,
    /// Adjust formatting when pasting from Excel.
    pub from_excel: bool,
}

impl Default for PasteDefaults {
    fn default() -> PasteDefaults {
        PasteDefaults {
            within: PasteChoice::KeepSource,
            between: PasteChoice::KeepSource,
            conflict: PasteChoice::DestinationStyles,
            other: PasteChoice::KeepSource,
            keep_bullets: true,
            show_button: true,
            smart: true,
            smart_spacing: true,
            smart_paragraphs: true,
            smart_tables: true,
            smart_styles: true,
            merge_lists: true,
            from_powerpoint: true,
            from_excel: true,
        }
    }
}

impl PasteDefaults {
    /// The choices each default may take, as Word's options list them.
    pub fn allowed(&self) -> bool {
        use PasteChoice::*;
        let plain = |c: PasteChoice| matches!(c, KeepSource | Merge | TextOnly);
        plain(self.within) && plain(self.between) && plain(self.other) && matches!(self.conflict, KeepSource | Merge | DestinationStyles | TextOnly)
    }

    /// Whether a paste or cut adds or takes away a space to keep words apart.
    pub fn spaces(&self) -> bool {
        self.smart && self.smart_spacing
    }

    /// Whether copied cells landing in a table go into it as the selection suggests.
    pub fn tables(&self) -> bool {
        self.smart && self.smart_tables
    }

    /// Whether pasted list items landing in a list item continue its list.
    pub fn lists(&self) -> bool {
        self.smart && self.merge_lists
    }
}

/// Where pasted content was copied: this document, another the engine has open, or another application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteFrom {
    /// This document.
    Within,
    /// Another document, its styles as here (or not here).
    Between,
    /// Another document, one of whose styles has a definition of its own here.
    Conflict,
    /// Another application.
    Other,
}

/// The paste just made, which its Paste Options can make again another way.
#[derive(Clone, Debug)]
pub struct LastPaste {
    /// What the clipboard held.
    pub clip: Rc<Clip>,
    /// The choice it was pasted with (for copied cells pasted into a table, the table choice it took).
    pub choice: PasteChoice,
    /// The choices it could take where it was pasted, in Word's order.
    pub choices: Rc<[PasteChoice]>,
    /// The history position the paste left.
    position: u64,
    /// Where it ends.
    pub end: Position,
}

impl LastPaste {
    /// The history position the paste left.
    pub(super) fn position(&self) -> u64 {
        self.position
    }
}

/// The Paste Options button of the paste just made: where it goes, what it offers and the choice made.
#[derive(Clone, Debug, PartialEq)]
pub struct PasteOptionsView {
    /// The page it is on.
    pub page: usize,
    /// The button's box on the page, points: below the end of the paste, at its right.
    pub button: [f64; 4],
    /// The end of the paste's caret box on the page.
    pub end: [f64; 4],
    /// The choices it offers, in Word's order; `current` among them.
    pub choices: Vec<PasteChoice>,
    /// The choice the paste was made with.
    pub current: PasteChoice,
}

/// The size of the Paste Options button, points.
const BUTTON: [f64; 2] = [24.0, 15.0];

/// The most pixels (at 96 dpi) one copy draws its objects with, together, for its HTML: past it, the objects left are
/// their alternative text. One selected object is drawn whatever its size.
pub const MAX_DRAWN_PIXELS: u64 = 1 << 22;

/// The width an object is laid out in to be drawn, points: wider than any object is (22 inches).
const DRAW_WIDTH: f64 = 4000.0;

/// A copied object as it is drawn: its items, placed as laid out (the picture made of them is cut to what they draw).
pub type Drawing = Rc<[crate::layout::fragment::Clipped]>;

impl Editor {
    /// The cut, copy and paste options.
    pub fn paste_defaults(&self) -> PasteDefaults {
        self.paste_defaults
    }

    /// Sets the cut, copy and paste options. A session setting: not saved, no undo step. `false` when a default is
    /// a choice its option doesn't offer.
    pub fn set_paste_defaults(&mut self, d: PasteDefaults) -> bool {
        if !d.allowed() {
            return false;
        }
        self.paste_defaults = d;
        if !d.show_button {
            self.last_paste = None;
        }
        true
    }

    /// Where content `f` was copied from.
    pub fn paste_from(&self, f: &Fragment) -> PasteFrom {
        let Some(o) = f.origin.as_deref().filter(|o| o.key != self.key) else { return PasteFrom::Within };
        let (Some(from), Some(here)) = (o.resolver.as_deref(), self.doc.resolver.as_deref()) else { return PasteFrom::Between };
        let mut ids: Vec<&str> = Vec::new();
        each_para(&f.blocks, &mut |p| {
            ids.extend(p.style.style_id.as_deref());
            ids.extend(p.inlines.iter().filter_map(|i| i.run()?.style_id.as_deref()));
        });
        ids.sort_unstable();
        ids.dedup();
        // Compared as they resolve: a style based on one that differs differs.
        let (from, here) = (from.typed(), here.typed());
        let differs = ids.iter().any(|&id| {
            if from.cascade.styles.get(id).is_none() || here.cascade.styles.get(id).is_none() {
                return false;
            }
            chains_differ(&from, &here, Some(id), Some(id))
        });
        if differs { PasteFrom::Conflict } else { PasteFrom::Between }
    }

    /// What a paste of `clip` takes when it isn't told: the engine's copy as the options say for where it came from,
    /// else HTML as they say for other programs (the picture when the HTML holds only remote pictures), else the
    /// text, else the picture; as it lands here
    /// ([`Editor::landing`]), one of [`Editor::paste_choices`].
    pub fn default_paste(&self, clip: &Clip) -> Option<PasteChoice> {
        self.asked_default(clip).map(|c| self.landing(clip, c))
    }

    /// The options' choice for `clip`, before it lands.
    fn asked_default(&self, clip: &Clip) -> Option<PasteChoice> {
        let d = &self.paste_defaults;
        if let Some(f) = &clip.fragment {
            return Some(match self.paste_from(f) {
                PasteFrom::Within => d.within,
                PasteFrom::Between => d.between,
                PasteFrom::Conflict => d.conflict,
                PasteFrom::Other => d.other,
            });
        }
        if let Some(h) = &clip.html {
            // A browser's Copy Image: HTML naming a remote picture, and the picture itself
            if clip.image.is_some() && super::html::only_remote_pictures(h) {
                return Some(PasteChoice::Picture);
            }
            return Some(d.other);
        }
        if !clip.text.is_empty() {
            return Some(d.other);
        }
        clip.image.as_ref().map(|_| PasteChoice::Picture)
    }

    /// `choice` as a paste of `clip` takes it here: copied cells landing in a table with a look choice (Keep Source,
    /// Merge or Use Destination Styles) take the table choice the selection suggests; a clipboard of plain text (or
    /// a picture alone) with a look choice Keep Text Only (or its picture), as Word pastes it.
    pub fn landing(&self, clip: &Clip, choice: PasteChoice) -> PasteChoice {
        use PasteChoice::*;
        if !matches!(choice, KeepSource | Merge | DestinationStyles) {
            return choice;
        }
        match &clip.fragment {
            Some(f) if self.cells_into_table(f) => self.cells_choice(f),
            Some(_) => choice,
            None if clip.html.is_some() => choice,
            None if !clip.text.is_empty() => TextOnly,
            None if clip.image.is_some() => Picture,
            None => choice,
        }
    }

    /// The table choice a paste of copied cells `f` into the table at the selection takes when it isn't told: what the
    /// selection suggests ([`PasteDefaults::tables`]), else Nest Table.
    fn cells_choice(&self, f: &Fragment) -> PasteChoice {
        if !self.paste_defaults.tables() {
            return PasteChoice::NestTable;
        }
        self.suggested_cells(f).unwrap_or(PasteChoice::NestTable)
    }

    /// The choices a paste of `clip` here can take, in Word's order: the table choices when copied cells land in a
    /// table, else the Paste Options; then Paste Special's formats the clipboard holds.
    pub fn paste_choices(&self, clip: &Clip) -> Vec<PasteChoice> {
        use PasteChoice::*;
        let mut out = Vec::new();
        let rich = clip.fragment.is_some() || clip.html.is_some();
        if clip.fragment.as_ref().is_some_and(|f| self.cells_into_table(f)) {
            out.extend([NestTable, MergeTable, InsertRows]);
            // Cells to write over: in a table, not after one
            if self.table_target().is_some() {
                out.push(OverwriteCells);
            }
        } else if rich {
            out.push(KeepSource);
            if clip.fragment.as_ref().and_then(|f| f.origin.as_deref()).is_some_and(|o| o.key != self.key) {
                out.push(DestinationStyles);
            }
            out.push(Merge);
        }
        if rich || !clip.text.is_empty() {
            out.push(TextOnly);
        }
        if clip.html.is_some() {
            out.push(Html);
        }
        if !clip.text.is_empty() {
            out.push(Unformatted);
        }
        if clip.image.is_some() {
            out.push(Picture);
        }
        out
    }

    /// Whether `f` is copied cells (or a table) and the selection is in a table, or `f` is whole rows at a paragraph
    /// right after a table (they go at its end): a paste offers the table choices.
    fn cells_into_table(&self, f: &Fragment) -> bool {
        let table = f.cells.is_some() || matches!(f.blocks.as_slice(), [Block::Table(_)]);
        table && (self.table_target().is_some() || f.cells.is_some_and(|c| c.whole_rows) && self.table_before_caret().is_some())
    }

    /// Pastes `clip` as one transaction, as `choice` takes it (`None`: as the options say), as it lands here
    /// ([`Editor::landing`]). Told nothing, HTML that pastes nothing gives way to the picture, else the text, the
    /// choice taken. The Paste Options of the paste then offer the other choices.
    pub fn paste_clip(&mut self, clip: Clip, choice: Option<PasteChoice>, fonts: &Fonts) -> Outcome {
        let told = choice.is_some();
        let Some(asked) = choice.or_else(|| self.asked_default(&clip)) else { return Outcome::Unchanged("nothing to paste") };
        let mut choice = self.landing(&clip, asked);
        let choices = self.paste_choices(&clip);
        if !choices.contains(&choice) {
            return Outcome::unavailable("the clipboard holds nothing to paste that way");
        }
        let out = match &clip.fragment {
            // As the selection suggests: the table choice it names
            Some(f) if choice.is_table() && !asked.is_table() && self.paste_defaults.tables() => self.paste_moving(f, fonts),
            _ => self.paste_as(&clip, choice, fonts),
        };
        let html_only = clip.fragment.is_none() && clip.html.is_some();
        let instead = match (&out, choice) {
            _ if told || !html_only => None,
            // HTML that pastes nothing (a remote picture without alternative text, or no content): what else it holds
            (Outcome::Unchanged(_), PasteChoice::KeepSource | PasteChoice::Merge | PasteChoice::DestinationStyles) => {
                Some([PasteChoice::Picture, PasteChoice::Unformatted])
            }
            // A picture the engine doesn't take for HTML naming only remote ones: the HTML, else the text
            (Outcome::Unchanged(_) | Outcome::Refused(_), PasteChoice::Picture) => {
                Some([self.landing(&clip, self.paste_defaults.other), PasteChoice::Unformatted])
            }
            _ => None,
        };
        let out = match instead {
            Some(next) => {
                let mut out = out;
                for next in next.into_iter().filter(|c| *c != choice && choices.contains(c)) {
                    out = self.paste_as(&clip, next, fonts);
                    if matches!(out, Outcome::Changed(_)) {
                        choice = next;
                        break;
                    }
                }
                out
            }
            None => out,
        };
        self.last_paste = match (&out, self.paste_defaults.show_button) {
            (Outcome::Changed(_), true) => {
                self.caret().map(|end| LastPaste { clip: Rc::new(clip), choice, choices: choices.into(), position: self.history.position(), end })
            }
            _ => None,
        };
        out
    }

    fn paste_as(&mut self, clip: &Clip, choice: PasteChoice, fonts: &Fonts) -> Outcome {
        use PasteChoice::*;
        match choice {
            Picture => match &clip.image {
                Some(data) => self.paste_picture(data, fonts),
                None => Outcome::unavailable("no picture"),
            },
            Unformatted => self.paste_plain(&clip.text, fonts),
            TextOnly => {
                let text = match &clip.fragment {
                    Some(f) => text_only(f, self.paste_defaults.keep_bullets),
                    None => match clip.html.as_deref().and_then(|h| self.html_text(h)).filter(|t| !t.is_empty()) {
                        Some(t) => t,
                        None => clip.text.clone(),
                    },
                };
                self.paste_plain(&text, fonts)
            }
            Html => match &clip.html {
                Some(h) => self.paste_html_as(h, KeepSource, fonts),
                None => Outcome::unavailable("no HTML"),
            },
            KeepSource | Merge | DestinationStyles => match (&clip.fragment, &clip.html) {
                (Some(f), _) => self.paste_fragment_as(f, choice, fonts),
                (None, Some(h)) => self.paste_html_as(h, choice, fonts),
                (None, None) => self.paste_plain(&clip.text, fonts),
            },
            NestTable | MergeTable | InsertRows | OverwriteCells => match &clip.fragment {
                Some(f) => self.paste_table_as(f, choice, fonts),
                None => Outcome::unavailable("no cells"),
            },
        }
    }

    /// Plain text pasted, one step named as a paste.
    fn paste_plain(&mut self, t: &str, fonts: &Fonts) -> Outcome {
        if t.is_empty() {
            return Outcome::Unchanged("no text");
        }
        let t = t.replace("\r\n", "\n");
        self.joined("Paste", fonts, |e| e.paste_text(&t, fonts))
    }

    /// `f` pasted as `choice` (keep source, merge or destination styles) takes it.
    pub(super) fn paste_fragment_as(&mut self, f: &Fragment, choice: PasteChoice, fonts: &Fonts) -> Outcome {
        let look = if choice == PasteChoice::DestinationStyles { Look::Destination } else { Look::Source };
        let merged;
        let f = match choice {
            PasteChoice::Merge => {
                let Some(m) = self.merged(f) else { return Outcome::Unchanged("no caret") };
                merged = m;
                &merged
            }
            _ if f.look == look => f,
            _ => {
                merged = Fragment { look, ..f.clone() };
                &merged
            }
        };
        self.paste_moving(f, fonts)
    }

    /// `f` pasted, with the move a tracked cut of it makes ([`Editor::paste_moved`]), as one step.
    fn paste_moving(&mut self, f: &Fragment, fonts: &Fonts) -> Outcome {
        self.begin_batch(None);
        let out = self.paste_fragment(f, fonts);
        self.paste_moved(f, &out, fonts);
        let end = self.end_batch(!matches!(out, Outcome::Refused(_)), fonts);
        if matches!(end, Outcome::Changed(_)) { end } else { out }
    }

    /// Copied cells (or a table) pasted into the table at the selection as `choice` takes them.
    fn paste_table_as(&mut self, f: &Fragment, choice: PasteChoice, fonts: &Fonts) -> Outcome {
        let mut f = f.clone();
        if choice == PasteChoice::NestTable {
            f.cells = None;
            return self.paste_fragment(&f, fonts);
        }
        if f.cells.is_none() {
            f.cells = Some(super::cell_clipboard::CellCopy { whole_rows: true, whole_table: true });
        }
        self.paste_cells_as(&f, Some(choice), fonts).unwrap_or(Outcome::unavailable("not in a table"))
    }

    /// The Paste Options of the paste just made, while nothing has changed since: the choice it was made with and
    /// the clipboard it took.
    pub fn last_paste(&self) -> Option<&LastPaste> {
        self.last_paste.as_ref().filter(|l| l.position == self.history.position() && self.history.redo_depth() == 0)
    }

    /// Escape: the Paste Options button goes.
    pub fn dismiss_paste_options(&mut self) {
        self.last_paste = None;
    }

    /// The Paste Options button of the paste just made, on `pages`: below the end of the paste.
    pub fn paste_options_view(&mut self, pages: std::ops::Range<usize>) -> Option<PasteOptionsView> {
        let last = self.last_paste()?.clone();
        let v = self.caret_at(last.end)?;
        if !pages.contains(&v.page) {
            return None;
        }
        let end = v.rect();
        let (x, y) = (end[2], end[3]);
        // The button offers Word's Paste Options and the choice taken; Paste Special's formats stay in Paste
        // Special, Picture but for a clipboard holding nothing else.
        let picture_only = last.clip.fragment.is_none() && last.clip.html.is_none() && last.clip.text.is_empty();
        let choices = last
            .choices
            .iter()
            .copied()
            .filter(|&c| {
                c == last.choice || (!matches!(c, PasteChoice::Html | PasteChoice::Unformatted) && (c != PasteChoice::Picture || picture_only))
            })
            .collect();
        Some(PasteOptionsView { page: v.page, button: [x, y, x + BUTTON[0], y + BUTTON[1]], end, choices, current: last.choice })
    }

    /// The paste just made made again as `choice` takes it, in place of it: one undo step takes it back.
    pub fn repaste(&mut self, choice: PasteChoice, fonts: &Fonts) -> Outcome {
        let Some(last) = self.last_paste().cloned() else { return Outcome::unavailable("no paste to change") };
        if choice == last.choice {
            return Outcome::Unchanged("pasted that way");
        }
        if !last.choices.contains(&choice) {
            return Outcome::unavailable("the clipboard holds nothing to paste that way");
        }
        let first_commit = self.commits.len();
        if !matches!(self.undo_many(true, 1, fonts), Outcome::Changed(_)) {
            return Outcome::unavailable("the paste can't be undone");
        }
        let clip = Rc::unwrap_or_clone(last.clip.clone());
        let out = self.paste_clip(clip, Some(choice), fonts);
        if !matches!(out, Outcome::Changed(_)) {
            // Back as it was
            let _ = self.undo_many(false, 1, fonts);
            self.last_paste = Some(last);
        }
        // One change: the paste made again
        self.join_commits(first_commit);
        out
    }

    /// [`Editor::repaste`] as an API choice names it; `formatted`: as the paste options say.
    pub fn repaste_as(&mut self, choice: crate::api::command::PasteAs, fonts: &Fonts) -> Outcome {
        let choice = match PasteChoice::of_api(choice) {
            Some(c) => Some(c),
            None => self.last_paste().map(|l| l.clip.clone()).and_then(|clip| self.default_paste(&clip)),
        };
        match choice {
            Some(c) => self.repaste(c, fonts),
            None => Outcome::unavailable("no paste to change"),
        }
    }

    /// `setPasteOptions`: the changed options applied to the cut, copy and paste options.
    pub fn set_paste_options(&mut self, c: &crate::api::command::PasteOptionsChange) -> Outcome {
        let mut d = self.paste_defaults;
        let pick = |to: &mut PasteChoice, from: Option<crate::api::command::PasteAs>| -> bool {
            match from.map(PasteChoice::of_api) {
                None => true,
                Some(Some(c)) => {
                    *to = c;
                    true
                }
                Some(None) => false,
            }
        };
        let set = |to: &mut bool, from: Option<bool>| {
            if let Some(v) = from {
                *to = v;
            }
        };
        let ok = pick(&mut d.within, c.within) & pick(&mut d.between, c.between) & pick(&mut d.conflict, c.conflict) & pick(&mut d.other, c.other);
        set(&mut d.keep_bullets, c.keep_bullets);
        set(&mut d.show_button, c.show_button);
        set(&mut d.smart, c.smart_cut_paste);
        if let Some(m) = &c.smart {
            set(&mut d.smart_spacing, m.word_spacing);
            set(&mut d.smart_paragraphs, m.paragraph_spacing);
            set(&mut d.smart_tables, m.tables);
            set(&mut d.smart_styles, m.styles);
            set(&mut d.merge_lists, m.merge_lists);
            set(&mut d.from_powerpoint, m.from_power_point);
            set(&mut d.from_excel, m.from_excel);
        }
        if !ok || !self.set_paste_defaults(d) {
            return Outcome::invalid("within, between and other take keepSource, merge or textOnly; conflict also destinationStyles");
        }
        Outcome::Moved
    }

    /// A picture from the clipboard pasted in line in place of the selection (a selected picture included), as one
    /// step: sized as Insert Picture sizes it, wrapped as pictures are inserted, the caret after it. Over a picture
    /// control's picture it fills the control, fitted to its box, as Change Picture does.
    pub fn paste_picture(&mut self, data: &[u8], fonts: &Fonts) -> Outcome {
        use crate::docx::diagnostics::WarningCode;
        let refused = |e: &mut Editor, why: &'static str| {
            e.events.push(super::input::Notice::Warning(WarningCode::ImageUndecodable, format!("the clipboard's picture wasn't pasted: {why}")));
            Outcome::unavailable(why)
        };
        if data.len() > MAX_PICTURE {
            return refused(self, "it is too large");
        }
        if crate::docx::image::image_info(data).is_none() || crate::docx::image::image_type(data).is_none() {
            return refused(self, "it isn't a picture format the engine draws");
        }
        let bytes = crate::api::value::Bytes(data.to_vec());
        if let Some(c) = self.selected_picture_control() {
            let id = super::merge::api_id(crate::docx::ElementId::Control(c));
            let fill = crate::api::command::FillControl { id, text: None, image: Some(bytes), docx: None, building_block: None, date: None };
            return self.joined("Paste", fonts, |e| e.fill_control(fill, fonts));
        }
        let p = crate::api::command::Picture { data: bytes, alt_text: None };
        self.joined("Paste", fonts, |e| {
            // Over a selected object: it goes, and the picture takes its place.
            let mut gone = false;
            if matches!(e.selection, Selection::Object(_)) {
                match e.delete_object(fonts) {
                    Some(o @ Outcome::Refused(_)) => return o,
                    Some(Outcome::Changed(_)) => gone = true,
                    _ => {}
                }
            }
            match e.insert_picture_with(p, fonts) {
                // The caret after what was pasted, as a paste leaves it
                out @ Outcome::Changed(_) => {
                    e.caret_after_object();
                    out
                }
                // The object stays without a picture in its place.
                Outcome::Unchanged(why) if gone => Outcome::unavailable(why),
                Outcome::Moved if gone => Outcome::unavailable("the picture wasn't inserted"),
                out => out,
            }
        })
    }

    /// The selected object, just inserted, left for a caret after it (an inline one), or at its anchor (a float).
    fn caret_after_object(&mut self) {
        let Selection::Object(o) = self.selection else { return };
        let (para, offset) = match o {
            ObjectRef::Inline { para, offset } => (para, Unit(offset.0 + 1)),
            ObjectRef::Floating(f) => {
                let Some(para) = self.index.float_anchor(f) else { return };
                let Some(p) = self.index.para(&self.doc, para) else { return };
                let k = p.inlines.iter().position(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f)).unwrap_or(0);
                (para, Unit(p.inlines[..k].iter().map(|i| i.units() as u32).sum()))
            }
        };
        let story = self.index.para_slot(para).map_or(super::position::StoryId::Body, |s| self.index.story(s.container));
        self.set_selection(Selection::Caret(Position::text(story, para, offset)));
    }

    /// The text of HTML as Keep Text Only pastes it.
    fn html_text(&self, html: &str) -> Option<String> {
        super::html::text_only(html, self.paste_defaults.keep_bullets)
    }

    /// `f` made to take the look of where it lands (Merge Formatting): each paragraph the caret paragraph's
    /// properties (its own list kept), each run the formatting at the caret with the bold, italic and underline
    /// applied to only part of `f` kept; hyperlinks keep their style. Its structure, pictures and fields stay.
    pub(super) fn merged(&self, f: &Fragment) -> Option<Fragment> {
        let res = self.doc.resolver.clone()?;
        let caret = self.selection.ordered(&self.index).map(|(a, _)| a)?;
        let (para, _) = caret.para_offset()?;
        let dest = self.index.para(&self.doc, para)?;
        let run = self.run_at(&caret)?;
        // Bold, italic and underline over all of the text are dropped, as Word does.
        let (mut all, mut any) = ([true; 3], false);
        each_para(&f.blocks, &mut |p| {
            for r in p.inlines.iter().filter(|i| matches!(i, Inline::Text(t, _) if !t.is_empty())).filter_map(Inline::run) {
                any = true;
                all[0] &= r.bold;
                all[1] &= r.italic;
                all[2] &= r.underline.is_some();
            }
        });
        if !any {
            all = [false; 3];
        }
        // The destination's own formatting, not its list
        let dest_props = crate::format::ParaProps { list: None, list_level: None, ..(**dest.style.direct.get()).clone() };
        let mut out = f.clone();
        out.look = Look::Destination;
        each_para_mut(&mut out.blocks, &mut |p| {
            // Per paragraph: its base is its own.
            let mut cache: HashMap<(*const RunStyle, [bool; 3]), Run> = HashMap::new();
            let mut props = dest_props.clone();
            if let Some(dest_id) = &dest.style.style_id {
                props.style = Some(dest_id.as_str().into());
            }
            if let Some(ListRef { list, level }) = p.style.num {
                (props.list, props.list_level) = (Some(ListChoice::In(list)), Some(level));
            }
            let direct = crate::docx::para_props::ParaDirect::typed(Rc::new(props));
            let resolved = crate::docx::para_xml::with(&res, direct, None, None);
            let base = resolved.base.clone();
            p.style = Rc::new(resolved.style);
            let mark_src = p.mark.clone();
            p.mark = merged_run(&res, &base, &run, &mark_src, [true; 3], &mut cache);
            for i in &mut p.inlines {
                if let Some(r) = i.run().cloned() {
                    let new = merged_run(&res, &base, &run, &r, all, &mut cache);
                    *i = super::format::with_run(i, &new);
                }
            }
        });
        Some(out)
    }

    /// The list items of `blocks` (not those in cells) made items of list `num`, each at its level: Merge Pasted
    /// Lists with Surrounding Lists, for a paste landing in an item of `num`.
    pub(super) fn join_list(&self, blocks: &mut [Block], num: i64) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        for b in blocks {
            let Block::Para(p) = b else { continue };
            let Some(ListRef { list: ListId(_), level: lvl }) = p.style.num.filter(|&ListRef { list: ListId(n), level: _ }| n != num) else {
                continue;
            };
            let mut e = ParaEdit::new((**p.style.direct.get()).clone());
            e.list = Some(if num == 0 { ListChoice::None } else { ListChoice::In(ListId(num)) });
            e.list_level = Some(lvl);
            e.whole(ParaProp::List);
            let direct = p.style.direct.edit(e);
            p.style = Rc::new(crate::docx::para_xml::of(&res, &direct, None, &cx).style);
        }
    }
}

impl Editor {
    /// The selected object (or objects) as a paragraph to copy: an inline object as its place in its paragraph, the
    /// selected floats in their anchor's paragraph, a selected group child as a float of its own where it is drawn.
    pub(super) fn copy_object(&mut self, o: ObjectRef) -> Option<Paragraph> {
        let (para, inlines) = match o {
            ObjectRef::Inline { para, offset } => {
                let p = self.index.para(&self.doc, para)?;
                (para, super::clipboard::slice(p, offset, Unit(offset.0 + 1)))
            }
            ObjectRef::Floating(f) => {
                let inlines = match self.object_child.clone().filter(|(g, c)| *g == f && !c.is_empty()) {
                    Some((g, path)) => vec![self.child_float(g, &path)?],
                    None => self.selected_floats().into_iter().filter_map(|g| self.float_inline_of(g)).collect(),
                };
                (self.index.float_anchor(f)?, inlines)
            }
        };
        if inlines.is_empty() {
            return None;
        }
        let mut q = self.index.para(&self.doc, para)?.clone();
        q.inlines = inlines;
        Some(q)
    }

    /// Float `f` as it is in its anchor's paragraph.
    fn float_inline_of(&self, f: FloatId) -> Option<Inline> {
        let p = self.index.para(&self.doc, self.index.float_anchor(f)?)?;
        p.inlines.iter().find(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f)).cloned()
    }

    /// The child at `path` of group `g` as a float of its own, where and as it is drawn: what a copy of a selected
    /// group child holds.
    fn child_float(&mut self, g: FloatId, path: &[u16]) -> Option<Inline> {
        use crate::drawing::units::Emu;
        let Some(Inline::Float(m, run)) = self.float_inline_of(g) else { return None };
        let tree = m.shape.as_deref()?;
        let (page, r) = self.laid_float_box(g)?;
        let emu = |v: f64| Emu((v * 12700.0).round() as i64);
        // The group's own box, unswapped about its laid-out centre
        let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        let (gw, gh) = if m.rot_swap { (r[3] - r[1], r[2] - r[0]) } else { (r[2] - r[0], r[3] - r[1]) };
        let own = [cx - gw / 2.0, cy - gh / 2.0, cx + gw / 2.0, cy + gh / 2.0];
        let (rect, deg, flip_h, flip_v) = tree.frame_at(own, path)?;
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let mut s = tree.at(path)?.clone();
        s.xfrm.off = [Emu(0), Emu(0)];
        s.xfrm.ext = [emu(w), emu(h)];
        (s.xfrm.flip_h, s.xfrm.flip_v) = (flip_h, flip_v);
        // Its unturned box in the group's, where its parts are measured from
        let (mut at, mut shape) = (own, tree);
        for &k in path {
            let gs = shape.group()?;
            let [sx, sy] = gs.scale(at);
            let c = gs.children.get(usize::from(k))?;
            let (x, y) = (at[0] + (c.xfrm.off[0] - gs.child_off[0]).to_pt() * sx, at[1] + (c.xfrm.off[1] - gs.child_off[1]).to_pt() * sy);
            at = [x, y, x + c.xfrm.ext[0].to_pt() * sx, y + c.xfrm.ext[1].to_pt() * sy];
            shape = c;
        }
        let (dx, dy) = (at[0] - r[0], at[1] - r[1]);
        let parts = m
            .parts
            .iter()
            .filter(|p| p.path.len() > path.len() && p.path.starts_with(path))
            .map(|p| crate::docx::GroupPart {
                rect: [p.rect[0] - dx, p.rect[1] - dy, p.rect[2] - dx, p.rect[3] - dy],
                path: p.path[path.len()..].to_vec(),
                ..p.clone()
            })
            .collect();
        let mut n = crate::docx::Float {
            id: crate::docx::next_para_id(),
            w,
            h,
            image: s.pic().and_then(|p| p.blip.image),
            text_box: m.parts.iter().find(|p| p.path == path).and_then(|p| p.text_box.clone()),
            parts,
            shape: Some(Box::new(s)),
            rot_swap: false,
            vml: false,
            relative: Default::default(),
            src: None,
            frame_src: None,
            in_note: false,
            ..(*m).clone()
        };
        super::shapes::rotate_to(&mut n, &[], deg, self.doc.settings.policy.mode);
        // Placed where it is drawn, from the group's place
        let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
        let (nw, nh) = if n.rot_swap { (n.h, n.w) } else { (n.w, n.h) };
        // An aligned or percentage place as the offset from its reference the group is drawn at
        let unplace = [m.h_align.is_some() || m.relative.pos[0].is_some(), m.v_align.is_some() || m.relative.pos[1].is_some()];
        let (h, v) = if unplace.contains(&true) {
            let at = self.placed_float(g, page, r, r, unplace)?;
            (n.h_align, n.v_align, n.relative.pos) = (at.h_align, at.v_align, at.relative.pos);
            (at.h_offset, at.v_offset)
        } else {
            (m.h_offset, m.v_offset)
        };
        n.h_offset = h + (cx - nw / 2.0) - r[0];
        n.v_offset = v + (cy - nh / 2.0) - r[1];
        Some(Inline::Float(Box::new(n), run))
    }

    /// Float `f`'s box, and the page it is laid out on, among those laid out.
    fn laid_float_box(&mut self, f: FloatId) -> Option<(usize, [f64; 4])> {
        let n = self.pages.len();
        let near = self.near.min(n.saturating_sub(1));
        (0..n).flat_map(|d| [near.checked_sub(d), (d > 0).then_some(near + d)]).flatten().filter(|&p| p < n).find_map(|p| {
            let geo = self.pages.page(p, &self.index)?;
            geo.stories.iter().find(|s| s.float == Some(f)).map(|s| (p, s.rect))
        })
    }

    /// Copies the selection as [`Editor::copy`] does; a selected object (or objects) drawn as it looks
    /// ([`Fragment::image`], and its HTML's picture). Other copies' objects are drawn when their HTML is asked for
    /// ([`Editor::drawings`]): a copy draws at most the objects selected.
    pub fn copy_drawn(&mut self, fonts: &Fonts) -> Option<Fragment> {
        let mut f = self.clip()?;
        if matches!(self.selection, super::selection::Selection::Object(_)) {
            self.draw_objects(&mut f, fonts);
        }
        Some(self.copied_to_clipboard(f))
    }

    /// The objects of `f`, copied here, laid out to be drawn into [`Fragment::drawn`] ([`Editor::drawings`]); with the
    /// selection an object, its drawing [`Fragment::image`] too (one object drawn whatever its size: its picture is
    /// cut down to the largest side a clipboard picture has).
    pub fn draw_objects(&self, f: &mut Fragment, fonts: &Fonts) {
        use super::selection::{ObjectRef, Selection};
        let floats = self.selected_floats();
        let (image, drawn) = match self.selection {
            Selection::Object(ObjectRef::Floating(_)) if floats.len() > 1 => (self.draw_floats(&floats), self.drawings(f, fonts)),
            Selection::Object(_) => {
                let (mut all, mut unbounded) = (Vec::new(), u64::MAX);
                each_object(&f.blocks, &mut |i| all.push(self.draw_object(i, fonts, &mut unbounded)));
                match all.as_slice() {
                    [one] => (one.clone(), all),
                    _ => (None, self.drawings(f, fonts)),
                }
            }
            _ => (None, self.drawings(f, fonts)),
        };
        f.image = image;
        f.drawn = drawn.into();
    }

    /// The objects of `f`, copied here, laid out to be drawn, in the order its HTML meets them, at most
    /// [`MAX_DRAWN_PIXELS`] in all (`None` past it, and for one that draws nothing).
    pub fn drawings(&self, f: &Fragment, fonts: &Fonts) -> Vec<Option<Drawing>> {
        let mut budget = MAX_DRAWN_PIXELS;
        let mut drawn = Vec::new();
        each_object(&f.blocks, &mut |i| drawn.push(self.draw_object(i, fonts, &mut budget)));
        drawn
    }

    /// One object of a copy (an inline picture or shape, or a float, drawn in line as a note's is), as it looks;
    /// `None` for one that draws nothing or would take more than `budget` pixels, which it takes from.
    fn draw_object(&self, i: &Inline, fonts: &Fonts, budget: &mut u64) -> Option<Drawing> {
        let (w, h, effect) = match i {
            Inline::Picture(p, _) => (p.w, p.h, p.effect),
            Inline::Float(f, _) if f.shape.is_some() || f.image.is_some() => (f.w, f.h, f.effect),
            _ => return None,
        };
        let k = 96.0 / 72.0;
        let px = ((w + effect[0] + effect[2]).max(1.0) * k).ceil() * ((h + effect[1] + effect[3]).max(1.0) * k).ceil();
        if !px.is_finite() || px > *budget as f64 {
            return None;
        }
        // Shown as it shows unmarked: not hidden, no revision's colour.
        let mut run = RunStyle::clone(i.run()?);
        run.hidden = false;
        run.rev = Default::default();
        let run: Run = Rc::new(run);
        let obj = match i {
            Inline::Picture(p, _) => Inline::Picture(p.clone(), run.clone()),
            Inline::Float(f, _) => {
                let mut f = f.clone();
                f.in_note = true;
                Inline::Float(f, run.clone())
            }
            _ => return None,
        };
        // Where it lands on the line is cut away: what it draws is what's kept.
        let style = crate::docx::para_xml::resolve(self.doc.resolver.as_ref()?, &Default::default(), None, None).style;
        let para = Paragraph {
            style: Rc::new(style),
            mark: run,
            list: None,
            inlines: vec![obj],
            src: None,
            id: crate::docx::next_para_id(),
            controls: Vec::new(),
            div: None,
        };
        let (_, children) = crate::layout::expansion::lay(&self.doc, &[Block::Para(para)], DRAW_WIDTH, fonts, None, self.layout.hyphenation());
        let items = crate::layout::traverse::clipped_items_of(&children);
        if items.is_empty() {
            return None;
        }
        *budget -= px as u64;
        Some(items.into())
    }

    /// Selected floats drawn together, where they are on the first page showing the first: the picture a copy of
    /// several objects puts on the clipboard.
    fn draw_floats(&self, floats: &[crate::layout::FloatId]) -> Option<Drawing> {
        let first = *floats.first()?;
        let snap = self.snapshot();
        let n = snap.pages().len();
        let near = self.near.min(n.saturating_sub(1));
        // Outward from the page last shown, where the selection is.
        let items = (0..n).flat_map(|d| [near.checked_sub(d), (d > 0).then_some(near + d)]).flatten().filter(|&k| k < n).find_map(|k| {
            let pg = snap.pages().get(k)?;
            let mut items = crate::layout::traverse::paint_float_clipped(pg, first);
            if items.is_empty() {
                return None;
            }
            items.extend(floats[1..].iter().flat_map(|f| crate::layout::traverse::paint_float_clipped(pg, *f)));
            Some(items)
        })?;
        Some(items.into())
    }

    /// Pasted paragraphs from another document whose style differs here (`from`'s of the same name) given their own
    /// look as formatting applied directly, as Keep Source Formatting gives it: what they showed there, they show here.
    pub(super) fn keep_para_looks(&self, blocks: &mut [Block], from: &Resolvers) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let cx = self.cx();
        let (theirs, ours) = (from.typed(), res.typed());
        // A style this document lacks comes with the paste, its look with it.
        let differs = |id: Option<&str>| {
            if id.is_some_and(|id| ours.cascade.styles.get(id).is_none()) {
                return false;
            }
            let (a, b) = (theirs.cascade.paragraph_style(id), ours.cascade.paragraph_style(id));
            !ours.cascade.chain(b).is_empty() && chains_differ(&theirs, &ours, a, b)
        };
        let mut seen: HashMap<Option<String>, bool> = HashMap::new();
        let mut looks: HashMap<*const RunStyle, Run> = HashMap::new();
        each_para_mut(blocks, &mut |p| {
            let id = p.style.style_id.clone();
            if !*seen.entry(id.clone()).or_insert_with(|| differs(id.as_deref())) {
                return;
            }
            let here = crate::docx::para_xml::of(&res, &p.style.direct, None, &cx);
            let mut e = ParaEdit::new((**p.style.direct.get()).clone());
            if !para_look(&p.style, &here.style, &mut e) {
                return;
            }
            let again = crate::docx::para_xml::of(&res, &p.style.direct.edit(e), None, &cx);
            p.mark = self.keep_look(&p.mark, &again.base, &mut looks);
            p.style = Rc::new(again.style);
        });
    }
}

/// What `src` shows that `here` (its look under this document's styles) doesn't, set on the paragraph's authored
/// properties `e`; whether anything was.
fn para_look(src: &crate::docx::ParaStyle, here: &crate::docx::ParaStyle, e: &mut ParaEdit) -> bool {
    use crate::docx::props::LineRule;
    use crate::format::{Color, ParaBorders, ParaFlag, Shading, TabStop, TextBorder};
    let twip = super::format::twip;
    let mut changed = false;
    if src.jc != here.jc {
        e.align = Some(src.jc);
        e.whole(ParaProp::Align);
        changed = true;
    }
    if (src.ind_left, src.ind_right, src.ind_first) != (here.ind_left, here.ind_right, here.ind_first) {
        (e.indent_left, e.indent_right) = (Some(twip(src.ind_left)), Some(twip(src.ind_right)));
        e.indent_first = Some(if src.ind_first < 0.0 { -twip(-src.ind_first) } else { twip(src.ind_first) });
        e.whole(ParaProp::IndentLeft);
        changed = true;
    }
    let spacing = |s: &crate::docx::ParaStyle| (s.space_before, s.space_after, s.before_auto, s.after_auto, s.line, s.line_rule);
    if spacing(src) != spacing(here) {
        let line = match src.line_rule {
            LineRule::Auto => (src.line * 240.0).round(),
            LineRule::Exact | LineRule::AtLeast => (src.line * 20.0).round(),
        };
        (e.space_before, e.space_after) = (Some(twip(src.space_before)), Some(twip(src.space_after)));
        (e.space_before_auto, e.space_after_auto) = (Some(src.before_auto), Some(src.after_auto));
        (e.line, e.line_rule) = (Some(line), Some(src.line_rule));
        e.whole(ParaProp::SpaceBefore);
        changed = true;
    }
    for (flag, a, b) in [
        (ParaFlag::KEEP_NEXT, src.keep_next, here.keep_next),
        (ParaFlag::KEEP_LINES, src.keep_lines, here.keep_lines),
        (ParaFlag::PAGE_BREAK_BEFORE, src.page_break_before, here.page_break_before),
        (ParaFlag::WIDOW_CONTROL, src.widow_control, here.widow_control),
        (ParaFlag::CONTEXTUAL, src.contextual, here.contextual),
        (ParaFlag::SUPPRESS_AUTO_HYPHENS, src.suppress_auto_hyphens, here.suppress_auto_hyphens),
        (ParaFlag::BIDI, src.bidi, here.bidi),
    ] {
        if a != b {
            e.flag(flag, a);
            changed = true;
        }
    }
    if src.outline_lvl != here.outline_lvl {
        e.outline_level = Some(src.outline_lvl.unwrap_or(9));
        e.whole(ParaProp::OutlineLevel);
        changed = true;
    }
    if src.shading != here.shading {
        let fill = src.shading.map_or(Color::Auto, Color::Rgb);
        e.shading = Some(Shading { pattern: crate::docx::props::ShadingPattern::Clear, color: Some(Color::Auto), fill: Some(fill) });
        e.whole(ParaProp::Shading);
        changed = true;
    }
    let sides = |s: &crate::docx::ParaStyle| {
        [s.border_top.clone(), s.border_left.clone(), s.border_bottom.clone(), s.border_right.clone(), s.border_between.clone()]
    };
    if sides(src) != sides(here) {
        // A side `src` doesn't draw is set to none, over the style's.
        let side = |b: &Option<crate::docx::props::Border>| {
            Some(match b.as_ref().filter(|b| b.visible()) {
                Some(b) => TextBorder {
                    style: b.style,
                    width: (b.width * 8.0).round() / 8.0,
                    space: b.space.round(),
                    shadow: b.shadow,
                    color: b.color_theme.map(Color::Theme).or(b.color.map(Color::Rgb)).or(Some(Color::Auto)),
                },
                None => TextBorder { style: crate::docx::props::BorderStyle::Nil, width: 0.0, space: 0.0, shadow: false, color: None },
            })
        };
        e.borders = ParaBorders {
            top: side(&src.border_top),
            left: side(&src.border_left),
            bottom: side(&src.border_bottom),
            right: side(&src.border_right),
            between: side(&src.border_between),
        };
        e.whole(ParaProp::Borders);
        changed = true;
    }
    if src.tabs != here.tabs {
        let mut tabs: Vec<TabStop> = src.tabs.iter().map(|t| TabStop { pos: twip(t.pos), kind: t.kind, leader: t.leader }).collect();
        for t in here.tabs.iter().filter(|t| !src.tabs.iter().any(|s| (s.pos - t.pos).abs() < 0.05)) {
            tabs.push(TabStop { pos: twip(t.pos), kind: crate::docx::props::TabKind::Clear, leader: crate::docx::props::Leader::None });
        }
        e.tabs = tabs;
        e.whole(ParaProp::Tabs);
        changed = true;
    }
    changed
}

/// A run of pasted text merged into where it lands: the formatting at the caret (`at`), with `src`'s bold, italic and
/// underline unless all of the text had it (`all`), and its Hyperlink style.
fn merged_run(
    res: &Resolvers,
    base: &crate::format::CharProps,
    at: &Run,
    src: &Run,
    all: [bool; 3],
    cache: &mut HashMap<(*const RunStyle, [bool; 3]), Run>,
) -> Run {
    use crate::format::Toggle as T;
    let key = (Rc::as_ptr(src), all);
    if let Some(r) = cache.get(&key) {
        return r.clone();
    }
    let mut e = CharEdit::new(at.direct.props().map_or_else(CharProps::default, |p| (**p).clone()));
    if src.bold && !all[0] {
        e.toggle(T::BOLD, true);
        e.toggle(T::BOLD_COMPLEX, true);
    }
    if src.italic && !all[1] {
        e.toggle(T::ITALIC, true);
        e.toggle(T::ITALIC_COMPLEX, true);
    }
    if let Some(style) = src.underline.filter(|_| !all[2]) {
        e.underline = Some(crate::format::UnderlineProps { style, color: None });
        e.whole(Prop::Underline);
    }
    if src.style_id.as_deref() == Some("Hyperlink") && res.has_style("Hyperlink") {
        e.style = Some("Hyperlink".into());
        e.whole(Prop::Style);
    }
    let mut r = run_xml::resolve(res, base, at.direct.edit(e), false);
    if !src.rev.is_empty() {
        r.rev = src.rev.clone();
    }
    let r = Rc::new(r);
    cache.insert(key, r.clone());
    r
}

/// Keep Text Only's text of `f`: each paragraph's text, a list paragraph's number or bullet before it when
/// `bullets`, a table's rows as lines of cells between tabs; pictures and other objects left out.
pub fn text_only(f: &Fragment, bullets: bool) -> String {
    fn walk(blocks: &[Block], bullets: bool, hidden: bool, out: &mut String) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    if bullets && let Some(l) = p.list.as_ref().filter(|l| !l.text.is_empty()) {
                        out.push_str(&l.text);
                        out.push('\t');
                    }
                    let seen = super::text::Seen::of(p, |r| hidden || !r.hidden, true);
                    out.extend(seen.chars.into_iter().filter(|c| *c != super::text::OBJECT).map(super::plain_char));
                    out.push('\n');
                }
                Block::Table(t) => {
                    for r in &t.rows {
                        for (k, c) in r.cells.iter().enumerate() {
                            if k > 0 {
                                out.push('\t');
                            }
                            let mut s = String::new();
                            walk(&c.blocks, bullets, hidden, &mut s);
                            out.push_str(s.trim_end_matches('\n').replace('\n', " ").as_str());
                        }
                        out.push('\n');
                    }
                }
            }
        }
    }
    let mut s = String::new();
    walk(&f.blocks, bullets, f.hidden_text, &mut s);
    if f.open_end && s.ends_with('\n') {
        s.pop();
    }
    s
}

/// Every object of `blocks` (inline pictures and shapes, and floats), table cells' included, in document order: the
/// order a fragment's HTML meets them.
fn each_object<'a>(blocks: &'a [Block], f: &mut dyn FnMut(&'a Inline)) {
    for b in blocks {
        match b {
            Block::Para(p) => p.inlines.iter().filter(|i| matches!(i, Inline::Picture(..) | Inline::Float(..))).for_each(&mut *f),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| each_object(&c.blocks, f)),
        }
    }
}

/// Every paragraph of `blocks`, table cells' included (not text boxes', which keep their own look).
fn each_para<'a>(blocks: &'a [Block], f: &mut dyn FnMut(&'a Paragraph)) {
    for b in blocks {
        match b {
            Block::Para(p) => f(p),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| each_para(&c.blocks, f)),
        }
    }
}

fn each_para_mut(blocks: &mut [Block], f: &mut dyn FnMut(&mut Paragraph)) {
    for b in blocks {
        match b {
            Block::Para(p) => f(p),
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| each_para_mut(&mut c.blocks, f)),
        }
    }
}

/// Whether style `a`'s chain in `from` and `b`'s in `here` differ: in length, or in a style written differently.
fn chains_differ(from: &TypedDefs, here: &TypedDefs, a: Option<&str>, b: Option<&str>) -> bool {
    let (ca, cb) = (from.cascade.chain(a), here.cascade.chain(b));
    ca.len() != cb.len() || ca.iter().zip(&cb).any(|(x, y)| from.style_xml(&x.style.id) != here.style_xml(&y.style.id))
}

/// Tests: the id of every float of `blocks`, in document order.
#[cfg(test)]
pub(super) fn tests_each_float(blocks: &[Block], out: &mut dyn FnMut(u64)) {
    each_object(blocks, &mut |i| {
        if let Inline::Float(f, _) = i {
            out(f.id);
        }
    });
}
