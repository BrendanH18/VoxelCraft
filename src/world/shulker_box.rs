//! Java 26.3 shulker boxes (v0.16 Track B): `ShulkerBoxBlock` and
//! `ShulkerBoxBlockEntity` for the plain box and its sixteen dyed forms,
//! each with six facings.
//!
//! The 27 slots are an ordinary chest-table entry (`world::chest`), so
//! screens, hoppers, comparators and saves treat a box like any container,
//! but removing a box never spills them (`preRemoveSideEffects` does
//! nothing): the drop carries them (`copy_components` of `container` in
//! `blocks/*shulker_box`), whether the box was mined, blown up or popped by
//! a piston (`PushReaction.POPPED`). A Creative player breaking a box that
//! holds something gets the box too (`playerWillDestroy`). Nothing that is a
//! shulker box goes in (`ShulkerBoxSlot`, `canPlaceItemThroughFace`).
//!
//! The lid opens over ten ticks while anyone has the box open
//! (`updateAnimation`: 0.1 a tick, a half-block lift and a 270° turn), and
//! a box only opens when the half block in front of its lid is free of
//! collision (`canOpen`). Boxes are drawn as free-standing models
//! (`ShulkerBoxRenderer`, within its 64-block view distance), never in the
//! chunk meshes.
//!
//! States 12416–12517 (Track B's reservation 12416–12543):
//! `FIRST + colour * 6 + facing`, colour 0 the plain box then `DyeColor`
//! order, facing in [`Dir`] order (up first, the item's state). Texture
//! layers 609–659, three per colour (side, lid top, bottom), original art.

use glam::{DVec3, IVec3, Vec3};
use rustc_hash::FxHashMap;

use super::World;
use super::block::{Block, RenderKind};
use super::chest::{Chest, SLOTS};
use super::colour_blocks::banner::Quad;
use crate::color::DyeColor;
use crate::inventory::Stack;

pub const FIRST: u16 = 12416;
/// The plain box and the sixteen dyed ones.
pub const COLOURS: u16 = 17;
pub const LAST: u16 = FIRST + COLOURS * 6 - 1;
pub const TEX: u16 = 609;
pub const TEX_END: u16 = TEX + COLOURS * 3;
const _: () = assert!(FIRST >= 12416 && LAST <= 12543 && TEX >= 609 && TEX_END <= 669);

/// `Blocks.SHULKER_BOX`'s `strength(2.0F)` (hardness and resistance).
pub const HARDNESS: f32 = 2.0;
/// `OPENING_TICK_LENGTH`: the lid moves a tenth a tick.
pub const LID_STEP: f32 = 0.1;
/// `MAX_LID_HEIGHT` (blocks) and `MAX_LID_ROTATION` (degrees).
pub const MAX_LID_HEIGHT: f32 = 0.5;
pub const MAX_LID_ROTATION: f32 = 270.0;
/// `BlockEntityRenderer.getViewDistance`'s default.
pub const VIEW_DISTANCE: f64 = 64.0;

/// `ShulkerBoxBlock.FACING`: the way the lid opens. Up comes first so the
/// item's state is each colour's first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
    North,
    South,
    West,
    East,
}

impl Dir {
    pub const ALL: [Dir; 6] = [Dir::Up, Dir::Down, Dir::North, Dir::South, Dir::West, Dir::East];

    pub const fn offset(self) -> IVec3 {
        match self {
            Dir::Up => IVec3::Y,
            Dir::Down => IVec3::NEG_Y,
            Dir::North => IVec3::NEG_Z,
            Dir::South => IVec3::Z,
            Dir::West => IVec3::NEG_X,
            Dir::East => IVec3::X,
        }
    }

    /// The facing for a unit face normal.
    pub fn from_normal(n: IVec3) -> Option<Dir> {
        Dir::ALL.into_iter().find(|d| d.offset() == n)
    }

    /// Java's serialized name.
    pub const fn name(self) -> &'static str {
        ["up", "down", "north", "south", "west", "east"][self as usize]
    }

    /// A point of the box in its upright frame (centred on the cell,
    /// -0.5..0.5), turned so that up points this way.
    fn orient(self, v: Vec3) -> Vec3 {
        match self {
            Dir::Up => v,
            Dir::Down => Vec3::new(v.x, -v.y, -v.z),
            Dir::North => Vec3::new(v.x, v.z, -v.y),
            Dir::South => Vec3::new(v.x, -v.z, v.y),
            Dir::West => Vec3::new(-v.y, v.x, v.z),
            Dir::East => Vec3::new(v.y, -v.x, v.z),
        }
    }
}

