//! Tagged structure: the document's structure elements in reading order, built from the model, and where each
//! paragraph's text, list number, pictures, links and notes sit in them. Headings, alternative text, header cells and
//! languages are derived as the accessibility tree derives them ([`crate::services::a11y::roles`]). Model nesting is
//! bounded by the reader ([`crate::docx::xml::MAX_DEPTH`]), so the walks recurse; notes referenced from notes are
//! walked from a queue. A sub-template's body is walked where it is shown ([`Tags::instance`]).

use std::collections::{HashMap, HashSet};

use twip_graphics::pdf::{ArtifactKind, Cell, Element, Kind, Node, Numbering, Scope};

use crate::docx::grid::Grid;
use crate::docx::{Block, Document, Inline, NoteKind, Paragraph, Table, VMerge, fields};
use crate::services::a11y::roles::{Alt, HeaderScope, alt, document_language, header_scopes, heading_level, other_language};

/// Bookmarks kept as named destinations, at most.
const MAX_BOOKMARKS: usize = 100_000;
/// A link's description, at most this many characters.
const MAX_DESCRIPTION: usize = 1000;

/// Where a link goes: a URI, or a bookmark's place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Uri(String),
    Bookmark(String),
}

/// A link: where it goes and what it says.
pub(super) struct LinkTag {
    pub target: Target,
    pub description: String,
}

/// A picture's or float's place in the structure: its figure's content, or an artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tag {
    Content(u32),
    Artifact(ArtifactKind),
}

/// A heading for the outline: its level (1 to 9), text, and content ids in order.
pub(super) struct Heading {
    pub level: u8,
    pub title: String,
    pub content: Vec<u32>,
}

/// Where a story's drawing goes in the structure, by model identity: each tagged paragraph's stretches ((first unit,
/// content id), in unit order) and its list number's content, inline pictures by (paragraph, unit), floats by id.
#[derive(Default)]
pub(super) struct Places {
    paras: HashMap<u64, Vec<(u32, u32)>>,
    labels: HashMap<u64, u32>,
    inline: HashMap<(u64, u32), Tag>,
    floats: HashMap<u64, Tag>,
}

impl Places {
    /// A paragraph's stretches, when it is tagged.
    pub fn stretches(&self, para: u64) -> Option<&[(u32, u32)]> {
        self.paras.get(&para).map(Vec::as_slice)
    }

    /// A list item's number's content.
    pub fn label(&self, para: u64) -> Option<u32> {
        self.labels.get(&para).copied()
    }

    /// The inline picture at `unit` of `para`.
    pub fn inline_picture(&self, para: u64, unit: u32) -> Option<Tag> {
        self.inline.get(&(para, unit)).copied()
    }

    pub fn float(&self, id: u64) -> Option<Tag> {
        self.floats.get(&id).copied()
    }

    /// Tags paragraph `para` with `stretches`.
    pub fn insert(&mut self, para: u64, stretches: Vec<(u32, u32)>) {
        self.paras.insert(para, stretches);
    }
}

/// The document's structure and the content ids its drawing is marked with.
#[derive(Default)]
pub(super) struct Tags {
    /// The top-level elements, in reading order.
    pub root: Vec<Node>,
    /// The body's, its notes' and its text boxes' places.
    pub places: Places,
    /// The document's language: text in another is a span in it.
    pub lang: String,
    /// Links by id ([`Node::Annotation`]).
    pub links: Vec<LinkTag>,
    /// The content ids that are a link's text, and its link.
    pub link_of: HashMap<u32, u32>,
    pub headings: Vec<Heading>,
    /// Bookmark starts by paragraph: (unit, name), in unit order.
    pub bookmarks: HashMap<u64, Vec<(u32, String)>>,
    /// Paragraphs of table header rows: drawn again on a later page, a repeated header is pagination.
    pub repeated: HashSet<u64>,
    /// The first body paragraph in a style named Title: its text.
    pub title: Option<String>,
    /// The figures without alternative text (not decorative): their content ids.
    pub missing_alt: HashSet<u32>,
    /// Header, footer and note separator paragraphs.
    artifacts: HashMap<u64, ArtifactKind>,
    next: u32,
    tables: u32,
    bookmark_count: usize,
    /// The notes walked or queued, and those referenced from notes, to walk after the body.
    notes: HashSet<(bool, i64)>,
    queued: Vec<(NoteKind, i64)>,
    in_note: bool,
    in_header_row: bool,
    /// Walking a sub-template's body where it is shown: no bookmarks, headings or title of its own.
    in_instance: bool,
}

