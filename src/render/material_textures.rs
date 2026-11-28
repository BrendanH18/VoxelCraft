//! v0.18 Track 3: original pixel art for the common natural materials and
//! woods (see the material rules in `docs/art-and-ui.md`).
//!
//! Each tile is a grid of palette indices built from integer maths, so it is
//! the same on every platform: soft patches of three base shades, clusters
//! of 2-5 texels placed on a stratified grid (cluster `k` starts in its own
//! 4 x 4 cell) so they never clump, light from the top left, and no run of
//! one shade longer than four texels except deliberate grain and strata.
//! Every tile wraps in both directions. Tiles are built once, on first use,
//! and kept (about half a MiB; see the asset manifest's budget).
//!
//! Stone itself is the authored asset `assets/textures/stone.png`; the ores
//! keep its texels exactly and add mineral clusters.

use std::sync::LazyLock;

use rustc_hash::FxHashMap;

use super::Rgba;
use crate::world::block::tex;
use crate::world::noise::hash3;
use crate::world::overworld_blocks::t;
use crate::world::stone_blocks::t as st;
use crate::world::wood::tex as wt;

// v1.0 Track 10: doors, trapdoors, chests, the village and farm blocks and
// the plants, drawn with this module's helpers.
#[path = "final_art_textures.rs"]
mod final_art;

const N: usize = 16;
/// Palette indices; [`HOLE`] is a transparent texel.
type Grid = [[u8; N]; N];
const HOLE: u8 = u8::MAX;
type Palette = &'static [[u8; 3]];
type Tile = [Rgba; N * N];

/// 32 random bits for texel `(x, y)` of stream `v` in material `seed`.
fn hu(seed: u32, x: usize, y: usize, v: u32) -> u32 {
    (hash3(x as i32, y as i32, v as i32, 0x7E57_0000 ^ seed as u64) >> 32) as u32
}

/// Tileable value noise, 0..=255, on a `cells` x `cells` lattice over the
/// tile, sampled at texel centres.
fn field(seed: u32, cells: usize, x: usize, y: usize, v: u32) -> u32 {
    let c2 = 2 * (N / cells); // cell size in half texels
    let (x2, y2) = (2 * x + 1, 2 * y + 1);
    let (i, j, fx, fy) = (x2 / c2, y2 / c2, (x2 % c2) as u32, (y2 % c2) as u32);
    let at = |a: usize, b: usize| hu(seed, a % cells, b % cells, v) & 255;
    let c = c2 as u32;
    let sum = at(i, j) * (c - fx) * (c - fy)
        + at(i + 1, j) * fx * (c - fy)
        + at(i, j + 1) * (c - fx) * fy
        + at(i + 1, j + 1) * fx * fy;
    sum / (c * c)
}

const DIRS: [(usize, usize); 4] = [(1, 0), (0, 1), (N - 1, 0), (0, N - 1)];

/// The sixteen 4 x 4 cells of the tile in a shuffled order for stream `v`.
/// Clusters take them in turn: spread over the tile like a stratified grid,
/// but in no arithmetic order, so repeats of the tile don't line them up.
fn cell_order(seed: u32, v: u32) -> [usize; 16] {
    let mut cells: [usize; 16] = std::array::from_fn(|i| i);
    for i in (1..16).rev() {
        cells.swap(i, hu(seed, i, 0, v + 200) as usize % (i + 1));
    }
    cells
}

/// The texels of random-walk cluster `k`, in paint order (2 to `max` long).
fn walk(seed: u32, k: usize, min: usize, max: usize, v: u32) -> Vec<(usize, usize)> {
    let h = hu(seed, k, 0, v);
    let cell = cell_order(seed, v)[k % 16];
    let (mut x, mut y) = ((cell % 4) * 4 + h as usize % 4, (cell / 4) * 4 + (h >> 2) as usize % 4);
    let n = min + (h >> 16) as usize % (max - min + 1);
    let mut cells = vec![(x, y)];
    let bits = hu(seed, k, 1, v);
    let mut last: Option<usize> = None;
    for s in 0..n - 1 {
        let mut d = (bits >> (2 * s)) as usize & 3;
        // No immediate step back.
        if last.is_some_and(|l| d == (l + 2) % 4) {
            d = (d + 1) % 4;
        }
        last = Some(d);
        (x, y) = ((x + DIRS[d].0) % N, (y + DIRS[d].1) % N);
        if !cells.contains(&(x, y)) {
            cells.push((x, y));
        }
    }
    cells
}

/// Breaks every run of one shade in `lo..=hi` (or of holes) longer than
/// `limit` in a row or column, counting across the tile edge so repeats
/// stay broken.
fn break_runs(g: &mut Grid, lo: u8, hi: u8, limit: usize) {
    for _ in 0..3 {
        let mut changed = false;
        for horizontal in [true, false] {
            for a in 0..N {
                let (mut run, mut prev) = (0, None);
                for b in 0..N + limit {
                    let (x, y) = if horizontal { (b % N, a) } else { (a, b % N) };
                    let c = g[y][x];
                    run = if Some(c) == prev { run + 1 } else { 1 };
                    prev = Some(c);
                    if run > limit && ((lo..=hi).contains(&c) || c == HOLE) {
                        // A run of holes gets the darkest allowed shade.
                        let alt = if c == HOLE {
                            lo
                        } else if c < hi {
                            c + 1
                        } else {
                            c - 1
                        };
                        g[y][x] = alt;
                        (prev, run, changed) = (Some(alt), 1, true);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Evens a tile out for distant walls: nudges base shades (`lo..=hi`) one
/// step at a time until every 4 x 4 block's brightness (squared bytes,
/// close to luminance) is within 4 % of the tile's mean, so repeats of the
/// tile show no larger pattern.
fn balance(g: &mut Grid, palette: &[[u8; 3]], lo: u8, hi: u8, seed: u32) {
    // Squared bytes: closer to luminance than the bytes themselves.
    let bright =
        |c: u8| if c == HOLE { 0 } else { palette[c as usize].iter().map(|&v| (v as i64).pow(2)).sum::<i64>() };
    let mut stuck = [false; 16];
    for step in 0..512 {
        let mut sums = [0i64; 16];
        for (y, row) in g.iter().enumerate() {
            for (x, &c) in row.iter().enumerate() {
                sums[(y / 4) * 4 + x / 4] += bright(c);
            }
        }
        let mean = sums.iter().sum::<i64>() / 16;
        let Some((block, &sum)) =
            sums.iter().enumerate().filter(|&(b, _)| !stuck[b]).max_by_key(|&(_, s)| (s - mean).abs())
        else {
            break;
        };
        if (sum - mean).abs() * 1000 <= mean * 40 {
            break;
        }
        let down = sum > mean;
        let start = hu(seed, step, 0, 41) as usize % 16;
        // Visit the block's texels from `start` in steps of 5 (coprime to 16).
        let found = (0..16)
            .map(|i| (start + i * 5) % 16)
            .map(|i| ((block % 4) * 4 + i % 4, (block / 4) * 4 + i / 4))
            .find(|&(x, y)| {
                let c = g[y][x];
                (lo..=hi).contains(&c) && if down { c > lo } else { c < hi }
            });
        let Some((x, y)) = found else {
            stuck[block] = true;
            continue;
        };
        if down {
            g[y][x] -= 1;
        } else {
            g[y][x] += 1;
        }
    }
}

/// Soft patches of base shades 2..=4: value noise (with a broad 2 x 2
/// octave when `broad`) plus a texel jitter, quantised at `thresholds`.
fn base_grid(seed: u32, cells: usize, jitter: u32, thresholds: [u32; 2], broad: bool) -> Grid {
    let mut g = [[0; N]; N];
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let mut f = field(seed, cells, x, y, 1);
            if broad {
                f = (field(seed, 2, x, y, 3) + f) / 2;
            }
            f = f * (256 - jitter) / 256 + hu(seed, x, y, 2) % jitter;
            *c = 2 + thresholds.iter().filter(|&&t| f >= t).count() as u8;
        }
    }
    g
}

/// Raised clusters lit from the top left: `light` texels, and `shadow` on
/// the texel down and to the right of each that is outside the cluster.
fn bumps(g: &mut Grid, seed: u32, count: usize, size: (usize, usize), light: u8, shadow: Option<u8>, v: u32) {
    for k in 0..count {
        let cells = walk(seed, k, size.0, size.1, v);
        for &(x, y) in &cells {
            g[y][x] = light;
        }
        if let Some(shadow) = shadow {
            for &(x, y) in &cells {
                let (sx, sy) = ((x + 1) % N, (y + 1) % N);
                if !cells.contains(&(sx, sy)) {
                    g[sy][sx] = shadow;
                }
            }
        }
    }
}

/// Sunken clusters of `dark` texels; every other one has a `deep` centre.
fn pits(g: &mut Grid, seed: u32, count: usize, size: (usize, usize), dark: u8, deep: Option<u8>, v: u32) {
    for k in 0..count {
        let cells = walk(seed, k, size.0, size.1, v);
        for &(x, y) in &cells {
            g[y][x] = dark;
        }
        if let Some(deep) = deep.filter(|_| k.is_multiple_of(2)) {
            let (x, y) = cells[0];
            g[y][x] = deep;
        }
    }
}

/// A natural material: base patches, pits and bumps over a six-shade palette.
struct Recipe {
    seed: u32,
    palette: Palette,
    cells: usize,
    jitter: u32,
    thresholds: [u32; 2],
    /// Count, size range, shade and deep shade.
    pits: (usize, (usize, usize), u8, Option<u8>),
    /// Count, size range, light shade and shadow shade.
    bumps: (usize, (usize, usize), u8, Option<u8>),
    broad: bool,
}

impl Recipe {
    fn grid(&self) -> Grid {
        self.grid_in(self.palette)
    }

    /// The recipe over another palette of the same shape (terracotta's
    /// colours are computed, not constants).
    fn grid_in(&self, palette: &[[u8; 3]]) -> Grid {
        let mut g = base_grid(self.seed, self.cells, self.jitter, self.thresholds, self.broad);
        break_runs(&mut g, 2, 4, 4);
        let (n, size, dark, deep) = self.pits;
        pits(&mut g, self.seed, n, size, dark, deep, 9);
        let (n, size, light, shadow) = self.bumps;
        bumps(&mut g, self.seed, n, size, light, shadow, 7);
        // Accents included: clusters that meet mustn't line up into a streak.
        break_runs(&mut g, 1, 5, 4);
        balance(&mut g, palette, 2, 4, self.seed);
        break_runs(&mut g, 1, 5, 4);
        g
    }
}

const fn natural(seed: u32, palette: Palette) -> Recipe {
    Recipe {
        seed,
        palette,
        cells: 4,
        jitter: 56,
        thresholds: [116, 146],
        pits: (6, (2, 3), 1, None),
        bumps: (3, (2, 3), 5, Some(2)),
        broad: true,
    }
}

const ANDESITE: Recipe = Recipe {
    cells: 8,
    jitter: 72,
    thresholds: [110, 150],
    pits: (9, (2, 3), 1, None),
    bumps: (7, (1, 3), 5, None),
    ..natural(11, &[[98, 99, 98], [116, 117, 116], [128, 129, 128], [137, 138, 137], [146, 147, 146], [160, 161, 160]])
};
const DIORITE: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (9, (1, 3), 1, Some(0)),
    bumps: (3, (2, 3), 5, None),
    ..natural(12, &[[70, 70, 74], [150, 150, 154], [206, 206, 208], [220, 220, 222], [232, 232, 233], [244, 244, 245]])
};
const GRANITE: Recipe = Recipe {
    cells: 8,
    jitter: 80,
    thresholds: [105, 150],
    pits: (10, (1, 3), 1, Some(0)),
    bumps: (8, (1, 3), 5, None),
    ..natural(13, &[[98, 60, 50], [122, 80, 66], [142, 98, 82], [152, 106, 89], [164, 116, 98], [188, 140, 120]])
};
const TUFF: Recipe = Recipe {
    jitter: 48,
    pits: (10, (1, 2), 1, Some(0)),
    bumps: (2, (2, 3), 5, Some(1)),
    ..natural(14, &[[66, 68, 60], [84, 86, 78], [100, 101, 94], [108, 109, 102], [116, 117, 110], [130, 132, 124]])
};
const DIRT: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (4, (1, 2), 5, Some(1)),
    ..natural(21, &[[92, 64, 44], [110, 78, 54], [124, 89, 62], [132, 94, 66], [140, 100, 70], [156, 116, 84]])
};
const SAND: Recipe = Recipe {
    jitter: 80,
    thresholds: [110, 150],
    pits: (6, (1, 2), 1, None),
    bumps: (7, (1, 2), 5, None),
    ..natural(
        31,
        &[[192, 178, 132], [204, 190, 143], [214, 201, 154], [220, 208, 162], [226, 215, 170], [236, 227, 186]],
    )
};
const RED_SAND: Recipe = Recipe {
    seed: 32,
    palette: &[[160, 80, 26], [174, 90, 30], [186, 99, 34], [194, 105, 37], [202, 112, 42], [214, 128, 56]],
    ..SAND
};
/// Grass at the plains colour (foliage group 0); `tint_foliage` recolours
/// the other groups. HSV saturation stays at or below 0.55.
const GRASS: Recipe = Recipe {
    jitter: 72,
    thresholds: [110, 150],
    pits: (9, (2, 3), 1, None),
    bumps: (6, (1, 3), 5, None),
    ..natural(41, &[[70, 104, 44], [84, 120, 52], [96, 134, 60], [106, 145, 66], [116, 156, 72], [132, 170, 84]])
};
const SNOW: Palette =
    &[[178, 196, 208], [200, 214, 224], [222, 232, 238], [232, 240, 245], [240, 246, 250], [250, 252, 254]];

const DEEPSLATE: Palette = &[[40, 40, 46], [54, 54, 60], [64, 64, 70], [72, 72, 79], [81, 81, 88], [96, 96, 104]];

/// Deepslate's horizontal strata: a value per row, long dark seams spread
/// one to each sixth of the tile, a few light chips.
fn deepslate_grid() -> Grid {
    const SEED: u32 = 51;
    let mut g = [[0; N]; N];
    for (y, row) in g.iter_mut().enumerate() {
        let band = hu(SEED, 0, y, 5) & 127;
        for (x, c) in row.iter_mut().enumerate() {
            let f = (band + field(SEED, 4, x, y, 1) / 2 + hu(SEED, x, y, 2) % 48) as i32 - 24;
            *c = 2 + [88, 128].iter().filter(|&&t| f >= t).count() as u8;
        }
    }
    break_runs(&mut g, 2, 4, 6);
    for k in 0..6 {
        let h = hu(SEED, k, 0, 9);
        let (mut x, mut y) = ((h >> 8) as usize % N, (k * N / 6 + 1 + h as usize % 2) % N);
        let n = 3 + (h >> 16) as usize % 4;
        for s in 0..n {
            g[y][x] = if s > 0 { 1 } else { 0 };
            // Along the strata, now and then down a row.
            if hu(SEED, k, s, 10) & 7 == 0 {
                y = (y + 1) % N;
            }
            x = (x + 1) % N;
        }
    }
    for k in 0..3 {
        for (x, y) in walk(SEED, k, 1, 2, 11) {
            g[y][x] = 5;
        }
    }
    g
}

/// Mineral clusters, as offsets from the cluster's corner.
const NUGGETS: [&[(usize, usize)]; 8] = [
    &[(0, 0), (1, 0), (0, 1), (1, 1), (2, 1)],
    &[(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)],
    &[(0, 0), (1, 0), (2, 0), (1, 1)],
    &[(0, 0), (1, 0), (1, 1), (2, 1)],
    &[(1, 0), (2, 0), (0, 1), (1, 1)],
    &[(0, 0), (0, 1), (1, 1), (1, 2)],
    &[(0, 0), (1, 0), (0, 1)],
    &[(0, 0), (1, 1), (1, 0), (2, 2), (2, 1)],
];
/// Where cluster `k` may sit: a 5 x 5 area from each corner.
const SPOTS: [(usize, usize); 8] = [(0, 0), (8, 0), (0, 8), (8, 8), (4, 4), (12, 12), (4, 12), (12, 4)];

/// Each ore's shade, body, highlight (and a fleck), and its cluster count.
const ORES: [(&[[u8; 3]], usize); 8] = [
    (&[[22, 22, 24], [42, 42, 44], [96, 96, 100]], 5),
    (&[[128, 92, 70], [210, 170, 138], [238, 212, 186]], 4),
    (&[[156, 104, 18], [236, 196, 48], [255, 244, 150]], 4),
    (&[[24, 124, 122], [86, 214, 206], [206, 252, 246]], 4),
    (&[[14, 32, 100], [38, 80, 186], [110, 150, 238]], 4),
    (&[[112, 60, 36], [192, 108, 66], [232, 156, 104], [88, 160, 120]], 4),
    (&[[100, 6, 6], [196, 22, 20], [255, 92, 76]], 4),
    (&[[8, 92, 36], [28, 178, 78], [140, 244, 166]], 3),
];

/// Ore `i` (in [`ORES`] order) over `background`, which it keeps exactly
/// outside its clusters: each cluster has a lit top-left texel and a shaded
/// rim to the bottom right.
fn ore(background: &Tile, seed: u32, i: usize) -> Tile {
    let (pal, count) = ORES[i];
    ore_on(background, seed, pal, count)
}

/// `count` mineral clusters of `pal` (shade, body, highlight, and an
/// optional fleck) over `background`.
fn ore_on(background: &Tile, seed: u32, pal: &[[u8; 3]], count: usize) -> Tile {
    let mut out = *background;
    let rgba = |c: [u8; 3]| [c[0], c[1], c[2], 255];
    for k in 0..count {
        let h = hu(seed, k, 0, 13);
        let (ox, oy) = SPOTS[(k + seed as usize) % SPOTS.len()];
        let (x0, y0) = (ox + h as usize % 5, oy + (h >> 4) as usize % 5);
        let shape = NUGGETS[(h >> 8) as usize % NUGGETS.len()];
        let cells: Vec<_> = shape.iter().map(|&(a, b)| ((x0 + a) % N, (y0 + b) % N)).collect();
        for &(x, y) in &cells {
            let (sx, sy) = ((x + 1) % N, (y + 1) % N);
            if !cells.contains(&(sx, sy)) {
                out[sy * N + sx] = rgba(pal[0]);
            }
        }
        for &(x, y) in &cells {
            out[y * N + x] = rgba(pal[1]);
        }
        let (x, y) = cells[0];
        out[y * N + x] = rgba(pal[2]);
        if pal.len() > 3 && cells.len() > 3 {
            let (x, y) = cells[cells.len() - 1];
            out[y * N + x] = rgba(pal[3]);
        }
    }
    out
}

/// Rounded stones on a one-texel joint, lit from the top left: the joint is
/// on each stone's bottom and right edge, the top and left edges are light.
fn stones(seed: u32, n: usize, jitter: u32, joint: u8) -> (Grid, [[usize; N]; N]) {
    let pts: Vec<(usize, usize)> = (0..n * n)
        .map(|c| {
            let (i, j) = (c / n, c % n);
            let h = hu(seed, i, j, 15);
            ((i * N / n + (h % jitter) as usize) % N, (j * N / n + ((h >> 4) % jitter) as usize) % N)
        })
        .collect();
    let mut ids = [[0usize; N]; N];
    for (y, row) in ids.iter_mut().enumerate() {
        for (x, id) in row.iter_mut().enumerate() {
            let d = |&(px, py): &(usize, usize)| {
                let (dx, dy) = (x.abs_diff(px), y.abs_diff(py));
                let (dx, dy) = (dx.min(N - dx), dy.min(N - dy));
                dx * dx + dy * dy
            };
            // The first nearest point, as `min_by_key` keeps the first.
            *id = (0..pts.len()).min_by_key(|&i| d(&pts[i])).unwrap();
        }
    }
    let mut g = [[0; N]; N];
    for y in 0..N {
        for x in 0..N {
            let i = ids[y][x];
            let (right, down) = (ids[y][(x + 1) % N], ids[(y + 1) % N][x]);
            let (left, up) = (ids[y][(x + N - 1) % N], ids[(y + N - 1) % N][x]);
            let tone = 2 + (hu(seed, i, 0, 16) % 3) as u8;
            let mut c = if right != i || down != i {
                joint
            } else if left != i || up != i {
                (tone + 1).min(5)
            } else if !hu(seed, x, y, 17).is_multiple_of(5) {
                tone
            } else {
                tone - 1
            };
            // Shade just inside the bottom right.
            if c != joint && c >= 2 && (ids[(y + 2) % N][x] != i || ids[y][(x + 2) % N] != i) {
                c = (c - 1).max(1);
            }
            g[y][x] = c;
        }
    }
    (g, ids)
}

const COBBLE: Palette =
    &[[70, 70, 73], [92, 92, 95], [110, 110, 113], [122, 122, 125], [134, 134, 137], [150, 150, 152]];
const GRAVEL: Palette = &[
    [100, 95, 92],
    [112, 107, 103],
    [124, 118, 114],
    [134, 128, 123],
    [144, 137, 131],
    [160, 153, 146],
    [124, 108, 96],
    [142, 124, 108],
    [110, 96, 86],
];

/// Gravel: twenty-five small pebbles with dark gaps, lit at their top left;
/// one in five is warm brown.
fn gravel_grid() -> Grid {
    let (mut g, ids) = stones(71, 5, 6, 0);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if hu(71, ids[y][x], 1, 18).is_multiple_of(5) && *c >= 1 {
                *c = match *c {
                    1 | 2 => 8,
                    3 | 4 => 6,
                    _ => 7,
                };
            }
        }
    }
    g
}

/// A grass (or snow) lip over the dirt tile: drips two texels wide of
/// uneven length, darker at their tips, with a shade on the dirt below.
/// Indices 0..6 are dirt's, 6.. the lip's.
fn side_grid(lip: &Grid, seed: u32) -> Grid {
    side_over(DIRT.grid(), lip, seed)
}

/// A lip over any six-shade `base` (nylium over netherrack, for one).
fn side_over(mut g: Grid, lip: &Grid, seed: u32) -> Grid {
    for x in 0..N {
        let drip = 2 + (hu(seed, x / 2, 0, 19) % 3) as usize + usize::from(hu(seed, x, 0, 20).is_multiple_of(3));
        for y in 0..drip {
            g[y][x] = 6 + lip[y][x];
        }
        g[drip - 1][x] = 6 + if lip[drip - 1][x] > 2 { 1 } else { 2 };
        g[drip][x] = u8::from(!hu(seed, x, 1, 21).is_multiple_of(2));
    }
    g
}

/// Leaves: soft patches, small lit leaf clusters, and cut-out holes (8-15 %
/// of the texels) with shade on the leaf below each.
fn leaves_grid(seed: u32) -> Grid {
    let mut g = base_grid(seed, 4, 80, [100, 150], false);
    break_runs(&mut g, 2, 4, 4);
    bumps(&mut g, seed, 9, (2, 3), 5, Some(1), 22);
    for k in 0..12 {
        let cells = walk(seed, k, 1, 4, 23);
        for &(x, y) in &cells {
            g[y][x] = HOLE;
        }
        for &(x, y) in &cells {
            let below = (y + 1) % N;
            if g[below][x] != HOLE {
                g[below][x] = 0;
            }
        }
    }
    break_runs(&mut g, 0, 5, 4);
    g
}

const OAK_LEAVES: Palette = &[[52, 84, 36], [66, 104, 46], [78, 120, 54], [88, 132, 61], [98, 144, 68], [116, 160, 82]];
const SPRUCE_LEAVES: Palette = &[[34, 58, 42], [44, 74, 52], [54, 88, 60], [61, 97, 66], [68, 106, 72], [84, 122, 86]];
const BIRCH_LEAVES: Palette =
    &[[58, 84, 36], [72, 102, 44], [86, 118, 52], [94, 128, 58], [104, 138, 64], [120, 154, 76]];
