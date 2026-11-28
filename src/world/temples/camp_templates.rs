//! v0.16 Track D (GEN-05f): generation 5 abandoned camps, with an original
//! generated catalogue and Java 26.3's placement, pools and loot.
//!
//! Java 26.3 has one structure set, `abandoned_camp` (random spread,
//! spacing 37, separation 8, salt 91231127), of eighteen equally weighted
//! jigsaw structures `abandoned_camp_<biome>`, each allowed only in its own
//! biome (`#has_structure/abandoned_camp_<biome>`): bamboo jungle, birch
//! forest, cherry grove, dappled forest, flower forest, forest, meadow, old
//! growth birch forest, old growth pine taiga, old growth spruce taiga, pale
//! garden, savanna, snowy taiga, sparse jungle, swamp, taiga, windswept
//! forest and wooded badlands. Each starts from `abandoned_camp/tent/<biome>`
//! projected to `WORLD_SURFACE_WG`, at most 2 deep and 80 blocks out, with
//! `beard_thin` terrain adaptation. A tent's `connector` joins the
//! `end_connector` of a campsite from `abandoned_camp/camp/<biome>` (the
//! biome's own four campsites and the 45 shared `camp/default` ones, every
//! element weight 1); tents and campsites hold `bottom` connectors into the
//! `abandoned_camp/trees/*` feature pools.
//!
//! The catalogue here is original (FE-02): per biome three tents and a
//! tree-shaded tent, and for every camp pool the default chest, barrel and
//! special campsites (each weighted 15, as Java's 15 of each kind) and one
//! biome campsite weighted 4. Tents are wool-stair A-frames on fence poles
//! with a straw bed and a barrel or a common chest; campsites ring a
//! campfire with a chest, a barrel or the special campsite's oxidized copper
//! chest of `chests/abandoned_camp_secret_chest`. Trees stand where the
//! templates mark a tree pool and grow an original shape of that pool's
//! species. Loot is Java's: `chests/abandoned_camp_common_chest`,
//! `chests/abandoned_camp_secret_chest` and `barrels/abandoned_camp_barrel`.
use super::*;
use crate::world::jigsaw::{self, Marker, Palette, Placed};

pub const SPACING: i32 = 37;
pub const SEPARATION: i32 = 8;
pub const SALT: u64 = 91_231_127;
/// `size` 2 and `max_distance_from_center` 80.
const SETTINGS_DEPTH: u8 = 2;
const MAX_DISTANCE: i32 = 80;

/// One camp biome: its Java name, tent fence, campsite ground, and the tree
/// pools its tents and campsites mark.
pub struct CampBiome {
    pub biome: Biome,
    fence: &'static str,
    ground: &'static str,
    tent_trees: &'static [&'static str],
    camp_trees: &'static [&'static str],
}

const fn b(
    biome: Biome,
    fence: &'static str,
    ground: &'static str,
    tent_trees: &'static [&'static str],
    camp_trees: &'static [&'static str],
) -> CampBiome {
    CampBiome { biome, fence, ground, tent_trees, camp_trees }
}

/// The eighteen camps, in the structure set's order.
pub const CAMPS: [CampBiome; 18] = [
    b(Biome::BambooJungle, "bamboo_fence", "podzol", &["bamboo"], &["bamboo", "bamboo"]),
    b(Biome::BirchForest, "birch_fence", "grass", &["birch"], &["birch", "birch"]),
    b(Biome::CherryGrove, "cherry_fence", "grass", &["cherry"], &["cherry", "cherry_bees"]),
    b(Biome::DappledForest, "poplar_fence", "grass", &["red_poplar"], &["orange_poplar", "yellow_poplar"]),
    b(Biome::FlowerForest, "oak_fence", "grass", &["oak"], &["fancy_oak_bees", "birch_bees"]),
    b(Biome::Forest, "oak_fence", "grass", &["oak"], &["birch", "oak"]),
    b(Biome::Meadow, "oak_fence", "grass", &["birch"], &["fancy_oak_bees", "birch_bees"]),
    b(Biome::OldGrowthPineTaiga, "spruce_fence", "podzol", &["pine"], &["mega_pine"]),
    b(Biome::OldGrowthBirchForest, "birch_fence", "grass", &["super_birch_bees"], &["super_birch_bees"]),
    b(Biome::OldGrowthSpruceTaiga, "spruce_fence", "podzol", &["spruce"], &["thick_spruce", "spruce"]),
    b(Biome::PaleGarden, "pale_oak_fence", "grass", &["pale_oak"], &["pale_oak"]),
    b(Biome::Savanna, "acacia_fence", "grass", &["acacia"], &["acacia", "acacia"]),
    b(Biome::SnowyTaiga, "spruce_fence", "grass", &["spruce_on_snow"], &["spruce_on_snow", "mega_pine"]),
    b(Biome::SparseJungle, "jungle_fence", "grass", &["jungle"], &["jungle"]),
    b(Biome::Swamp, "oak_fence", "grass", &[], &[]),
    b(Biome::Taiga, "spruce_fence", "grass", &["pine"], &["spruce", "spruce"]),
    b(Biome::WindsweptForest, "spruce_fence", "grass", &["spruce"], &["fancy_oak", "spruce"]),
    b(Biome::WoodedBadlands, "oak_fence", "red_sand", &[], &[]),
];

