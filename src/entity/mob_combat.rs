//! MOB-02d (v0.15 Track A): what a mob's equipment does in a fight, after
//! Java 26.3 `Mob` and `LivingEntity`.
//!
//! - The main-hand item's `ATTACK_DAMAGE` modifier adds to the mob's attack
//!   damage attribute, its Sharpness to the hit (`EnchantmentHelper.
//!   modifyDamage`; Smite and Bane of Arthropods against a mob that is
//!   sensitive to them, with Bane's Slowness), its Knockback to `getKnockback` and its Fire Aspect sets
//!   the target alight (`doPostAttackEffects`), all in `Mob.doHurtTarget`.
//! - Worn humanoid armor counts in `getDamageAfterArmorAbsorb` and its
//!   protection enchantments in `getDamageAfterMagicAbsorb`. Mobs' armor
//!   takes no wear (`LivingEntity.hurtArmor` is empty).
//! - `CanPickUpLoot`: `Mob.aiStep`'s looting pass under `mobGriefing`, with
//!   `pickUpItem`, `equipItemIfPossible`, `canReplaceCurrentItem` and the
//!   kinds' `wantsToPickUp` and preferred weapons.
//!
//! The constants are checked against `tests/fixtures/java/26.3/illagers.json`
//! (`mob_combat`).
use glam::DVec3;

use super::equipment::Slot;
use super::{Entities, EntityEvent, Mob, MobKind};
use crate::enchant::{Creature, Enchantment};
use crate::inventory::Stack;
use crate::item::Item;
use crate::simulation::damage::{Cause, DamageTag, DamageType};

/// The damage type of a melee hit from a mob or player (`mob_attack`),
/// which armor and Protection reduce.
pub const ATTACK_CAUSE: Cause = Cause::new(DamageType::MobAttack, "was slain by a mob");
/// `Mob.ITEM_PICKUP_REACH`: the box a mob picks items up from grows this
/// much on each axis.
pub const PICKUP_REACH: [f64; 3] = [1.0, 0.0, 1.0];
/// `Zombie` and `AbstractSkeleton.finalizeSpawn`: `CanPickUpLoot` with
/// chance `0.55 * specialMultiplier`.
pub const CAN_PICK_UP_LOOT_CHANCE: f32 = 0.55;
/// `equipItemIfPossible`: the replaced item drops if
/// `max(nextFloat() - 0.1, 0) < dropChance`.
pub const REPLACE_DROP_OFFSET: f32 = 0.1;

impl Mob {
    /// The main-hand stack, if any.
    pub fn main_hand(&self) -> Option<Stack> {
        self.equipment.main_hand().copied()
    }

    /// What the main-hand weapon adds to a melee hit: its attack damage
    /// modifier (the player's attack damage with it, less the player's
    /// base 1) and Sharpness. Smite and Bane of Arthropods depend on the
    /// target, so a mob target's kind adds them when the hit lands
    /// (`Entities::resolve_mob_hits`, v1.0 gap Track 1); players are
    /// sensitive to neither.
    pub fn weapon_attack_bonus(&self) -> f32 {
        let Some(stack) = self.main_hand() else { return 0.0 };
        let modifier = crate::mining::attack_damage(Some(stack.item)) - 1.0;
        modifier + crate::enchant::damage_bonus(stack.active_enchants(), Creature::Other)
    }

    /// The main-hand weapon's Knockback level, which `getKnockback` adds to
    /// the attack knockback attribute.
    pub fn weapon_knockback_level(&self) -> u32 {
        self.main_hand().map_or(0, |s| u32::from(s.active_enchants().level(Enchantment::Knockback)))
    }

    /// Seconds of fire the main-hand weapon's Fire Aspect sets on a target it
    /// hurts (4 per level).
    pub fn weapon_fire_seconds(&self) -> f32 {
        self.main_hand().map_or(0.0, |s| 4.0 * f32::from(s.active_enchants().level(Enchantment::FireAspect)))
    }

