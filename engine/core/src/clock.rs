//! Two clocks, never mixed: monotonic for durations, wall (chrony-synchronised) for comparing with venue timestamps.
use serde::{Deserialize, Serialize};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// Nanoseconds since this process's clock was created (std exposes no absolute monotonic origin).
    pub mono_ns: u64,
    /// Nanoseconds since the Unix epoch.
    pub wall_ns: u64,
}

pub struct Clock { anchor: Instant }

impl Clock {
    pub fn new() -> Self { Clock { anchor: Instant::now() } }
    pub fn now(&self) -> Stamp {
        Stamp {
            mono_ns: self.anchor.elapsed().as_nanos() as u64,
            wall_ns: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0),
        }
    }
}

impl Default for Clock { fn default() -> Self { Self::new() } }

/// What reads the time: the live clock, or a simulated one (backtest) that a test loop moves itself.
pub trait Now { fn now(&self) -> Stamp; }
impl Now for Clock { fn now(&self) -> Stamp { Clock::now(self) } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_never_goes_back_and_wall_is_current() {
        let c = Clock::new();
        let mut last = c.now();
        for _ in 0..10_000 { let s = c.now(); assert!(s.mono_ns >= last.mono_ns); last = s; }
        let sys = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
        assert!(sys.abs_diff(last.wall_ns) < 1_000_000_000);
    }
}
