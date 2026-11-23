//! Page layout, headers and footers

use crate::api::command::{
    Columns, HeaderFooterDistance, LineNumberRestart, LineNumbers, Margins, MultiplePages, MultiplePagesMode, PageNumber, PageNumberFormat,
    PageNumberPlace, PageNumbering, PageVerticalAlignment, SectionBreakKind, TextFlow,
};
use crate::api::value::{Alignment, HorizontalAlignment};
use crate::docx::{Inline, SectionPage};
use crate::font::Fonts;

use super::Outcome;
use crate::editing::Editor;
use crate::editing::step::{PageSettings, Step};

impl Editor {
    /// Edits the page setup of each of the selection's sections, in one transaction.
    pub(in crate::editing) fn set_page(&mut self, label: &'static str, edit: impl Fn(&mut SectionPage), fonts: &Fonts) -> Outcome {
        self.set_sections_page(label, self.selected_sections().into_iter(), edit, fonts)
    }

    /// Edits the page setup of each of `sections`, in one transaction.
    fn set_sections_page(
        &mut self,
        label: &'static str,
        sections: impl Iterator<Item = usize>,
        edit: impl Fn(&mut SectionPage),
        fonts: &Fonts,
    ) -> Outcome {
        let mut tx = None;
        for section in sections {
            let Some(s) = self.doc.sections.get(section) else { continue };
            let mut page = s.page();
            edit(&mut page);
            if page == s.page() {
                continue;
            }
            let change = s.change.clone();
            let t = tx.get_or_insert_with(|| self.begin(label));
            if self.step(t, Step::SectionPage { section, page: Box::new(page), change }).is_err() {
                if let Some(t) = tx.take() {
                    self.abort(t);
                }
                return Outcome::Unchanged("no section");
            }
        }
        match tx {
            Some(tx) => self.commit(tx, None, fonts),
            None => Outcome::Unchanged("already so"),
        }
    }

