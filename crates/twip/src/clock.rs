//! The engine's clocks: std's natively. `wasm32-unknown-unknown` has none, so there the host registers them
//! ([`set`]: the page's `performance.now()`, `Date.now()` and time zone, through the C ABI); before it does, time
//! stands still in UTC.

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use std::time::Instant;

/// Seconds since the Unix epoch, now.
pub fn unix_secs() -> i64 {
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    return std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    return (web::clocks().1() / 1000.0).floor() as i64;
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use web::{Instant, local_offset, set};

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
mod web {
    use std::sync::OnceLock;
    use std::time::Duration;

    /// The host's clocks, in milliseconds: monotonic, and since the Unix epoch.
    static CLOCKS: OnceLock<(fn() -> f64, fn() -> f64)> = OnceLock::new();
    /// The host's time zone: minutes east of UTC at a time (milliseconds since the Unix epoch).
    static ZONE: OnceLock<fn(f64) -> f64> = OnceLock::new();

    /// Registers the host's clocks (milliseconds: a monotonic one, and the time since the Unix epoch) and time zone
    /// (minutes east of UTC at a time in milliseconds). The first registration stays.
    pub fn set(monotonic: fn() -> f64, unix: fn() -> f64, zone: fn(f64) -> f64) {
        let _ = CLOCKS.set((monotonic, unix));
        let _ = ZONE.set(zone);
    }

    pub(super) fn clocks() -> (fn() -> f64, fn() -> f64) {
        *CLOCKS.get().unwrap_or(&(|| 0.0, || 0.0))
    }

    /// The host's time zone's offset from UTC at `t` (seconds since 1970, UTC), in seconds; 0 before it is registered.
    pub fn local_offset(t: i64) -> i64 {
        ZONE.get().map_or(0, |z| (z(t as f64 * 1000.0) * 60.0).round() as i64)
    }

    /// A point on the host's monotonic clock: what `std::time::Instant` is natively.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant(Duration);

    impl Instant {
        pub fn now() -> Instant {
            Instant(Duration::from_secs_f64((clocks().0() / 1000.0).max(0.0)))
        }

        pub fn elapsed(&self) -> Duration {
            Instant::now().0.saturating_sub(self.0)
        }

        pub fn duration_since(&self, earlier: Instant) -> Duration {
            self.0.saturating_sub(earlier.0)
        }

        pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
            self.duration_since(earlier)
        }

        pub fn checked_add(&self, d: Duration) -> Option<Instant> {
            self.0.checked_add(d).map(Instant)
        }
    }

    impl std::ops::Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, d: Duration) -> Instant {
            Instant(self.0 + d)
        }
    }

    impl std::ops::AddAssign<Duration> for Instant {
        fn add_assign(&mut self, d: Duration) {
            self.0 += d;
        }
    }

    impl std::ops::Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, earlier: Instant) -> Duration {
            self.0.saturating_sub(earlier.0)
        }
    }

    impl std::ops::Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, d: Duration) -> Instant {
            Instant(self.0.saturating_sub(d))
        }
    }
}
