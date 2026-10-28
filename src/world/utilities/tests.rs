//! Registry, rule and world tests for the v0.12 utility blocks.
use super::*;
use crate::world::{World, chunk::ChunkData, overworld_blocks as ob, terrain::Generator};
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(5)), Default::default(), 2);
    for y in w.generator.dimension.chunk_rows() {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    w
}

fn registered() -> impl Iterator<Item = Block> {
    (FIRST..END).filter(|&id| registry(id).is_some()).map(Block)
}

#[test]
fn every_state_is_registered_once_with_rules() {
    let states: Vec<Block> = registered().collect();
    // 8 cauldrons, 34 pots, a lodestone, 5 anchors, 64 campfires, 80 wall
    // fans, 8 ender chests, 24 trapped chests (single and double halves),
    // 16 decorated pots, 30 dripleaf states (north/no tilt is the existing
    // big dripleaf) and the plain chest's 16 double halves.
    assert_eq!(states.len(), 8 + 34 + 1 + 5 + 64 + 80 + 8 + 24 + 16 + 30 + 16);
    for b in states {
        assert_ne!(b.kind(), RenderKind::Invisible, "{}", b.0);
        assert!(b.hardness().is_finite(), "{}", b.name());
        if dripleaf_of(b).is_none() {
            assert!(mining(b).is_some(), "{} mines", b.name());
            assert!(Block::from_name(b.name()).is_some(), "{} looks itself up", b.name());
        }
        if let Some((other, wet)) = water_pair(b.0) {
            assert_eq!(water_pair(other), Some((b.0, !wet)), "{}", b.0);
            assert_eq!(b.is_waterlogged(), wet, "{}", b.0);
            assert_eq!(b.with_water(!wet), Block(other));
        }
        assert_ne!(b.base().kind(), RenderKind::Invisible, "{}", b.0);
    }
    for id in palette_ids() {
        let b = Block(id);
        assert_eq!(b.base(), b, "palette holds default states: {}", b.name());
        assert!(Block::creative_palette().any(|p| p == b));
        assert_eq!(Item::from_block(b).block(), Some(b));
    }
    const { assert!(TEX_END as u32 <= crate::world::block::tex::COUNT) };
}

#[test]
fn state_helpers_round_trip() {
    for f in FACINGS {
        for wet in [false, true] {
            assert_eq!(ender_chest_of(ender_chest(f, wet)), Some((f, wet)));
            assert_eq!(trapped_chest_of(trapped_chest(f, wet)), Some((f, wet)));
            for cracked in [false, true] {
                assert_eq!(decorated_pot_of(decorated_pot(f, cracked, wet)), Some((f, cracked, wet)));
            }
            for colour in 0..5 {
                for dead in [false, true] {
                    assert_eq!(wall_fan_of(wall_fan(colour, dead, f, wet)), Some((colour, dead, f, wet)));
                }
            }
            for tilt in Tilt::ALL {
                assert_eq!(dripleaf_of(big_dripleaf(f, tilt, wet)), Some((f, tilt, wet)));
            }
            for soul in [false, true] {
                for lit in [false, true] {
                    for signal in [false, true] {
                        let c = Campfire { soul, lit, signal, wet, facing: f };
                        assert_eq!(campfire_of(campfire(c)), Some(c));
                    }
                }
            }
        }
    }
    assert_eq!(big_dripleaf(Facing::North, Tilt::None, false), ob::BIG_DRIPLEAF);
    assert_eq!(big_dripleaf(Facing::North, Tilt::None, true), ob::BIG_DRIPLEAF.with_water(true));
    for (i, (plant, _)) in POTTED.iter().enumerate() {
        let pot = potted(*plant).unwrap();
        assert_eq!(potted_plant(pot), Some(*plant), "{i}");
        assert!(is_pot(pot));
    }
    for level in 1..=3 {
        assert_eq!(cauldron_of(water_cauldron(level)), Some(Contents::Water(level)));
        assert_eq!(cauldron_of(powder_snow_cauldron(level)), Some(Contents::PowderSnow(level)));
    }
    assert_eq!(cauldron(Contents::Water(0)), CAULDRON);
    // Each kind's default is its first id, so the name finds it.
    assert_eq!(Block::from_name("campfire"), Some(CAMPFIRE));
    assert_eq!(Block::from_name("soul_campfire"), Some(SOUL_CAMPFIRE));
    assert_eq!(Block::from_name("respawn anchor"), Some(RESPAWN_ANCHOR));
    assert_eq!(Block::from_name("potted azalea"), potted(ob::AZALEA));
}

