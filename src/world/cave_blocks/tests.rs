use std::sync::Arc;

use glam::{IVec3, Vec3};

use super::*;
use crate::item::Item;
use crate::world::World;
use crate::world::block::{Block, STATE_CAPACITY, Shaped};
use crate::world::chunk::ChunkData;
use crate::world::terrain::Generator;

/// Every registered state of this module.
fn states() -> Vec<Block> {
    (0..STATE_CAPACITY as u16).filter(|&id| owns(id) && registry(id).is_some()).map(Block).collect()
}

/// A headless world whose chunk (0, 4, 0) is air: cells 0..32 by 128..160.
fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(5)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}

#[test]
fn every_state_is_registered_once_with_its_java_name() {
    let all = states();
    // 63 face sets, dry and wet, for each multiface block; sculk; two
    // catalysts; three sensor phases and four shrieker states, dry and wet.
    assert_eq!(all.len(), 2 * 2 * 63 + 1 + 2 + 2 * 3 + 2 * 4);
    for b in &all {
        assert_ne!(b.name(), "unknown", "#{}", b.0);
        assert_eq!(Block::from_name(b.name()), Some(b.base()), "{} names its default state", b.name());
    }
    for (name, block) in [
        ("glow lichen", GLOW_LICHEN),
        ("sculk", SCULK),
        ("sculk vein", SCULK_VEIN),
        ("sculk catalyst", SCULK_CATALYST),
        ("sculk sensor", SCULK_SENSOR),
        ("sculk shrieker", SCULK_SHRIEKER),
    ] {
        assert_eq!(Block::from_name(name), Some(block));
        assert!(Block::creative_palette().any(|b| b == block), "{name} in the creative palette");
        assert_eq!(Item::from_block(block).block(), Some(block), "{name} has an item form");
    }
    // The ids are saved: pin the layout.
    assert_eq!(GLOW_LICHEN, Block(9729));
    assert_eq!(multiface(Multiface::GlowLichen, ALL_FACES, true), Block(9855));
    assert_eq!(SCULK_VEIN, Block(9857));
    assert_eq!(multiface(Multiface::SculkVein, ALL_FACES, true), Block(9983));
    assert_eq!((SCULK, SCULK_CATALYST, sculk_catalyst(true)), (Block(11264), Block(11265), Block(11266)));
    assert_eq!(sculk_sensor(Phase::Cooldown, true), Block(11274));
    assert_eq!(sculk_shrieker(true, true, true), Block(11283));
    for id in [9728, 9792, 9856, 9920, 11267, 11271, 11275, 11284] {
        assert!(registry(id).is_none(), "#{id} stays unregistered");
    }
}

#[test]
fn state_properties_round_trip() {
    for kind in [Multiface::GlowLichen, Multiface::SculkVein] {
        for faces in 1..=ALL_FACES {
            for wet in [false, true] {
                let b = multiface(kind, faces, wet);
                assert_eq!(multiface_of(b), Some((kind, faces, wet)));
                assert_eq!(b.is_waterlogged(), wet);
                assert_eq!(b.with_water(!wet), multiface(kind, faces, !wet));
            }
        }
    }
    for phase in Phase::ALL {
        for wet in [false, true] {
            let b = sculk_sensor(phase, wet);
            assert_eq!(sensor_of(b), Some((phase, wet)));
            assert_eq!((b.is_waterlogged(), b.with_water(!wet)), (wet, sculk_sensor(phase, !wet)));
        }
    }
    for i in 0..8 {
        let (shrieking, summon, wet) = (i & 1 != 0, i & 2 != 0, i & 4 != 0);
        let b = sculk_shrieker(shrieking, summon, wet);
        assert_eq!(shrieker_of(b), Some((shrieking, summon, wet)));
        assert_eq!(b.with_water(!wet), sculk_shrieker(shrieking, summon, !wet));
    }
    assert!(!SCULK.waterloggable() && !SCULK_CATALYST.waterloggable());
    assert_eq!(catalyst_bloom(sculk_catalyst(true)), Some(true));
}

