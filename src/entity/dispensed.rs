//! v0.17 Track C2: the entity half of Java 26.3's dispense table
//! (`DispenseItemBehavior.bootStrap`, `EquipmentDispenseItemBehavior`,
//! `ShearsDispenseItemBehavior`, `ProjectileDispenseBehavior` and the
//! anonymous armour stand, chest, brush and flint-and-steel behaviours).
//! `world::dispensing` decides which behaviour an item has and calls these
//! from `World::tick_automation_entities` with the cell in front of the
//! dispenser.
//!
//! Java finds its targets with `getEntitiesOfClass(..., new AABB(front))`
//! and takes the first in its section lists. Here mobs come first (through
//! the mob index, so a dispense never walks every mob), then armour stands,
//! then players; within each, the first whose box overlaps the cell.
use glam::{DVec3, IVec3};

use super::equipment::Slot;
use super::{Entities, MobKind};
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::dispensing::{self, Allowed};

/// A player a dispenser can dress (`LivingEntity.canEquipWithDispenser` for
/// a player): the app lends each one's worn armour and off hand.
pub struct DispenseTarget<'a> {
    /// Feet.
    pub pos: DVec3,
    pub half_width: f64,
    pub height: f64,
    /// Alive and not a spectator.
    pub can_equip: bool,
    /// Indexed by [`crate::item::ArmorPiece`].
    pub armor: &'a mut [Option<Stack>; 4],
    pub offhand: &'a mut Option<Stack>,
}

fn overlaps((min, max): (DVec3, DVec3), cell: IVec3) -> bool {
    let (lo, hi) = (cell.as_dvec3(), cell.as_dvec3() + DVec3::ONE);
    min.cmplt(hi).all() && max.cmpgt(lo).all()
}

fn armor_index(slot: Slot) -> Option<usize> {
    Some(match slot {
        Slot::Head => 0,
        Slot::Chest => 1,
        Slot::Legs => 2,
        Slot::Feet => 3,
        _ => return None,
    })
}

impl Entities {
    /// Mob indices whose box meets `cell`, in mob order, through the mob
    /// index (rebuilt here: dispenses are rare and the index may be stale).
    fn mobs_in_cell(&mut self, cell: IVec3) -> Vec<usize> {
        self.mob_index.rebuild(&self.mobs);
        let mut found = Vec::new();
        let mobs = &self.mobs;
        // The widest mob is under four blocks across; eight covers any box
        // that can reach the cell from its feet.
        self.mob_index.visit(cell.as_dvec3() + DVec3::splat(0.5), 8.0, |i| {
            if overlaps(mobs[i].aabb(), cell) {
                found.push(i);
            }
        });
        found.sort_unstable();
        found
    }

