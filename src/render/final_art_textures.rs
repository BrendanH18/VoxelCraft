//! v1.0 Track 10: the final art round, in v0.18's material style (see
//! `docs/art-and-ui.md`). Original pixel art, drawn with integer maths like
//! the rest of the material generator: the wooden doors, trapdoors and
//! chests, the dirt path, hay bale, bell, anvils and TNT, and the plants
//! (flowers, saplings, grasses and ferns, crops, mushrooms and the other
//! cross-model plants). Every tile replaces its existing layer; no layer is
//! added. Manufactured faces follow the guide's rules (straight edges, light
//! from the top left, joints one texel wide); plants are drawn as small
//! sprites with a lit upper-left and a shaded lower-right on every leaf,
//! petal and stalk, so they read as solid at a distance.

use super::{N, Tile, cracks, hu, paint, planks_grid, ramp, smooth_grid};
use crate::world::block::tex;

/// v1.0 Track 10b: the follow-up round, drawn with this round's primitives.
#[path = "final_art_b_textures.rs"]
mod round_b;

/// v1.0 gap Track 7: the amethyst buds, the bamboo's leaves for its crossed
/// planes and the potted bamboo.
#[path = "final_art_c_textures.rs"]
mod round_c;

type Px = [u8; 3];
/// Four shades of one colour: dark, mid, light, highlight.
type Shades = [Px; 4];

fn shades(c: Px) -> Shades {
    ramp(c, [640, 820, 1000, 1170])
}

/// A sprite being drawn: transparent until painted.
#[derive(Clone)]
struct Img([Option<Px>; N * N]);

impl Img {
    fn new() -> Self {
        Self([None; N * N])
    }

    fn set(&mut self, x: i32, y: i32, c: Px) {
        if (0..N as i32).contains(&x) && (0..N as i32).contains(&y) {
            self.0[y as usize * N + x as usize] = Some(c);
        }
    }

    fn get(&self, x: i32, y: i32) -> Option<Px> {
        if (0..N as i32).contains(&x) && (0..N as i32).contains(&y) {
            self.0[y as usize * N + x as usize]
        } else {
            None
        }
    }

    /// A straight one-texel line.
    fn line(&mut self, (x0, y0): (i32, i32), (x1, y1): (i32, i32), c: Px) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// A stamp of characters at `(x, y)` (its top-left), each character
    /// looked up in `key`; spaces and unknown characters are skipped.
    fn stamp(&mut self, x: i32, y: i32, rows: &[&str], key: &[(char, Px)]) {
        for (j, row) in rows.iter().enumerate() {
            for (i, ch) in row.chars().enumerate() {
                if let Some(&(_, c)) = key.iter().find(|(k, _)| *k == ch) {
                    self.set(x + i as i32, y + j as i32, c);
                }
            }
        }
    }

    /// Mirrored left to right.
    fn flipped(&self) -> Self {
        let mut out = Self::new();
        for y in 0..N {
            for x in 0..N {
                out.0[y * N + x] = self.0[y * N + N - 1 - x];
            }
        }
        out
    }

    fn tile(&self) -> Tile {
        std::array::from_fn(|i| match self.0[i] {
            Some([r, g, b]) => [r, g, b, 255],
            None => [0; 4],
        })
    }
}

// ---- Plant primitives ---------------------------------------------------

/// A stalk through `pts` (bottom first), mid shade, with the texel on its
/// right shaded where the stalk is two wide.
fn stalk(img: &mut Img, pts: &[(i32, i32)], s: &Shades) {
    for w in pts.windows(2) {
        img.line(w[0], w[1], s[1]);
    }
}

/// Thin twigs drawn in one shade: lit where open to the upper left,
/// shaded where open to the lower right.
fn edge_light(img: &mut Img, s: &Shades) {
    let before = img.clone();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            if before.get(x, y) != Some(s[1]) {
                continue;
            }
            let open = |dx: i32, dy: i32| before.get(x + dx, y + dy).is_none();
            if open(0, -1) && open(-1, 0) {
                img.set(x, y, s[2]);
            } else if open(0, 1) && open(1, 0) {
                img.set(x, y, s[0]);
            }
        }
    }
}

/// A two-texel stalk: lit on its left, shaded on its right, with a
/// brighter node every five texels.
fn thick_stalk(img: &mut Img, x: i32, y0: i32, y1: i32, s: &Shades) {
    for y in y0.min(y1)..=y0.max(y1) {
        let node = y % 5 == 2 || y == y0.min(y1);
        img.set(x, y, if node { s[3] } else { s[2] });
        img.set(x + 1, y, if node { s[1] } else { s[0] });
    }
}

/// A leaf from its base at `(x, y)` reaching `len` texels to `dir` (-1 or 1)
/// and rising `lift`: lit along its upper edge, mid below, a shaded
/// underside where it is three texels deep, and a bright tip.
fn leaf(img: &mut Img, (x, y): (i32, i32), dir: i32, len: i32, lift: i32, s: &Shades) {
    for t in 0..len {
        let lx = x + dir * (t + 1);
        let ly = y - (t * lift + len / 2) / len.max(1);
        let middle = t > 0 && t < len - 1;
        img.set(lx, ly, if t == len - 1 { s[3] } else { s[2] });
        if middle {
            img.set(lx, ly + 1, s[1]);
        }
        if middle && len >= 5 && t > 1 && t < len - 2 {
            img.set(lx, ly + 2, s[0]);
        }
    }
}

/// A rounded mass (a flower head, a crown of leaves, a cap): lit at its
/// upper left, shaded at its lower right with a dark rim there. `fill`
/// below 256 leaves gaps (leafy crowns); `seed` picks them.
fn blob(img: &mut Img, (cx, cy): (f32, f32), (rx, ry): (f32, f32), s: &Shades, fill: u32, seed: u32) {
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
            let d = dx * dx + dy * dy;
            if d > 1.0 {
                continue;
            }
            let h = hu(seed, x as usize, y as usize, 90);
            if h % 256 >= fill && d > 0.15 {
                continue;
            }
            let side = dx + dy;
            let mut k: usize = if d > 0.62 && side > 0.35 {
                0
            } else if side < -0.75 {
                3
            } else if side < -0.1 {
                2
            } else {
                1
            };
            // Dither one step now and then, so a mass is never flat.
            if (h >> 8).is_multiple_of(7) && k > 0 && k < 3 {
                k = if (h >> 12).is_multiple_of(2) { k + 1 } else { k - 1 };
            }
            img.set(x, y, s[k]);
        }
    }
}

/// A leafy crown: a blob with gaps, the gaps' lower neighbours shaded so
/// each gap reads as a hole between leaves.
fn crown(img: &mut Img, c: (f32, f32), r: (f32, f32), s: &Shades, seed: u32) {
    blob(img, c, r, s, 214, seed);
    for y in (0..N as i32).rev() {
        for x in 0..N as i32 {
            if img.get(x, y).is_none() && img.get(x, y - 1).is_some() && img.get(x, y + 1).is_some() {
                img.set(x, y + 1, s[0]);
            }
        }
    }
}

/// Grass blades rising from the bottom: `count` blades at hashed columns,
/// heights and leans; each one texel wide, lit on blades leaning left.
fn blades(img: &mut Img, seed: u32, count: usize, (lo, hi): (i32, i32), s: &Shades, bottom: i32) {
    for k in 0..count {
        let h = hu(seed, k, 0, 91);
        let x0 = (k as i32 * 16 / count as i32 + (h % 3) as i32) % 16;
        let height = lo + ((h >> 4) as i32 % (hi - lo + 1));
        let lean = (h >> 10) as i32 % 3 - 1;
        let shade = if lean < 0 {
            2
        } else if lean > 0 {
            1
        } else {
            2 - ((h >> 14) % 2) as usize
        };
        for t in 0..height {
            let x = x0 + lean * t * t / (height * 2).max(1);
            let y = bottom - t;
            let c = if t == height - 1 {
                s[3]
            } else if t < 2 {
                s[0]
            } else {
                s[shade]
            };
            img.set(x, y, c);
            // Blades are two wide at the root.
            if t < height / 3 {
                img.set(x + 1, y, s[0]);
            }
        }
    }
}

/// Leaflets along a frond: a stalk from `base` to `tip` with a leaflet
/// every other texel on both sides, shorter near the tip.
fn frond(img: &mut Img, base: (i32, i32), tip: (i32, i32), s: &Shades) {
    let steps = (tip.1 - base.1).abs().max((tip.0 - base.0).abs());
    for t in 0..=steps {
        let x = base.0 + (tip.0 - base.0) * t / steps.max(1);
        let y = base.1 + (tip.1 - base.1) * t / steps.max(1);
        img.set(x, y, s[1]);
        if t % 2 == 1 && t < steps {
            let reach = 1 + (steps - t) / 4;
            for r in 1..=reach.min(3) {
                img.set(x - r, y - (r > 1) as i32, s[2]);
                img.set(x + r, y - (r > 1) as i32, s[if r == 1 { 1 } else { 0 }]);
            }
        }
    }
    img.set(tip.0, tip.1, s[3]);
}

// Greens shared by the plants.
const LEAF: Px = [74, 128, 52];
const DARK_LEAF: Px = [54, 104, 44];
/// Tall grass and ferns use the grass top's mid shade, as before, so
/// they sit on grass blocks and take the biome tints the same way.
const GRASS: Px = [106, 145, 66];
const STEM: Px = [78, 132, 50];
const BARK: Px = [112, 82, 50];

// ---- Flowers ------------------------------------------------------------

/// A flower's stem from the ground up to `top`, with a leaf each side.
fn flower_stem(img: &mut Img, x: i32, top: i32, leaves: &[(i32, i32, i32)]) {
    let s = shades(STEM);
    stalk(img, &[(x, 15), (x, top)], &s);
    for &(y, dir, len) in leaves {
        leaf(img, (x, y), dir, len, 2, &s);
    }
}

/// The common head of a small flower, stamped centred on `(x, y)`.
fn head(img: &mut Img, x: i32, y: i32, rows: &[&str], petal: Px, centre: Px) {
    let p = shades(petal);
    let c = ramp(centre, [700, 1000, 1200]);
    let key = [('d', p[0]), ('m', p[1]), ('l', p[2]), ('h', p[3]), ('c', c[1]), ('C', c[0]), ('o', c[2])];
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0) as i32;
    img.stamp(x - w / 2, y - rows.len() as i32 / 2, rows, &key);
}

