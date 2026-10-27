//! Standing, wall, hanging and wall hanging signs of the twelve woods
//! (Java 26.3 `StandingSignBlock`, `WallSignBlock`,
//! `CeilingHangingSignBlock`, `WallHangingSignBlock`). Their text lives in
//! [`super::entities`]; the client draws them outside the chunk meshes
//! (`render::signs`), so their registered shape is only the outline.
//!
//! Each [`Kind`] has 128 ids from `17024 + 128 * kind`:
//!
//! | Local | State |
//! | --- | --- |
//! | 0..=31 | standing: `16 * waterlogged + rotation` |
//! | 32..=39 | wall: `32 + 4 * waterlogged + facing` |
//! | 40..=103 | hanging: `40 + 32 * waterlogged + 16 * attached + rotation` |
//! | 104..=111 | wall hanging: `104 + 4 * waterlogged + facing` |
//!
//! `facing` follows `Facing::ALL` (south, north, east, west); `rotation` is
//! Java's `ROTATION_16`, 0 facing south and turning through west. Locals
//! 112..=127 stay unregistered.

use glam::{DVec3, IVec3, Vec3};

use super::super::block::{Block, Facing, RenderKind};
use super::super::shape::{Box16, Boxes};
use super::{Kind, Props};
use crate::item::{Item, ToolKind};
use crate::session::placement::PlaceContext;

pub const FIRST: u16 = 17024;
pub const STRIDE: u16 = 128;
pub const END: u16 = FIRST + STRIDE * 12;

