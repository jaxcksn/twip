//! A map for the renderer's caches: small integer keys hashed with the multiply-rotate hash rustc uses (FxHash). Far
//! cheaper than SipHash per lookup, and every key is the renderer's own, so collisions can't be chosen by an attacker.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

pub(crate) type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

#[derive(Default)]
pub(crate) struct FxHasher(u64);

impl FxHasher {
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }
    fn write_u8(&mut self, v: u8) {
        self.add(v as u64);
    }
    fn write_u32(&mut self, v: u32) {
        self.add(v as u64);
    }
    fn write_i32(&mut self, v: i32) {
        self.add(v as u32 as u64);
    }
    fn write_u64(&mut self, v: u64) {
        self.add(v);
    }
    fn write_usize(&mut self, v: usize) {
        self.add(v as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
