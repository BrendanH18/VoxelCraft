//! Expected values come from Java 26.3: `Blocks`, `CandleBlock`,
//! `CandleCakeBlock`, `BannerBlock`, `WallBannerBlock`, `StrawBedBlock`,
//! `BannerRenderer`, `FireBlock` and the recipe, loot table and tag JSON of
//! `client-26.3.jar`.
use super::*;
use crate::{
    crafting::Grid,
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::Item,
    rules::GameMode,
    session::{
        PlayerSession,
        interact::{Use, UseContext, Used},
    },
    world::{
        World,
        block::Facing,
        chunk::ChunkData,
        slab_stair_states::{self as ss, SlabType, StairShape, Stairs},
        terrain::Generator,
    },
};
use glam::{DVec3, IVec3};
use std::sync::Arc;

const P: IVec3 = IVec3::new(8, 140, 8);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    for x in 0..16 {
        for z in 0..16 {
            w.set_block(IVec3::new(x, P.y - 1, z), Block::STONE);
        }
    }
    w
}

/// A survival player standing at `feet`, looking at `target`.
fn player(feet: DVec3, target: DVec3) -> PlayerSession {
    let mut s = PlayerSession::new(PlayerId::HOST, feet);
    s.mode = GameMode::Survival;
    let d = target - s.player.eye();
    s.player.yaw = d.z.atan2(d.x) as f32;
    s.player.pitch = d.y.atan2(d.with_y(0.0).length()) as f32;
    s
}

fn use_held(s: &mut PlayerSession, w: &mut World) -> Option<Use> {
    let mut entities = Entities::new(7);
    s.use_held(0, w, &mut entities, &UseContext { sneaking: false, others: &[] }).ok().flatten()
}

fn craft(cells: &[(usize, Item)]) -> Option<Stack> {
    let mut g = Grid::new(3);
    for &(i, item) in cells {
        g.cells[i] = Some(Stack::new(item, 1));
    }
    g.result()
}

#[test]
fn every_registered_state_decodes_once_and_the_rest_of_the_range_stays_free() {
    for id in FIRST..=RESERVED_LAST {
        let b = Block(id);
        let kinds = [
            ss::stairs(b).is_some(),
            ss::slab(b).is_some(),
            candle::decode(id).is_some(),
            candle::decode_cake(id).is_some(),
            banner::decode(id).is_some(),
            straw_bed::decode(id).is_some(),
        ];
        let count = kinds.iter().filter(|&&k| k).count();
        assert_eq!(count, owns(id) as usize, "{id}");
        assert_eq!(b.name() == "unknown", !owns(id), "{id} {}", b.name());
        if owns(id) {
            assert!(b.is_targetable() && b.is_solid(), "{id}");
            assert!(b.hardness().is_finite() && b.hardness() >= 0.0, "{id}");
        }
    }
    // 2,560 stairs, 160 slabs, 272 candles, 34 cakes, 256 + 64 banners, 8 bed halves.
    assert_eq!((FIRST..END).filter(|&id| owns(id)).count(), 2560 + 160 + 272 + 34 + 320 + 8);
    assert_eq!(END, 27944);
}

#[test]
fn wool_and_concrete_stairs_and_slabs_have_every_java_state() {
    for f in 0..SHAPE_FAMILIES {
        let family = ss::COLOUR_FIRST + f;
        let fam = ss::FAMILIES[family];
        assert_eq!((fam.base, fam.stairs, fam.slab), (family_base(f), family_stairs(f), family_slab(f)));
        let colour = colour_name(DyeColor::ALL[f % 16]);
        let material = if f < 16 { "wool" } else { "concrete" };
        assert_eq!(fam.stairs.name(), format!("{colour} {material} stairs"));
        assert_eq!(fam.slab.name(), format!("{colour} {material} slab"));
        assert_eq!(Block::from_name(&format!("{}_{material}_stairs", colour.replace(' ', "_"))), Some(fam.stairs));
        let mut ids = std::collections::HashSet::new();
        for facing in Facing::ALL {
            for top in [false, true] {
                for shape in StairShape::ALL {
                    for wet in [false, true] {
                        let s = Stairs { family: family as u8, facing, top, shape, wet };
                        let id = ss::stairs_id(s);
                        assert!(ids.insert(id) && stairs_local(id.0).is_some(), "{s:?}");
                        assert_eq!(ss::stairs(id), Some(s));
                        assert_eq!(id.is_waterlogged(), wet);
                        assert_eq!(id.with_water(!wet), ss::stairs_id(Stairs { wet: !wet, ..s }));
                        assert_eq!(id.shaped(), Some(Shaped::Stairs(facing)));
                        assert_eq!(id.with_facing(Facing::East), ss::stairs_id(Stairs { facing: Facing::East, ..s }));
                        assert!(id.uses_shape_for_light_occlusion());
                    }
                }
            }
        }
        for (kind, wet) in [
            (SlabType::Bottom, false),
            (SlabType::Bottom, true),
            (SlabType::Top, false),
            (SlabType::Top, true),
            (SlabType::Double, false),
        ] {
            let s = ss::Slab { family: family as u8, kind, wet };
            let id = ss::slab_id(s);
            assert!(slab_local(id.0).is_some());
            assert_eq!(ss::slab(id), Some(s));
            assert_eq!(id.is_waterlogged(), wet);
            assert_eq!(id.is_opaque(), kind == SlabType::Double);
            assert_eq!(id.top_drop(), if kind == SlabType::Bottom { 8 } else { 0 });
        }
        assert_eq!(ss::slab_id(ss::Slab { family: family as u8, kind: SlabType::Bottom, wet: false }), fam.slab);
    }
}

