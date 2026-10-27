//! The recording renderer: it consumes frames as a renderer would, recording each frame's
//! `View` as canonical JSON (sorted keys, 0.01 pt), and can lag behind or drop frames. Editor state must not depend on
//! whether, when or how often frames are consumed.

use std::collections::VecDeque;

use super::super::Editor;
use super::super::view::View;

pub struct Recorder {
    pub frames: Vec<String>,
    /// Views built but not consumed yet (a lagging consumer).
    pending: VecDeque<View>,
    delay: usize,
    /// Every n-th frame is dropped (0: none).
    drop_every: usize,
    built: usize,
}

pub use super::super::view::canonical;

impl Recorder {
    pub fn new(delay: usize, drop_every: usize) -> Recorder {
        Recorder { frames: Vec::new(), pending: VecDeque::new(), delay, drop_every, built: 0 }
    }

    /// One frame: the view of the first two pages is built now and consumed `delay` frames later, unless dropped.
    pub fn frame(&mut self, e: &mut Editor) {
        self.built += 1;
        if self.drop_every > 0 && self.built.is_multiple_of(self.drop_every) {
            return;
        }
        self.pending.push_back(e.view(0..e.stack().pages.len().min(2)));
        while self.pending.len() > self.delay {
            let v = self.pending.pop_front().expect("a pending view");
            self.frames.push(canonical(&v));
        }
    }
}
