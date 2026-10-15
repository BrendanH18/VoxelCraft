//! Java 26.3 ground pathfinding: `WalkNodeEvaluator` path types and neighbours,
//! `PathFinder`'s A* search and `PathNavigation`/`GroundPathNavigation` path
//! following, with `MoveControl`'s turn-and-jump steering in [`steer`].
//!
//! Reference: the official 26.3 server artifact (see
//! `tests/fixtures/java/26.3/README.md`), classes
//! `net.minecraft.world.level.pathfinder.{PathType, NodeEvaluator,
//! WalkNodeEvaluator, PathFinder, BinaryHeap, Node, Target, Path}` and
//! `net.minecraft.world.entity.ai.{navigation.PathNavigation,
//! navigation.GroundPathNavigation, control.MoveControl}`. The search keeps
//! Java's node identity, cost-to-max updates, binary-heap tie order,
//! 1.5 heuristic weight, visited-node budget and best-node fallback, so the
//! same terrain gives the same path.
//!
//! Known differences, all observable only at the edges:
//! - Unloaded cells are `Blocked` (Java's navigation region reads them as air,
//!   which the evaluator then rejects as an over-long fall).
//! - Entity collision boxes (boats, shulkers) and the world border are not
//!   part of `hasCollisions`; block collision is.
//! - Blocks VoxelCraft lacks (powder snow, honey, campfires, cauldrons,
//!   wither roses) have no path types of their own.
use rustc_hash::FxHashMap;
use std::sync::OnceLock;

use glam::{DVec3, IVec3};

use super::MobWorld;
use crate::physics::{self, BlockSource};
use crate::world::block::{Block, RenderKind, STATE_CAPACITY, Shaped};
use crate::world::chunk::WORLD_MIN_Y;

/// Java's `PathType`, in its ordinal order (iteration order matters when a
/// mob's box covers cells of equal cost).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum PathType {
    Blocked,
    Open,
    Walkable,
    WalkableDoor,
    Trapdoor,
    PowderSnow,
    OnTopOfPowderSnow,
    Fence,
    Lava,
    Water,
    WaterBorder,
    Rail,
    UnpassableRail,
    FireInNeighbor,
    Fire,
    DamagingInNeighbor,
    Damaging,
    DoorOpen,
    DoorWoodClosed,
    DoorIronClosed,
    Breach,
    Leaves,
    StickyHoney,
    Cocoa,
    DamageCautious,
    OnTopOfTrapdoor,
    BigMobsCloseToDanger,
}

impl PathType {
    pub const COUNT: usize = 27;
    const ALL: [PathType; Self::COUNT] = [
        PathType::Blocked,
        PathType::Open,
        PathType::Walkable,
        PathType::WalkableDoor,
        PathType::Trapdoor,
        PathType::PowderSnow,
        PathType::OnTopOfPowderSnow,
        PathType::Fence,
        PathType::Lava,
        PathType::Water,
        PathType::WaterBorder,
        PathType::Rail,
        PathType::UnpassableRail,
        PathType::FireInNeighbor,
        PathType::Fire,
        PathType::DamagingInNeighbor,
        PathType::Damaging,
        PathType::DoorOpen,
        PathType::DoorWoodClosed,
        PathType::DoorIronClosed,
        PathType::Breach,
        PathType::Leaves,
        PathType::StickyHoney,
        PathType::Cocoa,
        PathType::DamageCautious,
        PathType::OnTopOfTrapdoor,
        PathType::BigMobsCloseToDanger,
    ];

    /// Java's default cost; negative means impassable.
    pub const fn default_malus(self) -> f32 {
        match self {
            PathType::Blocked
            | PathType::PowderSnow
            | PathType::Fence
            | PathType::Lava
            | PathType::UnpassableRail
            | PathType::Damaging
            | PathType::DoorWoodClosed
            | PathType::DoorIronClosed
            | PathType::Leaves => -1.0,
            PathType::Water
            | PathType::WaterBorder
            | PathType::FireInNeighbor
            | PathType::DamagingInNeighbor
            | PathType::StickyHoney => 8.0,
            PathType::Fire => 16.0,
            PathType::Breach | PathType::BigMobsCloseToDanger => 4.0,
            _ => 0.0,
        }
    }
}

/// A mob's path costs: Java's defaults with `setPathfindingMalus` overrides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Malus([f32; PathType::COUNT]);

impl Default for Malus {
    fn default() -> Self {
        let mut costs = [0.0; PathType::COUNT];
        for t in PathType::ALL {
            costs[t as usize] = t.default_malus();
        }
        Self(costs)
    }
}

impl Malus {
    pub fn with(mut self, t: PathType, cost: f32) -> Self {
        self.0[t as usize] = cost;
        self
    }
    #[inline]
    pub fn get(&self, t: PathType) -> f32 {
        self.0[t as usize]
    }
}

/// What a mob's ground navigation can do (Java's evaluator flags and the
/// mob attributes the search reads).
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    /// Collision box width and height (Java's `getBbWidth`/`getBbHeight`).
    pub width: f32,
    pub height: f32,
    /// `maxUpStep`: 0.6 for most mobs, 1.0 for horses/golems/endermen, 1.5 for camels.
    pub step: f32,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    /// `FloatGoal` mobs swim at the surface; others sink and walk the bottom.
    pub can_float: bool,
    pub can_walk_over_fences: bool,
    /// Striders treat lava as floor (`canStandOnFluid`).
    pub stand_on_lava: bool,
    /// `Mob.getMaxFallDistance`.
    pub max_fall: i32,
    /// `max(FOLLOW_RANGE, requiredPathLength)`: the walked-distance limit.
    pub max_path_length: f32,
    /// `floor(max path length * 16)`.
    pub max_visited: u32,
    pub malus: Malus,
}

impl Caps {
    /// A default 0.6 x 1.8-ish mob with Java's base values.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            step: 0.6,
            can_open_doors: false,
            can_pass_doors: true,
            can_float: false,
            can_walk_over_fences: false,
            stand_on_lava: false,
            max_fall: 3,
            max_path_length: 16.0,
            max_visited: 256,
            malus: Malus::default(),
        }
    }

    /// Sets Java's `FOLLOW_RANGE` (the search budget and length both follow it).
    pub fn follow_range(mut self, range: f32) -> Self {
        self.max_path_length = range.max(16.0);
        self.max_visited = (self.max_path_length * 16.0).floor() as u32;
        self
    }

    fn malus(&self, t: PathType) -> f32 {
        self.malus.get(t)
    }
}

/// Where a search starts: the mob's feet and the movement state Java reads.
#[derive(Clone, Copy, Debug)]
pub struct Start {
    pub pos: DVec3,
    pub on_ground: bool,
    /// In water deep enough to float (`isInFloatableFluid`).
    pub in_water: bool,
}

// ---------------------------------------------------------------------------
// Block state -> path type (`WalkNodeEvaluator.getPathTypeFromState`).

static STATE_TYPES: OnceLock<Box<[PathType]>> = OnceLock::new();

/// Java's per-state path type, from a table built once for every state.
#[inline]
pub fn state_type(b: Option<Block>) -> PathType {
    let Some(b) = b else { return PathType::Blocked };
    let table = STATE_TYPES.get_or_init(|| (0..STATE_CAPACITY as u16).map(|i| classify(Block(i))).collect());
    table.get(b.0 as usize).copied().unwrap_or(PathType::Blocked)
}