const ROUND_HEAD: [&str; 5] = [" hl ", "hllm", "lmmd", "mmdd", " dd "];
const POPPY_HEAD: [&str; 6] = [" hl m ", "hllmmd", "lmCCmd", "lmCCdd", " mmdd ", "  d   "];
const DAISY_HEAD: [&str; 7] = ["  h  ", " hlh ", "hlolm", "lcccm", "hlcmd", " mdm ", "  d  "];
const STAR_HEAD: [&str; 3] = [" h ", "lom", " d "];
const TULIP_HEAD: [&str; 5] = ["h l m", "hlmmd", "llmmd", "lmmdd", " mdd "];
const CORNFLOWER_HEAD: [&str; 5] = ["h l m d", " hlmmd ", "lhlomdd", " lmmdd ", "  mdd  "];
const ROSE_HEAD: [&str; 5] = [" hlm ", "hlCmd", "lCmCd", "mmCdd", " mdd "];

fn small_flowers(put: &mut impl FnMut(u16, Tile)) {
    // Dandelion: a round yellow head over two toothed leaves.
    let mut img = Img::new();
    flower_stem(&mut img, 7, 6, &[(13, -1, 4), (12, 1, 5)]);
    img.set(5, 12, shades(STEM)[0]);
    head(&mut img, 8, 5, &ROUND_HEAD, [238, 204, 46], [200, 150, 30]);
    put(tex::DANDELION, img.tile());
    // Golden dandelion: the same flower in gold, with a glint.
    let mut img = Img::new();
    flower_stem(&mut img, 7, 6, &[(13, -1, 4), (12, 1, 5)]);
    head(&mut img, 8, 5, &ROUND_HEAD, [232, 178, 40], [170, 110, 20]);
    img.set(6, 3, [255, 246, 190]);
    img.set(9, 6, [255, 236, 150]);
    put(792, img.tile());
    // Poppy: four red petals round a dark eye.
    let mut img = Img::new();
    flower_stem(&mut img, 8, 7, &[(12, -1, 4), (10, 1, 3)]);
    head(&mut img, 8, 5, &POPPY_HEAD, [206, 40, 34], [40, 30, 28]);
    put(tex::POPPY, img.tile());
    // Blue orchid: three small star heads on a branching stem.
    let mut img = Img::new();
    let s = shades(STEM);
    stalk(&mut img, &[(8, 15), (8, 8), (5, 5)], &s);
    stalk(&mut img, &[(8, 10), (11, 7)], &s);
    stalk(&mut img, &[(8, 8), (8, 4)], &s);
    leaf(&mut img, (8, 13), -1, 4, 1, &s);
    for (x, y) in [(5, 4), (11, 6), (8, 3)] {
        head(&mut img, x, y, &["  h  ", " hlm ", "hlolm", " mmd ", "  d  "], [48, 150, 226], [220, 236, 250]);
    }
    put(tex::BLUE_ORCHID, img.tile());
    // The overworld flowers in `overworld_blocks::t::FLOWERS` order:
    // cornflower, lily of the valley, oxeye daisy, azure bluet, allium,
    // then the red, orange, white and pink tulips.
    let mut img = Img::new();
    flower_stem(&mut img, 8, 6, &[(12, -1, 4), (11, 1, 4)]);
    head(&mut img, 8, 4, &CORNFLOWER_HEAD, [62, 98, 214], [40, 40, 120]);
    put(1578, img.tile());
    // Lily of the valley: an arching stem of white bells over broad leaves.
    let mut img = Img::new();
    let s = shades(DARK_LEAF);
    stalk(&mut img, &[(7, 15), (7, 6), (9, 3), (12, 4)], &shades(STEM));
    for (x0, lean) in [(6, -1), (9, 1)] {
        for t in 0..9 {
            let x = x0 + lean * (t / 4);
            img.set(x, 15 - t, if lean < 0 { s[2] } else { s[1] });
            img.set(x + lean, 15 - t, if lean < 0 { s[3] } else { s[0] });
        }
    }
    let bell = shades([236, 236, 226]);
    for (x, y) in [(8, 5), (10, 5), (12, 6), (6, 8)] {
        img.set(x, y, bell[2]);
        img.set(x + 1, y, bell[1]);
        img.set(x, y + 1, bell[3]);
        img.set(x + 1, y + 1, bell[0]);
    }
    put(1579, img.tile());
    // Oxeye daisy: white petals round a yellow eye.
    let mut img = Img::new();
    flower_stem(&mut img, 8, 7, &[(13, -1, 4), (11, 1, 4)]);
    head(&mut img, 8, 5, &DAISY_HEAD, [236, 236, 228], [232, 190, 40]);
    put(1580, img.tile());
    // Azure bluet: four tiny white stars with yellow eyes, low down.
    let mut img = Img::new();
    let s = shades(STEM);
    for (x, top) in [(4, 9), (7, 7), (10, 10), (12, 8)] {
        stalk(&mut img, &[(x, 15), (x, top)], &s);
        head(&mut img, x, top - 1, &STAR_HEAD, [226, 230, 236], [234, 200, 60]);
    }
    leaf(&mut img, (8, 14), -1, 4, 1, &s);
    leaf(&mut img, (9, 14), 1, 4, 1, &s);
    put(1581, img.tile());
    // Allium: a ball of violet florets on a tall bare stem.
    let mut img = Img::new();
    flower_stem(&mut img, 8, 8, &[(14, -1, 5), (14, 1, 4)]);
    blob(&mut img, (8.5, 5.0), (3.6, 3.6), &shades([168, 102, 214]), 256, 1581);
    for (x, y) in [(6, 3), (9, 4), (7, 6), (10, 6), (8, 2)] {
        img.set(x, y, [222, 178, 246]);
    }
    put(1582, img.tile());
    // Tulips: a cupped head over two long leaves.
    for (i, colour) in [[204, 44, 36], [232, 120, 34], [232, 230, 222], [234, 150, 186]].into_iter().enumerate() {
        let mut img = Img::new();
        let s = shades(STEM);
        stalk(&mut img, &[(8, 15), (8, 7)], &s);
        leaf(&mut img, (8, 15), -1, 4, 6, &s);
        leaf(&mut img, (8, 14), 1, 4, 5, &s);
        head(&mut img, 8, 5, &TULIP_HEAD, colour, colour);
        put(1583 + i as u16, img.tile());
    }
    // Wither rose: a near-black rose on a dark stem.
    let mut img = Img::new();
    let s = shades([46, 54, 34]);
    stalk(&mut img, &[(8, 15), (8, 7)], &s);
    leaf(&mut img, (8, 12), -1, 4, 2, &s);
    leaf(&mut img, (8, 10), 1, 3, 1, &s);
    head(&mut img, 8, 5, &ROSE_HEAD, [58, 48, 50], [18, 14, 16]);
    put(706, img.tile());
    put(3442, torchflower().tile());
    // Eyeblossoms: open, pale petals round a glowing orange eye; closed, a
    // grey bud folded shut.
    let mut img = Img::new();
    flower_stem(&mut img, 8, 7, &[(13, -1, 4), (12, 1, 4)]);
    head(&mut img, 8, 5, &DAISY_HEAD, [210, 206, 214], [246, 140, 36]);
    img.set(8, 5, [255, 214, 120]);
    put(3373, img.tile());
    let mut img = Img::new();
    flower_stem(&mut img, 8, 8, &[(13, -1, 4), (12, 1, 4)]);
    head(&mut img, 8, 5, &[" h ", "hlm", "hlm", "lmd", "lmd", " d "], [112, 108, 120], [90, 80, 96]);
    put(3374, img.tile());
    // Cactus flower: a pink bloom with a yellow heart on a short green base.
    let mut img = Img::new();
    let g = shades([70, 128, 58]);
    img.stamp(5, 12, &[" lllm ", "llmmmd", "lmmmdd", "mmmddd"], &[('l', g[2]), ('m', g[1]), ('d', g[0])]);
    head(
        &mut img,
        8,
        9,
        &["  hlm  ", " hllmd ", "hlloomd", "lmocomd", " mmmdd ", "  mdd  "],
        [234, 104, 168],
        [244, 210, 70],
    );
    put(2845, img.tile());
}

/// Torchflower: flame-shaped petals rising from a cup (Track 10b's potted
/// torchflower draws it too).
fn torchflower() -> Img {
    let mut img = Img::new();
    flower_stem(&mut img, 8, 9, &[(13, -1, 4), (12, 1, 4)]);
    torch_bloom(&mut img, 3);
    img
}

/// A torchflower's flame-shaped bloom, `stage` 1 (bud) to 3 (open).
fn torch_bloom(img: &mut Img, stage: i32) {
    let s = ramp([226, 92, 32], [700, 1000, 1180, 1300]);
    let hot = [250, 196, 64];
    let tips: &[(i32, i32)] = match stage {
        1 => &[(8, 8)],
        2 => &[(7, 6), (9, 6), (8, 5)],
        _ => &[(5, 4), (8, 2), (11, 4), (6, 6), (10, 6)],
    };
    for &(x, y) in tips {
        for t in 0..(10 - y).min(4) {
            img.set(x, y + t, if t == 0 { s[3] } else { s[2] });
            img.set(x + 1, y + t, if t == 0 { hot } else { s[1] });
        }
    }
    if stage == 3 {
        img.stamp(5, 8, &["dmmmmd", " dmmd "], &[('m', s[1]), ('d', s[0])]);
    }
}

