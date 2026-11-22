//! Clipboard

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::docx::props::VertAlign;
use crate::docx::write::props::{Props, esc};
use crate::docx::{Block, Inline, Paragraph, Run, RunStyle, VMerge};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::Selection;
use super::step::{Step, para_len};

#[derive(Clone, Debug, Default)]
pub struct Fragment {
    pub id: u64,
    pub blocks: Vec<Block>,
    pub open_end: bool,
    pub cells: Option<super::cell_clipboard::CellCopy>,
    pub controls: std::collections::HashMap<crate::docx::ControlId, crate::docx::Control>,
    pub refs: Refs,
    /// The document it was copied from, and what its images, styles, lists and people are there.
    pub origin: Option<Rc<super::import::Origin>>,
    /// A drag's move: its bookmarks go with it, though the document still has them until the drag deletes them.
    pub moved: bool,
}

/// The notes and comments a fragment refers to, as they were when it was copied: pasting brings them back (a cut's
/// under their own ids, a copy's as copies).
#[derive(Clone, Debug, Default)]
pub struct Refs {
    pub notes: Vec<(crate::docx::NoteKind, i64, Vec<Block>)>,
    pub comments: Vec<crate::docx::review::Comment>,
}

/// The bookmark names and editing permission ids a paste can't give its pieces: the document's (read once, when a
/// piece has any) and those earlier pieces took; and the lists earlier pieces brought from another document.
#[derive(Default)]
pub(super) struct Taken {
    read: bool,
    names: HashSet<String>,
    ids: HashSet<i64>,
    lists: super::import::Lists,
}

/// Takes out the markers of comments the fragment holds only part of (its range's start, end and reference): those
/// stay with the document.
fn strip_partial_comments(blocks: &mut [Block]) {
    use crate::docx::review::{CommentId, RangeKind};
    let mut seen: HashMap<CommentId, u8> = HashMap::new();
    each_inline(blocks, &mut |i| match i {
        Inline::Range(r) if r.kind == RangeKind::CommentStart => *seen.entry(CommentId(r.id)).or_default() |= 1,
        Inline::Range(r) if r.kind == RangeKind::CommentEnd => *seen.entry(CommentId(r.id)).or_default() |= 2,
        Inline::CommentRef { id, .. } => *seen.entry(*id).or_default() |= 4,
        _ => {}
    });
    let partial: HashSet<CommentId> = seen.into_iter().filter(|(_, m)| *m != 7).map(|(id, _)| id).collect();
    if partial.is_empty() {
        return;
    }
    let keep = |i: &Inline| match i {
        Inline::Range(r) if matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => !partial.contains(&CommentId(r.id)),
        Inline::CommentRef { id, .. } => !partial.contains(id),
        _ => true,
    };
    fn walk(blocks: &mut [Block], keep: &dyn Fn(&Inline) -> bool) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    p.inlines.retain(keep);
                    for i in &mut p.inlines {
                        if let Some(tb) = i.story_mut() {
                            walk(&mut tb.blocks, keep);
                        }
                    }
                }
                Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| walk(&mut c.blocks, keep)),
            }
        }
    }
    walk(blocks, &keep);
}

/// The notes and comments (with their replies) of `doc` that `blocks` refer to.
pub(super) fn refs_of(doc: &crate::docx::Document, blocks: &[Block]) -> Refs {
    let mut notes = Vec::new();
    super::notes::refs_in(blocks, &mut notes);
    notes.sort_by_key(|&(k, id)| (k == crate::docx::NoteKind::End, id));
    notes.dedup();
    let notes = notes
        .into_iter()
        .filter_map(|(k, id)| {
            let body = match k {
                crate::docx::NoteKind::Foot => doc.footnotes.get(&id),
                crate::docx::NoteKind::End => doc.endnotes.get(&id),
            };
            Some((k, id, body?.clone()))
        })
        .collect();
    let mut ids = Vec::new();
    super::comments::ids_in_blocks(blocks, &mut ids);
    let ids: HashSet<_> = ids.into_iter().collect();
    let comments = doc.comments.by_id.values().filter(|c| ids.contains(&c.id) || c.parent.is_some_and(|p| ids.contains(&p))).cloned().collect();
    Refs { notes, comments }
}

/// Every paragraph's inlines in `blocks`, table cells and text boxes included.
fn each_inlines(blocks: &mut [Block], f: &mut dyn FnMut(&mut Vec<Inline>)) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                f(&mut p.inlines);
                for i in &mut p.inlines {
                    if let Some(tb) = i.story_mut() {
                        each_inlines(&mut tb.blocks, f);
                    }
                }
            }
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| each_inlines(&mut c.blocks, f)),
        }
    }
}

/// Every inline of `blocks`, table cells and text boxes included.
fn each_inline(blocks: &mut [Block], f: &mut dyn FnMut(&mut Inline)) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                for i in &mut p.inlines {
                    f(i);
                    if let Some(tb) = i.story_mut() {
                        each_inline(&mut tb.blocks, f);
                    }
                }
            }
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| each_inline(&mut c.blocks, f)),
        }
    }
}

#[derive(PartialEq)]
enum Key<'a> {
    Bookmark(&'a str),
    Perm(i64),
    Comment(i64),
}

/// The paragraph's bookmark, editing permission and (with `comments`) comment markers: inline index, units before,
/// key, and whether it starts its range.
fn range_markers(p: &Paragraph, comments: bool) -> Vec<(usize, u32, Key<'_>, bool)> {
    use crate::docx::review::RangeKind;
    let mut at = 0u32;
    let mut marks = Vec::new();
    for (k, i) in p.inlines.iter().enumerate() {
        let m = match i {
            Inline::Bookmark(n) => Some(n.strip_prefix('/').map_or((Key::Bookmark(n), true), |n| (Key::Bookmark(n), false))),
            Inline::Opaque(o) => o.permission().map(|(start, id)| (Key::Perm(id), start)),
            Inline::Range(r) if comments && r.kind == RangeKind::CommentStart => Some((Key::Comment(r.id), true)),
            Inline::Range(r) if comments && r.kind == RangeKind::CommentEnd => Some((Key::Comment(r.id), false)),
            Inline::CommentRef { id, .. } if comments => Some((Key::Comment(id.0), false)),
            _ => None,
        };
        if let Some((key, start)) = m {
            marks.push((k, at, key, start));
        }
        at += i.units() as u32;
    }
    marks
}

