//! Abandoned mineshafts, laid out like Java's `MineshaftPieces`.
//!
//! Each Java 16×16 chunk has probability 0.004 (`legacy_type_3` in the
//! `mineshafts` structure set). A start room at y = 50 grows corridors,
//! crossings and stairs within 80 blocks and depth 8, then the whole
//! layout sinks so its top sits below sea level (offset 10), like
//! `moveBelowSeaLevel`. Oak planks and fences, cobwebs, and rails are
//! painted per chunk so seams match. Chest-minecarts become plain chests
//! filled from `abandoned_mineshaft.json` mapped onto items we have.
//! Spider corridors hold one cave-spider spawner, as in Java.

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::{Block, Facing, RailShape};
use super::chest::Chest;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME};
use super::fortress::Feature;
use super::noise::{hash_f, hash3};
use super::structure::{Bounds, Oriented, Paint, Rng};
use crate::entity::MobKind;
#[cfg(test)]
use crate::item::{Item, Tier, ToolKind};

/// Java's `random_spread` frequency for mineshafts.
const FREQUENCY: f32 = 0.004;
/// Pieces must start within this many blocks of the start room (Java).
const REACH: i32 = 80;
const EXTENT: i32 = REACH + 24;
const MAX_DEPTH: u32 = 8;
const START_Y: i32 = 50;
/// Java `moveBelowSeaLevel(seaLevel, minY, random, 10)` uses sea = 63.
const SEA_LEVEL: i32 = 63;
const BELOW_SEA: i32 = 10;
const SALT: u64 = 0x4D49_4E45;
const CACHE_LIMIT: usize = 256;
const PLANKS: Block = Block::PLANKS;
const FENCE: Block = Block::OAK_FENCE;
const LOG: Block = Block::LOG;
const AIR: Block = Block::AIR;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Room,
    Corridor,
    Crossing,
    Stairs,
}

#[derive(Clone, Debug)]
pub struct Piece {
    pub kind: Kind,
    pub facing: Facing,
    pub bounds: Bounds,
    depth: u32,
    variant: u64,
    has_rails: bool,
    spider: bool,
    sections: i32,
    two_floor: bool,
    /// Crossing's incoming direction (the way it was entered).
    incoming: Facing,
    /// v0.16 Track E: a `mineshaft_mesa` piece (dark oak), generation 5.
    mesa: bool,
}

impl Oriented for Piece {
    fn facing(&self) -> Facing {
        self.facing
    }

    fn bounds(&self) -> &Bounds {
        &self.bounds
    }
}

pub struct Mineshaft {
    pub pieces: Vec<Piece>,
    pub bounds: Bounds,
    entrances: Vec<Bounds>,
}

struct Builder {
    rng: Rng,
    pieces: Vec<Piece>,
    start: Bounds,
    entrances: Vec<Bounds>,
}

impl Builder {
    fn collides(&self, b: &Bounds) -> bool {
        self.pieces.iter().any(|p| p.bounds.intersects(b))
    }

    fn add(&mut self, piece: Piece) -> usize {
        self.pieces.push(piece);
        self.pieces.len() - 1
    }

    /// Java `createRandomShaftPiece`: 20% crossing, 10% stairs, else corridor.
    fn create(&mut self, door: IVec3, facing: Facing, depth: u32) -> Option<Piece> {
        let roll = self.rng.below(100);
        if roll >= 80 {
            let two = self.rng.below(4) == 0;
            let h = if two { 7 } else { 3 };
            let bounds = Bounds::oriented(door, IVec3::new(-1, 0, 0), IVec3::new(5, h, 5), facing);
            if self.collides(&bounds) {
                return None;
            }
            return Some(Piece {
                kind: Kind::Crossing,
                facing,
                bounds,
                depth,
                variant: self.rng.next_u64(),
                has_rails: false,
                spider: false,
                sections: 0,
                two_floor: two,
                incoming: facing,
                mesa: false,
            });
        }
        if roll >= 70 {
            let bounds = Bounds::oriented(door, IVec3::new(0, -5, 0), IVec3::new(3, 8, 9), facing);
            if self.collides(&bounds) {
                return None;
            }
            return Some(Piece {
                kind: Kind::Stairs,
                facing,
                bounds,
                depth,
                variant: self.rng.next_u64(),
                has_rails: false,
                spider: false,
                sections: 0,
                two_floor: false,
                incoming: facing,
                mesa: false,
            });
        }
        let want = self.rng.below(3) as i32 + 2;
        for n in (1..=want).rev() {
            let len = n * 5;
            let bounds = Bounds::oriented(door, IVec3::ZERO, IVec3::new(3, 3, len), facing);
            if self.collides(&bounds) {
                continue;
            }
            let has_rails = self.rng.below(3) == 0;
            let spider = !has_rails && self.rng.below(23) == 0;
            return Some(Piece {
                kind: Kind::Corridor,
                facing,
                bounds,
                depth,
                variant: self.rng.next_u64(),
                has_rails,
                spider,
                sections: n,
                two_floor: false,
                incoming: facing,
                mesa: false,
            });
        }
        None
    }

    /// Java `generateAndAddPiece`: depth 8, 80 blocks from the start room.
    fn grow(&mut self, x: i32, y: i32, z: i32, facing: Facing, depth: u32) -> Option<usize> {
        if depth > MAX_DEPTH {
            return None;
        }
        if (x - self.start.min.x).abs() > REACH || (z - self.start.min.z).abs() > REACH {
            return None;
        }
        let piece = self.create(IVec3::new(x, y, z), facing, depth)?;
        Some(self.add(piece))
    }

    fn add_children(&mut self, i: usize) {
        let depth = self.pieces[i].depth;
        match self.pieces[i].kind {
            Kind::Room => self.room_children(i, depth),
            Kind::Corridor => self.corridor_children(i, depth),
            Kind::Crossing => self.crossing_children(i, depth),
            Kind::Stairs => self.stairs_children(i, depth),
        }
    }

