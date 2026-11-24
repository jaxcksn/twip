//! Section page setup

use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::index::Slot;
use super::position::{Position, StoryId};
use super::readback::BorderLine;
use super::selection::{ObjectRef, Selection};
use super::step::Step;
use crate::docx::NoteKind;
use crate::layout::Unit;

#[derive(Clone, Debug, PartialEq)]
pub enum SectionCmd {
    Orientation(bool),
    Size(f64, f64),
}

impl Editor {
    pub(super) fn focus_section(&self) -> usize {
        let slot = match self.selection {
            Selection::Cells { table, .. } => self.index.table_slot(table),
            Selection::Object(ObjectRef::Inline { para, .. }) => self.index.para_slot(para),
            Selection::Object(ObjectRef::Floating(f)) => self.index.float_anchor(f).and_then(|p| self.index.para_slot(p)),
            _ => self.selection.focus().and_then(|p| match p.place {
                super::position::Place::RowEnd { table, .. } => self.index.table_slot(table),
                _ => p.para_offset().and_then(|(p, _)| self.index.para_slot(p)),
            }),
        };
        slot.and_then(|s| self.slot_section(s)).unwrap_or(0)
    }

    /// The section a block is in: a note's is its reference's, a header's or footer's the shown page's, and in a
    /// sub-template being edited, the section it was entered from.
    fn slot_section(&self, s: Slot) -> Option<usize> {
        if let Some((sec, ..)) = self.index.top_block(s.container, s.block) {
            return Some(match &self.surface {
                Some(shown) if self.index.story(s.container) != StoryId::Body => shown.section,
                _ => sec,
            });
        }
        let (kind, id) = match self.index.host(self.index.story(s.container)) {
            StoryId::HeaderFooter(k) => return self.part_section(k),
            StoryId::Footnote(id) => (NoteKind::Foot, id),
            StoryId::Endnote(id) => (NoteKind::End, id),
            _ => return None,
        };
        let (at, _) = self.note_reference(kind, id)?;
        let s = self.index.para_slot(at.para_offset()?.0)?;
        self.index.top_block(s.container, s.block).map(|(sec, ..)| sec)
    }

    /// The section header or footer part `k` is edited for: the shown page's when that page shows it, else the first
    /// that refers to it.
    fn part_section(&self, k: u32) -> Option<usize> {
        let name = self.index.part_name(k)?;
        let sections = &self.doc.sections;
        if let Some(i) = self.snapshot().info().get(self.near).filter(|i| i.section < sections.len()) {
            let shown =
                |footer| crate::layout::page::story_ref(sections, self.doc.settings.even_and_odd, i.section, i.section_page, i.number, footer).1;
            if [false, true].into_iter().any(|footer| shown(footer).is_some_and(|n| n == name)) {
                return Some(i.section);
            }
        }
        sections.iter().position(|s| s.headers.iter().chain(s.footers.iter()).flatten().any(|n| n == name))
    }

    /// A section's `gutter` on a `pw` × `ph` page, where layout puts it: its share of the width and of the height.
    pub(super) fn gutter_axes(&self, gutter: f64, pw: f64, ph: f64) -> (f64, f64) {
        let set = &self.doc.settings;
        match set.book_fold {
            // Portrait book fold pages drop it
            Some(_) => (if pw > ph { gutter } else { 0.0 }, 0.0),
            None if set.gutter_at_top => (0.0, gutter),
            None => (gutter, 0.0),
        }
    }

    /// The section a position in the body is in.
    pub(super) fn section_at(&self, at: &super::position::Position) -> Option<usize> {
        if matches!(at.story, StoryId::HeaderFooter(_)) {
            return None;
        }
        let slot = match at.place {
            super::position::Place::Text { para, .. } => self.index.para_slot(para)?,
            // A row's end is in its table's section.
            super::position::Place::RowEnd { table, .. } => self.index.table_slot(table)?,
        };
        self.top_section(slot)
    }

    /// The section a paragraph lays out in: in a sub-template being edited, the section it was entered from.
    pub(super) fn para_section(&self, para: crate::layout::ParaId) -> Option<usize> {
        self.top_section(self.index.para_slot(para)?)
    }

    fn top_section(&self, slot: Slot) -> Option<usize> {
        let (section, _, _) = self.index.top_block(slot.container, slot.block)?;
        match &self.surface {
            Some(s) if self.index.story(slot.container) != StoryId::Body => Some(s.section),
            _ => Some(section),
        }
    }