    pub(in crate::editing) fn set_margins(&mut self, m: Margins, fonts: &Fonts) -> Outcome {
        for section in self.selected_sections() {
            let Some(s) = self.doc.sections.get(section) else { continue };
            let (gw, gh) = self.gutter_axes(m.gutter.map_or(s.gutter, |v| v.0), s.page_w, s.page_h);
            if m.left.map_or(s.left, |v| v.0) + m.right.map_or(s.right, |v| v.0) + gw >= s.page_w {
                return Outcome::invalid("left + right and a side gutter must be less than the page width");
            }
            if m.top.map_or(s.top, |v| v.0).abs() + m.bottom.map_or(s.bottom, |v| v.0).abs() + gh >= s.page_h {
                return Outcome::invalid("top + bottom and a top gutter must be less than the page height");
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
        let num = c.count.clamp(1, 45) as usize;
        // Columns of their own widths, each with the space after it.
        let cols: Vec<(f64, f64)> = c.widths.iter().flatten().map(|w| (w.width.0, w.spacing.0)).collect();
        if !cols.is_empty() {
            if cols.len() != num {
                return Outcome::invalid("one width for each column");
            }
            if cols.iter().any(|&(w, s)| !(w.is_finite() && w > 0.0 && s.is_finite() && s >= 0.0)) {
                return Outcome::invalid("a column's width is positive and its spacing not negative");
            }
            let total: f64 = cols.iter().map(|c| c.0).sum::<f64>() + cols[..num - 1].iter().map(|c| c.1).sum::<f64>();
            let set = &self.doc.settings;
            let facing = crate::layout::page::Facing { mirror: set.mirror_margins, gutter_at_top: set.gutter_at_top, book_fold: set.book_fold };
            for section in self.selected_sections() {
                let Some(s) = self.doc.sections.get(section) else { continue };
                let (left, right, _) = crate::layout::page::margins_in(s, crate::docx::review::MarkupView::All, facing, 1);
                if total > s.page_w - left - right + 0.5 {
                    return Outcome::invalid("the columns are wider than the text");
                }
            }
        }
        self.set_page(
            "Columns",
            |p| {
                let space = c.spacing.map_or(if p.columns.space > 0.0 { p.columns.space } else { 36.0 }, |s| s.0);
                let separator = c.separator.unwrap_or(p.columns.separator);
                let equal = cols.len() < 2;
                p.columns = crate::docx::Columns { num, equal, cols: if equal { Vec::new() } else { cols.clone() }, space, separator };
            },
            fonts,
        )
    }

    /// Moves the right edge of text column `column` of the caret's section to `to` (page points): the space after it
    /// moves with it and the next column takes the change, each column keeping at least half an inch, as in Word.
    pub(in crate::editing) fn move_column_boundary(&mut self, column: usize, to: f64, fonts: &Fonts) -> Outcome {
        const MIN: f64 = 36.0;
        let section = self.focus_section();
        let Some(s) = self.doc.sections.get(section) else { return Outcome::Unchanged("no section") };
        let c = &s.columns;
        let n = c.num.clamp(1, crate::docx::Columns::MAX);
        if column + 1 >= n {
            return Outcome::invalid("no such column boundary");
        }
        let left = s.left + self.gutter_axes(s.gutter, s.page_w, s.page_h).0;
        let width = s.page_w - s.right - left;
        let mut cols: Vec<(f64, f64)> =
            if c.equal || c.cols.len() < n { vec![((width - c.space * (n - 1) as f64) / n as f64, c.space); n] } else { c.cols[..n].to_vec() };
        let start = left + cols[..column].iter().map(|(w, sp)| w + sp).sum::<f64>();
        let (w, next) = (cols[column].0, cols[column + 1].0);
        if w + next < 2.0 * MIN {
            return Outcome::unavailable("the columns are too narrow");
        }
        let dx = (to - start - w).clamp(MIN - w, next - MIN);
        if dx.abs() < 1e-9 {
            return Outcome::Unchanged("already there");
        }
        (cols[column].0, cols[column + 1].0) = (w + dx, next - dx);
        self.set_sections_page(
            "Column Width",
            std::iter::once(section),
            |p| {
                let space = p.columns.space;
                p.columns = crate::docx::Columns { num: n, equal: false, cols: cols.clone(), space, separator: p.columns.separator };
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_page_numbering(&mut self, n: PageNumbering, fonts: &Fonts) -> Outcome {
        use crate::api::command::{ChapterSeparator, NumberFormat};
        if n.format.is_some_and(|f| matches!(f, NumberFormat::Bullet | NumberFormat::None)) {
            return Outcome::invalid("a page number format is a number style");
        }
        if let Some(Some(c)) = &n.chapter
            && !(1..=9).contains(&c.level)
        {
            return Outcome::invalid("the chapter level is 1…9");
        }
        if n.start.flatten().is_some_and(|s| s > 32767) {
            return Outcome::invalid("the first page number is 0…32767");
        }
        self.set_page(
            "Page Number Format",
            |p| {
                if let Some(f) = n.format {
                    p.pg_fmt = (f != NumberFormat::Decimal).then(|| f.val().to_string());
                }
                if let Some(start) = n.start {
                    p.pg_start = start.map(i64::from);
                }
                if let Some(c) = &n.chapter {
                    p.chapter = c.as_ref().map(|c| {
                        let sep = match c.separator {
                            ChapterSeparator::Hyphen => "-",
                            ChapterSeparator::Period => ".",
                            ChapterSeparator::Colon => ":",
                            ChapterSeparator::EmDash => "\u{2014}",
                            ChapterSeparator::EnDash => "\u{2013}",
                        };
                        (i64::from(c.level), sep.to_string())
                    });
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_page_vertical_alignment(&mut self, a: PageVerticalAlignment, fonts: &Fonts) -> Outcome {
        let v = match a {
            PageVerticalAlignment::Top => "top",
            PageVerticalAlignment::Center => "center",
            PageVerticalAlignment::Justified => "both",
            PageVerticalAlignment::Bottom => "bottom",
        };
        self.set_page("Vertical Alignment", |p| p.v_align = v.into(), fonts)
    }

    pub(in crate::editing) fn set_line_numbering(&mut self, l: Option<LineNumbers>, fonts: &Fonts) -> Outcome {
        if let Some(l) = &l
            && (l.start.is_some_and(|s| s == 0 || s > 32767) || l.count_by.is_some_and(|c| c == 0 || c > 100))
        {
            return Outcome::invalid("line numbers start at 1…32767 and count by 1…100");
        }
        if l.as_ref().and_then(|l| l.distance).is_some_and(|d| !(0.0..=1584.0).contains(&d.0)) {
            return Outcome::invalid("the distance is 0…1584 pt");
        }
        // `w:start` is the number before the first.
        let numbering = l.map(|l| crate::docx::LineNumbering {
            count_by: i64::from(l.count_by.unwrap_or(1)),
            start: i64::from(l.start.unwrap_or(1)) - 1,
            distance: l.distance.map_or(0.0, |d| d.0.max(0.0)),
            restart: match l.restart.unwrap_or(LineNumberRestart::EachPage) {
                LineNumberRestart::EachPage => "newPage",
                LineNumberRestart::EachSection => "newSection",
                LineNumberRestart::Continuous => "continuous",
            }
            .into(),
        });
        self.set_page("Line Numbers", |p| p.line_numbers = numbering.clone(), fonts)
    }

    pub(in crate::editing) fn set_section_break_type(&mut self, k: SectionBreakKind, fonts: &Fonts) -> Outcome {
        use crate::docx::SectionBreak as B;
        let b = match k {
            SectionBreakKind::NextPage => B::NextPage,
            SectionBreakKind::Continuous => B::Continuous,
            SectionBreakKind::EvenPage => B::EvenPage,
            SectionBreakKind::OddPage => B::OddPage,
            SectionBreakKind::NextColumn => B::NextColumn,
        };
        self.set_page("Section Start", |p| p.break_kind = b, fonts)
    }

    pub(in crate::editing) fn set_section_text_direction(&mut self, d: TextFlow, fonts: &Fonts) -> Outcome {
        // A section's `tbLrV` is Word's `tbRlV`.
        let code = match d {
            TextFlow::LrTb => 0,
            TextFlow::TbRl => 1,
            TextFlow::BtLr => 3,
            TextFlow::LrTbV => 4,
            TextFlow::TbRlV | TextFlow::TbLrV => 5,
        };
        self.set_page("Text Direction", |p| p.text_flow = code, fonts)
    }

    /// The document's page settings, edited.
    pub(in crate::editing) fn set_page_settings(&mut self, label: &'static str, edit: impl FnOnce(&mut PageSettings), fonts: &Fonts) -> Outcome {
        let was = PageSettings::of(&self.doc.settings);
        let mut now = was;
        edit(&mut now);
        if now == was {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin(label);
        if self.step(&mut tx, Step::PageSettings(now)).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the setting can't change");
        }
        self.commit(tx, None, fonts)
    }

    pub(in crate::editing) fn set_multiple_pages(&mut self, m: MultiplePages, fonts: &Fonts) -> Outcome {
        self.set_page_settings(
            "Multiple Pages",
            |p| {
                if let Some(mode) = m.mode {
                    (p.mirror_margins, p.book_fold) = match mode {
                        MultiplePagesMode::Normal => (false, None),
                        MultiplePagesMode::MirrorMargins => (true, None),
                        MultiplePagesMode::BookFold => (false, Some(false)),
                        MultiplePagesMode::ReverseBookFold => (false, Some(true)),
                    };
                }
                if let Some(top) = m.gutter_at_top {
                    p.gutter_at_top = top;
                }
            },
            fonts,
        )
    }

    pub(in crate::editing) fn insert_alignment_tab(&mut self, t: crate::api::command::AlignmentTab, fonts: &Fonts) -> Outcome {
        use crate::api::command::{AlignmentTabBase, AlignmentTabKind};
        use crate::api::value::TabLeader;
        use crate::docx::props::{Leader, TabKind};
        let tab = crate::docx::PTab {
            align: match t.alignment {
                AlignmentTabKind::Left => TabKind::Left,
                AlignmentTabKind::Center => TabKind::Center,
                AlignmentTabKind::Right => TabKind::Right,
            },
            margin: t.relative_to == AlignmentTabBase::Margin,
            leader: match t.leader.unwrap_or_default() {
                TabLeader::None => Leader::None,
                TabLeader::Dots => Leader::Dot,
                TabLeader::Dashes => Leader::Hyphen,
                TabLeader::Underline | TabLeader::Heavy => Leader::Underscore,
                TabLeader::MiddleDot => Leader::MiddleDot,
            },
        };
        self.insert_inline(|run| Inline::PTab(tab, run), "Alignment Tab", fonts)
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
