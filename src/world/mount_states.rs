//! v0.9 door hinges, floor/ceiling buttons and levers, wall and ceiling
//! grindstones and lanterns (BLD-04c/e; Java 26.3 `DoorBlock`,
//! `FaceAttachedHorizontalDirectionalBlock`, `ButtonBlock`, `LeverBlock`,
//! `GrindstoneBlock`, `LanternBlock`), ids 7680–8191.
//!
//! | Ids | Use |
//! | --- | --- |
//! | 7680..=7691 | lever mounts 6..=11 (floor/ceiling facings), off then on |
//! | 7696..=7767 | button mounts 6..=11 for stone, oak, crimson, warped, pale oak, bamboo |
//! | 7800..=7903 | right-hinge lower doors: oak, iron, then the 11 wood doors; 8 states each |
//! | 7904..=7911 | wall then ceiling grindstones, four facings each |
//! | 7920..=7927 | lantern then soul lantern: standing/hanging × dry/wet |
//!
//! Existing ids keep their meaning: the old door ids are the left-hinge
//! states, mounts 0 and 5 are floor/ceiling facing south, mounts 1..=4 are
//! the wall states, and 933..=936 are floor grindstones. Upper door halves
//! store no hinge; like a wood door's upper half they read the lower one.

use super::block::{Block, Facing, RenderKind, tex};
use super::{forms, nether_biome_blocks as nb, redstone_blocks as r, shape};
use crate::session::placement::PlaceContext;
use glam::{IVec3, Vec3};

pub const LEVER_FIRST: u16 = 7680;
pub const BUTTON_FIRST: u16 = 7696;
const BUTTON_STRIDE: u16 = 12;
pub const DOOR_FIRST: u16 = 7800;
const DOOR_FAMILIES: u16 = 13;
pub const GRINDSTONE_FIRST: u16 = 7904;
pub const LANTERN: Block = Block(7920);
pub const SOUL_LANTERN: Block = Block(7924);
const LANTERN_LAST: u16 = 7927;

/// Java's `AttachFace`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachFace {
    Floor,
    Wall,
    Ceiling,
}

/// The face and Java `facing` of a lever or button mount. Floor and ceiling
/// facings are the direction the placer looked; wall facings point away from
/// the wall. Mounts 0 and 5 are the original floor and ceiling states.
pub const fn mount_attach(mount: u8) -> (AttachFace, Facing) {
    match mount {
        0 => (AttachFace::Floor, Facing::South),
        1..=4 => (AttachFace::Wall, Facing::ALL[(mount - 1) as usize]),
        5 => (AttachFace::Ceiling, Facing::South),
        6..=8 => (AttachFace::Floor, Facing::ALL[(mount - 5) as usize]),
        _ => (AttachFace::Ceiling, Facing::ALL[(mount - 8) as usize]),
    }
}

pub const fn attach_mount(face: AttachFace, facing: Facing) -> u8 {
    let i = facing as u8;
    match face {
        AttachFace::Wall => i + 1,
        AttachFace::Floor => {
            if i == 0 {
                0
            } else {
                5 + i
            }
        }
        AttachFace::Ceiling => {
            if i == 0 {
                5
            } else {
                8 + i
            }
        }
    }
}

/// Java's `getConnectedDirection(state).getOpposite()`: from the block toward
/// the block it is attached to.
pub fn attach_direction(face: AttachFace, facing: Facing) -> IVec3 {
    match face {
        AttachFace::Floor => IVec3::NEG_Y,
        AttachFace::Ceiling => IVec3::Y,
        AttachFace::Wall => -facing.offset(),
    }
}

pub const fn lever(mount: u8, on: bool) -> Block {
    Block(LEVER_FIRST + (mount as u16 - 6) * 2 + on as u16)
}

/// Mounts 6..=11 of a stone (`wood` false) or oak button.
pub const fn button(mount: u8, on: bool, wood: bool) -> Block {
    Block(BUTTON_FIRST + wood as u16 * BUTTON_STRIDE + (mount as u16 - 6) * 2 + on as u16)
}

