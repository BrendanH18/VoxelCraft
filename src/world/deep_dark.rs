//! The deep dark (GEN-02c, generation 2): where the cave biome lies and
//! how its sculk is laid out.
//!
//! **Where.** Java places the deep dark as a bottom cave biome under ground
//! whose erosion parameter is at most -0.375 (the two lowest erosion bands:
//! un-eroded, mountainous land) at a depth parameter of about 1.1, which is
//! roughly 130 blocks under the surface. VoxelCraft keeps its own noise
//! (FE-01) and applies the same rule to its climate: erosion below
//! [`MAX_EROSION`], at least [`MIN_DEPTH`] blocks under the column's surface
//! and no higher than [`TOP_Y`]. Below that it takes precedence over lush
//! and dripstone caves, as Java's deep dark does at its depth.
//!
//! **Sculk.** Java grows patches by sending charges out from random origins
//! (`SculkPatchFeature` with `SculkSpreader`): sculk over the exposed faces
//! of stone, veins at the patch edges, half the patches with a catalyst at
//! their origin, and sensors and (rarely) shriekers on the sculk. This
//! module reproduces that look with a decision per cell from world
//! coordinates, so a patch is continuous across chunk borders whatever order
//! chunks generate in: a 3D patch noise turns exposed stone into sculk where
//! it is high, lays veins on stone faces in a band just under it (and a few
//! elsewhere), and hashes pick catalysts in patch cores and sensors and
//! shriekers on sculk floors. Generated shriekers can summon, like Java's
//! world-generated ones; nothing summons until the warden (v0.16).

use glam::IVec3;

use super::biome::{Biome, Climate};
use super::block::Block;
use super::cave_blocks::{self as cb, Multiface};
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, WORLD_MIN_Y, index};
use super::noise::{Perlin, hash_f, hash3};
use super::terrain::Generation;
use super::underground::Ctx;

/// Java's `erosions[1]` upper bound: the deep dark's erosion span is
/// `-1.0..-0.375`.
pub const MAX_EROSION: f32 = -0.375;
/// Blocks under the surface where Java's depth parameter reaches the deep
/// dark's (about 1.05 of 128 blocks per unit).
pub const MIN_DEPTH: i32 = 120;
/// The deep dark stays below this height.
pub const TOP_Y: i32 = 0;

const PATCH_SALT: u64 = 0x5C01_4D44;
const PICK_SALT: u64 = 0x5C01_4D45;
const VEIN_SALT: u64 = 0x5C01_4D46;
/// Patch noise above this is sculk.
const SCULK: f32 = 0.02;
/// Veins line stone faces in this band under [`SCULK`].
const VEIN_BAND: f32 = 0.14;
/// Scattered veins elsewhere in the biome, per stone face.
const STRAY_VEIN: f32 = 0.025;
/// A sculk floor in a patch core takes a catalyst with this chance.
const CATALYST: f32 = 1.0 / 220.0;
const CATALYST_CORE: f32 = 0.22;
/// A cell over sculk floor takes a sensor or shrieker with this chance;
/// one in eleven is a shrieker (`SculkBlock.getRandomGrowthState`).
const GROWTH: f32 = 0.012;

/// Whether a cell at height `y`, under a column of this climate whose
/// surface is at `height`, is deep dark.
pub fn under(climate: &Climate, height: i32, y: i32) -> bool {
    climate.erosion < MAX_EROSION && y <= TOP_Y && y > WORLD_MIN_Y && height - y >= MIN_DEPTH
}

/// The cave biome at height `y` under a column (`None`: the surface
/// biome): the deep dark in generation 2 where [`under`] holds, otherwise
/// lush or dripstone caves as before.
pub fn cave_biome(generation: Generation, climate: &Climate, height: i32, y: i32) -> Option<Biome> {
    if generation >= Generation::V2 && under(climate, height, y) {
        return Some(Biome::DeepDark);
    }
    super::biome::cave(climate, height - y)
}

/// A height inside the deep dark under this column, for `/locate biome`,
/// if it has any.
pub fn locate_y(climate: &Climate, height: i32) -> Option<i32> {
    let y = (height - MIN_DEPTH).min(TOP_Y) - 8;
    (y > WORLD_MIN_Y + 8 && under(climate, height, y)).then_some(y)
}

/// Stone the sculk spreads over and replaces (Java's
/// `sculk_replaceable_world_gen`, limited to what generates here).
fn replaceable(b: Block) -> bool {
    matches!(
        b,
        Block::STONE
            | Block::DEEPSLATE
            | Block::GRANITE
            | Block::DIORITE
            | Block::ANDESITE
            | Block::TUFF
            | Block::CALCITE
            | Block::DIRT
            | Block::GRAVEL
            | Block::CLAY
            | Block::SAND
    ) || b == super::overworld_blocks::DRIPSTONE_BLOCK
        || b == super::overworld_blocks::SMOOTH_BASALT
}

