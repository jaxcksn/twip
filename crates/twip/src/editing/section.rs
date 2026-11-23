//! Section page setup

use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::index::Slot;
use super::position::StoryId;
use super::selection::{ObjectRef, Selection};
use super::step::Step;
use crate::docx::NoteKind;

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
            _ => self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para_slot(p)),
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
        self.para_section(at.para_offset()?.0)
    }

    /// The section a paragraph lays out in: in a sub-template being edited, the section it was entered from.
    pub(super) fn para_section(&self, para: crate::layout::ParaId) -> Option<usize> {
        let slot = self.index.para_slot(para)?;
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

    /// Sets the page borders of the selection's section, or of every section.
    pub(super) fn set_page_borders(&mut self, c: crate::api::command::PageBordersChange, fonts: &Fonts) -> Outcome {
        use crate::api::command::{PageBorderDisplay, PageBorderOffset, PageBorderScope};
        let focus = self.focus_section();
        let in_scope = |section: usize| c.apply_to == PageBorderScope::WholeDocument || section == focus;
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
            let shadow = c.shadow.unwrap_or_else(|| b.bottom.as_ref().is_some_and(|x| x.shadow));
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
            let set = |old: &Option<crate::docx::props::Border>, new: &Option<Option<crate::api::command::Border>>, side: &str, far: bool| match new {
                None => old.clone().map(|mut o| {
                    o.shadow = shadow && far;
                    o
                }),
                Some(None) => None,
                Some(Some(l)) => Some(crate::docx::props::Border {
                    style: l.style.val().into(),
                    width: l.width.0,
                    space: old.as_ref().map_or_else(|| space(side, b.offset_from_page), |o| o.space),
                    shadow: shadow && far,
                    color: match l.color {
                        crate::api::value::AutoOr::Auto => None,
                        crate::api::value::AutoOr::Value(c) => Some(c.0),
                    },
                }),
            };
            b.top = set(&b.top, &c.top, "top", false);
            b.left = set(&b.left, &c.left, "left", false);
            b.bottom = set(&b.bottom, &c.bottom, "bottom", true);
            b.right = set(&b.right, &c.right, "right", true);
            let any = [&b.top, &b.left, &b.bottom, &b.right].iter().any(|x| x.is_some());
            let line = |side: &str, x: &Option<crate::docx::props::Border>| {
                x.as_ref().map_or(String::new(), |x| {
                    let color = x.color.map_or("auto".to_string(), |c| format!("{c:06X}"));
                    let shadow = if x.shadow { format!(" {w}shadow=\"1\"") } else { String::new() };
                    format!(
                        "<{w}{side} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"{}\" {w}color=\"{color}\"{shadow}/>",
                        crate::docx::write::props::esc(&x.style),
                        (x.width * 8.0).round() as i64,
                        x.space.round() as i64
                    )
                })
            };
            let xml = any.then(|| {
                let offset = if b.offset_from_page { format!(" {w}offsetFrom=\"page\"") } else { String::new() };
                let display = if b.display == "allPages" { String::new() } else { format!(" {w}display=\"{}\"", b.display) };
                format!(
                    "<{w}pgBorders{offset}{display}>{}{}{}{}</{w}pgBorders>",
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
