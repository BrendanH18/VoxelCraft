//! Strongholds, laid out like Java's `StrongholdPieces`.
//!
//! Java places 128 strongholds on concentric rings around the origin: three
//! on the first ring 1280-2816 blocks out, more on each ring further out.
//! Each grows from a spiral staircase through corridors, turns, stairways,
//! crossings, prison cells, libraries and chest corridors to exactly one
//! portal room, whose twelve End portal frames each hold an eye one time in
//! ten. Pieces follow Java's weights and limits within 112 blocks and 50
//! steps of the start, and the finished stronghold is sunk below sea level.
//!
//! Like fortresses, a layout is a pure function of the seed, cached, and
//! painted per chunk. Walls are randomly cracked or mossy stone bricks; the
//! inside is carved out of solid ground, and caves crossing a stronghold
//! leave holes in it as in Java.

use std::sync::{Arc, Mutex, OnceLock};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::{Block, Facing};
use super::chest::Chest;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME};
use super::fortress::Feature;
use super::noise::hash3;
use super::structure::{Bounds, LootEntry, Oriented, Paint, Rng, fill_chest};
use crate::entity::MobKind;
use crate::item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind};

/// Java's `concentric_rings` placement: 128 strongholds, 32 chunks apart
/// per ring step, three on the first ring.
const COUNT: usize = 128;
const RING_DISTANCE: f64 = 32.0;
const FIRST_RING: u32 = 3;
/// New pieces must start within this many blocks of the start piece.
const REACH: i32 = 112;
/// No stronghold block lies farther than this from its start piece.
const EXTENT: i32 = REACH + 20;
const MAX_DEPTH: u32 = 50;
const SALT: u64 = 0x5354_524F_4E47;
/// Java's sea level and the depth the top of a stronghold keeps below it.
const SEA_LEVEL: i32 = 63;
const BELOW_SEA: i32 = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    StairsDown,
    Straight,
    PrisonHall,
    LeftTurn,
    RightTurn,
    RoomCrossing,
    StraightStairsDown,
    FiveCrossing,
    ChestCorridor,
    Library,
    PortalRoom,
}

impl Kind {
    /// Box size and offset from the doorway, as in Java's `createPiece`s.
    /// Libraries are tall when they fit (see `Builder::create`).
    fn dims(self, tall: bool) -> (IVec3, IVec3) {
        let v = IVec3::new;
        match self {
            Kind::StairsDown => (v(5, 11, 5), v(-1, -7, 0)),
            Kind::Straight | Kind::ChestCorridor => (v(5, 5, 7), v(-1, -1, 0)),
            Kind::PrisonHall => (v(9, 5, 11), v(-1, -1, 0)),
            Kind::LeftTurn | Kind::RightTurn => (v(5, 5, 5), v(-1, -1, 0)),
            Kind::RoomCrossing => (v(11, 7, 11), v(-4, -1, 0)),
            Kind::StraightStairsDown => (v(5, 11, 8), v(-1, -7, 0)),
            Kind::FiveCrossing => (v(10, 9, 11), v(-4, -3, 0)),
            Kind::Library => (v(14, if tall { 11 } else { 6 }, 15), v(-4, -1, 0)),
            Kind::PortalRoom => (v(11, 8, 16), v(-4, -1, 0)),
        }
    }
}

/// What fills a piece's entry doorway (Java's `SmallDoorType`; iron doors
/// are wooden until iron doors exist).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Door {
    Opening,
    Wood,
    Grates,
}

#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub kind: Kind,
    pub facing: Facing,
    pub bounds: Bounds,
    depth: u32,
    door: Door,
    /// Per-kind choices: corridor side exits, crossing exits, room type,
    /// a tall library.
    flags: u8,
    /// Seeds wall stones, cobwebs, eyes and loot.
    variant: u64,
}

impl Oriented for Piece {
    fn facing(&self) -> Facing {
        self.facing
    }

    fn bounds(&self) -> &Bounds {
        &self.bounds
    }
}

struct Weight {
    kind: Kind,
    weight: u32,
    /// 0 for no limit.
    max: u32,
    placed: u32,
}

impl Weight {
    const fn new(kind: Kind, weight: u32, max: u32) -> Self {
        Self { kind, weight, max, placed: 0 }
    }

    /// Java's `doPlace`: libraries only past depth 4, the portal room past 5.
    fn can_place(&self, depth: u32) -> bool {
        let deep = match self.kind {
            Kind::Library => depth > 4,
            Kind::PortalRoom => depth > 5,
            _ => true,
        };
        deep && (self.max == 0 || self.placed < self.max)
    }

    fn exhausted(&self) -> bool {
        self.max > 0 && self.placed >= self.max
    }
}

