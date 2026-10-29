//! Page layout of the selection's section, and headers and footers.

use crate::api::command::{Columns, HeaderFooterDistance, Margins, PageNumber, PageNumberFormat, PageNumberPlace};
use crate::api::value::{Alignment, HorizontalAlignment};
use crate::docx::{Inline, SectionPage};
use crate::font::Fonts;

use super::Outcome;
use crate::editing::Editor;
use crate::editing::step::Step;

impl Editor {
    /// The focus's section's page setup changed by `edit`, as one step.
    pub(in crate::editing) fn set_page(&mut self, label: &'static str, edit: impl FnOnce(&mut SectionPage), fonts: &Fonts) -> Outcome {
        let section = self.focus_section();
        let Some(s) = self.doc.sections.get(section) else { return Outcome::Unchanged("no section") };
        let mut page = s.page();
        edit(&mut page);
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

    pub(in crate::editing) fn set_margins(&mut self, m: Margins, fonts: &Fonts) -> Outcome {
        let section = self.focus_section();
        if let Some(s) = self.doc.sections.get(section) {
            let left = m.left.map_or(s.left, |v| v.0) + m.right.map_or(s.right, |v| v.0) + m.gutter.map_or(s.gutter, |v| v.0);
            if left >= s.page_w {
                return Outcome::invalid("left + right + gutter must be less than the page width");
            }
        }
        self.set_page(
            "Margins",
            |p| {
                let set = |slot: &mut f64, v: Option<crate::api::value::Points>| {
                    if let Some(v) = v {
                        *slot = v.0;
                    }
                };
                set(&mut p.top, m.top);
                set(&mut p.bottom, m.bottom);
                set(&mut p.left, m.left);
                set(&mut p.right, m.right);
                set(&mut p.gutter, m.gutter);
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_columns(&mut self, c: Columns, fonts: &Fonts) -> Outcome {
        self.set_page(
            "Columns",
            |p| {
                let space = c.spacing.map_or(if p.columns.space > 0.0 { p.columns.space } else { 36.0 }, |s| s.0);
                let separator = c.separator.unwrap_or(p.columns.separator);
                p.columns = crate::docx::Columns { num: c.count.clamp(1, 45) as usize, equal: true, cols: Vec::new(), space, separator };
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_header_footer_distance(&mut self, d: HeaderFooterDistance, fonts: &Fonts) -> Outcome {
        self.set_page(
            "Page Setup",
            |p| {
                if let Some(h) = d.header {
                    p.header = h.0;
                }
                if let Some(f) = d.footer {
                    p.footer = f.0;
                }
            },
            fonts,
        )
    }

    /// A page number field at the caret, or in the caret's page's header or footer (entered, and made when the page
    /// has none), aligned there when an alignment is given. One step.
    pub(in crate::editing) fn insert_page_number(&mut self, p: PageNumber, fonts: &Fonts) -> Outcome {
        self.joined("Page Number", fonts, |e| {
            match p.place {
                Some(PageNumberPlace::Header) => {
                    e.edit_header_footer(None, false, fonts);
                }
                Some(PageNumberPlace::Footer) => {
                    e.edit_header_footer(None, true, fonts);
                }
                _ => {}
            }
            let out = e.insert_inlines(
                |run| {
                    let field = |code: &str| crate::docx::fields::field_inlines(code, "1", &run);
                    let text = |t: &str| Inline::Text(t.into(), run.clone());
                    match p.format {
                        PageNumberFormat::Page => field(" PAGE "),
                        PageNumberFormat::Total => field(" NUMPAGES "),
                        PageNumberFormat::PageOfTotal => {
                            let mut v = vec![text("Page ")];
                            v.extend(field(" PAGE "));
                            v.push(text(" of "));
                            v.extend(field(" NUMPAGES "));
                            v
                        }
                    }
                },
                "Page Number",
                fonts,
            );
            match p.alignment {
                Some(a) if matches!(out, Outcome::Changed(_)) => {
                    let a = match a {
                        HorizontalAlignment::Left => Alignment::Left,
                        HorizontalAlignment::Center => Alignment::Center,
                        HorizontalAlignment::Right => Alignment::Right,
                    };
                    match e.set_alignment(a, fonts) {
                        o @ Outcome::Changed(_) => o,
                        _ => out,
                    }
                }
                _ => out,
            }
        })
    }
}