/// A shulker box state: its colour (`None` plain) and facing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct State {
    pub colour: Option<DyeColor>,
    pub facing: Dir,
}

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

pub const fn block(s: State) -> Block {
    let c = match s.colour {
        None => 0,
        Some(c) => c as u16 + 1,
    };
    Block(FIRST + c * 6 + s.facing as u16)
}

pub const fn decode(b: Block) -> Option<State> {
    if !owns(b.0) {
        return None;
    }
    let n = b.0 - FIRST;
    let c = n / 6;
    Some(State {
        colour: if c == 0 { None } else { Some(DyeColor::ALL[(c - 1) as usize]) },
        facing: Dir::ALL[(n % 6) as usize],
    })
}

/// The state a colour's item stands for (facing up, Java's default).
pub const fn item_state(colour: Option<DyeColor>) -> Block {
    block(State { colour, facing: Dir::Up })
}

/// `ShulkerBoxBlock.getStateForPlacement`: the lid faces away from the
/// clicked face. `None` when `raw` isn't a shulker box.
pub fn placed(raw: Block, normal: IVec3) -> Option<Block> {
    let s = decode(raw)?;
    Some(block(State { facing: Dir::from_normal(normal).unwrap_or(Dir::Up), ..s }))
}

const NAMES: [&str; 17] = [
    "shulker box",
    "white shulker box",
    "orange shulker box",
    "magenta shulker box",
    "light blue shulker box",
    "yellow shulker box",
    "lime shulker box",
    "pink shulker box",
    "gray shulker box",
    "light gray shulker box",
    "cyan shulker box",
    "purple shulker box",
    "blue shulker box",
    "brown shulker box",
    "green shulker box",
    "red shulker box",
    "black shulker box",
];

/// Side, lid-top and bottom layers of colour index `c` (0 plain).
const fn layers(c: u16) -> (u16, u16, u16) {
    let base = TEX + c * 3;
    (base, base + 1, base + 2)
}

/// Name, render kind and faces (as an upright box: the icon and break
/// particles use them; the model itself is drawn by `render::block_model`).
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    let c = (id - FIRST) / 6;
    let (side, top, bottom) = layers(c);
    (NAMES[c as usize], RenderKind::Shaped, [side, side, top, bottom, side, side])
}

/// The shell colour: Java's plain box is the shulker's lavender purple.
fn shell(c: u16) -> [i32; 3] {
    if c == 0 {
        return [151, 102, 150];
    }
    let [r, g, b] = DyeColor::ALL[(c - 1) as usize].rgb();
    [i32::from(r), i32::from(g), i32::from(b)]
}

/// Original texture. Side (`base`): the lid's shell over the top twelve
/// rows with a ridge and a dark rim, the base below it; lid top
/// (`base + 1`): a raised panel with a darker border; bottom (`base + 2`):
/// the base's underside.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let i = layer - TEX;
    let (c, face) = (i / 3, i % 3);
    let [r, g, b] = shell(c);
    let (x, y) = (x as i32, y as i32);
    // Speckle so flat colours read as a shell.
    let grain = ((x * 7 + y * 13 + i32::from(c) * 5) % 11 - 5) * 2;
    let shade = |k: i32| {
        [
            (r * k / 100 + grain).clamp(0, 255) as u8,
            (g * k / 100 + grain).clamp(0, 255) as u8,
            (b * k / 100 + grain).clamp(0, 255) as u8,
            255,
        ]
    };
    let edge = x == 0 || x == 15;
    match face {
        // Side: rows 0..12 are the lid, 12..16 the base.
        0 => match y {
            0 => shade(118),
            11 => shade(62),
            12..=15 if y == 12 => shade(48),
            12..=15 => shade(if edge { 70 } else { 82 }),
            5 | 6 if (3..13).contains(&x) => shade(112),
            _ if edge => shade(88),
            _ => shade(100),
        },
        // Lid top: a raised square panel.
        1 => {
            let ring = x.min(y).min(15 - x).min(15 - y);
            match ring {
                0 => shade(80),
                1 | 2 => shade(104),
                3 => shade(90),
                _ => shade(114),
            }
        }
        _ => {
            let ring = x.min(y).min(15 - x).min(15 - y);
            shade(if ring == 0 { 58 } else { 72 })
        }
    }
}

