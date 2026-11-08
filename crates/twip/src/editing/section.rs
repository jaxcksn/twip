//! Section page setup

use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::position::StoryId;
use super::step::Step;

#[derive(Clone, Debug, PartialEq)]
pub enum SectionCmd {
    Orientation(bool),
    Size(f64, f64),
}

impl Editor {
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

    /// The selection's section's index.
    pub(super) fn selection_section(&self) -> usize {
        self.focus_section()
    }

    /// Sets the page borders of the selection's section, or of every section.
    pub(super) fn set_page_borders(&mut self, c: crate::api::command::PageBordersChange, fonts: &Fonts) -> Outcome {
        use crate::api::command::{PageBorderDisplay, PageBorderOffset, PageBorderScope};
        let sections: Vec<usize> = match c.apply_to {
            PageBorderScope::WholeDocument => (0..self.doc.sections.len()).collect(),
            PageBorderScope::ThisSection => vec![self.focus_section()],
        };
        let cx = self.cx();
        let w = cx.w.clone();
        let mut tx = self.begin("Page Borders");
        let mut changed = false;
        for section in sections {
            let Some(s) = self.doc.sections.get(section) else { continue };
            let mut page = s.page();
            let mut b = page.page_borders.clone().unwrap_or(crate::docx::PageBorders {
                display: "allPages".into(),
                offset_from_page: true,
                surround_header: true,
                surround_footer: true,
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
            if self.step(&mut tx, Step::SectionPage { section, page, change }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("no section");
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
        let section = self.focus_section();
        let Some(s) = self.doc.sections.get(section) else { return Outcome::Unchanged("no section") };
        let mut page = s.page();
        let label = match c {
            SectionCmd::Orientation(landscape) => {
                if (page.page_w > page.page_h) != landscape {
                    (page.page_w, page.page_h) = (page.page_h, page.page_w);
                    (page.top, page.right, page.bottom, page.left) = (page.left, page.top, page.right, page.bottom);
                }
                "Orientation"
            }
            SectionCmd::Size(w, h) => {
                let landscape = page.page_w > page.page_h;
                (page.page_w, page.page_h) = if landscape { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
                "Size"
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