impl Tags {
    /// The structure of `doc`'s body, its notes and text boxes; headers, footers and note separators are artifacts.
    /// `lang`: the document's language when its defaults name none.
    pub fn of(doc: &Document, lang: &str) -> Tags {
        let mut t = Tags { lang: document_language(doc, lang), ..Tags::default() };
        for s in &doc.sections {
            for (refs, kind) in [(&s.headers, ArtifactKind::Header), (&s.footers, ArtifactKind::Footer)] {
                for name in refs.iter().flatten() {
                    if let Some(blocks) = doc.stories.get(name) {
                        t.artifact_blocks(blocks, kind);
                    }
                }
            }
        }
        for blocks in doc.separators.values() {
            t.artifact_blocks(blocks, ArtifactKind::Pagination);
        }
        let mut root = Vec::new();
        for s in &doc.sections {
            t.blocks(doc, &s.blocks, &mut root);
        }
        // Notes referenced from notes, after the body: the queue grows while it is walked.
        t.in_note = true;
        let mut i = 0;
        while let Some(&(kind, id)) = t.queued.get(i) {
            i += 1;
            if let Some(n) = t.note_element(doc, kind, id) {
                root.push(n);
            }
        }
        t.in_note = false;
        t.root = root;
        t
    }

    /// The artifact a header's, footer's or note separator's paragraph is.
    pub fn artifact(&self, para: u64) -> Option<ArtifactKind> {
        self.artifacts.get(&para).copied()
    }

    /// How many content ids there are: each is at most this.
    pub fn content_ids(&self) -> u32 {
        self.next
    }

    /// Building block `part`'s body shown at a sub-template reference: its elements in a division, with fresh content
    /// ids (a body shown at two references is two instances), and its places.
    pub fn instance(&mut self, doc: &Document, part: u32) -> Option<(Node, Places)> {
        let body = &doc.glossary.as_ref()?.parts.get(part as usize)?.body;
        let outer = std::mem::take(&mut self.places);
        let was = std::mem::replace(&mut self.in_instance, true);
        let mut d = Element::new(Kind::Division, Vec::new());
        self.blocks(doc, body, &mut d.kids);
        self.in_instance = was;
        Some((Node::Element(d), std::mem::replace(&mut self.places, outer)))
    }