fn classify(b: Block) -> PathType {
    use crate::world::{overworld_blocks as ow, redstone_blocks as rs};
    let name = b.name();
    if b == Block::AIR {
        return PathType::Open;
    }
    let trapdoor = matches!(rs::component(b), Some(rs::Component::Trapdoor { .. }));
    if trapdoor || b == ow::LILY_PAD || name == "big dripleaf" {
        return PathType::Trapdoor;
    }
    if b == Block::CACTUS || name == "sweet berry bush" {
        return PathType::Damaging;
    }
    if name == "cocoa" {
        return PathType::Cocoa;
    }
    if name == "pointed dripstone" {
        return PathType::DamageCautious;
    }
    if b.is_lava() {
        return PathType::Lava;
    }
    // `NodeEvaluator.isBurningBlock`: fire, lava, magma (and lit campfires).
    if b.is_fire() || name == "magma block" {
        return PathType::Fire;
    }
    match b.shaped() {
        Some(Shaped::Door { open: true, .. }) => return PathType::DoorOpen,
        Some(Shaped::Door { .. }) if name == "iron door" => return PathType::DoorIronClosed,
        Some(Shaped::Door { .. }) => return PathType::DoorWoodClosed,
        Some(Shaped::Rail) => return PathType::Rail,
        _ => {}
    }
    if b.is_rail() {
        return PathType::Rail;
    }
    if b.is_leaves() || name.ends_with(" leaves") {
        return PathType::Leaves;
    }
    if matches!(b.shaped(), Some(Shaped::Fence | Shaped::Wall | Shaped::Gate { open: false, .. })) {
        return PathType::Fence;
    }
    if !pathfindable_on_land(b) {
        return PathType::Blocked;
    }
    if b.holds_water() || b.is_water() { PathType::Water } else { PathType::Open }
}

/// `BlockBehaviour.isPathfindable(LAND)`: not a full collision cube, except
/// for the blocks Java overrides (slabs, stairs, beds, chests, farmland, ...).
pub fn pathfindable_on_land(b: Block) -> bool {
    use crate::world::{overworld_blocks as ow, redstone_blocks as rs};
    let name = b.name();
    const NEVER: [&str; 24] = [
        "anvil",
        "chipped anvil",
        "damaged anvil",
        "azalea",
        "flowering azalea",
        "bamboo",
        "bell",
        "brewing stand",
        "cactus",
        "cake",
        "chain",
        "chest",
        "cocoa",
        "composter",
        "dragon egg",
        "enchanting table",
        "end portal frame",
        "farmland",
        "wet farmland",
        "grindstone",
        "lectern",
        "mud",
        "dirt path",
        "sea pickle",
    ];
    if NEVER.contains(&name)
        || matches!(name, "soul sand" | "pointed dripstone" | "stonecutter" | "iron bars")
        || name.ends_with(" slab")
        || name.ends_with(" stairs")
        || name.ends_with(" pane")
        || name.starts_with("bed ")
        || name.ends_with(" bed foot")
        || name.ends_with(" bed head")
        || b.is_slab()
        || b.is_bed()
        || matches!(rs::component(b), Some(rs::Component::Hopper { .. }))
        || matches!(b.shaped(), Some(Shaped::Stairs(_) | Shaped::Pane | Shaped::Fence | Shaped::Wall))
    {
        return false;
    }
    if let Some(Shaped::Door { open, .. } | Shaped::Gate { open, .. }) = b.shaped() {
        return open;
    }
    if let Some(rs::Component::Trapdoor { open, .. }) = rs::component(b) {
        return open;
    }
    if let Some(layers) = ow::snow_layers(b) {
        return layers < 5;
    }
    if b.is_fluid() {
        return !b.is_lava();
    }
    !full_collision(b)
}

/// `isCollisionShapeFullBlock`: one box filling the cell.
fn full_collision(b: Block) -> bool {
    if !b.is_solid() {
        return false;
    }
    if b.kind() != RenderKind::Shaped {
        return b.height() >= 1.0;
    }
    let boxes = crate::world::shape::collision(b, |_| Block::AIR, Block::AIR);
    matches!(boxes.as_slice(), [bx] if bx.min == [0, 0, 0] && bx.max == [16, 16, 16])
}

/// Java's `WalkNodeEvaluator.getPathTypeStatic`: a cell's own type, with open
/// cells classified by what lies under and beside them.
pub fn static_type<W: BlockSource + ?Sized>(world: &W, p: IVec3) -> PathType {
    let t = state_type(world.block(p));
    if t != PathType::Open || p.y < WORLD_MIN_Y + 1 {
        return t;
    }
    match state_type(world.block(p - IVec3::Y)) {
        PathType::Open | PathType::Water | PathType::Lava | PathType::Walkable => PathType::Open,
        PathType::Fire => PathType::Fire,
        PathType::Damaging => PathType::Damaging,
        PathType::StickyHoney => PathType::StickyHoney,
        PathType::PowderSnow => PathType::OnTopOfPowderSnow,
        PathType::DamageCautious => PathType::DamageCautious,
        PathType::Trapdoor => PathType::OnTopOfTrapdoor,
        _ => neighbour_type(world, p, PathType::Walkable),
    }
}

/// `checkNeighbourBlocks`: hazards in the surrounding 3x3x3 (minus the column).
fn neighbour_type<W: BlockSource + ?Sized>(world: &W, p: IVec3, fallback: PathType) -> PathType {
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                match state_type(world.block(p + IVec3::new(dx, dy, dz))) {
                    PathType::Damaging => return PathType::DamagingInNeighbor,
                    PathType::Fire | PathType::Lava => return PathType::FireInNeighbor,
                    PathType::Water => return PathType::WaterBorder,
                    PathType::DamageCautious => return PathType::DamageCautious,
                    _ => {}
                }
            }
        }
    }
    fallback
}

/// `WalkNodeEvaluator.getFloorLevel(level, pos)`: the top of the collision
/// shape under `p`.
pub fn floor_level<W: BlockSource + ?Sized>(world: &W, p: IVec3) -> f64 {
    let below = p - IVec3::Y;
    below.y as f64 + physics::collision_top(world, below).unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// The search (`PathFinder` over a `WalkNodeEvaluator`).

const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
struct Node {
    pos: IVec3,
    heap_idx: i32,
    g: f32,
    h: f32,
    f: f32,
    came_from: u32,
    closed: bool,
    walked: f32,
    cost: f32,
    ty: PathType,
}

impl Node {
    fn new(pos: IVec3) -> Self {
        Self {
            pos,
            heap_idx: -1,
            g: 0.0,
            h: 0.0,
            f: 0.0,
            came_from: NONE,
            closed: false,
            walked: 0.0,
            cost: 0.0,
            ty: PathType::Blocked,
        }
    }
}

fn dist(a: IVec3, b: IVec3) -> f32 {
    let d = (b - a).as_vec3();
    (d.x * d.x + d.y * d.y + d.z * d.z).sqrt()
}

fn manhattan(a: IVec3, b: IVec3) -> f32 {
    let d = (b - a).abs().as_vec3();
    d.x + d.y + d.z
}

/// Java's `Direction.Plane.HORIZONTAL` order (north, east, south, west) and
/// each direction's `get2DDataValue` slot.
const HORIZONTAL: [(IVec3, usize); 4] =
    [(IVec3::new(0, 0, -1), 2), (IVec3::new(1, 0, 0), 3), (IVec3::new(0, 0, 1), 0), (IVec3::new(-1, 0, 0), 1)];

/// A collision box, keyed by its exact bits for Java's per-search cache.
type BoxKey = [u64; 6];

struct Search<'a, W: ?Sized> {
    world: &'a W,
    caps: &'a Caps,
    start: Start,
    /// Mob box at the start position.
    bb: (DVec3, DVec3),
    block_pos: IVec3,
    entity_w: i32,
    entity_h: i32,
    nodes: Vec<Node>,
    index: FxHashMap<IVec3, u32>,
    types: FxHashMap<IVec3, PathType>,
    collisions: FxHashMap<BoxKey, bool>,
    heap: Vec<u32>,
}

