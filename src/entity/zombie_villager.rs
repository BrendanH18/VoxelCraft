//! Zombie infection, natural variants and golden-apple conversion.
use super::villager::{self, Profession};
use super::{Entities, EntityEvent, MobKind};
use crate::simulation::difficulty::Difficulty;
impl Entities {
    /// A killing blow from a zombie converts the villager on Normal (50%) and Hard (100%).
    pub(super) fn infect_or_hit(&mut self, attacker: usize, victim: usize, events: &mut Vec<EntityEvent>) {
        let (damage, _) = self.mobs[attacker].kind.melee();
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
            mob.built = true;
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
