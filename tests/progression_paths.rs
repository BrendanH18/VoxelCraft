//! v0.16 Track G: the M5 progression audit's item paths. Starting from what
//! a survival player gathers (mined blocks, mob drops, structure loot, each
//! with its own checks elsewhere), every item on the way from the first
//! night to the dragon, its respawn, the Wither and the outer End must be
//! reachable through the game's own crafting, smelting and smithing.
//! A missing link is a progression defect.

use std::collections::HashSet;

use voxelcraft::crafting::recipes;
use voxelcraft::inventory::Stack;
use voxelcraft::item::Item;
use voxelcraft::java_registry;

/// Items a survival player gathers rather than crafts, by Java name, with
/// where each comes from.
const GATHERED: &[(&str, &str)] = &[
    ("oak_log", "trees"),
    ("oak_sapling", "leaves"),
    ("stick", "dead bushes and leaves"),
    ("cobblestone", "stone, mined with a pickaxe"),
    ("dirt", "terrain"),
    ("sand", "terrain"),
    ("gravel", "terrain"),
    ("flint", "gravel"),
    ("coal", "coal ore"),
    ("raw_iron", "iron ore"),
    ("raw_gold", "gold ore"),
    ("diamond", "diamond ore"),
    ("redstone", "redstone ore"),
    ("lapis_lazuli", "lapis ore"),
    ("sugar_cane", "riverbanks"),
    ("leather", "cows"),
    ("white_wool", "sheep"),
    ("string", "spiders"),
    ("spider_eye", "spiders"),
    ("brown_mushroom", "dark places"),
    ("feather", "chickens"),
    ("gunpowder", "creepers and ghasts"),
    ("bone", "skeletons"),
    ("water_bucket", "a bucket in water"),
    ("lava_bucket", "a bucket in lava"),
    ("obsidian", "water on lava, mined with diamond"),
    ("netherrack", "the Nether"),
    ("soul_sand", "the Nether"),
    ("quartz", "nether quartz ore"),
    ("glowstone_dust", "glowstone"),
    ("nether_wart", "fortresses"),
    ("blaze_rod", "blazes, by players"),
    ("ender_pearl", "endermen and piglin bartering"),
    ("ghast_tear", "ghasts"),
    ("ancient_debris", "the Nether, mined with diamond"),
    ("netherite_upgrade_smithing_template", "bastion chests"),
    ("wither_skeleton_skull", "wither skeletons, by players"),
    ("nether_star", "the Wither"),
    ("end_stone", "the End"),
    ("shulker_shell", "shulkers"),
    ("elytra", "End ship item frames"),
    ("phantom_membrane", "phantoms"),
    ("iron_block", "crafted, or found"),
];

/// Items each stage of the M5 exit needs, by Java name.
const TARGETS: &[(&str, &[&str])] = &[
    (
        "first night",
        &["crafting_table", "wooden_pickaxe", "stone_pickaxe", "stone_sword", "furnace", "torch", "white_bed", "chest"],
    ),
    (
        "equipment",
        &[
            "iron_pickaxe",
            "iron_sword",
            "iron_chestplate",
            "shield",
            "bucket",
            "diamond_pickaxe",
            "diamond_sword",
            "diamond_chestplate",
        ],
    ),
    ("Nether access", &["flint_and_steel"]),
    ("brewing", &["blaze_powder", "brewing_stand", "glass_bottle", "fermented_spider_eye"]),
    (
        "enchanting and upgrades",
        &["book", "bookshelf", "enchanting_table", "anvil", "netherite_ingot", "smithing_table"],
    ),
    ("stronghold, dragon and return", &["ender_eye", "bow", "arrow"]),
    ("dragon respawn", &["end_crystal"]),
    ("post-dragon End", &["firework_rocket", "shulker_box"]),
    ("Wither and beacons", &["beacon", "diamond_block", "gold_block"]),
];

fn item(name: &str) -> Item {
    java_registry::item(name, "").unwrap_or_else(|| panic!("{name} isn't registered"))
}

/// Everything craftable, smeltable or forged from `start`.
fn closure(start: &HashSet<Item>) -> HashSet<Item> {
    let mut have = start.clone();
    loop {
        let before = have.len();
        for r in recipes() {
            if !have.contains(&r.result.item)
                && r.slots().iter().all(|options| options.iter().any(|i| have.contains(i)))
            {
                have.insert(r.result.item);
            }
        }
        let smelted: Vec<Item> = have.iter().filter_map(|&i| voxelcraft::world::furnace::smelt(i)).collect();
        have.extend(smelted);
        // Smithing: the netherite upgrade of whatever diamond gear we have.
        let template = item("netherite_upgrade_smithing_template");
        let ingot = item("netherite_ingot");
        if have.contains(&template) && have.contains(&ingot) {
            let upgraded: Vec<Item> = have
                .iter()
                .filter_map(|&base| {
                    voxelcraft::smithing::upgrade(Stack::new(template, 1), Stack::new(base, 1), Stack::new(ingot, 1))
                })
                .map(|s| s.item)
                .collect();
            have.extend(upgraded);
        }
        if have.len() == before {
            return have;
        }
    }
}

#[test]
fn every_progression_item_is_reachable_in_survival() {
    let start: HashSet<Item> = GATHERED.iter().map(|(name, _)| item(name)).collect();
    let have = closure(&start);
    let mut missing = Vec::new();
    for (stage, items) in TARGETS {
        for name in *items {
            if !have.contains(&item(name)) {
                missing.push(format!("{stage}: {name}"));
            }
        }
    }
    assert!(missing.is_empty(), "unreachable progression items: {missing:#?}");
    // Netherite gear comes from diamond gear at the smithing table.
    assert!(have.contains(&item("netherite_pickaxe")) && have.contains(&item("netherite_chestplate")));
}

#[test]
fn the_end_crystal_needs_the_nether_and_the_end() {
    // Without ghast tears there are no End crystals, so no respawn.
    let start: HashSet<Item> =
        GATHERED.iter().filter(|(name, _)| *name != "ghast_tear").map(|(name, _)| item(name)).collect();
    assert!(!closure(&start).contains(&item("end_crystal")));
    // Without ender pearls, no eyes of ender: no stronghold portal.
    let start: HashSet<Item> =
        GATHERED.iter().filter(|(name, _)| *name != "ender_pearl").map(|(name, _)| item(name)).collect();
    let have = closure(&start);
    assert!(!have.contains(&item("ender_eye")) && !have.contains(&item("end_crystal")));
}
