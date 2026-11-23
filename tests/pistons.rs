//! v0.17 Track B (AUT-02): pistons against Java 26.3. The fixture
//! `tests/fixtures/java/26.3/pistons.json` is extracted from the pinned jar by
//! `tools/extract_java_piston_fixtures.py`; the traces below follow
//! `PistonStructureResolver`, `PistonMovingBlockEntity` and `PistonBaseBlock`.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::{
    color::DyeColor,
    java_registry,
    physics::{self, Shape},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        piston::{self, PistonClass, PushBody, Resolver},
        redstone_blocks as r,
        stone_blocks::SLIME_BLOCK,
        terrain::Generator,
    },
};

const SOUTH: u8 = 0;
const EAST: u8 = 2;
const UP: u8 = 4;
/// A corner of the loaded test chunk, high above the generated terrain.
const O: IVec3 = IVec3::new(8, 140, 8);

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/pistons.json")).unwrap()
}

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    for c in [IVec3::new(0, 4, 0), IVec3::new(0, 3, 0)] {
        w.receive_chunk(c, Arc::new(ChunkData::Uniform(Block::AIR)));
    }
    w
}

fn put(w: &mut World, p: IVec3, b: Block) {
    assert!(w.set_block(p, b), "{p} not loaded");
}

fn ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.tick_redstone();
    }
}

fn resolve(w: &World, piston: IVec3, facing: u8, extending: bool) -> Option<(Vec<IVec3>, Vec<IVec3>)> {
    let mut resolver = Resolver::new(piston, r::direction(facing), extending);
    resolver.resolve(w).then(|| (resolver.to_push.clone(), resolver.to_destroy.clone()))
}

#[test]
fn fixture_constants_and_adhesion_match_the_engine() {
    let f = fixture();
    let c = &f["constants"];
    assert_eq!(c["max_push"].as_u64().unwrap() as usize, piston::MAX_PUSH);
    assert_eq!(c["ticks_to_extend"].as_f64().unwrap(), f64::from(1.0 / piston::STEP));
    assert_eq!(c["progress_step"].as_f64().unwrap(), f64::from(piston::STEP));
    assert_eq!(c["push_offset"].as_f64().unwrap(), piston::PUSH_OFFSET);
    assert_eq!(c["tick_movement"].as_f64().unwrap(), piston::TICK_MOVEMENT);
    assert_eq!(c["entity_axis_limit"].as_f64().unwrap(), piston::TICK_MOVEMENT);
    assert_eq!(c["sticky_top"].as_f64().unwrap(), piston::STICKY_TOP);
    assert_eq!(c["short_head_progress"].as_f64().unwrap(), f64::from(piston::SHORT_HEAD));
    assert_eq!(c["spit_progress"].as_f64().unwrap(), f64::from(piston::SPIT_PROGRESS));
    let a = &f["adhesion"];
    let block = |name: &Value| java_registry::block(name.as_str().unwrap(), "").unwrap();
    let sticky: Vec<Block> = a["sticky"].as_array().unwrap().iter().map(block).collect();
    assert_eq!(sticky, [SLIME_BLOCK, Block::HONEY_BLOCK]);
    assert!(sticky.iter().all(|&b| piston::is_sticky(b)));
    for pair in a["never_stick"].as_array().unwrap() {
        let (x, y) = (block(&pair[0]), block(&pair[1]));
        assert!(!piston::stick_together(x, y) && !piston::stick_together(y, x));
    }
    assert!(piston::stick_together(SLIME_BLOCK, SLIME_BLOCK) && piston::stick_together(Block::STONE, SLIME_BLOCK));
    assert!(piston::stick_together(Block::HONEY_BLOCK, Block::HONEY_BLOCK));
    assert_eq!(block(&a["bounces_entities"]), SLIME_BLOCK);
    assert_eq!(block(&a["carries_entities"]), Block::HONEY_BLOCK);
    assert_eq!(java_registry::block("moving_piston", "Moving Piston"), Some(r::MOVING));
}

