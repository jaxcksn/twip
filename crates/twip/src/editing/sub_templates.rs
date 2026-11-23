//! Sub-templates

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::api::command::{ApplicationFieldTypes, ReferenceRule, SubTemplateRules, SubTemplateView};
use crate::docx::fields::{self, FieldCode};
use crate::docx::{Block, Document, ElementId, FieldId, FieldKind, Gallery, Inline, Paragraph, Section};
use crate::font::Fonts;
use crate::layout::expansion::{Expansion, Expansions};
use crate::layout::{DocumentLayout, ParaId, Revision, TableId};

use super::Editor;
use super::commands::Outcome;
use super::elements;
use super::geometry::Pages;
use super::index::{Container, DocIndex};
use super::position::{Position, StoryId};
use super::selection::Selection;
use super::step::Step;

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    field_type: String,
    argument: usize,
    category: String,
    gallery: Option<Gallery>,
    api: ReferenceRule,
}

impl Rule {
    pub fn new(r: ReferenceRule) -> Rule {
        Rule {
            field_type: r.field_type.trim().to_uppercase(),
            argument: r.argument.unwrap_or(0) as usize,
            category: r.category.clone(),
            gallery: r.gallery.as_deref().map(Gallery::parse),
            api: r,
        }
    }

    fn name(&self, code: &FieldCode) -> Option<String> {
        (code.name == self.field_type).then(|| code.args.get(self.argument).cloned()).flatten()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Resolved,
    Missing,
    NearMatch(String),
    Cycle,
    TooDeep,
    NotAlone,
    NotInBody,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    pub name: String,
    pub part: Option<u32>,
    pub resolution: Resolution,
    pub at: ParaId,
    pub row: Option<(TableId, usize)>,
}

#[derive(Clone, Debug, Default)]
pub struct References {
    pub by_field: HashMap<FieldId, Reference>,
    pub revision: u64,
}

pub struct Surface {
    pub part: u32,
    /// The section the sub-template was entered from: its page is the surface's.
    pub section: usize,
    doc: Document,
    template: DocumentLayout,
    template_pages: Pages,
    template_expansions: Option<Rc<Expansions>>,
    template_return: Selection,
    template_touched: bool,
}

/// Blocks from another part or package, written afresh: their source XML dropped, their pictures', floats' and
/// shapes' image indices moved up by `base`, text boxes included.
fn detach(blocks: &mut [Block], base: usize) {
    let up = |k: &mut Option<usize>| *k = k.map(|k| k + base);
    for b in blocks {
        match b {
            Block::Para(p) => {
                p.src = None;
                for i in &mut p.inlines {
                    match i {
                        Inline::Picture(pic, _) => {
                            up(&mut pic.image);
                            if let Some(s) = &mut pic.shape {
                                s.rebase_images(base);
                            }
                        }
                        Inline::Float(f, _) => {
                            up(&mut f.image);
                            if let Some(s) = &mut f.shape {
                                s.rebase_images(base);
                            }
                            for part in &mut f.parts {
                                up(&mut part.image);
                                if let Some(tb) = &mut part.text_box {
                                    detach(&mut tb.blocks, base);
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some(tb) = i.story_mut() {
                        detach(&mut tb.blocks, base);
                    }
                }
            }
            Block::Table(t) => {
                t.src = None;
                t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| detach(&mut c.blocks, base));
            }
        }
    }
}

/// A body read from another package, and what it refers to there.
#[derive(Default)]
struct Imported {
    blocks: Vec<Block>,
    controls: Vec<(crate::docx::ControlId, crate::docx::Control)>,
    refs: super::clipboard::Refs,
    numbering: crate::docx::numbering::Numbering,
}

/// Each paragraph of `blocks`: in tables and text boxes too.
fn each_para(blocks: &mut [Block], f: &mut dyn FnMut(&mut Paragraph)) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                f(p);
                for i in &mut p.inlines {
                    if let Inline::Float(fl, _) = i {
                        for tb in fl.parts.iter_mut().filter_map(|part| part.text_box.as_mut()) {
                            each_para(&mut tb.blocks, f);
                        }
                    }
                    if let Some(tb) = i.story_mut() {
                        each_para(&mut tb.blocks, f);
                    }
                }
            }
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| each_para(&mut c.blocks, f)),
        }
    }
}

fn empty_para(doc: &Document, cx: &crate::docx::write::props::Cx) -> Option<Paragraph> {
    let res = doc.resolver.as_ref()?;
    let r = res.para_from_xml(None, None, cx);
    let mark = res.run_from_xml(&r.base, None, cx);
    Some(Paragraph {
        style: Rc::new(r.style),
        mark,
        list: None,
        inlines: Vec::new(),
        src: None,
        id: crate::docx::next_para_id(),
        div: None,
        controls: Vec::new(),
    })
}

pub const MAX_DEPTH: usize = 16;

/// Each building block part's references that can expand.
type Inside<'a> = HashMap<u32, Vec<(FieldId, &'a Reference)>>;

/// The expansions one sync lays out: each part's references expand again at every level, so `k` references a level
/// would be `k^16` layouts at [`MAX_DEPTH`].
// ponytail: a safety bound, not memoised; past it the remaining references show collapsed.
const MAX_EXPANSIONS: usize = 4096;

pub fn column_width(s: &Section) -> f64 {
    let text = s.page_w - s.left - s.right - s.gutter;
    match (&s.columns.cols[..], s.columns.equal || s.columns.cols.is_empty()) {
        (_, true) if s.columns.num > 1 => (text - s.columns.space * (s.columns.num - 1) as f64) / s.columns.num as f64,
        ([first, ..], false) => first.0,
        _ => text,
    }
}

fn rule_for<'r>(rules: &'r [Rule], code: &FieldCode) -> Option<(&'r Rule, String)> {
    rules.iter().find_map(|r| r.name(code).map(|n| (r, n)))
}

fn alone(p: &Paragraph, f: FieldId) -> bool {
    let Some(s) = fields::spans(&p.inlines).into_iter().find(|s| matches!(p.inlines[s.begin], Inline::FieldBegin { id, .. } if id == f)) else {
        return false;
    };
    p.inlines.iter().enumerate().all(|(k, i)| (s.begin..=s.end).contains(&k) || i.units() == 0)
}

fn empty(p: &Paragraph) -> bool {
    p.inlines.iter().all(|i| i.units() == 0)
}