/// How a sign is held up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mount {
    Standing {
        rotation: u8,
    },
    Wall {
        facing: Facing,
    },
    /// Under a ceiling; `attached` hangs from the middle on a V of chains
    /// (Java's `ATTACHED`).
    Hanging {
        rotation: u8,
        attached: bool,
    },
    WallHanging {
        facing: Facing,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sign {
    pub kind: Kind,
    pub mount: Mount,
    pub wet: bool,
}

impl Sign {
    pub const fn hanging(self) -> bool {
        matches!(self.mount, Mount::Hanging { .. } | Mount::WallHanging { .. })
    }

    pub const fn block(self) -> Block {
        let local = match self.mount {
            Mount::Standing { rotation } => self.wet as u16 * 16 + rotation as u16,
            Mount::Wall { facing } => 32 + self.wet as u16 * 4 + facing as u16,
            Mount::Hanging { rotation, attached } => 40 + self.wet as u16 * 32 + attached as u16 * 16 + rotation as u16,
            Mount::WallHanging { facing } => 104 + self.wet as u16 * 4 + facing as u16,
        };
        Block(FIRST + self.kind.index() * STRIDE + local)
    }

    /// The block the sign's item places: standing or hanging, rotation 0, dry.
    pub const fn item_block(self) -> Block {
        let mount = if self.hanging() {
            Mount::Hanging { rotation: 0, attached: false }
        } else {
            Mount::Standing { rotation: 0 }
        };
        Sign { kind: self.kind, mount, wet: false }.block()
    }

    /// Java's `getYRotationDegrees`: 0 faces south, 90 west, 180 north.
    pub fn yaw_degrees(self) -> f32 {
        match self.mount {
            Mount::Standing { rotation } | Mount::Hanging { rotation, .. } => rotation as f32 * 22.5,
            Mount::Wall { facing } | Mount::WallHanging { facing } => facing_yaw(facing),
        }
    }

    /// Java's `getSignHitboxCenterPosition`, relative to the cell.
    pub fn hitbox_centre(self) -> DVec3 {
        match self.mount {
            Mount::Wall { facing } => {
                let b = Box16 { min: [0, 4, 0], max: [16, 12, 2] }.turned(facing);
                DVec3::new(
                    (b.min[0] + b.max[0]) as f64 / 32.0,
                    (4.5 + 12.5) / 32.0,
                    (b.min[2] + b.max[2]) as f64 / 32.0,
                )
            }
            _ => DVec3::splat(0.5),
        }
    }

    /// Java's `SignBlockEntity.getSlotPlayerIsFacing`: whether a player at
    /// `player` (feet or eye; only x and z count) faces the front.
    pub fn faces_front(self, cell: IVec3, player: DVec3) -> bool {
        let c = cell.as_dvec3() + self.hitbox_centre();
        let (xd, zd) = (player.x - c.x, player.z - c.z);
        let player_yaw = (zd.atan2(xd).to_degrees() - 90.0) as f32;
        degrees_difference(self.yaw_degrees(), player_yaw) <= 90.0
    }

    /// `SignBlockEntity`'s line width and height in font pixels (60 and 9
    /// for hanging signs).
    pub const fn max_line_width(self) -> u32 {
        if self.hanging() { 60 } else { 90 }
    }
    pub const fn line_height(self) -> u32 {
        if self.hanging() { 9 } else { 10 }
    }
}

/// Java's `Direction.toYRot`.
pub const fn facing_yaw(facing: Facing) -> f32 {
    match facing {
        Facing::South => 0.0,
        Facing::West => 90.0,
        Facing::North => 180.0,
        Facing::East => 270.0,
    }
}

/// Java's `Mth.degreesDifferenceAbs`.
fn degrees_difference(a: f32, b: f32) -> f32 {
    ((b - a + 540.0).rem_euclid(360.0) - 180.0).abs()
}

/// Java's `RotationSegment.convertToSegment(float)` (`SegmentedAnglePrecision(4)`).
pub fn segment(degrees: f32) -> u8 {
    ((degrees * 16.0 / 360.0).round() as i32 & 15) as u8
}

/// The player's Java yaw (`getRotation`) from a look vector: 0 looking
/// south, 90 west.
pub fn look_yaw(look: Vec3) -> f32 {
    (-look.x).atan2(look.z).to_degrees()
}

pub const fn sign(b: Block) -> Option<Sign> {
    let id = b.0;
    if id < FIRST || id >= END {
        return None;
    }
    let kind = Kind::of((id - FIRST) / STRIDE);
    let local = (id - FIRST) % STRIDE;
    let (mount, wet) = match local {
        0..=31 => (Mount::Standing { rotation: (local % 16) as u8 }, local >= 16),
        32..=39 => (Mount::Wall { facing: Facing::ALL[(local % 4) as usize] }, local >= 36),
        40..=103 => {
            let l = local - 40;
            (Mount::Hanging { rotation: (l % 16) as u8, attached: l % 32 >= 16 }, l >= 32)
        }
        104..=111 => (Mount::WallHanging { facing: Facing::ALL[(local % 4) as usize] }, local >= 108),
        _ => return None,
    };
    Some(Sign { kind, mount, wet })
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some(s) = sign(Block(id)) else { return None };
    const STANDING: [&str; 12] = [
        "oak sign",
        "spruce sign",
        "birch sign",
        "jungle sign",
        "acacia sign",
        "dark oak sign",
        "mangrove sign",
        "cherry sign",
        "pale oak sign",
        "bamboo sign",
        "crimson sign",
        "warped sign",
    ];
    const WALL: [&str; 12] = [
        "oak wall sign",
        "spruce wall sign",
        "birch wall sign",
        "jungle wall sign",
        "acacia wall sign",
        "dark oak wall sign",
        "mangrove wall sign",
        "cherry wall sign",
        "pale oak wall sign",
        "bamboo wall sign",
        "crimson wall sign",
        "warped wall sign",
    ];
    const HANGING: [&str; 12] = [
        "oak hanging sign",
        "spruce hanging sign",
        "birch hanging sign",
        "jungle hanging sign",
        "acacia hanging sign",
        "dark oak hanging sign",
        "mangrove hanging sign",
        "cherry hanging sign",
        "pale oak hanging sign",
        "bamboo hanging sign",
        "crimson hanging sign",
        "warped hanging sign",
    ];
    const WALL_HANGING: [&str; 12] = [
        "oak wall hanging sign",
        "spruce wall hanging sign",
        "birch wall hanging sign",
        "jungle wall hanging sign",
        "acacia wall hanging sign",
        "dark oak wall hanging sign",
        "mangrove wall hanging sign",
        "cherry wall hanging sign",
        "pale oak wall hanging sign",
        "bamboo wall hanging sign",
        "crimson wall hanging sign",
        "warped wall hanging sign",
    ];
    let k = s.kind.index();
    let (names, icon) = match s.mount {
        Mount::Standing { .. } => (STANDING, super::tex::SIGN_ICONS + k),
        Mount::Wall { .. } => (WALL, super::tex::SIGN_ICONS + k),
        Mount::Hanging { .. } => (HANGING, super::tex::HANGING_SIGN_ICONS + k),
        Mount::WallHanging { .. } => (WALL_HANGING, super::tex::HANGING_SIGN_ICONS + k),
    };
    Some((names[k as usize], RenderKind::Shaped, [icon; 6]))
}

pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    match sign(Block(id)) {
        Some(s) => {
            let other = Sign { wet: !s.wet, ..s }.block().0;
            Some((other, s.wet))
        }
        None => None,
    }
}

