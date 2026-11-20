//! v0.16 Track C: mob heads and skulls (Java 26.3 `SkullBlock`,
//! `WallSkullBlock`, `PiglinWallSkullBlock`, `WitherSkullBlock`,
//! `WitherWallSkullBlock`, `SkullBlockEntity` and
//! `StandingAndWallBlockItem`). See the dated verification record for the
//! sources and the differences from Java.
//!
//! Block states 12544–12723 (Track C's reservation, 12544–12799):
//! - 12544–12683: unpowered heads, `12544 + kind * 20 + local`, kind in
//!   `SkullBlock.Types` order (skeleton, wither skeleton, player, zombie,
//!   creeper, piglin, dragon); `local` 0–15 is a floor head's `rotation`,
//!   16–19 a wall head's `facing` in [`Facing::ALL`] order (south, north,
//!   east, west).
//! - 12684–12723: powered piglin heads, then powered dragon heads, with the
//!   same `local` layout. Java registers `powered` on every head, but only
//!   these two animate when powered; the other kinds' powered states (which
//!   look and act the same) are not registered, so the reservation holds
//!   the beacon and wither rose as well.
//!
//! Heads have no chunk mesh: they draw as block-entity models (their
//! rotations aren't axis-aligned and the dragon's reaches past the cell),
//! as Java's `SkullBlockRenderer` draws them. Texture layers 669–705 are
//! original art.
use glam::{IVec3, Vec3};

use super::block::{Block, Facing, RenderKind};
use super::colour_blocks::banner::Quad;
use super::shape::{Box16, Boxes};

pub const FIRST: u16 = 12544;
const PER_KIND: u16 = 20;
pub const POWERED_FIRST: u16 = FIRST + 7 * PER_KIND;
pub const LAST: u16 = POWERED_FIRST + 2 * PER_KIND - 1;
const _: () = assert!(POWERED_FIRST == 12684 && LAST == 12723);

/// Texture layers: per kind its front, back, side, top and bottom, then
/// the piglin's snout-and-ears layer and the dragon's horn-and-jaw layer.
pub const TEX: u16 = 669;
pub const PIGLIN_DETAIL: u16 = TEX + 35;
pub const DRAGON_DETAIL: u16 = TEX + 36;
pub const TEX_END: u16 = TEX + 37;
const _: () = assert!(TEX >= 669 && TEX_END <= 749);

/// `SkullBlock.Types`, in Java's order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Skeleton,
    WitherSkeleton,
    Player,
    Zombie,
    Creeper,
    Piglin,
    Dragon,
}

impl Kind {
    pub const ALL: [Kind; 7] =
        [Kind::Skeleton, Kind::WitherSkeleton, Kind::Player, Kind::Zombie, Kind::Creeper, Kind::Piglin, Kind::Dragon];

    /// The kinds Java's `SkullBlockEntity.animation` animates while powered.
    pub const fn animated(self) -> bool {
        matches!(self, Kind::Piglin | Kind::Dragon)
    }

