//! Click and Type

use crate::api::command::{AddTabStop, CommandId, Indents};
use crate::api::value::{Alignment, Points, TabAlignment};
use crate::docx::Block;
use crate::font::Fonts;
use crate::layout::ParaId;

use super::commands::{Command, Outcome};
use super::position::{Position, StoryId};
use super::selection::Selection;
use super::{Editor, Mode};

/// How text typed where Click and Type puts it lines up: what the I-beam shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickAlign {
    /// Left-aligned at the click (a left tab stop), or at the text's left edge.
    Left,
    /// Left-aligned, the first line indented to the click: below the document's end, near the text's left edge.
    LeftIndent,
    /// Centered: the click is in the text column's middle zone.
    Center,
    /// Right-aligned: the click is near the text column's right edge.
    Right,
}

/// Where a Click and Type double-click goes: below the document's end (paragraphs down to the click), or beside a
/// paragraph's last line (a tab out to the click).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClickAndType {
    /// How what is typed there lines up.
    pub align: ClickAlign,
    /// The paragraph it adds to: the document's last, or the one beside the click.
    pub para: ParaId,
    /// Below the document's end (paragraphs down to the click), else beside a line (a tab out to it).
    pub below: bool,
    /// The click's distance from the text column's left edge (the left margin), points.
    pub at: f64,
    /// The click's height on its page, points.
    pub y: f64,
    /// The page clicked on, from 0.
    pub page: usize,
}

/// The half width of the center zone, and the width of the right zone at the text's right edge, points.
const ZONE: f64 = 36.0;
/// Within this distance of the text's left edge a click lines up with it, adding no tab; up to [`ZONE`] past it,
/// below the document's end, it indents the first line to the click.
const LEFT_EDGE: f64 = 9.0;
/// The most paragraphs one double-click adds.
const MAX_PARAGRAPHS: usize = 500;

impl Editor {
    /// Turns Click and Type on or off (on by default).
    pub fn set_click_and_type(&mut self, on: bool) {
        self.click_and_type_off = !on;
    }

    /// Whether Click and Type is on.
    pub fn click_and_type_on(&self) -> bool {
        !self.click_and_type_off
    }

    /// The style the paragraphs Click and Type adds take (a host setting, as Word's Click and Type option is);
    /// `None`: the default paragraph style.
    pub fn set_click_and_type_style(&mut self, style: Option<String>) {
        self.click_and_type_style = style;
    }

