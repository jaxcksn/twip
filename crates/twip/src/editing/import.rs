//! Pasted content from another document

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::docx::para_props::ParaDirect;
use crate::docx::resolve::Resolver;
use crate::docx::review::{AuthorId, FormatChange, People, RevisionMark, TableChange};
use crate::docx::write::props::Cx;
use crate::docx::{Block, Inline, Opaque, Paragraph, Raw, Run, RunStyle};
use crate::drawing::paint::Fill;
use crate::drawing::{Shape, ShapeKind};
use crate::layout::{ParaId, TableId};

use super::clipboard::{Fragment, Refs};
use super::step::Step;
use super::{Editor, Tx};

/// The document a fragment was copied from: what its image indices, style and list ids and authors are there.
#[derive(Debug)]
pub struct Origin {
    /// The document's identity among those open ([`next_key`]).
    pub key: u64,
    /// The bytes of each image the fragment shows, by its index there.
    pub images: Vec<(usize, Vec<u8>)>,
    /// Its styles and lists.
    pub resolver: Option<Rc<Resolver>>,
    /// Its people.
    pub people: People,
    /// Its main part's root start tag, whose namespace declarations its paragraphs' XML is written under (empty for a
    /// document not read from a file).
    pub main: String,
}

impl Origin {
    /// The origin of `f`, read from `doc` (a document no editor has open): its images move into it.
    pub(super) fn of_document(mut doc: crate::docx::Document, f: &mut Fragment) -> Rc<Origin> {
        let images = used_images(f).into_iter().filter_map(|k| Some((k, std::mem::take(doc.images.get_mut(k)?)))).collect();
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

/// The image indices `f` (its notes and comments too) shows, in order.
fn used_images(f: &mut Fragment) -> Vec<usize> {
    let mut used = Vec::new();
    each_block_of(&mut f.blocks, &mut f.refs, &mut |b| each_image(b, &mut |k| used.push(*k)));
    used.sort_unstable();
    used.dedup();
    used
}

/// The lists a paste has brought from another document so far, so its later pieces share them: each `w:num` and
/// `w:abstractNum` id there and the one it took here (0, a list that couldn't come).
#[derive(Default)]
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

fn fill_image(fill: Option<&mut Fill>, f: &mut dyn FnMut(&mut usize)) {
    if let Some(Fill::Blip(b)) = fill
        && let Some(k) = &mut b.image
    {
        f(k);
    }
}

/// The image indices of a shape's fills and pictures, its group's children included.
fn shape_images(s: &mut Shape, f: &mut dyn FnMut(&mut usize)) {
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

/// Every image index the paragraphs of `blocks` hold: pictures', floats' and their parts', and their shapes'.
fn each_image(block: &mut Block, f: &mut dyn FnMut(&mut usize)) {
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
        let change = |c: &mut Option<Rc<TableChange>>| {
            if let Some(c) = c
                && c.previous_raw.0.as_deref().is_some_and(|x| self.element(x).is_some())
            {
                self.raw(&mut Rc::make_mut(c).previous_raw);
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
                self.raw(&mut t.direct);
                change(&mut t.change);
                for r in &mut t.rows {
                    self.raw(&mut r.direct);
                    self.raw(&mut r.exceptions);
                    change(&mut r.rev.change);
                    change(&mut r.rev.ex_change);
                    for c in &mut r.cells {
                        self.raw(&mut c.direct);
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

/// The `w:numId` values `raw` (a `w:pPr` or a `w:style`) names.
fn num_ids(raw: &str, cx: &Cx) -> Vec<i64> {
    cx.parse(raw, |_, el| el.descendants().filter(|n| is_w(*n, "numId")).filter_map(|n| attr_range(n, "val").map(|a| a.1)).collect())
        .unwrap_or_default()
}

/// A `w:style`'s `w:default` taken out.
fn not_default(n: roxmltree::Node) -> Option<(std::ops::Range<usize>, String)> {
    let a = n.attributes().find(|a| a.name() == "default" && a.namespace() == Some(crate::docx::xml::W)).filter(|_| is_w(n, "style"))?;
    Some((a.range(), String::new()))
}

/// `raw`, an element as `from` writes one, declaring the namespaces it has from `from`'s root that `to`'s root
/// doesn't bind as it does, so it reads the same where `to` writes it.
fn rebound(raw: &str, from: &Cx, to: &Cx) -> Option<String> {
    let have: Vec<(Option<String>, String)> =
        to.parse("<y/>", |_, el| el.namespaces().map(|n| (n.name().map(str::to_string), n.uri().to_string())).collect())?;
    let decls = from.parse(raw, |text, el| {
        let head = &text[el.range().start..el.first_child().map_or(el.range().end, |c| c.range().start)];
        let declared = |name: &str| head.match_indices(name).any(|(i, m)| head[i + m.len()..].starts_with(|c: char| c == '=' || c.is_whitespace()));
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
        let images = used_images(f).into_iter().filter_map(|k| Some((k, self.doc.images.get(k)?.clone()))).collect();
        Rc::new(Origin { key: self.key, images, resolver: self.doc.resolver.clone(), people: self.doc.people.clone(), main: root_tag(&self.doc) })
    }

    /// Makes pasted `blocks`, and the bodies of the notes and comments they refer to, this document's: the images they
    /// show added (or found), and from another document, its styles and lists they use added under ids free here, its
    /// people found or added, and what they were read from forgotten (another part's XML).
    /// `lists`: those earlier pieces of the same paste brought.
    pub(super) fn import(&mut self, tx: &mut Tx, blocks: &mut [Block], refs: &mut Refs, origin: &Origin, lists: &mut Lists) {
        let mut moved: HashMap<usize, usize> = HashMap::new();
        for (k, data) in &origin.images {
            let to = if self.doc.images.get(*k) == Some(data) {
                *k
            } else if let Some(j) = self.doc.images.iter().position(|d| d == data) {
                j
            } else if let Some(j) = self.pending_images.iter().position(|d| d == data) {
                self.doc.images.len() + j
            } else {
                self.pending_image(data.clone())
            };
            if to != *k {
                moved.insert(*k, to);
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
    fn import_styles(&mut self, tx: &mut Tx, blocks: &mut [Block], refs: &mut Refs, from: &Resolver, main: Option<&Cx>, brought: &mut Lists) {
        let Some(res) = self.doc.resolver.clone() else { return };
        let (scx, ncx) = (from.styles.cx(), from.numbering.cx());
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
                if res.styles.by_id.contains_key(&id) || !seen.insert(id.clone()) {
                    continue;
                }
                let Some(s) = from.styles.get(&id) else { continue };
                want.extend([&s.based_on, &s.link, &s.next].into_iter().flatten().cloned());
                nums.extend(num_ids(&s.raw, &scx));
                styles.push(id);
            }
            while let Some(n) = nums.pop() {
                if n == 0 || lists.contains(&n) || brought.nums.contains_key(&n) {
                    continue;
                }
                let Some(num) = from.numbering.nums.get(&n) else { continue };
                lists.push(n);
                if let Some(a) = from.numbering.abstracts.get(&num.abstract_id) {
                    want.extend([&a.num_style_link, &a.style_link].into_iter().flatten().cloned());
                }
            }
        }
        let Lists { nums: num_map, abstracts: abstract_map } = brought;
        if styles.is_empty() && lists.is_empty() {
            renumber_blocks(blocks, refs, pcx, num_map);
            return;
        }
        let mut next = (*res).clone();
        let colours = next.styles.colours;
        let (mut free_abstract, mut free_num) = next.numbering.free_ids();
        for l in &self.doc.added_lists {
            free_abstract = free_abstract.max(l.abstract_id + 1);
            free_num = free_num.max(l.num_id + 1);
        }
        // Each list's `w:num` and its `w:abstractNum` (one per definition), under new ids; a picture bullet's
        // reference goes (the bullet's picture isn't brought), so the level shows its text.
        for n in lists {
            let num = &from.numbering.nums[&n];
            let a = num.abstract_id;
            let new_a = match abstract_map.get(&a) {
                Some(&x) => Some(x),
                None => from.numbering.abstracts.get(&a).and_then(|abs| {
                    let raw = rewrite(&abs.raw, &ncx, &|e| {
                        if is_w(e, "abstractNum") {
                            attr_range(e, "abstractNumId").map(|(r, _)| (r, free_abstract.to_string()))
                        } else if is_w(e, "lvlPicBulletId") {
                            Some((e.range(), String::new()))
                        } else {
                            None
                        }
                    })?;
                    let id = next.numbering.set_abstract_xml(raw, &colours)?;
                    abstract_map.insert(a, id);
                    free_abstract += 1;
                    Some(id)
                }),
            };
            let Some(new_a) = new_a else {
                num_map.insert(n, 0);
                continue;
            };
            let raw = rewrite(&num.raw, &ncx, &|e| {
                if is_w(e, "num") {
                    attr_range(e, "numId").map(|(r, _)| (r, free_num.to_string()))
                } else if is_w(e, "abstractNumId") {
                    attr_range(e, "val").map(|(r, _)| (r, new_a.to_string()))
                } else {
                    None
                }
            });
            match raw.and_then(|raw| next.numbering.set_num_xml(raw, &colours)) {
                Some(id) => {
                    num_map.insert(n, id);
                    free_num += 1;
                }
                None => {
                    num_map.insert(n, 0);
                }
            }
        }
        // Each style under this document's namespaces, not its default (this document's stays). One based on itself
        // through its chain comes without its base; one that isn't a style doesn't come, and what uses it shows as
        // the default style does.
        let dcx = next.styles.cx();
        for id in &styles {
            let raw = &from.styles.by_id[id].raw;
            let raw = if num_map.is_empty() { Some(raw.clone()) } else { renumber(raw, &scx, num_map) };
            let Some(raw) = raw.and_then(|r| rewrite(&r, &scx, &not_default)).and_then(|r| rebound(&r, &scx, &dcx)) else { continue };
            if let Err(crate::docx::styles::StyleEditError::Loop) = next.styles.set_xml(raw.clone())
                && let Some(raw) = rewrite(&raw, &dcx, &|n| is_w(n, "basedOn").then(|| (n.range(), String::new())))
            {
                let _ = next.styles.set_xml(raw);
            }
        }
        renumber_blocks(blocks, refs, pcx, num_map);
        let added = self.doc.added_lists.clone();
        if self.step(tx, Step::Resolver { resolver: Some(Rc::new(next)), added }).is_ok() {
            tx.stale_paras.extend(paras);
            tx.stale_tables.extend(tables);
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
}