#[test]
fn stairs_and_slabs_mine_burn_and_drop_like_their_full_block() {
    for f in 0..SHAPE_FAMILIES {
        let base = family_base(f);
        let all = (STAIRS_ORIGIN..SHAPES_END).map(Block).filter(|&b| ss::family_base(b) == Some(base));
        for b in all {
            let item = if ss::stairs(b).is_some() { family_stairs(f) } else { family_slab(f) };
            assert_eq!(b.base(), item);
            assert_eq!(b.drop(), Some(Item::from_block(item)), "{b:?}");
            assert_eq!(b.hardness(), base.hardness());
            assert_eq!(b.best_tool(), base.best_tool());
            assert_eq!(b.harvest_level(), base.harvest_level());
            assert_eq!(b.info().tex, base.info().tex);
            assert_eq!(b.emission(), 0);
            // `FireBlock.bootStrap`: wool stairs and slabs as wool; wet states never burn.
            let odds = if b.is_waterlogged() || f >= 16 { (0, 0) } else { (30, 60) };
            assert_eq!(b.fire_odds(), odds, "{b:?}");
        }
    }
    // Wool 0.8 by hand; concrete 1.8, a pickaxe to drop (`mineable/pickaxe`).
    assert_eq!(family_stairs(3).hardness(), 0.8);
    assert_eq!(family_slab(3).harvest_level(), None);
    assert_eq!(family_stairs(20).hardness(), 1.8);
    assert_eq!(family_slab(20).best_tool(), Some(ToolKind::Pickaxe));
    assert_eq!(family_slab(20).harvest_level(), Some(0));
    assert!(!crate::mining::can_harvest(family_stairs(20), None));
    assert!(crate::mining::can_harvest(family_stairs(3), None));
}

#[test]
fn double_slabs_drop_two_and_candles_drop_one_per_candle() {
    let mut w = world();
    let double = ss::slab_id(ss::Slab { family: ss::COLOUR_FIRST as u8 + 5, kind: SlabType::Double, wet: false });
    w.set_block(P, double);
    w.spill_block(P, double);
    assert_eq!(w.drops.pop().map(|(_, s)| (s.item, s.count)), Some((Item::from_block(family_slab(5)), 2)));
    let three = candle::candle(4, 3, true, false);
    w.spill_block(P, three);
    assert_eq!(
        w.drops.pop().map(|(_, s)| (s.item, s.count)),
        Some((Item::from_block(candle::candle(4, 1, false, false)), 3))
    );
    w.spill_block(P, candle::cake(4, true));
    assert_eq!(
        w.drops.pop().map(|(_, s)| (s.item, s.count)),
        Some((Item::from_block(candle::candle(4, 1, false, false)), 1))
    );
}

