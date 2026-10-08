//! Zombie infection, natural variants and golden-apple conversion.
use super::villager::{self, Profession};
use super::{Entities, EntityEvent, Mob, MobKind, MobWorld, Rng};
use crate::simulation::difficulty::Difficulty;
use crate::world::block::Block;
use glam::IVec3;
impl Entities {
    /// A killing blow from a zombie converts the villager on Normal (50%) and Hard (100%).
    pub(super) fn infect_or_hit(&mut self, attacker: usize, victim: usize, events: &mut Vec<EntityEvent>) {
        let (damage, _) = self.mobs[attacker].melee_attack();
        let chance = match self.mobs[attacker].difficulty {
            Difficulty::Peaceful | Difficulty::Easy => 0.0,
            Difficulty::Normal => 0.5,
            Difficulty::Hard => 1.0,
        };
        let killing = self.mobs[victim].health <= damage && self.mobs[victim].kind == MobKind::Villager;
        if killing && self.rng.chance(chance) {
            let mob = &mut self.mobs[victim];
            mob.kind = MobKind::ZombieVillager;
            mob.health = MobKind::ZombieVillager.max_health();
            mob.dying = None;
            mob.baby = mob.age < 0;
            mob.age = if mob.baby { -24000 } else { 0 };
            mob.built = false;
            mob.hurt = 0.3;
            if let Some(v) = &mut mob.villager {
                v.sleeping = false;
                v.trading = false;
                v.fleeing = false;
                v.goal = None;
            }
            return;
        }
        self.hit_mob(attacker, victim, damage, 5.0, events);
    }

    /// A golden apple starts the 3–5 minute cure while Weakness is still active.
    pub fn try_cure(&mut self, index: usize) -> bool {
        self.try_cure_for(index, super::PlayerId::HOST)
    }
    pub fn try_cure_for(&mut self, index: usize, owner: super::PlayerId) -> bool {
        let Some(mob) = self.mobs.get_mut(index) else { return false };
        if !mob.alive() || mob.kind != MobKind::ZombieVillager || mob.weakness_left <= 0.0 || mob.convert_left > 0.0 {
            return false;
        }
        mob.convert_left = (3600 + self.rng.next_int(2401)) as f32 / 20.0;
        mob.weakness_left = 0.0;
        mob.convert_tick = 0.0;
        mob.convert_by = Some(owner);
        true
    }

    /// Natural zombies everywhere have Java's 5% chance to be zombie villagers.
    pub(super) fn note_zombie_villager(&mut self, kind: MobKind) {
        if kind != MobKind::Zombie {
            return;
        }
        if self.mobs.is_empty() {
            return;
        }
        if self.rng.chance(0.05) {
            let id = self.next_villager_id;
            self.next_villager_id = self.next_villager_id.saturating_add(1);
            let seed = self.rng.next_int(u32::MAX) as u64;
            let prof = Profession::ALL[1 + self.rng.next_int(14) as usize];
            let mob = self.mobs.last_mut().unwrap();
            if mob.baby {
                mob.age = -24000;
            }
            mob.kind = MobKind::ZombieVillager;
            let mut v = villager::Villager::new(id, seed);
            v.set_profession(prof);
            mob.villager = Some(Box::new(v));
        }
    }
}

impl Mob {
    pub(super) fn advance_cure<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, rng: &mut Rng) {
        if self.kind != MobKind::ZombieVillager || !self.alive() || self.convert_left <= 0.0 {
            return;
        }
        self.convert_tick += dt * 20.0;
        while self.convert_tick >= 1.0 && self.convert_left > 0.0 {
            self.convert_tick -= 1.0;
            self.convert_left =
                (self.convert_left - conversion_progress(self.pos.as_ivec3(), world, rng) as f32 / 20.0).max(0.0);
        }
        if self.convert_left == 0.0 {
            self.finish_cure();
        }
    }
}
/// One percent of conversion ticks scan an 8³ cube, considering at most
/// fourteen beds/bars; each has a 30% chance to add one tick of progress.
fn conversion_progress<W: MobWorld + ?Sized>(pos: IVec3, world: &W, rng: &mut Rng) -> u8 {
    let mut progress = 1;
    if !rng.chance(0.01) {
        return progress;
    }
    let mut count = 0;
    'blocks: for x in pos.x - 4..pos.x + 4 {
        for y in pos.y - 4..pos.y + 4 {
            for z in pos.z - 4..pos.z + 4 {
                if world.block(IVec3::new(x, y, z)).is_some_and(|b| b == Block::IRON_BARS || b.is_bed()) {
                    if rng.chance(0.3) {
                        progress += 1;
                    }
                    count += 1;
                    if count == 14 {
                        break 'blocks;
                    }
                }
            }
        }
    }
    progress
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;
    #[test]
    fn curing_strength_fraction_and_equipment_roundtrip() {
        let mut e = Entities::new(11);
        e.spawn(MobKind::ZombieVillager, glam::DVec3::ZERO);
        e.mobs[0].weakness_left = 30.0;
        assert_eq!(e.mobs[0].melee_attack().0, 0.0);
        assert!(e.try_cure(0));
        assert_eq!(e.mobs[0].melee_attack().0, 6.0);
        e.mobs[0].convert_tick = 0.5;
        e.mobs[0].armor = [Some(super::super::armor::ArmorKind::Iron); 4];
        e.mobs[0].armor_glint = 15;
        let mut copy = Entities::new(12);
        copy.load_villagers(&e.villagers_to_string());
        assert_eq!(copy.mobs[0].convert_tick, 0.5);
        assert_eq!(copy.mobs[0].melee_attack().0, 6.0);
        copy.mobs[0].convert_left = 0.05;
        copy.mobs[0].advance_cure(0.025, &Grid::flat(0), &mut Rng::new(4));
        assert_eq!(copy.mobs[0].kind, MobKind::Villager);
        assert_eq!(copy.mobs[0].armor, [None; 4]);
        assert_eq!(copy.mobs[0].armor_glint, 0);
    }
    #[test]
    fn beds_and_bars_accelerate_and_only_first_fourteen_count() {
        let empty = Grid::flat(-10);
        let mut full = Grid::flat(-10);
        let mut fourteen = Grid::flat(-10);
        let mut count = 0;
        for x in -4..4 {
            for y in -4..4 {
                for z in -4..4 {
                    let b = if count % 2 == 0 { Block::IRON_BARS } else { Block::BED_HEAD };
                    full.set(IVec3::new(x, y, z), b);
                    if count < 14 {
                        fourteen.set(IVec3::new(x, y, z), b);
                    }
                    count += 1;
                }
            }
        }
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let mut c = Rng::new(7);
        let mut accelerated = 0;
        for _ in 0..40000 {
            assert_eq!(conversion_progress(IVec3::ZERO, &empty, &mut a), 1);
            let p = conversion_progress(IVec3::ZERO, &full, &mut b);
            assert_eq!(p, conversion_progress(IVec3::ZERO, &fourteen, &mut c));
            accelerated += p as u32;
        }
        assert!((41000..42500).contains(&accelerated), "{accelerated}");
    }
}
