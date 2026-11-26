//! Linked text boxes

use crate::docx::{Block, Inline};
use crate::drawing::ShapeText;
use crate::font::Fonts;
use crate::layout::FloatId;

use super::Editor;
use super::commands::Outcome;
use super::selection::{ObjectRef, Selection};
use crate::docx::run_xml;

/// The most boxes a chain takes.
pub const MAX_CHAIN: usize = 1024;

/// A text box's place in its chain.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBoxLink {
    /// Its place, 0 for the first box (the one holding the story).
    pub index: usize,
    /// How many boxes the chain has.
    pub count: usize,
    /// The box before it.
    pub previous: Option<FloatId>,
    /// The box its text goes on in.
    pub next: Option<FloatId>,
}

impl Editor {
    /// A float's text box chain in order (the first box holding the story first), with its id; a text box not in one
    /// is a chain of its own (id `None`).
    fn chain_of(&self, f: FloatId) -> Option<(Option<u32>, Vec<FloatId>)> {
        let sp = self.float_of(f)?.shape.as_deref()?.sp()?;
        if !sp.text_box && !matches!(sp.text, ShapeText::Story | ShapeText::Linked { .. }) {
            return None;
        }
        let Some((chain, _)) = self.index.link(f) else { return Some((None, vec![f])) };
        let mut members: Vec<(u32, FloatId)> = self
            .index
            .chain_members(chain)
            .filter_map(|g| {
                let sp = self.float_of(g)?.shape.as_deref()?.sp()?;
                match sp.text {
                    ShapeText::Story => Some((0, g)),
                    ShapeText::Linked { seq, .. } => Some((seq.saturating_add(1), g)),
                    ShapeText::None => None,
                }
            })
            .collect();
        members.sort();
        Some((Some(chain), members.into_iter().map(|(_, g)| g).collect()))
    }

