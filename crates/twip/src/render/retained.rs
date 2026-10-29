//! Retained page content: each page's content (its paper and items) recorded once into a scale-free picture and
//! replayed by the frames after it, so a frame that only moves what is drawn over the pages (the selection, the caret's
//! blink, a hover) walks no page content again, and an edit records again only the pages whose drawing changed. A
//! recording is keyed by its page's drawing ([`drawing_key`]): layout shares unchanged lines between layouts, so a page
//! laid out again with the same lines in the same places keeps its recording.
//!
//! The pictures are replayed, not rastered into tiles: on the benchmark documents a tile composite cost more than
//! drawing the content again, on the CPU raster at either density.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use skia_safe::{Canvas, Picture, Rect};

use crate::layout::fragment::{BoxFragment, Child, Item, PageChild, PageFragment};

/// What a page draws, as a hash: its size, every shared line and group by identity and place, and the loose items
/// by value. Two pages with one key draw alike. The lines are `Rc`s a recording keeps alive (it holds its fragment),
/// so an address isn't reused while a key that names it is in use.
pub fn drawing_key(page: &PageFragment) -> u64 {
    fn items(items: &[Item], h: &mut impl Hasher) {
        for i in items {
            match i {
                Item::Glyphs(r) => {
                    0u8.hash(h);
                    (Rc::as_ptr(&r.face) as usize, r.size.to_bits(), r.baseline.to_bits(), r.x.to_bits(), r.colour).hash(h);
                    r.glyphs.iter().for_each(|(g, x)| (g, x.to_bits()).hash(h));
                }
                Item::Rect { rect, colour } => (1u8, rect.map(f64::to_bits), colour).hash(h),
                Item::Image { rect, key } => (2u8, rect.map(f64::to_bits), key.index, key.generation).hash(h),
                Item::Gridline { line } => (3u8, line.map(f64::to_bits)).hash(h),
            }
        }
    }
    fn walk(children: &[Child], h: &mut impl Hasher) {
        for c in children {
            match c {
                Child::Box(b) => boxed(b, h),
                Child::Para(p) => {
                    4u8.hash(h);
                    p.lines.iter().for_each(|l| (Rc::as_ptr(&l.line) as usize, l.x.to_bits(), l.y.to_bits()).hash(h));
                    p.line_numbers.iter().for_each(|n| items(n, h));
                }
                Child::Table(t) => {
                    5u8.hash(h);
                    for r in t.rows.iter() {
                        (r.rect.map(f64::to_bits), r.top.to_bits(), r.repeat).hash(h);
                        items(&r.background, h);
                        r.cells.iter().for_each(|c| walk(&c.content, h));
                        items(&r.borders, h);
                    }
                }
                Child::Line { placed, .. } => (6u8, Rc::as_ptr(&placed.line) as usize, placed.x.to_bits(), placed.y.to_bits()).hash(h),
                Child::Items(i) => {
                    7u8.hash(h);
                    items(i, h);
                }
                Child::Group { offset, children } => (8u8, offset.0.to_bits(), offset.1.to_bits(), children.as_ptr() as usize).hash(h),
            }
        }
    }
    fn boxed(b: &BoxFragment, h: &mut impl Hasher) {
        (9u8, b.offset.0.to_bits(), b.offset.1.to_bits(), b.size.0.to_bits(), b.size.1.to_bits()).hash(h);
        walk(&b.children, h);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (page.w.to_bits(), page.h.to_bits()).hash(&mut h);
    for c in page.children.iter() {
        match c {
            PageChild::Body(b) => boxed(b, &mut h),
            PageChild::Header(b) | PageChild::Footer(b) => boxed(b, &mut h),
        }
    }
    h.finish()
}

/// What the retained content cost: for the frame statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub recorded: u64,
    pub reused: u64,
}

/// The paint inputs a recording depends on besides its page's drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs {
    /// Table gridlines (cell edges without borders) are drawn.
    pub gridlines: bool,
}

struct Recording {
    /// Held so the lines its key names stay alive while the recording does.
    _fragment: Rc<PageFragment>,
    inputs: Inputs,
    picture: Picture,
    bytes: usize,
    used: u64,
}

/// One presentation's retained page content.
pub struct Retained {
    recordings: HashMap<u64, Recording>,
    clock: u64,
    /// The recordings' memory past which the least recently drawn go, bytes.
    pub budget: usize,
    pub counts: Counts,
}

impl Default for Retained {
    fn default() -> Retained {
        Retained { recordings: HashMap::new(), clock: 0, budget: 32 << 20, counts: Counts::default() }
    }
}

impl Retained {
    /// Forgets everything (a new document).
    pub fn clear(&mut self) {
        self.recordings.clear();
    }

    pub fn bytes(&self) -> usize {
        self.recordings.values().map(|r| r.bytes).sum()
    }

    /// Draws `page` (`w` x `h` points) onto `canvas` (in that page's points) from its recording, recording it with
    /// `record` first when it has none for these inputs.
    pub fn draw(&mut self, canvas: &Canvas, page: &Rc<PageFragment>, inputs: Inputs, (w, h): (f64, f64), record: impl FnOnce(&Canvas)) {
        self.clock += 1;
        let key = drawing_key(page);
        match self.recordings.get_mut(&key).filter(|r| r.inputs == inputs) {
            Some(r) => {
                r.used = self.clock;
                self.counts.reused += 1;
            }
            None => {
                let mut recorder = skia_safe::PictureRecorder::new();
                record(recorder.begin_recording(Rect::from_wh(w as f32, h as f32), false));
                let Some(picture) = recorder.finish_recording_as_picture(None) else { return };
                let bytes = picture.approximate_bytes_used();
                self.recordings.insert(key, Recording { _fragment: page.clone(), inputs, picture, bytes, used: self.clock });
                self.counts.recorded += 1;
            }
        }
        if let Some(r) = self.recordings.get(&key) {
            canvas.draw_picture(&r.picture, None, None);
        }
    }

    /// After a frame: drops the least recently drawn recordings past the budget.
    pub fn trim(&mut self) {
        let mut bytes = self.bytes();
        if bytes <= self.budget {
            return;
        }
        let mut by_age: Vec<(u64, u64)> = self.recordings.iter().map(|(k, r)| (r.used, *k)).collect();
        by_age.sort_unstable_by_key(|(used, _)| *used);
        for (_, k) in by_age {
            if bytes <= self.budget {
                break;
            }
            if let Some(r) = self.recordings.remove(&k) {
                bytes -= r.bytes;
            }
        }
    }
}