#[test]
fn java_light_hardness_tools_and_drops() {
    use ToolKind::*;
    let lit = CAMPFIRE;
    let unlit = campfire(Campfire { lit: false, ..campfire_of(CAMPFIRE).unwrap() });
    assert_eq!((lit.emission(), unlit.emission(), SOUL_CAMPFIRE.emission()), (15, 0, 10));
    assert_eq!(LAVA_CAULDRON.emission(), 15);
    assert_eq!(ENDER_CHEST.emission(), 7);
    assert_eq!((0..=4).map(|c| respawn_anchor(c).emission()).collect::<Vec<_>>(), [0, 3, 7, 11, 15]);
    assert_eq!((CAULDRON.hardness(), CAULDRON.best_tool(), CAULDRON.harvest_level()), (2.0, Some(Pickaxe), Some(0)));
    assert_eq!((RESPAWN_ANCHOR.hardness(), RESPAWN_ANCHOR.harvest_level()), (50.0, Some(3)));
    assert_eq!((CAMPFIRE.best_tool(), CAMPFIRE.harvest_level()), (Some(Axe), None));
    // 26.3's ender chest has no `requiresCorrectToolForDrops`.
    assert_eq!((ENDER_CHEST.hardness(), ENDER_CHEST.harvest_level()), (22.5, None));
    assert_eq!((TRAPPED_CHEST.hardness(), TRAPPED_CHEST.best_tool()), (2.5, Some(Axe)));
    assert_eq!((LODESTONE.hardness(), LODESTONE.harvest_level()), (3.5, Some(0)));
    assert_eq!(FLOWER_POT.hardness(), 0.0);
    assert_eq!(water_cauldron(2).drop(), Some(CAULDRON.into()));
    assert_eq!(respawn_anchor(3).drop(), Some(RESPAWN_ANCHOR.into()));
    assert_eq!(CAMPFIRE.drop(), Some(Item::CHARCOAL));
    assert_eq!(extra_drops(CAMPFIRE).map(|s| (s.item, s.count)), Some((Item::CHARCOAL, 1)));
    assert_eq!(SOUL_CAMPFIRE.drop(), Some(crate::world::nether_biome_blocks::SOUL_SOIL.into()));
    assert_eq!(crate::mining::silk_drop(unlit), Some(CAMPFIRE.into()));
    assert_eq!(ENDER_CHEST.drop(), Some(Block::OBSIDIAN.into()));
    assert_eq!(extra_drops(ENDER_CHEST).map(|s| s.count), Some(7));
    let fan = wall_fan(2, false, Facing::East, true);
    assert_eq!(fan.drop(), None);
    assert_eq!(crate::mining::silk_drop(fan), Some(ob::coral(2, 2, false).into()));
    let poppy = potted(Block::POPPY).unwrap();
    assert_eq!(poppy.drop(), Some(FLOWER_POT.into()));
    assert_eq!(extra_drops(poppy).map(|s| s.item), Some(Block::POPPY.into()));
    assert_eq!(crate::session::interact::picked_item(poppy), Block::POPPY.into());
    assert_eq!(crate::session::interact::picked_item(water_cauldron(3)), CAULDRON.into());
    assert_eq!(decorated_pot(Facing::East, true, false).drop(), None);
    assert_eq!(DECORATED_POT.drop(), Some(DECORATED_POT.into()));
}

