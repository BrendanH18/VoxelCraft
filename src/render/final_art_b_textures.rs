//! v1.0 Track 10b: the follow-up art round, in the style the owner approved
//! for Track 10 (see `docs/art-and-ui.md`). Original pixel art drawn with
//! the same primitives: the flat ground plants (lily pad, pink petals,
//! wildflowers, leaf litter), vines and glow lichen, the corals (blocks,
//! plants, fans and wall fans, alive and dead) and sea pickles, the bamboo
//! stalk and its leaves, the potted torchflower and the ender chest. Every
//! tile replaces its existing layer; no layer is added. Light comes from the
//! top left: every leaf, petal, lobe and ridge is lit at its upper left and
//! shaded at its lower right.

use super::{Img, Px, Shades, blob, hu, leaf, ramp, shades, stalk};
use super::{N, Tile};
use crate::world::biome_plants::t as bp;
use crate::world::cave_blocks::t as cb;
use crate::world::overworld_blocks::t as ob;
use crate::world::utilities::t as ut;

/// Five shades of one colour: dark, shaded, mid, lit, highlight.
type Five = [Px; 5];

fn five(c: Px) -> Five {
    ramp(c, [600, 770, 900, 1040, 1190])
}

/// Sets a texel, wrapping at the tile's edges: for tiles that repeat
/// across neighbouring blocks (vines, lichen, coral blocks).
fn wset(img: &mut Img, x: i32, y: i32, c: Px) {
    img.set(x.rem_euclid(N as i32), y.rem_euclid(N as i32), c);
}

/// A stamp that wraps at the tile's edges.
fn wstamp(img: &mut Img, x: i32, y: i32, rows: &[&str], key: &[(char, Px)]) {
    for (j, row) in rows.iter().enumerate() {
        for (i, ch) in row.chars().enumerate() {
            if let Some(&(_, c)) = key.iter().find(|(k, _)| *k == ch) {
                wset(img, x + i as i32, y + j as i32, c);
            }
        }
    }
}

/// The stamp keys for five shades: `h` highlight, `l` lit, `m` mid, `s`
/// shaded, `d` dark.
fn keys(s: &Five) -> [(char, Px); 5] {
    [('h', s[4]), ('l', s[3]), ('m', s[2]), ('s', s[1]), ('d', s[0])]
}

/// The four quadrants a segmented ground plant fills, one per segment, in
/// the order its segments are added.
const QUADRANTS: [(i32, i32); 4] = [(0, 0), (8, 0), (8, 8), (0, 8)];

/// Draws into one 8 x 8 quadrant only: texels outside it are dropped, so
/// each segment's quadrant stays apart from the others.
struct Quad<'a> {
    img: &'a mut Img,
    ox: i32,
    oy: i32,
}

impl Quad<'_> {
    fn set(&mut self, x: i32, y: i32, c: Px) {
        if (0..8).contains(&x) && (0..8).contains(&y) {
            self.img.set(self.ox + x, self.oy + y, c);
        }
    }

    fn stamp(&mut self, x: i32, y: i32, rows: &[&str], key: &[(char, Px)]) {
        for (j, row) in rows.iter().enumerate() {
            for (i, ch) in row.chars().enumerate() {
                if let Some(&(_, c)) = key.iter().find(|(k, _)| *k == ch) {
                    self.set(x + i as i32, y + j as i32, c);
                }
            }
        }
    }

    fn line(&mut self, (x0, y0): (i32, i32), (x1, y1): (i32, i32), c: Px) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for t in 0..=steps {
            self.set(x0 + (x1 - x0) * t / steps, y0 + (y1 - y0) * t / steps, c);
        }
    }
}

// ---- Flat ground plants ---------------------------------------------------

/// A lily pad seen from above: a round pad with a notch, lit at its upper
/// left, a darker rim, and raised veins running out from the middle.
fn lily_pad() -> Tile {
    let s = five([66, 128, 50]);
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            // Half texels from the centre.
            let (dx, dy) = (2 * x - 15, 2 * y - 15);
            let d2 = dx * dx + dy * dy;
            let notch = dx > 0 && dy > 0 && (dx - dy).abs() * 5 < (dx + dy) * 2;
            if d2 > 221 || notch {
                continue;
            }
            let side = dx + dy;
            let mut k: usize = if d2 > 140 {
                if side < -4 { 2 } else { 0 }
            } else if side < -12 {
                3
            } else if side > 12 {
                1
            } else {
                2
            };
            let vein = x == 7 || y == 7 || x == y || x + y == 15;
            if vein && d2 > 4 && d2 <= 140 {
                k = (k + 1).min(4);
            }
            if d2 <= 2 {
                k = 0;
            }
            if hu(1599, x as usize, y as usize, 96).is_multiple_of(13) && k > 0 && d2 <= 140 {
                k -= 1;
            }
            img.set(x, y, s[k]);
        }
    }
    // The notch's edges: lit where the pad faces up-left across it.
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            if img.get(x, y).is_some() && img.get(x + 1, y + 1).is_none() && x > 8 && y > 8 {
                img.set(x, y, s[0]);
            }
        }
    }
    img.tile()
}

/// A small four-petalled bloom, centred on `(x, y)`.
const BLOOM: [&str; 5] = ["  l  ", " hlm ", "lhcms", " mms ", "  s  "];