/// The 21 `abandoned_camp/trees/*` pools.
pub const TREE_POOLS: [&str; 21] = [
    "acacia",
    "bamboo",
    "birch",
    "birch_bees",
    "cherry",
    "cherry_bees",
    "fancy_oak",
    "fancy_oak_bees",
    "jungle",
    "mega_jungle",
    "mega_pine",
    "oak",
    "orange_poplar",
    "pale_oak",
    "pine",
    "red_poplar",
    "spruce",
    "spruce_on_snow",
    "super_birch_bees",
    "thick_spruce",
    "yellow_poplar",
];

/// The camp a biome holds (`#has_structure/abandoned_camp_<biome>`).
pub fn camp_of(biome: Biome) -> Option<&'static CampBiome> {
    CAMPS.iter().find(|c| c.biome == biome)
}

/// The structure id of a camp biome's camp.
pub fn structure_name(c: &CampBiome) -> String {
    format!("abandoned_camp_{}", c.biome.name())
}

const COMMON: &str = "abandoned_camp_common_chest";
const SECRET: &str = "abandoned_camp_secret_chest";
const BARREL: &str = "barrels/abandoned_camp_barrel";

/// An A-frame tent of wool stairs along x from `x0` to `x1`, ridge at z 3,
/// with fence poles at both ends.
fn tent_frame(out: &mut String, x0: i32, x1: i32, fence: &str) {
    use std::fmt::Write;
    let _ = writeln!(out, "fill {x0} 1 1 {x1} 1 1 white_wool_stairs^south");
    let _ = writeln!(out, "fill {x0} 1 5 {x1} 1 5 white_wool_stairs^north");
    let _ = writeln!(out, "fill {x0} 2 2 {x1} 2 2 white_wool_stairs^south");
    let _ = writeln!(out, "fill {x0} 2 4 {x1} 2 4 white_wool_stairs^north");
    let _ = writeln!(out, "fill {x0} 3 3 {x1} 3 3 white_wool");
    let (p0, p1) = (x0 - 1, x1 + 1);
    let _ = writeln!(out, "fill {p0} 1 3 {p0} 3 3 {fence}");
    let _ = writeln!(out, "fill {p1} 1 3 {p1} 3 3 {fence}");
}