#[test]
fn every_registered_block_reacts_to_pistons_as_in_java() {
    let f = fixture();
    let names: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let listed = |key: &str, id: &str| f[key].as_array().unwrap().iter().any(|v| v.as_str() == Some(id));
    let mut checked = 0;
    let mut wrong = Vec::new();
    for (id, reaction) in f["block_push_reactions"].as_object().unwrap() {
        // Air is skipped before any reaction is read; pistons depend on their state.
        if matches!(id.as_str(), "air" | "cave_air" | "void_air" | "piston" | "sticky_piston") {
            continue;
        }
        let english = names["blocks"][id].as_str().unwrap_or("");
        let Some(b) = java_registry::block(id, english) else { continue };
        let destroy = if listed("unbreakable", id) { -1.0 } else { 1.0 };
        let java = PistonClass::of_java(reaction.as_str().unwrap(), listed("block_entities", id), destroy);
        let ours = piston::piston_class(b);
        checked += 1;
        if java != ours {
            wrong.push(format!("{id}: java {} ours {}", java.name(), ours.name()));
        }
    }
    println!("{checked} registered blocks compared");
    assert!(checked > 600, "only {checked} blocks resolved");
    assert!(wrong.is_empty(), "{} blocks differ:\n{}", wrong.len(), wrong.join("\n"));
    assert_eq!(piston::piston_class(r::piston(EAST, true, false)), PistonClass::Move);
    assert_eq!(piston::piston_class(r::piston(EAST, true, true)), PistonClass::Immovable);
}

#[test]
fn flying_machine_core_drags_its_second_piston_and_observer_in_java_order() {
    let mut w = world();
    let p = O;
    put(&mut w, p, r::piston(EAST, true, false));
    put(&mut w, p + IVec3::X, SLIME_BLOCK);
    put(&mut w, p + IVec3::new(2, 0, 0), r::observer(EAST, false));
    put(&mut w, p + IVec3::new(1, 1, 0), SLIME_BLOCK);
    put(&mut w, p + IVec3::new(2, 1, 0), r::piston(3, true, false));
    // The base piston itself is never dragged, though it touches the slime.
    let (push, destroy) = resolve(&w, p, EAST, true).unwrap();
    assert_eq!(
        push,
        [IVec3::new(1, 0, 0), IVec3::new(2, 0, 0), IVec3::new(1, 1, 0), IVec3::new(2, 1, 0)].map(|d| p + d)
    );
    assert!(destroy.is_empty());
    // An immovable block under the slime is left behind rather than blocking.
    put(&mut w, p + IVec3::new(1, -1, 0), Block::OBSIDIAN);
    assert_eq!(resolve(&w, p, EAST, true).unwrap().0.len(), 4);
    // In front of the moving line it blocks the whole machine.
    put(&mut w, p + IVec3::new(3, 1, 0), Block::OBSIDIAN);
    assert!(resolve(&w, p, EAST, true).is_none());
}

#[test]
fn honey_and_slime_side_by_side_do_not_stick() {
    let mut w = world();
    let p = O;
    put(&mut w, p, r::piston(EAST, false, false));
    put(&mut w, p + IVec3::X, SLIME_BLOCK);
    put(&mut w, p + IVec3::new(1, 0, -1), Block::STONE);
    put(&mut w, p + IVec3::new(1, 0, 1), Block::HONEY_BLOCK);
    put(&mut w, p + IVec3::new(1, 0, 2), Block::STONE);
    let (push, _) = resolve(&w, p, EAST, true).unwrap();
    assert_eq!(push, [p + IVec3::X, p + IVec3::new(1, 0, -1)]);
    put(&mut w, p + IVec3::new(0, 0, -2), r::REDSTONE_BLOCK);
    put(&mut w, p - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 3);
    assert_eq!(w.get_block(p + IVec3::new(2, 0, 0)), Some(SLIME_BLOCK));
    assert_eq!(w.get_block(p + IVec3::new(2, 0, -1)), Some(Block::STONE));
    assert_eq!(w.get_block(p + IVec3::new(1, 0, 1)), Some(Block::HONEY_BLOCK), "honey stays");
    assert_eq!(w.get_block(p + IVec3::new(1, 0, 2)), Some(Block::STONE));
    // Honey drags honey, and sticky pistons cannot pull glazed terracotta.
    let mut w = world();
    put(&mut w, p, r::piston(EAST, true, false));
    put(&mut w, p + IVec3::X, Block::HONEY_BLOCK);
    put(&mut w, p + IVec3::new(1, 1, 0), Block::HONEY_BLOCK);
    put(&mut w, p + IVec3::new(1, 0, 1), Block::glazed(DyeColor::Red));
    let (push, _) = resolve(&w, p, EAST, true).unwrap();
    assert_eq!(push, [p + IVec3::X, p + IVec3::new(1, 1, 0)], "glazed terracotta is push-only");
}

