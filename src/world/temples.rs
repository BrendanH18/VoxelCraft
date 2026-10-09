//! Overworld structures added in v0.6: desert pyramids, jungle temples,
//! swamp huts, igloos, pillager outposts, shipwrecks, ocean ruins and ocean
//! monuments.
//!
//! Placement follows Java's `random_spread` structure sets: the world is
//! cut into regions of `spacing` 16×16 chunks and each region holds at
//! most one start, at a seeded offset that keeps starts `separation`
//! chunks apart (monuments use Java's triangular spread). A start is kept
//! when the biome at its centre allows the structure. Layouts are original
//! builds in the shape and materials of Java's, built once per start,
//! cached, and painted clipped into every chunk they cross.

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::bastion::{Loot as L, pool};
use super::biome::Biome;
use super::block::{Block, Facing};
use super::chest::Chest;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, index};
use super::fortress::Feature;
use super::noise::hash3;
use super::overworld_blocks as ob;
use super::structure::{Bounds, Rng};
use super::terrain::{Dimension, Generator, SEA_LEVEL};
use crate::color::DyeColor;
use crate::enchant::JavaRandom;
use crate::inventory::Stack;
use crate::item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    DesertPyramid,
    JungleTemple,
    SwampHut,
    Igloo,
    Outpost,
    Shipwreck,
    OceanRuin,
    Monument,
}

impl Kind {
    pub const ALL: [Kind; 8] = [
        Kind::DesertPyramid,
        Kind::JungleTemple,
        Kind::SwampHut,
        Kind::Igloo,
        Kind::Outpost,
        Kind::Shipwreck,
        Kind::OceanRuin,
        Kind::Monument,
    ];

    /// Java's structure id.
    pub fn name(self) -> &'static str {
        match self {
            Kind::DesertPyramid => "desert_pyramid",
            Kind::JungleTemple => "jungle_pyramid",
            Kind::SwampHut => "swamp_hut",
            Kind::Igloo => "igloo",
            Kind::Outpost => "pillager_outpost",
            Kind::Shipwreck => "shipwreck",
            Kind::OceanRuin => "ocean_ruin",
            Kind::Monument => "monument",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        let name = match name {
            "jungle_temple" => "jungle_pyramid",
            "ocean_monument" => "monument",
            "outpost" => "pillager_outpost",
            "witch_hut" => "swamp_hut",
            other => other,
        };
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Java's `(spacing, separation, salt)` in 16×16 chunks.
    fn placement(self) -> (i32, i32, u64) {
        match self {
            Kind::DesertPyramid => (32, 8, 14_357_617),
            Kind::Igloo => (32, 8, 14_357_618),
            Kind::JungleTemple => (32, 8, 14_357_619),
            Kind::SwampHut => (32, 8, 14_357_620),
            Kind::OceanRuin => (20, 8, 14_357_621),
            Kind::Shipwreck => (24, 4, 165_745_295),
            Kind::Outpost => (32, 8, 165_745_296),
            Kind::Monument => (32, 5, 10_387_313),
        }
    }

    /// Furthest a build reaches from its start chunk's centre.
    fn reach(self) -> i32 {
        match self {
            Kind::Monument => 40,
            Kind::Shipwreck => 20,
            Kind::Outpost | Kind::DesertPyramid | Kind::JungleTemple => 16,
            _ => 12,
        }
    }

    fn allowed(self, biome: Biome) -> bool {
        use Biome::*;
        match self {
            Kind::DesertPyramid => biome == Desert,
            Kind::JungleTemple => matches!(biome, Jungle | BambooJungle),
            Kind::SwampHut => biome == Swamp,
            Kind::Igloo => matches!(biome, SnowyPlains | SnowyTaiga | SnowySlopes),
            Kind::Outpost => matches!(
                biome,
                Desert
                    | Plains
                    | Savanna
                    | SnowyPlains
                    | Taiga
                    | Meadow
                    | FrozenPeaks
                    | JaggedPeaks
                    | StonyPeaks
                    | SnowySlopes
                    | Grove
                    | CherryGrove
            ),
            Kind::Shipwreck => biome.is_ocean() || biome == Beach || biome == SnowyBeach,
            Kind::OceanRuin => biome.is_ocean(),
            Kind::Monument => biome.is_deep_ocean(),
        }
    }
}

/// Loot tables of these structures (Java's `chests/*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LootTable {
    DesertPyramid,
    JungleTemple,
    /// The arrows in a jungle temple's trap dispensers.
    Dispenser,
    Igloo,
    Outpost,
    ShipSupply,
    ShipTreasure,
    ShipMap,
    RuinSmall,
    RuinBig,
}

/// One built structure: its blocks, chests and where its mobs live.
pub struct Built {
    pub kind: Kind,
    pub bounds: Bounds,
    blocks: Vec<(IVec3, Block)>,
    features: Vec<(IVec3, Feature)>,
    /// Where the structure's own mobs spawn (witches, pillagers, guardians).
    pub spawns: Bounds,
    pub residents: Vec<(crate::entity::MobKind, IVec3)>,
}

/// Lays out a build in a rotated local frame: x across, z away from the
/// entrance, y up from `origin`.
struct Builder {
    origin: IVec3,
    size: IVec2,
    rot: u8,
    blocks: FxHashMap<IVec3, Block>,
    features: Vec<(IVec3, Feature)>,
    residents: Vec<(crate::entity::MobKind, IVec3)>,
}

impl Builder {
    fn new(origin: IVec3, size: IVec2, rot: u8) -> Self {
        // Swapping width/depth must preserve the start's horizontal centre.
        // Otherwise a rotated narrow ship moves entirely off its locate point.
        let origin = if rot % 2 == 1 {
            let shift = size.x / 2 - size.y / 2;
            origin + IVec3::new(shift, 0, -shift)
        } else {
            origin
        };
        Self { origin, size, rot, blocks: FxHashMap::default(), features: Vec::new(), residents: Vec::new() }
    }

    fn world(&self, x: i32, y: i32, z: i32) -> IVec3 {
        let (w, d) = (self.size.x - 1, self.size.y - 1);
        let (dx, dz) = match self.rot {
            0 => (x, z),
            1 => (d - z, x),
            2 => (w - x, d - z),
            _ => (z, w - x),
        };
        self.origin + IVec3::new(dx, y, dz)
    }

    /// A local facing (south = +z, away from the entrance) in the world.
    fn turn(&self, f: Facing) -> Facing {
        let mut f = f;
        for _ in 0..self.rot {
            f = match f {
                Facing::South => Facing::West,
                Facing::West => Facing::North,
                Facing::North => Facing::East,
                Facing::East => Facing::South,
            };
        }
        f
    }

    fn set(&mut self, x: i32, y: i32, z: i32, b: Block) {
        let p = self.world(x, y, z);
        self.blocks.insert(p, b);
    }

    #[allow(clippy::too_many_arguments)]
    fn fill(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, b: Block) {
        for y in y0.min(y1)..=y0.max(y1) {
            for z in z0.min(z1)..=z0.max(z1) {
                for x in x0.min(x1)..=x0.max(x1) {
                    self.set(x, y, z, b);
                }
            }
        }
    }

    /// Walls of the box (no floor or ceiling) in `wall`, inside in `inside`.
    #[allow(clippy::too_many_arguments)]
    fn room(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, wall: Block, inside: Block) {
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let edge = x == x0 || x == x1 || z == z0 || z == z1;
                    self.set(x, y, z, if edge { wall } else { inside });
                }
            }
        }
    }

    fn facing(&self, b: Block, f: Facing) -> Block {
        b.with_facing(self.turn(f))
    }

    fn chest(&mut self, x: i32, y: i32, z: i32, f: Facing, seed: u64, table: LootTable) {
        let b = self.facing(Block::CHEST, f);
        self.set(x, y, z, b);
        let p = self.world(x, y, z);
        self.features.push((p, Feature::TempleChest(seed ^ hash3(p.x, p.y, p.z, 0x7E), table)));
    }

    /// Pillars of `b` from local `(x, y, z)` down to the ground under it.
    fn foundation(&mut self, g: &Generator, x: i32, y: i32, z: i32, b: Block, deepest: i32) {
        let p = self.world(x, y, z);
        let ground = g.column(p.x, p.z).height;
        for wy in (ground + 1).max(p.y - deepest)..=p.y {
            self.blocks.insert(IVec3::new(p.x, wy, p.z), b);
        }
    }

    fn finish(self, kind: Kind, spawns_up: i32) -> Built {
        let mut min = IVec3::splat(i32::MAX);
        let mut max = IVec3::splat(i32::MIN);
        for p in self.blocks.keys() {
            min = min.min(*p);
            max = max.max(*p);
        }
        let mut blocks: Vec<_> = self.blocks.into_iter().collect();
        blocks.sort_unstable_by_key(|(p, _)| (p.y, p.z, p.x));
        let bounds = Bounds { min, max };
        let spawns = Bounds { min, max: max + IVec3::Y * spawns_up };
        Built { kind, bounds, blocks, features: self.features, spawns, residents: self.residents }
    }
}

