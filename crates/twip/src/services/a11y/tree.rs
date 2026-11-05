//! The document's reading tree: the body in order (headings, paragraphs, lists with their items, tables with their
//! rows, header cells and cells, text boxes at their anchors), then the regions a screen reader reaches without
//! interrupting the body (headers and footers, footnotes, endnotes, comments). A paragraph's text is split into text
//! nodes where its marks (insertions, deletions, misspellings), their author, its language or the comment it is in
//! change; links, note references and pictures sit in it where they are.

use std::hash::{DefaultHasher, Hash, Hasher};

use crate::api::services::{A11yChange, A11yUpdate, AccessibleId, AccessibleNode, AccessibleRole, CellPlace, ListPosition};
use crate::docx::review::CommentId;
use crate::docx::{Block, Document, Inline, NoteKind, Paragraph, Table, VMerge};
use crate::editing::Editor;
use crate::editing::table::Grid;
use crate::layout::ParaId;

use super::super::spelling::Spellers;
use super::{Reading, TextMarks, author, document_language};

/// What a derived id names, so ids of different things never meet.
#[derive(Clone, Copy)]
enum Kind {
    Root,
    Table,
    Row,
    Cell,
    List,
    Text,
    Link,
    Image,
    Note,
    Comment,
    Story,
    Region,
    SubTemplate,
}

/// The id of a node that is not a paragraph, float or inline shape (those use their own ids, from one process
/// counter): from `kind`, `of` and `n`, the same for the same thing in every build of the tree. Always in
/// `2^52..2^53`: above the counter's ids, and exact as a JavaScript number.
fn derived(kind: Kind, of: u64, n: u64) -> AccessibleId {
    let mut h = DefaultHasher::new();
    (kind as u8, of, n).hash(&mut h);
    AccessibleId(1 << 52 | (h.finish() & ((1 << 52) - 1)))
}

/// The id commented text points at: its thread's first comment.
fn comment_id(c: CommentId) -> AccessibleId {
    derived(Kind::Comment, c.0 as u64, 0)
}

/// The id of note `id` of `kind`.
fn note_id(kind: NoteKind, id: i64) -> AccessibleId {
    derived(Kind::Note, matches!(kind, NoteKind::End) as u64, id as u64)
}

/// The document's reading tree. `spelling` marks misspelled words (checked in `language` where their runs name
/// none); `language` is also the document's language when its defaults name none.
pub fn tree(e: &Editor, spelling: &Spellers, language: &str) -> AccessibleNode {
    tree_with(e, spelling, language, &mut TreeCache::default())
}

/// [`tree`], reusing the paragraphs `cache` read last time when their reading can't have changed.
pub fn tree_with(e: &Editor, spelling: &Spellers, language: &str, cache: &mut TreeCache) -> AccessibleNode {
    let doc = e.document();
    let lang = document_language(doc, language);
    let context = {
        let mut h = DefaultHasher::new();
        (&lang, spelling.signature(), std::mem::discriminant(&doc.review.view), doc.comments.by_id.len()).hash(&mut h);
        h.finish()
    };
    let mut b = Builder { e, doc, spelling, language, lang, open: Vec::new(), notes: Vec::new(), cache, context, seen: Vec::new() };
    let mut children: Vec<AccessibleNode> = doc.sections.iter().flat_map(|s| b.blocks(&s.blocks, 0)).collect();
    children.extend(b.regions());
    // Paragraphs that are gone go from the cache.
    let seen: std::collections::HashSet<u64> = std::mem::take(&mut b.seen).into_iter().collect();
    b.cache.paras.retain(|id, _| seen.contains(id));
    AccessibleNode { id: derived(Kind::Root, 0, 0), role: AccessibleRole::Document, lang: Some(b.lang), children, ..AccessibleNode::default() }
}

/// The paragraphs a tree build read, kept for the next build: a paragraph is read again only when its reading could
/// differ (its text, its runs' language, review marks and hidden text, its pictures' alternative text and fields'
/// values, the comments open at its start, the notes before it, the markup view, the spelling).
#[derive(Default)]
pub struct TreeCache {
    paras: std::collections::HashMap<u64, Read>,
}

/// What reading a paragraph gave, and what it was read with.
struct Read {
    key: u64,
    children: Vec<AccessibleNode>,
    floats: Vec<AccessibleNode>,
    /// The notes it referenced first, in order.
    notes: Vec<(NoteKind, i64)>,
    /// The comments open after it.
    open: Vec<CommentId>,
}

/// What a paragraph's reading depends on, besides the build's context.
fn fingerprint(p: &Paragraph, open: &[CommentId], notes: usize, context: u64) -> u64 {
    let mut h = DefaultHasher::new();
    (p.id, open, notes, context).hash(&mut h);
    for i in &p.inlines {
        std::mem::discriminant(i).hash(&mut h);
        i.units().hash(&mut h);
        if let Some(r) = i.run() {
            (&r.lang, r.hidden, r.rev.ins.as_ref().map(|m| m.author), r.rev.del.as_ref().map(|m| m.author)).hash(&mut h);
        }
        match i {
            Inline::Text(s, _) => s.hash(&mut h),
            Inline::Sym { ch, .. } => ch.hash(&mut h),
            Inline::Picture(pic, _) => (&*pic.alt, pic.shape.as_ref().is_some_and(|s| s.nv.decorative)).hash(&mut h),
            Inline::NoteRef { kind, id, .. } => (matches!(kind, NoteKind::End), id).hash(&mut h),
            Inline::FieldBegin { view, value, .. } => (std::mem::discriminant(view), value).hash(&mut h),
            Inline::Range(m) => (std::mem::discriminant(&m.kind), m.id).hash(&mut h),
            // The rest read the same whatever else they hold.
            _ => {}
        }
    }
    h.finish()
}