    /// A paragraph drawn that no story tags (a sub-template body's row shown in a table): a paragraph of one stretch,
    /// for after the body.
    pub fn loose(&mut self) -> (Node, Vec<(u32, u32)>) {
        let c = self.id();
        (Node::Element(Element::new(Kind::Paragraph, vec![Node::Content(c)])), vec![(0, c)])
    }

    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    /// Marks a header's, footer's or separator's paragraphs, text boxes and floats as `kind` artifacts.
    fn artifact_blocks(&mut self, blocks: &[Block], kind: ArtifactKind) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    self.artifacts.insert(p.id, kind);
                    for i in &p.inlines {
                        if let Inline::Float(f, _) = i {
                            self.places.floats.insert(f.id, Tag::Artifact(kind));
                            for tb in f.text_box.iter().chain(f.parts.iter().filter_map(|p| p.text_box.as_ref())) {
                                self.artifact_blocks(&tb.blocks, kind);
                            }
                        }
                        if let Inline::Picture(pic, _) = i
                            && let Some(t) = &pic.text
                        {
                            self.artifact_blocks(&t.text_box.blocks, kind);
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| self.artifact_blocks(&c.blocks, kind)),
            }
        }
    }

    /// Blocks into `out`: consecutive list items make lists, nested by level (a deeper list in the item before it).
    fn blocks(&mut self, doc: &Document, blocks: &[Block], out: &mut Vec<Node>) {
        let mut lists: Vec<(i64, Element)> = Vec::new();
        for b in blocks {
            match b {
                Block::Para(p) if heading_level(doc, p).is_none() && p.list.is_some() => {
                    let level = p.style.num.map_or(0, |n| n.1).max(0);
                    while lists.last().is_some_and(|l| l.0 > level) {
                        close(&mut lists, out);
                    }
                    if lists.last().is_none_or(|l| l.0 < level) {
                        lists.push((level, Element::new(Kind::List(numbering(p.style.list_fmt.as_deref())), Vec::new())));
                    }
                    let mut item = Vec::new();
                    self.para(doc, p, &mut item);
                    lists.last_mut().expect("a list was pushed").1.kids.extend(item);
                }
                Block::Para(p) => {
                    while !lists.is_empty() {
                        close(&mut lists, out);
                    }
                    self.para(doc, p, out);
                }
                Block::Table(t) => {
                    while !lists.is_empty() {
                        close(&mut lists, out);
                    }
                    self.table(doc, t, out);
                }
            }
        }
        while !lists.is_empty() {
            close(&mut lists, out);
        }
    }

    /// A paragraph: a heading, a list item (its number, then its body; consecutive ones share a list) or a
    /// paragraph, its text in stretches around its links, inline pictures, text boxes, notes and language changes,
    /// then its floats.
    fn para(&mut self, doc: &Document, p: &Paragraph, out: &mut Vec<Node>) {
        let level = heading_level(doc, p);
        let item = level.is_none() && p.list.is_some();
        let kind = match level {
            Some(l) => Kind::Heading(l.min(6) as u8),
            None if item => Kind::ListBody,
            None => Kind::Paragraph,
        };
        if self.in_header_row {
            self.repeated.insert(p.id);
        }
        if self.title.is_none() && !self.in_note && !self.in_instance && style_name(doc, p).is_some_and(|n| n.eq_ignore_ascii_case("title")) {
            self.title = Some(fields::text(p).trim().to_string()).filter(|t| !t.is_empty());
        }
        let mut e = Element::new(kind, Vec::new());
        let mut stretches = Vec::new();
        let mut floats = Vec::new();
        self.inlines(doc, p, &mut e.kids, &mut stretches, &mut floats);
        e.kids.extend(floats);
        if let Some(level) = level.filter(|_| !self.in_instance) {
            let content = stretches.iter().map(|s| s.1).collect();
            self.headings.push(Heading { level: level as u8, title: fields::text(p).trim().to_string(), content });
        }
        self.places.paras.insert(p.id, stretches);
        if !item {
            out.push(Node::Element(e));
            return;
        }
        let label = self.id();
        self.places.labels.insert(p.id, label);
        out.push(Node::Element(Element::new(
            Kind::ListItem,
            vec![Node::Element(Element::new(Kind::Label, vec![Node::Content(label)])), Node::Element(e)],
        )));
    }

    /// A text stretch from unit `u` in language `lang` (where it differs from the document's): its content, in a span
    /// of its language.
    fn text(&mut self, u: u32, lang: Option<&str>, stretches: &mut Vec<(u32, u32)>, to: &mut Vec<Node>) -> u32 {
        let c = self.id();
        stretches.push((u, c));
        to.push(match lang {
            Some(l) => Node::Element(Element { kind: Kind::Span, id: None, lang: Some(l.to_string()), kids: vec![Node::Content(c)] }),
            None => Node::Content(c),
        });
        c
    }

    fn inlines(&mut self, doc: &Document, p: &Paragraph, kids: &mut Vec<Node>, stretches: &mut Vec<(u32, u32)>, floats: &mut Vec<Node>) {
        let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, at.saturating_add(i.units() as u32)))).collect();
        let links = self.links(p);
        let mut next_link = 0;
        // The open text stretch's language, and the open link: (its end inline, its element's place in `kids`, its
        // id). Outer `None`: no stretch is open.
        let mut text: Option<Option<String>> = None;
        let mut link: Option<(usize, usize, u32)> = None;
        // The language the paragraph's text is in, where it isn't the document's.
        let lang_of = |i: &Inline, doc_lang: &str| i.run().and_then(|r| other_language(&r.lang, doc_lang)).map(str::to_string);
        for (k, inline) in p.inlines.iter().enumerate() {
            let u = starts[k];
            if let Some(&(begin, end, id)) = links.get(next_link)
                && begin == k
            {
                next_link += 1;
                kids.push(Node::Element(Element::new(Kind::Link, vec![Node::Annotation(id)])));
                link = Some((end, kids.len() - 1, id));
                text = None;
            }
            // A block inside the paragraph's text: the text after it is a new stretch.
            let inside = |kids: &mut Vec<Node>, node: Node| match link {
                Some((_, at, _)) => {
                    if let Some(Node::Element(l)) = kids.get_mut(at) {
                        l.kids.push(node);
                    }
                }
                None => kids.push(node),
            };
            match inline {
                Inline::Bookmark(name) if !self.in_instance && !name.starts_with('/') && self.bookmark_count < MAX_BOOKMARKS => {
                    self.bookmark_count += 1;
                    self.bookmarks.entry(p.id).or_default().push((u, name.clone()));
                }
                Inline::Picture(pic, _) => match (&pic.text, alt(&pic.alt, pic.shape.as_deref())) {
                    (Some(t), _) => {
                        self.places.inline.insert((p.id, u), Tag::Artifact(ArtifactKind::Layout));
                        let mut d = Element::new(Kind::Division, Vec::new());
                        self.blocks(doc, &t.text_box.blocks, &mut d.kids);
                        inside(kids, Node::Element(d));
                        text = None;
                    }
                    (None, Alt::Decorative) => {
                        self.places.inline.insert((p.id, u), Tag::Artifact(ArtifactKind::Layout));
                    }
                    (None, a) => {
                        let f = self.figure(a);
                        self.places.inline.insert((p.id, u), Tag::Content(f.0));
                        inside(kids, f.1);
                        text = None;
                    }
                },
                Inline::Float(f, _) => {
                    let boxes: Vec<&crate::docx::TextBox> = f.text_box.iter().chain(f.parts.iter().filter_map(|p| p.text_box.as_ref())).collect();
                    match alt(&f.alt, f.shape.as_deref()) {
                        Alt::Decorative => {
                            self.places.floats.insert(f.id, Tag::Artifact(ArtifactKind::Layout));
                        }
                        // A text box's frame: its text is what it says.
                        _ if !boxes.is_empty() => {
                            self.places.floats.insert(f.id, Tag::Artifact(ArtifactKind::Layout));
                        }
                        a => {
                            let (c, node) = self.figure(a);
                            self.places.floats.insert(f.id, Tag::Content(c));
                            floats.push(node);
                        }
                    }
                    for tb in boxes {
                        let mut d = Element::new(Kind::Division, Vec::new());
                        self.blocks(doc, &tb.blocks, &mut d.kids);
                        floats.push(Node::Element(d));
                    }
                }
                Inline::NoteRef { kind, id, .. } => {
                    // The reference is the text's; its note follows it.
                    if text.is_none() {
                        let lang = lang_of(inline, &self.lang);
                        self.open(u, lang.as_deref(), link, stretches, kids);
                    }
                    if let Some(n) = self.note(doc, *kind, *id) {
                        inside(kids, n);
                    }
                    text = None;
                }
                _ if inline.units() > 0 => {
                    let lang = lang_of(inline, &self.lang);
                    if text.as_ref() != Some(&lang) {
                        self.open(u, lang.as_deref(), link, stretches, kids);
                        text = Some(lang);
                    }
                }
                _ => {}
            }
            if link.is_some_and(|l| l.0 == k) {
                link = None;
                text = None;
            }
        }
    }

    /// Opens a text stretch at unit `u`: in the open link's element when there is one (a link's text).
    fn open(&mut self, u: u32, lang: Option<&str>, link: Option<(usize, usize, u32)>, stretches: &mut Vec<(u32, u32)>, kids: &mut Vec<Node>) {
        match link {
            Some((_, at, id)) => {
                let mut to = Vec::new();
                let c = self.text(u, lang, stretches, &mut to);
                self.link_of.insert(c, id);
                if let Some(Node::Element(l)) = kids.get_mut(at) {
                    l.kids.extend(to);
                }
            }
            None => {
                self.text(u, lang, stretches, kids);
            }
        }
    }

    /// A figure of `a` (its alternative text, or none when it has none): its content and element.
    fn figure(&mut self, a: Alt<'_>) -> (u32, Node) {
        let c = self.id();
        let text = match a {
            Alt::Text(t) => Some(t.to_string()),
            Alt::Missing | Alt::Decorative => {
                self.missing_alt.insert(c);
                None
            }
        };
        (c, Node::Element(Element::new(Kind::Figure(text), vec![Node::Content(c)])))
    }

    /// A paragraph's links: (begin inline, end inline, link id) of each HYPERLINK field and each REF, PAGEREF and
    /// NOTEREF field that links to its bookmark (`\h`), outermost only.
    fn links(&mut self, p: &Paragraph) -> Vec<(usize, usize, u32)> {
        let mut out: Vec<(usize, usize, u32)> = Vec::new();
        for s in fields::spans(&p.inlines) {
            if out.last().is_some_and(|l| s.begin < l.1) {
                continue;
            }
            let code = fields::parse(&fields::code_text(&p.inlines, s));
            let Some(target) = link_target(&code) else { continue };
            let shown = s.sep.map(|sep| fields::visible(&p.inlines[sep + 1..s.end])).unwrap_or_default();
            let tip = code.switches.iter().find(|w| w.flag == 'o').and_then(|w| w.arg.clone()).unwrap_or_default();
            let description = [tip.trim(), shown.trim()].into_iter().find(|t| !t.is_empty()).map_or_else(
                || match &target {
                    Target::Uri(u) | Target::Bookmark(u) => u.clone(),
                },
                str::to_string,
            );
            let id = self.links.len() as u32;
            self.links.push(LinkTag { target, description: description.chars().take(MAX_DESCRIPTION).collect() });
            out.push((s.begin, s.end, id));
        }
        out
    }

    /// A note's element where it is referenced, the first time; from inside a note, it is queued for after the body.
    fn note(&mut self, doc: &Document, kind: NoteKind, id: i64) -> Option<Node> {
        if self.in_note {
            if self.notes.insert((kind == NoteKind::End, id)) {
                self.queued.push((kind, id));
            }
            return None;
        }
        if !self.notes.insert((kind == NoteKind::End, id)) {
            return None;
        }
        self.in_note = true;
        let n = self.note_element(doc, kind, id);
        self.in_note = false;
        n
    }

    fn note_element(&mut self, doc: &Document, kind: NoteKind, id: i64) -> Option<Node> {
        let store = if kind == NoteKind::End { &doc.endnotes } else { &doc.footnotes };
        let blocks = store.get(&id)?;
        let mut e = Element::new(Kind::Note, Vec::new());
        e.id = Some(format!("{}-{id}", if kind == NoteKind::End { "endnote" } else { "footnote" }));
        self.blocks(doc, blocks, &mut e.kids);
        Some(Node::Element(e))
    }

    /// A table: its rows (not those hidden whole), and their cells with what they head, the header cells they list and
    /// their spans; a vertical merge's continuation cells are part of the cell above, and empty cells stand in for the
    /// grid columns a row leaves out, so every row has the table's columns. Cell ids are the table's own, made unique
    /// in the document.
    fn table(&mut self, doc: &Document, t: &Table, out: &mut Vec<Node>) {
        self.tables += 1;
        let n = self.tables;
        let grid = Grid::of(t);
        let summary = t.alt.as_deref().and_then(|a| {
            let parts: Vec<&str> = [a.title.trim(), a.description.trim()].into_iter().filter(|s| !s.is_empty()).collect();
            (!parts.is_empty()).then(|| parts.join(". "))
        });
        let mut table = Element::new(Kind::Table(summary), Vec::new());
        let header_row = self.in_header_row;
        let gap = |cols: usize| {
            let attrs = Cell { scope: None, headers: Vec::new(), row_span: 1, col_span: cols as u32 };
            Node::Element(Element::new(Kind::Cell(attrs), Vec::new()))
        };
        for (r, row) in t.rows.iter().enumerate() {
            if row.vanished {
                continue;
            }
            let scopes = header_scopes(t, r);
            let mut tr = Element::new(Kind::Row, Vec::new());
            let cells = grid.rows.get(r).map_or(&[][..], Vec::as_slice);
            let (start, end) = (cells.first().map_or(0, |c| c.0), cells.last().map_or(0, |c| c.0 + c.1));
            if start > 0 {
                tr.kids.push(gap(start));
            }
            for (k, cell) in row.cells.iter().enumerate() {
                let (col, span, merge) = cells.get(k).copied().unwrap_or((k, 1, VMerge::None));
                if merge == VMerge::Continue {
                    continue;
                }
                let continues = |g: &Vec<(usize, usize, VMerge)>| g.iter().any(|&(c, _, m)| c == col && m == VMerge::Continue);
                let below = t.rows.iter().zip(&grid.rows).skip(r + 1).take_while(|(_, g)| continues(g));
                let row_span = 1 + below.filter(|(row, _)| !row.vanished).count() as u32;
                let scope = scopes.get(k).copied().flatten().map(|s| match s {
                    HeaderScope::Row => Scope::Row,
                    HeaderScope::Column => Scope::Column,
                });
                let ids = cell.ids.as_deref();
                let attrs = Cell {
                    scope,
                    headers: ids.map(|i| i.headers.iter().map(|h| format!("t{n}-{h}")).collect()).unwrap_or_default(),
                    row_span,
                    col_span: span.max(1) as u32,
                };
                let mut c = Element::new(Kind::Cell(attrs), Vec::new());
                c.id = ids.and_then(|i| i.id.as_ref()).map(|id| format!("t{n}-{id}"));
                self.in_header_row = header_row || row.header;
                self.blocks(doc, &cell.blocks, &mut c.kids);
                self.in_header_row = header_row;
                tr.kids.push(Node::Element(c));
            }
            if grid.cols > end {
                tr.kids.push(gap(grid.cols - end));
            }
            table.kids.push(Node::Element(tr));
        }
        out.push(Node::Element(table));
    }
}