/// Wall signs answer as their standing sign and wall hanging signs as their
/// hanging sign: one item places both (Java's `StandingAndWallBlockItem`).
pub fn base(id: u16) -> Option<Block> {
    sign(Block(id)).map(Sign::item_block)
}

/// Java: 1.0, axe, no collision; drops the sign item; signs don't burn
/// (`FireBlock` lists none) but all except crimson and warped catch lava.
pub fn props(b: Block) -> Option<Props> {
    let s = sign(b)?;
    Some(Props {
        hardness: 1.0,
        tool: Some(ToolKind::Axe),
        fire: (0, 0),
        lava: s.kind.burns(),
        drop: Some(Item::from_block(s.item_block())),
    })
}

/// The selection outline (Java's `getShape`); signs have no collision and
/// no chunk-mesh geometry.
pub fn outline(b: Block) -> Option<Boxes> {
    let s = sign(b)?;
    let bx = |min: [u8; 3], max: [u8; 3]| Box16 { min, max };
    let mut out = Boxes::from_box(match s.mount {
        // `Block.column(8, 0, 16)`.
        Mount::Standing { .. } => bx([4, 0, 4], [12, 16, 12]),
        // `Block.boxZ(16, 4.5, 12.5, 14, 16)` for north, here rounded out to
        // whole pixels: y 4..13.
        Mount::Wall { facing } => bx([0, 4, 0], [16, 13, 2]).turned(facing),
        // `Block.column(14, 2, 0, 10)` for the four straight rotations,
        // otherwise `Block.column(10, 0, 16)`.
        Mount::Hanging { rotation, .. } => match rotation {
            0 | 8 => bx([1, 0, 7], [15, 10, 9]),
            4 | 12 => bx([7, 0, 1], [9, 10, 15]),
            _ => bx([3, 0, 3], [13, 16, 13]),
        },
        Mount::WallHanging { facing } => {
            if facing.along_x() {
                bx([6, 14, 0], [10, 16, 16])
            } else {
                bx([0, 14, 6], [16, 16, 10])
            }
        }
    });
    if let Mount::WallHanging { facing } = s.mount {
        out.push(if facing.along_x() { bx([7, 0, 1], [9, 10, 15]) } else { bx([1, 0, 7], [15, 10, 9]) });
    }
    Some(out)
}

/// Wall hanging signs collide with their plank (`SHAPES_PLANK`); other signs not at all.
pub fn collision(b: Block) -> Option<Boxes> {
    let s = sign(b)?;
    Some(match s.mount {
        Mount::WallHanging { facing } => Boxes::from_box(if facing.along_x() {
            Box16 { min: [6, 14, 0], max: [10, 16, 16] }
        } else {
            Box16 { min: [0, 14, 6], max: [16, 16, 10] }
        }),
        _ => Boxes::new(),
    })
}

