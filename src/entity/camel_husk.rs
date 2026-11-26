//! Java 26.3 camel husks and the mob-ridden mounts that come with natural
//! spawns (v0.15 Track C): `CamelHusk`, `Husk.finalizeSpawn` (one natural
//! husk in ten with room for a camel husk rides one, holding an iron
//! spear, with a parched behind it) and `Drowned.finalizeSpawn` (half the
//! natural trident drowned outside rivers ride a zombie nautilus).
//!
//! A camel husk is a camel (`CamelHusk extends Camel`): two seats, sitting
//! and the dash, ridden without taming and steered with a saddle. It eats
//! rabbit's feet (healing 2) but never breeds and is never a baby.
use super::{Entities, Mob, MobKind, MobWorld, mounts::MOB_VEHICLE};
use crate::inventory::Stack;
use crate::physics;
use crate::world::terrain::Biome;
use glam::DVec3;

/// `Husk.finalizeSpawn`: `random.nextFloat() < 0.1F`.
pub const CAMEL_HUSK_CHANCE: f32 = 0.1;
/// `Drowned.ZOMBIE_NAUTILUS_JOCKEY_CHANCE`.
pub const ZOMBIE_NAUTILUS_JOCKEY_CHANCE: f32 = 0.5;
/// Zombie-family riders' `ridingOffset(-0.7F)` below a seat.
pub const ZOMBIE_RIDING_OFFSET: f64 = 0.7;

/// Mobs that carry mob riders: undead horses, camel husks and zombie nautiluses.
pub fn mob_mount(kind: MobKind) -> bool {
    super::undead_mounts::undead(kind)
        || matches!(kind, MobKind::CamelHusk | MobKind::ZombieNautilus | MobKind::Strider)
}

/// Where the `n`th of `riders` mob riders of `vehicle` sits (feet).
pub fn rider_seat(vehicle: &Mob, n: usize, riders: usize, rider_offset: f64) -> DVec3 {
    match vehicle.kind {
        MobKind::CamelHusk => {
            // As the camel's player seats: half a block fore and aft with two aboard.
            let sitting = vehicle.mount.as_ref().is_some_and(|s| s.sit_ticks > 0);
            let seat =
                vehicle.pos + DVec3::Y * (vehicle.shape().height * 0.75 - 0.35 - if sitting { 0.9 } else { 0.0 });
            let forward = DVec3::new(vehicle.yaw.cos() as f64, 0.0, vehicle.yaw.sin() as f64) * 0.5;
            match (riders, n) {
                (1, _) => seat,
                (_, 0) => seat + forward,
                _ => seat - forward,
            }
        }
        MobKind::ZombieNautilus => vehicle.pos + DVec3::Y * (super::nautilus::PASSENGER_HEIGHT - ZOMBIE_RIDING_OFFSET),
        // v1.0 Track 7: a strider seats at its full height (Java's default
        // passenger attachment, 1.7 for an adult); a zombified piglin's
        // `ridingOffset` of -0.7 sinks it in, a baby strider sits on top.
        MobKind::Strider => vehicle.pos + DVec3::Y * (vehicle.shape().height - rider_offset),
        _ => vehicle.pos + DVec3::Y * (vehicle.shape().height - 0.25),
    }
}

impl Entities {
    /// The riders `Husk.finalizeSpawn` and `Drowned.finalizeSpawn` add to
    /// the natural spawn just pushed as the last mob.
    pub(super) fn natural_jockeys<W: MobWorld + ?Sized>(&mut self, world: &W, biome: Biome) {
        let Some(last) = self.mobs.len().checked_sub(1) else { return };
        let (kind, pos) = (self.mobs[last].kind, self.mobs[last].pos);
        match kind {
            MobKind::Husk => {
                // `level.noCollision(CAMEL_HUSK.getSpawnAABB(…))` decides the try.
                let clear = !physics::overlaps_solid(world, pos, MobKind::CamelHusk.shape());
                if clear && self.rng.next_f32() < CAMEL_HUSK_CHANCE {
                    self.spawn_camel_husk_jockey(last);
                }
            }
            // v1.0 Track 7: `Strider.finalizeSpawn` for an adult: one time in
            // thirty (while monsters spawn) a zombified piglin with a warped
            // fungus on a stick rides it saddled; else one in ten a baby
            // strider rides it.
            MobKind::Strider if !self.mobs[last].baby => {
                let monsters =
                    self.monster_spawning && self.difficulty != crate::simulation::difficulty::Difficulty::Peaceful;
                if self.rng.next_int(30) == 0 && monsters {
                    self.spawn_strider_jockey(last, MobKind::ZombifiedPiglin);
                } else if self.rng.next_int(10) == 0 {
                    self.spawn_strider_jockey(last, MobKind::Strider);
                }
            }
            MobKind::Drowned => {
                let trident = self.mobs[last].equipment.main_hand().is_some_and(|s| s.item == crate::weapons::TRIDENT);
                if trident
                    && self.rng.next_f32() < ZOMBIE_NAUTILUS_JOCKEY_CHANCE
                    && !self.mobs[last].baby
                    && !biome.is_river()
                {
                    self.spawn_zombie_nautilus_jockey(last, biome == Biome::WarmOcean);
                }
            }
            _ => {}
        }
    }

