//! Outline levels and headings

use std::collections::HashSet;
use std::rc::Rc;

use crate::api::command::CommandId;
use crate::docx::write::props::Props;
use crate::docx::{Block, ControlId, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};
use crate::services::a11y::roles::heading_level;

use super::Editor;
use super::commands::Outcome;
use super::index::Container;
use super::move_paras::{Span, controls, edge_para, edge_para_ref, span_of, spans, take_markers};
use super::position::Position;
use super::selection::Selection;
use super::step::{SectionBack, Step, StepError, para_len};

/// What an outline change does to a paragraph's level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineChange {
    /// A level up (Alt+Shift+Left); body text takes the level of the heading before it, Heading 1 stays.
    Promote,
    /// A level down (Alt+Shift+Right); Heading 9 and body text stay.
    Demote,
    /// Heading 1.
    Heading1,
    /// Body text, in the document's default paragraph style.
    BodyText,
}

/// Where a new heading goes beside a heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewHeading {
    /// Just before it, at its level.
    Before,
    /// After its content, at its level.
    After,
    /// After its content, a level below it.
    Subheading,
}

/// Which side of a heading another goes: before the heading, or after its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beside {
    /// Just before the heading.
    Before,
    /// After the heading's whole content.
    After,
}

/// A top-level block of a story, by where it is in the story's run of containers: the body's sections, else its one
/// container.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct At {
    /// The container, an index into the story's containers.
    pub part: usize,
    /// The block in that container.
    pub block: usize,
}

/// A heading's content: its paragraph through the block before the next heading of its level or higher, whole blocks
/// (tables, and section breaks, a section's last paragraph holding its break).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content {
    /// The story's containers in order.
    pub parts: Vec<Container>,
    /// The heading's own block.
    pub first: At,
    /// The content's last block.
    pub last: At,
    /// The heading's level.
    pub level: u32,
}

impl Content {
    /// The content ends a section that isn't the story's last: it holds that section's break.
    fn ends_break(&self, e: &Editor) -> bool {
        self.last.part + 1 < self.parts.len() && e.part_len(&self.parts, self.last.part) == self.last.block + 1
    }

    /// The section breaks it holds.
    fn breaks(&self, e: &Editor) -> usize {
        self.last.part - self.first.part + usize::from(self.ends_break(e))
    }
}

/// Where moved or new blocks go: before a top-level block, or at the story's end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dest {
    Before(At),
    End,
}

/// The level a paragraph is to take, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// A heading level: in the direct outline level when the paragraph has one, else by its heading style.
    Level(u32),
    /// The built-in heading style of a level, any direct outline level gone.
    Heading(u32),
    /// The default paragraph style, any direct outline level gone.
    Body,
}

/// Whether a releveling inside a larger edit failed: anything but a change, or nothing there was to change.
fn failed(out: &Outcome) -> bool {
    matches!(out, Outcome::Refused(_)) || matches!(out, Outcome::Unchanged(why) if !matches!(*why, "nothing to change" | "nothing to do"))
}

/// Whether `p` has a direct outline level.
fn direct_level(ppr: Option<&str>, cx: &crate::docx::write::props::Cx) -> bool {
    Props::parse(ppr, "pPr", cx).get("outlineLvl").is_some()
}

/// Every paragraph in `blocks` in order (tables' cells too) until `f` answers false; false then.
fn each_para<'a>(blocks: &'a [Block], f: &mut impl FnMut(&'a Paragraph) -> bool) -> bool {
    for b in blocks {
        let go = match b {
            Block::Para(p) => f(p),
            // Table nesting is bounded when read.
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).all(|c| each_para(&c.blocks, f)),
        };
        if !go {
            return false;
        }
    }
    true
}

/// [`each_para`] backwards.
fn each_para_back<'a>(blocks: &'a [Block], f: &mut impl FnMut(&'a Paragraph) -> bool) -> bool {
    for b in blocks.iter().rev() {
        let go = match b {
            Block::Para(p) => f(p),
            Block::Table(t) => t.rows.iter().rev().flat_map(|r| r.cells.iter().rev()).all(|c| each_para_back(&c.blocks, f)),
        };
        if !go {
            return false;
        }
    }
    true
}

/// The block controls common to the starts of `a` and `b`, outermost first.
fn common(a: &[ControlId], b: &[ControlId]) -> Vec<ControlId> {
    a.iter().zip(b).take_while(|(x, y)| x == y).map(|(x, _)| *x).collect()
}

impl Editor {
    /// A paragraph's heading level, 1 to 9: its outline level, from its style or set on it.
    pub fn outline_level(&self, para: ParaId) -> Option<u32> {
        heading_level(&self.doc, self.index.para(&self.doc, para)?)
    }

    /// The containers of the story a container's top-level blocks are in, in order, and which one it is.
    fn parts_of(&self, container: Container) -> Option<(Vec<Container>, usize)> {
        match container {
            Container::Section(k) => Some(((0..self.doc.sections.len()).map(Container::Section).collect(), k)),
            Container::Cell { .. } => None,
            c => Some((vec![c], 0)),
        }
    }

    fn part_len(&self, parts: &[Container], part: usize) -> usize {
        parts.get(part).and_then(|&c| self.index.blocks(&self.doc, c)).map_or(0, Vec::len)
    }

    fn part_block(&self, parts: &[Container], at: At) -> Option<&Block> {
        self.index.blocks(&self.doc, *parts.get(at.part)?)?.get(at.block)
    }

    /// The top-level block a paragraph is in (a table's for a cell's), and its container.
    fn top_of(&self, para: ParaId) -> Option<(Container, usize)> {
        let mut s = self.index.para_slot(para)?;
        // Each step goes a table out.
        while let Container::Cell { table, .. } = s.container {
            s = self.index.table_slot(table)?;
        }
        Some((s.container, s.block))
    }