fn one_row(body: &[Block]) -> bool {
    match body {
        [Block::Table(t)] => t.rows.len() == 1,
        [Block::Table(t), Block::Para(p)] => t.rows.len() == 1 && empty(p),
        _ => false,
    }
}

fn alone_in_row(doc: &Document, idx: &DocIndex, at: ParaId) -> Option<(TableId, usize)> {
    let Container::Cell { table, row, cell } = idx.para_slot(at)?.container else { return None };
    let t = idx.table(doc, table)?;
    let r = t.rows.get(row)?;
    let only = r.cells.iter().enumerate().all(|(k, c)| {
        if k == cell {
            matches!(c.blocks.as_slice(), [Block::Para(p)] if p.id == at.0)
        } else {
            c.blocks.iter().all(|b| matches!(b, Block::Para(p) if empty(p)))
        }
    });
    only.then_some((table, row))
}

fn mark_paths(idx: &DocIndex, refs: &mut HashMap<FieldId, Reference>) {
    let story = |r: &Reference| idx.para_slot(r.at).map(|s| idx.story(s.container));
    let mut inside: HashMap<u32, Vec<FieldId>> = HashMap::new();
    let mut roots = Vec::new();
    for (f, r) in refs.iter() {
        match story(r) {
            Some(StoryId::BuildingBlock(k)) => inside.entry(k).or_default().push(*f),
            _ => roots.push(*f),
        }
    }
    let order = |v: &mut Vec<FieldId>| v.sort_unstable_by_key(|f| f.0);
    order(&mut roots);
    inside.values_mut().for_each(order);
    fn walk(
        f: FieldId,
        path: &mut Vec<u32>,
        refs: &mut HashMap<FieldId, Reference>,
        inside: &HashMap<u32, Vec<FieldId>>,
        seen: &mut HashSet<FieldId>,
    ) {
        if !seen.insert(f) {
            return;
        }
        let Some(r) = refs.get_mut(&f) else { return };
        let Some(part) = r.part.filter(|_| r.resolution == Resolution::Resolved) else { return };
        if path.contains(&part) {
            r.resolution = Resolution::Cycle;
            return;
        }
        if path.len() >= MAX_DEPTH {
            r.resolution = Resolution::TooDeep;
            return;
        }
        path.push(part);
        for &g in inside.get(&part).into_iter().flatten() {
            walk(g, path, refs, inside, seen);
        }
        path.pop();
    }
    let (mut seen, mut path) = (HashSet::new(), Vec::new());
    for f in roots {
        walk(f, &mut path, refs, &inside, &mut seen);
    }
}

pub fn resolve(doc: &Document, idx: &DocIndex, rules: &[Rule]) -> HashMap<FieldId, Reference> {
    let mut out = HashMap::new();
    if rules.is_empty() {
        return out;
    }
    let parts = doc.glossary.as_ref().map_or(&[][..], |g| g.parts.as_slice());
    for e in elements::enumerate(doc, idx, &elements::Filter::default()) {
        let (ElementId::Field(f), Some(code), Some(at)) = (e.id, &e.code, e.at) else { continue };
        let Some((rule, name)) = rule_for(rules, &fields::parse(code)) else { continue };
        let live = |b: &&crate::docx::BuildingBlock| b.state != crate::docx::PartState::Removed && b.category == rule.category;
        let gallery = |b: &crate::docx::BuildingBlock| rule.gallery.as_ref().is_none_or(|g| *g == b.gallery);
        let part = doc.glossary.as_ref().and_then(|g| g.find(&name, &rule.category, rule.gallery.as_ref())).map(|k| k as u32);
        let near = || parts.iter().filter(live).find(|b| gallery(b) && b.name.eq_ignore_ascii_case(&name)).map(|b| b.name.clone());
        let in_flow = matches!(e.story, StoryId::Body | StoryId::BuildingBlock(_));
        let para = idx.para(doc, at);
        let resolution = match (part, in_flow, para.is_some_and(|p| alone(p, f))) {
            (_, false, _) => Resolution::NotInBody,
            (_, true, false) => Resolution::NotAlone,
            (Some(_), true, true) => Resolution::Resolved,
            (None, true, true) => near().map_or(Resolution::Missing, Resolution::NearMatch),
        };
        let row = part
            .filter(|_| resolution == Resolution::Resolved)
            .filter(|&k| one_row(&parts[k as usize].body))
            .and_then(|_| alone_in_row(doc, idx, at));
        out.insert(f, Reference { name, part, resolution, at, row });
    }
    mark_paths(idx, &mut out);
    out
}

impl Editor {
    // Rules and view
    pub(super) fn set_sub_template_rules(&mut self, r: SubTemplateRules) -> Outcome {
        let rules: Vec<Rule> = r.rules.into_iter().map(Rule::new).collect();
        if rules == self.sub_template_rules {
            return Outcome::Unchanged("same rules");
        }
        self.sub_template_rules = rules;
        self.warned.clear();
        self.rebuild_references();
        Outcome::Moved
    }

    pub(super) fn set_application_field_types(&mut self, t: ApplicationFieldTypes) -> Outcome {
        let mut types: Vec<String> = t.types.iter().map(|x| x.trim().to_uppercase()).filter(|x| !x.is_empty()).collect();
        types.sort();
        types.dedup();
        if types == self.application_field_types {
            return Outcome::Unchanged("same types");
        }
        self.application_field_types = types;
        self.rebuild_references();
        self.refresh_labels(None);
        Outcome::Moved
    }

    /// Whether `code`'s type is one the application declared.
    pub(super) fn application_field(&self, code: &FieldCode) -> bool {
        self.application_field_types.contains(&code.name)
    }

    pub(super) fn set_sub_template_view(&mut self, v: SubTemplateView) -> Outcome {
        let fields: Vec<FieldId> = match v.ids {
            None => self.references.by_field.keys().copied().collect(),
            Some(ids) => ids
                .iter()
                .filter_map(|i| match elements::parse_id(&i.0) {
                    Some(ElementId::Field(f)) if self.references.by_field.contains_key(&f) => Some(f),
                    _ => None,
                })
                .collect(),
        };
        let before = self.collapsed.len();
        for f in fields {
            if v.expanded {
                self.collapsed.remove(&f);
            } else {
                self.collapsed.insert(f);
            }
        }
        if self.collapsed.len() == before {
            return Outcome::Unchanged("no change");
        }
        self.expansions_stale = true;
        Outcome::Moved
    }