    fn room_children(&mut self, i: usize, depth: u32) {
        let b = self.pieces[i].bounds;
        let y_span = (b.max.y - b.min.y + 1 - 3 - 1).max(1);
        let x_span = b.max.x - b.min.x + 1;
        let z_span = b.max.z - b.min.z + 1;
        let mut x = 0;
        while x < x_span {
            x += self.rng.below(x_span as u32) as i32;
            if x + 3 > x_span {
                break;
            }
            let y = b.min.y + 1 + self.rng.below(y_span as u32) as i32;
            if let Some(child) = self.grow(b.min.x + x, y, b.min.z - 1, Facing::North, depth + 1) {
                let c = self.pieces[child].bounds;
                self.entrances.push(Bounds {
                    min: IVec3::new(c.min.x, c.min.y, b.min.z),
                    max: IVec3::new(c.max.x, c.max.y, b.min.z + 1),
                });
                self.add_children(child);
            }
            x += 4;
        }
        let mut x = 0;
        while x < x_span {
            x += self.rng.below(x_span as u32) as i32;
            if x + 3 > x_span {
                break;
            }
            let y = b.min.y + 1 + self.rng.below(y_span as u32) as i32;
            if let Some(child) = self.grow(b.min.x + x, y, b.max.z + 1, Facing::South, depth + 1) {
                let c = self.pieces[child].bounds;
                self.entrances.push(Bounds {
                    min: IVec3::new(c.min.x, c.min.y, b.max.z - 1),
                    max: IVec3::new(c.max.x, c.max.y, b.max.z),
                });
                self.add_children(child);
            }
            x += 4;
        }
        let mut z = 0;
        while z < z_span {
            z += self.rng.below(z_span as u32) as i32;
            if z + 3 > z_span {
                break;
            }
            let y = b.min.y + 1 + self.rng.below(y_span as u32) as i32;
            if let Some(child) = self.grow(b.min.x - 1, y, b.min.z + z, Facing::West, depth + 1) {
                let c = self.pieces[child].bounds;
                self.entrances.push(Bounds {
                    min: IVec3::new(b.min.x, c.min.y, c.min.z),
                    max: IVec3::new(b.min.x + 1, c.max.y, c.max.z),
                });
                self.add_children(child);
            }
            z += 4;
        }
        let mut z = 0;
        while z < z_span {
            z += self.rng.below(z_span as u32) as i32;
            if z + 3 > z_span {
                break;
            }
            let y = b.min.y + 1 + self.rng.below(y_span as u32) as i32;
            if let Some(child) = self.grow(b.max.x + 1, y, b.min.z + z, Facing::East, depth + 1) {
                let c = self.pieces[child].bounds;
                self.entrances.push(Bounds {
                    min: IVec3::new(b.max.x - 1, c.min.y, c.min.z),
                    max: IVec3::new(b.max.x, c.max.y, c.max.z),
                });
                self.add_children(child);
            }
            z += 4;
        }
    }

    fn corridor_children(&mut self, i: usize, depth: u32) {
        let p = &self.pieces[i];
        let b = p.bounds;
        let y = b.min.y - 1 + self.rng.below(3) as i32;
        let turn = self.rng.below(4);
        match p.facing {
            Facing::North => {
                if turn <= 1 {
                    if let Some(c) = self.grow(b.min.x, y, b.min.z - 1, Facing::North, depth + 1) {
                        self.add_children(c);
                    }
                } else if turn == 2 {
                    if let Some(c) = self.grow(b.min.x - 1, y, b.min.z, Facing::West, depth + 1) {
                        self.add_children(c);
                    }
                } else if let Some(c) = self.grow(b.max.x + 1, y, b.min.z, Facing::East, depth + 1) {
                    self.add_children(c);
                }
            }
            Facing::South => {
                if turn <= 1 {
                    if let Some(c) = self.grow(b.min.x, y, b.max.z + 1, Facing::South, depth + 1) {
                        self.add_children(c);
                    }
                } else if turn == 2 {
                    if let Some(c) = self.grow(b.min.x - 1, y, b.max.z - 3, Facing::West, depth + 1) {
                        self.add_children(c);
                    }
                } else if let Some(c) = self.grow(b.max.x + 1, y, b.max.z - 3, Facing::East, depth + 1) {
                    self.add_children(c);
                }
            }
            Facing::West => {
                if turn <= 1 {
                    if let Some(c) = self.grow(b.min.x - 1, y, b.min.z, Facing::West, depth + 1) {
                        self.add_children(c);
                    }
                } else if turn == 2 {
                    if let Some(c) = self.grow(b.min.x, y, b.min.z - 1, Facing::North, depth + 1) {
                        self.add_children(c);
                    }
                } else if let Some(c) = self.grow(b.min.x, y, b.max.z + 1, Facing::South, depth + 1) {
                    self.add_children(c);
                }
            }
            Facing::East => {
                if turn <= 1 {
                    if let Some(c) = self.grow(b.max.x + 1, y, b.min.z, Facing::East, depth + 1) {
                        self.add_children(c);
                    }
                } else if turn == 2 {
                    if let Some(c) = self.grow(b.max.x - 3, y, b.min.z - 1, Facing::North, depth + 1) {
                        self.add_children(c);
                    }
                } else if let Some(c) = self.grow(b.max.x - 3, y, b.max.z + 1, Facing::South, depth + 1) {
                    self.add_children(c);
                }
            }
        }
        if depth >= MAX_DEPTH {
            return;
        }
        let p = &self.pieces[i];
        let b = p.bounds;
        if matches!(p.facing, Facing::North | Facing::South) {
            let mut z = b.min.z + 3;
            while z + 3 <= b.max.z {
                let side = self.rng.below(5);
                if side == 0
                    && let Some(c) = self.grow(b.min.x - 1, b.min.y, z, Facing::West, depth + 1)
                {
                    self.add_children(c);
                } else if side == 1
                    && let Some(c) = self.grow(b.max.x + 1, b.min.y, z, Facing::East, depth + 1)
                {
                    self.add_children(c);
                }
                z += 5;
            }
        } else {
            let mut x = b.min.x + 3;
            while x + 3 <= b.max.x {
                let side = self.rng.below(5);
                if side == 0
                    && let Some(c) = self.grow(x, b.min.y, b.min.z - 1, Facing::North, depth + 1)
                {
                    self.add_children(c);
                } else if side == 1
                    && let Some(c) = self.grow(x, b.min.y, b.max.z + 1, Facing::South, depth + 1)
                {
                    self.add_children(c);
                }
                x += 5;
            }
        }
    }

    fn spawn(&mut self, x: i32, y: i32, z: i32, facing: Facing, depth: u32) {
        if let Some(c) = self.grow(x, y, z, facing, depth + 1) {
            self.add_children(c);
        }
    }

    fn crossing_children(&mut self, i: usize, depth: u32) {
        let p = &self.pieces[i];
        let b = p.bounds;
        let two = p.two_floor;
        let incoming = p.incoming;
        match incoming {
            Facing::North => {
                self.spawn(b.min.x + 1, b.min.y, b.min.z - 1, Facing::North, depth);
                self.spawn(b.min.x - 1, b.min.y, b.min.z + 1, Facing::West, depth);
                self.spawn(b.max.x + 1, b.min.y, b.min.z + 1, Facing::East, depth);
            }
            Facing::South => {
                self.spawn(b.min.x + 1, b.min.y, b.max.z + 1, Facing::South, depth);
                self.spawn(b.min.x - 1, b.min.y, b.min.z + 1, Facing::West, depth);
                self.spawn(b.max.x + 1, b.min.y, b.min.z + 1, Facing::East, depth);
            }
            Facing::West => {
                self.spawn(b.min.x + 1, b.min.y, b.min.z - 1, Facing::North, depth);
                self.spawn(b.min.x + 1, b.min.y, b.max.z + 1, Facing::South, depth);
                self.spawn(b.min.x - 1, b.min.y, b.min.z + 1, Facing::West, depth);
            }
            Facing::East => {
                self.spawn(b.min.x + 1, b.min.y, b.min.z - 1, Facing::North, depth);
                self.spawn(b.min.x + 1, b.min.y, b.max.z + 1, Facing::South, depth);
                self.spawn(b.max.x + 1, b.min.y, b.min.z + 1, Facing::East, depth);
            }
        }
        if two {
            let y = b.min.y + 4;
            if self.rng.below(2) == 0 {
                self.spawn(b.min.x + 1, y, b.min.z - 1, Facing::North, depth);
            }
            if self.rng.below(2) == 0 {
                self.spawn(b.min.x - 1, y, b.min.z + 1, Facing::West, depth);
            }
            if self.rng.below(2) == 0 {
                self.spawn(b.max.x + 1, y, b.min.z + 1, Facing::East, depth);
            }
            if self.rng.below(2) == 0 {
                self.spawn(b.min.x + 1, y, b.max.z + 1, Facing::South, depth);
            }
        }
    }

