//! Fields and content controls

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::docx::fields::{code_text, result_text, spans};
use crate::docx::{Block, ControlKind, ControlProps, Document, ElementId, Inline, Paragraph};
use crate::layout::{FloatId, ParaId};

use super::index::DocIndex;
use super::position::{Position, StoryId};
use super::selection::Selection;

const DISPLAY: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub id: ElementId,
    pub code: Option<String>,
    pub props: Option<ControlProps>,
    pub display: String,
    pub story: StoryId,
    pub parent: Option<ElementId>,
    pub locked: bool,
    pub at: Option<ParaId>,
}

pub fn id_str(id: ElementId) -> String {
    match id {
        ElementId::Field(f) => format!("f{}", f.0),
        ElementId::Control(c) => format!("c{}", c.0),
    }
}

pub fn parse_id(s: &str) -> Option<ElementId> {
    let n = s.get(1..)?.parse().ok()?;
    match s.as_bytes().first()? {
        b'f' => Some(ElementId::Field(crate::docx::FieldId(n))),
        b'c' => Some(ElementId::Control(crate::docx::ControlId(n))),
        _ => None,
    }
}

pub fn kind_name(k: &ControlKind) -> &'static str {
    match k {
        ControlKind::PlainText { .. } => "plain_text",
        ControlKind::RichText => "rich_text",
        ControlKind::Picture => "picture",
        ControlKind::Checkbox { .. } => "checkbox",
        ControlKind::Date { .. } => "date",
        ControlKind::DropDown => "drop_down",
        ControlKind::Group => "group",
        ControlKind::RepeatingSection => "repeating_section",
        ControlKind::RepeatingItem => "repeating_item",
        ControlKind::Other => "other",
    }
}

impl Element {
    pub fn kind(&self) -> &'static str {
        self.props.as_ref().map_or("field", |p| kind_name(&p.kind))
    }

    pub fn signature(&self) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        self.code.hash(&mut h);
        if self.props.is_none() {
            self.display.hash(&mut h);
        }
        self.locked.hash(&mut h);
        self.parent.hash(&mut h);
        if let Some(p) = &self.props {
            (kind_name(&p.kind), &p.tag, &p.alias, p.lock.val(), p.showing_placeholder, &p.binding).hash(&mut h);
        }
        h.finish()
    }
}

pub(super) fn story_name(s: StoryId, idx: &DocIndex) -> String {
    match s {
        StoryId::Body => "body".into(),
        StoryId::HeaderFooter(k) => idx.part_name(k).unwrap_or("header").to_string(),
        StoryId::Footnote(id) => format!("footnote {id}"),
        StoryId::Endnote(id) => format!("endnote {id}"),
        StoryId::TextBox(f) => format!("text box {}", f.0),
        StoryId::BuildingBlock(k) => format!("building block {k}"),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub kind: Option<String>,
    pub matching: Option<(String, String)>,
    pub story: Option<String>,
}

impl Filter {
    fn keeps(&self, e: &Element, idx: &DocIndex) -> bool {
        self.kind.as_deref().is_none_or(|k| k == e.kind())
            && self.story.as_deref().is_none_or(|s| s == story_name(e.story, idx))
            && self.matching.as_ref().is_none_or(|(field, text)| {
                let v = match field.as_str() {
                    "code" => e.code.as_deref(),
                    "tag" => e.props.as_ref().and_then(|p| p.tag.as_deref()),
                    "alias" => e.props.as_ref().and_then(|p| p.alias.as_deref()),
                    _ => None,
                };
                v.is_some_and(|v| v.contains(text.as_str()))
            })
    }
}

fn stories<'d>(doc: &'d Document, idx: &DocIndex) -> Vec<(StoryId, &'d [Block])> {
    let mut out: Vec<(StoryId, &[Block])> = Vec::new();
    for s in &doc.sections {
        out.push((StoryId::Body, &s.blocks));
    }
    let mut seen = Vec::new();
    for s in &doc.sections {
        for refs in [&s.headers, &s.footers] {
            for name in refs.iter().flatten() {
                if seen.contains(name) {
                    continue;
                }
                seen.push(name.clone());
                if let (Some(b), Some(id)) = (doc.stories.get(name), idx.header_footer(name)) {
                    out.push((id, b));
                }
            }
        }
    }
    let mut notes: Vec<(&i64, &Vec<Block>)> = doc.footnotes.iter().collect();
    notes.sort_by_key(|(id, _)| **id);
    out.extend(notes.into_iter().map(|(id, b)| (StoryId::Footnote(*id), b.as_slice())));
    let mut ends: Vec<(&i64, &Vec<Block>)> = doc.endnotes.iter().collect();
    ends.sort_by_key(|(id, _)| **id);
    out.extend(ends.into_iter().map(|(id, b)| (StoryId::Endnote(*id), b.as_slice())));
    out
}

