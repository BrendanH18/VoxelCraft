//! v0.18 Track 3: original pixel art for the common natural materials and
//! woods (see the material rules in `docs/art-and-ui.md`).
//!
//! Each tile is a grid of palette indices built from integer maths, so it is
//! the same on every platform: soft patches of three base shades, clusters
//! of 2-5 texels placed on a stratified grid (cluster `k` starts in its own
//! 4 x 4 cell) so they never clump, light from the top left, and no run of
//! one shade longer than four texels except deliberate grain and strata.
//! Every tile wraps in both directions. Tiles are built once, on first use,
//! and kept (about 105 KiB).
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
        let mut g = base_grid(self.seed, self.cells, self.jitter, self.thresholds, self.broad);
        break_runs(&mut g, 2, 4, 4);
        let (n, size, dark, deep) = self.pits;
        pits(&mut g, self.seed, n, size, dark, deep, 9);
        let (n, size, light, shadow) = self.bumps;
        bumps(&mut g, self.seed, n, size, light, shadow, 7);
        // Accents included: clusters that meet mustn't line up into a streak.
        break_runs(&mut g, 1, 5, 4);
        balance(&mut g, self.palette, 2, 4, self.seed);
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
    let mut g = DIRT.grid();
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
    out
}

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
    // The ores keep the stone asset's texels (its procedural fallback when
    // the asset is absent) and deepslate's.
    let mut stone = [[0; 4]; N * N];
    for (i, p) in stone.iter_mut().enumerate() {
        *p = super::pixel(tex::STONE, i % N, i / N);
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
