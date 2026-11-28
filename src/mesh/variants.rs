//! v1.0 final track F3: Java 26.3's random block model variants.
//!
//! A blockstate entry that lists several models (`blockstates/stone.json`'s
//! `stone`, `stone_mirrored`, each also turned `y` 180) is a
//! `WeightedVariants`: `ModelBlockRenderer.tesselateBlock` seeds a
//! `SingleThreadedRandomSource` (Java's 48-bit LCG) with
//! `BlockState.getSeed(pos)`, which is `Mth.getSeed(pos)` for every block
//! here, and `WeightedList.getRandomOrThrow` takes `nextInt(totalWeight)`.
//! Every full cube with variants weighs them equally, four or sixteen, so
//! the pick is the top two or four bits of the LCG's first step.
//!
//! The textures don't change, only their UVs: each variant turns or mirrors
//! some faces, as one of the square's eight symmetries
//! ([`super::java_uv`]'s order) against the planar default. The jar's
//! blockstates fall into three [groups](Group) (`tools/extract_java_block_variants.py`
//! writes the fixture `block_variants.json` the tests check them against).
//!
//! The mesher stores the group in a normal quad (word 1 bits 25-26) and the
//! fragment shader picks the variant for the cell it draws (`chunk.wgsl`'s
//! `java_variant`), so greedy merging and the 12-byte quad are unchanged.

use crate::world::block::Block;

/// A variant group: 0 none, else an index of [`GROUPS`] plus one.
pub type Group = u32;
/// `grass_block` (not snowy), `dirt`, `sand`, `red_sand`, `mycelium`,
/// `podzol`, `rooted_dirt` and the 16 concrete powders: the model turned
/// `y` 0, 90, 180 and 270.
pub const TURNED: Group = 1;
/// `stone`, `infested_stone`, `bedrock`, `sculk`, `deepslate` and
/// `infested_deepslate` (axis y, the game's only deepslate): the model,
/// its `_mirrored` form (`cube_mirrored_all`, every face's UVs reversed in
/// u), and each turned `y` 180.
pub const MIRRORED: Group = 2;
/// `netherrack`: the model turned `x` 0 to 270 for each `y` 0 to 270.
pub const NETHERRACK: Group = 3;

/// Each group's variants, in the blockstate's order: per face (+X, -X, +Y,
/// -Y, +Z, -Z) the symmetry of its UVs. Groups of 4 repeat to 16.
pub const GROUPS: [[[u8; 6]; 16]; 3] = {
    const T: [[u8; 6]; 4] = [[0, 0, 0, 0, 0, 0], [0, 0, 6, 5, 0, 0], [0, 0, 3, 3, 0, 0], [0, 0, 5, 6, 0, 0]];
    const M: [[u8; 6]; 4] = [[0, 0, 0, 0, 0, 0], [1, 1, 1, 1, 1, 1], [0, 0, 3, 3, 0, 0], [1, 1, 2, 2, 1, 1]];
    const N: [[u8; 6]; 16] = [
        [0, 0, 0, 0, 0, 0],
        [6, 5, 0, 3, 0, 3],
        [3, 3, 0, 0, 3, 3],
        [5, 6, 3, 0, 0, 3],
        [0, 0, 6, 5, 0, 0],
        [3, 0, 6, 6, 6, 5],
        [3, 3, 6, 5, 3, 3],
        [3, 0, 5, 5, 5, 6],
        [0, 0, 3, 3, 0, 0],
        [5, 6, 3, 0, 3, 0],
        [3, 3, 3, 3, 3, 3],
        [6, 5, 0, 3, 3, 0],
        [0, 0, 5, 6, 0, 0],
        [0, 3, 5, 5, 5, 6],
        [3, 3, 5, 6, 3, 3],
        [0, 3, 6, 6, 6, 5],
    ];
    const fn repeat(g: [[u8; 6]; 4]) -> [[u8; 6]; 16] {
        let mut out = [[0; 6]; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = g[i % 4];
            i += 1;
        }
        out
    }
    [repeat(T), repeat(M), N]
};

