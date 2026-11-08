//! Stage timers and the reuse report, for tests and diagnostic benchmark runs (`cargo bench -p twip --features
//! prof`): thread-local totals, a few ns per call. Without `cfg(test)` or the `prof` feature every call compiles to
//! nothing, so the shipped build pays neither the clock reads nor the counting.

#[cfg(any(test, feature = "prof"))]
use std::cell::{Cell, RefCell};

/// A kind of laid-out piece the reuse report counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    /// A paragraph's lines (cached per paragraph).
    Para,
    Row,
    Page,
}

/// What a layout did with a piece: took it where it was, placed a cached one at a new position, or computed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Reused,
    Moved,
    Laid,
}

#[cfg(any(test, feature = "prof"))]
thread_local! {
    static TOTALS: RefCell<Vec<(&'static str, f64, u64)>> = const { RefCell::new(Vec::new()) };
    static REUSE: Cell<[[u64; 3]; 3]> = const { Cell::new([[0; 3]; 3]) };
}

/// Runs `f`, adding its time to the phase `name`.
#[inline]
pub fn time<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    #[cfg(any(test, feature = "prof"))]
    {
        let t = std::time::Instant::now();
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
    #[cfg(not(any(test, feature = "prof")))]
    {
        let _ = name;
        f()
    }
}

/// The totals since the last call (name, ms, calls), cleared. Empty without `cfg(test)` or the `prof` feature.
pub fn take() -> Vec<(&'static str, f64, u64)> {
    #[cfg(any(test, feature = "prof"))]
    return TOTALS.with(|v| std::mem::take(&mut *v.borrow_mut()));
    #[cfg(not(any(test, feature = "prof")))]
    Vec::new()
}

/// Counts one piece under `outcome`.
#[inline]
pub fn count(piece: Piece, outcome: Outcome) {
    #[cfg(any(test, feature = "prof"))]
    REUSE.with(|c| {
        let mut v = c.get();
        v[piece as usize][outcome as usize] += 1;
        c.set(v);
    });
    #[cfg(not(any(test, feature = "prof")))]
    let _ = (piece, outcome);
}

/// The counts since the last call, `[piece][outcome]` (indexed by [`Piece`] and [`Outcome`] as `usize`), cleared.
pub fn take_reuse() -> [[u64; 3]; 3] {
    #[cfg(any(test, feature = "prof"))]
    return REUSE.with(|c| c.replace([[0; 3]; 3]));
    #[cfg(not(any(test, feature = "prof")))]
    [[0; 3]; 3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prof_counts_and_clears() {
        take_reuse();
        count(Piece::Para, Outcome::Laid);
        count(Piece::Para, Outcome::Laid);
        count(Piece::Page, Outcome::Reused);
        let r = take_reuse();
        assert_eq!((r[Piece::Para as usize][Outcome::Laid as usize], r[Piece::Page as usize][Outcome::Reused as usize]), (2, 1));
        assert_eq!(take_reuse(), [[0; 3]; 3], "taking clears");
        assert_eq!(time("prof-test", || 7), 7);
        assert!(take().iter().any(|t| t.0 == "prof-test" && t.2 == 1));
    }
}