const JUNGLE_LEAVES: Palette =
    &[[36, 76, 22], [46, 94, 28], [56, 110, 34], [64, 120, 38], [72, 132, 44], [88, 148, 56]];
const ACACIA_LEAVES: Palette =
    &[[58, 74, 26], [72, 90, 32], [84, 104, 38], [92, 114, 42], [101, 124, 47], [116, 140, 58]];
const DARK_OAK_LEAVES: Palette =
    &[[28, 58, 18], [38, 74, 24], [46, 88, 30], [52, 97, 34], [60, 107, 39], [72, 122, 48]];

/// `c` scaled by each factor in per mille.
fn ramp<const K: usize>(c: [u8; 3], factors: [u32; K]) -> [[u8; 3]; K] {
    factors.map(|f| c.map(|v| (v as u32 * f / 1000).min(255) as u8))
}

/// Bark: ridges two texels wide, dark furrows running down (now and then
/// stepping aside), each with a lit texel on its left.
fn bark_grid(seed: u32) -> Grid {
    let mut g = [[0; N]; N];
    for x in 0..N {
        let ridge = hu(seed, x / 2, 0, 24) % 3;
        for (y, row) in g.iter_mut().enumerate() {
            let f = ridge * 40 + field(seed, 4, x, y, 1) / 3 + hu(seed, x, y, 2) % 40;
            row[x] = 2 + [70, 120].iter().filter(|&&t| f >= t).count() as u8;
        }
    }
    for k in 0..8 {
        let h = hu(seed, k, 0, 25);
        let (mut x, mut y) = ((k * 2 + h as usize % 2) % N, (h >> 8) as usize % N);
        let n = 4 + (h >> 16) as usize % 5;
        for s in 0..n {
            g[y][x] = if s % 3 == 1 { 0 } else { 1 };
            let left = (x + N - 1) % N;
            if g[y][left] >= 2 {
                g[y][left] = 4;
            }
            y = (y + 1) % N;
            if hu(seed, k, s, 26) & 7 == 0 {
                x = if hu(seed, k, s, 27) & 1 == 1 { (x + 1) % N } else { (x + N - 1) % N };
            }
        }
    }
    break_runs(&mut g, 2, 4, 5);
    g
}

/// Birch bark: pale with short dark marks across it, one texel tall.
fn birch_grid() -> Grid {
    const SEED: u32 = 81;
    let mut g = base_grid(SEED, 4, 48, [110, 150], false);
    break_runs(&mut g, 2, 4, 4);
    for k in 0..10 {
        let h = hu(SEED, k, 0, 28);
        let cell = cell_order(SEED, 28)[k];
        let (x, y) = ((cell % 4) * 4 + h as usize % 4, (cell / 4) * 4 + (h >> 2) as usize % 4);
        let n = 2 + (h >> 16) as usize % 3;
        for s in 0..n {
            let inner = 0 < s && s < n - 1;
            g[y][(x + s) % N] = if inner { 0 } else { 1 };
            if inner && hu(SEED, k, s, 29).is_multiple_of(3) {
                g[(y + 1) % N][(x + s) % N] = 1;
            }
        }
    }
    g
}
const BIRCH_BARK: Palette =
    &[[36, 34, 30], [70, 66, 60], [196, 194, 184], [212, 210, 200], [224, 222, 214], [236, 235, 228]];

/// A log's end: growth rings around a dark pith, between a square and a
/// circle, inside a bark rim one texel thick. Indices: bark 0-1, dark
/// outer ring 2, rings 3-4.
fn log_top_grid(seed: u32) -> Grid {
    let mut g = [[0; N]; N];
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let (dx, dy) = ((2 * x).abs_diff(15), (2 * y).abs_diff(15)); // half texels from the centre
            let d = (dx.max(dy) * 3 + (dx + dy) * 2) / 5;
            let edge = x == 0 || y == 0 || x == N - 1 || y == N - 1;
            *c = if edge {
                if hu(seed, x, y, 30).is_multiple_of(3) { 0 } else { 1 }
            } else if d <= 2 || d >= 26 && (x == 1 || y == 1 || x == N - 2 || y == N - 2) {
                2
            } else {
                let ring = (d + usize::from(hu(seed, x, y, 31).is_multiple_of(3))) / 4 % 2;
                3 + ring as u8
            };
        }
    }
    g
}

/// Planks: four boards four texels tall, lit along the top and shaded
/// along the bottom, one butt joint each (lit to its right), short grain.
/// Indices: joint and seam 0, grain 1, body 2-3, highlight 4.
fn planks_grid(seed: u32) -> Grid {
    let mut g = [[3; N]; N];
    for board in 0..4 {
        let joint = (hu(seed, board, 0, 32) as usize % 6 + board * 5 + 2) % N;
        for r in 0..4 {
            let y = board * 4 + r;
            for (x, c) in g[y].iter_mut().enumerate() {
                *c = if r == 3 || x == joint {
                    0
                } else if x == (joint + 1) % N || r == 0 {
                    4
                } else if !hu(seed, x / 3, y, 33).is_multiple_of(3) {
                    3
                } else {
                    2
                };
            }
        }
        for k in 0..3 {
            let h = hu(seed, board, k, 34);
            let (x, r, n) = (h as usize % N, 1 + (h >> 8) as usize % 2, 2 + (h >> 12) as usize % 3);
            for s in 0..n {
                let c = &mut g[board * 4 + r][(x + s) % N];
                if matches!(*c, 2 | 3) {
                    *c = 1;
                }
            }
        }
    }
    g
}

// v0.18 Track 3, second round: the rest of the ground, stone and wood families.

const SANDSTONE_TOP: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (4, (1, 2), 1, None),
    bumps: (5, (1, 2), 5, None),
    ..natural(
        51,
        &[[190, 176, 128], [204, 190, 141], [214, 201, 152], [220, 208, 160], [226, 214, 168], [236, 226, 184]],
    )
};
const RED_SANDSTONE_TOP: Recipe = Recipe {
    seed: 52,
    palette: &[[160, 80, 24], [174, 90, 28], [184, 98, 32], [192, 104, 34], [200, 110, 38], [212, 124, 48]],
    ..SANDSTONE_TOP
};
/// Red sandstone's bottom: the top's stone, rougher.
const RED_SANDSTONE_BOTTOM: Recipe =
    Recipe { seed: 53, pits: (8, (1, 3), 1, Some(0)), bumps: (4, (1, 2), 5, Some(1)), ..RED_SANDSTONE_TOP };
const SANDSTONE_SIDE: Palette =
    &[[172, 156, 108], [188, 172, 122], [204, 189, 139], [212, 198, 148], [220, 207, 157], [232, 220, 172]];
const RED_SANDSTONE_SIDE: Palette =
    &[[130, 64, 16], [150, 76, 20], [170, 88, 26], [180, 96, 29], [190, 103, 33], [206, 118, 42]];
/// Each row's shade in a sandstone side, top to bottom: a light crust, then
/// layers split by two dark seams (1). Deliberate strata run across.
const SANDSTONE_ROWS: [u8; N] = [4, 4, 3, 1, 3, 3, 2, 3, 3, 4, 3, 3, 1, 2, 3, 3];

/// A sandstone side: horizontal layers whose seams break now and then, with
/// grains scattered over them.
fn strata_grid(seed: u32) -> Grid {
    let mut g = [[0; N]; N];
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let (base, j) = (SANDSTONE_ROWS[y], hu(seed, x, y, 2) % 8);
            *c = match (base, j) {
                (1, 0) => 2,
                (1, _) => 1,
                (b, 0) if b > 2 => b - 1,
                (b, 1) if b < 4 => b + 1,
                (b, _) => b,
            };
        }
    }
    bumps(&mut g, seed, 6, (1, 2), 5, None, 7);
    pits(&mut g, seed, 3, (1, 2), 0, None, 9);
    g
}

/// Cut sandstone: one smooth slab split by a groove, lit at the top left of
/// each half and shaded at the bottom right.
fn cut_grid(seed: u32) -> Grid {
    let mut g = base_grid(seed, 4, 24, [100, 156], false);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if y == N - 1 || y == 7 || x == N - 1 {
                *c = 1;
            } else if y == 0 || y == 8 || x == 0 {
                *c = 5;
            }
        }
    }
    g
}

const COARSE_DIRT: Palette = &[
    [82, 57, 39],
    [98, 69, 47],
    [110, 78, 54],
    [118, 84, 58],
    [126, 90, 62],
    [140, 102, 72],
    [112, 106, 98],
    [146, 140, 130],
];
const PODZOL: Recipe = Recipe {
    jitter: 72,
    thresholds: [110, 150],
    pits: (6, (1, 2), 1, None),
    bumps: (8, (2, 3), 5, None),
    ..natural(62, &[[70, 46, 22], [88, 60, 28], [104, 72, 34], [114, 80, 38], [124, 88, 42], [146, 106, 52]])
};
const MYCELIUM: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (8, (1, 2), 5, None),
    ..natural(63, &[[84, 72, 84], [100, 86, 100], [112, 97, 110], [120, 104, 118], [128, 111, 126], [150, 134, 150]])
};
const ROOTED_DIRT: Palette =
    &[[98, 70, 48], [118, 84, 58], [134, 96, 68], [142, 102, 72], [150, 108, 76], [168, 124, 88], [184, 146, 104]];
const CLAY: Recipe = Recipe {
    jitter: 40,
    thresholds: [110, 150],
    pits: (4, (1, 2), 1, None),
    bumps: (4, (1, 2), 5, None),
    ..natural(
        65,
        &[[132, 138, 152], [146, 152, 166], [156, 162, 176], [162, 168, 182], [168, 174, 188], [180, 186, 198]],
    )
};
/// Bedrock is meant to look broken and hard: the strongest contrast here.
const BEDROCK: Recipe = Recipe {
    jitter: 96,
    thresholds: [100, 156],
    pits: (12, (2, 4), 1, Some(0)),
    bumps: (9, (1, 3), 5, Some(1)),
    ..natural(66, &[[24, 24, 26], [48, 48, 50], [76, 76, 78], [92, 92, 94], [110, 110, 112], [140, 140, 142]])
};

/// Coarse dirt: darker dirt with grey pebbles, lit at their corner.
fn coarse_dirt_grid() -> Grid {
    const SEED: u32 = 61;
    let mut g = Recipe { seed: SEED, palette: COARSE_DIRT, ..DIRT }.grid();
    for k in 0..7 {
        let cells = walk(SEED, k, 2, 3, 30);
        for &(x, y) in &cells {
            let (sx, sy) = ((x + 1) % N, (y + 1) % N);
            if !cells.contains(&(sx, sy)) {
                g[sy][sx] = 0;
            }
        }
        for &(x, y) in &cells {
            g[y][x] = 6;
        }
        let (x, y) = cells[0];
        g[y][x] = 7;
    }
    balance(&mut g, COARSE_DIRT, 2, 4, SEED);
    g
}

/// Rooted dirt: light dirt with pale roots wandering down, shaded on their
/// right.
fn rooted_dirt_grid() -> Grid {
    const SEED: u32 = 64;
    let mut g = Recipe { seed: SEED, palette: ROOTED_DIRT, ..DIRT }.grid();
    for k in 0..5 {
        let h = hu(SEED, k, 0, 31);
        let (mut x, mut y) = ((cell_order(SEED, 31)[k] % 4) * 4 + h as usize % 4, (h >> 8) as usize % N);
        for s in 0..4 + (h >> 16) as usize % 4 {
            g[y][x] = 6;
            let right = (x + 1) % N;
            if g[y][right] != 6 {
                g[y][right] = 1;
            }
            y = (y + 1) % N;
            if hu(SEED, k, s, 32).is_multiple_of(3) {
                x = if hu(SEED, k, s, 33) & 1 == 1 { (x + 1) % N } else { (x + N - 1) % N };
            }
        }
    }
    g
}

/// Polished stone: a smooth face with a one-texel bevel, lit along the top
/// and left and shaded along the bottom and right, in tiles `tile` wide.
fn polished_grid(seed: u32, tile: usize) -> Grid {
    let mut g = base_grid(seed, 4, 24, [96, 160], false);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if x % tile == tile - 1 || y % tile == tile - 1 {
                *c = 1;
            } else if x % tile == 0 || y % tile == 0 {
                *c = 5;
            }
        }
    }
    g
}

/// Polished stone shades: bevel shade, body and bevel light around `c`.
fn polished_palette(c: [u8; 3]) -> [[u8; 3]; 6] {
    ramp(c, [700, 840, 960, 1000, 1040, 1130])
}

const COBBLED_DEEPSLATE: Palette =
    &[[30, 30, 34], [46, 46, 51], [58, 58, 64], [66, 66, 72], [76, 76, 82], [92, 92, 100]];
const PALE_OAK_LEAVES: Palette =
    &[[96, 104, 92], [116, 124, 112], [132, 140, 128], [140, 148, 136], [150, 158, 146], [170, 176, 166]];
const MANGROVE_LEAVES: Palette =
    &[[44, 84, 26], [56, 102, 32], [68, 118, 40], [76, 128, 44], [86, 140, 50], [102, 156, 62]];
const CHERRY_LEAVES: Palette =
    &[[170, 92, 130], [196, 118, 156], [214, 146, 180], [224, 160, 192], [234, 176, 206], [246, 204, 224]];

/// Stripped wood: smooth bark-less wood with long grain lines running down
/// (deliberate grain: they may run longer than four texels).
fn stripped_grid(seed: u32) -> Grid {
    let mut g = [[0; N]; N];
    for x in 0..N {
        let streak = hu(seed, x, 0, 24) % 3;
        for (y, row) in g.iter_mut().enumerate() {
            let f = streak * 48 + field(seed, 4, x, y, 1) / 4 + hu(seed, x, y, 2) % 32;
            row[x] = 2 + [70, 120].iter().filter(|&&t| f >= t).count() as u8;
        }
    }
    for k in 0..5 {
        let h = hu(seed, k, 0, 25);
        let (mut x, mut y) = ((cell_order(seed, 25)[k] % 4) * 4 + h as usize % 4, (h >> 8) as usize % N);
        for s in 0..4 + (h >> 16) as usize % 5 {
            g[y][x] = 1;
            y = (y + 1) % N;
            if hu(seed, k, s, 26) & 7 == 0 {
                x = (x + 1) % N;
            }
        }
    }
    break_runs(&mut g, 2, 4, 6);
    g
}

/// Stripped log colours of `wood::Kind` 0..9 (oak to pale oak; bamboo keeps
/// its own drawing in `wood_textures`).
const STRIPPED: [[u8; 3]; 9] = [
    [177, 144, 86],
    [115, 89, 52],
    [196, 176, 118],
    [171, 132, 84],
    [174, 92, 59],
    [72, 56, 36],
    [120, 54, 48],
    [215, 145, 148],
    [240, 232, 228],
];

// v0.18 Track 3, third round: masonry, carved faces, terracotta and the rest.

/// Masonry: running-bond blocks `w` x `h` whose bottom row and right column
/// are the one-texel joint, each course shifted `shift` texels; each block
/// lit along its top and left, shaded just inside its bottom and right when
/// it is tall enough, with its own tone over a soft surface.
/// Indices: joint 0, shade 1, faces 2-4, light 5.
fn masonry_grid(seed: u32, w: usize, h: usize, shift: usize) -> Grid {
    let mut g = base_grid(seed, 4, 48, [110, 150], false);
    break_runs(&mut g, 2, 4, 4);
    for (y, row) in g.iter_mut().enumerate() {
        let course = y / h;
        let off = course * shift % w;
        for (x, c) in row.iter_mut().enumerate() {
            let sx = (x + N - off) % N;
            let (lx, ly) = (sx % w, y % h);
            let tone = (hu(seed, sx / w, course, 50) % 3) as u8;
            *c = if ly == h - 1 || lx == w - 1 {
                0
            } else if ly == 0 || lx == 0 {
                5
            } else if h >= 6 && (ly == h - 2 || lx == w - 2) {
                1
            } else {
                (*c + tone).saturating_sub(1).clamp(2, 4)
            };
        }
    }
    g
}

/// A smooth face with a one-texel bevel: light along the top and left,
/// joint along the bottom and right.
fn bevel(g: &mut Grid) {
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if x == N - 1 || y == N - 1 {
                *c = 0;
            } else if x == 0 || y == 0 {
                *c = 5;
            }
        }
    }
}

/// Cuts a groove where `mask` holds: `depth` texels, and the groove's lower
/// and right walls (the texel right of or below it) catch the light.
fn carve(g: &mut Grid, depth: u8, mask: impl Fn(usize, usize) -> bool) {
    let cut = |x: usize, y: usize| mask(x % N, y % N);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if cut(x, y) {
                *c = depth;
            } else if (cut(x + N - 1, y) || cut(x, y + N - 1)) && (2..=4).contains(c) {
                *c = 5;
            }
        }
    }
}

/// Distance from the tile's edge, 0 to 7 (a square ring index).
fn ring(x: usize, y: usize) -> usize {
    let dx = if x < 8 { x } else { 15 - x };
    let dy = if y < 8 { y } else { 15 - y };
    dx.min(dy)
}

/// Two cracks running down through the blocks, stepping aside now and then.
fn crack(g: &mut Grid, seed: u32) {
    let mut path = Vec::new();
    for k in 0..2 {
        let h = hu(seed, k, 0, 51);
        let (mut x, mut y) = ((cell_order(seed, 51)[k] % 4) * 4 + h as usize % 4, (h >> 8) as usize % N);
        for s in 0..6 + (h >> 16) as usize % 5 {
            path.push((x, y));
            y = (y + 1) % N;
            match hu(seed, k, s, 52) % 4 {
                0 => x = (x + 1) % N,
                1 => x = (x + N - 1) % N,
                _ => {}
            }
        }
    }
    carve(g, 0, |x, y| path.contains(&(x, y)));
}

/// Moss over masonry: clusters (index 6, lit corner 7), more of them low.
fn moss(g: &mut Grid, seed: u32) {
    for k in 0..10 {
        let mut cells = walk(seed, k, 2, 5, 53);
        // Two clusters in three sit in the lower half, where moss gathers.
        if k % 3 != 0 {
            cells.iter_mut().for_each(|(_, y)| *y = 8 + *y % 8);
        }
        for &(x, y) in &cells {
            g[y][x] = 6;
        }
        let (x, y) = cells[0];
        g[y][x] = 7;
    }
}

const STONE_BRICKS: Palette = &[
    [78, 78, 81],
    [100, 100, 103],
    [114, 114, 117],
    [122, 122, 125],
    [130, 130, 133],
    [146, 146, 149],
    [70, 96, 44],
    [92, 120, 56],
];
const BRICKS: Palette = &[[176, 168, 158], [118, 56, 44], [136, 66, 52], [146, 72, 56], [156, 78, 61], [172, 92, 72]];
const NETHER_BRICKS: Palette = &[[28, 14, 17], [50, 24, 29], [60, 30, 35], [66, 33, 39], [72, 36, 42], [86, 44, 50]];
const RED_NETHER_BRICKS: Palette =
    &[[40, 6, 8], [84, 10, 14], [100, 12, 16], [108, 14, 18], [116, 16, 20], [132, 22, 26]];
const END_STONE_BRICKS: Palette =
    &[[176, 178, 124], [196, 198, 138], [212, 214, 152], [220, 222, 158], [228, 230, 164], [238, 240, 180]];
const PRISMARINE_BRICKS: Palette =
    &[[60, 112, 102], [80, 140, 128], [92, 160, 148], [99, 171, 158], [106, 180, 166], [122, 194, 180]];
const DEEPSLATE_BRICKS: Palette = &[[30, 30, 34], [46, 46, 52], [60, 60, 66], [66, 66, 72], [72, 72, 78], [86, 86, 94]];
const TUFF_BRICKS: Palette =
    &[[64, 66, 58], [84, 86, 78], [100, 101, 94], [108, 109, 102], [116, 117, 110], [130, 132, 124]];
const BLACKSTONE_BRICKS: Palette =
    &[[20, 18, 24], [34, 31, 38], [42, 38, 46], [48, 43, 52], [52, 47, 56], [66, 60, 70]];
const MUD_BRICKS: Palette =
    &[[112, 84, 62], [124, 92, 70], [132, 99, 75], [137, 103, 79], [142, 107, 82], [156, 120, 92]];
const BLACKSTONE: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (8, (1, 3), 1, Some(0)),
    bumps: (5, (1, 2), 5, None),
    ..natural(101, &[[24, 22, 28], [34, 31, 38], [42, 38, 46], [48, 43, 52], [54, 49, 58], [68, 62, 72]])
};
const MUD: Recipe = Recipe {
    jitter: 48,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (6, (1, 2), 5, None),
    ..natural(102, &[[40, 38, 42], [50, 48, 52], [56, 53, 57], [60, 57, 61], [64, 61, 65], [78, 74, 78]])
};
const PACKED_MUD: Recipe = Recipe {
    jitter: 48,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (3, (1, 2), 5, None),
    ..natural(103, &[[110, 82, 60], [124, 93, 69], [136, 102, 76], [142, 107, 80], [148, 112, 84], [162, 124, 94]])
};
const STRAW: [[u8; 3]; 2] = [[168, 136, 86], [190, 158, 102]];
const CALCITE: Recipe = Recipe {
    jitter: 48,
    thresholds: [110, 150],
    pits: (7, (2, 3), 1, None),
    bumps: (3, (1, 2), 5, None),
    ..natural(
        104,
        &[[170, 172, 166], [196, 198, 192], [214, 216, 210], [222, 224, 216], [230, 232, 224], [240, 242, 236]],
    )
};
/// Terracotta: fired clay, the quietest natural surface here.
const TERRACOTTA: Recipe = Recipe {
    jitter: 40,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (4, (1, 2), 5, None),
    ..natural(105, &[])
};
const AZALEA_LEAVES: Palette =
    &[[58, 78, 30], [72, 96, 38], [86, 112, 44], [94, 122, 48], [102, 132, 54], [120, 148, 66]];
const AZALEA_FLOWERS: [[u8; 3]; 2] = [[190, 92, 176], [226, 140, 210]];
const GOLD: [[u8; 3]; 2] = [[196, 150, 36], [242, 204, 92]];

/// Packed mud: mud pressed with short straws lying across it.
fn packed_mud_grid() -> Grid {
    let mut g = PACKED_MUD.grid();
    for k in 0..8 {
        let h = hu(PACKED_MUD.seed, k, 0, 54);
        let cell = cell_order(PACKED_MUD.seed, 54)[k];
        let (x, y) = ((cell % 4) * 4 + h as usize % 4, (cell / 4) * 4 + (h >> 2) as usize % 4);
        for s in 0..2 + (h >> 8) as usize % 2 {
            g[y][(x + s) % N] = if s == 0 { 7 } else { 6 };
        }
    }
    g
}

/// Gilded blackstone: blackstone with gold flecks lit at their corner.
fn gilded_grid() -> Grid {
    let mut g = BLACKSTONE.grid();
    for k in 0..6 {
        let cells = walk(BLACKSTONE.seed, k, 2, 4, 55);
        for &(x, y) in &cells {
            g[y][x] = 6;
        }
        let (x, y) = cells[0];
        g[y][x] = 7;
    }
    g
}

