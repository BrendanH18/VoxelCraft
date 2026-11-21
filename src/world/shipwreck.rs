//! v0.16 Track F: shipwrecks (GEN-05e), in generation 5.
//!
//! Placement and assembly follow Java 26.3:
//!
//! - `StructureSets.SHIPWRECKS`: a linear random spread of spacing 24,
//!   separation 4 and salt 165745295 holding `shipwreck` and
//!   `shipwreck_beached` at weight 1 each. `ChunkGenerator.createStructures`
//!   draws one of the two from the start chunk's `setLargeFeatureSeed`
//!   random and tries the other when it fails; their biome tags don't
//!   overlap (`#is_ocean` and `#is_beach`), so a start in an ocean is an
//!   ocean wreck and one on a beach a beached wreck.
//! - `ShipwreckStructure.findGenerationPoint`: the start chunk's centre on
//!   the `OCEAN_FLOOR_WG` heightmap (`WORLD_SURFACE_WG` when beached), whose
//!   biome must be in the structure's tag; then from the start's
//!   `setLargeFeatureSeed` random a rotation and a template from Java's list
//!   (11 beached, 20 ocean, in `ShipwreckPieces`' order), placed at the start
//!   chunk's corner at y 90 and turned about Java's pivot (4, 0, 15).
//! - `ShipwreckPiece.postProcess`: the template drops to the mean
//!   `OCEAN_FLOOR_WG` height of its unturned footprint (an ocean wreck), or
//!   to the lowest `WORLD_SURFACE_WG` height less half its height less
//!   `nextInt(3)` (a beached one); every template fits a world-gen region, so
//!   the structure never adjusts it earlier.
//! - The palette is one of the template's eight (`getRandomPalette` with
//!   `RandomSource.create(Mth.getSeed(position))` at the adjusted position):
//!   each swaps the oak and spruce of the first for another pair of woods.
//! - Template air and markers aren't placed (`BlockIgnoreProcessor
//!   .STRUCTURE_AND_AIR`), so the sea stays in the hull; blocks that hold
//!   water placed in water are waterlogged. The `map_chest`,
//!   `supply_chest` and `treasure_chest` markers fill the chest below them
//!   with `chests/shipwreck_map`, `shipwreck_supply` and
//!   `shipwreck_treasure`.
//!
//! The templates are original (fidelity exception FE-02, see
//! [`templates`]): each has the size, markers and chest facings of Java's
//! template of that name; the blocks are VoxelCraft's own ship. Java's
//! first chunk to decorate a beached wreck draws its `nextInt(3)`; here the
//! start chunk's decoration random always does. See
//! `docs/verification/2026-11-21-v0.16-ocean-structures.md`.

pub mod templates;

use glam::{IVec2, IVec3};

use super::block::Block;
use super::fortress::Feature;
use super::mansion::{Rotation, large_feature_random};
use super::noise::hash3;
use super::structure::Bounds;
use super::temples::LootTable;
use super::terrain::Generator;
use super::worldgen_random::{SURFACE_STRUCTURES, WorldgenRandom};
use crate::enchant::JavaRandom;
use templates::{Marker, TEMPLATES, template_index};

/// `StructureSets.SHIPWRECKS`, in 16-block chunks.
pub const SPACING: i32 = 24;
pub const SEPARATION: i32 = 4;
pub const SALT: u64 = 165_745_295;
/// `ShipwreckPieces.PIVOT`.
pub const PIVOT: IVec3 = IVec3::new(4, 0, 15);
/// `ShipwreckStructure.generatePieces`' start height.
pub const START_Y: i32 = 90;
/// `NUMBER_OF_BLOCKS_ALLOWED_IN_WORLD_GEN_REGION`.
pub const MAX_REGION_SIZE: i32 = 32;
/// The structures' indices in `surface_structures` (registry order).
pub const FEATURE_INDEX: i32 = 35;
pub const FEATURE_INDEX_BEACHED: i32 = 36;
/// Farthest a wreck's blocks reach from its start chunk's centre.
pub const REACH: i32 = 24;

/// `STRUCTURE_LOCATION_BEACHED`.
pub const BEACHED: [&str; 11] = [
    "with_mast",
    "sideways_full",
    "sideways_fronthalf",
    "sideways_backhalf",
    "rightsideup_full",
    "rightsideup_fronthalf",
    "rightsideup_backhalf",
    "with_mast_degraded",
    "rightsideup_full_degraded",
    "rightsideup_fronthalf_degraded",
    "rightsideup_backhalf_degraded",
];

/// `STRUCTURE_LOCATION_OCEAN`.
pub const OCEAN: [&str; 20] = [
    "with_mast",
    "upsidedown_full",
    "upsidedown_fronthalf",
    "upsidedown_backhalf",
    "sideways_full",
    "sideways_fronthalf",
    "sideways_backhalf",
    "rightsideup_full",
    "rightsideup_fronthalf",
    "rightsideup_backhalf",
    "with_mast_degraded",
    "upsidedown_full_degraded",
    "upsidedown_fronthalf_degraded",
    "upsidedown_backhalf_degraded",
    "sideways_full_degraded",
    "sideways_fronthalf_degraded",
    "sideways_backhalf_degraded",
    "rightsideup_full_degraded",
    "rightsideup_fronthalf_degraded",
    "rightsideup_backhalf_degraded",
];

