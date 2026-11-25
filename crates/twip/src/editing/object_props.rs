//! Object properties and the Selection Pane

use std::cmp::Reverse;

use crate::api::state::ObjectKind;
use crate::docx::Inline;
use crate::drawing::{Shape, ShapeKind};
use crate::font::Fonts;
use crate::layout::FloatId;

use super::Editor;
use super::commands::Outcome;
use super::position::StoryId;
use super::selection::{ObjectRef, Selection};
use super::stories::{inline_at, reachable};

/// The longest name or title an object takes, in characters.
pub const MAX_NAME: usize = 255;

/// How deep a listed group's children are listed: a deeper group is listed without its children.
const MAX_LISTED_DEPTH: usize = 16;

/// A floating object as the Selection Pane lists it, or one of a group's children.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectEntry {
    /// The float.
    pub id: FloatId,
    /// The child's place in the float's group; empty for the float itself.
    pub path: Vec<u16>,
    /// Its name (`wp:docPr/@name`, a child's `cNvPr/@name`).
    pub name: String,
    /// What it is.
    pub kind: ObjectKind,
    /// Hidden: not drawn and nothing wraps around it.
    pub hidden: bool,
    /// A group's children, front first.
    pub children: Vec<ObjectEntry>,
}

/// The selected object's properties a host shows beside its size, wrapping and formatting.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectProps {
    /// Its name.
    pub name: String,
    /// Its title (alternative text's short name).
    pub title: String,
    /// Hidden.
    pub hidden: bool,
    /// Resizing by a corner keeps its aspect ratio.
    pub lock_aspect: bool,
    /// A picture's image at its own resolution: width and height in points.
    pub original_size: Option<(f64, f64)>,
    /// Its place in the stack of its page's floating objects, 0 at the front; `None` for inline objects and frames.
    pub stack_index: Option<usize>,
    /// A frame (`w:framePr` paragraphs, a drop cap's too): placed by [`Editor::frame_properties`].
    pub frame: bool,
}

/// A shape's kind as the Selection Pane shows it.
fn kind_of(s: &Shape) -> ObjectKind {
    match &s.kind {
        ShapeKind::Pic(_) => ObjectKind::Picture,
        // A chart, a diagram or other graphic data.
        ShapeKind::Raw(_) => ObjectKind::Shape,
        ShapeKind::Group(_) => ObjectKind::Group,
        ShapeKind::Sp(sp) if sp.text_box => ObjectKind::TextBox,
        ShapeKind::Sp(sp) if sp.geom.preset().map(crate::api::drawing::ShapePreset).is_some_and(crate::api::drawing::ShapePreset::is_line) => {
            ObjectKind::Line
        }
        ShapeKind::Sp(_) => ObjectKind::Shape,
    }
}

fn children_of(id: FloatId, s: &Shape, path: &mut Vec<u16>) -> Vec<ObjectEntry> {
    let ShapeKind::Group(g) = &s.kind else { return Vec::new() };
    if path.len() >= MAX_LISTED_DEPTH {
        return Vec::new();
    }
    // Children are drawn in order: the last is in front.
    g.children
        .iter()
        .enumerate()
        .rev()
        .map(|(k, c)| {
            path.push(k as u16);
            let e = ObjectEntry {
                id,
                path: path.clone(),
                name: c.nv.name.clone(),
                kind: kind_of(c),
                hidden: c.nv.hidden,
                children: children_of(id, c, path),
            };
            path.pop();
            e
        })
        .collect()
}

/// The number a name ends in (`Picture 3`: 3).
fn name_number(name: &str) -> u32 {
    name.rsplit_once(' ').and_then(|(_, n)| n.parse().ok()).unwrap_or(0)
}