/// Chiseled sandstone: layered bands top and bottom, a carved diamond and
/// a pip on the smooth face between them.
fn chiseled_sandstone_grid(seed: u32) -> Grid {
    let mut g = strata_grid(seed);
    for row in g.iter_mut().take(13).skip(3) {
        for c in row.iter_mut() {
            *c = (*c).clamp(2, 4);
        }
    }
    carve(&mut g, 1, |x, y| {
        let d = (x as i32 * 2 - 15).abs() + (y as i32 * 2 - 15).abs();
        y == 2 || y == 13 || (3..=12).contains(&y) && ((9..=11).contains(&d) || d <= 2)
    });
    g
}

/// Stripped bamboo: four grooved canes a side, ridged at their nodes.
fn stripped_bamboo_grid() -> Grid {
    let mut g = base_grid(109, 4, 32, [100, 156], false);
    carve(&mut g, 1, |x, y| x % 4 == 3 || y % 8 == 3);
    g
}

/// The bamboo block's cut end: four by four cane ends.
fn bamboo_top_grid() -> Grid {
    let mut g = base_grid(110, 4, 32, [100, 156], false);
    carve(&mut g, 1, |x, y| x % 4 == 3 || y % 4 == 3);
    g
}

/// Terracotta's colours: plain and the six natural dyes, then ten dyed with
/// half dye and half clay.
fn terracotta_colours() -> Vec<(u16, [u8; 3])> {
    const PLAIN: [[u8; 3]; 7] =
        [[152, 94, 67], [162, 84, 38], [186, 134, 36], [144, 62, 48], [78, 52, 36], [210, 178, 160], [136, 108, 98]];
    let clay = [152u16, 94, 67];
    let mut out: Vec<(u16, [u8; 3])> = (0..7).map(|i| (tex::TERRACOTTA + i as u16, PLAIN[i])).collect();
    for (i, dye) in [2, 3, 5, 6, 7, 9, 10, 11, 13, 15].into_iter().enumerate() {
        let rgb = crate::color::DyeColor::ALL[dye].rgb();
        let mix = [0, 1, 2].map(|c| (rgb[c] as u16 / 2 + clay[c] / 2) as u8);
        out.push((tex::STAINED_TERRACOTTA + i as u16, mix));
    }
    out
}

// v0.18 Track 3, fourth round: quartz, purpur, copper, colours, ore blocks,
// the Nether and the End.

/// A smooth face: base shades only, very little variation.
fn smooth_grid(seed: u32, jitter: u32) -> Grid {
    let mut g = base_grid(seed, 4, jitter, [96, 160], false);
    break_runs(&mut g, 2, 4, 4);
    g
}

/// The grid turned a quarter: columns become rows (pillars lying down).
fn transpose(g: &Grid) -> Grid {
    std::array::from_fn(|y| std::array::from_fn(|x| g[x][y]))
}

/// Chebyshev distance from the tile's centre, 0 to 7.
fn square_ring(x: usize, y: usize) -> usize {
    x.abs_diff(7).max(y.abs_diff(7))
}

const QUARTZ: [u8; 3] = [236, 230, 222];
/// The block of redstone's layer (`world::redstone_blocks`).
const REDSTONE_BLOCK_LAYER: u16 = 1122;
const PURPUR: [u8; 3] = [170, 126, 170];
const PURPUR_TEX: u16 = crate::world::end_blocks::TEX;

/// Quartz, which is pale and smooth: its shades sit close together.
fn quartz_palette() -> [[u8; 3]; 6] {
    ramp(QUARTZ, [800, 880, 950, 975, 1000, 1020])
}

fn quartz_pillar_side() -> Grid {
    let mut g = smooth_grid(201, 16);
    carve(&mut g, 1, |x, _| x % 5 == 0);
    g
}

/// Copper per oxidation stage: bare metal, its patina, and how much of the
/// surface the patina covers in per mille (`copper_textures`' colours).
const COPPER_METAL: [[u8; 3]; 4] = [[196, 112, 76], [168, 126, 104], [116, 150, 114], [84, 160, 132]];
const COPPER_PATINA: [[u8; 3]; 4] = [[176, 98, 66], [112, 152, 124], [88, 166, 132], [70, 140, 116]];
const COPPER_COVER: [u32; 4] = [0, 300, 600, 900];

/// A copper stage's shades: metal 0-5, patina 6-8, the bulb's dark glass
/// 9-10, its glow 11-12 and the powered mark 13.
fn copper_palette(stage: usize) -> Vec<[u8; 3]> {
    let mut p = ramp(COPPER_METAL[stage], [700, 840, 940, 1000, 1060, 1180]).to_vec();
    p.extend(ramp(COPPER_PATINA[stage], [880, 1000, 1110]));
    p.extend([[70, 60, 58], [116, 84, 64], [232, 170, 96], [255, 214, 140], [210, 40, 30]]);
    p
}

/// Patina over the flat metal of a copper face: the same blotches on every
/// kind of one stage, covering more of it as the copper ages.
fn patina(g: &mut Grid, stage: usize) {
    patina_at(g, stage, 6);
}

/// [`patina`] with the patina's three shades at `first` in the palette.
fn patina_at(g: &mut Grid, stage: usize, first: u8) {
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let v = (field(230, 4, x, y, 1) * 3 + hu(230, x, y, 2) % 256) / 4;
            if (2..=4).contains(c) && v * 1000 < COPPER_COVER[stage] * 256 {
                *c = first + (*c - 2);
            }
        }
    }
}

/// One copper face of `kind` (`world::copper::t`) at `stage`.
fn copper_grid(stage: usize, kind: u16) -> Grid {
    use crate::world::copper::t as ct;
    let mut g = smooth_grid(220 + kind as u32, 32);
    match kind {
        ct::BLOCK => bevel(&mut g),
        ct::CUT => {
            // Four plates, each lit at its top left.
            for (y, row) in g.iter_mut().enumerate() {
                for (x, c) in row.iter_mut().enumerate() {
                    if x % 8 == 7 || y % 8 == 7 {
                        *c = 0;
                    } else if x % 8 == 0 || y % 8 == 0 {
                        *c = 5;
                    }
                }
            }
        }
        ct::CHISELED => {
            bevel(&mut g);
            carve(&mut g, 1, |x, y| ring(x, y) == 4);
        }
        ct::GRATE => {
            bevel(&mut g);
            for (y, row) in g.iter_mut().enumerate() {
                for (x, c) in row.iter_mut().enumerate() {
                    let rim = x == 0 || y == 0 || x == N - 1 || y == N - 1;
                    if !rim && !x.is_multiple_of(4) && !y.is_multiple_of(4) && (x % 4 == 2 || y % 4 == 2) {
                        *c = HOLE;
                    }
                }
            }
        }
        _ => {
            // A bulb: frame, a groove, and the window (lit, powered or not).
            let state = kind - ct::BULB;
            let (lit, powered) = (state & 1 == 1, state & 2 == 2);
            for (y, row) in g.iter_mut().enumerate() {
                for (x, c) in row.iter_mut().enumerate() {
                    let (dx, dy) = ((2 * x).abs_diff(15), (2 * y).abs_diff(15));
                    let d2 = dx * dx + dy * dy;
                    let mid = |v: usize| (6..=9).contains(&v);
                    *c = match ring(x, y) {
                        0 => 0,
                        1 if powered && ((y == 1 || y == 14) && mid(x) || (x == 1 || x == 14) && mid(y)) => 13,
                        1 => 5,
                        2 => *c,
                        3 => 1,
                        _ if lit => {
                            if d2 < 256 {
                                12
                            } else {
                                11
                            }
                        }
                        _ if d2 < 100 => 10,
                        _ => 9,
                    };
                }
            }
        }
    }
    patina(&mut g, stage);
    g
}

/// Concrete: dyed cement, smooth with a faint mottle.
const CONCRETE: Recipe = Recipe {
    jitter: 24,
    thresholds: [96, 160],
    pits: (3, (1, 2), 2, None),
    bumps: (2, (1, 2), 4, None),
    ..natural(240, &[])
};
/// Concrete powder: dyed grit, like sand.
const CONCRETE_POWDER: Recipe = Recipe { seed: 241, ..SAND };

/// Wool: rows of knitted stitches two texels tall, lit on their upper half,
/// with a few stray light fibres.
fn wool_grid() -> Grid {
    let mut g = [[3; N]; N];
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let k = (x + (y / 2) % 2 * 2) % 4;
            let v = [2u8, 3, 4, 3][k];
            *c = if y % 2 == 1 { v - 1 } else { v };
        }
    }
    bumps(&mut g, 250, 6, (1, 2), 5, None, 7);
    pits(&mut g, 250, 4, (1, 1), 1, None, 9);
    g
}

/// The wool and concrete colours, in `DyeColor` order.
fn dye(i: usize) -> [u8; 3] {
    crate::color::DyeColor::ALL[i].rgb()
}

/// Plates `w` x `h` of metal (iron, gold, netherite) with the masonry
/// joints and bevels, over a smooth metal face.
fn metal_plates(seed: u32, w: usize, h: usize, shift: usize) -> Grid {
    let mut g = masonry_grid(seed, w, h, shift);
    let face = smooth_grid(seed + 1, 24);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if (2..=4).contains(c) {
                *c = face[y][x];
            }
        }
    }
    g
}

/// A gem block: 8 x 8 cut stones, each a low pyramid lit at its top left.
fn gem_grid(seed: u32) -> Grid {
    let mut g = [[0; N]; N];
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let (lx, ly) = (x % 8, y % 8);
            *c = if lx == 7 || ly == 7 {
                0
            } else if lx == 0 || ly == 0 {
                5
            } else {
                match (lx + ly).cmp(&7) {
                    std::cmp::Ordering::Less => 4,
                    std::cmp::Ordering::Equal => 3,
                    std::cmp::Ordering::Greater => 2,
                }
            };
        }
    }
    // A glint on each stone's lit face.
    for k in 0..4 {
        let h = hu(seed, k, 0, 60);
        let (x, y) = ((k % 2) * 8 + 2 + h as usize % 2, (k / 2) * 8 + 2 + (h >> 4) as usize % 2);
        g[y][x] = 5;
    }
    g
}

const LAPIS: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (8, (1, 2), 5, None),
    ..natural(261, &[[16, 36, 104], [24, 50, 136], [28, 58, 152], [30, 64, 160], [34, 70, 170], [76, 116, 218]])
};
const REDSTONE_BLOCK: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (8, (1, 2), 5, None),
    ..natural(262, &[[96, 10, 6], [140, 18, 10], [166, 22, 14], [180, 25, 15], [194, 30, 18], [240, 80, 60]])
};
const COAL_BLOCK: Recipe = Recipe {
    jitter: 48,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (7, (1, 2), 5, None),
    ..natural(263, &[[12, 12, 14], [20, 20, 22], [26, 26, 28], [30, 30, 32], [36, 36, 38], [86, 86, 92]])
};
/// Raw ore blocks: chunks of raw metal with dark gaps.
const RAW_IRON: [u8; 3] = [198, 148, 118];
const RAW_GOLD: [u8; 3] = [232, 188, 62];
const RAW_COPPER: [u8; 3] = [176, 98, 64];
const AMETHYST: [u8; 3] = [133, 97, 191];
const AMETHYST_BUDS: [[u8; 3]; 2] = [[196, 156, 238], [236, 214, 255]];

const NETHERRACK: Recipe = Recipe {
    jitter: 72,
    thresholds: [110, 150],
    pits: (8, (2, 4), 1, Some(0)),
    bumps: (5, (1, 3), 5, Some(1)),
    ..natural(270, &[[66, 30, 30], [86, 42, 40], [100, 49, 47], [111, 54, 52], [120, 60, 57], [138, 72, 68]])
};
const CRIMSON_NYLIUM: Recipe = Recipe {
    jitter: 72,
    thresholds: [110, 150],
    pits: (6, (1, 2), 1, None),
    bumps: (9, (1, 3), 5, Some(1)),
    ..natural(271, &[[80, 14, 14], [100, 22, 22], [120, 28, 28], [130, 31, 31], [142, 36, 34], [176, 52, 42]])
};
const WARPED_NYLIUM: Recipe = Recipe {
    seed: 272,
    palette: &[[18, 70, 62], [26, 88, 78], [38, 104, 92], [43, 114, 101], [50, 126, 112], [78, 160, 138]],
    ..CRIMSON_NYLIUM
};
const SOUL_SAND: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (4, (1, 2), 1, None),
    bumps: (5, (1, 2), 5, None),
    ..natural(273, &[[44, 32, 26], [62, 47, 37], [76, 58, 46], [84, 64, 51], [92, 70, 56], [108, 84, 68]])
};
const SOUL_SOIL: Recipe = Recipe {
    pits: (9, (2, 3), 1, Some(0)),
    ..Recipe {
        seed: 274,
        palette: &[[40, 30, 24], [56, 42, 34], [68, 52, 42], [75, 57, 46], [82, 62, 50], [98, 76, 62]],
        ..SOUL_SAND
    }
};
const BASALT: [u8; 3] = [91, 88, 94];
const SMOOTH_BASALT: Recipe = Recipe {
    jitter: 32,
    thresholds: [110, 150],
    pits: (4, (1, 2), 1, None),
    bumps: (3, (1, 2), 5, None),
    ..natural(275, &[[50, 50, 56], [60, 60, 66], [68, 68, 74], [72, 72, 78], [76, 76, 82], [88, 88, 96]])
};
const MAGMA: Palette = &[
    [42, 18, 16],
    [56, 24, 21],
    [66, 29, 25],
    [72, 32, 27],
    [80, 36, 30],
    [96, 46, 38],
    [236, 111, 24],
    [255, 182, 64],
];
const GLOWSTONE: Palette =
    &[[150, 110, 60], [206, 160, 84], [232, 190, 102], [244, 206, 116], [250, 215, 120], [255, 236, 170]];
const QUARTZ_ORE: [[u8; 3]; 3] = [[150, 140, 132], [235, 228, 220], [255, 252, 246]];
const DEBRIS: Palette = &[
    [48, 34, 29],
    [62, 44, 37],
    [70, 49, 41],
    [76, 53, 44],
    [84, 59, 49],
    [100, 72, 60],
    [151, 110, 88],
    [176, 134, 108],
];
const NETHER_WART: Recipe = Recipe {
    jitter: 80,
    thresholds: [105, 150],
    pits: (8, (1, 3), 1, None),
    bumps: (9, (1, 3), 5, Some(1)),
    ..natural(276, &[[64, 2, 2], [86, 4, 4], [104, 6, 6], [114, 6, 6], [126, 10, 10], [156, 26, 22]])
};
const WARPED_WART: Recipe = Recipe {
    seed: 277,
    palette: &[[12, 66, 68], [16, 90, 92], [20, 108, 110], [22, 119, 121], [26, 132, 134], [52, 160, 160]],
    ..NETHER_WART
};
const SHROOMLIGHT: Recipe = Recipe {
    jitter: 64,
    thresholds: [110, 150],
    pits: (6, (1, 2), 1, None),
    bumps: (9, (1, 3), 5, None),
    ..natural(278, &[[196, 98, 40], [220, 124, 56], [234, 142, 66], [240, 150, 72], [246, 162, 84], [255, 214, 140]])
};
const END_STONE: Recipe = Recipe {
    jitter: 56,
    thresholds: [110, 150],
    pits: (8, (2, 4), 1, Some(0)),
    bumps: (3, (1, 2), 5, None),
    ..natural(
        280,
        &[[176, 180, 124], [196, 200, 140], [212, 216, 156], [220, 224, 164], [228, 232, 172], [240, 242, 190]],
    )
};
const OBSIDIAN: Recipe = Recipe {
    jitter: 56,
    thresholds: [110, 150],
    pits: (5, (1, 2), 1, None),
    bumps: (6, (1, 2), 5, None),
    ..natural(281, &[[10, 6, 16], [16, 11, 25], [20, 14, 31], [22, 16, 34], [26, 19, 40], [80, 60, 110]])
};
const TEARS: [[u8; 3]; 2] = [[150, 46, 236], [204, 124, 255]];

/// Soul sand: grains with a few face-like hollows sunk in each quarter.
fn soul_sand_grid() -> Grid {
    let mut g = SOUL_SAND.grid();
    carve(&mut g, 0, |x, y| {
        let (cx, cy) = (x % 8, y % 8);
        (cx == 2 || cx == 5) && cy == 2 || (2..=5).contains(&cx) && cy == 5
    });
    g
}

/// Magma: dark rock cracked by glowing seams (index 6), hottest (7) here and
/// there along them.
fn magma_grid() -> Grid {
    let (mut g, _) = stones(282, 3, 6, 6);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if *c == 6 && hu(282, x, y, 61).is_multiple_of(4) {
                *c = 7;
            }
        }
    }
    g
}

/// Ancient debris' side: dark rubble with pale mineral folds.
fn debris_side_grid() -> Grid {
    let mut g = Recipe { seed: 283, palette: DEBRIS, ..NETHERRACK }.grid();
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            match (y + x / 4 + (x / 7) * 2) % 8 {
                0 => *c = 7,
                1 => *c = 6,
                2 => *c = 1,
                _ => {}
            }
        }
    }
    g
}

/// Ancient debris' top: a squared spiral of pale mineral.
fn debris_top_grid() -> Grid {
    let mut g = Recipe { seed: 284, palette: DEBRIS, ..NETHERRACK }.grid();
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if ring(x, y).is_multiple_of(3) && !(x == 8 && y > 7) {
                *c = if ring(x, y) == 0 { 6 } else { 7 };
            }
        }
    }
    g
}

/// Crying obsidian: obsidian with violet tears running down.
fn crying_grid() -> Grid {
    let mut g = OBSIDIAN.grid();
    for k in 0..6 {
        let h = hu(285, k, 0, 62);
        let cell = cell_order(285, 62)[k];
        let (x, mut y) = ((cell % 4) * 4 + h as usize % 4, (cell / 4) * 4 + (h >> 2) as usize % 4);
        for s in 0..2 + (h >> 8) as usize % 3 {
            g[y][x] = if s == 0 { 7 } else { 6 };
            y = (y + 1) % N;
        }
    }
    g
}

// v0.18 Track 3, fifth round: metal fixtures, Nether wood, ice and snow, and
// the workstations' faces.

/// A face being composed: a grid over a palette that grows as colours are
/// inked in (tools, flames, books on a workstation face).
struct Canvas {
    g: Grid,
    pal: Vec<[u8; 3]>,
}

impl Canvas {
    fn new(g: Grid, pal: &[[u8; 3]]) -> Self {
        Self { g, pal: pal.to_vec() }
    }

    /// The palette index of `c`, added if new.
    fn ink(&mut self, c: [u8; 3]) -> u8 {
        if let Some(i) = self.pal.iter().position(|&p| p == c) {
            return i as u8;
        }
        self.pal.push(c);
        (self.pal.len() - 1) as u8
    }

    /// Sets every texel where `at` holds to colour `c`.
    fn fill(&mut self, c: [u8; 3], at: impl Fn(usize, usize) -> bool) {
        let i = self.ink(c);
        for (y, row) in self.g.iter_mut().enumerate() {
            for (x, v) in row.iter_mut().enumerate() {
                if at(x, y) {
                    *v = i;
                }
            }
        }
    }

    fn tile(&self) -> Tile {
        paint(&self.g, &self.pal)
    }
}

/// `count` cracks running down (as [`crack`] draws two), cut `depth` with lit
/// lower and right walls.
fn cracks(g: &mut Grid, seed: u32, count: usize, depth: u8) {
    let mut path = Vec::new();
    for k in 0..count {
        let h = hu(seed, k, 0, 51);
        let (mut x, mut y) = ((cell_order(seed, 51)[k % 16] % 4) * 4 + h as usize % 4, (h >> 8) as usize % N);
        for s in 0..6 + (h >> 16) as usize % 5 {
            path.push((x, y));
            y = (y + 1) % N;
            match hu(seed, k, s, 52) % 4 {
                0 => x = (x + 1) % N,
                1 => x = (x + N - 1) % N,
                _ => {}
            }
        }
    }
    carve(g, depth, |x, y| path.contains(&(x, y)));
}

/// Paints with per-index alpha (ice is see-through).
fn paint_alpha(g: &Grid, palette: &[[u8; 3]], alpha: impl Fn(u8) -> u8) -> Tile {
    let mut t = paint(g, palette);
    for (i, p) in t.iter_mut().enumerate() {
        if p[3] != 0 {
            p[3] = alpha(g[i / N][i % N]);
        }
    }
    t
}

/// A metal fixture's face (copper's `world::copper::t` kinds, also used for
/// the iron ones) over a metal palette (indices 0-5); see-through texels
/// are holes; `flame` lights a lantern. Keeps each kind's geometry, which
/// the block models sample by position.
fn fixture(kind: u16, palette: &[[u8; 3]], flame: [u8; 3]) -> Canvas {
    use crate::world::copper::t as ct;
    let mut c = Canvas::new(smooth_grid(400 + kind as u32, 32), palette);
    let g = &mut c.g;
    match kind {
        ct::DOOR_TOP | ct::DOOR_BOTTOM => {
            let top = kind == ct::DOOR_TOP;
            for (y, row) in g.iter_mut().enumerate() {
                for (x, v) in row.iter_mut().enumerate() {
                    let frame = x <= 1 || x >= 14 || top && y <= 1 || !top && y >= 14;
                    let rail = !top && (y == 1 || y == 2) || top && (y == 9 || y == 10);
                    let rivet = (x == 3 || x == 12) && (y == 3 || y == 12);
                    *v = if top && (3..=7).contains(&y) && matches!(x, 3..=6 | 9..=12) {
                        HOLE
                    } else if x == 0 || top && y == 0 {
                        5
                    } else if x == 15 || !top && y == 15 {
                        0
                    } else if rivet {
                        5
                    } else if frame || rail {
                        if y == 1 || y == 9 { 4 } else { 2 }
                    } else {
                        *v
                    };
                }
            }
        }
        ct::TRAPDOOR => {
            for (y, row) in g.iter_mut().enumerate() {
                for (x, v) in row.iter_mut().enumerate() {
                    let frame = x <= 1 || y <= 1 || x >= 14 || y >= 14;
                    let hole = !frame && (3..=12).contains(&x) && (3..=12).contains(&y) && x % 3 == 1 && y % 3 == 1;
                    *v = if hole {
                        HOLE
                    } else if x == 0 || y == 0 {
                        5
                    } else if x == 15 || y == 15 {
                        0
                    } else if frame {
                        2
                    } else {
                        *v
                    };
                }
            }
        }
        ct::BARS => {
            for (y, row) in g.iter_mut().enumerate() {
                for (x, v) in row.iter_mut().enumerate() {
                    let bar = x % 4 == 1 || x % 4 == 2;
                    let rail = (1..=2).contains(&y) || (13..=14).contains(&y);
                    *v = if !bar && !rail {
                        HOLE
                    } else if rail {
                        if y == 1 || y == 13 { 5 } else { 1 }
                    } else if x % 4 == 1 {
                        4
                    } else {
                        2
                    };
                }
            }
        }
        ct::CHAIN => {
            for (y, row) in g.iter_mut().enumerate() {
                for (x, v) in row.iter_mut().enumerate() {
                    *v = if matches!(y % 8, 3 | 4) && (7..=8).contains(&x) {
                        HOLE
                    } else if matches!(y % 8, 2 | 5) {
                        1
                    } else if y % 8 == 6 || y % 8 == 1 {
                        4
                    } else {
                        *v
                    };
                }
            }
        }
        ct::LANTERN => {
            let glow = ramp(flame, [840, 1000, 1120]);
            let (dim, mid, hot) = (c.ink(glow[0]), c.ink(glow[1]), c.ink(glow[2]));
            for (y, row) in c.g.iter_mut().enumerate() {
                for (x, v) in row.iter_mut().enumerate() {
                    let caged = x % 3 == 2 || y % 7 == 0 || y % 7 == 6 || matches!(x, 5 | 10);
                    *v = if caged {
                        if y % 7 == 0 {
                            5
                        } else if y % 7 == 6 {
                            1
                        } else {
                            3
                        }
                    } else if (6..=9).contains(&x) && (9..=12).contains(&y) {
                        hot
                    } else if (y % 7) == 1 {
                        dim
                    } else {
                        mid
                    };
                }
            }
        }
        ct::CHEST_TOP => bevel(g),
        ct::CHEST_SIDE | ct::CHEST_FRONT => {
            bevel(g);
            carve(g, 1, |x, y| y == 5 && (1..N - 1).contains(&x));
            if kind == ct::CHEST_FRONT {
                c.fill([62, 58, 56], |x, y| (7..=8).contains(&x) && (4..=7).contains(&y));
                c.fill([112, 106, 102], |x, y| (7..=8).contains(&x) && y == 3);
            }
        }
        _ => {
            // The lightning rod's columns, lit on one side.
            for (y, row) in g.iter_mut().enumerate() {
                for (x, v) in row.iter_mut().enumerate() {
                    if x % 8 == 7 || y < 4 && x % 4 == 1 {
                        *v = 5;
                    }
                }
            }
        }
    }
    c
}