/// Pink petals seen from above: clusters of four-petalled blooms joined by
/// thin stems, with a few fallen petals.
fn pink_petals() -> Tile {
    let p = five([236, 146, 186]);
    let g = shades([70, 120, 46]);
    let centre = [184, 74, 126];
    let mut img = Img::new();
    for (a, b) in [((3, 3), (6, 8)), ((6, 8), (2, 12)), ((10, 2), (12, 10)), ((12, 10), (8, 13)), ((6, 8), (12, 10))] {
        img.line(a, b, g[1]);
    }
    for (x, y) in [(4, 6), (9, 10), (11, 6), (4, 10)] {
        img.set(x, y, g[2]);
        img.set(x + 1, y, g[0]);
    }
    let key = [('h', p[4]), ('l', p[3]), ('m', p[2]), ('s', p[1]), ('c', centre)];
    for (x, y) in [(3, 3), (10, 2), (6, 8), (12, 10), (2, 12), (8, 13)] {
        img.stamp(x - 2, y - 2, &BLOOM, &key);
    }
    for (x, y) in [(14, 4), (1, 7), (14, 14), (5, 15)] {
        img.set(x, y, p[3]);
        img.set(x + 1, y, p[1]);
    }
    img.tile()
}

/// Wildflowers with `count` segments (1-4): each segment fills one
/// quadrant with stems, leaves and small blooms in yellow, white and a pale
/// violet. A higher count keeps every texel of the lower ones.
fn wildflowers(count: usize) -> Tile {
    let g = five([72, 126, 48]);
    let blooms = [five([238, 204, 58]), five([232, 230, 222]), five([186, 160, 226])];
    let mut img = Img::new();
    // Per quadrant: stems (from, to), leaves, then blooms (x, y, colour).
    type Seg = (&'static [((i32, i32), (i32, i32))], &'static [(i32, i32)], &'static [(i32, i32, usize)]);
    const SEGMENTS: [Seg; 4] = [
        (&[((1, 7), (2, 2)), ((4, 7), (5, 4))], &[(2, 5), (4, 6)], &[(2, 2, 0), (5, 4, 1)]),
        (&[((2, 7), (2, 3)), ((5, 7), (6, 1))], &[(3, 5), (5, 4)], &[(2, 3, 1), (6, 1, 0)]),
        (&[((1, 7), (1, 2)), ((4, 7), (5, 5)), ((6, 7), (6, 3))], &[(2, 6)], &[(1, 2, 0), (5, 5, 2), (6, 3, 1)]),
        (&[((3, 7), (2, 3)), ((5, 7), (6, 5))], &[(4, 5), (1, 6)], &[(2, 3, 2), (6, 5, 0)]),
    ];
    for (k, &(ox, oy)) in QUADRANTS.iter().enumerate().take(count) {
        let (stems, leaves, heads) = SEGMENTS[k];
        let mut q = Quad { img: &mut img, ox, oy };
        for &(a, b) in stems {
            q.line(a, b, g[1]);
        }
        for &(x, y) in leaves {
            q.stamp(x, y, &["lm", "ms"], &keys(&g));
        }
        for &(x, y, c) in heads {
            let s = &blooms[c];
            q.stamp(x - 1, y - 1, &[" l ", "lhm", " s "], &keys(s));
            q.set(x, y, [226, 170, 40]);
        }
    }
    img.tile()
}

/// One fallen leaf: `dir` 0 lying across, 1 lengthways, 2 and 3 the two
/// diagonals; each with a darker midrib and lit on its upper left.
const FALLEN: [&[&str]; 4] = [
    &[" hlm ", "lddds", " mss "],
    &[" h ", "ldm", "ldm", "mds", " s "],
    &["hl  ", "ldm ", " mds", "  ss"],
    &["  hl", " ldm", "lds ", "ms  "],
];

/// Leaf litter with `count` segments (1-4): fallen leaves in browns and
/// ambers, a quadrant per segment, nested like the wildflowers.
fn leaf_litter(count: usize) -> Tile {
    let colours = [[168, 104, 44], [140, 86, 40], [184, 134, 54], [118, 100, 44]];
    let mut img = Img::new();
    const LEAVES: [&[(i32, i32, usize, usize)]; 4] = [
        &[(0, 0, 2, 0), (3, 1, 0, 1), (1, 3, 1, 2), (4, 4, 3, 3)],
        &[(0, 1, 0, 2), (5, 0, 1, 0), (0, 4, 3, 1), (4, 4, 2, 3)],
        &[(1, 0, 3, 3), (3, 2, 0, 0), (0, 4, 0, 1), (4, 4, 2, 2)],
        &[(0, 0, 1, 1), (2, 0, 2, 3), (3, 5, 0, 2), (4, 1, 3, 0)],
    ];
    for (k, &(ox, oy)) in QUADRANTS.iter().enumerate().take(count) {
        let mut q = Quad { img: &mut img, ox, oy };
        for &(x, y, dir, c) in LEAVES[k] {
            q.stamp(x, y, FALLEN[dir], &keys(&five(colours[c])));
        }
    }
    img.tile()
}

// ---- Vines and glow lichen ---------------------------------------------------

/// A gentle sway, one period per tile, so stems join across stacked blocks.
const SWAY: [i32; 16] = [0, 0, 1, 1, 1, 1, 0, 0, 0, -1, -1, -1, -1, 0, 0, 0];

/// Vines: three swaying stems with lobed leaves on alternate sides; the
/// tile repeats both ways, so walls of vine join up.
fn vines() -> Tile {
    let stem = shades([84, 96, 44]);
    let s = five([62, 112, 40]);
    let mut img = Img::new();
    let lobe = ["hllm", "lmms", " mss"];
    for (k, x0) in [2, 7, 12].into_iter().enumerate() {
        let x_at = |y: i32| x0 + SWAY[((y + 5 * k as i32).rem_euclid(16)) as usize];
        for y in 0..N as i32 {
            wset(&mut img, x_at(y), y, stem[if (y + k as i32) % 4 == 0 { 2 } else { 1 }]);
        }
        for (i, y) in [1, 6, 11].into_iter().enumerate() {
            let y = y + k as i32;
            let left = (i + k) % 2 == 0;
            let x = x_at(y);
            wstamp(&mut img, if left { x - 4 } else { x + 1 }, y - 1, &lobe, &keys(&s));
        }
    }
    img.tile()
}

/// Glow lichen: irregular grey-green patches, lit at their upper left with
/// a dark rim below, each with glowing knots; the gaps show the wall.
fn glow_lichen() -> Tile {
    let s = five([112, 132, 112]);
    let glow = ramp([236, 214, 148], [760, 1000, 1080]);
    let mut img = Img::new();
    // Centre and radius (texels) of each patch.
    const PATCHES: [(i32, i32, i32); 6] = [(3, 3, 4), (12, 3, 3), (7, 10, 4), (14, 12, 3), (2, 13, 2), (11, 15, 2)];
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let mut best: Option<(i32, i32, i32)> = None;
            for &(cx, cy, r) in &PATCHES {
                let wrap = |d: i32| (d + 8).rem_euclid(16) - 8;
                let (dx, dy) = (2 * wrap(x - cx), 2 * wrap(y - cy));
                let d2 = dx * dx + dy * dy;
                let rough = (hu(2768, x as usize, y as usize, 97) % 3) as i32 * 6;
                // The patch this texel lies deepest in (distance over radius).
                if d2 + rough <= 4 * r * r && best.is_none_or(|b| d2 * b.0 * b.0 < b.2 * r * r) {
                    best = Some((r, dx + dy, d2));
                }
            }
            let Some((r, side, d2)) = best else { continue };
            let edge = d2 > 4 * r * r - 8 * r;
            let k = if edge && side > 0 {
                0
            } else if side < -r {
                3
            } else if side > r / 2 {
                1
            } else {
                2
            };
            img.set(x, y, s[k]);
        }
    }
    // A glowing knot in each patch, two texels across in the larger ones.
    for &(cx, cy, r) in &PATCHES {
        if r >= 3 {
            wset(&mut img, cx - 1, cy - 1, glow[2]);
            wset(&mut img, cx, cy - 1, glow[1]);
            wset(&mut img, cx - 1, cy, glow[1]);
            wset(&mut img, cx, cy, glow[0]);
        } else {
            wset(&mut img, cx, cy, glow[1]);
        }
    }
    img.tile()
}

