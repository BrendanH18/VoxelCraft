//! Generation 5 sulfur caves (v0.16 Track D, Java 26.3 `sulfur_caves`), a
//! child module of `terrain` so it shares the generator's columns:
//!
//! - `material_rule/overworld/sulfur_cave_bands`: stone in the biome turns
//!   to cinnabar where the 3D `sulfur_cave_gradient` noise is in
//!   -0.4..-0.1 or at least 0.4, and to sulfur in 0..0.4;
//! - `sulfur_spike_cluster` and `sulfur_spike` (speleothems of sulfur
//!   spikes on sulfur, up from floors and down from ceilings);
//! - `sulfur_pool` (a pool of water rimmed with sulfur over potent sulfur).
//!
//! Noise and random draws are the engine's own (FE-01); thresholds, block
//! choices and the order of the steps follow the reference. Differences:
//! the record `docs/verification/2026-11-20-v0.16-generation-5-overworld.md`.

use glam::IVec3;

use super::{Biome, Block, CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkColumns, Generator, index};
use crate::world::gen5_blocks::{self as g, Potent};
use crate::world::noise::{Perlin, splitmix64};
use crate::world::overworld_blocks::Thickness;

const NOISE_SALT: u64 = 0x0005_0F1E_CAFE;

/// `sulfur_cave_bands`: the block and its noise range, first match wins.
const BANDS: [(Block, f32, f32); 3] = [(g::CINNABAR, -0.4, -0.1), (g::SULFUR, 0.0, 0.4), (g::CINNABAR, 0.4, f32::MAX)];
const FEATURE_SALT: u64 = 0x0005_0F1E_5B1C;

impl Generator {
    /// Whether the cell at height `y` of column `(x, z)` of this chunk lies in
    /// the sulfur caves.
    fn sulfur_cell(&self, columns: &ChunkColumns, x: usize, y: i32, z: usize) -> bool {
        let c = columns.at(x, z);
        crate::world::deep_dark::cave_biome(self.generation, &c.climate, c.height, y) == Some(Biome::SulfurCaves)
    }

    /// Whether any column of this chunk can hold sulfur caves (the climate
    /// test alone, before depths).
    fn any_sulfur_column(&self, columns: &ChunkColumns) -> bool {
        (0..CHUNK_SIZE).any(|z| {
            (0..CHUNK_SIZE).any(|x| {
                let c = columns.at(x, z);
                crate::world::biome::cave_in(&c.climate, 40, true) == Some(Biome::SulfurCaves)
            })
        })
    }

