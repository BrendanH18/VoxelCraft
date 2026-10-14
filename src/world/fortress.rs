//! Nether fortresses, laid out like Java's `NetherFortressPieces`.
//!
//! Each 432-block region uses Java's shared Nether-complex placement:
//! fortress weight 2, bastion weight 3. A fortress layout is a
//! tree of pieces grown from a bridge crossing: bridges with pillars down
//! through the lava sea, small crossings, stair rooms, blaze spawner
//! thrones, and through a castle entrance, enclosed corridors with chests
//! and nether wart rooms. Pieces are picked by Java's weights and limits,
//! may not overlap, stay within 112 blocks of the start and 30 steps deep,
//! and the finished fortress is moved to sit between y = 48 and 70.
//!
//! A layout is a pure function of the seed and region, computed once and
//! cached; each chunk then paints only the pieces that touch it, so
//! generation stays independent per chunk like the rest of the terrain.

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::{Block, Facing};
use super::chest::Chest;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME};
use super::noise::hash3;
use super::structure::{Bounds, LootEntry, Oriented, Paint, Rng, fill_chest};
use crate::entity::MobKind;
use crate::item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind};

/// Region size in blocks: Java's 27 chunks of 16.
pub const REGION: i32 = 432;
/// Starts fall in the first 23 chunks of a region (spacing minus separation).
const SPREAD: u32 = 23;
/// The farthest a start piece's corner lies from its region's corner.
const START_MAX: i32 = (SPREAD as i32 - 1) * 16 + 2;
/// New pieces must start within this many blocks of the start piece.
const REACH: i32 = 112;
/// No fortress block lies farther than this from its start piece's corner.
const EXTENT: i32 = REACH + 19 + 8;
const MAX_DEPTH: u32 = 30;
const SALT: u64 = 30_084_232;
/// Java keeps cached layouts per structure start; this bounds ours.
const CACHE_LIMIT: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    BridgeStraight,
    BridgeEnd,
    BridgeCrossing,
    RoomCrossing,
    StairsRoom,
    Throne,
    Entrance,
    Corridor,
    CorridorCrossing,
    RightTurn,
    LeftTurn,
    CorridorStairs,
    Balcony,
    StalkRoom,
}

