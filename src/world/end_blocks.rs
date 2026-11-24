//! v0.16 Track A: the outer End's blocks (BLD-01b, GEN-04b). Java 26.3
//! references: `Blocks` (`PURPUR_BLOCK`, `PURPUR_PILLAR`, `PURPUR_STAIRS`,
//! `PURPUR_SLAB`, `END_ROD`, `CHORUS_PLANT`, `CHORUS_FLOWER`),
//! `RotatedPillarBlock`, `RodBlock`, `EndRodBlock`, `PipeBlock`,
//! `ChorusPlantBlock` and `ChorusFlowerBlock`.
//!
//! State ids, inside Track A's reservation 12288–12415 (the rest stay
//! unregistered):
//!
//! - 12288 purpur block;
//! - 12289–12291 purpur pillar, axis y, x, z;
//! - 12292–12371 purpur stairs, the 80 states of an appended stairs family
//!   (`slab_stair_states::PURPUR_FAMILY`, local layout as every v0.12 family);
//! - 12372–12376 purpur slab: bottom dry, bottom wet, top dry, top wet, double;
//! - 12377–12382 End rod, `facing` in Java's `Direction` order (down, up,
//!   north, south, west, east);
//! - 12383 chorus plant (one state: its six connections are read from the
//!   neighbours whenever they are needed, which is what `ChorusPlantBlock`'s
//!   `getStateWithConnections` and `updateShape` keep its 64 states equal to);
//! - 12384–12389 chorus flower, `age` 0–5.
//!
//! Texture layers 529–537 (blocks) and 538–540 (the chorus fruit, popped
//! chorus fruit and shulker shell icons, see [`super::super::content::chorus`]),
//! all original art.
use glam::IVec3;

use super::{
    World,
    block::{Block, Facing, RenderKind},
    shape::{Box16, Boxes},
};

pub const FIRST: u16 = 12288;
pub const PURPUR_BLOCK: Block = Block(FIRST);
/// The pillar standing up (`axis=y`); x and z follow.
pub const PURPUR_PILLAR: Block = Block(FIRST + 1);
pub const PURPUR_STAIRS: Block = Block(FIRST + 4);
pub const PURPUR_SLAB: Block = Block(FIRST + 84);
pub const END_ROD_FIRST: u16 = FIRST + 89;
/// The End rod pointing up, Java's default state and the item's block.
pub const END_ROD: Block = Block(END_ROD_FIRST + 1);
pub const CHORUS_PLANT: Block = Block(FIRST + 95);
/// A chorus flower of age 0; ages 1–5 follow (5 is dead).
pub const CHORUS_FLOWER: Block = Block(FIRST + 96);
pub const LAST: u16 = FIRST + 101;
/// The last id of Track A's reservation.
pub const RESERVED_LAST: u16 = 12415;
pub const PURPUR_SHAPES: super::slab_stair_states::Family =
    super::slab_stair_states::Family { base: PURPUR_BLOCK, stairs: PURPUR_STAIRS, slab: PURPUR_SLAB };

pub const TEX: u16 = 529;
const T_PURPUR: u16 = TEX;
const T_PILLAR_SIDE: u16 = TEX + 1;
const T_PILLAR_TOP: u16 = TEX + 2;
const T_ROD: u16 = TEX + 3;
const T_ROD_BASE: u16 = TEX + 4;
const T_CHORUS_PLANT: u16 = TEX + 5;
const T_FLOWER_SIDE: u16 = TEX + 6;
const T_FLOWER_TOP: u16 = TEX + 7;
const T_FLOWER_DEAD: u16 = TEX + 8;
/// The chorus fruit, popped chorus fruit and shulker shell icons.
pub const ICON_TEX: u16 = TEX + 9;
pub const TEX_END: u16 = TEX + 12;
const _: () = assert!(LAST <= RESERVED_LAST && TEX >= 529 && TEX_END <= 609);