#[test]
fn recipes_match_the_reference_data() {
    let wool = Item::from_block(Block::wool(DyeColor::Lime));
    let concrete = Item::from_block(Block::concrete(DyeColor::Lime));
    let lime = 5;
    // `#  / ## / ###` stairs, four; `###` slabs, six.
    let stairs = [(0, wool), (3, wool), (4, wool), (6, wool), (7, wool), (8, wool)];
    assert_eq!(craft(&stairs), Some(Stack::new(family_stairs(lime), 4)));
    assert_eq!(craft(&[(3, concrete), (4, concrete), (5, concrete)]), Some(Stack::new(family_slab(16 + lime), 6)));
    // A banner: six wool over a stick.
    let stick = Item::STICK;
    let banner_cells = [(0, wool), (1, wool), (2, wool), (3, wool), (4, wool), (5, wool), (7, stick)];
    assert_eq!(craft(&banner_cells), Some(Stack::new(banner::standing(DyeColor::Lime as u8, 0), 1)));
    // A dyed candle from the plain one; a dyed candle can't be dyed again.
    let plain = Item::from_block(candle::candle(0, 1, false, false));
    let dye = DyeColor::Lime.dye();
    assert_eq!(craft(&[(0, plain), (4, dye)]), Some(Stack::new(candle::candle(1 + lime as u8, 1, false, false), 1)));
    assert_eq!(craft(&[(0, Item::from_block(candle::candle(3, 1, false, false))), (4, dye)]), None);
    // Three hay bales make four straw beds.
    let hay = Item::from_block(Block::HAY_BALE);
    assert_eq!(craft(&[(3, hay), (4, hay), (5, hay)]), Some(Stack::new(straw_bed::ITEM_BLOCK, 4)));
    // Stonecutter data: concrete to one stairs or two slabs; wool has none.
    let cutting = crate::crafting::colour::stonecutting();
    assert_eq!(cutting.len(), 32);
    assert!(cutting.contains(&(concrete, Item::from_block(family_stairs(16 + lime)), 1)));
    assert!(cutting.contains(&(concrete, Item::from_block(family_slab(16 + lime)), 2)));
    assert!(!cutting.iter().any(|&(input, ..)| input == wool));
}

#[test]
fn items_names_stacks_and_creative_entries() {
    for id in palette_ids() {
        let item = Item::from_block(Block(id));
        assert!(Item::creative_palette().any(|i| i == item), "{}", Block(id).name());
        assert_eq!(Item::from_name(Block(id).name()), Some(item), "{}", Block(id).name());
    }
    assert_eq!(Item::from_block(banner::standing(3, 0)).max_stack(), 16);
    assert_eq!(Item::from_block(straw_bed::ITEM_BLOCK).max_stack(), 16);
    assert_eq!(Item::from_block(candle::candle(0, 1, false, false)).max_stack(), 64);
    assert_eq!(candle::candle(0, 1, false, false).name(), "candle");
    assert_eq!(candle::cake(0, false).name(), "cake with candle");
    assert_eq!(candle::cake(9, true).name(), "cake with light gray candle");
    assert_eq!(banner::wall(15, Facing::West).name(), "black wall banner");
    // Every banner state and candle state is picked and dropped as its item.
    assert_eq!(banner::wall(15, Facing::West).base(), banner::standing(15, 0));
    assert_eq!(banner::standing(15, 11).drop(), Some(Item::from_block(banner::standing(15, 0))));
    assert_eq!(candle::candle(2, 4, true, true).base(), candle::candle(2, 1, false, false));
    assert!(candle::candle(2, 4, true, true).flat_icon() && banner::standing(0, 0).flat_icon());
    // Pick-block on a candle cake gives the cake, like Java's `getCloneItemStack`.
    assert_eq!(crate::session::interact::picked_item(candle::cake(3, true)), Item::CAKE);
}

#[test]
fn candle_states_light_and_shapes_follow_candle_block() {
    for colour in 0..CANDLE_COLOURS as u8 {
        for candles in 1..=4 {
            for lit in [false, true] {
                for wet in [false, true] {
                    let b = candle::candle(colour, candles, lit, wet);
                    assert_eq!(candle::decode(b.0), Some(Candle { colour, candles, lit, wet }));
                    assert_eq!(b.emission(), if lit { 3 * candles } else { 0 });
                    assert_eq!(b.is_waterlogged(), wet);
                    assert_eq!(b.with_water(!wet), candle::candle(colour, candles, lit, !wet));
                    assert_eq!(b.hardness(), 0.1);
                    assert_eq!(b.light_opacity(), if wet { 1 } else { 0 });
                    assert!(b.can_stay_on(Block::STONE) && !b.can_stay_on(Block::AIR));
                    let boxes = candle::boxes(Candle { colour, candles, lit, wet });
                    assert_eq!(boxes.as_slice().len(), candles as usize * 2);
                    assert_eq!(candle::flame_offsets(b).len(), candles as usize);
                    // Java's selection shape holds every visible body.
                    let outline = candle::outline(Candle { colour, candles, lit, wet }).bounds().unwrap();
                    for bx in &boxes.as_slice()[..candles as usize] {
                        assert!((0..3).all(|i| bx.min[i] >= outline.min[i] && bx.max[i] <= outline.max[i]));
                    }
                }
            }
        }
        assert_eq!(candle::cake(colour, true).emission(), 3);
        assert_eq!(candle::cake(colour, false).emission(), 0);
        assert_eq!(candle::cake(colour, false).hardness(), 0.5);
    }
    // A wet candle can't stay lit (`placeLiquid` puts it out).
    let mut w = world();
    w.set_block(P, candle::candle(1, 2, true, false));
    assert!(!candle::can_light(candle::candle(1, 2, false, true)));
    w.set_block(P, candle::candle(1, 2, true, true));
    assert_eq!(w.get_block(P), Some(candle::candle(1, 2, false, true)));
}

