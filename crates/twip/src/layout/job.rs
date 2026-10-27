//! Budgets and interruption. Work is divided into atomic units. Completing one counts against
//! the budget; once it is used up, the next cache insertion (the completed unit stored) or page commit unwinds to the
//! job's `run` as `Err(Yield)`, so resumed work finds every completed unit in the cache.

use std::time::Instant;

use super::cache::LayoutCache;

/// How much work a job step may do now. A control input: any budget gives the same final layout.
#[derive(Clone, Copy, Debug)]
pub enum Budget {
    /// Stop at the first unit boundary after this instant.
    Until(Instant),
    /// Stop after this many units (deterministic, for tests).
    Units(u32),
    /// Stop after this many committed pages: only at page boundaries, so no page attempt is restarted (`through`).
    Pages(u32),
    Unlimited,
}

/// The budget was used up at a unit boundary: the caller's partial work is in its memo; unwind to `run`.
#[derive(Debug, PartialEq, Eq)]
pub struct Yield;

/// Whether a job step finished or yielded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Done,
    Yielded,
}

/// Counts completed atomic units against a budget.
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

    /// A meter that never yields (full layouts).
    pub fn unlimited() -> Meter {
        Meter::new(Budget::Unlimited)
    }

    /// Records a completed unit. Units recomputed to verify a cache hit aren't new work and don't count.
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

    /// `Err(Yield)` once the budget is used up: called where completed work is safe (stored or committed).
    pub fn check(&self) -> Result<(), Yield> {
        if self.exhausted { Err(Yield) } else { Ok(()) }
    }

    /// Records a committed page (or a composed header and footer); `Err(Yield)` when the budget is used up. A page
    /// boundary is always safe, and work that completes no unit (reused pages, composition) still takes time, so a
    /// deadline is checked here too.
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

/// The longest an atomic unit should take: longer ones are reported with `LAYOUT_UNITS=1`.
pub const UNIT_LIMIT_MS: f64 = 8.0;

thread_local! {
    /// Exclusive time of the units nested in the one running, and the units over the limit.
    static NESTED: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    static OVER: std::cell::RefCell<Vec<(&'static str, f64)>> = const { std::cell::RefCell::new(Vec::new()) };
}

static TIMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LIMIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Times units from now on (the `units` command), as `LAYOUT_UNITS=1` does, reporting those over `limit_ms`.
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

/// Whether units are timed: `LAYOUT_UNITS=1` (read once) or [`time_units`].
pub fn units_timed() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    TIMED.load(std::sync::atomic::Ordering::Relaxed) || *ON.get_or_init(|| std::env::var("LAYOUT_UNITS").is_ok_and(|v| v == "1"))
}

/// Runs one atomic unit of class `class` (a shaping call, a line breaking, a page attempt, a composition), recording it
/// when its own time, without the units nested in it, is over [`UNIT_LIMIT_MS`].
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

/// The units over the limit since the last call: (class, own milliseconds).
pub fn take_over_limit() -> Vec<(&'static str, f64)> {
    OVER.with(|o| std::mem::take(&mut *o.borrow_mut()))
}

/// Partial work of one job, kept across yields by measurement key. Empty until jobs keep partial work.
#[derive(Debug, Default)]
pub struct JobMemo {}

/// The mutable state layout threads through its algorithms: the cache, the running job's memo, and the meter.
/// Nested algorithms are nested calls, so `&mut` passes down without interior mutability.
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