/// Java's `STRONGHOLD_PIECE_WEIGHTS`.
fn weights() -> Vec<Weight> {
    vec![
        Weight::new(Kind::Straight, 40, 0),
        Weight::new(Kind::PrisonHall, 5, 5),
        Weight::new(Kind::LeftTurn, 20, 0),
        Weight::new(Kind::RightTurn, 20, 0),
        Weight::new(Kind::RoomCrossing, 10, 6),
        Weight::new(Kind::StraightStairsDown, 5, 5),
        Weight::new(Kind::StairsDown, 5, 5),
        Weight::new(Kind::FiveCrossing, 5, 4),
        Weight::new(Kind::ChestCorridor, 5, 4),
        Weight::new(Kind::Library, 10, 2),
        Weight::new(Kind::PortalRoom, 20, 1),
    ]
}

struct Builder {
    rng: Rng,
    pieces: Vec<Piece>,
    pending: Vec<usize>,
    weights: Vec<Weight>,
    previous: Option<Kind>,
    /// The start's first child is always a five-way crossing (Java).
    imposed: Option<Kind>,
}

impl Builder {
    fn collides(&self, b: &Bounds) -> bool {
        self.pieces.iter().any(|p| p.bounds.intersects(b))
    }

    fn fits(&self, b: &Bounds) -> bool {
        b.min.y > 10 && !self.collides(b)
    }

    fn create(&mut self, kind: Kind, door: IVec3, facing: Facing, depth: u32) -> Option<Piece> {
        let mut tall = true;
        let (size, off) = kind.dims(true);
        let mut bounds = Bounds::oriented(door, off, size, facing);
        if kind == Kind::Library && !self.fits(&bounds) {
            tall = false;
            let (size, off) = kind.dims(false);
            bounds = Bounds::oriented(door, off, size, facing);
        }
        if !self.fits(&bounds) {
            return None;
        }
        let door = match self.rng.below(5) {
            0 | 1 => Door::Opening,
            2 | 4 => Door::Wood,
            _ => Door::Grates,
        };
        let flags = match kind {
            Kind::Straight => self.rng.below(2) as u8 | (self.rng.below(2) as u8) << 1,
            Kind::FiveCrossing => {
                let bit = |b: bool, i: u8| (b as u8) << i;
                bit(self.rng.below(2) == 0, 0)
                    | bit(self.rng.below(2) == 0, 1)
                    | bit(self.rng.below(2) == 0, 2)
                    | bit(self.rng.below(3) > 0, 3)
            }
            Kind::RoomCrossing => self.rng.below(5) as u8,
            Kind::Library => tall as u8,
            _ => 0,
        };
        let variant = self.rng.next_u64();
        Some(Piece { kind, facing, bounds, depth, door, flags, variant })
    }

    /// Java's `generatePieceFromSmallDoor`.
    fn pick(&mut self, door: IVec3, facing: Facing, depth: u32) -> Option<Piece> {
        if !self.weights.iter().any(|w| w.max > 0 && w.placed < w.max) {
            return None;
        }
        if let Some(kind) = self.imposed.take()
            && let Some(piece) = self.create(kind, door, facing, depth)
        {
            return Some(piece);
        }
        let total: u32 = self.weights.iter().map(|w| w.weight).sum();
        for _ in 0..5 {
            let mut r = self.rng.below(total) as i32;
            for i in 0..self.weights.len() {
                let w = &self.weights[i];
                r -= w.weight as i32;
                if r >= 0 {
                    continue;
                }
                if !w.can_place(depth) || self.previous == Some(w.kind) {
                    break;
                }
                let kind = w.kind;
                if let Some(piece) = self.create(kind, door, facing, depth) {
                    self.weights[i].placed += 1;
                    self.previous = Some(kind);
                    if self.weights[i].exhausted() {
                        self.weights.remove(i);
                    }
                    return Some(piece);
                }
            }
        }
        None
    }

    /// Java's `generateAndAddPiece`.
    fn grow(&mut self, (door, facing): (IVec3, Facing), depth: u32) {
        let start = self.pieces[0].bounds.min;
        if depth > MAX_DEPTH || (door.x - start.x).abs() > REACH || (door.z - start.z).abs() > REACH {
            return;
        }
        if let Some(piece) = self.pick(door, facing, depth + 1) {
            self.pieces.push(piece);
            self.pending.push(self.pieces.len() - 1);
        }
    }