    /// The worn humanoid armor stacks (head, chest, legs, feet).
    pub(crate) fn worn_stacks(&self) -> [Option<Stack>; 4] {
        [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet].map(|slot| {
            self.equipment
                .get(slot)
                .copied()
                .filter(|s| s.item.as_armor().is_some_and(|(p, _)| Slot::of_piece(p) == slot))
        })
    }

    /// The `ARMOR` and `ARMOR_TOUGHNESS` the worn humanoid armor adds.
    pub fn worn_armor(&self) -> (u32, f32) {
        self.worn_stacks()
            .iter()
            .flatten()
            .filter_map(|s| s.item.as_armor())
            .fold((0, 0.0), |(p, t), (piece, m)| (p + u32::from(m.defense(piece)), t + m.toughness()))
    }
}

/// `getDamageAfterArmorAbsorb` with the worn humanoid armor: `cause`s that
/// bypass armor keep their amount.
pub fn after_worn_armor(m: &Mob, amount: f32, cause: Cause) -> f32 {
    let (points, toughness) = m.worn_armor();
    if cause.is(DamageTag::BypassesArmor) || points == 0 && toughness == 0.0 {
        return amount;
    }
    crate::simulation::survival::armor_reduce_with(amount, points, toughness, m.armor_effectiveness)
}

/// `LivingEntity.getEquipmentSlotForItem`: an armor piece goes in its slot,
/// anything else in the main hand.
pub fn slot_for(item: Item) -> Slot {
    item.as_armor().map_or(Slot::MainHand, |(piece, _)| Slot::of_piece(piece))
}

/// `getPreferredWeaponType`: `#pillager_preferred_weapons`,
/// `#skeleton_preferred_weapons`, `#drowned_preferred_weapons` and
/// `#piglin_preferred_weapons` (no others).
pub fn prefers(kind: MobKind, item: Item) -> Option<bool> {
    match kind {
        MobKind::Pillager => Some(item == Item::CROSSBOW),
        MobKind::Skeleton | MobKind::Stray | MobKind::Bogged | MobKind::WitherSkeleton => Some(item == Item::BOW),
        MobKind::Drowned => Some(item == crate::weapons::TRIDENT),
        MobKind::Piglin => Some(item == Item::CROSSBOW || item == crate::weapons::spear(crate::item::Tier::Gold)),
        _ => None,
    }
}

/// `getApproximateAttributeWith(stack, ATTACK_DAMAGE, MAINHAND)` less the
/// shared base: the item's attack damage modifier.
fn attack_modifier(item: Item) -> f32 {
    crate::mining::attack_damage(Some(item)) - 1.0
}

/// The `ARMOR` and `ARMOR_TOUGHNESS` a stack gives in `slot`.
fn armor_in(item: Item, slot: Slot) -> (u32, f32) {
    match item.as_armor() {
        Some((piece, m)) if Slot::of_piece(piece) == slot => (u32::from(m.defense(piece)), m.toughness()),
        _ => (0, 0.0),
    }
}

/// `Mob.canReplaceEqualItem`: more enchantments, then less damage, then a
/// custom name over none.
pub fn can_replace_equal(new: &Stack, current: &Stack) -> bool {
    let (n, c) = (new.enchants.iter().count(), current.enchants.iter().count());
    if n != c {
        return n > c;
    }
    if new.damage != current.damage {
        return new.damage < current.damage;
    }
    new.name.as_str().is_some() && current.name.as_str().is_none()
}

/// `Mob.canReplaceCurrentItem` (and the drowned's refusal to give up a
/// nautilus shell): an empty slot takes anything; armor compares defense,
/// then toughness; the main hand compares the preferred weapon type, then
/// attack damage; ties fall to [`can_replace_equal`]. Armor under Curse of
/// Binding stays.
pub fn can_replace(kind: MobKind, new: &Stack, current: Option<&Stack>, slot: Slot) -> bool {
    let Some(current) = current else { return true };
    if kind == MobKind::Drowned && current.item == Item::NAUTILUS_SHELL {
        return false;
    }
    if slot.is_armor() {
        if current.enchants.has(Enchantment::BindingCurse) {
            return false;
        }
        let (nd, nt) = armor_in(new.item, slot);
        let (cd, ct) = armor_in(current.item, slot);
        return if nd != cd {
            nd > cd
        } else if nt != ct {
            nt > ct
        } else {
            can_replace_equal(new, current)
        };
    }
    if slot != Slot::MainHand {
        return false;
    }
    if let (Some(np), Some(cp)) = (prefers(kind, new.item), prefers(kind, current.item)) {
        if cp && !np {
            return false;
        }
        if !cp && np {
            return true;
        }
    }
    let (n, c) = (attack_modifier(new.item), attack_modifier(current.item));
    if n != c { n > c } else { can_replace_equal(new, current) }
}