/// The copper fixtures' kinds, after the faces of the fourth round.
const COPPER_FIXTURES: [u16; 10] = {
    use crate::world::copper::t as ct;
    [
        ct::DOOR_TOP,
        ct::DOOR_BOTTOM,
        ct::TRAPDOOR,
        ct::BARS,
        ct::CHAIN,
        ct::LANTERN,
        ct::CHEST_TOP,
        ct::CHEST_SIDE,
        ct::CHEST_FRONT,
        ct::ROD,
    ]
};
const COPPER_FLAME: [u8; 3] = [142, 232, 128];
const IRON: [u8; 3] = [180, 185, 183];
const IRON_DARK: [u8; 3] = [126, 126, 132];
const IRON_DOOR_LAYER: u16 = 1123;

/// The iron door and trapdoor (one layer): a bevelled frame with two
/// window openings.
fn iron_door_grid() -> Grid {
    let mut g = smooth_grid(420, 32);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            if matches!(x, 3..=6 | 9..=12) && (3..=6).contains(&y) {
                *v = HOLE;
            } else if x == 0 || y == 0 {
                *v = 5;
            } else if x == N - 1 || y == N - 1 {
                *v = 0;
            }
        }
    }
    carve(&mut g, 1, |x, y| y == 9 && (1..N - 1).contains(&x));
    g
}

/// The iron chain: like copper's but with dark slots instead of holes
/// (the chain's own boxes give its shape).
fn iron_chain_grid() -> Grid {
    let mut g = smooth_grid(421, 32);
    for (y, row) in g.iter_mut().enumerate() {
        for v in row.iter_mut() {
            *v = match y % 8 {
                3 | 4 => 0,
                2 | 5 => 1,
                1 | 6 => 4,
                _ => *v,
            };
        }
    }
    g
}

/// Nether stems: (bark, vein, glint) of crimson and warped, then the
/// stripped colours.
const STEMS: [([u8; 3], [u8; 3], [u8; 3]); 2] =
    [([92, 25, 29], [148, 54, 74], [232, 132, 52]), ([58, 58, 77], [22, 150, 140], [120, 240, 220])];
const STRIPPED_STEMS: [[u8; 3]; 2] = [[137, 57, 90], [57, 150, 147]];
const STEM_CORES: [[u8; 3]; 2] = [[123, 57, 82], [56, 120, 117]];
const NETHER_PLANKS: [[u8; 3]; 2] = [[101, 48, 70], [43, 104, 99]];
const BONE: [u8; 3] = [229, 225, 207];

/// A stem's side: bark with glowing veins running down and a few glints.
fn stem_side(seed: u32, i: usize) -> Canvas {
    let (bark, vein, glint) = STEMS[i];
    let mut palette = ramp(bark, [560, 720, 880, 1000, 1080, 1200]).to_vec();
    palette[1] = vein;
    let mut c = Canvas::new(stripped_grid(seed), &palette);
    let gl = c.ink(glint);
    for k in 0..4 {
        let (x, y) = walk(seed, k, 1, 1, 64)[0];
        c.g[y][x] = gl;
    }
    c
}

const ICE: Palette =
    &[[232, 242, 255], [128, 166, 232], [142, 178, 242], [150, 186, 246], [158, 192, 248], [214, 230, 255]];
const BLUE_ICE: Palette =
    &[[180, 214, 255], [96, 150, 240], [108, 160, 248], [116, 168, 253], [124, 176, 255], [168, 206, 255]];
const FROSTED_ICE: Palette =
    &[[214, 236, 252], [132, 168, 244], [146, 180, 250], [152, 186, 252], [160, 192, 252], [226, 242, 255]];
const SNOW_BLOCK: Recipe = Recipe {
    jitter: 40,
    thresholds: [110, 150],
    pits: (4, (1, 2), 1, None),
    bumps: (6, (1, 1), 5, None),
    ..natural(
        430,
        &[[214, 222, 232], [226, 232, 240], [236, 241, 248], [240, 245, 252], [244, 248, 253], [255, 255, 255]],
    )
};
const POWDER_SNOW: Recipe = Recipe {
    seed: 431,
    pits: (2, (1, 1), 1, None),
    bumps: (8, (1, 2), 5, None),
    palette: &[[208, 222, 232], [220, 234, 243], [228, 240, 248], [232, 244, 251], [236, 247, 252], [250, 253, 255]],
    ..SNOW_BLOCK
};
const COMPOST: Recipe = Recipe {
    jitter: 72,
    thresholds: [110, 150],
    pits: (7, (1, 3), 1, None),
    bumps: (6, (1, 2), 5, None),
    ..natural(432, &[[60, 42, 24], [76, 54, 31], [90, 66, 38], [99, 72, 42], [108, 80, 47], [128, 98, 60]])
};
const COMPOST_READY: Recipe = Recipe {
    seed: 433,
    palette: &[[92, 78, 58], [114, 98, 74], [130, 111, 84], [139, 119, 90], [148, 127, 96], [168, 148, 114]],
    ..COMPOST
};

/// The workstations' oak, with a sixth shade for carved highlights.
fn oak() -> [[u8; 3]; 6] {
    ramp([162, 130, 78], [620, 800, 900, 1000, 1070, 1140])
}
const OAK_FRAME: [u8; 3] = [104, 74, 44];
fn stone_palette(c: [u8; 3]) -> [[u8; 3]; 6] {
    ramp(c, [700, 840, 960, 1000, 1040, 1130])
}

/// A workstation's planks with a dark wooden frame one texel wide.
fn framed_planks(seed: u32) -> Canvas {
    let mut c = Canvas::new(planks_grid(seed), &oak());
    c.fill(OAK_FRAME, |x, y| x == 0 || y == 0 || x == N - 1 || y == N - 1);
    c
}

/// A furnace-like mouth in rows 8-13 (columns 3-12): dark when cold,
/// flames rising from the bottom when lit; the stone just above it is lit.
fn mouth(c: &mut Canvas, lit: bool, seed: u32) {
    let (x0, x1, y0, y1) = (3, 12, 8, 13);
    c.fill([24, 22, 22], |x, y| (x0..=x1).contains(&x) && (y0..=y1).contains(&y));
    c.fill([58, 56, 54], |x, y| (x0..=x1).contains(&x) && y == y0);
    if lit {
        let hot = [[255, 228, 120], [255, 168, 52], [222, 96, 24]];
        for x in x0..=x1 {
            let tip = y0 + 1 + (hu(seed, x, 0, 65) % 3) as usize;
            for y in tip..=y1 {
                let k = c.ink(hot[((y1 - y) * 3 / (y1 - tip + 1)).min(2)]);
                c.g[y][x] = k;
            }
        }
    }
}

/// The open barrel: its lid's rim, an inner lip lit at the top left, and a
/// dark inside, darker deeper in.
fn barrel_open(lid: &Canvas) -> Canvas {
    let mut c = Canvas::new(lid.g, &lid.pal);
    let depth = [[78, 58, 34], [62, 46, 28], [50, 37, 22], [42, 31, 19], [36, 26, 16]];
    for y in 0..N {
        for x in 0..N {
            let r = ring(x, y);
            if r == 2 {
                let lit = x <= y && x + y <= 15;
                c.g[y][x] = c.ink(if lit { [118, 90, 54] } else { [74, 54, 32] });
            } else if r > 2 {
                c.g[y][x] = c.ink(depth[(r - 3).min(4)]);
            }
        }
    }
    c
}

/// A bookshelf's books: two shelves of spines two texels wide, each lit on
/// its left texel, now and then a short one.
fn books(c: &mut Canvas, seed: u32) {
    const SPINES: [[u8; 3]; 6] =
        [[140, 40, 36], [44, 70, 140], [60, 110, 50], [150, 120, 60], [100, 60, 120], [90, 60, 40]];
    for y in 0..N {
        for x in 1..N - 1 {
            let shelf = y <= 1 || y >= 14 || y == 7 || y == 8;
            if shelf {
                continue;
            }
            let book = (x - 1) / 2 + if y > 8 { 8 } else { 0 };
            let colour = SPINES[(book * 7 + hu(seed, book, 0, 66) as usize) % SPINES.len()];
            let top = y == 2 || y == 9;
            let short = top && hu(seed, book, 1, 66).is_multiple_of(3);
            let k =
                if short { c.ink([44, 32, 22]) } else { c.ink(ramp(colour, [if x % 2 == 1 { 1080 } else { 820 }])[0]) };
            c.g[y][x] = k;
        }
    }
}

/// A chiseled bookshelf's front with the books of `mask` (bit per slot).
fn chiseled_front(mask: u8) -> Canvas {
    const COVERS: [[u8; 3]; 6] =
        [[150, 40, 34], [42, 92, 150], [56, 120, 50], [126, 84, 44], [120, 50, 120], [196, 160, 64]];
    let mut c = framed_planks(510);
    for y in 0..N {
        for x in 0..N {
            let row = match y {
                1..=6 => 0,
                9..=14 => 1,
                _ => continue,
            };
            let column = match x {
                1..=4 => 0,
                6..=9 => 1,
                11..=14 => 2,
                _ => continue,
            };
            let slot = row * 3 + column;
            let first = if row == 0 { 1 } else { 9 };
            let left = [1, 6, 11][column];
            let k = if mask & (1 << slot) == 0 {
                // An empty slot: a recess in shadow at its top and left.
                c.ink(if y == first || x == left { [30, 21, 12] } else { [46, 32, 18] })
            } else {
                let book = (x + slot * 2) % 6;
                let height = 4 + (hu(511, x, slot, 4) % 2) as usize;
                if y - first >= 6 - height {
                    let band = y - first == 6 - height + 1;
                    c.ink(
                        ramp(
                            COVERS[book],
                            [if band {
                                1150
                            } else if x % 2 == 0 {
                                1000
                            } else {
                                860
                            }],
                        )[0],
                    )
                } else {
                    c.ink([46, 32, 18])
                }
            };
            c.g[y][x] = k;
        }
    }
    c
}

/// A village workstation's face: `tex::VILLAGE` + 3 x its index in
/// `village_blocks::BASES` + 0 (side), 1 (top) or 2 (front and bottom).
const fn village(i: u16, face: u16) -> u16 {
    tex::VILLAGE + i * 3 + face
}
/// The village workstations redrawn here, by `BASES` index.
const COMPOSTER: u16 = 2;
const BARREL: u16 = 3;
const SMOKER: u16 = 4;
const BLAST_FURNACE: u16 = 6;
const CARTOGRAPHY: u16 = 8;
const FLETCHING: u16 = 9;
const GRINDSTONE: u16 = 10;
const LECTERN: u16 = 11;
const LOOM: u16 = 12;
const STONECUTTER: u16 = 13;

/// The workstations' faces, each keeping its old layout: crafting table,
/// furnace, smithing table, bookshelves, the village workstations and the
/// lectern's book. Wood is the oak planks' grain, stone is a smooth or
/// cobbled face, metal is plate, all lit from the top left.
fn workstations(put: &mut impl FnMut(u16, Tile)) {
    // Crafting table: a framed top with a scored 2 x 2 grid; sides with a
    // saw and a hammer hung on the planks below a dark band.
    let mut c = framed_planks(500);
    carve(&mut c.g, 1, |x, y| {
        (3..13).contains(&x) && (3..13).contains(&y) && (x == 3 || x == 8 || x == 12 || y == 3 || y == 8 || y == 12)
    });
    put(tex::TABLE_TOP, c.tile());
    let mut c = Canvas::new(planks_grid(501), &oak());
    c.fill(OAK_FRAME, |_, y| y < 3);
    c.fill([128, 92, 56], |_, y| y == 0);
    c.fill([170, 170, 176], |x, y| (2..7).contains(&x) && (5..12).contains(&y) && !(x == 2 && y % 2 == 0));
    c.fill([138, 138, 146], |x, y| (2..7).contains(&x) && y == 11);
    c.fill([96, 64, 36], |x, y| (9..11).contains(&x) && (6..14).contains(&y));
    c.fill([128, 128, 136], |x, y| (8..13).contains(&x) && y == 4);
    c.fill([96, 96, 104], |x, y| (8..13).contains(&x) && y == 5);
    put(tex::TABLE_SIDE, c.tile());

    // Furnace: smooth stone top and sides (banded), a cobbled front with a
    // stone rim round its mouth.
    let stone = stone_palette([128, 128, 128]);
    let mut g = smooth_grid(502, 24);
    bevel(&mut g);
    put(tex::FURNACE_TOP, paint(&g, &stone));
    let mut g = smooth_grid(503, 24);
    carve(&mut g, 1, |_, y| y == 1 || y == 13);
    put(tex::FURNACE_SIDE, paint(&g, &stone));
    for (layer, lit) in [(tex::FURNACE_FRONT, false), (tex::FURNACE_LIT, true)] {
        let mut c = Canvas::new(stones(61, 3, 5, 0).0, COBBLE);
        c.fill([98, 98, 100], |x, y| (2..14).contains(&x) && (6..16).contains(&y));
        c.fill([120, 120, 122], |x, y| (2..14).contains(&x) && y == 6);
        mouth(&mut c, lit, 504);
        put(layer, c.tile());
    }

    // Smithing table: a dark iron top; dark wood sides under an iron band,
    // tongs on the front and a hammer on the sides.
    let iron = ramp([54, 55, 63], [600, 800, 920, 1000, 1120, 1360]);
    let mut g = smooth_grid(505, 48);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| ring(x, y) == 2);
    put(tex::SMITHING_TOP, paint(&g, &iron));
    let dark = ramp([74, 31, 26], [620, 800, 900, 1000, 1080]);
    for layer in [tex::SMITHING_FRONT, tex::SMITHING_SIDE, tex::SMITHING_BOTTOM] {
        let mut c = Canvas::new(planks_grid(506), &dark);
        if layer == tex::SMITHING_BOTTOM {
            c.fill([26, 27, 30], |x, y| !(2..=13).contains(&x) && !(2..=13).contains(&y));
        } else {
            c.fill([47, 48, 55], |_, y| y <= 2);
            c.fill([72, 74, 88], |_, y| y == 0);
            c.fill([17, 18, 19], |_, y| y == 3);
            c.fill([40, 41, 50], |x, y| y > 3 && (x == 0 || x == 15));
            c.fill([24, 25, 28], |x, y| y > 3 && (x == 1 || x == 14));
        }
        if layer == tex::SMITHING_FRONT {
            c.fill([140, 86, 60], |x, y| (x == 3 || x == 5) && (8..=11).contains(&y));
            c.fill([73, 75, 95], |x, y| (3..=5).contains(&x) && y == 7);
        }
        if layer == tex::SMITHING_SIDE {
            c.fill([35, 37, 51], |x, y| (3..=7).contains(&x) && y == 8);
            c.fill([140, 86, 60], |x, y| x == 5 && (9..=13).contains(&y));
        }
        put(layer, c.tile());
    }

    // Bookshelves.
    let mut c = Canvas::new(planks_grid(507), &oak());
    books(&mut c, 507);
    put(tex::BOOKSHELF, c.tile());
    let mut c = framed_planks(508);
    carve(&mut c.g, 1, |x, y| {
        (x == 3 || x == 12) && (3..=12).contains(&y) || (y == 3 || y == 12) && (3..=12).contains(&x)
    });
    put(crate::world::wood::tex::CHISELED_TOP, c.tile());
    let mut c = Canvas::new(planks_grid(509), &oak());
    c.fill(OAK_FRAME, |x, _| x == 0 || x == N - 1);
    put(crate::world::wood::tex::CHISELED_SIDE, c.tile());
    for mask in 0..64u8 {
        put(crate::world::wood::tex::CHISELED_FRONTS + mask as u16, chiseled_front(mask).tile());
    }

    // The village workstations.
    let planks = |seed: u32| paint(&planks_grid(seed), &oak());
    for (i, seed) in [(COMPOSTER, 512), (LECTERN, 513)] {
        let mut c = framed_planks(seed);
        bevel(&mut c.g);
        put(village(i, 0), c.tile());
        put(village(i, 1), planks(seed + 20));
        put(village(i, 2), planks(seed + 40));
    }
    // Barrel: upright staves with two iron hoops; a lid with a bung.
    let mut c = Canvas::new(transpose(&planks_grid(514)), &oak());
    c.fill([64, 66, 61], |_, y| y == 3 || y == 12);
    c.fill([104, 106, 100], |_, y| y == 2 || y == 11);
    put(village(BARREL, 0), c.tile());
    let mut lid = framed_planks(515);
    lid.fill([128, 92, 56], |x, y| ring(x, y) == 1);
    put(village(BARREL, 1), lid.tile());
    let mut front = Canvas::new(lid.g, &lid.pal);
    front.fill([30, 26, 19], |x, y| (7..=8).contains(&x) && (7..=8).contains(&y));
    front.fill([118, 90, 54], |x, y| (7..=8).contains(&x) && y == 9 || x == 9 && (7..=8).contains(&y));
    put(village(BARREL, 2), front.tile());
    put(crate::world::lids::BARREL_OPEN_TEX, barrel_open(&lid).tile());
    // Smoker and blast furnace, cold and lit: brick and iron-plate bodies
    // with a mouth on the front.
    let smoker = ramp([105, 91, 75], [600, 800, 920, 1000, 1080, 1200]);
    let blast = ramp([119, 123, 124], [560, 780, 920, 1000, 1060, 1160]);
    for (i, palette, body) in
        [(SMOKER, smoker, masonry_grid(516, 8, 4, 4)), (BLAST_FURNACE, blast, metal_plates(517, 8, 4, 0))]
    {
        let mut top = smooth_grid(518 + i as u32, 32);
        bevel(&mut top);
        for (k, lit) in [(0, false), (1, true)] {
            put(village(i + k, 0), paint(&body, &palette));
            put(village(i + k, 1), paint(&top, &palette));
            let mut c = Canvas::new(body, &palette);
            mouth(&mut c, lit, 519 + i as u32);
            put(village(i + k, 2), c.tile());
        }
    }
    // Cartography table: a map pinned on a framed top.
    let mut c = Canvas::new(planks_grid(520), &oak());
    c.fill(OAK_FRAME, |x, y| x < 2 || y < 2 || x > 13 || y > 13);
    c.fill([220, 215, 173], |x, y| (2..14).contains(&x) && (2..14).contains(&y) && (x / 4 + y / 4) % 2 == 0);
    c.fill([136, 160, 106], |x, y| (2..14).contains(&x) && (2..14).contains(&y) && (x / 4 + y / 4) % 2 == 1);
    c.fill([94, 76, 58], |x, y| (2..14).contains(&x) && (2..14).contains(&y) && (x == 2 || y == 2));
    put(village(CARTOGRAPHY, 1), c.tile());
    put(village(CARTOGRAPHY, 0), planks(521));
    put(village(CARTOGRAPHY, 2), planks(522));
    // Fletching table: a cream panel with crossed arrows on the front.
    let mut c = framed_planks(523);
    c.fill([212, 195, 148], |x, y| (3..13).contains(&x) && (3..13).contains(&y));
    c.fill([236, 222, 184], |x, y| (3..13).contains(&x) && y == 3 || x == 3 && (3..13).contains(&y));
    c.fill([69, 58, 42], |x, y| (3..13).contains(&x) && (3..13).contains(&y) && (x == y || x + y == 15));
    put(village(FLETCHING, 2), c.tile());
    put(village(FLETCHING, 0), planks(524));
    put(village(FLETCHING, 1), planks(525));
    // Grindstone and stonecutter: smooth stone; the cutter's blade on top.
    let grey = stone_palette([126, 126, 120]);
    for (i, seed) in [(GRINDSTONE, 526), (STONECUTTER, 527)] {
        let g = smooth_grid(seed, 40);
        put(village(i, 0), paint(&g, &grey));
        put(village(i, 2), paint(&g, &grey));
        let mut top = Canvas::new(g, &grey);
        if i == STONECUTTER {
            top.fill([193, 196, 193], |x, _| (6..10).contains(&x));
            top.fill([228, 230, 228], |x, _| x == 6);
            top.fill([150, 152, 150], |x, _| x == 9);
        }
        put(village(i, 1), top.tile());
    }
    // Loom: warp threads on the front between a wooden frame.
    let mut c = framed_planks(528);
    c.fill([188, 170, 139], |x, y| (2..14).contains(&x) && (2..14).contains(&y) && x % 2 == 0);
    c.fill([107, 85, 65], |x, y| (2..14).contains(&x) && (2..14).contains(&y) && x % 2 == 1);
    c.fill([214, 198, 168], |x, y| (2..14).contains(&x) && y == 2 && x % 2 == 0);
    put(village(LOOM, 2), c.tile());
    put(village(LOOM, 0), planks(529));
    put(village(LOOM, 1), planks(530));
    // The lectern's open book.
    let mut c = Canvas::new(planks_grid(531), &oak());
    c.fill([232, 224, 200], |x, y| (2..=13).contains(&x) && (3..=12).contains(&y));
    c.fill([206, 196, 170], |x, y| (2..=13).contains(&x) && y == 12);
    c.fill([150, 140, 120], |x, y| {
        y % 2 == 1 && (4..=11).contains(&y) && ((3..=6).contains(&x) || (9..=12).contains(&x))
    });
    c.fill([92, 60, 40], |x, y| (7..=8).contains(&x) && (3..=12).contains(&y));
    put(crate::world::workstations::BOOK_TOP, c.tile());
}

fn paint(g: &Grid, palette: &[[u8; 3]]) -> Tile {
    let mut out = [[0; 4]; N * N];
    for (y, row) in g.iter().enumerate() {
        for (x, &c) in row.iter().enumerate() {
            out[y * N + x] = if c == HOLE {
                [0; 4]
            } else {
                let [r, g, b] = palette[c as usize];
                [r, g, b, 255]
            };
        }
    }
    out
}

/// A wood's bark, plank colour, light and dark end-grain colours.
struct Wood {
    side: u16,
    top: u16,
    planks: u16,
    bark: [u8; 3],
    plank: [u8; 3],
    light: [u8; 3],
    dark: [u8; 3],
}

