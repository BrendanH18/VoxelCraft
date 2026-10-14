//! Java 26.3 copper tools and nuggets against the pinned material facts
//! (`fixtures/java/26.3/combat.json`) and reference recipes.
use voxelcraft::{
    crafting::Grid,
    enchant,
    inventory::Stack,
    item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind},
    mining,
    world::{
        block::Block,
        furnace::{self, FurnaceKind},
    },
};

const KINDS: [ToolKind; 5] = [ToolKind::Pickaxe, ToolKind::Shovel, ToolKind::Axe, ToolKind::Hoe, ToolKind::Sword];

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/combat.json")).unwrap()
}

fn tier(name: &str) -> Tier {
    match name {
        "wood" => Tier::Wood,
        "stone" => Tier::Stone,
        "copper" => Tier::Copper,
        "iron" => Tier::Iron,
        "gold" => Tier::Gold,
        "diamond" => Tier::Diamond,
        "netherite" => Tier::Netherite,
        other => panic!("unknown reference material {other}"),
    }
}

#[test]
fn every_tool_material_matches_the_reference_material() {
    let f = fixture();
    for (name, facts) in f["materials"].as_object().unwrap() {
        let tier = tier(name);
        assert_eq!(u64::from(tier.durability()), facts["durability"].as_u64().unwrap(), "{name} durability");
        assert_eq!(f64::from(tier.speed()), facts["speed"].as_f64().unwrap(), "{name} speed");
        let pickaxe = Item::tool(ToolKind::Pickaxe, tier);
        assert_eq!(u64::from(enchant::enchantability(pickaxe)), facts["enchantability"].as_u64().unwrap(), "{name}");
        // Each missing `needs_*_tool` tag raises the harvest level; Netherite
        // harvests what diamond does.
        let excluded = facts["incorrect_block_tag_values"].as_array().unwrap().len() as u8;
        assert_eq!(tier.level().min(3), 3 - excluded, "{name} harvest level");
    }
}

#[test]
fn copper_tools_are_distinct_items_that_harvest_like_stone() {
    for kind in KINDS {
        let item = Item::tool(kind, Tier::Copper);
        assert_eq!(item.as_tool(), Some((kind, Tier::Copper)));
        assert_eq!(Item::from_name(item.name()), Some(item));
        assert!(item.name().starts_with("copper "), "{}", item.name());
        assert_eq!(item.max_stack(), 1);
    }
    let pick = Some(Item::tool(ToolKind::Pickaxe, Tier::Copper));
    assert!(mining::can_harvest(Block::IRON_ORE, pick));
    assert!(!mining::can_harvest(Block::DIAMOND_ORE, pick));
    let stone = Some(Item::tool(ToolKind::Pickaxe, Tier::Stone));
    assert!(mining::break_time(Block::STONE, pick) < mining::break_time(Block::STONE, stone));
}

fn craft(size: usize, cells: &[(usize, Item)]) -> Option<Stack> {
    let mut grid = Grid::new(size);
    for &(i, item) in cells {
        grid.cells[i] = Some(Stack::new(item, 1));
    }
    grid.result()
}

#[test]
fn copper_tools_and_nuggets_use_the_reference_recipes() {
    let (c, s) = (Item::COPPER_INGOT, Item::STICK);
    // copper_pickaxe: XXX / #  / #
    let pick = craft(3, &[(0, c), (1, c), (2, c), (4, s), (7, s)]);
    assert_eq!(pick, Some(Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Copper), 1)));
    let sword = craft(3, &[(1, c), (4, c), (7, s)]);
    assert_eq!(sword, Some(Stack::new(Item::tool(ToolKind::Sword, Tier::Copper), 1)));
    assert_eq!(craft(2, &[(0, c)]), Some(Stack::new(Item::COPPER_NUGGET, 9)));
    let nine: Vec<_> = (0..9).map(|i| (i, Item::COPPER_NUGGET)).collect();
    assert_eq!(craft(3, &nine), Some(Stack::new(Item::COPPER_INGOT, 1)));
    // Repairs on the anvil take the material tag's copper ingot.
    assert!(enchant::repairs(Item::tool(ToolKind::Axe, Tier::Copper), c));
    assert!(!enchant::repairs(Item::tool(ToolKind::Axe, Tier::Copper), Item::IRON_INGOT));
}

#[test]
fn metal_gear_smelts_and_blasts_to_nuggets_as_in_java() {
    let cases = [
        (Item::tool(ToolKind::Sword, Tier::Copper), Some(Item::COPPER_NUGGET)),
        (Item::tool(ToolKind::Hoe, Tier::Iron), Some(Item::IRON_NUGGET)),
        (Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), Some(Item::IRON_NUGGET)),
        (Item::tool(ToolKind::Axe, Tier::Gold), Some(Item::GOLD_NUGGET)),
        (Item::armor(ArmorPiece::Boots, ArmorMaterial::Gold), Some(Item::GOLD_NUGGET)),
        (Item::from_name("iron_horse_armor").unwrap(), Some(Item::IRON_NUGGET)),
        (Item::from_name("golden_horse_armor").unwrap(), Some(Item::GOLD_NUGGET)),
        (Item::tool(ToolKind::Sword, Tier::Diamond), None),
        (Item::tool(ToolKind::Sword, Tier::Stone), None),
    ];
    for (item, nugget) in cases {
        assert_eq!(FurnaceKind::Normal.smelt(item), nugget, "{}", item.name());
        assert_eq!(FurnaceKind::Blast.smelt(item), nugget, "{} blasting", item.name());
        assert_eq!(FurnaceKind::Smoker.smelt(item), None, "{} smoker", item.name());
        if let Some(n) = nugget {
            assert_eq!(furnace::smelt_xp(n), 0.1);
        }
    }
}
