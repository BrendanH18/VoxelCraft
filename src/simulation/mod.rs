//! Fixed game time and device-independent gameplay steps shared by clients
//! and headless callers. Input and presentation stay with the caller.

pub mod experience;
pub mod survival;
pub mod weather;

use std::time::Duration;

use glam::DVec3;

use crate::player::{MoveInput, Player};
use crate::world::World;
use survival::{Env, Hurts, Vitals};

pub const TICK_DURATION: Duration = Duration::from_millis(50);
pub const TICK_SECONDS: f64 = 0.05;
/// Bound catch-up after a stall so the desktop client remains responsive.
/// Time beyond this budget is discarded, never passed as a large physics step.
pub const MAX_CATCH_UP_TICKS: u32 = 5;
/// Preserve the existing ten-minute VoxelCraft day during extraction.
pub const DAY_LENGTH: f64 = 600.0;

#[derive(Default)]
pub struct FixedClock {
    remainder: Duration,
    ticks: u64,
}

impl FixedClock {
    /// Number of 50 ms steps to run. Offline pause clears fractional time,
    /// so resuming never catches up time spent in a menu or out of focus.
    pub fn advance(&mut self, elapsed: Duration, paused: bool) -> u32 {
        if paused {
            self.remainder = Duration::ZERO;
            return 0;
        }
        self.remainder += elapsed.min(TICK_DURATION * MAX_CATCH_UP_TICKS);
        let ticks = (self.remainder.as_nanos() / TICK_DURATION.as_nanos()) as u32;
        self.remainder -= TICK_DURATION * ticks;
        self.ticks = self.ticks.saturating_add(ticks as u64);
        ticks
    }

    /// Number of steps returned by `advance` since creation; paused time does not count.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Render between the last two completed simulation snapshots.
    pub fn alpha(&self) -> f64 {
        self.remainder.as_secs_f64() / TICK_SECONDS
    }
}

/// Run the world systems once, in the same order for graphical and headless
/// sessions. Streaming is independent and may be polled between ticks.
pub fn tick_world(world: &mut World, player: DVec3) {
    world.tick_fluids(TICK_SECONDS);
    world.tick_falling(TICK_SECONDS);
    world.tick_furnaces(TICK_SECONDS);
    world.tick_fire(TICK_SECONDS, player);
    world.update_block_light();
    world.tick_random(TICK_SECONDS, player);
    world.tick_leaf_decay(TICK_SECONDS);
    world.update_block_light();
}

pub struct PlayerStep {
    pub moved: f64,
    pub hurts: Hurts,
}

/// Movement uses the player's existing <= 1/120 s collision substeps.
/// Damage is returned to the caller's damage/armor/death entry point.
pub fn tick_player(
    player: &mut Player,
    world: &World,
    vitals: &mut Vitals,
    input: MoveInput,
    creative: bool,
) -> PlayerStep {
    let before = player.pos;
    player.update(TICK_SECONDS, input, world);
    let moved = (player.pos - before).with_y(0.0).length();
    let env = player_environment(player, world, input, moved);
    PlayerStep { moved, hurts: vitals.tick(TICK_SECONDS as f32, &env, creative) }
}

/// Collect survival inputs after movement; `moved` is horizontal distance for this step.
pub fn player_environment(player: &Player, world: &World, input: MoveInput, moved: f64) -> Env {
    Env {
        y: player.pos.y,
        on_ground: player.on_ground,
        flying: player.flying,
        in_water: player.in_water,
        climbing: player.climbing,
        head_in_water: player.head_in_water(world),
        in_lava: player.in_lava(world),
        in_fire: player.in_fire(world),
        wet: world.rains_on(player.eye().floor().as_ivec3()),
        moved: if player.flying { 0.0 } else { moved },
        sprinting: input.sprint && moved > 0.0,
        jumped: player.jumped,
    }
}

/// Teleports should snap instead of drawing a flight through unloaded space.
pub fn interpolated_eye(previous: DVec3, current: DVec3, alpha: f64) -> DVec3 {
    if previous.distance_squared(current) > 16.0 { current } else { previous.lerp(current, alpha.clamp(0.0, 1.0)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_independent_of_frame_rate() {
        for frame_ms in [1, 5, 10, 20, 25, 50, 100, 200] {
            let mut clock = FixedClock::default();
            let steps: u32 = (0..1000 / frame_ms).map(|_| clock.advance(Duration::from_millis(frame_ms), false)).sum();
            assert_eq!(steps, 20);
            assert_eq!(clock.ticks(), 20);
            assert_eq!(clock.alpha(), 0.0);
        }
    }

    #[test]
    fn fractional_time_pause_and_stalls() {
        let mut clock = FixedClock::default();
        assert_eq!(clock.advance(Duration::from_millis(30), false), 0);
        assert!((clock.alpha() - 0.6).abs() < 1e-9);
        assert_eq!(clock.advance(Duration::from_secs(120), true), 0);
        assert_eq!(clock.alpha(), 0.0);
        assert_eq!(clock.advance(Duration::from_millis(20), false), 0);
        assert_eq!(clock.advance(Duration::from_millis(30), false), 1);
        assert_eq!(clock.advance(Duration::from_secs(120), false), MAX_CATCH_UP_TICKS);
        assert_eq!(clock.advance(Duration::ZERO, false), 0);
        assert_eq!(clock.ticks(), 6);
    }

    #[test]
    fn camera_interpolates_motion_and_snaps_teleports() {
        assert_eq!(interpolated_eye(DVec3::ZERO, DVec3::X, 0.5), DVec3::X * 0.5);
        assert_eq!(interpolated_eye(DVec3::ZERO, DVec3::X * 100.0, 0.5), DVec3::X * 100.0);
    }
}
