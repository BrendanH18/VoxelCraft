//! Survival health rules: fall damage, drowning, natural regeneration and
//! death. Pure logic with no world or rendering access, so it is unit
//! tested below; `Game` feeds it the player's surroundings every frame.

/// 10 hearts, in half-heart units.
pub const MAX_HEALTH: f32 = 20.0;
/// Seconds of air (Minecraft's 300 ticks).
pub const MAX_AIR: f32 = 15.0;
pub const AIR_BUBBLES: u32 = 10;
/// Seconds of air regained per second out of water.
const AIR_REFILL_RATE: f32 = 4.0;
const DROWN_DAMAGE: f32 = 2.0;
const DROWN_INTERVAL: f32 = 1.0;
/// Damage per hit while in lava; hurt immunity spaces hits 0.5 s apart.
const LAVA_DAMAGE: f32 = 4.0;
/// Seconds without damage before health starts regenerating.
const REGEN_DELAY: f32 = 4.0;
/// Seconds per half heart regenerated.
const REGEN_INTERVAL: f32 = 2.5;
/// After a hit, further damage only applies the amount exceeding it for
/// this long (Minecraft's 10-tick hurt immunity), so mobs hitting every
/// frame do not stack.
pub const INVULNERABLE: f32 = 0.5;
/// Blocks a player can fall without taking damage.
const SAFE_FALL: f64 = 3.0;

pub const CAUSE_FALL: &str = "fell from a high place";
pub const CAUSE_DROWN: &str = "drowned";
pub const CAUSE_LAVA: &str = "tried to swim in lava";

/// Damage for landing after falling `distance` blocks.
pub fn fall_damage(distance: f64) -> f32 {
    // The epsilon (above the collision skin) keeps exact block drops from
    // rounding down a point.
    (distance - SAFE_FALL + 1e-4).floor().max(0.0) as f32
}

/// What the player is touching this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Env {
    /// Feet height.
    pub y: f64,
    pub on_ground: bool,
    pub flying: bool,
    /// Body in water: breaks falls.
    pub in_water: bool,
    /// Eyes under water: uses up air.
    pub head_in_water: bool,
    /// Body touching lava: burns.
    pub in_lava: bool,
}

/// Damage produced by one [`Vitals::tick`]; the caller applies it through
/// its single damage entry point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hurts {
    pub fall: f32,
    pub drown: f32,
    pub lava: f32,
}

#[derive(Clone, Debug)]
pub struct Vitals {
    /// 0..=MAX_HEALTH, in half hearts.
    pub health: f32,
    /// Seconds of air left, 0..=MAX_AIR.
    pub air: f32,
    /// Seconds since the last damage (drives the hurt flash and regen).
    since_damage: f32,
    /// Counts down to the next regenerated half heart.
    regen_timer: f32,
    drown_timer: f32,
    /// Damage of the hit that started the current immunity window.
    last_damage: f32,
    /// Highest feet Y since last standing on the ground (None while
    /// flying or swimming).
    fall_peak: Option<f64>,
    /// Set while dead: what killed the player.
    pub death: Option<String>,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: MAX_HEALTH,
            air: MAX_AIR,
            since_damage: 1e3,
            regen_timer: 0.0,
            drown_timer: 0.0,
            last_damage: 0.0,
            fall_peak: None,
            death: None,
        }
    }
}

impl Vitals {
    /// Restores saved values; zero health means the player died.
    pub fn restore(health: f32, air: f32, death: Option<String>) -> Self {
        let mut v = Self {
            health: health.clamp(0.0, MAX_HEALTH),
            air: air.clamp(0.0, MAX_AIR),
            // Reloading must not skip the regeneration delay.
            regen_timer: REGEN_DELAY,
            ..Self::default()
        };
        if v.health <= 0.0 {
            v.death = Some(death.unwrap_or_else(|| "died".into()));
        }
        v
    }

    pub fn is_dead(&self) -> bool {
        self.death.is_some()
    }

    pub fn since_damage(&self) -> f32 {
        self.since_damage
    }

    /// Air bubbles to show, 0..=AIR_BUBBLES.
    pub fn bubbles(&self) -> u32 {
        ((self.air / MAX_AIR * AIR_BUBBLES as f32).ceil() as u32).min(AIR_BUBBLES)
    }