/// How many variants each group has.
pub const fn count(group: Group) -> u32 {
    if group == NETHERRACK { 16 } else { 4 }
}

/// The variant group of the full cube `b`.
pub fn group(b: Block) -> Group {
    use crate::world::{cave_blocks, infested, overworld_blocks};
    match b {
        Block::GRASS | Block::DIRT | Block::SAND | Block::RED_SAND => TURNED,
        overworld_blocks::MYCELIUM | overworld_blocks::PODZOL | overworld_blocks::ROOTED_DIRT => TURNED,
        Block::STONE | Block::BEDROCK | Block::DEEPSLATE => MIRRORED,
        infested::INFESTED_STONE | infested::INFESTED_DEEPSLATE | cave_blocks::SCULK => MIRRORED,
        Block::NETHERRACK => NETHERRACK,
        _ if b.concrete_powder_color().is_some() => TURNED,
        _ => 0,
    }
}

/// `Mth.getSeed(x, y, z)`.
pub fn java_seed(x: i32, y: i32, z: i32) -> i64 {
    let s = i64::from(x.wrapping_mul(3_129_871)) ^ i64::from(z).wrapping_mul(116_129_781) ^ i64::from(y);
    s.wrapping_mul(s).wrapping_mul(42_317_861).wrapping_add(s.wrapping_mul(11)) >> 16
}

const MULTIPLIER: u64 = 0x5_DEEC_E66D;
const MASK: u64 = (1 << 48) - 1;

/// A `SingleThreadedRandomSource` seeded with `seed`.
fn seeded(seed: i64) -> u64 {
    (seed as u64 ^ MULTIPLIER) & MASK
}

/// One `next(bits)` step of the LCG `state`.
fn next(state: &mut u64, bits: u32) -> i32 {
    *state = (state.wrapping_mul(MULTIPLIER).wrapping_add(11)) & MASK;
    (*state >> (48 - bits)) as i64 as i32
}

/// `nextInt(bound)` for a power of two.
fn next_int_pow2(state: &mut u64, bound: u32) -> u32 {
    debug_assert!(bound.is_power_of_two());
    ((i64::from(bound) * i64::from(next(state, 31))) >> 31) as u32
}

/// Java's pick out of `bound` (a power of two) equally weighted variants
/// at `(x, y, z)`.
pub fn pick(x: i32, y: i32, z: i32, bound: u32) -> u32 {
    next_int_pow2(&mut seeded(java_seed(x, y, z)), bound)
}

/// v1.0 final track F3: the picks of a multipart model whose matching parts
/// list `bounds` equally weighted models each (fire's): `MultiPartModel`
/// draws one `nextLong` and reseeds the random with it before every part.
pub fn multipart_picks<const N: usize>(x: i32, y: i32, z: i32, bounds: [u32; N]) -> [u32; N] {
    let mut state = seeded(java_seed(x, y, z));
    let upper = i64::from(next(&mut state, 32));
    let lower = i64::from(next(&mut state, 32));
    let part = (upper << 32).wrapping_add(lower);
    bounds.map(|b| next_int_pow2(&mut seeded(part), b))
}

