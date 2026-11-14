//! A map for the renderer's caches: small integer keys hashed with the multiply-rotate hash rustc uses (FxHash), far
//! cheaper than SipHash per lookup.
//!
//! Unseeded, and `finish` is the bare product, whose low bits (where hashbrown takes a bucket from) depend only on the
//! key's low bits: keys that differ only above them share a probe chain. Glyph ids are a face's, and faces come from
//! the host (a document names a family, never a face's bytes); the glyph caches still see them at most 0xFFFF
//! (`font::glyph_id`), which leaves a face at most a few hundred ids sharing a bucket. A map keyed by something wide
//! that a document chooses needs a mixing `finish`.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

pub(crate) type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

#[derive(Default)]
pub(crate) struct FxHasher(u64);

impl FxHasher {
    #[inline]
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }
    #[inline]
    fn write_u8(&mut self, v: u8) {
        self.add(v as u64);
    }
    #[inline]
    fn write_u32(&mut self, v: u32) {
        self.add(v as u64);
    }
    #[inline]
    fn write_i32(&mut self, v: i32) {
        self.add(v as u32 as u64);
    }
    #[inline]
    fn write_u64(&mut self, v: u64) {
        self.add(v);
    }
    #[inline]
    fn write_usize(&mut self, v: usize) {
        self.add(v as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}