/// The markers on the edges of `a..b` (bookmarks', editing permissions' and, with `comments`, comments' starts, ends and
/// references) whose partner is in it too: they belong to the range, as Word moves a bookmark whose text is selected.
pub(super) fn bound_markers(p: &Paragraph, a: Unit, b: Unit, comments: bool) -> Vec<usize> {
    if a >= b {
        return Vec::new();
    }
    let marks = range_markers(p, comments);
    let within = |x: u32| a.0 <= x && x <= b.0;
    marks
        .iter()
        .filter(|(_, u, key, start)| {
            (*u == a.0 || *u == b.0)
                && marks.iter().all(|(_, v, k, s)| k != key || s == start || within(*v))
                && marks.iter().any(|(_, _, k, s)| k == key && s != start)
        })
        .map(|m| m.0)
        .collect()
}

/// The units of the bookmark and editing permission markers inside `a..b` whose partner isn't in it: deleting the
/// range keeps them at its edge, so the range they mark shrinks.
pub(super) fn severed_markers(p: &Paragraph, a: Unit, b: Unit) -> Vec<u32> {
    let marks = range_markers(p, false);
    let within = |x: u32| a.0 <= x && x <= b.0;
    marks
        .iter()
        .filter(|(_, u, key, start)| a.0 < *u && *u < b.0 && !marks.iter().any(|(_, v, k, s)| k == key && s != start && within(*v)))
        .map(|m| m.1)
        .collect()
}

fn slice(p: &Paragraph, a: Unit, b: Unit) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut at = 0u32;
    let bound = bound_markers(p, a, b, true);
    for (k, i) in p.inlines.iter().enumerate() {
        let n = i.units() as u32;
        let (s, e) = (at, at + n);
        at = e;
        if n == 0 {
            let control = matches!(i, Inline::ControlStart(_) | Inline::ControlEnd(_));
            if (s > a.0 && s < b.0) || (control && s >= a.0 && s <= b.0) || bound.contains(&k) {
                out.push(i.clone());
            }
            continue;
        }
        if e <= a.0 || s >= b.0 {
            continue;
        }
        match i {
            Inline::Text(t, r) => {
                let (from, to) = (a.0.max(s) - s, b.0.min(e) - s);
                out.push(Inline::Text(t.chars().skip(from as usize).take((to - from) as usize).collect(), r.clone()));
            }
            other => out.push(other.clone()),
        }
    }
    out
}

pub(super) fn fresh(b: &Block) -> Block {
    match b {
        Block::Para(p) => {
            let mut p = p.clone();
            p.id = crate::docx::next_para_id();
            p.src = None;
            for i in &mut p.inlines {
                let story = match i {
                    Inline::Float(f, _) => {
                        f.id = crate::docx::next_para_id();
                        f.text_box.as_mut()
                    }
                    Inline::Picture(pic, _) => pic.text.as_mut().map(|t| {
                        t.id = crate::docx::next_para_id();
                        &mut t.text_box
                    }),
                    _ => None,
                };
                if let Some(tb) = story {
                    tb.blocks = tb.blocks.iter().map(fresh).collect();
                }
            }
            Block::Para(p)
        }
        Block::Table(t) => {
            let mut t = t.clone();
            t.id = crate::docx::next_para_id();
            t.src = None;
            for r in &mut t.rows {
                r.src = None;
                for c in &mut r.cells {
                    c.blocks = c.blocks.iter().map(fresh).collect();
                }
            }
            Block::Table(t)
        }
    }
}

fn plain(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                out.extend(super::text::UnitText::of(p).text.chars().map(|c| match c {
                    '\u{2028}' => '\n',
                    '\u{c}' | '\u{FFFC}' => ' ',
                    c => c,
                }));
                out.push('\n');
            }
            Block::Table(t) => {
                for r in &t.rows {
                    for (k, c) in r.cells.iter().enumerate() {
                        if k > 0 {
                            out.push('\t');
                        }
                        let mut s = String::new();
                        plain(&c.blocks, &mut s);
                        out.push_str(s.trim_end_matches('\n').replace('\n', " ").as_str());
                    }
                    out.push('\n');
                }
            }
        }
    }
}

fn css_run(r: &RunStyle) -> String {
    let mut s = format!("font-family:'{}';font-size:{}pt", esc(&r.ascii), r.size);
    if r.bold {
        s.push_str(";font-weight:bold");
    }
    if r.italic {
        s.push_str(";font-style:italic");
    }
    let mut deco = Vec::new();
    if r.underline.is_some() {
        deco.push("underline");
    }
    if r.strike || r.dstrike {
        deco.push("line-through");
    }
    if !deco.is_empty() {
        s.push_str(&format!(";text-decoration:{}", deco.join(" ")));
    }
    if let Some(c) = r.color {
        s.push_str(&format!(";color:#{c:06X}"));
    }
    if let Some(h) = r.highlight {
        s.push_str(&format!(";background:#{h:06X}"));
    }
    match r.vert_align {
        VertAlign::Superscript => s.push_str(";vertical-align:super"),
        VertAlign::Subscript => s.push_str(";vertical-align:sub"),
        VertAlign::Baseline => {}
    }
    if r.small_caps {
        s.push_str(";font-variant:small-caps");
    }
    if r.caps {
        s.push_str(";text-transform:uppercase");
    }
    s
}

