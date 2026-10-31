//! Generation 2's Overworld shape: a 3D density field.
//!
//! A block is solid where the column's surface height *at that block's
//! height* lies at or above it: `y <= base + relief * noise(x, y, z)`,
//! where `base` is generation 1's climate height ([`ClimateNoise::height`])
//! and `relief` ([`ClimateNoise::relief`]) says how far the 3D noise may
//! move it. Where relief is small (oceans, plains, rivers, swamps, coasts)
//! that is a heightmap; where it is large (mountains, windswept hills,
//! badlands) the surface leans over itself into overhangs, cliffs, arches
//! and ledges. Badlands terrace the result, so the 3D shape steps.
//!
//! The noise is sampled on a coarse grid (every 4 blocks across, every 8
//! up) and interpolated, and only in the band of heights the relief can
//! reach: below it everything is solid, above it everything is air. Grid
//! nodes sit at world coordinates, so every chunk column computes the same
//! values for the same block, whichever chunk asks first.
//!
//! [`ClimateNoise::height`]: super::super::climate::ClimateNoise::height
//! [`ClimateNoise::relief`]: super::super::climate::ClimateNoise::relief

use super::*;
use crate::world::chunk::{WORLD_MAX_Y, WORLD_MIN_Y};

const NODE_XZ: i32 = 4;
const NODE_Y: i32 = 8;
/// Node columns per side: from 4 blocks before the chunk to 4 after it,
/// so the one-block ring of columns interpolates too.
const NODES: usize = CHUNK_SIZE / NODE_XZ as usize + 3;
const LAYERS: i32 = (WORLD_MAX_Y - WORLD_MIN_Y) / NODE_Y + 1;
/// The lowest block that is always solid and the highest that can be.
const FLOOR: i32 = BOTTOM + 6;
const CEILING: i32 = 300;
/// Caves leave rock this thin over open air alone, so overhangs, arches
/// and ledges don't shatter into floating pieces.
const SLAB: u32 = 6;
/// Solid masks start this far below the chunk, so a block knows how far
/// the open air is below it and (up to 16 blocks) above it.
const MASK_BELOW: i32 = 16;
/// Relief above which caves stay below the base height.
const SPIRE_RELIEF: f32 = 8.0;
/// How much of the relief applies above the base height.
const UPWARD: f32 = 0.65;
/// How hard the surface is squashed above the base height.
const SQUASH: f32 = 2.0;
/// The least relief of badlands above the sea, for their overhanging mesas.
const BADLANDS_RELIEF: f32 = 10.0;

pub(super) type Masks = Box<[u64; CHUNK_SIZE * CHUNK_SIZE]>;

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Generation 1's badlands terraces, applied to a height.
#[inline]
fn terrace(h: f32) -> f32 {
    let t = h / 7.0;
    let terraced = (t.floor() + smoothstep(0.6, 0.9, t.fract())) * 7.0;
    h + (terraced - h) * 0.8
}

/// Distance from mask bit `bit` up to the first open bit (64 or more when
/// everything known above is solid).
#[inline]
fn open_above(mask: u64, bit: u32) -> u32 {
    ((!mask) >> (bit + 1)).trailing_zeros() + 1
}

/// Distance from mask bit `bit` down to the first open bit.
#[inline]
fn open_below(mask: u64, bit: u32) -> u32 {
    ((!mask) << (64 - bit)).leading_zeros() + 1
}

/// The density field of one chunk column and the ring around it.
pub(super) struct Relief {
    /// Per column, `[z][x]` like [`ChunkColumns::cols`]: base height,
    /// relief, the eroded badlands' hoodoos added on top, and whether the
    /// column terraces.
    base: [[f32; W]; W],
    amp: [[f32; W]; W],
    extra: [[f32; W]; W],
    terraced: [[bool; W]; W],
    /// Heights at and below `.0` are solid and above `.1` open, whatever
    /// the noise.
    bounds: [[(i32, i32); W]; W],
    /// Relief noise at the grid nodes: `[layer][z][x]`.
    nodes: Box<[f32]>,
    layer0: i32,
    layers: i32,
}

impl Relief {
    #[inline]
    fn slot(lx: i32, lz: i32) -> (usize, usize) {
        debug_assert!((-1..=CHUNK_SIZE_I).contains(&lx) && (-1..=CHUNK_SIZE_I).contains(&lz));
        ((lx + M as i32) as usize, (lz + M as i32) as usize)
    }

    /// The noise of node layer `k` (relative to `layer0`) interpolated to a column.
    #[inline]
    fn layer(&self, ix: usize, iz: usize, tx: f32, tz: f32, k: i32) -> f32 {
        let k = k.clamp(0, self.layers - 1) as usize;
        let at = |dx: usize, dz: usize| self.nodes[(k * NODES + iz + dz) * NODES + ix + dx];
        lerp(lerp(at(0, 0), at(1, 0), tx), lerp(at(0, 1), at(1, 1), tx), tz)
    }

    /// Horizontal grid cell and weights of local column `(lx, lz)`.
    #[inline]
    fn cell(lx: i32, lz: i32) -> (usize, usize, f32, f32) {
        let (gx, gz) = (lx + NODE_XZ, lz + NODE_XZ);
        (
            (gx / NODE_XZ) as usize,
            (gz / NODE_XZ) as usize,
            (gx % NODE_XZ) as f32 / NODE_XZ as f32,
            (gz % NODE_XZ) as f32 / NODE_XZ as f32,
        )
    }

