//! v0.16 Track G: End crystals, the dragon respawn ritual, the obsidian
//! spikes, End gateways and the Nether portal forcer against
//! `fixtures/java/26.3/dragon_respawn.json`, which
//! `tools/extract_java_dragon_respawn_fixtures.py` reads from the pinned
//! server jar.

use serde_json::Value;
use voxelcraft::crafting::Grid;
use voxelcraft::entity::{crystal, dragon};
use voxelcraft::inventory::Stack;
use voxelcraft::item::Item;
use voxelcraft::world::block::Block;
use voxelcraft::world::{end, end_portal, portal};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/dragon_respawn.json")).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

fn ints(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().filter_map(Value::as_i64).collect()
}

#[test]
fn end_crystals_match_the_entity_item_and_recipe() {
    let fx = fixture();
    let t = &fx["entity_types"]["end_crystal"];
    assert_eq!(crystal::WIDTH, f(&t["width"]));
    assert_eq!(crystal::HEIGHT, f(&t["height"]));
    assert_eq!(t["fire_immune"], true);
    assert_eq!(f64::from(crystal::POWER), f(&fx["end_crystal"]["explosion_power"]));
    assert_eq!(fx["end_crystal"]["placeable_on"], serde_json::json!(["obsidian", "bedrock"]));
    // `new AABB(x, y, z, x + 1, y + 2, z + 1)` and the crystal at `x + 0.5`.
    let n = &fx["end_crystal"]["placement_numbers"];
    assert_eq!((f(&n[0]), f(&n[1]), f(&n[2]), f(&n[3])), (1.0, 2.0, 1.0, 0.5));

    let item = &fx["items"]["end_crystal"];
    assert_eq!(voxelcraft::java_registry::item("end_crystal", "End Crystal"), Some(crystal::END_CRYSTAL));
    assert_eq!(item["glint"], true);
    assert!(crystal::END_CRYSTAL.has_glint());
    assert_eq!(item["rarity"], "common");
    assert!(!crystal::END_CRYSTAL.is_rare() && !crystal::END_CRYSTAL.is_epic());
    assert_eq!(crystal::END_CRYSTAL.icon_layer(), Some(crystal::ICON_TEX));

    // `GGG / GEG / GTG`: glass, an eye of ender, a ghast tear.
    let recipe = &fx["recipe"];
    assert_eq!(recipe["pattern"], serde_json::json!(["GGG", "GEG", "GTG"]));
    let key = |c: &str| match recipe["key"][c].as_str().unwrap() {
        "minecraft:glass" => Item::from_block(Block::GLASS),
        "minecraft:ender_eye" => Item::EYE_OF_ENDER,
        "minecraft:ghast_tear" => Item::GHAST_TEAR,
        other => panic!("unexpected ingredient {other}"),
    };
    let mut grid = Grid::new(3);
    for (y, row) in ["GGG", "GEG", "GTG"].iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            grid.cells[y * 3 + x] = Some(Stack::new(key(&c.to_string()), 1));
        }
    }
    assert_eq!(grid.result(), Some(Stack::new(crystal::END_CRYSTAL, 1)));
    assert_eq!(recipe["result"]["id"], "minecraft:end_crystal");
}