fn html(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                let align = match p.style.jc {
                    crate::docx::props::Jc::Center => "center",
                    crate::docx::props::Jc::Right => "right",
                    crate::docx::props::Jc::Both | crate::docx::props::Jc::Distribute => "justify",
                    crate::docx::props::Jc::Left => "left",
                };
                out.push_str(&format!("<p style=\"margin:0;text-align:{align}\">"));
                if let Some(l) = &p.list {
                    out.push_str(&format!("<span style=\"{}\">{}&#9;</span>", css_run(&l.run), esc(&l.text)));
                }
                for (i, shown) in p.inlines.iter().zip(crate::docx::fields::shown(&p.inlines)) {
                    let text = match (shown, i) {
                        (crate::docx::fields::Shown::Value(v), _) => esc(&v),
                        (crate::docx::fields::Shown::Not, _) => continue,
                        (_, Inline::Text(t, _)) => esc(t),
                        (_, Inline::Tab(_) | Inline::PTab(..)) => "&#9;".into(),
                        (_, Inline::Break(..)) => "<br>".into(),
                        (_, Inline::Sym { ch, .. }) => esc(&ch.to_string()),
                        _ => continue,
                    };
                    match i.run() {
                        Some(r) => out.push_str(&format!("<span style=\"{}\">{text}</span>", css_run(r))),
                        None => out.push_str(&text),
                    }
                }
                if p.inlines.iter().all(|i| i.units() == 0) {
                    out.push_str("<br>");
                }
                out.push_str("</p>");
            }
            Block::Table(t) => {
                out.push_str("<table style=\"border-collapse:collapse\">");
                let g = super::table::Grid::of(t);
                for (ri, r) in t.rows.iter().enumerate() {
                    out.push_str("<tr>");
                    for (ci, c) in r.cells.iter().enumerate() {
                        if c.v_merge == VMerge::Continue {
                            continue;
                        }
                        let col = g.rows[ri][ci].0;
                        let rows = if c.v_merge == VMerge::Restart {
                            1 + (ri + 1..t.rows.len()).take_while(|&k| g.cell_at(k, col).is_some_and(|kc| g.continues(k, kc))).count()
                        } else {
                            1
                        };
                        out.push_str(&format!(
                            "<td style=\"border:1px solid #000;vertical-align:top\" colspan=\"{}\" rowspan=\"{rows}\">",
                            c.span.max(1)
                        ));
                        html(&c.blocks, out);
                        out.push_str("</td>");
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
            }
        }
    }
}

fn without_revisions(r: &Run) -> Run {
    let mut s = RunStyle::clone(r);
    s.rev = Default::default();
    Rc::new(s)
}

fn accept(blocks: &mut Vec<Block>) {
    join_deleted_marks(blocks);
    for b in blocks.iter_mut() {
        match b {
            Block::Para(p) => {
                p.inlines.retain(|i| i.run().is_none_or(|r| r.rev.del.is_none()));
                for i in &mut p.inlines {
                    if let Some(r) = i.run().filter(|r| !r.rev.is_empty()) {
                        *i = super::format::with_run(i, &without_revisions(r));
                    }
                    if let Some(tb) = i.story_mut() {
                        accept(&mut tb.blocks);
                    }
                }
                if !p.mark.rev.is_empty() {
                    p.mark = without_revisions(&p.mark);
                }
                if p.style.change.is_some() {
                    Rc::make_mut(&mut p.style).change = None;
                }
            }
            Block::Table(t) => {
                t.rows.retain(|r| r.rev.del.is_none());
                (t.change, t.grid_change) = (None, None);
                for r in &mut t.rows {
                    r.rev = Default::default();
                    for c in &mut r.cells {
                        c.rev = Default::default();
                        accept(&mut c.blocks);
                    }
                }
            }
        }
    }
    blocks.retain(|b| !matches!(b, Block::Table(t) if t.rows.is_empty()));
}

/// Paragraphs whose mark is a tracked deletion joined to the paragraph after them, which keeps its own properties, as
/// accepting the deletion does; an empty one before a table goes.
fn join_deleted_marks(blocks: &mut Vec<Block>) {
    let deleted = |b: &Block| matches!(b, Block::Para(p) if p.mark.rev.del.is_some());
    if !blocks.iter().any(deleted) {
        return;
    }
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    for b in std::mem::take(blocks) {
        match (out.last_mut(), b) {
            (Some(Block::Para(p)), Block::Para(q)) if p.mark.rev.del.is_some() => {
                p.inlines.extend(q.inlines);
                (p.style, p.mark, p.list) = (q.style, q.mark, q.list);
            }
            (Some(Block::Para(p)), b @ Block::Table(_)) if p.mark.rev.del.is_some() && para_len(p) == Unit(0) => {
                out.pop();
                out.push(b);
            }
            (_, b) => out.push(b),
        }
    }
    *blocks = out;
}

impl Fragment {
    pub fn accepted(mut self) -> Fragment {
        accept(&mut self.blocks);
        self
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        plain(&self.blocks, &mut s);
        if self.open_end && s.ends_with('\n') {
            s.pop();
        }
        s
    }

    pub fn html(&self) -> String {
        let mut s = String::from("<meta charset=\"utf-8\">");
        html(&self.blocks, &mut s);
        s
    }
}

impl Editor {
    pub fn take_clipboard_request(&mut self) -> bool {
        let n = self.requests.len();
        self.requests.retain(|r| !matches!(r, super::input::Request::Clipboard { .. }));
        self.requests.len() < n
    }

    pub fn copy_fragment(&mut self) -> Option<Fragment> {
        let mut f = self.copy_content()?;
        strip_partial_comments(&mut f.blocks);
        f.refs = self.refs_of(&f.blocks);
        Some(f)
    }

