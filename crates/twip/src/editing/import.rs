//! Pasted content from another document

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::document::paragraph::ListId;
use crate::docx::list_xml::{ListDefDirect, ListDirect};
use crate::docx::para_props::ParaDirect;
use crate::docx::resolvers::{Resolvers, TypedDefs};
use crate::docx::review::{AuthorId, FormatChange, People, RevisionMark, TableChange};
use crate::docx::style_xml::StyleDef;
use crate::docx::style_xml::StyleEditError;
use crate::docx::write::props::Cx;
use crate::docx::{Block, Inline, Opaque, Paragraph, Raw, ResourceId, Resources, Run, RunStyle};
use crate::drawing::paint::Fill;
use crate::drawing::{Shape, ShapeKind};
use crate::format::{LevelProp, ListChoice, ListDef, ListDefId, ListDefProp, ListProp, ParaProp, ParaSet, Style, StyleProp, StyleSet};
use crate::layout::{ParaId, TableId};

use super::clipboard::{Fragment, Refs};
use super::step::Step;
use super::{Editor, Tx};

/// The document a fragment was copied from: what its image ids, style and list ids and authors are there.
#[derive(Debug)]
pub struct Origin {
    /// The document's identity among those open ([`next_key`]).
    pub key: u64,
    /// Each image the fragment shows, by its id there.
    pub images: Resources,
    /// Its styles and lists.
    pub resolver: Option<Rc<crate::docx::resolvers::Resolvers>>,
    /// Its people.
    pub people: People,
    /// Its main part's root start tag, whose namespace declarations its paragraphs' XML is written under (empty for a
    /// document not read from a file).
    pub main: String,
}

impl Origin {
    /// The origin of `f`, read from `doc` (a document no editor has open): its images move into it.
    pub(super) fn of_document(mut doc: crate::docx::Document, f: &mut Fragment) -> Rc<Origin> {
        let images = used_images(f).into_iter().filter_map(|k| Some((k, doc.images.remove(k)?))).collect();
        let main = root_tag(&doc);
        Rc::new(Origin { key: next_key(), images, resolver: doc.resolver, people: doc.people, main })
    }
}

/// The start tag of `doc`'s main part's root, as read.
fn root_tag(doc: &crate::docx::Document) -> String {
    let Some(x) = doc.source.as_ref().map(|s| s.xml.as_str()) else { return String::new() };
    let start = x.char_indices().find(|&(i, c)| c == '<' && x[i + 1..].starts_with(|c: char| c.is_alphabetic())).map_or(0, |(i, _)| i);
    x[start..x[start..].find('>').map_or(start, |e| start + e + 1)].to_string()
}

/// The images `f` (its notes and comments too) shows, in id order.
fn used_images(f: &mut Fragment) -> Vec<ResourceId> {
    let mut used = Vec::new();
    each_block_of(&mut f.blocks, &mut f.refs, &mut |b| each_image(b, &mut |k| used.push(*k)));
    used.sort_unstable();
    used.dedup();
    used
}

/// The lists a paste has brought from another document so far, so its later pieces share them: each `w:num` and
/// `w:abstractNum` id there and the one it took here (0, a list that couldn't come).
#[derive(Default)]
#[cfg_attr(test, derive(Clone, Debug))]
pub(super) struct Lists {
    nums: HashMap<i64, i64>,
    abstracts: HashMap<i64, i64>,
}

/// A new document identity.
pub(super) fn next_key() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Every block in `blocks`, at every depth: table cells and text boxes included.
fn each_block(blocks: &mut [Block], f: &mut dyn FnMut(&mut Block)) {
    for b in blocks {
        f(b);
        match b {
            Block::Para(p) => {
                for i in &mut p.inlines {
                    if let Some(tb) = i.story_mut() {
                        each_block(&mut tb.blocks, f);
                    }
                }
            }
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| each_block(&mut c.blocks, f)),
        }
    }
}

/// [`each_block`] over a fragment's blocks and its notes' and comments' bodies.
fn each_block_of(blocks: &mut [Block], refs: &mut Refs, f: &mut dyn FnMut(&mut Block)) {
    each_block(blocks, f);
    refs.notes.iter_mut().for_each(|n| each_block(&mut n.2, f));
    refs.comments.iter_mut().for_each(|c| each_block(&mut c.body, f));
}

fn fill_image(fill: Option<&mut Fill>, f: &mut dyn FnMut(&mut ResourceId)) {
    if let Some(Fill::Blip(b)) = fill
        && let Some(k) = &mut b.image
    {
        f(k);
    }
}

/// The images of a shape's fills and pictures, its group's children included.
fn shape_images(s: &mut Shape, f: &mut dyn FnMut(&mut ResourceId)) {
    match &mut s.kind {
        ShapeKind::Sp(sp) => {
            fill_image(sp.fill.as_mut(), f);
            fill_image(sp.line.as_mut().and_then(|l| l.fill.as_mut()), f);
        }
        ShapeKind::Pic(p) => {
            if let Some(k) = &mut p.blip.image {
                f(k);
            }
            fill_image(p.line.as_mut().and_then(|l| l.fill.as_mut()), f);
        }
        ShapeKind::Group(g) => {
            fill_image(g.fill.as_mut(), f);
            g.children.iter_mut().for_each(|c| shape_images(c, f));
        }
        ShapeKind::Raw(_) => {}
    }
}

/// Every image the paragraphs of `blocks` hold: pictures', floats' and their parts', and their shapes'.
fn each_image(block: &mut Block, f: &mut dyn FnMut(&mut ResourceId)) {
    let Block::Para(p) = block else { return };
    for i in &mut p.inlines {
        match i {
            Inline::Picture(pic, _) => {
                if let Some(k) = &mut pic.image {
                    f(k);
                }
                if let Some(s) = pic.shape.as_deref_mut() {
                    shape_images(s, f);
                }
            }
            Inline::Float(fl, _) => {
                if let Some(k) = &mut fl.image {
                    f(k);
                }
                for part in &mut fl.parts {
                    if let Some(k) = &mut part.image {
                        f(k);
                    }
                }
                if let Some(s) = fl.shape.as_deref_mut() {
                    shape_images(s, f);
                }
            }
            _ => {}
        }
    }
}

/// Each revision mark of a block's own (its runs', its mark's, its properties' changes; a table's, its rows' and
/// cells'), for `f` to change.
fn each_mark(block: &mut Block, f: &mut dyn FnMut(&mut RevisionMark)) {
    let mut mark = |m: &mut Rc<RevisionMark>| f(Rc::make_mut(m));
    let run = |r: &mut crate::docx::Run, mark: &mut dyn FnMut(&mut Rc<RevisionMark>)| {
        if r.rev.is_empty() {
            return;
        }
        let s = Rc::make_mut(r);
        s.rev.ins.iter_mut().chain(s.rev.del.iter_mut()).for_each(&mut *mark);
        if let Some(fc) = &mut s.rev.fmt {
            mark(&mut Rc::make_mut(fc).mark);
        }
    };
    match block {
        Block::Para(p) => {
            for i in &mut p.inlines {
                if let Some(r) = i.run() {
                    let mut r = r.clone();
                    let was = Rc::as_ptr(&r);
                    run(&mut r, &mut mark);
                    if Rc::as_ptr(&r) != was {
                        *i = super::format::with_run(i, &r);
                    }
                }
            }
            run(&mut p.mark, &mut mark);
            if p.style.change.is_some()
                && let Some(c) = &mut Rc::make_mut(&mut p.style).change
            {
                mark(&mut Rc::make_mut(c).mark);
            }
        }
        Block::Table(t) => {
            if let Some(c) = &mut t.change {
                mark(&mut Rc::make_mut(c).mark);
            }
            if let Some(c) = &mut t.grid_change {
                mark(&mut Rc::make_mut(c).mark);
            }
            for r in &mut t.rows {
                r.rev.ins.iter_mut().chain(r.rev.del.iter_mut()).for_each(&mut mark);
                for c in r.rev.change.iter_mut().chain(r.rev.ex_change.iter_mut()) {
                    mark(&mut Rc::make_mut(c).mark);
                }
                for cell in &mut r.cells {
                    if let Some((m, _)) = &mut cell.rev.structural {
                        mark(m);
                    }
                    if let Some(c) = &mut cell.rev.change {
                        mark(&mut Rc::make_mut(c).mark);
                    }
                }
            }
        }
    }
}

