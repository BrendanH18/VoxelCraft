//! v1.0 Track 5: Soul Speed, as Java 26.3's `enchantment/soul_speed.json`
//! defines it on the feet slot.
//!
//! - `location_changed` (Java's `LivingEntity.onChangedBlock`: the block
//!   position changed, or a fall landed): while the wearer isn't riding or
//!   flying, standing on `#soul_speed_blocks` (soul sand and soul soil, the
//!   block that affects its movement) activates the effect; once active it
//!   stays active on those blocks and in the air, and goes on the first
//!   block change anywhere else. While active the wearer has
//!   `movement_speed` + 0.0405 + 0.0105 per level past the first and
//!   `movement_efficiency` + 1, so soul sand's 0.4 speed factor becomes 1.
//! - The same check, on the ground on those blocks, wears the boots one
//!   point with chance 0.04 per level (`change_item_damage`, through
//!   Unbreaking); [`crate::session::PlayerSession::tick_soul_speed`] applies
//!   the wear.
//! - `tick`: every fifth tick on the ground on those blocks, moving and not
//!   flying, one `soul` particle and, with chance 0.35, the
//!   `particle.soul_escape` sound (also in `tick_soul_speed`).
//!
//! The activation isn't saved: Java's attribute modifiers are transient,
//! and the next block change decides again.

use glam::{DVec3, IVec3};

use crate::world::block::Block;

/// `#minecraft:soul_speed_blocks`.
pub fn soul_speed_block(b: Option<Block>) -> bool {
    matches!(b, Some(Block::SOUL_SAND | crate::world::nether_biome_blocks::SOUL_SOIL))
}

/// The `movement_speed` `add_value` of an active level: `linear` with base
/// 0.0405 and 0.0105 per level past the first.
pub fn speed_bonus(level: u8) -> f32 {
    if level == 0 { 0.0 } else { 0.0405 + 0.0105 * f32::from(level - 1) }
}

/// The boots' wear chance per location change (`enchantment_level` times
/// 0.04).
pub fn wear_chance(level: u8) -> f32 {
    0.04 * f32::from(level)
}

/// The wearer's Soul Speed state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoulSpeed {
    /// Level on the worn boots (0: none).
    pub level: u8,
    /// Java's active location-dependent effect: the speed and efficiency
    /// modifiers apply.
    pub active: bool,
    /// The block position at the last location change (`lastPos`).
    cell: Option<IVec3>,
    /// Boot wear rolled since the session last took it.
    pub wear: u8,
    /// The last tick's movement in blocks (`getKnownMovement`).
    pub moved: DVec3,
}

impl SoulSpeed {
    /// The boots changed (`runLocationChangedEffects` for the slot): a new
    /// level decides again at the next tick; none removes the effect.
    pub fn set_level(&mut self, level: u8) {
        if level != self.level {
            self.level = level;
            self.cell = None;
            if level == 0 {
                self.active = false;
            }
        }
    }

    /// The modifiers' `movement_speed` bonus now.
    pub fn speed_add(&self) -> f32 {
        if self.active { speed_bonus(self.level) } else { 0.0 }
    }

    /// Whether the location changed: a new block position (always, once
    /// the boots changed).
    pub fn moved_to(&mut self, cell: IVec3) -> bool {
        let changed = self.cell != Some(cell);
        self.cell = Some(cell);
        changed
    }

    /// `location_changed`: `on_soul` is whether the block that affects the
    /// wearer's movement is a soul speed block, `roll` a uniform 0..1 draw
    /// for the wear.
    pub fn location_changed(&mut self, on_soul: bool, on_ground: bool, riding: bool, flying: bool, roll: f32) {
        if self.level == 0 {
            self.active = false;
            return;
        }
        let holds = if self.active { on_soul || !on_ground } else { on_soul };
        self.active = !riding && !flying && holds;
        if on_ground && on_soul && roll < wear_chance(self.level) {
            self.wear = self.wear.saturating_add(1);
        }
    }

    /// The `tick` effects' requirement: on the ground on a soul speed
    /// block, moving (horizontal speed at least 1e-5) and not flying, on
    /// every fifth tick (`periodic_tick`).
    pub fn emits(&self, tick: u32, on_soul: bool, on_ground: bool, flying: bool) -> bool {
        self.level > 0
            && tick.is_multiple_of(5)
            && on_soul
            && on_ground
            && !flying
            && self.moved.with_y(0.0).length() >= 9.999_999_747_378_752e-6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javas_speed_bonus_and_wear_chance() {
        assert_eq!(speed_bonus(0), 0.0);
        assert!((speed_bonus(1) - 0.0405).abs() < 1e-7);
        assert!((speed_bonus(2) - 0.051).abs() < 1e-7);
        assert!((speed_bonus(3) - 0.0615).abs() < 1e-7);
        assert!((wear_chance(3) - 0.12).abs() < 1e-7);
    }

    #[test]
    fn activation_needs_soul_blocks_and_lasts_through_a_jump() {
        let mut s = SoulSpeed::default();
        s.set_level(2);
        assert!(s.moved_to(IVec3::ZERO) && !s.moved_to(IVec3::ZERO));
        // Off soul blocks it doesn't start, on the ground or in the air.
        s.location_changed(false, true, false, false, 1.0);
        s.location_changed(false, false, false, false, 1.0);
        assert!(!s.active);
        s.location_changed(true, true, false, false, 1.0);
        assert!(s.active);
        assert!((s.speed_add() - 0.051).abs() < 1e-7);
        // Active: kept in the air, even over other blocks.
        s.location_changed(false, false, false, false, 1.0);
        assert!(s.active);
        // Lost on the ground elsewhere, while riding or flying.
        s.location_changed(false, true, false, false, 1.0);
        assert!(!s.active);
        s.location_changed(true, true, true, false, 1.0);
        assert!(!s.active);
        s.location_changed(true, true, false, true, 1.0);
        assert!(!s.active);
        assert_eq!(s.speed_add(), 0.0);
        // Taking the boots off ends it at once.
        s.location_changed(true, true, false, false, 1.0);
        s.set_level(0);
        assert!(!s.active && s.moved_to(IVec3::ZERO));
    }

    #[test]
    fn boots_wear_only_on_the_ground_on_soul_blocks() {
        let mut s = SoulSpeed::default();
        s.set_level(1);
        s.location_changed(true, true, false, false, 0.039);
        s.location_changed(true, true, false, false, 0.041);
        s.location_changed(true, false, false, false, 0.0);
        s.location_changed(false, true, false, false, 0.0);
        assert_eq!(s.wear, 1);
        // Riding or flying stops the speed, not the wear.
        s.location_changed(true, true, true, false, 0.0);
        assert_eq!(s.wear, 2);
    }

    #[test]
    fn particles_every_fifth_tick_while_moving_on_soul_blocks() {
        let mut s = SoulSpeed::default();
        s.set_level(1);
        s.moved = DVec3::new(0.1, -0.2, 0.0);
        assert!(s.emits(10, true, true, false));
        assert!(!s.emits(11, true, true, false));
        assert!(!s.emits(10, false, true, false));
        assert!(!s.emits(10, true, false, false));
        assert!(!s.emits(10, true, true, true));
        s.moved = DVec3::new(0.0, -0.2, 0.0);
        assert!(!s.emits(10, true, true, false), "falling isn't moving");
    }
}