impl Kind {
    /// Box size (width, height, depth) and the offset of its corner from
    /// the doorway it grows from, as in Java's `orientBox` calls.
    fn dims(self) -> (IVec3, IVec3) {
        let v = IVec3::new;
        match self {
            Kind::BridgeStraight => (v(5, 10, 19), v(-1, -3, 0)),
            Kind::BridgeEnd => (v(5, 10, 8), v(-1, -3, 0)),
            Kind::BridgeCrossing => (v(19, 10, 19), v(-8, -3, 0)),
            Kind::RoomCrossing => (v(7, 9, 7), v(-2, 0, 0)),
            Kind::StairsRoom => (v(7, 11, 7), v(-2, 0, 0)),
            Kind::Throne => (v(7, 8, 9), v(-2, 0, 0)),
            Kind::Entrance | Kind::StalkRoom => (v(13, 14, 13), v(-5, -3, 0)),
            Kind::Corridor | Kind::CorridorCrossing | Kind::RightTurn | Kind::LeftTurn => (v(5, 7, 5), v(-1, 0, 0)),
            Kind::CorridorStairs => (v(5, 14, 10), v(-1, -7, 0)),
            Kind::Balcony => (v(9, 7, 9), v(-3, 0, 0)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub kind: Kind,
    /// The way the piece runs, from the doorway it was entered by.
    pub facing: Facing,
    pub bounds: Bounds,
    depth: u32,
    /// Seeds the crumbling of bridge ends.
    variant: u64,
    /// Corridor turns that hold a loot chest (one in three).
    chest: bool,
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
    /// May follow a piece of its own kind.
    in_row: bool,
}

impl Weight {
    const fn new(kind: Kind, weight: u32, max: u32, in_row: bool) -> Self {
        Self { kind, weight, max, placed: 0, in_row }
    }

    fn can_place(&self) -> bool {
        self.max == 0 || self.placed < self.max
    }
}

/// Java's `BRIDGE_PIECE_WEIGHTS`.
fn bridge_weights() -> Vec<Weight> {
    vec![
        Weight::new(Kind::BridgeStraight, 30, 0, true),
        Weight::new(Kind::BridgeCrossing, 10, 4, false),
        Weight::new(Kind::RoomCrossing, 10, 4, false),
        Weight::new(Kind::StairsRoom, 10, 3, false),
        Weight::new(Kind::Throne, 5, 2, false),
        Weight::new(Kind::Entrance, 5, 1, false),
    ]
}

/// Java's `CASTLE_PIECE_WEIGHTS`.
fn castle_weights() -> Vec<Weight> {
    vec![
        Weight::new(Kind::Corridor, 25, 0, true),
        Weight::new(Kind::CorridorCrossing, 15, 5, false),
        Weight::new(Kind::RightTurn, 5, 10, false),
        Weight::new(Kind::LeftTurn, 5, 10, false),
        Weight::new(Kind::CorridorStairs, 10, 3, true),
        Weight::new(Kind::Balcony, 7, 2, false),
        Weight::new(Kind::StalkRoom, 5, 2, false),
    ]
}

struct Builder {
    rng: Rng,
    pieces: Vec<Piece>,
    pending: Vec<usize>,
    bridges: Vec<Weight>,
    castle: Vec<Weight>,
    previous: Option<Kind>,
}

impl Builder {
    fn collides(&self, b: &Bounds) -> bool {
        self.pieces.iter().any(|p| p.bounds.intersects(b))
    }

    fn create(&mut self, kind: Kind, door: IVec3, facing: Facing, depth: u32) -> Option<Piece> {
        let (size, off) = kind.dims();
        let bounds = Bounds::oriented(door, off, size, facing);
        if bounds.min.y <= 10 || self.collides(&bounds) {
            return None;
        }
        let variant = self.rng.next_u64();
        let chest = matches!(kind, Kind::RightTurn | Kind::LeftTurn) && self.rng.below(3) == 0;
        Some(Piece { kind, facing, bounds, depth, variant, chest })
    }

    /// Java's `generatePiece`: up to five weighted picks, falling through
    /// to the next kinds when one doesn't fit, else a crumbling bridge end.
    fn pick(&mut self, castle: bool, door: IVec3, facing: Facing, depth: u32) -> Option<Piece> {
        let weights = if castle { &self.castle } else { &self.bridges };
        // Java stops growing once every limited piece is used up.
        let total: u32 = weights.iter().map(|w| w.weight).sum();
        let open = weights.iter().any(|w| w.max > 0 && w.placed < w.max);
        if open && total > 0 && depth <= MAX_DEPTH {
            for _ in 0..5 {
                let mut r = self.rng.below(total) as i32;
                let count = if castle { self.castle.len() } else { self.bridges.len() };
                for i in 0..count {
                    let w = if castle { &self.castle[i] } else { &self.bridges[i] };
                    r -= w.weight as i32;
                    if r >= 0 {
                        continue;
                    }
                    if !w.can_place() || (self.previous == Some(w.kind) && !w.in_row) {
                        break;
                    }
                    let kind = w.kind;
                    if let Some(piece) = self.create(kind, door, facing, depth) {
                        let list = if castle { &mut self.castle } else { &mut self.bridges };
                        list[i].placed += 1;
                        if !list[i].can_place() {
                            list.remove(i);
                        }
                        self.previous = Some(kind);
                        return Some(piece);
                    }
                }
            }
        }
        self.create(Kind::BridgeEnd, door, facing, depth)
    }

    /// Java's `generateAndAddPiece`.
    fn grow(&mut self, (door, facing): (IVec3, Facing), depth: u32, castle: bool) {
        let start = self.pieces[0].bounds.min;
        // Past the reach Java makes a bridge end but never adds it.
        if (door.x - start.x).abs() > REACH || (door.z - start.z).abs() > REACH {
            return;
        }
        if let Some(piece) = self.pick(castle, door, facing, depth + 1) {
            self.pieces.push(piece);
            if piece.kind != Kind::BridgeEnd {
                self.pending.push(self.pieces.len() - 1);
            }
        }
    }

    fn add_children(&mut self, i: usize) {
        let p = self.pieces[i];
        let d = p.depth;
        match p.kind {
            Kind::BridgeStraight => self.grow(p.ahead(1, 3), d, false),
            Kind::BridgeCrossing => {
                self.grow(p.ahead(8, 3), d, false);
                self.grow(p.side(true, 3, 8), d, false);
                self.grow(p.side(false, 3, 8), d, false);
            }
            Kind::RoomCrossing => {
                self.grow(p.ahead(2, 0), d, false);
                self.grow(p.side(true, 0, 2), d, false);
                self.grow(p.side(false, 0, 2), d, false);
            }
            Kind::StairsRoom => self.grow(p.side(false, 6, 2), d, false),
            Kind::Entrance => self.grow(p.ahead(5, 3), d, true),
            Kind::Corridor | Kind::CorridorStairs => self.grow(p.ahead(1, 0), d, true),
            Kind::CorridorCrossing => {
                self.grow(p.ahead(1, 0), d, true);
                self.grow(p.side(true, 0, 1), d, true);
                self.grow(p.side(false, 0, 1), d, true);
            }
            Kind::RightTurn => self.grow(p.side(false, 0, 1), d, true),
            Kind::LeftTurn => self.grow(p.side(true, 0, 1), d, true),
            Kind::Balcony => {
                self.grow(p.side(true, 0, 1), d, true);
                self.grow(p.side(false, 0, 1), d, true);
            }
            Kind::StalkRoom => self.grow(p.ahead(5, 3), d, true),
            Kind::Throne | Kind::BridgeEnd => {}
        }
    }
}

/// One fortress's pieces.
pub struct Fortress {
    pub pieces: Vec<Piece>,
    /// Every piece box, without pillars.
    pub bounds: Bounds,
}

impl Fortress {
    /// Lays out the fortress whose start piece's corner is at `x, z`.
    fn generate(rng: Rng, x: i32, z: i32) -> Fortress {
        let mut b = Builder {
            rng,
            pieces: Vec::new(),
            pending: Vec::new(),
            bridges: bridge_weights(),
            castle: castle_weights(),
            previous: None,
        };
        let facing = Facing::ALL[b.rng.below(4) as usize];
        let min = IVec3::new(x, 64, z);
        let start = Bounds { min, max: min + IVec3::new(18, 9, 18) };
        b.pieces.push(Piece { kind: Kind::BridgeCrossing, facing, bounds: start, depth: 0, variant: 0, chest: false });
        b.add_children(0);
        while !b.pending.is_empty() {
            let i = b.rng.below(b.pending.len() as u32) as usize;
            let piece = b.pending.swap_remove(i);
            b.add_children(piece);
        }
        // Java's `moveInsideHeights(48, 70)`.
        let mut bounds = b.pieces.iter().fold(start, |acc, p| acc.union(&p.bounds));
        let room = 70 - 48 + 1 - (bounds.max.y - bounds.min.y + 1);
        let y = if room > 1 { 48 + b.rng.below(room as u32) as i32 } else { 48 };
        let shift = IVec3::new(0, y - bounds.min.y, 0);
        for p in &mut b.pieces {
            p.bounds = p.bounds.shifted(shift);
        }
        bounds = bounds.shifted(shift);
        Fortress { pieces: b.pieces, bounds }
    }

    /// Whether `p` lies in one of the fortress's pieces (where Java
    /// spawns fortress mobs).
    pub fn holds(&self, p: IVec3) -> bool {
        self.bounds.contains(p) && self.pieces.iter().any(|piece| piece.bounds.contains(p))
    }
}

/// Block entities a fortress generates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feature {
    Spawner(MobKind),
    /// A chest filled from the fortress loot table with this seed.
    Chest(u64),
    /// A chest filled from a stronghold table (see `stronghold::loot`).
    StrongholdChest(u64),
    /// A chest filled from the simple dungeon loot table.
    DungeonChest(u64),
    /// A chest filled from the abandoned mineshaft table.
    MineshaftChest(u64),
    BastionChest(u64, super::bastion::ChestKind),
    VillageChest(u64, super::village::Style),
    VillageHome(IVec3),
    VillageWorkstation(Block),
    /// A chest or dispenser filled from a v0.6 structure's table.
    TempleChest(u64, super::temples::LootTable),
    /// Functional furnace or brewing stand in a non-village structure.
    UtilityBlock(Block),
    IglooBrewing,
}

/// Fortress layouts for one Nether, cached by region.
pub struct Fortresses {
    seed: u64,
    pub bastions: super::bastion::Bastions,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<Fortress>>>>,
}

impl Fortresses {
    pub fn new(seed: u64) -> Self {
        Self { seed, bastions: super::bastion::Bastions::new(seed), cache: Mutex::new(FxHashMap::default()) }
    }

    fn rng(&self, region: IVec2) -> Rng {
        Rng(hash3(region.x, 0, region.y, self.seed ^ SALT))
    }

    /// Corner of the start piece in `region`: the block (2, 2) of a chunk
    /// picked among the first 23 of each axis, like Java's random spread.
    fn start(&self, region: IVec2) -> (IVec2, Rng) {
        (super::nether_complexes::placement(self.seed, region).0, self.rng(region))
    }

    /// The fortress of `region`, if any.
    pub fn get(&self, region: IVec2) -> Option<Arc<Fortress>> {
        if let Some(f) = self.cache.lock().unwrap().get(&region) {
            return f.clone();
        }
        if super::nether_complexes::placement(self.seed, region).1 != super::nether_complexes::Complex::Fortress {
            return None;
        }
        let (start, rng) = self.start(region);
        let fortress = Some(Arc::new(Fortress::generate(rng, start.x, start.y)));
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.entry(region).or_insert(fortress).clone()
    }

    /// Centre of the fortress nearest `p`, searching the current placement
    /// region and its neighbours.
    pub fn nearest(&self, p: IVec2) -> Option<IVec3> {
        let region = p.div_euclid(IVec2::splat(REGION));
        (-2..=2)
            .flat_map(|z| (-2..=2).map(move |x| region + IVec2::new(x, z)))
            .filter_map(|region| self.get(region))
            .map(|fortress| {
                let b = fortress.bounds;
                IVec3::new((b.min.x + b.max.x) / 2, b.min.y, (b.min.z + b.max.z) / 2)
            })
            .min_by_key(|at| {
                let delta = IVec2::new(at.x, at.z) - p;
                delta.as_i64vec2().length_squared()
            })
    }

    /// Fortresses with blocks in the columns from `min` to `max` (x and z).
    pub fn near(&self, min: IVec2, max: IVec2) -> Vec<Arc<Fortress>> {
        let lo = (min - EXTENT - START_MAX).div_euclid(IVec2::splat(REGION));
        let hi = (max + EXTENT).div_euclid(IVec2::splat(REGION));
        let mut out = Vec::new();
        for rz in lo.y..=hi.y {
            for rx in lo.x..=hi.x {
                let region = IVec2::new(rx, rz);
                let (start, _) = self.start(region);
                // Cheap reject before building the layout.
                if (start - EXTENT).cmpgt(max).any() || (start + EXTENT).cmplt(min).any() {
                    continue;
                }
                if let Some(f) = self.get(region)
                    && f.bounds.min.x <= max.x
                    && f.bounds.max.x >= min.x
                    && f.bounds.min.z <= max.y
                    && f.bounds.max.z >= min.y
                {
                    out.push(f);
                }
            }
        }
        out
    }

    /// Paints the fortress pieces touching the chunk at `base` into
    /// `blocks`. `open(x, z, top, bottom)` tells whether the terrain leaves
    /// a column open (air or lava) from `top` down to `bottom`, for
    /// pillars that start above the chunk.
    pub fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, open: &dyn Fn(i32, i32, i32, i32) -> bool) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        for fortress in self.near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            for piece in &fortress.pieces {
                let b = &piece.bounds;
                // Pillars reach down from the pieces to the Nether floor.
                if b.min.x > top.x || b.max.x < base.x || b.min.z > top.z || b.max.z < base.z || b.max.y < base.y {
                    continue;
                }
                if b.min.y > top.y && !has_pillars(piece.kind) {
                    continue;
                }
                Paint { blocks: &mut *blocks, base, top, piece, open, pillar: BRICKS }.piece();
            }
        }
    }

    /// Paints just one terrain column with the same clipping and pillar stops
    /// as `paint`. Ore checks can sample neighbours without building their chunks.
    pub fn column_at(
        &self,
        x: i32,
        z: i32,
        base_y: i32,
        terrain: [Block; CHUNK_SIZE],
        open: &dyn Fn(i32, i32, i32, i32) -> bool,
    ) -> [Block; CHUNK_SIZE] {
        let mut cells = terrain;
        let base = IVec3::new(x, base_y, z);
        let top = IVec3::new(x, base_y + CHUNK_SIZE_I - 1, z);
        for fortress in self.near(IVec2::new(x, z), IVec2::new(x, z)) {
            for piece in &fortress.pieces {
                let b = &piece.bounds;
                if x < b.min.x || x > b.max.x || z < b.min.z || z > b.max.z || b.max.y < base.y {
                    continue;
                }
                if b.min.y > top.y && !has_pillars(piece.kind) {
                    continue;
                }
                Paint { blocks: &mut cells, base, top, piece, open, pillar: BRICKS }.piece();
            }
        }
        cells
    }

    /// Spawners and chests the fortresses put in the chunk at `cpos`.
    pub fn features(&self, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let mut out = self.bastions.features(cpos);
        for fortress in self.near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            for piece in fortress.pieces.iter().filter(|p| p.bounds.intersects(&chunk)) {
                let found = match piece.kind {
                    Kind::Throne => Some((piece.world(3, 5, 5), Feature::Spawner(MobKind::Blaze))),
                    Kind::RightTurn if piece.chest => Some((piece.world(1, 2, 3), Feature::Chest(piece.variant))),
                    Kind::LeftTurn if piece.chest => Some((piece.world(3, 2, 3), Feature::Chest(piece.variant))),
                    _ => None,
                };
                out.extend(found.filter(|(p, _)| chunk.contains(*p)));
            }
        }
        out
    }

    /// Whether `p` is inside a fortress piece.
    pub fn inside(&self, p: IVec3) -> bool {
        let c = IVec2::new(p.x, p.z);
        self.near(c, c).iter().any(|f| f.holds(p))
    }
}