const WOODS: [Wood; 9] = [
    Wood {
        side: tex::LOG_SIDE,
        top: tex::LOG_TOP,
        planks: tex::PLANKS,
        bark: [104, 82, 51],
        plank: [162, 130, 78],
        light: [176, 142, 88],
        dark: [150, 118, 70],
    },
    Wood {
        side: tex::SPRUCE_LOG_SIDE,
        top: tex::SPRUCE_LOG_TOP,
        planks: tex::SPRUCE_PLANKS,
        bark: [70, 50, 30],
        plank: [114, 84, 50],
        light: [128, 96, 58],
        dark: [106, 78, 46],
    },
    Wood {
        side: tex::BIRCH_LOG_SIDE,
        top: tex::BIRCH_LOG_TOP,
        planks: tex::BIRCH_PLANKS,
        bark: [216, 214, 204],
        plank: [196, 180, 124],
        light: [200, 182, 128],
        dark: [178, 160, 108],
    },
    Wood {
        side: tex::JUNGLE_LOG_SIDE,
        top: tex::JUNGLE_LOG_TOP,
        planks: tex::JUNGLE_PLANKS,
        bark: [88, 70, 32],
        plank: [160, 114, 80],
        light: [170, 124, 86],
        dark: [146, 104, 70],
    },
    Wood {
        side: tex::ACACIA_LOG_SIDE,
        top: tex::ACACIA_LOG_TOP,
        planks: tex::ACACIA_PLANKS,
        bark: [104, 96, 88],
        plank: [170, 92, 50],
        light: [176, 96, 54],
        dark: [150, 80, 44],
    },
    Wood {
        side: tex::DARK_OAK_LOG_SIDE,
        top: tex::DARK_OAK_LOG_TOP,
        planks: tex::DARK_OAK_PLANKS,
        bark: [58, 40, 22],
        plank: [66, 43, 20],
        light: [96, 70, 42],
        dark: [72, 50, 28],
    },
    Wood {
        side: tex::MANGROVE_LOG_SIDE,
        top: tex::MANGROVE_LOG_TOP,
        planks: tex::MANGROVE_PLANKS,
        bark: [118, 54, 49],
        plank: [117, 54, 48],
        light: [150, 90, 74],
        dark: [120, 68, 58],
    },
    Wood {
        side: tex::CHERRY_LOG_SIDE,
        top: tex::CHERRY_LOG_TOP,
        planks: tex::CHERRY_PLANKS,
        bark: [54, 32, 40],
        plank: [226, 178, 172],
        light: [214, 170, 164],
        dark: [186, 140, 138],
    },
    Wood {
        side: t::PALE_OAK_LOG_SIDE,
        top: t::PALE_OAK_LOG_TOP,
        planks: t::PALE_OAK_PLANKS,
        bark: [84, 76, 72],
        plank: [228, 218, 214],
        light: [232, 226, 220],
        dark: [210, 200, 196],
    },
];

/// Jungle bark's moss, laid over a few of its bark clusters.
const MOSS: [[u8; 3]; 2] = [[70, 88, 36], [92, 112, 46]];

/// The layers drawn here, in a fixed order (tests pin each one's checksum).
#[cfg(test)]
fn layers() -> Vec<u16> {
    let mut out = vec![
        tex::DEEPSLATE,
        tex::ANDESITE,
        tex::DIORITE,
        tex::GRANITE,
        tex::TUFF,
        tex::COBBLESTONE,
        tex::GRAVEL,
        tex::DIRT,
        tex::GRASS_TOP,
        tex::GRASS_SIDE,
        tex::SNOWY_GRASS_SIDE,
        tex::SAND,
        tex::RED_SAND,
    ];
    out.extend(STONE_ORES);
    out.extend(DEEPSLATE_ORES);
    out.extend([
        tex::LEAVES,
        tex::SPRUCE_LEAVES,
        tex::BIRCH_LEAVES,
        tex::JUNGLE_LEAVES,
        tex::ACACIA_LEAVES,
        t::DARK_OAK_LEAVES,
    ]);
    for w in &WOODS {
        out.extend([w.side, w.top, w.planks]);
    }
    out.extend(ROUND_TWO);
    for i in 0..STRIPPED.len() as u16 {
        out.extend([wt::STRIPPED_SIDE + i, wt::STRIPPED_TOP + i]);
    }
    out.extend(ROUND_THREE);
    out.extend(terracotta_colours().into_iter().map(|(layer, _)| layer));
    out.extend(ROUND_FOUR);
    out.extend([tex::OBSIDIAN, tex::CRYING_OBSIDIAN, PURPUR_TEX, PURPUR_TEX + 1, PURPUR_TEX + 2]);
    {
        use crate::world::copper::{Stage, t as ct};
        for stage in Stage::ALL {
            for kind in
                [ct::BLOCK, ct::CUT, ct::CHISELED, ct::GRATE, ct::BULB, ct::BULB + 1, ct::BULB + 2, ct::BULB + 3]
            {
                out.push(ct::layer(stage, kind));
            }
        }
    }
    for i in 0..16 {
        out.extend([tex::CONCRETE + i, tex::CONCRETE_POWDER + i, tex::COLORED_WOOL + i]);
    }
    out.extend(round_five());
    out.extend(final_art::layers());
    out.extend(final_art::round_b_layers());
    // Round B's bamboo leaves are redrawn by gap Track 7; each layer is
    // pinned once, as drawn last.
    let c = final_art::round_c_layers();
    out.retain(|l| !c.contains(l));
    out.extend(c);
    out
}

/// The fifth round's layers, in a fixed order.
#[cfg(test)]
fn round_five() -> Vec<u16> {
    use crate::world::copper::{Stage, t as ct};
    use crate::world::wood::tex as wood;
    let mut out = Vec::new();
    for stage in Stage::ALL {
        out.extend(COPPER_FIXTURES.map(|kind| ct::layer(stage, kind)));
    }
    out.extend([tex::IRON_BARS, tex::CHAIN, tex::LANTERN, tex::SOUL_LANTERN, IRON_DOOR_LAYER]);
    out.extend((0..4).map(|i| tex::NETHER_STEM_SIDE + i));
    out.extend((0..4).map(|i| tex::NETHER_STEM_TOP + i));
    out.extend([tex::CRIMSON_PLANKS, tex::WARPED_PLANKS, tex::BONE_BLOCK_SIDE, tex::BONE_BLOCK_TOP]);
    out.extend([tex::ICE, t::BLUE_ICE]);
    out.extend((0..4).map(|i| crate::world::frosted_ice::TEX + i));
    out.extend([tex::SNOW, t::SNOW_LAYER, tex::POWDER_SNOW, tex::COMPOST, tex::COMPOST_READY]);
    out.extend([
        tex::TABLE_TOP,
        tex::TABLE_SIDE,
        tex::FURNACE_TOP,
        tex::FURNACE_SIDE,
        tex::FURNACE_FRONT,
        tex::FURNACE_LIT,
    ]);
    out.extend([tex::SMITHING_TOP, tex::SMITHING_FRONT, tex::SMITHING_SIDE, tex::SMITHING_BOTTOM]);
    out.extend([tex::BOOKSHELF, wood::CHISELED_TOP, wood::CHISELED_SIDE]);
    out.extend((0..64).map(|m| wood::CHISELED_FRONTS + m));
    for i in [COMPOSTER, BARREL, SMOKER, SMOKER + 1, BLAST_FURNACE, BLAST_FURNACE + 1, CARTOGRAPHY, FLETCHING] {
        out.extend((0..3).map(|f| village(i, f)));
    }
    for i in [GRINDSTONE, LECTERN, LOOM, STONECUTTER] {
        out.extend((0..3).map(|f| village(i, f)));
    }
    out.extend([crate::world::lids::BARREL_OPEN_TEX, crate::world::workstations::BOOK_TOP]);
    out
}

/// The third round's layers, in `layers` order (terracotta after them).
#[cfg(test)]
const ROUND_THREE: [u16; 38] = [
    tex::STONE_BRICKS,
    tex::MOSSY_STONE_BRICKS,
    tex::CRACKED_STONE_BRICKS,
    st::CHISELED_STONE_BRICKS,
    tex::BRICKS,
    tex::NETHER_BRICKS,
    st::CRACKED_NETHER_BRICKS,
    st::CHISELED_NETHER_BRICKS,
    st::RED_NETHER_BRICKS,
    st::END_STONE_BRICKS,
    t::PRISMARINE_BRICKS,
    st::DEEPSLATE_BRICKS,
    st::CRACKED_DEEPSLATE_BRICKS,
    st::DEEPSLATE_TILES,
    st::CRACKED_DEEPSLATE_TILES,
    st::CHISELED_DEEPSLATE,
    st::TUFF_BRICKS,
    st::CHISELED_TUFF,
    st::CHISELED_TUFF_TOP,
    st::CHISELED_TUFF_BRICKS,
    st::CHISELED_TUFF_BRICKS_TOP,
    tex::POLISHED_BLACKSTONE_BRICKS,
    tex::CRACKED_POLISHED_BLACKSTONE_BRICKS,
    tex::POLISHED_BLACKSTONE,
    tex::CHISELED_POLISHED_BLACKSTONE,
    tex::BLACKSTONE,
    tex::GILDED_BLACKSTONE,
    t::MUD,
    t::PACKED_MUD,
    t::MUD_BRICKS,
    tex::CALCITE,
    tex::SMOOTH_STONE,
    st::CHISELED_SANDSTONE,
    st::CHISELED_RED_SANDSTONE,
    wt::STRIPPED_SIDE + 9,
    wt::STRIPPED_TOP + 9,
    t::AZALEA_LEAVES,
    t::FLOWERING_AZALEA_LEAVES,
];

/// The fourth round's layers, in `layers` order (copper, the colours and the
/// purpur layers are added in `layers`).
#[cfg(test)]
const ROUND_FOUR: [u16; 45] = [
    st::QUARTZ_SIDE,
    st::QUARTZ_TOP,
    st::QUARTZ_BOTTOM,
    st::QUARTZ_BRICKS,
    st::CHISELED_QUARTZ,
    st::CHISELED_QUARTZ_TOP,
    st::QUARTZ_PILLAR_SIDE,
    st::QUARTZ_PILLAR_SIDE_TURNED,
    st::QUARTZ_PILLAR_TOP,
    tex::COPPER_BLOCK,
    tex::WOOL,
    tex::IRON_BLOCK,
    tex::GOLD_BLOCK,
    tex::NETHERITE_BLOCK,
    st::DIAMOND_BLOCK,
    tex::EMERALD_BLOCK,
    tex::LAPIS_BLOCK,
    REDSTONE_BLOCK_LAYER,
    st::COAL_BLOCK,
    tex::RAW_IRON_BLOCK,
    tex::RAW_GOLD_BLOCK,
    tex::RAW_COPPER_BLOCK,
    t::AMETHYST,
    t::BUDDING_AMETHYST,
    tex::NETHERRACK,
    tex::CRIMSON_NYLIUM_TOP,
    tex::CRIMSON_NYLIUM_SIDE,
    tex::WARPED_NYLIUM_TOP,
    tex::WARPED_NYLIUM_SIDE,
    tex::QUARTZ_ORE,
    tex::SOUL_SAND,
    tex::SOUL_SOIL,
    tex::BASALT_SIDE,
    tex::BASALT_TOP,
    tex::POLISHED_BASALT_SIDE,
    tex::POLISHED_BASALT_TOP,
    t::SMOOTH_BASALT,
    tex::MAGMA,
    tex::GLOWSTONE,
    tex::DEBRIS_SIDE,
    tex::DEBRIS_TOP,
    tex::NETHER_WART_BLOCK,
    tex::WARPED_WART_BLOCK,
    tex::SHROOMLIGHT,
    tex::END_STONE,
];

/// The second round's single layers, in `layers` order.
#[cfg(test)]
const ROUND_TWO: [u16; 24] = [
    tex::SANDSTONE_SIDE,
    tex::SANDSTONE_TOP,
    st::RED_SANDSTONE,
    st::RED_SANDSTONE_TOP,
    st::RED_SANDSTONE_BOTTOM,
    st::CUT_SANDSTONE,
    st::CUT_RED_SANDSTONE,
    t::COARSE_DIRT,
    t::PODZOL_TOP,
    t::PODZOL_SIDE,
    t::MYCELIUM_TOP,
    t::MYCELIUM_SIDE,
    t::ROOTED_DIRT,
    tex::CLAY,
    tex::BEDROCK,
    tex::POLISHED_ANDESITE,
    tex::POLISHED_DIORITE,
    tex::POLISHED_GRANITE,
    tex::POLISHED_DEEPSLATE,
    st::POLISHED_TUFF,
    tex::COBBLED_DEEPSLATE,
    t::PALE_OAK_LEAVES,
    t::MANGROVE_LEAVES,
    t::CHERRY_LEAVES,
];

/// Ores in [`ORES`] order.
const STONE_ORES: [u16; 8] = [
    tex::COAL_ORE,
    tex::IRON_ORE,
    tex::GOLD_ORE,
    tex::DIAMOND_ORE,
    tex::LAPIS_ORE,
    tex::COPPER_ORE,
    tex::REDSTONE_ORE,
    tex::EMERALD_ORE,
];
const DEEPSLATE_ORES: [u16; 8] = [
    tex::DEEPSLATE_COAL_ORE,
    tex::DEEPSLATE_IRON_ORE,
    tex::DEEPSLATE_GOLD_ORE,
    tex::DEEPSLATE_DIAMOND_ORE,
    tex::DEEPSLATE_LAPIS_ORE,
    tex::DEEPSLATE_COPPER_ORE,
    tex::DEEPSLATE_REDSTONE_ORE,
    tex::DEEPSLATE_EMERALD_ORE,
];

/// Dirt's palette, then a grass, snow, podzol or mycelium lip's.
fn side_palette(lip: Palette) -> Vec<[u8; 3]> {
    DIRT.palette.iter().chain(lip).copied().collect()
}

