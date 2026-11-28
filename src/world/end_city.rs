//! v0.16 Track A: End cities and End ships (GEN-05h), in generation 5's End.
//!
//! Placement and assembly follow Java 26.3:
//!
//! - `StructureSets.END_CITIES`: a triangular random spread of spacing 20,
//!   separation 11 and salt 10387313 (16-block chunks).
//! - `EndCityStructure.findGenerationPoint`: block (7, 7) of the start
//!   chunk, where a `#has_structure/end_city` biome (End highlands or
//!   midlands) can exist; the rotation is the first draw of
//!   `setLargeFeatureSeed`; the start y is the lowest `WORLD_SURFACE_WG`
//!   height of four corners five blocks apart turned by the rotation; a
//!   start below y 60 is dropped, then the start's own biome must be in the
//!   tag (`Structure.isValidBiome`).
//! - `EndCityPieces`: `startHouseTower` and the four section generators
//!   (house tower, tower, tower bridge, fat tower) with their offsets,
//!   rotations, depth limit 8, the ship's `nextInt(10 - genDepth)` chance
//!   and one ship per city, and `recursiveChildren`' collision rule (a child
//!   may only overlap pieces of its parent's generation tag), all drawn from
//!   the same `WorldgenRandom` sequence, so a start lays out the same pieces
//!   Java's would.
//! - `EndCityPiece`: overwrite pieces store air, the others
//!   (`STRUCTURE_AND_AIR`) leave what is there; data markers make loot
//!   chests one block below (`chests/end_city_treasure`), shulkers
//!   (`Sentry`) and the ship's elytra item frame (`Elytra`).
//!
//! The templates are original (fidelity exception FE-02, see
//! [`templates`]): each has the size of Java's template of that name and
//! Java's markers; the blocks are our own design of the same piece kind.
//! Because piece boxes and collisions depend only on template sizes, the
//! pieces themselves are Java's. See
//! `docs/verification/2026-11-20-v0.16-outer-end.md`.

pub mod templates;

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, index};
use super::fortress::Feature;
use super::mansion::{Rotation, large_feature_random};
use super::noise::hash3;
use super::structure::Bounds;
use crate::enchant::JavaRandom;
use templates::{Marker, TEMPLATES, template, template_index};

/// `StructureSets.END_CITIES`, in 16-block chunks.
pub const SPACING: i32 = 20;
pub const SEPARATION: i32 = 11;
pub const SALT: u64 = 10_387_313;
/// `EndCityStructure.START_OFFSET_IN_CHUNK`.
pub const START_OFFSET: i32 = 7;
/// Starts lower than this are dropped (`findGenerationPoint`).
pub const MIN_START_Y: i32 = 60;
/// `EndCityPieces.MAX_GEN_DEPTH`.
pub const MAX_GEN_DEPTH: i32 = 8;
/// The farthest a city's pieces reach from its start, horizontally; the
/// tests check every generated city stays inside it.
pub const REACH: i32 = 176;

/// Where a city starts: its chunk, origin and rotation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Start {
    pub chunk: IVec2,
    pub origin: IVec3,
    pub rotation: Rotation,
}

/// The start chunk of placement region `region`
/// (`RandomSpreadStructurePlacement.getPotentialStructureChunk`, triangular).
pub fn start_chunk(seed: u64, region: IVec2) -> IVec2 {
    let s = seed
        .wrapping_add((region.x as i64).wrapping_mul(341_873_128_712) as u64)
        .wrapping_add((region.y as i64).wrapping_mul(132_897_987_541) as u64)
        .wrapping_add(SALT);
    let mut rng = JavaRandom::new(s as i64);
    let span = SPACING - SEPARATION;
    let ox = (rng.next_bounded(span) + rng.next_bounded(span)) / 2;
    let oz = (rng.next_bounded(span) + rng.next_bounded(span)) / 2;
    region * SPACING + IVec2::new(ox, oz)
}

/// What a start needs from the terrain: the biome rule and the first free
/// height of a column (`WORLD_SURFACE_WG`: the top block + 1, or the
/// dimension's floor where there is no land).
pub trait Terrain {
    /// Whether `#has_structure/end_city` holds at block `(x, z)`.
    fn city_biome(&self, x: i32, z: i32) -> bool;
    fn first_free(&self, x: i32, z: i32) -> i32;
}

