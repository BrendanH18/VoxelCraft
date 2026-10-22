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
    /// Start drawing a held bow or crossbow, or fire a charged crossbow.
    /// Aiming at a block that takes right-clicks defers to it.
    /// Whether using `slot` would start drawing a bow or loading a
    /// crossbow: one is held, an arrow is available (or Creative), and no
    /// targeted block takes the use first.
    pub fn can_draw(&self, slot: usize, world: &World) -> bool {
        let held = self.inventory.get(slot).map(|s| s.item);
        let has_arrow = self.mode.is_creative() || self.inventory.find(Item::ARROW).is_some();
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
        let infinite = enchants.has(Enchantment::Infinity);
        let mut broken = None;
        if survival {
            let arrow = self.inventory.find(Item::ARROW)?;
            if !infinite {
                self.inventory.take_one(arrow);
            }
            if self.inventory.wear(slot, 1) {
                broken = Some(Item::BOW);
            }
        }
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        entities.shoot_enchanted_for(eye, dir, power, survival && !infinite, enchants, self.id);
        if survival {
            self.vitals.hunger.exhaust(survival::EXHAUST_ATTACK);
        }
        Some(self.shot(Ranged::ArrowShot { from: eye, power }, broken))
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
        if self.mode.is_survival() {
            let arrow = self.inventory.find(Item::ARROW)?;
            self.inventory.take_one(arrow);
        }
        if let Some(stack) = &mut self.inventory.slots[slot] {
            stack.item = Item::CHARGED_CROSSBOW;
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
        let piercing = Enchants::NONE.with(Enchantment::Piercing, held.level(Enchantment::Piercing));
        if let Some(stack) = &mut self.inventory.slots[slot] {
            stack.item = Item::CROSSBOW;
        }
        let mut broken = None;
        for (dir, real) in crossbow_volley(held, forward) {
            entities.shoot_enchanted_for(eye, dir, CROSSBOW_POWER, survival && real, piercing, self.id);
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
}