    fn stairs_children(&mut self, i: usize, depth: u32) {
        let p = &self.pieces[i];
        let b = p.bounds;
        let (x, z, f) = match p.facing {
            Facing::North => (b.min.x, b.min.z - 1, Facing::North),
            Facing::South => (b.min.x, b.max.z + 1, Facing::South),
            Facing::West => (b.min.x - 1, b.min.z, Facing::West),
            Facing::East => (b.max.x + 1, b.min.z, Facing::East),
        };
        if let Some(c) = self.grow(x, b.min.y, z, f, depth + 1) {
            self.add_children(c);
        }
    }
}

impl Mineshaft {
    fn generate(mut rng: Rng, x: i32, z: i32) -> Mineshaft {
        let w = 7 + rng.below(6) as i32;
        let h = 4 + rng.below(6) as i32;
        let d = 7 + rng.below(6) as i32;
        let start = Bounds { min: IVec3::new(x, START_Y, z), max: IVec3::new(x + w, START_Y + h, z + d) };
        let room = Piece {
            kind: Kind::Room,
            facing: Facing::South,
            bounds: start,
            depth: 0,
            variant: rng.next_u64(),
            has_rails: false,
            spider: false,
            sections: 0,
            two_floor: false,
            incoming: Facing::South,
            mesa: false,
        };
        let mut b = Builder { rng, pieces: vec![room], start, entrances: Vec::new() };
        b.add_children(0);
        let mut bounds = b.pieces.iter().fold(start, |acc, p| acc.union(&p.bounds));
        // Java `moveBelowSeaLevel`.
        let ceiling = SEA_LEVEL - BELOW_SEA;
        let mut top = bounds.max.y - bounds.min.y + 1 + super::chunk::WORLD_MIN_Y + 1;
        if top < ceiling {
            top += b.rng.below((ceiling - top) as u32) as i32;
        }
        let shift = IVec3::new(0, top - bounds.max.y, 0);
        for p in &mut b.pieces {
            p.bounds = p.bounds.shifted(shift);
        }
        for e in &mut b.entrances {
            *e = e.shifted(shift);
        }
        bounds = bounds.shifted(shift);
        Mineshaft { pieces: b.pieces, bounds, entrances: b.entrances }
    }
}

// ---- Generation 5: Java's `MineshaftPieces` ------------------------------

/// What generation 5's mineshaft placement reads from the terrain.
pub trait ShaftTerrain {
    /// The biome at a block (cave biomes included).
    fn biome(&self, p: IVec3) -> super::biome::Biome;
    /// `WORLD_SURFACE_WG`: the first free block above the column's ground.
    fn surface(&self, x: i32, z: i32) -> i32;
}

/// v0.16 Track E: Java's `MineshaftPieces` generation over Java's random
/// sequence: pieces grow depth first (`generateAndAddPiece` adds a piece's
/// children as soon as it is placed).
struct JavaBuilder {
    rng: crate::enchant::JavaRandom,
    pieces: Vec<Piece>,
    start: Bounds,
    entrances: Vec<Bounds>,
    mesa: bool,
}

impl JavaBuilder {
    fn collides(&self, b: &Bounds) -> bool {
        self.pieces.iter().any(|p| p.bounds.intersects(b))
    }

    fn piece(&self, kind: Kind, facing: Facing, bounds: Bounds, depth: u32) -> Piece {
        Piece {
            kind,
            facing,
            bounds,
            depth,
            variant: hash3(bounds.min.x, bounds.min.y, bounds.min.z, SALT),
            has_rails: false,
            spider: false,
            sections: 0,
            two_floor: false,
            incoming: facing,
            mesa: self.mesa,
        }
    }

    /// `createRandomShaftPiece`: a crossing at 80..100, stairs at 70..80,
    /// else a corridor of the longest free length from `nextInt(3) + 2`
    /// sections down.
    fn create(&mut self, foot: IVec3, facing: Facing, depth: u32) -> Option<Piece> {
        let roll = self.rng.next_bounded(100);
        if roll >= 80 {
            let two = self.rng.next_bounded(4) == 0;
            let bounds =
                Bounds::oriented(foot, IVec3::new(-1, 0, 0), IVec3::new(5, if two { 7 } else { 3 }, 5), facing);
            if self.collides(&bounds) {
                return None;
            }
            let mut p = self.piece(Kind::Crossing, facing, bounds, depth);
            p.two_floor = two;
            return Some(p);
        }
        if roll >= 70 {
            let bounds = Bounds::oriented(foot, IVec3::new(0, -5, 0), IVec3::new(3, 8, 9), facing);
            return (!self.collides(&bounds)).then(|| self.piece(Kind::Stairs, facing, bounds, depth));
        }
        let longest = self.rng.next_bounded(3) + 2;
        for n in (1..=longest).rev() {
            let bounds = Bounds::oriented(foot, IVec3::ZERO, IVec3::new(3, 3, n * 5), facing);
            if self.collides(&bounds) {
                continue;
            }
            let mut p = self.piece(Kind::Corridor, facing, bounds, depth);
            p.has_rails = self.rng.next_bounded(3) == 0;
            p.spider = !p.has_rails && self.rng.next_bounded(23) == 0;
            p.sections = n;
            return Some(p);
        }
        None
    }

    /// `MineshaftPieces.generateAndAddPiece(…, depth)`: nothing past depth
    /// 8 or 80 blocks from the room; the new piece is `depth + 1` and grows
    /// its children at once.
    fn grow(&mut self, x: i32, y: i32, z: i32, facing: Facing, depth: u32) -> Option<usize> {
        if depth > MAX_DEPTH || (x - self.start.min.x).abs() > REACH || (z - self.start.min.z).abs() > REACH {
            return None;
        }
        let piece = self.create(IVec3::new(x, y, z), facing, depth + 1)?;
        self.pieces.push(piece);
        let i = self.pieces.len() - 1;
        self.add_children(i);
        Some(i)
    }