impl<'a, W: BlockSource + ?Sized> Search<'a, W> {
    fn new(world: &'a W, caps: &'a Caps, start: Start) -> Self {
        let half = caps.width as f64 / 2.0;
        let bb = (start.pos - DVec3::new(half, 0.0, half), start.pos + DVec3::new(half, caps.height as f64, half));
        Self {
            world,
            caps,
            start,
            bb,
            block_pos: start.pos.floor().as_ivec3(),
            entity_w: (caps.width + 1.0).floor() as i32,
            entity_h: (caps.height + 1.0).floor() as i32,
            nodes: Vec::with_capacity(512),
            index: FxHashMap::with_capacity_and_hasher(1024, Default::default()),
            types: FxHashMap::with_capacity_and_hasher(1024, Default::default()),
            collisions: FxHashMap::default(),
            heap: Vec::with_capacity(128),
        }
    }

    fn node(&mut self, p: IVec3) -> u32 {
        if let Some(&i) = self.index.get(&p) {
            return i;
        }
        let i = self.nodes.len() as u32;
        self.nodes.push(Node::new(p));
        self.index.insert(p, i);
        i
    }

    // -- types --------------------------------------------------------------

    fn cached_type(&mut self, p: IVec3) -> PathType {
        if let Some(&t) = self.types.get(&p) {
            return t;
        }
        let t = self.type_of_mob(p);
        self.types.insert(p, t);
        t
    }

    /// `getPathTypeOfMob`: the cells the mob's box would cover from this corner.
    fn type_of_mob(&self, p: IVec3) -> PathType {
        let mut set = 0u32;
        for dx in 0..self.entity_w {
            for dy in 0..self.entity_h {
                for dz in 0..self.entity_w {
                    let mut t = static_type(self.world, p + IVec3::new(dx, dy, dz));
                    if t == PathType::DoorWoodClosed && self.caps.can_open_doors && self.caps.can_pass_doors {
                        t = PathType::WalkableDoor;
                    }
                    if t == PathType::DoorOpen && !self.caps.can_pass_doors {
                        t = PathType::Blocked;
                    }
                    if t == PathType::Rail
                        && static_type(self.world, self.block_pos) != PathType::Rail
                        && static_type(self.world, self.block_pos - IVec3::Y) != PathType::Rail
                    {
                        t = PathType::UnpassableRail;
                    }
                    set |= 1 << t as u32;
                }
            }
        }
        if set.count_ones() == 1 {
            return PathType::ALL[set.trailing_zeros() as usize];
        }
        if set & 1 << PathType::Fence as u32 != 0 {
            return PathType::Fence;
        }
        if set & 1 << PathType::UnpassableRail as u32 != 0 {
            return PathType::UnpassableRail;
        }
        let mut highest = PathType::Blocked;
        let mut highest_cost = self.caps.malus(highest);
        for t in PathType::ALL {
            if set & 1 << t as u32 == 0 {
                continue;
            }
            let cost = self.caps.malus(t);
            if cost < 0.0 {
                return t;
            }
            if cost >= highest_cost {
                highest_cost = cost;
                highest = t;
            }
        }
        let own = static_type(self.world, p);
        if self.entity_w > 1 {
            let cheaper = self.caps.malus(own) < highest_cost;
            if cheaper && self.caps.malus(PathType::BigMobsCloseToDanger) < highest_cost {
                PathType::BigMobsCloseToDanger
            } else {
                highest
            }
        } else if own == PathType::Open && highest != PathType::Open && highest_cost == 0.0 {
            PathType::Open
        } else {
            highest
        }
    }

    fn floor(&self, p: IVec3) -> f64 {
        if self.caps.can_float && self.world.block(p).is_some_and(|b| b.holds_water()) {
            p.y as f64 + 0.5
        } else {
            floor_level(self.world, p)
        }
    }

    fn has_collisions(&mut self, min: DVec3, max: DVec3) -> bool {
        let key = [min.x, min.y, min.z, max.x, max.y, max.z].map(f64::to_bits);
        if let Some(&hit) = self.collisions.get(&key) {
            return hit;
        }
        let hit = physics::box_overlaps_solid(self.world, min, max);
        self.collisions.insert(key, hit);
        hit
    }

    // -- start and neighbours ----------------------------------------------

    fn can_start_at(&mut self, p: IVec3) -> bool {
        let t = self.cached_type(p);
        t != PathType::Open && self.caps.malus(t) >= 0.0
    }

    fn start_node(&mut self, p: IVec3) -> u32 {
        let i = self.node(p);
        let t = self.cached_type(p);
        let n = &mut self.nodes[i as usize];
        n.ty = t;
        n.cost = self.caps.malus(t);
        i
    }

    /// `WalkNodeEvaluator.getStart`.
    fn start(&mut self) -> u32 {
        let pos = self.start.pos;
        let (x, z) = (pos.x.floor() as i32, pos.z.floor() as i32);
        let mut y = pos.y.floor() as i32;
        let column = |y: i32| IVec3::new(x, y, z);
        let at = |s: &Self, y: i32| s.world.block(column(y));
        if self.caps.stand_on_lava && at(self, y).is_some_and(Block::is_lava) {
            while at(self, y).is_some_and(Block::is_lava) {
                y += 1;
            }
            y -= 1;
        } else if self.caps.can_float && self.start.in_water {
            while at(self, y).is_some_and(Block::holds_water) {
                y += 1;
            }
            y -= 1;
        } else if self.start.on_ground {
            y = (pos.y + 0.5).floor() as i32;
        } else {
            let mut probe = (pos.y + 1.0).floor() as i32;
            while probe > WORLD_MIN_Y {
                y = probe;
                probe -= 1;
                let below = at(self, probe);
                if below.is_some_and(|b| b != Block::AIR && !pathfindable_on_land(b)) || below.is_none() {
                    break;
                }
            }
        }
        let feet = IVec3::new(self.block_pos.x, y, self.block_pos.z);
        if !self.can_start_at(feet) {
            let (min, max) = self.bb;
            for (cx, cz) in [(min.x, min.z), (min.x, max.z), (max.x, min.z), (max.x, max.z)] {
                let corner = IVec3::new(cx.floor() as i32, y, cz.floor() as i32);
                if self.can_start_at(corner) {
                    return self.start_node(corner);
                }
            }
        }
        self.start_node(feet)
    }

    /// `getNeighbors`: up to four straight and four diagonal moves.
    fn neighbours(&mut self, current: u32, out: &mut Vec<u32>) {
        out.clear();
        let pos = self.nodes[current as usize].pos;
        let above = self.cached_type(pos + IVec3::Y);
        let here = self.cached_type(pos);
        let jump = if self.caps.malus(above) >= 0.0 && here != PathType::StickyHoney {
            self.caps.step.max(1.0).floor() as i32
        } else {
            0
        };
        let floor = self.floor(pos);
        let mut straight = [None; 4];
        for (step, slot) in HORIZONTAL {
            let n = self.accepted(pos + step, jump, floor, step, here);
            straight[slot] = n;
            if let Some(n) = n
                && self.neighbour_valid(n, current)
            {
                out.push(n);
            }
        }
        for (step, slot) in HORIZONTAL {
            let (second, second_slot) = clockwise(step);
            if self.diagonal_valid(current, straight[slot], straight[second_slot])
                && let Some(n) = self.accepted(pos + step + second, jump, floor, step, here)
            {
                let node = &self.nodes[n as usize];
                if !node.closed && node.ty != PathType::WalkableDoor && node.cost >= 0.0 {
                    out.push(n);
                }
            }
        }
    }