    /// `EquipmentDispenseItemBehavior.dispenseEquipment`: the first living
    /// entity in `cell` that `canEquipWithDispenser` takes one `stack` into
    /// its empty slot; a mob also keeps it on death and stops despawning.
    /// Returns whether one did.
    pub fn equip_dispensed(&mut self, cell: IVec3, stack: Stack, players: &mut [DispenseTarget<'_>]) -> bool {
        let Some(e) = dispensing::equippable(stack.item) else { return false };
        let one = Stack { count: 1, ..stack };
        for i in self.mobs_in_cell(cell) {
            let m = &mut self.mobs[i];
            if !m.alive() || !e.allowed.admits(m.kind) {
                continue;
            }
            let baby = m.baby || m.age < 0;
            let kind = m.kind;
            // `canUseSlot`: horses, llamas and the undead horses use every
            // slot at any age; the other horses, nautiluses, pigs, striders
            // and happy ghasts only grown. `canDispenserEquipIntoSlot`: the
            // horse family and nautiluses only once tamed.
            let any_age = matches!(
                kind,
                MobKind::Horse | MobKind::ZombieHorse | MobKind::SkeletonHorse | MobKind::Llama | MobKind::TraderLlama
            );
            let needs_tame = super::mounts::equine(kind) || super::nautilus::is_nautilus(kind);
            match (e.slot, m.mount.as_mut()) {
                (slot @ (Slot::Saddle | Slot::Body), Some(s)) => {
                    let index = if slot == Slot::Saddle { 0 } else { 1 };
                    if baby && !any_age || needs_tame && !s.tame || s.slots[index].is_some() {
                        continue;
                    }
                    s.slots[index] = Some(one);
                }
                (Slot::Saddle | Slot::Body, _) => continue,
                (slot, _) => {
                    if !m.can_pick_up_loot || m.equipment.get(slot).is_some() {
                        continue;
                    }
                    m.equipment.set(slot, Some(one));
                    m.equipment.set_guaranteed_drop(slot);
                }
            }
            m.persistent = true;
            return true;
        }
        // `ArmorStand.canUseSlot`: no body or saddle slot. VoxelCraft's
        // stands hold the four armour pieces only.
        if let Some(index) = armor_index(e.slot) {
            for d in &mut self.decor {
                if d.kind == super::decor::Kind::Stand && overlaps(d.aabb(), cell) && d.armor[index].is_none() {
                    d.armor[index] = Some(one);
                    return true;
                }
            }
        }
        if e.allowed != Allowed::Any {
            return false;
        }
        for p in players.iter_mut() {
            let (min, max) = (
                p.pos - DVec3::new(p.half_width, 0.0, p.half_width),
                p.pos + DVec3::new(p.half_width, p.height, p.half_width),
            );
            if !p.can_equip || !overlaps((min, max), cell) {
                continue;
            }
            let slot = match (armor_index(e.slot), e.slot) {
                (Some(i), _) => &mut p.armor[i],
                (None, Slot::OffHand) => &mut *p.offhand,
                _ => continue,
            };
            if slot.is_none() {
                *slot = Some(one);
                return true;
            }
        }
        false
    }

    /// `ShearsDispenseItemBehavior.tryShearEntity`: the first entity in
    /// `cell` whose leash comes off, or that is ready for shearing
    /// (sheep, mooshrooms, bogged, sulfur cubes holding a block, copper
    /// golems wearing a poppy). Returns whether one was.
    pub fn shear_dispensed(&mut self, cell: IVec3) -> bool {
        // A lead tied to a fence in the cell comes off too.
        if self.break_fence_knot(cell) {
            return true;
        }
        for i in self.mobs_in_cell(cell) {
            let m = &mut self.mobs[i];
            if !m.alive() {
                continue;
            }
            if self.shear_leashes(i) {
                return true;
            }
            let m = &mut self.mobs[i];
            let sheared = match m.kind {
                MobKind::Sheep => self.use_on_sheep(i, Item::SHEARS) == Some(true),
                MobKind::Mooshroom => {
                    matches!(self.use_mooshroom(i, Item::SHEARS), Some(super::mooshroom::Use::Shear))
                }
                MobKind::Bogged => self.shear_bogged(i, Item::SHEARS),
                MobKind::SulfurCube => self.shear_sulfur_cube(i),
                MobKind::CopperGolem => {
                    let pos = m.pos;
                    match m.copper_golem.as_mut().and_then(|s| s.shear()) {
                        Some(flower) => {
                            self.scatter(flower, pos + DVec3::Y);
                            true
                        }
                        None => false,
                    }
                }
                _ => false,
            };
            if sheared {
                return true;
            }
        }
        false
    }

    /// The brush behaviour: the first armadillo in `cell` that sheds a
    /// scute (`Armadillo.brushOffScute`, adults only).
    pub fn brush_dispensed(&mut self, cell: IVec3) -> bool {
        for i in self.mobs_in_cell(cell) {
            if self.mobs[i].kind == MobKind::Armadillo && self.brush_armadillo(i) {
                return true;
            }
        }
        false
    }

    /// The chest behaviour: the first living, tamed donkey, mule or llama
    /// in `cell` without a chest gets one (`AbstractChestedHorse` slot 499).
    pub fn chest_onto_mount(&mut self, cell: IVec3) -> bool {
        for i in self.mobs_in_cell(cell) {
            let m = &mut self.mobs[i];
            if !m.alive() || !super::mounts::chested(m.kind) {
                continue;
            }
            let Some(s) = m.mount.as_mut() else { continue };
            if s.tame && !s.chest {
                s.chest = true;
                m.persistent = true;
                return true;
            }
        }
        false
    }

    /// `FlintAndSteelDispenseItemBehavior.tryIgniteExplosiveEntities`: the
    /// first sulfur cube in `cell` that can explode starts its fuse.
    pub fn ignite_dispensed(&mut self, cell: IVec3, tnt_explodes: bool) -> bool {
        for i in self.mobs_in_cell(cell) {
            if self.mobs[i].prime_sulfur_cube(false, tnt_explodes, &mut self.rng) {
                return true;
            }
        }
        false
    }

    /// `ProjectileDispenseBehavior` for the thrown items (eggs, snowballs,
    /// wind charges, experience bottles, splash and lingering potions): the
    /// projectile leaves the item's `DispenseConfig` spawn point at its power
    /// and uncertainty (`Projectile.shoot`). Returns false for items this
    /// doesn't shoot (arrows, fire charges and rockets have their own).
    pub fn dispense_projectile(&mut self, stack: Stack, block: IVec3, step: IVec3) -> bool {
        let Some(config) = dispensing::projectile(stack.item) else { return false };
        let pos = block.as_dvec3() + DVec3::splat(0.5) + dispensing::projectile_offset(stack.item, step);
        if let Some(kind) = super::thrown::Kind::from_item(stack.item) {
            self.thrown.push(super::thrown::Thrown::dispensed(
                kind,
                pos,
                step.as_dvec3(),
                config.power,
                config.uncertainty,
                &mut self.rng,
            ));
            self.sculk.shot(pos, crate::world::vibration::EventSource::NONE);
            return true;
        }
        let lingering = stack.item == Item::LINGERING_POTION;
        let potion = if lingering { stack.lingering_contents() } else { stack.item.as_splash_potion() };
        let Some(potion) = potion else { return false };
        let vel = super::thrown::shot_movement(step.as_dvec3(), config.power, config.uncertainty, &mut self.rng) * 20.0;
        let mut thrown = super::potion::ThrownPotion::new(potion, None, pos, vel);
        thrown.lingering = lingering;
        self.potions.push(thrown);
        self.sculk.shot(pos, crate::world::vibration::EventSource::NONE);
        true
    }

    /// The armour stand behaviour: a stand in the cell in front, turned the
    /// dispenser's way (`Direction.toYRot`: south 0, west 90, north 180,
    /// east 270; up and down 270).
    pub fn dispense_armor_stand(&mut self, front: IVec3, step: IVec3) {
        let yaw = match (step.x, step.z) {
            (0, 1) => 0.0,
            (-1, 0) => 90.0,
            (0, -1) => 180.0,
            _ => 270.0,
        };
        self.decor.push(super::decor::Decor::dispensed_stand(front, yaw));
    }
}
