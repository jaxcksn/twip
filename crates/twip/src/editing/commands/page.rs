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
use crate::layout::Unit;

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
        let place = match p.place {
            Some(PageNumberPlace::Header) => PageNumberSpot::Header,
            Some(PageNumberPlace::Footer) => PageNumberSpot::Footer,
            _ => PageNumberSpot::Caret,
        };
        let align = p.alignment.map(|a| match a {
            HorizontalAlignment::Left => PageNumberAlign::Left,
            HorizontalAlignment::Center => PageNumberAlign::Center,
            HorizontalAlignment::Right => PageNumberAlign::Right,
        });
        self.insert_page_number_at(p.format, place, align, fonts)
    }

    /// Inserts a page number field showing `format` at `place`: at the caret, in the section's header or footer, or
    /// in a page margin. In the header or footer `align` aligns its paragraph, or (inside, outside) puts the number in
    /// a frame on the page's inside or outside edge of the margins, as facing pages want it.
    pub fn insert_page_number_at(
        &mut self,
        format: PageNumberFormat,
        place: PageNumberSpot,
        align: Option<PageNumberAlign>,
        fonts: &Fonts,
    ) -> Outcome {
        self.joined("Page Number", fonts, |e| {
            let opened = match place {
                PageNumberSpot::Header | PageNumberSpot::Margin(_) => e.edit_header_footer(None, false, fonts),
                PageNumberSpot::Footer => e.edit_header_footer(None, true, fonts),
                PageNumberSpot::Caret => Outcome::Moved,
            };
            if !matches!(opened, Outcome::Moved | Outcome::Changed(_)) {
                return opened;
            }
            let field = move |run: &crate::docx::Run| page_number_inlines(format, run);
            let owned = move |run: crate::docx::Run| page_number_inlines(format, &run);
            let framed = match (place, align) {
                (PageNumberSpot::Margin(side), _) => Some(e.margin_frame(side)),
                (PageNumberSpot::Header | PageNumberSpot::Footer, Some(a @ (PageNumberAlign::Inside | PageNumberAlign::Outside))) => {
                    let side = if a == PageNumberAlign::Inside { "inside" } else { "outside" };
                    Some(format!("wrap=\"around\" vAnchor=\"text\" hAnchor=\"margin\" xAlign=\"{side}\" y=\"1\""))
                }
                _ => None,
            };
            if let Some(frame) = framed {
                return e.insert_framed(&frame, &field, fonts);
            }
            let out = e.insert_inlines(owned, "Page Number", fonts);
            let a = match align {
                Some(PageNumberAlign::Left) => Alignment::Left,
                Some(PageNumberAlign::Center) => Alignment::Center,
                Some(PageNumberAlign::Right) => Alignment::Right,
                _ => return out,
            };
            if !matches!(out, Outcome::Changed(_)) {
                return out;
            }
            match e.set_alignment(a, fonts) {
                o @ Outcome::Changed(_) => o,
                _ => out,
            }
        })
    }

    /// [`Editor::insert_page_number_at`], with Word's Show number on first page: off, the section takes a different
    /// first page whose header and footer don't hold the number. One undo step.
    pub fn insert_page_number_shown(
        &mut self,
        format: PageNumberFormat,
        place: PageNumberSpot,
        align: Option<PageNumberAlign>,
        on_first_page: bool,
        fonts: &Fonts,
    ) -> Outcome {
        if on_first_page {
            return self.insert_page_number_at(format, place, align, fonts);
        }
        use crate::editing::commands::Command;
        // Text's own number shows on its page; only a header or footer's first-page version can leave it out.
        // The first page's own header or footer, its section's or one before it goes on from
        let si = self.focus_section();
        let first = |footer: bool| {
            (0..=si).rev().find_map(|j| self.doc.sections.get(j).and_then(|s| (if footer { &s.footers } else { &s.headers })[1].as_deref()))
        };
        if place == PageNumberSpot::Caret {
            let Some(crate::editing::position::StoryId::HeaderFooter(k)) = self.selection.focus().map(|p| p.story) else {
                return Outcome::unavailable("a number in the text shows on the first page too");
            };
            if self.index.part_name(k).is_some_and(|n| first(false) == Some(n) || first(true) == Some(n)) {
                return Outcome::unavailable("a number in the first page's header or footer shows on the first page");
            }
        } else if first(place == PageNumberSpot::Footer).and_then(|n| self.doc.stories.get(n)).is_some_and(|b| shows_page(b)) {
            // Its number would show there, the first page's header or footer kept
            return Outcome::unavailable("the first page's header or footer already shows a number");
        }
        self.begin_batch(Some("Page Number".into()));
        let out = (|e: &mut Editor| {
            // Into the header or footer every page but the first shows
            let first = e.doc.sections.get(e.focus_section()).is_some_and(|s| s.title_pg);
            if first {
                e.execute(Command::SetDifferentFirstPage(false), fonts);
            }
            let out = e.insert_page_number_at(format, place, align, fonts);
            if !matches!(out, Outcome::Changed(_)) {
                return out;
            }
            match e.execute(Command::SetDifferentFirstPage(true), fonts) {
                Outcome::Refused(why) => Outcome::Refused(why),
                _ => out,
            }
        })(self);
        // Anything short of the number placed undoes the first page's change too
        let end = self.end_batch(matches!(out, Outcome::Changed(_)), fonts);
        if matches!(end, Outcome::Changed(_)) { end } else { out }
    }

    /// The `w:framePr` attributes (unprefixed) of a frame filling the margin on `side` of the caret's section's
    /// pages, centred down the page.
    fn margin_frame(&self, side: MarginSide) -> String {
        let s = &self.doc.sections[self.focus_section().min(self.doc.sections.len().saturating_sub(1))];
        let gutter = if self.doc.settings.gutter_at_top { 0.0 } else { s.gutter };
        let (width, align) = match side {
            MarginSide::Left => (s.left + gutter, "left"),
            MarginSide::Right => (s.right, "right"),
            MarginSide::Inside => (s.left + gutter, "inside"),
            MarginSide::Outside => (s.right, "outside"),
        };
        let tw = |v: f64| (v * 20.0).round().max(0.0) as i64;
        format!("w=\"{}\" wrap=\"around\" hAnchor=\"page\" xAlign=\"{align}\" vAnchor=\"page\" yAlign=\"center\"", tw(width))
    }

    /// A frame of the attributes `frame` holding one centred paragraph of `inlines`, anchored to the caret's
    /// paragraph.
    fn insert_framed(&mut self, frame: &str, inlines: &dyn Fn(&crate::docx::Run) -> Vec<Inline>, fonts: &Fonts) -> Outcome {
        if self.refuse_replacing() {
            return Outcome::Unchanged("locked");
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let w = &cx.w;
        let Some(caret) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
        let Some((para, _)) = caret.para_offset() else { return Outcome::Unchanged("caret at a row end") };
        if matches!(caret.story, crate::editing::position::StoryId::TextBox(_)) {
            return Outcome::Unchanged("frames don't go in text boxes");
        }
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let Some(run) = self.run_at(&caret) else { return Outcome::Unchanged("no run") };
        let attrs: String = frame.split(' ').map(|a| format!(" {w}{a}")).collect();
        let style = p.style.style_id.as_deref().map_or(String::new(), |id| format!("<{w}pStyle {w}val=\"{}\"/>", crate::docx::write::props::esc(id)));
        let ppr = format!("<{w}pPr>{style}<{w}framePr{attrs}/><{w}jc {w}val=\"center\"/></{w}pPr>");
        let region = self.region_of_para(para, &res);
        let r = res.para_from_xml(Some(&ppr), region.as_ref(), &cx);
        let inner = crate::docx::Paragraph {
            style: std::rc::Rc::new(r.style),
            mark: run.clone(),
            list: None,
            inlines: inlines(&run),
            src: None,
            id: crate::docx::next_para_id(),
            div: None,
            controls: Vec::new(),
        };
        let mut float = crate::docx::frames::float(vec![inner], self.doc.settings.compat() == crate::docx::Compat::Legacy);
        float.z = self.front_z();
        let with = vec![Inline::Float(Box::new(float), run)];
        let mut tx = self.begin("Page Number");
        if self.step(&mut tx, Step::Text { para, range: Unit(0)..Unit(0), with, edges: Default::default() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale caret");
        }
        self.commit(tx, None, fonts)
    }
}

/// A page number's inlines in `run`: the `PAGE` field, the `NUMPAGES` field, or "Page N of M".
fn page_number_inlines(format: PageNumberFormat, run: &crate::docx::Run) -> Vec<Inline> {
    let field = |code: &str| crate::docx::fields::field_inlines(code, "1", run);
    let text = |t: &str| Inline::Text(t.into(), run.clone());
    match format {
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
}

/// Where a page number goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageNumberSpot {
    /// At the caret.
    Caret,
    /// In the section's header.
    Header,
    /// In the section's footer.
    Footer,
    /// In a page margin (Word's Page Margins), in a frame anchored in the header.
    Margin(MarginSide),
}

/// Whether `blocks` (a header or footer, its tables' cells too) hold a `PAGE` field. Table nesting is bounded when read.
fn shows_page(blocks: &[crate::docx::Block]) -> bool {
    blocks.iter().any(|b| match b {
        crate::docx::Block::Para(p) => p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { kind: Some(crate::docx::FieldKind::Page), .. })),
        crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| shows_page(&c.blocks)),
    })
}

