//! Every registered block state against Java 26.3's mining properties:
//! hardness, blast resistance, whether each tool (and a bare hand) harvests
//! it, and the per-tick dig progress each tool makes. See `block_mining.json`.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use voxelcraft::{
    enchant::Enchants,
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    mining::{self, Digger},
    world::block::{Block, STATE_CAPACITY},
};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/java/26.3/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// VoxelCraft state names that are a state of a differently named Java block.
const STATE_ALIASES: &[(&str, &str)] = &[
    ("snowy grass", "grass_block"),
    ("lit redstone ore", "redstone_ore"),
    ("lit deepslate redstone ore", "deepslate_redstone_ore"),
    ("lit furnace", "furnace"),
    ("lit smoker", "smoker"),
    ("lit blast furnace", "blast_furnace"),
    ("wet farmland", "farmland"),
    ("snow layer", "snow"),
    ("snow", "snow_block"),
    ("wool", "white_wool"),
    ("vines", "vine"),
    ("hay bale", "hay_block"),
    ("grass", "grass_block"),
    // VoxelCraft's single "tall grass" is Java's short grass; the double
    // plant is "tall grass plant".
    ("tall grass", "short_grass"),
    ("tall grass plant", "tall_grass"),
    ("tall grass plant top", "tall_grass"),
    ("sunflower top", "sunflower"),
    ("lilac top", "lilac"),
    ("rose bush top", "rose_bush"),
    ("peony top", "peony"),
    ("large fern top", "large_fern"),
    ("small dripleaf top", "small_dripleaf"),
    ("tall seagrass top", "tall_seagrass"),
    ("pale hanging moss plant", "pale_hanging_moss"),
    ("hanging mangrove propagule", "mangrove_propagule"),
    ("carrot crops", "carrots"),
    ("potato crops", "potatoes"),
    ("chain", "iron_chain"),
    ("legacy bone block", "bone_block"),
    ("flowing water", "water"),
    ("falling water", "water"),
    ("flowing lava", "lava"),
    ("falling lava", "lava"),
    ("bed foot", "red_bed"),
    ("bed head", "red_bed"),
];

const COLOURS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// Intentional differences, each with its reason. Keyed by Java ID.
const ALLOWED: &[(&str, &str)] = &[
    // Java never mines these: their outline shape is empty, so the crosshair
    // can't target them and `getDestroyProgress` is never asked. VoxelCraft
    // marks them unbreakable instead of carrying Java's unused 0 and 100.
    ("air", "never a mining target (empty outline shape)"),
    ("water", "never a mining target (empty outline shape)"),
    ("lava", "never a mining target (empty outline shape)"),
];

/// Java ID of each VoxelCraft state, resolved by Java ID or English name.
fn java_ids() -> BTreeMap<u16, String> {
    let names = fixture("registry_names.json");
    let mut by_name = BTreeMap::new();
    for (id, english) in names["blocks"].as_object().unwrap() {
        by_name.insert(id.replace('_', " "), id.clone());
        by_name.entry(english.as_str().unwrap().to_lowercase()).or_insert_with(|| id.clone());
    }
    for (alias, id) in STATE_ALIASES {
        by_name.insert(alias.to_string(), id.to_string());
    }
    for colour in COLOURS {
        for half in ["foot", "head"] {
            by_name.insert(format!("{colour} bed {half}"), format!("{}_bed", colour.replace(' ', "_")));
        }
    }
    (0..STATE_CAPACITY as u16)
        .filter_map(|i| {
            let name = Block(i).name();
            (!name.is_empty()).then(|| by_name.get(name).map(|id| (i, id.clone()))).flatten()
        })
        .collect()
}

struct Java {
    blocks: Value,
    mineable: HashMap<ToolKind, BTreeSet<String>>,
    incorrect: HashMap<Tier, BTreeSet<String>>,
    speed: HashMap<Tier, f32>,
    sword_instant: BTreeSet<String>,
    sword_efficient: BTreeSet<String>,
    shears: [(f32, BTreeSet<String>); 3],
}

fn set(v: &Value) -> BTreeSet<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

