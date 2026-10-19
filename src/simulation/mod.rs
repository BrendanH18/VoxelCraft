//! Fixed game time and device-independent gameplay steps shared by clients
//! and headless callers. Input and presentation stay with the caller.

pub mod difficulty;
pub mod effects;
pub mod experience;
pub mod lighting;
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
/// Wall-clock allowance for resolving queued light in each of a tick's two
/// light passes (a tick is 50 ms). A backlog from streaming or a large edit
/// finishes over several ticks; the next call resumes it.
pub const LIGHT_TICK_BUDGET: std::time::Duration = std::time::Duration::from_millis(2);
/// Java's day is 24000 ticks, 20 minutes at 20 ticks a second. The saved
/// `time` is the day fraction, so changing this needs no save migration.
pub const DAY_LENGTH: f64 = 1200.0;

#[derive(Default)]
pub struct FixedClock {
    remainder: Duration,
    ticks: u64,
    discarded: Duration,
    paused: Duration,
}

impl FixedClock {
    /// Number of 50 ms steps to run. Offline pause clears fractional time,
    /// so resuming never catches up time spent in a menu or out of focus.
    /// That wall time is counted separately in [`FixedClock::paused`].
    pub fn advance(&mut self, elapsed: Duration, paused: bool) -> u32 {
        if paused {
            self.paused += elapsed + self.remainder;
            self.remainder = Duration::ZERO;
            return 0;
        }
        let accepted = elapsed.min(TICK_DURATION * MAX_CATCH_UP_TICKS);
        self.discarded += elapsed - accepted;
        self.remainder += accepted;
        let ticks = (self.remainder.as_nanos() / TICK_DURATION.as_nanos()) as u32;
        self.remainder -= TICK_DURATION * ticks;
        self.ticks = self.ticks.saturating_add(ticks as u64);
        ticks
    }

    /// Number of steps returned by `advance` since creation; paused time does not count.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Wall time lost to the existing catch-up budget; performance reports must expose it.
    pub fn discarded(&self) -> Duration {
        self.discarded
    }

    /// Wall time (including dropped fractional time) that produced no steps
    /// because the game was paused; performance reports must expose it.
    pub fn paused(&self) -> Duration {
        self.paused
    }

    /// Render between the last two completed simulation snapshots.
    pub fn alpha(&self) -> f64 {
        self.remainder.as_secs_f64() / TICK_SECONDS
    }
}

/// Advances the Overworld day clock by `dt` seconds. The `advance_time` rule
/// stops it (Java's `ServerLevel.tick` skips `tickTime`); the day count and
/// with it the moon phase advance when the day wraps.
pub fn advance_day(day_time: f64, day_count: i64, dt: f64, rules: &crate::rules::GameRules) -> (f64, i64) {
    if !rules.bool("advance_time") {
        return (day_time, day_count);
    }
    let day_time = day_time + dt / DAY_LENGTH;
    if day_time >= 1.0 { (day_time.fract(), day_count.saturating_add(1)) } else { (day_time, day_count) }
}

/// Run the world systems once, in the same order for graphical and headless
/// sessions. Streaming is independent and may be polled between ticks.
pub fn tick_world(world: &mut World, player: DVec3) {
    tick_world_rules(world, player, true, 3);
}

/// [`tick_world`] with the gamerules that govern world block ticks.
pub fn tick_world_rules(world: &mut World, player: DVec3, fire_tick: bool, random_tick_speed: u32) {
    world.simulation_player(player);
    world.tick_redstone();
    world.tick_fluids(TICK_SECONDS);
    world.tick_falling(TICK_SECONDS);
    world.tick_furnaces(TICK_SECONDS);
    world.tick_brewing(TICK_SECONDS);
    if fire_tick {
        world.tick_fire(TICK_SECONDS, player);
    }
    world.update_block_light_within(LIGHT_TICK_BUDGET);
    world.tick_random_rules(TICK_SECONDS, player, random_tick_speed, fire_tick);
    world.tick_leaf_decay(TICK_SECONDS);
    world.update_block_light_within(LIGHT_TICK_BUDGET);
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
    armor: &[Option<crate::inventory::Stack>; 4],
    input: MoveInput,
    creative: bool,
) -> PlayerStep {
    let before = player.pos;
    player.apply_effects(&vitals.effects);
    player.wear_boots(crate::enchant::armor_level(armor, crate::enchant::Enchantment::DepthStrider));
    player.update(TICK_SECONDS, input, world);
    let moved = (player.pos - before).with_y(0.0).length();
    let env = Env {
        respiration: crate::enchant::armor_level(armor, crate::enchant::Enchantment::Respiration),
        turtle_helmet: armor[0].is_some_and(|s| s.item == crate::item::Item::TURTLE_HELMET),
        frost_walker: crate::enchant::armor_level(armor, crate::enchant::Enchantment::FrostWalker) > 0,
        ..player_environment(player, world, input, moved)
    };
    PlayerStep { moved, hurts: vitals.tick(TICK_SECONDS as f32, &env, creative) }
}