/// `Blocks`: purpur `strength(1.5, 6)` (the slab `2.0, 6`), End rod and
/// chorus `instabreak` and `strength(0.4)`.
pub const PURPUR_HARDNESS: f32 = 1.5;
pub const PURPUR_SLAB_HARDNESS: f32 = 2.0;
pub const PURPUR_RESISTANCE: f32 = 6.0;
pub const CHORUS_HARDNESS: f32 = 0.4;
/// `Blocks.END_ROD`: `lightLevel(14)`.
pub const END_ROD_LIGHT: u8 = 14;
/// `ChorusFlowerBlock.DEAD_AGE`.
pub const DEAD_AGE: u8 = 5;
/// `ChorusPlantFeature`'s `generatePlant(…, 8)`.
pub const MAX_SPREAD: i32 = 8;

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// Java's six directions, in `Direction` order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Dir {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Dir {
    pub const ALL: [Dir; 6] = [Dir::Down, Dir::Up, Dir::North, Dir::South, Dir::West, Dir::East];

    pub const fn offset(self) -> IVec3 {
        match self {
            Dir::Down => IVec3::NEG_Y,
            Dir::Up => IVec3::Y,
            Dir::North => IVec3::NEG_Z,
            Dir::South => IVec3::Z,
            Dir::West => IVec3::NEG_X,
            Dir::East => IVec3::X,
        }
    }

    pub const fn opposite(self) -> Dir {
        match self {
            Dir::Down => Dir::Up,
            Dir::Up => Dir::Down,
            Dir::North => Dir::South,
            Dir::South => Dir::North,
            Dir::West => Dir::East,
            Dir::East => Dir::West,
        }
    }

    pub fn from_offset(d: IVec3) -> Option<Dir> {
        Dir::ALL.into_iter().find(|f| f.offset() == d)
    }

    /// The horizontal direction, if this is one.
    pub const fn horizontal(self) -> Option<Facing> {
        match self {
            Dir::North => Some(Facing::North),
            Dir::South => Some(Facing::South),
            Dir::West => Some(Facing::West),
            Dir::East => Some(Facing::East),
            _ => None,
        }
    }

    pub const fn from_facing(f: Facing) -> Dir {
        match f {
            Facing::North => Dir::North,
            Facing::South => Dir::South,
            Facing::West => Dir::West,
            Facing::East => Dir::East,
        }
    }
}

/// The End rod pointing `dir`.
pub const fn end_rod(dir: Dir) -> Block {
    Block(END_ROD_FIRST + dir as u16)
}

/// An End rod's facing.
pub fn rod_facing(b: Block) -> Option<Dir> {
    (b.0 >= END_ROD_FIRST && b.0 < END_ROD_FIRST + 6).then(|| Dir::ALL[(b.0 - END_ROD_FIRST) as usize])
}

pub fn is_end_rod(b: Block) -> bool {
    rod_facing(b).is_some()
}

/// A purpur pillar's axis: 0 y, 1 x, 2 z.
pub fn pillar_axis(b: Block) -> Option<u16> {
    (b.0 >= PURPUR_PILLAR.0 && b.0 < PURPUR_PILLAR.0 + 3).then(|| b.0 - PURPUR_PILLAR.0)
}

pub const fn chorus_flower(age: u8) -> Block {
    Block(CHORUS_FLOWER.0 + age as u16)
}

/// A chorus flower's age.
pub fn flower_age(b: Block) -> Option<u8> {
    (b.0 >= CHORUS_FLOWER.0 && b.0 <= CHORUS_FLOWER.0 + DEAD_AGE as u16).then(|| (b.0 - CHORUS_FLOWER.0) as u8)
}

pub fn is_chorus(b: Block) -> bool {
    b == CHORUS_PLANT || flower_age(b).is_some()
}

/// `#supports_chorus_plant` and `#supports_chorus_flower`: End stone.
pub fn supports_chorus(b: Block) -> bool {
    b == Block::END_STONE
}

const fn stairs_local(id: u16) -> Option<u16> {
    if id >= PURPUR_STAIRS.0 && id < PURPUR_STAIRS.0 + 80 { Some(id - PURPUR_STAIRS.0) } else { None }
}