/// Pasted `blocks` and the bodies of the notes and comments they refer to, from another document, as written in the
/// part `cx` describes: their runs' `w:rPr` as read kept where they read the same there, else written from their
/// properties.
pub(super) fn write_runs_here(blocks: &mut [Block], refs: &mut Refs, cx: &Cx) {
    let mut seen = HashMap::new();
    each_block_of(blocks, refs, &mut |b| {
        if let Block::Para(p) = b {
            p.mark = written_here(&p.mark, cx, &mut seen);
            for i in &mut p.inlines {
                if let Some(r) = i.run() {
                    let here = written_here(r, cx, &mut seen);
                    if !Rc::ptr_eq(&here, r) {
                        *i = super::format::with_run(i, &here);
                    }
                }
            }
        }
    });
}

/// What another document's stored XML (written under its main part's root) needs to read the same under this one's:
/// the bindings that root gives and this one's doesn't, as prefix (empty: the default namespace) and URI.
pub(super) struct Rebind(Vec<(String, String)>);

impl Rebind {
    pub(super) fn between(from: &Cx, to: &Cx) -> Rebind {
        let bindings = |cx: &Cx| {
            cx.parse("<y/>", |_, el| {
                el.namespaces().filter(|n| n.name() != Some("xml")).map(|n| (n.name().unwrap_or_default().to_string(), n.uri().to_string())).collect()
            })
            .unwrap_or_default()
        };
        let have: Vec<(String, String)> = bindings(to);
        Rebind(bindings(from).into_iter().filter(|b| !have.contains(b)).collect())
    }

    /// Whether no stored XML needs a binding.
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `raw`, one element, declaring on itself each binding its names (or a markup compatibility attribute's value)
    /// use that it doesn't declare there; `None` when it needs none.
    fn element(&self, raw: &str) -> Option<String> {
        let at = raw.find(|c: char| c.is_whitespace() || c == '/' || c == '>')?;
        let head = &raw[..raw.find('>').unwrap_or(raw.len())];
        let mut decls = String::new();
        for (p, uri) in &self.0 {
            let (used, decl) = if p.is_empty() { (unprefixed(raw), "xmlns".to_string()) } else { (names(raw, p), format!("xmlns:{p}")) };
            if used && !head.match_indices(&decl).any(|(i, m)| head[i + m.len()..].trim_start().starts_with('=')) {
                decls.push_str(&format!(" {decl}=\"{}\"", crate::docx::write::props::esc(uri)));
            }
        }
        (!decls.is_empty()).then(|| format!("{}{decls}{}", &raw[..at], &raw[at..]))
    }

    /// Authored paragraph properties, the `w:pPr` they were read from declaring what it needs here.
    fn direct(&self, d: &mut ParaDirect) {
        if let Some(x) = d.origin_xml().and_then(|x| self.element(x)) {
            *d = d.with_origin_xml(x.into());
        }
    }

    fn raw(&self, r: &mut Raw) {
        if let Some(x) = r.0.as_deref().and_then(|x| self.element(x)) {
            r.0 = Some(x.into());
        }
    }

    fn string(&self, s: &mut String) {
        if let Some(x) = self.element(s) {
            *s = x;
        }
    }

    /// A pasted content control's properties.
    pub(super) fn control(&self, p: &mut crate::docx::ControlProps) {
        let strings = p.placeholder.iter_mut().chain(&mut p.binding).chain(&mut p.kind_xml).chain(p.rest.iter_mut().map(|(_, x)| x));
        strings.for_each(|x| self.string(x));
    }

    /// Pasted `blocks` and the bodies of the notes and comments they refer to: their paragraphs', tables', rows' and
    /// cells' properties and their tracked changes, and the elements kept as read (rubies, form fields' data, others).
    pub(super) fn blocks(&self, blocks: &mut [Block], refs: &mut Refs) {
        macro_rules! own {
            ($d:expr) => {
                if let Some(x) = $d.origin_xml().and_then(|x| self.element(x)) {
                    $d = $d.with_origin_xml(x.into());
                }
            };
        }
        let change = |c: &mut Option<Rc<TableChange>>| {
            if let Some(c) = c
                && c.previous_direct.origin_xml().is_some_and(|x| self.element(x).is_some())
            {
                own!(Rc::make_mut(c).previous_direct);
            }
        };
        each_block_of(blocks, refs, &mut |b| match b {
            Block::Para(p) => {
                let needs = |r: &Raw| r.0.as_deref().is_some_and(|x| self.element(x).is_some());
                let needs_direct = |d: &ParaDirect| d.origin_xml().is_some_and(|x| self.element(x).is_some());
                let change = p.style.change.as_ref().is_some_and(|c| needs_direct(&c.previous.direct) || needs(&c.numbering_change));
                if needs_direct(&p.style.direct) || change {
                    let s = Rc::make_mut(&mut p.style);
                    self.direct(&mut s.direct);
                    if let Some(c) = &mut s.change {
                        let c = Rc::make_mut(c);
                        self.direct(&mut Rc::make_mut(&mut c.previous).direct);
                        self.raw(&mut c.numbering_change);
                    }
                }
                for i in &mut p.inlines {
                    match i {
                        Inline::Opaque(o) => {
                            if let Some(xml) = o.xml.0.as_deref().and_then(|x| self.element(x)) {
                                *o = Rc::new(Opaque { xml: Raw(Some(xml.into())), field: o.field.clone(), src: o.src.clone(), run: o.run.clone() });
                            }
                        }
                        Inline::Ruby(r, _) if needs(&r.xml) => self.raw(&mut Rc::make_mut(r).xml),
                        Inline::FieldBegin { form: Some(f), .. } if needs(&f.xml) => self.raw(&mut Rc::make_mut(f).xml),
                        _ => {}
                    }
                }
            }
            Block::Table(t) => {
                own!(t.direct);
                change(&mut t.change);
                for r in &mut t.rows {
                    own!(r.direct);
                    own!(r.exceptions);
                    change(&mut r.rev.change);
                    change(&mut r.rev.ex_change);
                    for c in &mut r.cells {
                        own!(c.direct);
                        change(&mut c.rev.change);
                    }
                }
            }
        });
    }
}

/// Whether `raw` has a start tag whose name has no prefix.
fn unprefixed(raw: &str) -> bool {
    raw.match_indices('<').any(|(i, _)| {
        let name = &raw[i + 1..];
        let end = name.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(name.len());
        name.starts_with(|c: char| c.is_alphabetic()) && !name[..end].contains(':')
    })
}

/// Whether `raw` names prefix `p`: in an element or attribute name, or as a word of an attribute value (as markup
/// compatibility's `Ignorable` and `Requires` do).
fn names(raw: &str, p: &str) -> bool {
    let word = |i: usize, len: usize| {
        let before = raw[..i].chars().next_back();
        let after = raw[i + len..].chars().next();
        matches!(before, Some('"' | '\'' | ' ')) && matches!(after, Some('"' | '\'' | ' '))
    };
    raw.match_indices(p).any(|(i, m)| {
        let after = &raw[i + m.len()..];
        let before = raw[..i].chars().next_back();
        (after.starts_with(':') && matches!(before, Some('<' | '/' | ' ' | '\t' | '\n' | '\r'))) || word(i, m.len())
    })
}

/// `run` as written in the part `cx` describes: the `w:rPr` it and its format change's earlier formatting were read
/// from kept where they read the same there, else written from their properties. `seen`: runs done, by identity.
fn written_here(run: &Run, cx: &Cx, seen: &mut HashMap<*const RunStyle, Run>) -> Run {
    if let Some(r) = seen.get(&Rc::as_ptr(run)) {
        return r.clone();
    }
    let direct = run.direct.moved_to(cx);
    let fmt = run.rev.fmt.as_ref().and_then(|f| {
        let direct = f.previous.direct.moved_to(cx)?;
        Some(Rc::new(FormatChange { previous: Rc::new(RunStyle { direct, ..(*f.previous).clone() }), ..(**f).clone() }))
    });
    let out = if direct.is_none() && fmt.is_none() {
        run.clone()
    } else {
        let mut s = (**run).clone();
        if let Some(d) = direct {
            s.direct = d;
        }
        if fmt.is_some() {
            s.rev.fmt = fmt;
        }
        Rc::new(s)
    };
    seen.insert(Rc::as_ptr(run), out.clone());
    out
}

/// `raw`, an element as `cx` writes one, with the edits `f` names for each element in it: a byte range of the text it
/// was parsed from and what replaces it.
fn rewrite(raw: &str, cx: &Cx, f: &dyn Fn(roxmltree::Node) -> Option<(std::ops::Range<usize>, String)>) -> Option<String> {
    let mut edits = cx.parse(raw, |text, el| {
        // The element sits after the wrapper's start tag and before its end tag (`</x>`).
        let off = text.len() - raw.len() - 4;
        el.descendants().filter(|n| n.is_element()).filter_map(f).map(|(r, s)| (r.start - off..r.end - off, s)).collect::<Vec<_>>()
    })?;
    edits.sort_by_key(|e| e.0.start);
    let mut out = String::with_capacity(raw.len());
    let mut at = 0;
    for (r, s) in edits {
        if r.start < at {
            continue;
        }
        out.push_str(&raw[at..r.start]);
        out.push_str(&s);
        at = r.end;
    }
    out.push_str(&raw[at..]);
    Some(out)
}

/// A `w:val`-like attribute of `n` by local name, and its value's range.
fn attr_range(n: roxmltree::Node, name: &str) -> Option<(std::ops::Range<usize>, i64)> {
    let a = n.attributes().find(|a| a.name() == name)?;
    Some((a.range_value(), a.value().trim().parse().ok()?))
}

fn is_w(n: roxmltree::Node, local: &str) -> bool {
    n.tag_name().name() == local && n.tag_name().namespace() == Some(crate::docx::xml::W)
}

/// The `w:numId` values `raw` (a `w:pPr`) names.
fn num_ids(raw: &str, cx: &Cx) -> Vec<i64> {
    cx.parse(raw, |_, el| el.descendants().filter(|n| is_w(*n, "numId")).filter_map(|n| attr_range(n, "val").map(|a| a.1)).collect())
        .unwrap_or_default()
}

/// `raw` with its `w:numId` values mapped.
fn renumber(raw: &str, cx: &Cx, map: &HashMap<i64, i64>) -> Option<String> {
    rewrite(raw, cx, &|n| {
        let (r, v) = attr_range(n, "val").filter(|_| is_w(n, "numId"))?;
        map.get(&v).map(|to| (r, to.to_string()))
    })
}

impl Editor {
    /// Where a fragment copied from this document comes from: this document, and the images it shows.
    pub(super) fn origin_of(&self, f: &mut Fragment) -> Rc<Origin> {
        let images = used_images(f).into_iter().filter_map(|k| Some((k, self.doc.images.get(k)?.to_vec()))).collect();
        Rc::new(Origin { key: self.key, images, resolver: self.doc.resolver.clone(), people: self.doc.people.clone(), main: root_tag(&self.doc) })
    }