fn build() -> FxHashMap<u16, Tile> {
    let mut tiles = FxHashMap::default();
    let mut put = |layer: u16, tile: Tile| {
        tiles.insert(layer, tile);
    };
    let deepslate = paint(&deepslate_grid(), DEEPSLATE);
    put(tex::DEEPSLATE, deepslate);
    for (layer, r) in [
        (tex::ANDESITE, &ANDESITE),
        (tex::DIORITE, &DIORITE),
        (tex::GRANITE, &GRANITE),
        (tex::TUFF, &TUFF),
        (tex::DIRT, &DIRT),
        (tex::SAND, &SAND),
        (tex::RED_SAND, &RED_SAND),
        (tex::GRASS_TOP, &GRASS),
    ] {
        put(layer, paint(&r.grid(), r.palette));
    }
    put(tex::COBBLESTONE, paint(&stones(61, 3, 5, 0).0, COBBLE));
    put(tex::GRAVEL, paint(&gravel_grid(), GRAVEL));
    put(tex::GRASS_SIDE, paint(&side_grid(&GRASS.grid(), 42), &side_palette(GRASS.palette)));
    let snow = Recipe { seed: 43, palette: SNOW, ..GRASS };
    put(tex::SNOWY_GRASS_SIDE, paint(&side_grid(&snow.grid(), 43), &side_palette(SNOW)));
    // The ores keep the stone asset's texels and deepslate's.
    let mut stone = [[0; 4]; N * N];
    for (i, p) in stone.iter_mut().enumerate() {
        // Straight from the asset: `textures::pixel` asks this module first,
        // so calling it while the tiles are built would wait on itself.
        *p = voxelcraft::assets::texture_pixel(tex::STONE, i % N, i / N).expect("the stone asset is embedded");
    }
    for i in 0..ORES.len() {
        put(STONE_ORES[i], ore(&stone, 100 + i as u32, i));
        put(DEEPSLATE_ORES[i], ore(&deepslate, 120 + i as u32, i));
    }
    for (layer, seed, palette) in [
        (tex::LEAVES, 91, OAK_LEAVES),
        (tex::SPRUCE_LEAVES, 92, SPRUCE_LEAVES),
        (tex::BIRCH_LEAVES, 93, BIRCH_LEAVES),
        (tex::JUNGLE_LEAVES, 94, JUNGLE_LEAVES),
        (tex::ACACIA_LEAVES, 95, ACACIA_LEAVES),
        (t::DARK_OAK_LEAVES, 96, DARK_OAK_LEAVES),
    ] {
        put(layer, paint(&leaves_grid(seed), palette));
    }
    for (i, w) in WOODS.iter().enumerate() {
        let i = i as u32;
        if w.side == tex::BIRCH_LOG_SIDE {
            put(w.side, paint(&birch_grid(), BIRCH_BARK));
        } else {
            let mut g = bark_grid(140 + i);
            let mut palette = ramp(w.bark, [520, 680, 840, 940, 1040, 1180]).to_vec();
            if w.side == tex::JUNGLE_LOG_SIDE {
                palette.extend(MOSS);
                bumps(&mut g, 140 + i, 5, (2, 4), 7, Some(6), 35);
            }
            put(w.side, paint(&g, &palette));
        }
        let mut top = ramp(w.bark, [680, 900]).to_vec();
        top.extend([w.dark, w.light]);
        // Dark outer ring and pith use the darker wood.
        let pith = ramp(w.dark, [800])[0];
        let palette = [top[0], top[1], pith, top[2], top[3]];
        put(w.top, paint(&log_top_grid(150 + i), &palette));
        put(w.planks, paint(&planks_grid(160 + i), &ramp(w.plank, [620, 800, 900, 1000, 1070])));
    }
    // Second round.
    put(tex::SANDSTONE_SIDE, paint(&strata_grid(54), SANDSTONE_SIDE));
    put(tex::SANDSTONE_TOP, paint(&SANDSTONE_TOP.grid(), SANDSTONE_TOP.palette));
    put(st::RED_SANDSTONE, paint(&strata_grid(55), RED_SANDSTONE_SIDE));
    put(st::RED_SANDSTONE_TOP, paint(&RED_SANDSTONE_TOP.grid(), RED_SANDSTONE_TOP.palette));
    put(st::RED_SANDSTONE_BOTTOM, paint(&RED_SANDSTONE_BOTTOM.grid(), RED_SANDSTONE_BOTTOM.palette));
    put(st::CUT_SANDSTONE, paint(&cut_grid(56), SANDSTONE_SIDE));
    put(st::CUT_RED_SANDSTONE, paint(&cut_grid(57), RED_SANDSTONE_SIDE));
    put(t::COARSE_DIRT, paint(&coarse_dirt_grid(), COARSE_DIRT));
    put(t::ROOTED_DIRT, paint(&rooted_dirt_grid(), ROOTED_DIRT));
    for (top, side, r, seed) in
        [(t::PODZOL_TOP, t::PODZOL_SIDE, &PODZOL, 44), (t::MYCELIUM_TOP, t::MYCELIUM_SIDE, &MYCELIUM, 45)]
    {
        let lip = r.grid();
        put(top, paint(&lip, r.palette));
        put(side, paint(&side_grid(&lip, seed), &side_palette(r.palette)));
    }
    put(tex::CLAY, paint(&CLAY.grid(), CLAY.palette));
    put(tex::BEDROCK, paint(&BEDROCK.grid(), BEDROCK.palette));
    for (layer, seed, colour, tile) in [
        (tex::POLISHED_ANDESITE, 71, [148, 148, 150], N),
        (tex::POLISHED_DIORITE, 72, [228, 228, 230], N),
        (tex::POLISHED_GRANITE, 73, [164, 112, 96], N),
        (st::POLISHED_TUFF, 74, [116, 118, 110], N),
        (tex::POLISHED_DEEPSLATE, 75, [70, 70, 77], N / 2),
    ] {
        put(layer, paint(&polished_grid(seed, tile), &polished_palette(colour)));
    }
    put(tex::COBBLED_DEEPSLATE, paint(&stones(81, 4, 5, 0).0, COBBLED_DEEPSLATE));
    for (layer, seed, palette) in [
        (t::PALE_OAK_LEAVES, 97, PALE_OAK_LEAVES),
        (t::MANGROVE_LEAVES, 98, MANGROVE_LEAVES),
        (t::CHERRY_LEAVES, 99, CHERRY_LEAVES),
    ] {
        put(layer, paint(&leaves_grid(seed), palette));
    }
    for (i, &c) in STRIPPED.iter().enumerate() {
        let (i16, seed) = (i as u16, 170 + i as u32);
        put(wt::STRIPPED_SIDE + i16, paint(&stripped_grid(seed), &ramp(c, [560, 820, 920, 1000, 1060, 1140])));
        let ends = ramp(c, [800, 860, 720, 950, 1060]);
        put(wt::STRIPPED_TOP + i16, paint(&log_top_grid(seed + 20), &ends));
    }
    // Third round.
    let stone_bricks = masonry_grid(111, 16, 8, 8);
    put(tex::STONE_BRICKS, paint(&stone_bricks, STONE_BRICKS));
    let mut g = stone_bricks;
    moss(&mut g, 112);
    put(tex::MOSSY_STONE_BRICKS, paint(&g, STONE_BRICKS));
    let mut g = stone_bricks;
    crack(&mut g, 113);
    put(tex::CRACKED_STONE_BRICKS, paint(&g, STONE_BRICKS));
    let mut g = base_grid(114, 4, 32, [100, 156], false);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| matches!(ring(x, y), 3 | 6));
    put(st::CHISELED_STONE_BRICKS, paint(&g, STONE_BRICKS));
    put(tex::BRICKS, paint(&masonry_grid(115, 8, 4, 4), BRICKS));
    let nether = masonry_grid(116, 8, 4, 4);
    put(tex::NETHER_BRICKS, paint(&nether, NETHER_BRICKS));
    let mut g = nether;
    crack(&mut g, 117);
    put(st::CRACKED_NETHER_BRICKS, paint(&g, NETHER_BRICKS));
    let mut g = nether;
    let face = base_grid(118, 4, 32, [100, 156], false);
    for (y, row) in g.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if ring(x, y) >= 2 {
                *c = face[y][x];
            }
        }
    }
    carve(&mut g, 1, |x, y| matches!(ring(x, y), 2 | 5));
    put(st::CHISELED_NETHER_BRICKS, paint(&g, NETHER_BRICKS));
    put(st::RED_NETHER_BRICKS, paint(&masonry_grid(119, 8, 4, 4), RED_NETHER_BRICKS));
    put(st::END_STONE_BRICKS, paint(&masonry_grid(120, 8, 4, 4), END_STONE_BRICKS));
    put(t::PRISMARINE_BRICKS, paint(&masonry_grid(121, 8, 4, 4), PRISMARINE_BRICKS));
    let deep_bricks = masonry_grid(122, 8, 4, 4);
    put(st::DEEPSLATE_BRICKS, paint(&deep_bricks, DEEPSLATE_BRICKS));
    let mut g = deep_bricks;
    crack(&mut g, 123);
    put(st::CRACKED_DEEPSLATE_BRICKS, paint(&g, DEEPSLATE_BRICKS));
    let tiles8 = masonry_grid(124, 8, 8, 0);
    put(st::DEEPSLATE_TILES, paint(&tiles8, DEEPSLATE_BRICKS));
    let mut g = tiles8;
    crack(&mut g, 125);
    put(st::CRACKED_DEEPSLATE_TILES, paint(&g, DEEPSLATE_BRICKS));
    let mut g = base_grid(126, 4, 32, [100, 156], false);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| matches!(ring(x, y), 2 | 4 | 6));
    put(st::CHISELED_DEEPSLATE, paint(&g, DEEPSLATE_BRICKS));
    let tuff_bricks = masonry_grid(127, 8, 4, 4);
    put(st::TUFF_BRICKS, paint(&tuff_bricks, TUFF_BRICKS));
    let mut g = base_grid(128, 4, 32, [100, 156], false);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| y == 2 || y == 13 || (3..=12).contains(&y) && (x == 2 || x == 13 || y == 4 || y == 11));
    put(st::CHISELED_TUFF, paint(&g, TUFF_BRICKS));
    let mut g = base_grid(129, 4, 32, [100, 156], false);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| matches!(ring(x, y), 3 | 6));
    put(st::CHISELED_TUFF_TOP, paint(&g, TUFF_BRICKS));
    let mut g = tuff_bricks;
    let face = base_grid(130, 4, 32, [100, 156], false);
    for (y, row) in g.iter_mut().enumerate().take(12).skip(4) {
        row.copy_from_slice(&face[y]);
    }
    carve(&mut g, 1, |x, y| y == 3 || y == 11 || (4..=10).contains(&y) && ring(x, y) == 5);
    put(st::CHISELED_TUFF_BRICKS, paint(&g, TUFF_BRICKS));
    let mut g = base_grid(131, 4, 32, [100, 156], false);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| ring(x, y) == 4);
    put(st::CHISELED_TUFF_BRICKS_TOP, paint(&g, TUFF_BRICKS));
    let blackstone_bricks = masonry_grid(132, 16, 8, 8);
    put(tex::POLISHED_BLACKSTONE_BRICKS, paint(&blackstone_bricks, BLACKSTONE_BRICKS));
    let mut g = blackstone_bricks;
    crack(&mut g, 133);
    put(tex::CRACKED_POLISHED_BLACKSTONE_BRICKS, paint(&g, BLACKSTONE_BRICKS));
    put(tex::POLISHED_BLACKSTONE, paint(&polished_grid(134, N), &polished_palette([56, 51, 61])));
    let mut g = polished_grid(135, N);
    carve(&mut g, 1, |x, y| {
        (x == 3 || x == 12) && (3..13).contains(&y)
            || (y == 3 || y == 12) && (3..13).contains(&x)
            || (y == 7 || y == 9) && (6..10).contains(&x)
    });
    put(tex::CHISELED_POLISHED_BLACKSTONE, paint(&g, &polished_palette([56, 51, 61])));
    put(tex::BLACKSTONE, paint(&BLACKSTONE.grid(), BLACKSTONE.palette));
    let gilded: Vec<[u8; 3]> = BLACKSTONE.palette.iter().chain(&GOLD).copied().collect();
    put(tex::GILDED_BLACKSTONE, paint(&gilded_grid(), &gilded));
    put(t::MUD, paint(&MUD.grid(), MUD.palette));
    let packed: Vec<[u8; 3]> = PACKED_MUD.palette.iter().chain(&STRAW).copied().collect();
    put(t::PACKED_MUD, paint(&packed_mud_grid(), &packed));
    put(t::MUD_BRICKS, paint(&masonry_grid(136, 8, 4, 4), MUD_BRICKS));
    put(tex::CALCITE, paint(&CALCITE.grid(), CALCITE.palette));
    put(tex::SMOOTH_STONE, paint(&polished_grid(137, N), &polished_palette([158, 158, 158])));
    put(st::CHISELED_SANDSTONE, paint(&chiseled_sandstone_grid(138), SANDSTONE_SIDE));
    put(st::CHISELED_RED_SANDSTONE, paint(&chiseled_sandstone_grid(139), RED_SANDSTONE_SIDE));
    let bamboo = ramp([195, 175, 80], [560, 820, 920, 1000, 1060, 1140]);
    put(wt::STRIPPED_SIDE + 9, paint(&stripped_bamboo_grid(), &bamboo));
    put(wt::STRIPPED_TOP + 9, paint(&bamboo_top_grid(), &bamboo));
    put(t::AZALEA_LEAVES, paint(&leaves_grid(140), AZALEA_LEAVES));
    let mut g = leaves_grid(141);
    for k in 0..6 {
        let cells = walk(141, k, 2, 3, 56);
        for &(x, y) in &cells {
            g[y][x] = 6;
        }
        let (x, y) = cells[0];
        g[y][x] = 7;
    }
    let flowering: Vec<[u8; 3]> = AZALEA_LEAVES.iter().chain(&AZALEA_FLOWERS).copied().collect();
    put(t::FLOWERING_AZALEA_LEAVES, paint(&g, &flowering));
    for (i, (layer, colour)) in terracotta_colours().into_iter().enumerate() {
        let palette = ramp(colour, [820, 900, 960, 1000, 1040, 1100]);
        let r = Recipe { seed: 150 + i as u32, ..TERRACOTTA };
        put(layer, paint(&r.grid_in(&palette), &palette));
    }
    // Fourth round.
    let q = quartz_palette();
    let mut g = smooth_grid(200, 16);
    put(st::QUARTZ_SIDE, paint(&g, &q));
    bevel(&mut g);
    put(st::QUARTZ_TOP, paint(&g, &q));
    put(st::QUARTZ_BOTTOM, paint(&smooth_grid(202, 8), &q));
    put(st::QUARTZ_BRICKS, paint(&masonry_grid(203, 8, 4, 4), &q));
    let mut g = smooth_grid(204, 16);
    carve(&mut g, 1, |x, y| {
        y == 1
            || y == 14
            || (2..=13).contains(&y) && (3..13).contains(&x) && ((x + y) % 6 == 0 || (x + 15 - y) % 6 == 0)
    });
    put(st::CHISELED_QUARTZ, paint(&g, &q));
    let mut g = smooth_grid(205, 16);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| matches!(ring(x, y), 3 | 6));
    put(st::CHISELED_QUARTZ_TOP, paint(&g, &q));
    let pillar = quartz_pillar_side();
    put(st::QUARTZ_PILLAR_SIDE, paint(&pillar, &q));
    put(st::QUARTZ_PILLAR_SIDE_TURNED, paint(&transpose(&pillar), &q));
    let mut g = smooth_grid(206, 16);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| ring(x, y) == 4);
    put(st::QUARTZ_PILLAR_TOP, paint(&g, &q));
    let purpur = ramp(PURPUR, [620, 800, 920, 1000, 1060, 1140]);
    put(PURPUR_TEX, paint(&masonry_grid(207, 4, 4, 2), &purpur));
    let mut g = smooth_grid(208, 32);
    carve(&mut g, 1, |x, _| x % 4 == 0);
    for x in 0..N {
        (g[0][x], g[N - 1][x]) = (5, 0);
    }
    put(PURPUR_TEX + 1, paint(&g, &purpur));
    let mut g = smooth_grid(209, 32);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| ring(x, y) == 3);
    put(PURPUR_TEX + 2, paint(&g, &purpur));
    {
        use crate::world::copper::{Stage, t as ct};
        for (s, stage) in Stage::ALL.into_iter().enumerate() {
            let palette = copper_palette(s);
            for kind in
                [ct::BLOCK, ct::CUT, ct::CHISELED, ct::GRATE, ct::BULB, ct::BULB + 1, ct::BULB + 2, ct::BULB + 3]
            {
                put(ct::layer(stage, kind), paint(&copper_grid(s, kind), &palette));
            }
        }
        // The unaffected block of copper has its own layer.
        put(tex::COPPER_BLOCK, paint(&copper_grid(0, ct::BLOCK), &copper_palette(0)));
    }
    let wool = wool_grid();
    for i in 0..16 {
        let c = dye(i);
        let concrete = ramp(c, [900, 955, 980, 1000, 1020, 1060]);
        let r = Recipe { seed: 240 + i as u32, ..CONCRETE };
        put(tex::CONCRETE + i as u16, paint(&r.grid_in(&concrete), &concrete));
        let powder = ramp(c, [820, 900, 960, 1000, 1040, 1110]);
        let r = Recipe { seed: 300 + i as u32, ..CONCRETE_POWDER };
        put(tex::CONCRETE_POWDER + i as u16, paint(&r.grid_in(&powder), &powder));
        let knit = ramp(c, [760, 860, 930, 1000, 1060, 1130]);
        put(tex::COLORED_WOOL + i as u16, paint(&wool, &knit));
        if i == 0 {
            put(tex::WOOL, paint(&wool, &knit));
        }
    }
    put(
        tex::IRON_BLOCK,
        paint(&metal_plates(290, 16, 8, 0), &ramp([218, 218, 222], [720, 860, 950, 1000, 1030, 1080])),
    );
    put(tex::GOLD_BLOCK, paint(&metal_plates(292, 8, 8, 4), &ramp([246, 207, 56], [700, 840, 940, 1000, 1030, 1070])));
    put(
        tex::NETHERITE_BLOCK,
        paint(&metal_plates(294, 16, 8, 8), &ramp([67, 60, 63], [560, 780, 920, 1000, 1080, 1300])),
    );
    put(st::DIAMOND_BLOCK, paint(&gem_grid(296), &ramp([98, 226, 220], [600, 820, 920, 1000, 1060, 1150])));
    put(tex::EMERALD_BLOCK, paint(&gem_grid(297), &ramp([24, 176, 78], [560, 800, 920, 1000, 1080, 1300])));
    let mut g = LAPIS.grid();
    bevel(&mut g);
    put(tex::LAPIS_BLOCK, paint(&g, LAPIS.palette));
    let mut g = REDSTONE_BLOCK.grid();
    bevel(&mut g);
    put(REDSTONE_BLOCK_LAYER, paint(&g, REDSTONE_BLOCK.palette));
    put(st::COAL_BLOCK, paint(&COAL_BLOCK.grid(), COAL_BLOCK.palette));
    for (layer, seed, c) in [
        (tex::RAW_IRON_BLOCK, 298, RAW_IRON),
        (tex::RAW_GOLD_BLOCK, 299, RAW_GOLD),
        (tex::RAW_COPPER_BLOCK, 300, RAW_COPPER),
    ] {
        put(layer, paint(&stones(seed, 4, 6, 0).0, &ramp(c, [560, 780, 900, 1000, 1080, 1180])));
    }
    let amethyst: Vec<[u8; 3]> =
        ramp(AMETHYST, [600, 800, 920, 1000, 1080, 1200]).iter().chain(&AMETHYST_BUDS).copied().collect();
    let crystal = stones(301, 3, 6, 0).0;
    put(t::AMETHYST, paint(&crystal, &amethyst));
    let mut g = crystal;
    for k in 0..5 {
        let cells = walk(301, k, 2, 3, 63);
        for &(x, y) in &cells {
            g[y][x] = 6;
        }
        let (x, y) = cells[0];
        g[y][x] = 7;
    }
    put(t::BUDDING_AMETHYST, paint(&g, &amethyst));
    let netherrack = NETHERRACK.grid();
    let netherrack_tile = paint(&netherrack, NETHERRACK.palette);
    put(tex::NETHERRACK, netherrack_tile);
    for (top, side, r, seed) in [
        (tex::CRIMSON_NYLIUM_TOP, tex::CRIMSON_NYLIUM_SIDE, &CRIMSON_NYLIUM, 286),
        (tex::WARPED_NYLIUM_TOP, tex::WARPED_NYLIUM_SIDE, &WARPED_NYLIUM, 287),
    ] {
        let lip = r.grid();
        put(top, paint(&lip, r.palette));
        let palette: Vec<[u8; 3]> = NETHERRACK.palette.iter().chain(r.palette).copied().collect();
        put(side, paint(&side_over(netherrack, &lip, seed), &palette));
    }
    put(tex::QUARTZ_ORE, ore_on(&netherrack_tile, 288, &QUARTZ_ORE, 5));
    put(tex::SOUL_SAND, paint(&soul_sand_grid(), SOUL_SAND.palette));
    put(tex::SOUL_SOIL, paint(&SOUL_SOIL.grid(), SOUL_SOIL.palette));
    let basalt = ramp(BASALT, [560, 720, 880, 1000, 1080, 1200]);
    put(tex::BASALT_SIDE, paint(&stripped_grid(289), &basalt));
    let mut g = Recipe { seed: 302, palette: &[], ..SMOOTH_BASALT }.grid_in(&basalt);
    carve(&mut g, 1, |x, y| square_ring(x, y).is_multiple_of(3));
    put(tex::BASALT_TOP, paint(&g, &basalt));
    let mut g = smooth_grid(303, 32);
    carve(&mut g, 1, |x, _| x % 4 == 3);
    put(tex::POLISHED_BASALT_SIDE, paint(&g, &basalt));
    let mut g = smooth_grid(304, 32);
    bevel(&mut g);
    carve(&mut g, 1, |x, y| ring(x, y) == 3);
    put(tex::POLISHED_BASALT_TOP, paint(&g, &basalt));
    put(t::SMOOTH_BASALT, paint(&SMOOTH_BASALT.grid(), SMOOTH_BASALT.palette));
    put(tex::MAGMA, paint(&magma_grid(), MAGMA));
    put(tex::GLOWSTONE, paint(&stones(305, 4, 6, 0).0, GLOWSTONE));
    put(tex::DEBRIS_SIDE, paint(&debris_side_grid(), DEBRIS));
    put(tex::DEBRIS_TOP, paint(&debris_top_grid(), DEBRIS));
    put(tex::NETHER_WART_BLOCK, paint(&NETHER_WART.grid(), NETHER_WART.palette));
    put(tex::WARPED_WART_BLOCK, paint(&WARPED_WART.grid(), WARPED_WART.palette));
    put(tex::SHROOMLIGHT, paint(&SHROOMLIGHT.grid(), SHROOMLIGHT.palette));
    put(tex::END_STONE, paint(&END_STONE.grid(), END_STONE.palette));
    put(tex::OBSIDIAN, paint(&OBSIDIAN.grid(), OBSIDIAN.palette));
    let crying: Vec<[u8; 3]> = OBSIDIAN.palette.iter().chain(&TEARS).copied().collect();
    put(tex::CRYING_OBSIDIAN, paint(&crying_grid(), &crying));
    // Fifth round.
    {
        use crate::world::copper::{Stage, t as ct};
        for (s, stage) in Stage::ALL.into_iter().enumerate() {
            let palette = copper_palette(s);
            for kind in COPPER_FIXTURES {
                let mut c = fixture(kind, &palette[..6], COPPER_FLAME);
                let first = c.pal.len() as u8;
                c.pal.extend_from_slice(&palette[6..9]);
                patina_at(&mut c.g, s, first);
                put(ct::layer(stage, kind), c.tile());
            }
        }
        let iron = ramp(IRON_DARK, [600, 780, 920, 1000, 1080, 1200]);
        put(tex::IRON_BARS, fixture(ct::BARS, &iron, [0; 3]).tile());
        put(tex::CHAIN, paint(&iron_chain_grid(), &ramp([112, 119, 133], [460, 700, 900, 1000, 1080, 1200])));
        let cage = ramp([58, 58, 64], [600, 800, 920, 1000, 1100, 1300]);
        put(tex::LANTERN, fixture(ct::LANTERN, &cage, [255, 206, 110]).tile());
        put(tex::SOUL_LANTERN, fixture(ct::LANTERN, &cage, [96, 226, 238]).tile());
        put(IRON_DOOR_LAYER, paint(&iron_door_grid(), &ramp(IRON, [700, 840, 950, 1000, 1040, 1100])));
    }
    for i in 0..2 {
        let i16 = i as u16;
        put(tex::NETHER_STEM_SIDE + i16, stem_side(440 + i as u32, i).tile());
        let stripped = ramp(STRIPPED_STEMS[i], [560, 820, 920, 1000, 1060, 1140]);
        put(tex::NETHER_STEM_SIDE + 2 + i16, paint(&stripped_grid(442 + i as u32), &stripped));
        let core = STEM_CORES[i];
        let rim = ramp(STEMS[i].0, [800, 1000]);
        let ends = [rim[0], rim[1], ramp(core, [720])[0], ramp(core, [950])[0], ramp(core, [1080])[0]];
        put(tex::NETHER_STEM_TOP + i16, paint(&log_top_grid(444 + i as u32), &ends));
        let srim = ramp(STRIPPED_STEMS[i], [800, 900]);
        let sends = [srim[0], srim[1], ramp(core, [720])[0], ramp(core, [950])[0], ramp(core, [1080])[0]];
        put(tex::NETHER_STEM_TOP + 2 + i16, paint(&log_top_grid(446 + i as u32), &sends));
        let planks = ramp(NETHER_PLANKS[i], [620, 800, 900, 1000, 1070]);
        put([tex::CRIMSON_PLANKS, tex::WARPED_PLANKS][i], paint(&planks_grid(448 + i as u32), &planks));
    }
    let bone = stone_palette(BONE);
    let mut g = smooth_grid(450, 24);
    carve(&mut g, 1, |x, _| x % 5 == 2);
    put(tex::BONE_BLOCK_SIDE, paint(&g, &bone));
    let mut c = Canvas::new(smooth_grid(451, 24), &bone);
    c.fill([196, 186, 150], |x, y| square_ring(x, y) <= 2);
    carve(&mut c.g, 1, |x, y| matches!(square_ring(x, y), 3 | 7));
    put(tex::BONE_BLOCK_TOP, c.tile());
    let mut g = smooth_grid(452, 24);
    cracks(&mut g, 452, 2, 0);
    put(tex::ICE, paint_alpha(&g, ICE, |i| if i == 0 || i == 5 { 220 } else { 160 }));
    let mut g = smooth_grid(453, 32);
    cracks(&mut g, 453, 3, 0);
    put(t::BLUE_ICE, paint(&g, BLUE_ICE));
    for age in 0..4u16 {
        let mut g = smooth_grid(454, 24);
        cracks(&mut g, 455, age as usize, 0);
        put(crate::world::frosted_ice::TEX + age, paint_alpha(&g, FROSTED_ICE, |_| 190));
    }
    let snow = SNOW_BLOCK.grid();
    put(tex::SNOW, paint(&snow, SNOW_BLOCK.palette));
    put(t::SNOW_LAYER, paint(&snow, SNOW_BLOCK.palette));
    put(tex::POWDER_SNOW, paint(&POWDER_SNOW.grid(), POWDER_SNOW.palette));
    put(tex::COMPOST, paint(&COMPOST.grid(), COMPOST.palette));
    put(tex::COMPOST_READY, paint(&COMPOST_READY.grid(), COMPOST_READY.palette));
    workstations(&mut put);
    final_art::build(&mut put);
    tiles
}

static TILES: LazyLock<FxHashMap<u16, Tile>> = LazyLock::new(build);