/// `EndCityStructure.findGenerationPoint` and `isValidBiome` at `chunk`.
pub fn start(seed: u64, t: &impl Terrain, chunk: IVec2) -> Option<Start> {
    let (x, z) = (chunk.x * 16 + START_OFFSET, chunk.y * 16 + START_OFFSET);
    if !t.city_biome(x, z) {
        return None;
    }
    let mut rng = large_feature_random(seed, chunk);
    let rotation = Rotation::ALL[rng.next_bounded(4) as usize];
    let (ox, oz) = match rotation {
        Rotation::None => (5, 5),
        Rotation::Clockwise90 => (-5, 5),
        Rotation::Clockwise180 => (-5, -5),
        Rotation::Counterclockwise90 => (5, -5),
    };
    let y = [(x, z), (x, z + oz), (x + ox, z), (x + ox, z + oz)].into_iter().map(|(x, z)| t.first_free(x, z)).min()?;
    if y < MIN_START_Y {
        return None;
    }
    t.city_biome(x, z).then_some(Start { chunk, origin: IVec3::new(x, y, z), rotation })
}

/// One `EndCityPiece`: its template, the world position of its template
/// origin, rotation, whether its air is stored (`OW`) and its box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub template: u8,
    pub pos: IVec3,
    pub rotation: Rotation,
    pub overwrite: bool,
    pub bounds: Bounds,
    /// `StructurePiece.genDepth`, used as the generation tag.
    gen_depth: i32,
}

impl Piece {
    fn new(name: &str, pos: IVec3, rotation: Rotation, overwrite: bool) -> Piece {
        let template = template_index(name).expect("an End city template");
        let size = IVec3::from(TEMPLATES[template as usize].size);
        let (a, b) = (rotation.pos(IVec3::ZERO), rotation.pos(size - IVec3::ONE));
        Piece {
            template,
            pos,
            rotation,
            overwrite,
            bounds: Bounds { min: pos + a.min(b), max: pos + a.max(b) },
            gen_depth: 0,
        }
    }

    pub fn name(&self) -> &'static str {
        TEMPLATES[self.template as usize].name
    }

    /// World position of template cell `local`.
    pub fn world(&self, local: IVec3) -> IVec3 {
        self.pos + self.rotation.pos(local)
    }

    /// Template cell at `world` (the inverse of [`Piece::world`]).
    pub fn local(&self, world: IVec3) -> IVec3 {
        self.rotation.then(self.rotation).then(self.rotation).pos(world - self.pos)
    }
}

/// `EndCityPieces.addPiece`: a child placed `offset` from `parent` turned
/// by the parent's rotation (`calculateConnectedPosition`).
fn add_piece(parent: Piece, offset: IVec3, name: &str, rotation: Rotation, overwrite: bool) -> Piece {
    Piece::new(name, parent.pos + parent.rotation.pos(offset), rotation, overwrite)
}

/// `EndCityPieces`' section generators.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    HouseTower,
    Tower,
    TowerBridge,
    FatTower,
}

/// `TOWER_BRIDGES`: rotation and offset of each bridge off a tower piece.
const TOWER_BRIDGES: [(Rotation, IVec3); 4] = [
    (Rotation::None, IVec3::new(1, -1, 0)),
    (Rotation::Clockwise90, IVec3::new(6, -1, 1)),
    (Rotation::Counterclockwise90, IVec3::new(0, -1, 5)),
    (Rotation::Clockwise180, IVec3::new(5, -1, 6)),
];
/// `FAT_TOWER_BRIDGES`.
const FAT_TOWER_BRIDGES: [(Rotation, IVec3); 4] = [
    (Rotation::None, IVec3::new(4, -1, 0)),
    (Rotation::Clockwise90, IVec3::new(12, -1, 4)),
    (Rotation::Counterclockwise90, IVec3::new(0, -1, 8)),
    (Rotation::Clockwise180, IVec3::new(8, -1, 12)),
];