    /// Makes pasted `blocks`, and the bodies of the notes and comments they refer to, this document's: the images they
    /// show added (or found), and from another document, its styles and lists they use added under ids free here, its
    /// people found or added, and what they were read from forgotten (another part's XML).
    /// `lists`: those earlier pieces of the same paste brought.
    pub(super) fn import(&mut self, tx: &mut Tx, blocks: &mut [Block], refs: &mut Refs, origin: &Origin, lists: &mut Lists) {
        let mut moved: HashMap<ResourceId, ResourceId> = HashMap::new();
        for (k, data) in origin.images.iter() {
            let to = if self.doc.images.get(k) == Some(data) {
                k
            } else if let Some(j) = self.doc.images.find(data) {
                j
            } else if let Some(j) = self.pending_images.find(data) {
                j
            } else {
                self.pending_image(data.to_vec())
            };
            if to != k {
                moved.insert(k, to);
            }
        }
        if !moved.is_empty() {
            each_block_of(blocks, refs, &mut |b| {
                each_image(b, &mut |k| {
                    if let Some(&to) = moved.get(k) {
                        *k = to;
                    }
                })
            });
        }
        if origin.key == self.key {
            return;
        }
        each_block_of(blocks, refs, &mut |b| {
            if let Block::Para(p) = b {
                for i in &mut p.inlines {
                    match i {
                        Inline::Picture(pic, _) => pic.src = Default::default(),
                        Inline::Float(f, _) => (f.src, f.frame_src) = (None, None),
                        _ => {}
                    }
                }
            }
        });
        self.import_people(blocks, refs, &origin.people);
        if let Some(from) = &origin.resolver {
            let main = (!origin.main.is_empty()).then(|| Cx::of_xml(&origin.main));
            self.import_styles(tx, blocks, refs, from, main.as_ref(), lists);
        }
    }

    /// The authors of pasted comments and revisions, found or added among this document's people.
    fn import_people(&mut self, blocks: &mut [Block], refs: &mut Refs, from: &People) {
        let mut map: HashMap<AuthorId, AuthorId> = HashMap::new();
        let people = &mut self.doc.people;
        let mut to = |a: AuthorId| -> AuthorId {
            *map.entry(a).or_insert_with(|| match from.get(a) {
                Some(p) => {
                    let id = people.author(&p.name, p.identity.as_ref());
                    if let Some(q) = people.persons.get_mut(id.0 as usize)
                        && q.initials.is_none()
                    {
                        q.initials = p.initials.clone();
                    }
                    id
                }
                None => a,
            })
        };
        for c in &mut refs.comments {
            c.author = to(c.author);
        }
        each_block_of(blocks, refs, &mut |b| each_mark(b, &mut |m| m.author = to(m.author)));
    }