#[test]
fn shapes_follow_java_outlines() {
    use crate::world::shape;
    let air = |_| Block::AIR;
    let bounds = |b: Block| shape::outline(b, air, Block::AIR).bounds().unwrap();
    assert_eq!(bounds(FLOWER_POT), Box16 { min: [5, 0, 5], max: [11, 6, 11] });
    assert_eq!(bounds(CAMPFIRE), Box16 { min: [0, 0, 0], max: [16, 7, 16] });
    assert_eq!(bounds(DECORATED_POT), Box16 { min: [1, 0, 1], max: [15, 16, 15] });
    assert_eq!(bounds(wall_fan(0, false, Facing::North, true)), Box16 { min: [0, 4, 5], max: [16, 12, 16] });
    assert!(shape::collision(wall_fan(0, false, Facing::North, true), air, Block::AIR).is_empty());
    // The cauldron is hollow: nothing collides above its floor in the middle.
    let cauldron = shape::collision(water_cauldron(3), air, Block::AIR);
    assert!(cauldron.as_slice().iter().all(|b| !(b.min[0] <= 7
        && b.max[0] >= 9
        && b.min[2] <= 7
        && b.max[2] >= 9
        && b.max[1] > 4)));
    // Its water surface: (6 + 3 * level) / 16.
    let visible = shape::shape(water_cauldron(1), air, Block::AIR);
    assert!(visible.as_slice().iter().any(|b| b.max[1] == 9 && b.min[0] == 2));
    let full = big_dripleaf(Facing::East, Tilt::Full, false);
    assert!(shape::collision(full, air, Block::AIR).is_empty());
    assert_eq!(
        shape::collision(big_dripleaf(Facing::East, Tilt::Partial, false), air, Block::AIR).bounds().unwrap().max[1],
        13
    );
}

#[test]
fn cauldron_exchanges_follow_cauldron_interactions() {
    use Contents::*;
    let bottle = Item::potion(crate::potion::Potion::WATER);
    assert_eq!(cauldron::exchange(Empty, Item::WATER_BUCKET), Some((Water(3), Item::BUCKET)));
    assert_eq!(cauldron::exchange(Water(1), Item::LAVA_BUCKET), Some((Lava, Item::BUCKET)));
    assert_eq!(cauldron::exchange(Water(3), Item::BUCKET), Some((Empty, Item::WATER_BUCKET)));
    assert_eq!(cauldron::exchange(Water(2), Item::BUCKET), None, "only a full cauldron fills a bucket");
    assert_eq!(cauldron::exchange(Water(1), Item::GLASS_BOTTLE), Some((Water(0), bottle)));
    assert_eq!(cauldron(Water(0)), CAULDRON, "the last bottle empties it");
    assert_eq!(cauldron::exchange(Empty, bottle), Some((Water(1), Item::GLASS_BOTTLE)));
    assert_eq!(cauldron::exchange(Water(3), bottle), None);
    assert_eq!(cauldron::exchange(Lava, Item::BUCKET), Some((Empty, Item::LAVA_BUCKET)));
    assert_eq!(cauldron::exchange(PowderSnow(3), Item::BUCKET), Some((Empty, Item::POWDER_SNOW_BUCKET)));
    assert_eq!(cauldron::exchange(PowderSnow(2), Item::BUCKET), None);
    assert_eq!(cauldron::precipitation_fill(Empty, false, 0.04), Some(Water(1)));
    assert_eq!(cauldron::precipitation_fill(Empty, false, 0.06), None);
    assert_eq!(cauldron::precipitation_fill(Empty, true, 0.09), Some(PowderSnow(1)));
    assert_eq!(cauldron::precipitation_fill(Water(3), false, 0.0), None);
    assert_eq!(cauldron::drip_fill(Empty, true), Some(Lava));
    assert_eq!(cauldron::drip_fill(Water(2), false), Some(Water(3)));
    assert_eq!(cauldron::drip_fill(Water(1), true), None);
}

#[test]
fn stalactite_drips_fill_a_cauldron_below() {
    let mut w = world();
    let root = IVec3::new(4, 120, 4);
    // Java's arrangement: water on the stone the stalactite hangs from.
    w.set_block(root + IVec3::Y * 3, Block::STONE);
    w.set_block(root + IVec3::Y * 2, Block::WATER);
    w.set_block(root + IVec3::Y, Block::STONE);
    w.set_block(root, ob::pointed_dripstone(true, ob::Thickness::Base));
    w.set_block(root - IVec3::Y, ob::pointed_dripstone(true, ob::Thickness::Tip));
    let pot = root - IVec3::Y * 5;
    w.set_block(pot - IVec3::Y, Block::STONE);
    w.set_block(pot, CAULDRON);
    // Random ticks of the stalactite's top: one in about six transfers.
    for _ in 0..200 {
        w.dripstone_transfer(root, w.get_block(root).unwrap());
        if !w.utilities.drips.is_empty() {
            break;
        }
    }
    assert_eq!(w.utilities.drips.first().map(|&(p, t)| (p, t)), Some((pot, 54)));
    for _ in 0..54 {
        w.tick_utilities();
    }
    assert_eq!(w.get_block(pot), Some(water_cauldron(1)));
}