    /// The selection as content to paste, as Copy and dragging take it: revisions accepted, markers whose partner it
    /// doesn't hold and move ranges left out, block controls it holds only part of left out, and the definitions of
    /// the controls in it (and in the notes and comments it refers to) and where it was copied from kept with it.
    pub(super) fn clip(&mut self) -> Option<Fragment> {
        let mut f = self.copy_fragment()?.accepted();
        drop_unpaired(&mut f.blocks);
        drop_partial(&mut f.blocks, &self.index);
        f.controls = controls_of(&f.blocks, &self.doc);
        for body in f.refs.notes.iter().map(|n| &n.2).chain(f.refs.comments.iter().map(|c| &c.body)) {
            f.controls.extend(controls_of(body, &self.doc));
        }
        f.origin = Some(self.origin_of(&mut f));
        Some(f)
    }

    /// Pasted `blocks` (fresh copies) made this document's, in `tx`: their controls' definitions, the notes and comments
    /// they refer to, what they show and use from the document `f` came from, and no second bookmark of a name the
    /// document has (a copy's; a cut's or a move's goes with it) or editing permission of an id it has.
    pub(super) fn take_in(&mut self, tx: &mut super::Tx, blocks: &mut [Block], f: &Fragment, taken: &mut Taken) {
        let imported;
        let refs = match &f.origin {
            Some(o) if o.key != self.key || !o.images.is_empty() => {
                let mut refs = f.refs.clone();
                self.import(tx, blocks, &mut refs, o, &mut taken.lists);
                imported = refs;
                &imported
            }
            _ => &f.refs,
        };
        for (id, c) in adopt(blocks, &f.controls) {
            let _ = self.step(tx, Step::Control { id, control: Some(c) });
        }
        self.paste_refs(tx, blocks, refs, Some(&f.controls));
        self.unique_markers(blocks, f.moved, taken);
    }

    /// Bookmarks and editing permissions in pasted `blocks` that the document has, or an earlier piece of the same
    /// paste (`taken`): a bookmark (unless `moved`) goes, a permission takes a new id.
    fn unique_markers(&self, blocks: &mut [Block], moved: bool, taken: &mut Taken) {
        let mut bookmarks = false;
        let mut perms: Vec<i64> = Vec::new();
        each_inline(blocks, &mut |i| match i {
            Inline::Bookmark(_) => bookmarks = true,
            Inline::Opaque(o) => perms.extend(o.permission().map(|(_, id)| id)),
            _ => {}
        });
        if !bookmarks && perms.is_empty() {
            return;
        }
        if !taken.read {
            taken.read = true;
            for para in self.index.para_ids() {
                let Some(p) = self.index.para(&self.doc, para) else { continue };
                for i in &p.inlines {
                    match i {
                        Inline::Bookmark(n) if !n.starts_with('/') => {
                            taken.names.insert(n.to_lowercase());
                        }
                        Inline::Opaque(o) => taken.ids.extend(o.permission().map(|(_, id)| id)),
                        _ => {}
                    }
                }
            }
        }
        // New ids follow every id the document and the pieces have.
        let next = taken.ids.iter().chain(&perms).copied().max().unwrap_or(0);
        let (names, ids) = (&taken.names, &taken.ids);
        let mut kept: Vec<String> = Vec::new();
        let mut map: HashMap<i64, i64> = HashMap::new();
        let mut next = next;
        each_inlines(blocks, &mut |inlines| {
            if bookmarks {
                inlines.retain(|i| match i {
                    Inline::Bookmark(n) => {
                        let name = n.trim_start_matches('/').to_lowercase();
                        let keep = moved || !names.contains(&name);
                        if keep && !n.starts_with('/') {
                            kept.push(name);
                        }
                        keep
                    }
                    _ => true,
                });
            }
            for i in inlines.iter_mut() {
                let Inline::Opaque(o) = i else { continue };
                let Some((_, id)) = o.permission() else { continue };
                let to = *map.entry(id).or_insert_with(|| {
                    if ids.contains(&id) {
                        next += 1;
                        next
                    } else {
                        id
                    }
                });
                if to != id
                    && let Some(x) = o.xml.0.as_deref().and_then(|x| with_perm_id(x, to))
                {
                    *i = Inline::Opaque(Rc::new(crate::docx::Opaque { xml: crate::docx::Raw(Some(x.into())) }));
                }
            }
        });
        taken.names.extend(kept);
        taken.ids.extend(map.into_values());
    }

    /// The notes and comments (with their replies) `blocks` refer to.
    fn refs_of(&self, blocks: &[Block]) -> Refs {
        refs_of(&self.doc, blocks)
    }

