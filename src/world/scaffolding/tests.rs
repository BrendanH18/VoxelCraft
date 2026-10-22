//! Expected values come from Java 26.3 ScaffoldingBlock/ScaffoldingBlockItem,
//! PowderSnowBlock, the recipe/loot/tag JSON, and EntityCollisionContext.
use super::*;
use crate::{
    entity::{Entities, PlayerId},
    inventory::{Inventory, Stack},
    item::{Item, Tier, ToolKind},
    physics::{self, CollisionContext},
    player::Player,
    rules::GameMode,
    session::{PlayerSession, placement::PlaceContext},
    world::{chunk::ChunkData, terrain::Generator},
};
use glam::{DVec3, Vec3};
use std::sync::Arc;
const P: IVec3 = IVec3::new(8, 140, 8);
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w.set_block(P - IVec3::Y, Block::STONE);
    w
}
fn ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.tick_redstone();
    }
}
#[test]
fn java_scaffolding_distance_is_inherited_upward_and_increments_sideways() {
    let mut w = world();
    for y in 0..3 {
        w.set_block(P + IVec3::Y * y, state(0, false, false));
    }
    assert_eq!(distance(|p| w.get_block(p), P + IVec3::Y * 3), 0);
    for x in 1..=6 {
        let p = P + IVec3::X * x;
        assert_eq!(distance(|p| w.get_block(p), p), x as u8);
        w.set_block(p, placed(|p| w.get_block(p), p, false));
    }
    let p = P + IVec3::X * 7;
    assert_eq!(distance(|p| w.get_block(p), p), 7);
    assert_eq!(properties(placed(|p| w.get_block(p), P + IVec3::X, false)), Some((1, true, false)));
    w.set_block(P + IVec3::X + IVec3::Y, state(1, false, false));
    assert_eq!(properties(placed(|p| w.get_block(p), P + IVec3::X + IVec3::Y, false)), Some((1, false, false)));
}
#[test]
fn java_scaffolding_break_cascade_drops_formerly_stable_blocks() {
    let mut w = world();
    for y in 0..4 {
        w.set_block(P + IVec3::Y * y, state(0, false, false));
    }
    for x in 1..=6 {
        w.set_block(P + IVec3::X * x, state(x as u8, true, false));
    }
    ticks(&mut w, 2);
    w.set_block(P, Block::AIR);
    ticks(&mut w, 30);
    assert!(w.falling.is_empty(), "previous distance below 7 destroys rather than falls");
    assert!((1..4).all(|y| w.get_block(P + IVec3::Y * y) == Some(Block::AIR)));
    assert!((1..=6).all(|x| w.get_block(P + IVec3::X * x) == Some(Block::AIR)));
    assert_eq!(
        w.drops.iter().filter(|(_, s)| s.item == Item::SCAFFOLDING).map(|(_, s)| s.count as usize).sum::<usize>(),
        9
    );
}
#[test]
fn java_scaffolding_distance_seven_falls_then_lands_and_persists() {
    let mut w = world();
    let p = P + IVec3::Y * 4;
    w.set_block(p, SCAFFOLDING);
    let pending = w.redstone_to_string();
    w.load_redstone(&pending);
    ticks(&mut w, 1);
    assert_eq!(w.get_block(p), Some(Block::AIR));
    assert_eq!(w.falling.len(), 1);
    let saved = w.falling_to_string();
    w.falling.clear();
    w.load_falling(&saved);
    assert_eq!(w.falling.len(), 1);
    for _ in 0..100 {
        w.tick_falling(0.05);
        ticks(&mut w, 1);
    }
    assert!(w.falling.is_empty());
    assert_eq!(w.get_block(P), Some(state(0, false, false)));
}
#[test]
fn a_waterlogged_scaffold_that_falls_leaves_its_water_behind() {
    let mut w = world();
    let p = P + IVec3::Y * 4;
    w.set_block(p, state(7, false, true));
    ticks(&mut w, 1);
    assert_eq!(w.get_block(p), Some(Block::WATER));
    assert_eq!(w.falling.len(), 1);
    assert!(!properties(w.falling[0].block).unwrap().2, "the falling entity is dry");
}
#[test]
fn player_freezes_in_powder_snow_and_leather_prevents_it_through_tick_player() {
    use crate::{
        item::{ArmorMaterial, ArmorPiece},
        player::MoveInput,
        simulation::{self, survival::Vitals},
    };
    let run = |armor: [Option<Stack>; 4]| {
        let mut w = world();
        w.set_block(P, POWDER_SNOW);
        let mut player = Player::new(P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        let mut vitals = Vitals::default();
        let mut first_damage = None;
        let mut at_140 = 0;
        for tick in 1..=200 {
            let step = simulation::tick_player(&mut player, &mut w, &mut vitals, &armor, MoveInput::default(), false);
            if tick == 140 {
                at_140 = vitals.ticks_frozen;
            }
            if step.hurts.freeze > 0.0 && first_damage.is_none() {
                first_damage = Some(tick);
            }
        }
        (at_140, first_damage, vitals.ticks_frozen)
    };
    assert_eq!(run([None; 4]), (140, Some(160), 140));
    let mut boots = [None; 4];
    boots[3] = Some(Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather), 1));
    assert_eq!(run(boots), (0, None, 0));
}
fn context(clicked: IVec3, block: Block, face: IVec3, sneaking: bool) -> PlaceContext {
    PlaceContext {
        clicked,
        clicked_block: block,
        face,
        click: DVec3::splat(0.5),
        at: clicked + face,
        facing: Facing::East,
        look: Vec3::X,
        sneaking,
    }
}
#[test]
fn java_scaffolding_item_extends_up_or_out_and_obeys_seven_and_build_limit() {
    let mut w = world();
    w.set_block(P, state(0, false, false));
    let target =
        |w: &World, c: &PlaceContext, inside| placement_target(|p| w.get_block(p), c, inside, w.min_y(), w.max_y());
    assert_eq!(target(&w, &context(P, SCAFFOLDING, IVec3::X, false), false), Some(P + IVec3::Y));
    assert_eq!(target(&w, &context(P, SCAFFOLDING, IVec3::Y, false), false), Some(P + IVec3::X));
    assert_eq!(target(&w, &context(P, SCAFFOLDING, IVec3::X, true), false), Some(P + IVec3::X));
    assert_eq!(target(&w, &context(P, SCAFFOLDING, IVec3::X, true), true), Some(P - IVec3::X));
    for x in 1..=6 {
        w.set_block(P + IVec3::X * x, state(x as u8, true, false));
    }
    assert_eq!(
        target(&w, &context(P, SCAFFOLDING, IVec3::Y, false), false),
        Some(P + IVec3::X * 7),
        "seventh outward block is placed and then falls"
    );
    w.set_block(P + IVec3::X * 7, SCAFFOLDING);
    assert_eq!(target(&w, &context(P, SCAFFOLDING, IVec3::Y, false), false), None);
    assert_eq!(
        placement_target(|_| Some(SCAFFOLDING), &context(P, SCAFFOLDING, IVec3::X, false), false, 0, P.y + 4),
        None
    );
}
#[test]
fn java_conditional_collision_above_inside_descending_and_fall_threshold() {
    use physics::test_util::Grid;
    let mut g = Grid::flat(-10);
    g.set(IVec3::ZERO, state(0, false, false));
    let query = |g: &Grid, feet, desc, fall, boots| {
        let mut out = Vec::new();
        physics::colliders_context(
            g,
            DVec3::new(0.2, -0.1, 0.2),
            DVec3::new(0.8, 1.2, 0.8),
            Some(CollisionContext { feet_y: feet, descending: desc, fall_distance: fall, leather_boots: boots }),
            &mut out,
        );
        out
    };
    assert!(!query(&g, 1.0, false, 0.0, false).is_empty());
    assert!(query(&g, 0.5, false, 0.0, false).is_empty());
    assert!(query(&g, 1.0, true, 0.0, false).is_empty());
    g.set(IVec3::ZERO, state(1, true, false));
    let rim = query(&g, 0.5, true, 0.0, false);
    assert_eq!(rim.len(), 1);
    assert_eq!(rim[0].max.y, 0.125);
    g.set(IVec3::ZERO, POWDER_SNOW);
    assert!(query(&g, 1.0, false, 2.5, false).is_empty());
    assert_eq!(query(&g, 1.0, false, 2.50001, false)[0].max.y, f64::from(0.9_f32));
    assert_eq!(query(&g, 1.0, false, 0.0, true)[0].max.y, 1.0);
    assert!(query(&g, 0.5, false, 0.0, true).is_empty());
    assert!(query(&g, 1.0, true, 0.0, true).is_empty());
}
#[test]
fn java_recipe_fuel_mining_loot_pick_and_waterlogging() {
    use crate::{crafting::Grid, world::overworld_blocks::BAMBOO};
    let mut grid = Grid::new(3);
    for i in [0, 2, 3, 5, 6, 8] {
        grid.cells[i] = Some(Stack::new(BAMBOO, 1));
    }
    grid.cells[1] = Some(Stack::new(Item::STRING, 1));
    assert_eq!(grid.result(), Some(Stack::new(Item::SCAFFOLDING, 6)));
    assert_eq!(crate::content::fuel::ticks(Item::SCAFFOLDING), Some(50));
    assert_eq!(SCAFFOLDING.hardness(), 0.0);
    assert_eq!(SCAFFOLDING.fire_odds(), (60, 60));
    assert_eq!(SCAFFOLDING.with_water(true).fire_odds(), (0, 0));
    assert_eq!(crate::session::interact::picked_item(state(3, true, true)), Item::SCAFFOLDING);
    assert_eq!(crate::session::interact::picked_item(POWDER_SNOW), Item::POWDER_SNOW_BUCKET);
    assert_eq!(POWDER_SNOW.drop(), None);
    assert_eq!(crate::mining::silk_drop(POWDER_SNOW), None);
    assert_eq!(POWDER_SNOW.hardness(), 0.25);
    for kind in [ToolKind::Shovel, ToolKind::Axe, ToolKind::Pickaxe] {
        assert_eq!(
            crate::mining::break_time(POWDER_SNOW, Some(Item::tool(kind, Tier::Diamond))),
            crate::mining::break_time(POWDER_SNOW, None)
        );
    }
    assert_eq!(POWDER_SNOW.light_opacity(), 1);
    // `propagatesSkylightDown`: a dry scaffold's outline is not a full block.
    assert_eq!(SCAFFOLDING.light_opacity(), 0);
    assert!(!POWDER_SNOW.is_opaque());
    let mut w = world();
    w.set_block(P, Block::WATER);
    w.set_block(P, state(0, false, false));
    assert_eq!(w.get_block(P), Some(state(0, false, true)));
    assert_eq!(crate::survival_items::bucket::pick_up(&mut w, P), Some((Item::WATER_BUCKET, Block::WATER)));
    w.set_block(P, state(0, false, true));
    w.set_block(P, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::WATER));
}
#[test]
fn shared_session_places_scaffolding_and_bucket_can_place_and_pick_up() {
    let mut w = world();
    w.set_block(P, state(0, false, false));
    let mut s = PlayerSession::new(PlayerId::HOST, P.as_dvec3() + DVec3::new(-2.0, 0.0, 0.5));
    s.mode = GameMode::Survival;
    s.player.pitch = -0.45;
    s.inventory.slots[0] = Some(Stack::new(Item::SCAFFOLDING, 2));
    // Side click extends the scaffold upward, including into the player's box (placement context has no collision).
    let out = s.place_held(0, &mut w, &[]).unwrap();
    assert!(matches!(out,crate::session::placement::Outcome::Placed{cell,..} if cell==P+IVec3::Y));
    assert_eq!(s.inventory.get(0).unwrap().count, 1);
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::POWDER_SNOW_BUCKET, 1));
    let mut p = Player::new(P.as_dvec3() + DVec3::new(-2.0, 0.0, 0.5));
    p.pitch = -0.45;
    w.set_block(P, Block::STONE);
    let mut entities = Entities::new(7);
    let e = crate::survival_items::bucket::use_bucket(&p, &mut inv, 0, GameMode::Survival, &mut w, &mut entities, 4.5)
        .unwrap();
    let crate::survival_items::bucket::Effect::Poured { at, .. } = e else { panic!("not poured") };
    assert_eq!(w.get_block(at), Some(POWDER_SNOW));
    assert_eq!(inv.get(0).unwrap().item, Item::BUCKET);
    assert_eq!(crate::survival_items::bucket::pick_up(&mut w, at), Some((Item::POWDER_SNOW_BUCKET, POWDER_SNOW)));
    assert_eq!(w.get_block(at), Some(Block::AIR));
}