/// Java `isSolid` for sign support: blocks that block motion (signs
/// themselves count, through `forceSolidOn`).
fn solid(b: Block) -> bool {
    b.is_solid()
}

/// Whether a hanging sign can hang from `above` (`isFaceSturdy(DOWN, CENTER)`).
/// Hanging signs override `getBlockSupportShape` with their outline, whose
/// board covers the centre of the underside, so they hold one another.
fn holds_hanging(above: Block) -> bool {
    matches!(sign(above), Some(Sign { mount: Mount::Hanging { .. } | Mount::WallHanging { .. }, .. }))
        || super::super::mount_states::supports_center(above, IVec3::NEG_Y)
}

/// `WallHangingSignBlock.canAttachTo`: another wall hanging sign along the
/// same axis, or a full sturdy face.
fn wall_hanging_attaches(sign_facing: Facing, neighbour: Block, toward_sign: IVec3) -> bool {
    match sign(neighbour) {
        Some(Sign { mount: Mount::WallHanging { facing }, .. }) => facing.along_x() == sign_facing.along_x(),
        _ => super::super::mount_states::face_sturdy(neighbour, toward_sign),
    }
}

/// Whether the sign `s` at `at` still has its support (`canSurvive`).
/// `WallHangingSignBlock` inherits the always-true `canSurvive`, so once
/// placed it stays when its sides go; only [`can_place`] tests them.
pub fn survives(s: Sign, at: IVec3, get: &impl Fn(IVec3) -> Option<Block>) -> bool {
    match s.mount {
        Mount::Standing { .. } => get(at - IVec3::Y).is_some_and(solid),
        Mount::Wall { facing } => get(at - facing.offset()).is_some_and(solid),
        Mount::Hanging { .. } => get(at + IVec3::Y).is_some_and(holds_hanging),
        Mount::WallHanging { .. } => true,
    }
}

/// Whether the sign `s` may be placed at `at`: `canSurvive`, plus
/// `WallHangingSignBlock.canPlace` (either side along the plank holds it).
pub fn can_place(s: Sign, at: IVec3, get: &impl Fn(IVec3) -> Option<Block>) -> bool {
    match s.mount {
        Mount::WallHanging { facing } => {
            let side = facing.clockwise().offset();
            [side, -side].into_iter().any(|d| get(at + d).is_some_and(|n| wall_hanging_attaches(facing, n, -d)))
        }
        _ => survives(s, at, get),
    }
}

/// The face of `at` a sign there leans on, if its support is in that direction.
pub fn support_directions(s: Sign) -> [Option<IVec3>; 2] {
    match s.mount {
        Mount::Standing { .. } => [Some(IVec3::NEG_Y), None],
        Mount::Wall { facing } => [Some(-facing.offset()), None],
        Mount::Hanging { .. } => [Some(IVec3::Y), None],
        Mount::WallHanging { facing } => {
            let side = facing.clockwise().offset();
            [Some(side), Some(-side)]
        }
    }
}