fn has_pillars(kind: Kind) -> bool {
    !matches!(kind, Kind::CorridorCrossing | Kind::RightTurn | Kind::LeftTurn | Kind::BridgeEnd)
}

/// Java's nether bridge chest loot (2-4 rolls), including saddles and horse armor.
pub fn loot(seed: u64) -> Chest {
    let gold = |kind| Item::tool(kind, Tier::Gold);
    let table: [LootEntry; 12] = [
        (Item::SADDLE, 10, 1, 1),
        (Item::GOLDEN_HORSE_ARMOR, 8, 1, 1),
        (Item::IRON_HORSE_ARMOR, 5, 1, 1),
        (Item::DIAMOND_HORSE_ARMOR, 3, 1, 1),
        (Item::DIAMOND, 5, 1, 3),
        (Item::IRON_INGOT, 5, 1, 5),
        (Item::GOLD_INGOT, 15, 1, 3),
        (gold(ToolKind::Sword), 5, 1, 1),
        (Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Gold), 5, 1, 1),
        (Item::FLINT_AND_STEEL, 5, 1, 1),
        (Item::NETHER_WART, 5, 3, 7),
        (Item::from_block(Block::OBSIDIAN), 2, 2, 4),
    ];
    fill_chest(seed, (2, 4), &table)
}