const fn slab_local(id: u16) -> Option<u16> {
    if id >= PURPUR_SLAB.0 && id < PURPUR_SLAB.0 + 5 { Some(id - PURPUR_SLAB.0) } else { None }
}

/// Whether `id` is one of the purpur stairs or slab states (laid out by
/// `slab_stair_states`).
pub const fn owns_shape(id: u16) -> bool {
    stairs_local(id).is_some() || slab_local(id).is_some()
}

pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    const P: [u16; 6] = [T_PURPUR; 6];
    if stairs_local(id).is_some() {
        return ("purpur stairs", RenderKind::Shaped, P);
    }
    if let Some(local) = slab_local(id) {
        let kind = match local {
            4 => RenderKind::Opaque,
            0 | 1 => RenderKind::Cutout,
            _ => RenderKind::Shaped,
        };
        return ("purpur slab", kind, P);
    }
    if id >= END_ROD_FIRST && id < END_ROD_FIRST + 6 {
        return ("end rod", RenderKind::Shaped, [T_ROD, T_ROD, T_ROD, T_ROD_BASE, T_ROD, T_ROD]);
    }
    if id >= CHORUS_FLOWER.0 && id <= CHORUS_FLOWER.0 + DEAD_AGE as u16 {
        let (side, top) = if id == CHORUS_FLOWER.0 + DEAD_AGE as u16 {
            (T_FLOWER_DEAD, T_FLOWER_DEAD)
        } else {
            (T_FLOWER_SIDE, T_FLOWER_TOP)
        };
        return ("chorus flower", RenderKind::Shaped, [side, side, top, side, side, side]);
    }
    let (s, t) = (T_PILLAR_SIDE, T_PILLAR_TOP);
    match id {
        12288 => ("purpur block", RenderKind::Opaque, P),
        12289 => ("purpur pillar", RenderKind::Opaque, [s, s, t, t, s, s]),
        12290 => ("purpur pillar", RenderKind::Opaque, [t, t, s, s, s, s]),
        12291 => ("purpur pillar", RenderKind::Opaque, [s, s, s, s, t, t]),
        12383 => ("chorus plant", RenderKind::Shaped, [T_CHORUS_PLANT; 6]),
        _ => ("unknown", RenderKind::Invisible, [0; 6]),
    }
}

/// Creative palette entries: the item blocks.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [PURPUR_BLOCK.0, PURPUR_PILLAR.0, PURPUR_STAIRS.0, PURPUR_SLAB.0, END_ROD.0, CHORUS_PLANT.0, CHORUS_FLOWER.0]
        .into_iter()
}

/// The state's item block (`Block::base`): the upright pillar, the upward
/// End rod, an age 0 chorus flower.
pub fn base(b: Block) -> Option<Block> {
    if pillar_axis(b).is_some() {
        Some(PURPUR_PILLAR)
    } else if is_end_rod(b) {
        Some(END_ROD)
    } else if flower_age(b).is_some() {
        Some(CHORUS_FLOWER)
    } else {
        None
    }
}

/// What the block drops when mined (the shared drop path). Chorus plants
/// roll their fruit in [`World::chorus_plant_drops`].
pub fn drop(b: Block) -> Option<Option<crate::item::Item>> {
    if !owns(b.0) || owns_shape(b.0) {
        return None;
    }
    Some(match b {
        CHORUS_PLANT => None,
        b => Some(base(b).unwrap_or(b).into()),
    })
}

/// `Block::hardness` for the non-shape states (stairs and slabs too).
pub fn hardness(b: Block) -> Option<f32> {
    if !owns(b.0) {
        return None;
    }
    Some(if slab_local(b.0).is_some() {
        PURPUR_SLAB_HARDNESS
    } else if is_end_rod(b) {
        0.0
    } else if is_chorus(b) {
        CHORUS_HARDNESS
    } else {
        PURPUR_HARDNESS
    })
}

/// `Block::blast_resistance`.
pub fn resistance(b: Block) -> Option<f32> {
    hardness(b).map(|h| if is_end_rod(b) || is_chorus(b) { h } else { PURPUR_RESISTANCE })
}

