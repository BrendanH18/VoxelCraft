use super::*;
use crate::world::terrain::{Dimension, Generation};
use glam::Vec3Swizzles;

const JAVA: &str = include_str!("../../../tests/fixtures/java/26.3/monument_layouts.txt");

fn facing_name(f: Facing) -> &'static str {
    match f {
        Facing::North => "north",
        Facing::South => "south",
        Facing::West => "west",
        Facing::East => "east",
    }
}

fn piece_name(k: PieceKind) -> String {
    match k {
        PieceKind::Entry => "OceanMonumentEntryRoom".into(),
        PieceKind::Core => "OceanMonumentCoreRoom".into(),
        PieceKind::DoubleX => "OceanMonumentDoubleXRoom".into(),
        PieceKind::DoubleXY => "OceanMonumentDoubleXYRoom".into(),
        PieceKind::DoubleY => "OceanMonumentDoubleYRoom".into(),
        PieceKind::DoubleYZ => "OceanMonumentDoubleYZRoom".into(),
        PieceKind::DoubleZ => "OceanMonumentDoubleZRoom".into(),
        PieceKind::Simple(d) => format!("OceanMonumentSimpleRoom {d}"),
        PieceKind::SimpleTop => "OceanMonumentSimpleTopRoom".into(),
        PieceKind::Wing(d) => format!("OceanMonumentWingRoom {d}"),
        PieceKind::Penthouse => "OceanMonumentPenthouse".into(),
    }
}

fn bounds(f: &Frame) -> String {
    format!("{} {} {} {} {} {}", f.min.x, f.min.y, f.min.z, f.max.x, f.max.y, f.max.z)
}

/// `tools/JavaMonumentLayouts.java` ran Java's own `MonumentBuilding` for
/// these starts: the orientation, the building box and every child piece
/// (kind, design, box, room and its final openings) must match.
#[test]
fn room_graphs_and_pieces_match_java() {
    let mut cases = 0;
    let mut lines = JAVA.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(head) = line.strip_prefix("case ") else { continue };
        let w: Vec<&str> = head.split(' ').collect();
        let seed = w[0].parse::<i64>().unwrap() as u64;
        let chunk = IVec2::new(w[1].parse().unwrap(), w[2].parse().unwrap());
        let l = layout(seed, chunk);
        assert_eq!(format!("{} {}", facing_name(l.building.orient), bounds(&l.building)), w[3..].join(" "), "{head}");
        let mut want = Vec::new();
        while let Some(next) = lines.peek().filter(|n| n.starts_with("  ")) {
            want.push(next.trim().to_string());
            lines.next();
        }
        let got: Vec<String> = l
            .pieces
            .iter()
            .map(|p| {
                let mut s = format!("{} {}", piece_name(p.kind), bounds(&p.frame));
                if let Some(r) = p.room {
                    let open: String = l.rooms[r].open.iter().map(|o| if *o { '1' } else { '0' }).collect();
                    s += &format!(" room {} {open}", l.rooms[r].index);
                }
                s
            })
            .collect();
        assert_eq!(got, want, "{head}");
        cases += 1;
    }
    assert_eq!(cases, 4);
}

/// The decoration random of the fixture's last two lines: Java's
/// `setDecorationSeed(12345, -64, 48)`, `setFeatureSeed(.., 24, 4)` and
/// its first draws.
#[test]
fn decoration_random_matches_java() {
    let line = |p: &str| JAVA.lines().find_map(|l| l.strip_prefix(p)).unwrap().to_string();
    let mut r = WorldgenRandom::new(0);
    let decoration = r.set_decoration_seed(12345, -64, 48);
    assert_eq!(decoration.to_string(), line("decoration "));
    r.set_feature_seed(decoration, FEATURE_INDEX, SURFACE_STRUCTURES);
    let mut got: Vec<String> = (0..6).map(|_| r.next_int(3).to_string()).collect();
    got.extend((0..6).map(|_| r.next_bool().to_string()));
    got.push(r.next_int(4).to_string());
    got.push(r.next_long().to_string());
    assert_eq!(got.join(" "), line("draws "));
}

fn generator(seed: u64) -> Generator {
    Generator::with_generation(seed, Dimension::Overworld, Generation::V5)
}

/// A monument built over real generation 5 terrain: three elder guardians
/// (both wings and the penthouse), the core room's eight gold blocks, sea
/// lanterns, the building's box, and nothing but monument blocks, water
/// and air.
#[test]
fn a_built_monument_has_its_core_elders_and_materials() {
    let g = generator(12345);
    let l = layout(g.seed, IVec2::new(5, -3));
    let b = build(&g, &l);
    assert_eq!(b.elders.len(), 3, "{:?}", b.elders);
    let penthouse = l.pieces.iter().find(|p| p.kind == PieceKind::Penthouse).unwrap();
    assert_eq!(b.elders.iter().filter(|e| penthouse.frame.bounds().contains(**e)).count(), 1);
    let count = |k: Block| b.blocks.iter().filter(|(_, x)| *x == k).count();
    assert_eq!(count(Block::GOLD_BLOCK), 8);
    assert!(count(LAMP) > 40);
    assert!(count(GRAY) > 1000 && count(LIGHT) > 1000 && count(BLACK) > 20);
    assert_eq!(b.bounds.max - b.bounds.min, IVec3::new(57, 22, 57));
    for (p, x) in &b.blocks {
        assert!(
            [GRAY, LIGHT, BLACK, LAMP, Block::GOLD_BLOCK, ob::WET_SPONGE, Block::WATER, Block::AIR, Block::ICE]
                .contains(x),
            "{p} {x:?}"
        );
    }
    // The building's floor is all monument blocks: nothing of the sea floor
    // shows through its ground course.
    let f = l.building;
    for x in 0..58 {
        for z in 0..58 {
            let p = f.world(x, 0, z);
            if let Ok(i) = b.blocks.binary_search_by_key(&(p.y, p.z, p.x), |(q, _)| (q.y, q.z, q.x)) {
                assert_ne!(b.blocks[i].1, Block::AIR, "{p}");
            }
        }
    }
}

/// Every chunk of the building writes only inside itself, so painting the
/// monument chunk by chunk can't depend on the order chunks generate in.
#[test]
fn every_write_stays_within_the_building_chunks() {
    let g = generator(42);
    let l = layout(g.seed, IVec2::new(0, 0));
    let b = build(&g, &l);
    let lo = l.building.min.xz().div_euclid(IVec2::splat(16)) * 16;
    let hi = l.building.max.xz().div_euclid(IVec2::splat(16)) * 16 + IVec2::splat(15);
    for (p, _) in &b.blocks {
        assert!(p.x >= lo.x && p.x <= hi.x && p.z >= lo.y && p.z <= hi.y, "{p}");
    }
    // The legs reach down from the floor.
    assert!(b.blocks.iter().any(|(p, x)| p.y < BASE_Y && *x == LIGHT));
}