    fn add_children(&mut self, i: usize) {
        let p = self.pieces[i];
        let (b, d) = (p.bounds, p.depth);
        match p.kind {
            Kind::Room => self.room_children(b, d),
            Kind::Corridor => {
                let end = self.rng.next_bounded(4);
                let y = b.min.y - 1 + self.rng.next_bounded(3);
                let (x, z, f) = match (p.facing, end) {
                    (Facing::North, 0 | 1) => (b.min.x, b.min.z - 1, Facing::North),
                    (Facing::North, 2) => (b.min.x - 1, b.min.z, Facing::West),
                    (Facing::North, _) => (b.max.x + 1, b.min.z, Facing::East),
                    (Facing::South, 0 | 1) => (b.min.x, b.max.z + 1, Facing::South),
                    (Facing::South, 2) => (b.min.x - 1, b.max.z - 3, Facing::West),
                    (Facing::South, _) => (b.max.x + 1, b.max.z - 3, Facing::East),
                    (Facing::West, 0 | 1) => (b.min.x - 1, b.min.z, Facing::West),
                    (Facing::West, 2) => (b.min.x, b.min.z - 1, Facing::North),
                    (Facing::West, _) => (b.min.x, b.max.z + 1, Facing::South),
                    (Facing::East, 0 | 1) => (b.max.x + 1, b.min.z, Facing::East),
                    (Facing::East, 2) => (b.max.x - 3, b.min.z - 1, Facing::North),
                    (Facing::East, _) => (b.max.x - 3, b.max.z + 1, Facing::South),
                };
                self.grow(x, y, z, f, d);
                // Side branches pass `depth + 1` on (so they are two deeper).
                if d < MAX_DEPTH {
                    if matches!(p.facing, Facing::North | Facing::South) {
                        let mut z = b.min.z + 3;
                        while z + 3 <= b.max.z {
                            match self.rng.next_bounded(5) {
                                0 => _ = self.grow(b.min.x - 1, b.min.y, z, Facing::West, d + 1),
                                1 => _ = self.grow(b.max.x + 1, b.min.y, z, Facing::East, d + 1),
                                _ => {}
                            }
                            z += 5;
                        }
                    } else {
                        let mut x = b.min.x + 3;
                        while x + 3 <= b.max.x {
                            match self.rng.next_bounded(5) {
                                0 => _ = self.grow(x, b.min.y, b.min.z - 1, Facing::North, d + 1),
                                1 => _ = self.grow(x, b.min.y, b.max.z + 1, Facing::South, d + 1),
                                _ => {}
                            }
                            x += 5;
                        }
                    }
                }
            }
            Kind::Crossing => {
                let n = (b.min.x + 1, b.min.z - 1, Facing::North);
                let s = (b.min.x + 1, b.max.z + 1, Facing::South);
                let w = (b.min.x - 1, b.min.z + 1, Facing::West);
                let e = (b.max.x + 1, b.min.z + 1, Facing::East);
                let order = match p.incoming {
                    Facing::North => [n, w, e],
                    Facing::South => [s, w, e],
                    Facing::West => [n, s, w],
                    Facing::East => [n, s, e],
                };
                for (x, z, f) in order {
                    self.grow(x, b.min.y, z, f, d);
                }
                if p.two_floor {
                    for (x, z, f) in [n, w, e, s] {
                        if self.rng.next_boolean() {
                            self.grow(x, b.min.y + 4, z, f, d);
                        }
                    }
                }
            }
            Kind::Stairs => {
                let (x, z) = match p.facing {
                    Facing::North => (b.min.x, b.min.z - 1),
                    Facing::South => (b.min.x, b.max.z + 1),
                    Facing::West => (b.min.x - 1, b.min.z),
                    Facing::East => (b.max.x + 1, b.min.z),
                };
                self.grow(x, b.min.y, z, p.facing, d);
            }
        }
    }

    /// `MineShaftRoom.addChildren`: doorways along each side at random
    /// spacing and height, each remembered as an entrance to carve.
    fn room_children(&mut self, b: Bounds, d: u32) {
        let height = (b.max.y - b.min.y + 1 - 4).max(1);
        let span = b.max - b.min + IVec3::ONE;
        for side in [Facing::North, Facing::South, Facing::West, Facing::East] {
            let len = if matches!(side, Facing::North | Facing::South) { span.x } else { span.z };
            let mut pos = 0;
            while pos < len {
                pos += self.rng.next_bounded(len);
                if pos + 3 > len {
                    break;
                }
                let y = b.min.y + self.rng.next_bounded(height) + 1;
                let (x, z) = match side {
                    Facing::North => (b.min.x + pos, b.min.z - 1),
                    Facing::South => (b.min.x + pos, b.max.z + 1),
                    Facing::West => (b.min.x - 1, b.min.z + pos),
                    Facing::East => (b.max.x + 1, b.min.z + pos),
                };
                if let Some(child) = self.grow(x, y, z, side, d) {
                    let c = self.pieces[child].bounds;
                    self.entrances.push(match side {
                        Facing::North => Bounds {
                            min: IVec3::new(c.min.x, c.min.y, b.min.z),
                            max: IVec3::new(c.max.x, c.max.y, b.min.z + 1),
                        },
                        Facing::South => Bounds {
                            min: IVec3::new(c.min.x, c.min.y, b.max.z - 1),
                            max: IVec3::new(c.max.x, c.max.y, b.max.z),
                        },
                        Facing::West => Bounds {
                            min: IVec3::new(b.min.x, c.min.y, c.min.z),
                            max: IVec3::new(b.min.x + 1, c.max.y, c.max.z),
                        },
                        Facing::East => Bounds {
                            min: IVec3::new(b.max.x - 1, c.min.y, c.min.z),
                            max: IVec3::new(b.max.x, c.max.y, c.max.z),
                        },
                    });
                }
                pos += 4;
            }
        }
    }
}

impl Mineshaft {
    /// v0.16 Track E, generation 5: `MineshaftStructure.findGenerationPoint`
    /// for the start chunk `chunk` (`mesa` for `mineshaft_mesa`): the
    /// placement's `nextDouble`, the room at block (2, 2), its children,
    /// then `moveBelowSeaLevel(63, -64, random, 10)`, or for the mesa
    /// type the layout's centre moved to a random height between sea level
    /// and the surface there. Returns the layout and its start position
    /// (the chunk's middle x, y 50 moved like the pieces, its north edge),
    /// where Java checks the biome.
    pub fn generate_java(seed: u64, chunk: IVec2, mesa: bool, t: &dyn ShaftTerrain) -> (Mineshaft, IVec3) {
        let mut rng = super::java_pieces::large_feature_random(seed, chunk);
        let _ = super::java_pieces::next_double(&mut rng);
        let (x, z) = (chunk.x * 16 + 2, chunk.y * 16 + 2);
        let max_x = x + 7 + rng.next_bounded(6);
        let max_y = 54 + rng.next_bounded(6);
        let max_z = z + 7 + rng.next_bounded(6);
        let start = Bounds { min: IVec3::new(x, START_Y, z), max: IVec3::new(max_x, max_y, max_z) };
        let mut b = JavaBuilder { rng, pieces: Vec::new(), start, entrances: Vec::new(), mesa };
        let mut room = b.piece(Kind::Room, Facing::South, start, 0);
        room.variant = hash3(x, START_Y, z, SALT);
        b.pieces.push(room);
        b.add_children(0);
        let all = b.pieces.iter().fold(start, |acc, p| acc.union(&p.bounds));
        let dy = if mesa {
            // `Mth.randomBetweenInclusive(random, seaLevel, surface)`.
            let centre = all.min + (all.max - all.min + IVec3::ONE) / 2;
            let surface = t.surface(centre.x, centre.z);
            let target =
                if surface <= SEA_LEVEL { SEA_LEVEL } else { SEA_LEVEL + b.rng.next_bounded(surface - SEA_LEVEL + 1) };
            target - centre.y
        } else {
            super::java_pieces::below_sea_level_shift(&all, SEA_LEVEL, super::chunk::WORLD_MIN_Y, &mut b.rng, BELOW_SEA)
        };
        let shift = IVec3::new(0, dy, 0);
        for p in &mut b.pieces {
            p.bounds = p.bounds.shifted(shift);
        }
        for e in &mut b.entrances {
            *e = e.shifted(shift);
        }
        let shaft = Mineshaft { pieces: b.pieces, bounds: all.shifted(shift), entrances: b.entrances };
        (shaft, IVec3::new(chunk.x * 16 + 8, START_Y + dy, chunk.y * 16))
    }