#[test]
fn candles_stack_to_four_and_light_and_go_out() {
    let mut w = world();
    let candle_item = Item::from_block(candle::candle(6, 1, false, false));
    let mut s = player(P.as_dvec3() + DVec3::new(-2.5, 0.0, 0.5), P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    s.inventory.slots[0] = Some(Stack::new(candle_item, 8));
    for n in 1..=4 {
        let used = use_held(&mut s, &mut w).map(|u| u.used);
        assert!(matches!(used, Some(Used::Placed(_))), "{n}: {used:?}");
        assert_eq!(w.get_block(P), Some(candle::candle(6, n, false, false)));
    }
    // A fifth goes beside it (the full candle isn't replaceable).
    let _ = use_held(&mut s, &mut w);
    assert_eq!(w.get_block(P), Some(candle::candle(6, 4, false, false)));
    // Flint and steel lights it and wears.
    s.inventory.slots[0] = Some(Stack::new(Item::FLINT_AND_STEEL, 1));
    let used = use_held(&mut s, &mut w).map(|u| u.used);
    assert!(matches!(used, Some(Used::Ignited { cell, portal: false }) if cell == P), "{used:?}");
    assert_eq!(w.get_block(P), Some(candle::candle(6, 4, true, false)));
    assert_eq!(w.block_light(P), 12);
    // A bare hand puts it out.
    s.inventory.slots[0] = None;
    let used = use_held(&mut s, &mut w).map(|u| u.used);
    assert!(matches!(used, Some(Used::CandleExtinguished { cell }) if cell == P), "{used:?}");
    assert_eq!(w.get_block(P), Some(candle::candle(6, 4, false, false)));
    // A fire charge lights it too, and is used up.
    s.inventory.slots[0] = Some(Stack::new(Item::FIRE_CHARGE, 2));
    assert!(matches!(use_held(&mut s, &mut w).map(|u| u.used), Some(Used::Ignited { .. })));
    assert_eq!(s.inventory.get(0).map(|s| s.count), Some(1));
    // Breaking the block under it pops it off as four candles.
    w.drops.clear();
    w.set_block(P - IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::AIR));
    assert_eq!(w.drops.iter().map(|(_, s)| s.count).sum::<u8>(), 4);
}

#[test]
fn a_candle_goes_on_a_whole_cake_and_comes_off_when_it_is_eaten() {
    let mut w = world();
    w.set_block(P, Block::cake(0));
    let mut s = player(P.as_dvec3() + DVec3::new(-2.5, 0.0, 0.5), P.as_dvec3() + DVec3::new(0.5, 0.45, 0.5));
    s.vitals.hunger.food = 10.0;
    s.inventory.slots[0] = Some(Stack::new(Item::from_block(candle::candle(0, 1, false, false)), 2));
    let used = use_held(&mut s, &mut w).map(|u| u.used);
    assert!(matches!(used, Some(Used::CandlePlacedOnCake { .. })), "{used:?}");
    assert_eq!(w.get_block(P), Some(candle::cake(0, false)));
    assert_eq!(s.inventory.get(0).map(|s| s.count), Some(1));
    // Lit, it glows; flint and steel passes through to light it.
    s.inventory.slots[0] = Some(Stack::new(Item::FLINT_AND_STEEL, 1));
    assert!(matches!(use_held(&mut s, &mut w).map(|u| u.used), Some(Used::Ignited { .. })));
    assert_eq!(w.get_block(P), Some(candle::cake(0, true)));
    assert_eq!(w.block_light(P), 3);
    // A bare hand on the cake (not the candle) eats a slice and drops the candle.
    s.inventory.slots[0] = None;
    w.drops.clear();
    let used = use_held(&mut s, &mut w).map(|u| u.used);
    assert!(matches!(used, Some(Used::CakeBitten { .. })), "{used:?}");
    assert_eq!(w.get_block(P), Some(Block::cake(1)));
    assert_eq!(s.vitals.hunger.food, 12.0);
    assert_eq!(
        w.drops.iter().map(|(_, s)| s.item).collect::<Vec<_>>(),
        [Item::from_block(candle::candle(0, 1, false, false))]
    );
    // A bitten cake takes no candle.
    s.inventory.slots[0] = Some(Stack::new(Item::from_block(candle::candle(0, 1, false, false)), 1));
    assert!(!matches!(use_held(&mut s, &mut w).map(|u| u.used), Some(Used::CandlePlacedOnCake { .. })));
}

