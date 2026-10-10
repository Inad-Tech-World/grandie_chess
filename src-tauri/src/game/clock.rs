//! Chess clock with increment. Time is passed in explicitly so the clock can
//! be tested without sleeping.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::position::Side;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TimeControl {
    pub initial_ms: u32,
    pub increment_ms: u32,
}

impl TimeControl {
    /// PGN `TimeControl` tag, e.g. "300+3".
    pub fn pgn_tag(&self) -> String {
        format!("{}+{}", self.initial_ms / 1000, self.increment_ms / 1000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClockState {
    pub white_ms: f64,
    pub black_ms: f64,
    pub running: Option<Side>,
}

#[derive(Debug, Clone)]
pub struct Clock {
    remaining: [i64; 2],
    increment: i64,
    running: Option<(Side, Instant)>,
}

fn idx(side: Side) -> usize {
    match side {
        Side::White => 0,
        Side::Black => 1,
    }
}

impl Clock {
    pub fn new(tc: TimeControl) -> Self {
        let initial = tc.initial_ms as i64;
        Clock { remaining: [initial, initial], increment: tc.increment_ms as i64, running: None }
    }

    pub fn running(&self) -> Option<Side> {
        self.running.map(|(s, _)| s)
    }

    pub fn start(&mut self, side: Side, now: Instant) {
        self.stop(now);
        self.running = Some((side, now));
    }

    /// Stop the running side, charging it the elapsed time.
    pub fn stop(&mut self, now: Instant) {
        if let Some((side, since)) = self.running.take() {
            let elapsed = now.saturating_duration_since(since).as_millis() as i64;
            self.remaining[idx(side)] -= elapsed;
        }
    }

    /// The side to move completed a move: charge it, add its increment and
    /// start the opponent's clock. Returns the milliseconds the move took.
    pub fn press(&mut self, mover: Side, now: Instant) -> i64 {
        let spent = match self.running {
            Some((side, since)) if side == mover => now.saturating_duration_since(since).as_millis() as i64,
            _ => 0,
        };
        self.stop(now);
        self.remaining[idx(mover)] += self.increment;
        self.running = Some((mover.other(), now));
        spent
    }

    pub fn remaining(&self, side: Side, now: Instant) -> i64 {
        let mut ms = self.remaining[idx(side)];
        if let Some((running, since)) = self.running {
            if running == side {
                ms -= now.saturating_duration_since(since).as_millis() as i64;
            }
        }
        ms
    }

    pub fn flagged(&self, now: Instant) -> Option<Side> {
        let side = self.running()?;
        (self.remaining(side, now) <= 0).then_some(side)
    }

    pub fn state(&self, now: Instant) -> ClockState {
        ClockState {
            white_ms: self.remaining(Side::White, now).max(0) as f64,
            black_ms: self.remaining(Side::Black, now).max(0) as f64,
            running: self.running(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tc(initial_s: u32, inc_s: u32) -> TimeControl {
        TimeControl { initial_ms: initial_s * 1000, increment_ms: inc_s * 1000 }
    }

    #[test]
    fn charges_elapsed_time_and_adds_increment() {
        let t0 = Instant::now();
        let mut c = Clock::new(tc(180, 2));
        c.start(Side::White, t0);
        let t1 = t0 + Duration::from_millis(5_000);
        assert_eq!(c.remaining(Side::White, t1), 175_000);
        let spent = c.press(Side::White, t1);
        assert_eq!(spent, 5_000);
        assert_eq!(c.remaining(Side::White, t1), 177_000);
        assert_eq!(c.running(), Some(Side::Black));
        let t2 = t1 + Duration::from_millis(1_500);
        c.press(Side::Black, t2);
        assert_eq!(c.remaining(Side::Black, t2), 180_500);
        let s = c.state(t2);
        assert_eq!(s.running, Some(Side::White));
        assert_eq!(s.white_ms, 177_000.0);
    }

    #[test]
    fn flags_when_time_runs_out() {
        let t0 = Instant::now();
        let mut c = Clock::new(tc(1, 0));
        c.start(Side::White, t0);
        assert_eq!(c.flagged(t0 + Duration::from_millis(999)), None);
        assert_eq!(c.flagged(t0 + Duration::from_millis(1_000)), Some(Side::White));
        assert_eq!(c.state(t0 + Duration::from_secs(5)).white_ms, 0.0);
    }

    #[test]
    fn stop_freezes_both_clocks() {
        let t0 = Instant::now();
        let mut c = Clock::new(tc(60, 0));
        c.start(Side::Black, t0);
        c.stop(t0 + Duration::from_secs(10));
        let later = t0 + Duration::from_secs(100);
        assert_eq!(c.remaining(Side::Black, later), 50_000);
        assert_eq!(c.flagged(later), None);
        assert_eq!(tc(300, 3).pgn_tag(), "300+3");
    }
}