#[test]
fn campfires_cook_four_items_and_go_out_in_water() {
    let mut w = world();
    let p = IVec3::new(3, 110, 3);
    w.set_block(p - IVec3::Y, Block::STONE);
    w.set_block(p, CAMPFIRE);
    for item in [Item::RAW_BEEF, Item::POTATO, Item::COD, Item::RAW_CHICKEN] {
        assert!(w.campfire_place_food(p, item));
    }
    assert!(!w.campfire_place_food(p, Item::SALMON), "four slots");
    assert!(!w.campfire_place_food(p, Item::STICK));
    for _ in 0..599 {
        w.tick_utilities();
    }
    assert!(w.drops.iter().all(|(_, s)| s.item != Item::STEAK));
    w.tick_utilities();
    let cooked: Vec<Item> = w.drops.iter().map(|(_, s)| s.item).collect();
    for out in [Item::STEAK, Item::BAKED_POTATO, Item::COOKED_COD, Item::COOKED_CHICKEN] {
        assert!(cooked.contains(&out), "{out:?}");
    }
    w.drops.clear();
    // Unlit, progress cools by two a tick; breaking spills the food.
    assert!(w.campfire_place_food(p, Item::RAW_MUTTON));
    for _ in 0..10 {
        w.tick_utilities();
    }
    assert!(w.douse_campfire(p));
    w.tick_utilities();
    assert_eq!(w.campfire_slots(p).unwrap().progress[0], 8);
    assert!(w.light_campfire(p));
    // Water puts it out (`placeLiquid`).
    let lit = w.get_block(p).unwrap();
    w.set_block(p, lit.with_water(true));
    assert!(campfire_of(w.get_block(p).unwrap()).is_some_and(|c| !c.lit && c.wet));
    assert!(!campfire::can_light(w.get_block(p).unwrap()));
    w.set_block(p, Block::AIR);
    assert!(w.drops.iter().any(|(_, s)| s.item == Item::RAW_MUTTON));
    assert!(w.campfire_slots(p).is_none());
}

#[test]
fn campfires_over_hay_send_smoke_signals() {
    let mut w = world();
    let p = IVec3::new(6, 110, 6);
    w.set_block(p - IVec3::Y, Block::HAY_BALE);
    w.set_block(p, CAMPFIRE);
    w.update_campfire_signal(p);
    assert!(campfire_of(w.get_block(p).unwrap()).unwrap().signal);
    w.set_block(p - IVec3::Y, Block::STONE);
    assert!(!campfire_of(w.get_block(p).unwrap()).unwrap().signal);
    let placed = placed(CAMPFIRE, p, Facing::East, glam::Vec3::X, false, |q| {
        Some(if q == p - IVec3::Y { Block::HAY_BALE } else { Block::AIR })
    });
    let c = campfire_of(placed.unwrap()).unwrap();
    assert!(c.signal && c.lit && c.facing == Facing::East);
    let wet = campfire_of(placed_wet()).unwrap();
    assert!(!wet.lit && wet.wet);
}

fn placed_wet() -> Block {
    placed(CAMPFIRE, IVec3::ZERO, Facing::North, glam::Vec3::NEG_Z, true, |_| Some(Block::STONE)).unwrap()
}

#[test]
fn coral_fans_hang_on_walls_and_die_dry() {
    let mut w = world();
    let wall = IVec3::new(8, 110, 8);
    w.set_block(wall, Block::STONE);
    let at = wall + IVec3::X;
    // Looking west at the wall's east face: the fan hangs on it, facing east.
    let fan = placed(ob::coral(2, 1, false), at, Facing::West, glam::Vec3::NEG_X, false, |q| w.get_block(q)).unwrap();
    assert_eq!(wall_fan_of(fan), Some((1, false, Facing::East, false)));
    w.set_block(at, fan);
    // Dry, it dies on its scheduled tick.
    for _ in 0..120 {
        w.tick_redstone();
    }
    assert_eq!(wall_fan_of(w.get_block(at).unwrap()), Some((1, true, Facing::East, false)));
    // Without its wall it falls off.
    w.set_block(wall, Block::AIR);
    assert_eq!(w.get_block(at), Some(Block::AIR));
    // Looking down onto a floor places the standing fan.
    w.set_block(wall, Block::STONE);
    let above = wall + IVec3::Y;
    let floor = placed(ob::coral(2, 0, false), above, Facing::North, glam::Vec3::NEG_Y, false, |q| w.get_block(q));
    // Live coral's registered state is the wet one; out of water it's dry.
    assert_eq!(floor, Some(ob::coral(2, 0, false).with_water(false)));
}