#[test]
fn light_collision_and_rendering_follow_java() {
    for b in states() {
        let lichen = matches!(multiface_of(b), Some((Multiface::GlowLichen, ..)));
        let expected = if lichen {
            7
        } else if catalyst_bloom(b).is_some() {
            6
        } else if sensor_of(b).is_some() {
            1
        } else {
            0
        };
        assert_eq!(b.emission(), expected, "{} #{}", b.name(), b.0);
        if multiface_of(b).is_some() {
            assert!(!b.is_solid() && b.is_replaceable(), "{} is passable and replaceable", b.0);
            assert_eq!(b.shaped(), Some(Shaped::Custom));
            assert!(collision(b).as_slice().is_empty());
            assert_eq!(boxes(b).as_slice().len() as u32, faces(b).unwrap());
            assert_eq!(b.light_opacity(), u8::from(b.is_waterlogged()));
        }
    }
    assert!(SCULK.is_opaque() && SCULK_CATALYST.is_opaque());
    assert!(SCULK_SENSOR.is_solid() && !SCULK_SENSOR.is_opaque());
    assert_eq!(collision(SCULK_SHRIEKER).as_slice(), &[Box16 { min: [0, 0, 0], max: [16, 8, 16] }]);
    // Only glow lichen burns (`FireBlock`: 15, 100); wet lichen doesn't.
    assert_eq!(GLOW_LICHEN.fire_odds(), (15, 100));
    assert_eq!(GLOW_LICHEN.with_water(true).fire_odds(), (0, 0));
    assert_eq!(SCULK_VEIN.fire_odds(), (0, 0));
    assert!(GLOW_LICHEN.flat_icon() && SCULK_VEIN.flat_icon() && !SCULK.flat_icon());
}

#[test]
fn mining_drops_and_experience_follow_java_loot() {
    use crate::item::ToolKind;
    assert_eq!((GLOW_LICHEN.hardness(), GLOW_LICHEN.best_tool()), (0.2, Some(ToolKind::Axe)));
    for (b, hardness) in
        [(SCULK, 0.2), (SCULK_VEIN, 0.2), (SCULK_SENSOR, 1.5), (SCULK_CATALYST, 3.0), (SCULK_SHRIEKER, 3.0)]
    {
        assert_eq!((b.hardness(), b.best_tool(), b.harvest_level()), (hardness, Some(ToolKind::Hoe), None));
        assert_eq!(b.drop(), None, "{} needs silk touch", b.name());
        assert_eq!(crate::mining::silk_drop(b), Some(Item::from_block(b)));
    }
    assert_eq!(crate::mining::silk_drop(GLOW_LICHEN), None, "lichen needs shears");
    let three = multiface(Multiface::GlowLichen, 0b10101, true);
    assert_eq!(sheared(three), Some(Stack::new(GLOW_LICHEN, 3)));
    assert_eq!(drop_count(multiface(Multiface::SculkVein, ALL_FACES, false)), 6);
    let mut rng = crate::entity::Rng::new(1);
    for (b, xp) in [(SCULK, 1), (SCULK_CATALYST, 5), (sculk_sensor(Phase::Active, true), 5), (SCULK_SHRIEKER, 5)] {
        assert_eq!(crate::mining::ore_xp(b, &mut rng), xp, "{}", b.name());
        let silk = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1);
        assert_eq!(crate::mining::mined_xp(b, silk, &mut rng), 0, "silk touch keeps the block instead");
    }
    assert_eq!(crate::mining::ore_xp(GLOW_LICHEN, &mut rng), 0);
}