    fn add_children(&mut self, i: usize) {
        let p = self.pieces[i];
        let d = p.depth;
        let flag = |bit: u8| p.flags >> bit & 1 == 1;
        match p.kind {
            Kind::StairsDown => {
                if i == 0 {
                    self.imposed = Some(Kind::FiveCrossing);
                }
                self.grow(p.ahead(1, 1), d);
            }
            Kind::Straight => {
                self.grow(p.ahead(1, 1), d);
                if flag(0) {
                    self.grow(p.side(true, 1, 2), d);
                }
                if flag(1) {
                    self.grow(p.side(false, 1, 2), d);
                }
            }
            Kind::ChestCorridor | Kind::StraightStairsDown | Kind::PrisonHall => self.grow(p.ahead(1, 1), d),
            Kind::LeftTurn => self.grow(p.side(true, 1, 1), d),
            Kind::RightTurn => self.grow(p.side(false, 1, 1), d),
            Kind::RoomCrossing => {
                self.grow(p.ahead(4, 1), d);
                self.grow(p.side(true, 1, 4), d);
                self.grow(p.side(false, 1, 4), d);
            }
            Kind::FiveCrossing => {
                self.grow(p.ahead(5, 1), d);
                if flag(0) {
                    self.grow(p.side(true, 3, 1), d);
                }
                if flag(1) {
                    self.grow(p.side(true, 5, 7), d);
                }
                if flag(2) {
                    self.grow(p.side(false, 3, 1), d);
                }
                if flag(3) {
                    self.grow(p.side(false, 5, 7), d);
                }
            }
            Kind::Library | Kind::PortalRoom => {}
        }
    }
}

/// One stronghold's pieces.
pub struct Stronghold {
    pub pieces: Vec<Piece>,
    pub bounds: Bounds,
}

impl Stronghold {
    /// Lays out the stronghold whose start corner is at `x, z`. Like Java,
    /// tries again until one has a portal room.
    fn generate(mut rng: Rng, x: i32, z: i32) -> Stronghold {
        loop {
            let facing = Facing::ALL[rng.below(4) as usize];
            let min = IVec3::new(x, 64, z);
            let start = Bounds { min, max: min + IVec3::new(4, 10, 4) };
            let door = match rng.below(5) {
                0 | 1 => Door::Opening,
                2 | 4 => Door::Wood,
                _ => Door::Grates,
            };
            let first = Piece {
                kind: Kind::StairsDown,
                facing,
                bounds: start,
                depth: 0,
                door,
                flags: 0,
                variant: rng.next_u64(),
            };
            let mut b = Builder {
                rng,
                pieces: vec![first],
                pending: Vec::new(),
                weights: weights(),
                previous: None,
                imposed: None,
            };
            b.add_children(0);
            while !b.pending.is_empty() {
                let i = b.rng.below(b.pending.len() as u32) as usize;
                let piece = b.pending.swap_remove(i);
                b.add_children(piece);
            }
            rng = b.rng;
            if !b.pieces.iter().any(|p| p.kind == Kind::PortalRoom) {
                continue;
            }
            // Java's `moveBelowSeaLevel`.
            let mut bounds = b.pieces.iter().fold(start, |acc, p| acc.union(&p.bounds));
            let ceiling = SEA_LEVEL - BELOW_SEA;
            let mut top = bounds.max.y - bounds.min.y + 1 + 1;
            if top < ceiling {
                top += rng.below((ceiling - top) as u32) as i32;
            }
            let shift = IVec3::new(0, top - bounds.max.y, 0);
            for p in &mut b.pieces {
                p.bounds = p.bounds.shifted(shift);
            }
            bounds = bounds.shifted(shift);
            return Stronghold { pieces: b.pieces, bounds };
        }
    }

    /// The portal room (every stronghold has one).
    pub fn portal_room(&self) -> &Piece {
        self.pieces.iter().find(|p| p.kind == Kind::PortalRoom).expect("strongholds have a portal room")
    }
}

/// Java's concentric ring placement: the start corner (block 2, 2 of a
/// chunk) of every stronghold.
fn ring_positions(seed: u64) -> Vec<IVec2> {
    let mut rng = Rng(seed ^ SALT);
    let mut angle = rng.unit() * std::f64::consts::TAU;
    let (mut ring, mut in_ring, mut spread) = (0u32, 0u32, FIRST_RING);
    let mut out = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        let dist = 4.0 * RING_DISTANCE + RING_DISTANCE * ring as f64 * 6.0 + (rng.unit() - 0.5) * RING_DISTANCE * 2.5;
        let chunk = IVec2::new((angle.cos() * dist).round() as i32, (angle.sin() * dist).round() as i32);
        out.push(chunk * 16 + IVec2::splat(2));
        angle += std::f64::consts::TAU / spread as f64;
        in_ring += 1;
        if in_ring == spread {
            ring += 1;
            in_ring = 0;
            spread += 2 * spread / (ring + 1);
            spread = spread.min((COUNT - 1 - i) as u32);
            angle += rng.unit() * std::f64::consts::TAU;
        }
    }
    out
}

