//! Moving paragraphs up and down

use std::collections::HashSet;

use crate::api::command::CommandId;
use crate::docx::review::RangeKind;
use crate::docx::{Block, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::index::Container;
use super::position::Position;
use super::selection::{Granularity, Selection};
use super::step::{Step, para_len};

/// What may span paragraphs, by its start or end: a bookmark (by name), a comment's or move's range, a field.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Span {
    Bookmark(String),
    Comment(i64),
    Move(i64),
    Field(u64),
}

fn span_of(i: &Inline) -> Option<Span> {
    match i {
        Inline::Bookmark(name) => Some(Span::Bookmark(name.trim_start_matches('/').to_string())),
        Inline::Range(r) if matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => Some(Span::Comment(r.id)),
        Inline::Range(r) => Some(Span::Move(r.id)),
        Inline::FieldBegin { id, .. } | Inline::FieldEnd { id, .. } => Some(Span::Field(id.0)),
        _ => None,
    }
}

/// Every [`Span`] with a start or end in `blocks` (tables' cells too).
fn spans(blocks: &[Block], out: &mut HashSet<Span>) {
    let mut stack = vec![blocks];
    while let Some(blocks) = stack.pop() {
        for b in blocks {
            match b {
                Block::Para(p) => out.extend(p.inlines.iter().filter_map(span_of)),
                Block::Table(t) => stack.extend(t.rows.iter().flat_map(|r| &r.cells).map(|c| &c.blocks[..])),
            }
        }
    }
}

/// Takes the markers of `crossing` out of `blocks` (tables' cells too), in text order.
fn take_markers(blocks: &mut [Block], crossing: &HashSet<Span>, out: &mut Vec<Inline>) {
    for b in blocks {
        match b {
            Block::Para(p) => p.inlines.retain(|i| match span_of(i) {
                Some(s) if crossing.contains(&s) => {
                    out.push(i.clone());
                    false
                }
                _ => true,
            }),
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| take_markers(&mut c.blocks, crossing, out)),
        }
    }
}

/// The first (or last) paragraph in `b`, into a table's first (or last) cell.
fn edge_para(b: &mut Block, end: bool) -> Option<&mut Paragraph> {
    let mut b = b;
    // Each step goes a table deeper.
    loop {
        match b {
            Block::Para(p) => return Some(p),
            Block::Table(t) => {
                let row = if end { t.rows.last_mut() } else { t.rows.first_mut() }?;
                let cell = if end { row.cells.last_mut() } else { row.cells.first_mut() }?;
                b = if end { cell.blocks.last_mut() } else { cell.blocks.first_mut() }?;
            }
        }
    }
}

/// The first (or last) paragraph in `b`, as [`edge_para`] finds it.
fn edge_para_ref(b: &Block, end: bool) -> Option<&Paragraph> {
    let mut b = b;
    // Each step goes a table deeper.
    loop {
        match b {
            Block::Para(p) => return Some(p),
            Block::Table(t) => {
                let row = if end { t.rows.last() } else { t.rows.first() }?;
                let cell = if end { row.cells.last() } else { row.cells.first() }?;
                b = if end { cell.blocks.last() } else { cell.blocks.first() }?;
            }
        }
    }
}

/// The units before `u` in `p` once its deletions are accepted: where a place in it is in a copy.
fn accepted_offset(p: &Paragraph, u: Unit) -> u32 {
    let mut at = 0u32;
    let mut kept = 0u32;
    for i in &p.inlines {
        let n = i.units() as u32;
        if at >= u.0 {
            break;
        }
        if i.run().is_none_or(|r| r.rev.del.is_none()) {
            kept += n.min(u.0 - at);
        }
        at += n;
    }
    kept
}

fn controls(b: &Block) -> &[crate::docx::ControlId] {
    match b {
        Block::Para(p) => &p.controls,
        Block::Table(t) => &t.controls,
    }
}