    fn neighbour_valid(&self, n: u32, current: u32) -> bool {
        let n = &self.nodes[n as usize];
        !n.closed && (n.cost >= 0.0 || self.nodes[current as usize].cost < 0.0)
    }

    fn diagonal_valid(&self, pos: u32, ew: Option<u32>, ns: Option<u32>) -> bool {
        let (Some(ew), Some(ns)) = (ew, ns) else { return false };
        let (pos, ew, ns) = (&self.nodes[pos as usize], &self.nodes[ew as usize], &self.nodes[ns as usize]);
        if ns.pos.y > pos.pos.y || ew.pos.y > pos.pos.y {
            return false;
        }
        if ew.ty == PathType::WalkableDoor || ns.ty == PathType::WalkableDoor {
            return false;
        }
        if self.caps.width > 1.0 && (ew.cost > 0.0 || ns.cost > 0.0) {
            return false;
        }
        let between_posts = ns.ty == PathType::Fence && ew.ty == PathType::Fence && self.caps.width < 0.5;
        (ns.pos.y < pos.pos.y || ns.cost >= 0.0 || between_posts)
            && (ew.pos.y < pos.pos.y || ew.cost >= 0.0 || between_posts)
    }

    fn update_cost_to_max(&mut self, p: IVec3, t: PathType, cost: f32) -> u32 {
        let i = self.node(p);
        let n = &mut self.nodes[i as usize];
        n.ty = t;
        n.cost = n.cost.max(cost);
        i
    }

    fn blocked_node(&mut self, p: IVec3) -> u32 {
        let i = self.node(p);
        let n = &mut self.nodes[i as usize];
        n.ty = PathType::Blocked;
        n.cost = -1.0;
        i
    }

    fn closed_node(&mut self, p: IVec3, t: PathType) -> u32 {
        let i = self.node(p);
        let n = &mut self.nodes[i as usize];
        n.closed = true;
        n.ty = t;
        n.cost = t.default_malus();
        i
    }

    /// `findAcceptedNode`.
    fn accepted(&mut self, p: IVec3, jump: i32, height: f64, dir: IVec3, current: PathType) -> Option<u32> {
        if self.floor(p) - height > (self.caps.step as f64).max(1.125) {
            return None;
        }
        let t = self.cached_type(p);
        let cost = self.caps.malus(t);
        let mut best = (cost >= 0.0).then(|| self.update_cost_to_max(p, t, cost));
        if partial_collision(current)
            && let Some(b) = best
            && self.nodes[b as usize].cost >= 0.0
            && !self.reach_without_collision(self.nodes[b as usize].pos)
        {
            best = None;
        }
        if t == PathType::Walkable {
            return best;
        }
        if best.is_none_or(|b| self.nodes[b as usize].cost < 0.0)
            && jump > 0
            && (t != PathType::Fence || self.caps.can_walk_over_fences)
            && !matches!(t, PathType::UnpassableRail | PathType::Trapdoor | PathType::PowderSnow)
        {
            self.jump_on(p, jump, height, dir, current)
        } else if t == PathType::Water && !self.caps.can_float {
            self.first_non_water_below(p, best)
        } else if t == PathType::Open {
            Some(self.first_ground_below(p))
        } else if partial_collision(t) && best.is_none() {
            Some(self.closed_node(p, t))
        } else {
            best
        }
    }

    /// `canReachWithoutCollision`: sweep the mob's box toward the node.
    fn reach_without_collision(&mut self, to: IVec3) -> bool {
        let (mut min, mut max) = self.bb;
        let size = max - min;
        let mut delta = to.as_dvec3() - self.start.pos + size / 2.0;
        let average = (size.x + size.y + size.z) / 3.0;
        let steps = (delta.length() / average).ceil();
        if steps < 1.0 {
            return true;
        }
        delta /= steps;
        for _ in 0..steps as i32 {
            min += delta;
            max += delta;
            if self.has_collisions(min, max) {
                return false;
            }
        }
        true
    }

    /// `tryJumpOn`.
    fn jump_on(&mut self, p: IVec3, jump: i32, height: f64, dir: IVec3, current: PathType) -> Option<u32> {
        let above = self.accepted(p + IVec3::Y, jump - 1, height, dir, current)?;
        if self.caps.width >= 1.0 {
            return Some(above);
        }
        let node = self.nodes[above as usize];
        if node.ty != PathType::Open && node.ty != PathType::Walkable {
            return Some(above);
        }
        let cx = (p.x - dir.x) as f64 + 0.5;
        let cz = (p.z - dir.z) as f64 + 0.5;
        let half = self.caps.width as f64 / 2.0;
        let lower = self.floor(IVec3::new(cx.floor() as i32, p.y + 1, cz.floor() as i32)) + 0.001;
        let upper = self.caps.height as f64 + self.floor(node.pos) - 0.002;
        let min = DVec3::new(cx - half, lower.min(upper), cz - half);
        let max = DVec3::new(cx + half, lower.max(upper), cz + half);
        (!self.has_collisions(min, max)).then_some(above)
    }

    fn first_non_water_below(&mut self, p: IVec3, mut best: Option<u32>) -> Option<u32> {
        let mut y = p.y - 1;
        while y > WORLD_MIN_Y {
            let q = IVec3::new(p.x, y, p.z);
            let t = self.cached_type(q);
            if t != PathType::Water {
                return best;
            }
            best = Some(self.update_cost_to_max(q, t, self.caps.malus(t)));
            y -= 1;
        }
        best
    }

    fn first_ground_below(&mut self, p: IVec3) -> u32 {
        let mut y = p.y - 1;
        while y >= WORLD_MIN_Y {
            let q = IVec3::new(p.x, y, p.z);
            if p.y - y > self.caps.max_fall {
                return self.blocked_node(q);
            }
            let t = self.cached_type(q);
            let cost = self.caps.malus(t);
            if t != PathType::Open {
                return if cost >= 0.0 { self.update_cost_to_max(q, t, cost) } else { self.blocked_node(q) };
            }
            y -= 1;
        }
        self.blocked_node(p)
    }

    // -- binary heap (`BinaryHeap`, same comparisons as Java) ---------------

    fn heap_insert(&mut self, i: u32) {
        let at = self.heap.len();
        self.heap.push(i);
        self.nodes[i as usize].heap_idx = at as i32;
        self.up_heap(at);
    }

