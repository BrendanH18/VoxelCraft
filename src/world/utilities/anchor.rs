//! Respawn anchors as a Nether respawn point (`RespawnAnchorBlock`,
//! `ServerPlayer.findRespawnAndUseSpawnBlock`): respawning at a charged
//! anchor uses a charge and stands the player beside it.

use glam::{DVec3, IVec3};

use super::{anchor_charges, respawn_anchor};
use crate::physics::BlockSource;
use crate::world::{World, dismount, terrain::Dimension};

/// `RespawnAnchorBlock.RESPAWN_HORIZONTAL_OFFSETS`, in Java's order.
const AROUND: [IVec3; 8] = [
    IVec3::new(0, 0, -1),
    IVec3::new(-1, 0, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(1, 0, 0),
    IVec3::new(-1, 0, -1),
    IVec3::new(1, 0, -1),
    IVec3::new(-1, 0, 1),
    IVec3::new(1, 0, 1),
];

/// `RESPAWN_OFFSETS`: around the anchor level, then a block lower, then a
/// block higher, then on top.
fn offsets() -> impl Iterator<Item = IVec3> {
    let level = AROUND.into_iter();
    let below = AROUND.into_iter().map(|o| o - IVec3::Y);
    let above = AROUND.into_iter().map(|o| o + IVec3::Y);
    level.chain(below).chain(above).chain([IVec3::Y])
}

/// `RespawnAnchorBlock.findStandUpPosition`: the first offset where a
/// player stands safely, then the first where one fits at all.
pub fn stand_up(world: &impl BlockSource, anchor: IVec3) -> Option<DVec3> {
    [true, false]
        .into_iter()
        .find_map(|safe| offsets().find_map(|o| dismount::find_safe_dismount_location(world, anchor + o, safe)))
}

impl World {
    /// Respawning at the anchor at `anchor`: a charged anchor in the Nether
    /// (`RespawnAnchorBlock.canSetSpawn`) with room beside it loses a charge,
    /// and the player stands there facing it. `None` sends them to the
    /// world spawn instead.
    pub fn use_anchor_respawn(&mut self, anchor: IVec3) -> Option<(DVec3, f32)> {
        if self.generator.dimension != Dimension::Nether {
            return None;
        }
        let charges = self.get_block(anchor).and_then(anchor_charges).filter(|&c| c > 0)?;
        let at = stand_up(self, anchor)?;
        self.set_block(anchor, respawn_anchor(charges - 1));
        Some((at, dismount::look_at_bed(at, anchor)))
    }
}