    /// The paragraphs after `para` in its story, in order, until `f` answers false.
    fn paras_after(&self, para: ParaId, mut f: impl FnMut(&Paragraph) -> bool) {
        let Some((container, block)) = self.top_of(para) else { return };
        let Some((parts, part)) = self.parts_of(container) else { return };
        let mut seen = false;
        for (k, &c) in parts.iter().enumerate().skip(part) {
            let Some(blocks) = self.index.blocks(&self.doc, c) else { continue };
            let from = if k == part { block } else { 0 };
            let go = each_para(blocks.get(from..).unwrap_or_default(), &mut |p| {
                if seen {
                    return f(p);
                }
                seen = p.id == para.0;
                true
            });
            if !go {
                return;
            }
        }
    }

    /// The paragraphs before `para` in its story, nearest first, until `f` answers false.
    fn paras_before(&self, para: ParaId, mut f: impl FnMut(&Paragraph) -> bool) {
        let Some((container, block)) = self.top_of(para) else { return };
        let Some((parts, part)) = self.parts_of(container) else { return };
        let mut seen = false;
        for k in (0..=part).rev() {
            let Some(blocks) = self.index.blocks(&self.doc, parts[k]) else { continue };
            let to = if k == part { (block + 1).min(blocks.len()) } else { blocks.len() };
            let go = each_para_back(&blocks[..to], &mut |p| {
                if seen {
                    return f(p);
                }
                seen = p.id == para.0;
                true
            });
            if !go {
                return;
            }
        }
    }

    /// The level of the nearest heading before `para`; reaching `known`'s paragraph, the level it gives.
    fn level_before(&self, para: ParaId, known: Option<(ParaId, Option<u32>)>) -> Option<u32> {
        let mut out = None;
        self.paras_before(para, |p| {
            if let Some((_, l)) = known.filter(|k| k.0.0 == p.id) {
                out = l;
                return false;
            }
            out = heading_level(&self.doc, p);
            out.is_none()
        });
        out
    }

    /// The nearest paragraph at or before `para` in its story (tables included) that `is_heading`: with
    /// [`Self::outline_level`], the heading whose content a paragraph is in. One pass back to it.
    pub fn heading_of(&self, para: ParaId, is_heading: impl Fn(&Paragraph) -> bool) -> Option<ParaId> {
        if self.index.para(&self.doc, para).is_some_and(&is_heading) {
            return Some(para);
        }
        let mut out = None;
        self.paras_before(para, |p| {
            out = is_heading(p).then_some(ParaId(p.id));
            out.is_none()
        });
        out
    }

    /// The headings after `heading` under it: up to the next heading of its level or higher, with their levels.
    fn subtopics(&self, heading: ParaId, level: u32) -> Vec<(ParaId, u32)> {
        let mut out = Vec::new();
        self.paras_after(heading, |p| match heading_level(&self.doc, p) {
            Some(l) if l <= level => false,
            Some(l) => {
                out.push((ParaId(p.id), l));
                true
            }
            None => true,
        });
        out
    }

    /// A heading's content. Refused for a paragraph that isn't a heading, and a heading in a table, which moves with
    /// its table.
    pub fn heading_content(&self, heading: ParaId) -> Result<Content, Outcome> {
        let Some(level) = self.outline_level(heading) else { return Err(Outcome::invalid("not a heading")) };
        let slot = self.index.para_slot(heading).ok_or(Outcome::Unchanged("no paragraph"))?;
        let Some((parts, part)) = self.parts_of(slot.container) else {
            return Err(Outcome::unavailable("a heading in a table moves with its table"));
        };
        let first = At { part, block: slot.block };
        let ends = |b: &Block| match b {
            Block::Para(p) => heading_level(&self.doc, p).is_some_and(|l| l <= level),
            b => !each_para(std::slice::from_ref(b), &mut |p| heading_level(&self.doc, p).is_none_or(|l| l > level)),
        };
        let mut last = first;
        for (k, &c) in parts.iter().enumerate().skip(part) {
            let Some(blocks) = self.index.blocks(&self.doc, c) else { continue };
            let from = if k == part { slot.block + 1 } else { 0 };
            for (j, b) in blocks.iter().enumerate().skip(from) {
                if ends(b) {
                    return Ok(Content { parts, first, last, level });
                }
                last = At { part: k, block: j };
            }
        }
        Ok(Content { parts, first, last, level })
    }

    /// Selects a heading and its content, whole.
    pub fn select_heading_content(&mut self, heading: ParaId) -> Outcome {
        let c = match self.heading_content(heading) {
            Ok(c) => c,
            Err(o) => return o,
        };
        let story = self.index.story(c.parts[c.first.part]);
        let next = At { part: c.last.part, block: c.last.block + 1 };
        // A trailing table ends at the next paragraph's start, which copies it whole
        let end = match (self.part_block(&c.parts, c.last), self.part_block(&c.parts, next)) {
            (Some(Block::Table(_)), Some(Block::Para(p))) => Some((ParaId(p.id), Unit(0))),
            (b, _) => b.and_then(|b| edge_para_ref(b, true)).map(|p| (ParaId(p.id), para_len(p))),
        };
        let Some((last, len)) = end else {
            return Outcome::Unchanged("no paragraph");
        };
        let was = (self.selection, !self.multi.others.is_empty() || !self.multi.cells.is_empty());
        self.multi = super::multi::Multi::default();
        self.selection = Selection::range(Position::text(story, heading, Unit(0)), Position::text(story, last, len), Default::default());
        if (self.selection, false) == was { Outcome::Unchanged("selected already") } else { Outcome::Moved }
    }

    // Levels

    /// Promote, Demote, Promote to Heading 1 or Demote to Body Text on the selected paragraphs, as one undo step;
    /// `subtopics`: the headings under each heading changed move by as many levels.
    pub fn change_outline(&mut self, change: OutlineChange, subtopics: bool, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        if self.refuse_expanded() || self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
        let paras = self.selected_paras();
        self.outline_paras(&paras, change, subtopics, fonts)
    }

    /// [`Self::change_outline`] on one heading, as a navigation pane does it.
    pub fn change_heading_outline(&mut self, heading: ParaId, change: OutlineChange, subtopics: bool, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        if self.outline_level(heading).is_none() {
            return Outcome::invalid("not a heading");
        }
        self.outline_paras(&[heading], change, subtopics, fonts)
    }