    /// Husk `rider` (already spawned) gets an iron spear and mounts a new
    /// camel husk; a parched rides behind it.
    pub(super) fn spawn_camel_husk_jockey(&mut self, rider: usize) {
        let pos = self.mobs[rider].pos;
        let yaw = self.mobs[rider].yaw;
        self.mobs[rider]
            .equipment
            .set(super::equipment::Slot::MainHand, Some(Stack::new(crate::weapons::spear(crate::item::Tier::Iron), 1)));
        self.spawn(MobKind::CamelHusk, pos);
        let camel = self.mobs.len() - 1;
        self.mobs[camel].persistent = false;
        self.mobs[camel].yaw = yaw;
        self.ensure_mob_ids();
        let vehicle = MOB_VEHICLE | self.mobs[camel].uid;
        self.mobs[rider].riding = Some(vehicle);
        self.spawn(MobKind::Parched, pos);
        let parched = self.mobs.last_mut().unwrap();
        parched.persistent = false;
        parched.yaw = yaw;
        parched.riding = Some(vehicle);
        self.position_mob_riders();
    }

    /// `Strider.spawnJockey`: `rider` (a zombified piglin, or a baby
    /// strider) spawns on strider `mount` facing its way and rides it. The
    /// piglin holds a warped fungus on a stick and the strider gets a saddle
    /// that always drops.
    pub(super) fn spawn_strider_jockey(&mut self, mount: usize, rider: MobKind) {
        let (pos, yaw) = (self.mobs[mount].pos, self.mobs[mount].yaw);
        self.ensure_mob_ids();
        let vehicle = MOB_VEHICLE | self.mobs[mount].uid;
        if rider == MobKind::ZombifiedPiglin
            && let Some(s) = self.mobs[mount].mount.as_mut()
        {
            s.slots[0] = Some(Stack::new(crate::item::Item::SADDLE, 1));
        }
        self.spawn(rider, pos);
        let m = self.mobs.last_mut().unwrap();
        m.persistent = false;
        m.yaw = yaw;
        m.riding = Some(vehicle);
        if rider == MobKind::ZombifiedPiglin {
            // The stick replaces its golden sword (and adds no attack damage).
            let stick = Stack::new(crate::item::Item::WARPED_FUNGUS_ON_A_STICK, 1);
            m.equipment.set(super::equipment::Slot::MainHand, Some(stick));
            m.held = Some(super::nether::Weapon::None);
        } else {
            // `setAge(-24000)`.
            m.baby = true;
            m.age = -24000;
        }
        self.position_mob_riders();
    }

    /// Drowned `rider` mounts a new zombie nautilus (`EntitySpawnReason.JOCKEY`).
    pub(super) fn spawn_zombie_nautilus_jockey(&mut self, rider: usize, warm: bool) {
        let pos = self.mobs[rider].pos;
        self.spawn(MobKind::ZombieNautilus, pos);
        self.nautilus_on_spawn(warm);
        let mount = self.mobs.len() - 1;
        self.mobs[mount].persistent = false;
        self.mobs[mount].yaw = self.mobs[rider].yaw;
        self.ensure_mob_ids();
        self.mobs[rider].riding = Some(MOB_VEHICLE | self.mobs[mount].uid);
        self.position_mob_riders();
    }

    /// Mob riders follow their mounts' seats (horsemen, husks and parched
    /// on camel husks, drowned on zombie nautiluses); a rider whose mount
    /// is gone gets off.
    pub(super) fn position_mob_riders(&mut self) {
        let vehicles: rustc_hash::FxHashMap<u32, usize> = self
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.alive() && mob_mount(m.kind))
            .map(|(i, m)| (m.uid, i))
            .collect();
        let mut counts: rustc_hash::FxHashMap<u32, usize> = rustc_hash::FxHashMap::default();
        for m in &self.mobs {
            if let Some(v) = m.riding.filter(|v| v & MOB_VEHICLE != 0) {
                *counts.entry(v & !MOB_VEHICLE).or_default() += 1;
            }
        }
        let mut seated: rustc_hash::FxHashMap<u32, usize> = rustc_hash::FxHashMap::default();
        let seats: Vec<Option<(DVec3, DVec3)>> = self
            .mobs
            .iter()
            .map(|m| {
                let uid = m.riding.filter(|v| v & MOB_VEHICLE != 0)? & !MOB_VEHICLE;
                let &i = vehicles.get(&uid)?;
                let n = seated.entry(uid).or_default();
                let offset = -super::minecart::riding_offset(m.kind) * if m.baby { 0.5 } else { 1.0 };
                let seat = rider_seat(&self.mobs[i], *n, counts.get(&uid).copied().unwrap_or(1), offset);
                *n += 1;
                Some((seat, self.mobs[i].vel))
            })
            .collect();
        for (m, seat) in self.mobs.iter_mut().zip(seats) {
            if m.riding.is_none_or(|v| v & MOB_VEHICLE == 0) {
                continue;
            }
            m.previous_pos = m.pos;
            match seat {
                Some((p, v)) => {
                    m.pos = p;
                    m.vel = v;
                }
                None => m.riding = None,
            }
        }
    }
}

#[cfg(test)]
mod tests;