const BRICKS: Block = Block::NETHER_BRICKS;
const FENCE: Block = Block::NETHER_BRICK_FENCE;
const AIR: Block = Block::AIR;

impl Paint<'_, Piece> {
    fn piece(&mut self) {
        match self.piece.kind {
            Kind::BridgeStraight => self.bridge_straight(),
            Kind::BridgeEnd => self.bridge_end(),
            Kind::BridgeCrossing => self.bridge_crossing(),
            Kind::RoomCrossing => self.room_crossing(),
            Kind::StairsRoom => self.stairs_room(),
            Kind::Throne => self.throne(),
            Kind::Entrance => self.entrance(),
            Kind::Corridor => self.corridor(),
            Kind::CorridorCrossing => self.corridor_crossing(),
            Kind::RightTurn => self.turn(false),
            Kind::LeftTurn => self.turn(true),
            Kind::CorridorStairs => self.corridor_stairs(),
            Kind::Balcony => self.balcony(),
            Kind::StalkRoom => self.stalk_room(),
        }
    }

    fn bridge_straight(&mut self) {
        self.fill(0, 3, 0, 4, 4, 18, BRICKS);
        self.fill(1, 5, 0, 3, 7, 18, AIR);
        self.fill(0, 5, 0, 0, 5, 18, BRICKS);
        self.fill(4, 5, 0, 4, 5, 18, BRICKS);
        self.fill(0, 2, 0, 4, 2, 5, BRICKS);
        self.fill(0, 2, 13, 4, 2, 18, BRICKS);
        self.fill(0, 0, 0, 4, 1, 3, BRICKS);
        self.fill(0, 0, 15, 4, 1, 18, BRICKS);
        self.columns_down(0, 0, 4, 2);
        self.columns_down(0, 16, 4, 18);
        for x in [0, 4] {
            self.fill(x, 1, 1, x, 4, 1, FENCE);
            self.fill(x, 3, 4, x, 4, 4, FENCE);
            self.fill(x, 3, 14, x, 4, 14, FENCE);
            self.fill(x, 1, 17, x, 4, 17, FENCE);
        }
    }

    /// A broken-off bridge: rows of random length.
    fn bridge_end(&mut self) {
        let mut rng = Rng(self.piece.variant);
        for x in 0..=4 {
            for y in 3..=4 {
                let len = rng.below(8) as i32;
                self.fill(x, y, 0, x, y, len, BRICKS);
            }
        }
        let len = rng.below(8) as i32;
        self.fill(0, 5, 0, 0, 5, len, BRICKS);
        let len = rng.below(8) as i32;
        self.fill(4, 5, 0, 4, 5, len, BRICKS);
        for x in 0..=4 {
            let len = rng.below(5) as i32;
            self.fill(x, 2, 0, x, 2, len, BRICKS);
        }
        for x in 0..=4 {
            for y in 0..=1 {
                let len = rng.below(3) as i32;
                self.fill(x, y, 0, x, y, len, BRICKS);
            }
        }
    }

    fn bridge_crossing(&mut self) {
        self.fill(7, 3, 0, 11, 4, 18, BRICKS);
        self.fill(0, 3, 7, 18, 4, 11, BRICKS);
        self.fill(8, 5, 0, 10, 7, 18, AIR);
        self.fill(0, 5, 8, 18, 7, 10, AIR);
        for (a, b) in [(0, 7), (11, 18)] {
            self.fill(7, 5, a, 7, 5, b, BRICKS);
            self.fill(11, 5, a, 11, 5, b, BRICKS);
            self.fill(a, 5, 7, b, 5, 7, BRICKS);
            self.fill(a, 5, 11, b, 5, 11, BRICKS);
        }
        self.fill(7, 2, 0, 11, 2, 5, BRICKS);
        self.fill(7, 2, 13, 11, 2, 18, BRICKS);
        self.fill(7, 0, 0, 11, 1, 3, BRICKS);
        self.fill(7, 0, 15, 11, 1, 18, BRICKS);
        self.columns_down(7, 0, 11, 2);
        self.columns_down(7, 16, 11, 18);
        self.fill(0, 2, 7, 5, 2, 11, BRICKS);
        self.fill(13, 2, 7, 18, 2, 11, BRICKS);
        self.fill(0, 0, 7, 3, 1, 11, BRICKS);
        self.fill(15, 0, 7, 18, 1, 11, BRICKS);
        self.columns_down(0, 7, 2, 11);
        self.columns_down(16, 7, 18, 11);
    }

    /// An open platform on the bridges with corner posts and fenced arches
    /// on all four sides.
    fn room_crossing(&mut self) {
        self.fill(0, 0, 0, 6, 1, 6, BRICKS);
        self.fill(0, 2, 0, 6, 7, 6, AIR);
        for (x0, x1) in [(0, 1), (5, 6)] {
            for z in [0, 6] {
                self.fill(x0, 2, z, x1, 6, z, BRICKS);
            }
        }
        for x in [0, 6] {
            self.fill(x, 2, 0, x, 6, 1, BRICKS);
            self.fill(x, 2, 5, x, 6, 6, BRICKS);
            self.fill(x, 5, 2, x, 6, 4, FENCE);
        }
        for z in [0, 6] {
            self.fill(2, 5, z, 4, 6, z, FENCE);
        }
        self.columns_down(0, 0, 6, 6);
    }

    /// A walled room whose brick steps climb to a landing; the way on
    /// leaves from the top of the right wall.
    fn stairs_room(&mut self) {
        self.fill(0, 0, 0, 6, 1, 6, BRICKS);
        self.fill(0, 2, 0, 6, 10, 6, AIR);
        self.fill(0, 2, 0, 1, 8, 0, BRICKS);
        self.fill(5, 2, 0, 6, 8, 0, BRICKS);
        self.fill(0, 2, 1, 0, 8, 6, BRICKS);
        self.fill(6, 2, 1, 6, 8, 6, BRICKS);
        self.fill(1, 2, 6, 5, 8, 6, BRICKS);
        self.fill(0, 3, 2, 0, 5, 4, FENCE);
        self.fill(6, 3, 2, 6, 5, 2, FENCE);
        self.fill(6, 3, 4, 6, 5, 4, FENCE);
        for (i, x) in (1..=5).rev().enumerate() {
            self.fill(x, 2, 5, x, 2 + i as i32, 5, BRICKS);
        }
        self.fill(1, 7, 1, 5, 7, 4, BRICKS);
        self.fill(6, 8, 2, 6, 8, 4, AIR);
        self.fill(2, 6, 0, 4, 8, 0, BRICKS);
        self.fill(2, 5, 0, 4, 5, 0, FENCE);
        self.columns_down(0, 0, 6, 6);
    }

    /// Raised steps up to a blaze spawner behind fences.
    fn throne(&mut self) {
        self.fill(0, 2, 0, 6, 7, 7, AIR);
        self.fill(1, 0, 0, 5, 1, 7, BRICKS);
        self.fill(1, 2, 1, 5, 2, 7, BRICKS);
        self.fill(1, 3, 2, 5, 3, 7, BRICKS);
        self.fill(1, 4, 3, 5, 4, 7, BRICKS);
        self.fill(1, 2, 0, 1, 4, 2, FENCE);
        self.fill(5, 2, 0, 5, 4, 2, FENCE);
        self.fill(1, 5, 2, 1, 5, 3, FENCE);
        self.fill(5, 5, 2, 5, 5, 3, FENCE);
        self.fill(0, 5, 3, 0, 6, 8, FENCE);
        self.fill(6, 5, 3, 6, 6, 8, FENCE);
        self.fill(1, 5, 8, 5, 7, 8, FENCE);
        self.set(1, 6, 3, FENCE);
        self.set(5, 6, 3, FENCE);
        self.set(3, 5, 5, Block::SPAWNER);
        self.columns_down(1, 0, 5, 7);
    }

    /// The tall hall shared by the castle entrance and the nether wart room.
    fn hall(&mut self) {
        self.fill(0, 3, 0, 12, 4, 12, BRICKS);
        self.fill(0, 5, 0, 12, 13, 12, AIR);
        self.fill(0, 5, 0, 1, 12, 12, BRICKS);
        self.fill(11, 5, 0, 12, 12, 12, BRICKS);
        for (z0, z1) in [(0, 1), (11, 12)] {
            self.fill(2, 5, z0, 4, 12, z1, BRICKS);
            self.fill(8, 5, z0, 10, 12, z1, BRICKS);
            self.fill(5, 9, z0, 7, 12, z1, BRICKS);
            self.fill(5, 8, z0, 7, 8, z0, FENCE);
        }
        for i in (3..=9).step_by(2) {
            for x in [0, 12] {
                self.fill(x, 9, i, x, 10, i, FENCE);
            }
            for z in [0, 12] {
                if !(5..=7).contains(&i) {
                    self.fill(i, 9, z, i, 10, z, FENCE);
                }
            }
        }
        self.fill(2, 12, 2, 10, 12, 10, BRICKS);
        self.fill(4, 2, 0, 8, 2, 12, BRICKS);
        self.fill(0, 2, 4, 12, 2, 8, BRICKS);
        self.fill(4, 0, 0, 8, 1, 3, BRICKS);
        self.fill(4, 0, 9, 8, 1, 12, BRICKS);
        self.fill(0, 0, 4, 3, 1, 8, BRICKS);
        self.fill(9, 0, 4, 12, 1, 8, BRICKS);
        self.columns_down(4, 0, 8, 2);
        self.columns_down(4, 10, 8, 12);
        self.columns_down(0, 4, 2, 8);
        self.columns_down(10, 4, 12, 8);
    }

    /// The hall where the bridges give way to the castle, around a lava well.
    fn entrance(&mut self) {
        self.hall();
        self.fill(5, 5, 5, 7, 5, 7, BRICKS);
        self.fill(6, 1, 6, 6, 4, 6, AIR);
        self.set(6, 0, 6, BRICKS);
        self.set(6, 5, 6, Block::LAVA);
    }

    /// Nether wart beds on soul sand either side of a walkway.
    fn stalk_room(&mut self) {
        self.hall();
        for x0 in [2, 8] {
            self.fill(x0, 4, 3, x0 + 2, 4, 9, Block::SOUL_SAND);
            self.fill(x0, 5, 3, x0 + 2, 5, 9, Block::nether_wart(0));
        }
        // Stairs up onto the walkway between the beds.
        self.fill(5, 5, 2, 7, 5, 2, self.stairs(BRICKS, Facing::South));
        self.fill(5, 5, 3, 7, 5, 9, BRICKS);
        self.fill(5, 5, 10, 7, 5, 10, self.stairs(BRICKS, Facing::North));
    }

    /// The common shell of a castle corridor: floor, ceiling, open inside.
    fn corridor_shell(&mut self) {
        self.fill(0, 0, 0, 4, 1, 4, BRICKS);
        self.fill(0, 2, 0, 4, 5, 4, AIR);
        self.fill(0, 6, 0, 4, 6, 4, BRICKS);
    }

    fn corridor(&mut self) {
        self.corridor_shell();
        for x in [0, 4] {
            self.fill(x, 2, 0, x, 5, 4, BRICKS);
            self.fill(x, 3, 1, x, 4, 1, FENCE);
            self.fill(x, 3, 3, x, 4, 3, FENCE);
        }
        self.columns_down(0, 0, 4, 4);
    }

    fn corridor_crossing(&mut self) {
        self.corridor_shell();
        for x in [0, 4] {
            for z in [0, 4] {
                self.fill(x, 2, z, x, 5, z, BRICKS);
            }
        }
    }

    /// A corner; one in three holds a loot chest against the far wall.
    fn turn(&mut self, left: bool) {
        self.corridor_shell();
        let (wall, open) = if left { (4, 0) } else { (0, 4) };
        self.fill(wall, 2, 0, wall, 5, 4, BRICKS);
        self.fill(wall, 3, 1, wall, 4, 1, FENCE);
        self.fill(wall, 3, 3, wall, 4, 3, FENCE);
        self.fill(open, 2, 0, open, 5, 0, BRICKS);
        self.fill(0, 2, 4, 4, 5, 4, BRICKS);
        self.fill(1, 3, 4, 1, 4, 4, FENCE);
        self.fill(3, 3, 4, 3, 4, 4, FENCE);
        if self.piece.chest {
            let (x, front) = if left { (3, Facing::West) } else { (1, Facing::East) };
            self.set(x, 2, 3, self.facing(Block::CHEST, front));
        }
    }

    /// Seven steps down over ten blocks.
    fn corridor_stairs(&mut self) {
        let steps = self.stairs(BRICKS, Facing::North);
        for z in 0..=9 {
            let floor = (7 - z).max(1);
            let ceiling = (floor + 5).max(14 - z).min(13);
            self.fill(0, 0, z, 4, floor, z, BRICKS);
            self.fill(1, floor + 1, z, 3, ceiling - 1, z, AIR);
            if z <= 6 {
                self.fill(1, floor + 1, z, 3, floor + 1, z, steps);
            }
            self.fill(0, ceiling, z, 4, ceiling, z, BRICKS);
            self.fill(0, floor + 1, z, 0, ceiling - 1, z, BRICKS);
            self.fill(4, floor + 1, z, 4, ceiling - 1, z, BRICKS);
            if z % 2 == 0 {
                self.fill(0, floor + 2, z, 0, floor + 3, z, FENCE);
                self.fill(4, floor + 2, z, 4, floor + 3, z, FENCE);
            }
            for x in 0..=4 {
                self.column_down(x, -1, z);
            }
        }
    }

    /// A T junction with a fenced balcony looking out ahead.
    fn balcony(&mut self) {
        self.fill(0, 0, 0, 8, 1, 8, BRICKS);
        self.fill(0, 2, 0, 8, 5, 8, AIR);
        self.fill(0, 6, 0, 8, 6, 5, BRICKS);
        self.fill(0, 2, 0, 2, 5, 0, BRICKS);
        self.fill(6, 2, 0, 8, 5, 0, BRICKS);
        self.fill(1, 3, 0, 1, 4, 0, FENCE);
        self.fill(7, 3, 0, 7, 4, 0, FENCE);
        self.fill(0, 2, 4, 8, 2, 8, BRICKS);
        self.fill(1, 1, 4, 2, 2, 4, AIR);
        self.fill(6, 1, 4, 7, 2, 4, AIR);
        self.fill(0, 3, 8, 8, 3, 8, FENCE);
        self.fill(0, 3, 6, 0, 3, 7, FENCE);
        self.fill(8, 3, 6, 8, 3, 7, FENCE);
        self.fill(0, 3, 4, 0, 5, 5, BRICKS);
        self.fill(8, 3, 4, 8, 5, 5, BRICKS);
        self.fill(1, 3, 5, 2, 5, 5, BRICKS);
        self.fill(6, 3, 5, 7, 5, 5, BRICKS);
        self.fill(1, 4, 5, 1, 5, 5, FENCE);
        self.fill(7, 4, 5, 7, 5, 5, FENCE);
        self.columns_down(0, 0, 8, 5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::index;

    fn layouts(n: i32) -> Vec<Arc<Fortress>> {
        let f = Fortresses::new(42);
        (0..n).filter_map(|i| f.get(IVec2::new(i, -i))).collect()
    }

    #[test]
    fn layouts_are_deterministic_and_never_overlap() {
        let a = Fortresses::new(9);
        let b = Fortresses::new(9);
        for r in [IVec2::ZERO, IVec2::new(-3, 5)] {
            let (fa, fb) = (a.get(r), b.get(r));
            assert_eq!(fa.is_some(), fb.is_some());
            let (Some(fa), Some(fb)) = (fa, fb) else {
                continue;
            };
            assert_eq!(fa.pieces.len(), fb.pieces.len());
            assert!(fa.pieces.iter().zip(&fb.pieces).all(|(p, q)| p.bounds == q.bounds && p.kind == q.kind));
        }
        for f in layouts(12) {
            for (i, p) in f.pieces.iter().enumerate() {
                for q in &f.pieces[i + 1..] {
                    assert!(!p.bounds.intersects(&q.bounds), "{p:?} overlaps {q:?}");
                }
                assert!(p.bounds.min.y > 10 && f.bounds.contains(p.bounds.min) && f.bounds.contains(p.bounds.max));
            }
            assert!(f.bounds.min.y >= 48 && f.bounds.min.y <= 70, "{:?}", f.bounds);
            let start = f.pieces[0].bounds.min;
            assert!((f.bounds.min - start).abs().max_element() <= EXTENT);
            assert!((f.bounds.max - start).abs().max_element() <= EXTENT);
        }
    }

    #[test]
    fn sampled_columns_stop_pillars_at_glowstone_like_chunk_painting() {
        let f = Fortresses::new(42);
        let fortress = f.get(IVec2::ZERO).unwrap();
        let piece = &fortress.pieces[0];
        assert_eq!(piece.kind, Kind::BridgeCrossing);
        let start = piece.world(8, -1, 1);
        let stop = start - IVec3::Y * 2;
        let base = start - IVec3::new(5, 10, 5);
        let local = stop - base;
        let mut blocks = Box::new([Block::AIR; CHUNK_VOLUME]);
        blocks[index(local.x as usize, local.y as usize, local.z as usize)] = Block::GLOWSTONE;
        f.paint(&mut blocks, base, &|_, _, _, _| true);
        let mut terrain = [Block::AIR; CHUNK_SIZE];
        terrain[local.y as usize] = Block::GLOWSTONE;
        let sampled = f.column_at(stop.x, stop.z, base.y, terrain, &|_, _, _, _| true);
        for (y, &block) in sampled.iter().enumerate() {
            assert_eq!(block, blocks[index(local.x as usize, y, local.z as usize)], "column y={y}");
        }
        assert_eq!(sampled[local.y as usize], Block::GLOWSTONE);
        assert_eq!(sampled[local.y as usize - 1], Block::AIR, "pillar stops above the glowstone");
        assert_eq!(sampled[local.y as usize + 1], Block::NETHER_BRICKS);
    }

    #[test]
    fn fortresses_have_bridges_castles_and_thrones() {
        let all = layouts(24);
        let count = |k: Kind| all.iter().map(|f| f.pieces.iter().filter(|p| p.kind == k).count()).sum::<usize>();
        // Limits from Java's weights.
        for f in &all {
            let n = |k: Kind| f.pieces.iter().filter(|p| p.kind == k).count();
            assert!(n(Kind::Throne) <= 2 && n(Kind::Entrance) <= 1 && n(Kind::StalkRoom) <= 2);
            assert!(f.pieces.len() > 5, "tiny fortress: {} pieces", f.pieces.len());
        }
        for k in [Kind::BridgeStraight, Kind::Throne, Kind::Entrance, Kind::Corridor, Kind::StalkRoom] {
            assert!(count(k) > 0, "no {k:?} in 24 fortresses");
        }
        assert!(all.iter().flat_map(|f| &f.pieces).any(|p| p.chest), "no chests");
    }

    #[test]
    fn doorways_line_up_with_their_children() {
        // Every non-start piece's doorway cell sits just outside a wall of
        // an earlier piece, whichever way it faces.
        for f in layouts(8) {
            for (i, p) in f.pieces.iter().enumerate().skip(1) {
                let (_, off) = p.kind.dims();
                let door = p.world(-off.x, -off.y, 0) - p.turn(Facing::South).offset();
                assert!(
                    f.pieces[..i].iter().any(|q| q.bounds.contains(door)),
                    "{:?} facing {:?} at {:?} has no parent",
                    p.kind,
                    p.facing,
                    p.bounds
                );
            }
        }
    }

    #[test]
    fn loot_follows_the_table() {
        for seed in 0..200u64 {
            let chest = loot(seed);
            let stacks: Vec<_> = chest.slots.iter().flatten().collect();
            assert!((2..=4).contains(&stacks.len()), "{stacks:?}");
            for s in stacks {
                let ok = match s.item {
                    Item::DIAMOND | Item::GOLD_INGOT => (1..=3).contains(&s.count),
                    Item::IRON_INGOT => (1..=5).contains(&s.count),
                    Item::NETHER_WART => (3..=7).contains(&s.count),
                    i if i == Item::from_block(Block::OBSIDIAN) => (2..=4).contains(&s.count),
                    _ => s.count == 1,
                };
                assert!(ok, "{s:?}");
            }
        }
    }
}