/// Collect survival inputs after movement; `moved` is horizontal distance for this step.
pub fn player_environment(player: &Player, world: &World, input: MoveInput, moved: f64) -> Env {
    Env {
        y: player.pos.y,
        on_ground: player.on_ground,
        flying: player.flying,
        in_water: player.in_water,
        climbing: player.climbing || player.in_web,
        head_in_water: player.head_in_water(world),
        in_lava: player.in_lava(world),
        in_fire: player.in_fire(world),
        in_soul_fire: player.in_soul_fire(world),
        on_magma: player.on_ground
            && !player.sneaking
            && world.get_block((player.pos - DVec3::Y * 0.01).floor().as_ivec3())
                == Some(crate::world::block::Block::MAGMA),
        on_hay: player.on_ground
            && world.get_block((player.pos - DVec3::Y * 0.01).floor().as_ivec3()).map(|b| b.base())
                == Some(crate::world::block::Block::HAY_BALE),
        frost_walker: false,
        wet: world.rains_on(player.eye().floor().as_ivec3()),
        moved: if player.flying { 0.0 } else { moved },
        sprinting: input.sprint && moved > 0.0,
        jumped: player.jumped,
        respiration: 0,
        turtle_helmet: false,
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
    fn every_hay_axis_cushions_only_grounded_players() {
        use crate::world::{block::Block, chunk::ChunkData, terrain::Generator};
        use glam::IVec3;
        let mut world = World::new_remote(std::sync::Arc::new(Generator::new(7)), 2);
        world.receive_chunk(IVec3::new(0, 4, 0), std::sync::Arc::new(ChunkData::Uniform(Block::AIR)));
        let cell = IVec3::new(4, 145, 4);
        let mut player = Player::new(cell.as_dvec3() + DVec3::new(0.5, 1.0, 0.5));
        for id in 901..=903 {
            world.set_block(cell, Block(id));
            player.on_ground = true;
            assert!(player_environment(&player, &world, MoveInput::default(), 0.0).on_hay);
            player.on_ground = false;
            assert!(!player_environment(&player, &world, MoveInput::default(), 0.0).on_hay);
        }
        world.set_block(cell, Block::STONE);
        player.on_ground = true;
        assert!(!player_environment(&player, &world, MoveInput::default(), 0.0).on_hay);
    }

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
    fn paused_and_discarded_time_are_accounted_separately() {
        let mut clock = FixedClock::default();
        assert_eq!(clock.advance(Duration::from_millis(30), false), 0);
        // The fractional 30 ms is dropped by the pause and counted with it.
        assert_eq!(clock.advance(Duration::from_secs(2), true), 0);
        assert_eq!(clock.paused(), Duration::from_millis(2030));
        assert_eq!(clock.discarded(), Duration::ZERO);
        assert_eq!(clock.advance(Duration::from_millis(1000), false), MAX_CATCH_UP_TICKS);
        assert_eq!(clock.discarded(), Duration::from_millis(750));
        assert_eq!(clock.paused(), Duration::from_millis(2030));
        // Every non-paused, non-discarded millisecond became a step.
        let mut clock = FixedClock::default();
        for _ in 0..600 {
            clock.advance(Duration::from_millis(5), false);
        }
        clock.advance(Duration::from_millis(5), true);
        assert_eq!((clock.ticks(), clock.paused(), clock.discarded()), (60, Duration::from_millis(5), Duration::ZERO));
    }

    #[test]
    fn advance_time_stops_the_clock_and_the_day_count() {
        use crate::rules::GameRules;
        let mut rules = GameRules::default();
        let (t, d) = advance_day(0.5, 7, DAY_LENGTH / 4.0, &rules);
        assert_eq!((t, d), (0.75, 7));
        let (t, d) = advance_day(0.9, 7, DAY_LENGTH / 5.0, &rules);
        assert!((t - 0.1).abs() < 1e-9 && d == 8, "the day wraps and the moon phase moves on: {t} {d}");
        rules.set("advance_time", "false").unwrap();
        assert_eq!(advance_day(0.9, 7, DAY_LENGTH, &rules), (0.9, 7));
    }

    #[test]
    fn camera_interpolates_motion_and_snaps_teleports() {
        assert_eq!(interpolated_eye(DVec3::ZERO, DVec3::X, 0.5), DVec3::X * 0.5);
        assert_eq!(interpolated_eye(DVec3::ZERO, DVec3::X * 100.0, 0.5), DVec3::X * 100.0);
    }
}