impl Editor {
    /// Alt+Shift+Up and Down outside a table's rows: the selected paragraphs, whole, change places with the block
    /// before (or after) them in their story, a table as one. One undo step; they stay selected. Tracked as a move
    /// when changes are tracked. A bookmark, comment or move range from them into the block passed keeps its text
    /// there, as cutting and pasting them would: its end in them stays where they were.
    pub fn move_paragraphs(&mut self, up: bool, fonts: &Fonts) -> Outcome {
        if self.refuse_expanded() || self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let paras = self.selected_paras();
        let (Some(&first), Some(&last)) = (paras.first(), paras.last()) else { return Outcome::Unchanged("no paragraph") };
        let (Some(a), Some(b)) = (self.index.para_slot(first), self.index.para_slot(last)) else { return Outcome::Unchanged("no paragraph") };
        if a.container != b.container {
            return Outcome::unavailable("the paragraphs aren't in one place");
        }
        let container = a.container;
        let Some(blocks) = self.index.blocks(&self.doc, container) else { return Outcome::Unchanged("no paragraph") };
        let (s0, s1) = (a.block, b.block);
        if (up && s0 == 0) || (!up && s1 + 1 >= blocks.len()) {
            return Outcome::Unchanged("at the story's edge");
        }
        let (r0, r1) = if up { (s0 - 1, s1) } else { (s0, s1 + 1) };
        let passed = if up { r0 } else { r1 };
        if self.move_kept(container, blocks, (s0, s1), passed, up) {
            self.refused = Some("locked");
            self.events.push(super::input::Notice::Refused { sub_template: None, element: None });
            return Outcome::Unchanged("locked");
        }
        let Some(blocks) = self.index.blocks(&self.doc, container) else { return Outcome::Unchanged("no paragraph") };
        // A content control around some of them only would break.
        if blocks[r0..=r1].iter().any(|x| controls(x) != controls(&blocks[s0])) {
            return self.region_boundary();
        }
        let (mut moved, mut over) = (HashSet::new(), HashSet::new());
        spans(&blocks[s0..=s1], &mut moved);
        spans(&blocks[passed..=passed], &mut over);
        let crossing: HashSet<Span> = moved.intersection(&over).cloned().collect();
        if crossing.iter().any(|s| matches!(s, Span::Field(_))) {
            return Outcome::unavailable("a field runs into the paragraph passed");
        }
        if self.tracking() {
            return self.move_paragraphs_tracked(up, container, (s0, s1), &crossing, fonts);
        }
        let ends = r1 + 1 == blocks.len();
        let mut with = blocks[r0..=r1].to_vec();
        if !crossing.is_empty() {
            let (mine, x) = if up { (1..with.len(), 0) } else { (0..with.len() - 1, with.len() - 1) };
            let mut markers = Vec::new();
            take_markers(&mut with[mine], &crossing, &mut markers);
            // They go to the side of the block passed where the paragraphs were.
            if let Some(p) = edge_para(&mut with[x], up) {
                if up {
                    p.inlines.extend(markers);
                } else {
                    p.inlines.splice(0..0, markers);
                }
            }
        }
        if up {
            with.rotate_left(1);
        } else {
            with.rotate_right(1);
        }
        // A story ends with a paragraph: the last one moving above a table leaves an empty one after it.
        if ends
            && matches!(with.last(), Some(Block::Table(_)))
            && let Some(p) = self.split_template(last)
        {
            with.push(Block::Para(p));
        }
        let mut tx = self.begin("Move Paragraphs");
        if self.step(&mut tx, Step::Blocks { container, range: r0..r1 + 1, with }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the paragraphs didn't move");
        }
        self.commit(tx, None, fonts)
    }