type StructureCache = FxHashMap<(Kind, IVec2), Option<Arc<Built>>>;

pub struct Temples {
    seed: u64,
    cache: Mutex<StructureCache>,
}

const CACHE_LIMIT: usize = 256;

impl Temples {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    /// The start chunk (16×16) of `kind` in `region` (Java's
    /// `RandomSpreadStructurePlacement.getPotentialStructureChunk`).
    fn start(&self, kind: Kind, region: IVec2) -> IVec2 {
        let (spacing, separation, salt) = kind.placement();
        let seed = self
            .seed
            .wrapping_add((region.x as i64).wrapping_mul(341_873_128_712) as u64)
            .wrapping_add((region.y as i64).wrapping_mul(132_897_987_541) as u64)
            .wrapping_add(salt);
        let mut rng = JavaRandom::new(seed as i64);
        let span = spacing - separation;
        let offset = |rng: &mut JavaRandom| {
            if kind == Kind::Monument {
                (rng.next_bounded(span) + rng.next_bounded(span)) / 2
            } else {
                rng.next_bounded(span)
            }
        };
        let ox = offset(&mut rng);
        let oz = offset(&mut rng);
        region * spacing + IVec2::new(ox, oz)
    }

    /// The structure of `kind` in `region`, built on first use.
    fn get(&self, g: &Generator, kind: Kind, region: IVec2) -> Option<Arc<Built>> {
        if g.dimension != Dimension::Overworld {
            return None;
        }
        if let Some(found) = self.cache.lock().unwrap().get(&(kind, region)) {
            return found.clone();
        }
        let start = self.start(kind, region);
        let centre = start * 16 + IVec2::splat(8);
        let col = g.column(centre.x, centre.y);
        let mut rng = Rng(hash3(start.x, start.y, kind as i32, self.seed ^ 0x7E3D));
        let ok = kind.allowed(col.biome)
            // Outposts keep away from villages, and stay on dry land.
            && (kind != Kind::Outpost
                || (self.outpost_frequency(start) && col.height > SEA_LEVEL && !self.near_village(g, centre)))
            && match kind {
                Kind::Shipwreck | Kind::OceanRuin => col.height < SEA_LEVEL - 4 || col.biome.is_beach(),
                Kind::Monument => col.height < SEA_LEVEL - 18 && self.monument_biomes(g, centre),
                _ => col.height >= SEA_LEVEL - 1,
            };
        let built = ok.then(|| Arc::new(self.build(g, kind, centre, col.height, &mut rng)));
        let mut cache = self.cache.lock().unwrap();
        if let Some(found) = cache.get(&(kind, region)) {
            return found.clone();
        }
        if cache.len() >= CACHE_LIMIT
            && let Some(k) = cache.keys().next().copied()
        {
            cache.remove(&k);
        }
        cache.entry((kind, region)).or_insert(built).clone()
    }

    /// Java checks deep-ocean biomes within 16 blocks, and ocean/river
    /// biomes within 29 blocks. Sample on the biome noise's four-block grid.
    fn monument_biomes(&self, g: &Generator, centre: IVec2) -> bool {
        for dz in (-32..=28).step_by(4) {
            for dx in (-32..=28).step_by(4) {
                let biome = g.column(centre.x + dx, centre.y + dz).biome;
                if !biome.is_ocean() && !matches!(biome, Biome::River | Biome::FrozenRiver) {
                    return false;
                }
                if dx.abs().max(dz.abs()) <= 16 && !biome.is_deep_ocean() {
                    return false;
                }
            }
        }
        true
    }

    /// Java's legacy_type_1 reduction: one in five candidates, seeded
    /// independently of the layout and placement random streams.
    fn outpost_frequency(&self, chunk: IVec2) -> bool {
        let region_seed = (chunk.x >> 4) ^ ((chunk.y >> 4) << 4);
        let mut rng = JavaRandom::new(i64::from(region_seed) ^ self.seed as i64);
        rng.next_int();
        rng.next_bounded(5) == 0
    }

    fn near_village(&self, g: &Generator, at: IVec2) -> bool {
        g.villages.near(at, 10)
    }

    /// Every structure whose build may cross the box from `lo` to `hi`.
    pub fn near(&self, g: &Generator, p: IVec3, radius: i32) -> Vec<Arc<Built>> {
        let xz = IVec2::new(p.x, p.z);
        self.around(g, xz - IVec2::splat(radius), xz + IVec2::splat(radius))
            .into_iter()
            .filter(|b| {
                b.bounds.min.x <= p.x + radius
                    && b.bounds.max.x >= p.x - radius
                    && b.bounds.min.z <= p.z + radius
                    && b.bounds.max.z >= p.z - radius
            })
            .collect()
    }
    fn around(&self, g: &Generator, lo: IVec2, hi: IVec2) -> Vec<Arc<Built>> {
        let mut out = Vec::new();
        for kind in Kind::ALL {
            let (spacing, _, _) = kind.placement();
            let r = kind.reach();
            let span = spacing * 16;
            let (rlo, rhi) = (
                (lo - IVec2::splat(r + 16)).div_euclid(IVec2::splat(span)),
                (hi + IVec2::splat(r)).div_euclid(IVec2::splat(span)),
            );
            for rz in rlo.y..=rhi.y {
                for rx in rlo.x..=rhi.x {
                    let region = IVec2::new(rx, rz);
                    let c = self.start(kind, region) * 16 + IVec2::splat(8);
                    if c.x + r < lo.x || c.x - r > hi.x || c.y + r < lo.y || c.y - r > hi.y {
                        continue;
                    }
                    if let Some(b) = self.get(g, kind, region) {
                        out.push(b);
                    }
                }
            }
        }
        out
    }

    /// Paints the structures crossing the chunk at `base`.
    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        for b in self.around(g, IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            if !b.bounds.intersects(&chunk) {
                continue;
            }
            let start = b.blocks.partition_point(|(p, _)| p.y < base.y);
            for &(p, block) in &b.blocks[start..] {
                if p.y > top.y {
                    break;
                }
                if chunk.contains(p) {
                    let l = p - base;
                    blocks[index(l.x as usize, l.y as usize, l.z as usize)] = block;
                }
            }
        }
    }

    /// Chests and dispensers of structures in the chunk at `cpos`.
    pub fn features(&self, g: &Generator, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        self.around(g, IVec2::new(chunk.min.x, chunk.min.z), IVec2::new(chunk.max.x, chunk.max.z))
            .iter()
            .flat_map(|b| b.features.iter().filter(|(p, _)| chunk.contains(*p)).copied().collect::<Vec<_>>())
            .collect()
    }

    /// The structure whose spawning area holds `p`, if any.
    pub fn at(&self, g: &Generator, p: IVec3) -> Option<Arc<Built>> {
        let q = IVec2::new(p.x, p.z);
        self.around(g, q, q).into_iter().find(|b| b.spawns.contains(p))
    }

    /// Nearest start of `kind` (`/locate structure`), searching regions
    /// outward from `origin` up to `max_blocks` away.
    pub fn nearest(&self, g: &Generator, kind: Kind, origin: IVec2, max_blocks: i32) -> Option<IVec3> {
        if g.dimension != Dimension::Overworld || max_blocks < 0 {
            return None;
        }
        let (spacing, _, _) = kind.placement();
        let span = spacing * 16;
        let home = origin.div_euclid(IVec2::splat(span));
        let rings = max_blocks / span + 1;
        let radius_squared = i64::from(max_blocks).pow(2);
        let mut best: Option<(i64, IVec3)> = None;
        for ring in 0..=rings {
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let region = home + IVec2::new(dx, dz);
                    let c = self.start(kind, region) * 16 + IVec2::splat(8);
                    let d =
                        (i64::from(c.x) - i64::from(origin.x)).pow(2) + (i64::from(c.y) - i64::from(origin.y)).pow(2);
                    if d > radius_squared || best.is_some_and(|(bd, _)| d >= bd) {
                        continue;
                    }
                    if let Some(b) = self.get(g, kind, region) {
                        let y = (b.bounds.min.y + b.bounds.max.y) / 2;
                        best = Some((d, IVec3::new(c.x, y, c.y)));
                    }
                }
            }
            // A farther ring can still have a nearer start when the origin
            // lies near a region edge. Stop only once all unseen regions are
            // farther than the best start (or outside the requested radius).
            let lo = (home - IVec2::splat(ring)).as_i64vec2() * i64::from(span);
            let hi = (home + IVec2::splat(ring + 1)).as_i64vec2() * i64::from(span);
            let from = origin.as_i64vec2();
            let unseen = (from - lo).min(hi - from).min_element();
            if unseen * unseen > best.map_or(radius_squared, |(d, _)| d) {
                break;
            }
        }
        best.map(|(_, c)| c)
    }

    fn build(&self, g: &Generator, kind: Kind, centre: IVec2, ground: i32, rng: &mut Rng) -> Built {
        let rot = rng.below(4) as u8;
        let seed = rng.next_u64();
        match kind {
            Kind::DesertPyramid => desert_pyramid(g, centre, ground, rot, seed),
            Kind::JungleTemple => jungle_temple(g, centre, ground, rot, seed, rng),
            Kind::SwampHut => swamp_hut(g, centre, ground, rot),
            Kind::Igloo => igloo(g, centre, ground, rot, seed, rng),
            Kind::Outpost => outpost(g, centre, ground, rot, seed),
            Kind::Shipwreck => shipwreck(g, centre, ground, rot, seed, rng),
            Kind::OceanRuin => ocean_ruin(g, centre, ground, rot, seed, rng),
            Kind::Monument => monument(g, centre, rot, rng),
        }
    }
}