#[test]
fn clicking_a_lit_candle_cakes_candle_by_hand_puts_it_out() {
    let mut w = world();
    w.set_block(P, candle::cake(7, true));
    let mut s = player(P.as_dvec3() + DVec3::new(-2.5, 0.0, 0.5), P.as_dvec3() + DVec3::new(0.5, 0.8, 0.5));
    let used = use_held(&mut s, &mut w).map(|u| u.used);
    assert!(matches!(used, Some(Used::CandleExtinguished { .. })), "{used:?}");
    assert_eq!(w.get_block(P), Some(candle::cake(7, false)));
}

#[test]
fn banners_stand_on_the_ground_facing_the_player_or_hang_on_walls() {
    let mut w = world();
    let item = Item::from_block(banner::standing(DyeColor::Red as u8, 0));
    // Looking north and down at the floor: rotation 0 (the front faces south).
    let mut s = player(P.as_dvec3() + DVec3::new(0.5, 0.0, 3.5), P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    s.inventory.slots[0] = Some(Stack::new(item, 2));
    let placed = s.place_held(0, &mut w, &[]);
    assert!(placed.is_ok(), "{placed:?}");
    assert_eq!(w.get_block(P), Some(banner::standing(DyeColor::Red as u8, 0)));
    assert_eq!(w.banners().count(), 1);
    // Clicking the side of a wall hangs it there, facing the player.
    let wall_at = P + IVec3::new(4, 0, 0);
    w.set_block(wall_at, Block::STONE);
    let mut s = player(wall_at.as_dvec3() + DVec3::new(-2.5, 0.0, 0.5), wall_at.as_dvec3() + DVec3::new(0.0, 0.6, 0.5));
    s.inventory.slots[0] = Some(Stack::new(item, 1));
    let placed = s.place_held(0, &mut w, &[]);
    assert!(placed.is_ok(), "{placed:?}");
    let hung = wall_at - IVec3::X;
    assert_eq!(w.get_block(hung), Some(banner::wall(DyeColor::Red as u8, Facing::West)));
    assert_eq!(w.banners().count(), 2);
    // Taking the wall away drops it; taking the floor away drops the other.
    w.drops.clear();
    w.set_block(wall_at, Block::AIR);
    assert_eq!(w.get_block(hung), Some(Block::AIR));
    w.set_block(P - IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::AIR));
    assert_eq!(w.drops.iter().filter(|(_, s)| s.item == item).count(), 2);
    assert_eq!(w.banners().count(), 0);
}

#[test]
fn banners_have_no_collision_and_a_java_selection_box() {
    for colour in 0..16 {
        for r in 0..16 {
            let b = banner::standing(colour, r);
            let none = |_: Facing| Block::AIR;
            assert!(crate::world::shape::collision(b, none, Block::AIR).is_empty());
            assert!(crate::world::shape::shape(b, none, Block::AIR).is_empty(), "drawn as a model");
            let outline = crate::world::shape::outline(b, none, Block::AIR);
            assert_eq!(outline.as_slice(), [Box16 { min: [4, 0, 4], max: [12, 16, 12] }]);
            assert_eq!(b.hardness(), 1.0);
            assert_eq!(b.best_tool(), Some(ToolKind::Axe));
            assert!(b.ignited_by_lava());
            assert_eq!(b.light_opacity(), 0);
        }
        for f in Facing::ALL {
            let b = banner::wall(colour, f);
            let outline = crate::world::shape::outline(b, |_| Block::AIR, Block::AIR).bounds().unwrap();
            // Against the wall behind it, 13 high.
            let behind = -f.offset();
            let axis = if f.along_x() { 0 } else { 2 };
            let (lo, hi) = (outline.min[axis], outline.max[axis]);
            assert_eq!(hi - lo, 2);
            assert_eq!(if behind[axis] < 0 { lo } else { hi }, if behind[axis] < 0 { 0 } else { 16 });
            assert_eq!(outline.max[1], 13);
        }
    }
}

