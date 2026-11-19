//! v0.15 Track F: woodland mansion checks against `tests/fixtures/java/26.3/mansions.json`.

use std::collections::{BTreeMap, BTreeSet};

use glam::{IVec2, IVec3};
use serde_json::Value;

use super::*;
use crate::world::temples::Kind;
use crate::world::terrain::{Biome, Generation, Generator};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/mansions.json")).unwrap()
}

fn v4(seed: u64) -> Generator {
    Generator::with_generation(seed, Dimension::Overworld, Generation::V4)
}

/// Every block a template names exists in the game.
#[test]
fn rooms_resolve_every_block() {
    assert_eq!(rooms::unresolved(), Vec::<&str>::new());
}

/// Every template's size, stored-cell flag and data markers equal Java's,
/// with no template missing on either side.
#[test]
fn template_facts_match_java_both_ways() {
    let f = fixture();
    let java = f["templates"].as_object().unwrap();
    let ours: BTreeSet<&str> = TEMPLATES.iter().map(|t| t.name).collect();
    let theirs: BTreeSet<&str> = java.keys().map(String::as_str).collect();
    assert_eq!(ours, theirs);
    for t in &TEMPLATES {
        let j = &java[t.name];
        let size: Vec<i64> = j["size"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
        assert_eq!(size, t.size.iter().map(|&v| v as i64).collect::<Vec<_>>(), "{}", t.name);
        assert_eq!(j["filled"].as_bool().unwrap(), t.filled, "{}", t.name);
        let mut theirs: Vec<(String, Vec<i64>)> = j["markers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["marker"].as_str().unwrap().to_owned(),
                    m["pos"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect(),
                )
            })
            .collect();
        let mut ours: Vec<(String, Vec<i64>)> =
            t.markers.iter().map(|(m, p)| (m.java().to_owned(), p.iter().map(|&v| v as i64).collect())).collect();
        theirs.sort();
        ours.sort();
        assert_eq!(ours, theirs, "{}", t.name);
        // The built template has the same size and markers.
        let built = template(template_index(t.name).unwrap());
        assert_eq!(built.size, IVec3::from(t.size));
        assert_eq!(built.markers().len(), t.markers.len());
    }
}

/// The spawner and the fixed chests sit where Java's do with Java's contents.
#[test]
fn fixed_containers_match_java_both_ways() {
    let f = fixture();
    let mut theirs = BTreeMap::new();
    for (name, t) in f["templates"].as_object().unwrap() {
        for c in t["containers"].as_array().unwrap() {
            let pos: Vec<i64> = c["pos"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
            let what = if let Some(s) = c.get("spawner") {
                format!("spawner {}", s["entity"].as_str().unwrap())
            } else {
                let items: Vec<String> = c["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| {
                        let ench = i.get("enchantments").map_or(0, |e| e["minecraft:efficiency"].as_i64().unwrap());
                        format!("{}:{}:{}:{}", i["slot"], i["id"].as_str().unwrap(), i["count"], ench)
                    })
                    .collect();
                format!("{} {}", c["block"].as_str().unwrap(), items.join(","))
            };
            theirs.insert((name.clone(), pos), what);
        }
    }
    let mut ours = BTreeMap::new();
    for (i, fx) in FIXED.iter().enumerate() {
        let what = match fx.contents {
            Fixed::Spawner(kind) => {
                assert_eq!(kind, crate::entity::MobKind::Spider);
                "spawner spider".to_owned()
            }
            Fixed::Chest { trapped, items } => {
                let chest = rooms::fixed_chest(i as u8);
                for &(slot, id, count, _) in items {
                    let s = chest.slots[slot as usize].expect("a fixed stack resolves");
                    assert_eq!((s.item, s.count), (crate::java_registry::item(id, "").unwrap(), count));
                }
                let items: Vec<String> =
                    items.iter().map(|(slot, id, count, ench)| format!("{slot}:{id}:{count}:{ench}")).collect();
                format!("{} {}", if trapped { "trapped_chest" } else { "chest" }, items.join(","))
            }
        };
        ours.insert((fx.template.to_owned(), fx.pos.iter().map(|&v| v as i64).collect::<Vec<_>>()), what);
        // The template holds the container's block there.
        let t = template(template_index(fx.template).unwrap());
        let cell = t.cell(IVec3::from(fx.pos)).expect("a stored container cell").block;
        match fx.contents {
            Fixed::Spawner(_) => assert_eq!(cell, Block::SPAWNER),
            _ => assert!(crate::world::chest::is_chest(cell), "{}", fx.template),
        }
    }
    assert_eq!(ours, theirs);
}

