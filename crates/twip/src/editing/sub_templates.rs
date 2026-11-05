//! Sub-templates: fields the application declares as references to glossary building blocks, by field type, the
//! argument holding the name and the building block category. A reference resolves to the live building block of
//! that name; whether its body can show in its place depends on where the field is.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::api::command::{ReferenceRule, SubTemplateRules, SubTemplateView};
use crate::docx::fields::{self, FieldCode};
use crate::docx::{Block, Document, ElementId, FieldId, Gallery, Inline, Paragraph, Section};
use crate::font::Fonts;
use crate::layout::expansion::{Expansion, Expansions};
use crate::layout::{DocumentLayout, ParaId, Revision, TableId};

use super::Editor;
use super::commands::Outcome;
use super::elements;
use super::geometry::Pages;
use super::index::{Container, DocIndex};
use super::position::StoryId;
use super::selection::Selection;
use super::step::Step;

/// A reference rule as the editor matches it.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// Upper-cased, as field codes' names are.
    field_type: String,
    argument: usize,
    category: String,
    gallery: Option<Gallery>,
    /// As the application gave it, for the state.
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

    /// The sub-template a parsed field code names, when the code is this rule's.
    fn name(&self, code: &FieldCode) -> Option<String> {
        (code.name == self.field_type).then(|| code.args.get(self.argument).cloned()).flatten()
    }
}

/// Whether a reference's building block resolves, or why its body can't show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Resolved,
    Missing,
    /// Only a building block whose name differs in case matches: its name.
    NearMatch(String),
    Cycle,
    TooDeep,
    NotAlone,
    NotInBody,
}

/// One reference field.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    pub name: String,
    /// The building block it names, by index in the glossary's parts.
    pub part: Option<u32>,
    pub resolution: Resolution,
    /// The paragraph holding the field.
    pub at: ParaId,
    /// The table row it is alone in, when its body is one table row.
    pub row: Option<(TableId, usize)>,
}

/// The references, and a revision that changes whenever they do.
#[derive(Clone, Debug, Default)]
pub struct References {
    pub by_field: HashMap<FieldId, Reference>,
    pub revision: u64,
}

/// A sub-template being edited: its body is laid out as a document of its own, one section on the template's pages.
/// The template's layout, geometry and expansions wait here until it shows again.
pub struct Surface {
    pub part: u32,
    /// The document the body is laid out as: one section, with no headers or footers. Its blocks are the body only
    /// while a layout runs ([`Editor::with_flow`]); its images and controls are the document's then too.
    doc: Document,
    template: DocumentLayout,
    template_pages: Pages,
    template_expansions: Option<Rc<Expansions>>,
    /// The template's selection when the sub-template was entered.
    template_return: Selection,
    /// A template edit was made while the sub-template showed (an undo, a command naming a template element): the
    /// template lays out again when it shows.
    template_touched: bool,
}

/// An empty paragraph in the default paragraph style, as a new body holds.
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
        controls: Vec::new(),
    })
}

/// How deep sub-templates nest inside each other's bodies.
pub const MAX_DEPTH: usize = 16;

/// A section's first column's width.
pub fn column_width(s: &Section) -> f64 {
    let text = s.page_w - s.left - s.right - s.gutter;
    match (&s.columns.cols[..], s.columns.equal || s.columns.cols.is_empty()) {
        (_, true) if s.columns.num > 1 => (text - s.columns.space * (s.columns.num - 1) as f64) / s.columns.num as f64,
        ([first, ..], false) => first.0,
        _ => text,
    }
}

/// The rule a parsed field code matches, and the name it gives.
fn rule_for<'r>(rules: &'r [Rule], code: &FieldCode) -> Option<(&'r Rule, String)> {
    rules.iter().find_map(|r| r.name(code).map(|n| (r, n)))
}

/// Whether paragraph `p` holds nothing but field `f` (and marks that take no room).
fn alone(p: &Paragraph, f: FieldId) -> bool {
    let Some(s) = fields::spans(&p.inlines).into_iter().find(|s| matches!(p.inlines[s.begin], Inline::FieldBegin { id, .. } if id == f)) else {
        return false;
    };
    p.inlines.iter().enumerate().all(|(k, i)| (s.begin..=s.end).contains(&k) || i.units() == 0)
}

fn empty(p: &Paragraph) -> bool {
    p.inlines.iter().all(|i| i.units() == 0)
}

/// Whether a body is one table of one row (with the empty paragraph a body must end with).
fn one_row(body: &[Block]) -> bool {
    match body {
        [Block::Table(t)] => t.rows.len() == 1,
        [Block::Table(t), Block::Para(p)] => t.rows.len() == 1 && empty(p),
        _ => false,
    }
}

