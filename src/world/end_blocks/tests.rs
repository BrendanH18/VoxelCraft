use super::*;
use crate::world::slab_stair_states as ss;

#[test]
fn states_stay_inside_track_a_reservation_and_decode() {
    assert_eq!((FIRST, LAST), (12288, 12389));
    for id in FIRST..=LAST {
        let (name, kind, _) = registry(id);
        assert_ne!(name, "unknown", "{id}");
        assert_ne!(kind, RenderKind::Invisible, "{id}");
    }
    for d in Dir::ALL {
        assert_eq!(rod_facing(end_rod(d)), Some(d));
        assert_eq!(Dir::from_offset(d.offset()), Some(d));
        assert_eq!(d.opposite().opposite(), d);
    }
    for age in 0..=DEAD_AGE {
        assert_eq!(flower_age(chorus_flower(age)), Some(age));
    }
    assert_eq!(pillar_axis(Block(PURPUR_PILLAR.0 + 2)), Some(2));
    // The purpur stairs and slab are an appended family of every state.
    let mut stairs = 0;
    let mut slabs = 0;
    for id in FIRST..=LAST {
        if let Some(s) = ss::stairs(Block(id)) {
            assert_eq!(s.family as usize, ss::PURPUR_FAMILY);
            assert_eq!(ss::stairs_id(s), Block(id));
            stairs += 1;
        }
        if let Some(s) = ss::slab(Block(id)) {
            assert_eq!(s.family as usize, ss::PURPUR_FAMILY);
            slabs += 1;
        }
    }
    assert_eq!((stairs, slabs), (80, 5));
    assert_eq!(Block(PURPUR_STAIRS.0 + 40).base(), PURPUR_STAIRS);
    assert_eq!(PURPUR_SLAB.slab_base(), Some(PURPUR_BLOCK));
    assert_eq!(Block::from_name("purpur_stairs"), Some(PURPUR_STAIRS));
    assert_eq!(Block::from_name("end_rod"), Some(END_ROD));
}

#[test]
fn mining_light_and_drops_follow_java_blocks() {
    assert_eq!(PURPUR_BLOCK.hardness(), 1.5);
    assert_eq!(PURPUR_SLAB.hardness(), 2.0);
    assert_eq!(PURPUR_BLOCK.blast_resistance(), 6.0);
    assert_eq!(END_ROD.hardness(), 0.0);
    assert_eq!(CHORUS_PLANT.hardness(), 0.4);
    assert_eq!(CHORUS_FLOWER.best_tool(), Some(crate::item::ToolKind::Axe));
    assert_eq!(PURPUR_PILLAR.best_tool(), Some(crate::item::ToolKind::Pickaxe));
    assert_eq!(PURPUR_PILLAR.harvest_level(), Some(0));
    for d in Dir::ALL {
        assert_eq!(end_rod(d).emission(), 14);
        assert_eq!(end_rod(d).drop(), Some(END_ROD.into()));
    }
    assert_eq!(CHORUS_PLANT.drop(), None, "rolled by the world");
    assert_eq!(chorus_flower(5).drop(), Some(CHORUS_FLOWER.into()));
    assert_eq!(Block(PURPUR_PILLAR.0 + 1).drop(), Some(PURPUR_PILLAR.into()));
}

#[test]
fn placement_turns_pillars_and_rods() {
    assert_eq!(placed(PURPUR_PILLAR, IVec3::X, None), Block(PURPUR_PILLAR.0 + 1));
    assert_eq!(placed(PURPUR_PILLAR, IVec3::NEG_Z, None), Block(PURPUR_PILLAR.0 + 2));
    assert_eq!(placed(END_ROD, IVec3::NEG_X, None), end_rod(Dir::West));
    // `EndRodBlock.getStateForPlacement`: against a rod pointing at us, the
    // new one points back.
    assert_eq!(placed(END_ROD, IVec3::Y, Some(end_rod(Dir::Up))), end_rod(Dir::Down));
    assert_eq!(placed(END_ROD, IVec3::Y, Some(end_rod(Dir::North))), end_rod(Dir::Up));
    assert_eq!(rotated(end_rod(Dir::North), 1), end_rod(Dir::East));
    assert_eq!(rotated(end_rod(Dir::Up), 1), end_rod(Dir::Up));
    assert_eq!(rotated(Block(PURPUR_PILLAR.0 + 1), 1), Block(PURPUR_PILLAR.0 + 2));
}

#[test]
fn chorus_plants_join_their_neighbours() {
    let air = |_: Facing| Block::AIR;
    let core = boxes(CHORUS_PLANT, air, Block::AIR, Block::AIR).unwrap();
    assert_eq!(core.as_slice().len(), 1);
    let joined = boxes(
        CHORUS_PLANT,
        |f| if f == Facing::North { CHORUS_PLANT } else { Block::AIR },
        Block::END_STONE,
        chorus_flower(0),
    )
    .unwrap();
    assert_eq!(joined.as_slice().len(), 4, "north, down to end stone, up to the flower");
    assert!(chorus_connects(Dir::Down, Block::END_STONE));
    assert!(!chorus_connects(Dir::Up, Block::END_STONE));
}