    /// Brings back the notes and comments pasted content refers to: under their own ids when those are free (they were
    /// cut), else as copies under new ids, the pasted references renumbered. The controls in their bodies take new ids
    /// from `defs`, the definitions they came with; `None` when they are the document's already. Returns the notes'
    /// stories.
    pub(super) fn paste_refs(
        &mut self,
        tx: &mut super::Tx,
        blocks: &mut [Block],
        refs: &Refs,
        defs: Option<&HashMap<crate::docx::ControlId, crate::docx::Control>>,
    ) -> Vec<super::position::StoryId> {
        use crate::docx::NoteKind;
        use crate::docx::review::{CommentId, RangeKind};
        // Only those these blocks refer to: a cell block pastes its cells' content piece by piece.
        let mut held = Vec::new();
        super::notes::refs_in(blocks, &mut held);
        let mut ids = Vec::new();
        super::comments::ids_in_blocks(blocks, &mut ids);
        let ids: HashSet<CommentId> = ids.into_iter().collect();
        let refs = Refs {
            notes: refs.notes.iter().filter(|n| held.contains(&(n.0, n.1))).cloned().collect(),
            comments: refs.comments.iter().filter(|c| ids.contains(&c.id) || c.parent.is_some_and(|p| ids.contains(&p))).cloned().collect(),
        };
        let refs = &refs;
        // Every note's new id first, then the references renumbered once: renumbering one at a time would catch
        // references an earlier note was just given.
        let mut to_ids: HashMap<(NoteKind, i64), i64> = HashMap::new();
        let pasted: std::collections::HashSet<(NoteKind, i64)> = refs.notes.iter().map(|n| (n.0, n.1)).collect();
        let mut lo = [1; 2];
        for &(kind, id, _) in &refs.notes {
            let notes = if kind == NoteKind::Foot { &self.doc.footnotes } else { &self.doc.endnotes };
            let taken = |n: i64, map: &HashMap<(NoteKind, i64), i64>| notes.contains_key(&n) || map.iter().any(|(k, v)| k.0 == kind && *v == n);
            let to = if taken(id, &to_ids) {
                let used = map_max(&to_ids, kind).max(notes.keys().copied().max().unwrap_or(0));
                let used = used.max(refs.notes.iter().filter(|n| n.0 == kind).map(|n| n.1).max().unwrap_or(0));
                let k = usize::from(kind == NoteKind::End);
                let to = super::notes::free_note_id(used, lo[k], |n| taken(n, &to_ids) || pasted.contains(&(kind, n)));
                if to <= used {
                    lo[k] = to + 1;
                }
                to
            } else {
                id
            };
            to_ids.insert((kind, id), to);
        }
        if to_ids.iter().any(|((_, id), to)| id != to) {
            each_inline(blocks, &mut |i| {
                if let Inline::NoteRef { kind, id, .. } = i
                    && let Some(&to) = to_ids.get(&(*kind, *id))
                {
                    *id = to;
                }
            });
        }
        let mut stories = Vec::with_capacity(refs.notes.len());
        for (kind, id, body) in &refs.notes {
            let to = to_ids[&(*kind, *id)];
            let mut body: Vec<Block> = body.iter().map(fresh).collect();
            for (c, k) in defs.map(|d| adopt(&mut body, d)).unwrap_or_default() {
                let _ = self.step(tx, Step::Control { id: c, control: Some(k) });
            }
            if to != *id {
                each_inline(&mut body, &mut |i| {
                    if let Inline::NoteMark(k, n, _) = i
                        && *k == *kind
                    {
                        *n = to;
                    }
                });
            }
            let _ = self.step(tx, Step::Note { kind: *kind, id: to, blocks: Some(body) });
            stories.push(match kind {
                NoteKind::Foot => super::position::StoryId::Footnote(to),
                NoteKind::End => super::position::StoryId::Endnote(to),
            });
        }
        let mut next = self.doc.comments.by_id.keys().map(|c| c.0).max().unwrap_or(0) + 1;
        let mut map: HashMap<CommentId, CommentId> = HashMap::new();
        for c in &refs.comments {
            let to = if self.doc.comments.by_id.contains_key(&c.id) || map.values().any(|v| *v == c.id) {
                next += 1;
                CommentId(next - 1)
            } else {
                c.id
            };
            map.insert(c.id, to);
        }
        if map.iter().any(|(a, b)| a != b) {
            each_inline(blocks, &mut |i| match i {
                Inline::Range(r) if matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => {
                    if let Some(to) = map.get(&CommentId(r.id)) {
                        r.id = to.0;
                    }
                }
                Inline::CommentRef { id, .. } => {
                    if let Some(to) = map.get(id) {
                        *id = *to;
                    }
                }
                _ => {}
            });
        }
        for c in &refs.comments {
            let to = map[&c.id];
            let mut c = c.clone();
            if to != c.id {
                c.para_id = self.doc.review_ids.next_para_id();
                c.durable_id = None;
            }
            c.id = to;
            c.parent = c.parent.map(|p| map.get(&p).copied().unwrap_or(p));
            c.body = c.body.iter().map(fresh).collect();
            for (id, k) in defs.map(|d| adopt(&mut c.body, d)).unwrap_or_default() {
                let _ = self.step(tx, Step::Control { id, control: Some(k) });
            }
            let _ = self.step(tx, Step::Comment { id: to, after: Some(c) });
        }
        stories
    }

    fn copy_content(&mut self) -> Option<Fragment> {
        if matches!(self.selection, Selection::Cells { .. }) {
            let id = self.clipboard.as_ref().map_or(1, |f| f.id + 1);
            return self.copy_cells(id);
        }
        let id = self.clipboard.as_ref().map_or(1, |f| f.id + 1);
        // Block selection
        if let Selection::Block { .. } = self.selection {
            let mut out = Vec::new();
            for (_, _, para, a, b) in self.block_pieces() {
                let Some(p) = self.index.para(&self.doc, para) else { continue };
                let mut q = p.clone();
                q.inlines = slice(p, a, b);
                out.push(Block::Para(q));
            }
            return (!out.is_empty()).then_some(Fragment { id, blocks: out, open_end: true, ..Default::default() });
        }
        let (a, b) = self.selection.ordered(&self.index)?;
        if a.same_place(&b) {
            return None;
        }
        let (pa, oa) = a.para_offset()?;
        let (pb, ob) = match b.para_offset() {
            Some(x) => x,
            None => {
                let p = self.nav().left(b, false);
                p.para_offset()?
            }
        };
        let (sa, sb) = (self.index.para_slot(pa)?, self.index.para_slot(pb)?);
        if sa.container == sb.container {
            let blocks = self.index.blocks(&self.doc, sa.container)?;
            let mut out = Vec::new();
            for (k, blk) in blocks.iter().enumerate().take(sb.block + 1).skip(sa.block) {
                match blk {
                    Block::Para(p) => {
                        let from = if k == sa.block { oa } else { Unit(0) };
                        let to = if k == sb.block { ob } else { para_len(p) };
                        let mut q = p.clone();
                        q.inlines = slice(p, from, to);
                        out.push(Block::Para(q));
                    }
                    t @ Block::Table(_) => out.push(t.clone()),
                }
            }
            // The end is a place in its paragraph, so the paragraph's mark isn't in it, even at its end; a row's end is
            // the end of its last cell's paragraph, whose cell mark doesn't paste as a paragraph mark.
            return Some(Fragment { id, blocks: out, open_end: true, ..Default::default() });
        }
        // Across cells or levels
        let mut out = Vec::new();
        for (para, s, e) in self.selected_ranges() {
            if let Some(p) = self.index.para(&self.doc, para) {
                let mut q = p.clone();
                q.inlines = slice(p, s, e);
                out.push(Block::Para(q));
            }
        }
        Some(Fragment { id, blocks: out, open_end: true, ..Default::default() })
    }