const TIERS: [(Tier, &str, &str); 7] = [
    (Tier::Wood, "wooden", "wood"),
    (Tier::Stone, "stone", "stone"),
    (Tier::Copper, "copper", "copper"),
    (Tier::Iron, "iron", "iron"),
    (Tier::Gold, "gold", "gold"),
    (Tier::Diamond, "diamond", "diamond"),
    (Tier::Netherite, "netherite", "netherite"),
];
const KINDS: [(ToolKind, &str); 5] = [
    (ToolKind::Pickaxe, "pickaxe"),
    (ToolKind::Axe, "axe"),
    (ToolKind::Shovel, "shovel"),
    (ToolKind::Hoe, "hoe"),
    (ToolKind::Sword, "sword"),
];

impl Java {
    fn load() -> Java {
        let block_mining = fixture("block_mining.json");
        let mining = fixture("mining.json");
        let combat = fixture("combat.json");
        let rules = &mining["tool_rules"];
        assert_eq!(block_mining["correct_for_drops_only"]["sword"], serde_json::json!(["cobweb"]));
        assert_eq!(block_mining["correct_for_drops_only"]["shears"], serde_json::json!(["cobweb"]));
        Java {
            blocks: block_mining["blocks"].clone(),
            mineable: KINDS[..4].iter().map(|&(k, n)| (k, set(&rules["mineable"][n]))).collect(),
            incorrect: TIERS.iter().map(|&(t, n, _)| (t, set(&block_mining["incorrect_for_tool"][n]))).collect(),
            speed: TIERS
                .iter()
                .map(|&(t, _, m)| (t, combat["materials"][m]["speed"].as_f64().unwrap() as f32))
                .collect(),
            sword_instant: set(&rules["sword_instantly_mines"]),
            sword_efficient: set(&rules["sword_efficient"]),
            shears: [
                (15.0, set(&rules["shears"]["extreme"])),
                (5.0, set(&rules["shears"]["major"])),
                (2.0, set(&rules["shears"]["minor"])),
            ],
        }
    }

    fn hardness(&self, id: &str) -> f32 {
        self.blocks[id]["hardness"].as_f64().unwrap() as f32
    }

    /// `Tool.isCorrectForDrops` for the held item.
    fn correct(&self, id: &str, held: Option<Item>) -> bool {
        if held == Some(Item::SHEARS) {
            return id == "cobweb";
        }
        match held.and_then(Item::as_tool) {
            Some((ToolKind::Sword, _)) => id == "cobweb",
            Some((kind, tier)) => !self.incorrect[&tier].contains(id) && self.mineable[&kind].contains(id),
            None => false,
        }
    }

    /// `Player.hasCorrectToolForDrops`.
    fn harvests(&self, id: &str, held: Option<Item>) -> bool {
        !self.blocks[id]["requires_correct_tool"].as_bool().unwrap() || self.correct(id, held)
    }

    /// `Tool.getMiningSpeed`: the first rule with a speed, else 1.
    fn speed(&self, id: &str, held: Option<Item>) -> f32 {
        if held == Some(Item::SHEARS) {
            if id == "cobweb" {
                return 15.0;
            }
            return self.shears.iter().find(|(_, s)| s.contains(id)).map_or(1.0, |(v, _)| *v);
        }
        match held.and_then(Item::as_tool) {
            Some((ToolKind::Sword, _)) if id == "cobweb" => 15.0,
            Some((ToolKind::Sword, _)) if self.sword_instant.contains(id) => f32::MAX,
            Some((ToolKind::Sword, _)) if self.sword_efficient.contains(id) => 1.5,
            Some((ToolKind::Sword, _)) => 1.0,
            Some((kind, tier)) if self.mineable[&kind].contains(id) => self.speed[&tier],
            _ => 1.0,
        }
    }

    /// `BlockBehaviour.getDestroyProgress` on the ground, out of water.
    fn progress(&self, id: &str, held: Option<Item>) -> f32 {
        let hardness = self.hardness(id);
        if hardness == -1.0 {
            return 0.0;
        }
        let divisor = if self.harvests(id, held) { 30.0 } else { 100.0 };
        self.speed(id, held) / hardness / divisor
    }
}