#[test]
fn wet_wall_fans_live() {
    let mut w = world();
    let wall = IVec3::new(8, 110, 8);
    w.set_block(wall, Block::STONE);
    let at = wall + IVec3::Z;
    w.set_block(at, wall_fan(3, false, Facing::South, true));
    for _ in 0..120 {
        w.tick_redstone();
    }
    assert_eq!(wall_fan_of(w.get_block(at).unwrap()), Some((3, false, Facing::South, true)));
}

#[test]
fn dripleaves_tilt_on_java_timings() {
    let mut w = world();
    let p = IVec3::new(5, 110, 5);
    w.set_block(p - IVec3::Y, Block::DIRT);
    let leaf = big_dripleaf(Facing::East, Tilt::None, false);
    w.set_block(p, leaf);
    w.dripleaf_step(p.as_dvec3() + glam::DVec3::new(0.5, 0.5, 0.5), true);
    assert_eq!(w.get_block(p), Some(leaf), "too low on the leaf");
    w.dripleaf_step(p.as_dvec3() + glam::DVec3::new(0.5, 15.0 / 16.0, 0.5), true);
    let tilt = |w: &World| dripleaf_of(w.get_block(p).unwrap()).unwrap().1;
    assert_eq!(tilt(&w), Tilt::Unstable);
    let mut seen = vec![];
    for _ in 0..121 {
        w.tick_utilities();
        if seen.last() != Some(&tilt(&w)) {
            seen.push(tilt(&w));
        }
    }
    assert_eq!(seen, [Tilt::Unstable, Tilt::Partial, Tilt::Full, Tilt::None]);
}

#[test]
fn trapped_chests_signal_their_viewers() {
    let mut w = world();
    let p = IVec3::new(9, 110, 9);
    w.set_block(p, TRAPPED_CHEST);
    assert!(crate::world::chest::is_chest(TRAPPED_CHEST));
    assert!(w.chest(p).is_some(), "it stores items like a chest");
    assert_eq!(w.trapped_signal(p, TRAPPED_CHEST, IVec3::X, false), Some(0));
    w.set_chest_viewers([p, p]);
    assert_eq!(w.trapped_signal(p, TRAPPED_CHEST, IVec3::X, false), Some(2));
    assert_eq!(w.trapped_signal(p, TRAPPED_CHEST, IVec3::X, true), Some(0));
    assert_eq!(w.trapped_signal(p, TRAPPED_CHEST, IVec3::NEG_Y, true), Some(2));
    w.set_chest_viewers([]);
    assert_eq!(w.chest_viewers(p), 0);
}