struct Walk<'d> {
    doc: &'d Document,
    out: Vec<Element>,
    boxes: Vec<(FloatId, &'d [Block])>,
}

impl<'d> Walk<'d> {
    fn control(&mut self, id: crate::docx::ControlId, story: StoryId, parent: Option<ElementId>, display: String, at: Option<ParaId>) {
        let props = self.doc.controls.get(&id).map(|c| c.props.clone());
        let locked = props.as_ref().is_some_and(|p| !p.lock.editable());
        self.out.push(Element { id: ElementId::Control(id), code: None, props, display, story, parent, locked, at });
    }

    fn blocks(&mut self, blocks: &'d [Block], story: StoryId, parents: &[ElementId]) {
        let mut open: Vec<crate::docx::ControlId> = Vec::new();
        for (k, b) in blocks.iter().enumerate() {
            let ids: &[crate::docx::ControlId] = match b {
                Block::Para(p) => &p.controls,
                Block::Table(t) => &t.controls,
            };
            let keep = open.iter().zip(ids).take_while(|(a, b)| a == b).count();
            open.truncate(keep);
            for &c in &ids[keep..] {
                let parent = open.last().map(|c| ElementId::Control(*c)).or(parents.last().copied());
                let text = member_text(&blocks[k..], c);
                self.control(c, story, parent, text, None);
                open.push(c);
            }
            let mut chain = parents.to_vec();
            chain.extend(open.iter().map(|c| ElementId::Control(*c)));
            match b {
                Block::Para(p) => self.para(p, story, &chain),
                Block::Table(t) => {
                    let mut rows: Vec<crate::docx::ControlId> = Vec::new();
                    for r in &t.rows {
                        let keep = rows.iter().zip(&r.controls).take_while(|(a, b)| a == b).count();
                        rows.truncate(keep);
                        for &c in &r.controls[keep..] {
                            let parent = rows.last().map(|c| ElementId::Control(*c)).or(chain.last().copied());
                            self.control(c, story, parent, String::new(), None);
                            rows.push(c);
                        }
                        let mut row_chain = chain.clone();
                        row_chain.extend(rows.iter().map(|c| ElementId::Control(*c)));
                        for c in &r.cells {
                            let mut cell_chain = row_chain.clone();
                            for &k in &c.controls {
                                let text: String = c
                                    .blocks
                                    .iter()
                                    .filter_map(|b| if let Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None })
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                self.control(k, story, cell_chain.last().copied(), clip(text), None);
                                cell_chain.push(ElementId::Control(k));
                            }
                            self.blocks(&c.blocks, story, &cell_chain);
                        }
                    }
                }
            }
        }
    }

    fn para(&mut self, p: &'d Paragraph, story: StoryId, parents: &[ElementId]) {
        let here = Some(ParaId(p.id));
        let all = spans(&p.inlines);
        let mut stack: Vec<ElementId> = parents.to_vec();
        for (k, i) in p.inlines.iter().enumerate() {
            match i {
                Inline::FieldBegin { id, lock, .. } => {
                    let s = all.iter().find(|s| s.begin == k).copied();
                    let (code, display) = s.map_or((String::new(), String::new()), |s| (code_text(&p.inlines, s), result_text(&p.inlines, s)));
                    self.out.push(Element {
                        id: ElementId::Field(*id),
                        code: Some(code),
                        props: None,
                        display: clip(display),
                        story,
                        parent: stack.last().copied(),
                        locked: *lock,
                        at: here,
                    });
                    stack.push(ElementId::Field(*id));
                }
                Inline::FieldEnd { id, .. } if stack.last() == Some(&ElementId::Field(*id)) => {
                    stack.pop();
                }
                Inline::ControlStart(c) => {
                    let end = p.inlines[k..].iter().position(|x| *x == Inline::ControlEnd(*c)).map_or(p.inlines.len(), |e| k + e);
                    let text = crate::docx::fields::text(&Paragraph { inlines: p.inlines[k + 1..end].to_vec(), ..p.clone() });
                    self.control(*c, story, stack.last().copied(), clip(text), here);
                    stack.push(ElementId::Control(*c));
                }
                Inline::ControlEnd(c) if stack.last() == Some(&ElementId::Control(*c)) => {
                    stack.pop();
                }
                Inline::Sym { ch, control: Some(c), .. } => self.control(*c, story, stack.last().copied(), ch.to_string(), here),
                i if i.story().is_some() => {
                    if let Some((id, tb)) = i.story() {
                        self.boxes.push((FloatId(id), &tb.blocks));
                    }
                }
                _ => {}
            }
        }
    }
}