    /// `sulfur_cave_bands`: cinnabar and sulfur strata in the biome's stone.
    pub(super) fn sulfur_bands(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        if !self.any_sulfur_column(columns) {
            return;
        }
        let noise = Perlin::new(self.seed ^ NOISE_SALT);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    let i = index(x, y, z);
                    if !matches!(blocks[i], Block::STONE | Block::DEEPSLATE) {
                        continue;
                    }
                    let wy = base.y + y as i32;
                    if !self.sulfur_cell(columns, x, wy, z) {
                        continue;
                    }
                    let p = (base + IVec3::new(x as i32, y as i32, z as i32)).as_vec3();
                    let n = noise.noise3(p.x / 32.0, p.y / 32.0, p.z / 32.0)
                        + 0.25 * noise.noise3(p.x / 8.0 + 31.0, p.y / 8.0, p.z / 8.0 - 17.0);
                    match BANDS.iter().find(|(_, lo, hi)| (*lo..=*hi).contains(&n)) {
                        Some(&(band, _, _)) => blocks[i] = band,
                        None => continue,
                    }
                }
            }
        }
    }

    /// `sulfur_spike_cluster`, `sulfur_spike` and `sulfur_pool` in the
    /// chunk's sulfur-cave cells. Every feature stays inside the chunk.
    pub(super) fn sulfur_features(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        if !self.any_sulfur_column(columns) {
            return;
        }
        let mut rng = self.seed
            ^ FEATURE_SALT
            ^ (base.x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (base.y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (base.z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        let mut int = |n: u64| splitmix64(&mut rng) % n;
        let solid = |b: Block| b.is_opaque();
        // Pools first (Java's lakes step comes before underground decoration).
        for _ in 0..3 {
            let (x, z) = (1 + int(14) as usize, 1 + int(14) as usize);
            let y = 1 + int(14) as usize;
            if !self.sulfur_cell(columns, x, base.y + y as i32, z) {
                continue;
            }
            self.sulfur_pool(blocks, x, y, z);
        }
        // Spikes: from floors upward and from ceilings downward, each on a
        // layer of sulfur (`base_block`), one to four blocks long.
        let attempts = 24 + int(24);
        for _ in 0..attempts {
            let (x, y, z) = (int(16) as usize, int(16) as usize, int(16) as usize);
            if blocks[index(x, y, z)] != Block::AIR || !self.sulfur_cell(columns, x, base.y + y as i32, z) {
                continue;
            }
            let down = int(2) == 1;
            // Scan to the floor (or ceiling), at most twelve blocks.
            let mut root = y as i32;
            loop {
                let next = if down { root + 1 } else { root - 1 };
                if !(0..CHUNK_SIZE_I).contains(&next) || (root - y as i32).abs() > 12 {
                    root = -1;
                    break;
                }
                let b = blocks[index(x, next as usize, z)];
                if solid(b) {
                    break;
                }
                if b != Block::AIR {
                    root = -1;
                    break;
                }
                root = next;
            }
            if root < 0 {
                continue;
            }
            // The `base_block` layer: the support turns to sulfur.
            let support = index(x, (if down { root + 1 } else { root - 1 }) as usize, z);
            let b = blocks[support];
            if matches!(b, Block::STONE | Block::DEEPSLATE | Block::TUFF) || b == g::CINNABAR {
                blocks[support] = g::SULFUR;
            }
            if blocks[support] != g::SULFUR {
                continue;
            }
            let length = 1 + int(4) as i32;
            let mut cells = Vec::new();
            for k in 0..length {
                let cy = if down { root - k } else { root + k };
                if !(0..CHUNK_SIZE_I).contains(&cy) || blocks[index(x, cy as usize, z)] != Block::AIR {
                    break;
                }
                cells.push(cy);
            }
            let n = cells.len();
            for (k, &cy) in cells.iter().enumerate() {
                // `DripstoneUtils.buildBaseToTipColumn`: base, middles,
                // frustum, tip from the root outward.
                let t = match (n, k) {
                    (_, k) if k + 1 == n => Thickness::Tip,
                    (_, k) if k + 2 == n => Thickness::Frustum,
                    (_, 0) => Thickness::Base,
                    _ => Thickness::Middle,
                };
                blocks[index(x, cy as usize, z)] = g::spike(down, t, false);
            }
        }
    }

    /// A small `sulfur_pool`: water three by three over a sulfur floor with
    /// wet potent sulfur under its middle, where the floor and its rim are
    /// solid and the cells above are open.
    fn sulfur_pool(&self, blocks: &mut [Block; CHUNK_VOLUME], x: usize, y: usize, z: usize) {
        // Find the floor under (x, y, z): an open cell over a solid one.
        let mut fy = y;
        while fy > 1 && blocks[index(x, fy - 1, z)] == Block::AIR {
            fy -= 1;
        }
        if !(2..CHUNK_SIZE - 1).contains(&fy) || blocks[index(x, fy, z)] != Block::AIR {
            return;
        }
        let floor = fy - 1;
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                let (cx, cz) = ((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                if !blocks[index(cx, floor, cz)].is_opaque()
                    || !blocks[index(cx, floor - 1, cz)].is_opaque()
                    || blocks[index(cx, floor + 1, cz)] != Block::AIR
                {
                    return;
                }
            }
        }
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                let (cx, cz) = ((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                blocks[index(cx, floor, cz)] = Block::WATER;
                blocks[index(cx, floor - 1, cz)] = g::SULFUR;
            }
        }
        blocks[index(x, floor - 1, z)] = g::potent(Potent::Wet);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Dimension, Generation};
    use super::*;
    use glam::IVec2;

    /// [`BANDS`] against `tests/fixtures/java/26.3/gen5_overworld.json`.
    #[test]
    fn bands_match_the_java_material_rule() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/gen5_overworld.json")).unwrap();
        let java = f["sulfur_cave_bands"].as_array().unwrap();
        assert_eq!(java.len(), BANDS.len());
        for (j, &(block, lo, hi)) in java.iter().zip(&BANDS) {
            assert_eq!(j["block"].as_str().unwrap().replace('_', " "), block.name());
            assert_eq!(j["min"].as_f64().unwrap() as f32, lo);
            assert!(j["max"].as_f64().unwrap() as f32 == hi || (hi == f32::MAX && j["max"].as_f64().unwrap() > 1e300));
        }
    }

    /// A generation 5 world holds sulfur caves with their bands, spikes
    /// and potent sulfur; generation 4 holds none of them.
    #[test]
    fn generation_5_grows_sulfur_caves() {
        let g = Generator::with_generation(12345, Dimension::Overworld, Generation::V5);
        let at = g.nearest_biome(IVec2::ZERO, Biome::SulfurCaves, 6000).expect("a sulfur cave within 6000 blocks");
        let (mut cinnabar, mut sulfur, mut spikes) = (0, 0, 0);
        let c = at.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        for dx in -3..=3 {
            for dz in -3..=3 {
                for dy in -3..=1 {
                    g.generate(c + IVec3::new(dx, dy, dz)).for_each_block(|b| {
                        cinnabar += usize::from(b == g::CINNABAR);
                        sulfur += usize::from(b == g::SULFUR);
                        spikes += usize::from(g::spike_of(b).is_some());
                    });
                }
            }
        }
        assert!(cinnabar > 0 && sulfur > 0, "bands: {cinnabar} cinnabar, {sulfur} sulfur near {at}");
        assert!(spikes > 0, "spikes near {at}");
        let old = Generator::with_generation(12345, Dimension::Overworld, Generation::V4);
        let mut any = false;
        old.generate(c).for_each_block(|b| any |= b == g::CINNABAR || b == g::SULFUR);
        assert!(!any, "generation 4 stays without sulfur");
    }
}