#[test]
fn the_fight_and_ritual_follow_javas_numbers() {
    let fx = fixture();
    let c = &fx["fight"]["constants"];
    assert_eq!(i(&c["DRAGON_SPAWN_Y"]) as f64, dragon::SPAWN.y);
    assert_eq!(dragon::RESPAWN_BEAM.y as i64, i(&c["DRAGON_SPAWN_Y"]));
    assert_eq!(i(&c["TIME_BETWEEN_CRYSTAL_SCANS"]), i64::from(dragon::CRYSTAL_SCAN_TICKS));
    assert_eq!(i(&c["GATEWAY_COUNT"]), 20);
    assert_eq!(i(&c["GATEWAY_DISTANCE"]), 96);
    assert_eq!(f(&fx["fight"]["valid_player_distance"]), dragon::ARENA);
    assert_eq!(i(&fx["fight"]["gateway_y"]), i64::from(end::gateway_spot(0).y));
    assert_eq!(i(&fx["respawn"]["edge_distance"]), i64::from(dragon::RESPAWN_EDGE));

    let names: Vec<&str> = fx["respawn"]["stages"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    let ours = [
        dragon::Stage::Start,
        dragon::Stage::PreparingToSummonPillars,
        dragon::Stage::SummoningPillars,
        dragon::Stage::SummoningDragon,
    ]
    .map(dragon::Stage::name);
    assert_eq!(names[..4], ours);
    assert_eq!(names[4], "end");

    // PREPARING_TO_SUMMON_PILLARS: 100 ticks, growls (level event 3001 at
    // 0, 128, 0) at 0, 50, 51, 52 and from 95.
    let prep = ints(&fx["respawn"]["preparing_to_summon_pillars"]);
    assert_eq!(prep[..6], [i64::from(dragon::PREPARE_TICKS), 50, 51, 52, 95, 3001]);
    assert_eq!(prep[6..9], [0, 128, 0]);
    // SUMMONING_PILLARS: every 40 ticks, the 39th rebuilds, clearing 10
    // around the cap, a power 5 blast, the spike placed from y 45.
    let pillars = &fx["respawn"]["summoning_pillars"];
    let p = ints(pillars);
    assert_eq!(p[0], i64::from(dragon::PILLAR_TICKS));
    assert!(p.contains(&39));
    assert_eq!(p.iter().filter(|&&n| n == 10).count(), 7);
    assert_eq!(i64::from(dragon::PILLAR_CLEAR), 10);
    let floats: Vec<f64> = pillars.as_array().unwrap().iter().filter(|v| v.is_f64()).map(f).collect();
    assert_eq!(floats, vec![0.5, 0.5, f64::from(dragon::PILLAR_BLAST)]);
    // SUMMONING_DRAGON: at 100 the dragon and a power 6 blast per crystal,
    // growls from 80 and before 5.
    let d = &fx["respawn"]["summoning_dragon"];
    assert_eq!(i(&d[0]), i64::from(dragon::SUMMON_TICKS));
    assert_eq!(f(&d[1]), f64::from(dragon::CRYSTAL_POWER));
    assert_eq!(i(&d[2]), i64::from(dragon::SUMMON_GROWL_FROM));
    assert!(ints(d).contains(&5));

    let dr = &fx["dragon"];
    assert_eq!(i(&dr["first_kill_xp"]), i64::from(dragon::FIRST_XP));
    assert_eq!(i(&dr["later_kill_xp"]), i64::from(dragon::LATER_XP));
    let death: Vec<f64> = dr["death_numbers"].as_array().unwrap().iter().map(f).collect();
    assert!(death.windows(5).any(|w| w == [500.0, 12000.0, 150.0, 5.0, 0.08]));
    assert_eq!(*death.last().unwrap(), 0.2);
    assert_eq!(f(&dr["crystal_destroy_range"]), dragon::CRYSTAL_DESTROY_RANGE);
    let check: Vec<f64> = dr["check_crystals"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(check[..4], [10.0, 1.0, 10.0, dragon::HEAL_RANGE]);
}

#[test]
fn spikes_gateways_and_portals_follow_javas_numbers() {
    let fx = fixture();
    let s = &fx["spikes"];
    assert_eq!(i(&s["constants"]["NUMBER_OF_SPIKES"]), 10);
    assert_eq!(i(&s["constants"]["SPIKE_DISTANCE"]), 42);
    assert_eq!(i(&s["cache_key_mask"]), 0xFFFF);
    // `radius = 2 + size / 3`, `height = 76 + size * 3`, guarded 1 and 2.
    let l = ints(&s["loader_numbers"]);
    assert!(l.windows(6).any(|w| w == [2, 3, 76, 3, 1, 2]), "{l:?}");
    for spike in end::java_spikes(12345) {
        let size = (spike.top - 76) / 3;
        assert_eq!(spike.radius, 2 + size / 3);
        assert_eq!(spike.guarded, size == 1 || size == 2);
    }

    let g = &fx["gateway"];
    assert_eq!(i(&g["constants"]["GATEWAY_HEIGHT_ABOVE_SURFACE"]), i64::from(end_portal::EXIT_ABOVE));
    let t: Vec<f64> = g["tentative"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(t, vec![0.0, 1024.0, end_portal::EXIT_DISTANCE, 16.0, -16.0, 16.0, 16.0]);
    assert_eq!(i64::from(end_portal::EXIT_STEPS), 16);
    assert_eq!(i(&g["valid_spawn"][0]), i64::from(end_portal::SPAWN_MIN_Y));
    let tp: Vec<f64> = g["teleport_pos"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(tp[..4], [0.5, f64::from(end_portal::ISLAND_Y), 0.5, f64::from(end_portal::TALLEST_RADIUS)]);
    assert_eq!(ints(&g["exit_position"]), vec![0, 2, 0, i64::from(end_portal::ARRIVAL_RADIUS), 0]);
    // `EndIslandFeature`: `nextInt(3) + 4`, shrinking by `nextInt(2) + 0.5`.
    let isl: Vec<f64> = g["island"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(isl, vec![3.0, 4.0, 0.0, 0.5, 1.0, 1.0, 2.0, 0.5, 1.0]);
    let island = end_portal::java_island(glam::IVec3::new(1000, 75, -40));
    let top: Vec<_> = island.iter().filter(|p| p.y == 75).collect();
    let r = top.iter().map(|p| (p.x - 1000).abs()).max().unwrap();
    assert!((4..=6).contains(&r), "radius {r}");

    let pf = &fx["portal_forcer"]["constants"];
    assert_eq!(i(&pf["NETHER_PORTAL_RADIUS"]), i64::from(portal::NETHER_PORTAL_RADIUS));
    assert_eq!(i(&pf["OVERWORLD_PORTAL_RADIUS"]), i64::from(portal::OVERWORLD_PORTAL_RADIUS));
    let create = ints(&fx["portal_forcer"]["create_portal"]);
    assert!(create.contains(&i64::from(portal::CREATE_RADIUS)));
    assert!(create.windows(2).any(|w| w == [i64::from(portal::LEDGE_MIN_Y), i64::from(portal::LEDGE_BELOW_TOP)]));
}