    /// The selection's section's index.
    pub(super) fn selection_section(&self) -> usize {
        self.focus_section()
    }

    /// The sections the selection covers, every range's and block of cells' of a discontiguous one, in order; the
    /// focus's alone outside the body's text.
    pub(super) fn selected_sections(&self) -> Vec<usize> {
        if let Some(r) = &self.section_scope {
            return r.clone().collect();
        }
        let one = self.focus_section();
        let own = self.selection.ordered(&self.index);
        let blocks = if self.is_discontiguous() { self.cell_blocks() } else { Vec::new() };
        let cells = blocks.iter().filter_map(|c| self.cells_start(c)).map(|p| (p, p));
        let mut out: Vec<usize> = Vec::new();
        for (a, b) in self.other_ranges().iter().copied().chain(own).chain(cells) {
            match (self.section_at(&a), self.section_at(&b)) {
                (Some(x), Some(y)) => out.extend(x.min(y)..=x.max(y)),
                _ => out.push(one),
            }
        }
        out.sort_unstable();
        out.dedup();
        if out.is_empty() {
            out.push(one);
        }
        out
    }

    /// Runs the section command `f` over `scope`'s sections, as one action: the selection's (as it does alone), every
    /// section, or from the selection on (the selection collapses to its start, a new-page section break goes there,
    /// and the sections from the new one to the last take the change; selected text is kept).
    pub fn in_section_scope(
        &mut self,
        scope: crate::api::services::ApplyTo,
        label: &'static str,
        f: impl FnOnce(&mut Editor) -> Outcome,
        fonts: &Fonts,
    ) -> Outcome {
        use crate::api::services::ApplyTo;
        if scope == ApplyTo::Section {
            return f(self);
        }
        self.joined(label, fonts, |e| {
            if scope == ApplyTo::ThisPointForward {
                // The selection's start
                let Some((start, _)) = e.selection.ordered(&e.index) else {
                    return Outcome::unavailable("this point forward starts at a caret or in text");
                };
                e.multi = super::multi::Multi::default();
                e.set_selection(Selection::Caret(start));
                let out = e.section_break(crate::api::command::SectionBreakKind::NextPage, fonts);
                if !matches!(out, Outcome::Changed(_)) {
                    return out;
                }
            }
            let first = if scope == ApplyTo::WholeDocument { 0 } else { e.focus_section() };
            let outer = e.section_scope.replace(first..=e.doc.sections.len().saturating_sub(1));
            let out = f(e);
            e.section_scope = outer;
            out
        })
    }

