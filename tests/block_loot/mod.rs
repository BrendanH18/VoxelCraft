//! Shared by `tests/java_block_loot.rs` and `examples/block_loot_audit.rs`:
//! compares every registered state's drops with the Java 26.3 block loot
//! fixture. The fixture is independent Java data; this module only maps
//! VoxelCraft states to Java blocks and state values, and samples drops,
//! mined and caught in an explosion.
#![allow(dead_code)]

use glam::IVec3;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use voxelcraft::{
    enchant::{Enchantment, Enchants},
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    mining,
    world::{
        World,
        block::{Block, STATE_CAPACITY},
        colour_blocks::{candle, straw_bed},
        composter, overworld_blocks as ob,
        slab_stair_states::{self, SlabType},
        terrain::Generator,
        utilities,
    },
};

pub const FIXTURE: &str = include_str!("../fixtures/java/26.3/block_loot.json");
const REGISTRY: &str = include_str!("../fixtures/java/26.3/registry_names.json");

/// Runs per random case; fixed drops need only a few.
const RANDOM_RUNS: usize = 4000;
const FIXED_RUNS: usize = 4;
/// Further states of an already sampled case check only the range.
const REPEAT_RUNS: usize = 8;
/// The fixture's `explosion` scenario: a TNT block or creeper with decay.
const EXPLOSION_RADIUS: f32 = 4.0;

/// Intentional differences, each a decision or a mechanic a later release
/// brings. An entry that matches again fails the test so the list stays honest.
pub const ALLOWED: &[(&str, &str)] = &[];