fn clip(s: String) -> String {
    if s.chars().count() <= DISPLAY { s } else { s.chars().take(DISPLAY).collect() }
}

fn member_text(blocks: &[Block], c: crate::docx::ControlId) -> String {
    let mut out = Vec::new();
    for b in blocks {
        match b {
            Block::Para(p) if p.controls.contains(&c) => out.push(crate::docx::fields::text(p)),
            Block::Table(t) if t.controls.contains(&c) => {}
            _ => break,
        }
        if out.iter().map(String::len).sum::<usize>() > DISPLAY {
            break;
        }
    }
    clip(out.join("\n"))
}

pub fn enumerate(doc: &Document, idx: &DocIndex, filter: &Filter) -> Vec<Element> {
    let mut w = Walk { doc, out: Vec::new(), boxes: Vec::new() };
    for (story, blocks) in stories(doc, idx) {
        w.blocks(blocks, story, &[]);
    }
    let mut k = 0;
    while k < w.boxes.len() {
        let (f, blocks) = w.boxes[k];
        w.blocks(blocks, StoryId::TextBox(f), &[]);
        k += 1;
    }
    for (k, part) in doc.glossary.iter().flat_map(|g| g.parts.iter().enumerate()) {
        if part.state != crate::docx::PartState::Removed {
            w.blocks(&part.body, StoryId::BuildingBlock(k as u32), &[]);
        }
    }
    w.out.into_iter().filter(|e| filter.keeps(e, idx)).collect()
}

pub fn context(doc: &Document, idx: &DocIndex, sel: &Selection) -> Vec<(ElementId, bool)> {
    let Some(focus) = sel.focus() else { return Vec::new() };
    let anchor = sel.anchor().unwrap_or(focus);
    let mut out: Vec<(ElementId, bool)> = Vec::new();
    if let Some((para, at)) = focus.para_offset()
        && let Some(p) = idx.para(doc, para)
    {
        let other = anchor.para_offset().filter(|(q, _)| *q == para).map_or(at, |(_, u)| u);
        let (lo, hi) = (at.min(other).0, at.max(other).0);
        out.extend(inline_context(p, lo, hi));
        out.extend(p.controls.iter().rev().map(|c| (ElementId::Control(*c), false)));
    }
    for level in idx.chain_of(&focus).iter().rev() {
        let Some(t) = idx.table(doc, level.table) else { continue };
        if let Some(c) = t.rows.get(level.row).and_then(|r| r.cells.get(level.cell)) {
            out.extend(c.controls.iter().rev().map(|c| (ElementId::Control(*c), false)));
        }
        if let Some(r) = t.rows.get(level.row) {
            out.extend(r.controls.iter().rev().map(|c| (ElementId::Control(*c), false)));
        }
        out.extend(t.controls.iter().rev().map(|c| (ElementId::Control(*c), false)));
    }
    out
}