    /// Sets text columns where Word's Columns dialog applies them: the selected sections, every section, from the
    /// selection's start on (a continuous section break there, a next-column one when `start_new_column`), or the
    /// selected text alone (continuous section breaks around it, none at its section's edges). One undo step.
    pub fn set_columns_in(&mut self, c: crate::api::command::Columns, scope: ColumnsScope, start_new_column: bool, fonts: &Fonts) -> Outcome {
        use crate::api::command::{Command, SectionBreakKind};
        if start_new_column && scope != ColumnsScope::ThisPointForward {
            return Outcome::invalid("a new column starts only from this point forward");
        }
        let columns = |e: &mut Editor| e.execute(Command::SetColumns(c), fonts);
        match scope {
            ColumnsScope::Sections => columns(self),
            ColumnsScope::WholeDocument => self.in_section_scope(crate::api::services::ApplyTo::WholeDocument, "Columns", columns, fonts),
            ColumnsScope::ThisPointForward => self.joined("Columns", fonts, |e| {
                let Some((start, _)) = e.selection.ordered(&e.index) else { return Outcome::unavailable("no selection") };
                e.multi = super::multi::Multi::default();
                e.set_selection(Selection::Caret(start));
                let kind = if start_new_column { SectionBreakKind::NextColumn } else { SectionBreakKind::Continuous };
                let out = e.section_break(kind, fonts);
                if !matches!(out, Outcome::Changed(_)) {
                    return out;
                }
                let outer = e.section_scope.replace(e.focus_section()..=e.doc.sections.len().saturating_sub(1));
                let out = columns(e);
                e.section_scope = outer;
                out
            }),
            ColumnsScope::SelectedText => self.joined("Columns", fonts, |e| {
                let Some((a, b)) = e.selection.ordered(&e.index) else { return Outcome::unavailable("no selection") };
                if a.same_place(&b) {
                    return Outcome::unavailable("no text selected");
                }
                // Sections are body text's: a selection in a header, a note or a box has none to set.
                let (Some(ka), Some(kb)) = (e.section_at(&a), e.section_at(&b)) else {
                    return Outcome::unavailable("columns apply to text in the body");
                };
                let edge = |e: &Editor, p: &Position, end: bool| -> bool {
                    let Some(k) = e.section_at(p) else { return false };
                    let blocks = &e.doc.sections[k].blocks;
                    match (p.para_offset(), end) {
                        (Some((para, u)), false) => u == Unit(0) && blocks.first().and_then(super::first_para_of) == Some(para),
                        // A section's last paragraph holds its break: its end is the section's
                        (Some((para, u)), true) => blocks.last().and_then(super::last_para_of) == Some((para, u)),
                        // A row's end is the section's only as its last block's last row
                        (None, end) => {
                            let super::position::Place::RowEnd { table, row } = p.place else { return false };
                            end && e
                                .index
                                .table_slot(table)
                                .is_some_and(|s| s.container == super::index::Container::Section(k) && s.block + 1 == blocks.len())
                                && e.index.table(&e.doc, table).is_some_and(|t| row + 1 == t.rows.len())
                        }
                    }
                };
                let at_start = edge(e, &a, false);
                // An end at a later section's very start is the end of the section before it
                let (at_end, kb) = if kb > ka && edge(e, &b, false) { (true, kb - 1) } else { (edge(e, &b, true), kb) };
                // A break can't go inside a table
                if (!at_start && a.para_offset().is_none()) || (!at_end && b.para_offset().is_none()) {
                    return Outcome::unavailable("a section break can't split a table");
                }
                e.multi = super::multi::Multi::default();
                if !at_end {
                    e.set_selection(Selection::Caret(b));
                    let out = e.section_break(SectionBreakKind::Continuous, fonts);
                    if !matches!(out, Outcome::Changed(_)) {
                        return out;
                    }
                }
                e.set_selection(Selection::Caret(a));
                if !at_start {
                    let out = e.section_break(SectionBreakKind::Continuous, fonts);
                    if !matches!(out, Outcome::Changed(_)) {
                        return out;
                    }
                }
                // The sections the text is in: a break at its start moved it one section on, and its end's section is
                // the one before a break there.
                let shift = usize::from(!at_start);
                let outer = e.section_scope.replace(ka + shift..=kb + shift);
                let out = columns(e);
                e.section_scope = outer;
                out
            }),
        }
    }

    /// The columns Word's Columns preset `preset` gives the selection's section: equal ones half an inch apart, or
    /// two with the narrow one (as wide as one of three) on the left or right; its line between kept.
    pub fn column_preset(&self, preset: ColumnPreset) -> crate::api::command::Columns {
        use crate::api::command::{ColumnWidth, Columns};
        use crate::api::value::Points;
        const SPACE: f64 = 36.0;
        let s = self.doc.sections.get(self.focus_section());
        let separator = s.map(|s| s.columns.separator);
        let width = s.map_or(468.0, |s| s.page_w - s.right - s.left - self.gutter_axes(s.gutter, s.page_w, s.page_h).0);
        let equal = |n| Columns { count: n, spacing: Some(Points(SPACE)), separator, widths: None, apply_to: None, start_new_column: None };
        let narrow = ((width - 2.0 * SPACE) / 3.0).max(1.0);
        let wide = (width - SPACE - narrow).max(1.0);
        let two = |a: f64, b: f64| Columns {
            count: 2,
            spacing: Some(Points(SPACE)),
            separator,
            widths: Some(vec![ColumnWidth { width: Points(a), spacing: Points(SPACE) }, ColumnWidth { width: Points(b), spacing: Points(0.0) }]),
            apply_to: None,
            start_new_column: None,
        };
        match preset {
            ColumnPreset::One => equal(1),
            ColumnPreset::Two => equal(2),
            ColumnPreset::Three => equal(3),
            ColumnPreset::Left => two(narrow, wide),
            ColumnPreset::Right => two(wide, narrow),
        }
    }

