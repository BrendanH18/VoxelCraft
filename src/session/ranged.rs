//! Actor-owned bow drawing and crossbow loading, in fixed ticks.
//!
//! Hold use to draw a bow and release to shoot; hold use until a crossbow
//! loads, then use again to fire. Power follows Java's
//! `BowItem.getPowerForTime`.
use super::PlayerSession;
use crate::{
    enchant::{Enchantment, Enchants},
    entity::{Entities, PlayerId},
    item::Item,
    simulation::survival,
    world::World,
};
use glam::DVec3;

/// Ticks to draw a bow fully.
pub const FULL_DRAW_TICKS: u32 = 20;
/// Ticks to load a crossbow (Java's 25 without Quick Charge).
pub const CROSSBOW_CHARGE_TICKS: u32 = 25;
/// Multishot's spread between neighbouring arrows, in degrees.
const MULTISHOT_SPREAD: f32 = 10.0;
/// Weaker draws than this don't shoot.
const MIN_POWER: f32 = 0.1;
/// A crossbow bolt leaves at 3.15 blocks a tick against a full bow's 3.0.
const CROSSBOW_POWER: f32 = 3.15 / 3.0;

/// Bow power 0..1 after drawing for `ticks`.
pub fn power(ticks: u32) -> f32 {
    let f = ticks as f32 / FULL_DRAW_TICKS as f32;
    ((f * f + 2.0 * f) / 3.0).min(1.0)
}

/// Java's `CrossbowItem.getChargeDuration`: 1.25 seconds less 0.25 per Quick
/// Charge level (`crossbow_charge_time`), times 20, floored.
pub fn crossbow_charge_ticks(enchants: Enchants) -> u32 {
    let seconds = (1.25f32 - 0.25 * f32::from(enchants.level(Enchantment::QuickCharge))).max(0.0);
    (seconds * 20.0).floor() as u32
}

/// The directions a crossbow shot leaves in (`ProjectileWeaponItem.shoot`):
/// one arrow straight ahead, or with Multishot (`projectile_count` +2, spread
/// 10 degrees) the middle arrow then one to each side, turned about the
/// shooter's up vector. Returns the direction and whether the arrow can be picked up.
pub fn crossbow_volley(enchants: Enchants, forward: DVec3) -> Vec<(DVec3, bool)> {
    let count = if enchants.has(Enchantment::Multishot) { 3 } else { 1 };
    let spread = if count == 1 { 0.0 } else { MULTISHOT_SPREAD };
    let step = if count == 1 { 0.0 } else { 2.0 * spread / (count - 1) as f32 };
    let start = ((count - 1) % 2) as f32 * step / 2.0;
    let right = forward.cross(DVec3::Y).normalize_or_zero();
    let up = if right == DVec3::ZERO { DVec3::X } else { right.cross(forward).normalize_or_zero() };
    let mut sign = 1.0f32;
    (0..count)
        .map(|i| {
            let angle = start + sign * ((i + 1) / 2) as f32 * step;
            sign = -sign;
            // Rodrigues' rotation about `up`; `forward` is perpendicular to it.
            let theta = f64::from(angle).to_radians();
            let turned = forward * theta.cos() + up.cross(forward) * theta.sin();
            // Only the first arrow is a real ammunition item; the others are intangible.
            (turned.normalize_or_zero(), i == 0)
        })
        .collect()
}

