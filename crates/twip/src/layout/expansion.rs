//! Sub-template expansions

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

#[derive(Debug)]
pub struct Expansion {
    pub field: FieldId,
    pub part: u32,
    pub size: (f64, f64),
    pub content: Rc<[Child]>,
}

impl PartialEq for Expansion {
    fn eq(&self, o: &Self) -> bool {
        self.field == o.field && self.part == o.part && self.size == o.size && Rc::ptr_eq(&self.content, &o.content)
    }
}

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

pub fn lay(doc: &Document, blocks: &[Block], width: f64, fonts: &Fonts, nested: Option<&Expansions>) -> (f64, Rc<[Child]>) {
    let env = LayoutEnv::new(doc);
    let ctx = LayoutCtx { fonts, env: &env, preview: None, expansions: nested };
    let mut cache = LayoutCache::default();
    let mut meter = Meter::unlimited();
    let mut memos = Memos { cache: &mut cache, job: None, meter: &mut meter };
    let (fields, notes) = (FieldValues::default(), HashMap::new());
    let senv = StoryEnv { ctx, fields: &fields, notes: &notes };
    let refs: Vec<&Block> = blocks.iter().collect();
    let Ok(story) = layout(&refs, width.max(1.0), &senv, None, &mut memos) else { return (0.0, Rc::from(Vec::new())) };
    let (mut children, height) = stack_limited(&story, 0.0, 0.0, f64::INFINITY);
    children.extend(shown(&children));
    (height, children.into())
}

pub fn shown(children: &[Child]) -> Vec<Child> {
    let mut out = Vec::new();
    super::traverse::lines_in(children, &mut |_, l, dx, dy| {
        for (r, e) in l.line.expansions.iter() {
            out.push(Child::Group { offset: (r[0] + dx + l.x, r[1] + dy + l.y), children: e.content.clone() });
        }
    });
    out
}