    /// Java's block id of the floor head.
    pub const fn java_name(self) -> &'static str {
        match self {
            Kind::Skeleton => "skeleton_skull",
            Kind::WitherSkeleton => "wither_skeleton_skull",
            Kind::Player => "player_head",
            Kind::Zombie => "zombie_head",
            Kind::Creeper => "creeper_head",
            Kind::Piglin => "piglin_head",
            Kind::Dragon => "dragon_head",
        }
    }

    /// Java's block id of the wall head.
    pub const fn java_wall_name(self) -> &'static str {
        match self {
            Kind::Skeleton => "skeleton_wall_skull",
            Kind::WitherSkeleton => "wither_skeleton_wall_skull",
            Kind::Player => "player_wall_head",
            Kind::Zombie => "zombie_wall_head",
            Kind::Creeper => "creeper_wall_head",
            Kind::Piglin => "piglin_wall_head",
            Kind::Dragon => "dragon_wall_head",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Kind::Skeleton => "skeleton skull",
            Kind::WitherSkeleton => "wither skeleton skull",
            Kind::Player => "player head",
            Kind::Zombie => "zombie head",
            Kind::Creeper => "creeper head",
            Kind::Piglin => "piglin head",
            Kind::Dragon => "dragon head",
        }
    }

    const fn wall_name(self) -> &'static str {
        match self {
            Kind::Skeleton => "skeleton wall skull",
            Kind::WitherSkeleton => "wither skeleton wall skull",
            Kind::Player => "player wall head",
            Kind::Zombie => "zombie wall head",
            Kind::Creeper => "creeper wall head",
            Kind::Piglin => "piglin wall head",
            Kind::Dragon => "dragon wall head",
        }
    }

    /// `Items.*` rarity: 1 uncommon, 2 rare (the wither skeleton skull),
    /// 3 epic (the dragon head).
    pub const fn rarity(self) -> u8 {
        match self {
            Kind::WitherSkeleton => 2,
            Kind::Dragon => 3,
            _ => 1,
        }
    }

    /// The head a charged creeper's blast knocks off this mob
    /// (`loot_table/charged_creeper/*`).
    pub fn of_mob(kind: crate::entity::MobKind) -> Option<Kind> {
        use crate::entity::MobKind as M;
        Some(match kind {
            M::Skeleton => Kind::Skeleton,
            M::WitherSkeleton => Kind::WitherSkeleton,
            M::Zombie => Kind::Zombie,
            M::Creeper => Kind::Creeper,
            M::Piglin => Kind::Piglin,
            _ => return None,
        })
    }

    /// `Items.*`'s `loweredMobVisibility`: the mobs that see a wearer of
    /// this head at half range (`MobVisibility`, 0.5).
    pub fn disguises_from(self, mob: crate::entity::MobKind) -> bool {
        use crate::entity::MobKind as M;
        matches!(
            (self, mob),
            (Kind::Skeleton, M::Skeleton)
                | (Kind::Zombie, M::Zombie)
                | (Kind::Creeper, M::Creeper)
                | (Kind::Piglin, M::Piglin | M::PiglinBrute)
        )
    }
}

/// Where a head sits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mount {
    /// `SkullBlock.ROTATION`, sixteenths of a turn; 0 faces north.
    Floor(u8),
    /// `WallSkullBlock.FACING`: the way the head looks, away from its wall.
    Wall(Facing),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Skull {
    pub kind: Kind,
    pub mount: Mount,
    /// `AbstractSkullBlock.POWERED`; only stored for animated kinds.
    pub powered: bool,
}

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

const fn local(mount: Mount) -> u16 {
    match mount {
        Mount::Floor(r) => (r & 15) as u16,
        Mount::Wall(f) => 16 + f as u16,
    }
}

pub const fn encode(s: Skull) -> Block {
    let powered = s.powered && s.kind.animated();
    if powered {
        let k = if matches!(s.kind, Kind::Piglin) { 0 } else { 1 };
        Block(POWERED_FIRST + k * PER_KIND + local(s.mount))
    } else {
        Block(FIRST + s.kind as u16 * PER_KIND + local(s.mount))
    }
}

pub const fn decode(b: Block) -> Option<Skull> {
    if !owns(b.0) {
        return None;
    }
    let (kind, powered, l) = if b.0 >= POWERED_FIRST {
        let i = b.0 - POWERED_FIRST;
        (if i < PER_KIND { Kind::Piglin } else { Kind::Dragon }, true, i % PER_KIND)
    } else {
        let i = b.0 - FIRST;
        (Kind::ALL[(i / PER_KIND) as usize], false, i % PER_KIND)
    };
    let mount = if l < 16 { Mount::Floor(l as u8) } else { Mount::Wall(Facing::ALL[(l - 16) as usize]) };
    Some(Skull { kind, mount, powered })
}