/// `StructureSets`, `Structures` and `WoodlandMansionStructure` constants.
#[test]
fn placement_matches_java() {
    let f = fixture();
    let set = &f["structure_set"];
    assert_eq!(set["spacing"], SPACING);
    assert_eq!(set["separation"], SEPARATION);
    assert_eq!(set["salt"], SALT);
    assert_eq!(set["spread_type"], "triangular");
    assert_eq!(f["generation_point"]["start_offset_in_chunk"], START_OFFSET);
    assert_eq!(f["generation_point"]["min_start_y"], MIN_START_Y);
    assert_eq!(f["structure"]["terrain_adaptation"], "none");
    assert_eq!(f["structure"]["spawn_overrides"], serde_json::json!({}));
    let tag: BTreeSet<&str> = f["biome_tag"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
    let ours: BTreeSet<&str> = Biome::ALL.iter().filter(|b| allowed(**b)).map(|b| b.name()).collect();
    assert_eq!(ours, tag);
    // The start chunk is a triangular spread: the mean of two draws.
    let g = v4(42);
    for region in [IVec2::new(0, 0), IVec2::new(-3, 7), IVec2::new(11, -2)] {
        let seed = 42u64
            .wrapping_add((region.x as i64).wrapping_mul(341_873_128_712) as u64)
            .wrapping_add((region.y as i64).wrapping_mul(132_897_987_541) as u64)
            .wrapping_add(10_387_319);
        let mut r = JavaRandom::new(seed as i64);
        let span = 80 - 20;
        let ox = (r.next_bounded(span) + r.next_bounded(span)) / 2;
        let oz = (r.next_bounded(span) + r.next_bounded(span)) / 2;
        assert_eq!(g.temples.start_chunk(Kind::Mansion, region), region * 80 + IVec2::new(ox, oz));
    }
}

fn layout_hash(pieces: &[Piece]) -> u64 {
    let names = |r: Rotation| match r {
        Rotation::None => "NONE",
        Rotation::Clockwise90 => "CLOCKWISE_90",
        Rotation::Clockwise180 => "CLOCKWISE_180",
        Rotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
    };
    let mirror = |m: Mirror| match m {
        Mirror::None => "NONE",
        Mirror::LeftRight => "LEFT_RIGHT",
        Mirror::FrontBack => "FRONT_BACK",
    };
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in pieces {
        let line =
            format!("{} {} {} {} {} {}\n", p.name(), p.pos.x, p.pos.y, p.pos.z, names(p.rotation), mirror(p.mirror));
        for b in line.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

/// The layout of four fixed starts, pinned piece by piece. The expected
/// hashes come from an independent transliteration of Java 26.3's
/// `WoodlandMansionPieces` (rotation, grid, room and piece order), so a
/// wrong constant, direction or random draw changes them.
#[test]
fn golden_mansion_layouts() {
    type Case = (i64, IVec2, i32, Rotation, usize, usize, u64, [&'static str; 11]);
    let cases: [Case; 4] = [
        (
            42,
            IVec2::new(10, 20),
            70,
            Rotation::None,
            511,
            8,
            0xe3ad_608f_5b6a_93e3,
            [
                "55555555555",
                "55555555555",
                "00221111155",
                "00221112155",
                "00221223355",
                "00221223355",
                "22111112155",
                "22122211155",
                "22122222200",
                "55555555555",
                "55555555555",
            ],
        ),
        (
            12345,
            IVec2::new(-37, 81),
            66,
            Rotation::Clockwise90,
            544,
            8,
            0x6be7_5a85_972a_d872,
            [
                "55555555555",
                "55555555555",
                "11111111155",
                "12222112155",
                "11122223355",
                "22221223355",
                "22221112155",
                "00021111155",
                "00022222200",
                "55555555555",
                "55555555555",
            ],
        ),
        (
            7,
            IVec2::new(3, -5),
            90,
            Rotation::Counterclockwise90,
            579,
            9,
            0xf390_2088_3655_3608,
            [
                "55555555555",
                "55555555555",
                "22221111155",
                "22221112155",
                "11111223355",
                "12222223355",
                "12222112155",
                "20021111155",
                "00022222200",
                "55555555555",
                "55555555555",
            ],
        ),
        (
            -8_000_000_000,
            IVec2::new(1000, -1000),
            64,
            Rotation::Clockwise180,
            538,
            9,
            0xc599_920d_038b_53ad,
            [
                "55555555555",
                "55555555555",
                "11111111155",
                "12222112155",
                "11121223355",
                "22221223355",
                "22222112155",
                "11111111155",
                "22222222200",
                "55555555555",
                "55555555555",
            ],
        ),
    ];
    for (seed, chunk, y, rotation, count, mirrored, hash, rows) in cases {
        let seed = seed as u64;
        let mut r = large_feature_random(seed, chunk);
        assert_eq!(Rotation::ALL[r.next_bounded(4) as usize], rotation, "seed {seed}");
        let grid = Grid::new(&mut r);
        for (y, row) in rows.iter().enumerate() {
            let ours: String = (0..11).map(|x| char::from(b'0' + grid.base.get(x, y as i32) as u8)).collect();
            assert_eq!(&ours, row, "seed {seed} row {y}");
        }
        let origin = IVec3::new(chunk.x * 16 + START_OFFSET, y, chunk.y * 16 + START_OFFSET);
        let pieces = pieces(seed, &Start { chunk, origin, rotation });
        assert_eq!(pieces.len(), count, "seed {seed}");
        assert_eq!(pieces.iter().filter(|p| p.mirror != Mirror::None).count(), mirrored, "seed {seed}");
        assert_eq!(pieces[0].name(), "entrance");
        assert_eq!(pieces[0].pos, origin + rotation.facing(Facing::West).offset() * 9);
        assert_eq!(layout_hash(&pieces), hash, "seed {seed}");
    }
}

/// Java's transform helpers on hand-worked cases.
#[test]
fn transforms_match_java() {
    let p = IVec3::new(2, 1, 5);
    assert_eq!(transform(p, Mirror::None, Rotation::Clockwise90), IVec3::new(-5, 1, 2));
    assert_eq!(transform(p, Mirror::LeftRight, Rotation::None), IVec3::new(2, 1, -5));
    assert_eq!(transform(p, Mirror::FrontBack, Rotation::Counterclockwise90), IVec3::new(5, 1, 2));
    assert_eq!(Rotation::Clockwise90.then(Rotation::Counterclockwise90), Rotation::None);
    assert_eq!(Rotation::Clockwise180.then(Rotation::Clockwise90), Rotation::Counterclockwise90);
    assert_eq!(Rotation::Clockwise90.facing(Facing::North), Facing::East);
    assert_eq!(transform_facing(Facing::North, Mirror::LeftRight, Rotation::None), Facing::South);
    assert_eq!(transform_facing(Facing::East, Mirror::FrontBack, Rotation::Clockwise90), Facing::North);
    // `getZeroPositionWithTransform((1, 0, 0), NONE, rotation, 7, 7)`.
    let zero = IVec3::new(1, 0, 0);
    assert_eq!(zero_position(zero, Mirror::None, Rotation::None, 7, 7), IVec3::new(1, 0, 0));
    assert_eq!(zero_position(zero, Mirror::None, Rotation::Clockwise90, 7, 7), IVec3::new(7, 0, 0));
    assert_eq!(zero_position(zero, Mirror::None, Rotation::Clockwise180, 7, 7), IVec3::new(7, 0, 6));
    assert_eq!(zero_position(zero, Mirror::None, Rotation::Counterclockwise90, 7, 7), IVec3::new(1, 0, 6));
    // Pieces round-trip between template and world cells.
    for rotation in Rotation::ALL {
        for mirror in [Mirror::None, Mirror::LeftRight, Mirror::FrontBack] {
            let piece = Piece { template: 0, pos: IVec3::new(100, 64, -40), rotation, mirror };
            let b = piece.bounds();
            for local in [IVec3::ZERO, IVec3::new(6, 7, 6), IVec3::new(3, 2, 1)] {
                let w = piece.world(local);
                assert_eq!(piece.local(w), local);
                assert!(b.contains(w));
            }
        }
    }
}

/// The extractor's `FAMILIES`: block families that tell room kinds apart,
/// by Java registry id.
fn families(id: &str) -> Vec<&'static str> {
    type Rule = (&'static str, fn(&str) -> bool);
    let rules: [Rule; 33] = [
        ("anvil", |n| n.ends_with("anvil")),
        ("banner", |n| n.ends_with("_banner")),
        ("bookshelf", |n| n == "bookshelf"),
        ("carpet", |n| n.ends_with("_carpet")),
        ("carved_pumpkin", |n| n == "carved_pumpkin"),
        ("cauldron", |n| n.ends_with("cauldron")),
        ("chest", |n| n == "chest"),
        ("cobweb", |n| n == "cobweb"),
        ("crops", |n| n == "farmland" || n == "wheat"),
        ("diamond_block", |n| n == "diamond_block"),
        ("flower_pot", |n| n.starts_with("potted_")),
        ("glass", |n| n == "glass" || n == "glass_pane"),
        ("infested", |n| n.starts_with("infested_")),
        ("iron_bars", |n| n == "iron_bars"),
        ("iron_door", |n| n == "iron_door"),
        ("ladder", |n| n == "ladder"),
        ("lapis_block", |n| n == "lapis_block"),
        ("lava", |n| n == "lava"),
        ("leaves", |n| n.ends_with("_leaves")),
        ("lever", |n| n == "lever"),
        ("mushroom", |n| n.ends_with("_mushroom")),
        ("obsidian", |n| n == "obsidian"),
        ("pumpkin_melon", |n| n == "pumpkin" || n == "melon" || n.ends_with("_stem")),
        ("rail", |n| n == "rail"),
        ("redstone", |n| n == "redstone_wire"),
        ("sapling", |n| n.ends_with("_sapling")),
        ("spawner", |n| n == "spawner"),
        ("tnt", |n| n == "tnt"),
        ("torch", |n| n.ends_with("torch")),
        ("trapped_chest", |n| n == "trapped_chest"),
        ("vine", |n| n == "vine"),
        ("water", |n| n == "water" || n == "water_cauldron"),
        ("wool", |n| n.ends_with("_wool")),
    ];
    rules.iter().filter(|(_, test)| test(id)).map(|(f, _)| *f).collect()
}

/// Each original room is the kind of room Java's template of its name is:
/// it has every block family Java's has (bar the infested blocks, a
/// v0.16 gap), and none of the families that mark a different kind.
#[test]
fn rooms_are_javas_room_kinds() {
    let f = fixture();
    const SIGNATURE: [&str; 15] = [
        "anvil",
        "bookshelf",
        "carved_pumpkin",
        "cobweb",
        "crops",
        "diamond_block",
        "iron_door",
        "lapis_block",
        "lava",
        "mushroom",
        "obsidian",
        "pumpkin_melon",
        "rail",
        "spawner",
        "tnt",
    ];
    for t in &TEMPLATES {
        let built = template(template_index(t.name).unwrap());
        let mut ours = BTreeSet::new();
        for y in 0..built.size.y {
            for z in 0..built.size.z {
                for x in 0..built.size.x {
                    if let Some(c) = built.cell(IVec3::new(x, y, z)) {
                        ours.extend(families(c.id));
                    }
                }
            }
        }
        if t.markers.iter().any(|m| m.0.chest().is_some()) {
            ours.insert("chest");
        }
        let java: BTreeSet<&str> =
            f["templates"][t.name]["families"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        for fam in &java {
            if *fam == "infested" {
                continue;
            }
            assert!(ours.contains(fam), "{}: Java's room has {fam}, ours {:?}", t.name, ours);
        }
        for fam in SIGNATURE {
            assert!(!ours.contains(fam) || java.contains(fam), "{}: {fam} marks another room kind", t.name);
        }
    }
}

/// Mobs spawn on the marker cell with headroom; chest markers face as Java
/// turns them (by the piece's rotation, not its mirror).
#[test]
fn markers_leave_room_for_their_mobs() {
    for t in &TEMPLATES {
        let built = template(template_index(t.name).unwrap());
        for &(m, p) in built.markers() {
            assert!(built.cell(p).is_none(), "{} {m:?}: the marker cell is not stored", t.name);
            if m.chest().is_none() {
                let above = built.cell(p + IVec3::Y).map(|c| c.block);
                assert!(above.is_none_or(|b| !b.is_solid()), "{} {m:?} at {p}: {above:?} above", t.name);
            }
        }
    }
    let piece = Piece {
        template: template_index("1x1_a4").unwrap(),
        pos: IVec3::ZERO,
        rotation: Rotation::Clockwise90,
        mirror: Mirror::LeftRight,
    };
    let (m, _) = template(piece.template).markers()[0];
    assert_eq!(m, Marker::ChestWest);
    assert_eq!(m.block(piece.rotation), Block::CHEST.with_facing(Facing::North));
}

/// The first start found in `regions` of a generation 4 world.
fn find_start(g: &Generator) -> Start {
    for r in 0..40i32 {
        for z in -r..=r {
            for x in -r..=r {
                if x.abs() != r && z.abs() != r {
                    continue;
                }
                let chunk = g.temples.start_chunk(Kind::Mansion, IVec2::new(x, z));
                if let Some(s) = start(g, chunk) {
                    return s;
                }
            }
        }
    }
    panic!("no woodland mansion near the origin of seed {}", g.seed);
}

/// Mansions start only in generation 4, in dark forests and pale gardens,
/// at y 60 or above, and `/locate` finds them.
#[test]
fn mansions_start_only_in_generation_4() {
    let g = v4(42);
    let s = find_start(&g);
    assert!(s.origin.y >= MIN_START_Y);
    assert!(allowed(g.biome_at(s.origin)));
    assert_eq!(s.origin.x, s.chunk.x * 16 + START_OFFSET);
    for generation in [Generation::V1, Generation::V2, Generation::V3] {
        let old = Generator::with_generation(42, Dimension::Overworld, generation);
        assert_eq!(start(&old, s.chunk), None, "{generation:?}");
        assert!(old.temples.nearest(&old, Kind::Mansion, IVec2::new(s.origin.x, s.origin.z), 200).is_none());
    }
    let found = g.temples.nearest(&g, Kind::Mansion, IVec2::new(s.origin.x, s.origin.z), 200).unwrap();
    assert_eq!(IVec2::new(found.x, found.z), s.chunk * 16 + IVec2::splat(8));
    assert_eq!(Kind::from_name("minecraft:woodland_mansion"), Some(Kind::Mansion));
    assert_eq!(Kind::from_name("mansion"), Some(Kind::Mansion));
}

/// A generated mansion: its chunks hold its blocks, foundation, chests,
/// spawner and residents, built once and cached.
#[test]
fn a_generated_mansion_has_its_rooms_chests_and_residents() {
    let g = v4(42);
    let s = find_start(&g);
    let built = g.temples.near(&g, s.origin, 0).into_iter().find(|b| b.kind == Kind::Mansion).expect("cached mansion");
    let layout = built.mansion.clone().unwrap();
    assert_eq!(layout.start, s);
    assert_eq!(layout.pieces.len(), pieces(g.seed, &s).len());
    // Cached: a second lookup returns the same build.
    let again = g.temples.near(&g, s.origin, 0).into_iter().find(|b| b.kind == Kind::Mansion).unwrap();
    assert!(std::sync::Arc::ptr_eq(&built, &again));
    // Residents come from the markers: evokers, vindicators and allays.
    let evokers = layout.residents.iter().filter(|r| r.0 == MobKind::Evoker).count();
    let vindicators = layout.residents.iter().filter(|r| r.0 == MobKind::Vindicator).count();
    let marked = |m: Marker| {
        layout
            .pieces
            .iter()
            .map(|p| template(p.piece.template).markers().iter().filter(|x| x.0 == m).count())
            .sum::<usize>()
    };
    assert_eq!(evokers, marked(Marker::Mage));
    assert_eq!(vindicators, marked(Marker::Warrior));
    let allays = layout.residents.iter().filter(|r| r.0 == MobKind::Allay).count();
    assert!((marked(Marker::Allays)..=marked(Marker::Allays) * 3).contains(&allays));
    // Painted chunks hold the chests the features name, and air where mobs stand.
    // Engine chunks are 32 blocks a side.
    let chunk_of = crate::world::chunk::chunk_of;
    let local_of = crate::world::chunk::local_of;
    let mut chests = 0usize;
    for (p, f) in &layout.features {
        let data = g.generate(chunk_of(*p));
        let l = local_of(*p);
        let b = data.get(l.x as usize, l.y as usize, l.z as usize);
        match f {
            crate::world::fortress::Feature::TempleChest(_, table) => {
                assert_eq!(*table, crate::world::temples::LootTable::Mansion);
                assert!(crate::world::chest::is_chest(b), "{p}: {b:?}");
                assert!(g.structure_features(chunk_of(*p)).iter().any(|(q, g)| q == p && g == f));
                chests += 1;
            }
            crate::world::fortress::Feature::Spawner(kind) => {
                assert_eq!(*kind, MobKind::Spider);
                assert_eq!(b, Block::SPAWNER);
            }
            crate::world::fortress::Feature::MansionChest(_) => assert!(crate::world::chest::is_chest(b)),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(
        chests,
        [Marker::ChestNorth, Marker::ChestSouth, Marker::ChestEast, Marker::ChestWest]
            .map(marked)
            .iter()
            .sum::<usize>()
    );
    for (kind, p) in &layout.residents {
        let data = g.generate(chunk_of(*p));
        let l = local_of(*p);
        assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize), Block::AIR, "{kind:?} at {p}");
        assert!(layout.bounds.contains(*p));
    }
    // The entrance stands on the first floor; a cobblestone foundation fills
    // down to the ground where the floor overhangs it.
    let entrance = layout.pieces[0];
    assert_eq!(entrance.piece.name(), "entrance");
    assert_eq!(layout.floor_y, s.origin.y);
    let mut foundations = 0;
    for z in layout.bounds.min.z..=layout.bounds.max.z {
        for x in layout.bounds.min.x..=layout.bounds.max.x {
            if let Some((low, high)) = layout.foundation_at(x, z) {
                assert!(low <= high && high == s.origin.y - 1);
                foundations += 1;
                if foundations == 1 {
                    let p = IVec3::new(x, low, z);
                    let data = g.generate(chunk_of(p));
                    let l = local_of(p);
                    assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize), Block::COBBLESTONE);
                }
            }
        }
    }
    println!("mansion at {:?}: {} pieces, {} foundation columns", s.origin, layout.pieces.len(), foundations);
}

/// Generation cost: laying out a mansion and generating its chunks, for
/// the record (`cargo test --release mansion_generation_cost -- --ignored --nocapture`).
#[test]
#[ignore]
fn mansion_generation_cost() {
    let g = v4(42);
    let s = find_start(&g);
    let t = std::time::Instant::now();
    let layout = Layout::build(&g, s);
    let build = t.elapsed();
    let b = layout.bounds;
    let (lo, hi) = (crate::world::chunk::chunk_of(b.min), crate::world::chunk::chunk_of(b.max));
    let chunks: Vec<IVec3> = (lo.y..=hi.y)
        .flat_map(|y| (lo.z..=hi.z).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec3::new(x, y, z))))
        .collect();
    let t = std::time::Instant::now();
    for &c in &chunks {
        std::hint::black_box(g.generate(c));
    }
    let with = t.elapsed();
    let plain = Generator::with_generation(42, Dimension::Overworld, Generation::V3);
    let t = std::time::Instant::now();
    for &c in &chunks {
        std::hint::black_box(plain.generate(c));
    }
    let without = t.elapsed();
    let mut paint = std::time::Duration::ZERO;
    for &c in &chunks {
        let mut blocks = Box::new([Block::AIR; crate::world::chunk::CHUNK_VOLUME]);
        let t = std::time::Instant::now();
        layout.paint(&mut blocks, c * crate::world::chunk::CHUNK_SIZE_I);
        paint += t.elapsed();
    }
    println!(
        "mansion {:?}: {} pieces, layout {:.1} ms; {} chunks: generation 4 {:.1} ms, generation 3 {:.1} ms, paint alone {:.2} ms ({:.3} ms per chunk)",
        s.origin,
        layout.pieces.len(),
        build.as_secs_f64() * 1e3,
        chunks.len(),
        with.as_secs_f64() * 1e3,
        without.as_secs_f64() * 1e3,
        paint.as_secs_f64() * 1e3,
        paint.as_secs_f64() * 1e3 / chunks.len() as f64,
    );
}