// ---- Corals and sea pickles ------------------------------------------------

/// Tube, brain, bubble, fire and horn coral, as before.
const CORAL: [Px; 5] = [[52, 92, 208], [204, 84, 160], [158, 40, 186], [188, 40, 50], [214, 196, 70]];
/// Dead coral: one bleached grey for every kind.
const DEAD: Px = [138, 130, 124];

fn coral_shades(colour: usize, dead: bool) -> Five {
    if dead { ramp(DEAD, [680, 800, 900, 1000, 1110]) } else { five(CORAL[colour]) }
}

/// A coral block's relief as shade indices (0 dark to 4 highlight). Each
/// kind has its own structure and repeats seamlessly: packed tubes, brain
/// ridges, bubbles, flame streaks and overlapping horn plates.
fn coral_block(colour: usize) -> [[u8; N]; N] {
    let mut g = [[2u8; N]; N];
    let wrap = |d: i32| (d + 8).rem_euclid(16) - 8;
    for (y, row) in g.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            let (xi, yi) = (x as i32, y as i32);
            let h = hu(1522 + colour as u32, x, y, 98);
            *v = match colour {
                // Tubes on a staggered lattice: a dark mouth, its walls
                // shaded at the upper left (in shadow) and lit at the lower
                // right, crevices between.
                0 => {
                    let (j, shift) = (yi / 4, 2 * (yi / 4 % 2));
                    let cx = 4 * (xi - 1 - shift + 2).div_euclid(4) + 1 + shift;
                    let cy = 4 * j + 1;
                    let (dx, dy) = (wrap(xi - cx), wrap(yi - cy));
                    match dx * dx + dy * dy {
                        0 => 0,
                        1 | 2 => {
                            if dx + dy > 0 {
                                4
                            } else if dx + dy < 0 {
                                2
                            } else {
                                3
                            }
                        }
                        _ => 1 + h.is_multiple_of(3) as u8,
                    }
                }
                // Brain: ridges two texels wide that meander and turn at
                // the quadrants, lit on their upper or left side.
                1 => {
                    let u = if (x / 8 + y / 8) % 2 == 0 { xi + SWAY[y] } else { yi + SWAY[x] };
                    [3, 2, 1, 0][u.rem_euclid(4) as usize] + h.is_multiple_of(11) as u8
                }
                // Bubbles: round, lit at the upper left with a glint,
                // shaded and rimmed at the lower right, packed together.
                2 => {
                    const BUBBLES: [(i32, i32, i32); 8] =
                        [(3, 3, 3), (10, 2, 2), (8, 8, 3), (14, 9, 2), (3, 11, 3), (10, 14, 2), (14, 3, 1), (6, 15, 1)];
                    let mut best = None;
                    for &(cx, cy, r) in &BUBBLES {
                        let (dx, dy) = (wrap(xi - cx), wrap(yi - cy));
                        if dx * dx + dy * dy <= r * r + r && best.is_none() {
                            best = Some((dx, dy, r));
                        }
                    }
                    match best {
                        Some((dx, dy, r)) if dx == -(r + 1) / 2 && dy == dx && r > 1 => 4,
                        Some((dx, dy, r)) if dx + dy >= r => 1,
                        Some((dx, dy, _)) if dx + dy < 0 => 3,
                        Some(_) => 2,
                        None => 0,
                    }
                }
                // Fire: flame streaks leaning as they rise, bright at
                // their tips, with dark pores between.
                3 => {
                    let u = (2 * xi + SWAY[y] * 2 + yi / 3).rem_euclid(5);
                    let k = [3, 2, 2, 1, 0][u as usize];
                    if k == 3 && h.is_multiple_of(7) { 4 } else { k }
                }
                // Horn: rows of overlapping plates, each lit along its top
                // edge, shaded at its foot and right side.
                _ => {
                    // Rows of plates rippling with the sway, staggered.
                    let yy = (yi + SWAY[x]).rem_euclid(16);
                    let sx = (xi + 2 * (yy / 4 % 2)).rem_euclid(4);
                    match (yy % 4, sx) {
                        (0, 0 | 3) => 1,
                        (0, 1) => 4,
                        (0, _) => 3,
                        (1, 0) => 3,
                        (1, 3) | (2, 3) => 1,
                        (3, _) => 0,
                        _ => 2 - h.is_multiple_of(7) as u8,
                    }
                }
            };
        }
    }
    g
}

