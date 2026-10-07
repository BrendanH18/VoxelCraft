//! Overworld ore veins, following Minecraft 1.21's `ore` feature and the
//! vanilla placed-feature JSON (InventivetalentDev/minecraft-assets 1.21.4).
//!
//! Java's overworld bedrock is y = -64 and the column runs to y = 319. This
//! world is still y = 0..255 with bedrock at 0, so every vanilla absolute Y
//! is shifted by [`JAVA_Y_SHIFT`] and samples that fall outside 1..255 are
//! dropped. The top of ranges that extend past y = 191 in Java (coal's
//! ceiling, iron's mountain band, emerald's peak) is clipped. Decorator
//! seeds are a per-cell hash, not Java's xoroshiro `RandomState`, so a
//! vanilla seed will not reproduce the same coordinates; blob shape, count,
//! discard-on-air and the triangular height providers match `OreFeature`
//! and `TrapezoidHeight`.
//!
//! Veins are painted per Java 16×16 cell, including cells that only overlap
//! this chunk, and only stone (later deepslate) is replaced. Buried variants
//! skip blocks with an air neighbour inside the chunk.

use glam::IVec3;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, index};
use super::noise::hash3;
use super::structure::Rng;
use super::terrain::Biome;

/// Java y = -64 lands on this world's bedrock.
pub const JAVA_Y_SHIFT: i32 = 64;
const JAVA_CELL: i32 = 16;
/// How far a size-64 blob can reach; smaller veins are well inside this.
const VEIN_REACH: i32 = 16;

#[derive(Clone, Copy)]
enum Tries {
    /// `count` placement.
    Count(u32),
    /// Inclusive uniform count (gold's lower band is 0 or 1).
    CountRange(u32, u32),
    /// `rarity_filter`: one attempt, kept with probability 1/n.
    Rarity(u32),
}