/// What changed from `old` to `new`, as replaced top-level nodes and the new order: `None` when nothing did. A
/// change to the root itself (its language) sends the whole tree.
pub fn diff(old: &AccessibleNode, new: &AccessibleNode) -> Option<A11yUpdate> {
    if (old.id, &old.lang, &old.name) != (new.id, &new.lang, &new.name) {
        return Some(A11yUpdate::Full(Box::new(new.clone())));
    }
    let before: std::collections::HashMap<AccessibleId, &AccessibleNode> = old.children.iter().map(|c| (c.id, c)).collect();
    let mut changes: Vec<A11yChange> =
        new.children.iter().filter(|n| before.get(&n.id) != Some(n)).map(|n| A11yChange::Replace { node: Box::new(n.clone()) }).collect();
    if !old.children.iter().map(|c| c.id).eq(new.children.iter().map(|c| c.id)) {
        changes.push(A11yChange::Children { parent: new.id, ids: new.children.iter().map(|c| c.id).collect() });
    }
    (!changes.is_empty()).then_some(A11yUpdate::Patch(changes))
}

/// Where a caret goes to start editing at node `id`: the start of its paragraph (a paragraph, heading or list item
/// itself, the one a text node is in, or the first one in a table, cell, list, text box, note or story), a note
/// reference's note, a comment's commented text. `None`: no node has the id; `Some(None)`: it holds no caret
/// position (the document, a region).
pub(crate) fn caret_para(tree: &AccessibleNode, id: AccessibleId) -> Option<Option<ParaId>> {
    fn is_para(n: &AccessibleNode) -> bool {
        matches!(n.role, AccessibleRole::Paragraph | AccessibleRole::Heading | AccessibleRole::ListItem)
    }
    fn first_para(n: &AccessibleNode) -> Option<ParaId> {
        if is_para(n) { Some(ParaId(n.id.0)) } else { n.children.iter().find_map(first_para) }
    }
    /// The nodes from `n` down to the first one `pick` takes.
    fn path<'a>(n: &'a AccessibleNode, pick: &impl Fn(&AccessibleNode) -> bool, out: &mut Vec<&'a AccessibleNode>) -> bool {
        out.push(n);
        if pick(n) || n.children.iter().any(|c| path(c, pick, out)) {
            return true;
        }
        out.pop();
        false
    }
    /// The paragraph at the end of `p`: the deepest one on it, else the first inside its last node.
    fn para_of(p: &[&AccessibleNode]) -> Option<ParaId> {
        p.iter().rev().find(|n| is_para(n)).map(|n| ParaId(n.id.0)).or_else(|| p.last().and_then(|n| first_para(n)))
    }
    let mut to = Vec::new();
    if !path(tree, &|n| n.id == id, &mut to) {
        return None;
    }
    let node = *to.last()?;
    // In a sub-template body shown in place: its reference's paragraph.
    if let Some(t) = to.iter().find_map(|n| (n.role == AccessibleRole::TextBox).then_some(n.target).flatten()) {
        return Some(Some(ParaId(t.0)));
    }
    Some(match node.role {
        AccessibleRole::Document | AccessibleRole::Region => None,
        AccessibleRole::NoteRef => {
            let mut note = Vec::new();
            node.target.filter(|t| path(tree, &|n| n.id == *t, &mut note)).and_then(|_| para_of(&note))
        }
        AccessibleRole::Comment => {
            let mut anchor = Vec::new();
            path(tree, &|n| n.details == Some(node.id), &mut anchor).then(|| para_of(&anchor)).flatten()
        }
        _ => para_of(&to),
    })
}

/// Applies `update` to `tree`, as a host does.
#[cfg(test)]
pub(crate) fn apply(tree: &mut AccessibleNode, update: A11yUpdate) {
    fn find(n: &mut AccessibleNode, id: AccessibleId) -> Option<&mut AccessibleNode> {
        if n.id == id {
            return Some(n);
        }
        n.children.iter_mut().find_map(|c| find(c, id))
    }
    match update {
        A11yUpdate::Full(t) => *tree = *t,
        A11yUpdate::Patch(changes) => {
            let mut detached = Vec::new();
            for c in changes {
                match c {
                    A11yChange::Replace { node } => match find(tree, node.id) {
                        Some(n) => *n = *node,
                        None => detached.push(*node),
                    },
                    A11yChange::Children { parent, ids } => {
                        let p = find(tree, parent).expect("the parent is in the tree");
                        let mut kids: Vec<AccessibleNode> = std::mem::take(&mut p.children);
                        kids.append(&mut detached);
                        p.children = ids
                            .iter()
                            .map(|id| kids.iter().position(|k| k.id == *id).map(|i| kids.swap_remove(i)).expect("every id was sent"))
                            .collect();
                    }
                }
            }
        }
    }
}