fn tall_flowers(put: &mut impl FnMut(u16, Tile)) {
    let s = shades(STEM);
    let leafy = shades(LEAF);
    // The lower half of every tall flower: a leafy stalk, broad leaves.
    let lower = |seed: u32, broad: bool| {
        let mut img = Img::new();
        thick_stalk(&mut img, 7, 0, 15, &s);
        for (y, dir, len) in [(13, -1, 5), (10, 1, 6), (6, -1, 5), (3, 1, 4)] {
            let x = if dir < 0 { 7 } else { 8 };
            leaf(&mut img, (x, y), dir, len + broad as i32, 3, &leafy);
        }
        let _ = seed;
        img
    };
    // Sunflower: a big yellow head with a brown seed disc, facing out.
    put(1587, lower(1587, true).tile());
    let mut img = Img::new();
    thick_stalk(&mut img, 7, 9, 15, &s);
    leaf(&mut img, (7, 13), -1, 4, 2, &leafy);
    blob(&mut img, (8.0, 5.5), (5.2, 5.0), &shades([236, 190, 40]), 256, 1588);
    blob(&mut img, (8.0, 5.5), (2.6, 2.5), &shades([104, 66, 30]), 256, 1589);
    put(1588, img.tile());
    // Lilac: violet clusters on several stems.
    put(1589, lower(1589, false).tile());
    let mut img = Img::new();
    for (x, top) in [(5, 6), (8, 3), (11, 6)] {
        stalk(&mut img, &[(x, 15), (x, top)], &s);
    }
    let lilac = shades([186, 132, 202]);
    for (cx, cy, seed) in [(5.0, 6.5, 1), (8.5, 3.5, 2), (11.5, 7.0, 3)] {
        blob(&mut img, (cx, cy), (2.6, 3.6), &lilac, 220, 1590 + seed);
    }
    put(1590, img.tile());
    // Rose bush: dense leaves; red roses in the upper half.
    let bush = |seed: u32, top: f32| {
        let mut img = Img::new();
        crown(&mut img, (8.0, (top + 16.0) / 2.0), (7.6, (16.0 - top) / 2.0 + 0.4), &shades([60, 112, 44]), seed);
        img
    };
    put(1591, bush(1591, 0.0).tile());
    let mut img = bush(1592, 3.0);
    for (x, y) in [(4, 5), (9, 4), (11, 9), (6, 10), (12, 13)] {
        head(&mut img, x, y, &["hl", "md"], [196, 30, 36], [196, 30, 36]);
    }
    put(1592, img.tile());
    // Peony: leaves below, large pink blooms above.
    put(1593, bush(1593, 0.0).tile());
    let mut img = bush(1594, 6.0);
    let pink = shades([226, 156, 196]);
    blob(&mut img, (5.5, 6.0), (3.4, 3.0), &pink, 256, 1595);
    blob(&mut img, (11.0, 5.0), (3.2, 3.2), &pink, 256, 1596);
    put(1594, img.tile());
    // Pitcher plant: a tall stalk; the upper half carries two pink
    // pitchers with teal lips and a bud between.
    let mut img = Img::new();
    thick_stalk(&mut img, 7, 0, 15, &shades([66, 130, 86]));
    for (y, dir) in [(12, -1), (9, 1), (5, -1)] {
        leaf(&mut img, (if dir < 0 { 7 } else { 8 }, y), dir, 5, 3, &shades([72, 140, 92]));
    }
    put(3454, img.tile());
    put(3455, pitcher_top(4).tile());
}

/// The pitcher's upper half at `age` 3 or 4.
fn pitcher_top(age: u8) -> Img {
    let mut img = Img::new();
    let g = shades([66, 130, 86]);
    thick_stalk(&mut img, 7, 10, 15, &g);
    let pink = shades([170, 74, 112]);
    let lip = [76, 190, 186];
    for x0 in [2, 10] {
        img.stamp(
            x0,
            6,
            &["tttt", "lmmd", "lmmd", "lmdd", "lmdd", "mmdd", " md "],
            &[('t', lip), ('l', pink[2]), ('m', pink[1]), ('d', pink[0])],
        );
    }
    if age >= 4 {
        img.stamp(
            6,
            2,
            &[" hl ", "hlmd", "lmdd", " md "],
            &[('h', [236, 170, 196]), ('l', [210, 136, 164]), ('m', pink[1]), ('d', pink[0])],
        );
    }
    img
}

// ---- Saplings -----------------------------------------------------------

fn saplings(put: &mut impl FnMut(u16, Tile)) {
    let stem = |img: &mut Img, top: i32, bark: Px| thick_stalk(img, 7, top, 15, &shades(bark));
    // Oak: a round crown.
    let mut img = Img::new();
    stem(&mut img, 9, BARK);
    crown(&mut img, (8.0, 6.0), (5.2, 4.8), &shades([82, 136, 58]), 71);
    put(tex::OAK_SAPLING, img.tile());
    // Spruce: stacked tiers.
    let mut img = Img::new();
    stem(&mut img, 11, [92, 64, 38]);
    let s = shades([50, 92, 60]);
    for (top, w) in [(1, 1), (3, 3), (5, 2), (7, 4), (9, 3), (11, 5)] {
        for x in 8 - w..=7 + w {
            let k = if x < 7 {
                2
            } else if x > 8 {
                0
            } else {
                1
            };
            img.set(x, top, s[k]);
            if w >= 2 && (x - 7).abs() < w {
                img.set(x, top + 1, s[(k + 1).min(3) - (k == 2) as usize]);
            }
        }
        img.set(8 - w, top, s[3]);
    }
    put(tex::SPRUCE_SAPLING, img.tile());
    // Birch: a narrow oval crown on a pale, marked stem.
    let mut img = Img::new();
    stem(&mut img, 8, [210, 206, 194]);
    img.set(7, 11, [60, 56, 50]);
    img.set(8, 13, [60, 56, 50]);
    crown(&mut img, (8.0, 5.5), (3.8, 5.0), &shades([110, 160, 70]), 111);
    put(tex::BIRCH_SAPLING, img.tile());
    // Jungle: broad leaves fanning from the top of the stem.
    let mut img = Img::new();
    stem(&mut img, 7, [110, 84, 40]);
    let s = shades([46, 124, 34]);
    for (dir, len, lift) in [(-1, 6, 4), (1, 6, 4), (-1, 5, 1), (1, 5, 1)] {
        leaf(&mut img, (if dir < 0 { 7 } else { 8 }, 7), dir, len, lift, &s);
    }
    blob(&mut img, (8.0, 4.0), (2.2, 3.2), &s, 256, 112);
    put(tex::JUNGLE_SAPLING, img.tile());
    // Acacia: a leaning stem under a flat umbrella.
    let mut img = Img::new();
    let b = shades([120, 106, 96]);
    stalk(&mut img, &[(7, 15), (7, 10), (9, 6)], &b);
    stalk(&mut img, &[(8, 15), (8, 10)], &b);
    let s = shades([102, 132, 46]);
    blob(&mut img, (8.5, 4.5), (6.6, 2.2), &s, 230, 113);
    put(tex::ACACIA_SAPLING, img.tile());
    // Dark oak: a low, wide, dark crown on a short thick stem.
    let mut img = Img::new();
    stem(&mut img, 10, [70, 50, 30]);
    crown(&mut img, (8.0, 7.0), (6.2, 4.4), &shades([44, 88, 30]), 1556);
    put(1556, img.tile());
    // Mangrove propagule: a long green pod with a tuft of leaves.
    let mut img = Img::new();
    let pod = shades([104, 146, 60]);
    for y in 5..15 {
        img.set(7, y, pod[2]);
        img.set(8, y, pod[1]);
    }
    img.set(7, 15, pod[1]);
    img.set(7, 4, [150, 110, 60]);
    img.set(8, 4, [120, 86, 44]);
    let s = shades([66, 132, 48]);
    leaf(&mut img, (7, 3), -1, 4, 2, &s);
    leaf(&mut img, (8, 3), 1, 4, 2, &s);
    img.set(7, 1, s[3]);
    img.set(8, 1, s[2]);
    img.set(8, 2, s[1]);
    put(1557, img.tile());
    // Cherry: a pink crown on a dark stem.
    let mut img = Img::new();
    stem(&mut img, 9, [66, 40, 48]);
    crown(&mut img, (8.0, 6.0), (5.0, 4.6), &shades([230, 160, 186]), 1558);
    put(1558, img.tile());
    // Pale oak: a grey-green crown on a grey stem.
    let mut img = Img::new();
    stem(&mut img, 9, [104, 96, 92]);
    crown(&mut img, (8.0, 6.0), (5.0, 4.8), &shades([150, 160, 140]), 1559);
    put(1559, img.tile());
    // Bamboo shoot: a short jointed cane with two leaves.
    let mut img = Img::new();
    let cane = shades([98, 150, 54]);
    for y in 7..16 {
        img.set(7, y, if y == 11 { cane[0] } else { cane[2] });
        img.set(8, y, if y == 11 { cane[0] } else { cane[1] });
    }
    leaf(&mut img, (7, 8), -1, 5, 4, &shades([82, 150, 46]));
    leaf(&mut img, (8, 10), 1, 4, 3, &shades([82, 150, 46]));
    put(1570, img.tile());
    // Poplar: a tall narrow column of leaves.
    let mut img = Img::new();
    stem(&mut img, 11, [200, 196, 182]);
    crown(&mut img, (8.0, 6.0), (2.8, 6.0), &shades([102, 140, 56]), 759);
    put(759, img.tile());
}

// ---- Grasses, ferns and shrubs -----------------------------------------

fn grasses(put: &mut impl FnMut(u16, Tile)) {
    let g = ramp(GRASS, [640, 820, 1000, 1150]);
    let mut img = Img::new();
    blades(&mut img, 41, 9, (5, 11), &g, 15);
    put(tex::TALL_GRASS, img.tile());
    // Tall grass's halves: the lower full of blades, the upper their tips.
    let mut img = Img::new();
    blades(&mut img, 1597, 10, (14, 16), &g, 15);
    put(1597, img.tile());
    let mut img = Img::new();
    blades(&mut img, 1598, 9, (4, 12), &g, 15);
    put(1598, img.tile());
    // Fern: three arching fronds.
    let fern = ramp([96, 140, 64], [640, 820, 1000, 1150]);
    let mut img = Img::new();
    frond(&mut img, (8, 15), (8, 3), &fern);
    frond(&mut img, (7, 15), (3, 6), &fern);
    frond(&mut img, (8, 15), (13, 6), &fern);
    put(tex::FERN, img.tile());
    let mut img = Img::new();
    frond(&mut img, (8, 15), (7, 0), &fern);
    frond(&mut img, (7, 15), (2, 2), &fern);
    frond(&mut img, (8, 15), (13, 1), &fern);
    put(1595, img.tile());
    let mut img = Img::new();
    frond(&mut img, (8, 15), (8, 4), &fern);
    frond(&mut img, (7, 15), (2, 8), &fern);
    frond(&mut img, (9, 15), (14, 7), &fern);
    put(1596, img.tile());
    // Dead bush: bare forked twigs, lit on their upper side.
    let mut img = Img::new();
    let s = shades([150, 104, 56]);
    for (a, b) in [
        ((8, 15), (8, 10)),
        ((8, 10), (4, 5)),
        ((8, 10), (12, 4)),
        ((8, 12), (12, 9)),
        ((6, 8), (6, 3)),
        ((5, 6), (2, 5)),
        ((11, 6), (14, 6)),
        ((12, 9), (14, 8)),
    ] {
        img.line(a, b, s[1]);
    }
    for (x, y) in [(4, 5), (12, 4), (6, 3), (2, 5), (14, 6), (14, 8)] {
        img.set(x, y, s[3]);
    }
    edge_light(&mut img, &s);
    put(tex::DEAD_BUSH, img.tile());
    // Bush and firefly bush: a low rounded shrub of leaves.
    let mut img = Img::new();
    crown(&mut img, (8.0, 10.5), (7.2, 5.2), &shades([62, 116, 46]), 2840);
    put(2840, img.tile());
    let mut img = Img::new();
    crown(&mut img, (8.0, 9.5), (7.0, 6.2), &shades([52, 100, 56]), 2841);
    for (x, y) in [(4, 6), (11, 8), (7, 12), (9, 5)] {
        img.set(x, y, [222, 244, 112]);
        img.set(x + 1, y, [170, 204, 70]);
    }
    put(2841, img.tile());
    // The dry grasses: straw blades.
    let straw = shades([190, 164, 98]);
    for (layer, range, count) in [(2842, (4, 8), 8), (2843, (14, 16), 10), (2844, (5, 11), 9)] {
        let mut img = Img::new();
        blades(&mut img, layer as u32, count, range, &straw, 15);
        put(layer, img.tile());
    }
}

