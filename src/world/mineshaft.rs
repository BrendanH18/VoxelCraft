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

pub struct Mineshafts {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<Mineshaft>>>>,
}

impl Mineshafts {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    fn frequency_hit(&self, chunk16: IVec2) -> bool {
        hash_f(chunk16.x, 0, chunk16.y, self.seed ^ SALT) < FREQUENCY
    }

    fn get(&self, chunk16: IVec2) -> Option<Arc<Mineshaft>> {
        if let Some(found) = self.cache.lock().unwrap().get(&chunk16) {
            return found.clone();
        }
        let shaft = self.frequency_hit(chunk16).then(|| {
            let rng = Rng(hash3(chunk16.x, 0, chunk16.y, self.seed ^ SALT));
            Arc::new(Mineshaft::generate(rng, chunk16.x * 16 + 2, chunk16.y * 16 + 2))
        });
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT
            && let Some(k) = cache.keys().next().copied()
        {
            cache.remove(&k);
        }
        cache.entry(chunk16).or_insert(shaft).clone()
    }

    fn near(&self, min: IVec2, max: IVec2) -> Vec<Arc<Mineshaft>> {
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
                if let Some(s) = self.get(chunk16)
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

    pub fn nearest(&self, p: IVec3) -> Option<IVec3> {
        let here = IVec2::new(p.x, p.z);
        let origin = here.div_euclid(IVec2::splat(16));
        (0..=48i32).find_map(|ring| {
            (-ring..=ring).flat_map(move |dz| (-ring..=ring).map(move |dx| origin + IVec2::new(dx, dz))).find_map(
                |chunk16| {
                    if dx_ring(chunk16, origin, ring) {
                        self.get(chunk16).map(|s| {
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

    pub fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let solid = |_: i32, _: i32, _: i32, _: i32| false;
        for shaft in self.near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            if shaft.bounds.max.y < base.y || shaft.bounds.min.y > top.y {
                continue;
            }
            for piece in shaft.pieces.iter().filter(|p| p.bounds.intersects(&chunk) || floor_touches(p, &chunk)) {
                Paint { blocks: &mut *blocks, base, top, piece, open: &solid, pillar: LOG }.piece(&shaft.entrances);
            }
        }
    }

    pub fn features(&self, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let mut out = Vec::new();
        for shaft in self.near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
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
/// entries (golden apples, name tags, seeds, glow berries,
/// powered/detector/activator rails) stay weighted blanks.
type Entry = (Option<Item>, u32, u8, u8);
const RARE: &[Entry] = &[
    (None, 20, 1, 1),                 // golden apple
    (None, 1, 1, 1),                  // enchanted golden apple
    (Some(Item::NAME_TAG), 30, 1, 1), // name tag
    (Some(Item::ENCHANTED_BOOK), 10, 1, 1),
    (Some(Item::tool(ToolKind::Pickaxe, Tier::Iron)), 5, 1, 1),
    (None, 5, 1, 1), // empty
];
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
const RAILS: &[Entry] = &[
    (Some(Item::from_block(Block::RAIL)), 20, 4, 8),
    (None, 5, 1, 4), // powered rail
    (None, 5, 1, 4), // detector rail
    (None, 5, 1, 4), // activator rail
    (Some(Item::from_block(Block::TORCH)), 15, 1, 16),
];

fn roll_pool(chest: &mut Chest, rng: &mut Rng, table: &[Entry], rolls: u32) {
    let total: u32 = table.iter().map(|e| e.1).sum();
    for _ in 0..rolls {
        let mut choice = rng.below(total);
        for &(item, weight, lo, hi) in table {
            if choice < weight {
                if let Some(item) = item {
                    let count = rng.range(lo as u32, hi as u32) as u8;
                    let empty: Vec<usize> = (0..super::chest::SLOTS).filter(|&i| chest.slots[i].is_none()).collect();
                    if let Some(&slot) = empty.get(rng.below(empty.len() as u32) as usize) {
                        chest.slots[slot] = Some(crate::inventory::Stack::new(item, count));
                    }
                }
                break;
            }
            choice -= weight;
        }
    }
}

pub fn loot(seed: u64) -> Chest {
    let mut rng = Rng(seed ^ 0x6D69_6E65);
    let mut chest = Chest::default();
    roll_pool(&mut chest, &mut rng, RARE, 1);
    let common = rng.range(2, 4);
    roll_pool(&mut chest, &mut rng, COMMON, common);
    roll_pool(&mut chest, &mut rng, RAILS, 3);
    chest
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
                self.blocks[i] = PLANKS;
            }
        }
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
        self.fill(0, 0, z, 0, 1, z, FENCE);
        self.fill(2, 0, z, 2, 1, z, FENCE);
        if hash3(z, 0, self.piece.variant as i32, self.piece.variant).is_multiple_of(4) {
            self.set(0, 2, z, PLANKS);
            self.set(2, 2, z, PLANKS);
        } else {
            self.fill(0, 2, z, 2, 2, z, PLANKS);
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
            self.fill(x, y0, z, x, y1, z, PLANKS);
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
    use super::*;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    #[test]
    fn frequency_matches_java_legacy_type_3() {
        let m = Mineshafts::new(1);
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
        let m = Mineshafts::new(7);
        let found =
            (0..3000).find_map(|i| m.get(IVec2::new(i, 0))).expect("frequency 0.004 should hit within 3000 chunks");
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
                generator.mineshafts.get(IVec2::new(i, 2)).and_then(|s| {
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
                    let features = generator.mineshafts.features(cpos);
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
        let features = generator.mineshafts.features(cpos);
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
