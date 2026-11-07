//! Generation 3 trial chambers (v0.16 Track C): Java 26.3's
//! `worldgen/structure/trial_chambers` and its structure set, built by the
//! jigsaw engine from original templates (`jigsaw/trial_chambers.txt`).
//!
//! Placement is Java's `random_spread` (spacing 34, separation 12, salt
//! 94251327); the start sits at a uniform height in -40..=-20, its piece
//! floor one below like Java's ground-level delta; pieces stay within 116
//! blocks of the start and 20 jigsaw steps (`size`). The biome check reads
//! the 3D biome at the generation stub, so deep-dark starts are refused.
//! Pool aliases are resolved once per structure: ranged and slow ranged
//! spawners share a skeleton, stray or bogged group, melee picks zombie,
//! husk or spider, small melee slime, cave spider, silverfish or baby
//! zombie. Processors: the copper bulb degradation rules (and every bulb
//! placed lit), loot containers as barrels or dispensers by table.
use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME};
use super::fortress::Feature;
use super::jigsaw::{self, Base, Marker, Palette, Placed};
use super::noise::hash3;
use super::structure::Bounds;
use super::terrain::{Biome, Dimension, Generation, Generator};

pub const SPACING: i32 = 34;
pub const SEPARATION: i32 = 12;
pub const SALT: u64 = 94_251_327;
/// `max_distance_from_center`.
pub const MAX_DISTANCE: i32 = 116;
/// `size`: the jigsaw depth.
pub const MAX_DEPTH: u8 = 20;
const REACH: i32 = MAX_DISTANCE + 32;
const CACHE_LIMIT: usize = 256;

/// Loot tables the templates name (chests, barrels and dispensers).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Table {
    Corridor,
    Entrance,
    Intersection,
    IntersectionBarrel,
    Supply,
    DispenserChamber,
    DispenserCorridor,
    DispenserWater,
}

impl Table {
    pub const ALL: [Table; 8] = [
        Table::Corridor,
        Table::Entrance,
        Table::Intersection,
        Table::IntersectionBarrel,
        Table::Supply,
        Table::DispenserChamber,
        Table::DispenserCorridor,
        Table::DispenserWater,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Table::Corridor => "trial_chambers/corridor",
            Table::Entrance => "trial_chambers/entrance",
            Table::Intersection => "trial_chambers/intersection",
            Table::IntersectionBarrel => "trial_chambers/intersection_barrel",
            Table::Supply => "trial_chambers/supply",
            Table::DispenserChamber => "dispensers/trial_chambers/chamber",
            Table::DispenserCorridor => "dispensers/trial_chambers/corridor",
            Table::DispenserWater => "dispensers/trial_chambers/water",
        }
    }

    pub fn from_name(name: &str) -> Option<Table> {
        Table::ALL.into_iter().find(|t| t.name() == name)
    }

    fn container(self) -> Container {
        match self {
            Table::IntersectionBarrel => Container::Barrel,
            Table::DispenserChamber | Table::DispenserCorridor | Table::DispenserWater => Container::Dispenser,
            _ => Container::Chest,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Chest,
    Barrel,
    Dispenser,
}

/// Java's biome tag `has_structure/trial_chambers`: every overworld biome
/// but the deep dark.
pub fn allowed(b: Biome) -> bool {
    b != Biome::DeepDark
}

/// The structure's pool alias choices (`pool_aliases`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aliases {
    /// 0 skeleton, 1 stray, 2 bogged (`random_group`, shared by ranged
    /// and slow ranged).
    pub ranged: u8,
    /// 0 zombie, 1 husk, 2 spider.
    pub melee: u8,
    /// 0 slime, 1 cave spider, 2 silverfish, 3 baby zombie.
    pub small_melee: u8,
}

impl Aliases {
    /// The spawner configuration for a template's `trial:<alias>` name at
    /// `at` (`trial_chambers/spawner/all` picks one of its three
    /// connectors per position).
    pub fn config(&self, alias: &str, at: IVec3, seed: u64) -> Option<&'static str> {
        let alias = match alias {
            "all" => ["ranged", "melee", "small_melee"][(hash3(at.x, at.y, at.z, seed ^ 0xA11) % 3) as usize],
            a => a,
        };
        Some(match alias {
            "breeze" => "trial_chamber/breeze",
            "ranged" => {
                ["trial_chamber/ranged/skeleton", "trial_chamber/ranged/stray", "trial_chamber/ranged/poison_skeleton"]
                    [self.ranged as usize]
            }
            "slow_ranged" => [
                "trial_chamber/slow_ranged/skeleton",
                "trial_chamber/slow_ranged/stray",
                "trial_chamber/slow_ranged/poison_skeleton",
            ][self.ranged as usize],
            "melee" => ["trial_chamber/melee/zombie", "trial_chamber/melee/husk", "trial_chamber/melee/spider"]
                [self.melee as usize],
            "small_melee" => [
                "trial_chamber/small_melee/slime",
                "trial_chamber/small_melee/cave_spider",
                "trial_chamber/small_melee/silverfish",
                "trial_chamber/small_melee/baby_zombie",
            ][self.small_melee as usize],
            _ => return None,
        })
    }
}