fn paint_grid(g: &[[u8; N]; N], s: &Five) -> Tile {
    std::array::from_fn(|i| {
        let c = s[g[i / N][i % N].min(4) as usize];
        [c[0], c[1], c[2], 255]
    })
}

/// A coral plant: five branching shapes, one per kind, rising from the
/// bottom middle.
fn coral_plant(colour: usize, s: &Five) -> Img {
    let mut img = Img::new();
    let four: Shades = [s[0], s[1], s[3], s[4]];
    match colour {
        // Tube: five tubes fanning from the base, each a dark mouth under a
        // lit lip.
        0 => {
            for (tx, top) in [(2, 6), (5, 3), (8, 5), (11, 2), (13, 7)] {
                let base = if tx < 8 { 6 } else { 8 };
                img.line((base, 15), (tx, top + 2), s[3]);
                img.line((base + 1, 15), (tx + 1, top + 2), s[1]);
                img.set(tx, top, s[4]);
                img.set(tx + 1, top, s[3]);
                img.set(tx, top + 1, s[0]);
                img.set(tx + 1, top + 1, s[0]);
            }
        }
        // Brain: rounded lobes on short stalks.
        1 => {
            for (lx, ly) in [(4, 8), (8, 4), (12, 7), (6, 11), (11, 11)] {
                stalk(&mut img, &[(8, 15), (lx, ly)], &four);
            }
            for (i, (lx, ly)) in
                [(4.5, 8.0), (8.5, 4.5), (12.0, 7.5), (6.0, 11.0), (11.0, 11.5)].into_iter().enumerate()
            {
                blob(&mut img, (lx, ly), (2.4, 2.2), &four, 256, 1990 + i as u32);
            }
        }
        // Bubble: thin stalks ending in round bubbles with a glint.
        2 => {
            for (bx, by) in [(3, 5), (7, 2), (11, 4), (5, 9), (12, 10)] {
                stalk(&mut img, &[(7, 15), (7, 11), (bx, by)], &four);
            }
            for (bx, by) in [(3, 5), (7, 2), (11, 4), (5, 9), (12, 10)] {
                blob(&mut img, (bx as f32 + 0.5, by as f32 + 0.5), (1.8, 1.8), &four, 256, 1992);
                img.set(bx - 1, by - 1, s[4]);
            }
        }
        // Fire: forked flame-like branches with bright tips.
        3 => {
            for (a, b) in [
                ((7, 15), (7, 9)),
                ((7, 9), (3, 4)),
                ((7, 9), (11, 3)),
                ((7, 11), (12, 8)),
                ((4, 5), (2, 1)),
                ((4, 5), (6, 2)),
                ((10, 4), (9, 0)),
                ((10, 4), (13, 1)),
                ((12, 8), (14, 5)),
                ((6, 12), (2, 9)),
            ] {
                img.line(a, b, s[2]);
                img.line((a.0 + 1, a.1), (b.0 + 1, b.1), s[1]);
            }
            for (x, y) in [(2, 1), (6, 2), (9, 0), (13, 1), (14, 5), (2, 9)] {
                img.set(x, y, s[4]);
            }
        }
        // Horn: thick antlers with blunt, flattened tips.
        _ => {
            for (a, b) in
                [((7, 15), (7, 8)), ((7, 9), (3, 5)), ((7, 8), (8, 2)), ((7, 10), (12, 6)), ((12, 6), (13, 10))]
            {
                img.line(a, b, s[3]);
                img.line((a.0 + 1, a.1), (b.0 + 1, b.1), s[1]);
                img.line((a.0, a.1 + 1), (b.0, b.1 + 1), s[2]);
            }
            for (x, y) in [(2, 4), (7, 1), (12, 5), (13, 10)] {
                img.stamp(x, y, &["hl", "ms"], &keys(s));
            }
        }
    }
    img
}