/// A page margin, or a facing page's inside or outside one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginSide {
    /// The left side of every page, as wide as the section's left margin (and gutter). With mirrored margins that
    /// is the inside margin on odd pages and the outside one on even pages; [`MarginSide::Inside`] and
    /// [`MarginSide::Outside`] follow the facing page.
    Left,
    /// The right side of every page, as wide as the section's right margin (see [`MarginSide::Left`]).
    Right,
    /// The margin by the binding: the left on odd pages, the right on even ones.
    Inside,
    /// The margin away from the binding.
    Outside,
}

/// How a page number in the header or footer is aligned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageNumberAlign {
    /// On the left.
    Left,
    /// Centred.
    Center,
    /// On the right.
    Right,
    /// On the inside edge: the left on odd pages, the right on even ones.
    Inside,
    /// On the outside edge.
    Outside,
}

/// The sample Word's Format Page Numbers shows for page numbers in `format` with chapter numbers `chapter` (its
/// separator) before them: the first page of chapter 1 (`1-1`, `1-A`), or the first page alone.
pub fn page_number_sample(format: crate::api::command::NumberFormat, chapter: Option<crate::api::command::ChapterSeparator>) -> String {
    use crate::api::command::ChapterSeparator as S;
    let page = crate::docx::numbering::format_number(1, format.val());
    match chapter {
        None => page,
        Some(sep) => {
            let sep = match sep {
                S::Hyphen => "-",
                S::Period => ".",
                S::Colon => ":",
                S::EmDash => "\u{2014}",
                S::EnDash => "\u{2013}",
            };
            format!("1{sep}{page}")
        }
    }
}
