//! Recording an edit as revisions: each step is turned into the steps that mark it, before it applies.
//!
//! Inserted content is marked inserted by the session's author. Deleted content stays, marked deleted, except the
//! author's own insertions, which go. Property changes keep the oldest properties they replace, so rejecting one
//! restores what was there before any tracked change.

use std::collections::{HashMap, HashSet};
use std::mem::Discriminant;
use std::ops::Range;
use std::rc::Rc;

use crate::docx::review::{
    AuthorId, FormatChange, GridChange, IdCounter, MarkKind, ParaChange, PreviousProps, RevisionId, RevisionMark, RunRevisions, SectionChange,
    TableChange, Timestamp,
};
use crate::docx::{Block, Document, Inline, NoteKind, ParaStyle, Paragraph, Raw, Row, Run, RunStyle, Section, SectionPage};
use crate::layout::{ParaId, TableId, Unit};

use super::super::index::DocIndex;
use super::super::step::{Edges, Step, para_len};

/// Table edits that are applied as they are with tracking on: merging and splitting cells.
const UNTRACKED_TABLE_EDITS: [&str; 2] = ["Merge Cells", "Split Cells"];

/// One transaction's recording: who and when, and the marks its insertions, deletions and property changes share.
pub struct Recording {
    pub author: AuthorId,
    pub time: Timestamp,
    /// A move's name: what the transaction inserts is the move's destination, what it deletes its source.
    pub move_name: Option<Rc<str>>,
    /// The transaction's label: which table edits go untracked.
    label: &'static str,
    ins: Option<Rc<RevisionMark>>,
    del: Option<Rc<RevisionMark>>,
    fmt: Option<Rc<RevisionMark>>,
    /// Runs already marked in this transaction, by the run they replace (kept alive here, so the address stays
    /// theirs): equal runs stay one run.
    inserted: HashMap<*const RunStyle, (Run, Run)>,
    deleted: HashMap<*const RunStyle, (Run, Run)>,
    formatted: HashMap<(*const RunStyle, *const RunStyle, bool), (Run, Run, Run)>,
}

impl Recording {
    pub fn new(author: AuthorId, time: Timestamp, move_name: Option<Rc<str>>, label: &'static str) -> Recording {
        Recording {
            author,
            time,
            move_name,
            label,
            ins: None,
            del: None,
            fmt: None,
            inserted: HashMap::new(),
            deleted: HashMap::new(),
            formatted: HashMap::new(),
        }
    }

    /// The insertion and the deletion recorded so far.
    pub fn marks(&self) -> (Option<Rc<RevisionMark>>, Option<Rc<RevisionMark>>) {
        (self.ins.clone(), self.del.clone())
    }