struct ChamberPalette;

impl ChamberPalette {
    /// The loot table of the template cell a container lands at.
    fn table_at(at: IVec3, piece: &Placed) -> Option<(Table, Option<super::block::Facing>)> {
        let lib = jigsaw::trial_chambers();
        let t = &lib.templates[piece.template as usize];
        let local = jigsaw::rotate(at - piece.origin, (4 - (piece.rotation & 3)) & 3);
        t.cells.iter().find(|c| c.pos == local).and_then(|c| match c.spec.base {
            Base::Loot(i) => Table::from_name(&lib.loot[i as usize])
                .map(|table| (table, c.spec.facing.map(|f| jigsaw::rotate_facing(f, piece.rotation)))),
            _ => None,
        })
    }
}

impl Palette for ChamberPalette {
    fn role(&self, _role: &str) -> Option<Block> {
        None
    }

    fn process(&self, block: Block, at: IVec3, piece: &Placed) -> Option<Block> {
        if block == Block::SPAWNER {
            return Some(super::trial::TRIAL_SPAWNER);
        }
        if block.base() == Block::CHEST
            && let Some((table, facing)) = Self::table_at(at, piece)
        {
            use super::redstone_blocks as r;
            return Some(match table.container() {
                Container::Chest => block,
                Container::Barrel => Block::BARREL,
                // Ceiling dispensers face down; wall ones face their spec.
                Container::Dispenser => match facing {
                    Some(f) => {
                        r::dispenser(super::block::Facing::ALL.iter().position(|&g| g == f).unwrap() as u8, false)
                    }
                    None => r::dispenser(5, false),
                },
            });
        }
        // `trial_chambers_copper_bulb_degradation`: waxed bulbs, placed lit,
        // weather by Java's rule order and position-seeded random.
        if let Some(c) = super::copper::state(block)
            && c.family == super::copper::Family::Bulb
            && c.waxed
        {
            use super::copper::{Copper, Stage, encode};
            let mut random = jigsaw::processor_random(at);
            let stage = if random.next_float() < 0.1 {
                Stage::Oxidized
            } else if random.next_float() < 0.333_333_34 {
                Stage::Weathered
            } else if random.next_float() < 0.5 {
                Stage::Exposed
            } else {
                c.stage
            };
            return Some(encode(Copper { stage, local: 1, ..c }));
        }
        Some(block)
    }

    fn clear_above(&self) -> i32 {
        0
    }
}

/// One trial chamber layout.
#[derive(Debug)]
pub struct Chamber {
    /// The generation stub (the start piece's centre at its floor).
    pub at: IVec3,
    pub pieces: Vec<Placed>,
    pub aliases: Aliases,
    pub seed: u64,
}

impl Chamber {
    /// Whether `p` lies in one of the pieces (Java's `spawn_overrides`
    /// with `bounding_box: piece`, which empty every category inside).
    pub fn holds(&self, p: IVec3) -> bool {
        self.pieces.iter().any(|piece| piece.bounds.contains(p))
    }

    pub fn bounds(&self) -> Bounds {
        self.pieces.iter().map(|p| p.bounds).reduce(|a, b| a.union(&b)).unwrap()
    }
}

pub struct TrialChambers {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<Chamber>>>>,
}

impl TrialChambers {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    pub fn start_chunk(&self, region: IVec2) -> IVec2 {
        super::trail_ruins::spread(self.seed, region, SPACING, SEPARATION, SALT)
    }