pub const fn floor(kind: Kind, rotation: u8) -> Block {
    encode(Skull { kind, mount: Mount::Floor(rotation), powered: false })
}

pub const fn wall(kind: Kind, facing: Facing) -> Block {
    encode(Skull { kind, mount: Mount::Wall(facing), powered: false })
}

/// The state items, drops and the creative palette use: the floor head
/// facing north, unpowered.
pub const fn item_block(kind: Kind) -> Block {
    floor(kind, 0)
}

pub const WITHER_SKELETON_SKULL: Block = item_block(Kind::WitherSkeleton);
pub const DRAGON_HEAD: Block = item_block(Kind::Dragon);

/// Whether `b` is a wither skeleton skull, on the floor or a wall.
pub fn is_wither_skull(b: Block) -> bool {
    decode(b).is_some_and(|s| s.kind == Kind::WitherSkeleton)
}

/// The head's item, as `StandingAndWallBlockItem.registerBlocks` maps both
/// blocks to it. The wither skeleton skull keeps the item id it had before
/// it was placeable (641).
pub fn item(kind: Kind) -> crate::item::Item {
    crate::item::Item::from_block(item_block(kind))
}

/// The head kind of an item, if it is one.
pub fn kind_of_item(item: crate::item::Item) -> Option<Kind> {
    item.block().and_then(decode).map(|s| s.kind)
}

const fn layer(kind: Kind, face: u16) -> u16 {
    TEX + kind as u16 * 5 + face
}

/// Name, render kind and the layers the item icon draws (+X, -X, +Y, -Y,
/// +Z, -Z; the face on +Z).
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    let Some(s) = decode(Block(id)) else { return ("unknown", RenderKind::Invisible, [0; 6]) };
    let k = s.kind;
    let name = match s.mount {
        Mount::Floor(_) => k.name(),
        Mount::Wall(_) => k.wall_name(),
    };
    let side = layer(k, 2);
    (name, RenderKind::Shaped, [side, side, layer(k, 3), layer(k, 4), layer(k, 0), layer(k, 1)])
}

/// Every head's floor state for the creative palette, in Java's order.
/// The wither skeleton skull's item (641) is already among the items.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    Kind::ALL.into_iter().filter(|&k| k != Kind::WitherSkeleton).map(|k| item_block(k).0)
}

/// `SkullBlock`'s collision (`Block.column(8, 0, 8)`, the piglin's
/// `column(10, 0, 8)`) and `WallSkullBlock`'s `boxZ(8, 8, 16)` (the
/// piglin's `boxZ(10, 8, 8, 16)`), turned to the facing.
pub fn collision(s: Skull) -> Boxes {
    let half = if s.kind == Kind::Piglin { 5 } else { 4 };
    match s.mount {
        Mount::Floor(_) => Boxes::from_box(Box16 { min: [8 - half, 0, 4], max: [8 + half, 8, 12] }),
        Mount::Wall(f) => {
            // Facing north the head sits against the south side.
            let north = Box16 { min: [8 - half, 4, 8], max: [8 + half, 12, 16] };
            Boxes::from_box(north.turned(f.opposite()))
        }
    }
}

/// `getShape`: the dragon's floor head outlines as `column(8, 0, 8.5)`
/// (rounded up to whole pixels); every other shape is the collision.
pub fn outline(s: Skull) -> Boxes {
    match (s.kind, s.mount) {
        (Kind::Dragon, Mount::Floor(_)) => Boxes::from_box(Box16 { min: [4, 0, 4], max: [12, 9, 12] }),
        _ => collision(s),
    }
}

/// The boxes items draw: a full cube, so the head's face fills the icon
/// as Java's head items do.
pub fn item_boxes() -> Boxes {
    Boxes::from_box(Box16::FULL)
}

