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

fn armor_material(name: &str) -> Option<ArmorMaterial> {
    Some(match name {
        "leather" => ArmorMaterial::Leather,
        "copper" => ArmorMaterial::Copper,
        "iron" => ArmorMaterial::Iron,
        "gold" => ArmorMaterial::Gold,
        "diamond" => ArmorMaterial::Diamond,
        "netherite" => ArmorMaterial::Netherite,
        "turtle_scute" => ArmorMaterial::Turtle,
        // Chainmail is a monster-only look here; wolf armor is not registered.
        "chainmail" | "armadillo_scute" => return None,
        other => panic!("unknown reference armor material {other}"),
    })
}

#[test]
fn every_armor_material_matches_the_reference_material() {
    let f = fixture();
    let units = &f["armor_durability_units"];
    let pieces = [
        (ArmorPiece::Helmet, "helmet"),
        (ArmorPiece::Chestplate, "chestplate"),
        (ArmorPiece::Leggings, "leggings"),
        (ArmorPiece::Boots, "boots"),
    ];
    let mut checked = 0;
    for (name, facts) in f["armor_materials"].as_object().unwrap() {
        let Some(material) = armor_material(name) else { continue };
        for (piece, key) in pieces {
            // The turtle material only has a helmet.
            if material == ArmorMaterial::Turtle && piece != ArmorPiece::Helmet {
                continue;
            }
            let durability = facts["durability_multiplier"].as_u64().unwrap() * units[key].as_u64().unwrap();
            assert_eq!(u64::from(material.durability(piece)), durability, "{name} {key} durability");
            assert_eq!(u64::from(material.defense(piece)), facts["defense"][key].as_u64().unwrap(), "{name} {key}");
            let enchantable = enchant::enchantability(Item::armor(piece, material));
            assert_eq!(u64::from(enchantable), facts["enchantability"].as_u64().unwrap(), "{name} {key}");
        }
        assert_eq!(material.toughness(), facts["toughness"].as_f64().unwrap() as f32, "{name}");
        assert_eq!(material.knockback_resistance(), facts["knockback_resistance"].as_f64().unwrap() as f32, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 7);
}

#[test]
fn copper_armor_is_crafted_repaired_worn_and_melted_down() {
    let c = Item::COPPER_INGOT;
    // copper_chestplate: X X / XXX / XXX
    let chest = craft(3, &[(0, c), (2, c), (3, c), (4, c), (5, c), (6, c), (7, c), (8, c)]);
    assert_eq!(chest, Some(Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Copper), 1)));
    for piece in ArmorPiece::ALL {
        let item = Item::armor(piece, ArmorMaterial::Copper);
        assert_eq!(item.as_armor(), Some((piece, ArmorMaterial::Copper)));
        assert_eq!(Item::from_name(item.name()), Some(item));
        assert!(enchant::repairs(item, c));
        assert_eq!(FurnaceKind::Blast.smelt(item), Some(Item::COPPER_NUGGET));
    }
    assert_eq!(
        voxelcraft::entity::armor::ArmorKind::of(ArmorMaterial::Copper),
        voxelcraft::entity::armor::ArmorKind::Copper
    );
}