    /// Generation 5's start in `chunk`: `legacy_type_3` (a
    /// `setLargeFeatureSeed` random's first `nextDouble` below 0.004), then
    /// `ChunkGenerator.tryGenerateStructure`'s weighted pick between
    /// `mineshaft` and `mineshaft_mesa` (weight 1 each), each kept only
    /// where its start lies in its biomes (`#has_structure/mineshaft`: the
    /// Overworld but badlands and the deep dark; `mineshaft_mesa`: badlands).
    pub fn start_java(seed: u64, chunk: IVec2, t: &dyn ShaftTerrain) -> Option<Mineshaft> {
        let mut place = super::java_pieces::large_feature_random(seed, chunk);
        if super::java_pieces::next_double(&mut place) >= f64::from(FREQUENCY_F32) {
            return None;
        }
        let first_mesa = super::java_pieces::large_feature_random(seed, chunk).next_bounded(2) == 1;
        for mesa in [first_mesa, !first_mesa] {
            let (shaft, at) = Self::generate_java(seed, chunk, mesa, t);
            let biome = t.biome(at);
            let valid =
                if mesa { biome.is_badlands() } else { !biome.is_badlands() && biome != super::biome::Biome::DeepDark };
            if valid {
                return Some(shaft);
            }
        }
        None
    }
}

/// `mineshafts.json`'s `frequency` (a float).
const FREQUENCY_F32: f32 = 0.004;

pub struct Mineshafts {
    seed: u64,
    /// Generation 5 places and lays out mineshafts by Java's random sequence.
    generation: super::terrain::Generation,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<Mineshaft>>>>,
}

impl Mineshafts {
    pub fn new(seed: u64) -> Self {
        Self::with_generation(seed, super::terrain::Generation::LATEST)
    }

    pub fn with_generation(seed: u64, generation: super::terrain::Generation) -> Self {
        Self { seed, generation, cache: Mutex::new(FxHashMap::default()) }
    }

    fn java(&self) -> bool {
        self.generation >= super::terrain::Generation::V5
    }

    /// The placement's chance alone (generation 5: `legacy_type_3`).
    fn frequency_hit(&self, chunk16: IVec2) -> bool {
        if self.java() {
            let mut r = super::java_pieces::large_feature_random(self.seed, chunk16);
            return super::java_pieces::next_double(&mut r) < f64::from(FREQUENCY_F32);
        }
        hash_f(chunk16.x, 0, chunk16.y, self.seed ^ SALT) < FREQUENCY
    }

    /// Whether a mineshaft starts in the 16-block chunk `chunk16`
    /// (generation 5 also checks its biome).
    pub fn starts_at(&self, t: &dyn ShaftTerrain, chunk16: IVec2) -> bool {
        if self.java() { self.get(t, chunk16).is_some() } else { self.frequency_hit(chunk16) }
    }

    fn get(&self, t: &dyn ShaftTerrain, chunk16: IVec2) -> Option<Arc<Mineshaft>> {
        if let Some(found) = self.cache.lock().unwrap().get(&chunk16) {
            return found.clone();
        }
        let shaft = if self.java() {
            self.frequency_hit(chunk16).then(|| Mineshaft::start_java(self.seed, chunk16, t)).flatten().map(Arc::new)
        } else {
            self.frequency_hit(chunk16).then(|| {
                let rng = Rng(hash3(chunk16.x, 0, chunk16.y, self.seed ^ SALT));
                Arc::new(Mineshaft::generate(rng, chunk16.x * 16 + 2, chunk16.y * 16 + 2))
            })
        };
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT
            && let Some(k) = cache.keys().next().copied()
        {
            cache.remove(&k);
        }
        cache.entry(chunk16).or_insert(shaft).clone()
    }

    fn near(&self, t: &dyn ShaftTerrain, min: IVec2, max: IVec2) -> Vec<Arc<Mineshaft>> {
        let lo = (min - EXTENT).div_euclid(IVec2::splat(16));
        let hi = (max + EXTENT).div_euclid(IVec2::splat(16));
        let mut out = Vec::new();
        for cz in lo.y..=hi.y {
            for cx in lo.x..=hi.x {
                let chunk16 = IVec2::new(cx, cz);
                if !self.frequency_hit(chunk16) {
                    continue;
                }
                let start = IVec2::new(cx * 16 + 2, cz * 16 + 2);
                if (start - EXTENT).cmpgt(max).any() || (start + EXTENT).cmplt(min).any() {
                    continue;
                }
                if let Some(s) = self.get(t, chunk16)
                    && s.bounds.min.x <= max.x
                    && s.bounds.max.x >= min.x
                    && s.bounds.min.z <= max.y
                    && s.bounds.max.z >= min.y
                {
                    out.push(s);
                }
            }
        }
        out
    }

    pub fn nearest(&self, t: &dyn ShaftTerrain, p: IVec3) -> Option<IVec3> {
        let here = IVec2::new(p.x, p.z);
        let origin = here.div_euclid(IVec2::splat(16));
        (0..=48i32).find_map(|ring| {
            (-ring..=ring).flat_map(move |dz| (-ring..=ring).map(move |dx| origin + IVec2::new(dx, dz))).find_map(
                |chunk16| {
                    if dx_ring(chunk16, origin, ring) {
                        self.get(t, chunk16).map(|s| {
                            let b = s.bounds;
                            IVec3::new((b.min.x + b.max.x) / 2, b.min.y, (b.min.z + b.max.z) / 2)
                        })
                    } else {
                        None
                    }
                },
            )
        })
    }

    pub fn paint(&self, t: &dyn ShaftTerrain, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let solid = |_: i32, _: i32, _: i32, _: i32| false;
        for shaft in self.near(t, IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            if shaft.bounds.max.y < base.y || shaft.bounds.min.y > top.y {
                continue;
            }
            for piece in shaft.pieces.iter().filter(|p| p.bounds.intersects(&chunk) || floor_touches(p, &chunk)) {
                let pillar = if piece.mesa { Block::DARK_OAK_LOG } else { LOG };
                Paint { blocks: &mut *blocks, base, top, piece, open: &solid, pillar }.piece(&shaft.entrances);
            }
        }
    }