/// `RotationSegment.convertToSegment(yRot)`: `Math.round(yRot * 16 / 360) & 15`.
pub fn rotation_for(y_rot: f32) -> u8 {
    ((y_rot / 22.5 + 0.5).floor() as i32 & 15) as u8
}

/// The state a head item places at `ctx.at` (`StandingAndWallBlockItem`
/// with attachment `DOWN`): the floor head when the nearest look direction
/// (after skipping up) is down, else the wall head `WallSkullBlock` finds
/// against the first horizontal look direction with a non-replaceable
/// block. `None` if `raw` isn't a head, `Some(None)` if nothing fits.
/// `powered` is `hasNeighborSignal` at the cell.
pub fn placed(
    raw: Block,
    ctx: &crate::session::placement::PlaceContext,
    powered: bool,
    get: impl Fn(IVec3) -> Option<Block>,
) -> Option<Option<Block>> {
    let kind = decode(raw)?.kind;
    let nearest = super::mount_states::context_looking(ctx);
    let wall_facing = nearest
        .iter()
        .filter(|d| d.y == 0)
        .find(|&&d| get(ctx.at + d).is_some_and(|b| !b.is_replaceable()))
        .and_then(|&d| Facing::from_offset(-d));
    let horizontal = glam::Vec3::new(ctx.look.x, 0.0, ctx.look.z);
    let look = if horizontal.length_squared() > 1e-8 { horizontal } else { ctx.facing.offset().as_vec3() };
    let y_rot = super::colour_blocks::banner::java_y_rot(look);
    let state = nearest
        .iter()
        .filter(|d| d.y <= 0)
        .find_map(|&d| if d.y < 0 { Some(Mount::Floor(rotation_for(y_rot))) } else { wall_facing.map(Mount::Wall) });
    Some(state.map(|mount| encode(Skull { kind, mount, powered })))
}

/// The note block instrument a head above a note block sets
/// (`NoteBlockInstrument`'s mob-head entries); the player head is
/// `CUSTOM_HEAD`, which plays only a profile's `note_block_sound`.
pub fn note_instrument(b: Block) -> Option<Option<crate::world::gadgets::Instrument>> {
    use crate::world::gadgets::Instrument as I;
    let s = decode(b)?;
    Some(match s.kind {
        Kind::Skeleton => Some(I::Skeleton),
        Kind::WitherSkeleton => Some(I::WitherSkeleton),
        Kind::Zombie => Some(I::Zombie),
        Kind::Creeper => Some(I::Creeper),
        Kind::Piglin => Some(I::Piglin),
        Kind::Dragon => Some(I::Dragon),
        Kind::Player => None,
    })
}

/// The turn about the vertical axis our models use (`BlockModel::yaw`:
/// 0 faces south, a quarter turn west): `SkullBlockRenderer` turns a floor
/// head by `-rotation * 22.5` and its model faces north, a wall head by
/// its facing.
pub fn yaw_degrees(mount: Mount) -> f32 {
    match mount {
        Mount::Floor(r) => r as f32 * 22.5 + 180.0,
        Mount::Wall(f) => match f {
            Facing::South => 0.0,
            Facing::West => 90.0,
            Facing::North => 180.0,
            Facing::East => 270.0,
        },
    }
}

/// One box of a head model in head pixels (x right, y up, z toward the
/// face), hinged about `pivot` by `pitch` about x and `roll` about z.
struct Part {
    min: [f32; 3],
    max: [f32; 3],
    pivot: [f32; 3],
    pitch: f32,
    roll: f32,
    /// Layers per face: +X, -X, +Y, -Y, +Z (front), -Z (back).
    tex: [u16; 6],
}

fn plain(min: [f32; 3], max: [f32; 3], tex: [u16; 6]) -> Part {
    Part { min, max, pivot: [0.0; 3], pitch: 0.0, roll: 0.0, tex }
}