/// Every stronghold of one overworld, with cached layouts.
pub struct Strongholds {
    seed: u64,
    starts: OnceLock<Vec<IVec2>>,
    cache: Mutex<FxHashMap<usize, Arc<Stronghold>>>,
}

impl Strongholds {
    pub fn new(seed: u64) -> Self {
        Self { seed, starts: OnceLock::new(), cache: Mutex::new(FxHashMap::default()) }
    }

    /// Start corners of all 128 strongholds.
    pub fn starts(&self) -> &[IVec2] {
        self.starts.get_or_init(|| ring_positions(self.seed))
    }

    fn get(&self, i: usize) -> Arc<Stronghold> {
        if let Some(s) = self.cache.lock().unwrap().get(&i) {
            return s.clone();
        }
        let start = self.starts()[i];
        let rng = Rng(hash3(start.x, 0, start.y, self.seed ^ SALT));
        let stronghold = Arc::new(Stronghold::generate(rng, start.x, start.y));
        self.cache.lock().unwrap().entry(i).or_insert(stronghold).clone()
    }

    /// Strongholds with blocks in the columns from `min` to `max`.
    pub fn near(&self, min: IVec2, max: IVec2) -> Vec<Arc<Stronghold>> {
        let mut out = Vec::new();
        for (i, &s) in self.starts().iter().enumerate() {
            if (s - EXTENT).cmpgt(max).any() || (s + EXTENT).cmplt(min).any() {
                continue;
            }
            let sh = self.get(i);
            let b = sh.bounds;
            if b.min.x <= max.x && b.max.x >= min.x && b.min.z <= max.y && b.max.z >= min.y {
                out.push(sh);
            }
        }
        out
    }

    /// Where an eye of ender leads from `p`: the nearest stronghold's
    /// start (its spiral staircase), like Java's.
    pub fn nearest(&self, p: IVec3) -> Option<IVec3> {
        let here = IVec2::new(p.x, p.z);
        let i = (0..self.starts().len()).min_by_key(|&i| (self.starts()[i] - here).as_i64vec2().length_squared())?;
        let s = self.starts()[i];
        Some(IVec3::new(s.x + 2, self.get(i).pieces[0].bounds.min.y, s.y + 2))
    }

    /// Paints the stronghold pieces touching the chunk at `base`.
    pub fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let solid = |_: i32, _: i32, _: i32, _: i32| false;
        for sh in self.near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            for piece in sh.pieces.iter().filter(|p| p.bounds.intersects(&chunk)) {
                Paint { blocks: &mut *blocks, base, top, piece, open: &solid, pillar: Block::STONE_BRICKS }.piece();
            }
        }
    }

    /// Chests and the silverfish spawner strongholds put in a chunk.
    pub fn features(&self, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let mut out = Vec::new();
        for sh in self.near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            for p in sh.pieces.iter().filter(|p| p.bounds.intersects(&chunk)) {
                let found: &[(IVec3, Feature)] = &match p.kind {
                    Kind::ChestCorridor => {
                        vec![(p.world(3, 2, 3), Feature::StrongholdChest(p.variant << 2 | CORRIDOR))]
                    }
                    Kind::RoomCrossing if p.flags == 2 => {
                        vec![(p.world(3, 4, 8), Feature::StrongholdChest(p.variant << 2 | CROSSING))]
                    }
                    Kind::Library => {
                        let mut v = vec![(p.world(5, 1, 5), Feature::StrongholdChest(p.variant << 2 | LIBRARY))];
                        if p.flags & 1 == 1 {
                            v.push((
                                p.world(11, 6, 1),
                                Feature::StrongholdChest(p.variant.rotate_left(7) << 2 | LIBRARY),
                            ));
                        }
                        v
                    }
                    Kind::PortalRoom => vec![(p.world(5, 3, 6), Feature::Spawner(MobKind::Silverfish))],
                    _ => vec![],
                };
                out.extend(found.iter().copied().filter(|(q, _)| chunk.contains(*q)));
            }
        }
        out
    }
}

/// Loot table tags mixed into a piece's seed.
const CORRIDOR: u64 = 1;
const CROSSING: u64 = 2;
const LIBRARY: u64 = 3;