/// What a run of text is: the text nodes split where this changes.
#[derive(Clone, Default, PartialEq)]
struct Attrs {
    text: TextMarks,
    /// The hyperlink field it is in: its index among the paragraph's fields.
    link: Option<usize>,
}

struct Builder<'a> {
    cache: &'a mut TreeCache,
    /// The build's context for [`fingerprint`]: the language, spelling, markup view and comments.
    context: u64,
    /// The paragraphs this build read or reused.
    seen: Vec<u64>,
    e: &'a Editor,
    doc: &'a Document,
    spelling: &'a Spellers,
    language: &'a str,
    /// The document's language: text in another one says so.
    lang: String,
    /// The comments whose ranges are open, followed through the body.
    open: Vec<CommentId>,
    /// The notes referenced so far, in reference order: their numbers.
    notes: Vec<(NoteKind, i64)>,
}

impl Builder<'_> {
    /// The reading nodes of `list` at table nesting `depth`: consecutive list paragraphs become a list, nested by
    /// level; a heading is never a list item.
    fn blocks(&mut self, list: &[Block], depth: usize) -> Vec<AccessibleNode> {
        let mut out = Vec::new();
        // The lists being built: (level, list), outermost first.
        let mut lists: Vec<(i64, AccessibleNode)> = Vec::new();
        for b in list {
            match b {
                Block::Para(p) if p.style.num.is_some() && heading_level(p).is_none() => {
                    let level = p.style.num.map_or(0, |n| n.1).max(0);
                    let (children, floats) = self.inlines(p, depth);
                    if children.is_empty() && floats.is_empty() {
                        continue;
                    }
                    while lists.last().is_some_and(|l| l.0 > level) {
                        close(&mut lists, &mut out);
                    }
                    if lists.last().is_none_or(|l| l.0 < level) {
                        let id = derived(Kind::List, p.id, 0);
                        lists.push((
                            level,
                            AccessibleNode { id, role: AccessibleRole::List, level: Some(level as u32 + 1), ..AccessibleNode::default() },
                        ));
                    }
                    let label = list_label(p);
                    let item = AccessibleNode {
                        id: AccessibleId(p.id),
                        role: AccessibleRole::ListItem,
                        level: Some(level as u32 + 1),
                        label,
                        children: children.into_iter().chain(floats).collect(),
                        ..AccessibleNode::default()
                    };
                    lists.last_mut().expect("a list was pushed").1.children.push(item);
                }
                Block::Para(p) => {
                    while !lists.is_empty() {
                        close(&mut lists, &mut out);
                    }
                    if let Some(node) = self.shown_body(p) {
                        out.push(node);
                        continue;
                    }
                    let (children, floats) = self.inlines(p, depth);
                    if !children.is_empty() {
                        let level = heading_level(p);
                        let role = if level.is_some() { AccessibleRole::Heading } else { AccessibleRole::Paragraph };
                        out.push(AccessibleNode { id: AccessibleId(p.id), role, level, children, ..AccessibleNode::default() });
                    }
                    out.extend(floats);
                }
                Block::Table(t) => {
                    while !lists.is_empty() {
                        close(&mut lists, &mut out);
                    }
                    let node = self.table(t, depth + 1);
                    out.push(node);
                }
            }
        }
        while !lists.is_empty() {
            close(&mut lists, &mut out);
        }
        out
    }

    /// The sub-template body shown in place of the reference alone in `p`, as the screen sees it: a read-only group
    /// named for the sub-template, with the body's paragraphs. Its ids come from the reference, so a body shown at
    /// several references never repeats one.
    // ponytail: the body's paragraphs as text, without their headings, lists, tables or nested bodies, and a body
    // shown as a table row reads as the reference; read the body with `blocks` (ids made per reference) if users need
    // its structure.
    fn shown_body(&self, p: &Paragraph) -> Option<AccessibleNode> {
        let (field, r) = self.e.references().by_field.iter().find(|(f, r)| {
            r.at == ParaId(p.id)
                && r.row.is_none()
                && r.resolution == crate::editing::sub_templates::Resolution::Resolved
                && self.e.reference_expanded(**f)
        })?;
        let body = &self.doc.glossary.as_ref()?.parts.get(r.part? as usize)?.body;
        fn texts(list: &[Block], out: &mut Vec<String>) {
            for b in list {
                match b {
                    Block::Para(p) => out.push(crate::docx::fields::text(p)),
                    Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| texts(&c.blocks, out)),
                }
            }
        }
        let mut lines = Vec::new();
        texts(body, &mut lines);
        let id = derived(Kind::SubTemplate, field.0, 0);
        let children = lines
            .into_iter()
            .filter(|t| !t.trim().is_empty())
            .enumerate()
            .map(|(i, text)| AccessibleNode {
                id: derived(Kind::SubTemplate, field.0, i as u64 + 1),
                role: AccessibleRole::Paragraph,
                children: vec![AccessibleNode {
                    id: derived(Kind::Text, id.0, i as u64 + 1),
                    role: AccessibleRole::Text,
                    text: Some(text),
                    ..AccessibleNode::default()
                }],
                ..AccessibleNode::default()
            })
            .collect();
        // A group, as a text box is. Its target is the reference's paragraph: where activating it, or anything in it,
        // starts editing (the body itself is read only).
        Some(AccessibleNode {
            id,
            role: AccessibleRole::TextBox,
            name: Some(format!("Sub-template {}, read only", r.name)),
            target: Some(AccessibleId(p.id)),
            children,
            ..AccessibleNode::default()
        })
    }

    /// A table at nesting `level`: its rows, and their cells with their grid places; a header row's cells are column
    /// headers, and a vertical merge's continuation cells are part of the cell above.
    fn table(&mut self, t: &Table, level: usize) -> AccessibleNode {
        let id = derived(Kind::Table, t.id, 0);
        let grid = Grid::of(t);
        let mut rows = Vec::new();
        for (r, row) in t.rows.iter().enumerate() {
            let row_id = derived(Kind::Row, id.0, r as u64);
            let mut cells = Vec::new();
            for (k, cell) in row.cells.iter().enumerate() {
                let (col, span, merge) = grid.rows.get(r).and_then(|g| g.get(k)).copied().unwrap_or((k, 1, VMerge::None));
                if merge == VMerge::Continue {
                    continue;
                }
                let continues = |g: &Vec<(usize, usize, VMerge)>| g.iter().any(|&(c, _, m)| c == col && m == VMerge::Continue);
                let row_span = 1 + grid.rows.iter().skip(r + 1).take_while(|g| continues(g)).count() as u32;
                let role = if row.header { AccessibleRole::ColumnHeader } else { AccessibleRole::Cell };
                cells.push(AccessibleNode {
                    id: derived(Kind::Cell, row_id.0, k as u64),
                    role,
                    cell: Some(CellPlace { row: r as u32 + 1, col: col as u32 + 1, row_span, col_span: span.max(1) as u32 }),
                    children: self.blocks(&cell.blocks, level),
                    ..AccessibleNode::default()
                });
            }
            rows.push(AccessibleNode { id: row_id, role: AccessibleRole::Row, children: cells, ..AccessibleNode::default() });
        }
        // No name: screen readers say "table with 4 rows and 3 columns", and a name of "Table" only repeats it (and
        // keeps Firefox's NVDA on the table instead of its first cell).
        AccessibleNode { id, role: AccessibleRole::Table, level: Some(level as u32), children: rows, ..AccessibleNode::default() }
    }

    /// A paragraph's content, and the floating text boxes and pictures anchored in it (read after it).
    fn inlines(&mut self, p: &Paragraph, depth: usize) -> (Vec<AccessibleNode>, Vec<AccessibleNode>) {
        // A paragraph holding a text box reads the box's paragraphs too: it is always read again.
        let boxed = p.inlines.iter().any(|i| i.story().is_some());
        let key = fingerprint(p, &self.open, self.notes.len(), self.context);
        self.seen.push(p.id);
        if !boxed && let Some(r) = self.cache.paras.get(&p.id).filter(|r| r.key == key) {
            for n in &r.notes {
                if !self.notes.contains(n) {
                    self.notes.push(*n);
                }
            }
            self.open.clone_from(&r.open);
            return (r.children.clone(), r.floats.clone());
        }
        let notes_before = self.notes.len();
        let read = self.read(p, depth);
        if !boxed {
            let r =
                Read { key, children: read.0.clone(), floats: read.1.clone(), notes: self.notes[notes_before..].to_vec(), open: self.open.clone() };
            self.cache.paras.insert(p.id, r);
        }
        read
    }

    /// A paragraph's content and the floats anchored in it, read now.
    fn read(&mut self, p: &Paragraph, depth: usize) -> (Vec<AccessibleNode>, Vec<AccessibleNode>) {
        let doc_lang = self.lang.clone();
        let r = Reading::new(self.e, p, self.spelling, self.language, &doc_lang, &mut self.open);
        // The hyperlink fields' unit ranges.
        let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
        let at = |k: usize| starts.get(k).copied().unwrap_or(r.len());
        let links: Vec<(u32, u32)> = crate::docx::fields::spans(&p.inlines)
            .into_iter()
            .filter(|s| crate::docx::fields::parse(&crate::docx::fields::code_text(&p.inlines, *s)).name.eq_ignore_ascii_case("HYPERLINK"))
            .map(|s| (at(s.begin), at(s.end)))
            .collect();

        let mut out: Vec<AccessibleNode> = Vec::new();
        let mut floats = Vec::new();
        let mut run: Option<(Attrs, String)> = None;
        let mut segments = 0u64;
        let mut flush = |run: &mut Option<(Attrs, String)>, out: &mut Vec<AccessibleNode>| {
            let Some((a, text)) = run.take() else { return };
            segments += 1;
            let node = AccessibleNode {
                id: derived(Kind::Text, p.id, segments),
                role: AccessibleRole::Text,
                text: Some(text),
                marks: a.text.marks,
                author: a.text.author,
                lang: a.text.lang,
                details: a.text.comment.map(comment_id),
                ..AccessibleNode::default()
            };
            match a.link {
                Some(k) => match out.last_mut().filter(|l| l.role == AccessibleRole::Link && l.id == derived(Kind::Link, p.id, k as u64)) {
                    Some(link) => link.children.push(node),
                    None => out.push(AccessibleNode {
                        id: derived(Kind::Link, p.id, k as u64),
                        role: AccessibleRole::Link,
                        children: vec![node],
                        ..AccessibleNode::default()
                    }),
                },
                None => out.push(node),
            }
        };
        for (k, inline) in p.inlines.iter().enumerate() {
            match inline {
                Inline::NoteRef { kind, id, .. } => {
                    flush(&mut run, &mut out);
                    out.push(self.note_ref(*kind, *id, p.id, k));
                }
                Inline::Picture(pic, _) => {
                    flush(&mut run, &mut out);
                    let decorative = pic.shape.as_ref().is_some_and(|s| s.nv.decorative);
                    match &pic.text {
                        Some(t) => out.push(self.text_box(AccessibleId(t.id), &t.text_box.blocks, depth)),
                        None if !decorative && !pic.alt.is_empty() => out.push(image(derived(Kind::Image, p.id, k as u64), &pic.alt)),
                        None => {}
                    }
                }
                Inline::Float(f, _) => {
                    let decorative = f.shape.as_ref().is_some_and(|s| s.nv.decorative);
                    match &f.text_box {
                        Some(tb) => floats.push(self.text_box(AccessibleId(f.id), &tb.blocks, depth)),
                        None if !decorative && !f.alt.is_empty() => floats.push(image(AccessibleId(f.id), &f.alt)),
                        None => {}
                    }
                }
                _ => {
                    for u in starts[k]..starts[k] + inline.units() as u32 {
                        let Some(c) = r.char(u) else { continue };
                        let a = Attrs { text: r.marks(u), link: links.iter().position(|&(b, e)| b <= u && u <= e) };
                        match &mut run {
                            Some((same, text)) if *same == a => text.push(c),
                            _ => {
                                flush(&mut run, &mut out);
                                run = Some((a, c.to_string()));
                            }
                        }
                    }
                }
            }
        }
        flush(&mut run, &mut out);
        trim_end(&mut out);
        (out, floats)
    }

    /// A note reference, named by its note's number in reference order.
    fn note_ref(&mut self, kind: NoteKind, id: i64, para: u64, k: usize) -> AccessibleNode {
        if !self.notes.contains(&(kind, id)) {
            self.notes.push((kind, id));
        }
        let n = self.notes.iter().filter(|x| x.0 == kind).position(|x| x.1 == id).map_or(0, |i| i + 1);
        AccessibleNode {
            id: derived(Kind::Text, para, 1 << 32 | k as u64),
            role: AccessibleRole::NoteRef,
            name: Some(format!("{} {n}", note_word(kind))),
            target: Some(note_id(kind, id)),
            ..AccessibleNode::default()
        }
    }

    fn text_box(&mut self, id: AccessibleId, blocks: &[Block], depth: usize) -> AccessibleNode {
        AccessibleNode {
            id,
            role: AccessibleRole::TextBox,
            name: Some("Text box".into()),
            children: self.blocks(blocks, depth),
            ..AccessibleNode::default()
        }
    }

    /// The regions after the body, each only when it holds something.
    fn regions(&mut self) -> Vec<AccessibleNode> {
        let doc = self.doc;
        // Their content has no comment ranges of the body's open.
        let open = std::mem::take(&mut self.open);
        let mut regions = Vec::new();

        let mut stories = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (s, section) in doc.sections.iter().enumerate() {
            for (role, refs, word) in [(AccessibleRole::Header, &section.headers, "header"), (AccessibleRole::Footer, &section.footers, "footer")] {
                // HfRefs: default, first page, even pages.
                for (kind, name) in ["", "First page ", "Even page "].into_iter().zip(refs.iter()) {
                    let Some(name) = name.as_ref().filter(|n| seen.insert(n.to_string())) else { continue };
                    let Some(blocks) = doc.stories.get(name) else { continue };
                    let index = doc.stories.keys().position(|k| k == name).unwrap_or(0) as u64;
                    let children = self.blocks(blocks, 0);
                    if children.is_empty() {
                        continue;
                    }
                    let label = if kind.is_empty() { capitalize(word) } else { format!("{kind}{word}") };
                    stories.push(AccessibleNode {
                        id: derived(Kind::Story, index, 0),
                        role,
                        name: Some(format!("{label}, section {}", s + 1)),
                        children,
                        ..AccessibleNode::default()
                    });
                }
            }
        }
        regions.push((0, "Headers and footers", stories));

        for (n, kind, title) in [(1, NoteKind::Foot, "Footnotes"), (2, NoteKind::End, "Endnotes")] {
            let mut notes = Vec::new();
            // Notes can reference notes: the list grows while it is read.
            let mut i = 0;
            while let Some(&(k, id)) = self.notes.get(i) {
                i += 1;
                if k != kind {
                    continue;
                }
                let store = if kind == NoteKind::Foot { &doc.footnotes } else { &doc.endnotes };
                let Some(blocks) = store.get(&id) else { continue };
                let number = self.notes[..i].iter().filter(|x| x.0 == kind).count();
                notes.push(AccessibleNode {
                    id: note_id(kind, id),
                    role: AccessibleRole::Note,
                    name: Some(format!("{} {number}", note_word(kind))),
                    children: self.blocks(blocks, 0),
                    ..AccessibleNode::default()
                });
            }
            regions.push((n, title, notes));
        }

        let comments = doc
            .comments
            .threads()
            .into_iter()
            .filter_map(|(root, replies)| {
                let mut node = self.comment(root)?;
                node.children.extend(replies.into_iter().filter_map(|r| self.comment(r)));
                Some(node)
            })
            .collect();
        regions.push((3, "Comments", comments));

        self.open = open;
        regions
            .into_iter()
            .filter(|r| !r.2.is_empty())
            .map(|(n, name, children)| AccessibleNode {
                id: derived(Kind::Region, n, 0),
                role: AccessibleRole::Region,
                name: Some(name.into()),
                children,
                ..AccessibleNode::default()
            })
            .collect()
    }

    /// A comment: its author and text.
    fn comment(&mut self, id: CommentId) -> Option<AccessibleNode> {
        let c = self.doc.comments.by_id.get(&id)?;
        let who = author(self.doc, c.author).to_string();
        Some(AccessibleNode {
            id: comment_id(id),
            role: AccessibleRole::Comment,
            name: Some(format!("Comment by {who}")),
            author: Some(who),
            children: self.blocks(&c.body, 0),
            ..AccessibleNode::default()
        })
    }
}

