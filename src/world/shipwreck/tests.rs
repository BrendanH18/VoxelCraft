use super::templates::{Cell, PALETTES, resolve};
use super::*;
use crate::world::terrain::{Dimension, Generation};

fn generator(seed: u64) -> Generator {
    Generator::with_generation(seed, Dimension::Overworld, Generation::V5)
}

/// Every template keeps inside its box, holds a chest one under each of
/// Java's markers facing Java's way, and leaves the marker cell empty.
#[test]
fn templates_hold_java_chests_inside_java_sizes() {
    for (i, t) in TEMPLATES.iter().enumerate() {
        let cells = templates::cells(i as u8);
        let size = IVec3::from(t.size);
        assert!(cells.len() > 150, "{} has {} cells", t.name, cells.len());
        for (p, _) in cells {
            assert!(p.cmpge(IVec3::ZERO).all() && p.cmplt(size).all(), "{} {p}", t.name);
        }
        for &(_, at, facing) in t.markers {
            let at = IVec3::from(at);
            assert!(!cells.iter().any(|(p, _)| *p == at), "{} marker {at}", t.name);
            assert!(cells.contains(&(at - IVec3::Y, Cell::Chest(facing))), "{} chest under {at}", t.name);
        }
        let chests = cells.iter().filter(|(_, c)| matches!(c, Cell::Chest(_))).count();
        assert_eq!(chests, t.markers.len(), "{}", t.name);
    }
}

/// Degraded templates lose planks, halves keep the part of the ship whose
/// chests they hold.
#[test]
fn degraded_templates_lose_blocks() {
    for t in TEMPLATES.iter().filter(|t| t.name.ends_with("_degraded")) {
        let whole = template_index(t.name.trim_end_matches("_degraded")).unwrap();
        let degraded = template_index(t.name).unwrap();
        let (a, b) = (templates::cells(whole).len(), templates::cells(degraded).len());
        assert!(b < a * 95 / 100 && b > a * 70 / 100, "{}: {b} of {a}", t.name);
    }
}

/// Every cell resolves to a real block of each palette's woods.
#[test]
fn every_palette_resolves() {
    for palette in 0..PALETTES.len() as u8 {
        for i in 0..TEMPLATES.len() as u8 {
            for &(p, c) in templates::cells(i) {
                for r in Rotation::ALL {
                    let b = resolve(c, palette, r);
                    assert!(b != Block::STONE && b != Block::AIR, "{} {p} {c:?} {palette}", TEMPLATES[i as usize].name);
                }
            }
        }
    }
    assert!(crate::world::jigsaw::draw::missing_names().iter().all(|n| !n.contains("planks") && !n.contains("_log")));
    // The palette's woods: the hull and the trim of the first palette.
    let hull = resolve(Cell::Planks(templates::Wood::Hull), 2, Rotation::None);
    let trim = resolve(Cell::Planks(templates::Wood::Trim), 2, Rotation::None);
    assert_eq!(hull.name(), Block::from_name("dark_oak_planks").unwrap().name());
    assert_eq!(trim.name(), Block::from_name("jungle_planks").unwrap().name());
}

/// `StructureTemplate.transform` about the pivot (4, 0, 15).
#[test]
fn templates_turn_about_java_pivot() {
    let p = IVec3::new(1, 2, 3);
    assert_eq!(pivoted(Rotation::None, p), p);
    assert_eq!(pivoted(Rotation::Clockwise90, p), IVec3::new(4 + 15 - 3, 2, 15 - 4 + 1));
    assert_eq!(pivoted(Rotation::Clockwise180, p), IVec3::new(8 - 1, 2, 30 - 3));
    assert_eq!(pivoted(Rotation::Counterclockwise90, p), IVec3::new(4 - 15 + 3, 2, 4 + 15 - 1));
    assert_eq!(pivoted(Rotation::Clockwise90, PIVOT), PIVOT);
}

/// `Mth.getSeed` with Java's int overflow in `x * 3129871`.
#[test]
fn position_seed_overflows_like_java() {
    // (long)(1000 * 3129871 as int) ^ 7 * 116129781L ^ 64, squared and
    // mixed, then >> 16.
    let x = 1000i32.wrapping_mul(3_129_871) as i64;
    let s = x ^ (7i64 * 116_129_781) ^ 64;
    let want = s.wrapping_mul(s).wrapping_mul(42_317_861).wrapping_add(s.wrapping_mul(11)) >> 16;
    assert_eq!(position_seed(IVec3::new(1000, 64, 7)), want);
}

/// Shipwrecks generated over real generation 5 terrain: ocean wrecks use the
/// ocean list and sit on the floor, beached ones the beached list, every
/// chest is a chest of the right table inside the box.
#[test]
fn generated_wrecks_follow_java_lists_and_heights() {
    let g = generator(7);
    let (mut ocean, mut beached) = (0, 0);
    for rz in -6..6 {
        for rx in -6..6 {
            let chunk = g.temples.start_chunk(crate::world::temples::Kind::Shipwreck, IVec2::new(rx, rz));
            let Some(s) = start(&g, chunk) else { continue };
            let name = TEMPLATES[s.template as usize].name;
            let list: &[&str] = if s.beached { &BEACHED } else { &OCEAN };
            assert!(list.contains(&name), "{name}");
            let w = build(&g, s, 1);
            assert!(w.palette < 8);
            for (p, _) in &w.blocks {
                assert!(w.bounds.contains(*p), "{name} {p}");
            }
            assert_eq!(w.chests.len(), TEMPLATES[s.template as usize].markers.len());
            for (p, f) in &w.chests {
                assert!(w.blocks.iter().any(|(q, b)| q == p && crate::world::chest::is_chest(*b)), "{name} {p}");
                assert!(matches!(
                    f,
                    Feature::TempleChest(_, LootTable::ShipMap | LootTable::ShipSupply | LootTable::ShipTreasure)
                ));
            }
            if s.beached {
                beached += 1;
            } else {
                ocean += 1;
                // The mean `OCEAN_FLOOR_WG` height of the unturned footprint.
                let size = TEMPLATES[s.template as usize].size;
                let heights: Vec<i32> = (0..size[2])
                    .flat_map(|z| (0..size[0]).map(move |x| (x, z)))
                    .map(|(x, z)| g.structure_column(w.position.x + x, w.position.z + z).top(true) + 1)
                    .collect();
                let (lo, hi) = (*heights.iter().min().unwrap(), *heights.iter().max().unwrap());
                assert!((lo..=hi).contains(&w.position.y), "{name} at {} over {lo}..{hi}", w.position);
                assert_eq!(w.position.y, heights.iter().sum::<i32>() / heights.len() as i32);
            }
        }
    }
    assert!(ocean > 0, "no ocean wrecks around seed 7");
    let _ = beached;
}