/// The text of the whole catalogue (`jigsaw::abandoned_camps`).
pub fn catalogue() -> String {
    use std::fmt::Write;
    let mut out = String::from("# Generated original abandoned camp catalogue (v0.16 Track D).\n");
    for c in &CAMPS {
        let name = c.biome.name();
        let camp_pool = format!("abandoned_camp/camp/{name}");
        let mut tents = Vec::new();
        // Tents 1-3: a barrel tent, a chest tent and a small chest tent.
        for (i, (width, store)) in [
            (8, "loot:barrel:barrels/abandoned_camp_barrel@east"),
            (8, "chest:abandoned_camp_common_chest@east"),
            (6, "chest:abandoned_camp_common_chest@east"),
        ]
        .into_iter()
        .enumerate()
        {
            let t = format!("abandoned_camp/tent/{name}/tent_{name}_{}", i + 1);
            let x1 = width - 3;
            let _ = writeln!(out, "template {t}\nsize {width} 8 8\nprojection rigid");
            let _ = writeln!(out, "fill 0 1 0 {} 7 7 air", width - 1);
            let _ = writeln!(out, "fill 1 0 1 {} 0 6 dirt", width - 2);
            let _ = writeln!(out, "fill 0 0 3 2 0 3 dirt_path");
            tent_frame(&mut out, 2, x1, c.fence);
            let _ = writeln!(out, "cell 2 1 3 $bed_foot_east");
            let _ = writeln!(out, "cell 3 1 3 $bed_head_east");
            let _ = writeln!(out, "cell {x1} 1 2 {store}");
            let _ = writeln!(out, "jigsaw 0 0 3 west connector end_connector {camp_pool}");
            let _ = writeln!(out, "end\n");
            tents.push(t);
        }
        // Tent 4: the barrel tent beside a tree from the biome's pool.
        if let Some(tree) = c.tent_trees.first() {
            let t = format!("abandoned_camp/tent/{name}/tent_{name}_4");
            let _ = writeln!(out, "template {t}\nsize 8 8 8\nprojection rigid");
            let _ = writeln!(out, "fill 0 1 0 7 7 7 air");
            let _ = writeln!(out, "fill 1 0 1 6 0 6 dirt");
            let _ = writeln!(out, "fill 0 0 3 2 0 3 dirt_path");
            tent_frame(&mut out, 2, 4, c.fence);
            let _ = writeln!(out, "cell 2 1 3 $bed_foot_east");
            let _ = writeln!(out, "cell 3 1 3 $bed_head_east");
            let _ = writeln!(out, "cell 4 1 4 loot:barrel:{BARREL}@east");
            let _ = writeln!(out, "mob 6 1 6 abandoned_camp/trees/{tree}");
            let _ = writeln!(out, "jigsaw 0 0 3 west connector end_connector {camp_pool}");
            let _ = writeln!(out, "end\n");
            tents.push(t);
        }
        let _ = writeln!(out, "pool abandoned_camp/tent/{name} fallback empty");
        for t in &tents {
            let _ = writeln!(out, "1 {t}");
        }
        let _ = writeln!(out, "end\n");
        // The biome's own campsite: a log seat and trees round a fire.
        let t = format!("abandoned_camp/camp/{name}/campsite_{name}_1");
        let _ = writeln!(out, "template {t}\nsize 10 10 10\nprojection rigid");
        let _ = writeln!(out, "fill 0 1 0 9 9 9 air");
        let _ = writeln!(out, "fill 1 0 1 8 0 8 {}", c.ground);
        let _ = writeln!(out, "fill 4 0 4 6 0 6 stone");
        let _ = writeln!(out, "fill 6 0 4 9 0 4 dirt_path");
        let _ = writeln!(out, "cell 5 1 5 campfire");
        let _ = writeln!(out, "fill 3 1 7 5 1 7 {}", log_of(c.fence));
        let _ = writeln!(out, "cell 7 1 7 loot:barrel:{BARREL}@west");
        for (i, tree) in c.camp_trees.iter().enumerate() {
            let at = if i == 0 { (1, 1, 1) } else { (8, 1, 1) };
            let _ = writeln!(out, "mob {} {} {} abandoned_camp/trees/{tree}", at.0, at.1, at.2);
        }
        let _ = writeln!(out, "jigsaw 9 0 4 east end_connector connector empty");
        let _ = writeln!(out, "end\n");
        let _ = writeln!(out, "pool {camp_pool} fallback empty");
        let _ = writeln!(out, "15 abandoned_camp/camp/default/campsite_default_chest");
        let _ = writeln!(out, "15 abandoned_camp/camp/default/campsite_default_barrel");
        let _ = writeln!(out, "15 abandoned_camp/camp/default/campsite_default_special");
        let _ = writeln!(out, "4 {t}");
        let _ = writeln!(out, "end\n");
    }
    // The shared default campsites: a fire ring with a chest, a barrel, or
    // (special) the secret copper chest and a lantern.
    for (kind, store) in [
        ("chest", format!("cell 6 1 7 chest:{COMMON}@east")),
        ("barrel", format!("cell 4 1 6 loot:barrel:{BARREL}@east")),
        (
            "special",
            format!(
                "cell 3 1 3 loot:oxidized_copper_chest:{SECRET}@east\ncell 3 1 4 chest:{COMMON}@east\ncell 4 1 3 lantern"
            ),
        ),
    ] {
        let _ = writeln!(
            out,
            "template abandoned_camp/camp/default/campsite_default_{kind}\nsize 10 3 10\nprojection rigid"
        );
        let _ = writeln!(out, "fill 0 1 0 9 2 9 air");
        let _ = writeln!(out, "fill 3 0 3 7 0 7 dirt_path");
        let _ = writeln!(out, "fill 4 0 4 6 0 6 stone");
        let _ = writeln!(out, "fill 7 0 4 9 0 4 dirt_path");
        let _ = writeln!(out, "cell 5 1 5 campfire");
        let _ = writeln!(out, "{store}");
        let _ = writeln!(out, "jigsaw 9 0 4 east end_connector connector empty");
        let _ = writeln!(out, "end\n");
    }
    // The tree pools: one marker each (the tree grows where it's marked).
    for tree in TREE_POOLS {
        let _ = writeln!(out, "template abandoned_camp/trees/{tree}\nsize 1 1 1\nprojection rigid\nend\n");
        let _ = writeln!(out, "pool abandoned_camp/trees/{tree} fallback empty\n1 abandoned_camp/trees/{tree}\nend\n");
    }
    out.push_str("pool empty\n1 empty\nend\n");
    out
}