/// The tool that mines it (`#mineable/pickaxe` purpur, `#mineable/axe`
/// chorus) and whether that tool is needed (purpur only).
pub fn best_tool(b: Block) -> Option<Option<crate::item::ToolKind>> {
    if !owns(b.0) {
        return None;
    }
    Some(if is_chorus(b) {
        Some(crate::item::ToolKind::Axe)
    } else if is_end_rod(b) {
        None
    } else {
        Some(crate::item::ToolKind::Pickaxe)
    })
}

/// `Block::harvest_level`: purpur needs any pickaxe.
pub fn harvest_level(b: Block) -> Option<Option<u8>> {
    if !owns(b.0) {
        return None;
    }
    Some((!is_end_rod(b) && !is_chorus(b)).then_some(0))
}

/// Light emission.
pub const fn emission(id: u16) -> u8 {
    if id >= END_ROD_FIRST && id < END_ROD_FIRST + 6 { END_ROD_LIGHT } else { 0 }
}

/// The placed state for a click on face `normal` (Java's
/// `getStateForPlacement`): a pillar along the clicked axis, an End rod
/// pointing out of the clicked face unless that face is an End rod
/// pointing the same way, which the new one meets head on.
pub fn placed(raw: Block, normal: IVec3, behind: Option<Block>) -> Block {
    if pillar_axis(raw).is_some() {
        let axis = if normal.x != 0 {
            1
        } else if normal.z != 0 {
            2
        } else {
            0
        };
        return Block(PURPUR_PILLAR.0 + axis);
    }
    if is_end_rod(raw) {
        let clicked = Dir::from_offset(normal).unwrap_or(Dir::Up);
        if behind.and_then(rod_facing) == Some(clicked) {
            return end_rod(clicked.opposite());
        }
        return end_rod(clicked);
    }
    raw
}

/// `Rotation.rotate` for a horizontal facing state (End city templates).
pub fn rotated(b: Block, turns: u8) -> Block {
    if let Some(d) = rod_facing(b) {
        return match d.horizontal() {
            Some(mut f) => {
                for _ in 0..turns {
                    f = f.clockwise();
                }
                end_rod(Dir::from_facing(f))
            }
            None => b,
        };
    }
    if let Some(axis) = pillar_axis(b) {
        return if turns % 2 == 1 && axis != 0 { Block(PURPUR_PILLAR.0 + 3 - axis) } else { b };
    }
    b
}

const fn bx(min: [u8; 3], max: [u8; 3]) -> Box16 {
    Box16 { min, max }
}

/// A box of `size` across centred on the cell, stretched from the centre
/// to the face `dir` (`Block.boxZ(size, 0, 8)` turned to face `dir`).
fn arm(size: u8, dir: Dir) -> Box16 {
    let (lo, hi) = (8 - size / 2, 8 + size / 2);
    match dir {
        Dir::Down => bx([lo, 0, lo], [hi, 8, hi]),
        Dir::Up => bx([lo, 8, lo], [hi, 16, hi]),
        Dir::North => bx([lo, lo, 0], [hi, hi, 8]),
        Dir::South => bx([lo, lo, 8], [hi, hi, 16]),
        Dir::West => bx([0, lo, lo], [8, hi, hi]),
        Dir::East => bx([8, lo, lo], [16, hi, hi]),
    }
}

/// Whether a chorus plant joins the block `n` on its `dir` side
/// (`getStateWithConnections`).
pub fn chorus_connects(dir: Dir, n: Block) -> bool {
    is_chorus(n) || (dir == Dir::Down && supports_chorus(n))
}