// ---- Crops ----------------------------------------------------------------

fn crops(put: &mut impl FnMut(u16, Tile)) {
    use crate::world::crops::t as ct;
    // Wheat: stalks rise and turn gold; ripe stalks carry grain heads.
    for stage in 0..8i32 {
        let mut img = Img::new();
        let ripe = stage * 1000 / 7;
        let mix = |a: Px, b: Px| -> Px {
            std::array::from_fn(|k| (a[k] as i32 + (b[k] as i32 - a[k] as i32) * ripe * ripe / 1_000_000) as u8)
        };
        let s = shades(mix([82, 150, 46], [204, 170, 72]));
        for (k, x) in [1, 4, 7, 10, 13].into_iter().enumerate() {
            let h = 2 + stage * 12 / 7 - (k as i32 % 2) * (1 + stage / 3);
            let lean = if k.is_multiple_of(2) { 0 } else { 1 };
            for t in 0..h {
                let xx = x + lean * (t / 6);
                img.set(xx, 15 - t, if t == h - 1 { s[3] } else { s[2 - (t % 4 == 3) as usize] });
                img.set(xx + 1, 15 - t, s[0]);
            }
            if stage == 7 {
                let top = 16 - h;
                let grain = shades([226, 190, 86]);
                for t in 0..4 {
                    let xx = x + lean * ((h - 1 - t) / 6);
                    img.set(xx, top + t, grain[if t % 2 == 0 { 3 } else { 2 }]);
                    img.set(xx + 1, top + t, grain[if t % 2 == 0 { 1 } else { 0 }]);
                    if t % 2 == 1 {
                        img.set(xx - 1, top + t, grain[2]);
                    }
                }
            } else if stage >= 5 {
                img.set(x + lean, 16 - h, s[3]);
            }
        }
        put(tex::WHEAT_0 + stage as u16, img.tile());
    }
    // Carrots and potatoes: feathery or broad leaves, the crop showing
    // at the ground when ripe.
    for (base, kind) in [(tex::CARROT_0, 0), (tex::POTATO_0, 1)] {
        for stage in 0..8i32 {
            let mut img = Img::new();
            let s = shades(if kind == 0 { [72, 148, 42] } else { [64, 136, 44] });
            let height = 2 + stage * 9 / 7;
            for (i, x) in [2, 6, 10, 13].into_iter().enumerate() {
                let h = height - (i as i32 % 2) * (1 + stage / 4);
                if kind == 0 {
                    frond(&mut img, (x, 15), (x + (i as i32 % 2) * 2 - 1, 15 - h.max(1)), &s);
                } else {
                    stalk(&mut img, &[(x, 15), (x, 15 - h.max(1))], &s);
                    img.set(x, 15 - h.max(1), s[3]);
                    img.set(x, 15, s[0]);
                    if h >= 3 {
                        blob(
                            &mut img,
                            (x as f32 + 0.5, 15.5 - h as f32 + 1.5),
                            (2.2, 1.6),
                            &s,
                            256,
                            base as u32 + i as u32,
                        );
                    }
                }
                if stage == 7 {
                    let c = if kind == 0 { [226, 120, 30] } else { [196, 152, 84] };
                    let c = shades(c);
                    img.set(x, 15, c[2]);
                    img.set(x + 1, 15, c[1]);
                    img.set(x, 14, c[3]);
                    img.set(x + 1, 14, c[0]);
                }
            }
            put(base + stage as u16, img.tile());
        }
    }
    // Beetroots: leaves with red veins; the root's shoulder when ripe.
    for age in 0..4i32 {
        let mut img = Img::new();
        let s = shades([60, 136, 48]);
        let vein = [150, 34, 60];
        let h = 3 + age * 3;
        for (x, dir) in [(4, -1), (8, 1), (11, 1), (6, -1)] {
            stalk(&mut img, &[(x, 15), (x + dir, 15 - h)], &[vein, vein, vein, vein]);
            img.set(x + dir, 15 - h, s[3]);
            img.set(x + 2 * dir, 15 - h, s[1]);
            if h >= 4 {
                blob(&mut img, ((x + dir) as f32 + 0.5, 15.5 - h as f32 + 2.0), (2.0, 2.4), &s, 256, 3004 + x as u32);
                img.line((x, 15 - 1), (x + dir, 15 - h + 2), vein);
            }
        }
        if age == 3 {
            let r = shades([162, 36, 66]);
            img.stamp(3, 13, &["lmmd", "mmdd"], &[('l', r[2]), ('m', r[1]), ('d', r[0])]);
            img.stamp(9, 13, &["lmmd", "mmdd"], &[('l', r[2]), ('m', r[1]), ('d', r[0])]);
        }
        put(ct::BEETROOT + age as u16, img.tile());
    }
    // Melon and pumpkin stems: a stem that rises and yellows with age;
    // attached, it bends over to its fruit.
    for age in 0..8i32 {
        let mut img = Img::new();
        let colour = if age >= 6 { [170, 156, 52] } else { [76, 150, 46] };
        let s = shades(colour);
        let h = 3 + age * 13 / 7;
        for t in 0..h {
            let x = 7 + ((t / 3) % 2);
            let k = if t == 0 {
                0
            } else if t == h - 1 {
                3
            } else {
                2 - (t % 3 == 2) as usize
            };
            img.set(x, 15 - t, s[k]);
        }
        leaf(&mut img, (7, 16 - h + 1), -1, 1 + age / 2, 1, &s);
        leaf(&mut img, (8, 16 - h + 3), 1, 1 + age / 2, 1, &s);
        put(ct::STEM + age as u16, img.tile());
    }
    for i in 0..4u16 {
        let mut img = Img::new();
        let s = shades([170, 156, 52]);
        stalk(&mut img, &[(8, 15), (8, 8), (10, 5), (15, 4)], &s);
        leaf(&mut img, (8, 11), -1, 4, 2, &shades([90, 150, 50]));
        leaf(&mut img, (9, 7), -1, 3, 2, &shades([90, 150, 50]));
        // The existing models read an even layer as bending right.
        put(ct::ATTACHED + i, if i.is_multiple_of(2) { img.tile() } else { img.flipped().tile() });
    }
    // Torchflower crop: a seedling, then a stalk with a closed bud.
    use crate::world::sniffer_plants::t as sp;
    for age in 0..2 {
        let mut img = Img::new();
        let s = shades([66, 138, 60]);
        let top = if age == 0 { 10 } else { 7 };
        stalk(&mut img, &[(8, 15), (8, top)], &s);
        leaf(&mut img, (8, 14), -1, 4, 2, &s);
        leaf(&mut img, (8, 13), 1, 4, 2, &s);
        if age == 0 {
            img.set(7, 9, [150, 90, 70]);
            img.set(8, 9, [120, 70, 56]);
        } else {
            torch_bloom(&mut img, 1);
            torch_bloom(&mut img, 2);
        }
        put(sp::TORCH + age, img.tile());
    }
    // Pitcher crop: ages 0-4, lower then upper halves; the upper half is
    // empty until age 3, as the models expect.
    for upper in [false, true] {
        for age in 0..5u8 {
            let layer = sp::PITCHER + age as u16 + if upper { 5 } else { 0 };
            let mut img = Img::new();
            let g = shades([66, 132, 86]);
            if upper {
                if age >= 3 {
                    img = pitcher_top(age);
                }
            } else if age >= 3 {
                thick_stalk(&mut img, 7, 0, 15, &g);
                for (y, dir) in [(12, -1), (8, 1), (4, -1)] {
                    leaf(&mut img, (if dir < 0 { 7 } else { 8 }, y), dir, 5, 3, &g);
                }
            } else {
                let h = 3 + age as i32 * 4;
                thick_stalk(&mut img, 7, 16 - h, 15, &if age == 0 { shades([142, 84, 72]) } else { g });
                if age > 0 {
                    leaf(&mut img, (7, 14), -1, 5, 2, &g);
                    leaf(&mut img, (8, 13), 1, 5, 2, &g);
                }
            }
            put(layer, img.tile());
        }
    }
    // Nether wart: red nubs, taller and swollen when ripe.
    for i in 0..3i32 {
        let mut img = Img::new();
        let s = shades(if i == 2 { [170, 36, 40] } else { [140, 30, 36] });
        for (k, x) in [2, 6, 10, 13].into_iter().enumerate() {
            let h = 3 + i * 3 - (k as i32 % 2) * (1 + i);
            stalk(&mut img, &[(x, 15), (x + (k as i32 % 3) - 1, 15 - h)], &s);
            let tip = (x + (k as i32 % 3) - 1, 15 - h);
            let r = 0.9 + i as f32 * 0.5;
            blob(&mut img, (tip.0 as f32 + 0.5, tip.1 as f32 + 0.5), (r, r), &s, 256, 153 + k as u32);
            img.set(tip.0, tip.1, s[3]);
            img.set(x, 15, s[0]);
        }
        put(tex::NETHER_WART_0 + i as u16, img.tile());
    }
    // Sweet berry bush: a sprig, a small bush, a bush with green berries,
    // then with ripe red ones.
    for age in 0..4u16 {
        let mut img = Img::new();
        let s = shades([46, 100, 50]);
        match age {
            0 => {
                stalk(&mut img, &[(8, 15), (8, 9)], &s);
                leaf(&mut img, (8, 12), -1, 3, 2, &s);
                leaf(&mut img, (8, 10), 1, 3, 2, &s);
            }
            1 => crown(&mut img, (8.0, 11.5), (5.0, 4.4), &s, 1602),
            _ => crown(&mut img, (8.0, 9.0), (7.4, 6.8), &s, 1603 + age as u32),
        }
        if age >= 2 {
            let berry = if age == 3 { shades([196, 32, 46]) } else { shades([120, 170, 70]) };
            for (x, y) in [(4, 6), (10, 5), (7, 10), (12, 11), (3, 12)] {
                img.set(x, y, berry[3]);
                img.set(x + 1, y, berry[1]);
                img.set(x, y + 1, berry[1]);
                img.set(x + 1, y + 1, berry[0]);
            }
        }
        put(1601 + age, img.tile());
    }
}

// ---- Mushrooms and the other cross-model plants --------------------------