    /// Whether protection, a lock or a shown sub-template keeps blocks `s0..=s1` from moving past block `passed`:
    /// they are deleted, pasted at the edge of the one passed and, tracked, the story's last moving up joins it.
    fn move_kept(&self, container: Container, blocks: &[Block], (s0, s1): (usize, usize), passed: usize, up: bool) -> bool {
        let story = self.index.story(container);
        let start = |p: &Paragraph| Position::text(story, ParaId(p.id), Unit(0));
        let end = |p: &Paragraph| Position::text(story, ParaId(p.id), para_len(p));
        let (Some(a), Some(b), Some(x)) = (edge_para_ref(&blocks[s0], false), edge_para_ref(&blocks[s1], true), edge_para_ref(&blocks[passed], !up))
        else {
            return true;
        };
        let at = if up { start(x) } else { end(x) };
        if self.kept(CommandId::Delete, &start(a), &end(b)) || self.kept(CommandId::Paste, &at, &at) {
            return true;
        }
        // Tracked, the story's last paragraph moving up ends in the mark of the one passed.
        let joins = up && s1 + 1 == blocks.len() && self.tracking() && matches!(blocks[passed], Block::Para(_));
        joins && self.kept(CommandId::Delete, &end(x), &start(a))
    }

    /// [`Self::move_paragraphs`] while changes are tracked: the paragraphs `s0..=s1` of `container` copied, whole, to
    /// the other side of the block passed (moved to) and deleted where they are (moved from). The story keeps its last
    /// mark: its last paragraph moving up leaves its mark, which the paragraph passed then ends in (that one's mark
    /// deleted, its properties taken by the last as a tracked change); moving down past the last paragraph, a mark
    /// inserted after that one's text ends it and the last of them ends in the story's mark, taking their properties.
    fn move_paragraphs_tracked(
        &mut self,
        up: bool,
        container: Container,
        (s0, s1): (usize, usize),
        crossing: &HashSet<Span>,
        fonts: &Fonts,
    ) -> Outcome {
        let story = self.index.story(container);
        let Some(blocks) = self.index.blocks(&self.doc, container) else { return Outcome::Unchanged("no paragraph") };
        let n = blocks.len();
        let (Some(Block::Para(fp)), Some(Block::Para(lp))) = (blocks.get(s0), blocks.get(s1)) else { return Outcome::Unchanged("no paragraph") };
        let (first, last, last_len) = (ParaId(fp.id), ParaId(lp.id), para_len(lp));
        let src_last = lp.clone();
        let passed = if up { s0 - 1 } else { s1 + 1 };
        let passed_para = match blocks.get(passed) {
            Some(Block::Para(p)) => Some(ParaId(p.id)),
            _ => None,
        };
        let dest: Vec<crate::docx::ControlId> = controls(&blocks[passed]).to_vec();
        // The story's last paragraph moves up; or they go down past it.
        let with_final = up && s1 + 1 == n;
        let past_final = !up && passed + 1 == n && passed_para.is_some();
        // Where the selection's ends are in the copies: (in the last paragraph, its unit there)
        let place = |pos: Option<Position>| -> Option<(bool, u32)> {
            let (p, u) = pos?.para_offset()?;
            if p == first {
                Some((false, accepted_offset(fp, u)))
            } else if p == last {
                Some((true, accepted_offset(lp, u)))
            } else {
                None
            }
        };
        let was = self.selection;
        let ends = (place(was.anchor()), place(was.focus()));
        self.selection = Selection::range(Position::text(story, first, Unit(0)), Position::text(story, last, last_len), Granularity::Char);
        let Some(mut frag) = self.clip() else {
            self.selection = was;
            return Outcome::Unchanged("nothing to move");
        };
        frag.moved = true;
        // A comment's range into the block passed keeps its ends in the text moved from.
        for b in &mut frag.blocks {
            if let Block::Para(p) = b {
                p.inlines.retain(|i| !matches!(span_of(i), Some(s @ Span::Comment(_)) if crossing.contains(&s)));
            }
        }
        self.begin_batch(Some("Move Paragraphs".into()));
        let done =
            self.tracked_move_steps(up, container, (first, last, passed, passed_para), (with_final, past_final), &frag, &src_last, dest, fonts);
        let Some((copy_first, copy_last)) = done else {
            self.end_batch(false, fonts);
            self.selection = was;
            return Outcome::Unchanged("the paragraphs didn't move");
        };
        let at = |e: &Self, end: Option<(bool, u32)>| {
            let (in_last, u) = end?;
            let para = if in_last { copy_last } else { copy_first };
            let len = e.index.para(&e.doc, para).map_or(0, |p| para_len(p).0);
            Some(Position::text(story, para, Unit(u.min(len))))
        };
        let (a, f) = (at(self, ends.0), at(self, ends.1));
        self.selection = match (a, f) {
            (Some(a), Some(f)) => Selection::range(a, f, Granularity::Char),
            _ => {
                let len = self.index.para(&self.doc, copy_last).map_or(0, |p| para_len(p).0);
                Selection::range(Position::text(story, copy_first, Unit(0)), Position::text(story, copy_last, Unit(len)), Granularity::Char)
            }
        };
        self.end_batch(true, fonts)
    }