/// The model's parts at animation position `anim` (ticks, with the
/// fraction): `SkullModel`'s 8 px cube, `PiglinHeadModel` (Java's piglin
/// head with its snout, tusks and ears, which flap as
/// `-(cos(a·π·0.24) + 2.5)·0.2` and `(cos(a·π·0.2) + 2.5)·0.2`) and
/// `DragonHeadModel` at its 0.75 scale (whose jaw opens by
/// `(sin(a·π·0.2) + 1)·0.2`).
fn parts(kind: Kind, anim: f32) -> Vec<Part> {
    let k = kind;
    let head = [layer(k, 2), layer(k, 2), layer(k, 3), layer(k, 4), layer(k, 0), layer(k, 1)];
    match kind {
        Kind::Piglin => {
            let d = [PIGLIN_DETAIL; 6];
            let pi = std::f32::consts::PI;
            let left = -((anim * pi * 0.2 * 1.2).cos() + 2.5) * 0.2;
            let right = ((anim * pi * 0.2).cos() + 2.5) * 0.2;
            vec![
                plain([-5.0, 0.0, -4.0], [5.0, 8.0, 4.0], head),
                // Snout and tusks on the face (+Z).
                plain([-2.0, 0.0, 4.0], [2.0, 4.0, 5.0], d),
                plain([2.0, 0.0, 4.0], [3.0, 2.0, 5.0], d),
                plain([-3.0, 0.0, 4.0], [-2.0, 2.0, 5.0], d),
                // Ears hang from the top sides. Model x is mirrored from
                // Java's (its model faces -Z), so the left ear is on -x.
                Part {
                    min: [-5.5, -3.0, -2.0],
                    max: [-4.5, 2.0, 2.0],
                    pivot: [-5.0, 6.0, 0.0],
                    pitch: 0.0,
                    roll: left,
                    tex: d,
                },
                Part {
                    min: [4.5, -3.0, -2.0],
                    max: [5.5, 2.0, 2.0],
                    pivot: [5.0, 6.0, 0.0],
                    pitch: 0.0,
                    roll: right,
                    tex: d,
                },
            ]
        }
        Kind::Dragon => {
            let d = [DRAGON_DETAIL; 6];
            let jaw = ((anim * std::f32::consts::PI * 0.2).sin() + 1.0) * 0.2;
            vec![
                // upper_head 16³ and upper_lip 12×5×16 at 0.75.
                plain([-6.0, 0.0, -4.5], [6.0, 12.0, 7.5], head),
                plain([-4.5, 4.5, 7.5], [4.5, 8.25, 19.5], head),
                // Nostrils and scales (horns).
                plain([-3.75, 8.25, 16.5], [-2.25, 9.75, 19.5], d),
                plain([2.25, 8.25, 16.5], [3.75, 9.75, 19.5], d),
                plain([-3.75, 12.0, -1.5], [-2.25, 15.0, 3.0], d),
                plain([2.25, 12.0, -1.5], [3.75, 15.0, 3.0], d),
                // The jaw hinges under the lip and drops open.
                Part {
                    min: [-4.5, -3.0, 0.0],
                    max: [4.5, 0.0, 12.0],
                    pivot: [0.0, 4.5, 7.5],
                    pitch: jaw,
                    roll: 0.0,
                    tex: d,
                },
            ]
        }
        _ => vec![plain([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], head)],
    }
}