    /// Sets the page borders of the selection's section, or of every section.
    pub(super) fn set_page_borders(&mut self, c: crate::api::command::PageBordersChange, fonts: &Fonts) -> Outcome {
        let shadow = c.shadow;
        let line = |l: &crate::api::command::Border| BorderLine::of_api(l, 0.0, false);
        if let Some(why) = [&c.top, &c.left, &c.bottom, &c.right].into_iter().flatten().flatten().find_map(|l| line(l).check().err()) {
            return Outcome::invalid(&why);
        }
        let given = [&c.top, &c.left, &c.bottom, &c.right].map(|e| match e {
            Some(Some(l)) => (l.space.is_some(), l.shadow.is_some()),
            _ => (false, false),
        });
        let edit = PageBordersEdit {
            top: c.top.as_ref().map(|e| e.as_ref().map(line)),
            left: c.left.as_ref().map(|e| e.as_ref().map(line)),
            bottom: c.bottom.as_ref().map(|e| e.as_ref().map(line)),
            right: c.right.as_ref().map(|e| e.as_ref().map(line)),
            offset_from: c.offset_from,
            display: c.display,
            surround_header: c.surround_header,
            surround_footer: c.surround_footer,
            behind_text: c.behind_text,
            apply_to: c.apply_to,
        };
        // Where a line gives no distance or shadow of its own: an edge keeps its distance (a new one takes Word's),
        // and the shadow is the far edges', as given, else each edge's as it was.
        self.edit_page_borders(&edit, Some((shadow, given)), fonts)
    }

    /// Sets the page borders of the selection's section, or of every section: each edge's line, colour (RGB or a
    /// theme colour), distance and shadow, what they're measured from, the pages they show on, whether they go around
    /// the header and footer, and whether they're behind the text.
    pub fn set_page_border_edges(&mut self, c: &PageBordersEdit, fonts: &Fonts) -> Outcome {
        for l in [&c.top, &c.left, &c.bottom, &c.right].into_iter().flatten().flatten() {
            if let Err(why) = l.check() {
                return Outcome::invalid(&why);
            }
        }
        self.edit_page_borders(c, None, fonts)
    }

    /// The page borders of section `section` as read back.
    pub fn page_borders_of(&self, section: usize) -> Option<PageBordersReadback> {
        use crate::api::command::{PageBorderDisplay, PageBorderOffset};
        let s = self.doc.sections.get(section)?;
        let b = s.page_borders.as_ref();
        let edge = |x: Option<&Option<crate::docx::props::Border>>| x.and_then(|x| x.as_ref()).and_then(BorderLine::of);
        Some(PageBordersReadback {
            top: edge(b.map(|b| &b.top)),
            left: edge(b.map(|b| &b.left)),
            bottom: edge(b.map(|b| &b.bottom)),
            right: edge(b.map(|b| &b.right)),
            offset_from: if b.is_none_or(|b| b.offset_from_page) { PageBorderOffset::Page } else { PageBorderOffset::Text },
            display: match b.map(|b| b.display.as_str()) {
                Some("firstPage") => PageBorderDisplay::FirstPage,
                Some("notFirstPage") => PageBorderDisplay::NotFirstPage,
                _ => PageBorderDisplay::AllPages,
            },
            surround_header: !self.doc.settings.borders_do_not_surround_header,
            surround_footer: !self.doc.settings.borders_do_not_surround_footer,
            behind_text: b.is_some_and(|b| b.behind_text),
        })
    }

