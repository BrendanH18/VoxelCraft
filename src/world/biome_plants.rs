//! v0.13 biome plants (GEN-03): leaf litter, wildflowers, bushes, firefly
//! bushes, short and tall dry grass and cactus flowers, in the track's
//! reservation 9984–10111 (`docs/architecture.md`, "v0.13 track
//! reservations").
//!
//! | Ids | Block |
//! | --- | --- |
//! | 9984–9987 | leaf litter with 1–4 segments |
//! | 10000–10003 | wildflowers with 1–4 flowers |
//! | 10004 | bush |
//! | 10005 | firefly bush (light level 2) |
//! | 10006 | short dry grass |
//! | 10007, 10008 | tall dry grass, lower then upper half |
//! | 10009 | cactus flower |
//!
//! Leaf litter and wildflowers are one-sixteenth-thick carpets that stack up
//! to four segments (placing another on top adds one; breaking drops one
//! item per segment). Java's `facing` property is not stored: the carpets
//! look the same from every side, so it would only cost ids.
//! Sources and differences: `docs/verification/2026-10-30-v0.13-trees.md`.

use glam::IVec3;

use super::World;
use super::block::{Block, RenderKind};
use super::overworld_blocks as ob;
use crate::inventory::Stack;
use crate::item::Item;

/// First id of the reservation.
pub const FIRST: u16 = 9984;
/// Last id of the reservation.
pub const LAST: u16 = 10111;
const LITTER_FIRST: u16 = 9984;
const FLOWERS_FIRST: u16 = 10000;

pub const LEAF_LITTER: Block = Block(LITTER_FIRST);
pub const WILDFLOWERS: Block = Block(FLOWERS_FIRST);
pub const BUSH: Block = Block(10004);
pub const FIREFLY_BUSH: Block = Block(10005);
pub const SHORT_DRY_GRASS: Block = Block(10006);
pub const TALL_DRY_GRASS: Block = Block(10007);
pub const TALL_DRY_GRASS_TOP: Block = Block(10008);
pub const CACTUS_FLOWER: Block = Block(10009);
/// One past the last registered id.
const END: u16 = 10010;

/// Texture layers, as offsets from one base the integrator may move.
pub const TEX: u16 = 2832;
pub mod t {
    use super::TEX;
    /// Leaf litter with 1–4 segments.
    pub const LEAF_LITTER: u16 = TEX;
    /// Wildflowers with 1–4 flowers.
    pub const WILDFLOWERS: u16 = TEX + 4;
    pub const BUSH: u16 = TEX + 8;
    pub const FIREFLY_BUSH: u16 = TEX + 9;
    pub const SHORT_DRY_GRASS: u16 = TEX + 10;
    pub const TALL_DRY_GRASS: u16 = TEX + 11;
    pub const TALL_DRY_GRASS_TOP: u16 = TEX + 12;
    pub const CACTUS_FLOWER: u16 = TEX + 13;
    pub const COUNT: u16 = 14;
}
/// One past the last texture layer used.
pub const TEX_END: u16 = TEX + t::COUNT;
const _: () = assert!(TEX_END <= 2864);

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id < END
}

/// Leaf litter or wildflowers: the carpet family and its segment count.
pub const fn segments(b: Block) -> Option<(Block, u8)> {
    match b.0 {
        LITTER_FIRST..=9987 => Some((LEAF_LITTER, (b.0 - LITTER_FIRST) as u8 + 1)),
        FLOWERS_FIRST..=10003 => Some((WILDFLOWERS, (b.0 - FLOWERS_FIRST) as u8 + 1)),
        _ => None,
    }
}

/// The carpet of `family` (leaf litter or wildflowers) with `count` segments.
pub const fn with_segments(family: Block, count: u8) -> Block {
    Block(
        family.0
            + (if count < 1 {
                0
            } else if count > 4 {
                3
            } else {
                count - 1
            }) as u16,
    )
}

/// The state placing `item_block` on a cell holding `existing` makes:
/// one more segment of the same family, up to four.
pub fn stacked(item_block: Block, existing: Block) -> Option<Block> {
    let (family, _) = segments(item_block)?;
    match segments(existing) {
        Some((f, n)) if f == family && n < 4 => Some(with_segments(family, n + 1)),
        _ => None,
    }
}

