//! Integrity checks

use crate::docx::{Block, Document, VMerge};
use crate::layout::{ParaId, TableId};

use super::index::DocIndex;
use super::position::{Place, Position};
use super::selection::Selection;
use super::step::para_len;
use super::table::Grid;

#[derive(Clone, Debug, PartialEq)]
pub struct Violation {
    pub invariant: &'static str,
    pub detail: String,
}

pub fn check(doc: &Document, idx: &DocIndex, selection: &Selection) -> Vec<Violation> {
    let mut out = Vec::new();
    for s in &doc.sections {
        blocks(&s.blocks, &mut out, true);
    }
    for b in doc.stories.values().chain(doc.footnotes.values()).chain(doc.endnotes.values()).chain(doc.glossary_bodies()) {
        blocks(b, &mut out, false);
    }
    if let Some(Block::Table(t)) = doc.sections.last().and_then(|s| s.blocks.last()) {
        out.push(Violation { invariant: "8 final paragraph", detail: format!("the body ends with table {}", t.id) });
    }
    for id in duplicate_ids(doc) {
        out.push(Violation { invariant: "4 unique ids", detail: format!("id {id} appears twice") });
    }
    fn anchors(blocks: &[Block], idx: &DocIndex, out: &mut Vec<Violation>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        if let crate::docx::Inline::Float(f, _) = i {
                            let id = crate::layout::FloatId(f.id);
                            if idx.float_anchor(id) != Some(crate::layout::ParaId(p.id)) {
                                out.push(Violation {
                                    invariant: "5 float anchors",
                                    detail: format!("float {} in paragraph {} is indexed at {:?}", f.id, p.id, idx.float_anchor(id)),
                                });
                            }
                            if let Some(tb) = &f.text_box {
                                anchors(&tb.blocks, idx, out);
                            }
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| anchors(&c.blocks, idx, out)),
            }
        }
    }
    for b in doc
        .sections
        .iter()
        .map(|s| &s.blocks)
        .chain(doc.stories.values())
        .chain(doc.footnotes.values())
        .chain(doc.endnotes.values())
        .chain(doc.glossary_bodies())
    {
        anchors(b, idx, &mut out);
    }
    let mut position = |p: Option<Position>, what: &str| {
        let Some(p) = p else { return };
        if let Some(e) = invalid(doc, idx, &p) {
            out.push(Violation { invariant: "7 valid positions", detail: format!("{what}: {e}") });
        }
    };
    position(selection.anchor(), "anchor");
    position(selection.focus(), "focus");
    elements(doc, idx, &mut out);
    review(doc, &mut out);
    out
}