    fn keep_look(&self, src: &Run, base: &crate::docx::props::RunProps, cache: &mut HashMap<*const RunStyle, Run>) -> Run {
        if let Some(r) = cache.get(&Rc::as_ptr(src)) {
            return r.clone();
        }
        let Some(res) = self.doc.resolver.clone() else { return src.clone() };
        let cx = self.cx();
        let mut out = res.run_from_xml(base, src.direct.0.as_deref(), &cx);
        if *out != **src {
            let w = cx.w.clone();
            let mut p = Props::parse(src.direct.0.as_deref(), "rPr", &cx);
            let flag = |name: &str, on: bool| if on { format!("<{w}{name}/>") } else { format!("<{w}{name} {w}val=\"0\"/>") };
            if out.bold != src.bold {
                p.set("b", flag("b", src.bold));
                p.set("bCs", flag("bCs", src.bold));
            }
            if out.italic != src.italic {
                p.set("i", flag("i", src.italic));
                p.set("iCs", flag("iCs", src.italic));
            }
            if out.caps != src.caps {
                p.set("caps", flag("caps", src.caps));
            }
            if out.small_caps != src.small_caps {
                p.set("smallCaps", flag("smallCaps", src.small_caps));
            }
            if out.strike != src.strike {
                p.set("strike", flag("strike", src.strike));
            }
            if out.hidden != src.hidden {
                p.set("vanish", flag("vanish", src.hidden));
            }
            if out.underline != src.underline {
                p.set("u", format!("<{w}u {w}val=\"{}\"/>", src.underline.as_deref().unwrap_or("none")));
            }
            if (out.size - src.size).abs() > 0.01 {
                let hp = (src.size * 2.0).round() as i64;
                p.set("sz", format!("<{w}sz {w}val=\"{hp}\"/>"));
                p.set("szCs", format!("<{w}szCs {w}val=\"{hp}\"/>"));
            }
            if out.ascii != src.ascii || out.h_ansi != src.h_ansi || out.east_asia != src.east_asia || out.cs != src.cs {
                p.set(
                    "rFonts",
                    format!(
                        "<{w}rFonts {w}ascii=\"{}\" {w}hAnsi=\"{}\" {w}eastAsia=\"{}\" {w}cs=\"{}\"/>",
                        esc(&src.ascii),
                        esc(&src.h_ansi),
                        esc(&src.east_asia),
                        esc(&src.cs)
                    ),
                );
            }
            if out.color != src.color {
                p.set("color", format!("<{w}color {w}val=\"{}\"/>", src.color.map_or("auto".to_string(), |c| format!("{c:06X}"))));
            }
            if out.vert_align != src.vert_align {
                let v = match src.vert_align {
                    VertAlign::Superscript => "superscript",
                    VertAlign::Subscript => "subscript",
                    VertAlign::Baseline => "baseline",
                };
                p.set("vertAlign", format!("<{w}vertAlign {w}val=\"{v}\"/>"));
            }
            out = res.run_from_xml(base, p.xml().as_deref(), &cx);
        }
        cache.insert(Rc::as_ptr(src), out.clone());
        out
    }

    fn fit_inlines(&self, para: ParaId, inlines: &[Inline]) -> Vec<Inline> {
        let Some(res) = self.doc.resolver.clone() else { return inlines.to_vec() };
        let cx = self.cx();
        let Some(p) = self.index.para(&self.doc, para) else { return inlines.to_vec() };
        let region = self.region_of_para(para, &res);
        let base = res.para_from_xml(p.style.direct.0.as_deref(), region.as_ref(), &cx).base;
        let mut cache = HashMap::new();
        inlines
            .iter()
            .map(|i| match i.run() {
                Some(r) => super::format::with_run(i, &self.keep_look(r, &base, &mut cache)),
                None => i.clone(),
            })
            .collect()
    }