/// The table row whose only content is paragraph `at`.
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

/// The references that sit inside other sub-templates' bodies, followed from the template's own: one in a body
/// already on its path is a cycle, one past [`MAX_DEPTH`] too deep. A reference is decided by the first path that
/// reaches it.
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

/// Every reference field and what it resolves to.
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
    /// Set Sub-template Rules: replaces the rules, re-kinds the fields they match and resolves the references again.
    /// View only.
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

    /// Set Sub-template View: the references' bodies shown in their place, or their fields. View only.
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

    /// Whether the editor's layout shows any sub-template body in place of its reference.
    pub fn shows_expansions(&self) -> bool {
        self.references.by_field.iter().any(|(f, r)| r.resolution == Resolution::Resolved && !self.collapsed.contains(f))
    }

    /// The rules as the application set them.
    pub fn sub_template_rules(&self) -> Vec<ReferenceRule> {
        self.sub_template_rules.iter().map(|r| r.api.clone()).collect()
    }

    pub fn references(&self) -> &References {
        &self.references
    }

    /// Whether reference field `f`'s body shows in its place.
    pub fn reference_expanded(&self, f: FieldId) -> bool {
        !self.collapsed.contains(&f)
    }

    /// Fields re-kinded by the rules (and laid out again where one changed), then the references resolved.
    /// The width paragraph `at`'s text has: its section's column or its cell's text, or `outer` (the width its body is
    /// laid out at) in a building block, less its indents.
    fn reference_width(&self, at: ParaId, outer: f64) -> f64 {
        let indents = self.index.para(&self.doc, at).map_or(0.0, |p| p.style.ind_left.max(0.0) + p.style.ind_right.max(0.0));
        let base = match self.index.para_slot(at).map(|s| s.container) {
            Some(Container::Section(si)) => self.doc.sections.get(si).map_or(outer, column_width),
            Some(c @ Container::Cell { .. }) => self.text_width(c),
            Some(Container::Story(_)) | None => outer,
        };
        (base - indents).max(1.0)
    }

    /// Reference `f`'s body laid out at `width` with the references in it expanded, when it shows: resolved, not
    /// collapsed, and not already on `path` or deeper than [`MAX_DEPTH`].
    fn expand(&self, f: FieldId, r: &Reference, width: f64, path: &mut Vec<u32>, fonts: &Fonts) -> Option<Rc<Expansion>> {
        let part = r.part?;
        if r.resolution != Resolution::Resolved || self.collapsed.contains(&f) || path.len() >= MAX_DEPTH || path.contains(&part) {
            return None;
        }
        let body = &self.doc.glossary.as_ref()?.parts.get(part as usize)?.body;
        path.push(part);
        let mut nested = Expansions::default();
        for (g, q) in &self.references.by_field {
            let inside = self.index.para_slot(q.at).is_some_and(|s| self.index.story(s.container) == StoryId::BuildingBlock(part));
            if inside && let Some(e) = self.expand(*g, q, self.reference_width(q.at, width), path, fonts) {
                nested.paras.insert(q.at, e);
            }
        }
        path.pop();
        let nested = (!nested.paras.is_empty()).then_some(&nested);
        let (height, content) = crate::layout::expansion::lay(&self.doc, body, width, fonts, nested);
        Some(Rc::new(Expansion { field: f, part, size: (width, height), content }))
    }

    /// The template's layout shows the references' bodies as they are now: laid out again when anything they
    /// depend on changed since (the rules, a reference, a body, which are collapsed).
    pub(super) fn sync_expansions(&mut self, fonts: &Fonts) {
        if !self.expansions_stale {
            return;
        }
        self.expansions_stale = false;
        let now = (!self.references.by_field.is_empty()).then(|| {
            let mut out = Expansions { revision: self.expansions.as_ref().map_or(0, |e| e.revision) + 1, ..Expansions::default() };
            // The flow's own references: the template's body, or the sub-template's being edited (which can't show
            // itself again).
            let flow = self.editing_sub_template().map_or(StoryId::Body, StoryId::BuildingBlock);
            let mut path: Vec<u32> = self.editing_sub_template().into_iter().collect();
            let outer = self.surface.as_ref().map_or(0.0, |s| column_width(&s.doc.sections[0]));
            for (f, r) in &self.references.by_field {
                let top = self.index.para_slot(r.at).is_some_and(|s| self.index.story(s.container) == flow);
                if !top || r.resolution != Resolution::Resolved || self.collapsed.contains(f) {
                    continue;
                }
                // shortcut: a sub-template row shows its cells' text without expanding references inside them.
                if let Some(row) = r.row
                    && let Some(Block::Table(t)) = r.part.and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize)?.body.first())
                    && let Some(body) = t.rows.first()
                {
                    out.rows.insert(row, Rc::new(body.clone()));
                    out.paras.remove(&r.at);
                    continue;
                }
                if let Some(e) = self.expand(*f, r, self.reference_width(r.at, outer), &mut path, fonts) {
                    out.paras.insert(r.at, e);
                }
            }
            Rc::new(out)
        });
        // The paragraphs whose expansion may have changed: those holding a reference, before and now.
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

    /// Whether any live building block is in a declared category.
    pub fn has_sub_templates(&self) -> bool {
        let parts = self.doc.glossary.as_ref().map_or(&[][..], |g| g.parts.as_slice());
        parts.iter().any(|b| b.state != crate::docx::PartState::Removed && self.sub_template_rules.iter().any(|r| r.category == b.category))
    }

    /// The sub-template being edited, by its index in the glossary's parts.
    pub fn editing_sub_template(&self) -> Option<u32> {
        self.surface.as_ref().map(|s| s.part)
    }

    /// Runs `f` with the layout and the document it lays out: the template, or while a sub-template is edited, the
    /// document holding its body (the body, images and controls moved into it for the call, and back).
    pub(super) fn with_flow<R>(&mut self, f: impl FnOnce(&mut DocumentLayout, &Document) -> R) -> R {
        let Some(s) = &mut self.surface else { return f(&mut self.layout, &self.doc) };
        let Some(part) = self.doc.glossary.as_mut().and_then(|g| g.parts.get_mut(s.part as usize)) else {
            return f(&mut self.layout, &self.doc);
        };
        std::mem::swap(&mut s.doc.sections[0].blocks, &mut part.body);
        std::mem::swap(&mut s.doc.images, &mut self.doc.images);
        std::mem::swap(&mut s.doc.controls, &mut self.doc.controls);
        s.doc.resolver = self.doc.resolver.clone();
        s.doc.review = self.doc.review.clone();
        let out = f(&mut self.layout, &s.doc);
        std::mem::swap(&mut s.doc.controls, &mut self.doc.controls);
        std::mem::swap(&mut s.doc.images, &mut self.doc.images);
        if let Some(part) = self.doc.glossary.as_mut().and_then(|g| g.parts.get_mut(s.part as usize)) {
            std::mem::swap(&mut s.doc.sections[0].blocks, &mut part.body);
        }
        out
    }

    /// Edits sub-template `part` in the same editor: its body on its own pages, sized as the section of the
    /// reference it was entered from (`from`), else the template's first. The template's selection, layout and
    /// geometry are kept until it shows again. Switching from one sub-template to another keeps the template's.
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
        // The section's properties without its blocks or its headers and footers.
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
        self.surface = Some(Surface { part, doc, template, template_pages, template_expansions, template_return, template_touched: false });
        self.generation += 1;
        self.layout.edit(Revision(self.generation), None);
        self.history.break_typing();
        if let Some(p) = self.nav().story_edge(StoryId::BuildingBlock(part), false) {
            self.selection = Selection::Caret(p);
        }
        Outcome::Moved
    }

    /// Edit Sub-template: the named one, in its category or the rules' first with the name. Fails when none has it.
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

    /// The live building block `name` in `category`, or in the rules' categories in order.
    fn named_part(&self, name: &str, category: Option<&str>) -> Option<u32> {
        let g = self.doc.glossary.as_ref()?;
        match category {
            Some(c) => g.find(name, c, None),
            None => self.sub_template_rules.iter().find_map(|r| g.find(name, &r.category, None)),
        }
        .map(|k| k as u32)
    }

    /// Runs one building block step as a transaction labelled `label`.
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

    /// New Sub-template: a building block named `name` in its category (default: the first rule's), empty or with the
    /// body of the DOCX `from`, its ids made afresh and its styles joining the document's by name.
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
        let (body, controls) = match &c.from {
            None => (empty_para(&self.doc, &self.cx()).map(Block::Para).into_iter().collect(), Vec::new()),
            Some(bytes) => match self.imported_body(&bytes.0) {
                Some(b) => b,
                None => return invalid("not a DOCX"),
            },
        };
        let gallery = self.sub_template_rules.iter().find(|r| r.category == category).and_then(|r| r.gallery.clone()).unwrap_or(Gallery::Custom(1));
        let part =
            crate::docx::BuildingBlock { name: c.name, category, gallery, guid: None, body, state: crate::docx::PartState::Added, editable: true };
        let k = self.doc.glossary.as_ref().map_or(0, |g| g.parts.len()) as u32;
        let mut steps: Vec<Step> = controls.into_iter().map(|(id, c)| Step::Control { id, control: Some(c) }).collect();
        steps.push(Step::BuildingBlock { k, part: Some(Box::new(part)) });
        self.part_step("New Sub-template", steps, fonts)
    }

    /// A DOCX's body as a building block's, and its controls: its images join the document's (paragraph, field and
    /// control ids are already unique in the session); its styles are matched to the document's by id.
    // shortcut: an imported body's styles and lists are matched by id, not by name as a glossary's are; a style the
    // document lacks draws with the defaults.
    fn imported_body(&mut self, bytes: &[u8]) -> Option<(Vec<Block>, Vec<(crate::docx::ControlId, crate::docx::Control)>)> {
        let read = crate::docx::read_editable(bytes).ok()?;
        let mut blocks: Vec<Block> = read.sections.into_iter().flat_map(|s| s.blocks).collect();
        let base = self.doc.images.len() + self.pending_images.len();
        self.pending_images.extend(read.images);
        fn rebase(blocks: &mut [Block], base: usize) {
            for b in blocks {
                match b {
                    Block::Para(p) => {
                        p.src = None;
                        for i in &mut p.inlines {
                            if let Inline::Picture(pic, _) = i {
                                pic.image = pic.image.map(|k| k + base);
                            }
                        }
                    }
                    Block::Table(t) => {
                        t.src = None;
                        t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| rebase(&mut c.blocks, base));
                    }
                }
            }
        }
        rebase(&mut blocks, base);
        if !matches!(blocks.last(), Some(Block::Para(_))) {
            blocks.extend(empty_para(&self.doc, &self.cx()).map(Block::Para));
        }
        Some((blocks, read.controls.into_iter().collect()))
    }

    /// Rename Sub-template: its building block, and with `updateReferences` the name argument of every reference to
    /// it, in one step.
    pub(super) fn rename_sub_template(&mut self, r: crate::api::command::RenameSubTemplate, fonts: &Fonts) -> Outcome {
        let invalid = |m: String| Outcome::Refused(crate::api::error::Error::invalid(m));
        let Some(k) = self.named_part(&r.name, r.category.as_deref()) else { return invalid(format!("no sub-template {}", r.name)) };
        let Some(old) = self.doc.glossary.as_ref().map(|g| g.parts[k as usize].clone()) else { return invalid("no glossary".into()) };
        if r.to.trim().is_empty() || self.doc.glossary.as_ref().is_some_and(|g| g.find(&r.to, &old.category, None).is_some()) {
            return invalid(format!("a sub-template is named {}", r.to));
        }
        let mut renamed = old.clone();
        renamed.name = r.to.clone();
        let mut tx = self.begin("Rename Sub-template");
        if self.step(&mut tx, Step::BuildingBlock { k, part: Some(Box::new(renamed)) }).is_err() {
            self.abort(tx);
            return invalid("the sub-template changed meanwhile".into());
        }
        if r.update_references.unwrap_or(false) {
            let fields: Vec<FieldId> = self.references.by_field.iter().filter(|(_, x)| x.part == Some(k)).map(|(f, _)| *f).collect();
            for f in fields {
                self.replace_in_code(&mut tx, f, &r.name, &r.to);
            }
        }
        self.commit(tx, None, fonts)
    }

    /// Remove Sub-template: its building block goes (kept for undo); the editor leaves it first when on it.
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

    /// The template's layout, geometry, expansions and selection a surface kept, as they were.
    fn park_out(&mut self, s: Surface) -> (DocumentLayout, Pages, Option<Rc<Expansions>>, Selection) {
        if s.template_touched {
            self.expansions_stale = true;
        }
        (s.template, s.template_pages, s.template_expansions, s.template_return)
    }

    /// Back to the template from a sub-template: its layout and geometry as they were, laid out again where the
    /// sub-template's body shows (and wholly when the template was edited meanwhile), and `selection` (default: the
    /// one it had).
    pub(super) fn close_sub_template_to(&mut self, selection: Option<Selection>) -> Outcome {
        let Some(s) = self.surface.take() else { return Outcome::Unchanged("not on a sub-template") };
        let touched = s.template_touched;
        let (template, pages, expansions, back) = self.park_out(s);
        self.layout = template;
        self.pages = pages;
        self.pages.shows = None;
        self.index.flow = None;
        self.expansions = expansions;
        // The body may have changed: its references lay out again.
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

    /// After undo or redo: the editor shows where the selection went, a sub-template or the template.
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

    /// A transaction while a sub-template shows that changed the template too: it lays out again when it shows.
    pub(super) fn note_template_edits(&mut self, inverses: &[Step]) {
        let Some(part) = self.editing_sub_template() else { return };
        let template = match elements::touched(inverses) {
            elements::Touched::All => true,
            elements::Touched::Paras(paras, _) => {
                paras.iter().any(|&p| self.index.para_slot(p).is_none_or(|s| self.index.story(s.container) != StoryId::BuildingBlock(part)))
            }
        };
        if template && let Some(s) = &mut self.surface {
            s.template_touched = true;
        }
    }

    /// The reference (and its building block) whose body shows under document point `x`, `y`.
    pub(super) fn expansion_at(&mut self, x: f64, y: f64) -> Option<(FieldId, u32)> {
        let (page, px, py) = self.stack().page_at(x, y)?;
        let snapshot = self.layout.snapshot();
        let (frag, _) = snapshot.page(page)?;
        let mut found = None;
        for child in frag.children.iter() {
            let crate::layout::fragment::PageChild::Body(b) = child else { continue };
            let (bx, by) = b.offset;
            crate::layout::traverse::lines_in(&b.children, &mut |_, l, dx, dy| {
                for (r, e) in l.line.expansions.iter() {
                    let (x0, y0) = (r[0] + dx + l.x + bx, r[1] + dy + l.y + by);
                    if (x0..=x0 + e.size.0).contains(&px) && (y0..=y0 + e.size.1).contains(&py) {
                        found = Some((e.field, e.part));
                    }
                }
            });
        }
        found
    }

    /// A click on a sub-template's body shown in place of its reference selects the reference field; a double-click
    /// edits the sub-template.
    pub(super) fn expansion_click(&mut self, x: f64, y: f64, clicks: u8, fonts: &Fonts) -> bool {
        let Some((f, part)) = self.expansion_at(x, y) else { return false };
        if clicks >= 2 {
            return matches!(self.enter_sub_template(part, Some(f)), Outcome::Moved);
        }
        let id = crate::api::value::ElementId(elements::id_str(ElementId::Field(f)));
        matches!(self.select_element(&id, false, fonts), Outcome::Moved | Outcome::Changed(_))
    }

    /// Typing, pasting or dropping into a reference whose sub-template's body shows in its place: refused, the
    /// application told which sub-template. Deleting the reference isn't refused.
    pub(super) fn refuse_expanded(&mut self) -> bool {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return false };
        if pa != pb || ua == ub {
            return false;
        }
        let shown = self.references.by_field.iter().find(|(f, r)| r.at == pa && r.resolution == Resolution::Resolved && !self.collapsed.contains(f));
        let Some((&field, r)) = shown else { return false };
        let Some(p) = self.index.para(&self.doc, pa) else { return false };
        let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
        let Some(s) = fields::spans(&p.inlines).into_iter().find(|s| matches!(p.inlines[s.begin], Inline::FieldBegin { id, .. } if id == field))
        else {
            return false;
        };
        let (lo, hi) = (starts[s.begin], starts[s.end] + 1);
        if ub.0 <= lo || ua.0 >= hi {
            return false;
        }
        let name = r.part.and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize)).map(|b| b.name.clone());
        self.refused = Some("sub-template");
        self.events.push(super::input::Notice::Refused { sub_template: name, element: Some(ElementId::Field(field)) });
        true
    }

    fn rebuild_references(&mut self) {
        let rules = &self.sub_template_rules;
        let (changed, back) = fields::rekind(&mut self.doc, &|code: &FieldCode| rule_for(rules, code).is_some());
        if back {
            fields::evaluate(&mut self.doc, crate::docx::now_utc());
        }
        if changed {
            // Field results change wherever a rule matched or stopped matching.
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

    /// The application hears once of each reference that can't show for a reason it may not know of.
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

    /// After a transaction or its undo (`inverses`): the references again, when it touched a reference's paragraph,
    /// a building block's body, or a paragraph whose fields a rule matches.
    pub(super) fn refresh_references(&mut self, inverses: &[Step]) {
        if self.sub_template_rules.is_empty() {
            return;
        }
        // A field whose code was edited took its kind from its code again; a rule may match it.
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

/// The command a test sets rules with.
#[cfg(test)]
pub fn rules(field_type: &str, category: &str) -> crate::api::command::Command {
    crate::api::command::Command::SetSubTemplateRules(SubTemplateRules {
        rules: vec![ReferenceRule { field_type: field_type.into(), argument: None, category: category.into(), gallery: None }],
    })
}