/// A shaped block's boxes: the End rod's rod and base, the chorus plant's
/// core and arms (`PipeBlock(10)`), a chorus flower's cube. `side(f)`
/// gives the horizontal neighbours.
pub fn boxes(b: Block, side: impl Fn(Facing) -> Block, below: Block, above: Block) -> Option<Boxes> {
    if let Some(d) = rod_facing(b) {
        // The model: a 2×15×2 rod on a 4×1×4 base at the back.
        let mut out = Boxes::new();
        let (base, rod) = match d {
            Dir::Up => (bx([6, 0, 6], [10, 1, 10]), bx([7, 1, 7], [9, 16, 9])),
            Dir::Down => (bx([6, 15, 6], [10, 16, 10]), bx([7, 0, 7], [9, 15, 9])),
            Dir::South => (bx([6, 6, 0], [10, 10, 1]), bx([7, 7, 1], [9, 9, 16])),
            Dir::North => (bx([6, 6, 15], [10, 10, 16]), bx([7, 7, 0], [9, 9, 15])),
            Dir::East => (bx([0, 6, 6], [1, 10, 10]), bx([1, 7, 7], [16, 9, 9])),
            Dir::West => (bx([15, 6, 6], [16, 10, 10]), bx([0, 7, 7], [15, 9, 9])),
        };
        out.push(base);
        out.push(rod);
        return Some(out);
    }
    if b == CHORUS_PLANT {
        let mut out = Boxes::from_box(bx([3, 3, 3], [13, 13, 13]));
        for d in Dir::ALL {
            let n = match d {
                Dir::Down => below,
                Dir::Up => above,
                d => side(d.horizontal().unwrap()),
            };
            if chorus_connects(d, n) {
                out.push(arm(10, d));
            }
        }
        return Some(out);
    }
    if flower_age(b).is_some() {
        return Some(Boxes::from_box(bx([0, 0, 0], [16, 16, 16])));
    }
    None
}

/// Outline and collision: `RodBlock`'s 4×4 column along the rod's axis;
/// the others as their [`boxes`].
pub fn outline(b: Block, side: impl Fn(Facing) -> Block, below: Block, above: Block) -> Option<Boxes> {
    if let Some(d) = rod_facing(b) {
        return Some(Boxes::from_box(match d {
            Dir::Up | Dir::Down => bx([6, 0, 6], [10, 16, 10]),
            Dir::North | Dir::South => bx([6, 6, 0], [10, 10, 16]),
            Dir::West | Dir::East => bx([0, 6, 6], [16, 10, 10]),
        }));
    }
    boxes(b, side, below, above)
}

/// The boxes an item icon shows.
pub fn item_boxes(b: Block) -> Option<Boxes> {
    boxes(b, |_| Block::AIR, if b == CHORUS_PLANT { Block::END_STONE } else { Block::AIR }, Block::AIR)
}

fn hash(layer: u16, x: usize, y: usize) -> u32 {
    let mut h = (layer as u32).wrapping_mul(0x9E37_79B9) ^ (x as u32).wrapping_mul(0x85EB_CA6B) ^ (y as u32) << 16;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ h >> 12
}

fn shade(c: [i32; 3], d: i32) -> [u8; 4] {
    [(c[0] + d).clamp(0, 255) as u8, (c[1] + d).clamp(0, 255) as u8, (c[2] + d).clamp(0, 255) as u8, 255]
}

/// Original textures: the End rod pale and glowing; chorus as mottled
/// violet. Purpur and its pillar are drawn by the client's
/// `render::material_textures` (v0.18 Track 3).
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let n = (hash(layer, x, y) % 13) as i32 - 6;
    let (xi, yi) = (x as i32, y as i32);
    match layer {
        T_ROD => {
            let glow = 236 + n.abs();
            shade([glow, glow - 6, glow + 2], 0)
        }
        T_ROD_BASE => shade([196, 172, 204], n),
        T_CHORUS_PLANT => {
            let vein = (xi * 3 + yi * 5) % 11 == 0;
            shade([104, 70, 106], if vein { 30 } else { n * 2 })
        }
        T_FLOWER_SIDE | T_FLOWER_TOP => {
            // Petal lobes with clear corners; the top is a pale bud.
            let corner = !(2..=13).contains(&xi) && !(2..=13).contains(&yi);
            if corner {
                return [0, 0, 0, 0];
            }
            let centre = (6..=9).contains(&xi) && (6..=9).contains(&yi);
            if layer == T_FLOWER_TOP && centre {
                shade([226, 206, 230], n)
            } else {
                let lobe = (xi / 4 + yi / 4) % 2 == 0;
                shade([150, 104, 152], if lobe { 18 + n } else { n })
            }
        }
        T_FLOWER_DEAD => {
            let corner = !(2..=13).contains(&xi) && !(2..=13).contains(&yi);
            if corner {
                return [0, 0, 0, 0];
            }
            shade([96, 74, 96], n * 2)
        }
        _ => super::super::content::chorus::icon_pixel(layer - ICON_TEX, x, y),
    }
}

