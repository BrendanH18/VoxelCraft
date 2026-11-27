//! Clouds as Java 26.3's `CloudRenderer` builds them: a 256 x 256 grid of
//! 12-block cells that wraps around the world, drifting east at 0.03 blocks
//! per tick. Fast clouds are one downward face per filled cell at the cloud
//! base; Fancy clouds are 12 x 4 x 12 boxes with the faces the camera can
//! see, and inside faces for the cells around it. Faces are listed in
//! rings from the camera outward, as Java does, so near clouds draw over
//! far ones without sorting.
//!
//! Java reads which cells are filled from its `clouds.png`; VoxelCraft's
//! pattern is its own ([`Pattern::generate`]), with Java's cell size,
//! thickness, motion, shading and face rules.

use super::Clouds;

/// Cells along each side of the pattern.
pub const SIZE: i32 = 256;
/// Block size of a cell and the cloud layer's thickness.
pub const CELL: f32 = 12.0;
pub const THICKNESS: f32 = 4.0;
/// Drift east per game tick, and Java's constant z offset.
pub const DRIFT_PER_TICK: f64 = 0.03;
pub const Z_OFFSET: f64 = 3.96;

/// Face directions in Java's `Direction` 3D order.
pub const DOWN: u32 = 0;
pub const UP: u32 = 1;
pub const NORTH: u32 = 2;
pub const SOUTH: u32 = 3;
pub const WEST: u32 = 4;
pub const EAST: u32 = 5;
/// Face flags: an inside face (seen from within the cell) and one drawn
/// with the top face's colour (Fast clouds).
pub const INSIDE: u32 = 1 << 23;
pub const TOP_COLOUR: u32 = 1 << 24;
/// Cell offsets are packed with this bias in 10 bits each.
pub const BIAS: i32 = 512;

/// `clouds.vsh`'s face shades by direction.
pub const SHADES: [f32; 6] = [0.7, 1.0, 0.8, 0.8, 0.9, 0.9];

/// Which cells hold cloud.
pub struct Pattern {
    filled: Vec<bool>,
}

impl Pattern {
    /// VoxelCraft's original pattern: blobs from two octaves of value noise
    /// on the cell grid, thresholded to about a third of the sky, wrapping
    /// seamlessly at 256 cells.
    pub fn generate() -> Self {
        let hash = |x: i32, y: i32, s: u32| {
            let mut h = (x.rem_euclid(SIZE) as u32).wrapping_mul(0x8DA6_B343)
                ^ (y.rem_euclid(SIZE) as u32).wrapping_mul(0xD816_3841)
                ^ s.wrapping_mul(0xCB1A_B31F);
            h ^= h >> 13;
            h = h.wrapping_mul(0x5BD1_E995);
            h ^= h >> 15;
            (h & 0xFFFF) as f32 / 65535.0
        };
        // Value noise whose lattice divides the grid, so it tiles.
        let noise = |x: i32, y: i32, period: i32, s: u32| {
            let (gx, gy) = (x.div_euclid(period), y.div_euclid(period));
            let (fx, fy) =
                ((x.rem_euclid(period)) as f32 / period as f32, (y.rem_euclid(period)) as f32 / period as f32);
            let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
            let (u, v) = (smooth(fx), smooth(fy));
            let p = |dx, dy| hash((gx + dx) * period, (gy + dy) * period, s);
            let top = p(0, 0) + (p(1, 0) - p(0, 0)) * u;
            let bottom = p(0, 1) + (p(1, 1) - p(0, 1)) * u;
            top + (bottom - top) * v
        };
        let filled = (0..SIZE * SIZE)
            .map(|i| {
                let (x, y) = (i % SIZE, i / SIZE);
                let n = noise(x, y, 16, 1) * 0.55 + noise(x, y, 8, 2) * 0.3 + noise(x, y, 4, 3) * 0.15;
                n > 0.56
            })
            .collect();
        Self { filled }
    }

    pub fn filled(&self, x: i32, z: i32) -> bool {
        self.filled[(x.rem_euclid(SIZE) + z.rem_euclid(SIZE) * SIZE) as usize]
    }

    /// The share of filled cells.
    pub fn coverage(&self) -> f32 {
        self.filled.iter().filter(|&&f| f).count() as f32 / self.filled.len() as f32
    }
}

/// Where the camera is against the cloud layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relative {
    Above,
    Inside,
    Below,
}