    /// Node layer and weight of height `y`.
    #[inline]
    fn level(&self, y: i32) -> (i32, f32) {
        let gy = y - WORLD_MIN_Y;
        (gy / NODE_Y - self.layer0, (gy % NODE_Y) as f32 / NODE_Y as f32)
    }

    /// The column's surface height as seen at height `y`, given the noise
    /// there. Above the base the surface reaches less far up than down and
    /// is squashed the higher it goes, so the noise's highs don't leave
    /// rock floating clear of the ground. (`y` below the base, such as
    /// `f32::MIN`, gives the unsquashed extremes for the bounds.)
    #[inline]
    fn surface_at(&self, (sx, sz): (usize, usize), n: f32, y: f32) -> f32 {
        let (base, amp) = (self.base[sz][sx], self.amp[sz][sx]);
        let n = if n > 0.0 { n * UPWARD } else { n };
        let above = (y - base).max(0.0);
        let h = base + amp * n - SQUASH * above * above / amp;
        let h = if self.terraced[sz][sx] && h > SEA_LEVEL as f32 + 6.0 { terrace(h) } else { h };
        h + self.extra[sz][sx]
    }

    /// The interpolated relief noise at a block.
    #[inline]
    fn noise(&self, lx: i32, y: i32, lz: i32) -> f32 {
        let (ix, iz, tx, tz) = Self::cell(lx, lz);
        let (k, ty) = self.level(y);
        lerp(self.layer(ix, iz, tx, tz, k), self.layer(ix, iz, tx, tz, k + 1), ty)
    }

    /// Density in blocks: how far the surface seen at `y` lies above `y`.
    #[inline]
    fn density(&self, lx: i32, y: i32, lz: i32) -> f32 {
        self.surface_at(Self::slot(lx, lz), self.noise(lx, y, lz), y as f32) - y as f32
    }

    /// Whether the block at local column `(lx, lz)`, height `y`, is solid
    /// (before caves). `lx` and `lz` may be -1 or 32 (the ring).
    pub(super) fn solid(&self, lx: i32, y: i32, lz: i32) -> bool {
        if y <= FLOOR {
            return true;
        }
        if y > CEILING {
            return false;
        }
        let s = Self::slot(lx, lz);
        let (bot, top) = self.bounds[s.1][s.0];
        if y <= bot {
            return true;
        }
        if y > top {
            return false;
        }
        y as f32 <= self.surface_at(s, self.noise(lx, y, lz), y as f32)
    }

    /// [`Relief::solid`] for the 64 heights from `y0` up, as bits. The
    /// same arithmetic as `solid`, with each node layer interpolated once.
    fn mask(&self, lx: i32, lz: i32, y0: i32) -> u64 {
        let s = Self::slot(lx, lz);
        let (bot, top) = self.bounds[s.1][s.0];
        let (ix, iz, tx, tz) = Self::cell(lx, lz);
        let mut mask = 0u64;
        let mut cached = (i32::MIN, 0.0, 0.0);
        for i in 0..64 {
            let y = y0 + i;
            let solid = if y <= FLOOR || (y <= bot && y <= CEILING) {
                true
            } else if y > CEILING || y > top {
                false
            } else {
                let (k, ty) = self.level(y);
                if cached.0 != k {
                    cached = (k, self.layer(ix, iz, tx, tz, k), self.layer(ix, iz, tx, tz, k + 1));
                }
                y as f32 <= self.surface_at(s, lerp(cached.1, cached.2, ty), y as f32)
            };
            mask |= (solid as u64) << i;
        }
        mask
    }

    /// The highest block caves may open in a column: where the relief is
    /// large, nothing above the base height, so tunnels don't cut through
    /// spires and leave their caps floating. Lower slopes and flatter land
    /// keep their cave entrances.
    fn cave_ceiling(&self, lx: i32, lz: i32) -> f32 {
        let (sx, sz) = Self::slot(lx, lz);
        if self.amp[sz][sx] > SPIRE_RELIEF { self.base[sz][sx] } else { f32::MAX }
    }

    /// Generation 1's steepness rule (the largest height step across a
    /// column) measured on the 3D surface at height `y`: the horizontal
    /// change in density over the vertical one. Faces that lean out or
    /// overhang count as very steep.
    fn steepness(&self, lx: i32, y: i32, lz: i32) -> i32 {
        let d = |dx: i32, dy: i32, dz: i32| self.density(lx + dx, y + dy, lz + dz);
        let across = (d(1, 0, 0) - d(-1, 0, 0)).abs().max((d(0, 0, 1) - d(0, 0, -1)).abs());
        let up = d(0, 0, 0) - d(0, 1, 0);
        (across / up.max(0.25)).min(64.0) as i32
    }
}

impl Generator {
    /// Whether this generator shapes the Overworld from density (generation 2).
    #[inline]
    pub(super) fn density_terrain(&self) -> bool {
        self.generation >= Generation::V2 && self.dimension == Dimension::Overworld
    }