/// The redstone component an id in this module is, if any.
pub const fn component(id: u16) -> Option<r::Component> {
    match id {
        7680..=7691 => {
            let i = id - LEVER_FIRST;
            Some(r::Component::Lever { mount: 6 + (i / 2) as u8, on: i % 2 == 1 })
        }
        7696..=7767 => {
            let i = id - BUTTON_FIRST;
            let l = i % BUTTON_STRIDE;
            Some(r::Component::Button { mount: 6 + (l / 2) as u8, on: l % 2 == 1, wood: i >= BUTTON_STRIDE })
        }
        DOOR_FIRST..=7903 => match door_twin(id) {
            Some(t) => r::component(t),
            None => None,
        },
        _ => None,
    }
}

// ---- Doors -------------------------------------------------------------

/// The left-hinge lower door with the same facing and open state as the
/// right-hinge lower door `id`.
pub const fn door_twin(id: u16) -> Option<Block> {
    if id < DOOR_FIRST || id >= DOOR_FIRST + DOOR_FAMILIES * 8 {
        return None;
    }
    let i = id - DOOR_FIRST;
    let (family, local) = (i / 8, i % 8);
    Some(match family {
        0 => Block::door(Facing::ALL[(local % 4) as usize], local >= 4, false),
        1 => r::iron_door(Facing::ALL[(local % 4) as usize], local >= 4, false),
        n => forms::wood_id(n - 2, 14 + local),
    })
}

/// The right-hinge twin of a left-hinge lower door (`None` for anything else).
pub const fn right_of(left: Block) -> Option<Block> {
    if let Some(right) = super::copper::right_of(left) {
        return Some(right);
    }
    let id = left.0;
    let (family, local) = if id >= 149 && id <= 156 {
        (0, id - 149)
    } else if id >= 1249 && id <= 1256 {
        (1, id - 1249)
    } else {
        match forms::wood_form(id) {
            Some(forms::WoodForm::Door { index, facing, open, upper: false }) => {
                (2 + index, open as u16 * 4 + facing as u16)
            }
            _ => return None,
        }
    };
    Some(Block(DOOR_FIRST + family * 8 + local))
}

/// Java `DoorBlock.HINGE == RIGHT`. Upper halves answer `false`; read the
/// lower half.
pub const fn is_right_hinge(b: Block) -> bool {
    (b.0 >= DOOR_FIRST && b.0 < DOOR_FIRST + DOOR_FAMILIES * 8) || super::copper::is_right_hinge(b.0)
}

/// `to` (a door state computed from `from`) with `from`'s hinge.
pub const fn keep_hinge(from: Block, to: Block) -> Block {
    if is_right_hinge(from) {
        match right_of(to) {
            Some(b) => b,
            None => to,
        }
    } else {
        to
    }
}

/// `DoorBlock.getHinge`: whether a door placed with `ctx` hangs on the
/// right. The hinge goes away from a door beside it (so the two form a
/// double door), away from solid blocks, and otherwise to the side of the
/// clicked half of the cell.
pub fn door_hinge_right(ctx: &PlaceContext, get: impl Fn(IVec3) -> Option<Block>) -> bool {
    let pos = ctx.at;
    let look = ctx.facing;
    let left = look.clockwise().clockwise().clockwise();
    let right = look.clockwise();
    let block = |p: IVec3| get(p).unwrap_or(Block::AIR);
    let full = |b: Block| i32::from(collision_is_full(b));
    let (lp, rp) = (pos + left.offset(), pos + right.offset());
    let (left_state, right_state) = (block(lp), block(rp));
    let balance = -full(left_state) - full(block(lp + IVec3::Y)) + full(right_state) + full(block(rp + IVec3::Y));
    let lower_door = |b: Block| b.is_door() && !b.is_door_upper();
    let (door_left, door_right) = (lower_door(left_state), lower_door(right_state));
    if (door_left && !door_right) || balance > 0 {
        return true;
    }
    if (door_right && !door_left) || balance < 0 {
        return false;
    }
    // The click relative to the placed cell, not the clicked one.
    let click = ctx.click + (ctx.clicked - ctx.at).as_dvec3();
    let step = look.offset();
    let left_hinge = (step.x >= 0 || click.z >= 0.5)
        && (step.x <= 0 || click.z <= 0.5)
        && (step.z >= 0 || click.x <= 0.5)
        && (step.z <= 0 || click.x >= 0.5);
    !left_hinge
}