/// Where a field links to: a HYPERLINK's address (with its anchor) or bookmark, a REF's, PAGEREF's or NOTEREF's
/// bookmark when it is a link (`\h`). An address with a scheme other than the web's, mail's or a phone's is no link.
fn link_target(code: &fields::FieldCode) -> Option<Target> {
    let arg = code.args.first().map(|a| a.trim()).filter(|a| !a.is_empty());
    match code.name.as_str() {
        "HYPERLINK" => {
            let anchor = code.switches.iter().find(|s| s.flag == 'l').and_then(|s| s.arg.as_deref()).map(str::trim).filter(|a| !a.is_empty());
            match (arg, anchor) {
                (Some(uri), anchor) => {
                    let scheme = uri.split_once(':').map(|s| s.0).filter(|s| {
                        s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
                    });
                    if scheme.is_some_and(|s| !["http", "https", "mailto", "ftp", "tel"].iter().any(|k| s.eq_ignore_ascii_case(k))) {
                        return None;
                    }
                    Some(Target::Uri(match anchor {
                        Some(a) => format!("{uri}#{a}"),
                        None => uri.to_string(),
                    }))
                }
                (None, Some(a)) => Some(Target::Bookmark(a.to_string())),
                (None, None) => None,
            }
        }
        "REF" | "PAGEREF" | "NOTEREF" if code.switches.iter().any(|s| s.flag == 'h') => arg.map(|a| Target::Bookmark(a.to_string())),
        _ => None,
    }
}