/// A coral fan spreading from a short stalk at the bottom middle, `height`
/// rows tall: a pie-slice of webbing (mid on the left, shaded on the right,
/// with lace holes) braced by lit ribs, its rim scalloped between the rib
/// tips; how wide it opens and how many ribs it has differ by kind.
fn fan(colour: usize, s: &Five, height: i32) -> Img {
    // Half-angle as width per unit of height (tenths), and ribs.
    let (width, ribs) = [(8, 5), (14, 7), (11, 6), (12, 7), (14, 6)][colour];
    let h2 = 2 * height;
    // Half texels from the base (the bottom edge's middle), across and up.
    let at = |x: i32, y: i32| (2 * x - 15, 31 - 2 * y);
    let inside = |(dx, up): (i32, i32)| up >= 2 && dx * dx + up * up <= h2 * h2 && dx.abs() * 10 <= up * width + 10;
    // Ribs, straight from the base to the rim, evenly spread. Square roots
    // are exact in IEEE arithmetic, so every platform draws the same ribs.
    let mut rib = Img::new();
    let sine = width as f32 / ((100 + width * width) as f32).sqrt();
    for i in 0..ribs {
        let f = sine * (2 * i - (ribs - 1)) as f32 / (ribs - 1) as f32;
        let (ex, eu) = (h2 as f32 * f, h2 as f32 * (1.0 - f * f).sqrt());
        let tip = (((ex + 15.0) / 2.0).round() as i32, ((31.0 - eu) / 2.0).round() as i32);
        let base = (7 + (tip.0 > 7) as i32, 15);
        rib.line(base, tip, if tip.0 <= 7 { s[4] } else { s[3] });
        rib.set(tip.0, tip.1, s[4]);
    }
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let (dx, up) = at(x, y);
            if !inside((dx, up)) {
                continue;
            }
            let near_rib = (-1..=1).any(|i| (-1..=1).any(|j| rib.get(x + i, y + j).is_some()));
            let rim = dx * dx + up * up > (h2 - 4) * (h2 - 4);
            let h = hu(2000 + colour as u32, x as usize, y as usize, 99);
            let ragged = match colour {
                3 => rim && h.is_multiple_of(3),
                2 => rim && (x + y) % 3 == 0,
                _ => false,
            };
            // Scallops between the rib tips, and lace holes in the webbing.
            if rim && !near_rib || ragged || up > h2 / 2 && h.is_multiple_of(9) {
                continue;
            }
            img.set(x, y, if dx < 0 { s[2] } else { s[1] });
        }
    }
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            if let Some(c) = rib.get(x, y) {
                img.set(x, y, c);
            }
        }
    }
    img.set(7, 15, s[1]);
    img.set(8, 15, s[0]);
    img
}

/// A wall fan from its standing fan `f` (base at the bottom), turned for
/// `facing` (0 north, 1 east, 2 south, 3 west), which lies flat out from
/// the wall: the wall is at the bottom, left, top or right edge.
fn wall_fan(f: &Img, facing: u16) -> Tile {
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let (fx, fy) = match facing {
                0 => (x, y),
                1 => (y, 15 - x),
                2 => (x, 15 - y),
                _ => (y, x),
            };
            if let Some(c) = f.get(fx, fy) {
                img.set(x, y, c);
            }
        }
    }
    img.tile()
}

/// Sea pickles, 1-4: olive capsules lit on their left with bumps, each
/// capped by a glowing tip.
fn sea_pickles(count: usize) -> Tile {
    let s = five([98, 126, 48]);
    let glow = [[246, 255, 190], [212, 240, 132], [168, 206, 92]];
    const SPOTS: [&[(i32, i32)]; 4] =
        [&[(8, 9)], &[(5, 10), (10, 8)], &[(4, 10), (8, 7), (12, 10)], &[(3, 11), (6, 8), (10, 10), (13, 7)]];
    let mut img = Img::new();
    for &(x, top) in SPOTS[count - 1] {
        for y in top..15 {
            let bump = hu(1506 + count as u32, x as usize, y as usize, 100).is_multiple_of(4);
            img.set(x - 1, y, s[3]);
            img.set(x, y, if bump { s[4] } else { s[2] });
            img.set(x + 1, y, if bump { s[0] } else { s[1] });
        }
        img.set(x, 15, s[1]);
        img.set(x - 1, top - 1, glow[1]);
        img.set(x, top - 1, glow[1]);
        img.set(x + 1, top - 1, glow[2]);
        img.set(x, top - 2, glow[0]);
    }
    img.tile()
}

// ---- Bamboo -------------------------------------------------------------------

/// The bamboo post samples columns 6-9 for its sides and columns and rows
/// 6-9 for its ends; those columns are the cane, opaque on every row.
const CANE: std::ops::RangeInclusive<i32> = 6..=9;