/// A mushroom: a stem and a cap lit at its upper left with a dark gill
/// line under it; `spots` dots the cap.
fn mushroom(img: &mut Img, cap: Px, stem: Px, (x0, w, top): (i32, i32, i32), spots: Option<Px>) {
    let s = shades(stem);
    for y in top + 2..16 {
        img.set(x0 + w / 2 - 1, y, s[2]);
        img.set(x0 + w / 2, y, s[1]);
    }
    let c = shades(cap);
    for x in x0..x0 + w {
        let edge = x == x0 || x == x0 + w - 1;
        let k = if x < x0 + w / 2 - 1 {
            2
        } else if x > x0 + w / 2 {
            0
        } else {
            1
        };
        if !edge {
            img.set(x, top, c[if k == 2 { 3 } else { 2 }]);
        }
        img.set(x, top + 1, c[k.max(1) - (k == 0) as usize]);
        img.set(x, top + 2, c[0]);
    }
    img.set(x0 + w / 2 - 1, top + 3, [62, 46, 36]);
    img.set(x0 + w / 2, top + 3, [62, 46, 36]);
    if let Some(spot) = spots {
        for x in [x0 + 1, x0 + w - 3] {
            img.set(x, top + 1, spot);
        }
        img.set(x0 + w / 2, top, spot);
    }
}

fn other_plants(put: &mut impl FnMut(u16, Tile)) {
    // Mushrooms keep their small size: a stem and a cap six texels wide.
    let mut img = Img::new();
    mushroom(&mut img, [150, 108, 74], [214, 196, 162], (5, 6, 10), None);
    put(tex::BROWN_MUSHROOM, img.tile());
    let mut img = Img::new();
    mushroom(&mut img, [190, 40, 32], [218, 202, 170], (5, 6, 10), Some([240, 228, 210]));
    put(tex::RED_MUSHROOM, img.tile());
    // Nether fungi: a big cap over a twisted stem, with dangling bits.
    for (layer, cap, spot) in [(1324u16, [160, 34, 30], [236, 132, 48]), (1325, [26, 130, 124], [236, 128, 44])] {
        let mut img = Img::new();
        let st = shades([214, 196, 176]);
        stalk(&mut img, &[(8, 15), (7, 11), (8, 7)], &st);
        img.set(9, 13, st[0]);
        blob(&mut img, (8.0, 4.5), (6.4, 3.0), &shades(cap), 256, layer as u32);
        for x in [3, 6, 10, 12] {
            img.set(x, 7, shades(cap)[0]);
        }
        img.set(4, 8, shades(cap)[0]);
        img.set(11, 8, shades(cap)[0]);
        for (x, y) in [(5, 3), (9, 2), (11, 4)] {
            img.set(x, y, spot);
        }
        put(layer, img.tile());
    }
    // Sugar cane: three jointed canes running the full height (they
    // stack), each lit on its left, with a leaf or two.
    let mut img = Img::new();
    let cane = shades([136, 186, 82]);
    for (x, offset) in [(3, 0), (8, 3), (12, 1)] {
        for y in 0..16 {
            let joint = (y + offset) % 6 == 0;
            img.set(x, y, if joint { [206, 226, 150] } else { cane[2] });
            img.set(x + 1, y, if joint { cane[1] } else { cane[0] });
        }
    }
    leaf(&mut img, (5, 5), 1, 3, 2, &shades([110, 172, 62]));
    leaf(&mut img, (12, 11), -1, 3, 2, &shades([110, 172, 62]));
    put(tex::SUGAR_CANE, img.tile());
    // Nether roots: a tangle of curling roots; sprouts short and spiky.
    for (layer, c) in [(1326u16, [128, 20, 40]), (1327, [24, 148, 136])] {
        let mut img = Img::new();
        let s = shades(c);
        for (k, (x, top, curl)) in [(2, 5, 1), (5, 2, -1), (8, 4, 1), (11, 1, -1), (13, 6, -1)].into_iter().enumerate()
        {
            for y in top..16 {
                let dx = if y < top + 3 { curl * (top + 3 - y) / 2 } else { 0 };
                img.set(x + dx, y, s[if k.is_multiple_of(2) { 2 } else { 1 }]);
            }
            img.set(x + curl * 2, top - 1 + 1, s[3]);
        }
        put(layer, img.tile());
    }
    let mut img = Img::new();
    blades(&mut img, 1328, 8, (2, 6), &shades([20, 150, 132]), 15);
    put(1328, img.tile());
    // Weeping vines hang (their tip at the bottom); twisting vines climb
    // (their tip at the top). The plant halves run the full height.
    let vine = |c: Px, tip: Option<bool>, seed: u32| {
        let mut img = Img::new();
        let s = shades(c);
        for (k, x0) in [4, 8, 11].into_iter().enumerate() {
            for y in 0..16 {
                let x = x0 + ((y as usize + k * 5) / 4 % 2) as i32;
                let end = match tip {
                    Some(true) => y > 11 + k as i32,
                    Some(false) => y < 4 - k as i32 + 1,
                    None => false,
                };
                if end {
                    continue;
                }
                img.set(x, y, s[if (y + k as i32) % 4 == 0 { 3 } else { 2 }]);
                if hu(seed, k, y as usize, 92).is_multiple_of(3) {
                    img.set(x + 1, y, s[0]);
                }
            }
        }
        img
    };
    put(1329, vine([150, 26, 34], Some(true), 1329).tile());
    put(1330, vine([150, 26, 34], None, 1330).tile());
    put(1331, vine([20, 150, 136], Some(false), 1331).tile());
    put(1332, vine([20, 150, 136], None, 1332).tile());
    // Kelp: wavy blades with small air bladders; the top ends in a tip.
    let kelp = |top: bool| {
        let mut img = Img::new();
        let s = shades([80, 132, 46]);
        for y in 0..16 {
            if top && y < 3 {
                continue;
            }
            let x = 7 + (y / 3) % 2;
            img.set(x, y, s[2]);
            img.set(x + 1, y, s[1]);
            if y % 5 == 2 {
                img.set(x - 1, y, s[3]);
                img.set(x + 2, y, [170, 150, 60]);
            }
        }
        leaf(&mut img, (8, 10), 1, 4, 2, &s);
        leaf(&mut img, (7, 5), -1, 4, 2, &s);
        img
    };
    put(1501, kelp(true).tile());
    put(1502, kelp(false).tile());
    // Seagrass: ribbon blades.
    let sea = ramp([66, 140, 58], [640, 820, 1000, 1150]);
    for (layer, range, count) in [(1503u16, (8, 13), 8), (1504, (14, 16), 9), (1505, (5, 12), 8)] {
        let mut img = Img::new();
        blades(&mut img, layer as u32, count, range, &sea, 15);
        put(layer, img.tile());
    }
    // Cave vines (tip, tip with berries, plant, plant with berries):
    // hanging leafy vines, glow berries on the lit ones.
    for i in 0..4u16 {
        let mut img = Img::new();
        let s = shades([74, 118, 40]);
        let tip = i < 2;
        for y in 0..16 {
            if tip && y > 12 {
                continue;
            }
            let x = 7 + (y / 4) % 2;
            img.set(x, y, s[1]);
            if y % 3 == 1 {
                img.set(x - 1, y, s[2]);
                img.set(x - 2, y + 1, s[3]);
            }
            if y % 3 == 2 {
                img.set(x + 1, y, s[1]);
                img.set(x + 2, y + 1, s[0]);
            }
        }
        if i % 2 == 1 {
            for (x, y) in [(4, 4), (9, 10)] {
                img.stamp(
                    x,
                    y,
                    &["hl", "lm", "md"],
                    &[('h', [255, 236, 150]), ('l', [250, 176, 52]), ('m', [218, 120, 30]), ('d', [160, 80, 20])],
                );
            }
        }
        put(1490 + i, img.tile());
    }
    // Spore blossom: pink petals hanging from the ceiling (the top edge)
    // round a green heart.
    let mut img = Img::new();
    let pink = shades([214, 96, 170]);
    blob(&mut img, (8.0, 2.0), (3.0, 2.0), &shades([70, 120, 50]), 256, 1494);
    for (x, dir) in [(3, -1), (12, 1), (6, -1), (9, 1)] {
        for t in 0..5 {
            img.set(
                x + dir * (t / 3),
                2 + t,
                pink[if t == 4 {
                    3
                } else if dir < 0 {
                    2
                } else {
                    1
                }],
            );
        }
    }
    put(1494, img.tile());
    // Hanging roots: brown strands from the top.
    let mut img = Img::new();
    let s = shades([150, 106, 76]);
    for (x, len) in [(3, 9), (5, 13), (8, 11), (10, 14), (12, 8)] {
        for y in 0..len {
            img.set(
                x + (y / 5) % 2,
                y,
                s[if y == len - 1 {
                    3
                } else if x % 2 == 0 {
                    2
                } else {
                    1
                }],
            );
        }
    }
    put(1495, img.tile());
    // Big dripleaf stem (it stacks), small dripleaf's stem and leaf.
    let mut img = Img::new();
    thick_stalk(&mut img, 7, 0, 15, &shades([92, 138, 52]));
    put(1498, img.tile());
    let dl = shades([92, 146, 54]);
    let mut img = Img::new();
    thick_stalk(&mut img, 7, 0, 15, &dl);
    leaf(&mut img, (7, 12), -1, 4, 2, &dl);
    leaf(&mut img, (8, 9), 1, 4, 2, &dl);
    put(1499, img.tile());
    let mut img = Img::new();
    thick_stalk(&mut img, 7, 6, 15, &dl);
    blob(&mut img, (5.0, 5.0), (4.0, 1.8), &dl, 256, 1500);
    blob(&mut img, (11.0, 4.0), (4.0, 1.8), &dl, 256, 1501);
    put(1500, img.tile());
    // Pale hanging moss (tip, plant): grey strands hanging down.
    for (layer, tip) in [(1576u16, true), (1577, false)] {
        let mut img = Img::new();
        let s = shades([150, 156, 146]);
        for (k, x) in [3, 5, 7, 9, 11, 13].into_iter().enumerate() {
            let len = if tip { 7 + (hu(layer as u32, k, 0, 93) % 8) as i32 } else { 16 };
            for y in 0..len {
                let shade = if (y + k as i32) % 5 == 4 {
                    0
                } else if k.is_multiple_of(2) {
                    2
                } else {
                    1
                };
                img.set(x + ((y + k as i32) / 6) % 2, y, s[shade]);
            }
            if tip {
                img.set(x, len - 1, s[3]);
            }
        }
        put(layer, img.tile());
    }
    // Red shrub: a low rounded bush of red leaves.
    let mut img = Img::new();
    crown(&mut img, (8.0, 10.5), (7.0, 5.2), &shades([168, 54, 44]), 791);
    put(791, img.tile());
}

