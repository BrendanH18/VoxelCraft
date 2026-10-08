//! Food pickup and Java's food/bed/age requirements for resident breeding.
use super::villager::Villager;
use super::{Entities, MobKind, MobWorld};
use crate::inventory::{Stack, move_into};
use crate::item::Item;
use glam::{DVec3, IVec3};

pub fn food_points(item: Item) -> u16 {
    match item {
        Item::BREAD => 4,
        Item::CARROT | Item::POTATO | Item::BEETROOT => 1,
        _ => 0,
    }
}
impl Villager {
    pub(super) fn food_points(&self) -> u16 {
        self.food.iter().flatten().map(|s| food_points(s.item) * s.count as u16).sum::<u16>() + self.food_level as u16
    }
    pub(super) fn eat_for_breeding(&mut self) {
        for slot in &mut self.food {
            if let Some(s) = slot {
                while s.count > 0 && self.food_level < 12 {
                    self.food_level += food_points(s.item) as u8;
                    s.count -= 1;
                }
                if s.count == 0 {
                    *slot = None;
                }
            }
        }
        self.food_level = self.food_level.saturating_sub(12);
    }
    pub(super) fn accept_food(&mut self, stack: Stack) -> Option<Stack> {
        move_into(stack, &mut self.food, &[0, 1, 2, 3, 4, 5, 6, 7])
    }
}
impl Entities {
    pub(super) fn breed_villagers<W: MobWorld + ?Sized>(&mut self, world: &W) {
        self.mob_index.rebuild(&self.mobs);
        self.claimed_beds.clear();
        for m in &self.mobs {
            if m.alive()
                && m.kind == MobKind::Villager
                && let Some(home) = m.villager.as_ref().and_then(|v| v.home)
            {
                self.claimed_beds.insert(home);
            }
        }
        for item in &mut self.items {
            if item.pickup_delay > 0.0
                || food_points(item.stack.item) == 0
                || !world.loaded(item.pos.floor().as_ivec3())
            {
                continue;
            }
            if let Some(i) = self.mob_index.nearest(&self.mobs, item.pos, 1.75, |m| {
                m.kind == MobKind::Villager && m.villager.as_ref().is_some_and(|v| v.active && !v.sleeping)
            }) {
                let left = self.mobs[i].villager.as_mut().unwrap().accept_food(item.stack);
                item.stack.count = left.map_or(0, |s| s.count);
            }
        }
        self.items.retain(|i| i.stack.count > 0);
        let ready = |m: &super::Mob| {
            m.alive()
                && m.kind == MobKind::Villager
                && m.age == 0
                && m.villager
                    .as_ref()
                    .is_some_and(|v| v.active && !v.sleeping && !v.trading && !v.fleeing && v.food_points() >= 12)
        };
        let n = self.mobs.len();
        for i in 0..n {
            if !ready(&self.mobs[i]) {
                if let Some(v) = self.mobs[i].villager.as_mut() {
                    v.courtship = 0;
                }
                continue;
            }
            let pos = self.mobs[i].pos;
            let Some(j) = self.mob_index.nearest(&self.mobs, pos, 8.0, |m| {
                ready(m) && m.villager.as_ref().unwrap().id != self.mobs[i].villager.as_ref().unwrap().id
            }) else {
                self.mobs[i].villager.as_mut().unwrap().courtship = 0;
                continue;
            };
            self.mobs[i].villager.as_mut().unwrap().goal = Some(self.mobs[j].pos);
            if pos.distance_squared(self.mobs[j].pos) > 5.0 {
                continue;
            }
            let v = self.mobs[i].villager.as_mut().unwrap();
            if v.courtship == 0 {
                v.courtship = 275 + self.rng.next_int(50) as u16;
            }
            v.courtship = v.courtship.saturating_sub(20);
            if v.courtship > 0 {
                self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
                    crate::particles::Kind::Heart,
                    pos + DVec3::Y * 1.5,
                    2,
                )));
                continue;
            }
            self.mobs[i].villager.as_mut().unwrap().eat_for_breeding();
            self.mobs[j].villager.as_mut().unwrap().eat_for_breeding();
            let mut bed = None;
            let mut distance = 48.0 * 48.0;
            world.village_pois(&mut |p, b| {
                let d = p.as_dvec3().distance_squared(pos);
                if b.is_bed_head()
                    && !self.claimed_beds.contains(&p)
                    && d < distance
                    && [p + IVec3::Y, p + IVec3::Y * 2]
                        .into_iter()
                        .all(|q| world.block(q) == Some(crate::world::block::Block::AIR))
                {
                    bed = Some(p);
                    distance = d;
                }
            });
            if let Some(bed) = bed {
                self.claimed_beds.insert(bed);
                self.mobs[i].age = 6000;
                self.mobs[j].age = 6000;
                self.spawn(MobKind::Villager, pos);
                let baby = self.mobs.last_mut().unwrap();
                baby.age = -24000;
                baby.baby = true;
                baby.villager.as_mut().unwrap().home = Some(bed);
                self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
                    crate::particles::Kind::Heart,
                    pos + DVec3::Y,
                    7,
                )));
            } else {
                self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
                    crate::particles::Kind::Angry,
                    pos + DVec3::Y * 1.5,
                    2,
                )));
            }
        }
    }
}