/// A block lookup for the chorus rules.
pub trait Blocks {
    fn at(&self, p: IVec3) -> Block;
}

impl<F: Fn(IVec3) -> Block> Blocks for F {
    fn at(&self, p: IVec3) -> Block {
        self(p)
    }
}

/// `ChorusPlantBlock.canSurvive`: a plant stands on another plant or End
/// stone, or hangs from a horizontal plant that does, but no plant with a
/// block both above and below may branch sideways.
pub fn plant_survives(w: &impl Blocks, p: IVec3) -> bool {
    let below = w.at(p - IVec3::Y);
    let above_and_below = w.at(p + IVec3::Y) != Block::AIR && below != Block::AIR;
    for f in Facing::ALL {
        let n = p + f.offset();
        if w.at(n) == CHORUS_PLANT {
            if above_and_below {
                return false;
            }
            let under = w.at(n - IVec3::Y);
            if under == CHORUS_PLANT || supports_chorus(under) {
                return true;
            }
        }
    }
    below == CHORUS_PLANT || supports_chorus(below)
}

/// `ChorusFlowerBlock.canSurvive`: on a plant or End stone, or over air
/// with exactly one plant beside it and nothing else around.
pub fn flower_survives(w: &impl Blocks, p: IVec3) -> bool {
    let below = w.at(p - IVec3::Y);
    if below == CHORUS_PLANT || supports_chorus(below) {
        return true;
    }
    if below != Block::AIR {
        return false;
    }
    let mut one = false;
    for f in Facing::ALL {
        let n = w.at(p + f.offset());
        if n == CHORUS_PLANT {
            if one {
                return false;
            }
            one = true;
        } else if n != Block::AIR {
            return false;
        }
    }
    one
}

/// `ChorusFlowerBlock.allNeighborsEmpty`.
fn neighbours_empty(w: &impl Blocks, p: IVec3, ignore: Option<Facing>) -> bool {
    Facing::ALL.into_iter().filter(|&f| Some(f) != ignore).all(|f| w.at(p + f.offset()) == Block::AIR)
}

/// What a chorus flower's random tick does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Growth {
    /// Nothing changes.
    None,
    /// The flower becomes a plant and a flower of the same age grows above
    /// (level event 1033).
    Up,
    /// The flower becomes a plant and flowers one older grow on these sides.
    Branch(Vec<Facing>),
    /// The flower dies (level event 1034).
    Dies,
}

/// `ChorusFlowerBlock.randomTick` for the flower at `p`. `int(n)` is Java's
/// `random.nextInt(n)`; `top` is the world's highest block row.
pub fn flower_growth(w: &impl Blocks, p: IVec3, top: i32, int: &mut impl FnMut(u32) -> u32) -> Growth {
    let Some(age) = flower_age(w.at(p)) else { return Growth::None };
    let above = p + IVec3::Y;
    if w.at(above) != Block::AIR || above.y > top || age >= DEAD_AGE {
        return Growth::None;
    }
    let mut grow_up = false;
    let mut on_support = false;
    let below = w.at(p - IVec3::Y);
    if supports_chorus(below) {
        grow_up = true;
    } else if below == CHORUS_PLANT {
        let mut height = 1;
        for _ in 0..4 {
            let test = w.at(p - IVec3::Y * (height + 1));
            if test != CHORUS_PLANT {
                if supports_chorus(test) {
                    on_support = true;
                }
                break;
            }
            height += 1;
        }
        if height < 2 || height as u32 <= int(if on_support { 5 } else { 4 }) {
            grow_up = true;
        }
    } else if below == Block::AIR {
        grow_up = true;
    }
    if grow_up && neighbours_empty(w, above, None) && w.at(p + IVec3::Y * 2) == Block::AIR {
        return Growth::Up;
    }
    if age >= 4 {
        return Growth::Dies;
    }
    let attempts = int(4) + u32::from(on_support);
    let mut sides = Vec::new();
    for _ in 0..attempts {
        // `Plane.HORIZONTAL.getRandomDirection`: north, east, south, west.
        let f = [Facing::North, Facing::East, Facing::South, Facing::West][int(4) as usize];
        let t = p + f.offset();
        if w.at(t) == Block::AIR
            && w.at(t - IVec3::Y) == Block::AIR
            && neighbours_empty(w, t, Some(f.opposite()))
            && !sides.contains(&f)
        {
            sides.push(f);
        }
    }
    if sides.is_empty() { Growth::Dies } else { Growth::Branch(sides) }
}

