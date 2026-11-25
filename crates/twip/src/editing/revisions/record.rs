//! Revision recording

use std::collections::{HashMap, HashSet};
use std::mem::Discriminant;
use std::ops::Range;
use std::rc::Rc;

use crate::docx::review::{
    AuthorId, CellMarkKind, FormatChange, GridChange, IdCounter, MarkKind, ParaChange, PreviousProps, RevisionId, RevisionMark, RunRevisions,
    SectionChange, TableChange, Timestamp,
};
use crate::docx::{Block, Cell, Document, Inline, NoteKind, ParaStyle, Paragraph, Raw, Row, Run, RunStyle, Section, SectionPage};
use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::super::index::DocIndex;
use super::super::step::{Edges, Step, para_len};

// Transactions the recorder tracks by their label: the producers begin them with these.
pub(in crate::editing) const MERGE_CELLS: &str = "Merge Cells";
pub(in crate::editing) const SPLIT_CELLS: &str = "Split Cells";
pub(in crate::editing) const INSERT_COLUMNS: &str = "Insert Columns";
pub(in crate::editing) const DELETE_COLUMNS: &str = "Delete Columns";
pub(in crate::editing) const INSERT_CELLS: &str = "Insert Cells";
pub(in crate::editing) const DRAW_TABLE: &str = "Draw Table";

const UNTRACKED_TABLE_EDITS: [&str; 3] = [MERGE_CELLS, SPLIT_CELLS, DRAW_TABLE];

// Edits whose cells are tracked as inserted or deleted cells: accepting them gives the untracked edit. Word tracks
// Delete Cells as a change of the table's properties.
const CELL_EDITS: [&str; 3] = [INSERT_COLUMNS, DELETE_COLUMNS, INSERT_CELLS];

// Edits whose rows stay in place, matched by position: content moving between them is tracked as moved out and in.
const ROWS_IN_PLACE: [&str; 1] = [INSERT_CELLS];

pub struct Recording {
    pub author: AuthorId,
    pub time: Timestamp,
    pub move_name: Option<Rc<str>>,
    /// Formatting changes are recorded (Track formatting): without it they apply untracked.
    pub track_formatting: bool,
    label: &'static str,
    /// The step's marks: each step's edits are a revision of their own.
    ins: Option<Rc<RevisionMark>>,
    del: Option<Rc<RevisionMark>>,
    fmt: Option<Rc<RevisionMark>>,
    /// Every insertion and deletion mark the transaction made, the insertions also by address.
    inserts: Vec<Rc<RevisionMark>>,
    deletes: Vec<Rc<RevisionMark>>,
    made: HashSet<*const RevisionMark>,
    inserted: HashMap<*const RunStyle, (Run, Run)>,
    deleted: HashMap<*const RunStyle, (Run, Run)>,
    formatted: HashMap<(*const RunStyle, *const RunStyle, bool), (Run, Run, Run)>,
    /// The floats this transaction took out of a paragraph and kept there marked deleted, by float id.
    gone_floats: HashMap<u64, ParaId>,
    /// The floats this transaction put in a paragraph while still in another: moving there.
    arriving: HashSet<u64>,
}

impl Recording {
    pub fn new(author: AuthorId, time: Timestamp, move_name: Option<Rc<str>>, label: &'static str) -> Recording {
        Recording {
            author,
            time,
            move_name,
            track_formatting: true,
            label,
            ins: None,
            del: None,
            fmt: None,
            inserts: Vec::new(),
            deletes: Vec::new(),
            made: HashSet::new(),
            inserted: HashMap::new(),
            deleted: HashMap::new(),
            formatted: HashMap::new(),
            gone_floats: HashMap::new(),
            arriving: HashSet::new(),
        }
    }

    /// The insertion and deletion marks the transaction made.
    pub fn marks(&self) -> (Vec<Rc<RevisionMark>>, Vec<Rc<RevisionMark>>) {
        (self.inserts.clone(), self.deletes.clone())
    }