/// Items one broken block gives per drop: a segment each.
pub const fn drop_count(b: Block) -> u8 {
    match segments(b) {
        Some((_, n)) => n,
        None => 1,
    }
}

/// Every block's first state: the one its item, name and rules belong to.
pub const fn base(id: u16) -> Option<Block> {
    match id {
        9985..=9987 => Some(LEAF_LITTER),
        10001..=10003 => Some(WILDFLOWERS),
        10008 => Some(TALL_DRY_GRASS),
        _ => None,
    }
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    let (name, kind, layer) = match id {
        9984..=9987 => ("leaf litter", Cutout, t::LEAF_LITTER + (id - LITTER_FIRST)),
        10000..=10003 => ("wildflowers", Cutout, t::WILDFLOWERS + (id - FLOWERS_FIRST)),
        10004 => ("bush", Cross, t::BUSH),
        10005 => ("firefly bush", Cross, t::FIREFLY_BUSH),
        10006 => ("short dry grass", Cross, t::SHORT_DRY_GRASS),
        10007 => ("tall dry grass", Cross, t::TALL_DRY_GRASS),
        10008 => ("tall dry grass top", Cross, t::TALL_DRY_GRASS_TOP),
        10009 => ("cactus flower", Cross, t::CACTUS_FLOWER),
        _ => return None,
    };
    Some((name, kind, [layer; 6]))
}

/// Light emission: a firefly bush glows faintly.
pub const fn emission(id: u16) -> u8 {
    if id == FIREFLY_BUSH.0 { 2 } else { 0 }
}

/// How far (1/16) the top of a carpet sits under the cell's top.
pub const fn top_drop(b: Block) -> Option<u8> {
    if segments(b).is_some() { Some(15) } else { None }
}

/// Java's fire odds `(encouragement, flammability)`.
pub const fn fire_odds(b: Block) -> Option<(u8, u8)> {
    match b.0 {
        FIRST..=9987 | FLOWERS_FIRST..=10008 => Some((60, 100)),
        _ => None,
    }
}

/// Plants that placing another block overwrites.
pub const fn replaceable(b: Block) -> bool {
    matches!(b.0, 10006..=10008)
}

/// What breaking gives (`Some(None)`: nothing without shears); `None` for
/// blocks of other modules.
pub fn drop(b: Block) -> Option<Option<Item>> {
    if !owns(b.0) {
        return None;
    }
    Some(match b.0 {
        10004 | 10006..=10008 => None,
        _ => Some(b.base().into()),
    })
}

/// What shears and silk touch keep of the bush and the dry grasses.
pub fn sheared_drop(b: Block) -> Option<Item> {
    matches!(b.0, 10004 | 10006..=10008).then(|| b.base().into())
}

/// Whether `block` can stand on `below` (`None`: not a block of this module).
pub fn can_stay_on(block: Block, below: Block) -> Option<bool> {
    if !owns(block.0) {
        return None;
    }
    Some(match block.0 {
        // A carpet needs a full face to lie on, like moss carpet.
        9984..=9987 | 10000..=10003 => below.is_solid() && below.kind() != RenderKind::Cross,
        // Bushes root in soil (Java's `BlockTags.DIRT`, farmland).
        10004 | 10005 => ob::dirt_like(below),
        // Dry grass is a desert plant: sand, terracotta and soil.
        10006 | 10007 => {
            matches!(below, Block::SAND | Block::RED_SAND)
                || ob::dirt_like(below)
                || below.terracotta_colour().is_some()
                || below.stained_terracotta_color().is_some()
        }
        10008 => below == TALL_DRY_GRASS,
        // A cactus flower sits on a cactus or any solid block.
        _ => below == Block::CACTUS || below.is_opaque(),
    })
}

/// Bone meal on a plant of this module: wildflowers grow a segment (or
/// drop one when full), short dry grass becomes tall dry grass. `None` for
/// other blocks.
pub fn bone_meal(b: Block) -> Option<BoneMeal> {
    match b.0 {
        FLOWERS_FIRST..=10002 => Some(BoneMeal::Become(Block(b.0 + 1))),
        10003 => Some(BoneMeal::Drop(WILDFLOWERS)),
        10006 => Some(BoneMeal::Tall),
        _ => None,
    }
}

