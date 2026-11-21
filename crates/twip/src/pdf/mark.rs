//! Marking drawn items with their place in the structure ([`super::tags`]): glyph runs split where their paragraph's
//! stretches change (list numbers, links, figures, notes, languages), pictures by the identity they are drawn with,
//! turned text by what it holds. The links' areas, the bookmarks' places and the sub-template bodies shown are found
//! as the pages are marked.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use twip_graphics::kurbo::{self, Affine};
use twip_graphics::pdf::{self, ArtifactKind, Kind, Mark, Node};

use super::tags::{Kept, Places, Tag, Tags, Target, prune};
use crate::docx::Document;
use crate::layout::ParaId;
use crate::layout::fragment::{GlyphRun, Item, PictureOf, PlacedLine, Turned};
use crate::layout::traverse::Placed;
use crate::render::graphics::glyph_pieces;

/// The structure, with what wasn't drawn left out, the outline, and how many figures drawn have no alternative text.
pub(super) struct Structure {
    pub elements: Vec<pdf::Element>,
    pub outline: Vec<pdf::Heading>,
    pub missing_alt: usize,
}

/// A sub-template's body shown at a reference: its page and area, its places, its elements (until placed) and the
/// content of the reference's paragraph, which it follows.
struct Instance {
    page: usize,
    area: kurbo::Rect,
    places: Places,
    node: Node,
    after: Option<u32>,
}

/// A link's line: the placed line, and in turned text (one placed line for the whole box) the run's baseline.
type LineKey = (u32, Option<i64>);

/// Whose places a drawn item's paragraph is in.
#[derive(Clone, Copy)]
enum Source {
    Body,
    Instance(usize),
    Loose,
}

impl Source {
    /// A key for what is drawn per paragraph: each sub-template body shown is drawn apart.
    fn key(self) -> usize {
        match self {
            Source::Instance(i) => i,
            Source::Body | Source::Loose => usize::MAX,
        }
    }
}

/// The line an item is drawn on: its page, its id there and its paragraph.
#[derive(Clone, Copy)]
struct Line {
    page: usize,
    id: u32,
    para: Option<ParaId>,
}

pub(super) struct Marker<'d> {
    doc: &'d Document,
    tags: Tags<'d>,
    /// Whether each content id has been drawn.
    drawn: Vec<bool>,
    /// The first page each header row's run (whose places, paragraph, first unit) is drawn on.
    header_runs: HashMap<(usize, u64, u32), usize>,
    /// Each link's areas, one per line: (link, page, area), and where each line's is.
    areas: Vec<(u32, usize, kurbo::Rect)>,
    area_at: HashMap<(u32, usize, LineKey), usize>,
    /// The bookmarks found: (page, name, place), and each paragraph's next bookmark.
    dests: Vec<(usize, String, kurbo::Point)>,
    next_bookmark: HashMap<u64, usize>,
    /// The first line of each paragraph with bookmarks: page and top left.
    first_lines: HashMap<u64, (usize, kurbo::Point)>,
    /// The paragraph (and whose places it is in) and content of the last text marked: a run without glyphs (a caret
    /// stop) goes with it.
    last: ((usize, u64), u32),
    /// The paragraphs (and whose places they are in) whose text has been drawn: a run before it without units is its
    /// list number.
    started: HashSet<(usize, u64)>,
    instances: Vec<Instance>,
    /// Paragraphs drawn that no story tags, and their elements, for after the body.
    loose: Places,
    loose_nodes: Vec<Node>,
}