struct Builder<'a> {
    rng: &'a mut JavaRandom,
    /// `TOWER_BRIDGE_GENERATOR.shipCreated`: set even when the ship's
    /// branch is later dropped for a collision, as in Java.
    ship_created: bool,
}

fn push(pieces: &mut Vec<Piece>, p: Piece) -> usize {
    pieces.push(p);
    pieces.len() - 1
}

/// `addHelper(pieces, addPiece(pieces[from], …))`: the new piece's index.
fn chain(
    pieces: &mut Vec<Piece>,
    from: usize,
    offset: IVec3,
    name: &str,
    rotation: Rotation,
    overwrite: bool,
) -> usize {
    let p = add_piece(pieces[from], offset, name, rotation, overwrite);
    push(pieces, p)
}

impl Builder<'_> {
    /// `recursiveChildren`. `parent` indexes `pieces`.
    fn recursive_children(
        &mut self,
        section: Section,
        depth: i32,
        parent: usize,
        offset: Option<IVec3>,
        pieces: &mut Vec<Piece>,
    ) -> bool {
        if depth > MAX_GEN_DEPTH {
            return false;
        }
        let mut child = Vec::new();
        let parent_piece = pieces[parent];
        if !self.generate(section, depth, &parent_piece, offset, &mut child) {
            return false;
        }
        let tag = self.rng.next_int();
        let parent_depth = pieces[parent].gen_depth;
        for c in &mut child {
            c.gen_depth = tag;
            // `StructurePiece.findCollisionPiece`: the first piece it meets.
            if let Some(hit) = pieces.iter().find(|p| p.bounds.intersects(&c.bounds))
                && hit.gen_depth != parent_depth
            {
                return false;
            }
        }
        pieces.extend(child);
        true
    }

    fn generate(
        &mut self,
        section: Section,
        depth: i32,
        parent: &Piece,
        offset: Option<IVec3>,
        pieces: &mut Vec<Piece>,
    ) -> bool {
        match section {
            Section::HouseTower => self.house_tower(depth, parent, offset.unwrap_or(IVec3::ZERO), pieces),
            Section::Tower => self.tower(depth, parent, pieces),
            Section::TowerBridge => self.tower_bridge(depth, parent, pieces),
            Section::FatTower => self.fat_tower(depth, parent, pieces),
        }
    }

    fn house_tower(&mut self, depth: i32, parent: &Piece, offset: IVec3, pieces: &mut Vec<Piece>) -> bool {
        if depth > MAX_GEN_DEPTH {
            return false;
        }
        let r = parent.rotation;
        let mut last = push(pieces, add_piece(*parent, offset, "base_floor", r, true));
        match self.rng.next_bounded(3) {
            0 => {
                chain(pieces, last, IVec3::new(-1, 4, -1), "base_roof", r, true);
            }
            1 => {
                last = chain(pieces, last, IVec3::new(-1, 0, -1), "second_floor_2", r, false);
                last = chain(pieces, last, IVec3::new(-1, 8, -1), "second_roof", r, false);
                self.recursive_children(Section::Tower, depth + 1, last, None, pieces);
            }
            _ => {
                last = chain(pieces, last, IVec3::new(-1, 0, -1), "second_floor_2", r, false);
                last = chain(pieces, last, IVec3::new(-1, 4, -1), "third_floor_2", r, false);
                last = chain(pieces, last, IVec3::new(-1, 8, -1), "third_roof", r, true);
                self.recursive_children(Section::Tower, depth + 1, last, None, pieces);
            }
        }
        true
    }

    fn tower(&mut self, depth: i32, parent: &Piece, pieces: &mut Vec<Piece>) -> bool {
        let r = parent.rotation;
        let (ox, oz) = (3 + self.rng.next_bounded(2), 3 + self.rng.next_bounded(2));
        let mut last = push(pieces, add_piece(*parent, IVec3::new(ox, -3, oz), "tower_base", r, true));
        last = chain(pieces, last, IVec3::new(0, 7, 0), "tower_piece", r, true);
        let mut bridge = (self.rng.next_bounded(3) == 0).then_some(last);
        let height = 1 + self.rng.next_bounded(3);
        for i in 0..height {
            last = chain(pieces, last, IVec3::new(0, 4, 0), "tower_piece", r, true);
            if i < height - 1 && self.rng.next_boolean() {
                bridge = Some(last);
            }
        }
        if let Some(b) = bridge {
            for (turn, offset) in TOWER_BRIDGES {
                if self.rng.next_boolean() {
                    let end = chain(pieces, b, offset, "bridge_end", r.then(turn), true);
                    self.recursive_children(Section::TowerBridge, depth + 1, end, None, pieces);
                }
            }
            chain(pieces, last, IVec3::new(-1, 4, -1), "tower_top", r, true);
        } else {
            if depth != 7 {
                return self.recursive_children(Section::FatTower, depth + 1, last, None, pieces);
            }
            chain(pieces, last, IVec3::new(-1, 4, -1), "tower_top", r, true);
        }
        true
    }

    fn tower_bridge(&mut self, depth: i32, parent: &Piece, pieces: &mut Vec<Piece>) -> bool {
        let r = parent.rotation;
        let length = self.rng.next_bounded(4) + 1;
        let mut last = push(pieces, add_piece(*parent, IVec3::new(0, 0, -4), "bridge_piece", r, true));
        pieces[last].gen_depth = -1;
        let mut next_y = 0;
        for _ in 0..length {
            if self.rng.next_boolean() {
                last = chain(pieces, last, IVec3::new(0, next_y, -4), "bridge_piece", r, true);
                next_y = 0;
            } else {
                last = if self.rng.next_boolean() {
                    chain(pieces, last, IVec3::new(0, next_y, -4), "bridge_steep_stairs", r, true)
                } else {
                    chain(pieces, last, IVec3::new(0, next_y, -8), "bridge_gentle_stairs", r, true)
                };
                next_y = 4;
            }
        }
        if !self.ship_created && self.rng.next_bounded(10 - depth) == 0 {
            let x = -8 + self.rng.next_bounded(8);
            let z = -70 + self.rng.next_bounded(10);
            chain(pieces, last, IVec3::new(x, next_y, z), "ship", r, true);
            self.ship_created = true;
        } else if !self.recursive_children(
            Section::HouseTower,
            depth + 1,
            last,
            Some(IVec3::new(-3, next_y + 1, -11)),
            pieces,
        ) {
            return false;
        }
        last = push(
            pieces,
            add_piece(pieces[last], IVec3::new(4, next_y, 0), "bridge_end", r.then(Rotation::Clockwise180), true),
        );
        pieces[last].gen_depth = -1;
        true
    }

    fn fat_tower(&mut self, depth: i32, parent: &Piece, pieces: &mut Vec<Piece>) -> bool {
        let r = parent.rotation;
        let mut last = push(pieces, add_piece(*parent, IVec3::new(-3, 4, -3), "fat_tower_base", r, true));
        last = chain(pieces, last, IVec3::new(0, 4, 0), "fat_tower_middle", r, true);
        let mut i = 0;
        while i < 2 && self.rng.next_bounded(3) != 0 {
            last = chain(pieces, last, IVec3::new(0, 8, 0), "fat_tower_middle", r, true);
            for (turn, offset) in FAT_TOWER_BRIDGES {
                if self.rng.next_boolean() {
                    let end = chain(pieces, last, offset, "bridge_end", r.then(turn), true);
                    self.recursive_children(Section::TowerBridge, depth + 1, end, None, pieces);
                }
            }
            i += 1;
        }
        chain(pieces, last, IVec3::new(-2, 8, -2), "fat_tower_top", r, true);
        true
    }
}