    /// The styles and lists `blocks` and `refs` use that this document lacks, brought from `from`: a style and the ones
    /// it is based on, linked to and followed by, a list's definitions under new ids (its paragraphs and styles
    /// renumbered). A style this document has keeps its own definition, as Word's default (destination styles) does.
    /// `main`: how the paragraphs' own XML is written there (`None`: as its styles are).
    fn import_styles(&mut self, tx: &mut Tx, blocks: &mut [Block], refs: &mut Refs, from: &Resolvers, main: Option<&Cx>, brought: &mut Lists) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let theirs = from.typed();
        let scx = theirs.styles.cx();
        let (their_styles, their_lists) = (&theirs.cascade.styles, &theirs.cascade.lists);
        let def_of = |n: i64| their_lists.lists.get(&ListId(n)).map(|l| l.def.map_or(-1, |d| d.0));
        let pcx = main.unwrap_or(&scx);
        let mut want: Vec<String> = Vec::new();
        let mut nums: Vec<i64> = Vec::new();
        let (mut paras, mut tables): (Vec<ParaId>, Vec<TableId>) = (Vec::new(), Vec::new());
        each_block_of(blocks, refs, &mut |b| match b {
            Block::Para(p) => {
                paras.push(ParaId(p.id));
                want.extend(p.style.style_id.iter().cloned());
                want.extend(p.mark.style_id.iter().cloned());
                want.extend(p.inlines.iter().filter_map(|i| i.run()?.style_id.clone()));
                // Theme colours don't change the list ids written.
                if let Some(x) = p.style.direct.xml(&Default::default(), pcx) {
                    nums.extend(num_ids(&x, pcx));
                }
            }
            Block::Table(t) => {
                tables.push(TableId(t.id));
                want.extend(t.style_id.iter().cloned());
            }
        });
        // The styles and lists to bring, following what each refers to.
        let mut styles: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut lists: Vec<i64> = Vec::new();
        while !want.is_empty() || !nums.is_empty() {
            while let Some(id) = want.pop() {
                if res.has_style(&id) || !seen.insert(id.clone()) {
                    continue;
                }
                let Some(s) = their_styles.get(&id) else { continue };
                let based_on = s.based_on.as_ref().filter(|_| !s.word_default_table());
                want.extend([based_on, s.link.as_ref(), s.next.as_ref()].into_iter().flatten().map(|x| x.to_string()));
                nums.extend(style_lists(s));
                styles.push(id);
            }
            while let Some(n) = nums.pop() {
                if n == 0 || lists.contains(&n) || brought.nums.contains_key(&n) {
                    continue;
                }
                let Some(a) = def_of(n) else { continue };
                lists.push(n);
                if let Some(d) = their_lists.defs.get(&ListDefId(a)) {
                    want.extend([&d.num_style_link, &d.style_link].into_iter().flatten().map(|x| x.to_string()));
                }
            }
        }
        if styles.is_empty() && lists.is_empty() {
            renumber_blocks(blocks, refs, pcx, &brought.nums);
            return;
        }
        let mut next = (*res).clone();
        let (ListDefId(mut free_abstract), ListId(mut free_num)) = next.cascade().lists.free_ids();
        for l in &self.doc.added_lists {
            free_abstract = free_abstract.max(l.abstract_id + 1);
            free_num = free_num.max(l.num_id + 1);
        }
        let copy = Copied { theirs: &theirs, styles: &styles, lists: &lists, free: (free_abstract, free_num) };
        #[cfg(test)]
        let was = (next.clone(), brought.clone());
        copy.apply(&mut next, brought);
        #[cfg(test)]
        xml_route::check(&copy, was, &next, brought);
        renumber_blocks(blocks, refs, pcx, &brought.nums);
        let added = self.doc.added_lists.clone();
        if self.step(tx, Step::Resolver { resolver: Some(Rc::new(next)), added }).is_ok() {
            tx.stale_paras.extend(paras);
            tx.stale_tables.extend(tables);
        }
    }
}

/// The lists style `s` (its regions too) puts its paragraphs in, in the order it writes them.
fn style_lists(s: &Style) -> impl Iterator<Item = i64> + '_ {
    std::iter::once(&s.format.para.list).chain(s.regions.iter().map(|r| &r.format.para.list)).filter_map(|l| match l {
        Some(ListChoice::In(n)) => Some(n.0),
        _ => None,
    })
}

/// `list` mapped through `map` (0: no list); whether it changed.
fn renumbered(list: &mut Option<ListChoice>, map: &HashMap<i64, i64>) -> bool {
    let Some(ListChoice::In(n)) = *list else { return false };
    let Some(&to) = map.get(&n.0).filter(|to| **to != n.0) else { return false };
    *list = Some(if to == 0 { ListChoice::None } else { ListChoice::In(ListId(to)) });
    true
}

/// List definition `a` of `theirs` (another document's definitions) to set as definition `id` of a numbering part `cx`
/// describes: its holder there, as read where that reads the same here, else to be written from the definition; and
/// the definition with its id and part, to edit further.
pub(super) fn copied_def(theirs: &TypedDefs, a: i64, id: i64, cx: &Cx) -> Option<(ListDefDirect, ListDef)> {
    let d = theirs.cascade.lists.defs.get(&ListDefId(a))?;
    let held = theirs.numbering.defs.get(&ListDefId(a)).cloned().unwrap_or_else(|| ListDefDirect::typed(d.clone()));
    let held = held.moved_to(cx).unwrap_or(held);
    let mut v = (**held.get()).clone();
    (v.id, v.origin) = (ListDefId(id), crate::format::Origin::Document);
    Some((held, v))
}

/// List `n` of `theirs`, as [`copied_def`], set as list `id` numbering with definition `def`.
pub(super) fn copied_list(theirs: &TypedDefs, n: i64, id: i64, def: i64, cx: &Cx) -> Option<ListDirect> {
    let l = theirs.cascade.lists.lists.get(&ListId(n))?;
    let held = theirs.numbering.lists.get(&ListId(n)).cloned().unwrap_or_else(|| ListDirect::typed(l.clone()));
    let held = held.moved_to(cx).unwrap_or(held);
    let mut v = (**held.get()).clone();
    (v.id, v.origin) = (ListId(id), crate::format::Origin::Document);
    let set: &[ListProp] = if v.def == Some(ListDefId(def)) { &[] } else { &[ListProp::Def] };
    v.def = Some(ListDefId(def));
    Some(held.edited(v, set))
}

/// What a paste copies of another document's definitions: its lists and styles, by their ids there, and the
/// definition and list ids free here from which they take theirs.
struct Copied<'a> {
    theirs: &'a TypedDefs,
    styles: &'a [String],
    lists: &'a [i64],
    free: (i64, i64),
}