/// Java `isCollisionShapeFullBlock`.
pub fn collision_is_full(b: Block) -> bool {
    let b = b.canonical();
    b.is_solid()
        && if b.shaped().is_none() {
            b.height() >= 1.0 && !smaller_than_its_cell(b)
        } else {
            shape::collision(b, |_| Block::AIR, Block::AIR).as_slice().contains(&shape::Box16::FULL)
        }
}

/// Unshaped blocks drawn as cubes whose Java shape is smaller than the cell.
fn smaller_than_its_cell(b: Block) -> bool {
    matches!(b.base(), Block::CHEST | Block::CACTUS | Block::HONEY_BLOCK) || super::copper::is_chest(b)
}

// ---- Sturdy faces ------------------------------------------------------

/// Java's `SupportType`: how much of a face a support must cover.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Support {
    /// The whole face.
    Full,
    /// The 2×2 centre.
    Center,
    /// The outer two-pixel ring (`RIGID`, what diodes and plates stand on).
    Rigid,
}

/// Whether the union of `b`'s support boxes covers the face whose outward
/// normal is `dir` (`center_only`: just the 2×2 centre, Java's
/// `SupportType.CENTER`).
fn covers_face(b: Block, dir: IVec3, support: Support) -> bool {
    // A waterlogged partner has its dry original's shape.
    let b = b.canonical();
    // Scaffolding's empty-context support shape has a full top plate even
    // though player placement and inside collision are empty.
    if super::scaffolding::is_scaffolding(b) {
        return dir == IVec3::Y;
    }
    // `LeavesBlock.getBlockSupportShape` is empty.
    if b.is_leaves() || !b.is_solid() {
        return false;
    }
    // `HoneyBlock`'s support shape is its collision box, inset a pixel on
    // the sides and top: only its underside's centre holds anything.
    if b == Block::HONEY_BLOCK {
        return support == Support::Center && dir == IVec3::NEG_Y;
    }
    if b.is_opaque() || matches!(b, Block::SOUL_SAND | super::overworld_blocks::MUD) {
        return true;
    }
    // Every ordinary wall state covers the centre with its post or at
    // least one arm. Arms and posts both have 24-high collision/support
    // shapes. Side connections are derived, so this context-free support
    // query must not treat an UP=false wall as an empty isolated shape.
    if super::wall_states::up(b).is_some() && dir.y != 0 {
        return support == Support::Center;
    }
    if b.shaped().is_none() {
        // A plain cube of reduced height (slab, path, bed, snow layer) is
        // only sturdy on its underside.
        return !smaller_than_its_cell(b) && (b.height() >= 1.0 || dir == IVec3::NEG_Y);
    }
    let axis = if dir.x != 0 {
        0
    } else if dir.y != 0 {
        1
    } else {
        2
    };
    let positive = dir[axis] > 0;
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let boxes = shape::collision(b, |_| Block::AIR, Block::AIR);
    let mut rows = [0u16; 16];
    for bx in boxes.as_slice() {
        if if positive { bx.max[axis] >= 16 } else { bx.min[axis] == 0 } {
            for y in bx.min[v]..bx.max[v].min(16) {
                for x in bx.min[u]..bx.max[u].min(16) {
                    rows[y as usize] |= 1 << x;
                }
            }
        }
    }
    match support {
        Support::Center => (7..9).all(|y| rows[y] >> 7 & 0b11 == 0b11),
        Support::Full => rows.iter().all(|&row| row == u16::MAX),
        Support::Rigid => rows.iter().enumerate().all(|(y, &row)| {
            let ring = if !(2..14).contains(&y) { u16::MAX } else { 0b11 | 0b11 << 14 };
            row & ring == ring
        }),
    }
}

/// Java `isFaceSturdy(.., face, SupportType.FULL)`.
pub fn face_sturdy(b: Block, face: IVec3) -> bool {
    covers_face(b, face, Support::Full)
}

/// Java `Block.canSupportRigidBlock`: the top face covers its outer
/// two-pixel ring (a hopper's rim does; a wall post doesn't).
pub fn supports_rigid(b: Block) -> bool {
    covers_face(b, IVec3::Y, Support::Rigid)
}