    /// Applies damage and returns how much was actually taken (0 in
    /// creative, while dead, or when absorbed by hurt immunity).
    pub fn damage(&mut self, amount: f32, cause: &str, creative: bool) -> f32 {
        if creative || self.is_dead() || amount <= 0.0 {
            return 0.0;
        }
        let taken = if self.since_damage < INVULNERABLE {
            // Only a stronger hit gets through, and only the difference.
            if amount <= self.last_damage {
                return 0.0;
            }
            let extra = amount - self.last_damage;
            self.last_damage = amount;
            extra
        } else {
            self.last_damage = amount;
            self.since_damage = 0.0;
            amount
        };
        self.health = (self.health - taken).max(0.0);
        self.regen_timer = REGEN_DELAY;
        if self.health <= 0.0 {
            self.death = Some(cause.to_string());
        }
        taken
    }

    /// Advances air, regeneration and fall tracking by `dt` seconds and
    /// returns fall/drowning/lava damage for the caller to apply.
    pub fn tick(&mut self, dt: f32, env: &Env, creative: bool) -> Hurts {
        let mut hurts = Hurts::default();
        self.since_damage = (self.since_damage + dt).min(1e3);
        if self.is_dead() {
            return hurts;
        }

        // Falling: remember the highest point since standing on the ground
        // (including the ground itself, so stepping off a ledge counts
        // the full drop).
        if env.flying || env.in_water {
            self.fall_peak = None;
        } else if env.on_ground {
            if let Some(peak) = self.fall_peak {
                hurts.fall = fall_damage(peak - env.y);
            }
            self.fall_peak = Some(env.y);
        } else {
            self.fall_peak = Some(self.fall_peak.map_or(env.y, |p| p.max(env.y)));
        }

        // Air and drowning.
        if env.head_in_water && !creative {
            self.air = (self.air - dt).max(0.0);
            if self.air <= 0.0 {
                self.drown_timer += dt;
                if self.drown_timer >= DROWN_INTERVAL {
                    self.drown_timer -= DROWN_INTERVAL;
                    hurts.drown = DROWN_DAMAGE;
                }
            }
        } else {
            self.air = (self.air + dt * AIR_REFILL_RATE).min(MAX_AIR);
            self.drown_timer = 0.0;
        }

        if env.in_lava && !creative {
            hurts.lava = LAVA_DAMAGE;
        }

        // Natural regeneration.
        self.regen_timer -= dt;
        if self.health >= MAX_HEALTH {
            self.regen_timer = self.regen_timer.max(0.0);
        } else if self.regen_timer <= 0.0 {
            self.health = (self.health + 1.0).min(MAX_HEALTH);
            self.regen_timer += REGEN_INTERVAL;
        }
        hurts
    }