fn grid(cells: &[(IVec3, Block)]) -> impl Fn(IVec3) -> Block + use<> {
    let cells = cells.to_vec();
    move |p| {
        cells.iter().find(|(q, _)| *q == p).map(|(_, b)| *b).unwrap_or(if p.y < 0 {
            Block::END_STONE
        } else {
            Block::AIR
        })
    }
}

#[test]
fn chorus_survival_follows_java() {
    let o = IVec3::ZERO;
    // On End stone, or on another plant.
    assert!(plant_survives(&grid(&[]), o));
    assert!(plant_survives(&grid(&[(o, CHORUS_PLANT)]), o + IVec3::Y));
    // A side branch hangs from a plant standing on support.
    let side = IVec3::new(1, 1, 0);
    assert!(plant_survives(&grid(&[(o, CHORUS_PLANT), (IVec3::Y, CHORUS_PLANT)]), side));
    // ...unless the branch has a block both above and below it.
    let blocked =
        [(o, CHORUS_PLANT), (IVec3::Y, CHORUS_PLANT), (side - IVec3::Y, Block::STONE), (side + IVec3::Y, Block::STONE)];
    assert!(!plant_survives(&grid(&blocked), side));
    // Floating with nothing beside it.
    assert!(!plant_survives(&grid(&[]), IVec3::new(0, 5, 0)));
    // A flower on support, or over air beside exactly one plant.
    assert!(flower_survives(&grid(&[]), o));
    assert!(flower_survives(&grid(&[(IVec3::new(1, 5, 0), CHORUS_PLANT)]), IVec3::new(0, 5, 0)));
    assert!(!flower_survives(
        &grid(&[(IVec3::new(1, 5, 0), CHORUS_PLANT), (IVec3::new(-1, 5, 0), CHORUS_PLANT)]),
        IVec3::new(0, 5, 0)
    ));
    assert!(!flower_survives(&grid(&[(IVec3::new(0, 4, 0), Block::STONE)]), IVec3::new(0, 5, 0)));
}

#[test]
fn chorus_flowers_grow_up_branch_and_die_like_java() {
    let o = IVec3::ZERO;
    // On End stone with room above: always up.
    let w = grid(&[(o, chorus_flower(0))]);
    let mut never = |_| 0;
    assert_eq!(flower_growth(&w, o, 255, &mut never), Growth::Up);
    // A dead flower doesn't grow.
    let w = grid(&[(o, chorus_flower(DEAD_AGE))]);
    assert_eq!(flower_growth(&w, o, 255, &mut never), Growth::None);
    // Atop a tall stem of plants floating in the air, an age 4 flower dies.
    let mut cells: Vec<_> = (0..5).map(|y| (IVec3::new(0, 10 + y, 0), CHORUS_PLANT)).collect();
    let top = IVec3::new(0, 15, 0);
    cells.push((top, chorus_flower(4)));
    let mut high = |n: u32| n - 1;
    assert_eq!(flower_growth(&grid(&cells), top, 255, &mut high), Growth::Dies);
    // Below age 4 it branches where there's room.
    cells.pop();
    cells.push((top, chorus_flower(2)));
    let mut rolls = [3u32, 3, 0, 1, 2].into_iter().cycle();
    let mut pick = |n: u32| rolls.next().unwrap() % n;
    match flower_growth(&grid(&cells), top, 255, &mut pick) {
        Growth::Branch(sides) => assert!(!sides.is_empty()),
        other => panic!("{other:?}"),
    }
    // The world's top row stops growth.
    let w = grid(&[(o, chorus_flower(0))]);
    assert_eq!(flower_growth(&w, o, 0, &mut never), Growth::None);
}

#[test]
fn generated_plants_stand_on_end_stone_within_their_spread() {
    let empty = |p: IVec3| p.y >= 0;
    let mut state = 12345u64;
    let mut int = |n: u32| {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) % u64::from(n)) as u32
    };
    for _ in 0..20 {
        let plant = generate_plant(IVec3::ZERO, MAX_SPREAD, &empty, &mut int);
        assert_eq!(plant[0], (IVec3::ZERO, CHORUS_PLANT));
        let w = |p: IVec3| {
            plant.iter().find(|(q, _)| *q == p).map(|(_, b)| *b).unwrap_or(if p.y < 0 {
                Block::END_STONE
            } else {
                Block::AIR
            })
        };
        for &(p, b) in &plant {
            assert!(p.x.abs() < MAX_SPREAD && p.z.abs() < MAX_SPREAD, "{p}");
            if b == CHORUS_PLANT {
                assert!(plant_survives(&w, p), "plant at {p} must stand");
            } else {
                assert_eq!(b, chorus_flower(DEAD_AGE));
                assert!(flower_survives(&w, p), "flower at {p}");
            }
        }
        assert!(plant.iter().any(|(_, b)| flower_age(*b) == Some(DEAD_AGE)));
    }
}

#[test]
fn textures_are_drawn_in_the_reserved_layers() {
    for layer in TEX..TEX_END {
        assert!((0..16).any(|y| (0..16).any(|x| pixel(layer, x, y)[3] == 255)), "{layer} is blank");
    }
}