    /// Sets page borders by `c`; `legacy` (the shadow given, if any) when the lines' distances and shadows aren't.
    fn edit_page_borders(&mut self, c: &PageBordersEdit, legacy: Option<(Option<bool>, [(bool, bool); 4])>, fonts: &Fonts) -> Outcome {
        use crate::api::command::{PageBorderDisplay, PageBorderOffset, PageBorderScope};
        let focus = self.focus_section();
        let in_scope = |section: usize| c.apply_to == PageBorderScope::WholeDocument || section == focus;
        let colours: Vec<Option<super::theme::WordColour>> = {
            let mut v = Vec::with_capacity(4);
            for e in [&c.top, &c.left, &c.bottom, &c.right] {
                v.push(match e {
                    Some(Some(l)) => match super::readback::word_colour_or_auto(self, &l.color) {
                        Ok(c) => c,
                        Err(why) => return Outcome::invalid(&why),
                    },
                    _ => None,
                });
            }
            v
        };
        let cx = self.cx();
        let w = cx.w.clone();
        let mut tx = self.begin("Page Borders");
        let mut changed = false;
        for section in 0..self.doc.sections.len() {
            let s = &self.doc.sections[section];
            let mut page = s.page();
            // Around the header and footer: a document setting, every section's borders following it.
            if !in_scope(section) {
                let Some(b) = page.page_borders.as_mut() else { continue };
                b.surround_header = c.surround_header.unwrap_or(b.surround_header);
                b.surround_footer = c.surround_footer.unwrap_or(b.surround_footer);
                if page == s.page() {
                    continue;
                }
                let change = s.change.clone();
                if self.step(&mut tx, Step::SectionPage { section, page: Box::new(page), change }).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("no section");
                }
                changed = true;
                continue;
            }
            let mut b = page.page_borders.clone().unwrap_or(crate::docx::PageBorders {
                display: "allPages".into(),
                offset_from_page: true,
                surround_header: !self.doc.settings.borders_do_not_surround_header,
                surround_footer: !self.doc.settings.borders_do_not_surround_footer,
                ..Default::default()
            });
            if let Some(o) = c.offset_from {
                b.offset_from_page = o == PageBorderOffset::Page;
            }
            if let Some(d) = c.display {
                b.display = match d {
                    PageBorderDisplay::AllPages => "allPages",
                    PageBorderDisplay::FirstPage => "firstPage",
                    PageBorderDisplay::NotFirstPage => "notFirstPage",
                }
                .into();
            }
            if let Some(on) = c.surround_header {
                b.surround_header = on;
            }
            if let Some(on) = c.surround_footer {
                b.surround_footer = on;
            }
            if let Some(on) = c.behind_text {
                b.behind_text = on;
            }
            // A shadow given goes on the far edges; none given, each edge keeps its own (a new one the page's)
            let given = legacy.and_then(|(g, _)| g);
            let page_shadow = b.bottom.as_ref().is_some_and(|x| x.shadow);
            let own = |k: usize| legacy.map_or((true, true), |(_, g)| g[k]);
            // A new edge's space: 24 pt from the page's edge, or 1 pt (top, bottom) and 4 pt (sides) from the text.
            let space = |side: &str, page: bool| {
                if page {
                    24.0
                } else if matches!(side, "left" | "right") {
                    4.0
                } else {
                    1.0
                }
            };
            let set = |old: &Option<crate::docx::props::Border>, new: &Option<Option<BorderLine>>, k: usize, side: &str, far: bool| match new {
                None => old.clone().map(|mut o| {
                    if let Some(shadow) = given {
                        o.shadow = shadow && far;
                    }
                    o
                }),
                Some(None) => None,
                Some(Some(l)) => Some(crate::docx::props::Border {
                    style: l.style.val().into(),
                    width: l.width.0,
                    space: match own(k).0 {
                        false => old.as_ref().map_or_else(|| space(side, b.offset_from_page), |o| o.space),
                        true => l.space.0,
                    },
                    shadow: match (own(k).1, given) {
                        (true, _) => l.shadow,
                        (false, Some(s)) => s && far,
                        (false, None) => old.as_ref().map_or(page_shadow && far, |o| o.shadow),
                    },
                    color: colours[k].map(|c| c.rgb),
                    color_theme: colours[k].and_then(|c| c.theme),
                }),
            };
            b.top = set(&b.top, &c.top, 0, "top", false);
            b.left = set(&b.left, &c.left, 1, "left", false);
            b.bottom = set(&b.bottom, &c.bottom, 2, "bottom", true);
            b.right = set(&b.right, &c.right, 3, "right", true);
            let any = [&b.top, &b.left, &b.bottom, &b.right].iter().any(|x| x.is_some());
            let line = |side: &str, x: &Option<crate::docx::props::Border>| {
                x.as_ref().map_or(String::new(), |x| super::readback::border_element(&w, side, x))
            };
            let xml = any.then(|| {
                let z = if b.behind_text { format!(" {w}zOrder=\"back\"") } else { String::new() };
                let offset = if b.offset_from_page { format!(" {w}offsetFrom=\"page\"") } else { String::new() };
                let display = if b.display == "allPages" { String::new() } else { format!(" {w}display=\"{}\"", b.display) };
                format!(
                    "<{w}pgBorders{offset}{display}{z}>{}{}{}{}</{w}pgBorders>",
                    line("top", &b.top),
                    line("left", &b.left),
                    line("bottom", &b.bottom),
                    line("right", &b.right)
                )
            });
            (page.page_borders, page.page_borders_xml) = (any.then_some(b), xml);
            if page == s.page() {
                continue;
            }
            let change = s.change.clone();
            if self.step(&mut tx, Step::SectionPage { section, page: Box::new(page), change }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("no section");
            }
            changed = true;
        }
        let mut settings = crate::editing::step::PageSettings::of(&self.doc.settings);
        let was = settings;
        settings.no_surround_header = c.surround_header.map_or(settings.no_surround_header, |on| !on);
        settings.no_surround_footer = c.surround_footer.map_or(settings.no_surround_footer, |on| !on);
        if settings != was {
            if self.step(&mut tx, Step::PageSettings(settings)).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("the setting can't change");
            }
            changed = true;
        }
        if !changed {
            self.abort(tx);
            return Outcome::Unchanged("already so");
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn section_cmd(&mut self, c: SectionCmd, fonts: &Fonts) -> Outcome {
        match c {
            // The margins turn with the page, and back
            SectionCmd::Orientation(landscape) => self.set_page(
                "Orientation",
                |p| {
                    if p.page_w == p.page_h || (p.page_w > p.page_h) == landscape {
                        return;
                    }
                    (p.page_w, p.page_h) = (p.page_h, p.page_w);
                    (p.top, p.right, p.bottom, p.left) =
                        if landscape { (p.left, p.top, p.right, p.bottom) } else { (p.right, p.bottom, p.left, p.top) };
                },
                fonts,
            ),
            SectionCmd::Size(w, h) => {
                let size = |pw: f64, ph: f64| if pw > ph { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
                for section in self.selected_sections() {
                    let Some(s) = self.doc.sections.get(section) else { continue };
                    let (pw, ph) = size(s.page_w, s.page_h);
                    let (gw, gh) = self.gutter_axes(s.gutter, pw, ph);
                    if s.left + s.right + gw >= pw || s.top.abs() + s.bottom.abs() + gh >= ph {
                        return Outcome::invalid("the margins must fit the page");
                    }
                }
                self.set_page("Size", |p| (p.page_w, p.page_h) = size(p.page_w, p.page_h), fonts)
            }
        }
    }
}

/// Page borders to set: each edge set, removed (`Some(None)`) or kept (`None`), and each setting set or kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageBordersEdit {
    /// The top edge; its distance is from the page's edge or from the text, as `offset_from` says.
    pub top: Option<Option<BorderLine>>,
    /// The left edge.
    pub left: Option<Option<BorderLine>>,
    /// The bottom edge.
    pub bottom: Option<Option<BorderLine>>,
    /// The right edge.
    pub right: Option<Option<BorderLine>>,
    /// What the distances are measured from.
    pub offset_from: Option<crate::api::command::PageBorderOffset>,
    /// The pages they show on.
    pub display: Option<crate::api::command::PageBorderDisplay>,
    /// They go around the header (a document setting: every section's).
    pub surround_header: Option<bool>,
    /// They go around the footer (every section's).
    pub surround_footer: Option<bool>,
    /// Behind the text and objects they meet, rather than in front of them (Word's Always display in front, off).
    pub behind_text: Option<bool>,
    /// The sections they apply to.
    pub apply_to: crate::api::command::PageBorderScope,
}