    pub fn paste_fragment(&mut self, f: &Fragment, fonts: &Fonts) -> Outcome {
        if let Some(o) = self.paste_cells(f, fonts) {
            return o;
        }
        let spaces = match f.blocks.as_slice() {
            [Block::Para(p)] if f.open_end => self.smart_spaces(&super::text::UnitText::of(p).text),
            _ => (false, false),
        };
        if self.refuse_expanded() || self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let mut tx = self.begin("Paste");
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => self.past_row_end(p),
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let Some((para, at)) = caret.para_offset() else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let mut blocks: Vec<Block> = f.blocks.iter().map(fresh).collect();
        self.take_in(&mut tx, &mut blocks, f, &mut Taken::default());
        let dest: Vec<crate::docx::ControlId> = self.index.para(&self.doc, para).map(|p| p.controls.clone()).unwrap_or_default();
        if !dest.is_empty() {
            for b in &mut blocks {
                match b {
                    Block::Para(p) => p.controls.splice(0..0, dest.iter().copied()),
                    Block::Table(t) => t.controls.splice(0..0, dest.iter().copied()),
                };
            }
        }
        let n = blocks.len();
        let first_inlines = match blocks.first() {
            Some(Block::Para(p)) => Some(p.inlines.clone()),
            _ => None,
        };
        let units = |v: &[Inline]| v.iter().map(|i| i.units() as u32).sum::<u32>();
        // One open paragraph
        if n == 1
            && f.open_end
            && let Some(inl) = &first_inlines
        {
            let mut with = self.fit_inlines(para, inl);
            let space = |i: Option<&Inline>| i.and_then(|i| i.run()).map(|r| Inline::Text(" ".into(), r.clone()));
            if spaces.0
                && let Some(s) = space(with.first())
            {
                with.insert(0, s);
            }
            if spaces.1
                && let Some(s) = space(with.last())
            {
                with.push(s);
            }
            let k = units(&with);
            if self.step(&mut tx, Step::Text { para, range: at..at, with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("paste failed");
            }
            self.selection = Selection::Caret(Position::text(caret.story, para, Unit(at.0 + k)));
            return self.commit(tx, None, fonts);
        }
        // Paragraphs before a block control
        if at == Unit(0) && n > 1 && matches!(blocks.first(), Some(Block::Para(p)) if p.controls.iter().any(|c| !dest.contains(c))) {
            let mut middle = blocks;
            let last = match middle.last() {
                Some(Block::Para(_)) if f.open_end => match middle.pop() {
                    Some(Block::Para(p)) => Some(p.inlines),
                    _ => None,
                },
                _ => None,
            };
            let Some(slot) = self.index.para_slot(para) else {
                self.abort(tx);
                return Outcome::Unchanged("no paragraph");
            };
            let ids: Vec<ParaId> = middle.iter().filter_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None }).collect();
            if self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: middle }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("paste failed");
            }
            for p in ids {
                if let Some(q) = self.index.para(&self.doc, p).cloned() {
                    let with = self.fit_inlines(p, &q.inlines);
                    let _ = self.step(&mut tx, Step::Restyle { para: p, inlines: with });
                }
            }
            let mut k = 0;
            if let Some(inl) = last {
                let with = self.fit_inlines(para, &inl);
                k = units(&with);
                if k > 0 && self.step(&mut tx, Step::Text { para, range: Unit(0)..Unit(0), with, edges: Default::default() }).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("paste failed");
                }
            }
            self.selection = Selection::Caret(Position::text(caret.story, para, Unit(k)));
            return self.commit(tx, None, fonts);
        }
        // Split at the caret
        let Some(second) = self.split_template(para) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let tail = ParaId(second.id);
        if self.step(&mut tx, Step::Split { para, at, second, kept: Vec::new() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("split failed");
        }
        let mut middle: Vec<Block> = blocks.clone();
        if let Some(inl) = &first_inlines {
            middle.remove(0);
            let with = self.fit_inlines(para, inl);
            if self.step(&mut tx, Step::Text { para, range: at..at, with, edges: Default::default() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("paste failed");
            }
        }
        let last_open = f.open_end && matches!(middle.last(), Some(Block::Para(_)));
        let last_inlines = if last_open {
            match middle.pop() {
                Some(Block::Para(p)) => Some(p.inlines),
                _ => None,
            }
        } else {
            None
        };
        let Some(slot) = self.index.para_slot(tail) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let ids: Vec<ParaId> = middle.iter().filter_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None }).collect();
        if !middle.is_empty() && self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: middle }).is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("paste failed");
        }
        for p in ids {
            if let Some(q) = self.index.para(&self.doc, p).cloned() {
                let with = self.fit_inlines(p, &q.inlines);
                let _ = self.step(&mut tx, Step::Restyle { para: p, inlines: with });
            }
        }
        let caret_after = match last_inlines {
            Some(inl) => {
                let with = self.fit_inlines(tail, &inl);
                let k = units(&with);
                if self.step(&mut tx, Step::Text { para: tail, range: Unit(0)..Unit(0), with, edges: Default::default() }).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("paste failed");
                }
                Position::text(caret.story, tail, Unit(k))
            }
            None => Position::text(caret.story, tail, Unit(0)),
        };
        self.selection = Selection::Caret(caret_after);
        self.commit(tx, None, fonts)
    }
}

/// Markers whose partner isn't in `blocks` left out: a control's or field's in its paragraph, a bookmark's or editing
/// permission's anywhere in them; and move ranges, which a copy isn't part of.
pub(super) fn drop_unpaired(blocks: &mut [Block]) {
    use crate::docx::review::RangeKind;
    let (mut starts, mut ends): (HashSet<String>, HashSet<String>) = (HashSet::new(), HashSet::new());
    let (mut perm_starts, mut perm_ends): (HashSet<i64>, HashSet<i64>) = (HashSet::new(), HashSet::new());
    let mut any = false;
    each_inline(blocks, &mut |i| match i {
        Inline::Bookmark(n) => {
            any = true;
            match n.strip_prefix('/') {
                Some(n) => ends.insert(n.to_lowercase()),
                None => starts.insert(n.to_lowercase()),
            };
        }
        Inline::Opaque(o) => {
            if let Some((start, id)) = o.permission() {
                any = true;
                if start {
                    perm_starts.insert(id);
                } else {
                    perm_ends.insert(id);
                }
            }
        }
        Inline::Range(r) if !matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => any = true,
        _ => {}
    });
    let keep = |i: &Inline| match i {
        Inline::Bookmark(n) => match n.strip_prefix('/') {
            Some(n) => starts.contains(&n.to_lowercase()),
            None => ends.contains(&n.to_lowercase()),
        },
        Inline::Opaque(o) => o.permission().is_none_or(|(start, id)| if start { perm_ends.contains(&id) } else { perm_starts.contains(&id) }),
        Inline::Range(r) => matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd),
        _ => true,
    };
    drop_unpaired_in(blocks, any.then_some(&keep));
}

fn drop_unpaired_in(blocks: &mut [Block], keep: Option<&dyn Fn(&Inline) -> bool>) {
    use crate::docx::{ControlId, FieldId};
    for b in blocks {
        match b {
            Block::Para(p) => {
                let starts: Vec<ControlId> = p.inlines.iter().filter_map(|i| if let Inline::ControlStart(c) = i { Some(*c) } else { None }).collect();
                let ends: Vec<ControlId> = p.inlines.iter().filter_map(|i| if let Inline::ControlEnd(c) = i { Some(*c) } else { None }).collect();
                let begins: Vec<FieldId> =
                    p.inlines.iter().filter_map(|i| if let Inline::FieldBegin { id, .. } = i { Some(*id) } else { None }).collect();
                let field_ends: Vec<FieldId> =
                    p.inlines.iter().filter_map(|i| if let Inline::FieldEnd { id, .. } = i { Some(*id) } else { None }).collect();
                p.inlines.retain(|i| match i {
                    Inline::ControlStart(c) => ends.contains(c),
                    Inline::ControlEnd(c) => starts.contains(c),
                    Inline::FieldBegin { id, .. } | Inline::FieldSep { id, .. } => field_ends.contains(id),
                    Inline::FieldEnd { id, .. } => begins.contains(id),
                    i => keep.is_none_or(|k| k(i)),
                });
                for i in &mut p.inlines {
                    if let Some(tb) = i.story_mut() {
                        drop_unpaired_in(&mut tb.blocks, keep);
                    }
                }
            }
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| drop_unpaired_in(&mut c.blocks, keep)),
        }
    }
}

