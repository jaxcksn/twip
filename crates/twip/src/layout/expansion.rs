//! Sub-template bodies shown in place of their reference fields: an editor view, never part of the document, so the
//! PDF export and full layouts don't see it. A reference alone in its paragraph lays out as one object as wide as
//! the paragraph's text, as tall as its body laid out at that width, and drawn with the body's lines. The editor
//! lays the bodies out ([`lay`]) and hands layout the objects ([`Expansions`]).

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::{Block, Document, FieldId};
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::LayoutCache;
use super::env::LayoutEnv;
use super::fragment::Child;
use super::ids::ParaId;
use super::job::{Memos, Meter};
use super::story::{StoryEnv, layout, stack_limited};
use super::text::FieldValues;

/// A reference field shown as its building block's body.
#[derive(Debug)]
pub struct Expansion {
    pub field: FieldId,
    /// The building block, by its index in the glossary's parts.
    pub part: u32,
    /// Width and height, points.
    pub size: (f64, f64),
    /// The body as drawn, from the object's top left.
    pub content: Rc<[Child]>,
}

impl PartialEq for Expansion {
    fn eq(&self, o: &Self) -> bool {
        self.field == o.field && self.part == o.part && self.size == o.size && Rc::ptr_eq(&self.content, &o.content)
    }
}

/// The expansions a layout shows, by the paragraph holding the reference, and a revision that changes with them.
/// A reference alone in a table row whose body is one table row shows as that row, in the reference's table
/// (`rows`, by table and row).
#[derive(Debug, Default)]
pub struct Expansions {
    pub paras: HashMap<ParaId, Rc<Expansion>>,
    pub rows: HashMap<(super::ids::TableId, usize), Rc<crate::docx::Row>>,
    pub revision: u64,
}

impl PartialEq for Expansions {
    fn eq(&self, o: &Self) -> bool {
        self.revision == o.revision
    }
}

/// Body `blocks` laid out as a story at `width` with `nested` (the expansions inside it): its height and what it
/// draws, from its top left.
pub fn lay(doc: &Document, blocks: &[Block], width: f64, fonts: &Fonts, nested: Option<&Expansions>) -> (f64, Rc<[Child]>) {
    let env = LayoutEnv::new(doc);
    let ctx = LayoutCtx { fonts, env: &env, preview: None, expansions: nested };
    let mut cache = LayoutCache::default();
    let mut meter = Meter::unlimited();
    let mut memos = Memos { cache: &mut cache, job: None, meter: &mut meter };
    let (fields, notes) = (FieldValues::default(), HashMap::new());
    let senv = StoryEnv { ctx, fields: &fields, notes: &notes };
    let refs: Vec<&Block> = blocks.iter().collect();
    // An unlimited meter never yields.
    let Ok(story) = layout(&refs, width.max(1.0), &senv, None, &mut memos) else { return (0.0, Rc::from(Vec::new())) };
    let (mut children, height) = stack_limited(&story, 0.0, 0.0, f64::INFINITY);
    children.extend(shown(&children));
    (height, children.into())
}

/// The sub-template bodies the lines of `children` show, each as a group where its line puts it (a page's body, or
/// a body laid out by [`lay`] with expansions nested in it).
pub fn shown(children: &[Child]) -> Vec<Child> {
    let mut out = Vec::new();
    super::traverse::lines_in(children, &mut |_, l, dx, dy| {
        for (r, e) in l.line.expansions.iter() {
            out.push(Child::Group { offset: (r[0] + dx + l.x, r[1] + dy + l.y), children: e.content.clone() });
        }
    });
    out
}