/// The highest number in `blocks`' objects' names and ids, nested objects' and stories' included.
fn highest_number(blocks: &[crate::docx::Block], depth: usize) -> u32 {
    fn shape(s: &Shape) -> u32 {
        let own = name_number(&s.nv.name).max(s.nv.id);
        match &s.kind {
            ShapeKind::Group(g) => g.children.iter().map(shape).fold(own, u32::max),
            _ => own,
        }
    }
    // Deeper nesting than a document can be read with holds nothing to count.
    if depth > crate::docx::xml::MAX_DEPTH {
        return 0;
    }
    let mut n = 0;
    for b in blocks {
        match b {
            crate::docx::Block::Para(p) => {
                for i in &p.inlines {
                    let (s, story) = match i {
                        Inline::Float(f, _) => (f.shape.as_deref(), f.text_box.as_ref()),
                        Inline::Picture(p, _) => (p.shape.as_deref(), p.text.as_ref().map(|t| &t.text_box)),
                        _ => continue,
                    };
                    n = n.max(s.map_or(0, shape));
                    if let Some(tb) = story {
                        n = n.max(highest_number(&tb.blocks, depth + 1));
                    }
                }
            }
            crate::docx::Block::Table(t) => {
                for c in t.rows.iter().flat_map(|r| &r.cells) {
                    n = n.max(highest_number(&c.blocks, depth + 1));
                }
            }
        }
    }
    n
}

impl Editor {
    /// A new object's name: `base` numbered one past every object's so far, as Word names a new picture
    /// `Picture 4`. The document is counted once, on the first new object.
    pub(super) fn new_object_name(&mut self, base: &str) -> String {
        if self.object_number == 0 {
            let doc = &self.doc;
            let sections = doc.sections.iter().map(|s| highest_number(&s.blocks, 0));
            self.object_number = sections.chain(doc.stories.values().map(|b| highest_number(b, 0))).max().unwrap_or(0);
        }
        self.object_number = self.object_number.saturating_add(1);
        format!("{base} {}", self.object_number)
    }

    /// The floating objects on `page` the selection's story reaches (the body's, or the headers' and footers' while
    /// one is edited), front first, hidden ones too, as Word's Selection Pane lists them. Frames are paragraphs, not
    /// listed.
    pub fn object_list(&mut self, page: usize, fonts: &Fonts) -> Vec<ObjectEntry> {
        let ids = self.stacked(page, fonts);
        ids.into_iter()
            .filter_map(|id| {
                let s = self.float_of(id)?.shape.as_deref()?;
                let children = children_of(id, s, &mut Vec::new());
                Some(ObjectEntry { id, path: Vec::new(), name: s.nv.name.clone(), kind: kind_of(s), hidden: s.nv.hidden, children })
            })
            .collect()
    }

    /// The floats [`Editor::object_list`] lists on `page`, front first.
    fn stacked(&mut self, page: usize, fonts: &Fonts) -> Vec<FloatId> {
        self.ensure_page(page, fonts);
        let ctx = self.editing();
        let Some(geo) = self.pages.page(page, &self.index) else { return Vec::new() };
        // A paragraph's pages are one run: one shown on any earlier page is shown on the page before.
        let before = page.checked_sub(1).and_then(|p| self.pages.page(p, &self.index));
        let mut found: Vec<(bool, u32, usize, FloatId)> = Vec::new();
        for para in geo.paras() {
            let story = self.index.para_slot(para).map(|s| self.index.host(self.index.story(s.container)));
            if story.is_none_or(|s| !reachable(ctx, s)) {
                continue;
            }
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            for i in &p.inlines {
                let Inline::Float(f, _) = i else { continue };
                let id = FloatId(f.id);
                if f.in_note || f.is_frame() || f.shape.is_none() {
                    continue;
                }
                // A shown float where it is drawn; a hidden one on its anchor's first page.
                let here =
                    if f.hidden() { !before.as_ref().is_some_and(|g| g.shows_para(para)) } else { geo.stories.iter().any(|s| s.float == Some(id)) };
                if here && !found.iter().any(|e| e.3 == id) {
                    found.push((f.behind, f.z, f.order, id));
                }
            }
        }
        // As drawn, front first: in front of the text by stacking order, then behind it; equal z by document order.
        found.sort_unstable_by_key(|&(behind, z, order, id)| (behind, Reverse(z), Reverse(order), Reverse(id)));
        found.into_iter().map(|e| e.3).collect()
    }