    pub fn record(&mut self, step: Step, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Vec<Step> {
        (self.ins, self.del, self.fmt) = (None, None, None);
        self.inserted.clear();
        self.deleted.clear();
        self.formatted.clear();
        match step {
            Step::Text { para, range, with, edges } => self.text(para, range, with, edges, doc, idx, ids),
            Step::Restyle { para, inlines } => self.restyle(para, inlines, doc, idx, ids),
            Step::Split { para, at, second, kept } => self.split(para, at, second, kept, doc, idx, ids),
            Step::Join { first, second } => self.join(first, second, doc, idx, ids),
            Step::Para { para, style, mark, list } => match idx.para(doc, para) {
                Some(p) => {
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
            other => vec![other],
        }
    }

    // Marks and runs

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
        self.made.insert(Rc::as_ptr(&m));
        self.inserts.push(m.clone());
        m
    }

    fn del_mark(&mut self, ids: &mut IdCounter) -> Rc<RevisionMark> {
        if let Some(m) = &self.del {
            return m.clone();
        }
        let m = self.mark(ids, if self.move_name.is_some() { MarkKind::MoveFrom } else { MarkKind::Delete });
        self.del = Some(m.clone());
        self.deletes.push(m.clone());
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

    fn made_here(&self, m: &Option<Rc<RevisionMark>>) -> bool {
        m.as_ref().is_some_and(|m| self.made.contains(&Rc::as_ptr(m)))
    }

    fn new_here_row(&self, r: &Row) -> bool {
        self.made_here(&r.rev.ins)
    }

    fn own_insertion(&self, r: &RunStyle) -> bool {
        r.rev.del.is_none() && r.rev.ins.as_ref().is_some_and(|m| m.author == self.author && m.kind == MarkKind::Insert)
    }

    fn new_here(&self, r: &RunStyle) -> bool {
        self.made_here(&r.rev.ins)
    }

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

    fn deleted(&mut self, r: &Run, ids: &mut IdCounter) -> Option<Run> {
        let kind = if self.move_name.is_some() { MarkKind::MoveFrom } else { MarkKind::Delete };
        // Deleted by this author, or a move end
        if r.rev
            .del
            .as_ref()
            .is_some_and(|m| (m.author == self.author && (m.kind == kind || m.kind == MarkKind::Delete)) || m.kind == MarkKind::MoveFrom)
        {
            return None;
        }
        if let Some((_, n)) = self.deleted.get(&Rc::as_ptr(r)) {
            return Some(n.clone());
        }
        let n = with_rev(r, RunRevisions { del: Some(self.del_mark(ids)), ..r.rev.clone() });
        self.deleted.insert(Rc::as_ptr(r), (r.clone(), n.clone()));
        Some(n)
    }

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
        } else if !record || !self.track_formatting || same_look(old, new) {
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

    fn para_change(&mut self, old: &Rc<ParaStyle>, new: Rc<ParaStyle>, ids: &mut IdCounter) -> Rc<ParaStyle> {
        if Rc::ptr_eq(old, &new) {
            return new;
        }
        let change = if !self.track_formatting || same_para(old, &new) {
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

    fn section_change(&mut self, s: &Section, page: &SectionPage, ids: &mut IdCounter) -> Option<Rc<SectionChange>> {
        if *page == s.page() || !self.track_formatting {
            return s.change.clone();
        }
        let (previous, previous_raw) = match &s.change {
            Some(c) => (c.previous.clone(), c.previous_raw.clone()),
            None => (s.page(), Raw::default()),
        };
        (previous != *page).then(|| Rc::new(SectionChange { mark: self.fmt_mark(ids), previous, previous_raw }))
    }

    // Inlines and blocks

    /// Field edge `o` deleted with the whole field a tracked deletion takes: `None` when it is the author's own
    /// insertion, which goes.
    pub(in crate::editing) fn delete_edge(&mut self, o: &Rc<crate::docx::Opaque>, ids: &mut IdCounter) -> Option<Inline> {
        self.del_inline(&Inline::Opaque(o.clone()), true, ids, &mut Vec::new())
    }

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
                if self.track_formatting && t.change.is_none() {
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
                if self.track_formatting && n.change.is_none() {
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
            c.blocks = c.blocks.iter().filter_map(|b| self.del_block(b, false, ids, notes)).collect();
        }
        Some(n)
    }

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

    // Steps

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
            let with = with.into_iter().map(|i| self.ins_inline(i, ids)).collect();
            return vec![Step::Text { para, range, with, edges }];
        };
        let (a, b) = (range.start.0, range.end.0);
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
            // A field's edges go only with the whole field ([`Recording::delete_edge`]).
            let Some(r) = run(i).filter(|_| inside && !matches!(i, Inline::Opaque(_))) else {
                push(&mut inlines, i.clone());
                continue;
            };
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
        for g in gone.iter().rev() {
            out.push(Step::Text { para, range: Unit(g.start)..Unit(g.end), with: Vec::new(), edges: Edges::default() });
        }
        if !with.is_empty() {
            let end = Unit(b - gone.iter().map(|g| g.end - g.start).sum::<u32>());
            let with = with.into_iter().map(|i| self.ins_inline(i, ids)).collect();
            out.push(Step::Text { para, range: end..end, with, edges: Edges::default() });
        }
        out.extend(self.note_steps(notes, doc, ids));
        out
    }

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
        let float_id = |i: &Inline| if let Inline::Float(f, _) = i { Some(f.id) } else { None };
        let had: HashSet<u64> = p.inlines.iter().filter_map(float_id).collect();
        let has: HashSet<u64> = inlines.iter().filter_map(float_id).collect();
        if had != has {
            return self.floats_changed(para, p, inlines, &had, &has, doc, idx, ids);
        }
        if content(&p.inlines) == content(&inlines) {
            let inlines = self.formatted(&p.inlines, inlines, false, ids);
            return vec![Step::Restyle { para, inlines }];
        }
        let control = |i: &Inline| matches!(i, Inline::ControlStart(_) | Inline::ControlEnd(_));
        let kept: Vec<&Inline> = p.inlines.iter().filter(|i| !control(i) || inlines.contains(i)).collect();
        if kept.len() < p.inlines.len() && kept.len() == inlines.len() && kept.iter().zip(&inlines).all(|(a, b)| *a == b) {
            return Vec::new();
        }
        vec![Step::Restyle { para, inlines }]
    }

    /// A restyle that puts floats in or takes them out: one that is new is inserted, one taken out is kept marked
    /// deleted (unless it is the author's own insertion), and one moving between paragraphs (taken out of one and put
    /// in another in either order) is neither, its deleted copy removed.
    #[allow(clippy::too_many_arguments, reason = "the step's fields, then the document")]
    fn floats_changed(
        &mut self,
        para: ParaId,
        p: &Paragraph,
        inlines: Vec<Inline>,
        had: &HashSet<u64>,
        has: &HashSet<u64>,
        doc: &Document,
        idx: &DocIndex,
        ids: &mut IdCounter,
    ) -> Vec<Step> {
        let mut out = Vec::new();
        let mut moved: HashSet<u64> = HashSet::new();
        for f in has.difference(had) {
            if let Some(from) = self.gone_floats.remove(f) {
                moved.insert(*f);
                if let Some(q) = idx.para(doc, from) {
                    let left: Vec<Inline> = q.inlines.iter().filter(|i| !matches!(i, Inline::Float(x, _) if x.id == *f)).cloned().collect();
                    out.push(Step::Restyle { para: from, inlines: left });
                }
            } else if idx.float_anchor(FloatId(*f)).is_some_and(|a| a != para) {
                moved.insert(*f);
                self.arriving.insert(*f);
            }
        }
        let float_id = |i: &Inline| if let Inline::Float(f, _) = i { Some(f.id) } else { None };
        let mut with: Vec<Inline> = inlines
            .into_iter()
            .map(|i| match float_id(&i) {
                Some(f) if !had.contains(&f) && !moved.contains(&f) => self.ins_inline(i, ids),
                _ => i,
            })
            .collect();
        for (k, i) in p.inlines.iter().enumerate() {
            if let Inline::Float(f, r) = i
                && !has.contains(&f.id)
                && !self.own_insertion(r)
                && !self.arriving.remove(&f.id)
            {
                let d = self.deleted(r, ids).unwrap_or_else(|| r.clone());
                with.insert(k.min(with.len()), Inline::Float(f.clone(), d));
                self.gone_floats.insert(f.id, para);
            }
        }
        out.push(Step::Restyle { para, inlines: with });
        out
    }

    #[allow(clippy::too_many_arguments, reason = "the step's fields, then the document")]
    fn split(
        &mut self,
        para: ParaId,
        at: Unit,
        mut second: Paragraph,
        kept: Vec<Inline>,
        doc: &Document,
        idx: &DocIndex,
        ids: &mut IdCounter,
    ) -> Vec<Step> {
        let Some(p) = idx.para(doc, para).filter(|_| second.inlines.is_empty() && kept.is_empty()) else {
            return vec![Step::Split { para, at, second, kept }];
        };
        if second.mark.rev != p.mark.rev {
            second.mark = with_rev(&second.mark, p.mark.rev.clone());
        }
        let mark = with_rev(&p.mark, RunRevisions { ins: Some(self.ins_mark(ids)), del: None, fmt: None });
        vec![Step::Split { para, at, second, kept }, Step::Para { para, style: p.style.clone(), mark, list: p.list.clone() }]
    }

    fn join(&mut self, first: ParaId, second: ParaId, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Vec<Step> {
        let (Some(a), Some(b)) = (idx.para(doc, first), idx.para(doc, second)) else { return vec![Step::Join { first, second }] };
        let adjacent =
            matches!((idx.para_slot(first), idx.para_slot(second)), (Some(x), Some(y)) if x.container == y.container && y.block == x.block + 1);
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
            // Copies of blocks still indexed where they stay deleted (a removed frame's paragraphs)
            let with = with.into_iter().map(|b| if indexed(idx, &b) { super::super::clipboard::fresh(&b) } else { b }).collect();
            let with = self.ins_blocks(with, ids);
            return vec![Step::Blocks { container, range, with }];
        };
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
        let in_place = ROWS_IN_PLACE.contains(&self.label);
        // Rows added in place take content moved from rows that keep it, marked deleted: theirs is a copy.
        let added = |n: &Row| {
            let mut r = n.clone();
            if in_place {
                r.cells.iter_mut().for_each(|c| c.blocks = c.blocks.iter().map(super::super::clipboard::fresh).collect());
            }
            r
        };
        let matched: Vec<Option<usize>> = if in_place {
            (0..with.len()).map(|i| (i < old.len()).then_some(i)).collect()
        } else {
            let keys: Vec<HashSet<u64>> = old.iter().map(row_paras).collect();
            with.iter()
                .map(|r| {
                    let k = row_paras(r);
                    keys.iter().position(|o| !o.is_disjoint(&k))
                })
                .collect()
        };
        let order: Vec<usize> = matched.iter().flatten().copied().collect();
        if order.windows(2).any(|w| w[0] >= w[1]) {
            return vec![Step::Rows { table, range, with, grid }];
        }
        // Cells the kept rows lose or gain: lost cells stay, marked deleted, over the grid as it was.
        let kept: Vec<Option<usize>> = (0..old.len()).map(|i| matched.iter().position(|m| *m == Some(i))).collect();
        let cell_edit = CELL_EDITS.contains(&self.label);
        let pairs: Vec<Option<Vec<CellPair>>> = kept
            .iter()
            .zip(old)
            .map(|(k, o)| k.map(|k| &with[k]).filter(|n| cell_edit && n.cells.len() != o.cells.len()).and_then(|n| cell_pairs(o, n)))
            .collect();
        let gained = pairs.iter().flatten().flatten().any(|p| p.0.is_none());
        let lost =
            pairs.iter().zip(old).any(|(p, o)| p.iter().flatten().any(|&(i, k)| k.is_none() && i.is_some_and(|i| !self.own_cell(&o.cells[i]))));
        let retain = lost && !gained;
        let mut grid = if retain { None } else { grid };
        // Cells this author inserted that a retained row loses: they go outright, with their columns.
        let mut own: Vec<(usize, usize)> = Vec::new();
        let mut notes = Vec::new();
        let mut rows = Vec::with_capacity(with.len().max(old.len()));
        let mut changed = grid.as_ref().is_some_and(|g| *g != t.grid);
        // ponytail: a column edit isn't recorded (its cells go untracked), so a row it empties goes with it; cell marks
        // would keep both until accepted.
        let columns_go = grid.as_ref().is_some_and(|g| g.len() < t.grid.len());
        let mut next = 0;
        for (i, o) in old.iter().enumerate() {
            match kept[i] {
                Some(k) => {
                    for n in &with[next..k] {
                        rows.push(self.ins_row(added(n), ids));
                        changed = true;
                    }
                    let mut lost_own = Vec::new();
                    let (r, c) = self.kept_row(o, &with[k], pairs[i].as_deref(), retain, ids, &mut notes, &mut lost_own);
                    changed |= c;
                    own.extend(lost_own.into_iter().map(|k| (rows.len(), k)));
                    rows.push(r);
                    next = k + 1;
                }
                None if columns_go => changed = true,
                None => {
                    changed = true;
                    if let Some(r) = self.del_row(o, true, ids, &mut notes) {
                        rows.push(r);
                    }
                }
            }
        }
        for n in &with[next..] {
            rows.push(self.ins_row(added(n), ids));
            changed = true;
        }
        // Rejecting inserted cells drops their columns again.
        let grid_changed = !gained && grid.as_ref().is_some_and(|g| *g != t.grid);
        if !own.is_empty() {
            let mut g = t.grid.clone();
            if crate::editing::tables::remove_cells(&mut rows, &own, &mut g) {
                grid = Some(g);
            }
        }
        // A note whose reference moves stays: only those no reference is left to go with their references.
        notes.retain(|&(kind, id)| !rows.iter().flat_map(|r| &r.cells).any(|c| live_note_ref(&c.blocks, kind, id, 0)));
        let mut out: Vec<Step> = if changed { self.table_change(table, grid_changed, doc, idx, ids).into_iter().collect() } else { Vec::new() };
        out.push(Step::Rows { table, range, with: rows, grid });
        out.extend(self.note_steps(notes, doc, ids));
        out
    }

    /// `n` kept in place of `o`, with `pairs` matching their cells when the count changes. With `retain`, the row
    /// keeps `o`'s layout over the grid as it was, its lost cells marked deleted; those this author inserted stay as
    /// they are, their places in the row added to `own`, for the caller to remove.
    #[allow(clippy::too_many_arguments, reason = "the rows, how their cells match, then the recording state")]
    fn kept_row(
        &mut self,
        o: &Row,
        n: &Row,
        pairs: Option<&[CellPair]>,
        retain: bool,
        ids: &mut IdCounter,
        notes: &mut Vec<(NoteKind, i64)>,
        own: &mut Vec<usize>,
    ) -> (Row, bool) {
        let mut r = n.clone();
        if retain {
            (r.grid_before, r.grid_after, r.w_before, r.w_after) = (o.grid_before, o.grid_after, o.w_before, o.w_after);
        }
        let props = (o.height, o.cant_split, o.header) != (r.height, r.cant_split, r.header);
        if props && self.track_formatting && r.rev.change.is_none() && !self.new_here_row(o) {
            let previous = PreviousProps::Row { height: o.height, cant_split: o.cant_split, header: o.header };
            r.rev.change = Some(Rc::new(TableChange { mark: self.fmt_mark(ids), previous_raw: o.direct.clone(), previous: Some(previous) }));
        }
        let skips = (o.grid_before, o.grid_after, o.w_before, o.w_after) != (r.grid_before, r.grid_after, r.w_before, r.w_after);
        if o.cells.len() != n.cells.len() {
            let Some(pairs) = pairs else { return (r, true) };
            let mut cells = Vec::with_capacity(pairs.len());
            for &pair in pairs {
                match pair {
                    (Some(i), Some(k)) => {
                        let mut c = n.cells[k].clone();
                        if retain {
                            (c.span, c.width) = (o.cells[i].span, o.cells[i].width);
                        }
                        if c.blocks.iter().map(block_id).ne(o.cells[i].blocks.iter().map(block_id)) {
                            c.blocks = self.moved_in(&o.cells[i].blocks, &c.blocks, ids, notes);
                        }
                        cells.push(c);
                    }
                    (Some(i), None) if retain && self.own_cell(&o.cells[i]) => {
                        own.push(cells.len());
                        cells.push(o.cells[i].clone());
                    }
                    (Some(i), None) if retain => cells.push(self.cell_mark(o.cells[i].clone(), CellMarkKind::Delete, ids)),
                    (None, Some(k)) => cells.push(self.cell_mark(n.cells[k].clone(), CellMarkKind::Insert, ids)),
                    _ => {}
                }
            }
            r.cells = cells;
            return (r, true);
        }
        let mut changed = skips;
        let fresh = self.new_here_row(o);
        for (c, oc) in r.cells.iter_mut().zip(&o.cells) {
            if retain {
                (c.span, c.width) = (oc.span, oc.width);
            }
            let looks =
                |x: &crate::docx::Cell| (x.width, x.top.clone(), x.left.clone(), x.bottom.clone(), x.right.clone(), x.shading, x.margins, x.no_wrap);
            let restyled = looks(c) != looks(oc) || (&c.v_align, &c.text_direction) != (&oc.v_align, &oc.text_direction);
            if restyled && self.track_formatting && c.rev.change.is_none() && !fresh {
                let b = [oc.top.clone(), oc.left.clone(), oc.bottom.clone(), oc.right.clone()];
                let previous = PreviousProps::Cell {
                    width: oc.width,
                    borders: b,
                    shading: oc.shading,
                    margins: oc.margins,
                    v_align: oc.v_align,
                    text_direction: oc.text_direction.clone(),
                    no_wrap: oc.no_wrap,
                };
                c.rev.change = Some(Rc::new(TableChange { mark: self.fmt_mark(ids), previous_raw: oc.direct.clone(), previous: Some(previous) }));
            }
            if c.v_merge != oc.v_merge && self.track_formatting && c.rev.structural.is_none() && !fresh {
                let kind = CellMarkKind::Merge { v_merge: Some(c.v_merge), v_merge_orig: Some(oc.v_merge) };
                c.rev.structural = Some((self.fmt_mark(ids), kind));
            }
            changed |= c.span != oc.span;
            if c.blocks.iter().map(block_id).ne(oc.blocks.iter().map(block_id)) {
                changed = true;
                c.blocks = self.moved_in(&oc.blocks, &c.blocks, ids, notes);
            }
        }
        (r, changed)
    }

    /// `c` marked as an inserted or deleted cell, its content as it is (as Word marks cells).
    fn cell_mark(&mut self, mut c: Cell, kind: CellMarkKind, ids: &mut IdCounter) -> Cell {
        let mark = self.mark(ids, if kind == CellMarkKind::Insert { MarkKind::Insert } else { MarkKind::Delete });
        c.rev.structural = Some((mark, kind));
        c
    }

    /// `c` is a cell this author inserted, so deleting it removes it outright.
    fn own_cell(&self, c: &Cell) -> bool {
        c.rev.structural.as_ref().is_some_and(|(m, k)| *k == CellMarkKind::Insert && m.author == self.author)
    }

    fn moved_in(&mut self, old: &[Block], new: &[Block], ids: &mut IdCounter, notes: &mut Vec<(NoteKind, i64)>) -> Vec<Block> {
        let mut kept: Vec<Block> = old.iter().filter_map(|b| self.del_block(b, false, ids, notes)).collect();
        for (k, o) in kept.iter_mut().zip(old) {
            if let (Block::Para(k), Block::Para(o)) = (k, o) {
                k.mark = o.mark.clone();
            }
        }
        match (new, kept.first_mut()) {
            ([Block::Para(p)], Some(Block::Para(first))) => {
                // The paragraph takes the incoming one's properties, as a tracked change of them.
                first.style = self.para_change(&first.style, p.style.clone(), ids);
                first.list.clone_from(&p.list);
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

    fn table_change(&mut self, table: TableId, grid: bool, doc: &Document, idx: &DocIndex, ids: &mut IdCounter) -> Option<Step> {
        let slot = idx.table_slot(table)?;
        let Block::Table(t) = idx.blocks(doc, slot.container)?.get(slot.block)? else { return None };
        let (change, grid) = (t.change.is_none(), grid && t.grid_change.is_none());
        if !change && !grid || !self.track_formatting {
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

/// Whether `blocks` hold a reference to note `id` that isn't deleted.
fn live_note_ref(blocks: &[Block], kind: NoteKind, id: i64, depth: usize) -> bool {
    // Deeper nesting than a document can be read with holds nothing.
    if depth > crate::docx::xml::MAX_DEPTH {
        return false;
    }
    blocks.iter().any(|b| match b {
        Block::Para(p) => {
            p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { kind: k, id: n, run, .. } if *k == kind && *n == id && run.rev.del.is_none()))
        }
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| live_note_ref(&c.blocks, kind, id, depth + 1)),
    })
}

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

/// The run whose tracked changes an inline carries: a field edge's too.
fn run(i: &Inline) -> Option<&Run> {
    match i {
        Inline::CommentRef { .. } | Inline::Separator { .. } => None,
        Inline::Opaque(o) => o.field.as_ref().and(o.run.as_ref()),
        _ => i.run(),
    }
}

fn with_run(i: &Inline, r: Run) -> Inline {
    super::super::format::with_run(i, &r)
}

fn with_rev(r: &Run, rev: RunRevisions) -> Run {
    Rc::new(RunStyle { rev, ..(**r).clone() })
}

fn same_look(a: &RunStyle, b: &RunStyle) -> bool {
    RunStyle { rev: RunRevisions::default(), ..a.clone() } == RunStyle { rev: RunRevisions::default(), ..b.clone() }
}

fn same_para(a: &ParaStyle, b: &ParaStyle) -> bool {
    ParaStyle { change: None, ..a.clone() } == ParaStyle { change: None, ..b.clone() }
}

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

fn indexed(idx: &DocIndex, b: &Block) -> bool {
    match b {
        Block::Para(p) => idx.para_slot(ParaId(p.id)).is_some(),
        Block::Table(t) => idx.table_slot(TableId(t.id)).is_some(),
    }
}

fn block_id(b: &Block) -> u64 {
    match b {
        Block::Para(p) => p.id,
        Block::Table(t) => t.id,
    }
}

fn walk_paras(blocks: &[Block], out: &mut impl FnMut(u64)) {
    for b in blocks {
        match b {
            Block::Para(p) => out(p.id),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk_paras(&c.blocks, out)),
        }
    }
}

fn row_paras(r: &Row) -> HashSet<u64> {
    let mut out = HashSet::new();
    r.cells.iter().for_each(|c| {
        walk_paras(&c.blocks, &mut |id| {
            out.insert(id);
        })
    });
    out
}

/// A cell of the old row and the new row's cell that keeps it; one side alone for a cell lost or gained.
type CellPair = (Option<usize>, Option<usize>);

/// The cells of `o` and `n` in order, matched by their paragraphs; `None` when the matches cross.
fn cell_pairs(o: &Row, n: &Row) -> Option<Vec<CellPair>> {
    let mut at: HashMap<u64, usize> = HashMap::new();
    for (i, c) in o.cells.iter().enumerate() {
        walk_paras(&c.blocks, &mut |id| {
            at.insert(id, i);
        });
    }
    let mut out = Vec::with_capacity(o.cells.len().max(n.cells.len()));
    let mut next = 0;
    for (k, c) in n.cells.iter().enumerate() {
        let mut found = None;
        walk_paras(&c.blocks, &mut |id| {
            found = found.or_else(|| at.get(&id).copied());
        });
        match found {
            Some(i) if i >= next => {
                out.extend((next..i).map(|j| (Some(j), None)));
                out.push((Some(i), Some(k)));
                next = i + 1;
            }
            Some(_) => return None,
            None => out.push((None, Some(k))),
        }
    }
    out.extend((next..o.cells.len()).map(|j| (Some(j), None)));
    Some(out)
}

fn note(doc: &Document, kind: NoteKind, id: i64) -> Option<&Vec<Block>> {
    match kind {
        NoteKind::Foot => doc.footnotes.get(&id),
        NoteKind::End => doc.endnotes.get(&id),
    }
}

#[cfg(test)]
mod tests {
    use crate::api::value::{Alignment, ObjectWrap};
    use crate::docx::{Block, Inline, Paragraph, Table, VMerge};
    use crate::editing::Editor;
    use crate::editing::commands::{Command, Dir, Outcome};
    use crate::editing::position::{Position, StoryId};
    use crate::editing::revisions::index::revisions;
    use crate::editing::selection::{Granularity, ObjectRef, Selection};
    use crate::editing::tests::{blank_body, blank_with, fonts, open, para_id, texts};
    use crate::layout::{FloatId, ParaId, Unit};

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
        e.execute(crate::editing::tests::del(Dir::Back, false), fonts());
        let after = body(&e);
        assert_eq!(texts(&e), ["Alpha new beta gamma."], "the deletion stays");
        assert!(e.set_tracking(false));
        e.execute(Command::Undo(Default::default()), fonts());
        e.execute(Command::Undo(Default::default()), fonts());
        assert_eq!(body(&e), before);
        e.execute(Command::Redo(Default::default()), fonts());
        e.execute(Command::Redo(Default::default()), fonts());
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
        e.execute(crate::editing::tests::del(Dir::Forward, false), fonts());
        assert_eq!((texts(&e), revised(&e)), (vec!["Alpha beta.".to_string()], 0));
    }

    #[test]
    fn a_recorded_page_change_saves_with_the_section_before_it() {
        let mut e = tracked(&blank_with("Alpha beta."));
        let before = e.document().sections[0].page();
        e.execute(Command::SetOrientation(crate::api::command::SectionValue::here(crate::api::value::Orientation::Landscape)), fonts());
        let saved = crate::docx::write::save(e.document()).expect("saves");
        let read = crate::docx::read(&saved).expect("reads back");
        let c = read.sections[0].change.as_ref().expect("the change is saved");
        assert_eq!((&c.previous, read.sections[0].page_w > read.sections[0].page_h), (&before, true));
        let earlier = c.previous_raw.0.as_deref().expect("the earlier section");
        assert!(earlier.contains("cols") && !earlier.contains("headerReference"), "the section as read, its page earlier: {earlier}");
        let mut e = tracked(&saved);
        e.execute(Command::RejectAllRevisions(Default::default()), fonts());
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
        assert!(matches!(e.execute(Command::SetTrackChanges(false), fonts()), Outcome::Refused(err) if err.message == "tracking locked"));
        assert!(e.tracking());
    }

    fn paras(blocks: &[Block], out: &mut Vec<Paragraph>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push(p.clone()),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| paras(&c.blocks, out)),
            }
        }
    }