impl Copied<'_> {
    /// The lists and styles set in `next`, each definition once: a picture bullet's reference goes (the bullet's
    /// picture isn't brought), so the level shows its text. Each style comes with its lists renumbered, not its
    /// kind's default (this document's stays); one based on itself through its chain comes without its base. Each is
    /// written as read where that reads the same in this document's part, else from its definition. `brought` takes
    /// each list's and definition's id here (a list's 0 when it couldn't come).
    fn apply(&self, next: &mut Resolvers, brought: &mut Lists) {
        let (mut free_abstract, mut free_num) = self.free;
        let ncx = next.typed().numbering.cx();
        let (theirs, their) = (self.theirs, &self.theirs.cascade.lists);
        for &n in self.lists {
            let Some(list) = their.lists.get(&ListId(n)) else { continue };
            let a = list.def.map_or(-1, |d| d.0);
            let new_a = match brought.abstracts.get(&a) {
                Some(&x) => Some(x),
                None => copied_def(theirs, a, free_abstract, &ncx).map(|(held, mut v)| {
                    let mut set = Vec::new();
                    for (il, l) in v.levels.iter_mut().enumerate() {
                        if let Some(l) = l.as_mut().filter(|l| l.pic_bullet.is_some()) {
                            Rc::make_mut(l).pic_bullet = None;
                            set.push(ListDefProp::Level(il as u8, LevelProp::PicBullet));
                        }
                    }
                    let id = next.set_list_def(held.edited(v, &set)).0;
                    brought.abstracts.insert(a, id);
                    free_abstract += 1;
                    id
                }),
            };
            let Some(new_a) = new_a else {
                brought.nums.insert(n, 0);
                continue;
            };
            let Some(held) = copied_list(theirs, n, free_num, new_a, &ncx) else { continue };
            brought.nums.insert(n, next.set_list(held).0);
            free_num += 1;
        }
        let scx = next.cx();
        for id in self.styles {
            let Some(e) = theirs.cascade.styles.styles.get(id.as_str()) else { continue };
            let held = theirs.styles.styles.get(id.as_str()).cloned().unwrap_or_else(|| StyleDef::typed(e.style.clone()));
            let held = held.moved_to(&scx).unwrap_or(held);
            let mut s = (**held.get()).clone();
            let mut set = Vec::new();
            let list = ParaSet { prop: ParaProp::List, whole: false };
            if renumbered(&mut s.format.para.list, &brought.nums) {
                set.push(StyleSet::Para(None, list));
            }
            for r in &mut s.regions {
                if renumbered(&mut r.format.para.list, &brought.nums) {
                    set.push(StyleSet::Para(Some(r.kind), list));
                }
            }
            if s.default.take().is_some() {
                set.push(StyleSet::Style(StyleProp::Default));
            }
            let held = if set.is_empty() { held } else { held.edited(s.clone(), &set) };
            if let Err(StyleEditError::Loop) = next.set_style(held.clone()) {
                s.based_on = None;
                let _ = next.set_style(held.edited(s, &[StyleSet::Style(StyleProp::BasedOn)]));
            }
        }
    }
}

/// The list ids of pasted paragraphs mapped.
fn renumber_blocks(blocks: &mut [Block], refs: &mut Refs, cx: &Cx, map: &HashMap<i64, i64>) {
    if !map.is_empty() {
        each_block_of(blocks, refs, &mut |b| {
            if let Block::Para(p) = b {
                renumber_para(p, cx, map);
            }
        });
    }
}

/// A pasted paragraph's own list ids mapped.
fn renumber_para(p: &mut Paragraph, cx: &Cx, map: &HashMap<i64, i64>) {
    // Theme colours don't change the list ids written.
    let Some(x) = p.style.direct.xml(&Default::default(), cx) else { return };
    if !num_ids(&x, cx).iter().any(|k| map.contains_key(k)) {
        return;
    }
    if let Some(raw) = renumber(&x, cx, map) {
        Rc::make_mut(&mut p.style).direct = ParaDirect::from_xml(Some(&raw), cx);
    }
}

/// Copying definitions by rewriting their XML, as copies were made before they were copied typed: each copy made
/// typed is made this way too and the two compared.
#[cfg(test)]
pub(super) mod xml_route {
    use std::cell::RefCell;

    use super::*;

    /// A copied element whose bytes differ between the routes.
    #[derive(Clone, Debug)]
    pub struct Difference {
        /// The element: `style X`, `definition N`, `list N`.
        pub what: String,
        /// As the XML route writes it.
        pub xml: String,
        /// As the typed route writes it.
        pub typed: String,
    }

    thread_local! {
        static SEEN: RefCell<(usize, Vec<Difference>)> = const { RefCell::new((0, Vec::new())) };
        static CONTEXT: RefCell<String> = const { RefCell::new(String::new()) };
    }