/// The head's quads, relative to its cell's minimum corner (blocks), at
/// animation position `anim`: floor heads stand on the cell's floor
/// centre, wall heads sit a quarter block up against their wall
/// (`SkullBlockRenderer`'s `(0.5 - step * 0.25, 0.25, 0.5 - step * 0.25)`).
pub fn quads(s: Skull, anim: f32) -> Vec<Quad> {
    let origin = match s.mount {
        Mount::Floor(_) => Vec3::new(0.5, 0.0, 0.5),
        Mount::Wall(f) => {
            let back = f.opposite().offset().as_vec3();
            Vec3::new(0.5, 0.25, 0.5) + back * 0.25
        }
    };
    let (sin, cos) = yaw_degrees(s.mount).to_radians().sin_cos();
    let turn = |v: Vec3| Vec3::new(v.x * cos - v.z * sin, v.y, v.x * sin + v.z * cos);
    let mut out = Vec::new();
    for part in parts(s.kind, anim) {
        let (ps, pc) = part.pitch.sin_cos();
        let (rs, rc) = part.roll.sin_cos();
        let pivot = Vec3::from_array(part.pivot);
        // Hinge: the box is placed relative to its pivot, then pitched
        // (positive opens downward) and rolled about the pivot.
        let pose = |p: Vec3| {
            if part.pitch == 0.0 && part.roll == 0.0 && part.pivot == [0.0; 3] {
                return p;
            }
            let p = Vec3::new(p.x, p.y * pc + p.z * ps, -p.y * ps + p.z * pc);
            let p = Vec3::new(p.x * rc - p.y * rs, p.x * rs + p.y * rc, p.z);
            p + pivot
        };
        let pose_n = |n: Vec3| {
            let n = Vec3::new(n.x, n.y * pc + n.z * ps, -n.y * ps + n.z * pc);
            Vec3::new(n.x * rc - n.y * rs, n.x * rs + n.y * rc, n.z)
        };
        let (lo, hi) = (Vec3::from_array(part.min), Vec3::from_array(part.max));
        for (face, &layer) in part.tex.iter().enumerate() {
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
            let size = hi - lo;
            // Upright, unmirrored as seen from outside each face.
            let uv = model.map(|p| {
                let f = (p - lo) / size;
                match (d, positive) {
                    (0, true) => [1.0 - f.z, 1.0 - f.y],
                    (0, false) => [f.z, 1.0 - f.y],
                    (2, true) => [f.x, 1.0 - f.y],
                    (2, false) => [1.0 - f.x, 1.0 - f.y],
                    _ => [f.x, f.z],
                }
            });
            let mut normal = Vec3::ZERO;
            normal[d] = if positive { 1.0 } else { -1.0 };
            out.push(Quad {
                corners: model.map(|p| origin + turn(pose(p)) / 16.0),
                uv,
                layer,
                normal: turn(pose_n(normal)),
                tint: None,
            });
        }
    }
    out
}