/// The texel of a layer drawn here, or `None` for every other layer.
pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Option<Rgba> {
    TILES.get(&layer).map(|tile| tile[(y % N) * N + x % N])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    fn tile(layer: u16) -> Tile {
        std::array::from_fn(|i| super::super::pixel(layer, i % N, i / N))
    }

    /// WCAG relative luminance of an sRGB colour.
    fn luminance(p: Rgba) -> f64 {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(p[0]) + 0.7152 * lin(p[1]) + 0.0722 * lin(p[2])
    }

    fn opaque(t: &Tile) -> Vec<Rgba> {
        t.iter().copied().filter(|p| p[3] > 0).collect()
    }

    fn mean_luminance(t: &Tile) -> f64 {
        let px = opaque(t);
        px.iter().map(|&p| luminance(p)).sum::<f64>() / px.len() as f64
    }

    /// The share of opaque texels within `band` (a fraction) of the mean luminance.
    fn within(t: &Tile, band: f64) -> f64 {
        let (px, mean) = (opaque(t), mean_luminance(t));
        px.iter().filter(|&&p| (luminance(p) - mean).abs() <= band * mean).count() as f64 / px.len() as f64
    }

    /// The longest run of one exact colour in a row or column, across repeats.
    fn longest_run(t: &Tile) -> usize {
        let mut longest = 0;
        for horizontal in [true, false] {
            for a in 0..N {
                let (mut run, mut prev) = (0, None);
                for b in 0..2 * N {
                    let (x, y) = if horizontal { (b % N, a) } else { (a, b % N) };
                    let p = t[y * N + x];
                    run = if Some(p) == prev { run + 1 } else { 1 };
                    prev = Some(p);
                    longest = longest.max(run.min(N));
                }
            }
        }
        longest
    }

    /// The largest difference between a 4 x 4 block's mean luminance and
    /// the tile's, as a fraction of the tile's.
    fn block_deviation(t: &Tile) -> f64 {
        let mean = mean_luminance(t);
        (0..16)
            .map(|b| {
                let lum: f64 = (0..16).map(|i| luminance(t[((b / 4) * 4 + i / 4) * N + (b % 4) * 4 + i % 4])).sum();
                (lum / 16.0 - mean).abs() / mean
            })
            .fold(0.0, f64::max)
    }

    fn mean_rgb(t: &Tile) -> [f64; 3] {
        let px = opaque(t);
        let n = px.len() as f64;
        [0, 1, 2].map(|c| px.iter().map(|p| p[c] as f64).sum::<f64>() / n)
    }

    fn saturation(c: [f64; 3]) -> f64 {
        let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
        if max == 0.0 { 0.0 } else { (max - min) / max }
    }

    fn hue(c: [f64; 3]) -> f64 {
        let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
        let d = max - min;
        if d == 0.0 {
            return 0.0;
        }
        let h = if max == c[0] {
            (c[1] - c[2]) / d % 6.0
        } else if max == c[1] {
            (c[2] - c[0]) / d + 2.0
        } else {
            (c[0] - c[1]) / d + 4.0
        };
        (h * 60.0).rem_euclid(360.0)
    }

    fn fnv(t: &Tile) -> u64 {
        t.iter().flatten().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
    }

    /// Every redrawn tile (and the stone asset) is pinned: a change to the
    /// art must update this table on purpose.
    #[test]
    fn redrawn_tiles_are_deterministic_and_pinned() {
        let drawn: Vec<u16> = std::iter::once(tex::STONE).chain(layers()).collect();
        let got: Vec<(u16, u64)> = drawn.iter().map(|&l| (l, fnv(&tile(l)))).collect();
        // A second build gives the same tiles.
        assert_eq!(build().get(&tex::DIRT), TILES.get(&tex::DIRT));
        let table: String = got.iter().map(|(l, h)| format!("({l}, 0x{h:016x}),\n")).collect();
        assert_eq!(got, PINNED, "material art changed; if on purpose, pin:\n{table}");
    }

    const PINNED: &[(u16, u64)] = &[
        (0, 0xae810368f4376fbc),
        (204, 0xd41cb9633a14bf99),
        (199, 0x362efbfe8d6ed99c),
        (197, 0x6996671afc7f2198),
        (195, 0xba522dd5ed2440e8),
        (201, 0x96d7b8782ba72cf7),
        (10, 0xeb53c3b7b1a533b2),
        (13, 0x7011ae61e64c5df7),
        (1, 0x63722ef4af392629),
        (2, 0x278097d37ebf38b5),
        (3, 0xe2c691f54b1e5655),
        (15, 0xaa4a031435b74227),
        (4, 0x49d20430213de341),
        (114, 0xe9ba9e9e2a252237),
        (16, 0x967ad8ba0d824aaf),
        (17, 0x3bd2132b4bc1e957),
        (18, 0xaf2ca42e27e3e54a),
        (19, 0x7060752c24fe681a),
        (170, 0x3296c555e4fe7410),
        (187, 0x3d1ee3ff846068a7),
        (188, 0xa0d0a0402c643bb0),
        (189, 0x8cef5190f84c3cc3),
        (207, 0xfefc0d819d6a5c2e),
        (208, 0xd01e1f127ba1e4be),
        (210, 0xd273482e63869571),
        (214, 0xb2753accec5d1358),
        (213, 0x79980ea2d47f92d6),
        (209, 0xf00d6da11d9870c8),
        (211, 0xce30ec6f7732d978),
        (212, 0x563e107472f00dd2),
        (8, 0xad8eb2d4de40d1ed),
        (26, 0x2711eff28ed9d60f),
        (104, 0x5b326b56fb5abe47),
        (105, 0xf337182cd62f5e8e),
        (106, 0x7e806a54272b7baa),
        (1552, 0xf456c9f821a3681c),
        (6, 0xb9c1dfe5e7d7219e),
        (7, 0x818f10a6778f4f67),
        (9, 0x4096540ef9224c5b),
        (96, 0x5f9dca8be4989e1e),
        (97, 0x699b24a11e2fa2f5),
        (107, 0x15de0d59da4ec6d3),
        (98, 0xb1d6b1793fc23695),
        (99, 0xde7c5d7fcab895e5),
        (108, 0x7fbdd9811509bdeb),
        (100, 0x78cad4a2a575149c),
        (101, 0xb972b1eed6a3fab0),
        (109, 0x7e5c200cba9ab0dd),
        (102, 0x8f46b65efdbed01d),
        (103, 0x712ac1a20365a9a7),
        (110, 0x2f8eea87f1c6b357),
        (215, 0xa68240e975f37101),
        (216, 0x4627c498cac1838b),
        (221, 0x3d1bc64e34ae4f44),
        (217, 0x4ea8309621b85d1f),
        (218, 0x7a4b5599e0c64159),
        (222, 0x95a23e2a4cfbece1),
        (219, 0xca15b022740f2761),
        (220, 0xb2b4627998cb4859),
        (223, 0xe20d7fee3ae5b100),
        (1560, 0x489f76bc055a73c6),
        (1561, 0xca26cef8b8756002),
        (1562, 0x45571d11a3f5c969),
        (23, 0xfdf7d977ebf79357),
        (24, 0xb1f99954af32133e),
        (2019, 0x6f0253f2e9c99efe),
        (2020, 0xc30c46695af22665),
        (2021, 0x53dfeb677ef45a79),
        (2024, 0x2d963cfba97ad2e9),
        (2022, 0x161ba04bbc545b1b),
        (1452, 0xf9cb0c05444faabd),
        (1450, 0xc643a00f6489f323),
        (1451, 0xa75da94ff13c665a),
        (1454, 0x3da7002fe5c8aa17),
        (1455, 0x324d38e13cd4dae0),
        (1453, 0xb6cc06d6724862e7),
        (122, 0xdfe36a2f27631aa5),
        (12, 0xa21ad4c6a0f21d93),
        (200, 0x95b31419d6736774),
        (198, 0x8af6c5f23a2d4a06),
        (196, 0xf9d8e87d067bae11),
        (206, 0xaaa2a4309e0046fc),
        (2000, 0xbf0c398f7c2207f8),
        (205, 0xc9c0f7e117424e11),
        (1555, 0x85fcc4e9b8dea8ba),
        (1553, 0x905885d9bc355f3d),
        (1554, 0x94b43222d94dde53),
        (1776, 0x29c6ba43befb7e2c),
        (1786, 0x4eb864a606e00f2c),
        (1777, 0xffb9af72c67a657d),
        (1787, 0x8d12378ab6ae0627),
        (1778, 0xff2bcac6495b7a8a),
        (1788, 0x6906eb6c780cc3d4),
        (1779, 0x3d5a44781196bddb),
        (1789, 0x1eea6993a969896a),
        (1780, 0xe5d4e79612d2a9b0),
        (1790, 0xf8df9dbbace14e10),
        (1781, 0xfeb3cec080af5668),
        (1791, 0x8028c9156482d7f7),
        (1782, 0x4d360292f8e2c432),
        (1792, 0x3f2f2610d61b9ced),
        (1783, 0x8f6db4b1f2998d5d),
        (1793, 0x97961019dc7d8ba3),
        (1784, 0x13bc5933ed64106d),
        (1794, 0xe78179c1c58a6ee3),
        (158, 0x97e4bd37bea9daf5),
        (159, 0x890bc818f2315291),
        (160, 0x95e22768bf8d6ba3),
        (2030, 0x1ea42f0640f18fc9),
        (22, 0x40575369ce7dbd08),
        (80, 0xf286d4140f53f3cd),
        (2028, 0xbd00a852cd197245),
        (2027, 0x121c89f1613f15a5),
        (2026, 0x897f73d19c311599),
        (2029, 0x4cdb3d652571514b),
        (1513, 0x89cd70655ca14dcd),
        (2006, 0x62ba1178c86a15a9),
        (2007, 0x78ddb8a8ba007081),
        (2008, 0x4c421e1e7cb52633),
        (2009, 0xdf6f26539e9e5c17),
        (2010, 0x0162cb6d5be08793),
        (2001, 0xec77fec4ba3be7b5),
        (2002, 0x804a7af27d117007),
        (2003, 0xf113de615366fad3),
        (2004, 0xd68cf2595b3026e6),
        (2005, 0x0df8463f87d671b0),
        (246, 0x46842491f2f50d55),
        (247, 0x385f383048dbd59a),
        (245, 0x8dd9dc1b6c8d25bb),
        (248, 0x4091f253feb7855b),
        (244, 0x0251bc4eb7be2c1c),
        (249, 0x7f254dddbed81791),
        (1456, 0x585a91aad59f9cad),
        (1457, 0x18241999acaf741c),
        (1458, 0xfdb8e90bb31b9526),
        (202, 0x75bfeddc961f40a7),
        (203, 0x36d03bb4a8a23373),
        (2025, 0xcd8fc25980c6b9c5),
        (2023, 0x4164aa47229fe91b),
        (1785, 0x4c7ee45f64dfe881),
        (1795, 0x3a77413aeeb186f5),
        (1488, 0xfa0c2916e8ca3e9f),
        (1489, 0x7de72144511370c1),
        (115, 0xc9464d44ea7873b1),
        (116, 0x1803d6d5baff2422),
        (117, 0xd02e4bf1044f1cf8),
        (118, 0x42220ccd573c3b17),
        (119, 0xfef18607934fac67),
        (120, 0xab33dd5335b598b0),
        (121, 0xeb62d9cd6ab34671),
        (353, 0x90271a3f8dba3967),
        (354, 0x0913f2b4326592fa),
        (355, 0xbdb01c8fd7c7e515),
        (356, 0xd7848f76051eff2a),
        (357, 0x0e07e51e8f3aa949),
        (358, 0x43ba64495b13c07c),
        (359, 0x0aab093c295db1dd),
        (360, 0x98d114f4e837d587),
        (361, 0xb985ad67ba0a8793),
        (362, 0x5483464001a3a7ef),
        (2011, 0xd84b3640e476b4ab),
        (2012, 0xe6c9ef18629c544e),
        (2013, 0xaf7909c088f81de1),
        (2014, 0x22bf9fd3c7cec053),
        (2015, 0x1488b1bb5897c72b),
        (2016, 0x83dbf956b3c4437e),
        (2017, 0x58d71d5cb366b317),
        (2035, 0xa69d9d8681f5d407),
        (2018, 0x8a3438b5a59da80c),
        (193, 0x66b7a5d7e83aef9c),
        (48, 0x5f073c4826be15b1),
        (174, 0xea305e916bd91321),
        (255, 0xb0ebdfe265f2bd95),
        (181, 0x6821fb4419159c24),
        (2032, 0xf1445b5cb4b236cd),
        (194, 0x76d16b6661799e55),
        (171, 0x290545d7113e1957),
        (1122, 0x6f3d6d16f68f7dfb),
        (2031, 0xf07f4e99b04ae59b),
        (190, 0xd52ee230a9b70aea),
        (191, 0xdda45e470c8eee36),
        (192, 0x04da7952d4ffa10c),
        (1473, 0x77609d8461f8c42b),
        (1474, 0x4ebbee29077f3193),
        (77, 0xc902ef271b71dae6),
        (1307, 0x630a67b39b390865),
        (1308, 0xf98e5a34b951e299),
        (1309, 0x89758cb402141815),
        (1310, 0x8427f7fa2a9a2d91),
        (79, 0x426524a894d9744a),
        (78, 0x08db746024b8ed8b),
        (1333, 0x7a37483d3df333c3),
        (250, 0xd86535ca29baf051),
        (251, 0x9cf505fb70c71b9b),
        (252, 0xa39a260f8b603ffd),
        (253, 0x2ca47ce4640a7601),
        (1483, 0x7208ac569222f567),
        (254, 0x239d8f7663dc6bec),
        (25, 0x983b1295c9c1e27b),
        (179, 0xe2224e782f4548ca),
        (180, 0xda1458f960993f7b),
        (1321, 0x63c44db74e3d644b),
        (1322, 0x197399a92b13ca49),
        (1323, 0xba6178993a1866ac),
        (151, 0xd9829146645c4c59),
        (47, 0x6a7ee9615d532cbe),
        (1400, 0x4d6bc1c6f11a97cf),
        (529, 0xf89094352c6f6994),
        (530, 0xaf114e826573019b),
        (531, 0xde26c778a22ef318),
        (2128, 0x66b7a5d7e83aef9c),
        (2129, 0xd0f255e2e09f5d88),
        (2130, 0x111142c042cee882),
        (2131, 0xa5822aef04b928e9),
        (2132, 0xfa318d3863981610),
        (2133, 0x32f0d76b5ad62fac),
        (2134, 0x6d8ab8d9898dbefd),
        (2135, 0x30483a00c6821c48),
        (2146, 0x31e1c43e18624e4b),
        (2147, 0x9b2bfa743a0b0276),
        (2148, 0x7e656ec1531a9c05),
        (2149, 0x9c0696a9e227c434),
        (2150, 0x3678d4274675eeb4),
        (2151, 0xe80359be773cc6e8),
        (2152, 0xa7f5be802f4092b5),
        (2153, 0x1f3573ffb2fabf04),
        (2164, 0x36f6abb82292cf8f),
        (2165, 0xa90ca874a80b3fc8),
        (2166, 0x82f54473941e464e),
        (2167, 0x745c019ab32b60ac),
        (2168, 0xdb88078822c496ad),
        (2169, 0x8c4bdf32ee448293),
        (2170, 0xd2e14e4ba47a2ace),
        (2171, 0x1220585564ad7aba),
        (2182, 0xbcd810e38f151859),
        (2183, 0x0527c3aa5410492b),
        (2184, 0x5fb30f3bda0cc15f),
        (2185, 0x5b574a3d6c30213f),
        (2186, 0x4936de947ad14f43),
        (2187, 0xbcad07362d167ffb),
        (2188, 0x03b62ed72d451e31),
        (2189, 0x46fd3f822a2e0983),
        (379, 0xfde983214bf906c0),
        (363, 0x9ca005e780f254ca),
        (257, 0x5f073c4826be15b1),
        (380, 0xe0167c713c18a95a),
        (364, 0x8c2afd6e29ceb36f),
        (258, 0xfac45fa3790aeaa1),
        (381, 0x6b6c7c944221e126),
        (365, 0x2bb39dd5e633de5c),
        (259, 0xd417d9181ca98d81),
        (382, 0x9714b725f36687b4),
        (366, 0xd76300e4e5cbde4d),
        (260, 0x75c6d65684d58e41),
        (383, 0xe4725ef4c42038a1),
        (367, 0x93aaa6c1a4d9d167),
        (261, 0xc1730e30518bfbdf),
        (384, 0x68742140ee96e069),
        (368, 0x662c0b42f9905915),
        (262, 0x09f79911d3907821),
        (385, 0x6b523515ed987051),
        (369, 0xa8295f54926a6178),
        (263, 0x1eec6c606f7e7ed8),
        (386, 0x43ccc1aa5f3395b1),
        (370, 0xeb0aa77871188190),
        (264, 0x4c82f14a062dc74b),
        (387, 0x732a8f87cc9d03bd),
        (371, 0x95555df4883e36d5),
        (265, 0x8bcb5973098acb3a),
        (388, 0x681ea96b7c8d659e),
        (372, 0x98efba6450d213a0),
        (266, 0x37e51dbb07a195da),
        (389, 0xb43ac81096939f69),
        (373, 0xeb09f5cf04494b05),
        (267, 0x006063817bd93faa),
        (390, 0xa38794b1b1e6243d),
        (374, 0xaafe50be37d68f03),
        (268, 0x89f0e258ed1c8655),
        (391, 0x806f7a960bc3a58d),
        (375, 0xc59b29d06bfddaa1),
        (269, 0x29cb5cb12b7fcd96),
        (392, 0xe034db6a2e91c699),
        (376, 0xf76c786aac5fb238),
        (270, 0xaa75cbcec7349e34),
        (393, 0xf028ba983872523f),
        (377, 0x32a5eb81aaa9cb27),
        (271, 0xa485c4f9473f4f51),
        (394, 0x16a815028389c32e),
        (378, 0x3e797f401dcf27d8),
        (272, 0xf9c23555f24821e7),
        (2136, 0xc7038ba18dc61238),
        (2137, 0x9b7269cb74a9d34c),
        (2138, 0x3de6c3f2026fe1b5),
        (2139, 0x408a20b0c8a2bbe5),
        (2140, 0xa5b70fafce98fc44),
        (2141, 0x0d9864941b064aad),
        (2142, 0x0585c1a9cbc9b318),
        (2143, 0x0eca70ea55bf8b79),
        (2144, 0x11fbc63c4957ad39),
        (2145, 0x11f62db18dea8e54),
        (2154, 0xf18dd877a0257f85),
        (2155, 0x6a1239340cfcd52e),
        (2156, 0x0b24b2ebd91b181c),
        (2157, 0xfcc45b2d10a817d7),
        (2158, 0xf3fbd78218c42e30),
        (2159, 0x74d2362c7ac37faf),
        (2160, 0xab0d76f65230b8af),
        (2161, 0x682007550aa1ccca),
        (2162, 0x3674961bb031d758),
        (2163, 0x74fbc32cc1b642ba),
        (2172, 0xade15fc802f2d1cc),
        (2173, 0x784d57a71fa85d11),
        (2174, 0x011fa15042acdf5c),
        (2175, 0x5ff0fbf728d3bac7),
        (2176, 0x5b18a88f59c7b610),
        (2177, 0xc9551836aeea5c9d),
        (2178, 0xc9e08ec1fe2fad42),
        (2179, 0xb72573ee3c015027),
        (2180, 0x90306aa6713f5adc),
        (2181, 0xaded40151258d189),
        (2190, 0xd7601d28e7e1b6f5),
        (2191, 0xccf08e396293e053),
        (2192, 0xbaa328890bcbe2fe),
        (2193, 0x0877f21f1ac96265),
        (2194, 0xc7eb76088effe6e7),
        (2195, 0x15c89c5471aff84d),
        (2196, 0xcbc26c27e346c191),
        (2197, 0xab23d4829b77d117),
        (2198, 0x568ec4fe1a5a80bb),
        (2199, 0x6c4b739df1090887),
        (161, 0x265245cd5bb4b025),
        (256, 0x6c60f59f1b09b064),
        (1761, 0x9e752158d0194dbd),
        (1762, 0xf72b67b84a615215),
        (1123, 0x2a1dc190ced09380),
        (1311, 0x79c389f18a2ca92d),
        (1312, 0x5ef781220cbddd83),
        (1313, 0x98844abf82891513),
        (1314, 0x2bcd758bb3bb74a5),
        (1315, 0x39a61b56678a2929),
        (1316, 0x9f457e2c2afbf2bc),
        (1317, 0xd586df029d91fdb8),
        (1318, 0x9dd139fd7f37a2f8),
        (1319, 0xc2e7cfd9fca41223),
        (1320, 0xef99d52bb321331d),
        (1335, 0x0c0cc755d6eb44af),
        (1336, 0xd2edbff756d33d1d),
        (130, 0x179bbc0890ee5f37),
        (1461, 0xab5d51b8f2de7ff0),
        (4000, 0x437873d2a35a7b17),
        (4001, 0x2b50650418677228),
        (4002, 0x2f0faa686c3d019f),
        (4003, 0xdf59052f31028e34),
        (14, 0x93726f9e0b8f5421),
        (1611, 0x93726f9e0b8f5421),
        (1765, 0x829b452bcfa0a972),
        (527, 0x74867425031de051),
        (528, 0xce9beee23de48ef6),
        (49, 0xafa4c4535c2d3421),
        (50, 0x76f5a3f76ad0001b),
        (53, 0xf23348e40db0acc6),
        (57, 0xe422a0f6aabf577a),
        (51, 0x5d21cd9689880730),
        (52, 0x61ae0809534990b8),
        (182, 0x575b5c854bc119c6),
        (183, 0xdd38a2ef3558e2f9),
        (184, 0x0bc1e95aa652131d),
        (185, 0x3203d227377ef62f),
        (162, 0x55ed2b48ccab440e),
        (1919, 0x2acf5b22e204305f),
        (1920, 0x96af653f3e6296c6),
        (1921, 0x1f68c73c19d0611d),
        (1922, 0x3f07602f831e31b1),
        (1923, 0x43d36cfbd6e0a169),
        (1924, 0x68c57d940bd71ca1),
        (1925, 0xd340d2bb46c06e21),
        (1926, 0xd506ba17ef91f4f1),
        (1927, 0x07d21c1fe4ae96d1),
        (1928, 0xd4f810c3c0939d85),
        (1929, 0xa8d7b69ab53c10a4),
        (1930, 0x67dd2f4f47bc3358),
        (1931, 0xb0b4dc9f4725fa20),
        (1932, 0x059b604249d60f68),
        (1933, 0x4f726d05a21c97e8),
        (1934, 0x86074880d3230498),
        (1935, 0x9415eda3c7d21338),
        (1936, 0x2deb4b21e9a67d3c),
        (1937, 0xfabfdb2e678619e6),
        (1938, 0x0809d54662205daa),
        (1939, 0xec7ccf57c5d19512),
        (1940, 0x0d2a5759b9c09dda),
        (1941, 0x4e1a6045e5f31b5a),
        (1942, 0x6c23ff58828c186a),
        (1943, 0x7bd4bf5fe1dc0b4a),
        (1944, 0xaff03f800adc7f3e),
        (1945, 0x456954cf3d4569ff),
        (1946, 0x767d91b2b7a63b43),
        (1947, 0x467afd042a30f0db),
        (1948, 0x15911cbbf1ce1e13),
        (1949, 0xdb9f71cf7bb33693),
        (1950, 0x65519936aa944e03),
        (1951, 0x107f601b3afb63a3),
        (1952, 0xe0c7d666eca06987),
        (1953, 0x1bf87c7d22ffacb7),
        (1954, 0xbd4b3933c6f0eec3),
        (1955, 0x045185bd8b556dbb),
        (1956, 0x9399a80b547f9473),
        (1957, 0x3eb474253dbb9af3),
        (1958, 0x64c6467ffc64e283),
        (1959, 0xd00154c7c39726a3),
        (1960, 0x63a4d77699f35e3f),
        (1961, 0x5dd326304b9fc2c6),
        (1962, 0x0444afabd723f1f2),
        (1963, 0x049707bab89cd83a),
        (1964, 0xfc51d9799f19c8c2),
        (1965, 0x2c3bcfed4cb06242),
        (1966, 0x41d048673f96e3b2),
        (1967, 0x4c6cb545caab1a12),
        (1968, 0xf21349a75741543e),
        (1969, 0xde7f73d9674cb03c),
        (1970, 0xa09512e6e2e78a58),
        (1971, 0x741a6b307fd17780),
        (1972, 0xe670b26eb47e4bc8),
        (1973, 0x805928b08feb3c48),
        (1974, 0xeb6b6be241fceb98),
        (1975, 0x314e9c54afc8f2b8),
        (1976, 0x8fb0a996a561c174),
        (1977, 0xb8fc82fa09692f4d),
        (1978, 0xb05e0ddb72e9d899),
        (1979, 0x2e2b309353f7c051),
        (1980, 0x8f61a570226f5d29),
        (1981, 0x92fabf8034defaa9),
        (1982, 0x20a0aa63a57fa5d9),
        (1983, 0x7465bd2619e95439),
        (1984, 0x114f0bc294d53e35),
        (486, 0xb6223bbe42cbe454),
        (487, 0xc859bf5162dab929),
        (488, 0x24a9d5ab957e1bc0),
        (489, 0xfa0dd922be604ddd),
        (490, 0xb752556d011b7c64),
        (491, 0x95ffca6a4a3a94cb),
        (492, 0x8ef9b4f7fc505b01),
        (493, 0x9c7ae65527981129),
        (494, 0xaa47324a104d8617),
        (495, 0x8ef9b4f7fc505b01),
        (496, 0x9c7ae65527981129),
        (497, 0xc5437881b0af2444),
        (498, 0xb5e4d760066cec62),
        (499, 0xea351679916cefad),
        (500, 0x1c79fad48132af04),
        (501, 0xb5e4d760066cec62),
        (502, 0xea351679916cefad),
        (503, 0x9e976e895e15bf63),
        (504, 0xf90eb0657b1019c1),
        (505, 0x87a462730e8968cd),
        (506, 0x339ecde7b5995730),
        (507, 0x73a29a60fae88aa3),
        (508, 0x3e907d74440228c0),
        (509, 0xea6d896a7bedc38b),
        (510, 0xb97483cb4286b8e6),
        (511, 0xb97483cb4286b8e6),
        (512, 0xb97483cb4286b8e6),
        (513, 0x04083385996ae74e),
        (514, 0x2e75355a7e358664),
        (515, 0xe9732268ad6d94bf),
        (516, 0xd2a70b789542aa84),
        (517, 0x0f13388b22aa9e1f),
        (518, 0x1a2d79cac071f706),
        (519, 0x4d87dc23ddcb5171),
        (520, 0xc1da815bd3f348fc),
        (521, 0x4d87dc23ddcb5171),
        (1290, 0x56edac78a390ac15),
        (3945, 0xafafece853093a74),
        (86, 0x766069f4a8a7a5b1),
        (87, 0x9cf04453681c69bc),
        (1124, 0xe215cfdb587fc39a),
        (227, 0x562b6634e190a7f0),
        (226, 0xa9e46ed7dbdbd7a3),
        (1816, 0xb779e122ea2dfa88),
        (229, 0x6bc1ba2acaf6f095),
        (228, 0xc9d339d74963f41d),
        (1817, 0xceddb8129a946479),
        (231, 0x1d571b1e0baa4c8f),
        (230, 0x7331520ac89069cf),
        (1818, 0xfc461028e04646f3),
        (233, 0x53bd8be428e281b6),
        (232, 0x89d3987d2228530d),
        (1819, 0xd8b60685aba9b766),
        (225, 0xdd380c83223eee00),
        (224, 0x0b57d7fa9beec83f),
        (1820, 0x16106c61eb14290c),
        (235, 0x9029e611bb318193),
        (234, 0xa24849476c8f0a13),
        (1821, 0xb33c7ad053db76b3),
        (237, 0x5a1a401852b12c8c),
        (236, 0x1fb41779b750b5f6),
        (1822, 0x106c230aea0aae5b),
        (1754, 0xfc4ec58d388b7fc4),
        (1753, 0x47f6c8d32c72c81a),
        (1757, 0x6103649202b6a11b),
        (1756, 0xe99de4d6cbee35a6),
        (1755, 0x27ae7bf1cf0717e5),
        (1758, 0x5df0bf4951683cd6),
        (1338, 0x80269f6e699a3987),
        (1337, 0xf3ecb122f75298da),
        (1341, 0xfc37bc3d8bb0d413),
        (1340, 0xe2c1b868f35d4d5d),
        (1339, 0x83c67a1266a1bcdd),
        (1342, 0x6f4569ee64b52c4d),
        (761, 0x38ddcdca30bc9324),
        (760, 0xd56f2e1838090329),
        (762, 0x33ef0a64442b56d4),
        (58, 0x4902c6bbba365186),
        (59, 0xe66c2985376bedd6),
        (60, 0xcb2c1bdbd9898493),
        (2682, 0x17d7d5859084c817),
        (480, 0xeaabd6a840cf158e),
        (481, 0x8cc49ffca0e30aeb),
        (482, 0x63722ef4af392629),
        (483, 0xcaa401d681a4529f),
        (484, 0xeb879b941b29058d),
        (522, 0x336a60d884470505),
        (523, 0xcbf4abdde57d0296),
        (524, 0x946bf9e47d145e6d),
        (175, 0xa358017e0759fa7b),
        (176, 0x32436ec0504950fd),
        (177, 0x0537b403ddb008c7),
        (178, 0xaa208bd8c55b9f59),
        (82, 0x531f65a055e2c535),
        (83, 0x7786fc96a427e30d),
        (84, 0xbcbf0e3d99fe2365),
        (42, 0x70a14003930714fa),
        (43, 0x1a0c488e21311f49),
        (129, 0xafae8e850d0855d8),
        (1578, 0xb74f703c11083ea5),
        (1579, 0x7474af41f8634ead),
        (1580, 0xa4d306aafeee0846),
        (1581, 0x33431256d0f8c2ee),
        (1582, 0x950d69d876396e41),
        (1583, 0x4a3ebe21edf7fcfc),
        (1584, 0x30a0e396711dffd2),
        (1585, 0x19e161d1000736c8),
        (1586, 0x06bafcd91a9558b2),
        (706, 0x5f2636a97628162c),
        (3442, 0x406bae80f63ccd13),
        (792, 0x258cb91f09a9fbed),
        (3373, 0xe0c227c8a3820ada),
        (3374, 0x892f2387fae422c8),
        (2845, 0x78b7da17d31ad25c),
        (1587, 0xb1a8b078a77e594d),
        (1588, 0xeefe9edaf8f5331c),
        (1589, 0x5eb48b362e7b3f8d),
        (1590, 0x08825b750a5a7060),
        (1591, 0x151675f11b990920),
        (1592, 0x2b05ba009020c48b),
        (1593, 0x3a4415043e105f92),
        (1594, 0xdbf6f1013c1b88c3),
        (3454, 0xb8778b4ccc34b757),
        (3455, 0xe0f7a740af8677d7),
        (71, 0xf693f2f4919a9aa7),
        (72, 0x75bd3cffff424af2),
        (111, 0x7876b05d621687cf),
        (112, 0x65fe34601ae8229f),
        (113, 0x1a53fa4e9fb1b42e),
        (1556, 0x32e19a1e0b30e8ad),
        (1557, 0x1270c79979e2170a),
        (1558, 0x9983d2351e383871),
        (1559, 0x8e8be2d7d1d3d661),
        (1570, 0x31f5602434a6ec2c),
        (759, 0x63ff8af97d9774a3),
        (41, 0xefd5eb94ae85a1d5),
        (128, 0x2264f377738a02bc),
        (1595, 0xa2f4c85404b48a79),
        (1596, 0xc8df40eeffc2900a),
        (1597, 0x5cc79b805a1f7d13),
        (1598, 0x32976a55a60d0a18),
        (44, 0x326698506884ea71),
        (2840, 0x048294ec0cabe1b2),
        (2841, 0x1f4bd4695bf4b132),
        (2842, 0xe184578c1ba2da28),
        (2843, 0xf8ff275c1fac9e4e),
        (2844, 0x235598b75840aad0),
        (63, 0xe5304cd43879598c),
        (64, 0xe8cce5ed1bf25e6b),
        (65, 0x6c4e273dd80d1a49),
        (66, 0xcbcc877292ced154),
        (67, 0xaa0e0f4280863c4f),
        (68, 0x494b6bd49ed25432),
        (69, 0x55b02e1c48020b9a),
        (70, 0x5ec31d7b01be58bd),
        (461, 0x7fab6301f6c58691),
        (462, 0xb335deb7c6c12e81),
        (463, 0x3f9707c85a19eb51),
        (464, 0xf97a1e21b06b00a9),
        (465, 0x10ee3ffcb60671a9),
        (466, 0x61a9b8aca23ce0b1),
        (467, 0x972cd0e821a5d2a1),
        (468, 0x0ace98500536966d),
        (469, 0x2f2d4ec8c5059045),
        (470, 0xe6a932a40cb77f8d),
        (471, 0x98c625b1298e0f80),
        (472, 0x0aaf787c4d90fccd),
        (473, 0x34dd9f95d53407a9),
        (474, 0xa6b07a9f5e93b88c),
        (475, 0x71a26d8a2c3b2f81),
        (476, 0x9e31605204f77bcc),
        (3004, 0xa9d2d337dfcdd29d),
        (3005, 0xe9d8ffbedb4df1c2),
        (3006, 0x8560a73fb0258f0d),
        (3007, 0x3f545173143271d6),
        (2992, 0x85aa3833dd887f04),
        (2993, 0xf7c312cfcd5b680a),
        (2994, 0xd4443db78bad2032),
        (2995, 0xa7762bd5b68f1ec4),
        (2996, 0xf821a20af6cb9b12),
        (2997, 0x3f732b8eb924e95a),
        (2998, 0xe4122d7090a48e69),
        (2999, 0xfbcea4a7df421917),
        (3000, 0x331bbc070b81131e),
        (3001, 0xa6a42df6699d681a),
        (3002, 0x331bbc070b81131e),
        (3003, 0xa6a42df6699d681a),
        (3440, 0xcc1e30f49bcd1325),
        (3441, 0x7af5e283e9ca6a8d),
        (3444, 0x5259acca6b9c0e46),
        (3445, 0x05a3382361d633cc),
        (3446, 0x7f50df78b8609266),
        (3447, 0x70f7ab719be95ada),
        (3448, 0x70f7ab719be95ada),
        (3449, 0x51d88627df287325),
        (3450, 0x51d88627df287325),
        (3451, 0x51d88627df287325),
        (3452, 0x44016a056eee26c5),
        (3453, 0xe0f7a740af8677d7),
        (153, 0xf1e3fcff3acba479),
        (154, 0x8df8f88b7febb483),
        (155, 0x575c2f52755028c4),
        (1601, 0x5d37013e14844746),
        (1602, 0xa10dc39cfdb856bb),
        (1603, 0xce37411eaf58fafd),
        (1604, 0xcf20bef003cc4501),
        (459, 0xd7008079df1fbc3e),
        (460, 0x46608952bbf568f5),
        (1324, 0x670e1d359ceecf7b),
        (1325, 0xecd28eaf29144de5),
        (123, 0x88d214a6362fe90f),
        (1326, 0xe532f779269402d7),
        (1327, 0x88f02a5595908779),
        (1328, 0x9eb0fe0492f6de0d),
        (1329, 0x6486eaa4dfbe7be6),
        (1330, 0x71fc232b97d8623d),
        (1331, 0xf530b7316199c097),
        (1332, 0x5c1f94ec159356cb),
        (1501, 0x7565a5e9c928a4aa),
        (1502, 0x590c6772bef28936),
        (1503, 0xe6161c0e51036116),
        (1504, 0x3712ca6c6892e830),
        (1505, 0x697bcba5cebb8f67),
        (1490, 0xc47b239ae63a3974),
        (1491, 0x0485477b59707346),
        (1492, 0x369457d85c5648f0),
        (1493, 0xffeb38940f528322),
        (1494, 0x338e3451b2498568),
        (1495, 0xc1dc93e66ad9b53b),
        (1498, 0xb4110576bb8922dd),
        (1499, 0xf1ed6ffdeeaaa31d),
        (1500, 0xf531e1e4bfbb8426),
        (1576, 0x72132cb0071a9edc),
        (1577, 0x7250edec5eda13ac),
        (791, 0x717647274566aee9),
        (1599, 0x97862aff928d5ceb),
        (1575, 0x5bec02f1cf4d3c51),
        (2836, 0xe1b48bbde08407bf),
        (2837, 0x180e25fe68f4e9d4),
        (2838, 0x44aa83c7912386ad),
        (2839, 0xc21941e2374c9776),
        (2832, 0x3d65661f6e646f10),
        (2833, 0x7996c57b73be58d4),
        (2834, 0x41b6e7487a716546),
        (2835, 0x4984859dcfa550c4),
        (1600, 0xb0b73a1bc184d80d),
        (2768, 0x898c4ef095c47325),
        (1522, 0x68767d1bee7b6318),
        (1523, 0x4fec6c220b9c08f7),
        (1524, 0x080cb21d1835db55),
        (1525, 0x486f36c5f6d67fa1),
        (1526, 0xdb5df2b103b79d8d),
        (1527, 0xa583c1bd55ea48a0),
        (1528, 0x0378d8e63cfad268),
        (1529, 0xbebe374c145596e1),
        (1530, 0x5229ac4d687672b4),
        (1531, 0x9498ec5f442c86f9),
        (1532, 0xe23b260c7923de92),
        (1533, 0x0eb6a681e1ac45b3),
        (1534, 0xf228ccc9fa9ff82e),
        (1535, 0x05e058efd4240fe5),
        (1536, 0x669a2babfedba8b9),
        (1537, 0xff1725691d2a795c),
        (1538, 0x687198187fcf1bb4),
        (1539, 0x62d23a010b42680d),
        (1540, 0x68a1f3ea0caeefd8),
        (1541, 0x518449e87ab421e5),
        (1542, 0x9b4ab859fe985e5f),
        (1543, 0xa861cb3ccab34282),
        (1544, 0xb4673f3ca522e3ef),
        (1545, 0x5e10b55a2acd26e6),
        (1546, 0xabdcea662eca13e4),
        (1547, 0xa183c2fcc5228b89),
        (1548, 0x31234311e7e68159),
        (1549, 0x08674fc666aa488c),
        (1550, 0xef56689cac55dc65),
        (1551, 0x3fe5807fadac7be0),
        (2639, 0x2bfe19a41f6b3a01),
        (2640, 0x6fcafe7891da5469),
        (2641, 0x1c0e3ace7af28ead),
        (2642, 0xb565290bc89f9be9),
        (2643, 0x530d3333005a60d1),
        (2644, 0x70eed18ae60b5d91),
        (2645, 0x42d759d578f5648d),
        (2646, 0x99ae9e3be7b26d59),
        (2647, 0xba45e22f69f3eb5d),
        (2648, 0x4bf79d1217e152e1),
        (2649, 0xc32480144f501e25),
        (2650, 0xf87d1e846c4fea75),
        (2651, 0xda3898827fccce4f),
        (2652, 0xb271e6e579b19637),
        (2653, 0x374bf5881074e093),
        (2654, 0x77034ea3f076b963),
        (2655, 0xe380a449c957eacc),
        (2656, 0x482a96327f049ba8),
        (2657, 0x994480b0d6d10828),
        (2658, 0x7f47fab6a9f1f0bc),
        (2659, 0x2a58f98deea5e558),
        (2660, 0x14a50aa0342b7d2c),
        (2661, 0x7cb3be7f3e792124),
        (2662, 0x39bb109297ced5e8),
        (2663, 0xd40f6d3295a6e615),
        (2664, 0x383df83c86791e15),
        (2665, 0x47e206ed78ec38c9),
        (2666, 0x82d6b487022bdebd),
        (2667, 0x48ca90a09883fb79),
        (2668, 0x5ebc1177339e59e1),
        (2669, 0x1dae256d9d3ccd29),
        (2670, 0x8e9db14ff3559661),
        (2671, 0x0280b01c782e83a1),
        (2672, 0xe17601a00cbe5411),
        (2673, 0xb1396f5198e76479),
        (2674, 0xb54e0eb796e6c8dd),
        (2675, 0x3e62f5eb1f41551d),
        (2676, 0x185a3bc5b058ff59),
        (2677, 0x503462a685ad909d),
        (2678, 0x0d34a23b7ea4eef9),
        (1506, 0x506ab9412345ed52),
        (1507, 0x02fe8f1c20f2ff31),
        (1508, 0x6a16e03e6d7c12d3),
        (1509, 0x673668fba214e128),
        (1567, 0x9c54ca289455c1a3),
        (3443, 0x6fac2f71524ef0fe),
        (2679, 0x87c16b5959bd7f50),
        (2680, 0xe4f19cbd4fd82c29),
        (2681, 0x0c6755a053073d90),
        (1475, 0xcd33b0228e0c2508),
        (1476, 0xab1e895092dc492c),
        (1477, 0x569561ff4d353bd1),
        (1478, 0x21981e96f97b1b8f),
        (1479, 0x030f12ae177c0974),
        (1480, 0xf9af428bf376d0b4),
        (1481, 0xca75119e3e685805),
        (1482, 0xf206311a920153e7),
        (1568, 0x432c26bbd75ed7d1),
        (1569, 0x7771636c9a6e06cc),
        (2612, 0xe65da621fafeaf54),
        (4066, 0x97d0452fd8f75976),
    ];

    /// The measurable rules of `docs/art-and-ui.md` for natural materials.
    #[test]
    fn natural_materials_follow_the_art_guide() {
        // Every broken rule is listed, not just the first.
        let mut broken = Vec::new();
        let mut check = |ok: bool, what: String| {
            if !ok {
                broken.push(what);
            }
        };
        let stone = tile(tex::STONE);
        let l = mean_luminance(&stone);
        check((0.17..=0.24).contains(&l), format!("stone mean luminance {l}"));
        check(within(&stone, 0.25) >= 0.9, format!("stone contrast {}", within(&stone, 0.25)));
        for layer in [
            tex::STONE,
            tex::ANDESITE,
            tex::DIORITE,
            tex::GRANITE,
            tex::TUFF,
            tex::DIRT,
            tex::SAND,
            tex::RED_SAND,
            tex::GRASS_TOP,
            tex::LEAVES,
            tex::SPRUCE_LEAVES,
            tex::BIRCH_LEAVES,
            tex::JUNGLE_LEAVES,
            tex::ACACIA_LEAVES,
            t::DARK_OAK_LEAVES,
        ] {
            check(
                longest_run(&tile(layer)) <= 4,
                format!("layer {layer}: a straight run of {}", longest_run(&tile(layer))),
            );
        }
        for layer in [tex::DIRT, tex::SAND, tex::RED_SAND] {
            let w = within(&tile(layer), 0.18);
            check(w >= 0.9, format!("layer {layer}: only {w} of texels within 18 % of the mean"));
        }
        // Gravel's pebbles need their dark gaps: three quarters within ±25 %.
        let w = within(&tile(tex::GRAVEL), 0.25);
        check(w >= 0.75, format!("gravel: only {w} of texels within 25 % of the mean"));
        // No pattern at a distance: every 4 x 4 block is close to the tile's mean.
        for layer in [
            tex::STONE,
            tex::ANDESITE,
            tex::DIORITE,
            tex::GRANITE,
            tex::TUFF,
            tex::DIRT,
            tex::SAND,
            tex::RED_SAND,
            tex::GRASS_TOP,
            tex::SANDSTONE_TOP,
            tex::CLAY,
            t::COARSE_DIRT,
            t::PODZOL_TOP,
            t::MYCELIUM_TOP,
            tex::CALCITE,
            tex::BLACKSTONE,
            t::MUD,
            tex::TERRACOTTA,
            tex::STAINED_TERRACOTTA,
        ] {
            let d = block_deviation(&tile(layer));
            check(d <= 0.08, format!("layer {layer}: a 4 x 4 block {d} off the mean"));
        }
        let grass = tile(tex::GRASS_TOP);
        check(saturation(mean_rgb(&grass)) <= 0.55, format!("grass saturation {}", saturation(mean_rgb(&grass))));
        let darker = 1.0 - mean_luminance(&tile(tex::LEAVES)) / mean_luminance(&grass);
        check((0.10..=0.25).contains(&darker), format!("oak leaves are {darker} darker than grass"));
        for layer in [
            tex::LEAVES,
            tex::SPRUCE_LEAVES,
            tex::BIRCH_LEAVES,
            tex::JUNGLE_LEAVES,
            tex::ACACIA_LEAVES,
            t::DARK_OAK_LEAVES,
        ] {
            let holes = tile(layer).iter().filter(|p| p[3] == 0).count() as f64 / (N * N) as f64;
            check((0.08..=0.15).contains(&holes), format!("layer {layer}: {holes} of texels cut out"));
        }
        // No frame: the border ring is no darker than the tile. Deepslate's
        // strata run along whole rows, so only its side columns count.
        for layer in [tex::STONE, tex::ANDESITE, tex::DIRT, tex::SAND, tex::GRASS_TOP, tex::DEEPSLATE] {
            let t = tile(layer);
            let edge = |i: usize| matches!(i % N, 0 | 15) || layer != tex::DEEPSLATE && matches!(i / N, 0 | 15);
            let ring: Vec<Rgba> = (0..N * N).filter(|&i| edge(i)).map(|i| t[i]).collect();
            let ring = ring.iter().map(|&p| luminance(p)).sum::<f64>() / ring.len() as f64;
            check(ring >= 0.9 * mean_luminance(&t), format!("layer {layer}: a darker frame ({ring})"));
        }
        assert!(broken.is_empty(), "the art guide's rules are broken:\n{}", broken.join("\n"));
    }

    /// From eight blocks away the 2 x 2 mip still tells stone, cobblestone,
    /// andesite, dirt and gravel apart by mean colour.
    #[test]
    fn far_mips_keep_the_families_apart() {
        let families = [tex::STONE, tex::COBBLESTONE, tex::ANDESITE, tex::DIRT, tex::GRAVEL];
        let colour: Vec<[f64; 3]> = families.iter().map(|&l| mean_rgb(&tile(l))).collect();
        let lum = |c: [f64; 3]| luminance([c[0] as u8, c[1] as u8, c[2] as u8, 255]);
        for i in 0..families.len() {
            for j in i + 1..families.len() {
                let (a, b) = (colour[i], colour[j]);
                let apart = (lum(a) - lum(b)).abs() / lum(a).max(lum(b)) >= 0.04;
                let grey = saturation(a) < 0.12 || saturation(b) < 0.12;
                let hue_apart = !grey && (hue(a) - hue(b)).abs().min(360.0 - (hue(a) - hue(b)).abs()) >= 15.0;
                assert!(apart || hue_apart, "layers {} and {} look alike from afar", families[i], families[j]);
            }
        }
    }

    /// Ores keep their background's texels; only the mineral clusters change.
    #[test]
    fn ores_keep_the_stone_and_deepslate_texels() {
        for (ores, background) in [(STONE_ORES, tex::STONE), (DEEPSLATE_ORES, tex::DEEPSLATE)] {
            let bg = tile(background);
            for (i, &layer) in ores.iter().enumerate() {
                let t = tile(layer);
                let changed: Vec<usize> = (0..N * N).filter(|&k| t[k] != bg[k]).collect();
                assert!((8..=40).contains(&changed.len()), "layer {layer}: {} texels changed", changed.len());
                let palette = ORES[i].0;
                for k in changed {
                    let [r, g, b, a] = t[k];
                    assert!(a == 255 && palette.contains(&[r, g, b]), "layer {layer}: texel {k} isn't mineral");
                }
            }
        }
    }

    /// Block icons, held blocks and dropped blocks draw these faces: each
    /// redrawn block shows an isometric cube of its own layers.
    #[test]
    fn icons_draw_the_redrawn_faces() {
        let drawn: Vec<u16> = std::iter::once(tex::STONE).chain(layers()).collect();
        for block in [
            Block::STONE,
            Block::DIRT,
            Block::GRASS,
            Block::SAND,
            Block::GRAVEL,
            Block::COBBLESTONE,
            Block::COAL_ORE,
            Block::DIAMOND_ORE,
            Block::LOG,
            Block::SPRUCE_LOG,
            Block::BIRCH_LOG,
            Block::LEAVES,
            Block::SPRUCE_LEAVES,
            Block::PLANKS,
            Block::BIRCH_PLANKS,
        ] {
            check_icon(block, &drawn);
        }
        // Second round, by name.
        for name in [
            "sandstone",
            "red sandstone",
            "cut sandstone",
            "coarse dirt",
            "mycelium",
            "rooted dirt",
            "clay",
            "bedrock",
            "polished andesite",
            "polished deepslate",
            "cobbled deepslate",
            "stripped oak log",
            "stripped cherry log",
            "pale oak log",
            "pale oak planks",
            "cherry leaves",
        ] {
            check_icon(Block::from_name(name).unwrap_or_else(|| panic!("no block {name}")), &drawn);
        }
    }

    fn check_icon(block: Block, drawn: &[u16]) {
        assert!(!block.flat_icon(), "{block:?} has a flat icon");
        let faces = block.info().tex;
        assert!(faces.iter().all(|f| drawn.contains(f)), "{block:?} faces {faces:?} aren't all redrawn");
    }

    /// The tile cache stays within the budget recorded in the asset
    /// manifest, and the block array keeps its size: no layers were added.
    #[test]
    fn texture_memory_stays_within_the_recorded_budget() {
        let budget = &voxelcraft::assets::CATALOGUE["procedural_resources"]["material_art"];
        let cache = TILES.len() * std::mem::size_of::<Tile>();
        assert!(cache as u64 <= budget["cpu_cache_bytes"].as_u64().unwrap(), "{cache} bytes of cached tiles");
        let per_layer: usize = (0..super::super::MIP_LEVELS).map(|l| (N >> l).pow(2) * 4).sum();
        let array = tex::COUNT as usize * per_layer;
        assert_eq!(array as u64, budget["block_array_bytes"].as_u64().unwrap(), "the block texture array changed size");
        assert_eq!(super::super::generate_mips().iter().map(Vec::len).sum::<usize>(), array);
    }

    /// Writes a review sheet of every redrawn tile (12x) and a 3 x 3 repeat
    /// (4x) to `$VC_MATERIAL_SHEET`; run with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn material_sheet() {
        let Ok(path) = std::env::var("VC_MATERIAL_SHEET") else { return };
        let started = std::time::Instant::now();
        let cache = build();
        let build_time = started.elapsed();
        let started = std::time::Instant::now();
        let mips = super::super::generate_mips();
        println!(
            "material tiles: {} built in {build_time:?}; generate_mips: {} bytes in {:?}",
            cache.len(),
            mips.iter().map(Vec::len).sum::<usize>(),
            started.elapsed()
        );
        let layers: Vec<u16> = std::iter::once(tex::STONE).chain(layers()).collect();
        let (cols, cell) = (4, 16 * 12 + 8 + 16 * 3 * 4 + 16);
        let rows = layers.len().div_ceil(cols);
        let (w, h) = (cols * cell, rows * (16 * 12 + 8));
        let mut img = vec![30u8; w * h * 4];
        for (i, &layer) in layers.iter().enumerate() {
            let t = tile(layer);
            let (ox, oy) = ((i % cols) * cell, (i / cols) * (16 * 12 + 8));
            let mut put = |x: usize, y: usize, p: Rgba| {
                let k = ((oy + y) * w + ox + x) * 4;
                img[k..k + 4].copy_from_slice(&if p[3] == 0 { [60, 66, 78, 255] } else { p });
            };
            for y in 0..16 * 12 {
                for x in 0..16 * 12 {
                    put(x, y, t[(y / 12) * N + x / 12]);
                }
            }
            for y in 0..16 * 3 * 4 {
                for x in 0..16 * 3 * 4 {
                    put(16 * 12 + 8 + x, y, t[((y / 4) % N) * N + (x / 4) % N]);
                }
            }
            println!(
                "{layer}: luminance {:.3}, run {}, block deviation {:.3}, within 18 % {:.2}, 25 % {:.2}",
                mean_luminance(&t),
                longest_run(&t),
                block_deviation(&t),
                within(&t, 0.18),
                within(&t, 0.25)
            );
        }
        // The generator's stone, for authoring `assets/textures/stone.source.json`.
        let stone = natural(
            2,
            &[[84, 84, 87], [103, 103, 106], [116, 116, 119], [125, 125, 128], [134, 134, 137], [147, 147, 150]],
        );
        for row in stone.grid() {
            println!("stone row {}", row.iter().map(|c| c.to_string()).collect::<String>());
        }
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img).unwrap();
    }
}
