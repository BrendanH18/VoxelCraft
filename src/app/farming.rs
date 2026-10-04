//! Game-side farming: tilling with a hoe, bone meal, and trampling
//! farmland. Growth itself happens in `world::growth`.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::item::{Item, ToolKind};
use crate::world::block::Block;

use super::{Game, GameMode};

impl Game {
    /// Right-click with a hoe on grass or dirt tills it into farmland;
    /// bone meal makes crops, saplings and grass grow. Returns whether the
    /// held item was used.
    pub(super) fn use_item_on(&mut self, pos: IVec3, normal: IVec3) -> bool {
        let Some(item) = self.held_item() else { return false };
        let Some(block) = self.world.get_block(pos) else { return false };
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        if item.as_tool().is_some_and(|(kind, _)| kind == ToolKind::Hoe) {
            let tillable = matches!(block, Block::GRASS | Block::DIRT)
                && normal != IVec3::NEG_Y
                && self.world.get_block(pos + IVec3::Y) == Some(Block::AIR);
            if tillable && self.world.set_block(pos, Block::FARMLAND) {
                self.audio.play(Sound::Step(Material::Gravel), Some(at), 1.0, (0.8, 0.9));
                self.wear_held(false);
                return true;
            }
            return false;
        }
        if item == Item::BONE_MEAL && self.world.apply_bone_meal(pos) {
            self.audio.play(Sound::Place(Material::Grass), Some(at), 0.8, (1.2, 1.4));
            if self.mode == GameMode::Survival {
                self.inventory.take_one(self.actions.selected);
            }
            return true;
        }
        false
    }

    /// Landing on farmland from more than half a block may trample it back
    /// to dirt (the chance grows with the fall), popping off its crop.
    pub(super) fn trample(&mut self, fallen: f64) {
        if fallen <= 0.5 {
            return;
        }
        let below = (self.player.pos - DVec3::Y * 0.1).floor().as_ivec3();
        if !self.world.get_block(below).is_some_and(Block::is_farmland) {
            return;
        }
        let nanos = self.started.elapsed().subsec_nanos() as u64;
        if (crate::world::noise::hash_f(below.x, below.y, below.z, nanos) as f64) < fallen - 0.5 {
            self.world.set_block(below, Block::DIRT);
        }
    }
}

/// The item pick-block (middle click) looks for: crops give seeds, and
/// farmland and lit furnaces their plain blocks.
pub(super) fn picked_item(block: Block) -> Item {
    match block.base() {
        Block::LIT_FURNACE => Block::FURNACE.into(),
        Block::FARMLAND | Block::WET_FARMLAND => Block::DIRT.into(),
        b if b.crop_stage().is_some() => Item::WHEAT_SEEDS,
        b if b.wart_age().is_some() => Item::NETHER_WART,
        Block::OAK_DOOR => Item::OAK_DOOR,
        b => b.into(),
    }
}
