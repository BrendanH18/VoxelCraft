//! Java 26.3 weather-driven entity changes. Timers use game ticks, never wall time.
use super::{Mob, MobKind, MobWorld};
use glam::DVec3;
use serde_json::{Value, json};

/// `Zombie.drowningTracker` / `ConversionTracker`: exposure reaches 600,
/// then conversion counts 300 down through zero. Leaving water cancels both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterConversion {
    pub exposure: i32,
    pub remaining: i32,
    remainder: f64,
}
impl Default for WaterConversion {
    fn default() -> Self {
        Self { exposure: 0, remaining: -1, remainder: 0.0 }
    }
}
impl WaterConversion {
    pub fn shaking(self) -> bool {
        self.remaining >= 0
    }
    pub(crate) fn save(self) -> Value {
        json!([self.exposure, self.remaining, self.remainder])
    }
    pub(crate) fn load(v: &Value) -> Self {
        Self {
            exposure: v[0].as_i64().unwrap_or(0).clamp(-1, 600) as i32,
            remaining: v[1].as_i64().unwrap_or(-1).clamp(-1, 300) as i32,
            remainder: v[2].as_f64().filter(|x| x.is_finite()).unwrap_or(0.).clamp(0., 0.999999),
        }
    }
    fn tick(&mut self, wet: bool) -> bool {
        if !wet {
            self.exposure = -1;
            self.remaining = -1;
        } else if self.shaking() {
            self.remaining -= 1;
            return self.remaining < 0;
        } else {
            self.exposure += 1;
            if self.exposure >= 600 {
                self.remaining = 300;
            }
        }
        false
    }
}
impl Mob {
    pub(super) fn tick_water_conversion<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W) {
        if !self.alive() || !matches!(self.kind, MobKind::Zombie | MobKind::Husk) {
            return;
        }
        // Zombie.BABY_DIMENSIONS overrides its eye height to 0.775f.
        let eye = self.pos + DVec3::Y * f64::from(if self.baby { 0.775_f32 } else { 1.74_f32 });
        let wet = world.block(eye.floor().as_ivec3()).is_some_and(|b| b.holds_water());
        self.water_conversion.remainder += dt.max(0.) * 20.;
        let ticks = (self.water_conversion.remainder + 1e-9) as u32;
        self.water_conversion.remainder = (self.water_conversion.remainder - f64::from(ticks)).max(0.);
        for _ in 0..ticks {
            if self.water_conversion.tick(wet) {
                // `ConversionParams.single(..., true, true)` keeps equipment,
                // name, age, persistence, effects, identity and passengers.
                self.kind = if self.kind == MobKind::Husk { MobKind::Zombie } else { MobKind::Drowned };
                self.health = self.kind.max_health();
                self.water_conversion = WaterConversion::default();
                self.stop_navigation();
                self.goals = Default::default();
                break;
            }
        }
    }
}