    /// Selects a listed object (a group child at `path`); with `extend`, adds it to (or takes it from) the selected
    /// floats, as a Ctrl+click in the Selection Pane does.
    pub fn select_listed(&mut self, id: FloatId, path: &[u16], extend: bool) -> Outcome {
        let Some(f) = self.float_of(id) else { return Outcome::unavailable("no such object") };
        if f.shape.as_deref().and_then(|s| s.at(path)).is_none() {
            return Outcome::unavailable("no such object");
        }
        if extend
            && path.is_empty()
            && let Selection::Object(ObjectRef::Floating(p)) = self.selection
            && p != id
        {
            let story = |e: &Self, f: FloatId| e.index.float_anchor(f).and_then(|a| e.index.para_slot(a)).map(|s| e.index.story(s.container));
            if story(self, id) != story(self, p) {
                return Outcome::unavailable("in another story");
            }
            let mut also = self.selected_floats().split_off(1);
            match also.iter().position(|g| *g == id) {
                Some(i) => {
                    also.remove(i);
                }
                None => also.push(id),
            }
            self.object_also = Some((p, also));
            self.object_child = None;
            return Outcome::Moved;
        }
        self.set_selection(Selection::Object(ObjectRef::Floating(id)));
        self.object_also = None;
        self.object_child = (!path.is_empty()).then(|| (id, path.to_vec()));
        Outcome::Moved
    }

    /// The selected object's properties.
    pub fn object_props(&mut self, fonts: &Fonts) -> Option<ObjectProps> {
        let s = self.object_shape();
        let (name, title, hidden) = s.map_or((String::new(), String::new(), false), |s| (s.nv.name.clone(), s.nv.title.to_string(), s.nv.hidden));
        let mut frame = false;
        let (lock_aspect, stack) = match self.selection {
            Selection::Object(ObjectRef::Floating(f)) => {
                let fl = self.float_of(f)?;
                frame = fl.is_frame();
                (fl.lock_aspect, (!frame).then_some(f))
            }
            Selection::Object(ObjectRef::Inline { para, offset }) => match inline_at(self.index.para(&self.doc, para)?, offset)? {
                Inline::Picture(p, _) => (p.lock_aspect, None),
                _ => return None,
            },
            _ => return None,
        };
        let stack_index = match stack {
            Some(f) if self.selected_child().is_none() => {
                let page = self.float_box(f, fonts).map(|(p, _)| p);
                page.and_then(|p| self.stacked(p, fonts).iter().position(|e| *e == f))
            }
            _ => None,
        };
        Some(ObjectProps { name, title, hidden, lock_aspect, original_size: self.original_object_size(), stack_index, frame })
    }

    /// The selected picture's image bytes (a group child's when one is selected), as the document holds them.
    pub fn object_image(&self) -> Option<&[u8]> {
        self.doc.images.get(self.selected_image()?).map(Vec::as_slice)
    }

    /// The selected picture's image (a group child's when one is selected).
    pub(super) fn selected_image(&self) -> Option<usize> {
        match self.object_shape().map(|s| &s.kind) {
            Some(ShapeKind::Pic(p)) => p.blip.image,
            _ => match self.selection {
                Selection::Object(ObjectRef::Floating(f)) => self.float_of(f)?.image,
                Selection::Object(ObjectRef::Inline { para, offset }) => match inline_at(self.index.para(&self.doc, para)?, offset)? {
                    Inline::Picture(p, _) => p.image,
                    _ => None,
                },
                _ => None,
            },
        }
    }

    /// The selected shape's picture fill's image bytes (Fill > Picture or texture fill).
    pub fn shape_fill_image(&self) -> Option<&[u8]> {
        let k = match &self.object_shape()?.kind {
            ShapeKind::Sp(sp) => match sp.fill.as_ref()? {
                crate::drawing::paint::Fill::Blip(b) => b.image?,
                _ => return None,
            },
            ShapeKind::Group(g) => match g.fill.as_ref()? {
                crate::drawing::paint::Fill::Blip(b) => b.image?,
                _ => return None,
            },
            _ => return None,
        };
        self.doc.images.get(k).map(Vec::as_slice)
    }