    fn heap_pop(&mut self) -> u32 {
        let top = self.heap[0];
        let last = self.heap.pop().unwrap();
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.nodes[last as usize].heap_idx = 0;
            self.down_heap(0);
        }
        self.nodes[top as usize].heap_idx = -1;
        top
    }

    fn change_cost(&mut self, i: u32, cost: f32) {
        let old = self.nodes[i as usize].f;
        self.nodes[i as usize].f = cost;
        let at = self.nodes[i as usize].heap_idx as usize;
        if cost < old {
            self.up_heap(at);
        } else {
            self.down_heap(at);
        }
    }

    fn up_heap(&mut self, mut at: usize) {
        let node = self.heap[at];
        let cost = self.nodes[node as usize].f;
        while at > 0 {
            let parent = (at - 1) >> 1;
            let p = self.heap[parent];
            // Java's `!(cost < parent.f)`; costs are never NaN.
            if cost >= self.nodes[p as usize].f {
                break;
            }
            self.heap[at] = p;
            self.nodes[p as usize].heap_idx = at as i32;
            at = parent;
        }
        self.heap[at] = node;
        self.nodes[node as usize].heap_idx = at as i32;
    }

    fn down_heap(&mut self, mut at: usize) {
        let node = self.heap[at];
        let cost = self.nodes[node as usize].f;
        loop {
            let left = 1 + (at << 1);
            let right = left + 1;
            if left >= self.heap.len() {
                break;
            }
            let l = self.heap[left];
            let lc = self.nodes[l as usize].f;
            let (r, rc) = if right >= self.heap.len() {
                (NONE, f32::INFINITY)
            } else {
                let r = self.heap[right];
                (r, self.nodes[r as usize].f)
            };
            let (child, child_cost, child_at) = if lc < rc { (l, lc, left) } else { (r, rc, right) };
            if child_cost >= cost {
                break;
            }
            self.heap[at] = child;
            self.nodes[child as usize].heap_idx = at as i32;
            at = child_at;
        }
        self.heap[at] = node;
        self.nodes[node as usize].heap_idx = at as i32;
    }
}

fn clockwise(step: IVec3) -> (IVec3, usize) {
    // north -> east -> south -> west -> north
    let next = IVec3::new(-step.z, 0, step.x);
    let slot = HORIZONTAL.iter().find(|(d, _)| *d == next).unwrap().1;
    (next, slot)
}

fn partial_collision(t: PathType) -> bool {
    matches!(t, PathType::Fence | PathType::DoorWoodClosed | PathType::DoorIronClosed)
}

/// One step of a path: the cell a mob's feet pass through and its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathNode {
    pub pos: IVec3,
    pub ty: PathType,
}

/// Java's `Path`: nodes from the start, the next one to walk to, the target
/// it was searched for and whether it reaches it.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub nodes: Vec<PathNode>,
    pub next: usize,
    pub target: IVec3,
    pub reached: bool,
}

impl Path {
    pub fn is_done(&self) -> bool {
        self.next >= self.nodes.len()
    }
    pub fn end(&self) -> Option<IVec3> {
        self.nodes.last().map(|n| n.pos)
    }
    pub fn next_node(&self) -> Option<PathNode> {
        self.nodes.get(self.next).copied()
    }
    pub fn previous_node(&self) -> Option<PathNode> {
        self.next.checked_sub(1).and_then(|i| self.nodes.get(i).copied())
    }
    /// Distance (Manhattan) from the last node to the target.
    pub fn dist_to_target(&self) -> f32 {
        self.end().map_or(f32::MAX, |e| manhattan(e, self.target))
    }
    /// `getEntityPosAtNode`: where a mob of `width` aims within a node.
    pub fn entity_pos(&self, i: usize, width: f32) -> DVec3 {
        let n = self.nodes[i].pos;
        let half = ((width + 1.0) as i32) as f64 * 0.5;
        DVec3::new(n.x as f64 + half, n.y as f64, n.z as f64 + half)
    }
}

/// `PathFinder.findPath` from `start` to any of `targets`: a path that reaches
/// one within `reach` (Manhattan) or, failing that, the one ending closest.
pub fn find_path<W: BlockSource + ?Sized>(
    world: &W,
    caps: &Caps,
    start: Start,
    targets: &[IVec3],
    reach: i32,
) -> Option<Path> {
    if targets.is_empty() {
        return None;
    }
    let mut s = Search::new(world, caps, start);
    let from = s.start();
    // Each target remembers the node with its best heuristic.
    let mut best: Vec<(f32, u32, bool)> = vec![(f32::MAX, NONE, false); targets.len()];
    let best_h = |s: &Search<W>, n: u32, best: &mut Vec<(f32, u32, bool)>| {
        let p = s.nodes[n as usize].pos;
        let mut h = f32::MAX;
        for (i, &t) in targets.iter().enumerate() {
            let d = dist(p, t);
            if d < best[i].0 {
                best[i] = (d, n, best[i].2);
            }
            h = h.min(d);
        }
        h
    };
    let from_pos = s.nodes[from as usize].pos;
    {
        let h = best_h(&s, from, &mut best);
        let n = &mut s.nodes[from as usize];
        n.g = 0.0;
        n.h = h;
        n.f = h;
    }
    s.heap_insert(from);
    let budget = caps.max_visited as i64;
    let mut count = 0i64;
    let mut reached_any = false;
    let mut neighbours = Vec::with_capacity(32);
    while !s.heap.is_empty() {
        count += 1;
        if count >= budget {
            break;
        }
        let current = s.heap_pop();
        s.nodes[current as usize].closed = true;
        let cpos = s.nodes[current as usize].pos;
        for (i, &t) in targets.iter().enumerate() {
            if manhattan(cpos, t) <= reach as f32 {
                best[i].2 = true;
                reached_any = true;
            }
        }
        if reached_any {
            break;
        }
        if dist(cpos, from_pos) >= caps.max_path_length {
            continue;
        }
        s.neighbours(current, &mut neighbours);
        for &n in &neighbours {
            let d = dist(cpos, s.nodes[n as usize].pos);
            let walked = s.nodes[current as usize].walked + d;
            s.nodes[n as usize].walked = walked;
            let g = s.nodes[current as usize].g + d + s.nodes[n as usize].cost;
            let in_open = s.nodes[n as usize].heap_idx >= 0;
            if walked < caps.max_path_length && (!in_open || g < s.nodes[n as usize].g) {
                let h = best_h(&s, n, &mut best) * 1.5;
                let node = &mut s.nodes[n as usize];
                node.came_from = current;
                node.g = g;
                node.h = h;
                if in_open {
                    s.change_cost(n, g + h);
                } else {
                    s.nodes[n as usize].f = g + h;
                    s.heap_insert(n);
                }
            }
        }
    }
    let rebuild = |end: u32, target: IVec3, reached: bool| {
        let mut nodes = Vec::new();
        let mut at = end;
        while at != NONE {
            let n = &s.nodes[at as usize];
            nodes.push(PathNode { pos: n.pos, ty: n.ty });
            at = n.came_from;
        }
        nodes.reverse();
        Path { nodes, next: 0, target, reached }
    };
    let candidates = targets.iter().zip(&best).filter(|(_, b)| b.1 != NONE);
    if reached_any {
        candidates.filter(|(_, b)| b.2).map(|(&t, b)| rebuild(b.1, t, true)).min_by_key(|p| p.nodes.len())
    } else {
        candidates
            .map(|(&t, b)| rebuild(b.1, t, false))
            .min_by(|a, b| a.dist_to_target().total_cmp(&b.dist_to_target()).then(a.nodes.len().cmp(&b.nodes.len())))
    }
}

/// `GroundPathNavigation.findSurfacePosition`: aim at the surface of the
/// target's column rather than into the air or the ground.
pub fn surface_target<W: BlockSource + ?Sized>(world: &W, mut p: IVec3) -> IVec3 {
    const MAX_Y: i32 = 319;
    let air = |p: IVec3| world.block(p).is_some_and(|b| b == Block::AIR);
    let solid = |p: IVec3| world.block(p).is_none_or(|b| b.is_solid());
    if air(p) {
        let mut q = p - IVec3::Y;
        while q.y >= WORLD_MIN_Y && air(q) {
            q -= IVec3::Y;
        }
        if q.y >= WORLD_MIN_Y {
            return q + IVec3::Y;
        }
        q.y = p.y + 1;
        while q.y <= MAX_Y && air(q) {
            q += IVec3::Y;
        }
        p = q;
    }
    if !solid(p) {
        return p;
    }
    let mut q = p + IVec3::Y;
    while q.y <= MAX_Y && solid(q) {
        q += IVec3::Y;
    }
    q
}

