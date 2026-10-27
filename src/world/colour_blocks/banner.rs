//! Plain banners: Java 26.3 `BannerBlock` (16 rotations on the ground) and
//! `WallBannerBlock` (4 facings), drawn like `BannerRenderer` with Java's
//! `BannerModel` and `BannerFlagModel` at two-thirds scale.
//!
//! Banners have no collision and draw outside the chunk meshes (their
//! rotations aren't axis-aligned and the flag reaches past the cell), as
//! Java draws them with a block entity renderer. Patterns arrive with the
//! loom (v0.17).

use glam::{IVec3, Vec3};

use super::super::block::{Block, Facing};
use super::super::shape::{Box16, Boxes};
use super::{BANNER_ORIGIN, WALL_BANNER_ORIGIN, tex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BannerMount {
    /// Java's `ROTATION` (0..16, sixteenths of a turn from facing south).
    Ground(u8),
    /// Java's `FACING`: the side the banner faces, away from its wall.
    Wall(Facing),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Banner {
    /// `DyeColor` index.
    pub colour: u8,
    pub mount: BannerMount,
}

pub const fn standing(colour: u8, rotation: u8) -> Block {
    Block(BANNER_ORIGIN + colour as u16 * 16 + (rotation & 15) as u16)
}

pub const fn wall(colour: u8, facing: Facing) -> Block {
    Block(WALL_BANNER_ORIGIN + colour as u16 * 4 + facing as u16)
}

pub const fn decode(id: u16) -> Option<Banner> {
    if id >= BANNER_ORIGIN && id < WALL_BANNER_ORIGIN {
        let i = id - BANNER_ORIGIN;
        return Some(Banner { colour: (i / 16) as u8, mount: BannerMount::Ground((i % 16) as u8) });
    }
    if id >= WALL_BANNER_ORIGIN && id < WALL_BANNER_ORIGIN + 64 {
        let i = id - WALL_BANNER_ORIGIN;
        return Some(Banner { colour: (i / 4) as u8, mount: BannerMount::Wall(Facing::ALL[(i % 4) as usize]) });
    }
    None
}

pub const fn encode(b: Banner) -> Block {
    match b.mount {
        BannerMount::Ground(r) => standing(b.colour, r),
        BannerMount::Wall(f) => wall(b.colour, f),
    }
}

/// `RotationSegment.convertToSegment(yRot + 180)` for a player whose Java
/// `yRot` (degrees; 0 looks south, 90 west) is `y_rot`.
pub fn rotation_for(y_rot: f32) -> u8 {
    (((y_rot + 180.0) / 22.5 + 0.5).floor() as i32 & 15) as u8
}

/// Java's `yRot` of a look vector.
pub fn java_y_rot(look: Vec3) -> f32 {
    (-look.x).atan2(look.z).to_degrees()
}

/// `getShape`: `Block.column(8, 0, 16)` standing; on a wall
/// `boxZ(16, 0, 12.5, 14, 16)` turned to the facing, with 12.5 rounded up
/// to whole pixels.
pub fn outline(b: Banner) -> Boxes {
    let bx = |min: [u8; 3], max: [u8; 3]| Boxes::from_box(Box16 { min, max });
    match b.mount {
        BannerMount::Ground(_) => bx([4, 0, 4], [12, 16, 12]),
        BannerMount::Wall(Facing::South) => bx([0, 0, 0], [16, 13, 2]),
        BannerMount::Wall(Facing::North) => bx([0, 0, 14], [16, 13, 16]),
        BannerMount::Wall(Facing::East) => bx([0, 0, 0], [2, 13, 16]),
        BannerMount::Wall(Facing::West) => bx([14, 0, 0], [16, 13, 16]),
    }
}

/// The cell whose block holds a banner up: below a standing banner, behind
/// a wall banner.
pub fn support(b: Banner, at: IVec3) -> IVec3 {
    match b.mount {
        BannerMount::Ground(_) => at - IVec3::Y,
        BannerMount::Wall(f) => at - f.offset(),
    }
}

/// Java's `BlockStateBase.isSolid` for the supporting block, which is all
/// `canSurvive` asks of it.
pub fn holds(support: Block) -> bool {
    support.is_solid()
}

/// The turn about the cell's vertical centre line, in radians, our models
/// use: `BannerRenderer.modelTransformation` turns by `-angle` about +Y in
/// Java's right-handed frame, the same sense as `BlockModel::yaw`.
pub fn yaw(b: Banner) -> f32 {
    let degrees = match b.mount {
        BannerMount::Ground(r) => r as f32 * 22.5,
        BannerMount::Wall(f) => match f {
            Facing::South => 0.0,
            Facing::West => 90.0,
            Facing::North => 180.0,
            Facing::East => 270.0,
        },
    };
    degrees.to_radians()
}

/// `BannerRenderer.extractRenderState`'s sway phase, 0..1, for a banner at
/// `cell` after `ticks` game ticks.
pub fn phase(cell: IVec3, ticks: f64) -> f32 {
    let seed = (cell.x as i64 * 7 + cell.y as i64 * 9 + cell.z as i64 * 13) as f64;
    ((seed + ticks).rem_euclid(100.0) / 100.0) as f32
}

/// One quad of a banner model: corners relative to the cell's minimum
/// corner (blocks), UVs (16 texels per unit), its layer and the face's
/// outward normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub corners: [Vec3; 4],
    pub uv: [[f32; 2]; 4],
    pub layer: u16,
    pub normal: Vec3,
}

/// A box of Java's banner models in model pixels (y down, z toward the
/// back), with the pivot and `xRot` of its part.
struct Part {
    min: [f32; 3],
    max: [f32; 3],
    pivot: [f32; 3],
    pitch: f32,
    layer: u16,
}

/// The quads of banner `b` with its flag at sway `phase`.
pub fn quads(b: Banner, phase: f32) -> Vec<Quad> {
    // `BannerFlagModel.setupAnim`.
    let pitch = (-0.0125 + 0.01 * (std::f32::consts::TAU * phase).cos()) * std::f32::consts::PI;
    let cloth = tex::CLOTH + b.colour as u16;
    let parts: Vec<Part> = match b.mount {
        BannerMount::Ground(_) => vec![
            Part { min: [-1.0, -42.0, -1.0], max: [1.0, 0.0, 1.0], pivot: [0.0; 3], pitch: 0.0, layer: tex::POLE },
            Part { min: [-10.0, -44.0, -1.0], max: [10.0, -42.0, 1.0], pivot: [0.0; 3], pitch: 0.0, layer: tex::POLE },
            Part { min: [-10.0, 0.0, -2.0], max: [10.0, 40.0, -1.0], pivot: [0.0, -44.0, 0.0], pitch, layer: cloth },
        ],
        BannerMount::Wall(_) => vec![
            Part { min: [-10.0, -20.5, 9.5], max: [10.0, -18.5, 11.5], pivot: [0.0; 3], pitch: 0.0, layer: tex::POLE },
            Part { min: [-10.0, 0.0, -2.0], max: [10.0, 40.0, -1.0], pivot: [0.0, -20.5, 10.5], pitch, layer: cloth },
        ],
    };
    let (sin, cos) = yaw(b).sin_cos();
    // Model pixels to the cell: the part's pose, Java's (2/3, -2/3, -2/3)
    // scale, the turn, then (0.5, 0, 0.5).
    let place = |p: Vec3, part: &Part| {
        let (ps, pc) = part.pitch.sin_cos();
        let local = Vec3::new(p.x, p.y * pc - p.z * ps, p.y * ps + p.z * pc) + Vec3::from_array(part.pivot);
        let s = local * Vec3::new(2.0, -2.0, -2.0) / 3.0 / 16.0;
        Vec3::new(s.x * cos - s.z * sin + 0.5, s.y, s.x * sin + s.z * cos + 0.5)
    };
    let turn = |n: Vec3, part: &Part| {
        let (ps, pc) = part.pitch.sin_cos();
        let n = Vec3::new(n.x, n.y * pc - n.z * ps, n.y * ps + n.z * pc) * Vec3::new(1.0, -1.0, -1.0);
        Vec3::new(n.x * cos - n.z * sin, n.y, n.x * sin + n.z * cos)
    };
    let mut out = Vec::with_capacity(parts.len() * 6);
    for part in &parts {
        let (lo, hi) = (Vec3::from_array(part.min), Vec3::from_array(part.max));
        for face in 0..6 {
            let (d, positive) = (face / 2, face % 2 == 0);
            let (u, v) = ((d + 1) % 3, (d + 2) % 3);
            let corner = |a: bool, c: bool| {
                let mut p = [0.0f32; 3];
                p[d] = if positive { hi[d] } else { lo[d] };
                p[u] = if a { hi[u] } else { lo[u] };
                p[v] = if c { hi[v] } else { lo[v] };
                Vec3::from_array(p)
            };
            let model = [corner(false, false), corner(true, false), corner(true, true), corner(false, true)];
            // Texels are model pixels; sides run along x or z and down y.
            let uv = model.map(|p| match d {
                0 => [p.z / 16.0, p.y / 16.0],
                1 => [p.x / 16.0, p.z / 16.0],
                _ => [p.x / 16.0, p.y / 16.0],
            });
            let mut normal = Vec3::ZERO;
            normal[d] = if positive { 1.0 } else { -1.0 };
            out.push(Quad {
                corners: model.map(|p| place(p, part)),
                uv,
                layer: part.layer,
                normal: turn(normal, part),
            });
        }
    }
    out
}