/// Every piece of the city at `start`, in Java's order
/// (`EndCityPieces.startHouseTower`).
pub fn pieces(seed: u64, start: &Start) -> Vec<Piece> {
    let mut rng = large_feature_random(seed, start.chunk);
    // The rotation `findGenerationPoint` drew.
    rng.next_bounded(4);
    let mut b = Builder { rng: &mut rng, ship_created: false };
    let r = start.rotation;
    let mut pieces = Vec::new();
    let mut last = push(&mut pieces, Piece::new("base_floor", start.origin, r, true));
    last = chain(&mut pieces, last, IVec3::new(-1, 0, -1), "second_floor_1", r, false);
    last = chain(&mut pieces, last, IVec3::new(-1, 4, -1), "third_floor_1", r, false);
    last = chain(&mut pieces, last, IVec3::new(-1, 8, -1), "third_roof", r, true);
    b.recursive_children(Section::Tower, 1, last, None, &mut pieces);
    pieces
}

/// A built city: its pieces, box, chests, shulkers and the ship's frame.
pub struct City {
    pub start: Start,
    pub pieces: Vec<Piece>,
    pub bounds: Bounds,
    pub features: Vec<(IVec3, Feature)>,
    /// Shulker spawn cells (`Sentry` markers).
    pub sentries: Vec<IVec3>,
    /// The ship's item frame (`Elytra` marker), if the city has a ship.
    pub elytra_frame: Option<ElytraFrame>,
    /// Where the ship's dragon head stand is, if the city has a ship.
    pub dragon_head: Option<(IVec3, super::block::Facing)>,
}