fn inline_context(p: &Paragraph, lo: u32, hi: u32) -> Vec<(ElementId, bool)> {
    let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
    let mut found: Vec<(u32, ElementId, bool)> = Vec::new();
    for s in spans(&p.inlines) {
        let Inline::FieldBegin { id, .. } = &p.inlines[s.begin] else { continue };
        let (a, b) = (starts[s.begin], starts[s.end] + 1);
        let exact = lo == a && hi == b;
        if exact || (a < lo && hi < b) {
            found.push((b - a, ElementId::Field(*id), exact));
        }
    }
    let mut open: Vec<(crate::docx::ControlId, u32)> = Vec::new();
    for (k, i) in p.inlines.iter().enumerate() {
        match i {
            Inline::ControlStart(c) => open.push((*c, starts[k])),
            Inline::ControlEnd(c) => {
                if let Some(pos) = open.iter().rposition(|(o, _)| o == c) {
                    let (_, a) = open.remove(pos);
                    let b = starts[k];
                    if a <= lo && hi <= b {
                        found.push((b - a, ElementId::Control(*c), lo == a && hi == b && lo < hi));
                    }
                }
            }
            Inline::Sym { control: Some(c), .. } => {
                let (a, b) = (starts[k], starts[k] + 1);
                if a <= lo && hi <= b {
                    found.push((1, ElementId::Control(*c), lo == a && hi == b));
                }
            }
            _ => {}
        }
    }
    found.sort_by_key(|(len, ..)| *len);
    found.into_iter().map(|(_, e, x)| (e, x)).collect()
}

pub fn by_id(doc: &Document, idx: &DocIndex) -> HashMap<ElementId, Element> {
    enumerate(doc, idx, &Filter::default()).into_iter().map(|e| (e.id, e)).collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    pub added: Vec<Element>,
    pub removed: Vec<ElementId>,
    pub changed: Vec<Element>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    pub fn then(&mut self, later: Changes) {
        for e in later.added {
            self.removed.retain(|r| *r != e.id);
            self.added.retain(|a| a.id != e.id);
            self.changed.retain(|c| c.id != e.id);
            self.added.push(e);
        }
        for e in later.changed {
            if let Some(a) = self.added.iter_mut().find(|a| a.id == e.id) {
                *a = e;
            } else {
                self.changed.retain(|c| c.id != e.id);
                self.changed.push(e);
            }
        }
        for id in later.removed {
            let was_added = self.added.iter().any(|a| a.id == id);
            self.added.retain(|a| a.id != id);
            self.changed.retain(|c| c.id != id);
            if !was_added {
                self.removed.push(id);
            }
        }
    }
}

pub fn in_para(doc: &Document, idx: &DocIndex, para: ParaId) -> Vec<Element> {
    let Some(p) = idx.para(doc, para) else { return Vec::new() };
    let Some(slot) = idx.para_slot(para) else { return Vec::new() };
    let story = idx.story(slot.container);
    let pos = Position::text(story, para, crate::layout::Unit(0));
    let mut parents: Vec<ElementId> = Vec::new();
    for level in idx.chain_of(&pos) {
        let Some(t) = idx.table(doc, level.table) else { continue };
        parents.extend(t.controls.iter().map(|c| ElementId::Control(*c)));
        if let Some(r) = t.rows.get(level.row) {
            parents.extend(r.controls.iter().map(|c| ElementId::Control(*c)));
            if let Some(c) = r.cells.get(level.cell) {
                parents.extend(c.controls.iter().map(|c| ElementId::Control(*c)));
            }
        }
    }
    parents.extend(p.controls.iter().map(|c| ElementId::Control(*c)));
    let mut w = Walk { doc, out: Vec::new(), boxes: Vec::new() };
    w.para(p, story, &parents);
    w.out
}

pub enum Touched {
    Paras(Vec<ParaId>, Vec<ElementId>),
    All,
}

pub fn touched(inverses: &[super::step::Step]) -> Touched {
    use super::step::Step;
    let mut paras = Vec::new();
    let mut removed = Vec::new();
    for s in inverses {
        match s {
            Step::Text { para, with, .. } | Step::Restyle { para, inlines: with } => {
                paras.push(*para);
                removed.extend(with.iter().filter_map(|i| match i {
                    Inline::FieldBegin { id, .. } => Some(ElementId::Field(*id)),
                    Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => Some(ElementId::Control(*c)),
                    _ => None,
                }));
            }
            Step::Para { para, .. } | Step::FieldViews { para, .. } => paras.push(*para),
            Step::AddImage { .. } | Step::Resolver { .. } | Step::DefaultTab(_) | Step::Hyphenation(_) => {}
            Step::Join { first, second } => paras.extend([*first, *second]),
            Step::Split { para, second, .. } => {
                paras.push(*para);
                removed.extend(second.controls.iter().map(|c| ElementId::Control(*c)));
            }
            _ => return Touched::All,
        }
    }
    paras.sort();
    paras.dedup();
    Touched::Paras(paras, removed)
}