use super::super::shape::Box16;

#[test]
fn banner_models_follow_banner_renderer() {
    let red = DyeColor::Red as u8;
    // Rotation 0: pole up the middle to 28 px, flag in front (+z) of it.
    let quads = banner::quads(Banner { colour: red, mount: BannerMount::Ground(0) }, 0.25);
    assert_eq!(quads.len(), 18);
    let top = quads.iter().filter(|q| q.layer == tex::POLE).flat_map(|q| q.corners).fold(f32::MIN, |m, c| m.max(c.y));
    assert!((top - 44.0 * 2.0 / 3.0 / 16.0).abs() < 1e-4, "{top}");
    let flag: Vec<_> = quads.iter().filter(|q| q.layer == tex::CLOTH + red as u16).collect();
    assert_eq!(flag.len(), 6);
    assert!(flag.iter().flat_map(|q| q.corners).all(|c| c.z > 0.5));
    // Turned a quarter (rotation 4) the flag is on the -x side; a west-facing
    // wall banner hangs against the wall to its east.
    let turned = banner::quads(Banner { colour: red, mount: BannerMount::Ground(4) }, 0.25);
    assert!(turned.iter().filter(|q| q.layer != tex::POLE).flat_map(|q| q.corners).all(|c| c.x < 0.5));
    let wall = banner::quads(Banner { colour: red, mount: BannerMount::Wall(Facing::West) }, 0.25);
    assert!(wall.iter().flat_map(|q| q.corners).all(|c| c.x > 0.75));
    // The wall banner's flag hangs into the block below.
    assert!(wall.iter().flat_map(|q| q.corners).any(|c| c.y < -0.7));
    // `RotationSegment.convertToSegment(yRot + 180)`.
    assert_eq!(banner::rotation_for(180.0), 0);
    assert_eq!(banner::rotation_for(0.0), 8);
    assert_eq!(banner::rotation_for(-90.0), 4);
    assert_eq!(banner::rotation_for(11.24), 8);
    assert_eq!(banner::rotation_for(11.26), 9);
    assert_eq!(banner::java_y_rot(glam::Vec3::new(0.0, 0.0, -1.0)).abs(), 180.0);
    assert!((banner::java_y_rot(glam::Vec3::new(-1.0, 0.0, 0.0)) - 90.0).abs() < 1e-4);
}