fn held_items() -> Vec<Option<Item>> {
    let mut items = vec![None, Some(Item::STICK), Some(Item::SHEARS)];
    for (kind, _) in KINDS {
        for (tier, ..) in TIERS {
            items.push(Some(Item::tool(kind, tier)));
        }
    }
    items
}

fn describe(held: Option<Item>) -> String {
    held.map_or("hand".into(), |i| i.name().to_string())
}

#[test]
fn every_registered_block_state_mines_like_java_26_3() {
    let java = Java::load();
    let ids = java_ids();
    let allowed: BTreeMap<&str, &str> = ALLOWED.iter().copied().collect();
    let held = held_items();
    let mut mismatches: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut compared = BTreeSet::new();
    for (&state, id) in &ids {
        let block = Block(state);
        assert!(java.blocks.get(id).is_some(), "{id} is not a Java block");
        compared.insert(id.clone());
        let mut found = BTreeSet::new();
        let expected = java.hardness(id);
        let expected = if expected == -1.0 { f32::INFINITY } else { expected };
        if block.hardness() != expected {
            found.insert(format!("hardness {} (Java {expected})", block.hardness()));
        }
        let resistance = java.blocks[id]["blast_resistance"].as_f64().unwrap() as f32;
        if block.blast_resistance() != resistance {
            found.insert(format!("blast resistance {} (Java {resistance})", block.blast_resistance()));
        }
        for &item in &held {
            if mining::can_harvest(block, item) != java.harvests(id, item) {
                found.insert(format!("harvest with {}: {}", describe(item), mining::can_harvest(block, item)));
            }
            let digger = Digger {
                held: item.map(|i| Stack::new(i, 1)),
                helmet: Enchants::NONE,
                eyes_in_water: false,
                on_ground: true,
            };
            let progress = mining::dig_progress(block, digger, 1.0);
            let want = java.progress(id, item);
            if progress.to_bits() != want.to_bits() && !(progress.is_infinite() && want.is_infinite()) {
                found.insert(format!("progress with {}: {progress} (Java {want})", describe(item)));
            }
        }
        if !found.is_empty() && !allowed.contains_key(id.as_str()) {
            mismatches.entry(format!("{id} ({})", block.name())).or_default().extend(found);
        }
    }
    let unregistered: Vec<&String> =
        java.blocks.as_object().unwrap().keys().filter(|id| !compared.contains(*id)).collect();
    let unused: Vec<&&str> = allowed.keys().filter(|id| !compared.contains(**id)).collect();
    println!(
        "compared {} Java blocks over {} states; {} unregistered: {unregistered:?}",
        compared.len(),
        ids.len(),
        unregistered.len()
    );
    assert!(unused.is_empty(), "allow-list names blocks that aren't registered: {unused:?}");
    let report: Vec<String> = mismatches.iter().map(|(k, v)| format!("{k}: {v:?}")).collect();
    assert!(report.is_empty(), "{} blocks differ from Java 26.3:\n{}", report.len(), report.join("\n"));
}

/// Registered states with no Java name are VoxelCraft-only (retired invented
/// blocks, internal parts); list them so a renamed block can't slip through.
#[test]
fn unmatched_block_names_are_known_voxelcraft_states() {
    let ids = java_ids();
    let unmatched: BTreeSet<&str> = (0..STATE_CAPACITY as u16)
        .filter(|i| !ids.contains_key(i))
        .map(|i| Block(i).name())
        .filter(|n| !n.is_empty())
        .collect();
    // The 11 retired invented blocks (owner decision, 2026-10-25) and the
    // placeholder name of unassigned states.
    let known = BTreeSet::from([
        "calcite slab",
        "calcite stairs",
        "calcite wall",
        "deepslate slab",
        "deepslate stairs",
        "deepslate wall",
        "polished andesite wall",
        "polished diorite wall",
        "polished granite wall",
        "smooth stone stairs",
        "smooth stone wall",
        "unknown",
    ]);
    assert_eq!(unmatched, known, "registered states without a Java name lose mining coverage");
}