/// A bamboo stalk: a cane lit down its left, with a node (a dark groove
/// over a lit bulge) at rows 2 and 12, clear of the rows its top shows;
/// `leaves` 0 bare, 1 small leaves, 2 large leaves sprouting at the top.
fn bamboo(leaves: u8) -> Tile {
    let s = five([116, 160, 52]);
    let l = shades([74, 136, 44]);
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in CANE {
            let col = [3, 2, 2, 1][(x - 6) as usize];
            let k = match y {
                2 | 12 => 0,
                3 | 13 if x < 9 => 4,
                _ if hu(1567, x as usize, y as usize, 101).is_multiple_of(9) => col - 1,
                _ => col,
            };
            img.set(x, y, s[k]);
        }
    }
    if leaves > 0 {
        let sprigs: &[(i32, i32, i32, i32)] = if leaves == 1 {
            &[(6, 4, -1, 5), (9, 2, 1, 5)]
        } else {
            &[(6, 8, -1, 6), (9, 6, 1, 6), (6, 3, -1, 5), (9, 1, 1, 4)]
        };
        for &(x, y, dir, len) in sprigs {
            leaf(&mut img, (x, y), dir, len, 3, &l);
            // Where a leaf leaves the cane, its sheath wraps the post.
            for t in 0..4 {
                let cx = if dir < 0 { 9 - t } else { 6 + t };
                img.set(cx, y + 1 - t / 2, if t < 2 { l[2] } else { l[1] });
            }
        }
    }
    img.tile()
}

/// v1.0 gap track 6d: the stalk in the layout Java's stalk models sample
/// (`bamboo1_age0` to `bamboo4_age1`, original art): four canes side by
/// side, three texels wide from columns 0, 3, 6 and 9 (a thin stalk shows
/// the first two of its cane's columns), each lit down its left with its
/// nodes at its own rows; the cut end at (13, 0) for the top and (13, 4)
/// for the bottom, three texels square (a thin stalk shows the top-left
/// two). The rest is clear.
fn bamboo_stalk() -> Tile {
    let s = five([116, 160, 52]);
    let mut img = Img::new();
    // Each cane's node rows: a dark groove over a lit bulge.
    const NODES: [[i32; 2]; 4] = [[2, 12], [6, 15], [0, 9], [4, 11]];
    for (cane, nodes) in NODES.iter().enumerate() {
        let x0 = cane as i32 * 3;
        for y in 0..N as i32 {
            for i in 0..3 {
                let col = [3, 2, 1][i as usize];
                let k = if nodes.contains(&y) {
                    0
                } else if nodes.contains(&(y - 1)) && i < 2 {
                    4
                } else if hu(1567 + cane as u32, i as usize, y as usize, 101).is_multiple_of(9) {
                    col - 1
                } else {
                    col
                };
                img.set(x0 + i, y, s[k]);
            }
        }
    }
    // The cut ends: a pale pith ringed by the cane's wall.
    for (y0, pith) in [(0, 4), (4, 3)] {
        for y in 0..3 {
            for x in 0..3 {
                let k = if (x, y) == (1, 1) || (x, y) == (0, 0) { pith } else { 2 };
                img.set(13 + x, y0 + y, s[k]);
            }
        }
    }
    img.tile()
}

// ---- Potted torchflower and the ender chest -----------------------------------

/// The potted torchflower: the pot drawn as every other potted plant's pot
/// (it shares the pot's model), with Track 10's torchflower squeezed onto
/// the plant's cross planes as the other pots do (columns 3-12, rows 0-11).
fn potted_torchflower() -> Tile {
    let layer = crate::world::sniffer_plants::t::POTTED;
    let flower = super::torchflower();
    std::array::from_fn(|i| {
        let (x, y) = (i % N, i / N);
        if (y >= 10 && (5..11).contains(&x)) || y >= 12 {
            return super::super::super::utility::potted_side(layer, crate::world::sniffer_plants::TORCHFLOWER, x, y);
        }
        if !(3..13).contains(&x) {
            return [0; 4];
        }
        let (sx, sy) = (((x - 3) * 16 / 10).min(15), (y * 16 / 12).min(15));
        match flower.get(sx as i32, sy as i32) {
            Some([r, g, b]) => [r, g, b, 255],
            None => [0; 4],
        }
    })
}

/// An ender chest face, built like the chest's: obsidian with violet glints
/// in a teal trim, the lid's seam on the sides and front, and an eye for a
/// latch on the front.
fn ender_chest(face: u8) -> Tile {
    let body = ramp([30, 38, 44], [700, 850, 1000, 1200, 1450]);
    let trim = ramp([50, 104, 96], [700, 1000, 1300]);
    let glint = [86, 62, 120];
    let mut img = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            let outer = x == 0 || y == 0 || x == 15 || y == 15;
            let seam = face != 0 && (y == 5 || y == 6);
            let h = hu(2679 + face as u32, x as usize, y as usize, 102);
            let c = if outer {
                if x == 0 || y == 0 { trim[2] } else { trim[0] }
            } else if seam {
                if y == 5 { trim[0] } else { trim[1] }
            } else if x == 1 || y == 1 || face != 0 && y == 7 {
                body[3]
            } else if x == 14 || y == 14 || face != 0 && y == 4 {
                body[0]
            } else if h.is_multiple_of(17) {
                glint
            } else {
                body[[1, 2, 2, 3, 1][(h >> 8) as usize % 5]]
            };
            img.set(x, y, c);
        }
    }
    if face == 2 {
        img.stamp(
            6,
            3,
            &["tttt", "tGgt", "tppt", "tggt", "tttt", " tt "],
            &[('t', trim[0]), ('G', [176, 250, 214]), ('g', [60, 186, 148]), ('p', [10, 30, 26])],
        );
    }
    img.tile()
}