impl From<PageBordersReadback> for crate::api::state::PageBorders {
    fn from(b: PageBordersReadback) -> crate::api::state::PageBorders {
        crate::api::state::PageBorders {
            top: b.top.map(Into::into),
            left: b.left.map(Into::into),
            bottom: b.bottom.map(Into::into),
            right: b.right.map(Into::into),
            offset_from: b.offset_from,
            display: b.display,
            surround_header: b.surround_header,
            surround_footer: b.surround_footer,
            behind_text: b.behind_text,
        }
    }
}

/// A section's page borders as read back.
#[derive(Clone, Debug, PartialEq)]
pub struct PageBordersReadback {
    /// The top edge, if any.
    pub top: Option<BorderLine>,
    /// The left edge.
    pub left: Option<BorderLine>,
    /// The bottom edge.
    pub bottom: Option<BorderLine>,
    /// The right edge.
    pub right: Option<BorderLine>,
    /// What the distances are measured from.
    pub offset_from: crate::api::command::PageBorderOffset,
    /// The pages they show on.
    pub display: crate::api::command::PageBorderDisplay,
    /// They go around the header.
    pub surround_header: bool,
    /// They go around the footer.
    pub surround_footer: bool,
    /// They're behind the text.
    pub behind_text: bool,
}

/// Where a Columns change applies, as Word's Columns dialog's Apply to offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnsScope {
    /// The sections the selection covers.
    Sections,
    /// Every section.
    WholeDocument,
    /// From the selection's start on.
    ThisPointForward,
    /// The selected text alone.
    SelectedText,
}

/// Word's Columns presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnPreset {
    /// One column.
    One,
    /// Two equal columns.
    Two,
    /// Three equal columns.
    Three,
    /// Two, the narrow one on the left.
    Left,
    /// Two, the narrow one on the right.
    Right,
}