/// The result of bone meal on a plant of this module.
#[derive(Debug, PartialEq, Eq)]
pub enum BoneMeal {
    /// The block turns into this state.
    Become(Block),
    /// A full carpet drops a copy of itself.
    Drop(Block),
    /// A short plant grows into its two-block tall form.
    Tall,
}

impl World {
    /// Bone meal on one of this module's plants (`None`: not ours); returns
    /// whether the bone meal was used.
    pub(super) fn plant_bone_meal(&mut self, p: IVec3, b: Block) -> Option<bool> {
        Some(match bone_meal(b)? {
            BoneMeal::Become(next) => {
                self.edit(p, next, true);
                true
            }
            BoneMeal::Drop(item) => {
                self.drops.push((p, Stack::new(item, 1)));
                true
            }
            BoneMeal::Tall => {
                if self.get_block(p + IVec3::Y).is_some_and(|above| above.is_replaceable() && above != Block::LAVA) {
                    self.edit(p + IVec3::Y, TALL_DRY_GRASS_TOP, true);
                    self.edit(p, TALL_DRY_GRASS, true);
                    true
                } else {
                    false
                }
            }
        })
    }
}

/// Creative palette entries.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [LITTER_FIRST, FLOWERS_FIRST, BUSH.0, FIREFLY_BUSH.0, SHORT_DRY_GRASS.0, TALL_DRY_GRASS.0, CACTUS_FLOWER.0]
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_register_with_their_rules() {
        for id in FIRST..END {
            let Some((name, kind, tex)) = registry(id) else { continue };
            let b = Block(id);
            assert_eq!(b.name(), name);
            assert_eq!(b.kind(), kind);
            assert!(tex.iter().all(|&l| (TEX..TEX_END).contains(&l)));
            assert_eq!(b.hardness(), 0.0, "{name}");
            if let Some(base) = base(id) {
                assert_eq!(b.base(), base);
            }
        }
        assert_eq!(Block::from_name("leaf litter"), Some(LEAF_LITTER));
        assert_eq!(Block::from_name("wildflowers"), Some(WILDFLOWERS));
        assert_eq!(Block::from_name("firefly bush"), Some(FIREFLY_BUSH));
        assert_eq!(Block::from_name("cactus flower"), Some(CACTUS_FLOWER));
        assert_eq!(FIREFLY_BUSH.emission(), 2);
        assert_eq!(LEAF_LITTER.top_drop(), 15);
        assert_eq!(TALL_DRY_GRASS_TOP.base(), TALL_DRY_GRASS);
    }

    #[test]
    fn carpets_stack_and_drop_a_segment_each() {
        for family in [LEAF_LITTER, WILDFLOWERS] {
            let mut b = family;
            for n in 1..4 {
                assert_eq!(segments(b), Some((family, n)));
                assert_eq!(drop_count(b), n);
                b = stacked(family, b).expect("room for another segment");
            }
            assert_eq!(segments(b), Some((family, 4)));
            assert_eq!(stacked(family, b), None);
            assert_eq!(b.base(), family);
            assert_eq!(b.drop(), Some(Item::from_block(family)));
        }
        assert_eq!(stacked(LEAF_LITTER, WILDFLOWERS), None);
    }

    #[test]
    fn placement_rules() {
        assert_eq!(can_stay_on(BUSH, Block::GRASS), Some(true));
        assert_eq!(can_stay_on(BUSH, Block::SAND), Some(false));
        assert_eq!(can_stay_on(SHORT_DRY_GRASS, Block::SAND), Some(true));
        assert_eq!(can_stay_on(TALL_DRY_GRASS_TOP, TALL_DRY_GRASS), Some(true));
        assert_eq!(can_stay_on(CACTUS_FLOWER, Block::CACTUS), Some(true));
        assert_eq!(can_stay_on(LEAF_LITTER, Block::STONE), Some(true));
        assert_eq!(can_stay_on(LEAF_LITTER, Block::AIR), Some(false));
        assert!(BUSH.can_stay_on(Block::GRASS) && !BUSH.can_stay_on(Block::AIR));
    }

    fn world() -> World {
        use crate::world::chunk::ChunkData;
        use crate::world::terrain::Generator;
        let mut w = World::new_headless(std::sync::Arc::new(Generator::new(5)), Default::default(), 2);
        for y in w.generator.dimension.chunk_rows() {
            w.insert_chunk(IVec3::new(0, y, 0), std::sync::Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        w
    }

    #[test]
    fn breaking_drops_per_segment_and_dry_grass_needs_shears() {
        let mut w = world();
        let at = IVec3::new(4, 80, 4);
        w.set_block(at - IVec3::Y, Block::GRASS);
        w.set_block(at, with_segments(LEAF_LITTER, 3));
        w.spill_mined(at, with_segments(LEAF_LITTER, 3), Default::default());
        let total: u32 = w
            .drops
            .iter()
            .filter(|(_, s)| s.item == Item::from_block(LEAF_LITTER))
            .map(|(_, s)| u32::from(s.count))
            .sum();
        assert_eq!(total, 3);
        w.drops.clear();
        w.spill_mined(at, SHORT_DRY_GRASS, Default::default());
        assert!(w.drops.is_empty(), "short dry grass drops nothing without shears");
        w.spill_with_item(at, SHORT_DRY_GRASS, Some(Stack::new(Item::SHEARS, 1)));
        assert!(w.drops.iter().any(|(_, s)| s.item == Item::from_block(SHORT_DRY_GRASS)));
    }

    #[test]
    fn plants_fall_with_their_ground_and_bone_meal_grows_them() {
        let mut w = world();
        let at = IVec3::new(6, 80, 6);
        w.set_block(at - IVec3::Y, Block::SAND);
        w.set_block(at, SHORT_DRY_GRASS);
        assert!(w.apply_bone_meal(at));
        assert_eq!(w.get_block(at), Some(TALL_DRY_GRASS));
        assert_eq!(w.get_block(at + IVec3::Y), Some(TALL_DRY_GRASS_TOP));
        w.set_block(at + IVec3::Y, Block::AIR);
        assert_eq!(w.get_block(at), Some(Block::AIR), "the halves go together");

        let flowers = IVec3::new(9, 80, 9);
        w.set_block(flowers - IVec3::Y, Block::GRASS);
        w.set_block(flowers, WILDFLOWERS);
        assert!(w.apply_bone_meal(flowers));
        assert_eq!(w.get_block(flowers), Some(with_segments(WILDFLOWERS, 2)));
        w.set_block(flowers, with_segments(WILDFLOWERS, 4));
        assert!(w.apply_bone_meal(flowers));
        assert_eq!(w.get_block(flowers), Some(with_segments(WILDFLOWERS, 4)));
        assert!(w.drops.iter().any(|(_, s)| s.item == Item::from_block(WILDFLOWERS)));

        let litter = IVec3::new(12, 80, 12);
        w.set_block(litter - IVec3::Y, Block::DIRT);
        w.set_block(litter, LEAF_LITTER);
        w.set_block(litter - IVec3::Y, Block::AIR);
        assert_eq!(w.get_block(litter), Some(Block::AIR), "leaf litter needs ground");
    }

    #[test]
    fn every_state_survives_save_and_reload() {
        use crate::world::chunk::ChunkData;
        use crate::world::storage::{LevelInfo, Storage};
        let states: Vec<Block> = (FIRST..END).filter(|&i| registry(i).is_some()).map(Block).collect();
        assert_eq!(states.len(), 4 + 4 + 6);
        let dir = std::env::temp_dir().join(format!("voxelcraft-biome-plants-{}", std::process::id()));
        let storage = Storage::new(&dir);
        let mut chunk = ChunkData::Uniform(Block::STONE);
        for (i, b) in states.iter().enumerate() {
            chunk.set(i, 5, 3, *b);
        }
        let pos = IVec3::new(2, -2, -7);
        storage
            .save(&LevelInfo { seed: 1, player: None, props: Default::default() }, &[(pos, std::sync::Arc::new(chunk))])
            .unwrap();
        let loaded = storage.load_chunks().unwrap();
        for (i, b) in states.iter().enumerate() {
            assert_eq!(loaded[&pos].get(i, 5, 3), *b);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