// ---- Wooden doors and trapdoors ------------------------------------------

/// What a texel of a door or trapdoor is.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    /// The raised frame: stiles, rails and muntins.
    Frame,
    /// A recessed panel.
    Panel,
    /// Flat boards; a seam is a one-texel joint between them.
    Board,
    Seam,
    /// An opening (see-through).
    Hole,
    /// Iron fittings.
    Metal,
}

use Part::*;

/// Shades a door or trapdoor face from its parts: the frame lit at its
/// top and left edges and where it is the lower or right wall of an
/// opening or recess, shaded where it is the upper or left wall; recesses
/// shaded under the frame above and left of them; seams dark; boards with
/// short grain. `edges` lights the outer top-left and shades the outer
/// bottom-right (false for door halves along the joint between them).
fn shaded_face(
    parts: &[[Part; N]; N],
    wood: Px,
    seed: u32,
    vertical: bool,
    (top_edge, bottom_edge): (bool, bool),
) -> Tile {
    let pal = ramp(wood, [560, 720, 880, 1000, 1130, 1240]);
    let metal = ramp([96, 96, 104], [600, 1000, 1500]);
    let at = |x: i32, y: i32| -> Option<Part> {
        (0..N as i32).contains(&x).then_some(())?;
        (0..N as i32).contains(&y).then_some(())?;
        Some(parts[y as usize][x as usize])
    };
    let low = |p: Option<Part>| matches!(p, Some(Panel | Hole | Seam));
    std::array::from_fn(|i| {
        let (x, y) = ((i % N) as i32, (i / N) as i32);
        let part = parts[y as usize][x as usize];
        let grain = {
            let (a, b) = if vertical { (x, y / 4) } else { (y, x / 4) };
            if hu(seed, a as usize, b as usize, 94).is_multiple_of(3) { 2 } else { 3 }
        };
        let c = match part {
            Hole => return [0; 4],
            Seam => pal[0],
            Metal => {
                let lit = at(x - 1, y) != Some(Metal) || at(x, y - 1) != Some(Metal);
                let dark = at(x + 1, y) != Some(Metal) || at(x, y + 1) != Some(Metal);
                metal[if lit && !dark {
                    2
                } else if dark && !lit {
                    0
                } else {
                    1
                }]
            }
            Frame => {
                if x == 0 || y == 0 && top_edge {
                    pal[5]
                } else if x == N as i32 - 1 || y == N as i32 - 1 && bottom_edge {
                    pal[1]
                } else if low(at(x, y - 1)) || low(at(x - 1, y)) {
                    pal[4]
                } else if low(at(x, y + 1)) || low(at(x + 1, y)) {
                    pal[1]
                } else {
                    pal[grain + 1]
                }
            }
            Panel => {
                if matches!(at(x, y - 1), Some(Frame | Metal)) || matches!(at(x - 1, y), Some(Frame | Metal)) {
                    pal[1]
                } else {
                    pal[grain]
                }
            }
            Board => {
                if at(x - 1, y) == Some(Seam) {
                    pal[4]
                } else {
                    pal[grain + 1]
                }
            }
        };
        [c[0], c[1], c[2], 255]
    })
}

/// A wood's door and trapdoor: plank colour and design.
struct DoorWood {
    top: u16,
    bottom: u16,
    trapdoor: u16,
    wood: Px,
    design: u8,
}

const DOOR_WOODS: [DoorWood; 13] = [
    DoorWood { top: tex::DOOR_TOP, bottom: tex::DOOR_BOTTOM, trapdoor: 1124, wood: [162, 130, 78], design: 0 },
    DoorWood {
        top: tex::SPRUCE_DOOR_TOP,
        bottom: tex::SPRUCE_DOOR_BOTTOM,
        trapdoor: 1816,
        wood: [114, 84, 50],
        design: 1,
    },
    DoorWood {
        top: tex::BIRCH_DOOR_TOP,
        bottom: tex::BIRCH_DOOR_BOTTOM,
        trapdoor: 1817,
        wood: [196, 180, 124],
        design: 2,
    },
    DoorWood {
        top: tex::JUNGLE_DOOR_TOP,
        bottom: tex::JUNGLE_DOOR_BOTTOM,
        trapdoor: 1818,
        wood: [160, 114, 80],
        design: 3,
    },
    DoorWood {
        top: tex::ACACIA_DOOR_TOP,
        bottom: tex::ACACIA_DOOR_BOTTOM,
        trapdoor: 1819,
        wood: [170, 92, 50],
        design: 4,
    },
    DoorWood {
        top: tex::DARK_OAK_DOOR_TOP,
        bottom: tex::DARK_OAK_DOOR_BOTTOM,
        trapdoor: 1820,
        wood: [78, 52, 28],
        design: 5,
    },
    DoorWood {
        top: tex::MANGROVE_DOOR_TOP,
        bottom: tex::MANGROVE_DOOR_BOTTOM,
        trapdoor: 1821,
        wood: [117, 54, 48],
        design: 6,
    },
    DoorWood {
        top: tex::CHERRY_DOOR_TOP,
        bottom: tex::CHERRY_DOOR_BOTTOM,
        trapdoor: 1822,
        wood: [222, 174, 168],
        design: 7,
    },
    DoorWood {
        top: tex::BUILDING_DOORS + 1,
        bottom: tex::BUILDING_DOORS,
        trapdoor: tex::BUILDING_TRAPDOORS,
        wood: [222, 212, 206],
        design: 8,
    },
    DoorWood {
        top: tex::BUILDING_DOORS + 3,
        bottom: tex::BUILDING_DOORS + 2,
        trapdoor: tex::BUILDING_TRAPDOORS + 1,
        wood: [194, 172, 80],
        design: 9,
    },
    DoorWood {
        top: tex::NETHER_DOORS + 1,
        bottom: tex::NETHER_DOORS,
        trapdoor: tex::NETHER_TRAPDOORS,
        wood: [101, 48, 70],
        design: 10,
    },
    DoorWood {
        top: tex::NETHER_DOORS + 3,
        bottom: tex::NETHER_DOORS + 2,
        trapdoor: tex::NETHER_TRAPDOORS + 1,
        wood: [43, 104, 99],
        design: 11,
    },
    DoorWood { top: 761, bottom: 760, trapdoor: 762, wood: [204, 194, 166], design: 12 },
];

/// The interior (columns and rows 2..=13) of a door half or trapdoor of
/// `design`; `half` 0 the top, 1 the bottom, 2 the trapdoor.
fn interior(design: u8, half: u8, x: i32, y: i32) -> Part {
    let (cx, cy) = (2 * x - 15, 2 * y - 15); // half texels from the centre
    let in_box = |x0: i32, x1: i32, y0: i32, y1: i32| (x0..=x1).contains(&x) && (y0..=y1).contains(&y);
    match (design, half) {
        // Oak: four panes; two raised-and-fielded panels below.
        (0, 0) => {
            if (in_box(3, 6, 3, 6) || in_box(9, 12, 3, 6) || in_box(3, 6, 9, 12) || in_box(9, 12, 9, 12)) && y != 9 {
                Hole
            } else {
                Frame
            }
        }
        (0, 1) => {
            if in_box(3, 12, 3, 7) || in_box(3, 12, 9, 12) {
                if in_box(5, 10, 5, 5) || in_box(5, 10, 10, 11) { Frame } else { Panel }
            } else {
                Frame
            }
        }
        (0, _) => {
            if in_box(3, 6, 3, 6) || in_box(9, 12, 3, 6) || in_box(3, 6, 9, 12) || in_box(9, 12, 9, 12) {
                Hole
            } else {
                Frame
            }
        }
        // Spruce: vertical boards; a slit to look through; a Z brace.
        (1, 0) => {
            if in_box(4, 11, 4, 5) {
                Hole
            } else if x % 3 == 1 {
                Seam
            } else {
                Board
            }
        }
        (1, 1) => {
            if y == 4 || y == 11 || (y - 4) == (11 - x) && in_box(3, 12, 4, 11) {
                Frame
            } else if x % 3 == 1 {
                Seam
            } else {
                Board
            }
        }
        (1, _) => {
            if y == 4 || y == 11 {
                Frame
            } else if in_box(5, 10, 6, 9) && (x + y) % 2 == 0 {
                Hole
            } else if x % 3 == 1 {
                Seam
            } else {
                Board
            }
        }
        // Birch: a grid of nine small panes; one deep panel below.
        (2, 0) | (2, 2) => {
            if [3, 4, 7, 8, 11, 12].contains(&x) && [3, 4, 7, 8, 11, 12].contains(&y) {
                Hole
            } else {
                Frame
            }
        }
        (2, _) => {
            if in_box(3, 12, 3, 12) {
                if in_box(6, 9, 6, 9) { Frame } else { Panel }
            } else {
                Frame
            }
        }
        // Jungle: a round porthole; boards with iron studs below.
        (3, 0) | (3, 2) => {
            let d = cx * cx + cy * cy;
            if d < 64 {
                if cx.abs() <= 1 || cy.abs() <= 1 { Frame } else { Hole }
            } else if d < 100 {
                Metal
            } else {
                Frame
            }
        }
        (3, _) => {
            if [4, 11].contains(&x) && [4, 11].contains(&y) {
                Metal
            } else if x % 4 == 1 {
                Seam
            } else {
                Board
            }
        }
        // Acacia: a diamond window; two tall panels, each with a diamond.
        (4, 0) | (4, 2) => {
            let d = cx.abs() + cy.abs();
            if d < 12 && d > 2 { Hole } else { Frame }
        }
        (4, _) => {
            if in_box(3, 6, 3, 12) || in_box(9, 12, 3, 12) {
                let px = if x < 8 { 2 * x - 9 } else { 2 * x - 21 };
                if px.abs() + (cy.abs() / 2) < 3 { Frame } else { Panel }
            } else {
                Frame
            }
        }
        // Dark oak: heavy boards with iron straps, a barred grille.
        (5, 0) | (5, 2) => {
            if in_box(4, 11, 4, 8) {
                if x == 6 || x == 9 { Metal } else { Hole }
            } else if y == 11 {
                Metal
            } else if x % 4 == 1 {
                Seam
            } else {
                Board
            }
        }
        (5, _) => {
            if y == 4 || y == 11 {
                Metal
            } else if x % 4 == 1 {
                Seam
            } else {
                Board
            }
        }
        // Mangrove: two tall lights split by a bar; plank courses below.
        (6, 0) | (6, 2) => {
            if (in_box(3, 6, 3, 12) || in_box(9, 12, 3, 12)) && y != 7 && y != 8 {
                Hole
            } else {
                Frame
            }
        }
        (6, _) => {
            if y % 3 == 1 {
                Seam
            } else {
                Board
            }
        }
        // Cherry: an arched light; a panel with a carved blossom.
        (7, 0) | (7, 2) => {
            let arch = y >= 3 + (cx * cx) / 40;
            if in_box(3, 12, 3, 12) && arch && x != 7 && x != 8 { Hole } else { Frame }
        }
        (7, _) => {
            if in_box(3, 12, 3, 12) {
                let flower = (cx.abs() <= 1 && cy.abs() <= 5) || (cy.abs() <= 1 && cx.abs() <= 5);
                if flower { Frame } else { Panel }
            } else {
                Frame
            }
        }
        // Pale oak: two narrow lights; three tall panels below.
        (8, 0) | (8, 2) => {
            if in_box(4, 6, 3, 12) || in_box(9, 11, 3, 12) {
                Hole
            } else {
                Frame
            }
        }
        (8, _) => {
            if in_box(3, 12, 3, 12) && ![6, 9].contains(&x) {
                Panel
            } else {
                Frame
            }
        }
        // Bamboo: canes side by side, open between them at the top.
        (9, h) => {
            let node = (y + x / 3 * 2) % 7 == 0;
            if x % 3 == 1 {
                if h != 1 && in_box(2, 13, 3, 9) { Hole } else { Seam }
            } else if node {
                Frame
            } else {
                Board
            }
        }
        // Crimson and warped: rows of slots; a panel framed twice below.
        (10 | 11, 0) | (10 | 11, 2) => {
            if (in_box(3, 6, 3, 12) || in_box(9, 12, 3, 12)) && y % 3 != 2 {
                Hole
            } else {
                Frame
            }
        }
        (10 | 11, _) => {
            if in_box(3, 12, 3, 12) {
                if in_box(5, 10, 5, 10) && !in_box(6, 9, 6, 9) { Frame } else { Panel }
            } else {
                Frame
            }
        }
        // Poplar: three slits; chevron boards below.
        (_, 0) | (_, 2) => {
            if in_box(3, 12, 3, 12) && [4, 7, 8, 11].contains(&x) {
                Hole
            } else {
                Frame
            }
        }
        _ => {
            if (if x < 8 { x + y } else { y - x + 15 }) % 4 == 0 {
                Seam
            } else {
                Board
            }
        }
    }
}