/// One cuboid of the model in upright model pixels, with its texture rows.
struct Part {
    min: [f32; 3],
    max: [f32; 3],
    /// Side rows: the box's `y` maps to row `16 - y` (closed, the lid and
    /// base together draw the whole side texture).
    lid: bool,
}

/// `ShulkerBoxRenderer`'s quads for the box at `progress` (0 shut, 1 open),
/// in the cell's 0..1 coordinates, handed to `emit` one at a time.
pub fn quads(s: State, progress: f32, mut emit: impl FnMut(Quad)) {
    let c = match s.colour {
        None => 0,
        Some(c) => c as u16 + 1,
    };
    let (side, top, bottom) = layers(c);
    let lift = progress * MAX_LID_HEIGHT * 16.0;
    let (sin, cos) = (progress * MAX_LID_ROTATION).to_radians().sin_cos();
    let parts = [
        Part { min: [0.0, 0.0, 0.0], max: [16.0, 8.0, 16.0], lid: false },
        Part { min: [0.0, 4.0, 0.0], max: [16.0, 16.0, 16.0], lid: true },
    ];
    for part in &parts {
        let (lo, hi) = (Vec3::from_array(part.min), Vec3::from_array(part.max));
        // Model pixels to the upright frame: the lid lifts and turns about
        // the box's axis.
        let pose = |p: Vec3| {
            let q = (p - Vec3::new(8.0, 0.0, 8.0)) / 16.0;
            if part.lid {
                Vec3::new(q.x * cos - q.z * sin, q.y + lift / 16.0, q.x * sin + q.z * cos) - Vec3::Y * 0.5
            } else {
                q - Vec3::Y * 0.5
            }
        };
        let turn = |n: Vec3| if part.lid { Vec3::new(n.x * cos - n.z * sin, n.y, n.x * sin + n.z * cos) } else { n };
        for face in 0..6 {
            let (d, positive) = (face / 2, face % 2 == 0);
            let (u, v) = ((d + 1) % 3, (d + 2) % 3);
            let corner = |a: bool, b: bool| {
                let mut p = [0.0f32; 3];
                p[d] = if positive { hi[d] } else { lo[d] };
                p[u] = if a { hi[u] } else { lo[u] };
                p[v] = if b { hi[v] } else { lo[v] };
                Vec3::from_array(p)
            };
            let model = if positive {
                [corner(false, false), corner(true, false), corner(true, true), corner(false, true)]
            } else {
                [corner(false, false), corner(false, true), corner(true, true), corner(true, false)]
            };
            let layer = match (d, positive, part.lid) {
                (1, true, true) => top,
                (1, _, _) => bottom,
                _ => side,
            };
            let uv = model.map(|p| match d {
                0 => [p.z / 16.0, (16.0 - p.y) / 16.0],
                1 => [p.x / 16.0, p.z / 16.0],
                _ => [p.x / 16.0, (16.0 - p.y) / 16.0],
            });
            let mut normal = Vec3::ZERO;
            normal[d] = if positive { 1.0 } else { -1.0 };
            emit(Quad {
                corners: model.map(|p| s.facing.orient(pose(p)) + Vec3::splat(0.5)),
                uv,
                layer,
                normal: s.facing.orient(turn(normal)),
                tint: None,
            });
        }
    }
}

/// `ShulkerBoxBlockEntity.AnimationStatus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Status {
    #[default]
    Closed,
    Opening,
    Opened,
    Closing,
}

/// A box's lid: status and progress now and a tick ago.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Lid {
    pub status: Status,
    pub progress: f32,
    pub progress_old: f32,
}

impl Lid {
    /// `ShulkerBoxBlockEntity.updateAnimation`.
    pub fn tick(&mut self) {
        self.progress_old = self.progress;
        match self.status {
            Status::Closed => self.progress = 0.0,
            Status::Opening => {
                self.progress += LID_STEP;
                if self.progress >= 1.0 {
                    self.status = Status::Opened;
                    self.progress = 1.0;
                }
            }
            Status::Opened => self.progress = 1.0,
            Status::Closing => {
                self.progress -= LID_STEP;
                if self.progress <= 0.0 {
                    self.status = Status::Closed;
                    self.progress = 0.0;
                }
            }
        }
    }

