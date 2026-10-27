//! In-progress interactions (selection drags, resizes, object moves) with their preview, commit and cancel.
//!
//! Drag-and-drop of selected text: a press inside a text selection starts it; a release without
//! moving collapses the selection there; a release elsewhere moves the text (Ctrl: copies it) as one undo step; a
//! release inside the selection, or Esc, does nothing.

use crate::font::Fonts;

use super::Editor;
use super::commands::{Command, Outcome};
use super::input::Request;
use super::position::Position;
use super::selection::Selection;

/// A press inside the selection, waiting to become a drop.
#[derive(Clone, Copy, Debug)]
pub(super) struct TextDrag {
    /// Where the press was (document points) and the position under it.
    pub start: (f64, f64),
    pub press: Position,
    /// Past the threshold: a drag, not a click.
    pub moved: bool,
    pub copy: bool,
    /// Where the text would go.
    pub drop: Option<Position>,
}

impl Editor {
    /// Whether `p` is inside the text selection (its ends excluded).
    pub(super) fn inside_selection(&self, p: &Position) -> bool {
        let Selection::Text { .. } = self.selection else { return false };
        let Some((a, b)) = self.selection.ordered(&self.index) else { return false };
        self.index.cmp(&a, p) == std::cmp::Ordering::Less && self.index.cmp(p, &b) == std::cmp::Ordering::Less
    }

    pub(super) fn text_drag_move(&mut self, x: f64, y: f64, ctrl: bool, fonts: &Fonts) {
        let Some(mut d) = self.text_drag else { return };
        // 3 pixels at the zoom, as a drag threshold.
        let tol = 2.25 / self.viewport.zoom.max(0.1);
        if !d.moved && ((x - d.start.0).abs() > tol || (y - d.start.1).abs() > tol) {
            d.moved = true;
        }
        if d.moved {
            d.drop = self.hit(x, y, fonts);
            d.copy |= ctrl;
        }
        self.text_drag = Some(d);
    }

    /// The release: a click puts the caret there; a drop moves or copies the text.
    pub(super) fn text_drag_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.text_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        if !d.moved {
            self.set_selection(Selection::Caret(d.press));
            return Some(Outcome::Moved);
        }
        let Some(drop) = d.drop else { return Some(Outcome::Unchanged("no drop place")) };
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Some(Outcome::Unchanged("no selection")) };
        // Into a text box the selection holds: as onto the selection (the box goes with the text).
        let in_box = match drop.story {
            super::position::StoryId::TextBox(f) => self.index.float_anchor(f).and_then(|para| {
                let p = self.index.para(&self.doc, para)?;
                let k = p.inlines.iter().position(|i| matches!(i, crate::docx::Inline::Float(x, _) if crate::layout::FloatId(x.id) == f))?;
                let u: u32 = p.inlines[..k].iter().map(|i| i.units() as u32).sum();
                let story = self.index.para_slot(para).map(|s| self.index.story(s.container))?;
                Some(Position::text(story, para, crate::layout::Unit(u)))
            }),
            _ => None,
        };
        let inside = |p: &Position| self.index.cmp(&a, p) != std::cmp::Ordering::Greater && self.index.cmp(p, &b) != std::cmp::Ordering::Greater;
        if inside(&drop) || in_box.is_some_and(|p| inside(&p)) {
            // Onto itself: nothing moves.
            return Some(Outcome::Unchanged("dropped on the selection"));
        }
        let frag = self.copy_fragment()?;
        let sel = self.selection;
        // The dropped text's ends, followed through the edits after its paste.
        let mut placed: Option<(Position, Position)> = None;
        let paste = |e: &mut Self, placed: &mut Option<(Position, Position)>| {
            e.set_selection(Selection::Caret(drop));
            let start = drop;
            let ok = matches!(e.paste_fragment(&frag, fonts), Outcome::Changed(_));
            // The paste goes in at `drop`, which stays its start (a split there keeps it in the first part).
            if ok && let Some(end) = e.selection.focus() {
                *placed = Some((start, end));
            }
            ok
        };
        let delete = |e: &mut Self, placed: &mut Option<(Position, Position)>| {
            e.set_selection(sel);
            // Smart cut: the space the move would leave doubled goes with it.
            e.smart_cut();
            let ok = matches!(e.run(Command::Delete { dir: super::commands::Dir::Forward, word: false }, fonts), Outcome::Changed(_));
            if ok && let Some((s, t)) = placed.as_mut() {
                for m in &e.last_maps {
                    (*s, *t) = (m.apply(*s), m.apply(*t));
                }
            }
            ok
        };
        // The edit further down first, so the other place is still where it was.
        let after = self.index.cmp(&drop, &b) == std::cmp::Ordering::Greater;
        #[allow(clippy::if_same_then_else, reason = "the order differs: each call moves what the other one sees")]
        let steps = if d.copy {
            paste(self, &mut placed) as usize
        } else if after {
            paste(self, &mut placed) as usize + delete(self, &mut placed) as usize
        } else {
            delete(self, &mut placed) as usize + paste(self, &mut placed) as usize
        };
        if steps == 0 {
            return Some(Outcome::Unchanged("nothing dropped"));
        }
        // The dropped text is selected.
        if let Some((s, t)) = placed {
            self.selection = Selection::range(s, t, super::selection::Granularity::Char);
        }
        self.history.merge_last(steps, if d.copy { "Copy" } else { "Move" });
        Some(Outcome::Changed(self.revision))
    }
}