/// The state the sign item `raw` places (Java's `StandingAndWallBlockItem`
/// with `SignItem` (attachment down) or `HangingSignItem` (up)): `None` if
/// `raw` is not a sign, `Some(None)` if nothing can hold it.
pub fn placed(raw: Block, ctx: &PlaceContext, get: &impl Fn(IVec3) -> Option<Block>) -> Option<Option<Block>> {
    let s = sign(raw)?;
    let at = ctx.at;
    let nearest = super::super::mount_states::context_looking(ctx);
    let rotation = segment(look_yaw(ctx.look) + 180.0);
    let kind = s.kind;
    let fits = |s: Sign| can_place(s, at, get);
    let state = if s.hanging() {
        // `WallHangingSignBlock.getStateForPlacement`: the first horizontal
        // look direction not along the clicked face's axis whose state survives.
        let wall = nearest
            .iter()
            .filter(|d| d.y == 0 && !(d.x != 0 && ctx.face.x != 0 || d.z != 0 && ctx.face.z != 0))
            .find_map(|&d| {
                let facing = Facing::from_offset(d)?.opposite();
                let w = Sign { kind, mount: Mount::WallHanging { facing }, wet: false };
                fits(w).then_some(w)
            });
        nearest.iter().filter(|d| **d != IVec3::NEG_Y).find_map(|&d| {
            if d == IVec3::Y {
                let h = hanging_placement(kind, ctx, get, rotation);
                fits(h).then_some(h)
            } else {
                wall
            }
        })
    } else {
        // `WallSignBlock.getStateForPlacement`: the first horizontal look
        // direction whose wall survives.
        let wall = nearest.iter().filter(|d| d.y == 0).find_map(|&d| {
            let facing = Facing::from_offset(d)?.opposite();
            let w = Sign { kind, mount: Mount::Wall { facing }, wet: false };
            fits(w).then_some(w)
        });
        nearest.iter().filter(|d| **d != IVec3::Y).find_map(|&d| {
            if d == IVec3::NEG_Y {
                let standing = Sign { kind, mount: Mount::Standing { rotation }, wet: false };
                fits(standing).then_some(standing)
            } else {
                wall
            }
        })
    };
    Some(state.map(Sign::block))
}

/// `CeilingHangingSignBlock.getStateForPlacement`.
fn hanging_placement(kind: Kind, ctx: &PlaceContext, get: &impl Fn(IVec3) -> Option<Block>, rotation: u8) -> Sign {
    let above = get(ctx.at + IVec3::Y).unwrap_or(Block::AIR);
    let look = look_yaw(ctx.look);
    let direction = from_yaw(look);
    // Not hanging from a full bottom face (or sneaking) hangs from the middle.
    let full_bottom = {
        let neighbour = |f: Facing| get(ctx.at + IVec3::Y + f.offset()).unwrap_or(Block::AIR);
        let below = ctx.at;
        let boxes = super::super::shape::collision_or_cube(
            above,
            neighbour,
            get(below).unwrap_or(Block::AIR),
            super::super::shape::Above::NONE,
        );
        boxes
            .as_slice()
            .iter()
            .any(|b| b.min[1] == 0 && b.min[0] == 0 && b.min[2] == 0 && b.max[0] >= 16 && b.max[2] >= 16)
    };
    let mut attached = !full_bottom || ctx.sneaking;
    if let Some(a) = sign(above).filter(|a| a.hanging())
        && !ctx.sneaking
    {
        let same_axis = match a.mount {
            Mount::WallHanging { facing } => facing.along_x() == direction.along_x(),
            Mount::Hanging { rotation, .. } => match rotation {
                0 | 8 => !direction.along_x(),
                4 | 12 => direction.along_x(),
                _ => false,
            },
            _ => false,
        };
        if same_axis {
            attached = false;
        }
    }
    let rotation = if attached { rotation } else { facing_segment(direction.opposite()) };
    Sign { kind, mount: Mount::Hanging { rotation, attached }, wet: false }
}

/// Java's `Direction.fromYRot`.
pub fn from_yaw(degrees: f32) -> Facing {
    match ((degrees / 90.0 + 0.5).floor() as i32).rem_euclid(4) {
        0 => Facing::South,
        1 => Facing::West,
        2 => Facing::North,
        _ => Facing::East,
    }
}

/// Java's `RotationSegment.convertToSegment(Direction)`.
pub const fn facing_segment(f: Facing) -> u8 {
    match f {
        Facing::South => 0,
        Facing::West => 4,
        Facing::North => 8,
        Facing::East => 12,
    }
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    Kind::ALL.into_iter().flat_map(|kind| {
        [Mount::Standing { rotation: 0 }, Mount::Hanging { rotation: 0, attached: false }]
            .map(|mount| Sign { kind, mount, wet: false }.block().0)
    })
}

/// The sign items' default states, standing then hanging, by kind.
pub fn items() -> impl Iterator<Item = Block> {
    palette_ids().map(Block)
}
