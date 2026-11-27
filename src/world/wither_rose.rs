//! v0.16 Track C: the wither rose and its potted form (Java 26.3
//! `WitherRoseBlock`, `Blocks.WITHER_ROSE`, `Blocks.POTTED_WITHER_ROSE`).
//!
//! A `FlowerBlock` that grows on `#supports_wither_rose` (the vegetation
//! soils, netherrack, soul sand and soul soil), breaks instantly, drops
//! itself, gives anything living that stands in it Wither (40 ticks,
//! level I; not in Peaceful, not to the Wither-immune) and smokes. Bone
//! meal does nothing to it, and grass bone meal never grows one. The
//! Wither plants one where a mob it killed died (`LivingEntity.createWitherRose`).
//!
//! States 12724 (rose) and 12725 (potted rose); texture layers 706–707.
use glam::{DVec3, IVec3};

use super::block::{Block, RenderKind};

pub const WITHER_ROSE: Block = Block(12724);
pub const POTTED_WITHER_ROSE: Block = Block(12725);
pub const TEX: u16 = 706;
pub const POTTED_TEX: u16 = 707;
pub const TEX_END: u16 = 708;
/// `WitherRoseBlock.getBeeInteractionEffect`: Wither for 40 ticks.
pub const CONTACT_TICKS: u32 = 40;
/// `Blocks.WITHER_ROSE`'s suspicious stew effect: Wither for 7 seconds.
pub const STEW_TICKS: u32 = 140;

pub const fn owns(id: u16) -> bool {
    id == WITHER_ROSE.0 || id == POTTED_WITHER_ROSE.0
}

pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    use super::utilities::t;
    match id {
        12724 => ("wither rose", RenderKind::Cross, [TEX; 6]),
        12725 => (
            "potted wither rose",
            RenderKind::Shaped,
            [POTTED_TEX, POTTED_TEX, t::POT_SOIL, t::POT_BOTTOM, POTTED_TEX, POTTED_TEX],
        ),
        _ => ("unknown", RenderKind::Invisible, [0; 6]),
    }
}

/// `#supports_wither_rose`: `#supports_vegetation` plus netherrack, soul
/// sand and soul soil.
pub fn supports(below: Block) -> bool {
    super::overworld_blocks::dirt_like(below)
        || below == Block::NETHERRACK
        || below == Block::SOUL_SAND
        || below == super::nether_biome_blocks::SOUL_SOIL
}

pub fn can_stay_on(b: Block, below: Block) -> Option<bool> {
    (b == WITHER_ROSE).then(|| supports(below))
}

pub const fn potted(plant: Block) -> Option<Block> {
    if plant.0 == WITHER_ROSE.0 { Some(POTTED_WITHER_ROSE) } else { None }
}

pub const fn potted_plant(b: Block) -> Option<Block> {
    if b.0 == POTTED_WITHER_ROSE.0 { Some(WITHER_ROSE) } else { None }
}

/// The flower drawn above the pot on the potted texture layer.
pub const fn pot_layer_plant(layer: u16) -> Option<Block> {
    if layer == POTTED_TEX { Some(WITHER_ROSE) } else { None }
}

/// `WitherRoseBlock.animateTick`'s smoke: three tries, each a coin flip,
/// at the flower's centre (`x + rand / 5`, `y + 0.5 - rand`, `z + rand / 5`).
pub fn smoke_positions(cell: IVec3, mut rand: impl FnMut() -> f64) -> Vec<DVec3> {
    let c = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let mut out = Vec::new();
    for _ in 0..3 {
        if rand() < 0.5 {
            out.push(DVec3::new(c.x + rand() / 5.0, c.y + (0.5 - rand()), c.z + rand() / 5.0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_on_javas_soils_and_pots() {
        assert!(supports(Block::GRASS) && supports(Block::DIRT));
        assert!(supports(Block::NETHERRACK) && supports(Block::SOUL_SAND));
        assert!(supports(super::super::nether_biome_blocks::SOUL_SOIL));
        assert!(!supports(Block::STONE) && !supports(Block::SAND));
        assert_eq!(potted(WITHER_ROSE), Some(POTTED_WITHER_ROSE));
        assert_eq!(potted_plant(POTTED_WITHER_ROSE), Some(WITHER_ROSE));
        assert_eq!(WITHER_ROSE.name(), "wither rose");
        assert_eq!(POTTED_WITHER_ROSE.name(), "potted wither rose");
    }

    #[test]
    fn smoke_stays_near_the_flower() {
        let mut v = [0.1, 0.5, 0.9, 0.2, 0.7, 0.3, 0.3, 0.3].into_iter().cycle();
        let puffs = smoke_positions(IVec3::new(2, 64, 3), || v.next().unwrap());
        assert!(!puffs.is_empty());
        for p in puffs {
            assert!((2.5..2.7).contains(&p.x) && (63.5..64.5).contains(&p.y) && (3.5..3.7).contains(&p.z));
        }
    }
}