/// The cloud layer as seen from a camera: the cell it is over, where it
/// sits within that cell, and the layer's base relative to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub cell: (i32, i32),
    /// Camera position within its cell, 0..12 blocks.
    pub in_cell: (f32, f32),
    /// Cloud base y minus camera y.
    pub base: f32,
    pub relative: Relative,
}

impl View {
    /// `CloudRenderer.prepare`'s placement for a camera at `(x, y, z)`,
    /// `ticks` game ticks in (with the partial tick), clouds at `height`.
    pub fn new(x: f64, y: f64, z: f64, ticks: f64, height: f32) -> Self {
        let width = f64::from(SIZE) * f64::from(CELL);
        let offset = ticks.rem_euclid(f64::from(SIZE) * 400.0);
        let mut cx = x + offset * DRIFT_PER_TICK;
        let mut cz = z + Z_OFFSET;
        cx -= (cx / width).floor() * width;
        cz -= (cz / width).floor() * width;
        let cell = ((cx / 12.0).floor() as i32, (cz / 12.0).floor() as i32);
        let in_cell = ((cx - f64::from(cell.0) * 12.0) as f32, (cz - f64::from(cell.1) * 12.0) as f32);
        let base = (f64::from(height) - y) as f32;
        let relative = if base + THICKNESS < 0.0 {
            Relative::Above
        } else if base > 0.0 {
            Relative::Below
        } else {
            Relative::Inside
        };
        Self { cell, in_cell, base, relative }
    }
}

/// Cloud Distance in Java chunks as a radius in cells.
pub fn radius_cells(range_chunks: u32) -> i32 {
    (range_chunks as f32 * 16.0 / CELL).ceil() as i32
}

/// `CloudRenderer.getSizeForCloudDistance`'s bound on the faces one build
/// can make within `radius` cells: four per cell (a top or bottom, or both
/// inside the layer, and at most two sides facing the camera) over
/// `2 (radius + 1)^2` cells, which covers the disc, plus the 54 inside
/// faces of the 3 x 3 cells around the camera.
pub fn max_faces(radius: i32) -> usize {
    let r = radius.max(0) as usize + 1;
    let cells = r * 2 * r * 2 / 2;
    cells * 4 + 54
}

/// One face: cell offset from the camera's cell, direction and flags.
pub fn pack(x: i32, z: i32, dir: u32, flags: u32) -> u32 {
    ((x + BIAS) as u32 & 1023) | (((z + BIAS) as u32 & 1023) << 10) | (dir << 20) | flags
}

pub fn unpack(face: u32) -> (i32, i32, u32, u32) {
    ((face & 1023) as i32 - BIAS, ((face >> 10) & 1023) as i32 - BIAS, (face >> 20) & 7, face & (INSIDE | TOP_COLOUR))
}

/// `CloudRenderer.buildMesh`: the faces for `view` within `radius` cells,
/// near rings first. Empty for [`Clouds::Off`].
pub fn build(pattern: &Pattern, view: &View, mode: Clouds, radius: i32, out: &mut Vec<u32>) {
    out.clear();
    if mode == Clouds::Off {
        return;
    }
    let fancy = mode == Clouds::Fancy;
    for ring in 0..=2 * radius {
        for x in -ring..=ring {
            let z = ring - x.abs();
            if z < 0 || z > radius || x * x + z * z > radius * radius {
                continue;
            }
            if z != 0 {
                cell(pattern, view, fancy, x, -z, out);
            }
            cell(pattern, view, fancy, x, z, out);
        }
    }
}