/// `wantsToPickUp` (`canHoldItem` for the rest): zombies refuse glow ink
/// sacs, drowned and skeletons spears, and a riding baby zombie eggs.
pub fn wants(kind: MobKind, item: Item, baby: bool, riding: bool) -> bool {
    match kind {
        MobKind::Drowned => !crate::weapons::is_spear(item),
        k if k.is_zombie() => item != Item::GLOW_INK_SAC && !(baby && riding && item == Item::EGG),
        MobKind::Skeleton | MobKind::Stray | MobKind::Bogged | MobKind::WitherSkeleton => {
            !crate::weapons::is_spear(item)
        }
        _ => true,
    }
}

/// `Mob.equipItemIfPossible` on `m` for `stack`: the slot it goes in, or
/// `None` when the mob doesn't take it. An armor piece the slot won't take
/// may go in an empty main hand instead.
pub fn equip_slot(m: &Mob, stack: &Stack) -> Option<Slot> {
    let mut slot = slot_for(stack.item);
    let mut replace = can_replace(m.kind, stack, m.equipment.get(slot), slot);
    if slot.is_armor() && !replace {
        slot = Slot::MainHand;
        replace = m.equipment.get(slot).is_none();
    }
    replace.then_some(slot)
}

impl Entities {
    /// `Mob.aiStep`'s looting pass for every mob with `CanPickUpLoot`, under
    /// `mobGriefing`: each item without a pickup delay whose box touches
    /// the mob's box grown by [`PICKUP_REACH`] is equipped if the mob wants
    /// it (`pickUpItem`). The replaced item may drop; the new one always
    /// drops on death (`setGuaranteedDrop`) and the mob turns persistent.
    pub(super) fn mobs_pick_up_loot(&mut self, events: &mut Vec<EntityEvent>) {
        if !self.villager_griefing || self.items.is_empty() {
            return;
        }
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if !m.can_pick_up_loot || !m.alive() || !m.simulation_active {
                continue;
            }
            let (min, max) = m.aabb();
            let reach = DVec3::from_array(PICKUP_REACH);
            let (min, max) = (min - reach, max + reach);
            let mut j = 0;
            while j < self.items.len() {
                let item = &self.items[j];
                let (imin, imax) = item.aabb();
                let touches = imin.cmplt(max).all() && imax.cmpgt(min).all();
                let m = &self.mobs[i];
                if !touches
                    || item.pickup_delay > 0.0
                    || item.stack.count == 0
                    || !wants(m.kind, item.stack.item, m.baby, m.riding.is_some())
                {
                    j += 1;
                    continue;
                }
                let Some(slot) = equip_slot(m, &item.stack) else {
                    j += 1;
                    continue;
                };
                let mut taken = item.stack;
                if slot.count_limit() > 0 {
                    taken.count = taken.count.min(slot.count_limit());
                }
                let m = &mut self.mobs[i];
                if let Some(old) = m.equipment.get(slot).copied() {
                    let chance = m.equipment.drop_chance(slot);
                    if (self.rng.next_f32() - REPLACE_DROP_OFFSET).max(0.0) < chance {
                        events.push(EntityEvent::DropEquipment { stack: old, pos: m.pos });
                    }
                }
                m.equipment.set(slot, Some(taken));
                m.equipment.set_guaranteed_drop(slot);
                m.persistent = true;
                let left = self.items[j].stack.count - taken.count;
                if left == 0 {
                    self.items.swap_remove(j);
                } else {
                    self.items[j].stack.count = left;
                    j += 1;
                }
            }
        }
    }
}