/// Draws every tile of this round through `put`.
pub(super) fn build(put: &mut impl FnMut(u16, Tile)) {
    put(ob::LILY_PAD, lily_pad());
    put(ob::PINK_PETALS, pink_petals());
    for n in 0..4u16 {
        put(bp::WILDFLOWERS + n, wildflowers(n as usize + 1));
        put(bp::LEAF_LITTER + n, leaf_litter(n as usize + 1));
        put(ob::SEA_PICKLES + n, sea_pickles(n as usize + 1));
    }
    put(ob::VINE, vines());
    put(cb::GLOW_LICHEN, glow_lichen());
    for dead in [false, true] {
        for colour in 0..5usize {
            let s = coral_shades(colour, dead);
            let i = colour as u16 + if dead { 5 } else { 0 };
            put(ob::CORAL + i, paint_grid(&coral_block(colour), &s));
            put(ob::CORAL + 10 + i, coral_plant(colour, &s).tile());
            put(ob::CORAL + 20 + i, fan(colour, &s, 12).tile());
            let wall = fan(colour, &s, 10);
            for facing in 0..4u16 {
                put(ut::WALL_FAN + i * 4 + facing, wall_fan(&wall, facing));
            }
        }
    }
    put(ob::BAMBOO, bamboo_stalk());
    put(ob::BAMBOO_SMALL_LEAVES, bamboo(1));
    put(ob::BAMBOO_LARGE_LEAVES, bamboo(2));
    put(crate::world::sniffer_plants::t::POTTED, potted_torchflower());
    put(ut::ENDER_FRONT, ender_chest(2));
    put(ut::ENDER_SIDE, ender_chest(1));
    put(ut::ENDER_TOP, ender_chest(0));
}