    pub fn shows_expansions(&self) -> bool {
        self.references.by_field.iter().any(|(f, r)| r.resolution == Resolution::Resolved && !self.collapsed.contains(f))
    }

    pub fn sub_template_rules(&self) -> Vec<ReferenceRule> {
        self.sub_template_rules.iter().map(|r| r.api.clone()).collect()
    }

    pub fn references(&self) -> &References {
        &self.references
    }

    pub fn reference_expanded(&self, f: FieldId) -> bool {
        !self.collapsed.contains(&f)
    }

    fn reference_width(&self, at: ParaId, outer: f64) -> f64 {
        let indents = self.index.para(&self.doc, at).map_or(0.0, |p| p.style.ind_left.max(0.0) + p.style.ind_right.max(0.0));
        let base = match self.index.para_slot(at).map(|s| s.container) {
            Some(Container::Section(si)) => self.doc.sections.get(si).map_or(outer, column_width),
            Some(c @ Container::Cell { .. }) => self.text_width(c),
            Some(Container::Story(_)) | None => outer,
        };
        (base - indents).max(1.0)
    }

    /// `r`, reference `f`, laid out at `width` with what it holds (`inside`, each part's references that can expand),
    /// each attempt one of `budget`.
    fn expand(
        &self,
        f: FieldId,
        r: &Reference,
        width: f64,
        inside: &Inside,
        path: &mut Vec<u32>,
        budget: &mut usize,
        fonts: &Fonts,
    ) -> Option<Rc<Expansion>> {
        let part = r.part?;
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        if r.resolution != Resolution::Resolved || self.collapsed.contains(&f) || path.len() >= MAX_DEPTH || path.contains(&part) {
            return None;
        }
        let body = &self.doc.glossary.as_ref()?.parts.get(part as usize)?.body;
        path.push(part);
        let mut nested = Expansions::default();
        for (g, q) in inside.get(&part).into_iter().flatten() {
            if *budget == 0 {
                break;
            }
            if let Some(e) = self.expand(*g, q, self.reference_width(q.at, width), inside, path, budget, fonts) {
                nested.paras.insert(q.at, e);
            }
        }
        path.pop();
        let nested = (!nested.paras.is_empty()).then_some(&nested);
        let (height, content) = crate::layout::expansion::lay(&self.doc, body, width, fonts, nested);
        Some(Rc::new(Expansion { field: f, part, size: (width, height), content }))
    }