/// Java `Block.canSupportCenter`: fence posts, chains and slab tops hold a
/// torch or lantern, but nothing hangs from a fence gate.
pub fn supports_center(b: Block, face: IVec3) -> bool {
    if face == IVec3::NEG_Y && b.is_gate() {
        return false;
    }
    covers_face(b, face, Support::Center)
}

// ---- Placement ---------------------------------------------------------

/// Java `Direction.orderedByNearest`: the six directions by how closely they
/// follow `look`, nearest first.
pub fn nearest_looking(look: Vec3) -> [IVec3; 6] {
    let (ax, ay, az) = (look.x.abs(), look.y.abs(), look.z.abs());
    let dx = if look.x > 0.0 { IVec3::X } else { IVec3::NEG_X };
    let dy = if look.y > 0.0 { IVec3::Y } else { IVec3::NEG_Y };
    let dz = if look.z > 0.0 { IVec3::Z } else { IVec3::NEG_Z };
    let (a, b, c) = if ax > az {
        if ay > ax {
            (dy, dx, dz)
        } else if az > ay {
            (dx, dz, dy)
        } else {
            (dx, dy, dz)
        }
    } else if ay > az {
        (dy, dz, dx)
    } else if ax > ay {
        (dz, dx, dy)
    } else {
        (dz, dy, dx)
    };
    [a, b, c, -c, -b, -a]
}

/// Java `BlockPlaceContext.getNearestLookingDirections`: [`nearest_looking`],
/// with the direction into the clicked block moved to the front unless the
/// clicked block itself is being replaced.
pub fn context_looking(ctx: &PlaceContext) -> [IVec3; 6] {
    let mut dirs = nearest_looking(ctx.look);
    if ctx.at != ctx.clicked
        && let Some(i) = dirs.iter().position(|&d| d == -ctx.face)
    {
        dirs[..=i].rotate_right(1);
    }
    dirs
}

/// A lever, button, grindstone or redstone torch survives where it is put.
fn attach_survives(get: &impl Fn(IVec3) -> Option<Block>, at: IVec3, face: AttachFace, facing: Facing) -> bool {
    let to = attach_direction(face, facing);
    get(at + to).is_some_and(|b| face_sturdy(b, -to))
}

/// The state the item `raw` places at `ctx.at` for the mounted families of
/// this module and redstone torches: `None` if `raw` is not one of them,
/// `Some(None)` if no direction can hold it.
pub fn placed(raw: Block, ctx: &PlaceContext, get: impl Fn(IVec3) -> Option<Block>) -> Option<Option<Block>> {
    let at = ctx.at;
    let nearest = context_looking(ctx);
    let face_attached = |survives: &dyn Fn(AttachFace, Facing) -> bool| {
        nearest.iter().find_map(|&d| {
            let (face, facing) = match d.y {
                1 => (AttachFace::Ceiling, ctx.facing),
                -1 => (AttachFace::Floor, ctx.facing),
                _ => (AttachFace::Wall, Facing::from_offset(d)?.opposite()),
            };
            survives(face, facing).then_some((face, facing))
        })
    };
    if let Some(lantern) = lantern_of(raw) {
        // `LanternBlock.getStateForPlacement`.
        let wet = get(at) == Some(Block::WATER);
        let state = nearest.iter().filter(|d| d.y != 0).find_map(|&d| {
            let hanging = d.y > 0;
            get(at + d).is_some_and(|b| supports_center(b, -d)).then(|| lantern_block(lantern.soul, hanging, wet))
        });
        return Some(state);
    }
    if raw.base() == Block::GRINDSTONE {
        // `GrindstoneBlock.canSurvive` is always true: the first direction wins.
        let state = face_attached(&|_, _| true).map(|(face, facing)| grindstone_block(face, facing));
        return Some(state);
    }
    match r::component(raw)? {
        r::Component::Lever { .. } => {
            let state = face_attached(&|f, d| attach_survives(&get, at, f, d));
            Some(state.map(|(f, d)| nb::keep_wood(raw, r::lever(attach_mount(f, d), false))))
        }
        r::Component::Button { wood, .. } => {
            let state = face_attached(&|f, d| attach_survives(&get, at, f, d));
            Some(state.map(|(f, d)| nb::keep_wood(raw, r::button(attach_mount(f, d), false, wood))))
        }
        r::Component::Torch { .. } => {
            // `StandingAndWallBlockItem.getPlacementState`: the wall state is
            // the first horizontal direction that holds; it is taken at the
            // first horizontal direction in look order.
            let wall = nearest.iter().filter(|d| d.y == 0).find_map(|&d| {
                let facing = Facing::from_offset(d)?.opposite();
                attach_survives(&get, at, AttachFace::Wall, facing).then_some(facing)
            });
            let standing = get(at - IVec3::Y).is_some_and(|b| supports_center(b, IVec3::Y));
            let mount = nearest.iter().find_map(|&d| match d.y {
                1 => None,
                -1 => standing.then_some(0),
                _ => wall.map(|f| attach_mount(AttachFace::Wall, f)),
            });
            Some(mount.map(|m| r::torch(m, true)))
        }
        _ => None,
    }
}