/// Where `handleDataMarker("Elytra")` hangs its item frame: the marker cell,
/// facing the piece's rotation applied to south (it hangs on the block
/// behind). The entities populate it with an item frame holding Track B's
/// elytra when a player first comes near (`Entities::populate_end_cities`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ElytraFrame {
    pub cell: IVec3,
    pub facing: super::block::Facing,
}

impl City {
    pub fn build(seed: u64, start: Start) -> City {
        let pieces = pieces(seed, &start);
        let mut bounds = pieces[0].bounds;
        for p in &pieces {
            bounds = bounds.union(&p.bounds);
        }
        let salt = seed ^ (start.chunk.x as u64).wrapping_mul(0x9E37_79B9) ^ ((start.chunk.y as u64) << 32);
        let (mut features, mut sentries, mut elytra_frame, mut dragon_head) = (Vec::new(), Vec::new(), None, None);
        for p in &pieces {
            for &(marker, local) in template(p.template).markers() {
                let w = p.world(local);
                match marker {
                    // `RandomizableContainer.setBlockEntityLootTable` on the
                    // chest below the marker.
                    Marker::Chest => {
                        let c = w - IVec3::Y;
                        features.push((
                            c,
                            Feature::TempleChest(
                                hash3(c.x, c.y, c.z, salt ^ 0xE4C1),
                                super::temples::LootTable::EndCity,
                            ),
                        ));
                    }
                    Marker::Sentry => sentries.push(w),
                    Marker::Elytra => {
                        elytra_frame =
                            Some(ElytraFrame { cell: w, facing: p.rotation.facing(super::block::Facing::South) })
                    }
                }
            }
            if p.name() == "ship" {
                let (local, facing) = templates::DRAGON_HEAD;
                dragon_head = Some((p.world(IVec3::from(local)), p.rotation.facing(facing)));
            }
        }
        City { start, pieces, bounds, features, sentries, elytra_frame, dragon_head }
    }