/// The log of a fence's wood (bamboo's block for bamboo).
fn log_of(fence: &str) -> String {
    match fence.trim_end_matches("_fence") {
        "bamboo" => "block_of_bamboo".into(),
        "poplar" => "poplar_log".into(),
        wood => format!("{wood}_log"),
    }
}

struct CampPalette;

impl Palette for CampPalette {
    fn clear_above(&self) -> i32 {
        0
    }
    fn role(&self, role: &str) -> Option<Block> {
        use crate::world::colour_blocks::straw_bed;
        Some(match role {
            "foundation" => Block::DIRT,
            "bed_foot_east" => straw_bed::half(Facing::East, false),
            "bed_head_east" => straw_bed::half(Facing::East, true),
            _ => return None,
        })
    }
    fn process(&self, block: Block, _at: IVec3, piece: &Placed) -> Option<Block> {
        // Straw beds keep their state under `with_facing`: turn them with the piece.
        use crate::world::colour_blocks::straw_bed;
        Some(match straw_bed::decode(block.0) {
            Some((facing, head)) => straw_bed::half(jigsaw::rotate_facing(facing, piece.rotation), head),
            None => block,
        })
    }
}

/// The surface camps stand on (`WORLD_SURFACE_WG`, water included).
fn surface(g: &Generator, x: i32, z: i32) -> i32 {
    g.column(x, z).height.max(SEA_LEVEL - 1)
}

/// The camp biome sampled for a start chunk: the centre of an unturned
/// 8-wide tent at the chunk's corner.
pub fn start_biome(g: &Generator, start: IVec2) -> Option<&'static CampBiome> {
    let at = start * 16 + IVec2::splat(4);
    camp_of(g.column(at.x, at.y).biome)
}