    /// The selected text box's place in its chain; `None` for a text box in none and for other objects.
    pub fn text_box_link(&self) -> Option<TextBoxLink> {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return None };
        let (Some(_), boxes) = self.chain_of(f)? else { return None };
        let index = boxes.iter().position(|g| *g == f)?;
        (boxes.len() > 1).then(|| TextBoxLink {
            index,
            count: boxes.len(),
            previous: index.checked_sub(1).map(|k| boxes[k]),
            next: boxes.get(index + 1).copied(),
        })
    }

    /// Whether the selected text box's text can go on in `target` (Word's Create Link): the selected box has no box
    /// after it, and `target` is another empty text box in no chain, in the same story. Neither runs vertically.
    pub fn can_link_to(&self, target: FloatId) -> bool {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return false };
        if f == target || self.selected_floats().len() != 1 || self.selected_child().is_some() {
            return false;
        }
        let Some((_, boxes)) = self.chain_of(f) else { return false };
        if boxes.last() != Some(&f) || boxes.len() >= MAX_CHAIN {
            return false;
        }
        // Every member changeable, the first holding the story
        let owned = boxes.first().and_then(|&g| self.float_of(g)?.shape.as_deref()?.sp()).is_some_and(|sp| sp.text == ShapeText::Story);
        if !owned || !boxes.iter().all(|&g| self.float_changeable(g)) || !self.float_changeable(target) {
            return false;
        }
        // A chain's text is laid out horizontally: a vertical box's text would turn
        let vertical = |g: FloatId| {
            self.float_of(g)
                .and_then(|n| n.shape.as_deref()?.sp())
                .is_some_and(|sp| crate::layout::vertical::of_text_box(sp.body.direction).is_some())
        };
        if boxes.iter().chain([&target]).any(|&g| vertical(g)) {
            return false;
        }
        let story = |g: FloatId| self.index.float_anchor(g).and_then(|a| self.index.para_slot(a)).map(|s| self.index.story(s.container));
        let empty = self.float_of(target).is_some_and(|t| {
            let sp = t.shape.as_deref().and_then(|s| s.sp());
            let free = sp.is_some_and(|sp| sp.text_box && sp.text == ShapeText::Story && sp.chain.is_none());
            // Its story is dropped: only empty paragraphs, no markers (bookmarks, comments, controls), tracked paragraph
            // marks or formatting, or anchored objects.
            let empty_text = |i: &Inline| matches!(i, Inline::Text(s, _) if s.is_empty());
            let blank = t.text_box.as_ref().is_none_or(|tb| {
                tb.blocks
                    .iter()
                    .all(|b| matches!(b, Block::Para(p) if p.controls.is_empty() && p.mark.rev.is_empty() && p.style.change.is_none() && p.inlines.iter().all(empty_text)))
            });
            free && blank && !t.is_frame()
        });
        empty && self.index.link(target).is_none() && story(f) == story(target)
    }

    /// Word's Create Link: the selected text box's text goes on in `target`. One undo step ("Create Link").
    pub fn link_text_box(&mut self, target: FloatId, fonts: &Fonts) -> Outcome {
        if !self.can_link_to(target) {
            return Outcome::unavailable("the text can't go on in that box");
        }
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no text box selected") };
        let Some((chain, boxes)) = self.chain_of(f) else { return Outcome::unavailable("no text box selected") };
        let Some(chain) = chain.or_else(|| self.index.free_chain()) else { return Outcome::unavailable("no chain id left") };
        // After the last box's place; DrawingML's places are 16-bit, so a chain with no room after it is numbered again.
        let tail = match self.float_of(f).and_then(|n| n.shape.as_deref()?.sp()).map(|sp| &sp.text) {
            Some(ShapeText::Linked { seq, .. }) => *seq,
            _ => 0,
        };
        let renumber = tail >= u32::from(u16::MAX);
        let seq = if renumber { boxes.len() as u32 } else { tail + 1 };
        let mut ids = if renumber { boxes } else { vec![f] };
        ids.push(target);
        let order = ids.clone();
        self.end_linking();
        self.edit_floats(
            "Create Link",
            &ids,
            |n, _| {
                let id = FloatId(n.id);
                let Some(sp) = n.shape.as_deref_mut().and_then(|s| s.sp_mut()) else { return false };
                if id == target {
                    (sp.text, sp.chain) = (ShapeText::Linked { chain, seq }, None);
                    n.text_box = None;
                } else if sp.text == ShapeText::Story {
                    sp.chain = Some(chain);
                } else if renumber
                    && let ShapeText::Linked { seq, .. } = &mut sp.text
                    && let Some(j) = order.iter().position(|g| *g == id)
                {
                    *seq = j as u32;
                }
                true
            },
            fonts,
        )
    }

    /// Word's Break Link: the selected text box's text no longer goes on in the box after it; that box and those after
    /// it make a chain of their own, empty (or the box is a text box of its own when it was the last). One undo step
    /// ("Break Link").
    pub fn unlink_text_box(&mut self, fonts: &Fonts) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no text box selected") };
        let Some((Some(_), boxes)) = self.chain_of(f) else { return Outcome::unavailable("the text box isn't linked") };
        let Some(k) = boxes.iter().position(|g| *g == f).filter(|k| k + 1 < boxes.len()) else {
            return Outcome::unavailable("no box after it");
        };
        if !boxes.iter().all(|&g| self.float_changeable(g)) {
            return Outcome::unavailable("the text box can't be changed");
        }
        let rest = boxes[k + 1..].to_vec();
        // A chain id only when more than one box is left after the break.
        let fresh = match rest.len() {
            1 => 0,
            _ => match self.index.free_chain() {
                Some(c) => c,
                None => return Outcome::unavailable("no chain id left"),
            },
        };
        let Some(empty) = self.empty_story() else { return Outcome::Unchanged("no styles") };
        let last_kept = k == 0;
        let ids: Vec<FloatId> = boxes.clone();
        self.edit_floats(
            "Break Link",
            &ids,
            |n, _| {
                let id = FloatId(n.id);
                let Some(sp) = n.shape.as_deref_mut().and_then(|s| s.sp_mut()) else { return false };
                match rest.iter().position(|g| *g == id) {
                    // The first box after the break holds the rest's story, empty; a chain only when more follow.
                    Some(0) => {
                        (sp.text, sp.chain) = (ShapeText::Story, (rest.len() > 1).then_some(fresh));
                        let b = &sp.body;
                        let pt = |e: crate::drawing::units::Emu| e.0 as f64 / 12700.0;
                        let v_anchor = match b.anchor {
                            crate::drawing::text::VAnchor::Centre => 1,
                            crate::drawing::text::VAnchor::Bottom => 2,
                            _ => 0,
                        };
                        let outline = sp
                            .line
                            .as_ref()
                            .filter(|l| !matches!(l.fill, Some(crate::drawing::paint::Fill::None)))
                            .and_then(|l| l.width)
                            .map_or(0.0, pt);
                        n.text_box = Some(crate::docx::TextBox {
                            blocks: vec![Block::Para(empty.clone())],
                            insets: b.insets.map(pt),
                            outline,
                            auto_fit: matches!(b.autofit, crate::drawing::text::AutoFit::Shape),
                            v_anchor,
                            flow: 0,
                        });
                        true
                    }
                    Some(j) => {
                        sp.text = ShapeText::Linked { chain: fresh, seq: j as u32 };
                        true
                    }
                    // The first box alone again keeps its story out of a chain.
                    None if last_kept && sp.text == ShapeText::Story => {
                        sp.chain = None;
                        true
                    }
                    None => false,
                }
            },
            fonts,
        )
    }

    /// A paragraph a new empty text box story holds.
    fn empty_story(&self) -> Option<crate::docx::Paragraph> {
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let r = crate::docx::para_xml::from_xml(&res, None, None, &cx);
        let mark = std::rc::Rc::new(run_xml::resolve(&res, &run_xml::base(&r.base), Default::default(), false));
        Some(crate::docx::Paragraph {
            style: std::rc::Rc::new(r.style),
            mark,
            list: None,
            inlines: Vec::new(),
            src: None,
            id: crate::docx::next_para_id(),
            div: None,
            controls: Vec::new(),
        })
    }

    /// Starts Word's Create Link pointer: the next click on a text box the selected one's text can go on in links it;
    /// a click elsewhere or Escape ends it.
    pub fn begin_linking(&mut self) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no text box selected") };
        let Some((_, boxes)) = self.chain_of(f) else { return Outcome::unavailable("no text box selected") };
        if boxes.last() != Some(&f) || self.mode == super::Mode::ReadOnly {
            return Outcome::unavailable("the text box already goes on in another");
        }
        self.linking = Some(f);
        Outcome::Moved
    }

    /// Whether Create Link's pointer is on.
    pub fn linking(&self) -> bool {
        self.linking.is_some_and(|f| self.selection == Selection::Object(ObjectRef::Floating(f)))
    }

    /// Ends Create Link's pointer; false when it wasn't on.
    pub fn end_linking(&mut self) -> bool {
        self.linking.take().is_some()
    }

    /// The text box under document point (`x`, `y`) the selected one's text can go on in, while Create Link's
    /// pointer is on (the pouring pitcher over it, the upright one elsewhere).
    pub fn link_target_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<FloatId> {
        if !self.linking() {
            return None;
        }
        let (page, px, py) = self.stack().page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let f = geo.stories.iter().rev().find(|s| {
            let [a, b, c, d] = s.rect;
            let [px, py] = s.turn.back([px, py]);
            s.float.is_some() && px >= a && px <= c && py >= b && py <= d
        })?;
        let f = f.float?;
        self.can_link_to(f).then_some(f)
    }

    /// A press while Create Link's pointer is on: on a box the text can go on in it links it, anywhere else it ends
    /// the pointer (true either way: the press is taken).
    pub(super) fn linking_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        if !self.linking() {
            self.linking = None;
            return false;
        }
        match self.link_target_at(x, y, fonts) {
            Some(t) => {
                self.link_text_box(t, fonts);
            }
            None => {
                self.end_linking();
            }
        }
        true
    }
}