    pub(super) fn sync_expansions(&mut self, fonts: &Fonts) {
        if !self.expansions_stale {
            return;
        }
        self.expansions_stale = false;
        let now = (!self.references.by_field.is_empty()).then(|| {
            let mut out = Expansions { revision: self.expansions.as_ref().map_or(0, |e| e.revision) + 1, ..Expansions::default() };
            let flow = self.editing_sub_template().map_or(StoryId::Body, StoryId::BuildingBlock);
            let mut path: Vec<u32> = self.editing_sub_template().into_iter().collect();
            let outer = self.surface.as_ref().map_or(0.0, |s| column_width(&s.doc.sections[0]));
            let mut budget = MAX_EXPANSIONS;
            let mut top = Vec::new();
            let mut inside = Inside::new();
            for (f, r) in &self.references.by_field {
                if r.resolution != Resolution::Resolved || self.collapsed.contains(f) {
                    continue;
                }
                match self.index.para_slot(r.at).map(|s| self.index.story(s.container)) {
                    Some(story) if story == flow => top.push((f, r)),
                    Some(StoryId::BuildingBlock(k)) => inside.entry(k).or_default().push((*f, r)),
                    _ => {}
                }
            }
            for (f, r) in top {
                if let Some(row) = r.row
                    && let Some(Block::Table(t)) = r.part.and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize)?.body.first())
                    && let Some(body) = t.rows.first()
                {
                    out.rows.insert(row, Rc::new(body.clone()));
                    out.paras.remove(&r.at);
                    continue;
                }
                if let Some(e) = self.expand(*f, r, self.reference_width(r.at, outer), &inside, &mut path, &mut budget, fonts) {
                    out.paras.insert(r.at, e);
                }
            }
            Rc::new(out)
        });
        let mut paras: Vec<ParaId> = self.expansions.iter().chain(now.iter()).flat_map(|e| e.paras.keys().copied()).collect();
        paras.extend(self.references.by_field.values().filter(|r| r.row.is_some()).map(|r| r.at));
        paras.sort_unstable_by_key(|p| p.0);
        paras.dedup();
        let hints: Vec<_> = paras.into_iter().map(|p| super::step::para_hint(&self.index, p, 0)).collect();
        if self.layout.set_expansions(Revision(self.generation + 1), now.clone(), &hints) {
            self.generation += 1;
        }
        self.expansions = now;
    }

    pub fn has_sub_templates(&self) -> bool {
        let parts = self.doc.glossary.as_ref().map_or(&[][..], |g| g.parts.as_slice());
        parts.iter().any(|b| b.state != crate::docx::PartState::Removed && self.sub_template_rules.iter().any(|r| r.category == b.category))
    }

    /// Lays out everything again after a change outside the model (fonts, hyphenation patterns): the shown layout, the
    /// expansions, and the main document's layout parked behind a sub-template.
    pub(super) fn relayout_environment(&mut self) {
        self.relayout([None]);
        self.expansions_stale = true;
        if let Some(s) = &mut self.surface {
            s.template_touched = true;
        }
    }

    pub fn editing_sub_template(&self) -> Option<u32> {
        self.surface.as_ref().map(|s| s.part)
    }

    pub(super) fn with_flow<R>(&mut self, f: impl FnOnce(&mut DocumentLayout, &Document) -> R) -> R {
        if self.table_preview.is_empty() {
            return self.flow_in(f);
        }
        self.swap_table_preview(false);
        let out = self.flow_in(f);
        self.swap_table_preview(true);
        out
    }

    fn flow_in<R>(&mut self, f: impl FnOnce(&mut DocumentLayout, &Document) -> R) -> R {
        let Some(s) = &mut self.surface else { return f(&mut self.layout, &self.doc) };
        let Some(part) = self.doc.glossary.as_mut().and_then(|g| g.parts.get_mut(s.part as usize)) else {
            return f(&mut self.layout, &self.doc);
        };
        std::mem::swap(&mut s.doc.sections[0].blocks, &mut part.body);
        std::mem::swap(&mut s.doc.images, &mut self.doc.images);
        std::mem::swap(&mut s.doc.controls, &mut self.doc.controls);
        std::mem::swap(&mut s.doc.settings, &mut self.doc.settings);
        std::mem::swap(&mut s.doc.hyphenation, &mut self.doc.hyphenation);
        std::mem::swap(&mut s.doc.line_number_run, &mut self.doc.line_number_run);
        s.doc.resolver = self.doc.resolver.clone();
        s.doc.review = self.doc.review.clone();
        s.doc.theme = self.doc.theme.clone();
        s.doc.colour_map = self.doc.colour_map;
        if let Some(entry) = self.doc.sections.get(s.section)
            && entry.page() != s.doc.sections[0].page()
        {
            s.doc.sections[0].set_page(entry.page());
        }
        let out = f(&mut self.layout, &s.doc);
        std::mem::swap(&mut s.doc.line_number_run, &mut self.doc.line_number_run);
        std::mem::swap(&mut s.doc.hyphenation, &mut self.doc.hyphenation);
        std::mem::swap(&mut s.doc.settings, &mut self.doc.settings);
        std::mem::swap(&mut s.doc.controls, &mut self.doc.controls);
        std::mem::swap(&mut s.doc.images, &mut self.doc.images);
        if let Some(part) = self.doc.glossary.as_mut().and_then(|g| g.parts.get_mut(s.part as usize)) {
            std::mem::swap(&mut s.doc.sections[0].blocks, &mut part.body);
        }
        out
    }

    // Sub-template editing
    pub(super) fn enter_sub_template(&mut self, part: u32, from: Option<FieldId>) -> Outcome {
        if self.editing_sub_template() == Some(part) {
            return Outcome::Unchanged("already on it");
        }
        let Some(b) = self.doc.glossary.as_ref().and_then(|g| g.parts.get(part as usize)) else { return Outcome::Unchanged("no such sub-template") };
        if b.state == crate::docx::PartState::Removed || !b.editable {
            return Outcome::Unchanged("not editable");
        }
        let held = self.surface.take().map(|s| self.park_out(s));
        let si = from
            .and_then(|f| self.references.by_field.get(&f))
            .and_then(|r| self.index.para_slot(r.at))
            .and_then(|s| match s.container {
                Container::Section(si) => Some(si),
                _ => None,
            })
            .unwrap_or(0);
        let Some(section) = self.doc.sections.get_mut(si) else { return Outcome::Unchanged("no section") };
        let blocks = std::mem::take(&mut section.blocks);
        let mut sec = section.clone();
        section.blocks = blocks;
        (sec.headers, sec.footers, sec.change) = (Default::default(), Default::default(), None);
        let d = &self.doc;
        let doc = Document {
            sections: vec![sec],
            theme: d.theme.clone(),
            colour_map: d.colour_map,
            settings: d.settings.clone(),
            line_number_run: d.line_number_run.clone(),
            resolver: d.resolver.clone(),
            review: d.review.clone(),
            people: d.people.clone(),
            ..Document::default()
        };
        let (template, template_pages, template_expansions, template_return) = match held {
            Some(h) => h,
            None => (std::mem::take(&mut self.layout), std::mem::take(&mut self.pages), self.expansions.take(), self.selection),
        };
        self.layout = DocumentLayout::new();
        self.pages = Pages::default();
        self.pages.shows = Some(part);
        self.index.flow = Some(part);
        self.expansions = None;
        self.expansions_stale = true;
        self.surface =
            Some(Surface { part, section: si, doc, template, template_pages, template_expansions, template_return, template_touched: false });
        self.generation += 1;
        self.layout.edit(Revision(self.generation), None);
        self.history.break_typing();
        if let Some(p) = self.nav().story_edge(StoryId::BuildingBlock(part), false) {
            self.selection = Selection::Caret(p);
        }
        Outcome::Moved
    }

    pub(super) fn edit_sub_template(&mut self, n: &crate::api::command::SubTemplateName, fonts: &Fonts) -> Outcome {
        let categories: Vec<&str> = match &n.category {
            Some(c) => vec![c.as_str()],
            None => self.sub_template_rules.iter().map(|r| r.category.as_str()).collect(),
        };
        let part = self.doc.glossary.as_ref().and_then(|g| categories.iter().find_map(|c| g.find(&n.name, c, None)));
        let Some(part) = part else {
            return Outcome::Refused(crate::api::error::Error::invalid(format!("no sub-template {}", n.name)));
        };
        let out = self.enter_sub_template(part as u32, None);
        if let Outcome::Moved = out {
            self.ensure_current(fonts);
            self.scroll_to_caret(fonts);
        }
        out
    }

    fn named_part(&self, name: &str, category: Option<&str>) -> Option<u32> {
        let g = self.doc.glossary.as_ref()?;
        match category {
            Some(c) => g.find(name, c, None),
            None => self.sub_template_rules.iter().find_map(|r| g.find(name, &r.category, None)),
        }
        .map(|k| k as u32)
    }

    fn part_step(&mut self, label: &'static str, steps: Vec<Step>, fonts: &Fonts) -> Outcome {
        let mut tx = self.begin(label);
        for s in steps {
            if self.step(&mut tx, s).is_err() {
                self.abort(tx);
                return Outcome::Refused(crate::api::error::Error::invalid("the sub-template changed meanwhile"));
            }
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn create_sub_template(&mut self, c: crate::api::command::CreateSubTemplate, fonts: &Fonts) -> Outcome {
        let invalid = |m: &str| Outcome::Refused(crate::api::error::Error::invalid(m));
        let Some(category) = c.category.clone().or_else(|| self.sub_template_rules.first().map(|r| r.category.clone())) else {
            return invalid("no category: set the sub-template rules or name one");
        };
        if c.name.trim().is_empty() {
            return invalid("an empty name");
        }
        if self.doc.glossary.as_ref().is_some_and(|g| g.find(&c.name, &category, None).is_some()) {
            return invalid("a sub-template has that name");
        }
        let copied = match (&c.from, &c.copy_of) {
            (Some(_), Some(_)) => return invalid("from or copyOf, not both"),
            (None, Some(n)) => match self.export_sub_template(n, Some(&category)) {
                Ok(b) => Some(b),
                Err(e) => return Outcome::Refused(e),
            },
            _ => None,
        };
        let mut im = match copied.as_deref().or(c.from.as_ref().map(|b| b.0.as_slice())) {
            None => Imported { blocks: empty_para(&self.doc, &self.cx()).map(Block::Para).into_iter().collect(), ..Imported::default() },
            Some(bytes) => match self.imported_body(bytes) {
                Some(b) => b,
                None => return invalid("not a DOCX"),
            },
        };
        let gallery = self.sub_template_rules.iter().find(|r| r.category == category).and_then(|r| r.gallery.clone()).unwrap_or(Gallery::Custom(1));
        let k = self.doc.glossary.as_ref().map_or(0, |g| g.parts.len()) as u32;
        let mut tx = self.begin("New Sub-template");
        let adopted = match self.adopt_imported(&mut tx, &mut im, true) {
            Ok(a) => a,
            Err(e) => {
                self.abort(tx);
                return invalid(e);
            }
        };
        let body = std::mem::take(&mut im.blocks);
        let part =
            crate::docx::BuildingBlock { name: c.name, category, gallery, guid: None, body, state: crate::docx::PartState::Added, editable: true };
        let mut steps: Vec<Step> = std::mem::take(&mut im.controls).into_iter().map(|(id, c)| Step::Control { id, control: Some(c) }).collect();
        steps.push(Step::BuildingBlock { k, part: Some(Box::new(part)) });
        for s in steps {
            if self.step(&mut tx, s).is_err() {
                self.abort(tx);
                return invalid("the sub-template changed meanwhile");
            }
        }
        self.restyle_imported(&mut tx, adopted);
        self.commit(tx, None, fonts)
    }

    /// `bytes`' body, read from the DOCX: its pictures added, its blocks written afresh, a last paragraph after a
    /// last table; with the notes, comments and lists it refers to there.
    fn imported_body(&mut self, bytes: &[u8]) -> Option<Imported> {
        let mut read = crate::docx::read_editable(bytes).ok()?;
        let mut blocks: Vec<Block> = std::mem::take(&mut read.sections).into_iter().flat_map(|s| s.blocks).collect();
        let base = self.doc.images.len() + self.pending_images.len();
        self.pending_images.append(&mut read.images);
        detach(&mut blocks, base);
        if !matches!(blocks.last(), Some(Block::Para(_))) {
            blocks.extend(empty_para(&self.doc, &self.cx()).map(Block::Para));
        }
        let refs = super::clipboard::refs_of(&read, &blocks);
        let numbering = read.resolver.as_ref().map(|r| r.numbering.clone()).unwrap_or_default();
        Some(Imported { blocks, controls: std::mem::take(&mut read.controls).into_iter().collect(), refs, numbering })
    }

    /// Makes `im`'s references the document's, in `tx`: its lists join the document's under ids of their own, its
    /// bookmarks named as the document's go (as the body it replaces names them, they stay), and its notes and comments
    /// come too (a glossary part can hold neither: its comments go, and a note refuses it). Returns the stories and
    /// paragraphs to format again once it is in.
    fn adopt_imported(&mut self, tx: &mut super::Tx, im: &mut Imported, glossary: bool) -> Result<(HashSet<StoryId>, HashSet<ParaId>), &'static str> {
        use crate::docx::review::RangeKind;
        if glossary && !im.refs.notes.is_empty() {
            return Err("a sub-template can't hold footnotes or endnotes");
        }
        let replaced =
            |p: &ParaId| !glossary && self.index.para_slot(*p).is_some_and(|s| self.index.host(self.index.story(s.container)) == StoryId::Body);
        let taken: HashSet<String> = self
            .index
            .para_ids()
            .filter(|p| !replaced(p))
            .filter_map(|p| self.index.para(&self.doc, p))
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| match i {
                Inline::Bookmark(n) if !n.starts_with('/') => Some(n.to_lowercase()),
                _ => None,
            })
            .collect();
        let mut paras = HashSet::new();
        each_para(&mut im.blocks, &mut |p| {
            paras.insert(ParaId(p.id));
            p.inlines.retain(|i| match i {
                Inline::Bookmark(n) => !taken.contains(&n.trim_start_matches('/').to_lowercase()),
                Inline::Range(r) if glossary => !matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd),
                Inline::CommentRef { .. } => !glossary,
                _ => true,
            });
        });
        if glossary {
            im.refs.comments.clear();
        }
        self.import_lists(tx, im)?;
        let stories = if glossary { Vec::new() } else { self.paste_refs(tx, &mut im.blocks, &im.refs, None) };
        tx.relabel = true;
        Ok((stories.into_iter().collect(), paras))
    }

    /// The lists `im`'s paragraphs (its notes' and comments' too) name, added to the document's numbering under new
    /// ids, the paragraphs' `w:numId`s moved to them.
    fn import_lists(&mut self, tx: &mut super::Tx, im: &mut Imported) -> Result<(), &'static str> {
        use crate::docx::write::props::Props;
        let Some(res) = self.doc.resolver.clone() else { return Ok(()) };
        let cx = self.cx();
        let w = cx.w.clone();
        let num_of = |p: &Paragraph| -> Option<i64> {
            let pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
            let num = Props::parse(pr.get("numPr"), "numPr", &cx);
            crate::docx::write::props::attr_of(num.get("numId")?, "val")?.trim().parse().ok().filter(|n| *n != 0)
        };
        let mut used: Vec<i64> = Vec::new();
        let mut note = |blocks: &mut [Block]| each_para(blocks, &mut |p| used.extend(num_of(p)));
        note(&mut im.blocks);
        im.refs.notes.iter_mut().for_each(|n| note(&mut n.2));
        im.refs.comments.iter_mut().for_each(|c| note(&mut c.body));
        used.sort_unstable();
        used.dedup();
        used.retain(|n| im.numbering.nums.contains_key(n));
        if used.is_empty() {
            return Ok(());
        }
        let mut next = res.clone();
        let numbering = &mut Rc::make_mut(&mut next).numbering;
        let mut next_num = numbering.nums.keys().copied().chain(self.doc.added_lists.iter().map(|l| l.num_id)).max().unwrap_or(0).max(0) + 1;
        let mut next_abstract =
            numbering.abstracts.keys().copied().chain(self.doc.added_lists.iter().map(|l| l.abstract_id)).max().unwrap_or(-1).max(-1) + 1;
        let (mut nums, mut abstracts): (HashMap<i64, i64>, HashMap<i64, i64>) = (HashMap::new(), HashMap::new());
        for n in used {
            let Some(num) = im.numbering.nums.get(&n) else { continue };
            let Some(a) = im.numbering.abstracts.get(&num.abstract_id) else { continue };
            // The document's own list, as a copy of its content names it
            let own =
                numbering.nums.get(&n).is_some_and(|h| h.raw == num.raw && numbering.abstracts.get(&h.abstract_id).is_some_and(|ha| ha.raw == a.raw));
            if own {
                continue;
            }
            let to_abstract = *abstracts.entry(num.abstract_id).or_insert_with(|| {
                let id = next_abstract;
                next_abstract += 1;
                let mut el = Props::parse(Some(&a.raw), "abstractNum", &cx);
                el.set_own("abstractNumId", &id.to_string(), &cx);
                let raw = el.xml().unwrap_or_else(|| a.raw.clone());
                numbering
                    .abstracts
                    .insert(id, crate::docx::numbering::AbstractNum { raw, origin: crate::docx::styles::StyleOrigin::Document, ..a.clone() });
                id
            });
            let to = next_num;
            next_num += 1;
            let mut el = Props::parse(Some(&num.raw), "num", &cx);
            el.set_own("numId", &to.to_string(), &cx);
            el.set("abstractNumId", format!("<{w}abstractNumId {w}val=\"{to_abstract}\"/>"));
            let raw = el.xml().unwrap_or_else(|| num.raw.clone());
            numbering.nums.insert(
                to,
                crate::docx::numbering::Num { abstract_id: to_abstract, origin: crate::docx::styles::StyleOrigin::Document, raw, ..num.clone() },
            );
            nums.insert(n, to);
        }
        if nums.is_empty() {
            return Ok(());
        }
        let renumber = |blocks: &mut [Block]| {
            each_para(blocks, &mut |p| {
                let Some(to) = num_of(p).and_then(|n| nums.get(&n)) else { return };
                let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
                let mut num = Props::parse(pr.get("numPr"), "numPr", &cx);
                num.set("numId", format!("<{w}numId {w}val=\"{to}\"/>"));
                if let Some(x) = num.xml() {
                    pr.set("numPr", x);
                }
                let direct = crate::docx::Raw(pr.xml().map(|x| Rc::from(x.as_str())));
                p.style = Rc::new(crate::docx::ParaStyle { direct, ..(*p.style).clone() });
            })
        };
        renumber(&mut im.blocks);
        im.refs.notes.iter_mut().for_each(|n| renumber(&mut n.2));
        im.refs.comments.iter_mut().for_each(|c| renumber(&mut c.body));
        let added = self.doc.added_lists.clone();
        self.step(tx, Step::Resolver { resolver: Some(next), added }).map(|_| ()).map_err(|_| "the lists changed meanwhile")
    }

    /// The paragraphs of `stories` and `paras` formatted again in `tx`, as the document's styles and lists have them.
    fn restyle_imported(&mut self, tx: &mut super::Tx, (stories, paras): (HashSet<StoryId>, HashSet<ParaId>)) {
        let stale: Vec<ParaId> = self
            .index
            .para_ids()
            .filter(|p| paras.contains(p) || self.index.para_slot(*p).is_some_and(|s| stories.contains(&self.index.story(s.container))))
            .collect();
        tx.stale_paras.extend(stale);
    }

    /// The template's body becomes `bytes`' body: its sections merge into the last (whose page, headers and footers
    /// stay) and its blocks are the DOCX's. The glossary, data parts, settings and styles stay. One undo step.
    pub(super) fn replace_body(&mut self, bytes: &[u8], fonts: &Fonts) -> Outcome {
        if crate::docx::read_editable(bytes).is_err() {
            return Outcome::Refused(crate::api::error::Error::invalid("not a DOCX"));
        }
        if self.surface.is_some() {
            self.close_sub_template_to(None);
        }
        let Some(mut im) = self.imported_body(bytes) else { return Outcome::Refused(crate::api::error::Error::invalid("not a DOCX")) };
        let mut tx = self.begin("Replace Body");
        let ok = std::mem::take(&mut im.controls).into_iter().all(|(id, c)| self.step(&mut tx, Step::Control { id, control: Some(c) }).is_ok())
            && (1..self.doc.sections.len()).all(|_| self.merge_section(&mut tx, 0).is_ok());
        let adopted = if ok { self.adopt_imported(&mut tx, &mut im, false).ok() } else { None };
        let Some(adopted) = adopted else {
            self.abort(tx);
            return Outcome::Refused(crate::api::error::Error::invalid("the body changed meanwhile"));
        };
        let range = 0..self.doc.sections[0].blocks.len();
        if self.step(&mut tx, Step::Blocks { container: Container::Section(0), range, with: std::mem::take(&mut im.blocks) }).is_err() {
            self.abort(tx);
            return Outcome::Refused(crate::api::error::Error::invalid("the body changed meanwhile"));
        }
        self.restyle_imported(&mut tx, adopted);
        if let Some(p) = self.nav().story_edge(StoryId::Body, false) {
            self.selection = Selection::Caret(p);
        }
        self.commit(tx, None, fonts)
    }

    /// A standalone DOCX of a sub-template's body: the document with that body as its only one (on the last
    /// section's page) and an empty glossary, keeping its styles, numbering, theme, fonts and data parts.
    pub fn export_sub_template(&self, name: &str, category: Option<&str>) -> Result<Vec<u8>, crate::api::error::Error> {
        use crate::api::error::{Error, ErrorCode};
        let failed = |e: String| Error::new(ErrorCode::ExportFailed, e);
        let k = self.named_part(name, category).ok_or_else(|| Error::invalid(format!("no sub-template {name}")))? as usize;
        let mut doc = self.doc.clone();
        let read = doc.source.as_ref().map_or(1, |s| s.blocks.len());
        let g = doc.glossary.as_mut().ok_or_else(|| Error::invalid("no glossary"))?;
        let mut body = g.parts[k].body.clone();
        g.parts.iter_mut().for_each(|b| b.state = crate::docx::PartState::Removed);
        detach(&mut body, 0);
        let last = doc.sections.len().checked_sub(1).ok_or_else(|| failed("no section".into()))?;
        let mut sec = doc.sections.swap_remove(last);
        sec.blocks = body;
        doc.sections = vec![sec];
        doc.section_sources = vec![read];
        doc.edited_sections = doc.edited_sections.contains(&last).then_some(0).into_iter().collect();
        let bytes = crate::docx::write::save(&doc).map_err(failed)?;
        // The template's headers and footers aren't the body's: their references go (their parts stay, unused).
        let main = doc.source.as_ref().map(|s| s.main.clone()).ok_or_else(|| failed("not read for editing".into()))?;
        let mut xml = String::new();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).map_err(|e| failed(e.to_string()))?;
        std::io::Read::read_to_string(&mut zip.by_name(&main).map_err(|e| failed(e.to_string()))?, &mut xml).map_err(|e| failed(e.to_string()))?;
        let dom = roxmltree::Document::parse(&xml).map_err(|e| failed(e.to_string()))?;
        let mut out = String::with_capacity(xml.len());
        let mut at = 0;
        for n in dom
            .descendants()
            .filter(|n| n.tag_name().namespace() == Some(crate::docx::xml::W) && matches!(n.tag_name().name(), "headerReference" | "footerReference"))
        {
            out.push_str(&xml[at..n.range().start]);
            at = n.range().end;
        }
        out.push_str(&xml[at..]);
        crate::docx::write::repackage_without(&bytes, &[(main, out.into_bytes())], &[], zip::CompressionMethod::Deflated).map_err(failed)
    }

    pub(super) fn rename_sub_template(&mut self, r: crate::api::command::RenameSubTemplate, fonts: &Fonts) -> Outcome {
        let invalid = |m: String| Outcome::Refused(crate::api::error::Error::invalid(m));
        let Some(k) = self.named_part(&r.name, r.category.as_deref()) else { return invalid(format!("no sub-template {}", r.name)) };
        let Some(old) = self.doc.glossary.as_ref().map(|g| g.parts[k as usize].clone()) else { return invalid("no glossary".into()) };
        if r.to.trim().is_empty() || self.doc.glossary.as_ref().is_some_and(|g| g.find(&r.to, &old.category, None).is_some()) {
            return invalid(format!("a sub-template is named {}", r.to));
        }
        let update = r.update_references.unwrap_or(false);
        let to = match crate::docx::fields::quote_arg(&r.to) {
            Some(to) => to,
            None if update => return invalid(format!("a reference can't name {}", r.to)),
            None => String::new(),
        };
        let mut renamed = old.clone();
        renamed.name = r.to.clone();
        let mut tx = self.begin("Rename Sub-template");
        if self.step(&mut tx, Step::BuildingBlock { k, part: Some(Box::new(renamed)) }).is_err() {
            self.abort(tx);
            return invalid("the sub-template changed meanwhile".into());
        }
        if update {
            let rules = self.sub_template_rules.clone();
            let fields: Vec<FieldId> = self.references.by_field.iter().filter(|(_, x)| x.part == Some(k)).map(|(f, _)| *f).collect();
            for f in fields {
                self.set_code_arg(&mut tx, f, |code| rule_for(&rules, code).map(|(rule, _)| rule.argument), &to);
            }
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn remove_sub_template(&mut self, n: &crate::api::command::SubTemplateName, fonts: &Fonts) -> Outcome {
        let Some(k) = self.named_part(&n.name, n.category.as_deref()) else {
            return Outcome::Refused(crate::api::error::Error::invalid(format!("no sub-template {}", n.name)));
        };
        if self.editing_sub_template() == Some(k) {
            self.close_sub_template_to(None);
        }
        let Some(mut gone) = self.doc.glossary.as_ref().map(|g| g.parts[k as usize].clone()) else { return Outcome::Unchanged("no glossary") };
        gone.state = crate::docx::PartState::Removed;
        self.part_step("Remove Sub-template", vec![Step::BuildingBlock { k, part: Some(Box::new(gone)) }], fonts)
    }

    fn park_out(&mut self, s: Surface) -> (DocumentLayout, Pages, Option<Rc<Expansions>>, Selection) {
        if s.template_touched {
            self.expansions_stale = true;
        }
        (s.template, s.template_pages, s.template_expansions, s.template_return)
    }

    pub(super) fn close_sub_template_to(&mut self, selection: Option<Selection>) -> Outcome {
        let Some(s) = self.surface.take() else { return Outcome::Unchanged("not on a sub-template") };
        let touched = s.template_touched;
        let (template, pages, expansions, back) = self.park_out(s);
        self.layout = template;
        self.pages = pages;
        self.pages.shows = None;
        self.index.flow = None;
        self.expansions = expansions;
        self.expansions_stale = true;
        self.generation += 1;
        if touched {
            self.layout.edit(Revision(self.generation), None);
        }
        let back = selection.unwrap_or(back);
        let valid = back.focus().is_none_or(|p| super::integrity::invalid(&self.doc, &self.index, &p).is_none());
        self.selection = if valid { back } else { self.nav().story_edge(StoryId::Body, false).map_or(back, Selection::Caret) };
        self.history.break_typing();
        Outcome::Moved
    }

    pub(super) fn follow_selection(&mut self) {
        let story = self.selection.focus().map(|p| p.story);
        match (story, self.editing_sub_template()) {
            (Some(StoryId::BuildingBlock(k)), shown) if shown != Some(k) => {
                let sel = self.selection;
                self.enter_sub_template(k, None);
                self.selection = sel;
            }
            (Some(StoryId::BuildingBlock(_)), _) | (None, _) | (_, None) => {}
            (Some(_), Some(_)) => {
                let sel = self.selection;
                self.close_sub_template_to(Some(sel));
            }
        }
    }

    pub(super) fn note_template_edits(&mut self, inverses: &[Step]) {
        let Some(part) = self.editing_sub_template() else { return };
        let template = changes_environment(inverses)
            || match elements::touched(inverses) {
                elements::Touched::All => true,
                elements::Touched::Paras(paras, _) => {
                    paras.iter().any(|&p| self.index.para_slot(p).is_none_or(|s| self.index.story(s.container) != StoryId::BuildingBlock(part)))
                }
            };
        if template && let Some(s) = &mut self.surface {
            s.template_touched = true;
        }
    }

    pub(super) fn expansion_at(&mut self, x: f64, y: f64) -> Option<(FieldId, u32)> {
        let (page, px, py) = self.stack().page_at(x, y)?;
        let mut found = None;
        self.each_expansion(page, &mut |field, part, r, turn| {
            let [lx, ly] = turn.back([px, py]);
            if (r[0]..=r[2]).contains(&lx) && (r[1]..=r[3]).contains(&ly) {
                found = Some((field, part));
            }
        });
        found
    }

    /// Where a shown sub-template is drawn on `pages`: one rect per page.
    pub(super) fn expansion_rects(&mut self, f: FieldId, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let mut out = Vec::new();
        for page in pages {
            self.each_expansion(page, &mut |field, _, r, turn| {
                if field == f {
                    out.push((page, turn.rect(r)));
                }
            });
        }
        out
    }

    /// Each sub-template shown in the body of page `page`: its field and part, its box as its line is laid out and
    /// where that line is drawn.
    fn each_expansion(&mut self, page: usize, f: &mut dyn FnMut(FieldId, u32, [f64; 4], crate::layout::vertical::Turn)) {
        if self.expansions.as_ref().is_none_or(|x| x.paras.is_empty()) {
            return;
        }
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        let Some(g) = self.pages.page(page, &self.index) else { return };
        for &(field, part, r, turn) in &g.expansions {
            f(field, part, r, turn);
        }
    }

    pub(super) fn expansion_click(&mut self, x: f64, y: f64, clicks: u8, fonts: &Fonts) -> bool {
        let Some((f, part)) = self.expansion_at(x, y) else { return false };
        if clicks >= 2 {
            return matches!(self.enter_sub_template(part, Some(f)), Outcome::Moved);
        }
        let id = crate::api::value::ElementId(elements::id_str(ElementId::Field(f)));
        matches!(self.select_element(&id, false, fonts), Outcome::Moved | Outcome::Changed(_))
    }

    pub(super) fn refuse_expanded(&mut self) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let Some(field) = self.expanded_in(&a, &b).or_else(|| self.other_ranges().iter().find_map(|(a, b)| self.expanded_in(a, b))) else {
            return false;
        };
        let part = self.references.by_field.get(&field).and_then(|r| r.part);
        let name = part.and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize)).map(|b| b.name.clone());
        self.refused = Some("sub-template");
        self.events.push(super::input::Notice::Refused { sub_template: name, element: Some(ElementId::Field(field)) });
        true
    }

    /// The shown, resolved sub-template reference `a..b` edits, whose body is read only.
    pub(super) fn expanded_in(&self, a: &Position, b: &Position) -> Option<FieldId> {
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return None };
        if pa != pb || ua == ub {
            return None;
        }
        let shown = self.references.by_field.iter().find(|(f, r)| r.at == pa && r.resolution == Resolution::Resolved && !self.collapsed.contains(f));
        let (&field, _) = shown?;
        let p = self.index.para(&self.doc, pa)?;
        let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
        let s = fields::spans(&p.inlines).into_iter().find(|s| matches!(p.inlines[s.begin], Inline::FieldBegin { id, .. } if id == field))?;
        let (lo, hi) = (starts[s.begin], starts[s.end] + 1);
        (ub.0 > lo && ua.0 < hi).then_some(field)
    }

    fn rebuild_references(&mut self) {
        let (rules, types) = (&self.sub_template_rules, &self.application_field_types);
        let (changed, back) = fields::rekind(&mut self.doc, &|code: &FieldCode| field_kind(rules, types, code));
        if back {
            fields::evaluate(&mut self.doc, crate::docx::now_local());
        }
        if changed || back {
            self.changed_everywhere();
        }
        if changed {
            self.relayout(vec![None]);
        }
        let now = resolve(&self.doc, &self.index, &self.sub_template_rules);
        if now != self.references.by_field {
            self.references.by_field = now;
            self.references.revision += 1;
        }
        self.expansions_stale = true;
        self.warn_references();
    }

    fn warn_references(&mut self) {
        use crate::docx::diagnostics::WarningCode as W;
        let mut found: Vec<(W, String)> = self
            .references
            .by_field
            .values()
            .filter_map(|r| match &r.resolution {
                Resolution::Cycle => Some((W::SubTemplateCycle, format!("{} is inside its own body", r.name))),
                Resolution::TooDeep => Some((W::SubTemplateTooDeep, format!("{} is more than {MAX_DEPTH} sub-templates deep", r.name))),
                Resolution::NearMatch(n) => Some((W::SubTemplateNearMatch, format!("{} matches the sub-template {n} only in another case", r.name))),
                _ => None,
            })
            .collect();
        found.sort_by(|a, b| (a.0.name(), &a.1).cmp(&(b.0.name(), &b.1)));
        for w in found {
            if self.warned.insert(w.clone()) {
                self.events.push(super::input::Notice::Warning(w.0, w.1));
            }
        }
    }

    pub(super) fn refresh_references(&mut self, inverses: &[Step]) {
        if self.sub_template_rules.is_empty() {
            return;
        }
        if changes_environment(inverses) {
            self.expansions_stale = true;
        }
        let at: HashSet<ParaId> = self.references.by_field.values().map(|r| r.at).collect();
        let needed = match elements::touched(inverses) {
            elements::Touched::All => true,
            elements::Touched::Paras(paras, removed) => {
                removed.iter().any(|id| matches!(id, ElementId::Field(f) if self.references.by_field.contains_key(f)))
                    || paras.iter().any(|&p| {
                        at.contains(&p)
                            || self.index.para_slot(p).is_some_and(|s| matches!(self.index.story(s.container), StoryId::BuildingBlock(_)))
                            || elements::in_para(&self.doc, &self.index, p)
                                .iter()
                                .any(|e| e.code.as_deref().is_some_and(|c| rule_for(&self.sub_template_rules, &fields::parse(c)).is_some()))
                    })
            }
        };
        if needed {
            self.rebuild_references();
        }
    }
}

/// Whether the steps change what every paragraph lays out with (the default tab stop, hyphenation, page settings)
/// without touching one.
fn changes_environment(steps: &[Step]) -> bool {
    steps.iter().any(|s| matches!(s, Step::DefaultTab(_) | Step::Hyphenation(_) | Step::PageSettings(_)))
}

/// The kind a field with `code` has under the editor's settings: a sub-template reference; none, as a MERGEFIELD, for
/// an application type (it keeps its result and shows its label); else its code's own.
fn field_kind(rules: &[Rule], types: &[String], code: &FieldCode) -> Option<FieldKind> {
    if rule_for(rules, code).is_some() {
        Some(FieldKind::SubTemplate)
    } else if types.contains(&code.name) {
        None
    } else {
        code.kind()
    }
}

#[cfg(test)]
pub fn rules(field_type: &str, category: &str) -> crate::api::command::Command {
    crate::api::command::Command::SetSubTemplateRules(SubTemplateRules {
        rules: vec![ReferenceRule { field_type: field_type.into(), argument: None, category: category.into(), gallery: None }],
    })
}