fn doors(put: &mut impl FnMut(u16, Tile)) {
    for (i, w) in DOOR_WOODS.iter().enumerate() {
        for half in 0..3u8 {
            let mut parts = [[Frame; N]; N];
            for (y, row) in parts.iter_mut().enumerate() {
                for (x, p) in row.iter_mut().enumerate() {
                    let (x, y) = (x as i32, y as i32);
                    // Stiles two wide; the top rail two deep, the rails at
                    // the halves' joint two deep on each side.
                    let border = !(2..=13).contains(&x) || !(2..=13).contains(&y);
                    *p = if border { Frame } else { interior(w.design, half, x, y) };
                }
            }
            // Hinges on the left stile.
            if half < 2 {
                for y in [3, 4, 11, 12] {
                    parts[y][0] = Metal;
                }
            }
            let edges = match half {
                0 => (true, false),
                1 => (false, true),
                _ => (true, true),
            };
            let seed = 600 + i as u32 * 3 + half as u32;
            let tile = shaded_face(&parts, w.wood, seed, w.design != 6, edges);
            put([w.top, w.bottom, w.trapdoor][half as usize], tile);
        }
    }
}

// ---- Chests -----------------------------------------------------------------

const CHEST_WOOD: Px = [170, 120, 60];
const CHEST_RIM: Px = [78, 52, 30];

/// A chest face: plank boards in a dark rim, with the lid's seam on the
/// sides and front and a latch on the front (`latch` its colour).
fn chest(face: u8, latch: Option<Px>, seed: u32) -> Tile {
    let pal = ramp(CHEST_WOOD, [560, 720, 880, 1000, 1130, 1240]);
    let rim = ramp(CHEST_RIM, [800, 1000, 1250]);
    let boards = planks_grid(seed);
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let outer = x == 0 || y == 0 || x == 15 || y == 15;
            let seam = face != 0 && (y == 5 || y == 6);
            let c = if outer {
                if x == 0 || y == 0 { rim[2] } else { rim[0] }
            } else if seam {
                if y == 5 { rim[0] } else { rim[1] }
            } else {
                let k = boards[y as usize][x as usize] as usize;
                let lit = x == 1 || y == 1 || face != 0 && y == 7;
                let dark = x == 14 || y == 14 || face != 0 && y == 4;
                if lit {
                    pal[4]
                } else if dark {
                    pal[1]
                } else {
                    pal[[1, 2, 3, 3, 4][k]]
                }
            };
            img.set(x, y, c);
        }
    }
    if let Some(l) = latch {
        let m = ramp(l, [560, 800, 1000, 1250]);
        img.stamp(
            6,
            3,
            &["dddd", "dhld", "dlmd", "dlmd", "dmmd", " dd "],
            &[('d', m[0]), ('m', m[1]), ('l', m[2]), ('h', m[3])],
        );
    }
    img.tile()
}

// ---- Village and farm blocks ------------------------------------------------

/// Straw: strands along the rows (`across` turns them to columns), each a
/// short streak lit at its start.
fn straw(seed: u32, across: bool) -> [[u8; N]; N] {
    let mut g = [[2u8; N]; N];
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let (a, b) = if across { (x, y) } else { (y, x) };
            let h = hu(seed, a, b / 4, 95);
            *c = match h % 5 {
                0 => 1,
                1 | 2 => 2,
                3 => 3,
                _ => 4,
            };
            // A dark gap between strands now and then.
            if b % 4 == (h >> 8) as usize % 4 && (h >> 10).is_multiple_of(3) {
                *c = 0;
            }
        }
    }
    g
}

fn village_blocks(put: &mut impl FnMut(u16, Tile)) {
    // Dirt path: a packed, smoothed path top; its sides are dirt under a
    // lip of the path; the bottom is plain dirt.
    let path = [[118, 96, 58], [136, 112, 68], [150, 124, 76], [158, 132, 82], [166, 140, 88], [182, 156, 102]];
    let top = super::Recipe {
        seed: 480,
        palette: &[[0; 3]; 6],
        cells: 4,
        jitter: 40,
        thresholds: [110, 150],
        pits: (5, (1, 2), 1, None),
        bumps: (4, (1, 2), 5, Some(1)),
        broad: true,
    };
    let g = top.grid_in(&path);
    put(tex::VILLAGE + 1, paint(&g, &path));
    let mut side = super::DIRT.grid();
    let mut pal: Vec<Px> = super::DIRT.palette.to_vec();
    pal.extend(path);
    for x in 0..N {
        let lip = 2 + (hu(481, x / 3, 0, 96) % 2) as usize;
        for (y, row) in side.iter_mut().enumerate().take(lip) {
            row[x] = 6 + g[y][x].clamp(1, 4);
        }
        side[lip][x] = 0;
    }
    put(tex::VILLAGE, paint(&side, &pal));
    put(tex::VILLAGE + 2, paint(&super::DIRT.grid(), super::DIRT.palette));
    // Hay bale: straw strands bound by two red-brown bands; its ends show
    // the cut stalks.
    let hay = [[124, 98, 26], [176, 144, 40], [198, 166, 50], [214, 182, 62], [232, 204, 86]];
    let band = ramp([140, 66, 38], [700, 1000, 1200]);
    let mut side = straw(483, false);
    let mut pal = hay.to_vec();
    pal.extend(band);
    for (y, row) in side.iter_mut().enumerate() {
        let band = match y {
            3 | 11 => Some(7),
            4 | 12 => Some(6),
            5 | 13 => Some(5),
            _ => None,
        };
        if let Some(k) = band {
            *row = [k; N];
        }
    }
    put(tex::VILLAGE + 3, paint(&side, &pal));
    let mut end = [[0u8; N]; N];
    for (y, row) in end.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let h = hu(484, x, y, 97);
            *c = if (x + 2 * y + (h % 3) as usize).is_multiple_of(3) { 1 } else { 2 + (h >> 4) as u8 % 3 };
            if (y == 3 || y == 12) && x.is_multiple_of(2) {
                *c = 6;
            } else if y == 3 || y == 12 {
                *c = 5;
            }
        }
    }
    put(tex::VILLAGE + 4, paint(&end, &pal));
    // Bell: polished gold, lit down its left like a cylinder, with two lip
    // bands on the face models use for the rim.
    let gold = ramp([214, 170, 44], [560, 760, 900, 1000, 1120, 1240]);
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let k = match x {
                0 | 15 => 1,
                1..=3 => 4,
                4 => 5,
                5..=9 => 3,
                10..=12 => 2,
                _ => 1,
            };
            img.set(x, y, gold[k]);
        }
    }
    let mut face = img.clone();
    for y in [4, 12] {
        for x in 0..N as i32 {
            face.set(x, y - 1, gold[5]);
            face.set(x, y, gold[0]);
        }
    }
    put(tex::VILLAGE + 42, img.tile());
    let mut top = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let r = (x - 7).abs().max((y - 7).abs()).min((x - 8).abs().max((y - 8).abs()));
            let lit = x + y < 15;
            top.set(
                x,
                y,
                gold[if r == 7 {
                    if lit { 5 } else { 1 }
                } else if r % 3 == 0 {
                    2
                } else if lit {
                    4
                } else {
                    3
                }],
            );
        }
    }
    put(tex::VILLAGE + 43, top.tile());
    put(tex::VILLAGE + 44, face.tile());
}