// ---------------------------------------------------------------------------
// Following a path (`PathNavigation` + `GroundPathNavigation`).

/// The mob state navigation reads every tick.
#[derive(Clone, Copy, Debug)]
pub struct Mover {
    pub pos: DVec3,
    pub on_ground: bool,
    pub in_water: bool,
    /// In water and able to float there (`isInFloatableFluid() && canFloat()`).
    pub floating: bool,
    pub riding: bool,
    /// Ground speed in blocks per second (VoxelCraft's speed unit).
    pub speed: f64,
}

impl Mover {
    fn start(&self) -> Start {
        Start { pos: self.pos, on_ground: self.on_ground, in_water: self.in_water }
    }
    /// `GroundPathNavigation.canUpdatePath`.
    fn can_update(&self) -> bool {
        self.on_ground || self.in_water || self.riding
    }
}

/// Java's `mob.getSpeed()` for a ground speed in blocks per second. Mobs
/// accelerate by speed² per tick and keep 0.546 of their velocity, so the
/// steady ground speed is speed² / 0.454 blocks per tick.
pub fn java_speed(blocks_per_second: f64) -> f64 {
    (blocks_per_second.max(0.0) / 20.0 * (1.0 - 0.6 * 0.91)).sqrt()
}

/// A mob's navigation state. Java keeps none of this in saves; a reloaded
/// mob plans again.
#[derive(Clone, Debug, Default)]
pub struct Navigation {
    pub path: Option<Path>,
    target: Option<IVec3>,
    reach: i32,
    tick: i64,
    last_stuck_check: i64,
    last_stuck_pos: DVec3,
    timeout_node: IVec3,
    timeout_timer: i64,
    last_timeout_check: i64,
    timeout_limit: f64,
    pub stuck: bool,
    /// Ground speed (blocks per second) the current path is walked at.
    pub speed: f64,
    /// Skeletons by day: cut the path where it leaves shade (`RestrictSunGoal`).
    pub avoid_sun: bool,
}

impl Navigation {
    pub fn is_done(&self) -> bool {
        self.path.as_ref().is_none_or(Path::is_done)
    }

    pub fn stop(&mut self) {
        self.path = None;
    }

    pub fn target(&self) -> Option<IVec3> {
        self.target
    }

    fn reset_timeout(&mut self) {
        self.timeout_node = IVec3::ZERO;
        self.timeout_timer = 0;
        self.timeout_limit = 0.0;
        self.stuck = false;
    }

    /// `GroundPathNavigation.createPath(BlockPos, reach)`: keeps the current
    /// path while it still heads for the same target cell.
    pub fn create_path<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        caps: &Caps,
        mover: &Mover,
        target: IVec3,
        reach: i32,
    ) -> Option<Path> {
        world.block(target)?;
        let target = surface_target(world, target);
        if mover.pos.y < WORLD_MIN_Y as f64 || !mover.can_update() {
            return None;
        }
        if let Some(path) = &self.path
            && !path.is_done()
            && self.target == Some(target)
        {
            return Some(path.clone());
        }
        let path = find_path(world, caps, mover.start(), &[target], reach)?;
        self.target = Some(path.target);
        self.reach = reach;
        self.reset_timeout();
        Some(path)
    }

    /// `PathNavigation.moveTo(path, speed)`.
    pub fn move_to<W: MobWorld + ?Sized>(&mut self, world: &W, path: Option<Path>, mover: &Mover) -> bool {
        let Some(path) = path else {
            self.path = None;
            return false;
        };
        if self.path.as_ref().is_none_or(|p| p.nodes != path.nodes) {
            self.path = Some(path);
        }
        if self.is_done() {
            return false;
        }
        self.trim(world, mover);
        if self.path.as_ref().is_none_or(|p| p.nodes.is_empty()) {
            return false;
        }
        self.speed = mover.speed;
        self.last_stuck_check = self.tick;
        self.last_stuck_pos = self.temp_pos(world, mover);
        true
    }

    /// Plan to `target` and start walking it, as `moveTo(x, y, z, speed)`.
    pub fn walk_to<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        caps: &Caps,
        mover: &Mover,
        target: IVec3,
        reach: i32,
    ) -> bool {
        let path = self.create_path(world, caps, mover, target, reach);
        self.move_to(world, path, mover)
    }

    /// `GroundPathNavigation.trimPath`: skeletons in shade stop where the path
    /// would lead into the open sky.
    fn trim<W: MobWorld + ?Sized>(&mut self, world: &W, mover: &Mover) {
        if !self.avoid_sun {
            return;
        }
        let Some(path) = &mut self.path else { return };
        let here = (mover.pos + DVec3::Y * 0.5).floor().as_ivec3();
        if world.exposed(here) {
            return;
        }
        if let Some(i) = path.nodes.iter().position(|n| world.exposed(n.pos)) {
            path.nodes.truncate(i);
        }
    }

    fn temp_pos<W: BlockSource + ?Sized>(&self, world: &W, mover: &Mover) -> DVec3 {
        let y = if mover.floating {
            let mut surface = mover.pos.y.floor() as i32;
            let (x, z) = (mover.pos.x.floor() as i32, mover.pos.z.floor() as i32);
            let mut steps = 0;
            loop {
                if !world.block(IVec3::new(x, surface, z)).is_some_and(Block::holds_water) {
                    break surface;
                }
                surface += 1;
                steps += 1;
                if steps > 16 {
                    break mover.pos.y.floor() as i32;
                }
            }
        } else {
            (mover.pos.y + 0.5).floor() as i32
        };
        DVec3::new(mover.pos.x, y as f64, mover.pos.z)
    }

    /// `PathNavigation.tick`: advance along the path and return where
    /// `MoveControl` should head this tick.
    pub fn tick<W: BlockSource + ?Sized>(&mut self, world: &W, caps: &Caps, mover: &Mover) -> Option<DVec3> {
        self.tick += 1;
        if self.is_done() {
            return None;
        }
        if mover.can_update() {
            self.follow(world, caps, mover);
        } else if !self.is_done() {
            let at = self.temp_pos(world, mover);
            let path = self.path.as_mut().unwrap();
            let next = path.entity_pos(path.next, caps.width);
            if at.y > next.y && !mover.on_ground && at.x.floor() == next.x.floor() && at.z.floor() == next.z.floor() {
                path.next += 1;
            }
        }
        let path = self.path.as_ref().filter(|p| !p.is_done())?;
        let mut wanted = path.entity_pos(path.next, caps.width);
        let cell = wanted.floor().as_ivec3();
        if !world.block(cell - IVec3::Y).is_some_and(|b| b == Block::AIR) {
            wanted.y = floor_level(world, cell);
        }
        Some(wanted)
    }

    /// `followThePath` and `doStuckDetection`.
    fn follow<W: BlockSource + ?Sized>(&mut self, world: &W, caps: &Caps, mover: &Mover) {
        let at = self.temp_pos(world, mover);
        let w = caps.width as f64;
        let max = if w > 0.75 { w / 2.0 } else { 0.75 - w / 2.0 };
        let Some(path) = &mut self.path else { return };
        let node = path.nodes[path.next];
        let dx = (mover.pos.x - (node.pos.x as f64 + 0.5)).abs();
        let dy = (mover.pos.y - node.pos.y as f64).abs();
        let dz = (mover.pos.z - (node.pos.z as f64 + 0.5)).abs();
        let close = dx < max && dz < max && dy < 1.0;
        if close || can_cut_corner(node.ty) && next_in_direction(path, at, caps.width) {
            path.next += 1;
        }
        self.stuck_detection(at);
    }

    fn stuck_detection(&mut self, at: DVec3) {
        let speed = java_speed(self.speed);
        if self.tick - self.last_stuck_check > 100 {
            let effective = if speed >= 1.0 { speed } else { speed * speed };
            let threshold = effective * 100.0 * 0.25;
            if at.distance_squared(self.last_stuck_pos) < threshold * threshold {
                self.stuck = true;
                self.stop();
            } else {
                self.stuck = false;
            }
            self.last_stuck_check = self.tick;
            self.last_stuck_pos = at;
        }
        if let Some(path) = &self.path
            && let Some(node) = path.next_node()
        {
            if node.pos == self.timeout_node {
                self.timeout_timer += self.tick - self.last_timeout_check;
            } else {
                self.timeout_node = node.pos;
                let to = node.pos.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                self.timeout_limit = if speed > 0.0 { at.distance(to) / speed * 20.0 } else { 0.0 };
            }
            if self.timeout_limit > 0.0 && self.timeout_timer as f64 > self.timeout_limit * 3.0 {
                self.reset_timeout();
                self.stop();
            }
            self.last_timeout_check = self.tick;
        }
    }
}