/// A numbered list item's number as shown; a bullet is "•" whatever its glyph (often a symbol font's private
/// character), which screen readers say as "bullet", as Word's are.
pub(super) fn list_label(p: &Paragraph) -> Option<String> {
    let l = p.list.as_ref()?;
    if p.style.list_fmt.as_deref() == Some("bullet") {
        return Some("•".into());
    }
    Some(l.text.trim().to_string()).filter(|t| !t.is_empty())
}

/// A heading's level, 1 to 9, from its outline level.
pub(super) fn heading_level(p: &Paragraph) -> Option<u32> {
    p.style.outline_lvl.filter(|l| (0..9).contains(l)).map(|l| l as u32 + 1)
}

/// Ends the innermost list being built: its items get their positions, and it goes into the last item of the list
/// around it, or into `out`.
fn close(lists: &mut Vec<(i64, AccessibleNode)>, out: &mut Vec<AccessibleNode>) {
    let Some((_, mut list)) = lists.pop() else { return };
    let count = list.children.len() as u32;
    for (i, item) in list.children.iter_mut().enumerate() {
        item.position = Some(ListPosition { index: i as u32 + 1, count });
    }
    match lists.last_mut().and_then(|l| l.1.children.last_mut()) {
        Some(item) => item.children.push(list),
        None => out.push(list),
    }
}