    pub fn features(&self, t: &dyn ShaftTerrain, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let mut out = Vec::new();
        for shaft in self.near(t, IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            for p in shaft.pieces.iter().filter(|p| p.kind == Kind::Corridor && p.bounds.intersects(&chunk)) {
                let end = p.sections * 5 - 1;
                let spawner = p.world(1, 0, spider_spawner_z(p));
                if p.spider && chunk.contains(spawner) {
                    out.push((spawner, Feature::Spawner(MobKind::CaveSpider)));
                }
                for s in 0..p.sections {
                    let z = 2 + s * 5;
                    for &(lx, lz) in &[(2, z - 1), (0, z + 1)] {
                        if !(0..=end).contains(&lz) {
                            continue;
                        }
                        let at = p.world(lx, 0, lz);
                        if chunk.contains(at) && hash3(at.x, at.y, at.z, p.variant ^ 0x4348).is_multiple_of(100) {
                            out.push((at, Feature::MineshaftChest(p.variant ^ at.x as u64 ^ at.z as u64)));
                        }
                    }
                }
            }
        }
        out
    }
}

/// Java puts a spider corridor's spawner on the centre line, within a block
/// of the middle (`l / 2 - 1 + nextInt(3)` of its length `l`).
fn spider_spawner_z(p: &Piece) -> i32 {
    let middle = (p.sections * 5 - 1) / 2;
    middle - 1 + (hash3(p.bounds.min.x, p.bounds.min.y, p.bounds.min.z, p.variant ^ 0x5350) % 3) as i32
}

fn dx_ring(chunk16: IVec2, origin: IVec2, ring: i32) -> bool {
    let d = chunk16 - origin;
    d.x.abs() == ring || d.y.abs() == ring || ring == 0
}

fn floor_touches(p: &Piece, chunk: &Bounds) -> bool {
    // Floor planks sit at local y = -1.
    let floor =
        Bounds { min: p.bounds.min - IVec3::Y, max: IVec3::new(p.bounds.max.x, p.bounds.min.y - 1, p.bounds.max.z) };
    floor.intersects(chunk)
}

/// `chests/abandoned_mineshaft.json` mapped onto items we have. Missing
/// entries (name tags, seeds, glow berries,
/// powered/detector/activator rails) stay weighted blanks.
#[cfg(test)]
type Entry = (Option<Item>, u32, u8, u8);
#[cfg(test)]
const RARE: &[Entry] = &[
    (Some(Item::GOLDEN_APPLE), 20, 1, 1),          // golden apple
    (Some(Item::ENCHANTED_GOLDEN_APPLE), 1, 1, 1), // enchanted golden apple
    (Some(Item::NAME_TAG), 30, 1, 1),              // name tag
    (Some(Item::ENCHANTED_BOOK), 10, 1, 1),
    (Some(Item::tool(ToolKind::Pickaxe, Tier::Iron)), 5, 1, 1),
    (None, 5, 1, 1), // empty
];
#[cfg(test)]
const COMMON: &[Entry] = &[
    (Some(Item::IRON_INGOT), 10, 1, 5),
    (Some(Item::GOLD_INGOT), 5, 1, 3),
    (Some(Item::REDSTONE), 5, 4, 9),
    (Some(Item::LAPIS_LAZULI), 5, 4, 9),
    (Some(Item::DIAMOND), 3, 1, 2),
    (Some(Item::COAL), 10, 3, 8),
    (Some(Item::BREAD), 15, 1, 3),
    (None, 15, 3, 6),                       // glow berries
    (Some(Item::MELON_SEEDS), 10, 2, 4),    // melon seeds
    (Some(Item::PUMPKIN_SEEDS), 10, 2, 4),  // pumpkin seeds
    (Some(Item::BEETROOT_SEEDS), 10, 2, 4), // beetroot seeds
];
#[cfg(test)]
const RAILS: &[Entry] = &[
    (Some(Item::from_block(Block::RAIL)), 20, 4, 8),
    (None, 5, 1, 4), // powered rail
    (None, 5, 1, 4), // detector rail
    (None, 5, 1, 4), // activator rail
    (Some(Item::from_block(Block::TORCH)), 15, 1, 16),
];

#[cfg(test)]
fn roll_pool(chest: &mut super::loot::LootItems, rng: &mut super::loot::LootRandom, table: &[Entry], rolls: u32) {
    let total: u32 = table.iter().map(|e| e.1).sum();
    for _ in 0..rolls {
        let mut choice = rng.below(total);
        for &(item, weight, lo, hi) in table {
            if choice < weight {
                let count = rng.range(lo as u32, hi as u32) as u8;
                if let Some(item) = item {
                    chest.push(crate::inventory::Stack::new(item, count));
                }
                break;
            }
            choice -= weight;
        }
    }
}

pub fn loot(seed: u64) -> Chest {
    super::loot::table(seed, "abandoned_mineshaft")
}

#[cfg(test)]
#[allow(dead_code)]
fn legacy_loot(seed: u64) -> Chest {
    let mut rng = super::loot::LootRandom::new(seed);
    let mut chest = super::loot::LootItems::default();
    roll_pool(&mut chest, &mut rng, RARE, 1);
    let common = rng.range(2, 4);
    roll_pool(&mut chest, &mut rng, COMMON, common);
    roll_pool(&mut chest, &mut rng, RAILS, 3);
    chest.finish(&mut rng, 27)
}