    fn all_paras(e: &Editor) -> Vec<Paragraph> {
        let mut out = Vec::new();
        e.document().sections.iter().for_each(|s| paras(&s.blocks, &mut out));
        out
    }

    fn kinds(e: &Editor) -> Vec<&'static str> {
        revisions(e.document(), e.index()).iter().map(|r| r.kind.name()).collect()
    }

    /// The runs of the first paragraph marked inserted, and those marked deleted.
    fn marked(e: &Editor) -> (usize, usize) {
        let p = &all_paras(e)[0];
        let runs = || p.inlines.iter().filter_map(|i| i.run());
        (runs().filter(|r| r.rev.ins.is_some()).count(), runs().filter(|r| r.rev.del.is_some()).count())
    }

    fn inserted_and_deleted() -> Editor {
        let mut e = tracked(&blank_with("Alpha beta."));
        select(&mut e, 6, 6);
        e.execute(Command::InsertText("new ".into()), fonts());
        select(&mut e, 10, 14);
        e.execute(crate::editing::tests::del(Dir::Back, false), fonts());
        e
    }

    #[test]
    fn an_untracked_paragraph_format_keeps_the_paragraphs_revisions() {
        let mut e = inserted_and_deleted();
        let before = marked(&e);
        assert!(before.0 > 0 && before.1 > 0, "{before:?}");
        e.execute(Command::SetAlignment(Alignment::Right), fonts());
        assert!(all_paras(&e)[0].style.change.is_some(), "the tracked alignment is recorded");
        assert!(e.set_tracking(false));
        select(&mut e, 2, 2);
        assert!(matches!(e.execute(Command::SetAlignment(Alignment::Center), fonts()), Outcome::Changed(_)));
        assert_eq!(marked(&e), before, "the insertion and deletion stay");
        assert!(all_paras(&e)[0].style.change.is_some(), "the paragraph change stays");
    }

    #[test]
    fn untracked_bold_on_inserted_text_keeps_it_inserted() {
        let mut e = inserted_and_deleted();
        assert!(e.set_tracking(false));
        select(&mut e, 6, 9);
        e.execute(Command::SetBold(true), fonts());
        let p = &all_paras(&e)[0];
        let new = p.inlines.iter().find_map(|i| if let Inline::Text(t, r) = i { (t == "new").then_some(r) } else { None });
        assert!(new.is_some_and(|r| r.bold && r.rev.ins.is_some()), "{:?}", p.inlines);
    }

    #[test]
    fn an_untracked_join_keeps_the_joined_texts_revisions() {
        let mut e = tracked(&blank_with("Alpha beta.\nGamma."));
        select(&mut e, 6, 6);
        e.execute(Command::InsertText("new ".into()), fonts());
        assert!(e.set_tracking(false));
        let at = Position::text(StoryId::Body, para_id(&e, 1), Unit(0));
        e.set_caret_for_benchmark(Selection::Caret(at));
        e.execute(crate::editing::tests::del(Dir::Back, false), fonts());
        assert_eq!(texts(&e), ["Alpha new beta.Gamma."]);
        assert_eq!(marked(&e).0, 1, "the insertion stays");
    }

    #[test]
    fn editing_a_style_while_tracking_records_no_revision() {
        let mut e = open("FX-STYLES.docx");
        e.complete(fonts());
        let h1 = e.document().resolver.as_ref().expect("styles").styles.get("Heading1").expect("Heading 1").raw.clone();
        let bigger = h1.replace("<w:sz w:val=\"32\"/>", "<w:sz w:val=\"40\"/>").replace("<w:b/>", "<w:b/><w:color w:val=\"C00000\"/>");
        assert_ne!(bigger, h1, "the style changes");
        e.set_author("Author A", None);
        assert!(e.set_tracking(true));
        assert!(matches!(e.edit_styles("Modify Style", |s| s.set_xml(bigger), fonts()), Outcome::Changed(_)));
        assert_eq!(kinds(&e), Vec::<&str>::new());
    }

    #[test]
    fn a_tracked_paragraph_change_of_a_paragraph_without_properties_is_listed() {
        let mut e = open("FX-EMPTY.docx");
        e.execute(Command::InsertText("First para.".into()), fonts());
        e.set_author("Author A", None);
        assert!(e.set_tracking(true));
        e.execute(Command::SetAlignment(Alignment::Center), fonts());
        assert_eq!(kinds(&e), ["ParagraphProperty"]);
    }

    fn pictures_and_floats(e: &Editor) -> (usize, usize) {
        let all = all_paras(e);
        let count = |f: fn(&Inline) -> bool| all.iter().flat_map(|p| &p.inlines).filter(|i| f(i)).count();
        (count(|i| matches!(i, Inline::Picture(..))), count(|i| matches!(i, Inline::Float(..))))
    }

    #[test]
    fn rejecting_a_tracked_wrap_in_line_gives_back_the_floating_picture() {
        let mut e = open("FX-OBJ.docx");
        let id = all_paras(&e).iter().flat_map(|p| p.inlines.clone()).find_map(|i| if let Inline::Float(f, _) = i { Some(f.id) } else { None });
        let was = pictures_and_floats(&e);
        e.set_author("Author A", None);
        assert!(e.set_tracking(true));
        e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(FloatId(id.expect("a float")))));
        assert!(matches!(e.execute(Command::SetObjectWrap(ObjectWrap::Inline), fonts()), Outcome::Changed(_)));
        e.execute(Command::RejectAllRevisions(Default::default()), fonts());
        assert_eq!(pictures_and_floats(&e), was);
    }

    #[test]
    fn rejecting_a_tracked_wrap_square_gives_back_the_inline_picture() {
        let mut r = crate::editing::tests::scenarios::Run::open("FX-OBJ-LOCK.docx", "tracked-wrap-square");
        let was = pictures_and_floats(&r.e);
        r.e.set_author("Author A", None);
        assert!(r.e.set_tracking(true));
        r.step(&serde_json::json!({ "click": { "picture": 0 } }));
        assert!(matches!(r.e.execute(Command::SetObjectWrap(ObjectWrap::Square), fonts()), Outcome::Changed(_)));
        r.e.execute(Command::RejectAllRevisions(Default::default()), fonts());
        assert_eq!(pictures_and_floats(&r.e), was);
    }

    fn all_text(e: &Editor) -> String {
        fn go(blocks: &[Block], s: &mut String) {
            for p in blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }) {
                for i in &p.inlines {
                    match i {
                        Inline::Text(t, _) => s.push_str(t),
                        Inline::Float(f, _) => f.text_box.iter().for_each(|tb| go(&tb.blocks, s)),
                        _ => {}
                    }
                }
            }
        }
        let mut s = String::new();
        e.document().sections.iter().for_each(|sec| go(&sec.blocks, &mut s));
        s
    }

    #[test]
    fn rejecting_a_tracked_frame_removal_gives_back_the_frame() {
        let frame = r#"<w:p><w:pPr><w:framePr w:w="2400" w:h="600" w:hRule="exact" w:wrap="around" w:vAnchor="text" w:hAnchor="margin" w:x="2000" w:y="100"/></w:pPr><w:r><w:t>Framed text</w:t></w:r></w:p><w:p><w:r><w:t>Body text</w:t></w:r></w:p>"#;
        let mut e = tracked(&blank_body(frame));
        e.complete(fonts());
        let before = all_text(&e);
        let float = all_paras(&e).iter().flat_map(|p| p.inlines.clone()).find_map(|i| if let Inline::Float(f, _) = i { Some(f) } else { None });
        let f = float.expect("the frame is a float");
        let inner = f.text_box.as_ref().and_then(|t| t.blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p.id) } else { None }));
        let at = Position::text(StoryId::TextBox(FloatId(f.id)), ParaId(inner.expect("a paragraph in the frame")), Unit(0));
        e.set_caret_for_benchmark(Selection::Caret(at));
        assert!(matches!(e.execute(Command::RemoveFrame, fonts()), Outcome::Changed(_)));
        assert_eq!(pictures_and_floats(&e).1, 1, "the frame stays, marked deleted");
        e.execute(Command::RejectAllRevisions(Default::default()), fonts());
        assert_eq!((all_text(&e), pictures_and_floats(&e).1), (before, 1));
    }

    fn drop_cap(position: crate::api::command::DropCapPosition) -> Command {
        Command::SetDropCap(crate::api::command::DropCap { position, font: None, lines: None, distance: None })
    }

    #[test]
    fn rejecting_a_tracked_drop_cap_removal_gives_back_the_drop_cap() {
        use crate::api::command::DropCapPosition;
        let mut e = Editor::open(&blank_with("Once upon a time."), fonts()).expect("opens");
        select(&mut e, 2, 2);
        assert!(matches!(e.execute(drop_cap(DropCapPosition::Dropped), fonts()), Outcome::Changed(_)));
        let before = (all_text(&e), pictures_and_floats(&e).1);
        assert_eq!(before.1, 1, "the drop cap is a float");
        e.set_author("Author A", None);
        assert!(e.set_tracking(true));
        assert!(matches!(e.execute(drop_cap(DropCapPosition::None), fonts()), Outcome::Changed(_)));
        e.execute(Command::RejectAllRevisions(Default::default()), fonts());
        assert_eq!((all_text(&e), pictures_and_floats(&e).1), before);
    }

    fn table(rows: &[String]) -> Vec<u8> {
        let rows: String = rows.iter().map(|r| format!("<w:tr>{r}</w:tr>")).collect();
        blank_body(&format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>{rows}</w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#
        ))
    }

    fn tc(text: &str, pr: &str) -> String {
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{pr}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"#)
    }

    fn first_table(e: &Editor) -> Table {
        e.document().sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t.clone()) } else { None }).expect("a table")
    }

    #[test]
    fn rejecting_tracked_cell_shading_gives_back_the_cells_look() {
        let row: String = (0..3).map(|i| tc(&format!("c{i}"), "")).collect();
        let mut e = tracked(&table(&[row.clone(), row]));
        let was = first_table(&e).rows[0].cells[0].clone();
        select(&mut e, 0, 0);
        assert!(matches!(
            e.execute(Command::SetCellShading(crate::api::value::Color::parse("#D9D9D9").map(crate::api::drawing::ColourValue::rgb)), fonts()),
            Outcome::Changed(_)
        ));
        assert!(first_table(&e).rows[0].cells[0].shading.is_some());
        assert_eq!(kinds(&e), ["TableProperty"]);
        e.execute(Command::RejectAllRevisions(Default::default()), fonts());
        let now = &first_table(&e).rows[0].cells[0];
        assert_eq!((now.shading, &now.direct), (was.shading, &was.direct));
    }

    #[test]
    fn rejecting_a_tracked_row_deletion_gives_back_its_vertical_merge() {
        let r0 = tc("a", r#"<w:vMerge w:val="restart"/>"#) + &tc("b", "") + &tc("c", "");
        let r1 = tc("", "<w:vMerge/>") + &tc("e", "") + &tc("f", "");
        let r2: String = (0..3).map(|i| tc(&format!("g{i}"), "")).collect();
        let mut e = tracked(&table(&[r0, r1, r2]));
        select(&mut e, 0, 0);
        assert!(matches!(e.execute(Command::DeleteRows, fonts()), Outcome::Changed(_)));
        e.execute(Command::RejectAllRevisions(Default::default()), fonts());
        let merges: Vec<VMerge> = first_table(&e).rows.iter().map(|r| r.cells[0].v_merge).collect();
        assert_eq!(merges, [VMerge::Restart, VMerge::Continue, VMerge::None]);
    }

    #[test]
    fn removing_an_inline_control_while_tracking_keeps_its_text() {
        let sdt = r#"<w:p><w:r><w:t xml:space="preserve">Pre </w:t></w:r><w:sdt><w:sdtPr><w:id w:val="77"/><w:text/></w:sdtPr><w:sdtContent><w:r><w:t>abc</w:t></w:r></w:sdtContent></w:sdt><w:r><w:t xml:space="preserve"> post</w:t></w:r></w:p>"#;
        let mut e = tracked(&blank_body(sdt));
        let control = |e: &Editor| all_paras(e)[0].inlines.iter().find_map(|i| if let Inline::ControlStart(c) = i { Some(*c) } else { None });
        let id = crate::editing::merge::api_id(crate::docx::ElementId::Control(control(&e).expect("an inline control")));
        let remove = crate::api::command::RemoveControl { id, keep_content: Some(true) };
        assert!(matches!(e.execute(Command::RemoveControl(remove), fonts()), Outcome::Changed(_)));
        assert_eq!((control(&e), texts(&e)), (None, vec!["Pre abc post".to_string()]));
        assert_eq!(kinds(&e), Vec::<&str>::new());
    }
}