    /// Paints the part of the city inside the chunk at `base`, pieces in
    /// order so later ones overwrite earlier ones.
    pub fn paint(&self, blocks: &mut [Block], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        if !self.bounds.intersects(&chunk) {
            return;
        }
        for p in self.pieces.iter().filter(|p| p.bounds.intersects(&chunk)) {
            let t = template(p.template);
            let (lo, hi) = (p.bounds.min.max(base), p.bounds.max.min(top));
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    for x in lo.x..=hi.x {
                        let w = IVec3::new(x, y, z);
                        let Some(cell) = t.cell(p.local(w)) else { continue };
                        // `STRUCTURE_AND_AIR` skips stored air.
                        if !p.overwrite && cell == templates::Cell::Air {
                            continue;
                        }
                        let l = w - base;
                        blocks[index(l.x as usize, l.y as usize, l.z as usize)] = cell.resolve(p.rotation);
                    }
                }
            }
        }
        // The ship's `dragon_wall_head` on the bow (v0.16 integration:
        // Track C's head block).
        if let Some((cell, facing)) = self.dragon_head
            && chunk.contains(cell)
        {
            let l = cell - base;
            blocks[index(l.x as usize, l.y as usize, l.z as usize)] =
                super::skulls::wall(super::skulls::Kind::Dragon, facing);
        }
    }

    /// The block [`City::paint`] leaves at `p`, or `None` where the city
    /// leaves what is there.
    pub fn block_at(&self, p: IVec3) -> Option<Block> {
        if !self.bounds.contains(p) {
            return None;
        }
        if let Some((cell, facing)) = self.dragon_head
            && cell == p
        {
            return Some(super::skulls::wall(super::skulls::Kind::Dragon, facing));
        }
        let mut out = None;
        for piece in self.pieces.iter().filter(|piece| piece.bounds.contains(p)) {
            let Some(cell) = template(piece.template).cell(piece.local(p)) else { continue };
            if !piece.overwrite && cell == templates::Cell::Air {
                continue;
            }
            out = Some(cell.resolve(piece.rotation));
        }
        out
    }

    /// The city's chests in the chunk at `cpos`.
    pub fn features_in(&self, cpos: IVec3) -> impl Iterator<Item = (IVec3, Feature)> + '_ {
        let base = cpos * CHUNK_SIZE_I;
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        self.features.iter().copied().filter(move |(p, _)| chunk.contains(*p))
    }
}

/// The End's cities: placement lookups and built layouts, cached.
pub struct Cities {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<City>>>>,
}

const CACHE_LIMIT: usize = 64;

impl Cities {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    /// The city of placement region `region`, built on first use.
    pub fn get(&self, t: &impl Terrain, region: IVec2) -> Option<Arc<City>> {
        if let Some(found) = self.cache.lock().unwrap().get(&region) {
            return found.clone();
        }
        let built = start(self.seed, t, start_chunk(self.seed, region)).map(|s| Arc::new(City::build(self.seed, s)));
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        cache.entry(region).or_insert(built).clone()
    }

    /// The start of region `region`'s city, without laying it out.
    pub fn start_of(&self, t: &impl Terrain, region: IVec2) -> Option<Start> {
        start(self.seed, t, start_chunk(self.seed, region))
    }

    /// Cities whose reach overlaps the blocks `min..=max` (x and z).
    pub fn around(&self, t: &impl Terrain, min: IVec2, max: IVec2) -> Vec<Arc<City>> {
        let span = SPACING * 16;
        let lo = (min - IVec2::splat(REACH + span)).div_euclid(IVec2::splat(span));
        let hi = (max + IVec2::splat(REACH)).div_euclid(IVec2::splat(span));
        let mut out = Vec::new();
        for rz in lo.y..=hi.y {
            for rx in lo.x..=hi.x {
                let region = IVec2::new(rx, rz);
                let c = start_chunk(self.seed, region) * 16 + IVec2::splat(START_OFFSET);
                if c.x + REACH < min.x || c.x - REACH > max.x || c.y + REACH < min.y || c.y - REACH > max.y {
                    continue;
                }
                if let Some(city) = self.get(t, region) {
                    out.push(city);
                }
            }
        }
        out
    }

    /// `/locate structure end_city`: the nearest start within `max_blocks`
    /// of `origin` (horizontal), searched ring by ring of regions.
    pub fn nearest(&self, t: &impl Terrain, origin: IVec2, max_blocks: i32) -> Option<IVec3> {
        let span = SPACING * 16;
        let home = origin.div_euclid(IVec2::splat(span));
        let rings = max_blocks / span + 1;
        let mut best: Option<(i64, IVec3)> = None;
        for ring in 0..=rings {
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let region = home + IVec2::new(dx, dz);
                    let Some(s) = self.start_of(t, region) else { continue };
                    let d = (i64::from(s.origin.x - origin.x)).pow(2) + (i64::from(s.origin.z - origin.y)).pow(2);
                    if d <= i64::from(max_blocks).pow(2) && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, s.origin));
                    }
                }
            }
            if best.is_some_and(|(d, _)| d < i64::from(ring * span).pow(2)) {
                break;
            }
        }
        best.map(|(_, p)| p)
    }
}

#[cfg(test)]
mod tests;
