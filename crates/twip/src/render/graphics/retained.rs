//! Retained page content: each page's content recorded once into a picture, keyed by its page's drawing
//! ([`drawing_key`]) and the paint [`Inputs`], and placed by the frames after it. A frame that only moves what is drawn
//! over the pages (the selection, the caret's blink, a hover) records no page content again, and an edit records again
//! only the pages whose drawing changed: layout shares unchanged lines between layouts, so a page laid out again with
//! the same lines in the same places keeps its recording.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use twip_graphics::image::ImageId;
use twip_graphics::scene::{Picture, Recorder};

use crate::layout::fragment::{BoxFragment, BoxKind, Child, Item, PageChild, PageFragment};

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
                    r.paint.hash(h);
                }
                Item::Rect { rect, colour, snap, dash, phase } => (1u8, rect.map(f64::to_bits), colour, snap, dash, phase.to_bits()).hash(h),
                Item::Image { rect, key, .. } => (2u8, rect.map(f64::to_bits), key.index, key.generation).hash(h),
                Item::Gridline { line } => (3u8, line.map(f64::to_bits)).hash(h),
                // Shared by identity, as lines are: a layout that keeps the shape keeps its address.
                Item::Shape(s) => (4u8, Rc::as_ptr(s) as usize).hash(h),
                Item::Turned(t) => (5u8, Rc::as_ptr(t) as usize).hash(h),
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
                        for c in r.cells.iter() {
                            // The cell's clip
                            (c.place.rect.map(f64::to_bits), c.place.clip.map(f64::to_bits)).hash(h);
                            walk(&c.content, h);
                        }
                        items(&r.borders, h);
                    }
                }
                Child::Line { placed, .. } => (6u8, Rc::as_ptr(&placed.line) as usize, placed.x.to_bits(), placed.y.to_bits()).hash(h),
                Child::Items(i) | Child::Turned { items: i, .. } => {
                    7u8.hash(h);
                    items(i, h);
                }
                Child::Group { offset, children } => (8u8, offset.0.to_bits(), offset.1.to_bits(), children.as_ptr() as usize).hash(h),
            }
        }
    }
    fn boxed(b: &BoxFragment, h: &mut impl Hasher) {
        (9u8, b.offset.0.to_bits(), b.offset.1.to_bits(), b.size.0.to_bits(), b.size.1.to_bits()).hash(h);
        // A float's layer sets the order it paints in.
        if let BoxKind::Float { layer, .. } = &b.kind {
            layer.hash(h);
        }
        walk(&b.children, h);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (page.w.to_bits(), page.h.to_bits()).hash(&mut h);
    page.borders_behind.hash(&mut h);
    items(&page.borders, &mut h);
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
    pub appearance: crate::render::appearance::Appearance,
    /// The page background's fill, by [`crate::render::Renderer::page_fill_generation`].
    pub page_fill: u64,
}

struct Recording {
    /// Held so the lines its key names stay alive while the recording does.
    _fragment: Rc<PageFragment>,
    inputs: Inputs,
    picture: Picture,
    used: u64,
    /// The pictures with a changed colour it draws: they stay while it does.
    pins: Vec<ImageId>,
}

/// One presentation's retained page pictures.
pub struct Retained {
    recordings: HashMap<u64, Recording>,
    recorder: Recorder,
    clock: u64,
    /// The clock when the last frame ended: recordings used since are the frame's.
    trimmed: u64,
    /// The pictures' memory past which the least recently drawn go, bytes.
    pub budget: usize,
    pub counts: Counts,
}

impl Default for Retained {
    fn default() -> Retained {
        Retained { recordings: HashMap::new(), recorder: Recorder::new(), clock: 0, trimmed: 0, budget: 32 << 20, counts: Counts::default() }
    }
}

impl Retained {
    /// Forgets everything (a new document).
    pub fn clear(&mut self) {
        self.recordings.clear();
    }

