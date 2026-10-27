//! Cheap phase timers for `twip bench` (thread-local totals; a few ns per call).

use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static TOTALS: RefCell<Vec<(&'static str, f64, u64)>> = const { RefCell::new(Vec::new()) };
}

/// Runs `f`, adding its time to the phase `name`.
pub fn time<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    let ms = t.elapsed().as_secs_f64() * 1e3;
    TOTALS.with(|v| {
        let mut v = v.borrow_mut();
        match v.iter_mut().find(|e| e.0 == name) {
            Some(e) => {
                e.1 += ms;
                e.2 += 1;
            }
            None => v.push((name, ms, 1)),
        }
    });
    r
}

/// The totals since the last call (name, ms, calls), cleared.
pub fn take() -> Vec<(&'static str, f64, u64)> {
    TOTALS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

/// Adds the time since `t` to the phase `name`.
pub fn since(name: &'static str, t: Instant) {
    time(name, || ());
    TOTALS.with(|v| {
        if let Some(e) = v.borrow_mut().iter_mut().find(|e| e.0 == name) {
            e.1 += t.elapsed().as_secs_f64() * 1e3;
        }
    });
}
