//! Retained page content for the twip-graphics screen: each page's content recorded once into a picture, keyed as
//! the Skia path keys its recordings ([`drawing_key`] and the paint [`Inputs`]), and placed by the frames after it. A
//! frame that only moves what is drawn over the pages records no page content again.

use std::collections::HashMap;
use std::rc::Rc;

use twip_graphics::scene::{Picture, Recorder};

use crate::layout::fragment::PageFragment;
use crate::render::retained::{Counts, Inputs, drawing_key};

struct Recording {
    /// Held so the lines its key names stay alive while the recording does.
    _fragment: Rc<PageFragment>,
    inputs: Inputs,
    picture: Picture,
    used: u64,
}

/// One presentation's retained page pictures.
pub struct Retained {
    recordings: HashMap<u64, Recording>,
    recorder: Recorder,
    clock: u64,
    /// The pictures' memory past which the least recently drawn go, bytes.
    pub budget: usize,
    pub counts: Counts,
}

impl Default for Retained {
    fn default() -> Retained {
        Retained { recordings: HashMap::new(), recorder: Recorder::new(), clock: 0, budget: 32 << 20, counts: Counts::default() }
    }
}

impl Retained {
    /// Forgets everything (a new document).
    pub fn clear(&mut self) {
        self.recordings.clear();
    }

    pub fn bytes(&self) -> usize {
        self.recordings.values().map(|r| r.picture.bytes()).sum()
    }

    /// `page`'s picture (in its points), recorded with `record` first when there is none for these inputs. `None` when
    /// the page draws nothing.
    pub fn picture(&mut self, page: &Rc<PageFragment>, inputs: Inputs, record: impl FnOnce(&mut Recorder)) -> Option<Picture> {
        self.clock += 1;
        let key = drawing_key(page);
        match self.recordings.get_mut(&key).filter(|r| r.inputs == inputs) {
            Some(r) => {
                r.used = self.clock;
                self.counts.reused += 1;
            }
            None => {
                record(&mut self.recorder);
                let picture = self.recorder.finish();
                self.recordings.insert(key, Recording { _fragment: Rc::clone(page), inputs, picture, used: self.clock });
                self.counts.recorded += 1;
            }
        }
        self.recordings.get(&key).map(|r| r.picture.clone())
    }

    /// After a frame: drops the least recently drawn pictures past the budget.
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
                bytes -= r.picture.bytes();
            }
        }
    }
}