/// The layers this round draws, in review order (tests pin each one).
#[cfg(test)]
pub(super) fn layers() -> Vec<u16> {
    let mut out = vec![ob::LILY_PAD, ob::PINK_PETALS];
    out.extend(bp::WILDFLOWERS..bp::WILDFLOWERS + 4);
    out.extend(bp::LEAF_LITTER..bp::LEAF_LITTER + 4);
    out.extend([ob::VINE, cb::GLOW_LICHEN]);
    out.extend(ob::CORAL..ob::CORAL + 30);
    out.extend(ut::WALL_FAN..ut::WALL_FAN + 40);
    out.extend(ob::SEA_PICKLES..ob::SEA_PICKLES + 4);
    out.extend([ob::BAMBOO, ob::BAMBOO_SMALL_LEAVES, ob::BAMBOO_LARGE_LEAVES]);
    out.push(crate::world::sniffer_plants::t::POTTED);
    out.extend([ut::ENDER_FRONT, ut::ENDER_SIDE, ut::ENDER_TOP]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(layer: u16) -> Tile {
        std::array::from_fn(|i| super::super::super::pixel(layer, i % N, i / N).expect("drawn by this round"))
    }

    fn holes(t: &Tile) -> usize {
        t.iter().filter(|p| p[3] == 0).count()
    }

    fn colours(t: &Tile) -> usize {
        t.iter().copied().filter(|p| p[3] > 0).collect::<std::collections::BTreeSet<_>>().len()
    }

    /// Every tile replaces a layer that already existed, none of Track 10's,
    /// and none of the reserved layers 4066-4079.
    #[test]
    fn the_round_replaces_existing_layers_only() {
        let layers = layers();
        assert_eq!(layers.len(), 93);
        let unique: std::collections::BTreeSet<u16> = layers.iter().copied().collect();
        assert_eq!(unique.len(), layers.len(), "a layer is listed twice");
        let first: std::collections::BTreeSet<u16> = super::super::layers().into_iter().collect();
        assert!(layers.iter().all(|l| !first.contains(l)), "a layer Track 10 already draws");
        assert!(layers.iter().all(|&l| (l as u32) < crate::world::block::tex::COUNT), "a layer past the block array");
        // Only gap Track 7 (its potted bamboo leaf, 4066) and gap track 6b
        // (the item models' plant layers, 4080-4083) have grown the array since.
        assert_eq!(crate::world::block::tex::COUNT, 4084, "the block array grew");
        for &l in &layers {
            tile(l);
        }
    }

    /// The flat plants, vines, lichen, coral plants and fans, sea pickles
    /// and the potted torchflower are shaded cut-out sprites.
    #[test]
    fn sprites_are_shaded_and_cut_out() {
        let mut sprites = vec![ob::LILY_PAD, ob::PINK_PETALS, ob::VINE, cb::GLOW_LICHEN];
        sprites.extend(bp::WILDFLOWERS..bp::WILDFLOWERS + 4);
        sprites.extend(bp::LEAF_LITTER..bp::LEAF_LITTER + 4);
        sprites.extend(ob::CORAL + 10..ob::CORAL + 30);
        sprites.extend(ut::WALL_FAN..ut::WALL_FAN + 40);
        sprites.extend(ob::SEA_PICKLES..ob::SEA_PICKLES + 4);
        sprites.push(crate::world::sniffer_plants::t::POTTED);
        let mut bad = Vec::new();
        for layer in sprites {
            let t = tile(layer);
            let opaque = N * N - holes(&t);
            if opaque < 8 || holes(&t) < 16 || colours(&t) < 3 || t.iter().any(|p| p[3] != 0 && p[3] != 255) {
                bad.push(format!("{layer}: {opaque} texels drawn, {} colours", colours(&t)));
            }
        }
        assert!(bad.is_empty(), "not shaded cut-out sprites: {bad:?}");
    }

    /// Wildflowers and leaf litter: each segment adds one quadrant and keeps
    /// every texel of the segments before it.
    #[test]
    fn segments_fill_one_quadrant_each_and_nest() {
        for base in [bp::WILDFLOWERS, bp::LEAF_LITTER] {
            for n in 1..4u16 {
                let (before, after) = (tile(base + n - 1), tile(base + n));
                let (qx, qy) = (QUADRANTS[n as usize].0 as usize, QUADRANTS[n as usize].1 as usize);
                let mut added = 0;
                for i in 0..N * N {
                    let (x, y) = (i % N, i / N);
                    let inside = (qx..qx + 8).contains(&x) && (qy..qy + 8).contains(&y);
                    if inside {
                        assert_eq!(before[i][3], 0, "layer {}: quadrant {n} drawn early", base + n - 1);
                        added += (after[i][3] > 0) as usize;
                    } else {
                        assert_eq!(before[i], after[i], "layer {}: texel {x},{y} changed", base + n);
                    }
                }
                assert!(added >= 8, "layer {}: segment {} adds {added} texels", base + n, n + 1);
            }
        }
    }

    /// Coral blocks are opaque and every kind differs; dead corals keep the
    /// living shape in grey.
    #[test]
    fn dead_coral_keeps_the_living_shape_in_grey() {
        let mut seen = Vec::new();
        for i in 0..5 {
            let block = tile(ob::CORAL + i);
            assert_eq!(holes(&block), 0, "coral block {i} is opaque");
            assert!(colours(&block) >= 4, "coral block {i} has relief");
            assert!(!seen.contains(&block), "coral block {i} repeats another");
            seen.push(block);
            for kind in [0, 10, 20] {
                let (alive, dead) = (tile(ob::CORAL + kind + i), tile(ob::CORAL + kind + 5 + i));
                for (a, d) in alive.iter().zip(&dead) {
                    assert_eq!(a[3], d[3], "dead coral {} keeps its shape", ob::CORAL + kind + 5 + i);
                    let spread = d[..3].iter().max().unwrap() - d[..3].iter().min().unwrap();
                    assert!(d[3] == 0 || spread <= 16, "dead coral {} is grey", ob::CORAL + kind + 5 + i);
                }
            }
        }
    }

    /// A wall fan's four facings are one fan turned to its wall, reaching
    /// no further than Java's outline (11 texels out).
    #[test]
    fn wall_fans_turn_with_their_wall() {
        for kind in 0..10u16 {
            let north = tile(ut::WALL_FAN + kind * 4);
            for facing in 0..4u16 {
                let t = tile(ut::WALL_FAN + kind * 4 + facing);
                for y in 0..N {
                    for x in 0..N {
                        let (fx, fy) = match facing {
                            0 => (x, y),
                            1 => (y, 15 - x),
                            2 => (x, 15 - y),
                            _ => (y, x),
                        };
                        assert_eq!(t[y * N + x], north[fy * N + fx], "wall fan {kind} facing {facing} at {x},{y}");
                        let out = 15 - fy;
                        assert!(north[fy * N + fx][3] == 0 || out <= 10, "wall fan {kind} reaches {out} texels out");
                    }
                }
            }
        }
    }

    /// v1.0 gap track 6d: every texel Java's stalk models sample is
    /// opaque: the four canes' columns 0-11 on every row and the two end
    /// squares at (13, 0) and (13, 4).
    #[test]
    fn bamboo_keeps_a_solid_cane() {
        let t = tile(ob::BAMBOO);
        let solid = |x: usize, y: usize| t[y * N + x][3] == 255;
        for y in 0..N {
            for x in 0..12 {
                assert!(solid(x, y), "cane texel {x},{y} is see-through");
            }
        }
        for (x, y) in (13..16).flat_map(|x| (0..3).chain(4..7).map(move |y| (x, y))) {
            assert!(solid(x, y), "end texel {x},{y} is see-through");
        }
    }

    /// One to four sea pickles: each pickle has its glowing tip.
    #[test]
    fn each_sea_pickle_glows() {
        for n in 1..=4u16 {
            let t = tile(ob::SEA_PICKLES + n - 1);
            let tips = t.iter().filter(|p| p[3] > 0 && p[0] > 240 && p[1] > 250).count();
            assert_eq!(tips, n as usize, "{n} sea pickles");
        }
    }

    /// The potted torchflower's pot is the pot the other potted plants
    /// share, and the flower stands above it.
    #[test]
    fn the_potted_torchflower_shares_the_pot() {
        let layer = crate::world::sniffer_plants::t::POTTED;
        let t = tile(layer);
        for y in 12..N {
            for x in 0..N {
                let pot = super::super::super::super::utility::potted_side(
                    layer,
                    crate::world::sniffer_plants::TORCHFLOWER,
                    x,
                    y,
                );
                assert_eq!(t[y * N + x], pot, "pot texel {x},{y}");
            }
        }
        let flame = (0..10 * N).filter(|&i| t[i][3] > 0 && t[i][0] > 180 && t[i][2] < 90).count();
        assert!(flame >= 8, "the bloom shows above the pot");
    }

    /// The ender chest's front carries its eye; the side and top don't.
    #[test]
    fn the_ender_chest_has_an_eye_for_a_latch() {
        let eye = |t: &Tile| t.iter().filter(|p| p[1] > 150 && p[0] < 200 && p[1] > p[0] + 50).count();
        assert!(eye(&tile(ut::ENDER_FRONT)) >= 4);
        assert_eq!(eye(&tile(ut::ENDER_SIDE)), 0);
        assert_eq!(eye(&tile(ut::ENDER_TOP)), 0);
        assert!([ut::ENDER_FRONT, ut::ENDER_SIDE, ut::ENDER_TOP].iter().all(|&l| holes(&tile(l)) == 0));
    }
}