    /// What a double-click at (`x`, `y`) would do with Click and Type: in the body's blank area below its end, or
    /// right of a paragraph's last line, on a page with one column, not on an object, in a table or beside a list.
    pub fn click_and_type_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<ClickAndType> {
        if self.click_and_type_off || self.mode == Mode::ReadOnly || self.editing() != StoryId::Body {
            return None;
        }
        let (page, px, py) = self.stack().page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let (section, number) = self.snapshot().info().get(page).map_or((0, 1), |i| (i.section, i.number));
        let sec = self.doc.sections.get(section)?;
        if sec.columns.num > 1 || crate::layout::vertical::of_section(sec.text_flow).is_some() {
            return None;
        }
        let set = &self.doc.settings;
        let facing = crate::layout::page::Facing { mirror: set.mirror_margins, gutter_at_top: set.gutter_at_top, book_fold: set.book_fold };
        let (l, right, _) = crate::layout::page::margins_in(sec, self.doc.review.view, facing, number);
        let r = sec.section_in(self.doc.review.view).page_w - right;
        let (top, bottom) = geo.body_area?;
        let body = geo.stories.iter().position(|s| s.story == StoryId::Body && s.float.is_none())?;
        if py < top || py > bottom || px < l || px > r || geo.in_notes(px, py) {
            return None;
        }
        let other =
            |s: &super::geometry::GStory| s.story != StoryId::Body && s.rect[0] <= px && px <= s.rect[2] && s.rect[1] <= py && py <= s.rect[3];
        if geo.stories.iter().any(other) {
            return None;
        }
        if geo.cells.iter().any(|c| c.rect[0] <= px && px <= c.rect[2] && c.rect[1] <= py && py <= c.rect[3])
            || self.object_hit(x, y, fonts).is_some()
        {
            return None;
        }
        let lines = || geo.lines.iter().filter(|l| l.story == body && l.chain.is_empty());
        let align = if (px - (l + r) / 2.0).abs() <= ZONE {
            ClickAlign::Center
        } else if px >= r - ZONE {
            ClickAlign::Right
        } else {
            ClickAlign::Left
        };
        let allowed = |e: &Self, para: ParaId, below: bool| {
            let Some(len) = e.index.para(&e.doc, para).map(super::step::para_len) else { return false };
            let end = Position::text(StoryId::Body, para, len);
            let ids: &[CommandId] = if below { &[CommandId::InsertParagraph] } else { &[CommandId::InsertTab, CommandId::AddTabStop] };
            e.editable(&end) && !ids.iter().any(|&id| e.kept(id, &end, &end))
        };
        let last = self.doc.sections.last()?.blocks.last().and_then(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None });
        let end = lines().filter_map(|l| Some((l.bottom, l.para()?))).max_by(|a, b| a.0.total_cmp(&b.0));
        let endnotes = |b: f64| {
            let below = geo.stories.iter().filter(|s| matches!(s.story, StoryId::Endnote(_)) && s.rect[1] >= b);
            below.map(|s| s.rect[3]).reduce(f64::max)
        };
        if let (Some(last), Some((line_bottom, para))) = (last, end)
            && para == last
            && py > endnotes(line_bottom).unwrap_or(line_bottom)
            && self.click_and_type_para(last)
            && allowed(self, last, true)
        {
            let indent = align == ClickAlign::Left && px - l > LEFT_EDGE && px - l <= ZONE;
            return Some(ClickAndType {
                align: if indent { ClickAlign::LeftIndent } else { align },
                para: last,
                below: true,
                at: px - l,
                y: py,
                page,
            });
        }
        let line = lines().find(|l| l.top <= py && py <= l.bottom)?;
        let para = line.para()?;
        let len = self.index.para(&self.doc, para).map(super::step::para_len)?;
        let end = line.stops.iter().find(|s| s.para == para && s.unit == len)?;
        (px > end.x && self.click_and_type_para(para) && allowed(self, para, false)).then_some(())?;
        Some(ClickAndType { align, para, below: false, at: px - l, y: py, page })
    }

    /// A paragraph Click and Type can extend: in the body, not in a list.
    fn click_and_type_para(&self, para: ParaId) -> bool {
        self.index.para(&self.doc, para).is_some_and(|p| p.list.is_none() && p.style.list.is_none())
            && matches!(self.index.para_slot(para).map(|s| s.container), Some(super::index::Container::Section(_)))
    }

    /// A Click and Type double-click, as one undo step: paragraphs in the Click and Type style down to the click,
    /// then the alignment or a tab stop and a tab; or beside a paragraph, a tab stop at the click and a tab.
    pub fn click_and_type(&mut self, t: ClickAndType, fonts: &Fonts) -> Outcome {
        let Some(len) = self.index.para(&self.doc, t.para).map(super::step::para_len) else { return Outcome::Unchanged("no paragraph") };
        self.begin_batch(Some("Click and Type".into()));
        self.set_selection(Selection::Caret(Position::text(StoryId::Body, t.para, len)));
        let mut ok = true;
        if t.below {
            let style = self.click_and_type_style.clone();
            for _ in 0..MAX_PARAGRAPHS {
                ok &= matches!(self.execute(Command::InsertParagraph, fonts), Outcome::Changed(_));
                ok &= self.click_and_type_paragraph(style.as_deref(), fonts);
                let caret = self.selection.focus().and_then(|p| self.caret_at(p));
                match caret {
                    Some(c) if ok && c.page == t.page && c.bottom < t.y => {}
                    _ => break,
                }
            }
            match t.align {
                ClickAlign::Center => ok &= !matches!(self.execute(Command::SetAlignment(Alignment::Center), fonts), Outcome::Refused(_)),
                ClickAlign::Right => ok &= !matches!(self.execute(Command::SetAlignment(Alignment::Right), fonts), Outcome::Refused(_)),
                ClickAlign::Left | ClickAlign::LeftIndent => {
                    ok &= !matches!(self.execute(Command::SetAlignment(Alignment::Left), fonts), Outcome::Refused(_));
                    if t.align == ClickAlign::LeftIndent {
                        let first_line = Some(Points(snap(t.at)));
                        let indents = Command::SetIndents(Indents { first_line, ..Indents::default() });
                        ok &= !matches!(self.execute(indents, fonts), Outcome::Refused(_));
                    } else if t.at > LEFT_EDGE {
                        ok &= self.tab_out(TabAlignment::Left, t.at, fonts);
                    }
                }
            }
        } else {
            let alignment = match t.align {
                ClickAlign::Left | ClickAlign::LeftIndent => TabAlignment::Left,
                ClickAlign::Center => TabAlignment::Center,
                ClickAlign::Right => TabAlignment::Right,
            };
            ok &= self.tab_out(alignment, t.at, fonts);
        }
        self.end_batch(ok, fonts)
    }

    /// Gives the caret's paragraph the Click and Type `style` (`None`: the default paragraph style) and none of its
    /// direct paragraph formatting but its mark's and its section's.
    fn click_and_type_paragraph(&mut self, style: Option<&str>, fonts: &Fonts) -> bool {
        let cascade = self.doc.resolver.as_ref().map(|r| r.cascade());
        let styles = cascade.as_deref().map(|c| &*c.styles);
        let paragraph = |id: &str| styles.and_then(|s| s.get(id)).is_some_and(|s| s.kind == crate::format::StyleKind::Paragraph);
        let style = style.filter(|s| paragraph(s) && styles.and_then(|st| st.default_paragraph.as_deref()) != Some(*s));
        let style: Option<std::rc::Rc<str>> = style.map(Into::into);
        let edit = |e: &mut crate::format::ParaEdit, _: &crate::docx::Paragraph| {
            e.props = crate::format::ParaProps { style: style.clone(), ..Default::default() };
            if style.is_some() {
                e.whole(crate::format::ParaProp::Style);
            }
        };
        !matches!(self.format_paras_direct_typed("Style", &edit, fonts), Outcome::Refused(_))
    }

    fn tab_out(&mut self, alignment: TabAlignment, at: f64, fonts: &Fonts) -> bool {
        let stop = AddTabStop { position: Points(snap(at)), alignment: Some(alignment), leader: None };
        !matches!(self.execute(Command::AddTabStop(stop), fonts), Outcome::Refused(_))
            && matches!(self.execute(Command::InsertTab, fonts), Outcome::Changed(_))
    }
}

/// `at` to the nearest twip.
fn snap(at: f64) -> f64 {
    (at * 20.0).round() / 20.0
}