/// Ends the innermost list being built: it goes into the body of the last item of the list around it (an item holds
/// only its label and body), or into `out`.
fn close(lists: &mut Vec<(i64, Element)>, out: &mut Vec<Node>) {
    let Some((_, list)) = lists.pop() else { return };
    let body = lists.last_mut().and_then(|l| match l.1.kids.last_mut() {
        Some(Node::Element(item)) => match item.kids.last_mut() {
            Some(Node::Element(b)) if b.kind == Kind::ListBody => Some(b),
            _ => None,
        },
        _ => None,
    });
    match body {
        Some(b) => b.kids.push(Node::Element(list)),
        None => out.push(Node::Element(list)),
    }
}

/// How a list numbered `fmt` (`w:numFmt`) numbers its items in PDF's terms: bullets as discs, numbers by their
/// system (decimal for systems PDF has no name for).
fn numbering(fmt: Option<&str>) -> Numbering {
    match fmt {
        Some("bullet") => Numbering::Disc,
        Some("none") | None => Numbering::None,
        Some("lowerRoman") => Numbering::LowerRoman,
        Some("upperRoman") => Numbering::UpperRoman,
        Some("lowerLetter") => Numbering::LowerAlpha,
        Some("upperLetter") => Numbering::UpperAlpha,
        Some(_) => Numbering::Decimal,
    }
}