/// A bolt has an initial two-tick life, then 1–3 further flashes separated
/// by a random 0–9 tick pause (`LightningBolt.tick`). Not saved by Java.
#[derive(Clone, Debug)]
pub struct Bolt {
    pub pos: DVec3,
    pub seed: u64,
    pub id: u64,
    pub life: i32,
    flashes: u32,
    visual_only: bool,
    remainder: f64,
}
impl Bolt {
    pub fn visible(&self) -> bool {
        self.life >= 0
    }
    pub(crate) fn replica(pos: DVec3, seed: u64, life: i32) -> Self {
        Self { pos, seed, id: seed, life, flashes: 0, visual_only: true, remainder: 0. }
    }
}
impl super::Entities {
    /// Summon and weather share this entry point. Visual-only bolts are for traps.
    pub fn summon_lightning(&mut self, pos: DVec3, visual_only: bool) {
        if !pos.is_finite() {
            return;
        }
        let seed = u64::from(self.rng.next_int(u32::MAX));
        self.lightning.push(Bolt {
            pos,
            seed,
            id: seed,
            life: 2,
            flashes: 1 + self.rng.next_int(3),
            visual_only,
            remainder: 0.,
        });
    }
    /// Fixed-step weather/entity bridge, also callable by headless sessions.
    pub fn tick_lightning(&mut self, dt: f64, world: &mut crate::world::World, ctx: &super::Ctx, thundering: bool) {
        use super::{EntityEvent, MobSound};
        use glam::IVec3;
        self.lightning_remainder += dt.max(0.) * 20.;
        let weather_ticks = (self.lightning_remainder + 1e-9) as u32;
        self.lightning_remainder = (self.lightning_remainder - f64::from(weather_ticks)).max(0.);
        if world.raining && thundering && ctx.dimension.has_sky() {
            for _ in 0..weather_ticks {
                for c in world.weather_chunks() {
                    if self.rng.next_int(100_000) != 0 {
                        continue;
                    }
                    let x = c.x * 16 + self.rng.next_int(16) as i32;
                    let z = c.y * 16 + self.rng.next_int(16) as i32;
                    let Some(h) = world.weather_surface(x, z) else { continue };
                    let center = IVec3::new(x, h + 1, z);
                    let target = world.lightning_rod_target(center).unwrap_or_else(|| {
                        // Entity AABBs intersect the heightmap-to-sky box inflated by 3.
                        let mut candidates: Vec<_> = self
                            .mobs
                            .iter()
                            .filter(|m| m.alive())
                            .filter(|m| {
                                let s = m.shape();
                                m.pos.x + s.half_width > (x - 3) as f64
                                    && m.pos.x - s.half_width < (x + 4) as f64
                                    && m.pos.z + s.half_width > (z - 3) as f64
                                    && m.pos.z - s.half_width < (z + 4) as f64
                                    && m.pos.y + s.height > (h - 2) as f64
                                    && world.sky_exposed(m.pos.floor().as_ivec3())
                            })
                            .map(|m| m.pos.floor().as_ivec3())
                            .collect();
                        candidates.extend(
                            ctx.players
                                .iter()
                                .filter(|p| {
                                    let (lo, hi) = p.shape.aabb(p.pos);
                                    p.alive
                                        && !p.spectator
                                        && world.sky_exposed(p.pos.floor().as_ivec3())
                                        && hi.x > (x - 3) as f64
                                        && lo.x < (x + 4) as f64
                                        && hi.z > (z - 3) as f64
                                        && lo.z < (z + 4) as f64
                                        && hi.y > (h - 2) as f64
                                })
                                .map(|p| p.pos.floor().as_ivec3()),
                        );
                        if candidates.is_empty() {
                            center
                        } else {
                            candidates[self.rng.next_int(candidates.len() as u32) as usize]
                        }
                    });
                    if world.rains_on(target) {
                        let difficulty =
                            crate::simulation::difficulty::Difficulty::ALL[usize::from(world.rules.difficulty.min(3))];
                        let local = difficulty.effective(self.clock_time, 0, self.moon_brightness);
                        let trap = ctx.spawning
                            && self.rng.chance(local * 0.01)
                            && !world
                                .get_block(target - IVec3::Y)
                                .is_some_and(|b| crate::world::lightning::rod(b).is_some());
                        if trap {
                            self.spawn_skeleton_trap(target.as_dvec3());
                        }
                        self.summon_lightning(target.as_dvec3() + DVec3::new(0.5, 0., 0.5), trap);
                    }
                }
            }
        }
        let mut bolts = std::mem::take(&mut self.lightning);
        for bolt in &mut bolts {
            if !world.is_entity_ticking(bolt.pos.floor().as_ivec3()) {
                continue;
            }
            bolt.remainder += dt.max(0.) * 20.;
            while bolt.remainder + 1e-9 >= 1. {
                bolt.remainder = (bolt.remainder - 1.).max(0.);
                if bolt.life == 2 {
                    world.lightning_strike_blocks(bolt.pos);
                    // `LightningBolt.tick`.
                    world.emit_game_event(
                        bolt.pos,
                        crate::world::vibration::GameEvent::LightningStrike,
                        crate::world::vibration::EventSource::NONE,
                        None,
                    );
                    if !bolt.visual_only {
                        world.lightning_fire(bolt.pos.floor().as_ivec3(), 4);
                    }
                    self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Thunder, pos: bolt.pos });
                }
                bolt.life -= 1;
                if bolt.life < 0 {
                    if bolt.flashes == 0 {
                        break;
                    }
                    if bolt.life < -(self.rng.next_int(10) as i32) {
                        bolt.flashes -= 1;
                        bolt.life = 1;
                        bolt.seed = u64::from(self.rng.next_int(u32::MAX));
                        if !bolt.visual_only {
                            world.lightning_flash_fire(bolt.pos.floor().as_ivec3(), 0);
                        }
                    }
                }
                if bolt.life >= 0 && !bolt.visual_only {
                    let mut bowls = Vec::new();
                    for m in &mut self.mobs {
                        let (lo, hi) = m.shape().aabb(m.pos);
                        if !m.alive()
                            || !hi.cmpgt(bolt.pos - DVec3::splat(3.)).all()
                            || !lo.cmplt(bolt.pos + DVec3::new(3., 9., 3.)).all()
                        {
                            continue;
                        }
                        if m.kind == MobKind::Villager
                            && world.rules.difficulty != 0
                            && let Some(v) = &m.villager
                        {
                            if let Some(p) = v.home {
                                self.claimed_beds.remove(&p);
                            }
                            if let Some(p) = v.job {
                                self.claimed_jobs.remove(&p);
                            }
                        }
                        if m.thunder_transform(bolt.id, world.rules.difficulty) {
                            continue;
                        }
                        if m.kind == MobKind::Turtle {
                            if m.damage(f32::MAX, None, &mut self.rng) && !m.baby {
                                bowls.push(m.pos);
                                self.pending_sounds.push(EntityEvent::MobKilled {
                                    kind: m.kind,
                                    pos: m.pos,
                                    burning: false,
                                    player_kill: m.player_hit_left > 0.,
                                    looting: 0,
                                });
                            }
                        } else if m.hurt <= 0. {
                            m.ignite(8.);
                            if m.damage(5., None, &mut self.rng) {
                                self.pending_sounds.push(EntityEvent::MobKilled {
                                    kind: m.kind,
                                    pos: m.pos,
                                    burning: m.burning,
                                    player_kill: m.player_hit_left > 0.,
                                    looting: 0,
                                });
                            }
                        }
                    }
                    if self.mob_loot {
                        for p in bowls {
                            self.scatter(crate::inventory::Stack::new(crate::item::Item::BOWL, 1), p);
                        }
                    }
                    for p in &ctx.players {
                        let (lo, hi) = p.shape.aabb(p.pos);
                        if p.targetable
                            && p.alive
                            && hi.cmpgt(bolt.pos - DVec3::splat(3.)).all()
                            && lo.cmplt(bolt.pos + DVec3::new(3., 9., 3.)).all()
                        {
                            self.pending_sounds.push(EntityEvent::PlayerLightning { player: p.id });
                            self.pending_sounds.push(EntityEvent::Ignite { player: p.id, secs: 8. });
                        }
                    }
                }
            }
        }
        bolts.retain(|b| (b.life >= 0 || b.flashes > 0) && world.is_loaded(b.pos.floor().as_ivec3()));
        self.lightning = bolts;
    }
}
impl Mob {
    /// Species `thunderHit` overrides; true suppresses the base damage and fire.
    /// `bolt_id` stays constant through a bolt's repeated flashes.
    pub fn thunder_transform(&mut self, bolt_id: u64, difficulty: u8) -> bool {
        match self.kind {
            MobKind::CopperGolem => {
                self.copper_golem.as_mut().unwrap().lightning(bolt_id);
                false
            }
            // `MushroomCow.thunderHit` never calls super: no damage, and one
            // swap per bolt UUID however many ticks the bolt touches it.
            MobKind::Mooshroom => {
                if self.mooshroom.as_mut().is_some_and(|s| s.last_bolt.replace(bolt_id) != Some(bolt_id)) {
                    self.lightning_swap();
                }
                true
            }
            MobKind::Creeper => {
                self.lightning_charged = true;
                false
            }
            MobKind::Pig if difficulty != 0 => {
                self.kind = MobKind::ZombifiedPiglin;
                self.health = self.kind.max_health();
                self.persistent = true;
                self.held = Some(super::nether::Weapon::GoldenSword);
                self.mount = None;
                self.animal = None;
                self.stop_navigation();
                self.goals = Default::default();
                true
            }
            MobKind::Villager if difficulty != 0 => {
                self.kind = MobKind::Witch;
                self.health = self.kind.max_health();
                self.persistent = true;
                self.villager = None;
                self.baby = false;
                self.age = 0;
                self.stop_navigation();
                self.goals = Default::default();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod bolt_tests {
    use super::*;
    use crate::{
        entity::{Ctx, Entities},
        world::{
            World,
            block::Block,
            chunk::ChunkData,
            terrain::{Dimension, Generator},
        },
    };
    use glam::IVec3;
    use std::sync::Arc;
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
        for y in w.generator.dimension.chunk_rows() {
            w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        w.set_block(IVec3::new(8, 139, 8), Block::STONE);
        w
    }
    fn ctx() -> Ctx {
        Ctx {
            players: vec![],
            daylight: 0.,
            raining: false,
            spawning: false,
            dimension: Dimension::Overworld,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        }
    }
    #[test]
    fn summon_strikes_converts_charges_and_saves_the_results() {
        let mut w = world();
        let mut e = Entities::new(263);
        let p = DVec3::new(8., 140., 8.);
        for kind in [MobKind::Creeper, MobKind::Pig, MobKind::Villager, MobKind::Turtle, MobKind::Mooshroom] {
            e.spawn(kind, p);
        }
        e.mobs[2].name = crate::inventory::StackName::new("Storm witness").unwrap();
        let home = IVec3::new(1, 140, 1);
        let job = home + IVec3::X;
        e.mobs[2].villager.as_mut().unwrap().home = Some(home);
        e.mobs[2].villager.as_mut().unwrap().job = Some(job);
        e.claimed_beds.insert(home);
        e.claimed_jobs.insert(job);
        e.summon_lightning(p, false);
        e.tick_lightning(0.05, &mut w, &ctx(), false);
        assert!(e.mobs[0].lightning_charged);
        assert_eq!(e.mobs[0].health, 15.);
        assert_eq!(e.mobs[1].kind, MobKind::ZombifiedPiglin);
        assert_eq!(e.mobs[2].kind, MobKind::Witch);
        assert!(!e.claimed_beds.contains(&home) && !e.claimed_jobs.contains(&job));
        assert!(!e.mobs[3].alive());
        assert_eq!(e.items.iter().filter(|i| i.stack.item == crate::item::Item::BOWL).count(), 1);
        let mut restored = Entities::new(264);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        assert!(restored.mobs.iter().any(|m| m.kind == MobKind::Creeper && m.lightning_charged));
        assert!(restored.mobs.iter().any(|m| m.kind == MobKind::Witch && m.name.as_str() == Some("Storm witness")));
        assert!(matches!(
            crate::agent::command::Command::parse("summon lightning_bolt ~ ~ ~"),
            Ok(crate::agent::command::Command::SummonLightning(_))
        ));
    }
    #[test]
    fn mooshroom_swaps_once_per_bolt_without_damage() {
        let mut w = world();
        let mut e = Entities::new(263);
        let p = DVec3::new(8., 140., 8.);
        e.spawn(MobKind::Mooshroom, p);
        let brown = |e: &Entities| e.mobs[0].mooshroom.as_ref().unwrap().brown;
        assert!(!brown(&e));
        e.summon_lightning(p, false);
        // Every flash and tick of one bolt touches it; it swaps only once.
        for _ in 0..40 {
            e.tick_lightning(0.05, &mut w, &ctx(), false);
        }
        assert!(e.lightning.is_empty());
        assert!(brown(&e));
        assert_eq!(e.mobs[0].health, MobKind::Mooshroom.max_health());
        e.summon_lightning(p, false);
        e.tick_lightning(0.05, &mut w, &ctx(), false);
        assert!(!brown(&e), "a new bolt swaps it back");
    }
    #[test]
    fn visual_trap_bolt_does_no_damage_or_fire_and_eventually_expires() {
        let mut w = world();
        let mut e = Entities::new(263);
        let p = DVec3::new(8., 140., 8.);
        e.spawn(MobKind::Pig, p);
        e.summon_lightning(p, true);
        for _ in 0..100 {
            e.tick_lightning(0.05, &mut w, &ctx(), false);
        }
        assert!(e.lightning.is_empty());
        assert_eq!(e.mobs[0].kind, MobKind::Pig);
        assert_eq!(e.mobs[0].health, 10.);
        assert_eq!(w.get_block(p.as_ivec3()), Some(Block::AIR));
    }
    #[test]
    fn flashes_replicate_without_saving_bolts_or_losing_creeper_charge() {
        let mut e = Entities::new(263);
        let p = DVec3::new(8., 140., 8.);
        e.summon_lightning(p, false);
        e.spawn(MobKind::Creeper, p);
        e.mobs[0].lightning_charged = true;
        let look = e.mobs[0].look();
        let mut m = Mob::new(MobKind::Creeper, p, 0.);
        m.apply_look(&look);
        assert!(m.lightning_charged);
        let mut other = Entities::new(264);
        other.apply_replica_extras(&e.replica_extras(p, 160.));
        assert_eq!(other.lightning.len(), 1);
        assert_eq!(other.lightning[0].pos, p);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entities;
    use crate::world::block::Block;
    use glam::IVec3;
    struct Water(bool);
    impl crate::physics::BlockSource for Water {
        fn block(&self, _: IVec3) -> Option<Block> {
            Some(if self.0 { Block::WATER } else { Block::AIR })
        }
    }
    impl MobWorld for Water {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            None
        }
    }

    #[test]
    fn exposure_conversion_and_drying_match_263_tracker() {
        let mut m = Mob::new(MobKind::Husk, DVec3::ZERO, 0.);
        m.persistent = true;
        m.baby = true;
        for _ in 0..599 {
            m.tick_water_conversion(0.05, &Water(true));
        }
        assert!(!m.water_conversion.shaking());
        m.tick_water_conversion(0.05, &Water(true));
        assert_eq!(m.water_conversion.remaining, 300);
        for _ in 0..300 {
            m.tick_water_conversion(0.05, &Water(true));
        }
        assert_eq!(m.kind, MobKind::Husk);
        assert_eq!(m.water_conversion.remaining, 0);
        m.tick_water_conversion(0.05, &Water(false));
        assert_eq!(m.water_conversion.exposure, -1);
        assert!(!m.water_conversion.shaking());
        // Dry reset is -1: the next exposure takes 601 ticks.
        for _ in 0..601 {
            m.tick_water_conversion(0.05, &Water(true));
        }
        for _ in 0..301 {
            m.tick_water_conversion(0.05, &Water(true));
        }
        assert_eq!(m.kind, MobKind::Zombie);
        assert!(m.persistent && m.baby);
        for _ in 0..901 {
            m.tick_water_conversion(0.05, &Water(true));
        }
        assert_eq!(m.kind, MobKind::Drowned);
    }

    #[test]
    fn conversion_survives_save_reload_with_identity_and_equipment() {
        let mut e = Entities::new(263);
        e.spawn(MobKind::Zombie, DVec3::ZERO);
        let m = &mut e.mobs[0];
        m.persistent = true;
        m.baby = true;
        m.water_conversion = WaterConversion { exposure: 600, remaining: 42, remainder: 0.5 };
        m.armor[0] = Some(crate::entity::armor::ArmorKind::Iron);
        let saved = e.nether_mobs_to_string();
        let mut restored = Entities::new(264);
        restored.load_nether_mobs(&saved);
        let m = &mut restored.mobs[0];
        assert_eq!(m.water_conversion.remaining, 42);
        assert!(m.water_conversion.shaking());
        for _ in 0..43 {
            m.tick_water_conversion(0.05, &Water(true));
        }
        assert_eq!(m.kind, MobKind::Drowned);
        assert_eq!(m.armor[0], Some(crate::entity::armor::ArmorKind::Iron));
        assert!(m.persistent && m.baby);
    }
}