    /// The steps that record `step`, to apply in its place.
    pub fn record(&mut self, step: Step, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Vec<Step> {
        match step {
            Step::Text { para, range, with, edges } => self.text(para, range, with, edges, doc, idx, ids),
            Step::Restyle { para, inlines } => self.restyle(para, inlines, doc, idx, ids),
            Step::Split { para, at, second } => self.split(para, at, second, doc, idx, ids),
            Step::Join { first, second } => self.join(first, second, doc, idx, ids),
            Step::Para { para, style, mark, list } => match idx.para(doc, para) {
                Some(p) => {
                    // A paragraph this transaction inserted takes its properties as they come.
                    let style = if self.new_here(&p.mark) { style } else { self.para_change(&p.style, style, ids) };
                    let mark = self.formatted_run(&p.mark, &mark, true, ids);
                    vec![Step::Para { para, style, mark, list }]
                }
                None => vec![Step::Para { para, style, mark, list }],
            },
            Step::Blocks { container, range, with } => self.blocks(container, range, with, doc, idx, ids),
            Step::Rows { table, range, with, grid } => self.rows(table, range, with, grid, doc, idx, ids),
            Step::TableProps { table, props } => {
                let mut out: Vec<Step> = self.table_change(table, false, doc, idx, ids).into_iter().collect();
                out.push(Step::TableProps { table, props });
                out
            }
            Step::TableWidth { table, width } => {
                let mut out: Vec<Step> = self.table_change(table, false, doc, idx, ids).into_iter().collect();
                out.push(Step::TableWidth { table, width });
                out
            }
            // A new note's text is inserted with its reference.
            Step::Note { kind, id, blocks: Some(b) } if note(doc, kind, id).is_none() => {
                vec![Step::Note { kind, id, blocks: Some(self.ins_blocks(b, ids)) }]
            }
            Step::SectionPage { section, page, change } => match doc.sections.get(section) {
                Some(s) => {
                    let change = self.section_change(s, &page, ids);
                    vec![Step::SectionPage { section, page, change }]
                }
                None => vec![Step::SectionPage { section, page, change }],
            },
            // Every other kind is untracked and passes through as it is: comments, controls, data parts, image data,
            // header and footer parts, notes taken out. (Comment markers inserted by `Text` steps carry no revision
            // either: see `run`.)
            other => vec![other],
        }
    }

    // ---------------- marks and runs ----------------

    fn mark(&self, ids: &mut IdCounter, kind: MarkKind) -> Rc<RevisionMark> {
        let moving = matches!(kind, MarkKind::MoveFrom | MarkKind::MoveTo);
        Rc::new(RevisionMark {
            id: RevisionId(ids.next_revision_id()),
            author: self.author,
            date: Some(self.time.clone()),
            kind,
            move_name: if moving { self.move_name.clone() } else { None },
        })
    }

    fn ins_mark(&mut self, ids: &mut IdCounter) -> Rc<RevisionMark> {
        if let Some(m) = &self.ins {
            return m.clone();
        }
        let m = self.mark(ids, if self.move_name.is_some() { MarkKind::MoveTo } else { MarkKind::Insert });
        self.ins = Some(m.clone());
        m
    }

    fn del_mark(&mut self, ids: &mut IdCounter) -> Rc<RevisionMark> {
        if let Some(m) = &self.del {
            return m.clone();
        }
        let m = self.mark(ids, if self.move_name.is_some() { MarkKind::MoveFrom } else { MarkKind::Delete });
        self.del = Some(m.clone());
        m
    }

    fn fmt_mark(&mut self, ids: &mut IdCounter) -> Rc<RevisionMark> {
        if let Some(m) = &self.fmt {
            return m.clone();
        }
        let m = self.mark(ids, MarkKind::Format);
        self.fmt = Some(m.clone());
        m
    }

    /// Text the author inserted (in any session) and nobody deleted: deleting it removes it.
    fn own_insertion(&self, r: &RunStyle) -> bool {
        r.rev.del.is_none() && r.rev.ins.as_ref().is_some_and(|m| m.author == self.author && m.kind == MarkKind::Insert)
    }

    /// Inserted by this transaction.
    fn new_here(&self, r: &RunStyle) -> bool {
        matches!((&r.rev.ins, &self.ins), (Some(a), Some(b)) if Rc::ptr_eq(a, b))
    }

    /// The run inserted text takes. Beside the author's own insertion it is that insertion's run, so the texts join.
    fn inserted(&mut self, r: &Run, ids: &mut IdCounter) -> Run {
        if self.new_here(r) || (self.move_name.is_none() && self.own_insertion(r)) {
            return r.clone();
        }
        if let Some((_, n)) = self.inserted.get(&Rc::as_ptr(r)) {
            return n.clone();
        }
        let n = with_rev(r, RunRevisions { ins: Some(self.ins_mark(ids)), del: None, fmt: None });
        self.inserted.insert(Rc::as_ptr(r), (r.clone(), n.clone()));
        n
    }

    /// The run marked deleted by the author, or `None` when the author already deleted it.
    fn deleted(&mut self, r: &Run, ids: &mut IdCounter) -> Option<Run> {
        let kind = if self.move_name.is_some() { MarkKind::MoveFrom } else { MarkKind::Delete };
        if r.rev.del.as_ref().is_some_and(|m| m.author == self.author && m.kind == kind && m.move_name == self.move_name) {
            return None;
        }
        if let Some((_, n)) = self.deleted.get(&Rc::as_ptr(r)) {
            return Some(n.clone());
        }
        let n = with_rev(r, RunRevisions { del: Some(self.del_mark(ids)), ..r.rev.clone() });
        self.deleted.insert(Rc::as_ptr(r), (r.clone(), n.clone()));
        Some(n)
    }

    /// `new`, the formatting `old` got, with `old`'s insertion and deletion, and (`record`) the change recorded:
    /// against the oldest formatting `old` had, none when it is that again.
    fn formatted_run(&mut self, old: &Run, new: &Run, record: bool, ids: &mut IdCounter) -> Run {
        if Rc::ptr_eq(old, new) {
            return new.clone();
        }
        let key = (Rc::as_ptr(old), Rc::as_ptr(new), record);
        if let Some((_, _, n)) = self.formatted.get(&key) {
            return n.clone();
        }
        let fmt = if self.new_here(old) {
            None
        } else if !record || same_look(old, new) {
            old.rev.fmt.clone()
        } else {
            let (previous, previous_raw) = match &old.rev.fmt {
                Some(f) => (f.previous.clone(), f.previous_raw.clone()),
                None => (Rc::new(RunStyle { rev: RunRevisions::default(), ..(**old).clone() }), old.direct.clone()),
            };
            (!same_look(&previous, new)).then(|| Rc::new(FormatChange { mark: self.fmt_mark(ids), previous, previous_raw }))
        };
        let rev = RunRevisions { ins: old.rev.ins.clone(), del: old.rev.del.clone(), fmt };
        let n = if new.rev == rev { new.clone() } else { with_rev(new, rev) };
        self.formatted.insert(key, (old.clone(), new.clone(), n.clone()));
        n
    }

    /// `new` with the change from `old` recorded against the oldest properties, none when it has those again.
    fn para_change(&mut self, old: &Rc<ParaStyle>, new: Rc<ParaStyle>, ids: &mut IdCounter) -> Rc<ParaStyle> {
        if Rc::ptr_eq(old, &new) {
            return new;
        }
        let change = if same_para(old, &new) {
            old.change.clone()
        } else {
            let (previous, previous_raw) = match &old.change {
                Some(c) => (c.previous.clone(), c.previous_raw.clone()),
                None => (Rc::new(ParaStyle { change: None, ..(**old).clone() }), old.direct.clone()),
            };
            (!same_para(&previous, &new))
                .then(|| Rc::new(ParaChange { mark: self.fmt_mark(ids), previous, previous_raw, numbering_change: Raw::default() }))
        };
        if new.change == change { new } else { Rc::new(ParaStyle { change, ..(*new).clone() }) }
    }

    /// The change `s` has once its page is `page`: recorded against the oldest page it had, none when it has that again.
    fn section_change(&mut self, s: &Section, page: &SectionPage, ids: &mut IdCounter) -> Option<Rc<SectionChange>> {
        if *page == s.page() {
            return s.change.clone();
        }
        // The earlier `w:sectPr` of a new change is written from the section as read, with the earlier page.
        let (previous, previous_raw) = match &s.change {
            Some(c) => (c.previous.clone(), c.previous_raw.clone()),
            None => (s.page(), Raw::default()),
        };
        (previous != *page).then(|| Rc::new(SectionChange { mark: self.fmt_mark(ids), previous, previous_raw }))
    }

    // ---------------- inlines and blocks ----------------

    fn ins_inline(&mut self, i: Inline, ids: &mut IdCounter) -> Inline {
        let Some(r) = run(&i).cloned() else { return i };
        let r = self.inserted(&r, ids);
        let mut i = with_run(&i, r);
        if let Inline::Float(f, _) = &mut i
            && let Some(tb) = &mut f.text_box
        {
            tb.blocks = self.ins_blocks(std::mem::take(&mut tb.blocks), ids);
        }
        i
    }

    fn ins_blocks(&mut self, blocks: Vec<Block>, ids: &mut IdCounter) -> Vec<Block> {
        blocks.into_iter().map(|b| self.ins_block(b, ids)).collect()
    }

    fn ins_block(&mut self, b: Block, ids: &mut IdCounter) -> Block {
        match b {
            Block::Para(mut p) => {
                p.inlines = std::mem::take(&mut p.inlines).into_iter().map(|i| self.ins_inline(i, ids)).collect();
                p.mark = self.inserted(&p.mark, ids);
                Block::Para(p)
            }
            Block::Table(mut t) => {
                t.rows = std::mem::take(&mut t.rows).into_iter().map(|r| self.ins_row(r, ids)).collect();
                // A new table also has its properties recorded as changed.
                if t.change.is_none() {
                    t.change = Some(Rc::new(TableChange {
                        mark: self.fmt_mark(ids),
                        previous_raw: t.direct.clone(),
                        previous: Some(PreviousProps::of_table(&t)),
                    }));
                }
                Block::Table(t)
            }
        }
    }

    fn ins_row(&mut self, mut r: Row, ids: &mut IdCounter) -> Row {
        r.rev.ins = Some(self.ins_mark(ids));
        r.rev.del = None;
        for c in &mut r.cells {
            c.blocks = self.ins_blocks(std::mem::take(&mut c.blocks), ids);
        }
        r
    }

    /// The inline marked deleted, or `None` when it goes (`remove`: the author's own insertions do). Note references
    /// it deletes are added to `notes`.
    fn del_inline(&mut self, i: &Inline, remove: bool, ids: &mut IdCounter, notes: &mut Vec<(NoteKind, i64)>) -> Option<Inline> {
        let Some(r) = run(i) else { return Some(i.clone()) };
        if remove && self.own_insertion(r) {
            return None;
        }
        Some(match self.deleted(r, ids) {
            Some(n) => {
                if let Inline::NoteRef { kind, id, .. } = i {
                    notes.push((*kind, *id));
                }
                with_run(i, n)
            }
            None => i.clone(),
        })
    }

    /// The block kept and marked deleted, or `None` when it goes: `remove` lets the author's own insertions go (a
    /// paragraph when its text and mark are all theirs).
    fn del_block(&mut self, b: &Block, remove: bool, ids: &mut IdCounter, notes: &mut Vec<(NoteKind, i64)>) -> Option<Block> {
        match b {
            Block::Para(p) => {
                if remove && self.own_insertion(&p.mark) && p.inlines.iter().all(|i| run(i).is_none_or(|r| self.own_insertion(r))) {
                    return None;
                }
                let mut q = p.clone();
                q.inlines = p.inlines.iter().filter_map(|i| self.del_inline(i, remove, ids, notes)).collect();
                if !q.inlines.iter().any(|i| i.units() > 0 || matches!(i, Inline::Text(..))) {
                    q.inlines.push(Inline::Text(String::new(), p.mark.clone()));
                }
                if let Some(m) = self.deleted(&p.mark, ids) {
                    q.mark = m;
                }
                Some(Block::Para(q))
            }
            Block::Table(t) => {
                let mut n = t.clone();
                n.rows = t.rows.iter().filter_map(|r| self.del_row(r, remove, ids, notes)).collect();
                if n.rows.is_empty() {
                    return None;
                }
                if n.change.is_none() {
                    n.change = Some(Rc::new(TableChange {
                        mark: self.fmt_mark(ids),
                        previous_raw: t.direct.clone(),
                        previous: Some(PreviousProps::of_table(t)),
                    }));
                }
                Some(Block::Table(n))
            }
        }
    }

    /// The row kept and marked deleted (its cells' content too), or `None` for a row the author inserted (`remove`).
    fn del_row(&mut self, r: &Row, remove: bool, ids: &mut IdCounter, notes: &mut Vec<(NoteKind, i64)>) -> Option<Row> {
        let own = r.rev.del.is_none() && r.rev.ins.as_ref().is_some_and(|m| m.author == self.author && m.kind == MarkKind::Insert);
        if remove && own {
            return None;
        }
        let mut n = r.clone();
        if !r.rev.del.as_ref().is_some_and(|m| m.author == self.author) {
            n.rev.del = Some(self.del_mark(ids));
        }
        for c in &mut n.cells {
            // A cell keeps its paragraphs.
            c.blocks = c.blocks.iter().filter_map(|b| self.del_block(b, false, ids, notes)).collect();
        }
        Some(n)
    }

    /// The deleted notes' text, marked deleted with their references.
    fn note_steps(&mut self, notes: Vec<(NoteKind, i64)>, doc: &Document, ids: &mut IdCounter) -> Vec<Step> {
        let mut out = Vec::new();
        for (kind, id) in notes {
            let Some(blocks) = note(doc, kind, id) else { continue };
            let marked: Vec<Block> = blocks.iter().filter_map(|b| self.del_block(b, false, ids, &mut Vec::new())).collect();
            if marked != *blocks {
                out.push(Step::Note { kind, id, blocks: Some(marked) });
            }
        }
        out
    }

    // ---------------- steps ----------------

    #[allow(clippy::too_many_arguments, reason = "the step's fields, then the document")]
    fn text(
        &mut self,
        para: ParaId,
        range: Range<Unit>,
        with: Vec<Inline>,
        edges: Edges,
        doc: &Document,
        idx: &DocIndex,
        ids: &mut IdCounter,
    ) -> Vec<Step> {
        let Some(p) = idx.para(doc, para).filter(|p| range.start < range.end && range.end <= para_len(p)) else {
            // An insertion (or a step the document refuses).
            let with = with.into_iter().map(|i| self.ins_inline(i, ids)).collect();
            return vec![Step::Text { para, range, with, edges }];
        };
        let (a, b) = (range.start.0, range.end.0);
        // The same content in other runs: a formatting change.
        if !with.is_empty() && content(&slice(&p.inlines, a, b)) == content(&with) {
            let with = self.formatted(&slice(&p.inlines, a, b), with, true, ids);
            return vec![Step::Text { para, range, with, edges }];
        }
        let mut notes = Vec::new();
        let mut gone: Vec<Range<u32>> = Vec::new();
        let mut inlines: Vec<Inline> = Vec::with_capacity(p.inlines.len() + 2);
        let mut changed = false;
        let mut at = 0u32;
        for i in &p.inlines {
            let n = i.units() as u32;
            let (s, e) = (at, at + n);
            at = e;
            let inside = if n == 0 { s > a && s < b } else { s < b && e > a };
            let Some(r) = run(i).filter(|_| inside) else {
                push(&mut inlines, i.clone());
                continue;
            };
            // A text straddling the range's edges: its part inside is marked.
            let (before, mid, after) = match i {
                Inline::Text(t, r) if s < a || e > b => {
                    let cut = |u: u32| t.char_indices().nth((u - s) as usize).map_or(t.len(), |(k, _)| k);
                    let (x, y) = (cut(a.max(s)), cut(b.min(e)));
                    (
                        Inline::Text(t[..x].to_string(), r.clone()),
                        Inline::Text(t[x..y].to_string(), r.clone()),
                        Inline::Text(t[y..].to_string(), r.clone()),
                    )
                }
                _ => (Inline::Text(String::new(), r.clone()), i.clone(), Inline::Text(String::new(), r.clone())),
            };
            push(&mut inlines, before);
            if self.own_insertion(r) {
                if n > 0 {
                    let g = s.max(a)..e.min(b);
                    match gone.last_mut() {
                        Some(last) if last.end == g.start => last.end = g.end,
                        _ => gone.push(g),
                    }
                }
                push(&mut inlines, mid);
            } else {
                match self.del_inline(&mid, false, ids, &mut notes) {
                    Some(m) if m != mid => {
                        changed = true;
                        push(&mut inlines, m);
                    }
                    _ => push(&mut inlines, mid),
                }
            }
            push(&mut inlines, after);
        }
        let mut out = Vec::new();
        if changed {
            inlines.retain(|i| !matches!(i, Inline::Text(t, _) if t.is_empty()));
            if inlines.is_empty() {
                inlines.push(Inline::Text(String::new(), p.mark.clone()));
            }
            out.push(Step::Restyle { para, inlines });
        }
        // The author's own insertions go, last first so the earlier ones keep their place.
        for g in gone.iter().rev() {
            out.push(Step::Text { para, range: Unit(g.start)..Unit(g.end), with: Vec::new(), edges: Edges::default() });
        }
        // A replacement: the new content follows the deleted.
        if !with.is_empty() {
            let end = Unit(b - gone.iter().map(|g| g.end - g.start).sum::<u32>());
            let with = with.into_iter().map(|i| self.ins_inline(i, ids)).collect();
            out.push(Step::Text { para, range: end..end, with, edges: Edges::default() });
        }
        out.extend(self.note_steps(notes, doc, ids));
        out
    }

    /// `new`'s inlines with the revisions of `old`'s runs at the same units, and (`record`) their formatting changes
    /// recorded.
    fn formatted(&mut self, old: &[Inline], new: Vec<Inline>, record: bool, ids: &mut IdCounter) -> Vec<Inline> {
        let mut spans: Vec<(u32, u32, Run)> = Vec::new();
        let mut at = 0u32;
        for i in old {
            let n = i.units() as u32;
            if n > 0
                && let Some(r) = i.run()
            {
                spans.push((at, at + n, r.clone()));
            }
            at += n;
        }
        let old_at = |u: u32| spans.iter().find(|s| s.0 <= u && u < s.1).map(|s| s.2.clone());
        let mut out = Vec::with_capacity(new.len());
        let mut at = 0u32;
        for i in new {
            let n = i.units() as u32;
            let Some(r) = run(&i).cloned().filter(|_| n > 0) else {
                out.push(i);
                continue;
            };
            match &i {
                Inline::Text(t, _) => {
                    // Cut where the old runs change, so each piece records its own previous formatting.
                    let mut piece = String::new();
                    let mut from = at;
                    for (k, ch) in t.chars().enumerate() {
                        let u = at + k as u32;
                        if u > from && spans.iter().any(|s| s.0 == u) {
                            let o = old_at(from).unwrap_or_else(|| r.clone());
                            out.push(Inline::Text(std::mem::take(&mut piece), self.formatted_run(&o, &r, record, ids)));
                            from = u;
                        }
                        piece.push(ch);
                    }
                    let o = old_at(from).unwrap_or_else(|| r.clone());
                    out.push(Inline::Text(piece, self.formatted_run(&o, &r, record, ids)));
                }
                _ => {
                    let o = old_at(at).unwrap_or_else(|| r.clone());
                    let n = self.formatted_run(&o, &r, record, ids);
                    out.push(with_run(&i, n));
                }
            }
            at += n;
        }
        out
    }

    fn restyle(&mut self, para: ParaId, inlines: Vec<Inline>, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Vec<Step> {
        let Some(p) = idx.para(doc, para) else { return vec![Step::Restyle { para, inlines }] };
        // Runs resolved again (a paragraph's style or place changed) keep their revisions; that isn't a formatting
        // change of its own.
        if content(&p.inlines) == content(&inlines) {
            let inlines = self.formatted(&p.inlines, inlines, false, ids);
            return vec![Step::Restyle { para, inlines }];
        }
        // A control's markers taken out before its content is deleted: with tracking the control stays around its
        // deleted content.
        let control = |i: &Inline| matches!(i, Inline::ControlStart(_) | Inline::ControlEnd(_));
        let kept: Vec<&Inline> = p.inlines.iter().filter(|i| !control(i) || inlines.contains(i)).collect();
        if kept.len() < p.inlines.len() && kept.len() == inlines.len() && kept.iter().zip(&inlines).all(|(a, b)| *a == b) {
            return Vec::new();
        }
        vec![Step::Restyle { para, inlines }]
    }

    fn split(&mut self, para: ParaId, at: Unit, mut second: Paragraph, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Vec<Step> {
        let Some(p) = idx.para(doc, para).filter(|_| second.inlines.is_empty()) else { return vec![Step::Split { para, at, second }] };
        // The paragraph's own mark ends the second part; the first part's mark is the inserted one.
        if second.mark.rev != p.mark.rev {
            second.mark = with_rev(&second.mark, p.mark.rev.clone());
        }
        let mark = with_rev(&p.mark, RunRevisions { ins: Some(self.ins_mark(ids)), del: None, fmt: None });
        vec![Step::Split { para, at, second }, Step::Para { para, style: p.style.clone(), mark, list: p.list.clone() }]
    }

    fn join(&mut self, first: ParaId, second: ParaId, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Vec<Step> {
        let (Some(a), Some(b)) = (idx.para(doc, first), idx.para(doc, second)) else { return vec![Step::Join { first, second }] };
        let adjacent =
            matches!((idx.para_slot(first), idx.para_slot(second)), (Some(x), Some(y)) if x.container == y.container && y.block == x.block + 1);
        // A mark the author inserted goes: the paragraphs join, ending with the second's mark.
        if adjacent && self.own_insertion(&a.mark) {
            return vec![
                Step::Join { first, second },
                Step::Para { para: first, style: a.style.clone(), mark: b.mark.clone(), list: a.list.clone() },
            ];
        }
        let mut out = Vec::new();
        if let Some(mark) = self.deleted(&a.mark, ids) {
            out.push(Step::Para { para: first, style: a.style.clone(), mark, list: a.list.clone() });
        }
        // The paragraph after the deleted mark takes the first's properties, recorded as its change.
        if !same_para(&a.style, &b.style) {
            let style = self.para_change(&b.style, Rc::new(ParaStyle { change: None, ..(*a.style).clone() }), ids);
            out.push(Step::Para { para: second, style, mark: b.mark.clone(), list: b.list.clone() });
        }
        out
    }

    fn blocks(
        &mut self,
        container: super::super::index::Container,
        range: Range<usize>,
        with: Vec<Block>,
        doc: &Document,
        idx: &DocIndex,
        ids: &mut IdCounter,
    ) -> Vec<Step> {
        let old = idx.blocks(doc, container).and_then(|b| b.get(range.clone()));
        let Some(old) = old.filter(|o| !o.is_empty()) else {
            let with = self.ins_blocks(with, ids);
            return vec![Step::Blocks { container, range, with }];
        };
        // Blocks replaced by themselves (the same ids): a change of what they hold, applied as it is.
        let kept: HashSet<u64> = with.iter().map(block_id).collect();
        if old.iter().any(|b| kept.contains(&block_id(b))) {
            return vec![Step::Blocks { container, range, with }];
        }
        let mut notes = Vec::new();
        let mut new: Vec<Block> = old.iter().filter_map(|b| self.del_block(b, true, ids, &mut notes)).collect();
        new.extend(self.ins_blocks(with, ids));
        let mut out = vec![Step::Blocks { container, range, with: new }];
        out.extend(self.note_steps(notes, doc, ids));
        out
    }

    #[allow(clippy::too_many_arguments, reason = "the step's fields, then the document")]
    fn rows(
        &mut self,
        table: TableId,
        range: Range<usize>,
        with: Vec<Row>,
        grid: Option<Vec<f64>>,
        doc: &Document,
        idx: &DocIndex,
        ids: &mut IdCounter,
    ) -> Vec<Step> {
        let t = idx.table(doc, table);
        let Some((t, old)) = t.and_then(|t| Some((t, t.rows.get(range.clone())?))).filter(|_| !UNTRACKED_TABLE_EDITS.contains(&self.label)) else {
            return vec![Step::Rows { table, range, with, grid }];
        };
        // Rows are told apart by the paragraphs they hold. Rows that change places aren't recorded.
        let keys: Vec<HashSet<u64>> = old.iter().map(row_paras).collect();
        let matched: Vec<Option<usize>> = with
            .iter()
            .map(|r| {
                let k = row_paras(r);
                keys.iter().position(|o| !o.is_disjoint(&k))
            })
            .collect();
        let order: Vec<usize> = matched.iter().flatten().copied().collect();
        if order.windows(2).any(|w| w[0] >= w[1]) {
            return vec![Step::Rows { table, range, with, grid }];
        }
        let mut notes = Vec::new();
        let mut rows = Vec::with_capacity(with.len().max(old.len()));
        let mut changed = grid.as_ref().is_some_and(|g| *g != t.grid);
        let mut next = 0;
        for (i, o) in old.iter().enumerate() {
            match matched.iter().position(|m| *m == Some(i)) {
                Some(k) => {
                    for n in &with[next..k] {
                        rows.push(self.ins_row(n.clone(), ids));
                        changed = true;
                    }
                    let (r, c) = self.kept_row(o, &with[k], ids, &mut notes);
                    changed |= c;
                    rows.push(r);
                    next = k + 1;
                }
                None => {
                    changed = true;
                    if let Some(r) = self.del_row(o, true, ids, &mut notes) {
                        rows.push(r);
                    }
                }
            }
        }
        for n in &with[next..] {
            rows.push(self.ins_row(n.clone(), ids));
            changed = true;
        }
        let grid_changed = grid.as_ref().is_some_and(|g| *g != t.grid);
        let mut out: Vec<Step> = if changed { self.table_change(table, grid_changed, doc, idx, ids).into_iter().collect() } else { Vec::new() };
        out.push(Step::Rows { table, range, with: rows, grid });
        out.extend(self.note_steps(notes, doc, ids));
        out
    }

    /// A row that stays, as `n` has it. Content that came into a cell from another is inserted there, and what the
    /// cell had stays, deleted. Also whether its properties or cells changed.
    fn kept_row(&mut self, o: &Row, n: &Row, ids: &mut IdCounter, notes: &mut Vec<(NoteKind, i64)>) -> (Row, bool) {
        let mut r = n.clone();
        let props = (o.height.clone(), o.cant_split, o.header, o.grid_before, o.grid_after, o.w_before, o.w_after)
            != (n.height.clone(), n.cant_split, n.header, n.grid_before, n.grid_after, n.w_before, n.w_after);
        if o.cells.len() != n.cells.len() {
            // Cells came or went: applied, as a change of the table's properties.
            return (r, true);
        }
        let mut changed = props;
        for (c, oc) in r.cells.iter_mut().zip(&o.cells) {
            let looks = |x: &crate::docx::Cell| {
                (x.width, x.span, x.v_merge, x.top.clone(), x.left.clone(), x.bottom.clone(), x.right.clone(), x.shading, x.margins)
            };
            changed |= looks(c) != looks(oc) || (&c.v_align, &c.text_direction, c.no_wrap) != (&oc.v_align, &oc.text_direction, oc.no_wrap);
            if c.blocks.iter().map(block_id).ne(oc.blocks.iter().map(block_id)) {
                changed = true;
                c.blocks = self.moved_in(&oc.blocks, &c.blocks, ids, notes);
            }
        }
        (r, changed)
    }

    /// A cell's content when `new` took the place of `old`: `old`'s text kept and deleted (its paragraphs stay),
    /// with `new` inserted before it.
    fn moved_in(&mut self, old: &[Block], new: &[Block], ids: &mut IdCounter, notes: &mut Vec<(NoteKind, i64)>) -> Vec<Block> {
        let mut kept: Vec<Block> = old.iter().filter_map(|b| self.del_block(b, false, ids, notes)).collect();
        for (k, o) in kept.iter_mut().zip(old) {
            if let (Block::Para(k), Block::Para(o)) = (k, o) {
                k.mark = o.mark.clone();
            }
        }
        match (new, kept.first_mut()) {
            // One paragraph in: its text goes at the start of the cell's first paragraph.
            ([Block::Para(p)], Some(Block::Para(first))) => {
                let inlines: Vec<Inline> = p.inlines.iter().filter(|i| !matches!(i, Inline::Text(t, _) if t.is_empty())).cloned().collect();
                let inlines: Vec<Inline> = inlines.into_iter().map(|i| self.ins_inline(i, ids)).collect();
                first.inlines.splice(0..0, inlines);
                first.inlines.retain(|i| !matches!(i, Inline::Text(t, _) if t.is_empty()));
                if first.inlines.is_empty() {
                    first.inlines.push(Inline::Text(String::new(), first.mark.clone()));
                }
                kept
            }
            _ => {
                let fresh: Vec<Block> = new.iter().map(super::super::clipboard::fresh).collect();
                let mut out = self.ins_blocks(fresh, ids);
                out.extend(kept);
                out
            }
        }
    }

    /// A step recording a change of `table`'s properties (and of its grid), unless it has one: the oldest stays.
    fn table_change(&mut self, table: TableId, grid: bool, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Option<Step> {
        let slot = idx.table_slot(table)?;
        let Block::Table(t) = idx.blocks(doc, slot.container)?.get(slot.block)? else { return None };
        let (change, grid) = (t.change.is_none(), grid && t.grid_change.is_none());
        if !change && !grid {
            return None;
        }
        let mut n = t.clone();
        if change {
            n.change =
                Some(Rc::new(TableChange { mark: self.fmt_mark(ids), previous_raw: t.direct.clone(), previous: Some(PreviousProps::of_table(t)) }));
        }
        if grid {
            n.grid_change = Some(Rc::new(GridChange { mark: self.fmt_mark(ids), previous: t.grid.clone() }));
        }
        Some(Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: vec![Block::Table(n)] })
    }
}

/// Whether units `a..b` of `p` are all deleted text.
pub fn all_deleted(p: &Paragraph, a: Unit, b: Unit) -> bool {
    let mut at = 0u32;
    let mut any = false;
    for i in &p.inlines {
        let n = i.units() as u32;
        if n > 0 && at < b.0 && at + n > a.0 {
            if i.run().is_none_or(|r| r.rev.del.is_none()) {
                return false;
            }
            any = true;
        }
        at += n;
    }
    any
}

/// The run a revision marks on an inline: comment references and note separators carry none.
fn run(i: &Inline) -> Option<&Run> {
    match i {
        Inline::CommentRef { .. } | Inline::Separator { .. } => None,
        _ => i.run(),
    }
}

fn with_run(i: &Inline, r: Run) -> Inline {
    match i {
        Inline::Float(f, _) => Inline::Float(f.clone(), r),
        other => super::super::format::with_run(other, &r),
    }
}

fn with_rev(r: &Run, rev: RunRevisions) -> Run {
    Rc::new(RunStyle { rev, ..(**r).clone() })
}

/// The same formatting, revisions aside.
fn same_look(a: &RunStyle, b: &RunStyle) -> bool {
    RunStyle { rev: RunRevisions::default(), ..a.clone() } == RunStyle { rev: RunRevisions::default(), ..b.clone() }
}

/// The same paragraph properties, a recorded change aside.
fn same_para(a: &ParaStyle, b: &ParaStyle) -> bool {
    ParaStyle { change: None, ..a.clone() } == ParaStyle { change: None, ..b.clone() }
}

/// Appends `i`, joining a text in the same run before it.
fn push(out: &mut Vec<Inline>, i: Inline) {
    if let Inline::Text(t, r) = &i {
        if t.is_empty() {
            return;
        }
        if let Some(Inline::Text(prev, pr)) = out.last_mut()
            && Rc::ptr_eq(pr, r)
        {
            prev.push_str(t);
            return;
        }
    }
    out.push(i);
}

/// Units `a..b` of a paragraph's inlines, with the zero-unit inlines strictly inside.
fn slice(inlines: &[Inline], a: u32, b: u32) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut at = 0u32;
    for i in inlines {
        let n = i.units() as u32;
        let (s, e) = (at, at + n);
        at = e;
        if n == 0 {
            if s > a && s < b {
                out.push(i.clone());
            }
            continue;
        }
        if e <= a || s >= b {
            continue;
        }
        match i {
            Inline::Text(t, r) => {
                let part: String = t.chars().skip((a.max(s) - s) as usize).take((b.min(e) - a.max(s)) as usize).collect();
                out.push(Inline::Text(part, r.clone()));
            }
            other => out.push(other.clone()),
        }
    }
    out
}

/// What inlines hold, formatting aside: their kinds in order, texts joined.
fn content(v: &[Inline]) -> Vec<(Discriminant<Inline>, String)> {
    let mut out: Vec<(Discriminant<Inline>, String)> = Vec::new();
    for i in v {
        let d = std::mem::discriminant(i);
        match (i, out.last_mut()) {
            (Inline::Text(t, _), _) if t.is_empty() => {}
            (Inline::Text(t, _), Some((ld, s))) if *ld == d => s.push_str(t),
            (Inline::Text(t, _), _) => out.push((d, t.clone())),
            _ => out.push((d, String::new())),
        }
    }
    out
}

fn block_id(b: &Block) -> u64 {
    match b {
        Block::Para(p) => p.id,
        Block::Table(t) => t.id,
    }
}

/// The paragraphs a row holds, nested ones too.
fn row_paras(r: &Row) -> HashSet<u64> {
    fn walk(blocks: &[Block], out: &mut HashSet<u64>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    out.insert(p.id);
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = HashSet::new();
    r.cells.iter().for_each(|c| walk(&c.blocks, &mut out));
    out
}

fn note(doc: &Document, kind: NoteKind, id: i64) -> Option<&Vec<Block>> {
    match kind {
        NoteKind::Foot => doc.footnotes.get(&id),
        NoteKind::End => doc.endnotes.get(&id),
    }
}

#[cfg(test)]
mod tests {
    use crate::docx::Block;
    use crate::editing::Editor;
    use crate::editing::commands::{Command, Dir, Outcome};
    use crate::editing::position::{Position, StoryId};
    use crate::editing::selection::{Granularity, Selection};
    use crate::editing::tests::{blank_with, fonts, open, para_id, texts};
    use crate::layout::Unit;

    fn tracked(bytes: &[u8]) -> Editor {
        let mut e = Editor::open(bytes, fonts()).expect("opens");
        e.set_author("Author A", None);
        assert!(e.set_tracking(true));
        e
    }

    fn select(e: &mut Editor, a: u32, b: u32) {
        let p = para_id(e, 0);
        let at = |u| Position::text(StoryId::Body, p, Unit(u));
        e.set_caret_for_benchmark(if a == b { Selection::Caret(at(a)) } else { Selection::range(at(a), at(b), Granularity::Char) });
    }

    fn body(e: &Editor) -> Vec<Block> {
        e.document().sections.iter().flat_map(|s| s.blocks.iter().cloned()).collect()
    }

    /// How many inlines carry a revision.
    fn revised(e: &Editor) -> usize {
        let paras = body(e).into_iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
        paras.flat_map(|p| p.inlines).filter(|i| i.run().is_some_and(|r| !r.rev.is_empty())).count()
    }

    #[test]
    fn undo_and_redo_restore_a_tracked_edit_whatever_tracking_is_now() {
        let mut e = tracked(&blank_with("Alpha beta gamma."));
        let before = body(&e);
        select(&mut e, 6, 6);
        e.execute(Command::InsertText("new ".into()), fonts());
        select(&mut e, 10, 14);
        e.execute(Command::Delete { dir: Dir::Back, word: false }, fonts());
        let after = body(&e);
        assert_eq!(texts(&e), ["Alpha new beta gamma."], "the deletion stays");
        assert!(e.set_tracking(false));
        e.execute(Command::Undo, fonts());
        e.execute(Command::Undo, fonts());
        assert_eq!(body(&e), before);
        e.execute(Command::Redo, fonts());
        e.execute(Command::Redo, fonts());
        assert_eq!(body(&e), after);
    }

    #[test]
    fn the_authors_insertion_from_an_earlier_session_is_removed_outright() {
        let mut e = tracked(&blank_with("Alpha beta."));
        select(&mut e, 6, 6);
        e.execute(Command::InsertText("new ".into()), fonts());
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let mut e = tracked(&saved);
        assert_eq!(revised(&e), 1, "the insertion is read back");
        select(&mut e, 6, 10);
        e.execute(Command::Delete { dir: Dir::Forward, word: false }, fonts());
        assert_eq!((texts(&e), revised(&e)), (vec!["Alpha beta.".to_string()], 0));
    }

    #[test]
    fn a_recorded_page_change_saves_with_the_section_before_it() {
        let mut e = tracked(&blank_with("Alpha beta."));
        let before = e.document().sections[0].page();
        e.execute(Command::from_host("layout.orientation.landscape", None).expect("a command"), fonts());
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let read = crate::docx::read(&saved).expect("reads back");
        let c = read.sections[0].change.as_ref().expect("the change is saved");
        assert_eq!((&c.previous, read.sections[0].page_w > read.sections[0].page_h), (&before, true));
        let earlier = c.previous_raw.0.as_deref().expect("the earlier section");
        assert!(earlier.contains("cols") && !earlier.contains("headerReference"), "the section as read, its page earlier: {earlier}");
        // Rejected after reopening: the page before it, and no change saved.
        let mut e = tracked(&saved);
        e.execute(Command::from_host("review.reject", Some(&serde_json::json!("all"))).expect("a command"), fonts());
        assert!(e.document().sections[0].change.is_none() && e.document().sections[0].page() == before);
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let read = crate::docx::read(&saved).expect("reads back");
        assert!(read.sections[0].change.is_none() && read.sections[0].page() == before);
    }

    #[test]
    fn tracking_stays_on_under_tracked_changes_protection() {
        let mut e = open("review/FX-REVIEW-PROTECTED.docx");
        assert!(e.tracking());
        assert!(!e.set_tracking(false));
        assert_eq!(e.execute(Command::Track(Some(false)), fonts()), Outcome::Unchanged("tracking locked"));
        assert!(e.tracking());
    }
}
