//! Villager inventories (Java's 8-slot `AbstractVillager.inventory`), item
//! pickup and Java's food/bed/age requirements for resident breeding.
use super::villager::{Profession, Villager};
use super::{Entities, MobKind, MobWorld};
use crate::inventory::{Stack, move_into};
use crate::item::Item;
use glam::{DVec3, IVec3};

/// `DataComponents.VILLAGER_FOOD`'s nutrition: bread 4, carrots, potatoes
/// and beetroots 1.
pub fn food_points(item: Item) -> u16 {
    match item {
        Item::BREAD => 4,
        Item::CARROT | Item::POTATO | Item::BEETROOT => 1,
        _ => 0,
    }
}
/// v1.0 gap Track 2g: `ItemTags.VILLAGER_PLANTABLE_SEEDS` (26.3's data
/// pack): what `HarvestFarmland` plants and `Villager.hasFarmSeeds` looks for.
pub fn plantable_seed(item: Item) -> bool {
    matches!(
        item,
        Item::WHEAT_SEEDS
            | Item::POTATO
            | Item::CARROT
            | Item::BEETROOT_SEEDS
            | Item::TORCHFLOWER_SEEDS
            | Item::PITCHER_POD
    )
}

/// v1.0 gap Track 2g: `ItemTags.VILLAGER_PICKS_UP`: the plantable seeds,
/// bread, wheat and beetroots.
pub fn picked_up(item: Item) -> bool {
    plantable_seed(item) || matches!(item, Item::BREAD | Item::WHEAT | Item::BEETROOT)
}

/// v1.0 gap Track 2g: `VillagerProfession.requestedItems`: only the
/// farmer's (wheat, wheat seeds, beetroot seeds and bone meal) are set.
pub fn requested(profession: Profession, item: Item) -> bool {
    profession == Profession::Farmer
        && matches!(item, Item::WHEAT | Item::WHEAT_SEEDS | Item::BEETROOT_SEEDS | Item::BONE_MEAL)
}

impl Villager {
    /// `countFoodPointsInInventory` plus `foodLevel` (what `canBreed` adds).
    pub(super) fn food_points(&self) -> u16 {
        self.inventory_food_points() + self.food_level as u16
    }
    /// `countFoodPointsInInventory`.
    pub(super) fn inventory_food_points(&self) -> u16 {
        self.food.iter().flatten().map(|s| food_points(s.item) * s.count as u16).sum::<u16>()
    }
    /// `hasExcessFood`: 24 food points or more in the inventory.
    pub(super) fn has_excess_food(&self) -> bool {
        self.inventory_food_points() >= 24
    }
    /// `wantsMoreFood`: fewer than 12 food points in the inventory.
    pub(super) fn wants_more_food(&self) -> bool {
        self.inventory_food_points() < 12
    }
    /// `SimpleContainer.countItem`.
    pub(crate) fn count_item(&self, item: Item) -> u32 {
        self.food.iter().flatten().filter(|s| s.item == item).map(|s| u32::from(s.count)).sum()
    }
    /// `SimpleContainer.removeItemType`: up to `count` of `item`, from the
    /// first slot on.
    pub(crate) fn remove_item_type(&mut self, item: Item, mut count: u32) {
        for slot in &mut self.food {
            if count == 0 {
                break;
            }
            if let Some(s) = slot
                && s.item == item
            {
                let n = count.min(u32::from(s.count));
                s.count -= n as u8;
                count -= n;
                if s.count == 0 {
                    *slot = None;
                }
            }
        }
    }
    /// Takes one item from inventory slot `i` (`ItemStack.shrink(1)`).
    pub(crate) fn shrink_slot(&mut self, i: usize) {
        if let Some(s) = &mut self.food[i] {
            s.count -= 1;
            if s.count == 0 {
                self.food[i] = None;
            }
        }
    }
    /// The inventory's slots (`getInventory().getItems()`).
    pub(crate) fn inventory(&self) -> &[Option<Stack>; 8] {
        &self.food
    }
    /// `Villager.wantsToPickUp`: `VILLAGER_PICKS_UP`, villager food or the
    /// profession's requested items, with room for it.
    pub(super) fn wants_to_pick_up(&self, stack: Stack) -> bool {
        (picked_up(stack.item) || food_points(stack.item) > 0 || requested(self.profession, stack.item))
            && self.can_accept_food(stack)
    }
    pub(super) fn eat_for_breeding(&mut self) {
        for slot in &mut self.food {
            // `eatUntilFull` eats only `VILLAGER_FOOD`.
            if let Some(s) = slot
                && food_points(s.item) > 0
            {
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
    /// `SimpleContainer.canAddItem`.
    fn can_accept_food(&self, stack: Stack) -> bool {
        self.food.iter().any(|slot| slot.is_none_or(|s| s.stacks_with(&stack) && s.count < s.max()))
    }
    /// `SimpleContainer.addItem`: onto stacks of the same item, then into
    /// the first empty slot; what doesn't fit is returned.
    pub(crate) fn accept_food(&mut self, stack: Stack) -> Option<Stack> {
        move_into(stack, &mut self.food, &[0, 1, 2, 3, 4, 5, 6, 7])
    }
}
impl Entities {
    /// v1.0 gap Track 2g: villagers pick up what they want each tick
    /// (`Mob.aiStep`'s pickup with `mobGriefing`, `Villager.pickUpItem`).
    pub(super) fn villager_pickup<W: MobWorld + ?Sized>(&mut self, world: &W) {
        if !self.villager_griefing || self.items.is_empty() || !self.mobs.iter().any(|m| m.villager.is_some()) {
            return;
        }
        self.mob_index.rebuild(&self.mobs);
        for item in &mut self.items {
            // `Mob.aiStep`'s pickup with `mobGriefing`, and
            // `Villager.wantsToPickUp` (v1.0 gap Track 2g: seeds, wheat and a
            // farmer's bone meal as well as food).
            if !self.villager_griefing
                || item.pickup_delay > 0.0
                || !(picked_up(item.stack.item) || food_points(item.stack.item) > 0 || item.stack.item == Item::BONE_MEAL)
                || !world.loaded(item.pos.floor().as_ivec3())
            {
                continue;
            }
            if let Some(i) = self.mob_index.nearest(&self.mobs, item.pos, 1.75, |m| {
                m.kind == MobKind::Villager
                    && m.alive()
                    && m.villager.as_ref().is_some_and(|v| v.active && v.wants_to_pick_up(item.stack))
            }) {
                let left = self.mobs[i].villager.as_mut().unwrap().accept_food(item.stack);
                item.stack.count = left.map_or(0, |s| s.count);
            }
        }
        self.items.retain(|i| i.stack.count > 0);
    }
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
        // `VillagerMakeLove` runs only in the idle activity (`getIdlePackage`).
        let tick = (self.village_time.rem_euclid(1.0) * 24000.0) as u32;
        let idle = super::villager::schedule_activity(tick, false) == super::villager::Activity::Idle;
        let ready = |m: &super::Mob| {
            idle && m.alive()
                && m.kind == MobKind::Villager
                && m.age == 0
                && m.villager.as_ref().is_some_and(|v| {
                    v.active && !v.sleeping && !v.trading && !v.fleeing && v.bell_hide <= 0.0 && v.food_points() >= 12
                })
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
                    && !self.storage.claims_bed(p)
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