fn image(id: AccessibleId, alt: &str) -> AccessibleNode {
    AccessibleNode { id, role: AccessibleRole::Image, name: Some(alt.to_string()), ..AccessibleNode::default() }
}

fn note_word(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::Foot => "Footnote",
        NoteKind::End => "Endnote",
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Drops trailing white space from the last text nodes (inside a trailing link too), and the nodes it empties.
fn trim_end(out: &mut Vec<AccessibleNode>) {
    while let Some(last) = out.last_mut() {
        match last.role {
            AccessibleRole::Text => {
                let text = last.text.as_deref().unwrap_or("").trim_end().to_string();
                if text.is_empty() {
                    out.pop();
                    continue;
                }
                last.text = Some(text);
            }
            AccessibleRole::Link => {
                trim_end(&mut last.children);
                if last.children.is_empty() {
                    out.pop();
                    continue;
                }
            }
            _ => {}
        }
        break;
    }
    // A paragraph of white space only.
    if out.iter().all(|n| n.role == AccessibleRole::Text && n.text.as_deref().is_some_and(|t| t.trim().is_empty())) {
        out.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::services::AccessibleMark;
    use crate::editing::keymap::Mods;
    use crate::font::Fonts;

    fn open(name: &str) -> (Editor, Fonts) {
        let fonts = Fonts::system();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
        let e = Editor::open(&std::fs::read(path).expect("fixture"), &fonts).expect("opens");
        (e, fonts)
    }

    fn spellers() -> Spellers {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictionaries");
        let mut s = Spellers::default();
        if let (Ok(aff), Ok(dic)) = (std::fs::read(d.join("en_US.aff")), std::fs::read(d.join("en_US.dic"))) {
            s.add_dictionary("en-US", &aff, &dic).expect("loads");
        }
        s
    }

    fn all(n: &AccessibleNode) -> Vec<&AccessibleNode> {
        std::iter::once(n).chain(n.children.iter().flat_map(all)).collect()
    }

    fn with(n: &AccessibleNode, role: AccessibleRole) -> Vec<&AccessibleNode> {
        all(n).into_iter().filter(|x| x.role == role).collect()
    }

    /// A node's text: its text nodes joined.
    fn text(n: &AccessibleNode) -> String {
        all(n).iter().filter_map(|x| x.text.as_deref()).collect()
    }

    #[test]
    fn ids_are_unique() {
        for name in ["FX-A11Y-STRUCTURE.docx", "FX-NEST-DEEP.docx", "FX-STORIES.docx"] {
            let (e, _) = open(name);
            let t = tree(&e, &Spellers::default(), "en-US");
            let mut seen = std::collections::HashSet::new();
            for n in all(&t) {
                assert!(seen.insert(n.id), "{name}: {:?} {:?} repeats id {:?}", n.role, n.name, n.id);
                assert!(n.id.0 < 1 << 53, "{name}: ids are exact as JavaScript numbers");
            }
        }
    }

    #[test]
    fn structure_is_what_a_screen_reader_navigates() {
        let (e, _) = open("FX-A11Y-STRUCTURE.docx");
        let t = tree(&e, &Spellers::default(), "en-US");
        assert_eq!(t.lang.as_deref(), Some("en-US"));

        let headings: Vec<_> = with(&t, AccessibleRole::Heading).iter().map(|h| (h.level, text(h))).collect();
        assert_eq!(headings[..3], [(Some(1), "Introduction".into()), (Some(2), "Lists".into()), (Some(2), "Tables".into())]);
        assert!(headings.iter().any(|h| h.0 == Some(3)), "a level 3 heading: {headings:?}");

        // The numbered list: three items, numbered, with the bullets nested in the second.
        let lists = with(&t, AccessibleRole::List);
        let numbered = lists.iter().find(|l| l.level == Some(1)).expect("a top-level list");
        let items: Vec<_> = numbered.children.iter().map(|i| (i.position.map(|p| (p.index, p.count)), i.label.clone())).collect();
        assert_eq!(items, [(Some((1, 3)), Some("1.".into())), (Some((2, 3)), Some("2.".into())), (Some((3, 3)), Some("3.".into()))]);
        let nested = with(numbered, AccessibleRole::List).into_iter().find(|l| l.level == Some(2)).expect("a nested list");
        assert_eq!(nested.children.len(), 2);
        assert!(nested.children.iter().all(|i| i.label.as_deref() == Some("•")), "bullets are said as bullets");
        assert!(numbered.children[1].children.iter().any(|c| c.role == AccessibleRole::List), "nested in the second item");

        // The table: header cells, spans, a nested table.
        let tables = with(&t, AccessibleRole::Table);
        assert!(tables.iter().any(|t| t.level == Some(2)), "a nested table");
        let outer = tables.iter().find(|t| t.level == Some(1)).expect("the table");
        let headers: Vec<String> = with(outer, AccessibleRole::ColumnHeader).iter().map(|h| text(h)).collect();
        assert_eq!(headers[..3], ["Name", "Role", "Office"]);
        let cells = with(outer, AccessibleRole::Cell);
        let north = cells.iter().find(|c| text(c) == "North").expect("the merged cell");
        assert_eq!(north.cell.map(|c| (c.row, c.col, c.row_span)), Some((2, 3, 2)), "merged down over two rows");
        let span = cells.iter().find(|c| text(c).starts_with("Both offices")).expect("the spanning cell");
        assert_eq!(span.cell.map(|c| c.col_span), Some(2));

        // Links, pictures (not the decorative one), the text box after its anchor.
        let links: Vec<String> = with(&t, AccessibleRole::Link).iter().map(|l| text(l)).collect();
        assert_eq!(links, ["agenda", "minutes"]);
        let images: Vec<_> = with(&t, AccessibleRole::Image).iter().map(|i| i.name.clone()).collect();
        assert_eq!(images, [Some("Board seal".to_string())], "the decorative picture is left out");
        let body = &t.children;
        let anchor = body.iter().position(|n| text(n).starts_with("This paragraph anchors a text box")).expect("the anchor");
        assert_eq!(body[anchor + 1].role, AccessibleRole::TextBox);
        assert_eq!(text(&body[anchor + 1]), "Text box paragraph.");

        // Notes: references point at notes in their regions.
        let refs = with(&t, AccessibleRole::NoteRef);
        let notes = with(&t, AccessibleRole::Note);
        let names: Vec<String> = refs.iter().map(|r| r.name.clone().unwrap_or_default()).collect();
        assert_eq!(names, ["Footnote 1", "Footnote 2", "Endnote 1"]);
        for r in &refs {
            assert!(notes.iter().any(|n| Some(n.id) == r.target && n.name == r.name), "{:?} points at its note", r.name);
        }
        assert_eq!(text(notes[0]).trim(), "The first footnote.");

        // The comment: the commented text points at it, and its reply is in it.
        let comments = with(&t, AccessibleRole::Comment);
        let grace = comments.iter().find(|c| c.author.as_deref() == Some("Grace")).expect("Grace's comment");
        assert_eq!(grace.name.as_deref(), Some("Comment by Grace"));
        assert!(grace.children.iter().any(|c| c.role == AccessibleRole::Comment && c.author.as_deref() == Some("Ada")), "the reply");
        let commented = with(&t, AccessibleRole::Text).into_iter().find(|n| n.details == Some(grace.id)).expect("commented text");
        assert_eq!(commented.text.as_deref(), Some("the first Monday"));

        // Regions after the body, in order.
        let regions: Vec<String> = body.iter().filter(|n| n.role == AccessibleRole::Region).map(|r| r.name.clone().unwrap_or_default()).collect();
        assert_eq!(regions, ["Headers and footers", "Footnotes", "Endnotes", "Comments"]);
        let stories: Vec<String> =
            with(&t, AccessibleRole::Header).iter().chain(&with(&t, AccessibleRole::Footer)).map(|s| s.name.clone().unwrap_or_default()).collect();
        assert_eq!(stories, ["Header, section 1", "First page header, section 1", "Footer, section 1"]);

        // Languages: French and Hebrew text say so; the Hebrew is in its stored (logical) order.
        let texts = with(&t, AccessibleRole::Text);
        let french = texts.iter().find(|n| n.text.as_deref().is_some_and(|s| s.starts_with("Bonjour"))).expect("French");
        assert_eq!(french.lang.as_deref(), Some("fr-FR"));
        let hebrew = texts.iter().find(|n| n.lang.as_deref() == Some("he-IL")).expect("Hebrew");
        assert_eq!(hebrew.text.as_deref(), Some("שלום לכולם"));
        assert!(with(&t, AccessibleRole::Paragraph).iter().all(|p| p.lang.is_none()), "language is on text, not paragraphs");
    }

    #[test]
    fn marks_are_on_the_text_they_cover() {
        let (e, _) = open("FX-A11Y-REVIEW.docx");
        let spelling = spellers();
        let t = tree(&e, &spelling, "en-US");
        let texts = with(&t, AccessibleRole::Text);
        let marked = |m: AccessibleMark| -> Vec<(String, Option<String>)> {
            texts.iter().filter(|n| n.marks.contains(&m)).map(|n| (n.text.clone().unwrap_or_default(), n.author.clone())).collect()
        };
        assert_eq!(marked(AccessibleMark::Insertion), [("unanimously ".to_string(), Some("Ada".to_string()))]);
        assert_eq!(marked(AccessibleMark::Deletion), [("for next year ".to_string(), Some("Grace".to_string()))]);
        if spelling.speller("en-US").is_some() {
            // The first five; the dictionary doesn't know some of the filler words after them.
            let words: Vec<String> = marked(AccessibleMark::Misspelling).into_iter().map(|w| w.0).take(5).collect();
            assert_eq!(words, ["recieve", "teh", "adress", "definately", "seperate"]);
        }
    }

    #[test]
    fn going_to_a_node_finds_its_paragraph() {
        let (e, _) = open("FX-A11Y-STRUCTURE.docx");
        let t = tree(&e, &Spellers::default(), "en-US");
        let first = |role| with(&t, role)[0];
        let heading = first(AccessibleRole::Heading);
        assert_eq!(caret_para(&t, heading.id), Some(Some(ParaId(heading.id.0))), "a heading is its paragraph");
        let text = &heading.children[0];
        assert_eq!(caret_para(&t, text.id), Some(Some(ParaId(heading.id.0))), "a text node goes to its paragraph");
        let table = first(AccessibleRole::Table);
        let header = with(table, AccessibleRole::ColumnHeader)[0];
        assert_eq!(caret_para(&t, table.id), Some(Some(ParaId(header.children[0].id.0))), "a table goes to its first cell");
        let note_ref = first(AccessibleRole::NoteRef);
        let note = with(&t, AccessibleRole::Note).into_iter().find(|n| Some(n.id) == note_ref.target).expect("the note");
        assert_eq!(caret_para(&t, note_ref.id), Some(Some(ParaId(note.children[0].id.0))), "a note reference goes to its note");
        let comment = first(AccessibleRole::Comment);
        let anchor = t.children.iter().find(|n| all(n).iter().any(|x| x.details == Some(comment.id))).expect("the commented paragraph");
        assert_eq!(caret_para(&t, comment.id), Some(Some(ParaId(anchor.id.0))), "a comment goes to its commented text");
        assert_eq!(caret_para(&t, first(AccessibleRole::Region).id), Some(None), "a region holds no caret");
        assert_eq!(caret_para(&t, AccessibleId((1 << 53) - 1)), None, "no such node");
    }

    #[test]
    fn patches_rebuild_the_tree() {
        let (mut e, fonts) = open("FX-A11Y-STRUCTURE.docx");
        let spelling = spellers();
        let mut cache = TreeCache::default();
        let first = tree_with(&e, &spelling, "en-US", &mut cache);
        let (mut mirror, mut last) = (first.clone(), first);
        let key = |e: &mut Editor, code: &str, ctrl: bool| {
            let cmd = e.key_command(code, Mods { ctrl, ..Mods::default() }).expect("a command");
            e.execute(cmd, &fonts);
        };
        let mut patched = 0;
        for step in 0..40 {
            match step % 8 {
                0 | 1 => {
                    e.execute(crate::api::command::Command::InsertText("ab".into()), &fonts);
                }
                2 => key(&mut e, "Enter", false),
                3 => key(&mut e, "Backspace", false),
                4 | 5 => key(&mut e, "ArrowDown", false),
                6 => key(&mut e, "Backspace", true),
                _ => key(&mut e, "ArrowDown", true),
            }
            let now = tree_with(&e, &spelling, "en-US", &mut cache);
            assert_eq!(now, tree(&e, &spelling, "en-US"), "after step {step} the cached build is a fresh one");
            if let Some(u) = diff(&last, &now) {
                patched += matches!(u, A11yUpdate::Patch(_)) as usize;
                apply(&mut mirror, u);
            }
            assert_eq!(mirror, now, "after step {step} the patched tree is the full tree");
            last = now;
        }
        assert!(patched > 10, "edits are sent as patches, not whole trees ({patched})");
    }
}