impl Paint<'_, Piece> {
    fn piece(&mut self, entrances: &[Bounds]) {
        match self.piece.kind {
            Kind::Room => self.room(entrances),
            Kind::Corridor => self.corridor(),
            Kind::Crossing => self.crossing(),
            Kind::Stairs => self.shaft_stairs(),
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Option<Block> {
        self.cell(x, y, z).map(|i| self.blocks[i])
    }

    fn chance(&self, x: i32, y: i32, z: i32, p: f32) -> bool {
        let w = self.piece.world(x, y, z);
        hash_f(w.x, w.y, w.z, self.piece.variant) < p
    }

    fn cobweb(&mut self, p: f32, x: i32, y: i32, z: i32) {
        if self.chance(x, y, z, p) {
            self.set(x, y, z, Block::COBWEB);
        }
    }

    fn set_planks(&mut self, x: i32, y: i32, z: i32) {
        if let Some(i) = self.cell(x, y, z) {
            let b = self.blocks[i];
            if !b.is_solid() || b.top_drop() != 0 {
                self.blocks[i] = self.planks();
            }
        }
    }

    /// v0.16 Track E: `MineshaftStructure.Type`'s planks and fence (dark
    /// oak for `mineshaft_mesa`).
    fn planks(&self) -> Block {
        if self.piece.mesa { Block::DARK_OAK_PLANKS } else { PLANKS }
    }

    fn fence(&self) -> Block {
        if self.piece.mesa { super::forms::wood_id(0, 5) } else { FENCE }
    }

    fn room(&mut self, entrances: &[Bounds]) {
        let b = self.piece.bounds;
        let (w, h, d) = (b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z);
        let top = h.min(3);
        self.fill(0, 1, 0, w, top, d, AIR);
        for e in entrances {
            let lo = e.min - b.min;
            let hi = e.max - b.min;
            self.fill(lo.x, (hi.y - 2).max(0), lo.z, hi.x, hi.y, hi.z, AIR);
        }
        self.dome(0, 4, 0, w, h, d);
    }

    fn dome(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) {
        if y1 < y0 {
            return;
        }
        let rx = (x1 - x0 + 1) as f32;
        let ry = (y1 - y0 + 1) as f32;
        let rz = (z1 - z0 + 1) as f32;
        let cx = x0 as f32 + rx / 2.0;
        let cy = y0 as f32 + ry / 2.0;
        let cz = z0 as f32 + rz / 2.0;
        for x in x0..=x1 {
            let dx = (x as f32 - cx) / (rx * 0.5);
            for y in y0..=y1 {
                let dy = (y as f32 - cy) / (ry * 0.5);
                for z in z0..=z1 {
                    let dz = (z as f32 - cz) / (rz * 0.5);
                    if dx * dx + dy * dy + dz * dz <= 1.05 {
                        self.set(x, y, z, AIR);
                    }
                }
            }
        }
    }

    fn corridor(&mut self) {
        let end = self.piece.sections * 5 - 1;
        self.fill(0, 0, 0, 2, 1, end, AIR);
        for z in 0..=end {
            if self.chance(1, 2, z, 0.8) {
                self.fill(0, 2, z, 2, 2, z, AIR);
            }
        }
        if self.piece.spider {
            for x in 0..=2 {
                for y in 0..=1 {
                    for z in 0..=end {
                        self.cobweb(0.6, x, y, z);
                    }
                }
            }
            // The spawner replaces whatever the webs left in its cell.
            self.set(1, 0, spider_spawner_z(self.piece), Block::SPAWNER);
        }
        for s in 0..self.piece.sections {
            let z = 2 + s * 5;
            self.support(z);
            self.cobweb(0.1, 0, 2, z - 1);
            self.cobweb(0.1, 2, 2, z - 1);
            self.cobweb(0.1, 0, 2, z + 1);
            self.cobweb(0.1, 2, 2, z + 1);
            self.cobweb(0.05, 0, 2, z - 2);
            self.cobweb(0.05, 2, 2, z - 2);
            self.cobweb(0.05, 0, 2, z + 2);
            self.cobweb(0.05, 2, 2, z + 2);
            for &(lx, lz) in &[(2, z - 1), (0, z + 1)] {
                let at = self.piece.world(lx, 0, lz);
                if hash3(at.x, at.y, at.z, self.piece.variant ^ 0x4348).is_multiple_of(100) {
                    let below = self.get(lx, -1, lz).or_else(|| self.get(lx, 0, lz));
                    if self.get(lx, 0, lz) == Some(AIR) && below.is_none_or(|b| b.is_solid()) {
                        self.set(lx, 0, lz, Block::CHEST);
                    }
                }
            }
        }
        for x in 0..=2 {
            for z in 0..=end {
                self.set_planks(x, -1, z);
            }
        }
        self.column_down(0, -1, 2);
        self.column_down(2, -1, 2);
        if self.piece.sections > 1 {
            self.column_down(0, -1, end - 2);
            self.column_down(2, -1, end - 2);
        }
        if self.piece.has_rails {
            let shape = if matches!(self.piece.facing, Facing::East | Facing::West) {
                RailShape::EastWest
            } else {
                RailShape::NorthSouth
            };
            let rail = Block::rail(shape);
            for z in 0..=end {
                let p = if self.get(1, 0, z) == Some(AIR) { 0.7 } else { 0.9 };
                if self.get(1, -1, z).is_some_and(|b| b.is_solid())
                    && self.chance(1, 0, z, p)
                    && self.get(1, 0, z).is_none_or(|b| b == AIR)
                {
                    self.set(1, 0, z, rail);
                }
            }
        }
    }

    fn support(&mut self, z: i32) {
        let (fence, planks) = (self.fence(), self.planks());
        self.fill(0, 0, z, 0, 1, z, fence);
        self.fill(2, 0, z, 2, 1, z, fence);
        if hash3(z, 0, self.piece.variant as i32, self.piece.variant).is_multiple_of(4) {
            self.set(0, 2, z, planks);
            self.set(2, 2, z, planks);
        } else {
            self.fill(0, 2, z, 2, 2, z, planks);
        }
    }

    fn crossing(&mut self) {
        let b = self.piece.bounds;
        let (w, h, d) = (b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z);
        if self.piece.two_floor {
            self.fill(1, 0, 0, w - 1, 2, d, AIR);
            self.fill(0, 0, 1, w, 2, d - 1, AIR);
            self.fill(1, h - 2, 0, w - 1, h, d, AIR);
            self.fill(0, h - 2, 1, w, h, d - 1, AIR);
            self.fill(1, 3, 1, w - 1, 3, d - 1, AIR);
        } else {
            self.fill(1, 0, 0, w - 1, h, d, AIR);
            self.fill(0, 0, 1, w, h, d - 1, AIR);
        }
        self.pillar(1, 0, 1, h);
        self.pillar(1, 0, d - 1, h);
        self.pillar(w - 1, 0, 1, h);
        self.pillar(w - 1, 0, d - 1, h);
        for x in 0..=w {
            for z in 0..=d {
                self.set_planks(x, -1, z);
            }
        }
    }

    fn pillar(&mut self, x: i32, y0: i32, z: i32, y1: i32) {
        if self.get(x, y1 + 1, z) != Some(AIR) {
            let planks = self.planks();
            self.fill(x, y0, z, x, y1, z, planks);
        }
    }

    fn shaft_stairs(&mut self) {
        self.fill(0, 5, 0, 2, 7, 1, AIR);
        self.fill(0, 0, 7, 2, 2, 8, AIR);
        for i in 0..5 {
            let drop = if i < 4 { 1 } else { 0 };
            self.fill(0, 5 - i - drop, 2 + i, 2, 7 - i, 2 + i, AIR);
        }
    }
}

#[cfg(test)]
mod tests {
    /// Plains everywhere, the surface at y 64 (the placement stub tests use).
    pub(super) struct Flat;

    impl ShaftTerrain for Flat {
        fn biome(&self, _: IVec3) -> crate::world::biome::Biome {
            crate::world::biome::Biome::Plains
        }

        fn surface(&self, _: i32, _: i32) -> i32 {
            64
        }
    }

    use super::*;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    /// `chests/abandoned_mineshaft.json`: golden apple 20 and enchanted golden apple 1 of 71.
    #[test]
    fn mineshaft_chests_award_both_golden_apples_with_java_weights() {
        assert!(RARE.contains(&(Some(Item::GOLDEN_APPLE), 20, 1, 1)));
        assert!(RARE.contains(&(Some(Item::ENCHANTED_GOLDEN_APPLE), 1, 1, 1)));
        let found = |item| (0..20_000u64).any(|seed| loot(seed).slots.iter().flatten().any(|s| s.item == item));
        assert!(found(Item::GOLDEN_APPLE) && found(Item::ENCHANTED_GOLDEN_APPLE));
    }

    #[test]
    fn frequency_matches_java_legacy_type_3() {
        let m = Mineshafts::with_generation(1, super::super::terrain::Generation::V4);
        let hits = (0..10000).filter(|&i| m.frequency_hit(IVec2::new(i, 0))).count();
        assert!((20..60).contains(&hits), "{hits} hits in 10000, expected ~40");
    }

    #[test]
    fn loot_tables_keep_java_weights() {
        assert_eq!(RARE.iter().map(|e| e.1).sum::<u32>(), 71);
        assert_eq!(COMMON.iter().map(|e| e.1).sum::<u32>(), 98);
        assert_eq!(RAILS.iter().map(|e| e.1).sum::<u32>(), 50);
        assert_eq!(loot(9).serialize(), loot(9).serialize());
        assert!(loot(3).slots.iter().flatten().any(|s| s.item == Item::from_block(Block::RAIL)
            || s.item == Item::IRON_INGOT
            || s.item == Item::BREAD
            || s.item == Item::ENCHANTED_BOOK
            || s.item == Item::from_block(Block::TORCH)
            || s.item == Item::COAL
            || s.item == Item::LAPIS_LAZULI
            || s.item == Item::DIAMOND
            || s.item == Item::GOLD_INGOT
            || s.item == Item::tool(ToolKind::Pickaxe, Tier::Iron)));
    }

    #[test]
    fn layouts_have_rooms_and_corridors_and_cross_chunk_seams() {
        let m = Mineshafts::with_generation(7, super::super::terrain::Generation::V4);
        let found = (0..3000)
            .find_map(|i| m.get(&Flat, IVec2::new(i, 0)))
            .expect("frequency 0.004 should hit within 3000 chunks");
        assert!(found.pieces.iter().any(|p| p.kind == Kind::Room));
        assert!(found.pieces.iter().any(|p| p.kind == Kind::Corridor));
        assert!(found.pieces.len() > 2, "tiny mineshaft: {}", found.pieces.len());
        let kinds = found.pieces.iter().map(|p| p.kind).collect::<Vec<_>>();
        assert!(kinds.contains(&Kind::Room));

        let corridor = found.pieces.iter().find(|p| p.kind == Kind::Corridor).unwrap();
        let mut left = [Block::STONE; CHUNK_VOLUME];
        let mut right = [Block::STONE; CHUNK_VOLUME];
        let y = corridor.bounds.min.y.div_euclid(CHUNK_SIZE_I) * CHUNK_SIZE_I;
        let mid = (corridor.bounds.min.x + corridor.bounds.max.x) / 2;
        let seam = mid.div_euclid(CHUNK_SIZE_I) * CHUNK_SIZE_I;
        Paint {
            blocks: &mut left,
            base: IVec3::new(seam - CHUNK_SIZE_I, y, corridor.bounds.min.z.div_euclid(CHUNK_SIZE_I) * CHUNK_SIZE_I),
            top: IVec3::new(seam - 1, y + 31, corridor.bounds.min.z.div_euclid(CHUNK_SIZE_I) * CHUNK_SIZE_I + 31),
            piece: corridor,
            open: &|_, _, _, _| false,
            pillar: LOG,
        }
        .piece(&[]);
        Paint {
            blocks: &mut right,
            base: IVec3::new(seam, y, corridor.bounds.min.z.div_euclid(CHUNK_SIZE_I) * CHUNK_SIZE_I),
            top: IVec3::new(seam + 31, y + 31, corridor.bounds.min.z.div_euclid(CHUNK_SIZE_I) * CHUNK_SIZE_I + 31),
            piece: corridor,
            open: &|_, _, _, _| false,
            pillar: LOG,
        }
        .piece(&[]);
        let airs = left.iter().filter(|&&b| b == AIR).count() + right.iter().filter(|&&b| b == AIR).count();
        assert!(airs > 0, "corridor should carve air on at least one side of a seam");
    }

    #[test]
    fn rails_and_chests_register_through_the_world() {
        let generator = Generator::new(11);
        let found = (0..800)
            .find_map(|i| {
                generator.mineshafts.get(&generator, IVec2::new(i, 2)).and_then(|s| {
                    s.pieces.iter().find(|p| p.kind == Kind::Corridor && p.has_rails).cloned().map(|p| (s, p))
                })
            })
            .expect("some mineshaft corridor should have rails");
        let (_shaft, piece) = found;
        eprintln!("mineshaft rails corridor {:?} facing {:?}", piece.bounds, piece.facing);
        let mut rails = 0;
        let mut planks = 0;
        let lo = piece.bounds.min.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let hi = piece.bounds.max.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let cpos = IVec3::new(x, y, z);
                    let generated = generator.generate(cpos);
                    generated.for_each_block(|b| {
                        if b.is_rail() {
                            rails += 1;
                        }
                        if b == Block::PLANKS || b == Block::OAK_FENCE {
                            planks += 1;
                        }
                    });
                    let features = generator.mineshafts.features(&generator, cpos);
                    let mut world =
                        super::super::World::new_headless(Arc::new(Generator::new(11)), Default::default(), 1);
                    world.register_structure_features(cpos, &generated);
                    for (p, feature) in features {
                        if let Feature::MineshaftChest(_) = feature {
                            let local = p - cpos * CHUNK_SIZE_I;
                            if generated.get(local.x as usize, local.y as usize, local.z as usize) == Block::CHEST {
                                assert!(world.chest(p).is_some());
                            }
                        }
                    }
                }
            }
        }
        assert!(planks > 0, "mineshaft should place oak planks or fences");
        assert!(rails > 0, "a has_rails corridor should place rail blocks, found {rails}");
    }

    #[test]
    fn spider_corridors_register_a_cave_spider_spawner() {
        let generator = Generator::new(11);
        let piece = (0..4000)
            .find_map(|i| {
                generator
                    .mineshafts
                    .get(IVec2::new(i % 200, i / 200))
                    .and_then(|s| s.pieces.iter().find(|p| p.kind == Kind::Corridor && p.spider).cloned())
            })
            .expect("some mineshaft should have a spider corridor");
        let at = piece.world(1, 0, spider_spawner_z(&piece));
        let cpos = at.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let features = generator.mineshafts.features(&generator, cpos);
        assert!(features.contains(&(at, Feature::Spawner(MobKind::CaveSpider))));
        let local = at - cpos * CHUNK_SIZE_I;
        let generated = generator.generate(cpos);
        if generated.get(local.x as usize, local.y as usize, local.z as usize) == Block::SPAWNER {
            let mut world = super::super::World::new_headless(Arc::new(Generator::new(11)), Default::default(), 1);
            world.register_structure_features(cpos, &generated);
            assert_eq!(world.spawner(at), Some(MobKind::CaveSpider));
        }
    }
}