    /// Edits the selected object's shape (a group child's when one is selected), inline or floating.
    fn edit_nv(&mut self, label: &'static str, edit: impl Fn(&mut crate::drawing::NonVisual), fonts: &Fonts) -> Outcome {
        if self.selected_object_kept() {
            return Outcome::unavailable("the object can't be changed");
        }
        match self.selection {
            Selection::Object(ObjectRef::Inline { .. }) => self.edit_inline(
                label,
                |p| {
                    let Some(s) = p.shape.as_deref_mut() else { return false };
                    edit(&mut s.nv);
                    true
                },
                fonts,
            ),
            Selection::Object(ObjectRef::Floating(f)) => {
                let path = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
                self.edit_floats(
                    label,
                    &[f],
                    |n, _| {
                        let Some(s) = n.shape.as_deref_mut().and_then(|s| s.at_mut(&path)) else { return false };
                        edit(&mut s.nv);
                        true
                    },
                    fonts,
                )
            }
            _ => Outcome::unavailable("no object selected"),
        }
    }

    /// Whether the selected object is kept from changing: read only, document protection or a locked control.
    fn selected_object_kept(&self) -> bool {
        match self.selection {
            Selection::Object(ObjectRef::Inline { .. }) => self.gesture_kept(crate::api::command::CommandId::SetObjectSize),
            Selection::Object(ObjectRef::Floating(f)) => !self.float_changeable(f),
            _ => false,
        }
    }

    /// Renames the selected object (at most [`MAX_NAME`] characters).
    pub fn set_object_name(&mut self, name: &str, fonts: &Fonts) -> Outcome {
        let name: String = name.trim().chars().take(MAX_NAME).collect();
        if name.is_empty() {
            return Outcome::unavailable("an object needs a name");
        }
        let out = self.edit_nv("Rename", |nv| nv.name.clone_from(&name), fonts);
        // New names number past it once the document is counted.
        if self.object_number != 0 && matches!(out, Outcome::Changed(_)) {
            self.object_number = self.object_number.max(name_number(&name));
        }
        out
    }

    /// Sets the selected object's title (at most [`MAX_NAME`] characters; empty for none).
    pub fn set_object_title(&mut self, title: &str, fonts: &Fonts) -> Outcome {
        let title: std::rc::Rc<str> = title.chars().take(MAX_NAME).collect::<String>().into();
        self.edit_nv("Alt Text", |nv| nv.title = title.clone(), fonts)
    }

    /// Sets the selected object's Lock aspect ratio: resizing it by a corner keeps its proportions.
    pub fn set_lock_aspect(&mut self, on: bool, fonts: &Fonts) -> Outcome {
        let set = |lock: &mut bool, shape: Option<&mut Shape>| {
            if let Some(s) = shape {
                s.nv.lock_aspect = on;
            }
            std::mem::replace(lock, on) != on
        };
        if self.selected_object_kept() {
            return Outcome::unavailable("the object can't be changed");
        }
        if self.selected_child().is_some() {
            return Outcome::unavailable("a group's shape resizes with its group");
        }
        match self.selection {
            Selection::Object(ObjectRef::Inline { .. }) => {
                self.edit_inline("Lock Aspect Ratio", |p| set(&mut p.lock_aspect, p.shape.as_deref_mut()), fonts)
            }
            Selection::Object(ObjectRef::Floating(f)) => {
                self.edit_floats("Lock Aspect Ratio", &[f], |n, _| set(&mut n.lock_aspect, n.shape.as_deref_mut()), fonts)
            }
            _ => Outcome::unavailable("no object selected"),
        }
    }