fn desert_pyramid(g: &Generator, centre: IVec2, ground: i32, rot: u8, seed: u64) -> Built {
    let s = Block::SANDSTONE;
    let origin = IVec3::new(centre.x - 10, ground, centre.y - 10);
    let mut b = Builder::new(origin, IVec2::new(21, 21), rot);
    for z in 0..21 {
        for x in 0..21 {
            b.foundation(g, x, -1, z, s, 12);
        }
    }
    b.fill(0, 0, 0, 20, 0, 20, s);
    // A stepped pyramid: each layer one block in, hollow over the hall.
    for y in 1..=10 {
        for z in y..=20 - y {
            for x in y..=20 - y {
                let hall = y <= 5 && (4..=16).contains(&x) && (4..=16).contains(&z);
                b.set(x, y, z, if hall { Block::AIR } else { s });
            }
        }
    }
    // The two front towers, with orange and blue terracotta bands.
    for tx in [0, 16] {
        b.fill(tx, 1, 0, tx + 4, 10, 4, s);
        b.fill(tx + 1, 1, 1, tx + 3, 9, 3, Block::AIR);
        for x in tx..=tx + 4 {
            b.set(x, 7, 0, Block::terracotta(1));
            b.set(x, 8, 0, Block::stained_terracotta(DyeColor::Blue));
        }
        for x in [tx, tx + 2, tx + 4] {
            b.set(x, 11, 0, s);
            b.set(x, 11, 4, s);
        }
    }
    // Entrance and the hall's floor pattern.
    b.fill(9, 1, 0, 11, 3, 4, Block::AIR);
    b.fill(8, 0, 8, 12, 0, 12, Block::terracotta(1));
    b.fill(9, 0, 9, 11, 0, 11, s);
    b.set(10, 0, 10, Block::stained_terracotta(DyeColor::Blue));
    for (x, z) in [(10, 8), (10, 12), (8, 10), (12, 10)] {
        b.set(x, 0, z, Block::stained_terracotta(DyeColor::Blue));
    }
    // The hidden chamber: a drop under the blue tile, four chests and a TNT trap.
    b.fill(10, -9, 10, 10, -1, 10, Block::AIR);
    b.room(6, -14, 6, 14, -10, 14, s, Block::AIR);
    b.fill(6, -15, 6, 14, -15, 14, s);
    b.fill(7, -10, 7, 13, -10, 13, s);
    b.set(10, -10, 10, Block::AIR);
    b.fill(9, -16, 9, 11, -16, 11, Block::TNT);
    b.fill(9, -15, 9, 11, -15, 11, s);
    b.set(10, -15, 10, Block::TNT);
    b.set(10, -14, 10, super::redstone_blocks::STONE_PLATE);
    for (x, z, f) in [(10, 7, Facing::South), (10, 13, Facing::North), (7, 10, Facing::East), (13, 10, Facing::West)] {
        b.chest(x, -14, z, f, seed, LootTable::DesertPyramid);
    }
    b.finish(Kind::DesertPyramid, 0)
}

fn jungle_temple(g: &Generator, centre: IVec2, ground: i32, rot: u8, seed: u64, rng: &mut Rng) -> Built {
    let origin = IVec3::new(centre.x - 6, ground, centre.y - 7);
    let mut b = Builder::new(origin, IVec2::new(12, 15), rot);
    let stone = |rng: &mut Rng| if rng.below(10) < 4 { Block::MOSSY_COBBLESTONE } else { Block::COBBLESTONE };
    for z in 0..15 {
        for x in 0..12 {
            b.foundation(g, x, -5, z, Block::COBBLESTONE, 10);
        }
    }
    // Basement, two floors and a stepped roof.
    for y in -4..=9 {
        for z in 0..15 {
            for x in 0..12 {
                let edge = x == 0 || x == 11 || z == 0 || z == 14;
                let floor = matches!(y, -4 | 0 | 4 | 8);
                let block = if edge || floor { stone(rng) } else { Block::AIR };
                b.set(x, y, z, block);
            }
        }
    }
    for (y, inset) in [(10, 1), (11, 2), (12, 4)] {
        for z in inset..15 - inset {
            for x in inset..12 - inset {
                b.set(x, y, z, stone(rng));
            }
        }
    }
    b.fill(5, 1, 0, 6, 3, 0, Block::AIR);
    // Stairs down to the basement and up between floors.
    for i in 0..4 {
        b.set(9, -i, 3 + i, Block::AIR);
        b.set(9, -i + 1, 3 + i, Block::AIR);
        b.set(2, 1 + i, 9 + i, Block::COBBLESTONE);
        b.set(2, 4, 9 + i, Block::AIR);
    }
    // Arrow trap: a tripwire across the side passage fires the dispenser in the wall.
    b.fill(1, 1, 5, 3, 3, 13, Block::AIR);
    let east = b.turn(Facing::East);
    let west = b.turn(Facing::West);
    b.set(1, 1, 7, super::gadgets::hook(east, true, false));
    b.set(2, 1, 7, super::gadgets::tripwire(false, true, false));
    b.set(3, 1, 7, super::gadgets::hook(west, true, false));
    b.set(4, 1, 7, Block::COBBLESTONE);
    let dispenser = super::redstone_blocks::dispenser(face_code(east), false);
    b.set(0, 1, 7, dispenser);
    let p = b.world(0, 1, 7);
    b.features.push((p, Feature::TempleChest(seed ^ 1, LootTable::Dispenser)));
    b.chest(2, 1, 13, Facing::North, seed, LootTable::JungleTemple);
    b.chest(8, -3, 12, Facing::West, seed ^ 2, LootTable::JungleTemple);
    // Vines over the walls.
    for z in 0..15 {
        for y in 1..9 {
            if rng.below(4) == 0 {
                b.set(-1, y, z, ob::vine(b.turn(Facing::East)));
            }
            if rng.below(4) == 0 {
                b.set(12, y, z, ob::vine(b.turn(Facing::West)));
            }
        }
    }
    b.finish(Kind::JungleTemple, 0)
}

/// Dispenser facing code (`Facing::ALL` order) of a horizontal facing.
fn face_code(f: Facing) -> u8 {
    Facing::ALL.iter().position(|&a| a == f).unwrap() as u8
}