/// A chorus plant grown at once (`ChorusFlowerBlock.generatePlant`): the
/// cells it fills, in placement order, with the plant's blocks. `empty`
/// tells which cells hold nothing before the plant.
pub fn generate_plant(
    origin: IVec3,
    max_spread: i32,
    empty: &impl Fn(IVec3) -> bool,
    int: &mut impl FnMut(u32) -> u32,
) -> Vec<(IVec3, Block)> {
    let mut placed: Vec<(IVec3, Block)> = vec![(origin, CHORUS_PLANT)];
    grow_tree(origin, origin, max_spread, 0, empty, int, &mut placed);
    placed
}

fn grow_tree(
    current: IVec3,
    start: IVec3,
    max_spread: i32,
    depth: i32,
    empty: &impl Fn(IVec3) -> bool,
    int: &mut impl FnMut(u32) -> u32,
    placed: &mut Vec<(IVec3, Block)>,
) {
    let is_empty = |placed: &Vec<(IVec3, Block)>, p: IVec3| empty(p) && !placed.iter().any(|(q, _)| *q == p);
    let neighbours_empty = |placed: &Vec<(IVec3, Block)>, p: IVec3, ignore: Option<Facing>| {
        Facing::ALL.into_iter().filter(|&f| Some(f) != ignore).all(|f| is_empty(placed, p + f.offset()))
    };
    let set = |placed: &mut Vec<(IVec3, Block)>, p: IVec3, b: Block| match placed.iter_mut().find(|(q, _)| *q == p) {
        Some(c) => c.1 = b,
        None => placed.push((p, b)),
    };
    let mut height = int(4) as i32 + 1;
    if depth == 0 {
        height += 1;
    }
    for i in 0..height {
        let target = current + IVec3::Y * (i + 1);
        if !neighbours_empty(placed, target, None) {
            return;
        }
        set(placed, target, CHORUS_PLANT);
        set(placed, target - IVec3::Y, CHORUS_PLANT);
    }
    let mut stem = false;
    if depth < 4 {
        let mut stems = int(4);
        if depth == 0 {
            stems += 1;
        }
        for _ in 0..stems {
            let f = [Facing::North, Facing::East, Facing::South, Facing::West][int(4) as usize];
            let target = current + IVec3::Y * height + f.offset();
            if (target.x - start.x).abs() < max_spread
                && (target.z - start.z).abs() < max_spread
                && is_empty(placed, target)
                && is_empty(placed, target - IVec3::Y)
                && neighbours_empty(placed, target, Some(f.opposite()))
            {
                stem = true;
                set(placed, target, CHORUS_PLANT);
                set(placed, target - f.offset(), CHORUS_PLANT);
                grow_tree(target, start, max_spread, depth + 1, empty, int, placed);
            }
        }
    }
    if !stem {
        set(placed, current + IVec3::Y * height, chorus_flower(DEAD_AGE));
    }
}

impl World {
    /// After a block change at `p`: every chorus block beside it that can
    /// no longer survive breaks on a tick one game tick away (`updateShape`
    /// scheduling a tick; a flower ignores changes above it).
    pub(super) fn update_chorus(&mut self, p: IVec3) {
        for d in Dir::ALL {
            let q = p - d.offset();
            let Some(b) = self.get_block(q) else { continue };
            if !is_chorus(b) || (d == Dir::Up && flower_age(b).is_some()) {
                continue;
            }
            if !self.chorus_survives(q, b) {
                self.schedule_redstone(q, 1, 0);
            }
        }
    }