// ---- Grindstones -------------------------------------------------------

/// Face and Java `facing` of a grindstone. The four original ids are floor
/// states named by the legacy front (toward the placer).
pub fn grindstone_state(id: u16) -> Option<(AttachFace, Facing)> {
    match id {
        933..=936 => Some((AttachFace::Floor, Facing::ALL[(id - 933) as usize].opposite())),
        7904..=7907 => Some((AttachFace::Wall, Facing::ALL[(id - GRINDSTONE_FIRST) as usize])),
        7908..=7911 => Some((AttachFace::Ceiling, Facing::ALL[(id - GRINDSTONE_FIRST - 4) as usize])),
        _ => None,
    }
}

pub fn grindstone_block(face: AttachFace, facing: Facing) -> Block {
    match face {
        AttachFace::Floor => Block(933 + facing.opposite() as u16),
        AttachFace::Wall => Block(GRINDSTONE_FIRST + facing as u16),
        AttachFace::Ceiling => Block(GRINDSTONE_FIRST + 4 + facing as u16),
    }
}

// ---- Lanterns ----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanternState {
    pub soul: bool,
    pub hanging: bool,
    pub wet: bool,
}

pub const fn lantern_block(soul: bool, hanging: bool, wet: bool) -> Block {
    Block(LANTERN.0 + soul as u16 * 4 + hanging as u16 * 2 + wet as u16)
}

pub const fn lantern_of(b: Block) -> Option<LanternState> {
    if b.0 < LANTERN.0 || b.0 > LANTERN_LAST {
        return None;
    }
    let i = b.0 - LANTERN.0;
    Some(LanternState { soul: i >= 4, hanging: i % 4 >= 2, wet: i % 2 == 1 })
}

/// Hardness and tool of lanterns (3.5, pickaxe; no tool needed to collect).
pub const fn is_lantern(id: u16) -> bool {
    id >= LANTERN.0 && id <= LANTERN_LAST
}

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet
/// one, for this module's families. See [`super::waterlog::pair`].
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if is_lantern(id) { Some((id ^ 1, id & 1 == 1)) } else { None }
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [LANTERN.0, SOUL_LANTERN.0].into_iter()
}

// ---- Registry ----------------------------------------------------------

/// The pre-existing state with this module's name, textures and properties.
pub const fn twin(id: u16) -> Option<Block> {
    match id {
        7680..=7691 => Some(r::LEVER),
        7696..=7767 => {
            let kind = (id - BUTTON_FIRST) / BUTTON_STRIDE;
            let old = r::button(0, false, kind >= 1);
            Some(if kind >= 2 { nb::switch_reskin(old, kind - 2) } else { old })
        }
        DOOR_FIRST..=7903 => door_twin(id),
        7904..=7911 => Some(Block::GRINDSTONE),
        _ => None,
    }
}

/// Name, render kind and textures of the lanterns (everything else copies
/// its [`twin`]).
pub const fn lantern_registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    match lantern_of(Block(id)) {
        Some(l) => Some((
            if l.soul { "soul lantern" } else { "lantern" },
            RenderKind::Shaped,
            [if l.soul { tex::SOUL_LANTERN } else { tex::LANTERN }; 6],
        )),
        None => None,
    }
}

/// Light emitted by the lanterns.
pub const fn emission(id: u16) -> u8 {
    match lantern_of(Block(id)) {
        Some(l) => {
            if l.soul {
                10
            } else {
                15
            }
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests;