    /// `getProgress(partialTick)`.
    pub fn at(&self, alpha: f32) -> f32 {
        self.progress_old + (self.progress - self.progress_old) * alpha.clamp(0.0, 1.0)
    }
}

/// A box sound: `SHULKER_BOX_OPEN` when the first viewer opens it,
/// `SHULKER_BOX_CLOSE` when the last leaves (0.5 volume, pitch 0.9–1.0,
/// `BLOCKS`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cue {
    pub pos: IVec3,
    pub open: bool,
}

/// The shulker boxes' block-entity state beyond their slots (not saved:
/// boxes load shut, as Java's do).
#[derive(Default)]
pub struct Shulkers {
    /// Every box in a loaded chunk, for the renderer.
    positions: FxHashMap<IVec3, Block>,
    lids: FxHashMap<IVec3, Lid>,
    viewers: FxHashMap<IVec3, u8>,
    /// The slots of boxes just removed, for the drop that follows.
    removed: FxHashMap<IVec3, Chest>,
    /// Lid sounds since the game last asked.
    pub cues: Vec<Cue>,
}

/// Whether a chunk can hold a box, without visiting every cell when its
/// storage says it can't (byte storage holds only ids below 256).
fn may_hold(data: &super::chunk::ChunkData) -> bool {
    use super::chunk::ChunkData;
    match data {
        ChunkData::Uniform(b) => owns(b.0),
        ChunkData::Bytes(_) => false,
        ChunkData::Paletted { palette, len, .. } => palette[..*len as usize].iter().any(|b| owns(b.0)),
        ChunkData::Dense(_) => true,
    }
}