/// Sculk patches and their veins, catalysts, sensors and shriekers in the
/// deep-dark cells of the chunk.
pub(super) fn decorate(ctx: &mut Ctx, blocks: &mut [Block; super::chunk::CHUNK_VOLUME]) {
    if !ctx.any_deep {
        return;
    }
    let noise = Perlin::new(ctx.seed ^ PATCH_SALT);
    let patch = |p: IVec3| {
        let f = p.as_vec3();
        noise.noise3(f.x / 11.0, f.y / 7.0, f.z / 11.0) + 0.5 * noise.noise3(f.x / 4.0 + 31.0, f.y / 4.0, f.z / 4.0)
    };
    let base = ctx.base;
    // First the sculk itself: exposed stone where the patch noise is high.
    let mut sculk = vec![false; super::chunk::CHUNK_VOLUME];
    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if !ctx.deep(x, y, z) {
                    continue;
                }
                let i = index(x, y, z);
                if !replaceable(ctx.snapshot[i]) {
                    continue;
                }
                let l = IVec3::new(x as i32, y as i32, z as i32);
                let p = base + l;
                // Most stone is buried: look inside the chunk first, then
                // at the patch, then across the border.
                let inside = |q: IVec3| q.cmpge(IVec3::ZERO).all() && q.cmplt(IVec3::splat(CHUNK_SIZE_I)).all();
                let open_here = super::cave_blocks::DIRS.iter().any(|&d| {
                    let q = l + d;
                    inside(q) && super::underground::open(ctx.snapshot[index(q.x as usize, q.y as usize, q.z as usize)])
                });
                let on_border = l.cmpeq(IVec3::ZERO).any() || l.cmpeq(IVec3::splat(CHUNK_SIZE_I - 1)).any();
                if !(open_here || on_border) || patch(p) <= SCULK {
                    continue;
                }
                if !open_here && !super::cave_blocks::DIRS.iter().any(|&d| !inside(l + d) && ctx.open(p + d)) {
                    continue;
                }
                sculk[i] = true;
                blocks[i] = cb::SCULK;
            }
        }
    }
    // Then what grows on it and around it, from the open cells.
    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let i = index(x, y, z);
                let here = ctx.snapshot[i];
                let wet = here == Block::WATER;
                if !(here == Block::AIR || wet) || !ctx.deep(x, y, z) {
                    continue;
                }
                let p = base + IVec3::new(x as i32, y as i32, z as i32);
                // Catalysts sit in the sculk floor of a patch core; sensors
                // and shriekers stand on sculk.
                if y > 0 && sculk[index(x, y - 1, z)] {
                    let floor = p - IVec3::Y;
                    if hash_f(floor.x, floor.y, floor.z, ctx.seed ^ PICK_SALT) < CATALYST
                        && patch(floor) > CATALYST_CORE
                    {
                        blocks[index(x, y - 1, z)] = cb::sculk_catalyst(false);
                        continue;
                    }
                    if hash_f(p.x, p.y, p.z, ctx.seed ^ PICK_SALT) < GROWTH {
                        blocks[i] = if hash3(p.x, p.y, p.z, ctx.seed ^ PICK_SALT).is_multiple_of(11) {
                            cb::sculk_shrieker(false, true, wet)
                        } else {
                            cb::sculk_sensor(cb::Phase::Inactive, wet)
                        };
                        continue;
                    }
                }
                // Veins on the stone faces around the cell (in this chunk:
                // a face needs its stone to be there).
                let mut faces = 0u8;
                for (f, &d) in super::cave_blocks::DIRS.iter().enumerate() {
                    let q = IVec3::new(x as i32, y as i32, z as i32) + d;
                    if q.cmplt(IVec3::ZERO).any() || q.cmpge(IVec3::splat(CHUNK_SIZE_I)).any() {
                        continue;
                    }
                    let j = index(q.x as usize, q.y as usize, q.z as usize);
                    if sculk[j] || !replaceable(ctx.snapshot[j]) {
                        continue;
                    }
                    let n = q + base;
                    let v = patch(n);
                    let stray = hash_f(n.x * 6 + f as i32, n.y, n.z, ctx.seed ^ VEIN_SALT) < STRAY_VEIN;
                    if v > SCULK - VEIN_BAND || stray {
                        faces |= 1 << f;
                    }
                }
                if faces != 0 {
                    blocks[i] = cb::multiface(Multiface::SculkVein, faces, wet);
                }
            }
        }
    }
}