    /// Full health and air, alive, nothing pending.
    pub fn respawn(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn ground(y: f64) -> Env {
        Env { y, on_ground: true, ..Env::default() }
    }

    fn air(y: f64) -> Env {
        Env { y, ..Env::default() }
    }

    /// Falls from `from` to `to` in `steps` frames and lands.
    fn fall(v: &mut Vitals, from: f64, to: f64, landing: Env) -> Hurts {
        v.tick(DT, &ground(from), false);
        for i in 0..=20 {
            let y = from + (to - from) * i as f64 / 20.0;
            v.tick(DT, &air(y), false);
        }
        v.tick(DT, &landing, false)
    }

    #[test]
    fn fall_damage_thresholds() {
        assert_eq!(fall_damage(0.0), 0.0);
        assert_eq!(fall_damage(3.0), 0.0);
        assert_eq!(fall_damage(3.9), 0.0);
        assert_eq!(fall_damage(74.00001 - 70.00001), 1.0);
        assert_eq!(fall_damage(4.0), 1.0);
        assert_eq!(fall_damage(10.5), 7.0);
        assert_eq!(fall_damage(-5.0), 0.0);
    }

    #[test]
    fn landing_after_fall_hurts() {
        let mut v = Vitals::default();
        // A jump peaking 1.25 blocks up does nothing.
        v.tick(DT, &ground(64.0), false);
        v.tick(DT, &air(65.25), false);
        assert_eq!(v.tick(DT, &ground(64.0), false).fall, 0.0);
        // Ten blocks: 7 damage.
        let hurts = fall(&mut v, 74.0, 64.0, ground(64.0));
        assert_eq!(hurts.fall, 7.0);
        // Stepping off a 4-block ledge counts from the ledge top.
        v.tick(DT, &ground(68.0), false);
        v.tick(DT, &air(67.99), false);
        assert_eq!(v.tick(DT, &ground(64.0), false).fall, 1.0);
        // Standing still never hurts.
        for _ in 0..10 {
            assert_eq!(v.tick(DT, &ground(64.0), false).fall, 0.0);
        }
    }

    #[test]
    fn water_and_flight_break_falls() {
        let mut v = Vitals::default();
        let splash = Env { y: 60.0, in_water: true, ..Env::default() };
        assert_eq!(fall(&mut v, 100.0, 60.0, splash).fall, 0.0);
        // Swimming to the bottom afterwards is not a fall either.
        assert_eq!(v.tick(DT, &ground(58.0), false).fall, 0.0);

        let mut v = Vitals::default();
        v.tick(DT, &Env { y: 100.0, flying: true, ..Env::default() }, false);
        // Flight stops 2 blocks up: only those 2 blocks count.
        v.tick(DT, &air(66.0), false);
        assert_eq!(v.tick(DT, &ground(64.0), false).fall, 0.0);
    }

    #[test]
    fn drowning_after_air_runs_out() {
        let mut v = Vitals::default();
        let under = Env { y: 50.0, in_water: true, head_in_water: true, ..Env::default() };
        let mut t = 0.0;
        let mut first_hurt = None;
        while t < 20.0 {
            let h = v.tick(DT, &under, false);
            t += DT;
            if h.drown > 0.0 && first_hurt.is_none() {
                first_hurt = Some(t);
                assert_eq!(h.drown, 2.0);
            }
            if t < MAX_AIR - 0.1 {
                assert!(v.air > 0.0, "air ran out early at {t}");
                assert!(v.bubbles() >= 1);
            }
        }
        let first = first_hurt.expect("never drowned");
        assert!((MAX_AIR + 0.9..MAX_AIR + 1.1).contains(&first), "first drown hit at {first}");
        assert_eq!(v.bubbles(), 0);

        // Surfacing refills air within a few seconds.
        for _ in 0..(4.0 / DT) as usize {
            v.tick(DT, &ground(50.0), false);
        }
        assert_eq!(v.air, MAX_AIR);
        assert_eq!(v.bubbles(), AIR_BUBBLES);
    }

    #[test]
    fn regen_after_delay() {
        let mut v = Vitals::default();
        assert_eq!(v.damage(6.0, "test", false), 6.0);
        assert_eq!(v.health, 14.0);
        let mut t = 0.0;
        while t < REGEN_DELAY - 0.1 {
            v.tick(DT, &ground(0.0), false);
            t += DT;
        }
        assert_eq!(v.health, 14.0, "healed before the delay");
        while t < REGEN_DELAY + 0.1 {
            v.tick(DT, &ground(0.0), false);
            t += DT;
        }
        assert_eq!(v.health, 15.0);
        while t < REGEN_DELAY + REGEN_INTERVAL * 2.0 + 0.1 {
            v.tick(DT, &ground(0.0), false);
            t += DT;
        }
        assert_eq!(v.health, 17.0);
        // Never exceeds the maximum.
        for _ in 0..(60.0 / DT) as usize {
            v.tick(DT, &ground(0.0), false);
        }
        assert_eq!(v.health, MAX_HEALTH);
    }

    #[test]
    fn lava_burns_every_half_second() {
        let mut v = Vitals::default();
        let lava = Env { y: 10.0, in_water: true, in_lava: true, ..Env::default() };
        let mut taken = 0.0;
        for _ in 0..60 {
            let h = v.tick(1.0 / 60.0, &lava, false);
            taken += v.damage(h.lava, CAUSE_LAVA, false);
        }
        // One second in lava: hits at 0 s and 0.5 s.
        assert_eq!(taken, 8.0);
        assert_eq!(v.tick(0.1, &lava, true).lava, 0.0, "creative is immune");
    }

    #[test]
    fn creative_is_immune() {
        let mut v = Vitals::default();
        assert_eq!(v.damage(100.0, "test", true), 0.0);
        assert_eq!(v.health, MAX_HEALTH);
        let under = Env { head_in_water: true, in_water: true, ..Env::default() };
        for _ in 0..(30.0 / DT) as usize {
            let h = v.tick(DT, &under, true);
            assert_eq!(h.drown, 0.0);
        }
        assert_eq!(v.air, MAX_AIR);
        assert!(!v.is_dead());
    }

    #[test]
    fn hurt_immunity_absorbs_repeated_hits() {
        let mut v = Vitals::default();
        assert_eq!(v.damage(3.0, "test", false), 3.0);
        assert_eq!(v.damage(3.0, "test", false), 0.0);
        assert_eq!(v.damage(5.0, "test", false), 2.0);
        assert_eq!(v.health, 15.0);
        for _ in 0..(INVULNERABLE / DT) as usize + 2 {
            v.tick(DT, &ground(0.0), false);
        }
        assert_eq!(v.damage(3.0, "test", false), 3.0);
    }

    /// Real player physics on generated terrain feeding the rules.
    #[test]
    fn falls_through_player_physics() {
        use crate::player::{MoveInput, Player};
        use crate::world::World;
        use crate::world::block::Block;
        use crate::world::terrain::Generator;
        use glam::{DVec3, IVec3};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let generator = Arc::new(Generator::new(7));
        let spawn = generator.find_spawn();
        let mut world = World::new(generator, Default::default(), 3);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !(world.loaded_chunks() > 0 && world.is_idle()) {
            world.update(spawn.as_dvec3());
            world.mesh_uploads.clear();
            assert!(Instant::now() < deadline, "world never settled");
            std::thread::sleep(Duration::from_millis(1));
        }
        let (x, z) = (spawn.x, spawn.z);
        let ground =
            (0..512).rev().find(|&y| world.get_block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid())).unwrap();
        let top = ground as f64 + 1.0;

        let drop = |world: &World, height: f64| {
            let mut player = Player::new(DVec3::new(x as f64 + 0.5, top + height, z as f64 + 0.5));
            let mut v = Vitals::default();
            v.tick(DT, &Env { y: player.pos.y, ..Env::default() }, false);
            let mut taken = 0.0;
            for _ in 0..240 {
                player.update(DT as f64, MoveInput::default(), world);
                let env = Env {
                    y: player.pos.y,
                    on_ground: player.on_ground,
                    flying: player.flying,
                    in_water: player.in_water,
                    head_in_water: player.head_in_water(world),
                    in_lava: player.in_lava(world),
                };
                let h = v.tick(DT, &env, false);
                taken += v.damage(h.fall, CAUSE_FALL, false);
            }
            assert!(player.on_ground, "player never landed");
            (taken, v.death)
        };
        assert_eq!(drop(&world, 3.0), (0.0, None));
        assert_eq!(drop(&world, 10.0), (7.0, None));
        assert_eq!(drop(&world, 30.0), (27.0, Some(CAUSE_FALL.to_string())));

        // Two blocks of water cushion the same fall.
        world.set_block(IVec3::new(x, ground + 1, z), Block::WATER);
        world.set_block(IVec3::new(x, ground + 2, z), Block::WATER);
        assert_eq!(drop(&world, 30.0), (0.0, None));
    }

    #[test]
    fn death_and_respawn() {
        let mut v = Vitals::default();
        v.damage(25.0, CAUSE_FALL, false);
        assert_eq!(v.health, 0.0);
        assert_eq!(v.death.as_deref(), Some(CAUSE_FALL));
        // Dead players take no more damage and do not regenerate.
        assert_eq!(v.damage(5.0, "test", false), 0.0);
        for _ in 0..(10.0 / DT) as usize {
            v.tick(DT, &ground(0.0), false);
        }
        assert_eq!(v.health, 0.0);
        v.respawn();
        assert!(!v.is_dead());
        assert_eq!((v.health, v.air), (MAX_HEALTH, MAX_AIR));

        let restored = Vitals::restore(0.0, 3.0, None);
        assert!(restored.is_dead());
        let restored = Vitals::restore(7.5, 99.0, None);
        assert_eq!((restored.health, restored.air), (7.5, MAX_AIR));
    }
}
