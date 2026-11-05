//! Java 26.3 undead equines and skeleton traps, using the shared mount controls.
use super::{Ctx, Entities, MobKind, MobWorld};
use glam::DVec3;
use serde_json::{Value, json};

pub fn undead(kind: MobKind) -> bool {
    matches!(kind, MobKind::SkeletonHorse | MobKind::ZombieHorse)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    /// -1 is an ordinary horse; >=0 is SkeletonTrapTime in game ticks.
    pub trap_age: i32,
    /// Seconds left of the trap horsemen's `setInvulnerableTime(60)`, during
    /// which they ignore ordinary damage.
    pub protection: f64,
    remainder: f64,
}
impl Default for State {
    fn default() -> Self {
        Self { trap_age: -1, protection: 0., remainder: 0. }
    }
}
impl State {
    pub(crate) fn save(self) -> Value {
        json!([self.trap_age, self.protection, self.remainder])
    }
    pub(crate) fn load(v: &Value) -> Self {
        Self {
            trap_age: v[0].as_i64().unwrap_or(-1).clamp(-1, 18001) as i32,
            protection: v[1].as_f64().filter(|x| x.is_finite()).unwrap_or(0.).clamp(0., 3.),
            remainder: v[2].as_f64().filter(|x| x.is_finite()).unwrap_or(0.).clamp(0., 0.999999),
        }
    }
}
impl Entities {
    pub fn spawn_skeleton_trap(&mut self, pos: DVec3) {
        self.spawn(MobKind::SkeletonHorse, pos);
        let m = self.mobs.last_mut().unwrap();
        m.persistent = false;
        m.undead_mount.trap_age = 0;
    }
    pub(super) fn spawn_zombie_horse_rider(&mut self) {
        let i = self.mobs.len() - 1;
        self.ensure_mob_ids();
        let uid = self.mobs[i].uid;
        let pos = self.mobs[i].pos;
        self.spawn(MobKind::Zombie, pos);
        let m = self.mobs.last_mut().unwrap();
        m.riding = Some(super::mounts::MOB_VEHICLE | uid);
        m.persistent = false;
    }
    pub(super) fn tick_undead_mounts<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) {
        let mut traps = Vec::new();
        for (i, m) in self.mobs.iter_mut().enumerate() {
            if !m.alive() || !world.entity_ticking(m.pos.floor().as_ivec3()) {
                continue;
            }
            m.undead_mount.protection = (m.undead_mount.protection - dt.max(0.)).max(0.);
            if m.kind != MobKind::SkeletonHorse || m.undead_mount.trap_age < 0 {
                continue;
            }
            m.undead_mount.remainder += dt.max(0.) * 20.;
            let ticks = (m.undead_mount.remainder + 1e-9) as i32;
            m.undead_mount.remainder = (m.undead_mount.remainder - f64::from(ticks)).max(0.);
            // SkeletonTrapGoal uses nearby alive players, including creative.
            if ctx.players.iter().any(|p| p.alive && !p.spectator && p.pos.distance_squared(m.pos) < 100.) {
                traps.push(i);
            } else if !m.persistent {
                m.undead_mount.trap_age += ticks;
                if m.undead_mount.trap_age > 18000 {
                    m.health = 0.;
                    m.dying = Some(super::mob::DEATH_TIME);
                }
            }
        }
        // Mob passengers hand their pursuit to the mount; the passenger's
        // own skeleton bow/zombie attack goal continues ticking in place.
        let ridden: rustc_hash::FxHashMap<_, _> = self
            .mobs
            .iter()
            .filter(|r| r.alive())
            .filter_map(|r| r.riding.filter(|v| v & super::mounts::MOB_VEHICLE != 0).map(|v| (v, r.kind)))
            .collect();
        for m in self.mobs.iter_mut().filter(|m| m.simulation_active && undead(m.kind)) {
            if let Some(&rider) = ridden.get(&(super::mounts::MOB_VEHICLE | m.uid)) {
                m.hunt = ctx
                    .nearest_target(m.pos)
                    .filter(|p| p.pos.distance_squared(m.pos) < 48. * 48.)
                    .filter(|p| {
                        p.pos.distance_squared(m.pos) > if rider == MobKind::Skeleton { 8. * 8. } else { 1.5 * 1.5 }
                    })
                    .map(|p| p.pos);
            }
        }
        for i in traps {
            let pos = self.mobs[i].pos;
            self.mobs[i].undead_mount.trap_age = -1;
            self.mobs[i].mount.as_mut().unwrap().tame = true;
            self.summon_lightning(pos, true);
            for n in 0..4 {
                let h = if n == 0 {
                    i
                } else {
                    self.spawn(MobKind::SkeletonHorse, pos);
                    let h = self.mobs.len() - 1;
                    let m = &mut self.mobs[h];
                    m.persistent = true;
                    m.mount.as_mut().unwrap().tame = true;
                    m.undead_mount.protection = 3.;
                    m.vel.x = (self.rng.next_f32() - self.rng.next_f32()) as f64 * 1.1485 * 20.;
                    m.vel.z = (self.rng.next_f32() - self.rng.next_f32()) as f64 * 1.1485 * 20.;
                    h
                };
                self.ensure_mob_ids();
                let uid = self.mobs[h].uid;
                self.spawn(MobKind::Skeleton, pos);
                let r = self.mobs.last_mut().unwrap();
                r.persistent = true;
                r.riding = Some(super::mounts::MOB_VEHICLE | uid);
                r.undead_mount.protection = 3.;
                r.armor[0] = Some(super::armor::ArmorKind::Iron);
            }
        }
    }
    /// Horsemen share the saved tagged vehicle id. Ordinary mounted player
    /// controls still use State::rider; the two occupancy paths never overlap.
    pub(super) fn position_undead_riders(&mut self) {
        let seats: rustc_hash::FxHashMap<_, _> = self
            .mobs
            .iter()
            .filter(|m| m.alive() && undead(m.kind))
            .map(|m| (m.uid, (m.pos + DVec3::Y * (m.shape().height - 0.25), m.vel)))
            .collect();
        for m in &mut self.mobs {
            let Some(vehicle) = m.riding.filter(|v| v & super::mounts::MOB_VEHICLE != 0) else { continue };
            m.previous_pos = m.pos;
            if let Some((p, v)) = seats.get(&(vehicle & !super::mounts::MOB_VEHICLE)) {
                m.pos = *p;
                m.vel = *v;
            } else {
                m.riding = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Mob, PlayerId, Target},
        physics::test_util::Grid,
        world::terrain::Dimension,
    };
    fn ctx(pos: DVec3) -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId::HOST, pos, true)],
            daylight: 0.,
            raining: true,
            spawning: false,
            dimension: Dimension::Overworld,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        }
    }
    #[test]
    fn undead_attributes_drops_names_and_mount_saves_match_reference() {
        let mut e = Entities::new(263);
        for (kind, hp) in [(MobKind::SkeletonHorse, 15.), (MobKind::ZombieHorse, 25.)] {
            e.spawn(kind, DVec3::new(1., 10., 1.));
            let m = e.mobs.last_mut().unwrap();
            let s = m.mount.as_mut().unwrap();
            assert_eq!(s.max_health, hp);
            assert!(!s.tame);
            assert_eq!(MobKind::from_name(kind.name()), Some(kind));
            assert_eq!(kind.creature(), crate::enchant::Creature::Undead);
            assert!(!kind.is_breedable());
            assert!(!kind.despawns_in_peaceful(), "26.3 EntityTypes does not mark undead horses notInPeaceful");
            s.tame = true;
            s.slots[0] = Some(crate::inventory::Stack::new(crate::item::Item::SADDLE, 1));
            s.rider = Some(PlayerId(7));
        }
        let mut restored = Entities::new(264);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(restored.mobs.len(), 2);
        for m in &restored.mobs {
            let s = m.mount.as_ref().unwrap();
            assert!(s.saddled() && s.tame);
            assert_eq!(s.rider, Some(PlayerId(7)));
        }
        assert_eq!(MobKind::SkeletonHorse.loot(), &[(crate::item::Item::BONE, 0, 2)]);
        assert_eq!(MobKind::ZombieHorse.loot(), &[(crate::item::Item::ROTTEN_FLESH, 2, 3)]);
        let mut rng = super::super::Rng::new(263);
        for _ in 0..1000 {
            let s = super::super::mounts::State::new(MobKind::ZombieHorse, &mut rng);
            assert!((0.5..0.7).contains(&s.jump));
            assert!((9. / f64::from(42.16_f32)..12. / f64::from(42.16_f32)).contains(&s.speed));
        }
    }
    #[test]
    fn trap_triggers_once_spawns_four_riders_and_saves_their_links() {
        let mut e = Entities::new(263);
        let p = DVec3::new(0., 10., 0.);
        e.spawn_skeleton_trap(p);
        e.tick_undead_mounts(0.05, &Grid::flat(10), &ctx(p + DVec3::X * 11.));
        assert_eq!(e.mobs.len(), 1);
        let saved = e.nether_mobs_to_string();
        let mut restored = Entities::new(264);
        restored.load_nether_mobs(&saved);
        e = restored;
        assert_eq!(e.mobs[0].undead_mount.trap_age, 1);
        e.tick_undead_mounts(0.05, &Grid::flat(10), &ctx(p));
        assert_eq!(e.count(MobKind::SkeletonHorse), 4);
        assert_eq!(e.count(MobKind::Skeleton), 4);
        assert_eq!(e.lightning.len(), 1);
        e.position_undead_riders();
        let mut restored = Entities::new(265);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(restored.count(MobKind::SkeletonHorse), 4);
        assert_eq!(restored.mobs.iter().filter(|m| m.riding.is_some()).count(), 4);
        restored.tick_undead_mounts(0.05, &Grid::flat(10), &ctx(p));
        assert_eq!(restored.mobs.len(), 8);
        assert_eq!(
            e.mobs.iter().filter(|m| m.kind == MobKind::SkeletonHorse && m.mount.as_ref().unwrap().tame).count(),
            4
        );
    }
    #[test]
    fn trap_expires_after_18000_ticks_and_natural_zombie_horse_has_a_rider() {
        let mut e = Entities::new(263);
        e.spawn_skeleton_trap(DVec3::ZERO);
        e.mobs[0].undead_mount.trap_age = 18000;
        e.tick_undead_mounts(0.05, &Grid::flat(10), &ctx(DVec3::X * 100.));
        assert!(!e.mobs[0].alive());
        e.spawn(MobKind::ZombieHorse, DVec3::ZERO);
        e.spawn_zombie_horse_rider();
        assert_eq!(e.mobs.last().unwrap().kind, MobKind::Zombie);
        assert!(e.mobs.last().unwrap().riding.unwrap() & super::super::mounts::MOB_VEHICLE != 0);
        let mut m = Mob::new(MobKind::SkeletonHorse, DVec3::ZERO, 0.);
        m.undead_mount.protection = 3.;
        assert!(!m.damage(100., Some(DVec3::new(4., 2., 0.)), &mut e.rng));
        assert_eq!((m.health, m.vel), (15., DVec3::ZERO), "no damage and no knockback");
    }
    #[test]
    fn ordinary_use_equips_saddle_and_mounts_only_a_tamed_skeleton_horse() {
        use crate::{
            inventory::{Inventory, Stack},
            item::Item,
        };
        let mut e = Entities::new(263);
        e.spawn(MobKind::SkeletonHorse, DVec3::new(5., 140., 5.));
        let eye = DVec3::new(5., 140.5, 2.);
        let mut inv = Inventory::default();
        assert!(!e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        e.mobs[0].mount.as_mut().unwrap().tame = true;
        inv.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert!(inv.slots[0].is_none());
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert!(e.mount_for_player(PlayerId::HOST).is_some());
        assert!(e.mobs[0].mount.as_ref().unwrap().saddled());
        assert!(super::super::mounts::food(MobKind::SkeletonHorse, Item::GOLDEN_CARROT).is_none());
        assert_eq!(
            super::super::mounts::food(
                MobKind::ZombieHorse,
                Item::from_block(crate::world::block::Block::RED_MUSHROOM)
            ),
            Some((3., 3, 0))
        );
    }
    #[test]
    fn zombie_horse_body_armor_protects_against_daylight() {
        for protected in [false, true] {
            let mut m = Mob::new(MobKind::ZombieHorse, DVec3::new(5., 140., 5.), 0.);
            if protected {
                m.mount.as_mut().unwrap().slots[1] =
                    Some(crate::inventory::Stack::new(crate::item::Item::IRON_HORSE_ARMOR, 1));
            }
            let mut c = ctx(DVec3::ZERO);
            c.daylight = 1.;
            c.raining = false;
            m.update(0.05, &Grid::flat(140), &c, &mut super::super::Rng::new(263), &mut Vec::new());
            assert_eq!(m.burning, !protected, "body protection: {protected}");
        }
    }
}