pub(super) fn adopt(
    blocks: &mut [Block],
    defs: &std::collections::HashMap<crate::docx::ControlId, crate::docx::Control>,
) -> Vec<(crate::docx::ControlId, crate::docx::Control)> {
    use crate::docx::{ControlId, FieldId};
    use std::collections::HashMap;
    struct Ids {
        fields: HashMap<FieldId, FieldId>,
        controls: HashMap<ControlId, ControlId>,
    }
    impl Ids {
        fn field(&mut self, f: &mut FieldId) {
            *f = *self.fields.entry(*f).or_insert_with(FieldId::next);
        }
        fn control(&mut self, c: &mut ControlId) {
            *c = *self.controls.entry(*c).or_insert_with(ControlId::next);
        }
    }
    fn walk(blocks: &mut [Block], ids: &mut Ids) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    p.controls.iter_mut().for_each(|c| ids.control(c));
                    for i in &mut p.inlines {
                        match i {
                            Inline::FieldBegin { id, .. } | Inline::FieldSep { id, .. } | Inline::FieldEnd { id, .. } => ids.field(id),
                            Inline::ControlStart(c) | Inline::ControlEnd(c) | Inline::Sym { control: Some(c), .. } => ids.control(c),
                            i if i.story().is_some() => {
                                if let Some(tb) = i.story_mut() {
                                    walk(&mut tb.blocks, ids);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => {
                    t.controls.iter_mut().for_each(|c| ids.control(c));
                    for r in &mut t.rows {
                        r.controls.iter_mut().for_each(|c| ids.control(c));
                        for c in &mut r.cells {
                            c.controls.iter_mut().for_each(|k| ids.control(k));
                            walk(&mut c.blocks, ids);
                        }
                    }
                }
            }
        }
    }
    let mut ids = Ids { fields: HashMap::new(), controls: HashMap::new() };
    walk(blocks, &mut ids);
    let mut out: Vec<(ControlId, crate::docx::Control)> = ids
        .controls
        .iter()
        .filter_map(|(old, new)| {
            let mut c = defs.get(old)?.clone();
            c.props.rest.retain(|(n, _)| n != "id");
            Some((*new, crate::docx::Control::new(c.props)))
        })
        .collect();
    out.sort_by_key(|(id, _)| *id);
    out
}

pub(super) fn controls_of(blocks: &[Block], doc: &crate::docx::Document) -> std::collections::HashMap<crate::docx::ControlId, crate::docx::Control> {
    fn ids(blocks: &[Block], out: &mut Vec<crate::docx::ControlId>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    out.extend(p.controls.iter().copied());
                    for i in &p.inlines {
                        match i {
                            Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => out.push(*c),
                            i if i.story().is_some() => {
                                if let Some((_, tb)) = i.story() {
                                    ids(&tb.blocks, out);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => {
                    out.extend(t.controls.iter().copied());
                    for r in &t.rows {
                        out.extend(r.controls.iter().copied());
                        for c in &r.cells {
                            out.extend(c.controls.iter().copied());
                            ids(&c.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut v = Vec::new();
    ids(blocks, &mut v);
    v.into_iter().filter_map(|c| Some((c, doc.controls.get(&c)?.clone()))).collect()
}

pub(super) fn drop_partial(blocks: &mut [Block], index: &super::index::DocIndex) {
    use super::index::Loc;
    use crate::docx::ElementId;
    use crate::layout::TableId;
    fn ids(blocks: &[Block], paras: &mut HashSet<ParaId>, tables: &mut HashSet<TableId>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    paras.insert(ParaId(p.id));
                }
                Block::Table(t) => {
                    tables.insert(TableId(t.id));
                    t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| ids(&c.blocks, paras, tables));
                }
            }
        }
    }
    let (mut paras, mut tables) = (HashSet::new(), HashSet::new());
    ids(blocks, &mut paras, &mut tables);
    let whole = |c: &crate::docx::ControlId| {
        index.element(ElementId::Control(*c)).is_some_and(|locs| {
            locs.iter().all(|l| match l {
                Loc::Para(p) => paras.contains(p),
                Loc::Table(t) | Loc::Row(t, _) | Loc::Cell(t, _, _) => tables.contains(t),
            })
        })
    };
    fn walk(blocks: &mut [Block], keep: &dyn Fn(&crate::docx::ControlId) -> bool) {
        for b in blocks {
            match b {
                Block::Para(p) => p.controls.retain(|c| keep(c)),
                Block::Table(t) => {
                    t.controls.retain(|c| keep(c));
                    t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| walk(&mut c.blocks, keep));
                }
            }
        }
    }
    walk(blocks, &whole);
}

/// The highest id given so far to notes of `kind`.
fn map_max(map: &HashMap<(crate::docx::NoteKind, i64), i64>, kind: crate::docx::NoteKind) -> i64 {
    map.iter().filter(|(k, _)| k.0 == kind).map(|(_, v)| *v).max().unwrap_or(0)
}

/// A permission marker's XML with its id `to`.
fn with_perm_id(x: &str, to: i64) -> Option<String> {
    let at = x.find(":id=\"").map(|k| k + 5).or_else(|| x.find(" id=\"").map(|k| k + 5))?;
    let end = at + x[at..].find('"')?;
    Some(format!("{}{to}{}", &x[..at], &x[end..]))
}
