//! Section and page commands: margins, orientation, paper size, columns,
//! a different first page, and Page Setup. One [`Step::SectionPage`] on the focus's section; layout runs in full.

use crate::docx::SectionPage;
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::position::StoryId;
use super::step::Step;

#[derive(Clone, Debug, PartialEq)]
pub enum SectionCmd {
    /// Top, bottom, left, right, points.
    Margins([f64; 4]),
    /// Landscape (`true`) or portrait: width and height swap when they disagree with it.
    Orientation(bool),
    /// Paper width and height in portrait, points (kept in the section's orientation).
    Size(f64, f64),
    /// Equal columns.
    Columns(usize),
    /// Different First Page, toggled.
    DifferentFirst,
    /// Page Setup: every value.
    Page(SectionPage),
}

impl Editor {
    /// The focus's section: a body position's own; in a header or footer, the first section showing it.
    pub(super) fn focus_section(&self) -> usize {
        let Some(focus) = self.selection.focus() else { return 0 };
        match focus.story {
            StoryId::HeaderFooter(k) => {
                let name = self.index.part_name(k);
                self.doc
                    .sections
                    .iter()
                    .position(|s| s.headers.iter().chain(s.footers.iter()).flatten().any(|n| Some(n.as_str()) == name))
                    .unwrap_or(0)
            }
            _ => focus
                .para_offset()
                .and_then(|(p, _)| self.index.para_slot(p))
                .and_then(|s| self.index.top_block(s.container, s.block))
                .map_or(0, |(sec, _, _)| sec),
        }
    }

    pub(super) fn section_cmd(&mut self, c: SectionCmd, fonts: &Fonts) -> Outcome {
        let section = self.focus_section();
        let Some(s) = self.doc.sections.get(section) else { return Outcome::Unchanged("no section") };
        let mut page = s.page();
        let label = match c {
            SectionCmd::Margins([t, b, l, r]) => {
                (page.top, page.bottom, page.left, page.right) = (t, b, l, r);
                "Margins"
            }
            SectionCmd::Orientation(landscape) => {
                if (page.page_w > page.page_h) != landscape {
                    (page.page_w, page.page_h) = (page.page_h, page.page_w);
                    // The margins turn with the page.
                    (page.top, page.right, page.bottom, page.left) = (page.left, page.top, page.right, page.bottom);
                }
                "Orientation"
            }
            SectionCmd::Size(w, h) => {
                let landscape = page.page_w > page.page_h;
                (page.page_w, page.page_h) = if landscape { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
                "Size"
            }
            SectionCmd::Columns(n) => {
                page.columns = crate::docx::Columns {
                    num: n.clamp(1, 45),
                    equal: true,
                    cols: Vec::new(),
                    space: if page.columns.space > 0.0 { page.columns.space } else { 36.0 },
                    separator: page.columns.separator,
                };
                "Columns"
            }
            SectionCmd::DifferentFirst => {
                page.title_pg = !page.title_pg;
                "Different First Page"
            }
            SectionCmd::Page(p) => {
                page = p;
                "Page Setup"
            }
        };
        if page == s.page() {
            return Outcome::Unchanged("already so");
        }
        let change = s.change.clone();
        let mut tx = self.begin(label);
        if self.step(&mut tx, Step::SectionPage { section, page, change }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("no section");
        }
        self.commit(tx, None, fonts)
    }
}