#[test]
fn placement_adds_faces_in_look_order_where_there_is_support() {
    let at = IVec3::new(4, 140, 4);
    let get = |stone: &[IVec3], existing: Option<Block>| {
        let stone = stone.to_vec();
        move |p: IVec3| {
            Some(if stone.contains(&p) {
                Block::STONE
            } else if p == at {
                existing.unwrap_or(Block::AIR)
            } else {
                Block::AIR
            })
        }
    };
    // Looking north at a wall: the north face.
    let look = Vec3::new(0.0, -0.3, -1.0).normalize();
    let north = 1 << 2;
    let floor_and_wall = [at - IVec3::Y, at - IVec3::Z];
    assert_eq!(
        placed(GLOW_LICHEN, at, look, get(&floor_and_wall, None)),
        Some(Some(multiface(Multiface::GlowLichen, north, false)))
    );
    // That face taken, the next nearest looking direction with support (down).
    let existing = multiface(Multiface::GlowLichen, north, true);
    assert_eq!(
        placed(GLOW_LICHEN, at, look, get(&floor_and_wall, Some(existing))),
        Some(Some(multiface(Multiface::GlowLichen, north | DOWN, false)))
    );
    // No support anywhere: nothing to place.
    assert_eq!(placed(SCULK_VEIN, at, look, get(&[], None)), Some(None));
    // A vein doesn't merge into lichen: it looks for its own faces.
    assert_eq!(
        placed(SCULK_VEIN, at, look, get(&floor_and_wall, Some(existing))),
        Some(Some(multiface(Multiface::SculkVein, north, false)))
    );
    assert_eq!(placed(Block::STONE, at, look, get(&[], None)), None);
    // Glass and slabs aren't full faces toward the lichen... glass is.
    assert!(supports(Block::GLASS, 0));
    assert!(!supports(Block::TORCH, 0));
}

#[test]
fn faces_fall_off_when_their_support_goes_and_water_stays() {
    let mut w = world();
    let at = IVec3::new(4, 140, 4);
    w.set_block(at - IVec3::Y, Block::STONE);
    w.set_block(at + IVec3::X, Block::STONE);
    let east = 1 << 5;
    w.set_block(at, multiface(Multiface::GlowLichen, DOWN | east, true));
    w.set_block(at + IVec3::X, Block::AIR);
    assert_eq!(w.get_block(at), Some(multiface(Multiface::GlowLichen, DOWN, true)));
    w.set_block(at - IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(at), Some(Block::WATER), "a waterlogged lichen leaves its water");
    assert!(w.drops.is_empty(), "lost faces drop nothing");
}

#[test]
fn bone_meal_spreads_glow_lichen_but_not_veins() {
    let mut w = world();
    let at = IVec3::new(4, 140, 4);
    for x in 0..9 {
        for z in 0..9 {
            w.set_block(IVec3::new(x, 139, z), Block::STONE);
        }
    }
    w.set_block(at, GLOW_LICHEN);
    assert!(w.apply_bone_meal(at));
    let lichen: Vec<_> = (0..9)
        .flat_map(|x| (0..9).map(move |z| IVec3::new(x, 140, z)))
        .filter(|&p| w.get_block(p).is_some_and(|b| multiface_of(b).is_some()))
        .collect();
    assert_eq!(lichen.len(), 2, "one new lichen on the floor next to it: {lichen:?}");
    w.set_block(at + IVec3::Z * 3, SCULK_VEIN);
    assert!(!w.apply_bone_meal(at + IVec3::Z * 3), "sculk veins don't take bone meal");
}

#[test]
fn states_survive_a_save_and_reload() {
    use crate::world::storage::{LevelInfo, Storage};
    let dir = std::env::temp_dir().join(format!("voxelcraft-cave-blocks-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let mut chunk = ChunkData::Uniform(Block::STONE);
    let all = states();
    for (i, b) in all.iter().enumerate() {
        chunk.set(i % 32, i / 32, 3, *b);
    }
    let pos = IVec3::new(-1, -2, 3);
    storage.save(&LevelInfo { seed: 1, player: None, props: Default::default() }, &[(pos, Arc::new(chunk))]).unwrap();
    let loaded = storage.load_chunks().unwrap();
    for (i, b) in all.iter().enumerate() {
        assert_eq!(loaded[&pos].get(i % 32, i / 32, 3), *b, "#{}", b.0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