    /// Whether a chorus block may be placed at `p` (`canSurvive`); `None`
    /// for other blocks.
    pub fn chorus_placement_ok(&self, p: IVec3, b: Block) -> Option<bool> {
        is_chorus(b).then(|| self.chorus_survives(p, b))
    }

    fn chorus_survives(&self, p: IVec3, b: Block) -> bool {
        let w = |q: IVec3| self.get_block(q).unwrap_or(Block::AIR);
        if b == CHORUS_PLANT { plant_survives(&w, p) } else { flower_survives(&w, p) }
    }

    /// The scheduled tick (`tick`): a chorus block that still can't survive
    /// breaks. A plant drops its fruit (`destroyBlock(pos, true)`); a
    /// flower's loot table needs a breaking entity, so it drops nothing.
    pub(super) fn chorus_tick(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p).filter(|&b| is_chorus(b)) else { return };
        if self.chorus_survives(p, b) {
            return;
        }
        if b == CHORUS_PLANT {
            self.spill_block(p, b);
        }
        self.particles.push(crate::particles::Request::Break { cell: p, block: b });
        self.edit(p, Block::AIR, true);
        self.update_chorus(p);
    }

    /// `ChorusFlowerBlock.randomTick`.
    pub(super) fn chorus_flower_random_tick(&mut self, p: IVec3) {
        let top = self.max_y() - 1;
        let rolls: Vec<u64> = (0..16).map(|_| self.roll()).collect();
        let growth = {
            let w = |q: IVec3| self.get_block(q).unwrap_or(Block::AIR);
            let mut i = 0;
            let mut int = |n: u32| {
                let r = rolls[i % rolls.len()];
                i += 1;
                (r % u64::from(n)) as u32
            };
            flower_growth(&w, p, top, &mut int)
        };
        let Some(age) = self.get_block(p).and_then(flower_age) else { return };
        match growth {
            Growth::None => {}
            Growth::Up => {
                self.edit(p, CHORUS_PLANT, false);
                self.edit(p + IVec3::Y, chorus_flower(age), false);
                self.chorus_events.push(ChorusEvent::Grew(p + IVec3::Y));
            }
            Growth::Branch(sides) => {
                for f in sides {
                    self.edit(p + f.offset(), chorus_flower(age + 1), false);
                    self.chorus_events.push(ChorusEvent::Grew(p + f.offset()));
                }
                self.edit(p, CHORUS_PLANT, false);
            }
            Growth::Dies => {
                self.edit(p, chorus_flower(DEAD_AGE), false);
                self.chorus_events.push(ChorusEvent::Died(p));
            }
        }
    }

    /// A chorus plant's loot (`blocks/chorus_plant`): one chorus fruit half
    /// the time.
    pub(super) fn chorus_plant_drops(&mut self, b: Block, out: &mut Vec<crate::inventory::Stack>) {
        if b == CHORUS_PLANT && self.roll().is_multiple_of(2) {
            out.push(crate::inventory::Stack::new(super::super::content::chorus::CHORUS_FRUIT, 1));
        }
    }

    /// `ChorusFlowerBlock.onProjectileHit`: a projectile that may break
    /// blocks breaks the flower, which drops itself (the projectile is the
    /// breaking entity). Returns whether it broke.
    pub fn chorus_flower_shot(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p).filter(|&b| flower_age(b).is_some()) else { return false };
        self.spill_block(p, b);
        self.particles.push(crate::particles::Request::Break { cell: p, block: b });
        self.set_block(p, Block::AIR);
        true
    }
}

/// What a chorus flower did, for the client's sounds (level events 1033
/// `CHORUS_FLOWER_GROW` and 1034 `CHORUS_FLOWER_DEATH`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChorusEvent {
    Grew(IVec3),
    Died(IVec3),
}

#[cfg(test)]
mod tests;