/// Lays out the camp starting in chunk `start`, or `None` where its biome
/// takes no camp or the assembled start lands outside that biome.
pub(super) fn build(g: &Generator, start: IVec2, seed: u64) -> Option<Built> {
    let camp = start_biome(g, start)?;
    let lib = jigsaw::abandoned_camps();
    let surface = |x, z| surface(g, x, z);
    let pool = format!("abandoned_camp/tent/{}", camp.biome.name());
    let settings = jigsaw::Settings { start_pool: &pool, max_depth: SETTINGS_DEPTH, max_distance: MAX_DISTANCE };
    let at = start * 16;
    let placed = if g.generation >= crate::world::terrain::Generation::V7 {
        jigsaw::assemble_adapted(lib, &settings, at, &surface, seed)
    } else {
        jigsaw::assemble_enveloped(lib, &settings, at, &surface, seed, &|ps, p| g.beard_fit(lib, ps, p))
    };
    // `Structure.isValidBiome`: the biome at the start piece's centre.
    let first = placed.first()?;
    let centre = (first.bounds.min + first.bounds.max) / 2;
    if camp_of(g.column(centre.x, centre.z).biome).map(|c| c.biome) != Some(camp.biome) {
        return None;
    }
    let mut bounds = placed.iter().map(|p| p.bounds).reduce(|a, b| a.union(&b))?;
    let mut features = Vec::new();
    let mut trees = Vec::new();
    // Markers borrow the pieces; their names live in the static library.
    let statics = |name: &str, list: &'static [String]| list.iter().find(|l| *l == name).map(String::as_str);
    let markers: Vec<_> = jigsaw::markers(lib, &placed, &surface)
        .into_iter()
        .map(|m| match m {
            Marker::Loot(at, table, seed) => (at, statics(table, &lib.loot).map(|t| (t, seed)), None),
            Marker::Mob(at, name) => (at, None, Some(name.to_owned())),
            Marker::Spawner(..) => unreachable!("camps have no spawners"),
        })
        .collect();
    for (at, loot, tree) in markers {
        if let Some((table, seed)) = loot {
            features.push((at, Feature::StructureLoot(seed, table)));
        }
        if let Some(name) = tree {
            let tree = name.strip_prefix("abandoned_camp/trees/").expect("camp markers are trees");
            trees.extend(tree_blocks(tree, at, hash3(at.x, at.y, at.z, seed)));
        }
    }
    for &(p, _) in &trees {
        bounds = bounds.union(&Bounds { min: p, max: p });
    }
    trees.sort_by_key(|(p, _)| p.y);
    Some(Built {
        kind: Kind::AbandonedCamp,
        bounds,
        blocks: trees,
        features,
        spawns: bounds,
        residents: Vec::new(),
        placed,
        mansion: None,
    })
}

pub(super) fn paint(g: &Generator, built: &Built, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
    jigsaw::paint(
        jigsaw::abandoned_camps(),
        &built.placed,
        if g.generation >= crate::world::terrain::Generation::V7 {
            &jigsaw::NoFoundation(&CampPalette)
        } else {
            &CampPalette
        },
        &jigsaw::Ground {
            surface: &|x, z| g.terrain_column(x, z).height.max(SEA_LEVEL - 1),
            solid: &|x, z| g.terrain_column(x, z).height,
        },
        blocks,
        base,
    );
    // Trees fill only air, leaves and plants, as a tree feature would.
    let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
    for &(p, block) in &built.blocks {
        if p.cmpge(base).all() && p.cmple(top).all() {
            let l = p - base;
            let i = index(l.x as usize, l.y as usize, l.z as usize);
            if blocks[i] == Block::AIR || blocks[i].is_leaves() || blocks[i].is_replaceable() {
                blocks[i] = block;
            }
        }
    }
}

/// A named block, cached (`Block::from_name` scans every state).
fn named(name: &str) -> Block {
    static CACHE: std::sync::OnceLock<Mutex<FxHashMap<String, Block>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(&b) = cache.lock().unwrap().get(name) {
        return b;
    }
    let b = Block::from_name(name).unwrap_or_else(|| panic!("camp tree block {name}"));
    cache.lock().unwrap().insert(name.to_owned(), b);
    b
}

/// The (log, leaves) of a tree pool's species.
fn wood(tree: &str) -> (Block, Block) {
    use crate::world::wood::poplar;
    match tree {
        "red_poplar" => (poplar::LOG, poplar::RED_LEAVES),
        "orange_poplar" => (poplar::LOG, poplar::ORANGE_LEAVES),
        "yellow_poplar" => (poplar::LOG, poplar::YELLOW_LEAVES),
        "birch" | "birch_bees" | "super_birch_bees" => (named("birch_log"), named("birch_leaves")),
        "spruce" | "pine" | "spruce_on_snow" | "mega_pine" | "thick_spruce" => {
            (named("spruce_log"), named("spruce_leaves"))
        }
        "jungle" | "mega_jungle" => (named("jungle_log"), named("jungle_leaves")),
        "acacia" => (named("acacia_log"), named("acacia_leaves")),
        "cherry" | "cherry_bees" => (named("cherry_log"), named("cherry_leaves")),
        "pale_oak" => (named("pale_oak_log"), named("pale_oak_leaves")),
        _ => (named("oak_log"), named("oak_leaves")),
    }
}