fn review(doc: &Document, out: &mut Vec<Violation>) {
    use crate::docx::review::{CommentId, MarkKind, RangeKind, RevisionId, RevisionMark};
    use crate::docx::{Inline, RunStyle};
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;

    #[derive(Default)]
    struct Seen<'a> {
        marks: HashMap<RevisionId, &'a RevisionMark>,
        move_from: HashSet<Rc<str>>,
        move_to: HashSet<Rc<str>>,
        refs: HashSet<CommentId>,
        ranged: HashSet<CommentId>,
    }
    #[derive(Clone, Copy)]
    enum Slot {
        Ins,
        Del,
        Any,
    }
    fn mark<'a>(m: &'a RevisionMark, slot: Slot, seen: &mut Seen<'a>, out: &mut Vec<Violation>) {
        if let Some(prev) = seen.marks.insert(m.id, m)
            && *prev != *m
        {
            out.push(Violation { invariant: "review: unique revision ids", detail: format!("revision {} names two changes", m.id.0) });
        }
        let fits = match slot {
            Slot::Any => true,
            Slot::Ins => matches!(m.kind, MarkKind::Insert | MarkKind::MoveTo),
            Slot::Del => matches!(m.kind, MarkKind::Delete | MarkKind::MoveFrom),
        };
        if !fits {
            out.push(Violation { invariant: "review: slot kinds", detail: format!("revision {} is a {:?} in the wrong slot", m.id.0, m.kind) });
        }
        if let Some(name) = &m.move_name {
            match m.kind {
                MarkKind::MoveFrom => {
                    seen.move_from.insert(name.clone());
                }
                MarkKind::MoveTo => {
                    seen.move_to.insert(name.clone());
                }
                MarkKind::Insert | MarkKind::Delete | MarkKind::Format => {}
            }
        }
    }
    fn run<'a>(r: &'a RunStyle, seen: &mut Seen<'a>, out: &mut Vec<Violation>) {
        if let Some(m) = &r.rev.ins {
            mark(m, Slot::Ins, seen, out);
        }
        if let Some(m) = &r.rev.del {
            mark(m, Slot::Del, seen, out);
        }
        if let Some(f) = &r.rev.fmt {
            mark(&f.mark, Slot::Any, seen, out);
        }
    }
    fn unended(open: &mut Vec<CommentId>, out: &mut Vec<Violation>) {
        for id in open.drain(..) {
            out.push(Violation { invariant: "review: comment ranges", detail: format!("comment {} has no range end", id.0) });
        }
    }
    fn blocks<'a>(bs: &'a [Block], open: &mut Vec<CommentId>, seen: &mut Seen<'a>, doc: &Document, out: &mut Vec<Violation>) {
        for b in bs {
            match b {
                Block::Para(p) => {
                    run(&p.mark, seen, out);
                    if let Some(c) = &p.style.change {
                        mark(&c.mark, Slot::Any, seen, out);
                    }
                    for i in &p.inlines {
                        if let Some(r) = i.run() {
                            run(r, seen, out);
                        }
                        match i {
                            Inline::Range(m) if matches!(m.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => {
                                let id = CommentId(m.id);
                                if !doc.comments.by_id.contains_key(&id) {
                                    out.push(Violation { invariant: "review: comment ranges", detail: format!("range of missing comment {}", m.id) });
                                }
                                if m.kind == RangeKind::CommentStart {
                                    open.push(id);
                                    seen.ranged.insert(id);
                                } else if let Some(k) = open.iter().rposition(|o| *o == id) {
                                    open.remove(k);
                                } else {
                                    out.push(Violation {
                                        invariant: "review: comment ranges",
                                        detail: format!("comment {} ends before it starts", m.id),
                                    });
                                }
                            }
                            Inline::CommentRef { id, .. } => {
                                if !doc.comments.by_id.contains_key(id) {
                                    out.push(Violation {
                                        invariant: "review: comment ranges",
                                        detail: format!("reference to missing comment {}", id.0),
                                    });
                                }
                                seen.refs.insert(*id);
                            }
                            i if i.story().is_some() => {
                                if let Some((_, tb)) = i.story() {
                                    blocks(&tb.blocks, open, seen, doc, out);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => {
                    for m in [t.change.as_ref().map(|c| &c.mark), t.grid_change.as_ref().map(|c| &c.mark)].into_iter().flatten() {
                        mark(m, Slot::Any, seen, out);
                    }
                    for r in &t.rows {
                        if let Some(m) = &r.rev.ins {
                            mark(m, Slot::Ins, seen, out);
                        }
                        if let Some(m) = &r.rev.del {
                            mark(m, Slot::Del, seen, out);
                        }
                        for c in [&r.rev.change, &r.rev.ex_change].into_iter().flatten() {
                            mark(&c.mark, Slot::Any, seen, out);
                        }
                        for c in &r.cells {
                            if let Some((m, _)) = &c.rev.structural {
                                mark(m, Slot::Any, seen, out);
                            }
                            if let Some(ch) = &c.rev.change {
                                mark(&ch.mark, Slot::Any, seen, out);
                            }
                            blocks(&c.blocks, open, seen, doc, out);
                        }
                    }
                }
            }
        }
    }
    let mut seen = Seen::default();
    for s in &doc.sections {
        if let Some(c) = &s.change {
            mark(&c.mark, Slot::Any, &mut seen, out);
        }
    }
    let mut open = Vec::new();
    for s in &doc.sections {
        blocks(&s.blocks, &mut open, &mut seen, doc, out);
    }
    unended(&mut open, out);
    let others = doc.stories.values().chain(doc.footnotes.values()).chain(doc.endnotes.values()).chain(doc.glossary_bodies());
    for b in others.chain(doc.comments.by_id.values().map(|c| &c.body)) {
        blocks(b, &mut open, &mut seen, doc, out);
        unended(&mut open, out);
    }
    for id in seen.ranged.difference(&seen.refs) {
        out.push(Violation { invariant: "review: comment ranges", detail: format!("comment {} has a range but no reference", id.0) });
    }
    for name in seen.move_from.symmetric_difference(&seen.move_to) {
        out.push(Violation { invariant: "review: moves", detail: format!("move {name} has one end") });
    }
    for c in doc.comments.by_id.values() {
        let Some(p) = c.parent else { continue };
        if !doc.comments.by_id.contains_key(&p) {
            out.push(Violation { invariant: "review: threads", detail: format!("comment {} replies to missing comment {}", c.id.0, p.0) });
        }
        let (mut at, mut steps) = (p, 0);
        while let Some(next) = doc.comments.by_id.get(&at).and_then(|x| x.parent) {
            steps += 1;
            if next == c.id || steps > doc.comments.by_id.len() {
                out.push(Violation { invariant: "review: threads", detail: format!("comment {} is in a reply cycle", c.id.0) });
                break;
            }
            at = next;
        }
    }
}

fn elements(doc: &Document, idx: &DocIndex, out: &mut Vec<Violation>) {
    use crate::docx::{ControlId, ControlKind, ElementId, Inline};
    use std::collections::{HashMap, HashSet};
    let mut seen_fields: HashSet<crate::docx::FieldId> = HashSet::new();
    let mut used: HashSet<ControlId> = HashSet::new();
    let mut members: HashMap<ControlId, (String, Vec<usize>)> = HashMap::new();
    fn walk(
        blocks: &[Block],
        container: &str,
        doc: &Document,
        seen: &mut HashSet<crate::docx::FieldId>,
        used: &mut HashSet<ControlId>,
        members: &mut HashMap<ControlId, (String, Vec<usize>)>,
        out: &mut Vec<Violation>,
    ) {
        let lists = |list: &[ControlId], what: String, out: &mut Vec<Violation>| {
            for (k, c) in list.iter().enumerate() {
                let kind = doc.controls.get(c).map(|c| &c.props.kind);
                let next = list.get(k + 1).and_then(|n| doc.controls.get(n)).map(|c| &c.props.kind);
                let prev = k.checked_sub(1).and_then(|p| doc.controls.get(&list[p])).map(|c| &c.props.kind);
                if kind == Some(&ControlKind::RepeatingItem) && prev != Some(&ControlKind::RepeatingSection) {
                    out.push(Violation { invariant: "C3 item in a section", detail: format!("{what}: item {c:?} not directly in a section") });
                }
                if kind == Some(&ControlKind::RepeatingSection) && next != Some(&ControlKind::RepeatingItem) {
                    out.push(Violation { invariant: "C3 item in a section", detail: format!("{what}: section {c:?} without an item") });
                }
            }
        };
        for (bi, b) in blocks.iter().enumerate() {
            let (ids, what) = match b {
                Block::Para(p) => (&p.controls, format!("paragraph {}", p.id)),
                Block::Table(t) => (&t.controls, format!("table {}", t.id)),
            };
            lists(ids, what.clone(), out);
            for c in ids {
                used.insert(*c);
                let e = members.entry(*c).or_insert_with(|| (container.to_string(), Vec::new()));
                if e.0 != container {
                    out.push(Violation { invariant: "C4 one container", detail: format!("control {c:?} in {} and {container}", e.0) });
                }
                e.1.push(bi);
            }
            match b {
                Block::Para(p) => {
                    let mut stack: Vec<ElementId> = Vec::new();
                    let mut sep: HashSet<crate::docx::FieldId> = HashSet::new();
                    for i in &p.inlines {
                        match i {
                            Inline::FieldBegin { id, .. } => {
                                if !seen.insert(*id) {
                                    out.push(Violation {
                                        invariant: "F1 one field",
                                        detail: format!("field {id:?} begins twice (paragraph {})", p.id),
                                    });
                                }
                                stack.push(ElementId::Field(*id));
                            }
                            Inline::FieldSep { id, .. } => {
                                if stack.last() != Some(&ElementId::Field(*id)) || !sep.insert(*id) {
                                    out.push(Violation {
                                        invariant: "F2 fields nest",
                                        detail: format!("separator of {id:?} out of place (paragraph {})", p.id),
                                    });
                                }
                            }
                            Inline::FieldEnd { id, .. } => {
                                if stack.pop() != Some(ElementId::Field(*id)) {
                                    out.push(Violation {
                                        invariant: "F2 fields nest",
                                        detail: format!("end of {id:?} out of place (paragraph {})", p.id),
                                    });
                                }
                            }
                            Inline::ControlStart(c) => {
                                used.insert(*c);
                                stack.push(ElementId::Control(*c));
                            }
                            Inline::ControlEnd(c) => {
                                if stack.pop() != Some(ElementId::Control(*c)) {
                                    out.push(Violation {
                                        invariant: "C2 controls nest",
                                        detail: format!("end of {c:?} out of place (paragraph {})", p.id),
                                    });
                                }
                            }
                            Inline::Sym { control: Some(c), .. } => {
                                used.insert(*c);
                            }
                            i if i.story().is_some() => {
                                if let Some((id, tb)) = i.story() {
                                    walk(&tb.blocks, &format!("text box {id}"), doc, seen, used, members, out);
                                }
                            }
                            _ => {}
                        }
                    }
                    if !stack.is_empty() {
                        out.push(Violation { invariant: "F1 one field", detail: format!("{stack:?} not closed in paragraph {}", p.id) });
                    }
                    for s in crate::docx::fields::spans(&p.inlines) {
                        if let Inline::FieldBegin { kind, .. } = &p.inlines[s.begin]
                            && *kind != Some(crate::docx::FieldKind::SubTemplate)
                            // A legacy form field's kind is from its form data.
                            && !matches!(kind, Some(crate::docx::FieldKind::CheckBox { .. } | crate::docx::FieldKind::DropDown))
                            // None: a locked field, or one of the application's types.
                            && kind.is_some()
                            && *kind != crate::docx::fields::kind_at(&p.inlines, s)
                        {
                            out.push(Violation { invariant: "F3 kind from code", detail: format!("field in paragraph {} has {kind:?}", p.id) });
                        }
                    }
                }
                Block::Table(t) => {
                    let mut rows: HashMap<ControlId, Vec<usize>> = HashMap::new();
                    for (ri, r) in t.rows.iter().enumerate() {
                        lists(&r.controls, format!("table {} row {ri}", t.id), out);
                        for c in &r.controls {
                            used.insert(*c);
                            rows.entry(*c).or_default().push(ri);
                        }
                        for (ci, c) in r.cells.iter().enumerate() {
                            lists(&c.controls, format!("table {} row {ri} cell {ci}", t.id), out);
                            used.extend(c.controls.iter().copied());
                            walk(&c.blocks, &format!("table {} row {ri} cell {ci}", t.id), doc, seen, used, members, out);
                        }
                    }
                    for (c, v) in rows {
                        if v.windows(2).any(|w| w[1] != w[0] + 1) {
                            out.push(Violation { invariant: "C2 contiguous", detail: format!("control {c:?} wraps rows {v:?} of table {}", t.id) });
                        }
                    }
                }
            }
        }
    }
    for (k, s) in doc.sections.iter().enumerate() {
        walk(&s.blocks, &format!("section {k}"), doc, &mut seen_fields, &mut used, &mut members, out);
    }
    let mut stories: Vec<(&String, &Vec<Block>)> = doc.stories.iter().collect();
    stories.sort_by_key(|(k, _)| *k);
    for (name, b) in stories {
        walk(b, name, doc, &mut seen_fields, &mut used, &mut members, out);
    }
    for (kind, notes) in [("footnote", &doc.footnotes), ("endnote", &doc.endnotes)] {
        for (id, b) in notes {
            walk(b, &format!("{kind} {id}"), doc, &mut seen_fields, &mut used, &mut members, out);
        }
    }
    for (k, b) in doc.glossary_bodies().enumerate() {
        walk(b, &format!("building block {k}"), doc, &mut seen_fields, &mut used, &mut members, out);
    }
    for (c, (container, v)) in &members {
        if v.windows(2).any(|w| w[1] != w[0] + 1) {
            out.push(Violation { invariant: "C2 contiguous", detail: format!("control {c:?} wraps blocks {v:?} of {container}") });
        }
    }
    for c in &used {
        if !doc.controls.contains_key(c) {
            out.push(Violation { invariant: "C1 controls resolve", detail: format!("control {c:?} is used but not in the document's controls") });
        }
    }
    for c in doc.controls.keys() {
        if !used.contains(c) {
            out.push(Violation { invariant: "C1 controls resolve", detail: format!("control {c:?} has no content") });
        }
        if used.contains(c) != idx.element(ElementId::Control(*c)).is_some() {
            out.push(Violation {
                invariant: "C1 controls resolve",
                detail: format!("control {c:?} indexed {:?}", idx.element(ElementId::Control(*c))),
            });
        }
    }
    let mut items = HashSet::new();
    for p in &doc.data_parts {
        if !items.insert(&p.item_id) {
            out.push(Violation { invariant: "D1 unique item ids", detail: format!("item {} twice", p.item_id) });
        }
        if roxmltree::Document::parse(&p.xml).is_err() {
            out.push(Violation { invariant: "D2 well-formed", detail: format!("item {} isn't well-formed", p.item_id) });
        }
    }
}

fn blocks(blocks: &[Block], out: &mut Vec<Violation>, _body: bool) {
    for b in blocks {
        let Block::Table(t) = b else { continue };
        let grid = Grid::of(t);
        for (r, row) in t.rows.iter().enumerate() {
            let width = row.grid_before + row.cells.iter().map(|c| c.span.max(1)).sum::<usize>() + row.grid_after;
            if !t.grid.is_empty() && width > t.grid.len() {
                out.push(Violation {
                    invariant: "2 grid spans",
                    detail: format!("table {} row {r} spans {width} of {} columns", t.id, t.grid.len()),
                });
            }
            for (c, cell) in row.cells.iter().enumerate() {
                if !matches!(cell.blocks.last(), Some(Block::Para(_))) {
                    out.push(Violation { invariant: "1 cell ends with a paragraph", detail: format!("table {} row {r} cell {c}", t.id) });
                }
                if cell.v_merge == VMerge::Continue {
                    let (sr, _) = grid.merge_start(r, c);
                    if sr == r {
                        out.push(Violation { invariant: "3 merge has a start", detail: format!("table {} row {r} cell {c}", t.id) });
                    }
                }
                self::blocks(&cell.blocks, out, false);
            }
        }
    }
}

pub fn invalid(doc: &Document, idx: &DocIndex, p: &Position) -> Option<String> {
    match p.place {
        Place::Text { para, offset, .. } => {
            let Some(par) = idx.para(doc, para) else { return Some(format!("paragraph {para:?} isn't in the document")) };
            if offset > para_len(par) {
                return Some(format!("offset {offset:?} past the end of paragraph {para:?}"));
            }
            let slot = idx.para_slot(para)?;
            if idx.story(slot.container) != p.story {
                return Some(format!("paragraph {para:?} isn't in story {:?}", p.story));
            }
            if !super::text::UnitText::of(par).is_boundary(offset) {
                return Some(format!("offset {offset:?} of {para:?} splits a grapheme"));
            }
            None
        }
        Place::RowEnd { table, row } => match idx.table(doc, table) {
            Some(t) if row < t.rows.len() => None,
            _ => Some(format!("row {row} of table {table:?} isn't in the document")),
        },
    }
}

pub fn duplicate_ids(doc: &Document) -> Vec<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut dup = Vec::new();
    fn walk(blocks: &[Block], seen: &mut std::collections::HashSet<u64>, dup: &mut Vec<u64>) {
        for b in blocks {
            let id = match b {
                Block::Para(p) => ParaId(p.id).0,
                Block::Table(t) => TableId(t.id).0,
            };
            if !seen.insert(id) {
                dup.push(id);
            }
            if let Block::Para(p) = b {
                for i in &p.inlines {
                    if let crate::docx::Inline::Float(f, _) = i {
                        if !seen.insert(f.id) {
                            dup.push(f.id);
                        }
                        if let Some(tb) = &f.text_box {
                            walk(&tb.blocks, seen, dup);
                        }
                    }
                }
            }
            if let Block::Table(t) = b {
                t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, seen, dup));
            }
        }
    }
    for s in &doc.sections {
        walk(&s.blocks, &mut seen, &mut dup);
    }
    for b in doc.stories.values().chain(doc.footnotes.values()).chain(doc.endnotes.values()).chain(doc.glossary_bodies()) {
        walk(b, &mut seen, &mut dup);
    }
    dup
}