    /// Hides or shows a listed object (a group child at `path`), as the Selection Pane's eye does. A hidden
    /// selected object is left: the caret goes to its anchor.
    pub fn set_object_hidden(&mut self, id: FloatId, path: &[u16], hidden: bool, fonts: &Fonts) -> Outcome {
        if self.float_of(id).and_then(|f| f.shape.as_deref()).and_then(|s| s.at(path)).is_none() {
            return Outcome::unavailable("no such object");
        }
        if !self.float_changeable(id) {
            return Outcome::unavailable("the object can't be changed");
        }
        let label = if hidden { "Hide" } else { "Show" };
        let out = self.edit_floats(
            label,
            &[id],
            |n, _| {
                let Some(s) = n.shape.as_deref_mut().and_then(|s| s.at_mut(path)) else { return false };
                std::mem::replace(&mut s.nv.hidden, hidden) != hidden
            },
            fonts,
        );
        // The whole object selected is its empty path
        let selected = self.selected_floats().contains(&id) && self.selected_child().unwrap_or_default().starts_with(path);
        if hidden && selected && matches!(out, Outcome::Changed(_)) {
            self.object_to_caret();
        }
        out
    }

    /// Whether float `id` stacks with the headers' and footers' floats (`true`) or the body's (`false`); `None` for a
    /// frame, a note's float or an unknown one, which the Selection Pane doesn't list.
    fn stack_group(&self, id: FloatId) -> Option<bool> {
        let f = self.float_of(id)?;
        if f.in_note || f.is_frame() {
            return None;
        }
        Some(matches!(self.index.host(self.float_point(id)?.story), StoryId::HeaderFooter(_)))
    }

    /// Moves float `id` in the stacking order to just in front of `in_front_of` (`None`: behind every other), as a
    /// drag in the Selection Pane does. Its layer (in front of or behind the text) stays.
    pub fn restack_object(&mut self, id: FloatId, in_front_of: Option<FloatId>, fonts: &Fonts) -> Outcome {
        if in_front_of == Some(id) {
            return Outcome::Unchanged("already there");
        }
        let Some(group) = self.stack_group(id) else { return Outcome::unavailable("no such object") };
        // The floats drawn with it: the body's, or the headers' and footers'.
        let mut others = self.stacking();
        let own = others.iter().find(|o| o.1 == id).map_or(0, |o| o.0);
        others.retain(|&(_, g)| g != id && self.stack_group(g) == Some(group));
        let below = match in_front_of {
            Some(t) => match others.iter().position(|(_, g)| *g == t) {
                Some(k) => Some(k),
                None => return Outcome::unavailable("no such object"),
            },
            None => None,
        };
        // The z it takes; those in front of it move up as far as they must to stay in front.
        let (z, from) = match below {
            Some(k) => (others[k].0.checked_add(1), k + 1),
            None => match others.first() {
                Some((n, _)) if *n > 0 => (Some(n - 1), others.len()),
                Some(_) => (Some(0), 0),
                None => return Outcome::Unchanged("alone"),
            },
        };
        let bumped = || {
            let mut prev = z?;
            let mut set: Vec<(FloatId, u32)> = vec![(id, prev)];
            for &(n, g) in &others[from..] {
                if n > prev {
                    break;
                }
                prev = prev.checked_add(1)?;
                set.push((g, prev));
            }
            Some(set)
        };
        // No room in front: the layer is numbered again from 0, in its new order.
        let set = bumped().unwrap_or_else(|| {
            let mut order = others.clone();
            order.insert(below.map_or(0, |k| k + 1), (own, id));
            order.into_iter().zip(0u32..).filter(|((was, _), z)| was != z).map(|((_, g), z)| (g, z)).collect()
        });
        let ids: Vec<FloatId> = set.iter().map(|s| s.0).collect();
        // Every float whose place in the stack changes, and the one it goes in front of.
        if !ids.iter().chain(&in_front_of).all(|&g| self.float_changeable(g)) {
            return Outcome::unavailable("the object can't be changed");
        }
        self.edit_floats(
            "Reorder",
            &ids,
            |n, _| set.iter().find(|s| s.0 == FloatId(n.id)).is_some_and(|s| std::mem::replace(&mut n.z, s.1) != s.1),
            fonts,
        )
    }
}
