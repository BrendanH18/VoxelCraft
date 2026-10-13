//! Game-side farming: trampling farmland and pick-block items. Tilling and
//! bone meal are shared session uses; growth happens in `world::growth`.

use glam::DVec3;

use crate::item::Item;
use crate::world::block::Block;

use super::Game;

impl Game {
    /// Landing on farmland from more than half a block may trample it back
    /// to dirt (the chance grows with the fall), popping off its crop.
    pub(super) fn trample(&mut self, fallen: f64) {
        if fallen <= 0.5 {
            return;
        }
        let below = (self.session.player.pos - DVec3::Y * 0.1).floor().as_ivec3();
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
    if matches!(
        voxelcraft::world::redstone_blocks::component(block),
        Some(voxelcraft::world::redstone_blocks::Component::Wire(_))
    ) {
        return Item::REDSTONE;
    }
    match block.base() {
        Block::LIT_FURNACE => Block::FURNACE.into(),
        Block::LIT_SMOKER => Block::SMOKER.into(),
        Block::LIT_BLAST_FURNACE => Block::BLAST_FURNACE.into(),
        Block::FARMLAND | Block::WET_FARMLAND => Block::DIRT.into(),
        b if matches!(b.as_crop(), Some((crate::world::block::Crop::Wheat, _))) => Item::WHEAT_SEEDS,
        b if matches!(b.as_crop(), Some((crate::world::block::Crop::Carrot, _))) => Item::CARROT,
        b if matches!(b.as_crop(), Some((crate::world::block::Crop::Potato, _))) => Item::POTATO,
        b if b.wart_age().is_some() => Item::NETHER_WART,
        crate::world::nether_biome_blocks::WEEPING_VINES_PLANT => {
            crate::world::nether_biome_blocks::WEEPING_VINES.into()
        }
        crate::world::nether_biome_blocks::TWISTING_VINES_PLANT => {
            crate::world::nether_biome_blocks::TWISTING_VINES.into()
        }
        Block::OAK_DOOR => Item::OAK_DOOR,
        b if let Some(c) = b.bed_color() => c.bed(),
        b => b.into(),
    }
}