/// Original head art on an 8 × 8 grid drawn at two texels a cell: the face
/// (`f`), back (`b`), sides, top and bottom of each kind, in the colours
/// of VoxelCraft's own mob models, and the piglin's and dragon's detail.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let i = layer - TEX;
    let (cx, cy) = (x / 2, y / 2);
    let n = ((cx * 7 + cy * 13 + (cx ^ cy) * 5 + i as usize * 3) % 9) as i32 - 4;
    let shade = |c: [i32; 3], k: i32| -> [u8; 4] {
        [(c[0] + k).clamp(0, 255) as u8, (c[1] + k).clamp(0, 255) as u8, (c[2] + k).clamp(0, 255) as u8, 255]
    };
    if layer == PIGLIN_DETAIL {
        // Snout pink with darker nostrils; ears share it.
        let nostril = (cy == 3 || cy == 4) && (cx == 2 || cx == 5);
        return if nostril { shade([150, 82, 80], n) } else { shade([232, 160, 150], n * 2) };
    }
    if layer == DRAGON_DETAIL {
        let ridge = (cx + cy) % 4 == 0;
        return if ridge { shade([70, 64, 78], n) } else { shade([34, 30, 40], n * 2) };
    }
    let kind = Kind::ALL[(i / 5) as usize];
    let face = i % 5;
    // Base skin, a darker accent and the feature colour.
    let (skin, dark, feature): ([i32; 3], [i32; 3], [i32; 3]) = match kind {
        Kind::Skeleton => ([214, 214, 208], [150, 150, 146], [40, 40, 42]),
        Kind::WitherSkeleton => ([58, 58, 60], [36, 36, 38], [14, 14, 16]),
        Kind::Player => ([196, 150, 110], [70, 46, 30], [60, 80, 160]),
        Kind::Zombie => ([92, 146, 74], [64, 108, 52], [24, 40, 24]),
        Kind::Creeper => ([94, 172, 80], [70, 140, 60], [20, 26, 20]),
        Kind::Piglin => ([226, 150, 140], [196, 118, 110], [40, 30, 30]),
        Kind::Dragon => ([42, 38, 50], [28, 26, 34], [196, 90, 220]),
    };
    let mottle = n * if matches!(kind, Kind::Creeper | Kind::Zombie) { 5 } else { 2 };
    let (cx, cy) = (cx as i32, cy as i32);
    match face {
        // Front.
        0 => {
            let eye = (cy == 3 || (cy == 2 && kind == Kind::Creeper)) && (cx == 1 || cx == 2 || cx == 5 || cx == 6);
            let hair = kind == Kind::Player && cy <= 1;
            let c = match kind {
                Kind::Creeper if eye => feature,
                Kind::Creeper if cy == 4 && (cx == 3 || cx == 4) => feature,
                Kind::Creeper if (cy == 5 || cy == 6) && (2..=5).contains(&cx) => feature,
                Kind::Creeper if cy == 7 && (cx == 2 || cx == 5) => feature,
                Kind::Skeleton | Kind::WitherSkeleton if eye || (cy == 4 && eye_row(cx)) => feature,
                Kind::Skeleton | Kind::WitherSkeleton if cy == 5 && (cx == 3 || cx == 4) => dark,
                Kind::Skeleton | Kind::WitherSkeleton if cy == 6 && (1..=6).contains(&cx) => {
                    if cx % 2 == 0 {
                        dark
                    } else {
                        skin
                    }
                }
                Kind::Player if hair => dark,
                Kind::Player if cy == 4 && (cx == 1 || cx == 6) => [236, 236, 236],
                Kind::Player if cy == 4 && (cx == 2 || cx == 5) => feature,
                Kind::Player if cy == 6 && (cx == 3 || cx == 4) => [120, 70, 60],
                Kind::Zombie if cy == 3 && (cx == 1 || cx == 2 || cx == 5 || cx == 6) => feature,
                Kind::Zombie if cy == 6 && (2..=5).contains(&cx) => dark,
                Kind::Piglin if cy == 2 && (cx == 1 || cx == 2 || cx == 5 || cx == 6) => {
                    if cx == 1 || cx == 6 {
                        [236, 236, 236]
                    } else {
                        feature
                    }
                }
                Kind::Dragon if cy == 2 && (cx == 1 || cx == 6) => feature,
                Kind::Dragon if (cy + cx) % 3 == 0 => dark,
                _ => skin,
            };
            shade(c, mottle)
        }
        // Back: hair for the player, plain elsewhere.
        1 => shade(if kind == Kind::Player { dark } else { skin }, mottle - 6),
        // Sides: a player's hair comes down the upper rows.
        2 => {
            let c =
                if (kind == Kind::Player && cy <= 2) || (kind == Kind::Dragon && cy % 3 == 0) { dark } else { skin };
            shade(c, mottle - 3)
        }
        // Top.
        3 => shade(if matches!(kind, Kind::Player | Kind::Dragon) { dark } else { skin }, mottle + 4),
        _ => shade(dark, mottle - 8),
    }
}

/// A skull's nose holes sit between the eye columns.
const fn eye_row(cx: i32) -> bool {
    cx == 1 || cx == 2 || cx == 5 || cx == 6
}