/// Fills a stronghold chest; the table is picked by the seed's tag.
pub fn loot(seed: u64) -> Chest {
    let iron = |p| Item::armor(p, ArmorMaterial::Iron);
    let tool = |k| Item::tool(k, Tier::Iron);
    // Java's stronghold_corridor, stronghold_crossing and stronghold_library
    // tables without the items that don't exist (redstone, saddles, horse
    // armor, discs, maps, compasses); enchanted books are plain books.
    let corridor: [LootEntry; 13] = [
        (Item::ENDER_PEARL, 10, 1, 1),
        (Item::DIAMOND, 3, 1, 3),
        (Item::IRON_INGOT, 10, 1, 5),
        (Item::GOLD_INGOT, 5, 1, 3),
        (Item::BREAD, 15, 1, 3),
        (Item::APPLE, 15, 1, 3),
        (tool(ToolKind::Pickaxe), 5, 1, 1),
        (tool(ToolKind::Sword), 5, 1, 1),
        (iron(ArmorPiece::Chestplate), 5, 1, 1),
        (iron(ArmorPiece::Helmet), 5, 1, 1),
        (iron(ArmorPiece::Leggings), 5, 1, 1),
        (iron(ArmorPiece::Boots), 5, 1, 1),
        (Item::BOOK, 1, 1, 1),
    ];
    let crossing: [LootEntry; 6] = [
        (Item::IRON_INGOT, 10, 1, 5),
        (Item::GOLD_INGOT, 5, 1, 3),
        (Item::COAL, 10, 3, 8),
        (Item::BREAD, 15, 1, 3),
        (Item::APPLE, 15, 1, 3),
        (tool(ToolKind::Pickaxe), 1, 1, 1),
    ];
    let library: [LootEntry; 2] = [(Item::BOOK, 30, 1, 3), (Item::PAPER, 20, 2, 7)];
    match seed & 3 {
        CROSSING => fill_chest(seed, (1, 4), &crossing),
        LIBRARY => fill_chest(seed, (2, 10), &library),
        _ => fill_chest(seed, (2, 3), &corridor),
    }
}

const AIR: Block = Block::AIR;
const BRICKS: Block = Block::STONE_BRICKS;
const SLAB: Block = Block::STONE_SLAB;
const BARS: Block = Block::IRON_BARS;