fn swamp_hut(g: &Generator, centre: IVec2, ground: i32, rot: u8) -> Built {
    let floor = ground.max(SEA_LEVEL) + 2;
    let origin = IVec3::new(centre.x - 3, floor, centre.y - 4);
    let mut b = Builder::new(origin, IVec2::new(7, 9), rot);
    let planks = Block::SPRUCE_PLANKS;
    for (x, z) in [(1, 2), (5, 2), (1, 7), (5, 7)] {
        b.foundation(g, x, -1, z, Block::LOG, 8);
    }
    b.fill(1, 0, 1, 5, 0, 7, planks);
    b.room(1, 1, 2, 5, 3, 7, planks, Block::AIR);
    for (x, z) in [(1, 2), (5, 2), (1, 7), (5, 7)] {
        b.fill(x, 1, z, x, 3, z, Block::LOG);
    }
    b.set(3, 1, 2, Block::AIR);
    b.set(3, 2, 2, Block::AIR);
    b.set(1, 2, 4, Block::GLASS_PANE);
    b.set(5, 2, 4, Block::GLASS_PANE);
    b.fill(0, 4, 1, 6, 4, 8, planks);
    b.fill(1, 5, 2, 5, 5, 7, planks);
    b.set(4, 1, 6, Block::CRAFTING_TABLE);
    b.set(2, 1, 6, Block::BROWN_MUSHROOM);
    b.residents.push((crate::entity::MobKind::Witch, b.world(3, 1, 5)));
    b.residents.push((crate::entity::MobKind::Cat, b.world(2, 1, 4)));
    b.finish(Kind::SwampHut, 2)
}

fn igloo(g: &Generator, centre: IVec2, ground: i32, rot: u8, seed: u64, rng: &mut Rng) -> Built {
    let origin = IVec3::new(centre.x - 4, ground + 1, centre.y - 4);
    let mut b = Builder::new(origin, IVec2::new(9, 9), rot);
    for z in 0..9 {
        for x in 0..9 {
            b.foundation(g, x, -1, z, Block::SNOW, 4);
        }
    }
    // A snow dome: a hemisphere of radius 4 around (4, 0, 5).
    for y in 0..=4 {
        for z in 1..9 {
            for x in 0..9 {
                let d = ((x - 4) * (x - 4) + (z - 5) * (z - 5) + y * y) as f32;
                if d <= 17.0 {
                    b.set(x, y, z, if d >= 9.5 { Block::SNOW } else { Block::AIR });
                }
            }
        }
    }
    // Entrance tunnel.
    b.fill(3, 0, 0, 5, 2, 1, Block::SNOW);
    b.fill(4, 0, 0, 4, 1, 2, Block::AIR);
    b.fill(2, -1, 2, 6, -1, 8, Block::SNOW);
    b.set(2, 0, 5, Block::CRAFTING_TABLE);
    b.set(6, 0, 5, b.facing(Block::FURNACE, Facing::West));
    b.features.push((b.world(6, 0, 5), Feature::UtilityBlock(Block::FURNACE)));
    b.set(2, 0, 6, b.facing(Block::BED_FOOT, Facing::South));
    b.set(2, 0, 7, b.facing(Block::BED_HEAD, Facing::South));
    b.set(4, 0, 7, Block::TORCH);
    for z in 3..=7 {
        for x in 3..=5 {
            if (x, z) != (4, 7) {
                b.set(x, 0, z, Block::carpet(DyeColor::White));
            }
        }
    }
    if rng.below(2) == 0 {
        // The secret basement: a trapdoor under the carpet, a ladder down
        // to a stone brick room with a chest and a brewing stand.
        b.set(4, -1, 4, super::redstone_blocks::WOOD_TRAPDOOR);
        let ladder = b.facing(Block::LADDER, Facing::South);
        for y in -9..=-2 {
            b.set(4, y, 4, ladder);
            b.set(4, y, 3, Block::STONE_BRICKS);
        }
        b.room(1, -12, 4, 7, -9, 10, Block::STONE_BRICKS, Block::AIR);
        b.fill(1, -13, 4, 7, -13, 10, Block::STONE_BRICKS);
        b.fill(1, -8, 4, 7, -8, 10, Block::STONE_BRICKS);
        // Reopen the shaft after the room walls and ceiling have been
        // painted; otherwise two wall blocks seal the basement entrance.
        b.fill(4, -12, 4, 4, -8, 4, ladder);
        b.fill(4, -12, 3, 4, -2, 3, Block::STONE_BRICKS);
        b.chest(2, -12, 9, Facing::North, seed, LootTable::Igloo);
        b.set(6, -12, 9, Block::BREWING_STAND);
        b.features.push((b.world(6, -12, 9), Feature::IglooBrewing));
        b.set(4, -12, 9, Block::TORCH);
        b.residents.push((crate::entity::MobKind::Villager, b.world(2, -12, 5)));
        b.residents.push((crate::entity::MobKind::ZombieVillager, b.world(6, -12, 5)));
        for (x0, x1) in [(1, 3), (5, 7)] {
            b.fill(x0, -12, 6, x1, -10, 6, Block::IRON_BARS);
        }
        b.fill(3, -12, 4, 3, -10, 6, Block::IRON_BARS);
        b.fill(5, -12, 4, 5, -10, 6, Block::IRON_BARS);
    }
    b.finish(Kind::Igloo, 0)
}

fn outpost(g: &Generator, centre: IVec2, ground: i32, rot: u8, seed: u64) -> Built {
    let origin = IVec3::new(centre.x - 4, ground + 1, centre.y - 4);
    let mut b = Builder::new(origin, IVec2::new(9, 9), rot);
    let (log, planks, walls) = (Block::DARK_OAK_LOG, Block::DARK_OAK_PLANKS, Block::BIRCH_PLANKS);
    for z in 0..9 {
        for x in 0..9 {
            b.foundation(g, x, -1, z, Block::COBBLESTONE, 6);
        }
    }
    b.fill(0, 0, 0, 8, 0, 8, Block::COBBLESTONE);
    for (x, z) in [(1, 1), (7, 1), (1, 7), (7, 7)] {
        b.fill(x, 1, z, x, 18, z, log);
    }
    for y in [6, 12, 18] {
        b.fill(1, y, 1, 7, y, 7, planks);
    }
    // The upper room: birch walls with windows.
    b.room(1, 13, 1, 7, 17, 7, walls, Block::AIR);
    for (x, z) in [(1, 1), (7, 1), (1, 7), (7, 7)] {
        b.fill(x, 13, z, x, 17, z, log);
    }
    for i in [3, 5] {
        b.set(i, 15, 1, Block::AIR);
        b.set(i, 15, 7, Block::AIR);
        b.set(1, 15, i, Block::AIR);
        b.set(7, 15, i, Block::AIR);
    }
    // The lookout: a wider top with a fence railing.
    b.fill(0, 19, 0, 8, 19, 8, planks);
    for i in 0..9 {
        for (x, z) in [(i, 0), (i, 8), (0, i), (8, i)] {
            b.set(x, 20, z, Block::OAK_FENCE);
        }
    }
    let ladder = b.facing(Block::LADDER, Facing::South);
    for y in 1..=19 {
        b.set(4, y, 2, ladder);
        b.set(4, y, 1, if (13..=17).contains(&y) { walls } else { planks });
    }
    b.set(4, 20, 2, Block::AIR);
    b.chest(6, 20, 6, Facing::West, seed, LootTable::Outpost);
    for (x, y, z) in [(3, 20, 4), (5, 13, 4), (3, 1, 5)] {
        b.residents.push((crate::entity::MobKind::Pillager, b.world(x, y, z)));
    }
    b.finish(Kind::Outpost, 0)
}

fn shipwreck(g: &Generator, centre: IVec2, ground: i32, rot: u8, seed: u64, rng: &mut Rng) -> Built {
    let beached = g.column(centre.x, centre.y).biome.is_beach();
    let upside_down = !beached && rng.below(4) == 0;
    let wood = if rng.below(2) == 0 { Block::SPRUCE_PLANKS } else { Block::PLANKS };
    let inside = if beached { Block::AIR } else { Block::WATER };
    let origin = IVec3::new(centre.x - 3, ground - 1, centre.y - 10);
    let mut b = Builder::new(origin, IVec2::new(6, 20), rot);
    let integrity = 0.85;
    let put = |b: &mut Builder, rng: &mut Rng, x: i32, y: i32, z: i32, block: Block| {
        let y = if upside_down { 8 - y } else { y };
        let keep = block != wood || rng.unit() < integrity;
        b.set(x, y, z, if keep { block } else { inside });
    };
    for z in 0..20 {
        let narrow = !(2..=17).contains(&z);
        let (x0, x1) = if narrow { (1, 4) } else { (0, 5) };
        for x in x0..=x1 {
            for y in 0..=4 {
                let shell = y == 0 && !narrow || x == x0 || x == x1 || z == 0 || z == 19 || y == 4;
                let block = if shell { wood } else { inside };
                put(&mut b, rng, x, y, z, block);
            }
        }
    }
    // The cabin at the stern and the mast.
    for y in 5..=7 {
        for z in 14..=18 {
            for x in 1..=4 {
                let edge = x == 1 || x == 4 || z == 14 || z == 18 || y == 7;
                put(&mut b, rng, x, y, z, if edge { wood } else { inside });
            }
        }
    }
    put(&mut b, rng, 2, 5, 14, inside);
    put(&mut b, rng, 2, 6, 14, inside);
    for y in 5..=11 {
        put(&mut b, rng, 2, y, 7, Block::LOG);
    }
    let flip = |y: i32| if upside_down { 8 - y } else { y };
    b.chest(2, flip(1), 3, Facing::South, seed, LootTable::ShipSupply);
    b.chest(3, flip(1), 16, Facing::North, seed ^ 1, LootTable::ShipTreasure);
    b.chest(3, flip(5), 17, Facing::North, seed ^ 2, LootTable::ShipMap);
    b.finish(Kind::Shipwreck, 0)
}