fn can_cut_corner(t: PathType) -> bool {
    !matches!(t, PathType::FireInNeighbor | PathType::DamagingInNeighbor | PathType::WalkableDoor)
}

/// `shouldTargetNextNodeInDirection` (ground navigation never moves directly).
fn next_in_direction(path: &Path, at: DVec3, _width: f32) -> bool {
    if path.next + 1 >= path.nodes.len() {
        return false;
    }
    let centre = |p: IVec3| p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let current = centre(path.nodes[path.next].pos);
    if at.distance_squared(current) >= 4.0 {
        return false;
    }
    let next = centre(path.nodes[path.next + 1].pos);
    let to_current = current - at;
    let to_next = next - at;
    let closer = to_next.length_squared() < to_current.length_squared();
    let within = to_current.length_squared() < 0.5;
    if !closer && !within {
        return false;
    }
    to_next.normalize_or_zero().dot(to_current.normalize_or_zero()) < 0.0
}

/// `MoveControl.tick` for `MOVE_TO`: turn the body up to 90 degrees toward the
/// wanted position, and jump when it is a step above or the mob stands inside
/// a block's collision (not a door or fence). Returns the new yaw (radians,
/// VoxelCraft's convention) and whether to jump.
pub fn steer<W: BlockSource + ?Sized>(
    world: &W,
    pos: DVec3,
    yaw: f32,
    wanted: DVec3,
    step: f32,
    width: f32,
) -> Option<(f32, bool)> {
    let d = wanted - pos;
    if d.length_squared() < 2.500_000_3e-7 {
        return None;
    }
    let target = (d.z as f32).atan2(d.x as f32);
    let diff = (target - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let yaw = yaw + diff.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
    let cell = pos.floor().as_ivec3();
    let inside = world.block(cell).is_some_and(|b| {
        !b.is_door()
            && !matches!(b.shaped(), Some(Shaped::Fence))
            && physics::collision_top(world, cell).is_some_and(|top| pos.y < cell.y as f64 + top)
    });
    let jump = d.y > step as f64 && d.x * d.x + d.z * d.z < (width as f64).max(1.0) || inside;
    Some((yaw, jump))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;

    struct World(Grid);
    impl BlockSource for World {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for World {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(63)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
    }

    fn flat() -> World {
        World(Grid::flat(64))
    }

    fn zombie() -> Caps {
        Caps::new(0.6, 1.95).follow_range(35.0)
    }

    fn at(x: i32, z: i32) -> Start {
        Start { pos: DVec3::new(x as f64 + 0.5, 64.0, z as f64 + 0.5), on_ground: true, in_water: false }
    }

    fn cells(p: &Path) -> Vec<IVec3> {
        p.nodes.iter().map(|n| n.pos).collect()
    }

    #[test]
    fn walks_a_straight_line_on_open_ground() {
        let w = flat();
        let p = find_path(&w, &zombie(), at(0, 0), &[IVec3::new(6, 64, 0)], 0).unwrap();
        assert!(p.reached);
        assert_eq!(p.nodes.first().unwrap().pos, IVec3::new(0, 64, 0));
        assert_eq!(p.end(), Some(IVec3::new(6, 64, 0)));
        assert_eq!(p.nodes.len(), 7, "{:?}", cells(&p));
        assert!(p.nodes.iter().all(|n| n.ty == PathType::Walkable));
    }

    #[test]
    fn goes_around_a_wall_it_cannot_jump() {
        let mut w = flat();
        for z in -3..=3 {
            for y in 64..=65 {
                w.0.set(IVec3::new(3, y, z), Block::STONE);
            }
        }
        let p = find_path(&w, &zombie(), at(0, 0), &[IVec3::new(6, 64, 0)], 0).unwrap();
        assert!(p.reached);
        assert!(p.nodes.iter().all(|n| n.pos.x != 3 || n.pos.z.abs() > 3), "{:?}", cells(&p));
        assert!(p.nodes.iter().all(|n| n.pos.y == 64));
    }

    #[test]
    fn jumps_one_block_but_not_two() {
        let mut w = flat();
        w.0.set(IVec3::new(2, 64, 0), Block::STONE);
        let p = find_path(&w, &zombie(), at(0, 0), &[IVec3::new(2, 65, 0)], 0).unwrap();
        assert!(p.reached, "{:?}", cells(&p));
        assert_eq!(p.end(), Some(IVec3::new(2, 65, 0)));
        // A two-high pillar can't be climbed from the side.
        w.0.set(IVec3::new(2, 65, 0), Block::STONE);
        for (x, z) in [(1, 0), (3, 0), (2, 1), (2, -1)] {
            w.0.set(IVec3::new(x, 64, z), Block::AIR);
        }
        let p = find_path(&w, &zombie(), at(0, 0), &[IVec3::new(2, 66, 0)], 0).unwrap();
        assert!(!p.reached);
    }

    #[test]
    fn closed_wooden_doors_open_only_for_mobs_that_can_open_them() {
        let mut w = flat();
        for z in -12..=12 {
            for y in 64..=66 {
                w.0.set(IVec3::new(3, y, z), Block::STONE);
            }
        }
        let door = |upper| Block::door(crate::world::block::Facing::East, false, upper);
        w.0.set(IVec3::new(3, 64, 0), door(false));
        w.0.set(IVec3::new(3, 65, 0), door(true));
        let target = [IVec3::new(6, 64, 0)];
        let zombie = find_path(&w, &zombie(), at(0, 0), &target, 0).unwrap();
        assert!(!zombie.reached, "{:?}", cells(&zombie));
        let mut villager = Caps::new(0.6, 1.95);
        villager.can_open_doors = true;
        let p = find_path(&w, &villager, at(0, 0), &target, 0).unwrap();
        assert!(p.reached, "{:?}", cells(&p));
        assert!(p.nodes.iter().any(|n| n.pos == IVec3::new(3, 64, 0) && n.ty == PathType::WalkableDoor));
    }

    #[test]
    fn avoids_lava_and_pays_to_stay_clear_of_it() {
        let mut w = flat();
        // A lava strip with a gap at z = 4.
        for z in -6..=6 {
            if z != 4 {
                w.0.set(IVec3::new(3, 63, z), Block::LAVA);
            }
        }
        let p = find_path(&w, &zombie(), at(0, 0), &[IVec3::new(6, 64, 0)], 0).unwrap();
        assert!(p.reached);
        // Cells beside lava cost 8 (`FIRE_IN_NEIGHBOR`), so walking round the
        // end of the strip beats squeezing through the gap.
        assert!(p.nodes.iter().all(|n| n.pos.x != 3 || n.pos.z.abs() > 6), "{:?}", cells(&p));
        assert!(p.nodes.iter().all(|n| n.ty != PathType::Lava && n.ty != PathType::FireInNeighbor));
    }

    #[test]
    fn drops_at_most_three_blocks_when_idle() {
        // A ledge: ground at 63 for x <= 2, then a pit down to `floor`.
        let pit = |depth: i32| {
            let mut w = flat();
            for x in 3..=8 {
                for z in -8..=8 {
                    for y in (63 - depth + 1)..=63 {
                        w.0.set(IVec3::new(x, y, z), Block::AIR);
                    }
                }
            }
            w
        };
        let shallow = pit(3);
        let p = find_path(&shallow, &zombie(), at(0, 0), &[IVec3::new(5, 61, 0)], 0).unwrap();
        assert!(p.reached, "{:?}", cells(&p));
        let deep = pit(4);
        let p = find_path(&deep, &zombie(), at(0, 0), &[IVec3::new(5, 60, 0)], 0).unwrap();
        assert!(!p.reached, "{:?}", cells(&p));
    }

    #[test]
    fn fences_are_not_jumped() {
        let mut w = flat();
        for z in -20..=20 {
            w.0.set(IVec3::new(3, 64, z), Block(136));
        }
        let p = find_path(&w, &zombie(), at(0, 0), &[IVec3::new(6, 64, 0)], 0).unwrap();
        assert!(!p.reached, "{:?}", cells(&p));
        assert_eq!(state_type(Some(Block(136))), PathType::Fence);
    }

    #[test]
    fn bottom_slabs_are_half_steps() {
        let slab =
            (0..STATE_CAPACITY as u16).map(Block).find(|b| b.name() == "stone slab" && b.height() < 1.0).unwrap();
        assert_eq!(state_type(Some(slab)), PathType::Blocked);
        let mut w = flat();
        w.0.set(IVec3::new(2, 64, 0), slab);
        assert!((floor_level(&w, IVec3::new(2, 65, 0)) - 64.5).abs() < 1e-9);
        let p = find_path(&w, &zombie(), at(0, 0), &[IVec3::new(4, 64, 0)], 0).unwrap();
        assert!(p.reached);
        assert!(p.nodes.iter().any(|n| n.pos == IVec3::new(2, 65, 0)), "{:?}", cells(&p));
    }

    #[test]
    fn static_types_classify_neighbourhoods() {
        let mut w = flat();
        assert_eq!(static_type(&w, IVec3::new(0, 64, 0)), PathType::Walkable);
        assert_eq!(static_type(&w, IVec3::new(0, 66, 0)), PathType::Open);
        assert_eq!(static_type(&w, IVec3::new(0, 63, 0)), PathType::Blocked);
        w.0.set(IVec3::new(1, 63, 0), Block::WATER);
        assert_eq!(static_type(&w, IVec3::new(0, 64, 0)), PathType::WaterBorder);
        w.0.set(IVec3::new(0, 65, 1), Block::fire(0));
        assert_eq!(static_type(&w, IVec3::new(0, 64, 0)), PathType::FireInNeighbor);
        w.0.set(IVec3::new(5, 64, 5), Block::CACTUS);
        assert_eq!(static_type(&w, IVec3::new(5, 64, 6)), PathType::DamagingInNeighbor);
        assert_eq!(static_type(&w, IVec3::new(5, 65, 5)), PathType::Damaging);
    }

    #[test]
    fn navigation_follows_and_advances_nodes() {
        let w = flat();
        let caps = zombie();
        let mut nav = Navigation::default();
        let mut mover = Mover {
            pos: DVec3::new(0.5, 64.0, 0.5),
            on_ground: true,
            in_water: false,
            floating: false,
            riding: false,
            speed: 2.4,
        };
        assert!(nav.walk_to(&w, &caps, &mover, IVec3::new(4, 64, 0), 0));
        let mut ticks = 0;
        while let Some(wanted) = nav.tick(&w, &caps, &mover) {
            let d = (wanted - mover.pos) * DVec3::new(1.0, 0.0, 1.0);
            mover.pos += d.normalize_or_zero() * (mover.speed / 20.0).min(d.length());
            ticks += 1;
            assert!(ticks < 200);
        }
        assert!(mover.pos.distance(DVec3::new(4.5, 64.0, 0.5)) < 0.8, "{:?}", mover.pos);
    }

    #[test]
    fn stuck_mobs_give_up() {
        let w = flat();
        let caps = zombie();
        let mut nav = Navigation::default();
        let mover = Mover {
            pos: DVec3::new(0.5, 64.0, 0.5),
            on_ground: true,
            in_water: false,
            floating: false,
            riding: false,
            speed: 2.4,
        };
        assert!(nav.walk_to(&w, &caps, &mover, IVec3::new(10, 64, 0), 0));
        let mut ticks = 0;
        while nav.tick(&w, &caps, &mover).is_some() {
            ticks += 1;
            assert!(ticks < 2000);
        }
        assert!(ticks > 50, "{ticks}");
    }

    #[test]
    fn steering_turns_at_most_ninety_degrees_and_jumps_ledges() {
        let w = flat();
        let (yaw, jump) = steer(&w, DVec3::new(0.5, 64.0, 0.5), 0.0, DVec3::new(-5.0, 64.0, 0.5), 0.6, 0.6).unwrap();
        assert!((yaw.abs() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!(!jump);
        let (_, jump) = steer(&w, DVec3::new(0.5, 64.0, 0.5), 0.0, DVec3::new(1.2, 65.0, 0.5), 0.6, 0.6).unwrap();
        assert!(jump);
    }

    /// Search cost on a real `World`: an open 20-block chase and a search
    /// that exhausts a zombie's 560-node budget against an enclosed target.
    #[test]
    #[ignore]
    fn path_search_cost() {
        let mut world = crate::world::lighting::flat_test_world(
            (-2..=2).flat_map(|x| (-2..=2).map(move |z| glam::IVec2::new(x, z))),
            63,
        );
        for x in 20..=24 {
            for z in -2i32..=2 {
                for y in 64..=67 {
                    if x == 20 || x == 24 || z.abs() == 2 || y == 67 {
                        world.set_block(IVec3::new(x, y, z), Block::STONE);
                    }
                }
            }
        }
        let caps = zombie();
        let start = Start { pos: DVec3::new(0.5, 64.0, 0.5), on_ground: true, in_water: false };
        let time = |target: IVec3| {
            let mut runs: Vec<f64> = (0..15)
                .map(|_| {
                    let t = std::time::Instant::now();
                    for _ in 0..20 {
                        std::hint::black_box(find_path(&world, &caps, start, &[target], 0));
                    }
                    t.elapsed().as_secs_f64() * 1e6 / 20.0
                })
                .collect();
            runs.sort_by(f64::total_cmp);
            runs[runs.len() / 2]
        };
        println!("open 20-block chase: {:.0} us/search", time(IVec3::new(20, 64, 8)));
        println!("enclosed target (budget exhausted): {:.0} us/search", time(IVec3::new(22, 64, 0)));
    }

    #[test]
    fn surface_targets_drop_to_the_ground_and_climb_out_of_it() {
        let w = flat();
        assert_eq!(surface_target(&w, IVec3::new(3, 80, 3)), IVec3::new(3, 64, 3));
        assert_eq!(surface_target(&w, IVec3::new(3, 50, 3)), IVec3::new(3, 64, 3));
    }
}