/// An original tree of `tree`'s species rooted at `at` (the cell above the
/// ground), its size varied by `seed`.
pub fn tree_blocks(tree: &str, at: IVec3, seed: u64) -> Vec<(IVec3, Block)> {
    let mut out = Vec::new();
    let mut rng = Rng(seed);
    if tree == "bamboo" {
        let stalk = named("bamboo");
        for y in 0..6 + rng.below(5) as i32 {
            out.push((at + IVec3::Y * y, stalk));
        }
        return out;
    }
    let (log, leaves) = wood(tree);
    let thick = matches!(tree, "mega_pine" | "thick_spruce" | "mega_jungle" | "pale_oak");
    let conical = matches!(tree, "spruce" | "pine" | "spruce_on_snow" | "mega_pine" | "thick_spruce");
    let height = match tree {
        "mega_pine" | "thick_spruce" | "mega_jungle" => 12 + rng.below(4) as i32,
        "super_birch_bees" => 9 + rng.below(3) as i32,
        "fancy_oak" | "fancy_oak_bees" | "jungle" => 7 + rng.below(3) as i32,
        "red_poplar" | "orange_poplar" | "yellow_poplar" => 8 + rng.below(3) as i32,
        "spruce" | "pine" | "spruce_on_snow" => 7 + rng.below(3) as i32,
        "acacia" | "pale_oak" => 5 + rng.below(2) as i32,
        _ => 5 + rng.below(2) as i32,
    };
    let trunk: &[IVec3] = if thick { &[IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::new(1, 0, 1)] } else { &[IVec3::ZERO] };
    let leaf = |p: IVec3, out: &mut Vec<(IVec3, Block)>| {
        if !out.iter().any(|(q, _)| *q == p) {
            out.push((p, leaves));
        }
    };
    if conical {
        // Rings narrowing toward the top; pines keep the lower trunk bare.
        let bare = if matches!(tree, "pine" | "mega_pine") { height / 2 } else { 2 };
        for y in bare..=height {
            let r =
                ((height - y) / 2 + 1).min(if thick { 4 } else { 3 }) - i32::from((y - bare) % 2 == 1 && y < height);
            for dx in -r..=r + i32::from(thick) {
                for dz in -r..=r + i32::from(thick) {
                    if dx.abs() + dz.abs() <= r + 1 {
                        leaf(at + IVec3::new(dx, y, dz), &mut out);
                    }
                }
            }
        }
    } else if tree == "acacia" {
        // A bent trunk under a flat crown.
        let lean = if rng.below(2) == 0 { IVec3::X } else { IVec3::Z };
        let top = at + lean * 2 + IVec3::Y * height;
        for dx in -2i32..=2 {
            for dz in -2i32..=2 {
                if dx.abs() + dz.abs() <= 3 {
                    leaf(top + IVec3::new(dx, 0, dz), &mut out);
                }
                if dx.abs() + dz.abs() <= 1 {
                    leaf(top + IVec3::new(dx, 1, dz), &mut out);
                }
            }
        }
        out.retain(|(p, _)| *p != top);
        for y in 0..height {
            let off = if y >= height - 2 { lean * (y - height + 3) } else { IVec3::ZERO };
            out.push((at + IVec3::Y * y + off, log));
        }
        out.push((top, log));
        return out;
    } else {
        // A round crown; poplars are tall and narrow, cherries wide and flat.
        let narrow = tree.ends_with("poplar");
        let (r, depth) = match tree {
            t if t.ends_with("poplar") => (1, height - 3),
            "cherry" | "cherry_bees" | "pale_oak" => (3, 3),
            "fancy_oak" | "fancy_oak_bees" | "jungle" | "mega_jungle" => (3, 4),
            _ => (2, 3),
        };
        for y in height - depth..=height + 1 {
            let rr = if y > height { r - 1 } else { r };
            for dx in -rr..=rr + i32::from(thick) {
                for dz in -rr..=rr + i32::from(thick) {
                    let corner = dx.abs() == rr && dz.abs() == rr;
                    if !corner || (narrow && y < height) {
                        leaf(at + IVec3::new(dx, y, dz), &mut out);
                    }
                }
            }
        }
    }
    // The trunk replaces the leaves it passes through.
    for y in 0..height {
        for t in trunk {
            let p = at + IVec3::Y * y + *t;
            out.retain(|(q, _)| *q != p);
            out.push((p, log));
        }
    }
    out
}

#[cfg(test)]
mod tests;
