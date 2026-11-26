//! Farmland's `MOISTURE` 0–7 (Java 26.3 `FarmlandBlock`,
//! `BlockStateProperties.MOISTURE`).
//!
//! A random tick sets moisture to 7 when water (any fluid state in the
//! `#water` tag, waterlogged blocks included) lies within 4 blocks across at
//! the farmland's level or one above, or when it rains on the block above.
//! Otherwise moisture falls by one per random tick, and dry farmland (0)
//! with nothing from `#maintains_farmland` above turns back to dirt. Both
//! moisture changes are `setBlock(..., 2)`. Farmland also turns to dirt
//! when a solid block that doesn't maintain it is above
//! (`updateShape` → `tick`), and when trampled (`fallOn`).
//! `CropBlock.getGrowthSpeed` treats every moisture above 0 as moist.
//!
//! Moisture 0 stays the original state 57 ([`Block::FARMLAND`]) and 7 the
//! original 58 ([`Block::WET_FARMLAND`]), so saves from before moisture
//! load as Java's dry and wet farmland. Moistures 1–6 are states
//! 13968–13973 (`FIRST + moisture - 1`, from Track 4's v1.0 reservation
//! 13968–14015); each looks and answers like dry farmland
//! ([`Block::alias`]), since Java draws `farmland_moist` only at 7.
//! Sources and differences: `docs/verification/2026-11-25-v1.0-farming-weather.md`.

use super::{
    World,
    block::{Block, RenderKind},
};
use glam::IVec3;

/// Moisture 1 is `FIRST`; moisture 6 is [`LAST`].
pub const FIRST: u16 = 13968;
pub const LAST: u16 = FIRST + 5;
/// `FarmlandBlock.MAX_MOISTURE`.
pub const MAX_MOISTURE: u8 = 7;
/// `isNearWater`: `pos.offset(-4, 0, -4)` to `pos.offset(4, 1, 4)`.
pub const WATER_REACH: i32 = 4;
/// `fallOn`: only entities with `width² × height` above this trample.
pub const TRAMPLE_VOLUME: f32 = 0.512;
/// `Level.setBlock` flags of the moisture changes (`UPDATE_CLIENTS`): no
/// neighbour updates.
pub const MOISTURE_FLAGS: u16 = 0;

/// Moistures 1..=6 (0 and 7 are the original states).
pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// Any farmland state.
pub const fn is_farmland(b: Block) -> bool {
    b.0 == Block::FARMLAND.0 || b.0 == Block::WET_FARMLAND.0 || owns(b.0)
}

/// The farmland state of `moisture` (clamped to 7).
pub const fn farmland(moisture: u8) -> Block {
    match moisture {
        0 => Block::FARMLAND,
        m if m >= MAX_MOISTURE => Block::WET_FARMLAND,
        m => Block(FIRST + m as u16 - 1),
    }
}

/// Farmland's moisture, `None` for other blocks.
pub const fn moisture(b: Block) -> Option<u8> {
    if b.0 == Block::FARMLAND.0 {
        Some(0)
    } else if b.0 == Block::WET_FARMLAND.0 {
        Some(MAX_MOISTURE)
    } else if owns(b.0) {
        Some((b.0 - FIRST) as u8 + 1)
    } else {
        None
    }
}

/// Moistures 1–6 look like dry farmland.
pub const fn registry(_id: u16, tex: [u16; 6]) -> (&'static str, RenderKind, [u16; 6]) {
    ("farmland", RenderKind::Cutout, tex)
}

/// `#maintains_farmland`: the crops, stems, torchflower (crop and flower),
/// pitcher crop, moving pistons and fence gates. Farmland under one of
/// these neither dries to dirt nor gives way to it being solid.
pub fn maintains_farmland(b: Block) -> bool {
    b.as_crop().is_some()
        || super::crops::stem(b).is_some()
        || super::sniffer_plants::torch_age(b).is_some()
        || b == super::sniffer_plants::TORCHFLOWER
        || super::sniffer_plants::pitcher_state(b).is_some()
        || b == super::redstone_blocks::MOVING
        || b.is_gate()
}