    /// Relief of a column from its climate, base height and biome.
    fn column_relief(&self, climate: &Climate, h: f32, biome: Biome) -> f32 {
        let r = self.climate.relief(climate, h);
        if biome.is_badlands() && h > SEA_LEVEL as f32 + 6.0 { r.max(BADLANDS_RELIEF) } else { r }
    }

    /// The columns of chunk column `pos` for generation 2, with heights
    /// read off the density field.
    pub(super) fn chunk_columns_v2(&self, pos: IVec2) -> ChunkColumns {
        let origin = IVec2::new(pos.x * CHUNK_SIZE_I, pos.y * CHUNK_SIZE_I);
        let node = |i: usize| (i as i32 - 1) * NODE_XZ;
        // Relief at the nodes, interpolated to columns so it changes
        // smoothly across biome borders.
        let mut relief_nodes = [[0.0f32; NODES]; NODES];
        for (iz, row) in relief_nodes.iter_mut().enumerate() {
            for (ix, r) in row.iter_mut().enumerate() {
                let (x, z) = (origin.x + node(ix), origin.y + node(iz));
                let climate = self.climate.sample(x, z);
                let h = self.climate.height(&climate, x, z);
                *r = self.column_relief(&climate, h, super::super::biome::pick(&climate));
            }
        }
        let blank = Column { height: 0, biome: Biome::Plains, frozen: false, climate: Climate::default() };
        let mut cols = [[blank; W]; W];
        let mut relief = Box::new(Relief {
            base: [[0.0; W]; W],
            amp: [[0.0; W]; W],
            extra: [[0.0; W]; W],
            terraced: [[false; W]; W],
            bounds: [[(0, 0); W]; W],
            nodes: Box::new([]),
            layer0: 0,
            layers: 0,
        });
        let (mut lo, mut hi) = (i32::MAX, i32::MIN);
        for (sz, row) in cols.iter_mut().enumerate() {
            for (sx, col) in row.iter_mut().enumerate() {
                let (lx, lz) = (sx as i32 - M as i32, sz as i32 - M as i32);
                let (x, z) = (origin.x + lx, origin.y + lz);
                let climate = self.climate.sample(x, z);
                let biome = super::super::biome::pick(&climate);
                let base = self.climate.height(&climate, x, z);
                let extra = if biome == Biome::ErodedBadlands && base > SEA_LEVEL as f32 + 6.0 {
                    let n = self.feature.fbm2(x as f32 / 11.0, z as f32 / 11.0, 2);
                    ((n - 0.18) * 70.0).clamp(0.0, 16.0)
                } else {
                    0.0
                };
                let (ix, iz, tx, tz) = Relief::cell(lx, lz);
                let r = |dx: usize, dz: usize| relief_nodes[iz + dz][ix + dx];
                let amp = lerp(lerp(r(0, 0), r(1, 0), tx), lerp(r(0, 1), r(1, 1), tx), tz);
                relief.base[sz][sx] = base;
                relief.amp[sz][sx] = amp;
                relief.extra[sz][sx] = extra;
                relief.terraced[sz][sx] = biome.is_badlands();
                let bot = relief.surface_at((sx, sz), -1.0, f32::MIN).floor() as i32 - 2;
                let top = relief.surface_at((sx, sz), 1.0, f32::MIN).ceil() as i32 + 2;
                relief.bounds[sz][sx] = (bot, top);
                lo = lo.min(bot);
                hi = hi.max(top);
                *col = Column { height: 0, biome, frozen: biome.is_cold(), climate };
            }
        }
        // Node layers covering the band, with one to spare on each side.
        let layer0 = ((lo.max(WORLD_MIN_Y) - WORLD_MIN_Y) / NODE_Y - 1).max(0);
        let last = ((hi.min(WORLD_MAX_Y) - WORLD_MIN_Y) / NODE_Y + 2).min(LAYERS - 1);
        let layers = (last - layer0 + 1).max(2);
        let mut nodes = vec![0.0f32; layers as usize * NODES * NODES];
        for k in 0..layers {
            let y = (WORLD_MIN_Y + (layer0 + k) * NODE_Y) as f32;
            for iz in 0..NODES {
                for ix in 0..NODES {
                    let (x, z) = ((origin.x + node(ix)) as f32, (origin.y + node(iz)) as f32);
                    nodes[(k as usize * NODES + iz) * NODES + ix] = self.climate.relief_noise(x, y, z);
                }
            }
        }
        relief.nodes = nodes.into_boxed_slice();
        relief.layer0 = layer0;
        relief.layers = layers;
        // Heights: the topmost solid block of each column.
        let (mut max_h, mut min_h) = (i32::MIN, i32::MAX);
        for (sz, row) in cols.iter_mut().enumerate() {
            for (sx, col) in row.iter_mut().enumerate() {
                let (lx, lz) = (sx as i32 - M as i32, sz as i32 - M as i32);
                let (bot, top) = relief.bounds[sz][sx];
                let floor = bot.clamp(FLOOR, CEILING);
                let height = (floor + 1..=top.min(CEILING)).rev().find(|&y| relief.solid(lx, y, lz)).unwrap_or(floor);
                col.height = height;
                if (0..CHUNK_SIZE_I).contains(&lx) && (0..CHUNK_SIZE_I).contains(&lz) {
                    max_h = max_h.max(height);
                    min_h = min_h.min(height);
                }
            }
        }
        ChunkColumns { cols, max_h, min_h, relief: Some(relief) }
    }

