//! Sugar cane's `AGE` (Java 26.3 `SugarCaneBlock`, `BlockStateProperties.AGE_15`).
//!
//! A cane block with air above, in a stalk shorter than three, ages one
//! step per random tick; at age 15 the next random tick puts a new cane
//! (age 0) above and resets its own age to 0. A block therefore grows after
//! exactly 16 random ticks, not by chance. The age changes are
//! `setBlock(..., 260)`: no neighbour updates, but shape updates still go
//! out, so an observer facing the cane sees every step. `randomTick` doesn't
//! look at water (survival is `canSurvive`, checked on shape updates).
//!
//! Age 0 stays the original state 92 ([`Block::SUGAR_CANE`]); ages 1–15
//! are states 13953–13967 (`FIRST + age`, from the v0.17 integrator's
//! reservation; 13952 stays unregistered). Every aged state answers as
//! sugar cane for its properties ([`Block::alias`]), drops, item and name.

use super::{
    World,
    block::{Block, RenderKind},
    redstone_order::flags,
};
use glam::IVec3;

/// `FIRST + age` for ages 1..=15; `FIRST` itself is unused.
pub const FIRST: u16 = 13952;
pub const LAST: u16 = FIRST + MAX_AGE as u16;
/// `SugarCaneBlock.AGE` (`AGE_15`).
pub const MAX_AGE: u8 = 15;
/// `SugarCaneBlock.MAX_HEIGHT`: a stalk grows while shorter than this.
pub const MAX_HEIGHT: i32 = 3;
/// `Level.setBlock` flags for the age change (Java 260:
/// `UPDATE_INVISIBLE | UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS`, neither of
/// which VoxelCraft models): shape updates only.
pub const AGE_FLAGS: u16 = 0;

/// Ages 1..=15 (age 0 is [`Block::SUGAR_CANE`]).
pub const fn owns(id: u16) -> bool {
    id > FIRST && id <= LAST
}

/// Any sugar cane state.
pub const fn is_cane(b: Block) -> bool {
    b.0 == Block::SUGAR_CANE.0 || owns(b.0)
}

/// The sugar cane state of `age` (clamped to 15).
pub const fn cane(age: u8) -> Block {
    match age {
        0 => Block::SUGAR_CANE,
        a => Block(FIRST + if a > MAX_AGE { MAX_AGE } else { a } as u16),
    }
}

/// The cane's age, `None` for other blocks.
pub const fn age(b: Block) -> Option<u8> {
    if b.0 == Block::SUGAR_CANE.0 {
        Some(0)
    } else if owns(b.0) {
        Some((b.0 - FIRST) as u8)
    } else {
        None
    }
}

/// Every age looks like the age-0 cane.
pub const fn registry(_id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    ("sugar cane", RenderKind::Cross, [super::block::tex::SUGAR_CANE; 6])
}

impl World {
    /// `SugarCaneBlock.randomTick` for the cane at `p`.
    pub(super) fn tick_cane(&mut self, p: IVec3, b: Block) {
        let Some(age) = age(b) else { return };
        if self.get_block(p + IVec3::Y) != Some(Block::AIR) {
            return;
        }
        let mut height = 1;
        while self.get_block(p - IVec3::Y * height).is_some_and(is_cane) {
            height += 1;
        }
        if height >= MAX_HEIGHT {
            return;
        }
        if age == MAX_AGE {
            self.edit_flags(p + IVec3::Y, Block::SUGAR_CANE, flags::UPDATE_ALL);
            self.edit_flags(p, cane(0), AGE_FLAGS);
        } else {
            self.edit_flags(p, cane(age + 1), AGE_FLAGS);
        }
    }