    pub fn get(&self, g: &Generator, region: IVec2) -> Option<Arc<Chamber>> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return None;
        }
        if let Some(v) = self.cache.lock().unwrap().get(&region).cloned() {
            return v;
        }
        let at = self.start_chunk(region) * 16;
        let seed = hash3(at.x, 0, at.y, self.seed ^ SALT);
        // `start_height`: uniform -40..=-20 (absolute).
        let start_y = -40 + (seed % 21) as i32;
        let pieces = jigsaw::assemble(
            jigsaw::trial_chambers(),
            &jigsaw::Settings {
                start_pool: "trial_chambers/chamber/end",
                max_depth: MAX_DEPTH,
                max_distance: MAX_DISTANCE,
            },
            at,
            &|_, _| start_y - 1,
            seed,
        );
        let a = seed.rotate_left(17);
        let aliases = Aliases { ranged: (a % 3) as u8, melee: (a / 3 % 3) as u8, small_melee: (a / 9 % 4) as u8 };
        let found = pieces.first().copied().and_then(|p| {
            let mut at = (p.bounds.min + p.bounds.max) / 2;
            at.y = p.origin.y + 1;
            allowed(g.biome_at(at)).then(|| Arc::new(Chamber { at, pieces, aliases, seed }))
        });
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.entry(region).or_insert(found).clone()
    }

    /// Chambers whose reach covers the columns `min..=max`.
    fn around(&self, g: &Generator, min: IVec2, max: IVec2) -> Vec<Arc<Chamber>> {
        let lo = (min - IVec2::splat(REACH)).div_euclid(IVec2::splat(16 * SPACING));
        let hi = (max + IVec2::splat(REACH)).div_euclid(IVec2::splat(16 * SPACING));
        let mut out = Vec::new();
        for z in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                if let Some(c) = self.get(g, IVec2::new(x, z)) {
                    out.push(c);
                }
            }
        }
        out
    }

    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld || base.y > 0 {
            return;
        }
        let min = IVec2::new(base.x, base.z);
        let ground = |_: i32, _: i32| 0;
        for c in self.around(g, min, min + IVec2::splat(CHUNK_SIZE_I - 1)) {
            jigsaw::paint(
                jigsaw::trial_chambers(),
                &c.pieces,
                &ChamberPalette,
                &jigsaw::Ground { surface: &ground, solid: &ground },
                blocks,
                base,
            );
        }
    }

    /// Spawners, vaults and loot containers in the chunk at `cpos`.
    pub fn features(&self, g: &Generator, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let mut out = Vec::new();
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return out;
        }
        let base = cpos * CHUNK_SIZE_I;
        let inside =
            |p: IVec3| (p - base).cmpge(IVec3::ZERO).all() && (p - base).cmplt(IVec3::splat(CHUNK_SIZE_I)).all();
        let min = IVec2::new(base.x, base.z);
        let lib = jigsaw::trial_chambers();
        for c in self.around(g, min, min + IVec2::splat(CHUNK_SIZE_I - 1)) {
            for m in jigsaw::markers(lib, &c.pieces, &|_, _| 0) {
                match m {
                    Marker::Loot(p, name, seed) if inside(p) => {
                        if let Some(table) = Table::from_name(name) {
                            out.push((p, Feature::TrialLoot(seed, table)));
                        }
                    }
                    Marker::Spawner(p, name) if inside(p) => {
                        let alias = name.strip_prefix("trial:").unwrap_or(name);
                        if let Some(id) = c.aliases.config(alias, p, c.seed).and_then(super::trial::spawner::config_id)
                        {
                            out.push((p, Feature::TrialSpawner(id)));
                        }
                    }
                    _ => {}
                }
            }
            for piece in &c.pieces {
                if !jigsaw_meets(&piece.bounds, base) {
                    continue;
                }
                let t = &lib.templates[piece.template as usize];
                let vaults = t
                    .cells
                    .iter()
                    .map(|cell| (cell.pos, cell.spec))
                    .chain(t.fills.iter().filter(|(a, b, _)| a == b).map(|(a, _, spec)| (*a, *spec)));
                for (pos, spec) in vaults {
                    if let Base::Block(b) = spec.base
                        && super::trial::vault(b).is_some()
                    {
                        let p = piece.world(pos);
                        if inside(p) {
                            out.push((p, Feature::TrialVault));
                        }
                    }
                }
            }
        }
        out
    }

    /// Whether `p` lies inside a generated chamber piece.
    pub fn holds(&self, g: &Generator, p: IVec3) -> bool {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld || p.y > 64 {
            return false;
        }
        let c = IVec2::new(p.x, p.z);
        self.around(g, c, c).iter().any(|ch| ch.holds(p))
    }

    pub fn nearest(&self, g: &Generator, origin: IVec2, max_blocks: i32) -> Option<IVec3> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return None;
        }
        let centre = origin.div_euclid(IVec2::splat(16 * SPACING));
        let rings = max_blocks.div_euclid(16 * SPACING) + 1;
        let mut best = None;
        let mut distance = (max_blocks as i64).pow(2) + 1;
        for z in -rings..=rings {
            for x in -rings..=rings {
                if let Some(r) = self.get(g, centre + IVec2::new(x, z)) {
                    let d = (IVec2::new(r.at.x, r.at.z).as_i64vec2() - origin.as_i64vec2()).length_squared();
                    if d < distance {
                        distance = d;
                        best = Some(r.at);
                    }
                }
            }
        }
        best
    }
}

fn jigsaw_meets(b: &Bounds, base: IVec3) -> bool {
    let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
    b.min.cmple(top).all() && b.max.cmpge(base).all()
}

#[cfg(test)]
mod tests;