    fn outline_paras(&mut self, paras: &[ParaId], change: OutlineChange, subtopics: bool, fonts: &Fonts) -> Outcome {
        let mut seen: HashSet<ParaId> = HashSet::new();
        let mut out: Vec<(ParaId, Target)> = Vec::new();
        // The last paragraph whose heading level before it was found, and the level in force after it: each search
        // back stops there.
        let mut known: Option<(ParaId, Option<u32>)> = None;
        for &para in paras {
            if !seen.insert(para) || self.index.para(&self.doc, para).is_none() {
                continue;
            }
            let level = self.outline_level(para);
            let target = match (change, level) {
                (OutlineChange::Promote, Some(1)) | (OutlineChange::Demote, Some(9) | None) => continue,
                (OutlineChange::Promote, Some(l)) => Target::Level(l - 1),
                (OutlineChange::Promote, None) => {
                    let l = self.level_before(para, known);
                    known = Some((para, l));
                    Target::Level(l.unwrap_or(1))
                }
                (OutlineChange::Demote, Some(l)) => Target::Level(l + 1),
                (OutlineChange::Heading1, _) => Target::Heading(1),
                (OutlineChange::BodyText, _) => Target::Body,
            };
            if level.is_some() {
                known = Some((para, level));
            }
            out.push((para, target));
            let to = match target {
                Target::Level(n) | Target::Heading(n) => n,
                Target::Body => continue,
            };
            let Some(l) = level.filter(|_| subtopics) else { continue };
            let delta = to as i64 - l as i64;
            // Nothing moves under a heading that stays: a selected child takes its own change.
            if delta == 0 {
                continue;
            }
            for (q, k) in self.subtopics(para, l) {
                let n = (k as i64 + delta).clamp(1, 9) as u32;
                if seen.insert(q) && n != k {
                    out.push((q, Target::Level(n)));
                }
            }
        }
        let label = match change {
            OutlineChange::Promote => "Promote",
            OutlineChange::Demote => "Demote",
            OutlineChange::Heading1 => "Promote to Heading 1",
            OutlineChange::BodyText => "Demote to Body Text",
        };
        self.relevel(label, &out, fonts)
    }

    /// Moves a heading and the headings under it by `delta` levels, each kept within 1 to 9.
    fn shift_heading(&mut self, label: &'static str, heading: ParaId, delta: i64, fonts: &Fonts) -> Outcome {
        let Some(l) = self.outline_level(heading) else { return Outcome::Unchanged("not a heading") };
        let headings: Vec<(ParaId, u32)> = std::iter::once((heading, l)).chain(self.subtopics(heading, l)).collect();
        self.shift_headings(label, &headings, delta, fonts)
    }

    /// Moves exactly `headings` (with their levels) by `delta` levels, each kept within 1 to 9.
    fn shift_headings(&mut self, label: &'static str, headings: &[(ParaId, u32)], delta: i64, fonts: &Fonts) -> Outcome {
        let to = |k: u32| (k as i64 + delta).clamp(1, 9) as u32;
        let out: Vec<(ParaId, Target)> = headings.iter().filter(|&&(_, k)| to(k) != k).map(|&(q, k)| (q, Target::Level(to(k)))).collect();
        self.relevel(label, &out, fonts)
    }

    /// The headings from paragraph `first` through paragraph `last` of its story, with their levels.
    fn headings_between(&self, first: ParaId, last: ParaId) -> Vec<(ParaId, u32)> {
        let mut out = Vec::new();
        let mut take = |p: &Paragraph| out.extend(heading_level(&self.doc, p).map(|l| (ParaId(p.id), l)));
        if let Some(p) = self.index.para(&self.doc, first) {
            take(p);
        }
        if first != last {
            self.paras_after(first, |p| {
                take(p);
                p.id != last.0
            });
        }
        out
    }

