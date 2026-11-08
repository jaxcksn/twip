//! The caret's blink: a visible and a hidden phase of one interval each, restarted from visible by every input, run on
//! one engine timer. Time comes in from the caller, so the phases are testable without waiting.

use std::time::{Duration, Instant};

/// The engine timer the blink runs on (timer 1 is background layout).
pub(super) const TIMER: u32 = 2;

/// The shortest phase: a blink stays well under 3 flashes a second.
const SHORTEST: Duration = Duration::from_millis(100);

pub(super) struct Blink {
    /// A phase's length; `None` is a steady caret.
    interval: Option<Duration>,
    /// Blinking stops after this long without input, the caret left visible.
    stop_after: Option<Duration>,
    visible: bool,
    /// When the phase ends.
    due: Instant,
    last_input: Instant,
    /// A timer is armed and hasn't fired: the blink keeps one at a time.
    armed: bool,
}

impl Default for Blink {
    fn default() -> Blink {
        let now = Instant::now();
        Blink { interval: Some(Duration::from_millis(530)), stop_after: None, visible: true, due: now, last_input: now, armed: false }
    }
}

/// What the presentation does after a blink step: arm the timer for this long, and whether the frame changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Step {
    pub arm: Option<u32>,
    pub frame: bool,
}

impl Blink {
    /// New settings: the caret shows, and blinking starts over.
    pub fn set(&mut self, interval: Option<Duration>, stop_after: Option<Duration>, now: Instant) -> Step {
        self.interval = interval.map(|i| i.max(SHORTEST));
        self.stop_after = stop_after;
        self.input(now)
    }

    /// An input that edits or moves the selection, or the focus coming back: the caret shows, and its phase starts.
    pub fn input(&mut self, now: Instant) -> Step {
        let frame = !self.visible;
        self.visible = true;
        self.last_input = now;
        let Some(interval) = self.interval else { return Step { arm: None, frame } };
        self.due = now + interval;
        Step { arm: self.arm(interval), frame }
    }

    /// The blink timer went off. `active`: a caret is showing (focused, collapsed); without one the blink stops.
    pub fn fired(&mut self, active: bool, now: Instant) -> Step {
        self.armed = false;
        let Some(interval) = self.interval.filter(|_| active) else { return Step::default() };
        if self.stop_after.is_some_and(|s| now.duration_since(self.last_input) >= s) {
            let frame = !self.visible;
            self.visible = true;
            return Step { arm: None, frame };
        }
        if now + Duration::from_millis(2) < self.due {
            // An earlier arm, before the last input restarted the phase.
            return Step { arm: self.arm(self.due - now), frame: false };
        }
        self.visible = !self.visible;
        self.due = now + interval;
        Step { arm: self.arm(interval), frame: true }
    }

    /// Whether the caret is in its visible phase (always, when steady).
    pub fn visible(&self) -> bool {
        self.visible || self.interval.is_none()
    }

    fn arm(&mut self, after: Duration) -> Option<u32> {
        if self.armed {
            return None;
        }
        self.armed = true;
        Some(u32::try_from(after.as_millis()).unwrap_or(u32::MAX).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn phases_alternate_on_the_interval() {
        let t0 = Instant::now();
        let mut b = Blink::default();
        assert_eq!(b.input(t0), Step { arm: Some(530), frame: false });
        assert!(b.visible());
        assert_eq!(b.fired(true, t0 + 530 * MS), Step { arm: Some(530), frame: true });
        assert!(!b.visible());
        assert_eq!(b.fired(true, t0 + 1060 * MS), Step { arm: Some(530), frame: true });
        assert!(b.visible());
    }

    #[test]
    fn input_restarts_from_visible_and_keeps_one_timer() {
        let t0 = Instant::now();
        let mut b = Blink::default();
        b.input(t0);
        b.fired(true, t0 + 530 * MS);
        assert!(!b.visible());
        // Typing while the timer is armed: visible at once, no second timer.
        assert_eq!(b.input(t0 + 600 * MS), Step { arm: None, frame: true });
        assert!(b.visible());
        // The armed timer fires early for the new phase: it re-arms for the rest.
        assert_eq!(b.fired(true, t0 + 1060 * MS), Step { arm: Some(70), frame: false });
        assert!(b.visible());
        assert_eq!(b.fired(true, t0 + 1130 * MS), Step { arm: Some(530), frame: true });
    }

    #[test]
    fn no_caret_stops_the_blink() {
        let t0 = Instant::now();
        let mut b = Blink::default();
        b.input(t0);
        assert_eq!(b.fired(false, t0 + 530 * MS), Step::default());
        assert_eq!(b.input(t0 + 900 * MS), Step { arm: Some(530), frame: false }, "focus back: armed again");
    }

    #[test]
    fn steady_and_stop_after() {
        let t0 = Instant::now();
        let mut b = Blink::default();
        assert_eq!(b.set(None, None, t0), Step::default());
        assert!(b.visible());
        let mut b = Blink::default();
        b.set(Some(530 * MS), Some(1000 * MS), t0);
        b.fired(true, t0 + 530 * MS);
        assert!(!b.visible());
        assert_eq!(b.fired(true, t0 + 1060 * MS), Step { arm: None, frame: true }, "past stop_after: visible, stopped");
        assert!(b.visible());
        assert_eq!(Blink::default().set(Some(10 * MS), None, t0).arm, Some(100), "phases are at least 100 ms");
    }
}