fn anvils_and_tnt(put: &mut impl FnMut(u16, Tile)) {
    // Anvil body: dark hammered iron, a few lit dents.
    let iron = ramp([72, 72, 76], [600, 780, 900, 1000, 1100, 1300]);
    let mut g = smooth_grid(175, 40);
    for k in 0..7 {
        let h = hu(175, k, 0, 98);
        let (x, y) = ((k * 5 + h as usize % 3) % N, (h >> 4) as usize % N);
        g[y][x] = 5;
        g[(y + 1) % N][(x + 1) % N] = 1;
    }
    put(tex::ANVIL, paint(&g, &iron));
    // The striking face: worn bright along the middle, more cracks with
    // each stage of damage, and chipped corners.
    for wear in 0..3u16 {
        let mut g = smooth_grid(176, 32);
        for (y, row) in g.iter_mut().enumerate() {
            for (x, c) in row.iter_mut().enumerate() {
                let middle = (4..=11).contains(&x);
                *c = if middle { *c + 1 } else { *c };
                if x == 3 || x == 12 {
                    *c = 1;
                }
                let _ = y;
            }
        }
        if wear > 0 {
            cracks(&mut g, 177 + wear as u32, 2 * wear as usize, 0);
            for &(x, y) in &[(3usize, 0usize), (12, 15), (4, 9)][..wear as usize + 1] {
                g[y][x] = 0;
            }
        }
        put(tex::ANVIL_TOP + wear, paint(&g, &iron));
    }
    // TNT: red paper sticks, lit on their left, with a white band and a
    // stencilled name across the sides; the ends show the sticks' tops
    // and, on top, the fuse.
    let red = ramp([196, 48, 36], [620, 800, 1000, 1150]);
    let paper = ramp([226, 222, 212], [700, 880, 1000]);
    let ink = [44, 34, 32];
    const GLYPHS: [&str; 5] = ["### #  # ###", " #  ## #  # ", " #  # ##  # ", " #  #  #  # ", " #  #  #  # "];
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let c = if (4..=11).contains(&y) {
                let glyph =
                    GLYPHS.get((y - 5) as usize).and_then(|r| r.as_bytes().get((x - 2) as usize)) == Some(&b'#');
                if glyph {
                    ink
                } else if y == 4 || y == 11 {
                    paper[0]
                } else {
                    paper[if y == 5 { 2 } else { 1 }]
                }
            } else {
                match x % 4 {
                    0 => red[3],
                    3 => red[0],
                    _ => red[if hu(82, x as usize, y as usize, 99).is_multiple_of(4) { 1 } else { 2 }],
                }
            };
            img.set(x, y, c);
        }
    }
    put(tex::TNT_SIDE, img.tile());
    for layer in [tex::TNT_TOP, tex::TNT_BOTTOM] {
        let mut img = Img::new();
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                let (sx, sy) = (x % 4, y % 4);
                let c = match (sx, sy) {
                    (0, _) | (_, 0) => red[0],
                    (1, 1) => red[3],
                    (1, _) | (_, 1) => red[2],
                    (3, 3) => red[1],
                    _ => [140, 34, 26],
                };
                img.set(x, y, c);
            }
        }
        if layer == tex::TNT_TOP {
            img.stamp(
                6,
                5,
                &[" wl ", " lw ", "wldl", "dddd"],
                &[('w', [210, 210, 204]), ('l', [150, 150, 146]), ('d', [60, 58, 56])],
            );
        }
        put(layer, img.tile());
    }
}

/// Draws every tile of this round through `put`.
pub(super) fn build(put: &mut impl FnMut(u16, Tile)) {
    doors(put);
    let latch = [196, 196, 204];
    put(tex::CHEST_TOP, chest(0, None, 58));
    put(tex::CHEST_SIDE, chest(1, None, 59));
    put(tex::CHEST_FRONT, chest(2, Some(latch), 59));
    put(crate::world::utilities::t::TRAPPED_FRONT, chest(2, Some([176, 52, 40]), 59));
    village_blocks(put);
    anvils_and_tnt(put);
    small_flowers(put);
    tall_flowers(put);
    saplings(put);
    grasses(put);
    crops(put);
    other_plants(put);
    round_b::build(put);
    // After round B: the bamboo's leaf tiles are redrawn for the planes.
    round_c::build(put);
}

/// Track 10b's layers, in review order.
#[cfg(test)]
pub(super) fn round_b_layers() -> Vec<u16> {
    round_b::layers()
}

/// Gap Track 7's layers, in review order.
#[cfg(test)]
pub(super) fn round_c_layers() -> Vec<u16> {
    round_c::layers()
}

/// The layers this round draws, in review order (tests pin each one).
#[cfg(test)]
pub(super) fn layers() -> Vec<u16> {
    let mut out = Vec::new();
    for w in &DOOR_WOODS {
        out.extend([w.top, w.bottom, w.trapdoor]);
    }
    out.extend([tex::CHEST_TOP, tex::CHEST_SIDE, tex::CHEST_FRONT, crate::world::utilities::t::TRAPPED_FRONT]);
    out.extend([tex::VILLAGE, tex::VILLAGE + 1, tex::VILLAGE + 2, tex::VILLAGE + 3, tex::VILLAGE + 4]);
    out.extend([tex::VILLAGE + 42, tex::VILLAGE + 43, tex::VILLAGE + 44]);
    out.extend(tex::ANVIL..tex::ANVIL_TOP + 3);
    out.extend([tex::TNT_SIDE, tex::TNT_TOP, tex::TNT_BOTTOM]);
    out.extend(plant_layers());
    out
}

/// The plants' layers.
#[cfg(test)]
pub(super) fn plant_layers() -> Vec<u16> {
    let mut out = vec![tex::DANDELION, tex::POPPY, tex::BLUE_ORCHID];
    out.extend(1578..=1586);
    out.extend([706, 3442, 792, 3373, 3374, 2845]);
    out.extend(1587..=1594);
    out.extend([3454, 3455]);
    out.extend([tex::OAK_SAPLING, tex::SPRUCE_SAPLING, tex::BIRCH_SAPLING, tex::JUNGLE_SAPLING]);
    out.extend([tex::ACACIA_SAPLING, 1556, 1557, 1558, 1559, 1570, 759]);
    out.extend([tex::TALL_GRASS, tex::FERN, 1595, 1596, 1597, 1598, tex::DEAD_BUSH]);
    out.extend(2840..=2844);
    out.extend(tex::WHEAT_0..tex::WHEAT_0 + 8);
    out.extend(tex::CARROT_0..tex::CARROT_0 + 8);
    out.extend(tex::POTATO_0..tex::POTATO_0 + 8);
    out.extend(3004..=3007);
    out.extend(2992..=3003);
    out.extend([3440, 3441]);
    out.extend(3444..=3453);
    out.extend(tex::NETHER_WART_0..tex::NETHER_WART_0 + 3);
    out.extend(1601..=1604);
    out.extend([tex::BROWN_MUSHROOM, tex::RED_MUSHROOM, 1324, 1325]);
    out.extend([tex::SUGAR_CANE, 1326, 1327, 1328, 1329, 1330, 1331, 1332]);
    out.extend(1501..=1505);
    out.extend(1490..=1495);
    out.extend([1498, 1499, 1500, 1576, 1577, 791]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(layer: u16) -> Tile {
        std::array::from_fn(|i| super::super::pixel(layer, i % N, i / N).expect("drawn by this round"))
    }

    fn holes(t: &Tile) -> usize {
        t.iter().filter(|p| p[3] == 0).count()
    }

    /// Every tile replaces a layer that already existed: none of the
    /// reserved layers 4066-4079 (or any new layer) is used, and none is
    /// drawn twice.
    #[test]
    fn the_round_replaces_existing_layers_only() {
        let layers = layers();
        assert_eq!(layers.len(), 197);
        let unique: std::collections::BTreeSet<u16> = layers.iter().copied().collect();
        assert_eq!(unique.len(), layers.len(), "a layer is listed twice");
        assert!(layers.iter().all(|&l| (l as u32) < tex::COUNT), "a layer past the block array");
        // Only gap Track 7 (its potted bamboo leaf, 4066), gap track 6b
        // (the item models' plant layers, 4080-4083) and gap track 6d (the
        // in-hand models' layers, 4084-4085) and gap track 6f (the
        // hopper's top and inside, 4086-4087) and final F10 (the repeater's and
        // comparator's tops, 4092-4095) have grown the array since.
        assert_eq!(tex::COUNT, 4096, "the block array grew");
        for &l in &layers {
            tile(l);
        }
    }

    /// Door halves and trapdoors: solid stiles at both sides; the upper
    /// halves and the trapdoors have see-through openings, the lower halves
    /// none; every wood looks different.
    #[test]
    fn doors_and_trapdoors_keep_their_openings() {
        let mut seen = Vec::new();
        for w in &DOOR_WOODS {
            for (layer, open) in [(w.top, true), (w.bottom, false), (w.trapdoor, true)] {
                let t = tile(layer);
                for y in 0..N {
                    for x in [0, 1, N - 2, N - 1] {
                        assert_eq!(t[y * N + x][3], 255, "layer {layer}: stile texel {x},{y} is see-through");
                    }
                }
                assert_eq!(holes(&t) > 0, open, "layer {layer}: openings");
                assert!(t.iter().all(|p| p[3] == 0 || p[3] == 255), "layer {layer}: only cut-out alpha");
                assert!(!seen.contains(&t), "layer {layer} repeats another wood's face");
                seen.push(t);
            }
        }
    }

    /// Tall grass and the ferns stay green in every texel, so the biome
    /// tints (`textures::tint_foliage`) recolour all of them.
    #[test]
    fn grasses_and_ferns_keep_taking_the_biome_tints() {
        for layer in [tex::TALL_GRASS, tex::FERN] {
            let t = tile(layer);
            assert!(holes(&t) > 128, "layer {layer} is a sprite");
            for p in t.iter().filter(|p| p[3] > 0) {
                assert!(p[1] > p[0] && p[1] >= p[2], "layer {layer}: texel {p:?} would not take the tint");
            }
        }
    }

    /// Plants are cut-out sprites with at least three shades (lit, mid and
    /// shaded), except the pitcher crop's upper half, which stays empty
    /// until age 3 as its model expects.
    #[test]
    fn plants_are_shaded_cut_out_sprites() {
        use crate::world::sniffer_plants::t as sp;
        let empty: Vec<u16> = (0..3).map(|age| sp::PITCHER + 5 + age).collect();
        let mut bad = Vec::new();
        for layer in plant_layers() {
            let t = tile(layer);
            let opaque = N * N - holes(&t);
            if empty.contains(&layer) {
                if opaque != 0 {
                    bad.push(format!("{layer}: the young pitcher has no upper half"));
                }
                continue;
            }
            let colours: std::collections::BTreeSet<[u8; 4]> = t.iter().copied().filter(|p| p[3] > 0).collect();
            if opaque < 4 || holes(&t) < 16 || colours.len() < 3 {
                bad.push(format!("{layer}: {opaque} texels drawn, {} colours", colours.len()));
            }
        }
        assert!(bad.is_empty(), "plants that aren't shaded cut-out sprites: {bad:?}");
    }

    /// The trapped chest's front is the chest's front with a red latch.
    #[test]
    fn the_trapped_chest_differs_only_at_its_latch() {
        let (plain, trapped) = (tile(tex::CHEST_FRONT), tile(crate::world::utilities::t::TRAPPED_FRONT));
        for i in 0..N * N {
            let (x, y) = (i % N, i / N);
            if plain[i] != trapped[i] {
                assert!((6..10).contains(&x) && (3..9).contains(&y), "texel {x},{y} differs outside the latch");
            }
        }
        let red = (0..N * N).filter(|&i| trapped[i][0] > trapped[i][1] + 60).count();
        assert!(red >= 6, "the trapped latch is red");
    }
}