#[test]
fn twelve_blocks_move_but_thirteen_fail_across_the_whole_assembly() {
    let mut w = world();
    let p = O;
    put(&mut w, p, r::piston(EAST, false, false));
    for i in 1..=12 {
        put(&mut w, p + IVec3::X * i, Block::STONE);
    }
    assert_eq!(resolve(&w, p, EAST, true).unwrap().0.len(), 12);
    put(&mut w, p + IVec3::X * 13, Block::STONE);
    assert!(resolve(&w, p, EAST, true).is_none());
    // Slime with a dragged row above: 1 + 6 below and 1 + 4 above is 12.
    let mut w = world();
    put(&mut w, p, r::piston(EAST, false, false));
    put(&mut w, p + IVec3::X, SLIME_BLOCK);
    put(&mut w, p + IVec3::new(1, 1, 0), SLIME_BLOCK);
    for i in 2..=7 {
        put(&mut w, p + IVec3::X * i, Block::STONE);
    }
    for i in 2..=5 {
        put(&mut w, p + IVec3::new(i, 1, 0), Block::STONE);
    }
    assert_eq!(resolve(&w, p, EAST, true).unwrap().0.len(), 12);
    put(&mut w, p + IVec3::new(6, 1, 0), Block::STONE);
    assert!(resolve(&w, p, EAST, true).is_none(), "13 over the assembly");
    // The failure is checked before any edit: power changes nothing.
    put(&mut w, p - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 4);
    assert_eq!(w.get_block(p), Some(r::piston(EAST, false, false)));
    assert_eq!(w.get_block(p + IVec3::X), Some(SLIME_BLOCK));
}

#[test]
fn destroyable_blocks_in_the_path_break_and_drop() {
    let mut w = world();
    let p = O;
    put(&mut w, p, r::piston(EAST, false, false));
    put(&mut w, p + IVec3::X, Block::STONE);
    put(&mut w, p + IVec3::X * 2, Block::COBWEB);
    put(&mut w, p + IVec3::X * 3, Block::STONE);
    let (push, destroy) = resolve(&w, p, EAST, true).unwrap();
    assert_eq!(push, [p + IVec3::X]);
    assert_eq!(destroy, [p + IVec3::X * 2]);
    // A destroyable block right in front is broken on its own.
    let mut w2 = world();
    put(&mut w2, p, r::piston(EAST, false, false));
    put(&mut w2, p + IVec3::X, Block::COBWEB);
    assert_eq!(resolve(&w2, p, EAST, true).unwrap(), (vec![], vec![p + IVec3::X]));
    w.drops.clear();
    put(&mut w, p - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 3);
    assert_eq!(w.get_block(p + IVec3::X * 2), Some(Block::STONE));
    assert_eq!(w.get_block(p + IVec3::X * 3), Some(Block::STONE), "the far stone stays");
    assert!(!w.drops.is_empty(), "the cobweb dropped");
}

/// A standing body for traces: a player-sized box with no gravity of its
/// own, so only pistons move it.
struct Body {
    pos: DVec3,
    vel: DVec3,
    on_ground: bool,
}

impl Body {
    fn push(&mut self, w: &World) {
        piston::push_body(
            w,
            PushBody {
                pos: &mut self.pos,
                vel: &mut self.vel,
                vel_per_tick: 20.0,
                shape: Shape::new(0.3, 1.8),
                on_ground: Some(self.on_ground),
                noclip: false,
            },
        );
    }
}