    /// The pictures with a changed colour the recordings draw.
    pub fn pinned(&self) -> impl Iterator<Item = ImageId> + '_ {
        self.recordings.values().flat_map(|r| r.pins.iter().copied())
    }

    pub fn bytes(&self) -> usize {
        self.recordings.values().map(|r| r.picture.bytes()).sum()
    }

    /// `page`'s picture (in its points), recorded with `record` first when there is none for these inputs.
    pub fn picture(&mut self, page: &Rc<PageFragment>, inputs: Inputs, record: impl FnOnce(&mut Recorder) -> Vec<ImageId>) -> Picture {
        self.clock += 1;
        let key = drawing_key(page);
        match self.recordings.get_mut(&key).filter(|r| r.inputs == inputs) {
            Some(r) => {
                r.used = self.clock;
                self.counts.reused += 1;
                r.picture.clone()
            }
            None => {
                let pins = record(&mut self.recorder);
                let picture = self.recorder.finish();
                self.recordings.insert(key, Recording { _fragment: Rc::clone(page), inputs, picture: picture.clone(), used: self.clock, pins });
                self.counts.recorded += 1;
                picture
            }
        }
    }

    /// After a frame: drops the least recently drawn pictures past the budget, then, of those the frame didn't draw,
    /// the least recently drawn while the pictures with a changed colour they hold (each counted once, `pixels` bytes)
    /// are past `max_pinned` bytes.
    pub fn trim(&mut self, pixels: impl Fn(ImageId) -> u64, max_pinned: u64) {
        let trimmed = std::mem::replace(&mut self.trimmed, self.clock);
        let mut bytes = self.bytes();
        // Pinned bytes, an image two recordings hold counted twice.
        let pinning = self.pinned().map(&pixels).sum::<u64>() > max_pinned;
        if bytes <= self.budget && !pinning {
            return;
        }
        let mut by_age: Vec<(u64, u64)> = self.recordings.iter().map(|(k, r)| (r.used, *k)).collect();
        by_age.sort_unstable_by_key(|(used, _)| *used);
        let mut by_age = by_age.into_iter().peekable();
        while bytes > self.budget
            && let Some((_, k)) = by_age.next()
        {
            if let Some(r) = self.recordings.remove(&k) {
                bytes -= r.picture.bytes();
            }
        }
        if !pinning {
            return;
        }
        let mut holders: HashMap<ImageId, u32> = HashMap::new();
        for id in self.pinned() {
            *holders.entry(id).or_default() += 1;
        }
        let mut pinned: u64 = holders.keys().map(|&id| pixels(id)).sum();
        while pinned > max_pinned
            && let Some((_, k)) = by_age.next_if(|&(used, _)| used <= trimmed)
        {
            let Some(r) = self.recordings.remove(&k) else { continue };
            for id in r.pins {
                if let Some(n) = holders.get_mut(&id) {
                    *n -= 1;
                    if *n == 0 {
                        pinned -= pixels(id);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::fragment::{CellFragment, CellPlace, RowFragment, TableFragment};
    use crate::layout::ids::{FragmentId, TableId};

    /// Pages alike but for a cell's clip draw differently.
    #[test]
    fn a_cells_clip_is_part_of_the_drawing_key() {
        let page = |clip: Option<f64>| {
            let cell = CellFragment {
                cell: 0,
                content: Box::new([Child::Items(Box::new([Item::Gridline { line: [0.0, 0.0, 10.0, 30.0] }]))]),
                place: CellPlace {
                    index: 0,
                    col: 0,
                    span: 1,
                    rect: [0.0, 0.0, 10.0, 30.0],
                    clip,
                    v_merge: crate::docx::VMerge::None,
                    hidden: Box::new([]),
                    closing: None,
                },
            };
            let row = RowFragment {
                row: 0,
                rect: [0.0, 0.0, 10.0, 30.0],
                top: 0.0,
                edges: Box::new([]),
                background: Box::new([]),
                cells: Box::new([cell]),
                borders: Box::new([]),
                repeat: false,
                changed: false,
            };
            let body = BoxFragment {
                kind: BoxKind::Body,
                id: None,
                offset: (0.0, 0.0),
                size: (100.0, 100.0),
                children: Box::new([Child::Table(TableFragment { table: TableId(1), rows: Box::new([row]) })]),
            };
            PageFragment {
                id: FragmentId::next(),
                w: 100.0,
                h: 100.0,
                body_area: None,
                children: Box::new([PageChild::Body(Rc::new(body))]),
                borders: Box::new([]),
                borders_behind: false,
            }
        };
        assert_eq!(drawing_key(&page(Some(20.0))), drawing_key(&page(Some(20.0))));
        assert_ne!(drawing_key(&page(Some(20.0))), drawing_key(&page(Some(10.0))));
        assert_ne!(drawing_key(&page(Some(20.0))), drawing_key(&page(None)));
    }

    /// Recordings holding pictures with a changed colour past the bound go, least recently drawn first, a picture two
    /// of them hold counted once; the frame's own stay over it.
    #[test]
    fn pinned_pictures_past_the_bound_evict_the_recordings_holding_them() {
        let mut images = twip_graphics::Resources::default().images;
        let mut image = || images.add_pixels(5, 5, vec![0; 100]).expect("pixels");
        let (shared, own) = (image(), [image(), image(), image()]);
        let pixels = |_: ImageId| 100;
        let pages: Vec<Rc<PageFragment>> = (0..3)
            .map(|i| {
                Rc::new(PageFragment {
                    id: FragmentId::next(),
                    w: 100.0 + f64::from(i),
                    h: 100.0,
                    body_area: None,
                    children: Box::new([]),
                    borders: Box::new([]),
                    borders_behind: false,
                })
            })
            .collect();
        let inputs = Inputs { gridlines: false, appearance: Default::default(), page_fill: 0 };
        let mut kept = Retained::default();
        let draw = |kept: &mut Retained, i: usize| {
            kept.picture(&pages[i], inputs, |_| vec![shared, own[i]]);
        };
        (0..3).for_each(|i| draw(&mut kept, i));
        kept.trim(pixels, 0);
        assert_eq!(kept.recordings.len(), 3, "the frame's recordings stay over the bound");
        draw(&mut kept, 2);
        kept.trim(pixels, 300);
        assert_eq!(kept.recordings.len(), 2, "the least recently drawn goes; the shared picture counts once");
        draw(&mut kept, 2);
        kept.trim(pixels, 200);
        assert_eq!(kept.recordings.len(), 1);
        assert_eq!(kept.counts, Counts { recorded: 3, reused: 2 });
        let left: Vec<ImageId> = kept.pinned().collect();
        assert!(left.len() == 2 && left.contains(&shared) && left.contains(&own[2]), "{left:?}");
    }
}