    /// Gives paragraphs their targets as one undo step: the built-in heading styles they take join the document first.
    fn relevel(&mut self, label: &'static str, targets: &[(ParaId, Target)], fonts: &Fonts) -> Outcome {
        if targets.is_empty() {
            return Outcome::Unchanged("nothing to change");
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        // What each paragraph's level goes in
        let mut plan: Vec<(ParaId, Target)> = Vec::with_capacity(targets.len());
        for &(para, t) in targets {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let story = self.index.para_slot(para).map_or(super::position::StoryId::Body, |s| self.index.story(s.container));
            if self.kept(CommandId::SetParagraphStyle, &Position::text(story, para, Unit(0)), &Position::text(story, para, para_len(p))) {
                self.refused = Some("locked");
                self.events.push(super::input::Notice::Refused { sub_template: None, element: None });
                return Outcome::Unchanged("locked");
            }
            plan.push((
                para,
                if let Target::Level(n) = t {
                    if direct_level(self.ppr(&p.style.direct, &cx).as_deref(), &cx) { t } else { Target::Heading(n) }
                } else {
                    t
                },
            ));
        }
        let mut wanted: Vec<u32> = plan.iter().filter_map(|&(_, t)| if let Target::Heading(n) = t { Some(n) } else { None }).collect();
        wanted.sort_unstable();
        wanted.dedup();
        self.begin_batch(Some(label.into()));
        let mut styles: [Option<String>; 9] = Default::default();
        for n in wanted {
            let id = format!("Heading{n}");
            let found = |e: &Editor| e.doc.resolver.as_ref().and_then(|r| super::list_defs::linkable(&r.styles, &id));
            if found(self).is_none()
                && let Some(b) = crate::services::styles::builtin::built_in(&id)
            {
                self.add_built_in(b, fonts);
            }
            styles[n as usize - 1] = found(self);
            if styles[n as usize - 1].is_none() {
                self.end_batch(false, fonts);
                return Outcome::Unchanged("the heading style couldn't be added");
            }
        }
        let res = self.doc.resolver.clone().unwrap_or(res);
        let body = res.styles.default_paragraph.clone();
        let mut tx = self.begin(label);
        for (para, t) in plan {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut pr = Props::parse(self.ppr(&p.style.direct, &cx).as_deref(), "pPr", &cx);
            let w = &cx.w;
            let style = match t {
                Target::Level(n) => {
                    pr.set("outlineLvl", format!("<{w}outlineLvl {w}val=\"{}\"/>", n - 1));
                    None
                }
                Target::Heading(n) => styles[n as usize - 1].clone(),
                Target::Body => body.clone(),
            };
            if !matches!(t, Target::Level(_)) {
                pr.remove("outlineLvl");
                match style {
                    Some(id) => pr.set("pStyle", format!("<{w}pStyle {w}val=\"{}\"/>", crate::docx::write::props::esc(&id))),
                    None => pr.remove("pStyle"),
                }
            }
            let Some(r) = self.para_again(para, Some(pr.xml().as_deref()), &res, &cx) else { continue };
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if r.style.layout_eq(&p.style) && r.style.authored_eq(&p.style) {
                continue;
            }
            let (mark, list) = (p.mark.clone(), p.list.clone());
            if self.step(&mut tx, Step::Para { para, style: Rc::new(r.style), mark, list }).is_err() {
                self.abort(tx);
                self.end_batch(false, fonts);
                return Outcome::Unchanged("formatting failed");
            }
            tx.stale_paras.push(para);
        }
        self.commit(tx, None, fonts);
        self.end_batch(true, fonts)
    }

    // Moving, deleting and inserting

    /// Where a heading's blocks go beside `target`: before it, or after its content.
    fn dest_beside(&self, target: ParaId, side: Beside) -> Result<(Vec<Container>, Dest), Outcome> {
        let t = self.heading_content(target)?;
        let dest = match side {
            Beside::Before => Dest::Before(t.first),
            Beside::After => self.after_at(&t.parts, t.last),
        };
        Ok((t.parts, dest))
    }

    /// Moves a heading with its content before `target` or after its content (a navigation pane's drag), as one undo
    /// step; `level`: the heading then takes that level, the headings under it moving by as many, each kept within 1
    /// to 9 (a subtopic pressed against 9 becomes the heading's peer, as Word's Demote leaves it). Tracked as a move
    /// when changes are tracked. Bookmarks and comments go with the text, as cutting and pasting it would; a field
    /// running from it into the text it passes refuses.
    pub fn move_heading(&mut self, heading: ParaId, target: ParaId, side: Beside, level: Option<u32>, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        if level.is_some_and(|l| !(1..=9).contains(&l)) {
            return Outcome::invalid("the level is 1…9");
        }
        let c = match self.heading_content(heading) {
            Ok(c) => c,
            Err(o) => return o,
        };
        let (parts, dest) = match self.dest_beside(target, side) {
            Ok(d) => d,
            Err(o) => return o,
        };
        if parts != c.parts {
            return Outcome::unavailable("the headings aren't in one story");
        }
        let after = |at: At| at > c.last;
        let inside = |at: At| at >= c.first && at <= c.last;
        let next = self.after_at(&parts, c.last);
        match dest {
            _ if dest == next || dest == Dest::Before(c.first) => {
                return match level {
                    Some(l) => self.shift_heading("Move Heading", heading, l as i64 - c.level as i64, fonts),
                    None => Outcome::Unchanged("in place already"),
                };
            }
            Dest::Before(at) if inside(at) => return Outcome::invalid("a heading can't move into its own content"),
            _ => {}
        }
        let down = match dest {
            Dest::Before(at) => after(at),
            Dest::End => true,
        };
        self.begin_batch(Some("Move Heading".into()));
        let moved = if self.tracking() { self.move_content_tracked(&c, dest, down, fonts) } else { self.move_content(&c, dest, down, fonts) };
        let (moved, moved_last) = match moved {
            Ok(h) => h,
            Err(o) => {
                self.end_batch(false, fonts);
                return o;
            }
        };
        // The headings that moved, as the move placed them: not the ones that follow them in their new place.
        let headings = self.headings_between(moved, moved_last);
        if let Some(l) = level {
            let out = self.shift_headings("Move Heading", &headings, l as i64 - c.level as i64, fonts);
            if failed(&out) {
                self.end_batch(false, fonts);
                return out;
            }
        }
        if let Some(story) = self.index.para_slot(moved).map(|s| self.index.story(s.container)) {
            self.selection = Selection::Caret(Position::text(story, moved, Unit(0)));
        }
        self.end_batch(true, fonts)
    }

    /// Whether protection, a lock or a shown sub-template keeps content `c` from being deleted, or blocks from going
    /// at `dest`.
    fn content_kept(&self, c: &Content, dest: Option<Dest>) -> bool {
        let story = self.index.story(c.parts[c.first.part]);
        let start = |p: &Paragraph| Position::text(story, ParaId(p.id), Unit(0));
        let end = |p: &Paragraph| Position::text(story, ParaId(p.id), para_len(p));
        let edge = |at: At, last: bool| self.part_block(&c.parts, at).and_then(|b| edge_para_ref(b, last));
        let (Some(a), Some(b)) = (edge(c.first, false), edge(c.last, true)) else { return true };
        if self.kept(CommandId::Delete, &start(a), &end(b)) {
            return true;
        }
        let at = match dest {
            None => return false,
            Some(Dest::Before(at)) => edge(at, false).map(start),
            Some(Dest::End) => {
                let part = c.parts.len() - 1;
                let n = self.part_len(&c.parts, part);
                n.checked_sub(1).and_then(|block| edge(At { part, block }, true)).map(end)
            }
        };
        at.is_none_or(|at| self.kept(CommandId::Paste, &at, &at))
    }

    /// The blocks in order from `from` through `to`, as (part, block range) runs.
    fn runs(&self, parts: &[Container], from: At, to: At) -> Vec<(usize, std::ops::Range<usize>)> {
        (from.part..=to.part)
            .map(|k| (k, if k == from.part { from.block } else { 0 }..if k == to.part { to.block + 1 } else { self.part_len(parts, k) }))
            .collect()
    }

    /// The place just after top-level block `at`, across its story's sections.
    fn after_at(&self, parts: &[Container], at: At) -> Dest {
        if at.block + 1 < self.part_len(parts, at.part) {
            Dest::Before(At { block: at.block + 1, ..at })
        } else if at.part + 1 < parts.len() {
            Dest::Before(At { part: at.part + 1, block: 0 })
        } else {
            Dest::End
        }
    }

    /// The block controls blocks put at `dest` go in: those around both blocks beside it.
    fn controls_at(&self, parts: &[Container], dest: Dest) -> Vec<ControlId> {
        match self.blocks_beside(parts, dest) {
            (Some(x), Some(y)) => common(controls(x), controls(y)),
            (Some(z), None) | (None, Some(z)) => controls(z).to_vec(),
            (None, None) => Vec::new(),
        }
    }

    /// The top-level block before `at` and the one at `at` in the story, across its sections.
    fn blocks_beside(&self, parts: &[Container], at: Dest) -> (Option<&Block>, Option<&Block>) {
        let last = |k: usize| self.part_len(parts, k).checked_sub(1).and_then(|b| self.part_block(parts, At { part: k, block: b }));
        match at {
            Dest::End => (last(parts.len() - 1), None),
            Dest::Before(at) if at.block > 0 => (self.part_block(parts, At { block: at.block - 1, ..at }), self.part_block(parts, at)),
            Dest::Before(at) => (at.part.checked_sub(1).and_then(last), self.part_block(parts, at)),
        }
    }

    /// The blocks `c` passes on its way to `dest`, as runs.
    fn passed(&self, c: &Content, dest: Dest, down: bool) -> Vec<(usize, std::ops::Range<usize>)> {
        let parts = &c.parts;
        let prev = |at: Dest| {
            let (part, block) = match at {
                Dest::Before(at) if at.block > 0 => (at.part, at.block),
                Dest::Before(at) => (at.part.checked_sub(1)?, self.part_len(parts, at.part - 1)),
                Dest::End => (parts.len() - 1, self.part_len(parts, parts.len() - 1)),
            };
            Some(At { part, block: block.checked_sub(1)? })
        };
        let next = match self.after_at(parts, c.last) {
            Dest::Before(at) => Some(at),
            Dest::End => None,
        };
        let (from, to) = match (down, dest) {
            (true, _) => (next, prev(dest)),
            (false, Dest::Before(at)) => (Some(at), prev(Dest::Before(c.first))),
            (false, Dest::End) => (None, None),
        };
        match (from, to) {
            (Some(a), Some(b)) if a <= b => self.runs(parts, a, b),
            _ => Vec::new(),
        }
    }

    /// The spans with an end in `c` and the other in the blocks it passes: a field refuses; bookmarks, comment and
    /// move ranges keep their text where it is, their ends in `c` staying at its side of the blocks passed.
    fn crossing(&self, c: &Content, dest: Dest, down: bool) -> Result<HashSet<Span>, Outcome> {
        let mut moved = HashSet::new();
        for (k, r) in self.runs(&c.parts, c.first, c.last) {
            if let Some(blocks) = self.index.blocks(&self.doc, c.parts[k]) {
                spans(&blocks[r], &mut moved);
            }
        }
        if moved.is_empty() {
            return Ok(moved);
        }
        let mut over = HashSet::new();
        for (k, r) in self.passed(c, dest, down) {
            if let Some(blocks) = self.index.blocks(&self.doc, c.parts[k]) {
                spans(&blocks[r], &mut over);
            }
        }
        let crossing: HashSet<Span> = moved.intersection(&over).cloned().collect();
        if crossing.iter().any(|s| matches!(s, Span::Field(_))) {
            return Err(Outcome::unavailable("a field runs from the heading's content into the text it passes"));
        }
        Ok(crossing)
    }

    /// The edge of the blocks passed beside where `c` was: the last before it moving up, the first after it moving down.
    fn passed_edge(&self, c: &Content, dest: Dest, down: bool) -> Option<At> {
        let runs = self.passed(c, dest, down);
        let (k, r) = if down { runs.first()? } else { runs.last()? };
        Some(At { part: *k, block: if down { r.start } else { r.end.checked_sub(1)? } })
    }

    /// [`Self::move_heading`]'s move, untracked: the blocks themselves (their paragraphs keep their ids) and the section
    /// breaks they hold go. Block controls around the content stay and the destination's go round it; a section break
    /// moved to the story's end is followed by a new last paragraph, as a section ends in its last paragraph. The
    /// heading and the content's last paragraph.
    fn move_content(&mut self, c: &Content, dest: Dest, down: bool, fonts: &Fonts) -> Result<(ParaId, ParaId), Outcome> {
        if self.content_kept(c, Some(dest)) {
            self.refused = Some("locked");
            self.events.push(super::input::Notice::Refused { sub_template: None, element: None });
            return Err(Outcome::Unchanged("locked"));
        }
        let crossing = self.crossing(c, dest, down)?;
        let parts = c.parts.clone();
        let heading =
            self.part_block(&parts, c.first).and_then(|b| edge_para_ref(b, false)).map(|p| ParaId(p.id)).ok_or(Outcome::Unchanged("no paragraph"))?;
        let end =
            self.part_block(&parts, c.last).and_then(|b| edge_para_ref(b, true)).map(|p| ParaId(p.id)).ok_or(Outcome::Unchanged("no paragraph"))?;
        let runs = self.runs(&parts, c.first, c.last);
        // The blocks and the breaks of the sections they end
        let ends_break = c.ends_break(self);
        let mut pieces: Vec<(Vec<Block>, Option<crate::docx::Section>)> = Vec::new();
        for (k, r) in &runs {
            let Some(blocks) = self.index.blocks(&self.doc, parts[*k]) else { return Err(Outcome::Unchanged("no paragraph")) };
            let shell = (*k < c.last.part || ends_break).then(|| self.doc.sections.get(*k).map(crate::docx::Section::settings)).flatten();
            pieces.push((blocks[r.clone()].to_vec(), shell));
        }
        // Block controls
        let before = self.blocks_beside(&parts, Dest::Before(c.first)).0;
        let next = self.blocks_beside(&parts, self.after_at(&parts, c.last)).1;
        let out_of: HashSet<ControlId> = before.into_iter().chain(next).flat_map(|b| controls(b).iter().copied()).collect();
        let into = self.controls_at(&parts, dest);
        for b in pieces.iter_mut().flat_map(|p| p.0.iter_mut()) {
            let own = match b {
                Block::Para(p) => &mut p.controls,
                Block::Table(t) => &mut t.controls,
            };
            own.retain(|c| !out_of.contains(c));
            own.splice(0..0, into.iter().copied());
        }
        // Markers whose partner is in the blocks passed
        let mut edge_step = None;
        if !crossing.is_empty() {
            let mut markers = Vec::new();
            for b in pieces.iter_mut().flat_map(|p| p.0.iter_mut()) {
                take_markers(std::slice::from_mut(b), &crossing, &mut markers);
            }
            if let Some(at) = self.passed_edge(c, dest, down)
                && let Some(mut b) = self.part_block(&parts, at).cloned()
            {
                if let Some(p) = edge_para(&mut b, !down) {
                    if down {
                        p.inlines.splice(0..0, markers);
                    } else {
                        p.inlines.extend(markers);
                    }
                }
                edge_step = Some(Step::Blocks { container: parts[at.part], range: at.block..at.block + 1, with: vec![b] });
            }
        }
        let tail = self.part_block(&parts, c.last).and_then(|b| edge_para_ref(b, true)).map(|p| ParaId(p.id)).and_then(|p| self.split_template(p));
        let last_para = {
            let part = parts.len() - 1;
            self.part_len(&parts, part)
                .checked_sub(1)
                .and_then(|b| self.part_block(&parts, At { part, block: b }))
                .and_then(|b| edge_para_ref(b, true))
        }
        .and_then(|p| self.split_template(ParaId(p.id)));
        let mut tx = self.begin("Move Heading");
        let ok = (|| -> Result<(), StepError> {
            if let Some(s) = edge_step {
                self.step(&mut tx, s)?;
            }
            let breaks = c.breaks(self);
            let start_r = if ends_break { 0 } else { c.last.block + 1 };
            let r = c.first.part + breaks;
            self.remove_content(&mut tx, &parts, c, breaks)?;
            // Where the place is once the content is gone
            let dest = match dest {
                Dest::Before(at) if down && at.part == r => Dest::Before(At { part: c.first.part, block: c.first.block + at.block - start_r }),
                Dest::Before(at) if down => Dest::Before(At { part: at.part - breaks, ..at }),
                d => d,
            };
            let parts: Vec<Container> = match parts[0] {
                Container::Section(_) => (0..self.doc.sections.len()).map(Container::Section).collect(),
                _ => parts.clone(),
            };
            self.insert_pieces(&mut tx, &parts, dest, pieces, last_para)?;
            self.end_story(&mut tx, &parts, tail, false)
        })();
        if ok.is_err() {
            self.abort(tx);
            return Err(Outcome::Unchanged("the heading didn't move"));
        }
        self.commit(tx, None, fonts);
        Ok((heading, end))
    }

    /// Takes content `c`, holding `breaks` section breaks, out of its story: its breaks go, the sections they end
    /// joining the one after.
    fn remove_content(&mut self, tx: &mut super::Tx, parts: &[Container], c: &Content, breaks: usize) -> Result<(), StepError> {
        let n: usize = self.runs(parts, c.first, c.last).into_iter().map(|(_, r)| r.len()).sum();
        for _ in 0..breaks {
            self.step(tx, Step::SectionBreak { section: c.first.part, back: None })?;
        }
        let container = match parts[c.first.part] {
            Container::Section(_) => Container::Section(c.first.part),
            other => other,
        };
        self.step(tx, Step::Blocks { container, range: c.first.block..c.first.block + n, with: Vec::new() }).map(|_| ())
    }

    /// Puts `pieces` (blocks, and the section each ends if it ends one) at `dest`, the section it is in split at each
    /// break. `last` is a paragraph like the story's last, for a story that would end in a break.
    fn insert_pieces(
        &mut self,
        tx: &mut super::Tx,
        parts: &[Container],
        dest: Dest,
        pieces: Vec<(Vec<Block>, Option<crate::docx::Section>)>,
        last: Option<Paragraph>,
    ) -> Result<(), StepError> {
        let (part, at) = match dest {
            Dest::Before(at) => (at.part, at.block),
            Dest::End => (parts.len() - 1, self.part_len(parts, parts.len() - 1)),
        };
        // A section break at the story's end
        let closes = dest == Dest::End && pieces.last().is_some_and(|p| p.1.is_some());
        let mut blocks: Vec<Block> = Vec::new();
        let mut cuts: Vec<(usize, crate::docx::Section)> = Vec::new();
        for (k, (b, shell)) in pieces.into_iter().enumerate() {
            let n = b.len() + if k == 0 { at } else { 0 };
            blocks.extend(b);
            if let Some(s) = shell {
                cuts.push((n, s));
            }
        }
        if closes && let Some(p) = last {
            blocks.push(Block::Para(Paragraph { id: crate::docx::next_para_id(), ..p }));
        }
        self.step(tx, Step::Blocks { container: parts[part], range: at..at, with: blocks })?;
        for (t, (n, shell)) in cuts.into_iter().enumerate() {
            let back = Box::new(SectionBack { blocks: n, section: shell, sources: 0, edited: true });
            self.step(tx, Step::SectionBreak { section: part + t, back: Some(back) })?;
        }
        Ok(())
    }

    /// A story ends in a paragraph: one like `like` after a table, or in a story left empty; `always`, whatever it
    /// ends in.
    fn end_story(&mut self, tx: &mut super::Tx, parts: &[Container], like: Option<Paragraph>, always: bool) -> Result<(), StepError> {
        let part = parts.len() - 1;
        let n = self.part_len(parts, part);
        let needs = always || n == 0 || matches!(self.part_block(parts, At { part, block: n - 1 }), Some(Block::Table(_)));
        if needs && let Some(p) = like {
            self.step(tx, Step::Blocks { container: parts[part], range: n..n, with: vec![Block::Para(p)] })?;
        }
        Ok(())
    }

    /// [`Self::move_heading`]'s move while changes are tracked: copies of the blocks go to `dest` (moved to) and they
    /// are deleted where they are (moved from), a move as Word records it. The copies stay in one section: a section
    /// break can't be tracked, so the breaks stay where they were, as a tracked delete keeps them. A comment's range
    /// into the blocks passed keeps its ends in the text moved from. Past the story's last paragraph, the copies' last
    /// ends in the story's last mark, taking its properties; moving the story's end up, the paragraph before it joins
    /// that mark. The copied heading and the copies' last paragraph.
    fn move_content_tracked(&mut self, c: &Content, dest: Dest, down: bool, fonts: &Fonts) -> Result<(ParaId, ParaId), Outcome> {
        if self.content_kept(c, Some(dest)) {
            self.refused = Some("locked");
            self.events.push(super::input::Notice::Refused { sub_template: None, element: None });
            return Err(Outcome::Unchanged("locked"));
        }
        let crossing = self.crossing(c, dest, down)?;
        let parts = c.parts.clone();
        let mut blocks = Vec::new();
        for (k, r) in self.runs(&parts, c.first, c.last) {
            blocks.extend_from_slice(self.index.blocks(&self.doc, parts[k]).map_or(&[][..], |b| &b[r]));
        }
        let (first, last) = match (blocks.first().and_then(|b| edge_para_ref(b, false)), blocks.last().and_then(|b| edge_para_ref(b, true))) {
            (Some(a), Some(b)) => (ParaId(a.id), ParaId(b.id)),
            _ => return Err(Outcome::Unchanged("no paragraph")),
        };
        let mut frag = self.clip_blocks(blocks);
        frag.moved = true;
        // Comment ranges into the blocks passed
        for b in &mut frag.blocks {
            if let Block::Para(p) = b {
                p.inlines.retain(|i| !matches!(span_of(i), Some(s @ Span::Comment(_)) if crossing.contains(&s)));
            }
        }
        let into = self.controls_at(&parts, dest);
        let story_end = c.last.part + 1 == parts.len() && c.last.block + 1 == self.part_len(&parts, c.last.part);
        // Moved to
        let past = match dest {
            Dest::End => self.blocks_beside(&parts, Dest::End).0.and_then(|b| edge_para_ref(b, true)).map(|p| ParaId(p.id)),
            Dest::Before(_) => None,
        };
        let (container, at) = match dest {
            Dest::Before(at) => (parts[at.part], at.block),
            Dest::End => (parts[parts.len() - 1], self.part_len(&parts, parts.len() - 1)),
        };
        let src_last = self.index.para(&self.doc, last).cloned();
        // The paragraph joining the story's last mark
        let joins = match self.blocks_beside(&parts, Dest::Before(c.first)) {
            (Some(Block::Para(x)), _) if story_end && c.first.block > 0 => Some(ParaId(x.id)),
            _ => None,
        };
        let (copy_first, copy_last) =
            self.paste_move_copies(&frag, container, at, past, &into, fonts).ok_or(Outcome::Unchanged("the heading didn't move"))?;
        let to = self.recorded.0.clone();
        // Moved from
        let story = self.index.story(container);
        let mut tx = self.begin("Move Heading");
        let ok = (|| -> Result<(), StepError> {
            let (Some((fc, fb)), Some((lc, lb))) = (self.top_of(first), self.top_of(last)) else { return Err(StepError::Missing) };
            let (Some((_, fp)), Some((_, lp))) = (self.parts_of(fc), self.parts_of(lc)) else { return Err(StepError::Missing) };
            for (k, r) in self.runs(&parts, At { part: fp, block: fb }, At { part: lp, block: lb }) {
                let keep_last = story_end && k == lp;
                let r = if keep_last { r.start..r.end - 1 } else { r };
                if !r.is_empty() {
                    self.step(&mut tx, Step::Blocks { container: parts[k], range: r, with: Vec::new() })?;
                }
            }
            if story_end && let Some(p) = self.index.para(&self.doc, last) {
                let len = para_len(p);
                self.step(&mut tx, Step::Text { para: last, range: Unit(0)..len, with: Vec::new(), edges: Default::default() })?;
            }
            Ok(())
        })();
        if ok.is_err() {
            self.abort(tx);
            return Err(Outcome::Unchanged("the heading didn't move"));
        }
        if !matches!(self.commit(tx, None, fonts), Outcome::Changed(_)) {
            return Err(Outcome::Unchanged("the heading didn't move"));
        }
        let from = self.recorded.1.clone();
        if !to.is_empty() && !from.is_empty() {
            self.record_move(&to, &from, fonts);
        }
        // The story's last mark
        let step = match (past, src_last, joins) {
            (Some(_), Some(src), _) => Some(Step::Para { para: copy_last, style: src.style.clone(), mark: src.mark.clone(), list: src.list.clone() }),
            (None, _, Some(x)) => Some(Step::Join { first: x, second: last }),
            _ => None,
        };
        if let Some(step) = step {
            let mut tx = self.begin("Move Heading");
            if self.step(&mut tx, step).is_err() {
                self.abort(tx);
            } else {
                self.commit(tx, None, fonts);
            }
        }
        self.selection = Selection::Caret(Position::text(story, copy_first, Unit(0)));
        Ok((copy_first, copy_last))
    }

    /// Deletes a heading with its content (a navigation pane's Delete), as one undo step; tracked when changes are. The
    /// story's last paragraph mark stays.
    pub fn delete_heading(&mut self, heading: ParaId, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        let c = match self.heading_content(heading) {
            Ok(c) => c,
            Err(o) => return o,
        };
        if self.content_kept(&c, None) {
            self.refused = Some("locked");
            self.events.push(super::input::Notice::Refused { sub_template: None, element: None });
            return Outcome::Unchanged("locked");
        }
        let parts = c.parts.clone();
        let story = self.index.story(parts[c.first.part]);
        let story_end = c.last.part + 1 == parts.len() && c.last.block + 1 == self.part_len(&parts, c.last.part);
        let last = self.part_block(&parts, c.last).and_then(|b| edge_para_ref(b, true)).map(|p| ParaId(p.id));
        let tail = last.and_then(|p| self.split_template(p));
        // The caret afterwards
        let after = match self.after_at(&parts, c.last) {
            Dest::Before(at) => {
                self.part_block(&parts, at).and_then(|b| edge_para_ref(b, false)).map(|p| Position::text(story, ParaId(p.id), Unit(0)))
            }
            Dest::End => None,
        };
        let before = self
            .blocks_beside(&parts, Dest::Before(c.first))
            .0
            .and_then(|b| edge_para_ref(b, true))
            .map(|p| Position::text(story, ParaId(p.id), para_len(p)));
        let tracked = self.tracking();
        let mut tx = self.begin("Delete Heading");
        let ok = (|| -> Result<(), StepError> {
            if tracked {
                for (k, r) in self.runs(&parts, c.first, c.last) {
                    let r = if story_end && k == c.last.part { r.start..r.end - 1 } else { r };
                    if !r.is_empty() {
                        self.step(&mut tx, Step::Blocks { container: parts[k], range: r, with: Vec::new() })?;
                    }
                }
                if story_end && let Some(last) = last.and_then(|l| self.index.para(&self.doc, l).map(|p| (l, para_len(p)))) {
                    self.step(&mut tx, Step::Text { para: last.0, range: Unit(0)..last.1, with: Vec::new(), edges: Default::default() })?;
                }
                return Ok(());
            }
            let breaks = c.breaks(self);
            self.remove_content(&mut tx, &parts, &c, breaks)?;
            let parts: Vec<Container> = match parts[0] {
                Container::Section(_) => (0..self.doc.sections.len()).map(Container::Section).collect(),
                _ => parts.clone(),
            };
            // The story's last mark
            self.end_story(&mut tx, &parts, tail, story_end)
        })();
        if ok.is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the heading wasn't deleted");
        }
        self.selection = match (tracked, after, before) {
            (true, _, _) => Selection::Caret(Position::text(story, heading, Unit(0))),
            (false, Some(a), _) => Selection::Caret(a),
            (false, None, Some(b)) => Selection::Caret(b),
            _ => self.selection,
        };
        self.commit(tx, None, fonts)
    }