    /// Whether sugar cane at `p` (its lowest block) has water beside the
    /// block it's planted on.
    pub fn cane_has_water(&self, p: IVec3) -> bool {
        let soil = p - IVec3::Y;
        [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z].iter().any(|&d| {
            self.get_block(soil + d)
                .is_some_and(|b| b.holds_water() || b == Block::ICE || super::frosted_ice::owns(b.0))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;

    #[test]
    fn aged_cane_states_answer_as_sugar_cane() {
        assert_eq!(cane(0), Block::SUGAR_CANE);
        assert_eq!((cane(1).0, cane(15).0), (13953, 13967));
        assert!(!owns(FIRST) && !is_cane(Block(FIRST)) && !owns(LAST + 1));
        for a in 0..=MAX_AGE {
            let b = cane(a);
            assert_eq!(age(b), Some(a));
            assert!(is_cane(b));
            assert_eq!(b.name(), "sugar cane");
            assert_eq!(b.kind(), RenderKind::Cross);
            assert_eq!(b.base(), Block::SUGAR_CANE);
            assert_eq!(b.drop(), Some(Item::from_block(Block::SUGAR_CANE)));
            assert_eq!(b.hardness(), Block::SUGAR_CANE.hardness());
            assert_eq!(b.light_opacity(), Block::SUGAR_CANE.light_opacity());
            assert!(b.can_stay_on(Block::SAND) && b.can_stay_on(cane(7)) && !b.can_stay_on(Block::STONE));
            assert_eq!(crate::session::interact::picked_item(b), Item::from_block(Block::SUGAR_CANE));
        }
        assert_eq!(Block::from_name("sugar cane"), Some(Block::SUGAR_CANE));
    }

    /// Cane on sand beside water in a flat test world, `height` blocks tall.
    fn stalk(height: i32) -> (World, IVec3) {
        let mut world = crate::world::lighting::flat_test_world(
            (-1..=1).flat_map(|x| (-1..=1).map(move |z| glam::IVec2::new(x, z))),
            63,
        );
        let base = IVec3::new(16, 65, 16);
        world.set_block(base - IVec3::Y, Block::SAND);
        world.set_block(base - IVec3::Y + IVec3::X, Block::WATER);
        for y in 0..height {
            world.set_block(base + IVec3::Y * y, Block::SUGAR_CANE);
        }
        (world, base)
    }

    #[test]
    fn a_block_grows_on_the_sixteenth_random_tick_up_to_three_tall() {
        let (mut world, base) = stalk(1);
        for n in 1..=15 {
            world.random_tick(base);
            assert_eq!(world.get_block(base), Some(cane(n)), "tick {n}");
            assert_eq!(world.get_block(base + IVec3::Y), Some(Block::AIR));
        }
        world.random_tick(base);
        assert_eq!(world.get_block(base), Some(cane(0)), "the age resets");
        assert_eq!(world.get_block(base + IVec3::Y), Some(Block::SUGAR_CANE));
        // Covered now, the lower block no longer ages; the new top does.
        world.random_tick(base);
        assert_eq!(world.get_block(base), Some(cane(0)));
        let top = base + IVec3::Y;
        for _ in 0..16 {
            world.random_tick(top);
        }
        assert_eq!(world.get_block(top + IVec3::Y), Some(Block::SUGAR_CANE));
        // Three tall: the top neither ages nor grows.
        for _ in 0..40 {
            world.random_tick(top + IVec3::Y);
        }
        assert_eq!(world.get_block(top + IVec3::Y), Some(cane(0)));
        assert_eq!(world.get_block(top + IVec3::Y * 2), Some(Block::AIR));
    }

    #[test]
    fn random_ticks_age_cane_whatever_its_water_but_not_under_a_block() {
        // Java's `randomTick` doesn't look at water; `canSurvive` does, on
        // shape updates.
        let (mut world, base) = stalk(1);
        world.set_block(base - IVec3::Y + IVec3::X, Block::STONE);
        world.random_tick(base);
        assert_eq!(world.get_block(base), Some(cane(1)));
        world.set_block(base + IVec3::Y, Block::STONE);
        world.random_tick(base);
        assert_eq!(world.get_block(base), Some(cane(1)));
    }

    #[test]
    fn growth_constants_match_java() {
        let java: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/growth.json")).unwrap();
        let cane = &java["sugar_cane"];
        assert_eq!(cane["age_max"], u64::from(MAX_AGE));
        assert_eq!(cane["grows_at_age"], u64::from(MAX_AGE));
        assert_eq!(cane["max_height"], MAX_HEIGHT as u64);
        // 260 is `UPDATE_INVISIBLE | UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS`:
        // no neighbour updates (VoxelCraft's `AGE_FLAGS` 0), shape updates on.
        assert_eq!(cane["age_set_block_flags"], 260);
        assert_eq!(AGE_FLAGS & (flags::UPDATE_NEIGHBORS | flags::UPDATE_KNOWN_SHAPE), 0);
        assert_eq!(cane["checks_water"], false);
        let stem = &java["stem"];
        assert_eq!(stem["min_raw_brightness"], u64::from(super::super::crops::MIN_LIGHT));
        assert_eq!(stem["growth_numerator"].as_f64(), Some(f64::from(super::super::crops::GROWTH_NUMERATOR)));
    }
}