fn cell(pattern: &Pattern, view: &View, fancy: bool, x: i32, z: i32, out: &mut Vec<u32>) {
    let (cx, cz) = (view.cell.0 + x, view.cell.1 + z);
    if !pattern.filled(cx, cz) {
        return;
    }
    if !fancy {
        out.push(pack(x, z, DOWN, TOP_COLOUR));
        return;
    }
    if view.relative != Relative::Below {
        out.push(pack(x, z, UP, 0));
    }
    if view.relative != Relative::Above {
        out.push(pack(x, z, DOWN, 0));
    }
    // Side faces only where the neighbour is empty and the face looks
    // toward the camera's cell.
    if z > 0 && !pattern.filled(cx, cz - 1) {
        out.push(pack(x, z, NORTH, 0));
    }
    if z < 0 && !pattern.filled(cx, cz + 1) {
        out.push(pack(x, z, SOUTH, 0));
    }
    if x > 0 && !pattern.filled(cx - 1, cz) {
        out.push(pack(x, z, WEST, 0));
    }
    if x < 0 && !pattern.filled(cx + 1, cz) {
        out.push(pack(x, z, EAST, 0));
    }
    if x.abs() <= 1 && z.abs() <= 1 {
        for dir in DOWN..=EAST {
            out.push(pack(x, z, dir, INSIDE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pattern_covers_about_a_third_of_the_sky_and_wraps() {
        let p = Pattern::generate();
        let c = p.coverage();
        assert!((0.2..0.45).contains(&c), "coverage {c}");
        assert_eq!(p.filled(-1, 3), p.filled(SIZE - 1, 3 + SIZE));
    }

    /// Every other cell filled: the most side faces a pattern can make.
    fn checkerboard() -> Pattern {
        Pattern { filled: (0..SIZE * SIZE).map(|i| (i % SIZE + i / SIZE) % 2 == 0).collect() }
    }

    #[test]
    fn build_never_exceeds_javas_bound() {
        let mut faces = Vec::new();
        for pattern in [Pattern::generate(), checkerboard(), Pattern { filled: vec![true; (SIZE * SIZE) as usize] }] {
            for range in [2u32, 12, 64, 128] {
                let radius = radius_cells(range);
                for y in [100.0, 194.0, 300.0] {
                    for mode in [Clouds::Fancy, Clouds::Fast] {
                        let view = View::new(5.0, y, 7.0, 0.0, 192.33);
                        build(&pattern, &view, mode, radius, &mut faces);
                        assert!(faces.len() <= max_faces(radius), "{range} {y} {mode:?}: {}", faces.len());
                    }
                }
            }
        }
    }

    /// v1.0 Track 2 draws clouds in one pass, as Java does: the nearest
    /// surface must be drawn before anything it hides. Along any ray from
    /// the camera the cells it crosses come in strictly increasing build
    /// order, and within a cell the outside faces come before the inside
    /// ones.
    #[test]
    fn faces_come_nearest_first_along_every_ray() {
        let pattern = Pattern { filled: vec![true; (SIZE * SIZE) as usize] };
        let mut faces = Vec::new();
        let mut rng = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 11) as f64 / (1u64 << 53) as f64
        };
        let radius = radius_cells(16);
        for _ in 0..200 {
            let view = View::new(next() * 3000.0, 100.0, next() * 3000.0, 0.0, 192.33);
            build(&pattern, &view, Clouds::Fancy, radius, &mut faces);
            let mut first = std::collections::HashMap::new();
            for (i, &f) in faces.iter().enumerate() {
                let (x, z, _, flags) = unpack(f);
                let at = first.entry((x, z)).or_insert(i);
                // Within a cell, inside faces follow every outside face.
                if flags & INSIDE == 0 {
                    assert!(
                        faces[*at..i].iter().all(|&g| unpack(g).3 & INSIDE == 0),
                        "outside face after an inside one"
                    );
                }
            }
            // March rays across the cell grid from the camera's spot in its cell.
            for _ in 0..50 {
                let angle = next() * std::f64::consts::TAU;
                let (dx, dz) = (angle.cos(), angle.sin());
                let (mut px, mut pz) = (f64::from(view.in_cell.0) / 12.0, f64::from(view.in_cell.1) / 12.0);
                let (mut cx, mut cz) = (0i32, 0i32);
                let mut last = first[&(0, 0)];
                loop {
                    let tx = if dx > 0.0 {
                        (1.0 - px) / dx
                    } else if dx < 0.0 {
                        -px / dx
                    } else {
                        f64::INFINITY
                    };
                    let tz = if dz > 0.0 {
                        (1.0 - pz) / dz
                    } else if dz < 0.0 {
                        -pz / dz
                    } else {
                        f64::INFINITY
                    };
                    if tx < tz {
                        cx += dx.signum() as i32;
                        px = if dx > 0.0 { 0.0 } else { 1.0 };
                        pz += dz * tx;
                    } else {
                        cz += dz.signum() as i32;
                        pz = if dz > 0.0 { 0.0 } else { 1.0 };
                        px += dx * tz;
                    }
                    let Some(&i) = first.get(&(cx, cz)) else { break };
                    assert!(i > last, "cell ({cx}, {cz}) is built before a nearer cell on the same ray");
                    last = i;
                }
            }
        }
    }

    #[test]
    fn faces_pack_and_unpack() {
        for (x, z, d, f) in [(0, 0, UP, 0), (-171, 171, EAST, INSIDE), (5, -9, DOWN, TOP_COLOUR)] {
            assert_eq!(unpack(pack(x, z, d, f)), (x, z, d, f));
        }
    }
}