    /// Generation 2's column: the topmost solid block of the density field.
    pub(super) fn column_v2(&self, x: i32, z: i32) -> Column {
        let columns = self.chunk_columns(IVec2::new(x.div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I)));
        *columns.at(x.rem_euclid(CHUNK_SIZE_I) as usize, z.rem_euclid(CHUNK_SIZE_I) as usize)
    }

    /// The block generation 2 puts at a solid block: surface rules on
    /// every face open to the air above, filler below it, stone deeper
    /// down, and stone (or sandstone, which doesn't fall) under overhangs.
    /// `up` is how far the open air above is (1: right above), and
    /// `open_below` whether the block below is open.
    #[allow(clippy::too_many_arguments)]
    fn solid_block(
        &self,
        relief: &Relief,
        col: Column,
        (lx, lz): (i32, i32),
        p: IVec3,
        up: u32,
        open_below: bool,
    ) -> Block {
        let (wx, wy, wz) = (p.x, p.y, p.z);
        let rock = || {
            if wy < 0 || (wy < 8 && hash_f(wx, wy, wz, self.seed ^ 0xDEE) < (8 - wy) as f32 / 8.0) {
                Block::DEEPSLATE
            } else {
                Block::STONE
            }
        };
        let b = if up > MASK_BELOW as u32 {
            if col.biome.is_badlands() && wy >= SEA_LEVEL - 6 { self.stratum(wy) } else { rock() }
        } else {
            // The face this block lies under, as a column of that height.
            let face = Column { height: wy + up as i32 - 1, ..col };
            if up as i32 - 1 > Self::soil_depth(face) {
                if col.biome.is_badlands() && wy >= SEA_LEVEL - 6 { self.stratum(wy) } else { rock() }
            } else if up == 1 {
                let steep_rule = col.biome.is_mountain() || col.biome.is_peak() || col.biome == Biome::StonyShore;
                let steep = if steep_rule { relief.steepness(lx, wy, lz) } else { 0 };
                self.surface_block(face, wx, wz, steep)
            } else {
                self.filler_block(face, wy)
            }
        };
        if !open_below {
            return b;
        }
        match b {
            Block::SAND => Block::SANDSTONE,
            Block::RED_SAND => crate::world::stone_blocks::RED_SANDSTONE,
            Block::GRASS | Block::SNOWY_GRASS | Block::DIRT | Block::GRAVEL => rock(),
            b if b == ob::PODZOL || b == ob::COARSE_DIRT || b == ob::MYCELIUM || b == ob::MUD => rock(),
            b if b == ob::PALE_MOSS_BLOCK => rock(),
            b => b,
        }
    }

    /// The block generation 2 puts on top of column `(lx, lz)` of `columns`.
    /// `(wx, wz)` is the column's world position.
    pub(super) fn top_block_v2(&self, columns: &ChunkColumns, lx: usize, lz: usize, wx: i32, wz: i32) -> Block {
        let relief = columns.relief.as_deref().expect("generation 2 columns");
        let col = *columns.at(lx, lz);
        let (lx, lz) = (lx as i32, lz as i32);
        let open_below = !relief.solid(lx, col.height - 1, lz);
        self.solid_block(relief, col, (lx, lz), IVec3::new(wx, col.height, wz), 1, open_below)
    }

    /// The block on top of world column `(x, z)` (for trees decided
    /// outside their chunk).
    pub(super) fn top_block_at_v2(&self, x: i32, z: i32) -> Block {
        let columns = self.chunk_columns(IVec2::new(x.div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I)));
        self.top_block_v2(&columns, x.rem_euclid(CHUNK_SIZE_I) as usize, z.rem_euclid(CHUNK_SIZE_I) as usize, x, z)
    }

    /// Generation 2's [`Generator::fill`]: rock, soil and surfaces from the
    /// density field, and the sea in every open block below sea level.
    /// Returns the solid masks the caves use.
    pub(super) fn fill_v2(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) -> Masks {
        let relief = columns.relief.as_deref().expect("generation 2 columns");
        let y0 = base.y - MASK_BELOW;
        let mut masks: Masks = Box::new([0; CHUNK_SIZE * CHUNK_SIZE]);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let mask = relief.mask(x as i32, z as i32, y0);
                masks[x + z * CHUNK_SIZE] = mask;
                let col = *columns.at(x, z);
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                for y in 0..CHUNK_SIZE {
                    let bit = MASK_BELOW as u32 + y as u32;
                    let p = IVec3::new(wx, base.y + y as i32, wz);
                    let b = if mask >> bit & 1 == 0 {
                        if p.y >= SEA_LEVEL {
                            continue;
                        }
                        if p.y == SEA_LEVEL - 1 && col.frozen && self.ice_sheet(wx, wz, col.biome) {
                            Block::ICE
                        } else {
                            Block::WATER
                        }
                    } else if self.bedrock_at(p) {
                        Block::BEDROCK
                    } else {
                        let open_below = mask >> (bit - 1) & 1 == 0;
                        self.solid_block(relief, col, (x as i32, z as i32), p, open_above(mask, bit), open_below)
                    };
                    blocks[index(x, y, z)] = b;
                }
            }
        }
        masks
    }

    /// Whether caves may open the solid block at mask bit `bit`, and how
    /// deep under its own surface it is: not in thin rock over open air
    /// (overhangs and arches), not above `ceiling` (see
    /// [`Relief::cave_ceiling`]) and not right under the sea or a river.
    #[inline]
    fn cave_depth_v2(mask: u64, bit: u32, wy: i32, ceiling: f32) -> Option<i32> {
        let up = open_above(mask, bit);
        if open_below(mask, bit) <= SLAB || wy as f32 > ceiling || (up <= 2 && wy + up as i32 <= SEA_LEVEL) {
            return None;
        }
        Some(up.min(MASK_BELOW as u32 + 1) as i32 - 1)
    }

    /// The surface height generation 2's aquifers flood against: the
    /// lowest the column's surface can reach, from climate alone, so it is
    /// the same whichever chunk asks.
    fn aquifer_surface_v2(&self, x: i32, z: i32) -> i32 {
        let climate = self.climate.sample(x, z);
        let h = self.climate.height(&climate, x, z);
        (h - self.column_relief(&climate, h, super::super::biome::pick(&climate))) as i32
    }

    /// Generation 2's [`Generator::carve`]: noise caves, ravines and
    /// aquifers in the solid parts of the density field. Depth is measured
    /// from the nearest open air above, so caverns stay sealed under
    /// overhangs too; aquifer water never opens onto dry air beside it.
    pub(super) fn carve_v2(
        &self,
        blocks: &mut [Block; CHUNK_VOLUME],
        base: IVec3,
        columns: &ChunkColumns,
        masks: &Masks,
    ) {
        let relief = columns.relief.as_deref().expect("generation 2 columns");
        let field = self.caves.field(base);
        let ravines = self.caves.ravines(base);
        let mut aquifer = Aquifer::new(&self.caves, |x, z| self.aquifer_surface_v2(x, z));
        let y0 = base.y - MASK_BELOW;
        // Open, dry air beside a block (fluids must not spill into it).
        let dry_air = |x: i32, y: i32, z: i32| {
            if y < SEA_LEVEL {
                return false;
            }
            if (0..CHUNK_SIZE_I).contains(&x) && (0..CHUNK_SIZE_I).contains(&z) {
                masks[x as usize + z as usize * CHUNK_SIZE] >> (y - y0) & 1 == 0
            } else {
                !relief.solid(x, y, z)
            }
        };
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = columns.at(x, z);
                let limit = (col.height - base.y).min(CHUNK_SIZE_I - 1);
                if limit < 0 {
                    continue;
                }
                let mask = masks[x + z * CHUNK_SIZE];
                let profile = Caves::profile(&field, x, z);
                let ceiling = relief.cave_ceiling(x as i32, z as i32);
                for y in 0..=limit as usize {
                    let bit = MASK_BELOW as u32 + y as u32;
                    let wy = base.y + y as i32;
                    let i = index(x, y, z);
                    if mask >> bit & 1 == 0 || blocks[i] == Block::BEDROCK {
                        continue;
                    }
                    let Some(depth) = Self::cave_depth_v2(mask, bit, wy, ceiling) else { continue };
                    let ravine = ravines.as_ref().is_some_and(|m| m[i]) && wy > super::super::caves::CARVE_FLOOR;
                    if !ravine && !Caves::carved(&profile, y, wy, depth) {
                        continue;
                    }
                    let p = IVec3::new(base.x + x as i32, wy, base.z + z as i32);
                    let Some(fill) = aquifer.fill(p) else { continue };
                    // Fluid stays in: not over a dry aquifer (where the cell
                    // window shifts, the aquifer can switch without a
                    // barrier) and not beside open dry air.
                    let (lx, lz) = (x as i32, z as i32);
                    if fill != Block::AIR
                        && (aquifer.fill(p - IVec3::Y) == Some(Block::AIR)
                            || [(1, 0), (-1, 0), (0, 1), (0, -1)]
                                .into_iter()
                                .any(|(dx, dz)| dry_air(lx + dx, wy, lz + dz)))
                    {
                        continue;
                    }
                    blocks[i] = fill;
                }
            }
        }
    }

    /// Generation 2's [`Generator::natural_block`].
    pub(super) fn natural_block_v2(&self, p: IVec3, nodes: &mut FxHashMap<IVec3, [f32; 7]>) -> Block {
        let columns = self.chunk_columns(IVec2::new(p.x.div_euclid(CHUNK_SIZE_I), p.z.div_euclid(CHUNK_SIZE_I)));
        let relief = columns.relief.as_deref().expect("generation 2 columns");
        let (lx, lz) = (p.x.rem_euclid(CHUNK_SIZE_I), p.z.rem_euclid(CHUNK_SIZE_I));
        let y0 = p.y - MASK_BELOW;
        let mask = relief.mask(lx, lz, y0);
        let bit = MASK_BELOW as u32;
        if mask >> bit & 1 == 0 {
            return if p.y < SEA_LEVEL { Block::WATER } else { Block::AIR };
        }
        if p.y <= BOTTOM + 4 {
            return Block::BEDROCK;
        }
        match Self::cave_depth_v2(mask, bit, p.y, relief.cave_ceiling(lx, lz)) {
            Some(depth) if self.caves.carved_at(p, depth, nodes) => {
                if p.y < LAVA_LEVEL {
                    Block::LAVA
                } else {
                    Block::AIR
                }
            }
            _ => Block::STONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2(seed: u64) -> Generator {
        Generator::with_generation(seed, Dimension::Overworld, Generation::V2)
    }

    /// A mountainous chunk column for `g`, near the origin.
    fn mountains(g: &Generator) -> IVec2 {
        let p = g
            .nearest_biome(IVec2::ZERO, Biome::JaggedPeaks, 20_000)
            .or_else(|| g.nearest_biome(IVec2::ZERO, Biome::StonyPeaks, 20_000))
            .expect("peaks");
        IVec2::new(p.x.div_euclid(CHUNK_SIZE_I), p.z.div_euclid(CHUNK_SIZE_I))
    }

    fn column_blocks(g: &Generator, x: i32, z: i32, chunks: &mut FxHashMap<IVec3, ChunkData>) -> Vec<Block> {
        let (cx, cz) = (x.div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I));
        let (lx, lz) = (x.rem_euclid(CHUNK_SIZE_I) as usize, z.rem_euclid(CHUNK_SIZE_I) as usize);
        Dimension::Overworld
            .chunk_rows()
            .flat_map(|cy| {
                let chunk = chunks.entry(IVec3::new(cx, cy, cz)).or_insert_with(|| g.generate(IVec3::new(cx, cy, cz)));
                (0..CHUNK_SIZE).map(|y| chunk.get(lx, y, lz)).collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn masks_match_point_samples() {
        let g = v2(5);
        let pos = mountains(&g);
        let columns = g.chunk_columns(pos);
        let relief = columns.relief.as_deref().unwrap();
        for (lx, lz) in [(0, 0), (5, 17), (31, 31), (-1, 12), (32, -1)] {
            for y0 in (-80..320).step_by(37) {
                let mask = relief.mask(lx, lz, y0);
                for i in 0..64 {
                    assert_eq!(mask >> i & 1 == 1, relief.solid(lx, y0 + i, lz), "{lx},{lz} y {}", y0 + i);
                }
            }
        }
        assert_eq!(open_above(0b0111, 0), 3);
        assert_eq!(open_below(0b1110, 3), 3);
        assert_eq!(open_below(!0, 5), 65);
    }

    /// The ring of one chunk column agrees with the neighbour's own columns.
    #[test]
    fn neighbouring_chunk_columns_agree() {
        let g = v2(5);
        let pos = mountains(&g);
        let here = g.chunk_columns(pos);
        let east = g.chunk_columns(pos + IVec2::X);
        for z in 0..CHUNK_SIZE {
            assert_eq!(here.cols[z + M][CHUNK_SIZE + M].height, east.at(0, z).height);
            assert_eq!(east.cols[z + M][0].height, here.at(CHUNK_SIZE - 1, z).height);
        }
    }

    /// Generation 2 makes overhangs: columns with open air between solid
    /// blocks above the base of the relief band, in mountains.
    #[test]
    fn mountains_overhang() {
        let g = v2(12345);
        let pos = mountains(&g);
        let mut overhangs = 0;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let columns = g.chunk_columns(pos + IVec2::new(dx, dz));
                let relief = columns.relief.as_deref().unwrap();
                for z in 0..CHUNK_SIZE_I {
                    for x in 0..CHUNK_SIZE_I {
                        let h = columns.at(x as usize, z as usize).height;
                        let mut seen_air = false;
                        for y in (h - 60..h).rev() {
                            if !relief.solid(x, y, z) {
                                seen_air = true;
                            } else if seen_air && y > SEA_LEVEL {
                                overhangs += 1;
                                break;
                            }
                        }
                    }
                }
            }
        }
        assert!(overhangs > 200, "{overhangs} overhanging columns");
    }

    /// Solid density blocks (before caves) not connected to the ground,
    /// in the 5x5 chunk columns around the peaks, and the total.
    fn floating_rock(g: &Generator, carved: bool) -> (usize, usize) {
        let pos = mountains(g);
        const SIDE: usize = 5 * CHUNK_SIZE;
        let (y0, y1) = (SEA_LEVEL, CEILING + 1);
        let height = (y1 - y0) as usize;
        let mut solid = vec![false; SIDE * SIDE * height];
        let at = |x: usize, y: usize, z: usize| (y * SIDE + z) * SIDE + x;
        for cz in 0..5 {
            for cx in 0..5 {
                let c = pos + IVec2::new(cx as i32 - 2, cz as i32 - 2);
                let columns = g.chunk_columns(c);
                let relief = columns.relief.as_deref().unwrap();
                let mut chunks = FxHashMap::default();
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        for y in 0..height {
                            let (wx, wz, wy) = (cx * CHUNK_SIZE + x, cz * CHUNK_SIZE + z, y0 + y as i32);
                            solid[at(wx, y, wz)] = if carved {
                                let cy = wy.div_euclid(CHUNK_SIZE_I);
                                let chunk = chunks.entry(cy).or_insert_with(|| g.generate(IVec3::new(c.x, cy, c.y)));
                                let b = chunk.get(x, wy.rem_euclid(CHUNK_SIZE_I) as usize, z);
                                b.is_opaque() && !b.is_leaves() && !b.is_log()
                            } else {
                                relief.solid(x as i32, wy, z as i32)
                            };
                        }
                    }
                }
            }
        }
        // Grounded: reached from the bottom layer or the region's sides.
        let mut grounded = vec![false; solid.len()];
        let mut queue = Vec::new();
        for z in 0..SIDE {
            for x in 0..SIDE {
                for y in 0..height {
                    let edge = y == 0 || x == 0 || z == 0 || x == SIDE - 1 || z == SIDE - 1;
                    if edge && solid[at(x, y, z)] {
                        grounded[at(x, y, z)] = true;
                        queue.push((x, y, z));
                    }
                }
            }
        }
        while let Some((x, y, z)) = queue.pop() {
            let mut visit = |x: usize, y: usize, z: usize| {
                let i = at(x, y, z);
                if solid[i] && !grounded[i] {
                    grounded[i] = true;
                    queue.push((x, y, z));
                }
            };
            if x > 0 {
                visit(x - 1, y, z)
            }
            if x + 1 < SIDE {
                visit(x + 1, y, z)
            }
            if z > 0 {
                visit(x, y, z - 1)
            }
            if z + 1 < SIDE {
                visit(x, y, z + 1)
            }
            if y > 0 {
                visit(x, y - 1, z)
            }
            if y + 1 < height {
                visit(x, y + 1, z)
            }
        }
        let total = solid.iter().filter(|&&s| s).count();
        let floating = solid.iter().zip(&grounded).filter(|&(&s, &g)| s && !g).count();
        if let Some(i) = (0..solid.len()).rev().find(|&i| solid[i] && !grounded[i]) {
            let (x, z, y) = (i % SIDE, i / SIDE % SIDE, i / (SIDE * SIDE));
            let origin = (pos - IVec2::splat(2)) * CHUNK_SIZE_I;
            println!("highest floating block: {}", IVec3::new(origin.x + x as i32, y0 + y as i32, origin.y + z as i32));
        }
        (floating, total)
    }

    /// Hardly any rock floats free of the ground in the mountains.
    #[test]
    fn little_rock_floats() {
        for seed in [12345, 42] {
            for carved in [false, true] {
                let (floating, total) = floating_rock(&v2(seed), carved);
                println!("seed {seed}, caves {carved}: {floating} of {total} solid blocks above the sea float");
                assert!(floating * 1000 < total, "seed {seed}, caves {carved}: {floating} of {total} float");
            }
        }
    }

    /// `column().height` is the topmost generated solid (not cave) block.
    #[test]
    fn column_heights_are_the_generated_tops() {
        let g = v2(12345);
        let pos = mountains(&g);
        let mut chunks = FxHashMap::default();
        let ground = |b: Block| {
            matches!(b, Block::STONE | Block::DEEPSLATE | Block::DIRT | Block::GRASS | Block::SNOWY_GRASS)
                || matches!(b, Block::SAND | Block::SANDSTONE | Block::GRAVEL | Block::RED_SAND | Block::SNOW)
        };
        let mut tops = 0;
        for (dx, dz) in [(3, 7), (17, 2), (30, 30), (12, 25), (0, 16), (21, 9)] {
            for (cx, cz) in [(0, 0), (1, 0), (-1, 1), (40, -3)] {
                let (x, z) = ((pos.x + cx) * CHUNK_SIZE_I + dx, (pos.y + cz) * CHUNK_SIZE_I + dz);
                let column = column_blocks(&g, x, z, &mut chunks);
                let h = g.column(x, z).height;
                let i = (h - BOTTOM) as usize;
                assert!(!column[i + 1..].iter().any(|&b| ground(b)), "{x},{z}: ground above the top {h}");
                // The top is ground unless a cave entrance opened it.
                if !matches!(column[i], Block::AIR | Block::WATER | Block::LAVA) {
                    assert!(column[i].is_opaque(), "{x},{z} top {h}: {:?}", column[i]);
                    tops += 1;
                }
            }
        }
        assert!(tops > 15, "{tops}");
    }

    /// Chunks are the same whatever order (and thread) generates them.
    #[test]
    fn chunks_do_not_depend_on_generation_order() {
        let pos = mountains(&v2(777));
        let mut cells: Vec<IVec3> = (-1..=1)
            .flat_map(|dx| (-1..=1).flat_map(move |dz| (1..8).map(move |cy| IVec3::new(pos.x + dx, cy, pos.y + dz))))
            .collect();
        let hash = |c: &ChunkData| {
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            c.for_each_block(|b| h = (h ^ u64::from(b.0)).wrapping_mul(0x100_0000_01b3));
            h
        };
        let forward: Vec<u64> = {
            let g = v2(777);
            cells.iter().map(|&p| hash(&g.generate(p))).collect()
        };
        cells.reverse();
        let backward: FxHashMap<IVec3, u64> = {
            let g = Arc::new(v2(777));
            let chunks: Vec<_> = cells
                .chunks(9)
                .map(|part| {
                    let g = Arc::clone(&g);
                    let part = part.to_vec();
                    std::thread::spawn(move || part.into_iter().map(|p| (p, hash(&g.generate(p)))).collect::<Vec<_>>())
                })
                .collect();
            chunks.into_iter().flat_map(|t| t.join().unwrap()).collect()
        };
        cells.reverse();
        for (p, h) in cells.iter().zip(forward) {
            assert_eq!(backward[p], h, "chunk {p}");
        }
    }

    #[test]
    fn spawn_is_on_dry_land() {
        for seed in [7, 12345, 99] {
            let g = v2(seed);
            let s = g.find_spawn();
            let c = g.column(s.x, s.z);
            assert!(c.height >= SEA_LEVEL && !c.biome.is_watery(), "{seed}: {s} {:?}", c.biome);
            assert_eq!(s.y, c.height + 1);
        }
    }

    /// No water rests on open air, and no grass, sand or gravel hangs under
    /// an overhang.
    #[test]
    fn nothing_hangs_or_floods_into_open_air() {
        let g = v2(12345);
        let pos = mountains(&g);
        let columns = g.chunk_columns(pos);
        for cy in 1..9 {
            let base = IVec3::new(pos.x, cy, pos.y) * CHUNK_SIZE_I;
            let chunk = g.generate(IVec3::new(pos.x, cy, pos.y));
            let mut filled = ChunkData::new_dense(Block::AIR);
            g.fill_v2(&mut filled, base, &columns);
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    for y in 1..CHUNK_SIZE {
                        let wy = base.y + y as i32;
                        if chunk.get(x, y - 1, z) == Block::AIR {
                            assert!(chunk.get(x, y, z) != Block::WATER, "water over air at {x},{wy},{z}");
                        }
                        let (above, below) = (filled[index(x, y, z)], filled[index(x, y - 1, z)]);
                        if below == Block::AIR {
                            let loose = matches!(above, Block::SAND | Block::GRAVEL | Block::RED_SAND | Block::GRASS);
                            assert!(!loose, "{above:?} over air at {x},{wy},{z}");
                        }
                    }
                }
            }
        }
    }

    /// Prints where the nearest peaks are and how much of them overhangs,
    /// for screenshots and tuning (run with `--ignored --nocapture`).
    #[test]
    #[ignore]
    fn print_mountains() {
        for seed in [12345, 42] {
            let g = v2(seed);
            let pos = mountains(&g);
            let (mut best, mut best_at, mut columns_over, mut total) = (0, IVec3::ZERO, 0, 0);
            for dz in -2..=2 {
                for dx in -2..=2 {
                    let columns = g.chunk_columns(pos + IVec2::new(dx, dz));
                    let relief = columns.relief.as_deref().unwrap();
                    let mut here = 0;
                    for z in 0..CHUNK_SIZE_I {
                        for x in 0..CHUNK_SIZE_I {
                            let h = columns.at(x as usize, z as usize).height;
                            total += 1;
                            let mut gaps = 0;
                            let mut last = true;
                            for y in (h - 80..=h).rev() {
                                let s = relief.solid(x, y, z);
                                gaps += (s && !last) as i32;
                                last = s;
                            }
                            columns_over += (gaps > 0) as i32;
                            here += gaps;
                        }
                    }
                    if here > best {
                        best = here;
                        let c = pos + IVec2::new(dx, dz);
                        best_at = IVec3::new(c.x * 32 + 16, columns.at(16, 16).height, c.y * 32 + 16);
                    }
                }
            }
            println!(
                "seed {seed}: peaks chunk {pos}, {columns_over} of {total} columns overhang; most at {best_at} ({best})"
            );
        }
    }

    /// Prints places to look at generation 2 (run with `--ignored --nocapture`).
    #[test]
    #[ignore]
    fn print_places() {
        let g = v2(12345);
        for biome in [
            Biome::Badlands,
            Biome::ErodedBadlands,
            Biome::WindsweptHills,
            Biome::River,
            Biome::Beach,
            Biome::Plains,
            Biome::StonyShore,
        ] {
            if let Some(p) = g.nearest_biome(IVec2::ZERO, biome, 20_000) {
                println!("place {biome:?}: {p}");
            }
        }
    }

    /// Prints generation 1 and 2 chunk generation speed in mountains and
    /// around the origin (run with `--ignored --nocapture`).
    #[test]
    #[ignore]
    fn print_generation_speed() {
        for (name, centre) in [("origin", IVec2::ZERO), ("mountains", mountains(&v2(12345)))] {
            for generation in [Generation::V1, Generation::V2] {
                let g = Generator::with_generation(12345, Dimension::Overworld, generation);
                let t = std::time::Instant::now();
                let mut n = 0;
                for dz in -3..=3 {
                    for dx in -3..=3 {
                        for cy in Dimension::Overworld.chunk_rows() {
                            std::hint::black_box(g.generate(IVec3::new(centre.x + dx, cy, centre.y + dz)));
                            n += 1;
                        }
                    }
                }
                let ms = t.elapsed().as_secs_f64() * 1e3;
                println!("{name} {generation:?}: {n} chunks in {ms:.1} ms, {:.3} ms/chunk", ms / n as f64);
            }
        }
    }
}