fn ocean_ruin(g: &Generator, centre: IVec2, ground: i32, rot: u8, seed: u64, rng: &mut Rng) -> Built {
    let warm = matches!(
        g.column(centre.x, centre.y).biome,
        Biome::WarmOcean | Biome::LukewarmOcean | Biome::DeepLukewarmOcean
    );
    let big = rng.below(10) < 3;
    let size = if big { 12 } else { 7 };
    let origin = IVec3::new(centre.x - size / 2, ground, centre.y - size / 2);
    let mut b = Builder::new(origin, IVec2::new(size, size), rot);
    let pick = |rng: &mut Rng| -> Block {
        if warm {
            [Block::SANDSTONE, Block::SANDSTONE, Block::SMOOTH_STONE][rng.below(3) as usize]
        } else {
            [Block::STONE_BRICKS, Block::MOSSY_STONE_BRICKS, Block::CRACKED_STONE_BRICKS, Block::STONE_BRICKS]
                [rng.below(4) as usize]
        }
    };
    for z in 0..size {
        for x in 0..size {
            let block = pick(rng);
            b.set(x, 0, z, block);
        }
    }
    let rooms: &[(i32, i32, i32, i32)] = if big { &[(0, 0, 11, 11), (3, 3, 8, 8)] } else { &[(0, 0, 6, 6)] };
    for &(x0, z0, x1, z1) in rooms {
        for z in z0..=z1 {
            for x in x0..=x1 {
                if x != x0 && x != x1 && z != z0 && z != z1 {
                    continue;
                }
                let tall = 1 + rng.below(if big { 5 } else { 3 }) as i32;
                for y in 1..=tall {
                    if rng.unit() < 0.75 {
                        let block = pick(rng);
                        b.set(x, y, z, block);
                    }
                }
            }
        }
    }
    let (cx, cz) = if big { (5, 5) } else { (3, 3) };
    b.chest(cx, 1, cz, Facing::South, seed, if big { LootTable::RuinBig } else { LootTable::RuinSmall });
    if warm {
        b.set(cx + 1, 1, cz, Block::MAGMA);
    }
    b.finish(Kind::OceanRuin, 6)
}

/// Java's ocean monument: a 58×58 prismarine building on the deep sea
/// floor, two wings and a central tower over the gold-filled core room,
/// lit by sea lanterns and flooded inside.
fn monument(g: &Generator, centre: IVec2, rot: u8, rng: &mut Rng) -> Built {
    const BASE: i32 = 39;
    let origin = IVec3::new(centre.x - 29, BASE, centre.y - 29);
    let mut b = Builder::new(origin, IVec2::new(58, 58), rot);
    let (pr, br, dark, lamp) = (ob::PRISMARINE, ob::PRISMARINE_BRICKS, ob::DARK_PRISMARINE, ob::SEA_LANTERN);
    let w = Block::WATER;
    // Legs down to the sea floor.
    for z in (0..58).step_by(6) {
        for x in (0..58).step_by(6) {
            b.foundation(g, x, -1, z, pr, 30);
        }
    }
    for z in 0..58 {
        for x in [0, 57] {
            b.foundation(g, x, -1, z, pr, 30);
        }
        b.foundation(g, z, -1, 0, pr, 30);
        b.foundation(g, z, -1, 57, pr, 30);
    }
    // The ground floor: a flooded hall under the whole building.
    b.fill(0, 0, 0, 57, 0, 57, br);
    for y in 1..=7 {
        for z in 0..58 {
            for x in 0..58 {
                let edge = x == 0 || x == 57 || z == 0 || z == 57;
                let lamp_spot = edge && y == 4 && (x + z) % 6 == 3;
                b.set(
                    x,
                    y,
                    z,
                    if lamp_spot {
                        lamp
                    } else if edge {
                        pr
                    } else {
                        w
                    },
                );
            }
        }
    }
    b.fill(0, 8, 0, 57, 8, 57, br);
    // Pillars holding up the hall.
    for z in (6..52).step_by(9) {
        for x in (6..52).step_by(9) {
            b.fill(x, 1, z, x + 1, 7, z + 1, br);
            b.set(x, 4, z, lamp);
        }
    }
    // The entrance in the front wall.
    b.fill(25, 1, 0, 32, 6, 0, w);
    // Two wings with rooms for the elder guardians.
    for x0 in [2, 39] {
        b.room(x0, 9, 22, x0 + 16, 15, 55, pr, w);
        b.fill(x0, 16, 22, x0 + 16, 16, 55, br);
        b.fill(x0 + 7, 9, 34, x0 + 9, 9, 40, w);
        for z in (25..55).step_by(6) {
            b.set(x0, 12, z, lamp);
            b.set(x0 + 16, 12, z, lamp);
        }
        // Sponge rooms tucked under the wing roofs.
        if rng.below(2) == 0 {
            b.fill(x0 + 3, 15, 44, x0 + 6, 15, 47, ob::WET_SPONGE);
        }
    }
    // The central tower over the core.
    b.room(20, 9, 20, 37, 20, 49, pr, w);
    b.fill(20, 21, 20, 37, 21, 49, br);
    b.fill(24, 22, 24, 33, 22, 45, dark);
    b.fill(26, 23, 26, 31, 23, 43, dark);
    b.fill(27, 9, 27, 30, 9, 30, w);
    for z in (22..48).step_by(5) {
        b.set(20, 16, z, lamp);
        b.set(37, 16, z, lamp);
    }
    // Open passages through the hall ceiling into each upper room.
    for x0 in [2, 39] {
        b.fill(x0 + 7, 8, 34, x0 + 9, 9, 36, w);
    }
    b.fill(27, 8, 27, 30, 9, 29, w);
    // The core room: dark prismarine around eight blocks of gold.
    b.room(24, 10, 30, 33, 16, 39, dark, w);
    b.fill(24, 17, 30, 33, 17, 39, dark);
    b.fill(28, 12, 34, 29, 13, 35, Block::GOLD_BLOCK);
    b.set(25, 13, 31, lamp);
    b.set(32, 13, 38, lamp);
    b.set(28, 10, 30, w);
    b.set(28, 11, 30, w);
    for (x, y, z) in [(10, 12, 38), (47, 12, 38), (29, 18, 43)] {
        b.residents.push((crate::entity::MobKind::ElderGuardian, b.world(x, y, z)));
    }
    b.finish(Kind::Monument, 4)
}