impl Paint<'_, Piece> {
    /// Java's random stone selector: cracked one time in five, mossy three
    /// in ten, plain otherwise, by position.
    fn stone(&self, x: i32, y: i32, z: i32) -> Block {
        let p = self.piece.world(x, y, z);
        match hash3(p.x, p.y, p.z, self.piece.variant) % 100 {
            0..20 => Block::CRACKED_STONE_BRICKS,
            20..50 => Block::MOSSY_STONE_BRICKS,
            _ => BRICKS,
        }
    }

    /// Java's `generateBox(..., skipAir, random, SMOOTH_STONE_SELECTOR)`:
    /// random stone bricks on the shell, air inside; with `skip_air`, cells
    /// that are already air (caves) stay open.
    #[allow(clippy::too_many_arguments)]
    fn shell(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, skip_air: bool) {
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let Some(i) = self.cell(x, y, z) else { continue };
                    if skip_air && self.blocks[i] == AIR {
                        continue;
                    }
                    let edge = x == x0 || x == x1 || y == y0 || y == y1 || z == z0 || z == z1;
                    self.blocks[i] = if edge { self.stone(x, y, z) } else { AIR };
                }
            }
        }
    }

    /// Java's `generateSmallDoor`: a 3x3 doorway at `(x, y, z)`.
    fn door(&mut self, door: Door, x: i32, y: i32, z: i32) {
        match door {
            Door::Opening => self.fill(x, y, z, x + 2, y + 2, z, AIR),
            Door::Wood => {
                self.fill(x, y, z, x, y + 2, z, BRICKS);
                self.fill(x + 2, y, z, x + 2, y + 2, z, BRICKS);
                self.set(x + 1, y + 2, z, BRICKS);
                let facing = self.piece.turn(Facing::South);
                self.set(x + 1, y, z, Block::door(facing, false, false));
                self.set(x + 1, y + 1, z, Block::door(facing, false, true));
            }
            Door::Grates => {
                self.fill(x + 1, y, z, x + 1, y + 1, z, AIR);
                self.fill(x, y, z, x, y + 1, z, BARS);
                self.fill(x + 2, y, z, x + 2, y + 1, z, BARS);
                self.fill(x, y + 2, z, x + 2, y + 2, z, BARS);
            }
        }
    }

    fn piece(&mut self) {
        let p = *self.piece;
        match p.kind {
            Kind::StairsDown => self.stairs_down(),
            Kind::Straight => {
                self.shell(0, 0, 0, 4, 4, 6, true);
                self.door(p.door, 1, 1, 0);
                self.door(Door::Opening, 1, 1, 6);
                if p.flags & 1 == 1 {
                    self.fill(0, 1, 2, 0, 3, 4, AIR);
                }
                if p.flags & 2 == 2 {
                    self.fill(4, 1, 2, 4, 3, 4, AIR);
                }
            }
            Kind::ChestCorridor => {
                self.shell(0, 0, 0, 4, 4, 6, true);
                self.door(p.door, 1, 1, 0);
                self.door(Door::Opening, 1, 1, 6);
                self.fill(3, 1, 2, 3, 1, 4, BRICKS);
                for (x, y, z) in [(3, 1, 1), (3, 1, 5), (3, 2, 2), (3, 2, 4)] {
                    self.set(x, y, z, SLAB);
                }
                self.fill(2, 1, 2, 2, 1, 4, SLAB);
                self.set(3, 2, 3, self.facing(Block::CHEST, Facing::West));
            }
            Kind::StraightStairsDown => {
                self.shell(0, 0, 0, 4, 10, 7, true);
                self.door(p.door, 1, 7, 0);
                self.door(Door::Opening, 1, 1, 7);
                let steps = self.stairs(Block::COBBLESTONE, Facing::North);
                for i in 0..6 {
                    self.fill(1, 6 - i, 1 + i, 3, 6 - i, 1 + i, steps);
                    if i < 5 {
                        self.fill(1, 5 - i, 1 + i, 3, 5 - i, 1 + i, BRICKS);
                    }
                }
            }
            Kind::LeftTurn | Kind::RightTurn => {
                self.shell(0, 0, 0, 4, 4, 4, true);
                self.door(p.door, 1, 1, 0);
                let x = if p.kind == Kind::LeftTurn { 0 } else { 4 };
                self.fill(x, 1, 1, x, 3, 3, AIR);
            }
            Kind::RoomCrossing => self.room_crossing(),
            Kind::PrisonHall => self.prison_hall(),
            Kind::FiveCrossing => self.five_crossing(),
            Kind::Library => self.library(),
            Kind::PortalRoom => self.portal_room(),
        }
    }

    /// The spiral staircase the stronghold starts from (and more like it).
    fn stairs_down(&mut self) {
        self.shell(0, 0, 0, 4, 10, 4, true);
        self.door(self.piece.door, 1, 7, 0);
        self.door(Door::Opening, 1, 1, 4);
        let bricks =
            [(2, 6, 1), (1, 5, 1), (1, 5, 2), (1, 4, 3), (2, 4, 3), (3, 3, 3), (3, 3, 2), (3, 2, 1), (2, 2, 1)];
        for (x, y, z) in bricks.into_iter().chain([(1, 1, 1), (1, 1, 2)]) {
            self.set(x, y, z, BRICKS);
        }
        for (x, y, z) in [(1, 6, 1), (1, 5, 3), (3, 4, 3), (3, 3, 1), (1, 2, 1), (1, 1, 3)] {
            self.set(x, y, z, SLAB);
        }
    }

    /// A large room crossed by three exits: plain, a pillar, a fountain or
    /// a balcony with a chest (Java's four room types).
    fn room_crossing(&mut self) {
        self.shell(0, 0, 0, 10, 6, 10, true);
        self.door(self.piece.door, 4, 1, 0);
        self.fill(4, 1, 10, 6, 3, 10, AIR);
        self.fill(0, 1, 4, 0, 3, 6, AIR);
        self.fill(10, 1, 4, 10, 3, 6, AIR);
        match self.piece.flags {
            0 => {
                self.fill(4, 1, 4, 6, 6, 6, BRICKS);
                for (x, z) in [(5, 3), (5, 7), (3, 5), (7, 5)] {
                    self.set(x, 1, z, SLAB);
                }
            }
            1 => {
                for i in 0..5 {
                    self.set(3, 1, 3 + i, Block::COBBLESTONE);
                    self.set(7, 1, 3 + i, Block::COBBLESTONE);
                    self.set(3 + i, 1, 3, Block::COBBLESTONE);
                    self.set(3 + i, 1, 7, Block::COBBLESTONE);
                }
                self.fill(5, 1, 5, 5, 3, 5, BRICKS);
                self.set(5, 4, 5, Block::WATER);
            }
            2 => {
                // A cobblestone balcony round the walls, reached by a ladder.
                for i in 1..=9 {
                    self.set(1, 3, i, Block::COBBLESTONE);
                    self.set(9, 3, i, Block::COBBLESTONE);
                    self.set(i, 3, 1, Block::COBBLESTONE);
                    self.set(i, 3, 9, Block::COBBLESTONE);
                }
                self.fill(5, 3, 1, 5, 3, 1, Block::COBBLESTONE);
                let ladder = self.facing(Block::LADDER, Facing::West);
                self.fill(9, 1, 3, 9, 3, 3, ladder);
                self.set(3, 4, 8, self.facing(Block::CHEST, Facing::North));
            }
            _ => {}
        }
    }

    /// Cells behind iron bars, with wooden doors (Java's iron ones).
    fn prison_hall(&mut self) {
        self.shell(0, 0, 0, 8, 4, 10, true);
        self.door(self.piece.door, 1, 1, 0);
        self.fill(1, 1, 10, 3, 3, 10, AIR);
        for z in [1, 3, 7, 9] {
            self.fill(4, 1, z, 4, 3, z, BRICKS);
        }
        self.fill(4, 1, 4, 4, 3, 6, BARS);
        self.fill(5, 1, 5, 7, 3, 5, BARS);
        self.set(4, 3, 2, BARS);
        self.set(4, 3, 8, BARS);
        let facing = self.piece.turn(Facing::West);
        for z in [2, 8] {
            self.set(4, 1, z, Block::door(facing, false, false));
            self.set(4, 2, z, Block::door(facing, false, true));
        }
    }

    /// A tall junction with a raised walkway; exits low and high on each
    /// side by chance.
    fn five_crossing(&mut self) {
        let f = self.piece.flags;
        self.shell(0, 0, 0, 9, 8, 10, true);
        self.door(self.piece.door, 4, 3, 0);
        if f & 1 != 0 {
            self.fill(0, 3, 1, 0, 5, 3, AIR);
        }
        if f & 4 != 0 {
            self.fill(9, 3, 1, 9, 5, 3, AIR);
        }
        if f & 2 != 0 {
            self.fill(0, 5, 7, 0, 7, 9, AIR);
        }
        if f & 8 != 0 {
            self.fill(9, 5, 7, 9, 7, 9, AIR);
        }
        self.fill(5, 1, 10, 7, 3, 10, AIR);
        self.fill(1, 2, 1, 8, 2, 6, BRICKS);
        self.fill(4, 1, 5, 4, 4, 9, BRICKS);
        self.fill(8, 1, 5, 8, 4, 9, BRICKS);
        self.fill(1, 4, 7, 3, 4, 9, BRICKS);
        self.fill(1, 3, 5, 3, 3, 6, BRICKS);
        self.fill(1, 3, 4, 3, 3, 4, SLAB);
        self.fill(1, 4, 6, 3, 4, 6, SLAB);
        self.fill(5, 1, 7, 7, 1, 8, BRICKS);
        self.fill(5, 1, 9, 7, 1, 9, SLAB);
        self.fill(5, 2, 7, 7, 2, 7, SLAB);
        self.fill(4, 5, 7, 4, 5, 9, SLAB);
        self.fill(8, 5, 7, 8, 5, 9, SLAB);
        self.fill(5, 5, 7, 7, 5, 9, BRICKS);
    }

    /// Walls of bookshelves between oak pillars, rows of shelves, cobwebs,
    /// and in tall libraries a balcony with a railing and a ladder.
    fn library(&mut self) {
        let tall = self.piece.flags & 1 == 1;
        let h = if tall { 10 } else { 5 };
        self.shell(0, 0, 0, 13, h, 14, true);
        self.door(self.piece.door, 4, 1, 0);
        // Java's 7% cobwebs.
        for y in 1..=4 {
            for z in 1..=13 {
                for x in 2..=11 {
                    let w = self.piece.world(x, y, z);
                    if hash3(w.x, w.y, w.z, self.piece.variant ^ 0x77) % 100 < 7 {
                        self.set(x, y, z, Block::COBWEB);
                    }
                }
            }
        }
        let shelf_top = if tall { h - 1 } else { 4 };
        for z in 1..=13 {
            let block = if (z - 1) % 4 == 0 { Block::PLANKS } else { Block::BOOKSHELF };
            self.fill(1, 1, z, 1, shelf_top, z, block);
            self.fill(12, 1, z, 12, shelf_top, z, block);
        }
        for z in (3..12).step_by(2) {
            for x in [3, 6, 9] {
                self.fill(x, 1, z, x + 1, 3, z, Block::BOOKSHELF);
            }
        }
        if tall {
            // The balcony: planks round the walls at y = 5 with a fence rail.
            for z in 1..=13 {
                self.fill(1, 5, z, 3, 5, z, Block::PLANKS);
                self.fill(10, 5, z, 12, 5, z, Block::PLANKS);
            }
            self.fill(4, 5, 1, 9, 5, 2, Block::PLANKS);
            self.fill(4, 5, 12, 9, 5, 13, Block::PLANKS);
            self.fill(4, 6, 3, 9, 6, 3, Block::OAK_FENCE);
            self.fill(4, 6, 11, 9, 6, 11, Block::OAK_FENCE);
            self.fill(3, 6, 3, 3, 6, 11, Block::OAK_FENCE);
            self.fill(10, 6, 3, 10, 6, 11, Block::OAK_FENCE);
            self.fill(4, 1, 13, 4, 5, 13, AIR);
            let ladder = self.facing(Block::LADDER, Facing::North);
            self.fill(4, 1, 13, 4, 5, 13, ladder);
            self.set(11, 6, 1, self.facing(Block::CHEST, Facing::South));
        }
        self.set(5, 1, 5, self.facing(Block::CHEST, Facing::South));
    }

    /// Lava under a raised ring of twelve End portal frames, stairs up past
    /// a silverfish spawner, and barred windows.
    fn portal_room(&mut self) {
        self.shell(0, 0, 0, 10, 7, 15, false);
        self.door(Door::Grates, 4, 1, 0);
        self.fill(1, 6, 1, 1, 6, 14, BRICKS);
        self.fill(9, 6, 1, 9, 6, 14, BRICKS);
        self.fill(2, 6, 1, 8, 6, 2, BRICKS);
        self.fill(2, 6, 14, 8, 6, 14, BRICKS);
        self.fill(1, 1, 1, 2, 1, 4, BRICKS);
        self.fill(8, 1, 1, 9, 1, 4, BRICKS);
        self.fill(1, 1, 1, 1, 1, 3, Block::LAVA);
        self.fill(9, 1, 1, 9, 1, 3, Block::LAVA);
        self.fill(3, 1, 8, 7, 2, 12, BRICKS);
        self.fill(4, 1, 9, 6, 2, 11, Block::LAVA);
        for z in (3..=13).step_by(2) {
            self.fill(0, 3, z, 0, 4, z, BARS);
            self.fill(10, 3, z, 10, 4, z, BARS);
        }
        let up = self.stairs(BRICKS, Facing::South);
        self.fill(4, 1, 5, 6, 1, 5, up);
        self.fill(4, 1, 6, 6, 1, 7, BRICKS);
        self.fill(4, 2, 6, 6, 2, 6, up);
        self.fill(4, 2, 7, 6, 2, 7, BRICKS);
        // Twelve frames facing in, each with an eye one time in ten.
        let mut rng = Rng(self.piece.variant ^ 0xE7E);
        let mut frames = Vec::with_capacity(12);
        for x in 4..=6 {
            frames.push((x, 8, Facing::South));
            frames.push((x, 12, Facing::North));
        }
        for z in 9..=11 {
            frames.push((3, z, Facing::East));
            frames.push((7, z, Facing::West));
        }
        let mut all_eyes = true;
        for (x, z, inward) in frames {
            let eye = rng.unit() > 0.9;
            all_eyes &= eye;
            let frame = Block(Block::END_PORTAL_FRAME.0 + if eye { 4 } else { 0 });
            self.set(x, 3, z, self.facing(frame, inward));
        }
        if all_eyes {
            self.fill(4, 3, 9, 6, 3, 11, Block::END_PORTAL);
        }
        self.set(5, 3, 6, Block::SPAWNER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_start_with_three_far_out_and_hold_128() {
        let s = Strongholds::new(7);
        let starts = s.starts();
        assert_eq!(starts.len(), COUNT);
        for p in &starts[..3] {
            let d = p.as_vec2().length();
            assert!((1280.0..=2816.0 + 64.0).contains(&d), "first ring at {d}");
        }
        assert!(starts[3].as_vec2().length() > 3000.0, "the second ring lies farther out");
    }

    #[test]
    fn layouts_reach_a_portal_room_without_overlaps() {
        let s = Strongholds::new(11);
        for i in 0..6 {
            let sh = s.get(i);
            let n = |k: Kind| sh.pieces.iter().filter(|p| p.kind == k).count();
            assert_eq!(n(Kind::PortalRoom), 1);
            assert!(n(Kind::Library) <= 2 && n(Kind::FiveCrossing) <= 5 && n(Kind::RoomCrossing) <= 6);
            assert_eq!(sh.pieces[1].kind, Kind::FiveCrossing, "the start leads to a five-way crossing");
            for (j, p) in sh.pieces.iter().enumerate() {
                for q in &sh.pieces[j + 1..] {
                    assert!(!p.bounds.intersects(&q.bounds), "{p:?} overlaps {q:?}");
                }
            }
            assert!(sh.bounds.max.y < SEA_LEVEL - BELOW_SEA + 1 && sh.bounds.min.y > 0, "{:?}", sh.bounds);
            // Eyes lead to the start.
            let start = s.starts()[i];
            let to = s.nearest(IVec3::new(start.x + 5, 70, start.y - 5)).unwrap();
            assert_eq!((to.x, to.z), (start.x + 2, start.y + 2));
        }
    }

    #[test]
    fn stronghold_loot_tables() {
        for seed in 0..100u64 {
            for tag in [CORRIDOR, CROSSING, LIBRARY] {
                let n = loot((seed << 2) | tag).slots.iter().flatten().count();
                let (lo, hi) = match tag {
                    CROSSING => (1, 4),
                    LIBRARY => (2, 10),
                    _ => (2, 3),
                };
                assert!((lo..=hi).contains(&n), "tag {tag}: {n} stacks");
            }
        }
    }
}