impl World {
    /// `FarmlandBlock.isNearWater`.
    pub fn farmland_near_water(&self, p: IVec3) -> bool {
        (-WATER_REACH..=WATER_REACH).any(|dx| {
            (-WATER_REACH..=WATER_REACH)
                .any(|dz| (0..=1).any(|dy| self.get_block(p + IVec3::new(dx, dy, dz)).is_some_and(Block::holds_water)))
        })
    }

    /// `FarmlandBlock.canSurvive`: not under a solid block, unless what's
    /// above maintains it.
    pub fn farmland_survives(&self, p: IVec3) -> bool {
        self.get_block(p + IVec3::Y).is_none_or(|a| !a.is_solid() || maintains_farmland(a))
    }

    /// `FarmlandBlock.randomTick`.
    pub(super) fn tick_farmland(&mut self, p: IVec3, b: Block) {
        let Some(m) = moisture(b) else { return };
        if !self.farmland_near_water(p) && !self.rains_on(p + IVec3::Y) {
            if m > 0 {
                self.edit_flags(p, farmland(m - 1), MOISTURE_FLAGS);
            } else if !self.get_block(p + IVec3::Y).is_some_and(maintains_farmland) {
                self.farmland_to_dirt(p);
            }
        } else if m < MAX_MOISTURE {
            self.edit_flags(p, farmland(MAX_MOISTURE), MOISTURE_FLAGS);
        }
    }

    /// `FarmlandBlock.turnToBaseBlock`: dirt, with whatever stood on the
    /// 15/16 top lifted onto the full block (`Block.pushEntitiesUp`), and
    /// a `block_change` game event. A crop above loses its soil and pops
    /// off with the block update.
    pub fn farmland_to_dirt(&mut self, p: IVec3) {
        if !self.get_block(p).is_some_and(is_farmland) {
            return;
        }
        self.queue_block_push(p, 15. / 16., 1.);
        self.set_block(p, Block::DIRT);
        self.emit_block_event(p, super::vibration::GameEvent::BlockChange, super::vibration::EventSource::NONE, None);
    }

    /// The shape update a change at `p` sends the farmland below it:
    /// `FarmlandBlock.updateShape` schedules a one-tick check when it can't
    /// survive, and the check turns it to dirt. VoxelCraft turns it at once.
    pub(super) fn update_farmland_below(&mut self, p: IVec3) {
        let soil = p - IVec3::Y;
        if self.get_block(soil).is_some_and(is_farmland) && !self.farmland_survives(soil) {
            self.farmland_to_dirt(soil);
        }
    }

    /// `FarmlandBlock.fallOn` for a living entity landing on the farmland at
    /// `cell` after falling `fall_distance` blocks: with chance
    /// `fall_distance - 0.5` (Java's `nextFloat() < fallDistance - 0.5`), an
    /// entity bigger than 0.512 cubic blocks tramples it to dirt. `allowed`
    /// is Java's permission: always for players (single-player has no spawn
    /// protection), `mob_griefing` for mobs. Returns whether it was trampled.
    pub fn trample_farmland(
        &mut self,
        cell: IVec3,
        fall_distance: f64,
        width: f32,
        height: f32,
        allowed: bool,
    ) -> bool {
        if !self.get_block(cell).is_some_and(is_farmland) {
            return false;
        }
        if f64::from(self.next_float()) >= fall_distance - 0.5 {
            return false;
        }
        if !allowed || width * width * height <= TRAMPLE_VOLUME {
            return false;
        }
        self.farmland_to_dirt(cell);
        true
    }

    /// `fallOn` for a player whose feet (`pos`) landed `landed` blocks
    /// below where the fall began, on the block 0.1 under them. Returns
    /// whether farmland was trampled.
    pub fn player_landed(&mut self, pos: glam::DVec3, shape: crate::physics::Shape, landed: f64) -> bool {
        if landed <= 0.5 {
            return false;
        }
        let cell = (pos - glam::DVec3::Y * 0.1).floor().as_ivec3();
        let width = (shape.half_width * 2.0) as f32;
        self.trample_farmland(cell, landed, width, shape.height as f32, true)
    }
}

#[cfg(test)]
mod tests;
