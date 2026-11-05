//! Job budgets

use std::time::Instant;

use super::cache::LayoutCache;

#[derive(Clone, Copy, Debug)]
pub enum Budget {
    Until(Instant),
    Units(u32),
    Pages(u32),
    Unlimited,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Yield;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Done,
    Yielded,
}

#[derive(Debug)]
pub struct Meter {
    budget: Budget,
    units: u32,
    pages: u32,
    exhausted: bool,
}

impl Meter {
    pub fn new(budget: Budget) -> Meter {
        Meter { budget, units: 0, pages: 0, exhausted: false }
    }

    pub fn unlimited() -> Meter {
        Meter::new(Budget::Unlimited)
    }

    pub fn unit_done(&mut self) {
        if super::cache::verifying() {
            return;
        }
        self.units += 1;
        self.exhausted |= match self.budget {
            Budget::Unlimited | Budget::Pages(_) => false,
            Budget::Units(n) => self.units >= n,
            Budget::Until(t) => Instant::now() >= t,
        };
    }

    pub fn check(&self) -> Result<(), Yield> {
        if self.exhausted { Err(Yield) } else { Ok(()) }
    }

    pub fn page_done(&mut self) -> Result<(), Yield> {
        self.pages += 1;
        let out = match self.budget {
            Budget::Pages(n) => self.pages >= n,
            Budget::Until(t) => Instant::now() >= t,
            _ => false,
        };
        if out || self.exhausted { Err(Yield) } else { Ok(()) }
    }

    pub fn budget(&self) -> Budget {
        self.budget
    }

    pub fn units(&self) -> u32 {
        self.units
    }
}

pub const UNIT_LIMIT_MS: f64 = 8.0;

thread_local! {
    static NESTED: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    static OVER: std::cell::RefCell<Vec<(&'static str, f64)>> = const { std::cell::RefCell::new(Vec::new()) };
}

static TIMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LIMIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn time_units(limit_ms: f64) {
    LIMIT.store(limit_ms.to_bits(), std::sync::atomic::Ordering::Relaxed);
    TIMED.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn limit_ms() -> f64 {
    match LIMIT.load(std::sync::atomic::Ordering::Relaxed) {
        0 => UNIT_LIMIT_MS,
        bits => f64::from_bits(bits),
    }
}

pub fn units_timed() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    TIMED.load(std::sync::atomic::Ordering::Relaxed) || *ON.get_or_init(|| std::env::var("LAYOUT_UNITS").is_ok_and(|v| v == "1"))
}

pub fn unit<T>(class: &'static str, f: impl FnOnce() -> T) -> T {
    if !units_timed() {
        return f();
    }
    let outer = NESTED.replace(0.0);
    let t = Instant::now();
    let r = f();
    let total = t.elapsed().as_secs_f64() * 1e3;
    let own = total - NESTED.get();
    NESTED.set(outer + total);
    if own > limit_ms() {
        OVER.with(|o| o.borrow_mut().push((class, own)));
    }
    r
}

pub fn take_over_limit() -> Vec<(&'static str, f64)> {
    OVER.with(|o| std::mem::take(&mut *o.borrow_mut()))
}

#[derive(Debug, Default)]
pub struct JobMemo {}

pub struct Memos<'a> {
    pub cache: &'a mut LayoutCache,
    pub job: Option<&'a mut JobMemo>,
    pub meter: &'a mut Meter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_budget_yields_after_its_units() {
        let mut m = Meter::new(Budget::Units(3));
        m.unit_done();
        m.unit_done();
        assert_eq!(m.check(), Ok(()));
        m.unit_done();
        assert_eq!(m.check(), Err(Yield));
    }

    #[test]
    fn unlimited_never_yields() {
        let mut m = Meter::unlimited();
        for _ in 0..10_000 {
            m.unit_done();
        }
        assert_eq!(m.check(), Ok(()));
    }
}