// Supported entries retain Java's pool weights, counts and roll ranges.
// Entries whose systems have not landed yet stay empty at their original
// weight rather than redistributing their probability to another item.
const DESERTPYRAMID_0: &[L] = &[
    L { item: Some(Item::DIAMOND), weight: 5, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::IRON_INGOT), weight: 15, lo: 1, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::GOLD_INGOT), weight: 15, lo: 2, hi: 7, enchanted: false, damage: None },
    L { item: Some(Item::EMERALD), weight: 15, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::BONE), weight: 25, lo: 4, hi: 6, enchanted: false, damage: None },
    L { item: Some(Item::SPIDER_EYE), weight: 25, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::ROTTEN_FLESH), weight: 25, lo: 3, hi: 7, enchanted: false, damage: None },
    L { item: None, weight: 20, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 15, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 10, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::BOOK), weight: 20, lo: 1, hi: 1, enchanted: true, damage: None },
    L { item: Some(Item::GOLDEN_APPLE), weight: 20, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 2, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 15, lo: 1, hi: 1, enchanted: false, damage: None },
];
const DESERTPYRAMID_1: &[L] = &[
    L { item: Some(Item::BONE), weight: 10, lo: 1, hi: 8, enchanted: false, damage: None },
    L { item: Some(Item::GUNPOWDER), weight: 10, lo: 1, hi: 8, enchanted: false, damage: None },
    L { item: Some(Item::ROTTEN_FLESH), weight: 10, lo: 1, hi: 8, enchanted: false, damage: None },
    L { item: Some(Item::STRING), weight: 10, lo: 1, hi: 8, enchanted: false, damage: None },
    L { item: Some(Item::from_block(Block::SAND)), weight: 10, lo: 1, hi: 8, enchanted: false, damage: None },
];
const DESERTPYRAMID_2: &[L] = &[
    L { item: None, weight: 6, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 2, hi: 2, enchanted: false, damage: None },
];
const JUNGLETEMPLE_0: &[L] = &[
    L { item: Some(Item::DIAMOND), weight: 3, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::IRON_INGOT), weight: 10, lo: 1, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::GOLD_INGOT), weight: 15, lo: 2, hi: 7, enchanted: false, damage: None },
    L { item: Some(Item::from_block(ob::BAMBOO)), weight: 15, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::EMERALD), weight: 2, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::BONE), weight: 20, lo: 4, hi: 6, enchanted: false, damage: None },
    L { item: Some(Item::ROTTEN_FLESH), weight: 16, lo: 3, hi: 7, enchanted: false, damage: None },
    L { item: None, weight: 3, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::BOOK), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
];
const JUNGLETEMPLE_1: &[L] = &[
    L { item: None, weight: 2, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 2, hi: 2, enchanted: false, damage: None },
];
const IGLOO_0: &[L] = &[
    L { item: Some(Item::APPLE), weight: 15, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::COAL), weight: 15, lo: 1, hi: 4, enchanted: false, damage: None },
    L { item: Some(Item::GOLD_NUGGET), weight: 10, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::tool(ToolKind::Axe, Tier::Stone)), weight: 2, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::ROTTEN_FLESH), weight: 10, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::EMERALD), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::WHEAT), weight: 10, lo: 2, hi: 3, enchanted: false, damage: None },
];
const IGLOO_1: &[L] = &[L { item: Some(Item::GOLDEN_APPLE), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }];
const OUTPOST_0: &[L] = &[L { item: Some(Item::CROSSBOW), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }];
const OUTPOST_1: &[L] = &[
    L { item: Some(Item::WHEAT), weight: 7, lo: 3, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::POTATO), weight: 5, lo: 2, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::CARROT), weight: 5, lo: 3, hi: 5, enchanted: false, damage: None },
];
const OUTPOST_2: &[L] =
    &[L { item: Some(Item::from_block(Block::DARK_OAK_LOG)), weight: 1, lo: 2, hi: 3, enchanted: false, damage: None }];
const OUTPOST_3: &[L] = &[
    L { item: None, weight: 7, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::STRING), weight: 4, lo: 1, hi: 6, enchanted: false, damage: None },
    L { item: Some(Item::ARROW), weight: 4, lo: 2, hi: 7, enchanted: false, damage: None },
    L { item: Some(Item::from_block(super::gadgets::HOOK)), weight: 3, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::IRON_INGOT), weight: 3, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::BOOK), weight: 1, lo: 1, hi: 1, enchanted: true, damage: None },
];
const OUTPOST_4: &[L] = &[L { item: None, weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }];
const OUTPOST_5: &[L] = &[
    L { item: None, weight: 3, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 2, hi: 2, enchanted: false, damage: None },
];
const SHIPSUPPLY_0: &[L] = &[
    L { item: Some(Item::PAPER), weight: 8, lo: 1, hi: 12, enchanted: false, damage: None },
    L { item: Some(Item::POTATO), weight: 7, lo: 2, hi: 6, enchanted: false, damage: None },
    L { item: Some(Item::from_block(ob::MOSS_BLOCK)), weight: 7, lo: 1, hi: 4, enchanted: false, damage: None },
    L { item: Some(Item::POISONOUS_POTATO), weight: 7, lo: 2, hi: 6, enchanted: false, damage: None },
    L { item: Some(Item::CARROT), weight: 7, lo: 4, hi: 8, enchanted: false, damage: None },
    L { item: Some(Item::WHEAT), weight: 7, lo: 8, hi: 21, enchanted: false, damage: None },
    L { item: None, weight: 10, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::COAL), weight: 6, lo: 2, hi: 8, enchanted: false, damage: None },
    L { item: Some(Item::ROTTEN_FLESH), weight: 5, lo: 5, hi: 24, enchanted: false, damage: None },
    L { item: Some(Item::from_block(Block::PUMPKIN)), weight: 2, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::from_block(ob::BAMBOO)), weight: 2, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::GUNPOWDER), weight: 3, lo: 1, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::from_block(Block::TNT)), weight: 1, lo: 1, hi: 2, enchanted: false, damage: None },
    L {
        item: Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Leather)),
        weight: 3,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    },
    L {
        item: Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather)),
        weight: 3,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    },
    L {
        item: Some(Item::armor(ArmorPiece::Leggings, ArmorMaterial::Leather)),
        weight: 3,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    },
    L {
        item: Some(Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather)),
        weight: 3,
        lo: 1,
        hi: 1,
        enchanted: true,
        damage: None,
    },
];
const SHIPSUPPLY_1: &[L] = &[
    L { item: None, weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 2, hi: 2, enchanted: false, damage: None },
];
const SHIPTREASURE_0: &[L] = &[
    L { item: Some(Item::IRON_INGOT), weight: 90, lo: 1, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::GOLD_INGOT), weight: 10, lo: 1, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::EMERALD), weight: 40, lo: 1, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::DIAMOND), weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
];
const SHIPTREASURE_1: &[L] = &[
    L { item: Some(Item::IRON_NUGGET), weight: 50, lo: 1, hi: 10, enchanted: false, damage: None },
    L { item: Some(Item::GOLD_NUGGET), weight: 10, lo: 1, hi: 10, enchanted: false, damage: None },
    L { item: Some(Item::LAPIS_LAZULI), weight: 20, lo: 1, hi: 10, enchanted: false, damage: None },
];
const SHIPTREASURE_2: &[L] = &[
    L { item: None, weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 2, hi: 2, enchanted: false, damage: None },
];
const SHIPMAP_0: &[L] = &[L { item: None, weight: 1, lo: 1, hi: 1, enchanted: false, damage: None }];
const SHIPMAP_1: &[L] = &[
    L { item: Some(Item::COMPASS), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::CLOCK), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::PAPER), weight: 20, lo: 1, hi: 10, enchanted: false, damage: None },
    L { item: Some(Item::FEATHER), weight: 10, lo: 1, hi: 5, enchanted: false, damage: None },
    L { item: Some(Item::BOOK), weight: 5, lo: 1, hi: 5, enchanted: false, damage: None },
];
const SHIPMAP_2: &[L] = &[
    L { item: None, weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: None, weight: 1, lo: 2, hi: 2, enchanted: false, damage: None },
];
const RUINSMALL_0: &[L] = &[
    L { item: Some(Item::COAL), weight: 10, lo: 1, hi: 4, enchanted: false, damage: None },
    L { item: Some(Item::tool(ToolKind::Axe, Tier::Stone)), weight: 2, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::ROTTEN_FLESH), weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::EMERALD), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::WHEAT), weight: 10, lo: 2, hi: 3, enchanted: false, damage: None },
];
const RUINSMALL_1: &[L] = &[
    L {
        item: Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    },
    L {
        item: Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    },
    L { item: Some(Item::FISHING_ROD), weight: 5, lo: 1, hi: 1, enchanted: true, damage: None },
    L { item: None, weight: 5, lo: 1, hi: 1, enchanted: false, damage: None },
];
const RUINBIG_0: &[L] = &[
    L { item: Some(Item::COAL), weight: 10, lo: 1, hi: 4, enchanted: false, damage: None },
    L { item: Some(Item::GOLD_NUGGET), weight: 10, lo: 1, hi: 3, enchanted: false, damage: None },
    L { item: Some(Item::EMERALD), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::WHEAT), weight: 10, lo: 2, hi: 3, enchanted: false, damage: None },
];
const RUINBIG_1: &[L] = &[
    L { item: Some(Item::GOLDEN_APPLE), weight: 1, lo: 1, hi: 1, enchanted: false, damage: None },
    L { item: Some(Item::BOOK), weight: 5, lo: 1, hi: 1, enchanted: true, damage: None },
    L {
        item: Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    },
    L {
        item: Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Gold)),
        weight: 1,
        lo: 1,
        hi: 1,
        enchanted: false,
        damage: None,
    },
    L { item: Some(Item::FISHING_ROD), weight: 5, lo: 1, hi: 1, enchanted: true, damage: None },
    L { item: None, weight: 10, lo: 1, hi: 1, enchanted: false, damage: None },
];

