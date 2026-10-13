//! Newly completed families use reference recipes, preserved numeric state IDs and shapes.
use serde_json::Value;
use voxelcraft::{
    crafting::Grid,
    inventory::Stack,
    item::Item,
    world::{
        block::{Block, Facing, RenderKind},
        forms,
    },
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/building_integrations.json")).unwrap()
}
#[test]
fn every_new_building_recipe_crafts_and_consumes_the_java_grid() {
    for case in fixture()["recipes"].as_array().unwrap() {
        let mut grid = Grid::new(3);
        for (y, row) in case["pattern"].as_array().unwrap().iter().enumerate() {
            for (x, c) in row.as_str().unwrap().chars().enumerate() {
                if c != ' ' {
                    let name = case["ingredients"][c.to_string()].as_str().unwrap().trim_start_matches("minecraft:");
                    grid.cells[y * 3 + x] =
                        Some(Stack::new(Item::from_name(name).unwrap_or_else(|| panic!("missing {name}")), 1));
                }
            }
        }
        let name = case["result"].as_str().unwrap().trim_start_matches("minecraft:");
        let item = Item::from_name(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(grid.result(), Some(Stack::new(item, case["count"].as_u64().unwrap() as u8)), "{name}");
        grid.consume();
        assert!(grid.cells.iter().all(Option::is_none));
        let block = item.block().unwrap();
        assert_ne!(block.kind(), RenderKind::Invisible);
        assert_eq!(block.drop(), Some(item));
    }
    for name in fixture()["intentionally_absent"].as_array().unwrap() {
        assert!(Item::from_name(name.as_str().unwrap()).is_none());
    }
}
#[test]
fn new_shapes_have_material_tools_fuel_and_orientation_without_reusing_old_ids() {
    for i in 9..=11 {
        for (local, fuel) in [(0, 300), (4, 150), (5, 300), (6, 300), (14, 200)] {
            if i == 11 && local > 4 {
                continue;
            }
            let b = forms::wood_id(i, local);
            assert!(b.0 >= 2200);
            assert_eq!(b.best_tool(), Some(voxelcraft::item::ToolKind::Axe));
            assert_eq!(voxelcraft::content::fuel::ticks(Item::from_block(b)), Some(fuel));
            if local == 0 {
                for facing in Facing::ALL {
                    let state = b.with_facing(facing);
                    assert_eq!(state.stairs_base(), Some(forms::wood_planks(i)));
                    assert_eq!(state.drop(), Some(Item::from_block(b)));
                }
            }
        }
    }
    for i in 15..19 {
        for local in 0..=4 {
            let b = forms::stone_id(i, local);
            assert_eq!(b.best_tool(), Some(voxelcraft::item::ToolKind::Pickaxe));
            assert_eq!(b.harvest_level(), Some(0));
            assert_eq!(b.hardness(), 1.5);
        }
    }
    assert_eq!(forms::wood_id(7, 0), Block(1688));
    assert_eq!(forms::wood_id(8, 22), Block(1733));
    assert_eq!(forms::stone_id(12, 5), Block(330));
}
