//! Max Framerate: Java 26.3's `FramerateLimiter`, which waits after each
//! presented frame until the next one is due. It sleeps for most of the gap
//! and spins for the rest, learning how far the system's sleeps overshoot.

use std::time::{Duration, Instant};

/// Sleeps this much short of the target, then spins (Java's safety buffer).
const SPIN: Duration = Duration::from_micros(500);
/// Overshoots longer than this are treated as hiccups, not the sleep's bias.
const MAX_OVERSHOOT: Duration = Duration::from_millis(25);
const MAX_AVERAGE_OVERSHOOT: Duration = Duration::from_millis(2);

#[derive(Debug)]
pub(crate) struct FrameLimit {
    last_frame: Instant,
    overshoot: Duration,
    limit: u32,
}

impl Default for FrameLimit {
    fn default() -> Self {
        Self { last_frame: Instant::now(), overshoot: Duration::ZERO, limit: 0 }
    }
}

impl FrameLimit {
    /// After a frame: waits until `1 / fps` after the last one, or returns at
    /// once for `None` (Unlimited).
    pub fn wait(&mut self, fps: Option<u32>) {
        let Some(fps) = fps.filter(|&f| f > 0) else {
            self.last_frame = Instant::now();
            return;
        };
        if fps != self.limit {
            self.overshoot = Duration::ZERO;
            self.limit = fps;
        }
        let target = self.last_frame + Duration::from_secs(1) / fps;
        loop {
            let now = Instant::now();
            let Some(remaining) = target.checked_duration_since(now).filter(|r| !r.is_zero()) else { break };
            if remaining > self.overshoot + SPIN {
                let expected = remaining - self.overshoot - SPIN;
                std::thread::sleep(expected);
                let over = now.elapsed().saturating_sub(expected);
                if !over.is_zero() && over < MAX_OVERSHOOT {
                    self.overshoot = (over.mul_f64(0.1) + self.overshoot.mul_f64(0.9)).min(MAX_AVERAGE_OVERSHOOT);
                }
            } else {
                std::hint::spin_loop();
            }
        }
        self.last_frame = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_spaced_to_the_limit_and_unlimited_never_waits() {
        let mut limit = FrameLimit::default();
        limit.wait(Some(100));
        let start = Instant::now();
        for _ in 0..5 {
            limit.wait(Some(100));
        }
        let took = start.elapsed();
        assert!(took >= Duration::from_millis(49), "five frames at 100 fps take 50 ms, took {took:?}");
        // Loose: shared CI runners oversleep (356 ms was seen on macOS).
        assert!(took < Duration::from_secs(2), "{took:?}");
        let start = Instant::now();
        for _ in 0..100 {
            limit.wait(None);
        }
        assert!(start.elapsed() < Duration::from_millis(20));
    }
}