    /// Inserts an empty heading beside `heading` (a navigation pane's New Heading Before, New Heading After and New
    /// Subheading), as one undo step, the caret in it: like the heading, a level below for a subheading.
    pub fn insert_heading(&mut self, heading: ParaId, place: NewHeading, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        let Some(level) = self.outline_level(heading) else { return Outcome::invalid("not a heading") };
        if place == NewHeading::Subheading && level == 9 {
            return Outcome::unavailable("no level below 9");
        }
        let c = match self.heading_content(heading) {
            Ok(c) => c,
            Err(o) => return o,
        };
        let story = self.index.story(c.parts[0]);
        let end = |b: Option<&Block>| edge_para_ref(b?, true).map(|p| Position::text(story, ParaId(p.id), para_len(p)));
        // Where the new paragraph goes and where it is typed
        let (dest, typed) = if place == NewHeading::Before {
            let at = self.part_block(&c.parts, c.first).and_then(|b| edge_para_ref(b, false)).map(|p| Position::text(story, ParaId(p.id), Unit(0)));
            (Dest::Before(c.first), at)
        } else {
            (self.after_at(&c.parts, c.last), end(self.part_block(&c.parts, c.last)))
        };
        let parts = c.parts;
        let at = match dest {
            Dest::Before(_) => typed,
            Dest::End => end(self.blocks_beside(&parts, Dest::End).0),
        };
        if typed.is_none_or(|at| self.kept(CommandId::InsertParagraph, &at, &at)) {
            self.refused = Some("locked");
            self.events.push(super::input::Notice::Refused { sub_template: None, element: None });
            return Outcome::Unchanged("locked");
        }
        let Some(mut new) = self.split_template(heading) else { return Outcome::Unchanged("no paragraph") };
        let id = ParaId(new.id);
        new.controls = self.controls_at(&parts, dest);
        // The editable ranges the content's last paragraph ends go on through the new one
        let ends: Vec<Inline> = match (place, self.part_block(&parts, c.last)) {
            (NewHeading::After | NewHeading::Subheading, Some(Block::Para(p))) => {
                let n =
                    p.inlines.iter().rev().take_while(|i| matches!(i, Inline::Opaque(o) if o.permission().is_some_and(|(start, _)| !start))).count();
                p.inlines[p.inlines.len() - n..].to_vec()
            }
            _ => Vec::new(),
        };
        let split = match (dest, self.part_block(&parts, c.last)) {
            (Dest::Before(next), Some(Block::Para(p))) if !ends.is_empty() && next.part == c.last.part => Some((ParaId(p.id), para_len(p))),
            _ => None,
        };
        self.begin_batch(Some(match place {
            NewHeading::Before => "New Heading Before".into(),
            NewHeading::After => "New Heading After".into(),
            NewHeading::Subheading => "New Subheading".into(),
        }));
        let mut tx = self.begin("New Heading");
        let step = match (dest, at.and_then(|a| a.para_offset())) {
            (Dest::Before(_), _) if let Some((last, len)) = split => {
                new.inlines = ends;
                Step::Split { para: last, at: len, second: new, kept: Vec::new() }
            }
            (Dest::Before(at), _) => Step::Blocks { container: parts[at.part], range: at.block..at.block, with: vec![Block::Para(new)] },
            // A split of the story's last paragraph
            (Dest::End, Some((last, len))) => {
                if self.part_block(&parts, c.last).and_then(|b| edge_para_ref(b, true)).is_some_and(|p| p.id == last.0) {
                    new.inlines = ends;
                }
                Step::Split { para: last, at: len, second: new, kept: Vec::new() }
            }
            (Dest::End, None) => {
                self.abort(tx);
                self.end_batch(false, fonts);
                return Outcome::Unchanged("no paragraph");
            }
        };
        if self.step(&mut tx, step).is_err() {
            self.abort(tx);
            self.end_batch(false, fonts);
            return Outcome::Unchanged("the heading wasn't inserted");
        }
        self.selection = Selection::Caret(Position::text(story, id, Unit(0)));
        self.commit(tx, None, fonts);
        if place == NewHeading::Subheading {
            let out = self.relevel("New Subheading", &[(id, Target::Level(level + 1))], fonts);
            if failed(&out) {
                self.end_batch(false, fonts);
                return out;
            }
        }
        self.end_batch(true, fonts)
    }
}
