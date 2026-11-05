//! Layout cache

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;
use std::sync::OnceLock;

use super::job::{Memos, Yield};

#[derive(Default)]
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for c in chunks {
            self.write_u64(u64::from_le_bytes(*c));
        }
        for &b in rest {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type Buckets<K, V> = HashMap<u64, Slot<K, V>, BuildHasherDefault<KeyHasher>>;

enum Slot<K, V> {
    One(Entry<K, V>),
    Many(Vec<Entry<K, V>>),
}

impl<K, V> Slot<K, V> {
    fn entries(&self) -> &[Entry<K, V>] {
        match self {
            Slot::One(e) => std::slice::from_ref(e),
            Slot::Many(v) => v,
        }
    }

    fn entries_mut(&mut self) -> &mut [Entry<K, V>] {
        match self {
            Slot::One(e) => std::slice::from_mut(e),
            Slot::Many(v) => v,
        }
    }

    fn push(&mut self, e: Entry<K, V>) {
        match std::mem::replace(self, Slot::Many(Vec::new())) {
            Slot::One(first) => *self = Slot::Many(vec![first, e]),
            Slot::Many(mut v) => {
                v.push(e);
                *self = Slot::Many(v);
            }
        }
    }
}

pub trait CacheKey: Debug + Sized {
    type Probe<'a>;
    fn bucket(probe: &Self::Probe<'_>) -> u64;
    fn matches(&self, probe: &Self::Probe<'_>) -> bool;
    fn from_probe(probe: &Self::Probe<'_>) -> Self;
    const PIECE: Option<crate::prof::Piece> = None;
}

struct Entry<K, V> {
    key: K,
    value: Rc<V>,
    used: u64,
    bytes: usize,
}

pub struct Boundary<K, V> {
    name: &'static str,
    buckets: Buckets<K, V>,
    generation: u64,
    hits: u64,
    misses: u64,
    added: usize,
}

impl<K: CacheKey, V> Boundary<K, V> {
    pub fn new(name: &'static str) -> Boundary<K, V> {
        Boundary { name, buckets: Buckets::default(), generation: 0, hits: 0, misses: 0, added: 0 }
    }

    fn evictable_by_generation(&self, by_generation: &mut std::collections::BTreeMap<u64, usize>) {
        for e in self.buckets.values().flat_map(Slot::entries) {
            if Rc::strong_count(&e.value) == 1 {
                *by_generation.entry(e.used).or_default() += e.bytes;
            }
        }
    }

    fn evict(&mut self, cutoff: u64, more: &mut usize) {
        let mut gone = |e: &Entry<K, V>| {
            if Rc::strong_count(&e.value) != 1 || e.used > cutoff {
                return false;
            }
            if e.used < cutoff {
                return true;
            }
            if *more == 0 {
                return false;
            }
            *more = more.saturating_sub(e.bytes);
            true
        };
        self.buckets.retain(|_, slot| match slot {
            Slot::One(e) => !gone(e),
            Slot::Many(v) => {
                v.retain(|e| !gone(e));
                !v.is_empty()
            }
        });
    }

    pub fn lookup(&mut self, probe: &K::Probe<'_>) -> Option<Rc<V>> {
        let generation = self.generation;
        let found = self.buckets.get_mut(&K::bucket(probe)).and_then(|b| b.entries_mut().iter_mut().find(|e| e.key.matches(probe)));
        match found {
            Some(e) => {
                e.used = generation;
                self.hits += 1;
                Some(e.value.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub fn insert(&mut self, probe: &K::Probe<'_>, value: Rc<V>, bytes: usize) {
        let generation = self.generation;
        use std::collections::hash_map::Entry as Bucket;
        match self.buckets.entry(K::bucket(probe)) {
            Bucket::Vacant(v) => {
                self.added += bytes;
                v.insert(Slot::One(Entry { key: K::from_probe(probe), value, used: generation, bytes }));
            }
            Bucket::Occupied(mut o) => match o.get_mut().entries_mut().iter_mut().find(|e| e.key.matches(probe)) {
                Some(e) => {
                    e.value = value;
                    e.used = generation;
                    e.bytes = bytes;
                }
                None => {
                    self.added += bytes;
                    o.get_mut().push(Entry { key: K::from_probe(probe), value, used: generation, bytes });
                }
            },
        }
    }

    pub fn begin(&mut self, generation: u64) {
        self.generation = generation;
    }

    pub fn len(&self) -> usize {
        self.buckets.values().map(|s| s.entries().len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    pub fn bytes(&self) -> (usize, usize) {
        let (mut evictable, mut live) = (0, 0);
        for e in self.buckets.values().flat_map(Slot::entries) {
            if Rc::strong_count(&e.value) == 1 {
                evictable += e.bytes;
            } else {
                live += e.bytes;
            }
        }
        (evictable, live)
    }
}

thread_local! {
    static VERIFYING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn verifying() -> bool {
    VERIFYING.get()
}

pub fn verify_on() -> bool {
    if cfg!(test) {
        return true;
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("LAYOUT_VERIFY_CACHE").is_ok_and(|v| v == "1"))
}

pub fn cached<K: CacheKey, V: PartialEq + Debug>(
    memos: &mut Memos,
    select: fn(&mut LayoutCache) -> &mut Boundary<K, V>,
    probe: K::Probe<'_>,
    bytes: impl FnOnce(&V) -> usize,
    compute: impl FnOnce(&mut Memos) -> Result<V, Yield>,
) -> Result<Rc<V>, Yield> {
    if let Some(found) = select(memos.cache).lookup(&probe) {
        if verify_on() && !VERIFYING.get() {
            struct Reset;
            impl Drop for Reset {
                fn drop(&mut self) {
                    VERIFYING.set(false);
                }
            }
            VERIFYING.set(true);
            let reset = Reset;
            let again = compute(memos);
            drop(reset);
            let again = again?;
            if *found != again {
                let name = select(memos.cache).name();
                panic!(
                    "layout cache verify mode: boundary `{name}` returned a stale result {found:?}, now {again:?}, for key {:?}",
                    K::from_probe(&probe)
                );
            }
        }
        if !VERIFYING.get()
            && let Some(piece) = K::PIECE
        {
            crate::prof::count(piece, crate::prof::Outcome::Moved);
        }
        return Ok(found);
    }
    let value = compute(memos)?;
    if !VERIFYING.get()
        && let Some(piece) = K::PIECE
    {
        crate::prof::count(piece, crate::prof::Outcome::Laid);
    }
    let size = bytes(&value);
    let value = Rc::new(value);
    select(memos.cache).insert(&probe, value.clone(), size);
    memos.meter.check()?;
    Ok(value)
}

pub const DEFAULT_BUDGET: usize = 32 << 20;

const PAGE_BUDGET: usize = 512 << 10;

const MAX_BUDGET: usize = 128 << 20;

fn page_budget(pages: usize) -> usize {
    pages.saturating_mul(PAGE_BUDGET).clamp(DEFAULT_BUDGET, MAX_BUDGET)
}

const EVICT_CHECK: usize = 1 << 20;

pub struct LayoutCache {
    generation: u64,
    budget: usize,
    pub(crate) store_tables: bool,
    pub(crate) shaped: Boundary<super::text::ShapeKey, super::text::ShapedPara>,
    pub(crate) lines: Boundary<super::line::LinesKey, super::line::ParaLines>,
    pub(crate) cell_widths: Boundary<super::story::StoryKey, (i64, i64, i64)>,
    pub(crate) stories: Boundary<super::story::StoryKey, super::story::StoryLayout>,
    pub(crate) tables: Boundary<super::table::TableKey, super::table::TableLayout>,
    pub(crate) rows: Boundary<super::table::RowKey, super::table::DrawnRow>,
}

impl Default for LayoutCache {
    fn default() -> Self {
        LayoutCache::new()
    }
}

impl LayoutCache {
    pub fn new() -> LayoutCache {
        LayoutCache {
            generation: 0,
            budget: DEFAULT_BUDGET,
            store_tables: true,
            shaped: Boundary::new("shaped"),
            lines: Boundary::new("lines"),
            cell_widths: Boundary::new("cell widths"),
            stories: Boundary::new("stories"),
            tables: Boundary::new("tables"),
            rows: Boundary::new("rows"),
        }
    }

    pub fn with_budget(budget: usize) -> LayoutCache {
        LayoutCache { budget, ..LayoutCache::new() }
    }

    pub fn fit(&mut self, pages: usize) {
        self.budget = page_budget(pages);
    }

    pub fn begin(&mut self) {
        let added = self.shaped.added + self.lines.added + self.cell_widths.added + self.stories.added + self.tables.added + self.rows.added;
        if added >= EVICT_CHECK {
            self.shaped.added = 0;
            self.lines.added = 0;
            self.cell_widths.added = 0;
            self.stories.added = 0;
            self.tables.added = 0;
            self.rows.added = 0;
            self.enforce_budget();
        }
        self.generation += 1;
        let g = self.generation;
        self.shaped.begin(g);
        self.lines.begin(g);
        self.cell_widths.begin(g);
        self.stories.begin(g);
        self.tables.begin(g);
        self.rows.begin(g);
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn enforce_budget(&mut self) {
        for _ in 0..6 {
            let before = self.len();
            if self.evict_once() || self.len() == before {
                return;
            }
        }
    }

    fn evict_once(&mut self) -> bool {
        let mut by_generation = std::collections::BTreeMap::new();
        self.shaped.evictable_by_generation(&mut by_generation);
        self.lines.evictable_by_generation(&mut by_generation);
        self.cell_widths.evictable_by_generation(&mut by_generation);
        self.stories.evictable_by_generation(&mut by_generation);
        self.tables.evictable_by_generation(&mut by_generation);
        self.rows.evictable_by_generation(&mut by_generation);
        let total: usize = self.report().iter().map(|r| r.2 + r.3).sum();
        if total <= self.budget || by_generation.is_empty() {
            return true;
        }
        let mut excess = total - self.budget;
        let mut cutoff = 0;
        for (gen_, bytes) in by_generation {
            cutoff = gen_;
            if bytes >= excess {
                break;
            }
            excess -= bytes;
        }
        self.shaped.evict(cutoff, &mut excess);
        self.lines.evict(cutoff, &mut excess);
        self.cell_widths.evict(cutoff, &mut excess);
        self.stories.evict(cutoff, &mut excess);
        self.tables.evict(cutoff, &mut excess);
        self.rows.evict(cutoff, &mut excess);
        false
    }

    pub fn len(&self) -> usize {
        self.shaped.len() + self.lines.len() + self.cell_widths.len() + self.stories.len() + self.tables.len() + self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn report(&self) -> Vec<(&'static str, usize, usize, usize)> {
        let row = |name, len, (e, l)| (name, len, e, l);
        vec![
            row(self.shaped.name(), self.shaped.len(), self.shaped.bytes()),
            row(self.lines.name(), self.lines.len(), self.lines.bytes()),
            row(self.cell_widths.name(), self.cell_widths.len(), self.cell_widths.bytes()),
            row(self.stories.name(), self.stories.len(), self.stories.bytes()),
            row(self.tables.name(), self.tables.len(), self.tables.bytes()),
            row(self.rows.name(), self.rows.len(), self.rows.bytes()),
        ]
    }

    pub fn counts(&self) -> (u64, u64) {
        [self.shaped.counts(), self.lines.counts(), self.cell_widths.counts(), self.stories.counts(), self.tables.counts(), self.rows.counts()]
            .into_iter()
            .fold((0, 0), |(h, m), (a, b)| (h + a, m + b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Key(u32);

    impl CacheKey for Key {
        type Probe<'a> = u32;
        fn bucket(p: &u32) -> u64 {
            u64::from(p % 2)
        }
        fn matches(&self, p: &u32) -> bool {
            self.0 == *p
        }
        fn from_probe(p: &u32) -> Key {
            Key(*p)
        }
    }

    #[test]
    fn hits_require_equality_not_just_the_bucket() {
        let mut b: Boundary<Key, String> = Boundary::new("test");
        b.insert(&1, Rc::new("one".into()), 3);
        assert_eq!(b.lookup(&1).as_deref().map(String::as_str), Some("one"));
        assert!(b.lookup(&3).is_none(), "same bucket, different key");
        assert_eq!(b.counts(), (1, 1));
    }

    #[test]
    fn the_budget_follows_the_page_count_within_bounds() {
        assert_eq!(page_budget(0), DEFAULT_BUDGET);
        assert_eq!(page_budget(20), DEFAULT_BUDGET, "a short document keeps the default");
        assert_eq!(page_budget(107), 107 * PAGE_BUDGET);
        assert_eq!(page_budget(10_000), MAX_BUDGET, "a long one stops at the cap");
        assert_eq!(page_budget(usize::MAX), MAX_BUDGET, "no overflow");
    }

    #[test]
    fn shared_entries_are_live_not_evictable() {
        let mut b: Boundary<Key, String> = Boundary::new("test");
        let held = Rc::new("held".to_string());
        b.insert(&1, held.clone(), 4);
        b.insert(&2, Rc::new("alone".into()), 5);
        assert_eq!(b.bytes(), (5, 4));
    }
}