/// A piston facing `facing` at `O`, `front` in front of it, and a body
/// standing at `at`; then the piston is powered and `n` ticks traced.
fn trace(facing: u8, front: Block, at: DVec3, n: usize) -> (World, Vec<DVec3>, Vec<DVec3>) {
    let mut w = world();
    put(&mut w, O, r::piston(facing, false, false));
    put(&mut w, O + r::direction(facing), front);
    let mut body = Body { pos: at, vel: DVec3::ZERO, on_ground: true };
    put(&mut w, O - IVec3::new(0, 0, 1), r::REDSTONE_BLOCK);
    let (mut pos, mut vel) = (Vec::new(), Vec::new());
    for _ in 0..n {
        w.tick_redstone();
        body.push(&w);
        pos.push(body.pos);
        vel.push(body.vel);
    }
    (w, pos, vel)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn rising_blocks_push_entities_and_slime_launches_them() {
    // `moveCollidedEntities`: 0.5 + 0.01, then up to the block's new top + 0.01.
    let top = O.as_dvec3() + DVec3::new(0.5, 2.0, 0.5);
    let (w, pos, vel) = trace(UP, Block::STONE, top, 4);
    let ys: Vec<f64> = pos.iter().map(|p| p.y - top.y).collect();
    assert!(close(ys[0], 0.51) && close(ys[1], 1.01) && close(ys[2], 1.01) && close(ys[3], 1.01), "{ys:?}");
    assert!(vel.iter().all(|v| *v == DVec3::ZERO), "stone does not launch");
    assert_eq!(w.get_block(O + IVec3::Y * 2), Some(Block::STONE));
    // Slime sets the vertical velocity to one block per tick, every push tick.
    let (_, pos, vel) = trace(UP, SLIME_BLOCK, top, 3);
    assert!(close(pos[0].y - top.y, 0.51), "{pos:?}");
    assert_eq!(vel[0].y, 20.0);
    assert_eq!(vel[1].y, 20.0);
}

#[test]
fn honey_drags_entities_standing_on_it_without_the_push_offset() {
    let top = O.as_dvec3() + DVec3::new(1.5, 15.0 / 16.0, 0.5);
    let (w, pos, _) = trace(EAST, Block::HONEY_BLOCK, top, 3);
    let xs: Vec<f64> = pos.iter().map(|p| p.x - top.x).collect();
    assert!(close(xs[0], 0.5) && close(xs[1], 1.0) && close(xs[2], 1.0), "{xs:?}");
    assert!(pos.iter().all(|p| p.y == top.y));
    assert_eq!(w.get_block(O + IVec3::X * 2), Some(Block::HONEY_BLOCK));
    // A stone carries nobody.
    let (_, pos, _) = trace(EAST, Block::STONE, top + DVec3::new(0.0, 1.0 / 16.0, 0.0), 3);
    assert!(pos.iter().all(|p| p.x == top.x), "{pos:?}");
}

#[test]
fn entities_pushed_into_blocks_stop_against_them() {
    // A body in front of the pushed stone, a wall just past it: Java's
    // `Entity.move(PISTON)` collides, so the body ends against the wall.
    let at = O.as_dvec3() + DVec3::new(2.5, 0.0, 0.5);
    let mut w = world();
    put(&mut w, O + IVec3::new(3, 0, 0), Block::STONE);
    put(&mut w, O + IVec3::new(3, 1, 0), Block::STONE);
    put(&mut w, O, r::piston(EAST, false, false));
    put(&mut w, O + IVec3::X, Block::STONE);
    let mut body = Body { pos: at, vel: DVec3::ZERO, on_ground: true };
    put(&mut w, O - IVec3::Z, r::REDSTONE_BLOCK);
    let mut xs = Vec::new();
    for _ in 0..3 {
        w.tick_redstone();
        body.push(&w);
        xs.push(body.pos.x - at.x);
    }
    // 0.2 to the wall (2.5 + 0.3 = 2.8 against 3.0), then nothing more.
    assert!(xs.iter().all(|&x| close(x, 0.2)), "{xs:?}");
    // The per-axis limit holds while two moving blocks push at once.
    assert!(xs[0] <= piston::TICK_MOVEMENT);
}

#[test]
fn moving_blocks_collide_with_their_shape_offset_by_progress() {
    let mut w = world();
    put(&mut w, O, r::piston(EAST, false, false));
    put(&mut w, O + IVec3::X, Block::STONE);
    put(&mut w, O - IVec3::Z, r::REDSTONE_BLOCK);
    w.tick_redstone();
    // After one tick the stone moving into x + 2 sits half-way, crossing cells.
    let to = O + IVec3::X * 2;
    let m = w.moving_piston(to).unwrap();
    assert_eq!((m.moved, m.facing, m.extending, m.source, m.progress), (Block::STONE, EAST, true, false, 0.5));
    assert_eq!(w.moving_piston_state(to), Some((EAST, false)));
    assert_eq!(w.moving_piston_state(O + IVec3::X), Some((EAST, false)), "a plain piston's arm");
    let mut out = Vec::new();
    let lo = O.as_dvec3() + DVec3::new(1.2, 0.2, 0.2);
    physics::colliders(&w, lo, lo + DVec3::splat(0.4), &mut out);
    let stone = out.iter().find(|c| c.cell == to).expect("the moving stone reaches back into x + 1");
    assert!(close(stone.min.x, f64::from(O.x) + 1.5) && close(stone.max.x, f64::from(O.x) + 2.5));
}

#[test]
fn a_one_tick_pulse_makes_a_sticky_piston_spit_its_block() {
    for (pulse, pulled) in [(1, false), (3, true)] {
        let mut w = world();
        put(&mut w, O, r::piston(EAST, true, false));
        put(&mut w, O + IVec3::X, Block::IRON_BLOCK);
        put(&mut w, O - IVec3::Z, r::REDSTONE_BLOCK);
        ticks(&mut w, pulse);
        w.set_block(O - IVec3::Z, Block::AIR);
        ticks(&mut w, 5);
        assert_eq!(w.get_block(O), Some(r::piston(EAST, true, false)), "pulse {pulse}");
        let (near, far) = (w.get_block(O + IVec3::X), w.get_block(O + IVec3::X * 2));
        if pulled {
            assert_eq!((near, far), (Some(Block::IRON_BLOCK), Some(Block::AIR)), "pulse {pulse}: pulled back");
        } else {
            assert_eq!((near, far), (Some(Block::AIR), Some(Block::IRON_BLOCK)), "pulse {pulse}: dropped");
        }
    }
}

#[test]
fn observers_see_cells_turn_into_moving_pistons_and_land_unpowered() {
    let mut w = world();
    put(&mut w, O, r::piston(EAST, false, false));
    put(&mut w, O + IVec3::X, Block::STONE);
    // Watching the cell in front of the piston from above.
    put(&mut w, O + IVec3::new(1, 1, 0), r::observer(5, false));
    put(&mut w, O - IVec3::Z, r::REDSTONE_BLOCK);
    let mut lit = Vec::new();
    for _ in 0..4 {
        w.tick_redstone();
        lit.push(w.get_block(O + IVec3::new(1, 1, 0)) == Some(r::observer(5, true)));
    }
    assert!(lit.contains(&true), "{lit:?}");
    // `ObserverBlock.onPlace`: a powered observer arriving with no pending tick turns off.
    let mut w = world();
    put(&mut w, O, r::observer(SOUTH, true));
    assert_eq!(w.get_block(O), Some(r::observer(SOUTH, false)));
}

#[test]
fn saves_taken_mid_motion_resume_exactly_and_v1_records_migrate() {
    let mut w = world();
    put(&mut w, O, r::piston(EAST, true, false));
    put(&mut w, O + IVec3::X, Block::GOLD_BLOCK);
    put(&mut w, O - IVec3::Z, r::REDSTONE_BLOCK);
    w.tick_redstone();
    let saved = w.automation_to_string();
    assert!(saved.starts_with("v2|M,"), "{saved}");
    let mut copy = world();
    for x in -1..=3 {
        for z in -1..=1 {
            let p = O + IVec3::new(x, 0, z);
            copy.set_block(p, w.get_block(p).unwrap());
        }
    }
    copy.load_automation(&saved);
    for p in [O + IVec3::X, O + IVec3::X * 2] {
        assert_eq!(copy.moving_piston(p).map(|m| (m.moved, m.progress, m.progress_o)), {
            w.moving_piston(p).map(|m| (m.moved, m.progress, m.progress_o))
        });
    }
    for _ in 0..3 {
        w.tick_redstone();
        copy.tick_redstone();
        for x in 0..=3 {
            let p = O + IVec3::X * x;
            assert_eq!(copy.get_block(p), w.get_block(p), "x + {x}");
        }
    }
    assert_eq!(copy.get_block(O + IVec3::X * 2), Some(Block::GOLD_BLOCK));
    // A v1 record (`to, block, from, base, age, install`) becomes a block entity.
    let mut old = world();
    put(&mut old, O, r::piston(EAST, true, true));
    let (to, from) = (O + IVec3::X * 2, O + IVec3::X);
    old.set_block(to, r::MOVING);
    old.load_automation(&format!(
        "v1|m,{},{},{},{},{},{},{},{},{},{},1,1",
        to.x,
        to.y,
        to.z,
        Block::GOLD_BLOCK.0,
        from.x,
        from.y,
        from.z,
        O.x,
        O.y,
        O.z
    ));
    let m = old.moving_piston(to).unwrap();
    assert_eq!((m.moved, m.facing, m.extending, m.source, m.progress), (Block::GOLD_BLOCK, EAST, true, false, 0.5));
    ticks(&mut old, 2);
    assert_eq!(old.get_block(to), Some(Block::GOLD_BLOCK));
}

#[test]
fn breaking_a_moving_cell_drops_its_moved_block() {
    let mut w = world();
    put(&mut w, O, r::piston(EAST, false, false));
    put(&mut w, O + IVec3::X, Block::GOLD_BLOCK);
    put(&mut w, O - IVec3::Z, r::REDSTONE_BLOCK);
    w.tick_redstone();
    let to = O + IVec3::X * 2;
    assert_eq!(w.get_block(to), Some(r::MOVING));
    w.drops.clear();
    w.detonate(&[to], 4.0, true, false, false);
    assert_eq!(w.get_block(to), Some(Block::AIR));
    assert!(w.moving_piston(to).is_none());
    assert!(w.drops.iter().any(|(_, s)| s.item == voxelcraft::item::Item::from(Block::GOLD_BLOCK)), "{:?}", w.drops);
    ticks(&mut w, 3);
    assert_eq!(w.get_block(to), Some(Block::AIR), "nothing lands later");
}