/// Where a wreck starts and how its piece lies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Start {
    pub chunk: IVec2,
    pub beached: bool,
    pub template: u8,
    pub rotation: Rotation,
}

/// Whether `b` is in `#has_structure/shipwreck` (`#is_ocean`) or, for a
/// beached wreck, `#has_structure/shipwreck_beached` (`#is_beach`).
fn allowed(beached: bool, b: super::biome::Biome) -> bool {
    if beached { b.is_beach() } else { b.is_ocean() }
}

/// `ChunkGenerator.createStructures` for the `shipwrecks` set at its start
/// chunk `chunk`: which of the two structures generates, if either.
pub fn kind_at(g: &Generator, chunk: IVec2) -> Option<bool> {
    let first = large_feature_random(g.seed, chunk).next_bounded(2) == 1;
    [first, !first].into_iter().find(|&beached| valid(g, chunk, beached))
}

/// `onTopOfChunkCenter` and `isValidBiome` for one of the two structures.
fn valid(g: &Generator, chunk: IVec2, beached: bool) -> bool {
    let (x, z) = (chunk.x * 16 + 8, chunk.y * 16 + 8);
    // `couldValidBiomeExistOnTopOfChunkCenter`.
    if !allowed(beached, g.column(x, z).biome) {
        return false;
    }
    let y = g.structure_column(x, z).top(!beached);
    allowed(beached, g.biome_at(IVec3::new(x, y, z)))
}

/// The start of the wreck at `chunk`, if one generates there.
pub fn start(g: &Generator, chunk: IVec2) -> Option<Start> {
    let beached = kind_at(g, chunk)?;
    let mut rng = large_feature_random(g.seed, chunk);
    // `Rotation.getRandom`, then `Util.getRandom` over the list.
    let rotation = Rotation::ALL[rng.next_bounded(4) as usize];
    let list: &[&str] = if beached { &BEACHED } else { &OCEAN };
    let name = list[rng.next_bounded(list.len() as i32) as usize];
    Some(Start { chunk, beached, template: template_index(name).expect("a shipwreck template"), rotation })
}

/// `StructureTemplate.transform` about [`PIVOT`] (no mirror).
pub fn pivoted(rotation: Rotation, p: IVec3) -> IVec3 {
    rotation.pos(p - PIVOT) + PIVOT
}

/// `Mth.getSeed(x, y, z)`.
pub fn position_seed(p: IVec3) -> i64 {
    let mut seed = i64::from(p.x.wrapping_mul(3_129_871)) ^ i64::from(p.z).wrapping_mul(116_129_781) ^ i64::from(p.y);
    seed = seed.wrapping_mul(seed).wrapping_mul(42_317_861).wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

/// A built wreck: its blocks (template cells already turned and in the
/// chosen palette, before waterlogging), its chests and its box.
pub struct Built {
    pub start: Start,
    /// The template position after `adjustPositionHeight`.
    pub position: IVec3,
    pub palette: u8,
    pub bounds: Bounds,
    pub blocks: Vec<(IVec3, Block)>,
    pub chests: Vec<(IVec3, Feature)>,
}

/// `ShipwreckPiece.postProcess`' height and the template's placement.
pub fn build(g: &Generator, s: Start, seed: u64) -> Built {
    let t = &TEMPLATES[s.template as usize];
    let size = IVec3::from(t.size);
    let corner = IVec3::new(s.chunk.x * 16, START_Y, s.chunk.y * 16);
    // The unturned footprint, as Java reads it.
    let (mut min, mut sum) = (i32::MAX, 0i64);
    for z in corner.z..corner.z + size.z {
        for x in corner.x..corner.x + size.x {
            let h = g.structure_column(x, z).top(!s.beached) + 1;
            min = min.min(h);
            sum += i64::from(h);
        }
    }
    let y = if s.beached {
        let mut r = WorldgenRandom::structure(
            g.seed,
            s.chunk.x * 16,
            s.chunk.y * 16,
            FEATURE_INDEX_BEACHED,
            SURFACE_STRUCTURES,
        );
        min - size.y / 2 - r.next_int(3)
    } else {
        (sum / i64::from(size.x * size.z)) as i32
    };
    let position = IVec3::new(corner.x, y, corner.z);
    let palette = JavaRandom::new(position_seed(position)).next_bounded(templates::PALETTES.len() as i32) as u8;
    let mut blocks = Vec::new();
    let mut chests = Vec::new();
    let cells = templates::cells(s.template);
    for &(local, cell) in cells.iter() {
        let p = position + pivoted(s.rotation, local);
        blocks.push((p, templates::resolve(cell, palette, s.rotation)));
    }
    for &(marker, at, _) in t.markers {
        let p = position + pivoted(s.rotation, IVec3::from(at) - IVec3::Y);
        let table = match marker {
            Marker::Map => LootTable::ShipMap,
            Marker::Supply => LootTable::ShipSupply,
            Marker::Treasure => LootTable::ShipTreasure,
        };
        chests.push((p, Feature::TempleChest(seed ^ hash3(p.x, p.y, p.z, 0x7E), table)));
    }
    let (a, b) = (pivoted(s.rotation, IVec3::ZERO), pivoted(s.rotation, size - IVec3::ONE));
    let bounds = Bounds { min: position + a.min(b), max: position + a.max(b) };
    blocks.sort_unstable_by_key(|(p, _)| (p.y, p.z, p.x));
    Built { start: s, position, palette, bounds, blocks, chests }
}

#[cfg(test)]
mod tests;