#[derive(Clone, Copy)]
enum Band {
    /// `uniform` height, inclusive, in this world's Y.
    Uniform(i32, i32),
    /// `trapezoid` with plateau 0: Java's triangle distribution.
    Triangle(i32, i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Where {
    Overworld,
    Mountains,
    Badlands,
}

struct Feature {
    salt: u64,
    tries: Tries,
    band: Band,
    size: i32,
    /// `discard_chance_on_air_exposure`. 0 places every hit; 1 buries them.
    discard: f32,
    where_: Where,
    ore: Block,
}

const fn j(y: i32) -> i32 {
    y + JAVA_Y_SHIFT
}

/// Vanilla placed features, Y already shifted by [`JAVA_Y_SHIFT`].
const FEATURES: &[Feature] = &[
    Feature {
        salt: 0xC0A1,
        tries: Tries::Count(30),
        band: Band::Uniform(j(136), j(319)),
        size: 17,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::COAL_ORE,
    },
    Feature {
        salt: 0xC0A2,
        tries: Tries::Count(20),
        band: Band::Triangle(j(0), j(192)),
        size: 17,
        discard: 0.5,
        where_: Where::Overworld,
        ore: Block::COAL_ORE,
    },
    Feature {
        salt: 0x1701,
        tries: Tries::Count(10),
        band: Band::Triangle(j(-24), j(56)),
        size: 9,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::IRON_ORE,
    },
    Feature {
        salt: 0x1702,
        tries: Tries::Count(90),
        band: Band::Triangle(j(80), j(384)),
        size: 9,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::IRON_ORE,
    },
    Feature {
        salt: 0x1703,
        tries: Tries::Count(10),
        band: Band::Uniform(j(-64), j(72)),
        size: 4,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::IRON_ORE,
    },
    Feature {
        salt: 0xC0C1,
        tries: Tries::Count(16),
        band: Band::Triangle(j(-16), j(112)),
        size: 20,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::COPPER_ORE,
    },
    Feature {
        salt: 0xC0C2,
        tries: Tries::Count(16),
        band: Band::Triangle(j(-16), j(112)),
        size: 10,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::COPPER_ORE,
    },
    Feature {
        salt: 0xA01D,
        tries: Tries::Count(4),
        band: Band::Triangle(j(-64), j(32)),
        size: 9,
        discard: 0.5,
        where_: Where::Overworld,
        ore: Block::GOLD_ORE,
    },
    Feature {
        salt: 0xA01E,
        tries: Tries::CountRange(0, 1),
        band: Band::Uniform(j(-64), j(-48)),
        size: 9,
        discard: 0.5,
        where_: Where::Overworld,
        ore: Block::GOLD_ORE,
    },
    Feature {
        salt: 0xA01F,
        tries: Tries::Count(50),
        band: Band::Uniform(j(32), j(256)),
        size: 9,
        discard: 0.0,
        where_: Where::Badlands,
        ore: Block::GOLD_ORE,
    },
    Feature {
        salt: 0x4ED5,
        tries: Tries::Count(4),
        band: Band::Uniform(j(-64), j(15)),
        size: 8,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::REDSTONE_ORE,
    },
    Feature {
        salt: 0x4ED6,
        tries: Tries::Count(8),
        band: Band::Triangle(j(-96), j(-32)),
        size: 8,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::REDSTONE_ORE,
    },
    Feature {
        salt: 0x1A91,
        tries: Tries::Count(2),
        band: Band::Triangle(j(-32), j(32)),
        size: 7,
        discard: 0.0,
        where_: Where::Overworld,
        ore: Block::LAPIS_ORE,
    },
    Feature {
        salt: 0x1A92,
        tries: Tries::Count(4),
        band: Band::Uniform(j(-64), j(64)),
        size: 7,
        discard: 1.0,
        where_: Where::Overworld,
        ore: Block::LAPIS_ORE,
    },
    Feature {
        salt: 0xD1A1,
        tries: Tries::Count(7),
        band: Band::Triangle(j(-144), j(16)),
        size: 4,
        discard: 0.5,
        where_: Where::Overworld,
        ore: Block::DIAMOND_ORE,
    },
    Feature {
        salt: 0xD1A2,
        tries: Tries::Rarity(9),
        band: Band::Triangle(j(-144), j(16)),
        size: 12,
        discard: 0.7,
        where_: Where::Overworld,
        ore: Block::DIAMOND_ORE,
    },
    Feature {
        salt: 0xD1A3,
        tries: Tries::Count(2),
        band: Band::Uniform(j(-64), j(-4)),
        size: 8,
        discard: 0.5,
        where_: Where::Overworld,
        ore: Block::DIAMOND_ORE,
    },
    Feature {
        salt: 0xD1A4,
        tries: Tries::Count(4),
        band: Band::Triangle(j(-144), j(16)),
        size: 8,
        discard: 1.0,
        where_: Where::Overworld,
        ore: Block::DIAMOND_ORE,
    },
    Feature {
        salt: 0xE3E1,
        tries: Tries::Count(100),
        band: Band::Triangle(j(-16), j(480)),
        size: 3,
        discard: 0.0,
        where_: Where::Mountains,
        ore: Block::EMERALD_ORE,
    },
];

fn band_limits(band: Band) -> (i32, i32) {
    match band {
        Band::Uniform(min, max) | Band::Triangle(min, max) => (min, max),
    }
}

/// Java's `TrapezoidHeight` with plateau 0, or `UniformHeight`.
fn sample_band(rng: &mut Rng, band: Band) -> i32 {
    let (min, max) = band_limits(band);
    if min > max {
        return min;
    }
    match band {
        Band::Uniform(_, _) => min + rng.below((max - min + 1) as u32) as i32,
        Band::Triangle(_, _) => {
            let span = max - min;
            let half = span / 2;
            let rest = span - half;
            min + rng.below(rest as u32 + 1) as i32 + rng.below(half as u32 + 1) as i32
        }
    }
}

/// How far a vein can stick out from its origin. The endpoint sits up to
/// `size/8 + size/16` away and the sphere radius adds `size/16`; `size/4 + 2`
/// also covers the two blocks of vertical jitter.
fn vein_reach(size: i32) -> i32 {
    size / 4 + 2
}

fn overlaps_chunk(band: Band, reach: i32, base_y: i32) -> bool {
    let (min, max) = band_limits(band);
    max + reach >= base_y && min - reach < base_y + CHUNK_SIZE_I
}

/// Replace stone in `blocks` with ore veins that can reach this chunk.
pub fn paint(seed: u64, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, biome_at: impl Fn(i32, i32) -> Biome) {
    let min_cx = (base.x - VEIN_REACH).div_euclid(JAVA_CELL);
    let max_cx = (base.x + CHUNK_SIZE_I - 1 + VEIN_REACH).div_euclid(JAVA_CELL);
    let min_cz = (base.z - VEIN_REACH).div_euclid(JAVA_CELL);
    let max_cz = (base.z + CHUNK_SIZE_I - 1 + VEIN_REACH).div_euclid(JAVA_CELL);
    for feature in FEATURES {
        if !overlaps_chunk(feature.band, vein_reach(feature.size), base.y) {
            continue;
        }
        for cz in min_cz..=max_cz {
            for cx in min_cx..=max_cx {
                paint_cell(seed, feature, cx, cz, blocks, base, &biome_at);
            }
        }
    }
}

fn paint_cell(
    seed: u64,
    feature: &Feature,
    cx: i32,
    cz: i32,
    blocks: &mut [Block; CHUNK_VOLUME],
    base: IVec3,
    biome_at: &impl Fn(i32, i32) -> Biome,
) {
    let mut rng = Rng(hash3(cx, cz, feature.salt as i32, seed ^ feature.salt));
    let tries = match feature.tries {
        Tries::Count(n) => n,
        Tries::CountRange(lo, hi) => rng.range(lo, hi),
        Tries::Rarity(n) => {
            if rng.unit() >= 1.0 / f64::from(n) {
                return;
            }
            1
        }
    };
    for _ in 0..tries {
        let x = cx * JAVA_CELL + rng.below(JAVA_CELL as u32) as i32;
        let z = cz * JAVA_CELL + rng.below(JAVA_CELL as u32) as i32;
        let y = sample_band(&mut rng, feature.band);
        if feature.where_ != Where::Overworld {
            let biome = biome_at(x, z);
            let ok = match feature.where_ {
                Where::Mountains => biome == Biome::Mountains,
                Where::Badlands => biome == Biome::Badlands,
                Where::Overworld => true,
            };
            if !ok {
                continue;
            }
        }
        let reach = vein_reach(feature.size);
        let misses = |origin: i32, chunk: i32| origin + reach < chunk || origin - reach >= chunk + CHUNK_SIZE_I;
        if misses(x, base.x) || misses(y, base.y) || misses(z, base.z) {
            continue;
        }
        place_vein(&mut rng, IVec3::new(x, y, z), feature, blocks, base);
    }
}

/// `OreFeature.doPlace`: a chain of spheres along a random horizontal axis.
fn place_vein(rng: &mut Rng, origin: IVec3, feature: &Feature, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
    let size = feature.size;
    let angle = rng.unit() as f32 * std::f32::consts::PI;
    let spread = size as f32 / 8.0;
    let (sin, cos) = (f64::from(angle.sin()), f64::from(angle.cos()));
    let x0 = f64::from(origin.x) + sin * f64::from(spread);
    let x1 = f64::from(origin.x) - sin * f64::from(spread);
    let z0 = f64::from(origin.z) + cos * f64::from(spread);
    let z1 = f64::from(origin.z) - cos * f64::from(spread);
    let y0 = f64::from(origin.y + rng.below(3) as i32 - 2);
    let y1 = f64::from(origin.y + rng.below(3) as i32 - 2);

    // Centre xyz and radius. Size never exceeds 64 (granite); ores are smaller.
    let mut sphere = [(0.0f64, 0.0f64, 0.0f64, 0.0f64); 64];
    let n = size.min(64) as usize;
    for (i, s) in sphere.iter_mut().enumerate().take(n) {
        let t = i as f32 / size as f32;
        let radius_noise = rng.unit() * f64::from(size) / 16.0;
        let blob = (f64::from((std::f32::consts::PI * t).sin()) + 1.0) * radius_noise + 1.0;
        let radius = blob / 2.0;
        let lerp = |a: f64, b: f64| a + (b - a) * f64::from(t);
        *s = (lerp(x0, x1), lerp(y0, y1), lerp(z0, z1), radius);
    }
    // Drop a sphere completely inside a larger neighbour, as Java does.
    for i in 0..n {
        if sphere[i].3 <= 0.0 {
            continue;
        }
        for j in (i + 1)..n {
            if sphere[j].3 <= 0.0 {
                continue;
            }
            let dx = sphere[i].0 - sphere[j].0;
            let dy = sphere[i].1 - sphere[j].1;
            let dz = sphere[i].2 - sphere[j].2;
            let dr = sphere[i].3 - sphere[j].3;
            if dr * dr > dx * dx + dy * dy + dz * dz {
                if dr > 0.0 { sphere[j].3 = -1.0 } else { sphere[i].3 = -1.0 }
            }
        }
    }

    for s in sphere.iter().take(n) {
        let (cx, cy, cz, radius) = *s;
        if radius < 0.0 {
            continue;
        }
        let y_lo = (cy - radius).floor() as i32;
        let y_hi = (cy + radius).floor() as i32;
        let x_lo = (cx - radius).floor() as i32;
        let x_hi = (cx + radius).floor() as i32;
        let z_lo = (cz - radius).floor() as i32;
        let z_hi = (cz + radius).floor() as i32;
        for y in y_lo..=y_hi {
            let dy = (f64::from(y) + 0.5 - cy) / radius;
            if dy * dy >= 1.0 || !(1..256).contains(&y) {
                continue;
            }
            for x in x_lo..=x_hi {
                let dx = (f64::from(x) + 0.5 - cx) / radius;
                if dx * dx + dy * dy >= 1.0 {
                    continue;
                }
                for z in z_lo..=z_hi {
                    let dz = (f64::from(z) + 0.5 - cz) / radius;
                    if dx * dx + dy * dy + dz * dz >= 1.0 {
                        continue;
                    }
                    try_place(feature, blocks, base, IVec3::new(x, y, z), rng);
                }
            }
        }
    }
}

fn try_place(feature: &Feature, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, p: IVec3, rng: &mut Rng) {
    let local = p - base;
    let inside = local.cmpge(IVec3::ZERO).all() && local.cmplt(IVec3::splat(CHUNK_SIZE_I)).all();
    if !inside {
        // Partial discard still draws, so every chunk that shares the vein
        // consumes the same random stream. The block itself belongs to
        // whichever chunk contains it.
        if feature.discard > 0.0 && feature.discard < 1.0 {
            let _ = rng.unit();
        }
        return;
    }
    let i = index(local.x as usize, local.y as usize, local.z as usize);
    if blocks[i] != Block::STONE {
        if feature.discard > 0.0 && feature.discard < 1.0 {
            let _ = rng.unit();
        }
        return;
    }
    if feature.discard > 0.0
        && feature.discard < 1.0
        && rng.unit() < f64::from(feature.discard)
        && exposed(blocks, base, p)
    {
        return;
    }
    if feature.discard >= 1.0 && exposed(blocks, base, p) {
        return;
    }
    blocks[i] = feature.ore;
}

fn exposed(blocks: &[Block; CHUNK_VOLUME], base: IVec3, p: IVec3) -> bool {
    const DIRS: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
    DIRS.into_iter().any(|d| {
        let q = p + d;
        let local = q - base;
        if local.cmpge(IVec3::ZERO).all() && local.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
            blocks[index(local.x as usize, local.y as usize, local.z as usize)] == Block::AIR
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::CHUNK_SIZE;
    use crate::world::terrain::Generator;

    #[test]
    fn triangle_peaks_between_its_ends() {
        let mut rng = Rng(0x1234_5678);
        let mut low = 0;
        let mut high = 0;
        for _ in 0..4000 {
            let y = sample_band(&mut rng, Band::Triangle(40, 120));
            assert!((40..=120).contains(&y));
            if (40..60).contains(&y) {
                low += 1;
            }
            if (70..90).contains(&y) {
                high += 1;
            }
        }
        assert!(high > low * 2, "the middle of a triangle is where the ore is ({high} vs {low})");
    }

    #[test]
    fn veins_are_deterministic_and_sit_in_vanilla_bands() {
        let worldgen = Generator::new(7);
        let mut copper = 0;
        let mut iron = 0;
        let mut diamond = 0;
        let mut diamond_y = 0i64;
        let mut copper_y = 0i64;
        for cx in 0..3 {
            for cz in 0..3 {
                for cy in 0..4 {
                    let pos = IVec3::new(cx, cy, cz);
                    let blocks = worldgen.generate(pos);
                    let again = worldgen.generate(pos);
                    let base_y = cy * CHUNK_SIZE_I;
                    for y in 0..CHUNK_SIZE {
                        for z in 0..CHUNK_SIZE {
                            for x in 0..CHUNK_SIZE {
                                let b = blocks.get(x, y, z);
                                assert_eq!(b, again.get(x, y, z), "ore painting is a pure function of the seed");
                                let wy = base_y + y as i32;
                                match b {
                                    Block::COPPER_ORE => {
                                        copper += 1;
                                        copper_y += i64::from(wy);
                                    }
                                    Block::IRON_ORE => iron += 1,
                                    Block::DIAMOND_ORE => {
                                        diamond += 1;
                                        diamond_y += i64::from(wy);
                                        assert!(wy < 100, "shifted diamond stays deep");
                                    }
                                    Block::REDSTONE_ORE => assert!(wy < 90, "redstone stays in the lower band"),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(copper > 20 && iron > 20, "copper {copper}, iron {iron}");
        assert!(diamond > 0, "diamond generates near bedrock");
        assert!(diamond_y / i64::from(diamond) < copper_y / i64::from(copper));
    }

    /// The cull distance has to cover every block `place_vein` can write,
    /// including the ±2 vertical jitter and the sphere radius.
    #[test]
    fn vein_reach_covers_every_placed_block() {
        for size in [3, 4, 7, 8, 9, 10, 12, 17, 20, 64] {
            let mut furthest = 0;
            for salt in 0..64u64 {
                let mut rng = Rng(hash3(size, 0, salt as i32, 0x0E));
                let origin = IVec3::new(100, 80, 100);
                let angle = rng.unit() as f32 * std::f32::consts::PI;
                let spread = size as f32 / 8.0;
                let (sin, cos) = (f64::from(angle.sin()), f64::from(angle.cos()));
                let x0 = f64::from(origin.x) + sin * f64::from(spread);
                let x1 = f64::from(origin.x) - sin * f64::from(spread);
                let z0 = f64::from(origin.z) + cos * f64::from(spread);
                let z1 = f64::from(origin.z) - cos * f64::from(spread);
                let y0 = f64::from(origin.y + rng.below(3) as i32 - 2);
                let y1 = f64::from(origin.y + rng.below(3) as i32 - 2);
                for i in 0..size {
                    let t = i as f32 / size as f32;
                    let radius_noise = rng.unit() * f64::from(size) / 16.0;
                    let radius = ((f64::from((std::f32::consts::PI * t).sin()) + 1.0) * radius_noise + 1.0) / 2.0;
                    let lerp = |a: f64, b: f64| a + (b - a) * f64::from(t);
                    let (cx, cy, cz) = (lerp(x0, x1), lerp(y0, y1), lerp(z0, z1));
                    let y_lo = (cy - radius).floor() as i32;
                    let y_hi = (cy + radius).floor() as i32;
                    let x_lo = (cx - radius).floor() as i32;
                    let x_hi = (cx + radius).floor() as i32;
                    let z_lo = (cz - radius).floor() as i32;
                    let z_hi = (cz + radius).floor() as i32;
                    for y in y_lo..=y_hi {
                        let dy = (f64::from(y) + 0.5 - cy) / radius;
                        if dy * dy >= 1.0 {
                            continue;
                        }
                        for x in x_lo..=x_hi {
                            let dx = (f64::from(x) + 0.5 - cx) / radius;
                            if dx * dx + dy * dy >= 1.0 {
                                continue;
                            }
                            for z in z_lo..=z_hi {
                                let dz = (f64::from(z) + 0.5 - cz) / radius;
                                if dx * dx + dy * dy + dz * dz >= 1.0 {
                                    continue;
                                }
                                let d = (x - origin.x).abs().max((y - origin.y).abs()).max((z - origin.z).abs());
                                furthest = furthest.max(d);
                            }
                        }
                    }
                }
            }
            assert!(furthest <= vein_reach(size), "size {size} reached {furthest}, cull is {}", vein_reach(size));
        }
    }
}