impl<'d> Marker<'d> {
    pub fn new(doc: &'d Document, tags: Tags<'d>) -> Marker<'d> {
        let n = tags.content_ids() as usize + 1;
        Marker {
            doc,
            tags,
            drawn: vec![false; n],
            header_runs: HashMap::new(),
            areas: Vec::new(),
            area_at: HashMap::new(),
            dests: Vec::new(),
            next_bookmark: HashMap::new(),
            first_lines: HashMap::new(),
            last: ((0, 0), 0),
            started: HashSet::new(),
            instances: Vec::new(),
            loose: Places::default(),
            loose_nodes: Vec::new(),
        }
    }

    /// A line placed on page `page`: where a paragraph's bookmarks go when no character is found after them, and the
    /// sub-template bodies shown in it.
    pub fn line(&mut self, page: usize, para: Option<ParaId>, l: &PlacedLine, dx: f64, dy: f64) {
        let (x, y) = (dx + l.x, dy + l.y);
        if let Some(p) = para
            && self.tags.bookmarks.contains_key(&p.0)
        {
            self.first_lines.entry(p.0).or_insert((page, kurbo::Point::new(x, y)));
        }
        for (r, e) in l.line.expansions.iter() {
            let after = para.and_then(|p| {
                let src = self.find(page, kurbo::Point::new(x, y + l.line.baseline), |pl| pl.stretches(p.0).is_some())?;
                Some(self.places(src).stretches(p.0)?.first()?.1)
            });
            let Some((node, places)) = self.tags.instance(self.doc, e.part) else { continue };
            self.drawn.resize(self.tags.content_ids() as usize + 1, false);
            let area = kurbo::Rect::new(x + r[0], y + r[1], x + r[2], y + r[3]);
            self.instances.push(Instance { page, area, places, node, after });
        }
    }

    fn places(&self, src: Source) -> &Places {
        match src {
            Source::Body => &self.tags.places,
            Source::Instance(i) => &self.instances[i].places,
            Source::Loose => &self.loose,
        }
    }

    /// The places that `has` what is drawn at `at` on `page`: the body's, else those of the innermost sub-template
    /// body shown there, else the loose paragraphs'.
    fn find(&self, page: usize, at: kurbo::Point, has: impl Fn(&Places) -> bool) -> Option<Source> {
        if has(&self.tags.places) {
            return Some(Source::Body);
        }
        let inner = self
            .instances
            .iter()
            .enumerate()
            .filter(|(_, i)| i.page == page && i.area.contains(at) && has(&i.places))
            .min_by(|a, b| a.1.area.area().total_cmp(&b.1.area.area()))
            .map(|(k, _)| Source::Instance(k));
        inner.or_else(|| has(&self.loose).then_some(Source::Loose))
    }

    /// The pieces of item `p` on page `page`, in order, each with its mark, into `out`. `line`: the paragraph of the
    /// line it is on.
    pub fn mark(&mut self, page: usize, p: &Placed<'_>, line: Option<ParaId>, out: &mut Vec<(Mark, Item)>) {
        let float = p.float.map(|f| f.0);
        let at = Line { page, id: p.line, para: line };
        match p.absolute() {
            Item::Glyphs(r) => self.glyphs(at, r, None, out),
            item @ (Item::Rect { .. } | Item::Gridline { .. }) => out.push((Mark::Artifact(ArtifactKind::Layout), item)),
            item @ Item::Image { .. } => {
                let Item::Image { rect, of, .. } = &item else { return };
                let mark = self.picture(at, *of, float, *rect, None);
                out.push((mark, item));
            }
            Item::Shape(s) => {
                let of = if s.of == PictureOf::None && s.float != 0 { PictureOf::Float(crate::layout::FloatId(s.float)) } else { s.of };
                let mark = self.picture(at, of, float, s.rect, None);
                out.push((mark, Item::Shape(s)));
            }
            Item::Turned(t) => self.turned(at, t, float, out),
        }
    }

    /// A picture's or shape's mark: the inline picture or float it draws, else the float it is drawn in, as tagged;
    /// else the artifact of the header or footer it is in, else a layout artifact. A linked picture adds its box to
    /// its link's area. `rot`: the turn it is drawn under.
    fn picture(&mut self, line: Line, of: PictureOf, float: Option<u64>, rect: [f64; 4], rot: Option<Affine>) -> Mark {
        let page = line.page;
        let centre = kurbo::Point::new((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
        let at = rot.map_or(centre, |t| t * centre);
        let tag = |m: &Self, f: &dyn Fn(&Places) -> Option<Tag>| m.find(page, at, |pl| f(pl).is_some()).and_then(|s| f(m.places(s)));
        let found = match of {
            PictureOf::Inline(p, u) => tag(self, &|pl| pl.inline_picture(p.0, u.0)),
            PictureOf::Float(f) => tag(self, &|pl| pl.float(f.0)),
            PictureOf::None => None,
        };
        let found = found.or_else(|| float.and_then(|f| tag(self, &|pl| pl.float(f))));
        match found {
            Some(Tag::Content(c)) => {
                self.drawn[c as usize] = true;
                if let Some(&link) = self.tags.link_of.get(&c) {
                    let area = kurbo::Rect::new(rect[0], rect[1], rect[2], rect[3]);
                    self.link_area(link, line, rot.map(|_| centre.y), rot.map_or(area, |t| t.transform_rect_bbox(area)));
                }
                Mark::Content(c)
            }
            Some(Tag::Artifact(k)) => Mark::Artifact(k),
            None => {
                let para = match of {
                    PictureOf::Inline(p, _) => Some(p),
                    _ => line.para,
                };
                Mark::Artifact(para.and_then(|p| self.tags.artifact(p.0)).unwrap_or(ArtifactKind::Layout))
            }
        }
    }

    /// Turned text: its items marked, each stretch of one mark turned as the whole is.
    fn turned(&mut self, line: Line, t: Rc<Turned>, float: Option<u64>, out: &mut Vec<(Mark, Item)>) {
        let rot = Affine::rotate_about(t.deg.to_radians(), kurbo::Point::new(t.centre[0], t.centre[1]));
        let mut inner = Vec::new();
        for item in &t.items {
            match item {
                Item::Glyphs(r) => self.glyphs(line, r.clone(), Some(rot), &mut inner),
                Item::Image { rect, of, .. } => {
                    let mark = self.picture(line, *of, float, *rect, Some(rot));
                    inner.push((mark, item.clone()));
                }
                Item::Shape(s) => {
                    let of = if s.of == PictureOf::None && s.float != 0 { PictureOf::Float(crate::layout::FloatId(s.float)) } else { s.of };
                    let mark = self.picture(line, of, float, s.rect, Some(rot));
                    inner.push((mark, item.clone()));
                }
                Item::Turned(u) => self.turned(line, Rc::clone(u), float, &mut inner),
                other => inner.push((Mark::Artifact(ArtifactKind::Layout), other.clone())),
            }
        }
        if inner.windows(2).all(|w| w[0].0 == w[1].0) {
            if let Some(&(mark, _)) = inner.first() {
                out.push((mark, Item::Turned(t)));
            }
            return;
        }
        let mut items: Vec<Item> = Vec::new();
        let mut current = None;
        for (mark, item) in inner {
            if current.is_some_and(|c| c != mark) {
                let turned = Turned { centre: t.centre, deg: t.deg, items: std::mem::take(&mut items) };
                out.push((current.expect("a mark"), Item::Turned(Rc::new(turned))));
            }
            current = Some(mark);
            items.push(item);
        }
        if let Some(mark) = current {
            out.push((mark, Item::Turned(Rc::new(Turned { centre: t.centre, deg: t.deg, items }))));
        }
    }

    /// A glyph run's pieces: a header's, footer's or separator's run is its artifact; a tagged paragraph's run is split
    /// where its stretches change, a run before its text without units being its list number; another paragraph's
    /// is tagged as a loose paragraph; a run of no paragraph (a line number) is pagination. `rot`: the turn it is
    /// drawn under.
    fn glyphs(&mut self, line: Line, r: GlyphRun, rot: Option<Affine>, out: &mut Vec<(Mark, Item)>) {
        let page = line.page;
        let Some(para) = r.para.map(|p| p.0) else {
            out.push((Mark::Artifact(ArtifactKind::Pagination), Item::Glyphs(r)));
            return;
        };
        if let Some(k) = self.tags.artifact(para) {
            out.push((Mark::Artifact(k), Item::Glyphs(r)));
            return;
        }
        let origin = kurbo::Point::new(r.x, r.baseline);
        let at = rot.map_or(origin, |t| t * origin);
        let src = match self.find(page, at, |pl| pl.stretches(para).is_some()) {
            Some(s) => s,
            None => {
                let (node, stretches) = self.tags.loose();
                self.drawn.resize(self.tags.content_ids() as usize + 1, false);
                self.loose.insert(para, stretches);
                self.loose_nodes.push(node);
                Source::Loose
            }
        };
        // A header row drawn again on a later page.
        if self.tags.repeated.contains(&para)
            && let Some(u) = r.chars.iter().find_map(|c| c.2)
        {
            match self.header_runs.entry((src.key(), para, u.0)) {
                Entry::Occupied(e) if *e.get() < page => {
                    out.push((Mark::Artifact(ArtifactKind::Pagination), Item::Glyphs(r)));
                    return;
                }
                Entry::Occupied(_) => {}
                Entry::Vacant(e) => {
                    e.insert(page);
                }
            }
        }
        if matches!(src, Source::Body) {
            self.bookmarks(page, para, &r);
        }
        let places = self.places(src);
        let stretches = places.stretches(para).unwrap_or_default();
        let label = places.label(para);
        if stretches.is_empty() && label.is_none() {
            // A paragraph without text: its caret stop.
            out.push((Mark::Artifact(ArtifactKind::Other), Item::Glyphs(r)));
            return;
        }
        // A numbered heading's number is its text's first content, and reads apart from it.
        let heading = label.is_some() && stretches.first().map(|s| s.1) == label;
        let content = |u: u32| stretches.get(stretches.partition_point(|s| s.0 <= u).saturating_sub(1)).map(|s| s.1);
        // Each character's content: its unit's stretch's, the list number's before the paragraph's text, else its
        // neighbours'.
        let key = (src.key(), para);
        let mut started = self.started.contains(&key);
        let of_char: Vec<Option<u32>> = r
            .chars
            .iter()
            .map(|ch| match ch.2 {
                Some(u) => {
                    started = true;
                    content(u.0).or(label)
                }
                None if !started => label,
                None => None,
            })
            .collect();
        let fallback = if self.last.0 == key { Some(self.last.1) } else { stretches.first().map(|s| s.1).or(label) };
        let first = of_char.iter().flatten().next().copied().or(fallback).unwrap_or_default();
        let split = of_char.iter().flatten().any(|&c| c != first);
        // The stretches of glyphs of one content: (content, glyphs, characters).
        let mut parts: Vec<(u32, std::ops::Range<usize>, std::ops::Range<usize>)> = Vec::new();
        match split.then(|| glyph_pieces(&r)).flatten() {
            None => parts.push((first, 0..r.glyphs.len(), 0..r.chars.len())),
            Some((pieces, piece_of)) => {
                // Each piece's content: its first character's, else the piece's before it (or the first).
                let mut of_piece: Vec<u32> = Vec::with_capacity(pieces.len());
                for &(_, at, n) in &pieces {
                    let own = of_char[at..at + n].iter().flatten().next().copied();
                    of_piece.push(own.or(of_piece.last().copied()).unwrap_or(first));
                }
                let mut g0 = 0;
                while g0 < r.glyphs.len() {
                    let c = of_piece[piece_of[g0]];
                    let g1 = (g0..r.glyphs.len()).find(|&g| of_piece[piece_of[g]] != c).unwrap_or(r.glyphs.len());
                    let from = if g0 == 0 { 0 } else { pieces[piece_of[g0]].1 };
                    let to = if g1 == r.glyphs.len() { r.chars.len() } else { pieces[piece_of[g1]].1 };
                    parts.push((c, g0..g1, from..to));
                    g0 = g1;
                }
            }
        }
        if started {
            self.started.insert(key);
        }
        if parts.len() == 1 {
            let c = parts[0].0;
            let r = if heading { spaced(r) } else { r };
            let mark = self.text_mark(c, Some(c) == label, &r, line, rot);
            if Some(c) != label {
                self.last = (key, c);
            }
            out.push((mark, Item::Glyphs(r)));
            return;
        }
        for (c, glyphs, chars) in parts {
            let part = GlyphRun {
                face: r.face.clone(),
                size: r.size,
                baseline: r.baseline,
                x: r.glyphs[glyphs.start].1,
                glyphs: r.glyphs[glyphs].into(),
                chars: r.chars[chars].into(),
                colour: r.colour,
                para: r.para,
                paint: r.paint.clone(),
            };
            let part = if heading { spaced(part) } else { part };
            let mark = self.text_mark(c, Some(c) == label, &part, line, rot);
            if Some(c) != label {
                self.last = (key, c);
            }
            out.push((mark, Item::Glyphs(part)));
        }
    }

    /// The mark of run `r`, text of content `c` (a list number's when `label`): white space alone (an empty
    /// paragraph's caret stop) is an artifact until the content has drawn more. A link's text adds to its area.
    fn text_mark(&mut self, c: u32, label: bool, r: &GlyphRun, line: Line, rot: Option<Affine>) -> Mark {
        if r.chars.iter().any(|ch| (label || ch.2.is_some()) && !ch.0.is_whitespace()) {
            self.drawn[c as usize] = true;
        }
        if !self.drawn[c as usize] {
            return Mark::Artifact(ArtifactKind::Other);
        }
        if let Some(&link) = self.tags.link_of.get(&c)
            && let (Some(first), Some(last)) = (r.glyphs.first(), r.glyphs.last())
        {
            let scale = r.face.scale(r.size);
            let x1 = last.1 + r.face.advance(last.0) * scale;
            let area = kurbo::Rect::new(first.1, r.baseline - r.face.ascent * scale, x1, r.baseline + r.face.descent.abs() * scale);
            self.link_area(link, line, rot.map(|_| r.baseline), rot.map_or(area, |t| t.transform_rect_bbox(area)));
        }
        Mark::Content(c)
    }

    /// Adds `area` to link `link`'s area on `line` (at `turned`, the baseline in turned text).
    fn link_area(&mut self, link: u32, line: Line, turned: Option<f64>, area: kurbo::Rect) {
        let key = (link, line.page, (line.id, turned.map(|y| (y * 4.0).round() as i64)));
        match self.area_at.get(&key) {
            Some(&i) => self.areas[i].2 = self.areas[i].2.union(area),
            None => {
                self.area_at.insert(key, self.areas.len());
                self.areas.push((link, line.page, area));
            }
        }
    }

    /// The bookmarks of `para` the run's characters are at or after.
    fn bookmarks(&mut self, page: usize, para: u64, r: &GlyphRun) {
        let Some(list) = self.tags.bookmarks.get(&para) else { return };
        let next = self.next_bookmark.entry(para).or_insert(0);
        for &(_, x, u) in r.chars.iter() {
            let Some(u) = u else { continue };
            while let Some((at, name)) = list.get(*next)
                && *at <= u.0
            {
                self.dests.push((page, name.clone(), kurbo::Point::new(x, r.baseline - r.size)));
                *next += 1;
            }
        }
    }

    /// The structure without what wasn't drawn, the outline, and each page's links and named destinations: a
    /// bookmark no character follows is at its paragraph's first line, a link to a bookmark that isn't drawn is none.
    /// A sub-template body shown follows its reference's paragraph; loose paragraphs follow the body.
    pub fn finish(mut self, pages: &mut [pdf::Page]) -> Structure {
        for (para, list) in &self.tags.bookmarks {
            let found = self.next_bookmark.get(para).copied().unwrap_or(0);
            if let Some(&(page, at)) = self.first_lines.get(para) {
                self.dests.extend(list[found..].iter().map(|(_, name)| (page, name.clone(), at)));
            }
        }
        // In document order, the first of a name kept.
        self.dests.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.y.total_cmp(&b.2.y)).then(a.2.x.total_cmp(&b.2.x)));
        let mut named = HashSet::new();
        for (page, name, at) in std::mem::take(&mut self.dests) {
            if let Some(p) = pages.get_mut(page)
                && named.insert(name.clone())
            {
                p.destinations.push(pdf::Destination { name, at });
            }
        }
        let mut annotated = vec![false; self.tags.links.len()];
        for &(link, page, rect) in &self.areas {
            let Some(l) = self.tags.links.get(link as usize) else { continue };
            let target = match &l.target {
                Target::Uri(u) => pdf::LinkTarget::Uri(u.clone()),
                Target::Bookmark(b) if named.contains(b) => pdf::LinkTarget::Named(b.clone()),
                Target::Bookmark(_) => continue,
            };
            if let Some(p) = pages.get_mut(page) {
                p.links.push(pdf::Link { link, rect, target, description: l.description.clone() });
                annotated[link as usize] = true;
            }
        }
        let mut root = std::mem::take(&mut self.tags.root);
        for i in std::mem::take(&mut self.instances) {
            match i.after {
                Some(c) => insert_after(&mut root, c, i.node),
                None => root.push(i.node),
            }
        }
        root.append(&mut self.loose_nodes);
        let drawn = |c: u32| self.drawn.get(c as usize).copied().unwrap_or(false);
        let outline = self
            .tags
            .headings
            .iter()
            .filter_map(|h| {
                h.content.iter().copied().find(|&c| drawn(c)).map(|content| pdf::Heading { level: h.level, content, title: h.title.clone() })
            })
            .collect();
        let missing_alt = self.tags.missing_alt.iter().filter(|&&c| drawn(c)).count();
        let kept = Kept { drawn, annotated: |l: u32| annotated.get(l as usize).copied().unwrap_or(false), lists: &self.tags.lists };
        let mut elements: Vec<pdf::Element> =
            prune(root, &kept).into_iter().filter_map(|n| if let Node::Element(e) = n { Some(e) } else { None }).collect();
        heading_levels(&mut elements);
        Structure { elements, outline, missing_alt }
    }
}