    /// The elements compared on this thread so far, and those whose bytes differed; both taken.
    pub fn take() -> (usize, Vec<Difference>) {
        SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()))
    }

    /// What this thread copies from now on, for the comparisons' messages and differences.
    pub fn context(what: &str) {
        CONTEXT.with(|c| *c.borrow_mut() = what.to_string());
    }

    /// Elements `what` the XML route refused where the typed route copied them (one of them as read, `xml`), noted.
    pub fn refused(what: &str, xml: String) {
        let what = CONTEXT.with(|c| format!("{}: {what} refused by the XML route", c.borrow()));
        SEEN.with(|s| s.borrow_mut().1.push(Difference { what, xml, typed: String::new() }));
    }

    /// `n` with its names expanded, its attributes in order, and no whitespace-only text: what an XML reader reads of
    /// it whatever its prefixes, namespace declarations, attribute order and spacing between elements.
    fn expanded(n: roxmltree::Node, out: &mut String) {
        use std::fmt::Write;
        if n.is_text() {
            if let Some(t) = n.text().filter(|t| !t.trim().is_empty()) {
                out.push_str(t);
            }
            return;
        }
        if !n.is_element() {
            return;
        }
        let name = |ns: Option<&str>, local: &str| format!("{{{}}}{local}", ns.unwrap_or_default());
        let _ = write!(out, "<{}", name(n.tag_name().namespace(), n.tag_name().name()));
        let mut attrs: Vec<(String, &str)> = n.attributes().map(|a| (name(a.namespace(), a.name()), a.value())).collect();
        attrs.sort();
        for (k, v) in attrs {
            let _ = write!(out, " {k}={v:?}");
        }
        out.push('>');
        n.children().for_each(|c| expanded(c, out));
        out.push_str("</>");
    }

    /// `x`, an element in the part `cx` describes, as [`expanded`] reads it.
    pub fn read_as(x: &str, cx: &Cx) -> String {
        cx.parse(x, |_, el| {
            let mut s = String::new();
            expanded(el, &mut s);
            s
        })
        .unwrap_or_else(|| panic!("{x} reads"))
    }

    /// One copied element compared, each route's as its holder writes it in the part `cx` describes and as its
    /// definition writes it with nothing as read: copied by both or neither, and each pair read alike
    /// ([`read_as`]); a byte difference between the holders' is noted.
    pub fn compare(what: &str, xml: Option<(String, String)>, typed: Option<(String, String)>, cx: &Cx) {
        let what = CONTEXT.with(|c| format!("{}: {what}", c.borrow()));
        assert_eq!(xml.is_some(), typed.is_some(), "{what}: copied by both routes or neither");
        let (Some((xml, xml_fresh)), Some((typed, typed_fresh))) = (xml, typed) else { return };
        assert_eq!(read_as(&xml, cx), read_as(&typed, cx), "{what}: written alike\n xml:   {xml}\n typed: {typed}");
        assert_eq!(read_as(&xml_fresh, cx), read_as(&typed_fresh, cx), "{what}: the same definition\n xml:   {xml_fresh}\n typed: {typed_fresh}");
        SEEN.with(|s| {
            let mut s = s.borrow_mut();
            s.0 += 1;
            if xml != typed {
                s.1.push(Difference { what, xml, typed });
            }
        });
    }

    /// A `w:style`'s `w:default` taken out.
    fn not_default(n: roxmltree::Node) -> Option<(std::ops::Range<usize>, String)> {
        let a = n.attributes().find(|a| a.name() == "default" && a.namespace() == Some(crate::docx::xml::W)).filter(|_| is_w(n, "style"))?;
        Some((a.range(), String::new()))
    }

    /// `raw`, an element as `from` writes one, declaring the namespaces it has from `from`'s root that `to`'s root
    /// doesn't bind as it does, so it reads the same where `to` writes it.
    pub fn rebound(raw: &str, from: &Cx, to: &Cx) -> Option<String> {
        let have: Vec<(Option<String>, String)> =
            to.parse("<y/>", |_, el| el.namespaces().map(|n| (n.name().map(str::to_string), n.uri().to_string())).collect())?;
        let decls = from.parse(raw, |text, el| {
            let head = &text[el.range().start..el.first_child().map_or(el.range().end, |c| c.range().start)];
            let declared =
                |name: &str| head.match_indices(name).any(|(i, m)| head[i + m.len()..].starts_with(|c: char| c == '=' || c.is_whitespace()));
            el.namespaces()
                .filter(|n| n.name() != Some("xml") && !have.iter().any(|(p, u)| p.as_deref() == n.name() && u == n.uri()))
                .filter_map(|n| {
                    let name = n.name().map_or_else(|| "xmlns".to_string(), |p| format!("xmlns:{p}"));
                    (!declared(&name)).then(|| format!(" {name}=\"{}\"", crate::docx::write::props::esc(n.uri())))
                })
                .collect::<String>()
        })?;
        if decls.is_empty() {
            return Some(raw.to_string());
        }
        let at = raw.find(|c: char| c.is_whitespace() || c == '/' || c == '>')?;
        Some(format!("{}{decls}{}", &raw[..at], &raw[at..]))
    }

    /// [`Copied::apply`] over the definitions' XML: each element as written there rewritten and set from its XML;
    /// with `rebind`, a definition's and a list's declaring the namespaces they have there that this document's
    /// numbering part lacks, as a style's do.
    fn apply(c: &Copied, next: &mut Resolvers, brought: &mut Lists, rebind: bool) {
        let theirs = c.theirs;
        let (scx, ncx) = (theirs.styles.cx(), theirs.numbering.cx());
        let here = next.typed().numbering.cx();
        let rebound_here = |raw: String| if rebind { rebound(&raw, &ncx, &here) } else { Some(raw) };
        let their_lists = &theirs.cascade.lists;
        let def_of = |n: i64| their_lists.lists.get(&ListId(n)).map(|l| l.def.map_or(-1, |d| d.0));
        let Lists { nums: num_map, abstracts: abstract_map } = brought;
        let (mut free_abstract, mut free_num) = c.free;
        for &n in c.lists {
            let Some(a) = def_of(n) else { continue };
            let new_a = match abstract_map.get(&a) {
                Some(&x) => Some(x),
                None => theirs.def_xml(a).and_then(|abs| {
                    let raw = rewrite(&abs, &ncx, &|e| {
                        if is_w(e, "abstractNum") {
                            attr_range(e, "abstractNumId").map(|(r, _)| (r, free_abstract.to_string()))
                        } else if is_w(e, "lvlPicBulletId") {
                            Some((e.range(), String::new()))
                        } else {
                            None
                        }
                    })?;
                    let id = next.set_abstract_xml(rebound_here(raw)?)?;
                    abstract_map.insert(a, id);
                    free_abstract += 1;
                    Some(id)
                }),
            };
            let Some(new_a) = new_a else {
                num_map.insert(n, 0);
                continue;
            };
            let Some(num) = theirs.list_xml(n) else { continue };
            let raw = rewrite(&num, &ncx, &|e| {
                if is_w(e, "num") {
                    attr_range(e, "numId").map(|(r, _)| (r, free_num.to_string()))
                } else if is_w(e, "abstractNumId") {
                    attr_range(e, "val").map(|(r, _)| (r, new_a.to_string()))
                } else {
                    None
                }
            });
            match raw.and_then(rebound_here).and_then(|raw| next.set_num_xml(raw)) {
                Some(id) => {
                    num_map.insert(n, id);
                    free_num += 1;
                }
                None => {
                    num_map.insert(n, 0);
                }
            }
        }
        let dcx = next.cx();
        for id in c.styles {
            let Some(raw) = theirs.style_xml(id) else { continue };
            let raw = if num_map.is_empty() { Some(raw.into_owned()) } else { renumber(&raw, &scx, num_map) };
            let Some(raw) = raw.and_then(|r| rewrite(&r, &scx, &not_default)).and_then(|r| rebound(&r, &scx, &dcx)) else { continue };
            if let Err(StyleEditError::Loop) = next.set_style_xml(raw.clone())
                && let Some(raw) = rewrite(&raw, &dcx, &|n| is_w(n, "basedOn").then(|| (n.range(), String::new())))
            {
                let _ = next.set_style_xml(raw);
            }
        }
    }

    /// The copy `c` made over the XML from `was` (the definitions before it, and the lists brought before it) set
    /// against `next` and `brought`, the typed route's: the same lists each style names, the same ids taken, and
    /// each copied element compared.
    pub(super) fn check(c: &Copied, was: (Resolvers, Lists), next: &Resolvers, brought: &Lists) {
        let (mut old, mut old_brought) = was;
        let theirs = c.theirs;
        let context = CONTEXT.with(|c| c.borrow().clone());
        // A glossary style's XML names its ids as the glossary does: the XML route copied it under those
        let mut glossary = Vec::new();
        for id in c.styles {
            let Some(e) = theirs.cascade.styles.styles.get(id.as_str()) else { continue };
            let Some(x) = theirs.style_xml(id) else { continue };
            let cx = theirs.styles.cx();
            let from_xml: Vec<i64> = num_ids(&x, &cx).into_iter().filter(|n| *n != 0).collect();
            let named = StyleDef::from_xml(&x, &cx).get().id.clone();
            if e.origin == crate::format::Origin::Glossary && (*named != **id || from_xml != style_lists(&e.style).collect::<Vec<_>>()) {
                let typed = next.style_xml(id).unwrap_or_default();
                SEEN.with(|s| {
                    s.borrow_mut().1.push(Difference { what: format!("{context}: glossary style {id} as {named}"), xml: x.into_owned(), typed })
                });
                glossary.push(id.clone());
                continue;
            }
            assert_eq!(from_xml, style_lists(&e.style).collect::<Vec<_>>(), "{context}: style {id}: the lists it names");
        }
        let (mut rebound, mut rebound_brought) = (old.clone(), old_brought.clone());
        apply(c, &mut old, &mut old_brought, false);
        if (&old_brought.nums, &old_brought.abstracts) != (&brought.nums, &brought.abstracts) {
            // Lists whose XML doesn't read in this document's numbering part: the XML route refused them
            let mut refused: Vec<i64> =
                old_brought.nums.iter().filter(|(k, v)| **v == 0 && brought.nums.get(*k).is_some_and(|t| *t != 0)).map(|(k, _)| *k).collect();
            refused.sort_unstable();
            assert!(!refused.is_empty(), "{context}: the same ids taken\n xml:   {old_brought:?}\n typed: {:?}", (&brought.nums, &brought.abstracts));
            apply(c, &mut rebound, &mut rebound_brought, true);
            assert_eq!(
                (&rebound_brought.nums, &rebound_brought.abstracts),
                (&brought.nums, &brought.abstracts),
                "{context}: the same ids taken, rebound"
            );
            let def = theirs.cascade.lists.lists.get(&ListId(refused[0])).and_then(|l| l.def).map_or(-1, |d| d.0);
            let first = theirs.def_xml(def).map(|x| x.into_owned()).unwrap_or_default();
            self::refused(&format!("lists {refused:?}"), first);
            old = rebound;
        }
        let styles: Vec<String> = c.styles.iter().filter(|s| !glossary.contains(s)).cloned().collect();
        if glossary.is_empty() {
            assert_eq!(old.typed().cascade.styles.styles.len(), next.typed().cascade.styles.styles.len(), "{context}: the same styles");
        }
        compare_parts(&old, next, &styles, &brought.nums, &brought.abstracts);
    }

    /// The styles `styles`, the definitions `abstracts` maps to and the lists `nums` maps to compared between `old` and
    /// `next`.
    pub fn compare_parts(old: &Resolvers, next: &Resolvers, styles: &[String], nums: &HashMap<i64, i64>, abstracts: &HashMap<i64, i64>) {
        let (a, b) = (old.typed(), next.typed());
        let colours = next.colours();
        let (scx, ncx) = (b.styles.cx(), b.numbering.cx());
        let style = |d: &StyleDef| (d.xml(&colours, &scx).into_owned(), StyleDef::typed(d.get().clone()).xml(&colours, &scx).into_owned());
        for id in styles {
            let (x, y) = (a.styles.styles.get(id.as_str()), b.styles.styles.get(id.as_str()));
            compare(&format!("style {id}"), x.map(style), y.map(style), &scx);
        }
        let ids = |m: &HashMap<i64, i64>| {
            let mut v: Vec<i64> = m.values().copied().filter(|n| *n > 0).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let def = |d: &ListDefDirect| (d.xml(&colours, &ncx).into_owned(), ListDefDirect::typed(d.get().clone()).xml(&colours, &ncx).into_owned());
        for n in abstracts.values().copied().collect::<HashSet<_>>() {
            let (x, y) = (a.numbering.defs.get(&ListDefId(n)), b.numbering.defs.get(&ListDefId(n)));
            compare(&format!("definition {n}"), x.map(def), y.map(def), &ncx);
        }
        let list = |d: &ListDirect| (d.xml(&colours, &ncx).into_owned(), ListDirect::typed(d.get().clone()).xml(&colours, &ncx).into_owned());
        for n in ids(nums) {
            let (x, y) = (a.numbering.lists.get(&ListId(n)), b.numbering.lists.get(&ListId(n)));
            compare(&format!("list {n}"), x.map(list), y.map(list), &ncx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stored XML declares the bindings it uses that the destination's root lacks, once, and only those.
    #[test]
    fn stored_xml_declares_what_it_uses() {
        let w = crate::docx::xml::W;
        let w14 = "http://schemas.microsoft.com/office/word/2010/wordml";
        let mc = "http://schemas.openxmlformats.org/markup-compatibility/2006";
        let from = Cx::of_xml(&format!(r#"<w:document xmlns:w="{w}" xmlns:w14="{w14}" xmlns:mc="{mc}" xmlns:v="urn:v">"#));
        let to = Cx::of_xml(&format!(r#"<x:document xmlns:x="{w}" xmlns:mc="{mc}">"#));
        let r = Rebind::between(&from, &to);
        let got = r.element(r#"<w:ruby><w:rt mc:Ignorable="w14"/></w:ruby>"#);
        assert_eq!(got.as_deref(), Some(format!(r#"<w:ruby xmlns:w="{w}" xmlns:w14="{w14}"><w:rt mc:Ignorable="w14"/></w:ruby>"#).as_str()));
        let got = r.element(&format!(r#"<w:permEnd xmlns:w="{w}" w:id="1"/>"#));
        assert_eq!(got, None, "declared on itself");
        assert_eq!(Rebind::between(&from, &from).element("<w:pPr/>"), None, "the same root");
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(crate::layout::tests::output::root().join("fixtures").join(name)).expect("reads")
    }

    /// The body's paragraphs, top level.
    fn body(e: &Editor) -> Vec<&Paragraph> {
        e.document().sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect()
    }

    /// The numbered paragraphs of the body whose list is defined, with a definition, in the document.
    fn numbered(e: &Editor) -> usize {
        let lists = e.document().resolver.as_ref().expect("a resolver").cascade().lists.clone();
        let defined = |n: ListId| lists.lists.get(&n).and_then(|l| l.def).is_some_and(|d| lists.defs.contains_key(&d));
        body(e).iter().filter(|p| p.list.is_some() && p.style.num.is_some_and(|r| defined(r.list))).count()
    }

    /// All of `from` copied and pasted at the start of `into`, and what was copied.
    fn paste_whole(from: &mut Editor, into: &[u8]) -> (Editor, Fragment) {
        use crate::editing::tests::{fonts, texts_ids};
        from.execute(crate::editing::commands::Command::SelectAll, fonts());
        let frag = from.copy().expect("copies");
        let mut e = Editor::open(into, fonts()).expect("opens");
        let first = texts_ids(&e)[0];
        e.select_range(first, crate::layout::Unit(0), crate::layout::Unit(0), fonts());
        e.paste_fragment(&frag, fonts());
        (e, frag)
    }

    /// A Word document's numbered paragraphs pasted in one with no numbering part stay numbered, in lists defined
    /// there, though their definitions name namespaces that document doesn't declare.
    #[test]
    fn word_lists_pasted_in_a_document_without_numbering_stay_numbered() {
        use crate::editing::tests::{blank_body, fonts};
        let mut src = Editor::open(&fixture("lists.docx"), fonts()).expect("opens");
        let want = numbered(&src);
        assert!(want > 0, "the source is numbered");
        let (e, _) = paste_whole(&mut src, &blank_body("<w:p/>"));
        assert_eq!(numbered(&e), want, "each numbered paragraph stays numbered");
    }

    /// A sub-template made from a Word document's body, in a document with a list of its own and no namespaces
    /// beyond the main one: each of its lists numbers with a definition that exists, and the document's own
    /// definition stays as it was.
    #[test]
    fn a_sub_template_s_lists_have_their_definitions() {
        use crate::api::command::{Command as C, CreateSubTemplate, ListKind};
        use crate::api::value::Bytes;
        use crate::editing::tests::{blank_with, fonts, texts_ids};
        let mut e = Editor::open(&blank_with("One"), fonts()).expect("opens");
        let first = texts_ids(&e)[0];
        e.select_range(first, crate::layout::Unit(0), crate::layout::Unit(1), fonts());
        e.execute(C::SetList(Some(ListKind::Numbered)), fonts());
        e.execute(crate::editing::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
        let defs = |e: &Editor| e.document().resolver.as_ref().expect("a resolver").typed();
        let before = defs(&e);
        let own: Vec<(ListDefId, String)> = before.numbering.defs.keys().map(|k| (*k, before.def_xml(k.0).expect("written").into_owned())).collect();
        assert!(!own.is_empty(), "a list of its own");
        let c = CreateSubTemplate { name: "Lists".into(), category: None, from: Some(Bytes(fixture("lists.docx"))), copy_of: None };
        assert!(matches!(e.execute(C::CreateSubTemplate(c), fonts()), crate::editing::commands::Outcome::Changed(_)), "created");
        let after = defs(&e);
        let lists = &after.cascade.lists;
        assert!(lists.lists.len() > 1, "its lists came");
        for (id, l) in &lists.lists {
            assert!(l.def.is_some_and(|d| lists.defs.contains_key(&d)), "list {} numbers with a definition that exists", id.0);
        }
        for (k, x) in &own {
            assert_eq!(after.def_xml(k.0).as_deref(), Some(x.as_str()), "definition {} as it was", k.0);
        }
    }

    /// A sub-template's body copied where it is edited, its paragraphs in a glossary style read under another id
    /// (the document has one of that id), pasted in another document: each style the copied paragraphs name is there.
    #[test]
    fn a_renamed_glossary_style_comes_under_its_name() {
        use crate::api::command::{Command as C, SubTemplateName};
        use crate::editing::tests::{blank_body, fonts};
        let mut src = Editor::open(&fixture("editor/FX-SUBTEMPLATES.docx"), fonts()).expect("opens");
        src.execute(crate::editing::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
        src.execute(C::EditSubTemplate(SubTemplateName { name: "FeeRow".into(), category: None }), fonts());
        let (e, frag) = paste_whole(&mut src, &blank_body("<w:p/>"));
        let res = e.document().resolver.as_ref().expect("a resolver");
        let mut styled = Vec::new();
        each_block(&mut frag.blocks.clone(), &mut |b| {
            if let Block::Para(p) = b {
                styled.extend(p.style.style_id.clone());
            }
        });
        assert!(styled.iter().any(|s| s == "~g~Header"), "a paragraph in the renamed style: {styled:?}");
        for s in &styled {
            assert!(res.has_style(s), "style {s} exists");
        }
    }

    /// `docx` with its styles part's main namespace bound to prefix `x`.
    fn with_x_prefix(docx: &[u8]) -> Vec<u8> {
        use std::io::{Read, Write};
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(docx)).expect("a zip");
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..z.len() {
            let mut f = z.by_index(i).expect("an entry");
            let name = f.name().expect("a name").to_string();
            let mut data = Vec::new();
            f.read_to_end(&mut data).expect("reads");
            if name == "word/styles.xml" {
                let x = String::from_utf8(data).expect("utf-8");
                data = x.replace("<w:", "<x:").replace("</w:", "</x:").replace(" w:", " x:").replace("xmlns:w=", "xmlns:x=").into_bytes();
            }
            out.start_file(name, zip::write::SimpleFileOptions::default()).expect("starts");
            out.write_all(&data).expect("writes");
        }
        out.finish().expect("finishes").into_inner()
    }

    /// Every fixture's body inserted in a document with few namespaces, in one with lists and styles of its own, and in
    /// one whose styles part binds the main namespace to another prefix; and copied whole and pasted in the first: each
    /// style, definition and list copied is the definition the XML route copied, and written as it wrote it.
    #[test]
    fn copied_definitions_are_written_as_their_xml_was() {
        use crate::api::command::{Command as C, InsertFile};
        use crate::api::value::Bytes;
        use crate::editing::commands::Command;
        use crate::editing::tests::{blank_body, fonts, par_map, texts_ids};
        let mut paths = Vec::new();
        crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
        let fixture = |n: &str| std::fs::read(crate::layout::tests::output::root().join("fixtures").join(n)).expect("reads");
        let blank = blank_body("<w:p/>");
        let dests = [blank.clone(), fixture("lists.docx"), with_x_prefix(&blank)];
        let caret = |e: &mut Editor| {
            let first = texts_ids(e)[0];
            e.select_range(first, crate::layout::Unit(0), crate::layout::Unit(0), fonts());
        };
        // Its comment ranges fail the editor's integrity check on insertion and paste, whichever route copies
        paths.retain(|p| !p.ends_with("FX-REVIEW-STORIES.docx"));
        let seen = par_map(&paths, |path| {
            let bytes = std::fs::read(path).expect("reads");
            if crate::docx::read_editable(&bytes).ok().and_then(|d| d.resolver).is_none() {
                return (0, Vec::new());
            }
            let name = path.strip_prefix(crate::layout::tests::output::root()).unwrap_or(path).display().to_string();
            for (d, into) in dests.iter().zip(["blank", "lists", "x prefix"]) {
                xml_route::context(&format!("{name} inserted in {into}"));
                let mut e = Editor::open(d, fonts()).expect("opens");
                caret(&mut e);
                e.execute(C::InsertFile(InsertFile { from: Bytes(bytes.clone()) }), fonts());
            }
            for (d, into) in dests.iter().zip(["blank", "lists"]) {
                xml_route::context(&format!("{name} the body of {into}"));
                Editor::open(d, fonts()).expect("opens").replace_body(&bytes, fonts());
            }
            if let Ok(mut src) = Editor::open(&bytes, fonts()) {
                src.execute(Command::SelectAll, fonts());
                if let Some(frag) = src.copy() {
                    xml_route::context(&format!("{name} pasted in blank"));
                    let mut e = Editor::open(&blank, fonts()).expect("opens");
                    caret(&mut e);
                    e.paste_fragment(&frag, fonts());
                }
            }
            xml_route::take()
        });
        // Each sub-template's body, copied where it is edited (its styles the glossary's) and pasted in the first
        let mut e = Editor::open(&fixture("editor/FX-SUBTEMPLATES.docx"), fonts()).expect("opens");
        e.execute(crate::editing::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
        for name in ["Violation", "FeeRow", "Appeals notice"] {
            e.execute(C::EditSubTemplate(crate::api::command::SubTemplateName { name: name.into(), category: None }), fonts());
            e.execute(Command::SelectAll, fonts());
            let frag = e.copy().expect("copies");
            xml_route::context(&format!("sub-template {name} pasted in blank"));
            let mut b = Editor::open(&blank, fonts()).expect("opens");
            caret(&mut b);
            b.paste_fragment(&frag, fonts());
        }
        let seen: Vec<(usize, Vec<xml_route::Difference>)> = seen.into_iter().chain([xml_route::take()]).collect();
        let compared: usize = seen.iter().map(|s| s.0).sum();
        let differences: Vec<&xml_route::Difference> = seen.iter().flat_map(|s| &s.1).collect();
        let mut kinds: std::collections::BTreeMap<String, (usize, &xml_route::Difference)> = Default::default();
        for d in &differences {
            let kind = difference_kind(d);
            assert!(
                kind != "prefixes" || d.what.contains("x prefix"),
                "{}: prefixes differ only where the part binds others\n xml:   {}\n typed: {}",
                d.what,
                d.xml,
                d.typed
            );
            assert!(kind != "refused" || !d.what.contains("lists:"), "{}: refused only where there is no numbering part", d.what);
            let element = d.what.rsplit(": ").next().and_then(|w| w.split(' ').next()).unwrap_or_default();
            kinds.entry(format!("{kind} {element}")).or_insert((0, d)).0 += 1;
        }
        for (k, (n, d)) in &kinds {
            eprintln!("{k}: {n}, as {}\n  xml:   {}\n  typed: {}", d.what, d.xml, d.typed);
        }
        eprintln!("{compared} compared, {} differ", differences.len());
        assert!(compared > 1000, "definitions are copied");
    }

    /// What a byte difference between the routes is, in the first of these that tells them apart no more: spacing
    /// in tags and between elements; namespace declarations; attribute order; prefixes (what [`xml_route::read_as`]
    /// already reads alike).
    fn difference_kind(d: &xml_route::Difference) -> &'static str {
        if d.typed.is_empty() {
            return "refused";
        }
        if d.what.contains(": glossary style ") {
            return "glossary ids";
        }
        let spacing = |x: &str| {
            let x = regex_lite::Regex::new(r">\s+<").expect("a pattern").replace_all(x, "><").into_owned();
            let x = regex_lite::Regex::new(r#"\s+"#).expect("a pattern").replace_all(&x, " ").into_owned();
            x.replace(" />", "/>").replace(" >", ">")
        };
        let undeclared = |x: &str| regex_lite::Regex::new(r#" xmlns(:[\w.-]+)?="[^"]*""#).expect("a pattern").replace_all(x, "").into_owned();
        let sorted = |x: &str| {
            let tag = regex_lite::Regex::new(r"<([^/!?][^>]*?)(/?)>").expect("a pattern");
            tag.replace_all(x, |c: &regex_lite::Captures| {
                let mut parts: Vec<&str> = c[1].split(' ').collect();
                parts[1..].sort_unstable();
                format!("<{}{}>", parts.join(" "), &c[2])
            })
            .into_owned()
        };
        let (a, b) = (spacing(&d.xml), spacing(&d.typed));
        if a == b {
            return "spacing";
        }
        let (a, b) = (undeclared(&a), undeclared(&b));
        if a == b {
            return "declarations";
        }
        if sorted(&a) == sorted(&b) {
            return "attribute order";
        }
        "prefixes"
    }
}