/// The UV symmetry of `face` of a cube of `group` at world `(x, y, z)`.
pub fn face_symmetry(group: Group, x: i32, y: i32, z: i32, face: usize) -> u32 {
    if group == 0 {
        return 0;
    }
    let i = pick(x, y, z, count(group)) as usize;
    u32::from(GROUPS[group as usize - 1][i][face])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> serde_json::Value {
        let path = format!("{}/tests/fixtures/java/26.3/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn picks_match_javas_random() {
        let rows = fixture("block_variant_picks.json");
        for row in rows.as_array().unwrap() {
            let v: Vec<i64> = row.as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect();
            let (x, y, z) = (v[0] as i32, v[1] as i32, v[2] as i32);
            assert_eq!(pick(x, y, z, 4), v[3] as u32, "{x} {y} {z} of 4");
            assert_eq!(pick(x, y, z, 16), v[4] as u32, "{x} {y} {z} of 16");
            assert_eq!(multipart_picks(x, y, z, [2, 4]), [v[5] as u32, v[6] as u32], "{x} {y} {z} fire");
        }
    }

    /// `chunk.wgsl`'s 64-bit helpers, word for word in wrapping `u32`s.
    fn mul_wide(a: u32, b: u32) -> [u32; 2] {
        let (a0, a1, b0, b1) = (a & 0xFFFF, a >> 16, b & 0xFFFF, b >> 16);
        let (p00, p01, p10) = (a0 * b0, a0 * b1, a1 * b0);
        let mid = (p00 >> 16) + (p01 & 0xFFFF) + (p10 & 0xFFFF);
        [
            (p00 & 0xFFFF) | (mid << 16),
            (a1 * b1).wrapping_add(p01 >> 16).wrapping_add(p10 >> 16).wrapping_add(mid >> 16),
        ]
    }
    fn mul64(a: [u32; 2], b: [u32; 2]) -> [u32; 2] {
        let p = mul_wide(a[0], b[0]);
        [p[0], p[1].wrapping_add(a[0].wrapping_mul(b[1])).wrapping_add(a[1].wrapping_mul(b[0]))]
    }
    fn add64(a: [u32; 2], b: [u32; 2]) -> [u32; 2] {
        let lo = a[0].wrapping_add(b[0]);
        [lo, a[1].wrapping_add(b[1]).wrapping_add(u32::from(lo < a[0]))]
    }
    fn widen(i: i32) -> [u32; 2] {
        [i as u32, if i < 0 { u32::MAX } else { 0 }]
    }
    /// `java_seeded`.
    fn shader_seeded(x: i32, y: i32, z: i32) -> [u32; 2] {
        let xor = |a: [u32; 2], b: [u32; 2]| [a[0] ^ b[0], a[1] ^ b[1]];
        let s = xor(xor(widen((x as u32).wrapping_mul(3_129_871) as i32), mul64(widen(z), [116_129_781, 0])), widen(y));
        let t = add64(mul64(mul64(s, s), [42_317_861, 0]), mul64(s, [11, 0]));
        [((t[0] >> 16) | (t[1] << 16)) ^ 0xDEEC_E66D, ((t[1] >> 16) & 0xFFFF) ^ 5]
    }
    /// `java_step`.
    fn shader_step(state: [u32; 2]) -> [u32; 2] {
        let n = add64(mul64(state, [0xDEEC_E66D, 5]), [11, 0]);
        [n[0], n[1] & 0xFFFF]
    }
    /// `java_variant`.
    fn shader_variant(x: i32, y: i32, z: i32, bits: u32) -> u32 {
        shader_step(shader_seeded(x, y, z))[1] >> (16 - bits)
    }
    /// `java_fire_floor`.
    fn shader_fire_floor(x: i32, y: i32, z: i32) -> u32 {
        let s1 = shader_step(shader_seeded(x, y, z));
        let s2 = shader_step(s1);
        let upper = (s1[0] >> 16) | (s1[1] << 16);
        let lower = (s2[0] >> 16) | (s2[1] << 16);
        let part = add64([0, upper], widen(lower as i32));
        shader_step([part[0] ^ 0xDEEC_E66D, (part[1] ^ 5) & 0xFFFF])[1] >> 15
    }

    #[test]
    fn the_shaders_pick_and_tables_are_javas() {
        let rows = fixture("block_variant_picks.json");
        for row in rows.as_array().unwrap() {
            let v: Vec<i32> = row.as_array().unwrap().iter().map(|n| n.as_i64().unwrap() as i32).collect();
            assert_eq!(shader_variant(v[0], v[1], v[2], 2), v[3] as u32, "{v:?}");
            assert_eq!(shader_variant(v[0], v[1], v[2], 4), v[4] as u32, "{v:?}");
            assert_eq!(shader_fire_floor(v[0], v[1], v[2]), v[5] as u32, "{v:?}");
        }
        let shader = include_str!("../render/shaders/chunk.wgsl");
        let table = |name: &str| -> Vec<u32> {
            let start = shader.find(&format!("var {name} = array<u32, ")).unwrap();
            let body = &shader[start..];
            let body = &body[body.find(">(").unwrap() + 2..body.find(");").unwrap()];
            body.split(',')
                .map(|h| u32::from_str_radix(h.trim().trim_start_matches("0x").trim_end_matches('u'), 16).unwrap())
                .collect()
        };
        for (g, name) in [(TURNED, "turned"), (MIRRORED, "mirrored"), (NETHERRACK, "netherrack")] {
            let packed = table(name);
            assert_eq!(packed.len() as u32, count(g));
            for (i, p) in packed.iter().enumerate() {
                let faces: Vec<u8> = (0..6).map(|f| (p >> (3 * f) & 7) as u8).collect();
                assert_eq!(faces, GROUPS[g as usize - 1][i], "{name} {i}");
            }
        }
    }

    #[test]
    fn groups_match_javas_blockstates() {
        use crate::world::{cave_blocks, infested, overworld_blocks};
        let states = fixture("block_variants.json");
        let states = states.as_object().unwrap();
        let mut blocks = vec![
            ("bedrock[]", Block::BEDROCK),
            ("deepslate[axis=y]", Block::DEEPSLATE),
            ("dirt[]", Block::DIRT),
            ("grass_block[snowy=false]", Block::GRASS),
            ("infested_deepslate[axis=y]", infested::INFESTED_DEEPSLATE),
            ("infested_stone[]", infested::INFESTED_STONE),
            ("mycelium[snowy=false]", overworld_blocks::MYCELIUM),
            ("netherrack[]", Block::NETHERRACK),
            ("podzol[snowy=false]", overworld_blocks::PODZOL),
            ("red_sand[]", Block::RED_SAND),
            ("rooted_dirt[]", overworld_blocks::ROOTED_DIRT),
            ("sand[]", Block::SAND),
            ("sculk[]", cave_blocks::SCULK),
            ("stone[]", Block::STONE),
        ];
        let powders: Vec<(String, Block)> = (0..16u16)
            .map(|i| {
                let b = Block(667 + i);
                let name = b.concrete_powder_color().unwrap().adjective().replace(' ', "_");
                (format!("{name}_concrete_powder[]"), b)
            })
            .collect();
        blocks.extend(powders.iter().map(|(n, b)| (n.as_str(), *b)));
        let mut seen = 0;
        for (key, b) in blocks {
            let rows = states[key].as_array().unwrap_or_else(|| panic!("{key}"));
            let g = group(b);
            assert_ne!(g, 0, "{key}");
            assert_eq!(rows.len() as u32, count(g), "{key}");
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row["weight"], 1, "{key}");
                let faces: Vec<u8> =
                    row["faces"].as_array().unwrap().iter().map(|f| f.as_u64().unwrap() as u8).collect();
                assert_eq!(faces, GROUPS[g as usize - 1][i], "{key} variant {i}");
            }
            seen += 1;
        }
        // Every axis=y or unconditioned entry of the fixture is drawn; the
        // game has no deepslate on its side.
        let drawn = states.keys().filter(|k| !k.contains("axis=x") && !k.contains("axis=z")).count();
        assert_eq!(seen, drawn);
        assert_eq!(group(Block::SNOWY_GRASS), 0);
        assert_eq!(group(Block::GRAVEL), 0, "Java 26.3's gravel has one model");
    }
}