/// VoxelCraft states with no Java block.
pub const VOXELCRAFT_ONLY: &[(&str, &str)] = &[
    ("calcite stairs", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("calcite slab", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("calcite wall", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("deepslate stairs", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("deepslate slab", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("deepslate wall", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("smooth stone stairs", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("smooth stone wall", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("polished granite wall", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("polished diorite wall", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
    ("polished andesite wall", "retired invented block: placed ones drop themselves (owner, 2026-10-25)"),
];

/// VoxelCraft names that differ from the Java block they stand for.
const BLOCK_NAMES: &[(&str, &str)] = &[
    ("grass", "grass_block"),
    ("snowy grass", "grass_block"),
    ("snow", "snow_block"),
    ("snow layer", "snow"),
    ("tall grass", "short_grass"),
    ("tall grass plant", "tall_grass"),
    ("tall grass plant top", "tall_grass"),
    ("tall dry grass top", "tall_dry_grass"),
    ("flowing water", "water"),
    ("falling water", "water"),
    ("flowing lava", "lava"),
    ("falling lava", "lava"),
    ("wool", "white_wool"),
    ("lit furnace", "furnace"),
    ("wet farmland", "farmland"),
    ("bed foot", "red_bed"),
    ("bed head", "red_bed"),
    ("wheat crops", "wheat"),
    ("carrot crops", "carrots"),
    ("potato crops", "potatoes"),
    ("chain", "iron_chain"),
    ("legacy bone block", "bone_block"),
    ("small dripleaf top", "small_dripleaf"),
    ("tall seagrass top", "tall_seagrass"),
    ("pale hanging moss plant", "pale_hanging_moss"),
    ("hanging mangrove propagule", "mangrove_propagule"),
    ("sunflower top", "sunflower"),
    ("lilac top", "lilac"),
    ("rose bush top", "rose_bush"),
    ("peony top", "peony"),
    ("large fern top", "large_fern"),
];

pub struct Audit {
    /// Java blocks with at least one VoxelCraft state.
    pub compared: usize,
    pub matched: usize,
    pub unregistered: Vec<String>,
    pub allowed: Vec<(String, &'static str)>,
    pub failures: Vec<String>,
    /// Failing Java blocks, for the audit example.
    pub failing: BTreeSet<String>,
    pub voxelcraft_only: Vec<String>,
    pub states: usize,
    /// State and scenario pairs broken and compared.
    pub checks: usize,
    /// Pairs skipped because no tool of the scenario harvests the state.
    pub skipped: usize,
}

struct Reference {
    blocks: serde_json::Map<String, Value>,
    english: HashMap<String, String>,
    item_english: HashMap<String, String>,
    breaks_pots: BTreeSet<String>,
}

impl Reference {
    fn load() -> Self {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let registry: Value = serde_json::from_str(REGISTRY).unwrap();
        let english = |kind: &str| {
            registry[kind]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, name)| (name.as_str().unwrap().to_lowercase(), id.clone()))
                .collect()
        };
        let mut blocks = fixture["blocks"].as_object().unwrap().clone();
        // Wall variants use their standing block's table.
        let redirects: Vec<(String, String)> =
            blocks.iter().filter_map(|(id, b)| Some((id.clone(), b.get("loot_table")?.as_str()?.to_owned()))).collect();
        for (id, table) in redirects {
            let base = blocks[&table].clone();
            blocks.insert(id, base);
        }
        let breaks_pots =
            fixture["tags"]["breaks_decorated_pots"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned());
        Reference {
            blocks,
            english: english("blocks"),
            item_english: english("items"),
            breaks_pots: breaks_pots.collect(),
        }
    }

    /// The Java block a state stands for.
    fn java_block(&self, name: &str) -> Option<String> {
        if let Some((_, id)) = BLOCK_NAMES.iter().find(|(n, _)| *n == name) {
            return Some(format!("minecraft:{id}"));
        }
        for half in [" bed foot", " bed head"] {
            if let Some(colour) = name.strip_suffix(half) {
                return Some(format!("minecraft:{}_bed", colour.replace(' ', "_")));
            }
        }
        let id = format!("minecraft:{}", name.replace(' ', "_"));
        if self.blocks.contains_key(&id) {
            return Some(id);
        }
        self.english.get(name).map(|id| format!("minecraft:{id}"))
    }

    /// The VoxelCraft item for a Java item id.
    fn item(&self, id: &str) -> Option<Item> {
        let block_item = |name| Block::from_name(name).map(Item::from_block);
        let item = match id.strip_prefix("minecraft:").unwrap() {
            "snow" => Some(Item::from_block(ob::SNOW_LAYER)),
            "snow_block" => Some(Item::from_block(Block::SNOW)),
            "short_grass" => Some(Item::from_block(Block::TALL_GRASS)),
            "grass_block" => Some(Item::from_block(Block::GRASS)),
            "tall_grass" => block_item("tall grass plant"),
            "iron_chain" => block_item("chain"),
            "red_bed" => Some(Item::BED),
            name => Item::from_name(name).or_else(|| {
                let english = self.item_english.iter().find(|(_, v)| *v == name).map(|(k, _)| k.as_str())?;
                Item::from_name(english)
            }),
        };
        item.filter(|i| i.is_valid())
    }
}

/// A Java state value of `b` for the property a loot condition names.
fn property(b: Block, java: &str, prop: &str) -> String {
    // A waterlogged partner has its dry original's properties.
    let b = b.canonical();
    let missing = || -> u8 { panic!("{java} #{}: no {prop}", b.0) };
    let flag = |v: bool| if v { "true" } else { "false" }.to_owned();
    match prop {
        "type" => match slab_stair_states::slab_type(b) {
            Some(SlabType::Double) => "double",
            Some(SlabType::Top) => "top",
            _ => "bottom",
        }
        .to_owned(),
        "candles" => candle::decode(b.0).map_or(1, |c| c.candles).to_string(),
        "part" => {
            let head = match straw_bed::decode(b.0) {
                Some((_, head)) => head,
                None => b.name().ends_with("head"),
            };
            if head { "head" } else { "foot" }.to_owned()
        }
        "half" => {
            let upper = if b.is_door() {
                b.is_door_upper()
            } else {
                ob::double_of(b).map_or(b.name().ends_with(" top"), |(_, upper)| upper)
            };
            if upper { "upper" } else { "lower" }.to_owned()
        }
        "age" => b
            .as_crop()
            .map(|(_, age)| age)
            .or(b.wart_age())
            .or(ob::cocoa_of(b).map(|(age, _)| age))
            .or(ob::berry_age(b))
            .or(ob::propagule_age(b))
            // A standing propagule is placed at age 4 (`MangrovePropaguleBlock.getStateForPlacement`).
            .or((java == "minecraft:mangrove_propagule").then_some(4))
            .unwrap_or_else(missing)
            .to_string(),
        // Cave vines with berries glow.
        "berries" => flag(b.emission() > 0),
        // VoxelCraft's pale moss carpet is the bottom carpet only.
        "bottom" => flag(true),
        "cracked" => flag(utilities::decorated_pot_of(b).is_some_and(|(_, cracked, _)| cracked)),
        // Pink petals are one petal per state; wildflowers and leaf litter
        // keep their count in the state.
        "flower_amount" => voxelcraft::world::biome_plants::segments(b).map_or(1, |(_, n)| n).to_string(),
        "segment_amount" => voxelcraft::world::biome_plants::segments(b).map_or_else(missing, |(_, n)| n).to_string(),
        "layers" => ob::snow_layers(b).unwrap_or_else(missing).to_string(),
        "level" => composter::level(b).unwrap_or_else(missing).to_string(),
        "pickles" => ob::pickle_count(b).unwrap_or_else(missing).to_string(),
        // VoxelCraft TNT has no unstable state.
        "unstable" => flag(false),
        // Glow lichen and sculk vein faces (`MultifaceBlock`).
        "down" | "up" | "north" | "south" | "west" | "east" => {
            let i = ["down", "up", "north", "south", "west", "east"].iter().position(|&d| d == prop).unwrap();
            flag(
                voxelcraft::world::cave_blocks::multiface_of(b).unwrap_or_else(|| panic!("{java}: no faces")).1
                    & (1 << i)
                    != 0,
            )
        }
        _ => panic!("{java}: no mapping for state property {prop}"),
    }
}

/// The table case of `b` broken with `held`.
fn case_key(reference: &Reference, table: &Value, b: Block, java: &str, held: Option<Stack>) -> String {
    let mut parts = Vec::new();
    for (prop, values) in table["properties"].as_object().unwrap() {
        let mut value = property(b, java, prop);
        // `DecoratedPotBlock.playerWillDestroy` cracks the pot first.
        if prop == "cracked"
            && let Some(s) = held
            && reference.breaks_pots.contains(&format!("minecraft:{}", java_tool_name(s.item)))
            && !s.active_enchants().has(Enchantment::SilkTouch)
        {
            value = "true".to_owned();
        }
        let named = values.as_array().unwrap().iter().any(|v| v.as_str() == Some(value.as_str()));
        parts.push(format!("{prop}={}", if named { value.as_str() } else { "*" }));
    }
    parts.join(",")
}

fn java_tool_name(item: Item) -> String {
    match item.as_tool() {
        Some((kind, tier)) => {
            let tier = match tier {
                Tier::Wood => "wooden",
                Tier::Stone => "stone",
                Tier::Copper => "copper",
                Tier::Iron => "iron",
                Tier::Gold => "golden",
                Tier::Diamond => "diamond",
                Tier::Netherite => "netherite",
            };
            let kind = match kind {
                ToolKind::Pickaxe => "pickaxe",
                ToolKind::Shovel => "shovel",
                ToolKind::Axe => "axe",
                ToolKind::Hoe => "hoe",
                ToolKind::Sword => "sword",
            };
            format!("{tier}_{kind}")
        }
        None => item.name().replace(' ', "_"),
    }
}

/// The scenario's held item, or `None` when no such tool harvests `b`
/// (`requiresCorrectToolForDrops` is outside the loot table).
fn held(b: Block, scenario: &str) -> Option<Option<Stack>> {
    let harvests = |s: Option<Stack>| mining::can_harvest(b, s.map(|s| s.item)).then_some(s);
    match scenario {
        "hand" => harvests(None),
        // A blast rolls the table with an empty tool whatever the block needs.
        "explosion" => Some(None),
        "shears" => harvests(Some(Stack::new(Item::SHEARS, 1))),
        _ => {
            let mut enchants = Enchants::NONE;
            match scenario {
                "pickaxe" => {}
                "silk_touch" => enchants.set(Enchantment::SilkTouch, 1),
                f => enchants.set(Enchantment::Fortune, f.strip_prefix("fortune_").unwrap().parse().unwrap()),
            }
            [ToolKind::Pickaxe, ToolKind::Shovel, ToolKind::Axe, ToolKind::Hoe, ToolKind::Sword]
                .into_iter()
                .find_map(|kind| harvests(Some(Stack { enchants, ..Stack::new(Item::tool(kind, Tier::Netherite), 1) })))
        }
    }
}

struct Row {
    item: Option<Item>,
    id: String,
    min: u32,
    max: u32,
    chance: f64,
    mean: f64,
    variance: f64,
}

fn rows(reference: &Reference, value: &Value) -> Vec<Row> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let id = r[0].as_str().unwrap().to_owned();
            Row {
                item: reference.item(&id),
                id,
                min: r[1].as_u64().unwrap() as u32,
                max: r[2].as_u64().unwrap() as u32,
                chance: r[3].as_f64().unwrap(),
                mean: r[4].as_f64().unwrap(),
                variance: r[5].as_f64().unwrap(),
            }
        })
        .collect()
}

/// Item totals of `runs` breaks.
fn sample(world: &mut World, b: Block, held: Option<Stack>, runs: usize) -> Vec<BTreeMap<u16, u32>> {
    (0..runs)
        .map(|_| {
            world.drops.clear();
            world.spill_with_item(IVec3::ZERO, b, held);
            let mut totals = BTreeMap::new();
            for (_, s) in &world.drops {
                *totals.entry(s.item.0).or_insert(0) += u32::from(s.count);
            }
            totals
        })
        .collect()
}

/// Item totals of `runs` blasts of radius [`EXPLOSION_RADIUS`] with decay
/// (`ServerExplosion.interactWithBlocks`), before drops are merged.
fn sample_exploded(world: &mut World, b: Block, runs: usize) -> Vec<BTreeMap<u16, u32>> {
    (0..runs)
        .map(|_| {
            world.drops.clear();
            world.spill_exploded(IVec3::ZERO, b, Some(EXPLOSION_RADIUS));
            let mut totals = BTreeMap::new();
            for (_, s) in &world.drops {
                *totals.entry(s.item.0).or_insert(0) += u32::from(s.count);
            }
            totals
        })
        .collect()
}

fn describe(item: Item) -> String {
    if item.is_valid() { item.name().to_owned() } else { format!("item #{}", item.0) }
}

/// Differences between sampled drops and the reference rows.
fn compare(expected: &[Row], runs: &[BTreeMap<u16, u32>], statistics: bool) -> Option<String> {
    if let Some(row) = expected.iter().find(|r| r.item.is_none()) {
        return Some(format!("drops {}, which VoxelCraft doesn't register", row.id));
    }
    let n = runs.len() as f64;
    for run in runs {
        if let Some(item) = run.keys().find(|i| !expected.iter().any(|r| r.item == Some(Item(**i)))) {
            let wanted: Vec<_> = expected.iter().map(|r| format!("{} {}..={}", r.id, r.min, r.max)).collect();
            return Some(format!("drops {} x{} (Java: {wanted:?})", describe(Item(*item)), run[item]));
        }
    }
    for row in expected {
        let counts: Vec<u32> = runs.iter().map(|r| r.get(&row.item.unwrap().0).copied().unwrap_or(0)).collect();
        if let Some(c) = counts.iter().find(|&&c| c > 0 && (c < row.min || c > row.max)) {
            return Some(format!("{} x{c}, Java {}..={}", row.id, row.min, row.max));
        }
        if row.chance >= 1.0 && counts.contains(&0) {
            return Some(format!("{} sometimes missing, Java always drops it", row.id));
        }
        if !statistics || row.chance >= 1.0 && row.variance == 0.0 {
            continue;
        }
        let present = counts.iter().filter(|&&c| c > 0).count() as f64 / n;
        let spread = 5.0 * (row.chance * (1.0 - row.chance) / n).sqrt() + 1e-9;
        if (present - row.chance).abs() > spread {
            return Some(format!("{} dropped in {present:.4} of breaks, Java {:.4}", row.id, row.chance));
        }
        let mean = counts.iter().map(|&c| f64::from(c)).sum::<f64>() / n;
        let spread = 5.0 * (row.variance / n).sqrt() + 1e-9;
        if (mean - row.mean).abs() > spread {
            return Some(format!("{} mean {mean:.4}, Java {:.4}", row.id, row.mean));
        }
    }
    None
}

pub fn audit() -> Audit {
    let reference = Reference::load();
    let allowed: HashMap<&str, &str> = ALLOWED.iter().copied().collect();
    let only: HashMap<&str, &str> = VOXELCRAFT_ONLY.iter().copied().collect();
    let mut groups: BTreeMap<String, Vec<Block>> = BTreeMap::new();
    let mut voxelcraft_only = BTreeSet::new();
    let mut invented = Vec::new();
    let mut states = 0;
    for b in (0..STATE_CAPACITY as u16).map(Block) {
        let name = b.name();
        if name.is_empty() || name == "unknown" {
            continue;
        }
        states += 1;
        match reference.java_block(name) {
            Some(id) if reference.blocks.contains_key(&id) => groups.entry(id).or_default().push(b),
            _ if only.contains_key(name) => invented.push(b),
            _ => {
                voxelcraft_only.insert(name.to_owned());
            }
        }
    }
    let mut world = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
    let mut audit = Audit {
        compared: 0,
        matched: 0,
        unregistered: Vec::new(),
        allowed: Vec::new(),
        failures: Vec::new(),
        failing: BTreeSet::new(),
        voxelcraft_only: voxelcraft_only.into_iter().collect(),
        states,
        checks: 0,
        skipped: 0,
    };
    // The retired invented blocks keep dropping themselves.
    for b in invented {
        let held = held(b, "pickaxe").expect("a tool harvests it");
        let own = Item::from_block(b.base());
        if let Some(run) =
            sample(&mut world, b, held, FIXED_RUNS).iter().find(|r| r.keys().any(|&i| i != own.0) || r.is_empty())
        {
            audit.failures.push(format!("{} #{}: drops {run:?}, not itself", b.name(), b.0));
        }
    }
    for (java, table) in &reference.blocks {
        let Some(blocks) = groups.get(java) else {
            audit.unregistered.push(java.clone());
            continue;
        };
        audit.compared += 1;
        let mut sampled = BTreeSet::new();
        let mut problems: Vec<String> = Vec::new();
        let mut failed_scenarios = BTreeSet::new();
        for &b in blocks {
            for scenario in
                ["hand", "pickaxe", "shears", "silk_touch", "fortune_1", "fortune_2", "fortune_3", "explosion"]
            {
                let Some(held) = held(b, scenario) else {
                    audit.skipped += 1;
                    continue;
                };
                if failed_scenarios.contains(scenario) {
                    continue;
                }
                audit.checks += 1;
                let key = case_key(&reference, table, b, java, held);
                let case = &table["cases"][&key];
                let expected = rows(&reference, case.get(scenario).unwrap_or(&case["pickaxe"]));
                let random = expected.iter().any(|r| r.chance < 1.0 || r.variance > 0.0);
                let first = sampled.insert((key.clone(), scenario));
                let runs = match (first, random) {
                    (true, true) => RANDOM_RUNS,
                    (true, false) => FIXED_RUNS,
                    (false, _) => REPEAT_RUNS,
                };
                let observed = if scenario == "explosion" {
                    sample_exploded(&mut world, b, runs)
                } else {
                    sample(&mut world, b, held, runs)
                };
                if let Some(message) = compare(&expected, &observed, first) {
                    failed_scenarios.insert(scenario);
                    problems.push(format!("{java} [{} #{} {key}] {scenario}: {message}", b.name(), b.0));
                }
            }
        }
        let problem = (!problems.is_empty()).then(|| problems.join("\n"));
        match (problem, allowed.get(java.as_str())) {
            (None, None) => audit.matched += 1,
            (None, Some(_)) => audit.failures.push(format!("{java} is allow-listed but matches Java")),
            (Some(_), Some(reason)) => audit.allowed.push((java.clone(), reason)),
            (Some(message), None) => {
                audit.failing.insert(java.clone());
                audit.failures.push(message);
            }
        }
    }
    audit
}