impl super::World {
    /// `AbstractSkullBlock.neighborChanged`: a head follows the redstone
    /// signal at its cell; only animated kinds store it.
    pub(super) fn skull_neighbour(&mut self, p: IVec3, s: Skull) {
        if !s.kind.animated() {
            return;
        }
        let signal = self.redstone_power(p) > 0;
        if signal != s.powered {
            self.edit(p, encode(Skull { powered: signal, ..s }), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_in_the_reservation() {
        let mut seen = std::collections::HashSet::new();
        for kind in Kind::ALL {
            for powered in [false, true] {
                let mounts = (0..16).map(Mount::Floor).chain(Facing::ALL.map(Mount::Wall));
                for mount in mounts {
                    let s = Skull { kind, mount, powered };
                    let b = encode(s);
                    assert!(owns(b.0), "{s:?}");
                    let back = decode(b).unwrap();
                    assert_eq!(back, Skull { powered: powered && kind.animated(), ..s });
                    seen.insert(b.0);
                }
            }
        }
        // 7 kinds × 20 unpowered, plus 2 × 20 powered.
        assert_eq!(seen.len(), 180);
        assert_eq!(*seen.iter().min().unwrap(), FIRST);
        assert_eq!(*seen.iter().max().unwrap(), LAST);
    }

    #[test]
    fn rotations_follow_rotation_segment() {
        assert_eq!(rotation_for(0.0), 0);
        assert_eq!(rotation_for(90.0), 4);
        assert_eq!(rotation_for(-90.0), 12);
        assert_eq!(rotation_for(180.0), 8);
        assert_eq!(rotation_for(11.24), 0);
        assert_eq!(rotation_for(11.26), 1);
    }

    #[test]
    fn shapes_match_java() {
        let floor = Skull { kind: Kind::Zombie, mount: Mount::Floor(3), powered: false };
        assert_eq!(collision(floor).as_slice(), [Box16 { min: [4, 0, 4], max: [12, 8, 12] }]);
        let piglin = Skull { kind: Kind::Piglin, ..floor };
        assert_eq!(collision(piglin).as_slice(), [Box16 { min: [3, 0, 4], max: [13, 8, 12] }]);
        let north = Skull { mount: Mount::Wall(Facing::North), ..floor };
        assert_eq!(collision(north).as_slice(), [Box16 { min: [4, 4, 8], max: [12, 12, 16] }]);
        let south = Skull { mount: Mount::Wall(Facing::South), ..floor };
        assert_eq!(collision(south).as_slice(), [Box16 { min: [4, 4, 0], max: [12, 12, 8] }]);
        let east = Skull { mount: Mount::Wall(Facing::East), ..floor };
        assert_eq!(collision(east).as_slice(), [Box16 { min: [0, 4, 4], max: [8, 12, 12] }]);
        let dragon = Skull { kind: Kind::Dragon, ..floor };
        assert_eq!(outline(dragon).as_slice(), [Box16 { min: [4, 0, 4], max: [12, 9, 12] }]);
    }

    #[test]
    fn floor_heads_face_the_player_who_placed_them() {
        // A player looking south (yRot 0) puts down rotation 0, which faces north.
        let s = Skull { kind: Kind::Skeleton, mount: Mount::Floor(rotation_for(0.0)), powered: false };
        let q = quads(s, 0.0);
        // The front face (+Z in the model) is the fifth quad.
        assert!(q[4].normal.z < -0.99, "{:?}", q[4].normal);
        // A wall head facing east looks east from the west side of its cell.
        let w = Skull { mount: Mount::Wall(Facing::East), ..s };
        let q = quads(w, 0.0);
        assert!(q[4].normal.x > 0.99);
        assert!(q.iter().flat_map(|q| q.corners).all(|c| c.x <= 0.5 + 1e-4 && c.y >= 0.25 - 1e-4));
    }

    #[test]
    fn names_resolve() {
        assert_eq!(Block::from_name("dragon wall head").and_then(decode).map(|s| s.kind), Some(Kind::Dragon));
        assert_eq!(item_block(Kind::Creeper).name(), "creeper head");
        assert_eq!(wall(Kind::Skeleton, Facing::West).name(), "skeleton wall skull");
    }
}