/// A heading's number (a run of no unit) with a space after its characters, so that it reads apart from the text.
fn spaced(r: GlyphRun) -> GlyphRun {
    match r.chars.last() {
        Some(&(c, x, None)) if !c.is_whitespace() && r.chars.iter().all(|ch| ch.2.is_none()) => {
            let mut chars = r.chars.into_vec();
            chars.push((' ', x, None));
            GlyphRun { chars: chars.into(), ..r }
        }
        _ => r,
    }
}

/// Inserts `node` after the paragraph (or heading, or list item body) that holds content `c`, else at the end.
fn insert_after(root: &mut Vec<Node>, c: u32, node: Node) {
    /// The kids' indexes down to content `c`.
    fn path_to(nodes: &[Node], c: u32, path: &mut Vec<usize>) -> bool {
        for (i, n) in nodes.iter().enumerate() {
            path.push(i);
            match n {
                Node::Content(d) if *d == c => return true,
                Node::Element(e) if path_to(&e.kids, c, path) => return true,
                _ => {}
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    if !path_to(root, c, &mut path) {
        root.push(node);
        return;
    }
    // The deepest block on the path: the paragraph.
    let mut block = None;
    let mut nodes: &[Node] = root;
    for (d, &i) in path.iter().enumerate() {
        let Node::Element(e) = &nodes[i] else { break };
        if matches!(e.kind, Kind::Paragraph | Kind::Heading(_) | Kind::ListBody) {
            block = Some(d);
        }
        nodes = &e.kids;
    }
    let Some(d) = block else {
        root.push(node);
        return;
    };
    let mut kids = root;
    for &i in &path[..d] {
        let Node::Element(e) = &mut kids[i] else { unreachable!("the path goes through elements") };
        kids = &mut e.kids;
    }
    kids.insert(path[d] + 1, node);
}

/// The heading tags in reading order from H1, each at most one level below the one before (PDF/UA's heading rule);
/// the outline keeps the headings' own levels.
fn heading_levels(elements: &mut [pdf::Element]) {
    let mut last = 0u8;
    let mut todo: Vec<&mut pdf::Element> = elements.iter_mut().rev().collect();
    while let Some(e) = todo.pop() {
        if let pdf::Kind::Heading(level) = &mut e.kind {
            *level = (*level).min(last + 1);
            last = *level;
        }
        todo.extend(e.kids.iter_mut().rev().filter_map(|k| if let Node::Element(k) = k { Some(k) } else { None }));
    }
}