/// The name of a paragraph's style.
fn style_name<'d>(doc: &'d Document, p: &Paragraph) -> Option<&'d str> {
    Some(doc.resolver.as_ref()?.styles.by_id.get(p.style.style_id.as_deref()?)?.name.as_str())
}

/// The document's title: its `title` property, else its first paragraph in the Title style, else its first heading,
/// else its file name.
pub(super) fn title(doc: &Document, tags: &Tags) -> String {
    let property = doc.properties.core("title").map(str::trim).filter(|t| !t.is_empty());
    let heading = tags.headings.iter().map(|h| h.title.as_str()).find(|t| !t.is_empty());
    let file = doc.file_name.as_deref().map(|f| {
        let name = f.rsplit(['/', '\\']).next().unwrap_or(f);
        name.rsplit_once('.').map_or(name, |n| n.0)
    });
    property.or(tags.title.as_deref()).or(heading).or(file.filter(|f| !f.trim().is_empty())).unwrap_or("Document").to_string()
}

/// `kids` without what wasn't drawn: content ids not `drawn`, annotations of links without any (`annotated`), and the
/// elements that leave empty, but the empty cells of a table with anything ([`prune_table`]). A link without
/// annotations gives its kids to its parent; consecutive lists numbered alike merge.
pub(super) fn prune(kids: Vec<Node>, drawn: &impl Fn(u32) -> bool, annotated: &impl Fn(u32) -> bool) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(kids.len());
    for k in kids {
        match k {
            Node::Content(c) if drawn(c) => out.push(Node::Content(c)),
            Node::Annotation(l) if annotated(l) => out.push(Node::Annotation(l)),
            Node::Content(_) | Node::Annotation(_) => {}
            Node::Element(mut e) => {
                let table = matches!(e.kind, Kind::Table(_));
                e.kids = if table { prune_table(e.kids, drawn, annotated) } else { prune(e.kids, drawn, annotated) };
                if e.kind == Kind::Link && !e.kids.iter().any(|k| matches!(k, Node::Annotation(_))) {
                    out.extend(e.kids);
                    continue;
                }
                if e.kids.is_empty() {
                    continue;
                }
                match out.last_mut() {
                    Some(Node::Element(last)) if matches!(e.kind, Kind::List(_)) && last.kind == e.kind => last.kids.append(&mut e.kids),
                    _ => out.push(Node::Element(e)),
                }
            }
        }
    }
    out
}

/// A table's cells pruned, each kept (an empty one too, so the rows stay regular) while the table has anything drawn.
fn prune_table(mut rows: Vec<Node>, drawn: &impl Fn(u32) -> bool, annotated: &impl Fn(u32) -> bool) -> Vec<Node> {
    let mut any = false;
    for r in &mut rows {
        let Node::Element(tr) = r else { continue };
        for c in &mut tr.kids {
            if let Node::Element(cell) = c {
                cell.kids = prune(std::mem::take(&mut cell.kids), drawn, annotated);
                any |= !cell.kids.is_empty();
            }
        }
    }
    if any { rows } else { Vec::new() }
}

#[cfg(test)]
#[path = "tags_tests.rs"]
mod tests;
