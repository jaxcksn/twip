//! Ranges of the kept instance buffer: first fit over a sorted free list, freed ranges merged with their neighbours.
//! When no free range fits, the buffer grows (doubling) and the new range goes at the end; the renderer then makes a
//! larger buffer and uploads every live range again.

use std::ops::Range;

#[derive(Debug, Default)]
pub(super) struct Arena {
    /// Instances the buffer holds.
    capacity: u32,
    /// Free ranges, sorted and apart (no two touch).
    free: Vec<Range<u32>>,
}

impl Arena {
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// A range of `n` instances (`n` > 0), growing the buffer when no free range holds it.
    pub fn alloc(&mut self, n: u32) -> Range<u32> {
        if let Some(i) = self.free.iter().position(|r| r.end - r.start >= n) {
            let r = &mut self.free[i];
            let start = r.start;
            r.start += n;
            if r.start == r.end {
                self.free.remove(i);
            }
            return start..start + n;
        }
        // The free range at the end, if any, is extended by the growth.
        let tail = self.free.last().filter(|r| r.end == self.capacity).map_or(self.capacity, |r| r.start);
        let need = tail + n;
        let mut capacity = self.capacity.max(1024);
        while capacity < need {
            capacity *= 2;
        }
        if tail < self.capacity {
            self.free.pop();
        }
        if need < capacity {
            self.free.push(need..capacity);
        }
        self.capacity = capacity;
        tail..need
    }

    /// Returns `r` to the free list.
    pub fn free(&mut self, r: Range<u32>) {
        if r.start >= r.end {
            return;
        }
        let i = self.free.partition_point(|f| f.start < r.start);
        self.free.insert(i, r);
        // Merge with the next, then the previous.
        if i + 1 < self.free.len() && self.free[i].end == self.free[i + 1].start {
            self.free[i].end = self.free[i + 1].end;
            self.free.remove(i + 1);
        }
        if i > 0 && self.free[i - 1].end == self.free[i].start {
            self.free[i - 1].end = self.free[i].end;
            self.free.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_reused_merged_and_grown_into() {
        let mut a = Arena::default();
        let x = a.alloc(600);
        let y = a.alloc(300);
        assert_eq!((x.clone(), y.clone(), a.capacity()), (0..600, 600..900, 1024));
        // Too large for the 124 left: the buffer doubles, the range starting where the free tail did.
        let z = a.alloc(200);
        assert_eq!((z.clone(), a.capacity()), (900..1100, 2048));
        // Freed neighbours merge into one range, which then holds a larger request.
        a.free(x);
        a.free(y);
        assert_eq!(a.alloc(850), 0..850);
        a.free(0..850);
        a.free(z);
        assert_eq!(a.free, vec![(0..2048)]);
        assert_eq!(a.alloc(2048), 0..2048);
        assert_eq!(a.capacity(), 2048);
    }
}