    /// The insertion, the deletion and what keeps the story's last mark, of [`Self::move_paragraphs_tracked`], in the
    /// batch open: the copies' first and last paragraphs.
    #[allow(clippy::too_many_arguments, reason = "the move's places, then what is moved")]
    fn tracked_move_steps(
        &mut self,
        up: bool,
        container: Container,
        (first, last, passed, passed_para): (ParaId, ParaId, usize, Option<ParaId>),
        (with_final, past_final): (bool, bool),
        frag: &super::clipboard::Fragment,
        src_last: &Paragraph,
        dest: Vec<crate::docx::ControlId>,
        fonts: &Fonts,
    ) -> Option<(ParaId, ParaId)> {
        let story = self.index.story(container);
        let changed = |o: Outcome| matches!(o, Outcome::Changed(_));
        // Moved to
        let (copy_first, copy_last) = if past_final {
            let c = passed_para?;
            let len = para_len(self.index.para(&self.doc, c)?);
            let second = self.split_template(c)?;
            let tail = ParaId(second.id);
            let mut tx = self.begin("Move Paragraphs");
            if self.step(&mut tx, Step::Split { para: c, at: len, second, kept: Vec::new() }).is_err() {
                self.abort(tx);
                return None;
            }
            if !changed(self.commit(tx, None, fonts)) {
                return None;
            }
            self.selection = Selection::Caret(Position::text(story, tail, Unit(0)));
            let mut open = frag.clone();
            open.open_end = true;
            if !changed(self.paste_fragment(&open, fonts)) {
                return None;
            }
            (tail, self.selection.focus()?.para_offset()?.0)
        } else {
            let mut whole = frag.clone();
            whole.open_end = false;
            self.paste_blocks(&whole, container, if up { passed } else { passed + 1 }, &dest, fonts)?
        };
        let to = self.recorded.0.clone();
        // Moved from
        let (b0, b1) = (self.index.para_slot(first)?.block, self.index.para_slot(last)?.block);
        let mut tx = self.begin("Move Paragraphs");
        let mut ok = true;
        if with_final {
            let len = para_len(self.index.para(&self.doc, last)?);
            ok &= b1 == b0 || self.step(&mut tx, Step::Blocks { container, range: b0..b1, with: Vec::new() }).is_ok();
            ok &= self.step(&mut tx, Step::Text { para: last, range: Unit(0)..len, with: Vec::new(), edges: Default::default() }).is_ok();
        } else {
            ok &= self.step(&mut tx, Step::Blocks { container, range: b0..b1 + 1, with: Vec::new() }).is_ok();
        }
        if !ok {
            self.abort(tx);
            return None;
        }
        if !changed(self.commit(tx, None, fonts)) {
            return None;
        }
        let from = self.recorded.1.clone();
        if !to.is_empty() && !from.is_empty() {
            self.record_move(&to, &from, fonts);
        }
        // The story's last mark
        let mut tx = self.begin("Move Paragraphs");
        let step = match (with_final, past_final, passed_para) {
            (true, _, Some(x)) => Some(Step::Join { first: x, second: last }),
            (_, true, _) => {
                Some(Step::Para { para: copy_last, style: src_last.style.clone(), mark: src_last.mark.clone(), list: src_last.list.clone() })
            }
            _ => None,
        };
        if let Some(step) = step {
            if self.step(&mut tx, step).is_err() {
                self.abort(tx);
                return None;
            }
            self.commit(tx, None, fonts);
        } else {
            self.abort(tx);
        }
        Some((copy_first, copy_last))
    }
}