#[test]
fn straw_beds_place_both_halves_and_break_together() {
    let mut w = world();
    let mut s = player(P.as_dvec3() + DVec3::new(0.5, 0.0, -2.5), P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    s.inventory.slots[0] = Some(Stack::new(Item::from_block(straw_bed::ITEM_BLOCK), 1));
    assert!(matches!(use_held(&mut s, &mut w).map(|u| u.used), Some(Used::Placed(_))));
    // Looking south: the head goes south of the foot.
    assert_eq!(w.get_block(P), Some(straw_bed::half(Facing::South, false)));
    assert_eq!(w.get_block(P + IVec3::Z), Some(straw_bed::half(Facing::South, true)));
    assert_eq!(straw_bed::head_of(&w, P), Some(P + IVec3::Z));
    // Only the head drops the bed, whichever half breaks.
    w.drops.clear();
    w.set_block(P, Block::AIR);
    w.spill_block(P, straw_bed::half(Facing::South, false));
    assert_eq!(w.get_block(P + IVec3::Z), Some(Block::AIR));
    assert_eq!(
        w.drops.iter().map(|(_, s)| (s.item, s.count)).collect::<Vec<_>>(),
        [(Item::from_block(straw_bed::ITEM_BLOCK), 1)]
    );
    let head = straw_bed::half(Facing::East, true);
    assert_eq!(head.hardness(), 0.2);
    assert_eq!(head.best_tool(), Some(ToolKind::Hoe));
    assert_eq!(head.fire_odds(), (30, 60));
    let boxes = straw_bed::boxes(Facing::East, true);
    assert_eq!(boxes.as_slice()[1], Box16 { min: [8, 0, 0], max: [16, 5, 16] }, "the pillow at the far end");
    // Players stand and lie on it: its collision is its shape.
    for half in [straw_bed::half(Facing::East, false), head] {
        let collision = crate::world::shape::collision(half, |_| Block::AIR, Block::STONE);
        assert_eq!(collision.as_slice(), super::boxes(half).unwrap().as_slice());
    }
}

#[test]
fn colour_blocks_survive_save_and_reload() {
    use crate::world::storage::Storage;
    // The highest state in the file decides its header: `VXC4` here.
    let dir = std::env::temp_dir().join(format!("voxelcraft-colour-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let states = [
        family_stairs(31),
        ss::stairs_id(Stairs {
            family: ss::COLOUR_FIRST as u8 + 3,
            facing: Facing::West,
            top: true,
            shape: StairShape::OuterLeft,
            wet: true,
        }),
        ss::slab_id(ss::Slab { family: ss::COLOUR_FIRST as u8 + 20, kind: SlabType::Top, wet: true }),
        candle::candle(16, 3, true, false),
        candle::cake(5, true),
        banner::standing(9, 13),
        banner::wall(2, Facing::North),
        straw_bed::half(Facing::West, true),
    ];
    let mut chunk = ChunkData::Uniform(Block::AIR);
    for (i, &b) in states.iter().enumerate() {
        chunk.set(i, 0, 0, b);
    }
    let pos = IVec3::new(3, 4, 5);
    let storage = Storage::new(&dir);
    let level = crate::world::storage::LevelInfo { seed: 42, player: None, props: Default::default() };
    storage.save(&level, &[(pos, Arc::new(chunk))]).unwrap();
    let loaded = storage.load_chunks().unwrap();
    for (i, &b) in states.iter().enumerate() {
        assert_eq!(loaded[&pos].get(i, 0, 0), b);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cushions_are_crafted_placed_on_top_faces_ridden_and_broken() {
    use crate::entity::minecart::{CartKind, Minecart};
    // Three wool slabs of one colour make a cushion; a dye recolours one.
    let slab = Item::from_block(family_slab(DyeColor::Cyan as usize));
    assert_eq!(craft(&[(0, slab), (1, slab), (2, slab)]), Some(Stack::new(Item::cushion(DyeColor::Cyan), 1)));
    let red = Item::cushion(DyeColor::Red);
    assert_eq!(craft(&[(0, red), (4, DyeColor::Cyan.dye())]), Some(Stack::new(Item::cushion(DyeColor::Cyan), 1)));
    assert_eq!(craft(&[(0, red), (4, DyeColor::Red.dye())]), None, "not its own colour");
    assert_eq!(red.max_stack(), 16);
    assert_eq!(Item::from_name("red_cushion"), Some(red));
    assert_eq!(crate::content::fuel::ticks(red), Some(50));

    let mut w = world();
    let mut entities = Entities::new(7);
    // On the stone floor's top face, centred, at the face's height.
    let mut s = player(P.as_dvec3() + DVec3::new(0.5, 0.0, -2.5), P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    s.inventory.slots[0] = Some(Stack::new(red, 2));
    let ctx = UseContext { sneaking: false, others: &[] };
    let used = s.use_held(0, &mut w, &mut entities, &ctx).ok().flatten().map(|u| u.used);
    assert!(matches!(used, Some(Used::CushionPlaced { .. })), "{used:?}");
    assert_eq!(s.inventory.get(0).map(|s| s.count), Some(1));
    assert_eq!(entities.minecarts.len(), 1);
    let cushion = &entities.minecarts[0];
    assert_eq!(cushion.kind, CartKind::Cushion { colour: DyeColor::Red as u8 });
    assert!(cushion.pos.distance(P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)) < 1e-6, "{:?}", cushion.pos);
    let id = cushion.id;
    // The same spot holds only one cushion.
    assert!(entities.place_cushion(DyeColor::Lime as u8, P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), 0.0).is_none());
    // Not on a side face.
    w.set_block(P + IVec3::new(3, 0, 0), Block::STONE);
    assert_eq!(cushion::placement(&w, P + IVec3::new(3, 0, 0), IVec3::NEG_X, DVec3::new(11.0, 140.5, 8.5)), None);
    // Sneaking passes over it (`Cushion.interact`).
    let sneak = UseContext { sneaking: true, others: &[] };
    let used = s.use_held(0, &mut w, &mut entities, &sneak).ok().flatten().map(|u| u.used);
    assert!(!matches!(used, Some(Used::Mounted(_))), "{used:?}");
    assert_eq!(s.player.vehicle, None);
    // Sitting: the rider's feet below the cushion's top, as Java's attachments put them.
    let used = s.use_held(0, &mut w, &mut entities, &ctx).ok().flatten().map(|u| u.used);
    assert!(matches!(used, Some(Used::Mounted(m)) if m == id), "{used:?}");
    let seat = entities.cart(id).unwrap().passenger_seat(0);
    assert!((seat.y - (P.y as f64 + 0.25 - 0.6)).abs() < 1e-9);
    // Saved and loaded with its colour.
    let text = entities.cart(id).unwrap().serialize();
    assert_eq!(Minecart::deserialize(&text).map(|c| c.kind), Some(CartKind::Cushion { colour: DyeColor::Red as u8 }));
    // The floor going leaves it without an anchor: it breaks on the 101st
    // tick into its item (`BlockAttachedEntity.tick`: `checkInterval++ == 100`).
    w.set_block(P - IVec3::Y, Block::AIR);
    for _ in 0..100 {
        entities.tick_minecarts(&mut w, &[]);
        assert_eq!(entities.minecarts.len(), 1, "it checks only every hundred ticks");
    }
    entities.tick_minecarts(&mut w, &[]);
    assert!(entities.minecarts.is_empty());
    assert_eq!(entities.items.iter().map(|i| i.stack.item).collect::<Vec<_>>(), [red]);
    // One hit breaks one; a creative hit leaves no item.
    let pos = P.as_dvec3() + DVec3::new(2.5, 0.0, 0.5);
    let id = entities.place_cushion(1, pos, 0.0).unwrap();
    let eye = pos + DVec3::new(0.0, 1.0, -1.5);
    assert!(entities.hurt_cart(eye, (pos + DVec3::Y * 0.1 - eye).normalize(), 5.0, true));
    assert!(entities.cart(id).is_none());
    assert_eq!(entities.items.len(), 1);
    // Set down in fire, it burns at once into its item (`Cushion.destroyIfInFire`).
    let mut w = world();
    let mut entities = Entities::new(7);
    w.set_block(P, Block::FIRE);
    let id = entities.place_cushion(1, P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), 0.0).unwrap();
    assert!(entities.burn_cushion(&w, id));
    assert!(entities.minecarts.is_empty());
    assert_eq!(entities.items.len(), 1);
}

#[test]
fn cushion_placement_follows_its_anchor_and_burial_rules() {
    let w = world();
    let floor = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    assert!(cushion::can_be_placed_at(&w, floor));
    assert!(!cushion::can_be_placed_at(&w, floor + DVec3::Y * 0.5), "nothing under it");
    assert!(!cushion::can_be_placed_at(&w, floor - DVec3::Y * 0.5), "inside the stone");
    // A bottom slab's top holds one; a carpet lifts it a sixteenth.
    let mut w = world();
    w.set_block(P, family_slab(2));
    assert!(cushion::can_be_placed_at(&w, floor + DVec3::Y * 0.5));
    assert_eq!(
        cushion::placement(&w, P, IVec3::Y, floor + DVec3::Y * 0.5),
        Some(DVec3::new(P.x as f64 + 0.5, P.y as f64 + 0.5, P.z as f64 + 0.5))
    );
}

#[test]
fn lan_clients_index_banners_from_snapshots_and_edits() {
    use crate::world::chunk::CHUNK_BITS;
    let red = banner::standing(DyeColor::Red as u8, 0);
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    let pos = IVec3::new(0, 4, 0);
    let mut data = ChunkData::Uniform(Block::AIR);
    data.set(1, 2, 3, red);
    w.receive_chunk(pos, Arc::new(data));
    assert_eq!(w.banners().collect::<Vec<_>>(), vec![(IVec3::new(1, 130, 3), red)], "from the snapshot");
    let cell = |x: u16, y: u16, z: u16| x | z << CHUNK_BITS | y << (2 * CHUNK_BITS);
    w.apply_replica_edits(pos, &[(cell(1, 2, 3), Block::AIR), (cell(4, 5, 6), red)]);
    assert_eq!(w.banners().collect::<Vec<_>>(), vec![(IVec3::new(4, 133, 6), red)], "from the host's edits");
    w.receive_chunk(pos, Arc::new(ChunkData::Uniform(Block::AIR)));
    assert_eq!(w.banners().count(), 0, "a new snapshot replaces them");
}

#[test]
fn sleepers_stand_up_beside_a_straw_bed() {
    let mut w = world();
    let head = P + IVec3::X;
    w.set_block(P, straw_bed::half(Facing::East, false));
    w.set_block(head, straw_bed::half(Facing::East, true));
    assert!(crate::world::dismount::is_sleep_head(w.get_block(head).unwrap()));
    let (stand, _) = crate::world::dismount::bed_stand_up(&w, head, Facing::East, 0.0).expect("room beside the bed");
    assert!(stand.distance(head.as_dvec3() + DVec3::splat(0.5)) < 2.5, "{stand}");
}