/// Fills a structure's chest from its loot table.
pub fn loot(seed: u64, table: LootTable) -> Chest {
    let mut rng = Rng(seed ^ 0x7E3D_1007);
    let mut chest = Chest::default();
    match table {
        LootTable::Dispenser => {
            // Dispensers expose only their first nine backing slots.
            for slot in 0..rng.range(1, 2) as usize {
                chest.slots[slot] = Some(Stack::new(Item::ARROW, rng.range(2, 7) as u8));
            }
        }
        LootTable::DesertPyramid => {
            pool(&mut chest, &mut rng, DESERTPYRAMID_0, (2, 4));
            pool(&mut chest, &mut rng, DESERTPYRAMID_1, (4, 4));
            pool(&mut chest, &mut rng, DESERTPYRAMID_2, (1, 1));
        }
        LootTable::JungleTemple => {
            pool(&mut chest, &mut rng, JUNGLETEMPLE_0, (2, 6));
            pool(&mut chest, &mut rng, JUNGLETEMPLE_1, (1, 1));
        }
        LootTable::Igloo => {
            pool(&mut chest, &mut rng, IGLOO_0, (2, 8));
            pool(&mut chest, &mut rng, IGLOO_1, (1, 1));
        }
        LootTable::Outpost => {
            pool(&mut chest, &mut rng, OUTPOST_0, (0, 1));
            pool(&mut chest, &mut rng, OUTPOST_1, (2, 3));
            pool(&mut chest, &mut rng, OUTPOST_2, (1, 3));
            pool(&mut chest, &mut rng, OUTPOST_3, (2, 3));
            pool(&mut chest, &mut rng, OUTPOST_4, (0, 1));
            pool(&mut chest, &mut rng, OUTPOST_5, (1, 1));
        }
        LootTable::ShipSupply => {
            pool(&mut chest, &mut rng, SHIPSUPPLY_0, (3, 10));
            pool(&mut chest, &mut rng, SHIPSUPPLY_1, (1, 1));
        }
        LootTable::ShipTreasure => {
            pool(&mut chest, &mut rng, SHIPTREASURE_0, (3, 6));
            pool(&mut chest, &mut rng, SHIPTREASURE_1, (2, 5));
            pool(&mut chest, &mut rng, SHIPTREASURE_2, (1, 1));
        }
        LootTable::ShipMap => {
            pool(&mut chest, &mut rng, SHIPMAP_0, (1, 1));
            pool(&mut chest, &mut rng, SHIPMAP_1, (3, 3));
            pool(&mut chest, &mut rng, SHIPMAP_2, (1, 1));
        }
        LootTable::RuinSmall => {
            pool(&mut chest, &mut rng, RUINSMALL_0, (2, 8));
            pool(&mut chest, &mut rng, RUINSMALL_1, (1, 1));
        }
        LootTable::RuinBig => {
            pool(&mut chest, &mut rng, RUINBIG_0, (2, 8));
            pool(&mut chest, &mut rng, RUINBIG_1, (1, 1));
        }
    }
    chest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_seeded_starts_match_reference_coordinates() {
        for (seed, region, kind, expected) in [
            (1, IVec2::ZERO, Kind::DesertPyramid, IVec2::new(14, 10)),
            (5, IVec2::new(-3, 2), Kind::Shipwreck, IVec2::new(-59, 65)),
            (12345, IVec2::new(-7, -8), Kind::Monument, IVec2::new(-216, -242)),
        ] {
            assert_eq!(Temples::new(seed).start(kind, region), expected);
        }
    }

    #[test]
    fn rotated_builds_and_their_features_fit_the_discovery_radius() {
        let g = Generator::new(1);
        let centre = IVec2::new(-40, -56);
        for kind in Kind::ALL {
            for rot in 0..4 {
                let mut rng = Rng(3);
                let built = match kind {
                    Kind::DesertPyramid => desert_pyramid(&g, centre, 70, rot, 9),
                    Kind::JungleTemple => jungle_temple(&g, centre, 70, rot, 9, &mut rng),
                    Kind::SwampHut => swamp_hut(&g, centre, 70, rot),
                    Kind::Igloo => igloo(&g, centre, 70, rot, 9, &mut rng),
                    Kind::Outpost => outpost(&g, centre, 70, rot, 9),
                    Kind::Shipwreck => shipwreck(&g, centre, 40, rot, 9, &mut rng),
                    Kind::OceanRuin => ocean_ruin(&g, centre, 40, rot, 9, &mut rng),
                    Kind::Monument => monument(&g, centre, rot, &mut rng),
                };
                assert!(
                    built.bounds.contains(IVec3::new(centre.x, built.bounds.min.y, centre.y)),
                    "{kind:?} rotation {rot} moves off its start"
                );
                for &(p, _) in &built.blocks {
                    let offset = IVec2::new(p.x, p.z) - centre;
                    assert!(offset.abs().max_element() <= kind.reach(), "{kind:?} rotation {rot} clips {p}");
                }
                for (p, feature) in &built.features {
                    assert!(built.bounds.contains(*p));
                    let block = built.blocks.iter().find(|(q, _)| q == p).unwrap().1;
                    match feature {
                        Feature::TempleChest(..) => assert!(super::super::chest::is_chest(block)),
                        Feature::UtilityBlock(site) => assert_eq!(block.base(), *site),
                        Feature::IglooBrewing => assert_eq!(block, Block::BREWING_STAND),
                        _ => panic!("unexpected feature in {kind:?}"),
                    }
                }
                if kind == Kind::JungleTemple {
                    for &(p, block) in &built.blocks {
                        if let Some((facing, _, _)) = super::super::gadgets::hook_state(block) {
                            let support = p - facing.offset();
                            assert!(built.blocks.iter().find(|(q, _)| *q == support).unwrap().1.is_solid());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dispenser_loot_fits_the_visible_inventory() {
        for seed in 0..256 {
            let chest = loot(seed, LootTable::Dispenser);
            assert!(chest.slots[..9].iter().flatten().all(|s| s.item == Item::ARROW && (2..=7).contains(&s.count)));
            assert!((1..=2).contains(&chest.slots[..9].iter().flatten().count()));
            assert!(chest.slots[9..].iter().all(Option::is_none));
            assert!(loot(seed, LootTable::Igloo).slots.iter().flatten().any(|s| s.item == Item::GOLDEN_APPLE));
            assert!(
                !loot(seed, LootTable::ShipTreasure).slots.iter().flatten().any(|s| s.item == Item::HEART_OF_THE_SEA)
            );
        }
    }

    #[test]
    fn locate_obeys_radius_and_dimension() {
        for dimension in [Dimension::Nether, Dimension::End] {
            let g = Generator::for_dimension(1, dimension);
            assert!(g.temples.nearest(&g, Kind::Shipwreck, IVec2::ZERO, 6000).is_none());
            assert!(g.temples.at(&g, IVec3::new(8, 40, 8)).is_none());
        }
        let g = Generator::new(1);
        assert!(g.temples.nearest(&g, Kind::Shipwreck, IVec2::ZERO, 1).is_none());
        let at = g.temples.nearest(&g, Kind::Shipwreck, IVec2::ZERO, 6000).unwrap();
        let distance = IVec2::new(at.x, at.z).as_dvec2().length();
        assert!(distance <= 6000.0);
        assert!(g.temples.nearest(&g, Kind::Shipwreck, IVec2::ZERO, distance.floor() as i32 - 1).is_none());
        assert_eq!(g.temples.nearest(&g, Kind::Shipwreck, IVec2::new(at.x, at.z), 0), Some(at));
    }

    #[test]
    fn structure_containers_register_once_and_survive_regeneration() {
        use super::super::chunk::{chunk_of, local_of};
        let g = Arc::new(Generator::new(1));
        let at = g.temples.nearest(&g, Kind::JungleTemple, IVec2::ZERO, 8000).unwrap();
        let built = g.temples.at(&g, at).unwrap();
        let mut world = super::super::World::new_headless(g.clone(), Default::default(), 1);
        for &(p, feature) in &built.features {
            let Feature::TempleChest(_, table) = feature else { continue };
            let chunk = chunk_of(p);
            let data = g.generate(chunk);
            let l = local_of(p);
            assert!(super::super::chest::is_chest(data.get(l.x as usize, l.y as usize, l.z as usize)));
            world.register_structure_features(chunk, &data);
            let chest = world.chest(p).expect("generated container registered");
            assert!(chest.slots.iter().any(Option::is_some));
            if table == LootTable::Dispenser {
                assert!(chest.slots[..9].iter().any(Option::is_some));
                assert!(chest.slots[9..].iter().all(Option::is_none));
            }
            world.chest_mut(p).unwrap().slots.fill(None);
            world.register_structure_features(chunk, &data);
            assert!(world.chest(p).unwrap().slots.iter().all(Option::is_none));
        }
    }

    #[test]
    fn generated_jungle_tripwire_fires_its_loaded_dispenser() {
        use super::super::chunk::chunk_of;
        let g = Arc::new(Generator::new(1));
        let at = g.temples.nearest(&g, Kind::JungleTemple, IVec2::ZERO, 8000).unwrap();
        let built = g.temples.at(&g, at).unwrap();
        let mut world = super::super::World::new_headless(g.clone(), Default::default(), 1);
        let (lo, hi) = (chunk_of(built.bounds.min), chunk_of(built.bounds.max));
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let cpos = IVec3::new(x, y, z);
                    world.insert_chunk(cpos, Arc::new(g.generate(cpos)), false);
                }
            }
        }
        let wire = built.blocks.iter().find(|(_, b)| super::super::gadgets::is_tripwire(*b)).unwrap().0;
        world.redstone.contacts.entry(wire).or_default().all = 1;
        world.refresh_tripwires();
        for _ in 0..8 {
            world.tick_redstone();
        }
        let mut entities = crate::entity::Entities::new(7);
        world.tick_automation_entities(&mut entities);
        assert_eq!(entities.arrows.len(), 1, "entering generated tripwire fires one arrow");
    }

    #[test]
    fn all_residents_fit_their_rotated_rooms() {
        use crate::{
            entity::MobKind,
            physics::{self, test_util::Grid},
        };
        let g = Generator::new(1);
        for rot in 0..4 {
            let buildings = [
                swamp_hut(&g, IVec2::ZERO, 70, rot),
                igloo(&g, IVec2::ZERO, 70, rot, 1, &mut Rng(1)),
                outpost(&g, IVec2::ZERO, 70, rot, 1),
                monument(&g, IVec2::ZERO, rot, &mut Rng(1)),
            ];
            for b in buildings {
                let mut grid = Grid::flat(-64);
                for &(p, block) in &b.blocks {
                    grid.set(p, block);
                }
                for &(kind, p) in &b.residents {
                    assert!(
                        !physics::overlaps_solid(&grid, p.as_dvec3() + glam::DVec3::new(0.5, 0.0, 0.5), kind.shape()),
                        "{:?} {kind:?} rot{rot} {p}",
                        b.kind
                    );
                }
                if b.kind == Kind::Monument {
                    assert_eq!(b.residents.iter().filter(|(k, _)| *k == MobKind::ElderGuardian).count(), 3);
                }
            }
        }
    }

    #[test]
    fn igloo_basement_ladder_is_open_all_the_way_to_the_floor() {
        let g = Generator::new(1);
        let centre = IVec2::ZERO;
        let built = (0..16)
            .find_map(|seed| {
                let mut rng = Rng(seed);
                let built = igloo(&g, centre, 70, 0, seed, &mut rng);
                built.features.iter().any(|(_, f)| matches!(f, Feature::TempleChest(..))).then_some(built)
            })
            .unwrap();
        for y in 59..=69 {
            let p = IVec3::new(0, y, 0);
            assert_eq!(built.blocks.iter().find(|(q, _)| *q == p).unwrap().1.base(), Block::LADDER);
        }
    }

    #[test]
    fn igloo_utilities_register_and_keep_their_inventory() {
        use super::super::chunk::chunk_of;
        let g = Arc::new(Generator::new(1));
        let region = IVec2::ZERO;
        let centre = g.temples.start(Kind::Igloo, region) * 16 + IVec2::splat(8);
        let ground = g.column(centre.x, centre.y).height;
        let built = (0..16)
            .find_map(|seed| {
                let mut rng = Rng(seed);
                let built = igloo(&g, centre, ground, 1, seed, &mut rng);
                built.features.iter().any(|(_, f)| matches!(f, Feature::TempleChest(..))).then_some(built)
            })
            .unwrap();
        let built = Arc::new(built);
        g.temples.cache.lock().unwrap().insert((Kind::Igloo, region), Some(built.clone()));
        let mut world = super::super::World::new_headless(g.clone(), Default::default(), 1);
        let mut utilities = 0;
        for &(p, feature) in &built.features {
            let block = match feature {
                Feature::UtilityBlock(block) => block,
                Feature::IglooBrewing => Block::BREWING_STAND,
                _ => continue,
            };
            utilities += 1;
            let data = g.generate(chunk_of(p));
            world.register_structure_features(chunk_of(p), &data);
            if block == Block::FURNACE {
                let fuel = Some(Stack::new(Item::COAL, 3));
                world.furnace_mut(p).expect("usable generated furnace").fuel = fuel;
                world.register_structure_features(chunk_of(p), &data);
                assert_eq!(world.furnace(p).unwrap().fuel, fuel);
            } else {
                assert_eq!(
                    world.brewing_stand(p).unwrap().bottles[0].unwrap().item,
                    Item::splash_potion(crate::potion::Potion::from_id("weakness").unwrap())
                );
                let fuel = Some(Stack::new(Item::BLAZE_POWDER, 2));
                world.brewing_stand_mut(p).expect("usable generated brewing stand").fuel = fuel;
                world.register_structure_features(chunk_of(p), &data);
                assert_eq!(world.brewing_stand(p).unwrap().fuel, fuel);
            }
        }
        assert_eq!(utilities, 2);
    }

    #[test]
    fn every_kind_names_itself_and_places_in_its_biomes() {
        for k in Kind::ALL {
            assert_eq!(Kind::from_name(k.name()), Some(k));
        }
        assert_eq!(Kind::from_name("jungle_temple"), Some(Kind::JungleTemple));
        assert!(Kind::Monument.allowed(Biome::DeepOcean) && !Kind::Monument.allowed(Biome::Ocean));
        assert!(Kind::DesertPyramid.allowed(Biome::Desert));
    }

    #[test]
    fn starts_keep_their_separation() {
        let t = Temples::new(5);
        for kind in Kind::ALL {
            let (spacing, separation, _) = kind.placement();
            for r in -3..3 {
                let s = t.start(kind, IVec2::new(r, -r));
                let off = s - IVec2::new(r, -r) * spacing;
                assert!(
                    off.cmpge(IVec2::ZERO).all() && off.cmplt(IVec2::splat(spacing - separation)).all(),
                    "{kind:?}"
                );
            }
        }
    }

    #[test]
    fn every_structure_paints_its_blocks_across_chunk_boundaries() {
        let g = Generator::new(1);
        let mut found = 0;
        for kind in Kind::ALL {
            let at =
                g.temples.nearest(&g, kind, IVec2::ZERO, 8000).expect("seed 1 has every structure within 8000 blocks");
            found += 1;
            let built = g.temples.at(&g, at).or_else(|| {
                let q = IVec2::new(at.x, at.z);
                g.temples.around(&g, q, q).into_iter().find(|b| b.kind == kind)
            });
            let built = built.expect("the located structure is found again");
            assert!(!built.blocks.is_empty());
            let mut chunks = FxHashMap::default();
            for &(p, block) in &built.blocks {
                let cpos = super::super::chunk::chunk_of(p);
                let painted = chunks.entry(cpos).or_insert_with(|| {
                    let mut blocks = [Block::STONE; CHUNK_VOLUME];
                    g.temples.paint(&g, &mut blocks, cpos * CHUNK_SIZE_I);
                    blocks
                });
                let l = super::super::chunk::local_of(p);
                assert_eq!(painted[index(l.x as usize, l.y as usize, l.z as usize)], block, "{kind:?} at {p}");
            }
            // Eviction and regeneration cannot change seams or loot seeds.
            g.temples.cache.lock().unwrap().clear();
            let rebuilt = g.temples.at(&g, at).unwrap();
            assert_eq!(built.blocks, rebuilt.blocks);

            for (p, f) in &built.features {
                if let Feature::TempleChest(seed, table) = f {
                    let chest = loot(*seed, *table);
                    assert!(chest.slots.iter().any(Option::is_some), "{kind:?} chest at {p} is filled");
                }
            }
        }
        assert_eq!(found, Kind::ALL.len());
        // Loot is deterministic.
        assert_eq!(loot(9, LootTable::DesertPyramid).slots, loot(9, LootTable::DesertPyramid).slots);
    }
}