#[test]
fn scaffold_states_powder_bucket_and_pending_tick_survive_storage_and_unload() {
    use crate::world::{
        chunk::chunk_of,
        storage::{LevelInfo, Storage},
    };
    let mut w = world();
    let q = P + IVec3::Y * 2;
    w.set_block(P, state(0, false, true));
    w.set_block(P + IVec3::X, POWDER_SNOW);
    w.set_block(q, SCAFFOLDING);
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::POWDER_SNOW_BUCKET, 1));
    inv.slots[1] = Some(Stack::new(Item::SCAFFOLDING, 64));
    let dir = std::env::temp_dir().join(format!("voxelcraft-scaffolding-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let props = [
        ("inventory".to_string(), inv.serialize()),
        ("ticks_frozen".to_string(), "139".to_string()),
        ("redstone".to_string(), w.redstone_to_string()),
    ]
    .into();
    storage.save(&LevelInfo { seed: 7, player: None, props }, &w.modified_chunks()).unwrap();
    let level = storage.load_level().unwrap();
    let chunks = storage.load_chunks().unwrap();
    let data = chunks[&chunk_of(P)].clone();
    let mut restored = world();
    restored.insert_chunk(chunk_of(P), data.clone(), true);
    restored.load_redstone(&level.props["redstone"]);
    assert_eq!(restored.get_block(P), Some(state(0, false, true)));
    assert_eq!(restored.get_block(P + IVec3::X), Some(POWDER_SNOW));
    assert_eq!(level.props["ticks_frozen"], "139");
    assert_eq!(Inventory::deserialize(&level.props["inventory"]).unwrap().serialize(), inv.serialize());
    restored.remove_chunk(chunk_of(P));
    ticks(&mut restored, 5);
    assert!(restored.falling.is_empty());
    restored.insert_chunk(chunk_of(P), data, true);
    ticks(&mut restored, 1);
    assert_eq!(restored.falling.len(), 1, "pending unsupported scaffold survives unload and reload");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn burning_player_melts_powder_and_dispensers_place_and_pick_up_solid_bucket() {
    let mut w = world();
    w.set_block(P, POWDER_SNOW);
    let p = Player::new(P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    w.powder_snow_fire(&p, true);
    assert_eq!(w.get_block(P), Some(Block::AIR));
    // The dispenser's empty bucket shares BucketPickup, and its solid bucket
    // can only place into air (SolidBucketItem.emptyContents).
    assert_eq!(crate::survival_items::bucket::pick_up(&mut w, P), None);
    w.set_block(P, POWDER_SNOW);
    assert_eq!(crate::survival_items::bucket::pick_up(&mut w, P), Some((Item::POWDER_SNOW_BUCKET, POWDER_SNOW)));
    let dispenser = P + IVec3::X * 3;
    let front = dispenser + IVec3::X;
    w.set_block(dispenser, crate::world::redstone_blocks::dispenser(2, false));
    w.chest_mut(dispenser).unwrap().slots[0] = Some(Stack::new(Item::POWDER_SNOW_BUCKET, 1));
    w.set_block(dispenser - IVec3::X, crate::world::redstone_blocks::REDSTONE_BLOCK);
    ticks(&mut w, 5);
    assert_eq!(w.get_block(front), Some(POWDER_SNOW));
    assert_eq!(w.chest(dispenser).unwrap().slots[0].unwrap().item, Item::BUCKET);
    w.set_block(dispenser - IVec3::X, Block::AIR);
    ticks(&mut w, 1);
    w.set_block(dispenser - IVec3::X, crate::world::redstone_blocks::REDSTONE_BLOCK);
    ticks(&mut w, 5);
    assert_eq!(w.get_block(front), Some(Block::AIR));
    assert_eq!(w.chest(dispenser).unwrap().slots[0].unwrap().item, Item::POWDER_SNOW_BUCKET);
    assert!(super::super::mount_states::face_sturdy(state(0, false, false), IVec3::Y));
    assert!(!super::super::mount_states::face_sturdy(POWDER_SNOW, IVec3::Y));
}

#[test]
#[ignore = "release microbenchmark, run explicitly for the verification record"]
fn scaffolding_powder_snow_tick_cost() {
    use crate::player::MoveInput;
    use std::time::Instant;
    let mut w = world();
    for x in 2..14 {
        for z in 2..14 {
            w.set_block(IVec3::new(x, P.y - 1, z), Block::STONE);
            for y in 0..8 {
                w.set_block(IVec3::new(x, P.y + y, z), state(0, false, false));
            }
        }
    }
    ticks(&mut w, 2);
    let started = Instant::now();
    ticks(&mut w, 100_000);
    let idle = started.elapsed();
    let mut p = Player::new(P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    let started = Instant::now();
    for _ in 0..10_000 {
        p.pos = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        p.tick(MoveInput { jump: true, ..Default::default() }, &w);
    }
    let climb = started.elapsed();
    w.set_block(P, POWDER_SNOW);
    let started = Instant::now();
    for _ in 0..10_000 {
        p.pos = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        p.tick(MoveInput::default(), &w);
    }
    eprintln!(
        "scaffolding_powder_snow: 1152 settled scaffold blocks; idle {:?}/100000 ticks; climbing {:?}/10000 player ticks; powder {:?}/10000 player ticks",
        idle,
        climb,
        started.elapsed()
    );
}

/// `Blocks.SCAFFOLDING` has `PushReaction.POPPED`: a piston destroys it with
/// its drop, and the unsupported block above then collapses.
#[test]
fn pistons_pop_scaffolding_and_the_column_above_collapses() {
    use crate::world::redstone_blocks as r;
    let mut w = world();
    let s = P + IVec3::X;
    w.set_block(s - IVec3::Y, Block::STONE);
    w.set_block(s, state(0, false, false));
    w.set_block(s + IVec3::Y, state(0, false, false));
    ticks(&mut w, 2);
    w.set_block(P, r::piston(2, false, false));
    w.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 5);
    assert!(!w.get_block(s).is_some_and(is_scaffolding), "popped, not pushed");
    assert!(!w.get_block(s + IVec3::X).is_some_and(is_scaffolding));
    assert_eq!(w.get_block(s + IVec3::Y), Some(Block::AIR), "the block above lost its support");
    assert_eq!(
        w.drops.iter().filter(|(_, st)| st.item == Item::SCAFFOLDING).map(|(_, st)| st.count as usize).sum::<usize>(),
        2
    );
}

/// `LivingEntity.tryAddFrost`: `-0.05F * percentFrozen` is an `ADD_VALUE`
/// modifier, so the sprint multiplier applies to the reduced base.
#[test]
fn frost_is_added_to_the_base_speed_before_the_sprint_multiplier() {
    let mut p = Player::new(DVec3::ZERO);
    p.powder_snow_equipment(&[None; 4], 70);
    p.sprinting = true;
    let expected = ((0.10000000149011612 + f64::from(-0.05_f32 * 0.5)) * (1.0 + f64::from(0.3_f32))) as f32;
    assert_eq!(p.movement_speed(), expected);
}