#[derive(Default)]
pub(super) struct State {
    /// The hotbar slot drawing and ticks drawn. Selecting another slot lets go.
    draw: Option<(usize, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ranged {
    Drawing,
    CrossbowLoaded { from: DVec3 },
    CrossbowFired { from: DVec3 },
    ArrowShot { from: DVec3, power: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub player: PlayerId,
    pub ranged: Ranged,
    /// The bow or crossbow broke.
    pub tool_broken: Option<Item>,
}

impl PlayerSession {
    /// Java prefers projectiles held in the offhand, then inventory order.
    fn arrow_ammo(&self) -> Option<(Option<usize>, crate::inventory::Stack)> {
        let is_arrow = |s: crate::inventory::Stack| matches!(s.item, Item::ARROW | Item::TIPPED_ARROW);
        if let Some(s) = self.inventory.offhand.filter(|s| is_arrow(*s)) {
            return Some((None, s));
        }
        self.inventory.slots.iter().enumerate().find_map(|(i, s)| s.filter(|s| is_arrow(*s)).map(|s| (Some(i), s)))
    }
    fn take_arrow(&mut self) -> Option<crate::inventory::Stack> {
        let (slot, stack) = self.arrow_ammo()?;
        if let Some(i) = slot {
            self.inventory.take_one(i);
        } else {
            let mut s = stack;
            s.count -= 1;
            self.inventory.offhand = (s.count > 0).then_some(s);
        }
        Some(stack)
    }
    /// Start drawing a held bow or crossbow, or fire a charged crossbow.
    /// Aiming at a block that takes right-clicks defers to it.
    /// Whether using `slot` would start drawing a bow or loading a
    /// crossbow: one is held, an arrow is available (or Creative), and no
    /// targeted block takes the use first.
    pub fn can_draw(&self, slot: usize, world: &World) -> bool {
        let held = self.inventory.get(slot).map(|s| s.item);
        let has_arrow = self.mode.is_creative() || self.arrow_ammo().is_some();
        self.mode.can_interact()
            && !self.vitals.is_dead()
            && self.sleeping.is_none()
            && matches!(held, Some(Item::BOW | Item::CROSSBOW))
            && has_arrow
            && !self.block_takes_use(slot, world, self.player.sneaking)
    }

    pub fn start_draw(&mut self, slot: usize, world: &World, entities: &mut Entities) -> Option<Shot> {
        if !self.mode.can_interact() || self.vitals.is_dead() || self.sleeping.is_some() {
            return None;
        }
        if self.block_takes_use(slot, world, self.player.sneaking) {
            return None;
        }
        let held = self.inventory.get(slot).map(|s| s.item);
        if held == Some(Item::CHARGED_CROSSBOW) {
            return Some(self.fire_crossbow(slot, entities));
        }
        if !self.can_draw(slot, world) {
            return None;
        }
        self.ranged.draw = Some((slot, 0));
        Some(self.shot(Ranged::Drawing, None))
    }

    /// One fixed tick of holding use. Switching away from the bow lets the
    /// string go without shooting; a crossbow loads when fully drawn.
    pub fn tick_draw(&mut self, slot: usize, using: bool) -> Option<Shot> {
        let (drawn_slot, ticks) = self.ranged.draw?;
        let held = self.inventory.get(slot).map(|s| s.item);
        if !using || drawn_slot != slot || !matches!(held, Some(Item::BOW | Item::CROSSBOW)) {
            self.ranged.draw = None;
            return None;
        }
        let charge = crossbow_charge_ticks(self.inventory.get(slot).map_or(Enchants::NONE, |s| s.active_enchants()));
        if held == Some(Item::CROSSBOW) && ticks + 1 >= charge {
            self.ranged.draw = None;
            return self.load_crossbow(slot);
        }
        self.ranged.draw = Some((slot, ticks + 1));
        None
    }

    /// Releasing use looses an arrow if the bow was drawn enough. Letting go
    /// of a crossbow before it loads just lowers it.
    pub fn release_draw(&mut self, slot: usize, entities: &mut Entities) -> Option<Shot> {
        let (drawn_slot, ticks) = self.ranged.draw.take()?;
        let power = power(ticks);
        if drawn_slot != slot || power < MIN_POWER || self.inventory.get(slot).map(|s| s.item) != Some(Item::BOW) {
            return None;
        }
        let survival = self.mode.is_survival();
        let enchants = self.inventory.get(slot).map_or(Default::default(), |s| s.enchants);
        // Infinity needs one arrow but never uses it up (the shot can't be
        // picked up, like Creative's).
        let ammo = self.arrow_ammo().map(|(_, s)| s);
        let potion = ammo.and_then(|s| s.arrow_potion());
        let infinite = enchants.has(Enchantment::Infinity) && ammo.is_some_and(|s| s.item == Item::ARROW);
        let mut broken = None;
        if survival {
            self.arrow_ammo()?;
            if !infinite {
                self.take_arrow()?;
            }
            if self.inventory.wear(slot, 1) {
                broken = Some(Item::BOW);
            }
        }
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        entities.shoot_enchanted_for(eye, dir, power, survival && !infinite, enchants, self.id);
        entities.arrows.last_mut().unwrap().potion = potion;
        entities.arrows.last_mut().unwrap().tipped = ammo.is_some_and(|s| s.item == Item::TIPPED_ARROW);
        if survival {
            self.vitals.hunger.exhaust(survival::EXHAUST_ATTACK);
        }
        Some(self.shot(Ranged::ArrowShot { from: eye, power }, broken))
    }

    /// The hotbar slot whose bow or crossbow is being drawn, if any.
    pub fn drawing_slot(&self) -> Option<usize> {
        self.ranged.draw.map(|(slot, _)| slot)
    }

    /// Ticks the held bow or crossbow has been drawn, while drawing.
    pub fn draw_ticks(&self) -> Option<u32> {
        self.ranged.draw.map(|(_, ticks)| ticks)
    }

    pub fn drawing(&self) -> bool {
        self.ranged.draw.is_some()
    }

    pub fn cancel_draw(&mut self) {
        self.ranged.draw = None;
    }

    /// Current bow draw strength for the HUD and field of view. Loading a
    /// crossbow doesn't zoom the view like a drawn bow.
    pub fn draw_power(&self, slot: usize) -> Option<f32> {
        let (drawn_slot, ticks) = self.ranged.draw?;
        (drawn_slot == slot && self.inventory.get(slot).map(|s| s.item) == Some(Item::BOW)).then(|| power(ticks))
    }

    /// A loaded crossbow holds its bolt: the held item becomes the charged
    /// crossbow (keeping wear and enchantments) and an arrow is used up.
    fn load_crossbow(&mut self, slot: usize) -> Option<Shot> {
        let ammo = self.arrow_ammo().map(|(_, s)| s);
        let potion = ammo.and_then(|s| s.arrow_potion());
        if self.mode.is_survival() {
            self.take_arrow()?;
        }
        if let Some(stack) = &mut self.inventory.slots[slot] {
            stack.item = Item::CHARGED_CROSSBOW;
            stack.entity_data = potion.map_or_else(
                || if ammo.is_some_and(|s| s.item == Item::TIPPED_ARROW) { u32::MAX } else { 0 },
                |p| u32::from(p.0) + 1,
            );
        }
        Some(self.shot(Ranged::CrossbowLoaded { from: self.player.eye() }, None))
    }

    /// Shoots the bolt in a charged crossbow, which then needs loading again.
    /// Multishot looses three arrows and every arrow wears the crossbow by one
    /// (`ProjectileWeaponItem.shoot`); Piercing lets each pass through mobs.
    fn fire_crossbow(&mut self, slot: usize, entities: &mut Entities) -> Shot {
        let survival = self.mode.is_survival();
        let (eye, forward) = (self.player.eye(), self.player.forward().as_dvec3());
        let held = self.inventory.get(slot).map_or(Enchants::NONE, |s| s.active_enchants());
        let tipped = self.inventory.get(slot).is_some_and(|s| s.entity_data != 0);
        let potion = self.inventory.get(slot).and_then(|s| s.arrow_potion());
        let piercing = Enchants::NONE.with(Enchantment::Piercing, held.level(Enchantment::Piercing));
        if let Some(stack) = &mut self.inventory.slots[slot] {
            stack.item = Item::CROSSBOW;
            stack.entity_data = 0;
        }
        let mut broken = None;
        for (dir, real) in crossbow_volley(held, forward) {
            entities.shoot_enchanted_for(eye, dir, CROSSBOW_POWER, survival && real, piercing, self.id);
            entities.arrows.last_mut().unwrap().potion = potion;
            entities.arrows.last_mut().unwrap().tipped = tipped;
            if survival && self.inventory.wear(slot, 1) {
                broken = Some(Item::CROSSBOW);
                break;
            }
        }
        self.shot(Ranged::CrossbowFired { from: eye }, broken)
    }

    fn shot(&self, ranged: Ranged, tool_broken: Option<Item>) -> Shot {
        Shot { player: self.id, ranged, tool_broken }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_rises_to_full_after_twenty_ticks() {
        assert_eq!(power(0), 0.0);
        assert!(power(1) < MIN_POWER, "a tap doesn't shoot");
        assert!((power(10) - 0.4167).abs() < 1e-3);
        assert_eq!(power(FULL_DRAW_TICKS), 1.0);
        assert_eq!(power(40), 1.0);
    }

    #[test]
    fn quick_charge_shortens_loading_by_five_ticks_a_level() {
        let ticks: Vec<_> =
            (0..=3).map(|l| crossbow_charge_ticks(Enchants::NONE.with(Enchantment::QuickCharge, l))).collect();
        assert_eq!(ticks, [25, 20, 15, 10]);
    }

    #[test]
    fn multishot_turns_two_extra_arrows_ten_degrees_about_the_up_vector() {
        let forward = DVec3::new(0.6, 0.3, 0.74).normalize();
        let single = crossbow_volley(Enchants::NONE, forward);
        assert_eq!(single.len(), 1);
        assert!((single[0].0 - forward).length() < 1e-12 && single[0].1);
        let volley = crossbow_volley(Enchants::NONE.with(Enchantment::Multishot, 1), forward);
        assert_eq!(volley.len(), 3);
        // The first arrow is straight and the only recoverable one; the others turn +-10 degrees.
        assert!((volley[0].0 - forward).length() < 1e-12);
        assert_eq!(volley.iter().map(|v| v.1).collect::<Vec<_>>(), [true, false, false]);
        for (dir, _) in &volley[1..] {
            let angle = dir.dot(forward).clamp(-1.0, 1.0).acos().to_degrees();
            assert!((angle - 10.0).abs() < 1e-9, "{angle}");
        }
        // The side arrows lie on opposite sides and keep the aim's pitch plane (about the up vector).
        let right = forward.cross(DVec3::Y).normalize();
        let up = right.cross(forward);
        assert!(volley[1].0.dot(up).abs() < 1e-9 && volley[2].0.dot(up).abs() < 1e-9);
        assert!(volley[1].0.dot(right) * volley[2].0.dot(right) < 0.0);
    }

    #[test]
    fn crossbows_keep_their_wear_when_loaded_and_take_java_durability() {
        assert_eq!(Item::CROSSBOW.durability(), Some(465));
        assert_eq!(Item::CHARGED_CROSSBOW.durability(), Some(465));
        assert_eq!(Item::CROSSBOW.max_stack(), 1);
        assert_eq!(Item::from_name("crossbow"), Some(Item::CROSSBOW));
        assert_ne!(Item::CROSSBOW.icon_layer(), Item::CHARGED_CROSSBOW.icon_layer());
        const { assert!(CROSSBOW_POWER > 1.0 && CROSSBOW_CHARGE_TICKS == 25) };
    }
    #[test]
    fn tipped_bow_ammo_is_consumed_with_infinity_and_keeps_its_potion() {
        use crate::{inventory::Stack, potion::Potion};
        let mut s = PlayerSession::new(PlayerId(12), DVec3::new(0., 140., 0.));
        let mut bow = Stack::new(Item::BOW, 1);
        bow.enchants = Enchants::NONE.with(Enchantment::Infinity, 1);
        s.inventory.slots[0] = Some(bow);
        let p = Potion::from_id("long_poison").unwrap();
        s.inventory.slots[1] = Some(Stack::tipped_arrow(p, 2));
        s.ranged.draw = Some((0, 20));
        let mut e = Entities::new(8);
        assert!(s.release_draw(0, &mut e).is_some());
        assert_eq!(s.inventory.get(1).unwrap().count, 1);
        assert_eq!(e.arrows[0].potion, Some(p));
        assert!(e.arrows[0].pickup);
        assert_eq!(e.arrows[0].owner, Some(PlayerId(12)));
    }
    #[test]
    fn charged_multishot_crossbow_saves_its_tipped_bolt_and_clears_it_when_fired() {
        use crate::{
            inventory::{Inventory, Stack},
            potion::Potion,
        };
        let mut s = PlayerSession::new(PlayerId(13), DVec3::new(0., 140., 0.));
        let mut bow = Stack::new(Item::CROSSBOW, 1);
        bow.enchants = Enchants::NONE.with(Enchantment::Multishot, 1);
        bow.damage = 17;
        s.inventory.slots[0] = Some(bow);
        let p = Potion::from_id("slowness").unwrap();
        s.inventory.slots[1] = Some(Stack::tipped_arrow(p, 2));
        s.ranged.draw = Some((0, 24));
        assert!(s.tick_draw(0, true).is_some());
        assert_eq!(s.inventory.get(1).unwrap().count, 1);
        s.inventory = Inventory::deserialize(&s.inventory.serialize()).unwrap();
        assert_eq!(s.inventory.get(0).unwrap().arrow_potion(), Some(p));
        let mut e = Entities::new(8);
        s.fire_crossbow(0, &mut e);
        assert_eq!(e.arrows.len(), 3);
        assert!(e.arrows.iter().all(|a| a.potion == Some(p)));
        assert_eq!(e.arrows.iter().filter(|a| a.pickup).count(), 1);
        assert_eq!(s.inventory.get(0).unwrap().entity_data, 0);
        assert_eq!(s.inventory.get(0).unwrap().damage, 20);
    }
    #[test]
    fn offhand_tipped_ammunition_has_priority_over_inventory_arrows() {
        use crate::{inventory::Stack, potion::Potion};
        let mut s = PlayerSession::new(PlayerId::HOST, DVec3::new(0., 140., 0.));
        s.inventory.slots[0] = Some(Stack::new(Item::BOW, 1));
        s.inventory.slots[1] = Some(Stack::new(Item::ARROW, 8));
        let p = Potion::from_id("poison").unwrap();
        s.inventory.offhand = Some(Stack::tipped_arrow(p, 1));
        s.ranged.draw = Some((0, 20));
        let mut e = Entities::new(8);
        assert!(s.release_draw(0, &mut e).is_some());
        assert!(s.inventory.offhand.is_none());
        assert_eq!(s.inventory.get(1).unwrap().count, 8);
        assert_eq!(e.arrows[0].potion, Some(p));
    }
}