impl World {
    /// Indexes the boxes of a chunk being loaded.
    pub(super) fn register_shulker_boxes(&mut self, cpos: IVec3, data: &super::chunk::ChunkData) {
        use super::chunk::CHUNK_SIZE;
        if !may_hold(data) {
            return;
        }
        let base = cpos * CHUNK_SIZE as i32;
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let b = data.get(x, y, z);
                    if owns(b.0) {
                        self.shulker_boxes.positions.insert(base + IVec3::new(x as i32, y as i32, z as i32), b);
                    }
                }
            }
        }
    }

    /// Drops an unloading (or replaced) chunk's boxes from the index.
    pub(super) fn forget_shulker_boxes(&mut self, cpos: IVec3) {
        if !self.shulker_boxes.positions.is_empty() {
            self.shulker_boxes.positions.retain(|p, _| super::chunk::chunk_of(*p) != cpos);
        }
    }

    /// Keeps the index in step with a block change.
    pub(super) fn track_shulker_box(&mut self, p: IVec3, old: Block, new: Block) {
        if owns(new.0) {
            self.shulker_boxes.positions.insert(p, new);
        } else if owns(old.0) {
            self.shulker_boxes.positions.remove(&p);
        }
    }

    /// Boxes within [`VIEW_DISTANCE`] of `camera`, for the renderer.
    pub fn shulker_boxes_near(&self, camera: DVec3) -> impl Iterator<Item = (IVec3, Block)> + '_ {
        self.shulker_boxes
            .positions
            .iter()
            .filter(move |(p, _)| (p.as_dvec3() + DVec3::splat(0.5)).distance_squared(camera) < VIEW_DISTANCE.powi(2))
            .map(|(&p, &b)| (p, b))
    }

    /// Who has which box open this tick, from every input path
    /// (`startOpen`/`stopOpen`): the lid starts opening or closing and the
    /// box sounds when the first viewer arrives or the last leaves.
    pub fn set_shulker_viewers(&mut self, open: &[IVec3]) {
        let mut viewers: FxHashMap<IVec3, u8> = FxHashMap::default();
        for &p in open {
            if self.get_block(p).is_some_and(|b| owns(b.0)) {
                let n = viewers.entry(p).or_default();
                *n = n.saturating_add(1);
            }
        }
        if viewers == self.shulker_boxes.viewers {
            return;
        }
        let old = std::mem::replace(&mut self.shulker_boxes.viewers, viewers);
        for (&p, _) in self.shulker_boxes.viewers.iter().filter(|(p, _)| !old.contains_key(p)) {
            self.shulker_boxes.lids.entry(p).or_default().status = Status::Opening;
            if self.shulker_boxes.cues.len() < 64 {
                self.shulker_boxes.cues.push(Cue { pos: p, open: true });
            }
        }
        let closed: Vec<IVec3> = old.keys().filter(|p| !self.shulker_boxes.viewers.contains_key(p)).copied().collect();
        for p in closed {
            if let Some(lid) = self.shulker_boxes.lids.get_mut(&p) {
                lid.status = Status::Closing;
            }
            if self.shulker_boxes.cues.len() < 64 {
                self.shulker_boxes.cues.push(Cue { pos: p, open: false });
            }
            self.emit_game_event(
                p.as_dvec3() + DVec3::splat(0.5),
                super::vibration::GameEvent::ContainerClose,
                super::vibration::EventSource::NONE,
                None,
            );
        }
    }

    /// Moves every lid that is open or moving one tick; shut lids are
    /// forgotten.
    pub fn tick_shulker_boxes(&mut self) {
        if self.shulker_boxes.lids.is_empty() {
            return;
        }
        let lids = &mut self.shulker_boxes.lids;
        for lid in lids.values_mut() {
            lid.tick();
        }
        lids.retain(|_, l| l.status != Status::Closed || l.progress_old != 0.0);
        let gone: Vec<IVec3> = self
            .shulker_boxes
            .lids
            .keys()
            .filter(|&&p| !self.get_block(p).is_some_and(|b| owns(b.0)))
            .copied()
            .collect();
        for p in gone {
            self.shulker_boxes.lids.remove(&p);
        }
    }

    /// The lid of the box at `p` between the last two ticks (0 shut).
    pub fn shulker_lid(&self, p: IVec3, alpha: f32) -> f32 {
        self.shulker_boxes.lids.get(&p).map_or(0.0, |l| l.at(alpha))
    }

    /// The boxes whose lids are open or moving, with progress now and a
    /// tick ago (LAN sends these to clients).
    pub fn shulker_lids(&self) -> impl Iterator<Item = (IVec3, Lid)> + '_ {
        self.shulker_boxes.lids.iter().map(|(&p, &l)| (p, l))
    }

    /// A LAN client's lids: the host's open or moving boxes near it.
    pub fn replace_shulker_lids(&mut self, lids: impl IntoIterator<Item = (IVec3, Lid)>) {
        self.shulker_boxes.lids.clear();
        self.shulker_boxes.lids.extend(lids);
    }

    /// Lid sounds since the last call.
    pub fn take_shulker_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.shulker_boxes.cues)
    }

    /// `ShulkerBoxBlock.canOpen`: a moving or open lid, or room for the lid
    /// in the half block in front of it.
    pub fn shulker_box_opens(&self, p: IVec3) -> bool {
        let Some(s) = self.get_block(p).and_then(decode) else { return false };
        if self.shulker_boxes.lids.get(&p).is_some_and(|l| l.status != Status::Closed) {
            return true;
        }
        let d = s.facing.offset().as_dvec3();
        let centre = p.as_dvec3() + DVec3::splat(0.5);
        // `Shulker.getProgressDeltaAabb(1, facing, 0, 0.5)`, deflated 1e-6.
        let face = centre + d * 0.5;
        let far = face + d * 0.5;
        let lateral = DVec3::ONE - d.abs();
        let min = face.min(far) - lateral * 0.5 + DVec3::splat(1.0e-6);
        let max = face.max(far) + lateral * 0.5 - DVec3::splat(1.0e-6);
        crate::physics::no_collision(self, min, max)
    }

    /// Keeps a removed box's slots for its drop instead of spilling them.
    pub(super) fn stash_shulker_box(&mut self, p: IVec3, chest: Chest) {
        self.shulker_boxes.removed.insert(p, chest);
    }

    /// Forgets slots no drop claimed (a box replaced by a command, or broken
    /// with drops off).
    pub fn clear_removed_shulker_boxes(&mut self) {
        self.shulker_boxes.removed.clear();
    }

    /// The item a box at `p` drops: itself with its contents. Takes the
    /// slots of a box just removed there, else those of the box still
    /// standing (explosions and pistons roll the drop first).
    pub(crate) fn shulker_box_drop(&mut self, p: IVec3, block: Block) -> Option<Stack> {
        let s = decode(block)?;
        let slots = match self.shulker_boxes.removed.remove(&p) {
            Some(chest) => chest.slots,
            None => self.chests.get(&p).map_or([None; SLOTS], |c| c.slots),
        };
        Some(crate::content::shulker_box::with_contents(
            Stack::new(crate::content::shulker_box::item(s.colour), 1),
            &slots,
        ))
    }

    /// `ShulkerBoxBlock.playerWillDestroy` for a Creative player: a box that
    /// holds something still drops, with its contents. Call after removing it.
    pub fn creative_shulker_box_drop(&mut self, p: IVec3, block: Block) {
        if decode(block).is_none() {
            return;
        }
        if self.shulker_boxes.removed.get(&p).is_some_and(|c| c.slots.iter().any(Option::is_some))
            && let Some(stack) = self.shulker_box_drop(p, block)
        {
            self.drops.push((p, stack));
        }
    }

    /// The box at `p` takes the placed item's contents
    /// (`BlockItem.updateCustomBlockEntityTag` with its `container`).
    pub fn restore_shulker_box_item(&mut self, p: IVec3, item: &Stack) {
        if !self.get_block(p).is_some_and(|b| owns(b.0)) || !crate::content::shulker_box::is_shulker_box(item.item) {
            return;
        }
        let slots = crate::content::shulker_box::contents(item);
        if let Some(chest) = self.chests.get_mut(&p) {
            chest.slots = slots;
        }
        self.redstone_changed(p);
    }

    /// Whether the container at `p` takes `stack`: a shulker box takes no
    /// shulker box (`ShulkerBoxSlot.mayPlace`).
    pub fn container_accepts(&self, p: IVec3, stack: &Stack) -> bool {
        !self.get_block(p).is_some_and(|b| owns(b.0)) || crate::content::shulker_box::can_fit_inside_container(stack)
    }

    /// `ShulkerBoxDispenseBehavior`: a dispenser facing `toward` places the
    /// box in front (lid toward the dispenser's facing, or up when there is
    /// something under the spot). Returns whether it was placed.
    pub fn dispense_shulker_box(&mut self, front: IVec3, toward: IVec3, stack: &Stack) -> bool {
        let Some(raw) = stack.item.block().filter(|b| owns(b.0)) else { return false };
        if !self.get_block(front).is_some_and(Block::is_replaceable) {
            return false;
        }
        let below_empty = self.get_block(front - IVec3::Y) == Some(Block::AIR);
        let normal = if below_empty { toward } else { IVec3::Y };
        let Some(placed) = placed(raw, normal) else { return false };
        if !self.set_block(front, placed) {
            return false;
        }
        self.restore_shulker_box_item(front, stack);
        true
    }

    /// Open or moving lids within `radius` of `at`, for a LAN client.
    pub fn shulker_lids_near(&self, at: DVec3, radius: f64) -> Vec<serde_json::Value> {
        self.shulker_boxes
            .lids
            .iter()
            .filter(|(p, _)| (p.as_dvec3() + DVec3::splat(0.5)).distance_squared(at) <= radius * radius)
            .take(256)
            .map(|(p, l)| serde_json::json!([p.x, p.y, p.z, l.progress_old, l.progress]))
            .collect()
    }

    /// Applies [`World::shulker_lids_near`] on a LAN client.
    pub fn apply_shulker_lids(&mut self, list: &serde_json::Value) {
        let Some(list) = list.as_array() else { return };
        let lids = list.iter().filter_map(|v| {
            let a = v.as_array()?;
            let n = |i: usize| a.get(i).and_then(serde_json::Value::as_f64);
            let p = IVec3::new(n(0)? as i32, n(1)? as i32, n(2)? as i32);
            let (old, now) = (n(3)? as f32, n(4)? as f32);
            let status = if now >= 1.0 {
                Status::Opened
            } else if now < old {
                Status::Closing
            } else if now <= 0.0 {
                Status::Closed
            } else {
                Status::Opening
            };
            Some((p, Lid { status, progress: now.clamp(0.0, 1.0), progress_old: old.clamp(0.0, 1.0) }))
        });
        self.replace_shulker_lids(lids.collect::<Vec<_>>());
    }
}

#[cfg(test)]
#[path = "shulker_box/tests.rs"]
mod tests;