#[test]
fn block_entities_and_states_survive_save_and_reload() {
    use crate::world::storage::{LevelInfo, Storage};
    let mut w = world();
    let p = IVec3::new(1, 110, 1);
    w.set_block(p, CAMPFIRE);
    assert!(w.campfire_place_food(p, Item::SALMON));
    w.tick_utilities();
    w.utilities.decorated.insert(IVec3::new(2, 3, 4), Stack::new(Item::DIAMOND, 5));
    let saved = w.utilities_to_string();
    let mut other = world();
    other.load_utilities(&saved);
    assert_eq!(other.campfire_slots(p), w.campfire_slots(p));
    assert_eq!(other.utilities.decorated.get(&IVec3::new(2, 3, 4)).map(|s| s.count), Some(5));
    other.load_utilities("c1,2=junk|pq=|x1,2,3=4");

    let dir = std::env::temp_dir().join(format!("voxelcraft-utilities-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let mut chunk = ChunkData::Uniform(Block::STONE);
    let states: Vec<Block> = registered().collect();
    for (i, b) in states.iter().enumerate() {
        chunk.set(i % 32, i / 32, 3, *b);
    }
    let pos = IVec3::new(2, -2, -7);
    storage.save(&LevelInfo { seed: 1, player: None, props: Default::default() }, &[(pos, Arc::new(chunk))]).unwrap();
    let loaded = storage.load_chunks().unwrap();
    for (i, b) in states.iter().enumerate() {
        assert_eq!(loaded[&pos].get(i % 32, i / 32, 3), *b);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recipes_make_the_blocks() {
    use crate::crafting::Grid;
    let mut g = Grid::new(3);
    let iron = Some(Stack::new(Item::IRON_INGOT, 1));
    for i in [0, 2, 3, 5, 6, 7, 8] {
        g.cells[i] = iron;
    }
    assert_eq!(g.result().map(|s| s.item), Some(CAULDRON.into()));
    let mut g = Grid::new(3);
    let stick = Some(Stack::new(Item::STICK, 1));
    g.cells[1] = stick;
    g.cells[3] = stick;
    g.cells[5] = stick;
    g.cells[4] = Some(Stack::new(Item::CHARCOAL, 1));
    for i in 6..9 {
        g.cells[i] = Some(Stack::new(crate::world::nether_biome_blocks::CRIMSON_STEM, 1));
    }
    assert_eq!(g.result().map(|s| s.item), Some(CAMPFIRE.into()));
    let mut g = Grid::new(2);
    g.cells[0] = Some(Stack::new(Block::CHEST, 1));
    g.cells[3] = Some(Stack::new(crate::world::gadgets::HOOK, 1));
    assert_eq!(g.result().map(|s| s.item), Some(TRAPPED_CHEST.into()));
    let mut g = Grid::new(3);
    let brick = Some(Stack::new(Item::BRICK, 1));
    for i in [0, 2, 4] {
        g.cells[i] = brick;
    }
    assert_eq!(g.result().map(|s| s.item), Some(FLOWER_POT.into()));
}

#[test]
fn campfire_contact_hurts_the_player() {
    let mut w = world();
    let p = IVec3::new(4, 110, 4);
    w.set_block(p - IVec3::Y, Block::STONE);
    w.set_block(p, SOUL_CAMPFIRE);
    let player = crate::player::Player::new(p.as_dvec3() + glam::DVec3::new(0.5, 7.0 / 16.0, 0.5));
    assert_eq!(campfire::contact_damage(&w, &player), 2.0);
    w.douse_campfire(p);
    assert_eq!(campfire::contact_damage(&w, &player), 0.0);
}

#[test]
fn unsupported_stalactites_fall_break_and_hurt() {
    let mut w = world();
    let roof = IVec3::new(4, 120, 4);
    w.set_block(roof, Block::STONE);
    // A three-block stalactite: base, middle, tip; the floor ten below the tip.
    w.set_block(roof - IVec3::Y, ob::pointed_dripstone(true, ob::Thickness::Base));
    w.set_block(roof - IVec3::Y * 2, ob::pointed_dripstone(true, ob::Thickness::Middle));
    w.set_block(roof - IVec3::Y * 3, ob::pointed_dripstone(true, ob::Thickness::Tip));
    let floor = roof - IVec3::Y * 13;
    w.set_block(floor, Block::STONE);
    w.drops.clear();
    w.set_block(roof, Block::AIR);
    for i in 1..=3 {
        assert_eq!(w.get_block(roof - IVec3::Y * i), Some(Block::AIR), "the whole run falls");
    }
    assert_eq!(w.falling_blocks().len(), 3);
    let mut hit = None;
    for _ in 0..60 {
        w.tick_falling(crate::simulation::TICK_SECONDS);
        if let Some(&h) = w.stalactite_hits().first() {
            hit = Some(h);
        }
    }
    // The tip fell 9 blocks (from y 117 to 108): ceil(9 - 1) * max(3, 6) = 48, capped at 40.
    assert_eq!(hit, Some((floor + IVec3::Y, 40.0)));
    assert_eq!(dripstone::fall_damage(3.0, 6.0), 12.0);
    assert!(w.falling_blocks().is_empty());
    assert_eq!(w.get_block(floor + IVec3::Y), Some(Block::AIR), "dripstone breaks where it lands");
    let items = w
        .drops
        .iter()
        .filter(|(_, s)| s.item == Item::from_block(ob::pointed_dripstone(false, ob::Thickness::Tip)))
        .count();
    assert_eq!(items, 3);
}

#[test]
fn respawn_anchors_use_a_charge_and_stand_the_player_beside_them() {
    use crate::world::terrain::Dimension;
    let mut w = World::new_headless(Arc::new(Generator::for_dimension(5, Dimension::Nether)), Default::default(), 2);
    for y in w.generator.dimension.chunk_rows() {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    let p = IVec3::new(5, 70, 5);
    for x in 3..8 {
        for z in 3..8 {
            w.set_block(IVec3::new(x, 69, z), Block::NETHERRACK);
        }
    }
    w.set_block(p, respawn_anchor(2));
    let (at, _) = w.use_anchor_respawn(p).unwrap();
    // The first offset, north of the anchor, standing on the netherrack.
    assert_eq!(at, glam::DVec3::new(5.5, 70.0, 4.5));
    assert_eq!(w.get_block(p), Some(respawn_anchor(1)));
    w.use_anchor_respawn(p).unwrap();
    assert_eq!(w.use_anchor_respawn(p), None, "out of charge");
    // An overworld anchor never works as a spawn.
    let mut o = world();
    o.set_block(p, respawn_anchor(4));
    assert_eq!(o.use_anchor_respawn(p), None);
}

#[test]
fn a_stalactite_falling_through_a_save_still_lands_and_hurts() {
    let mut w = world();
    let roof = IVec3::new(4, 120, 4);
    w.set_block(roof, Block::STONE);
    w.set_block(roof - IVec3::Y, ob::pointed_dripstone(true, ob::Thickness::Tip));
    let floor = roof - IVec3::Y * 13;
    w.set_block(floor, Block::STONE);
    w.set_block(roof, Block::AIR);
    for _ in 0..3 {
        w.tick_falling(crate::simulation::TICK_SECONDS);
    }
    let saved = w.falling_to_string();
    let mut loaded = world();
    loaded.set_block(floor, Block::STONE);
    loaded.load_falling(&saved);
    assert_eq!(loaded.falling_blocks().len(), 1, "the tip survives the reload: {saved}");
    let mut hit = None;
    for _ in 0..60 {
        loaded.tick_falling(crate::simulation::TICK_SECONDS);
        if let Some(&h) = loaded.stalactite_hits().first() {
            hit = Some(h);
        }
    }
    // Fell from y 119 to 108: ceil(11 - 1) * 6 = 60, capped at 40.
    assert_eq!(hit, Some((floor + IVec3::Y, 40.0)), "it keeps its damage and start height");
    // Entries from before v0.12 (five fields) still load.
    loaded.load_falling(&format!("v1|{},4.0,100.0,4.0,0", Block::SAND.0));
    assert_eq!(loaded.falling_blocks().len(), 1);
}

#[test]
fn tilting_dripleaves_and_pending_drips_survive_a_save() {
    let mut w = world();
    let p = IVec3::new(5, 110, 5);
    w.set_block(p - IVec3::Y, Block::DIRT);
    w.set_block(p, big_dripleaf(Facing::East, Tilt::None, false));
    w.dripleaf_step(p.as_dvec3() + glam::DVec3::new(0.5, 15.0 / 16.0, 0.5), true);
    w.utilities.drips.push((IVec3::new(9, 100, 9), 30));
    let saved = w.utilities_to_string();
    let mut loaded = world();
    loaded.set_block(p - IVec3::Y, Block::DIRT);
    loaded.set_block(p, w.get_block(p).unwrap());
    loaded.load_utilities(&saved);
    assert_eq!(loaded.utilities.drips, w.utilities.drips);
    assert_eq!(loaded.utilities.tilts, w.utilities.tilts);
    for _ in 0..121 {
        loaded.tick_utilities();
    }
    assert_eq!(dripleaf_of(loaded.get_block(p).unwrap()).unwrap().1, Tilt::None, "it springs back after a reload");
}

#[test]
fn an_open_ender_chest_is_never_saved_as_a_chest() {
    let mut w = world();
    let p = IVec3::new(6, 100, 6);
    w.set_block(p, ender_chest(Facing::North, false));
    let mut contents = crate::world::chest::Chest::default();
    contents.slots[0] = Some(crate::inventory::Stack::new(Item::DIAMOND, 1));
    w.lend_chest(p, contents);
    assert!(w.chest(p).is_some(), "the screen sees it");
    assert_eq!(w.chests_to_string(), "", "the player saves their ender inventory");
    assert_eq!(w.take_lent_chest(p).map(|c| c.slots[0]), Some(contents.slots[0]));
}

#[test]
fn an_empty_lit_campfire_still_smokes() {
    let mut w = world();
    let p = IVec3::new(5, 100, 5);
    w.set_block(p, campfire(Campfire { soul: false, lit: true, signal: false, wet: false, facing: Facing::North }));
    while w.particles.pop().is_some() {}
    for _ in 0..200 {
        w.tick_utilities();
    }
    assert!(w.particles.pop().is_some(), "smoke rises with nothing cooking");
}

#[test]
fn a_powered_dripleaf_levels_at_once() {
    let mut w = world();
    let p = IVec3::new(5, 110, 5);
    w.set_block(p - IVec3::Y, Block::DIRT);
    w.set_block(p, big_dripleaf(Facing::East, Tilt::Full, false));
    w.set_block(p + IVec3::X, crate::world::redstone_blocks::REDSTONE_BLOCK);
    for _ in 0..2 {
        w.tick_redstone();
    }
    assert_eq!(dripleaf_of(w.get_block(p).unwrap()).unwrap().1, Tilt::None, "no wait for its timer");
}

#[test]
fn ender_chests_feed_no_hoppers_or_comparators() {
    let mut w = world();
    let p = IVec3::new(6, 100, 6);
    w.set_block(p, ender_chest(Facing::North, false));
    let mut contents = crate::world::chest::Chest::default();
    contents.slots[0] = Some(crate::inventory::Stack::new(Item::DIAMOND, 64));
    w.lend_chest(p, contents);
    assert_eq!(w.container_signal(p), None);
}

#[test]
fn a_redstone_conductor_on_top_keeps_an_ender_chest_shut() {
    let mut w = world();
    let p = IVec3::new(6, 100, 6);
    w.set_block(p, ender_chest(Facing::North, false));
    assert!(ender_chest_opens(&w, p));
    // `isRedstoneConductor`: the see-through slime block counts, glass doesn't.
    for (above, opens) in
        [(Block::GLASS, true), (crate::world::stone_blocks::SLIME_BLOCK, false), (Block::STONE, false)]
    {
        w.set_block(p + IVec3::Y, above);
        assert_eq!(ender_chest_opens(&w, p), opens, "{}", above.name());
    }
}

#[test]
fn lava_cauldrons_burn_and_water_cauldrons_put_out_fire() {
    let mut w = world();
    let p = IVec3::new(7, 100, 7);
    for (contents, lava, wet) in [(Contents::Lava, true, false), (Contents::Water(3), false, true)] {
        w.set_block(p, cauldron(contents));
        let player = crate::player::Player::new(p.as_dvec3() + glam::DVec3::new(0.5, 0.3, 0.5));
        let env = crate::simulation::player_environment(&player, &w, 0.0);
        assert_eq!((env.in_lava, env.wet), (lava, wet), "{contents:?}");
        // Standing on the rim is outside the contents.
        let player = crate::player::Player::new(p.as_dvec3() + glam::DVec3::new(0.5, 1.0, 0.5));
        let env = crate::simulation::player_environment(&player, &w, 0.0);
        assert!(!env.in_lava, "{contents:?} from the rim");
    }
}

#[test]
fn a_tilt_timer_waits_for_its_chunk_to_load() {
    let mut w = world();
    // A tilt saved for a cell whose chunk isn't loaded.
    let far = IVec3::new(5, 110, 5 + 32 * 3);
    w.load_utilities(&format!("t{},{},{}=1", far.x, far.y, far.z));
    for _ in 0..5 {
        w.tick_utilities();
    }
    assert_eq!(w.utilities.tilts, vec![(far, 1)], "kept until the chunk is there");
}

#[test]
fn a_lent_ender_inventory_comes_back_after_its_chest_is_mined() {
    let mut w = world();
    let p = IVec3::new(6, 100, 6);
    w.set_block(p, ender_chest(Facing::North, false));
    let mut contents = crate::world::chest::Chest::default();
    contents.slots[0] = Some(crate::inventory::Stack::new(Item::DIAMOND, 1));
    w.lend_chest(p, contents);
    // Someone else mines the ender chest while the screen is open.
    w.set_block(p, Block::AIR);
    assert_eq!(w.chests_to_string(), "", "never saved as a ghost chest");
    assert_eq!(w.container_signal(p), None, "hoppers and comparators can't reach it under air either");
    assert_eq!(w.take_lent_chest(p).map(|c| c.slots[0]), Some(contents.slots[0]), "the player gets it back");
    assert_eq!(w.lent_chest(), None);
    // A chest placed in its cell meanwhile keeps empty storage of its own.
    w.lend_chest(p, contents);
    w.set_block(p, Block::CHEST);
    assert!(w.take_lent_chest(p).is_some());
    assert_eq!(w.chest(p).map(|c| c.slots[0]), Some(None));
}
