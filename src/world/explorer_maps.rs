//! v0.15 Track F: the five explorer map items Java 26.3 added for
//! existing destinations (GEN-06): items 1629..=1633.
//!
//! Each map is an `exploration_map` modifier on its own item (Java 26.3
//! `ExplorationMapFunction`): its destination is a structure tag, its
//! decoration a `MapDecorationTypes` entry, zoom 2, and the search
//! `ChunkGenerator.findNearestMapStructure` with `skip_existing_chunks`:
//! starts already referenced by an earlier map are skipped, and the found
//! start gains a reference. The search walks rings of placement regions
//! outward from the origin's chunk and takes the first start on the first
//! ring that has one (x outer, z inner); the radius counts rings.
//!
//! Sources in Java 26.3: the woodland mansion map is the master
//! cartographer's `emerald_and_compass_woodland_mansion_map` trade (search
//! radius 100); every one of the five is in `chests/abandoned_camp_secret_chest`
//! (radius 50), whose camps are v0.16 content. No other trade or table
//! gives them (`tests/fixtures/java/26.3/mansions.json`).

use glam::{IVec2, IVec3};

use super::temples::Kind;
use super::terrain::Generator;
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

pub const FIRST: u16 = 1629;
pub const END: u16 = 1634;

pub const WOODLAND_MANSION_MAP: Item = Item(1629);
pub const DESERT_PYRAMID_MAP: Item = Item(1630);
pub const WARM_OCEAN_RUINS_MAP: Item = Item(1631);
pub const BURIED_MINESHAFT_MAP: Item = Item(1632);
pub const BURIED_ANCIENT_CITY_MAP: Item = Item(1633);

/// Texture layers 4056..=4065: the five item icons, then the five map
/// decorations, in item order. 4066..=4071 stay reserved.
pub const TEX: u16 = 4056;
pub const DECORATION_TEX: u16 = TEX + 5;
pub const TEX_END: u16 = TEX + 10;

/// One explorer map.
pub struct Map {
    pub item: Item,
    /// The item's registry name.
    pub java: &'static str,
    pub name: &'static str,
    /// The `exploration_map` destination tag (`#minecraft:<tag>`).
    pub destination: &'static str,
    /// The `MapDecorationTypes` id drawn at the target.
    pub decoration: &'static str,
}

pub const MAPS: [Map; 5] = [
    Map {
        item: WOODLAND_MANSION_MAP,
        java: "woodland_mansion_map",
        name: "woodland mansion map",
        destination: "on_woodland_mansion_maps",
        decoration: "mansion",
    },
    Map {
        item: DESERT_PYRAMID_MAP,
        java: "desert_pyramid_map",
        name: "desert pyramid map",
        destination: "on_desert_pyramid_maps",
        decoration: "desert_pyramid",
    },
    Map {
        item: WARM_OCEAN_RUINS_MAP,
        java: "warm_ocean_ruins_map",
        name: "warm ocean ruins map",
        destination: "on_ocean_ruin_warm_maps",
        decoration: "ocean_ruin_warm",
    },
    Map {
        item: BURIED_MINESHAFT_MAP,
        java: "buried_mineshaft_map",
        name: "buried mineshaft map",
        destination: "on_mineshaft_maps",
        decoration: "mineshaft",
    },
    Map {
        item: BURIED_ANCIENT_CITY_MAP,
        java: "buried_ancient_city_map",
        name: "buried ancient city map",
        destination: "on_ancient_city_maps",
        decoration: "ancient_city",
    },
];

pub fn get(item: Item) -> Option<&'static Map> {
    MAPS.iter().find(|m| m.item == item)
}

pub fn is_explorer(item: Item) -> bool {
    (FIRST..END).contains(&item.0)
}

pub fn items() -> impl Iterator<Item = Item> {
    (FIRST..END).map(Item)
}

pub fn item_info(item: Item) -> Option<ItemInfo> {
    let m = get(item)?;
    Some(ItemInfo { name: m.name, kind: ItemKind::Material, max_stack: 64, sprite: Sprite::Map(true) })
}

/// The item's inventory icon layer.
pub fn icon_layer(item: Item) -> Option<u16> {
    is_explorer(item).then(|| TEX + item.0 - FIRST)
}

/// The texture layer a map draws for decoration `kind`.
pub fn decoration_layer(kind: &str) -> Option<u16> {
    MAPS.iter().position(|m| m.decoration == kind).map(|i| DECORATION_TEX + i as u16)
}

/// The search radius in rings and the start chunk of the destination
/// found from `from`, skipping starts `known` already holds
/// (`skip_existing_chunks`). The map's target is the start chunk's
/// north-west corner (`StructurePlacement.getLocatePos`).
pub fn locate(
    g: &Generator,
    from: IVec3,
    item: Item,
    rings: i32,
    known: &dyn Fn(&str, IVec2) -> bool,
) -> Option<IVec2> {
    let m = get(item)?;
    let skip = |start: IVec2| known(m.destination, start);
    let t = &g.temples;
    match item {
        WOODLAND_MANSION_MAP => ring_search(from, rings, super::mansion::SPACING, &|r| {
            let s = t.start_chunk(Kind::Mansion, r);
            (super::mansion::start(g, s).is_some() && !skip(s)).then_some(s)
        }),
        DESERT_PYRAMID_MAP => ring_search(from, rings, 32, &|r| {
            let s = t.start_chunk(Kind::DesertPyramid, r);
            (t.generates(g, Kind::DesertPyramid, s) && !skip(s)).then_some(s)
        }),
        WARM_OCEAN_RUINS_MAP => ring_search(from, rings, 20, &|r| {
            let s = t.start_chunk(Kind::OceanRuin, r);
            (t.generates(g, Kind::OceanRuin, s) && t.ocean_ruin_is_warm(g, s) && !skip(s)).then_some(s)
        }),
        // Mineshafts are a spacing-1 spread: each ring is a ring of chunks.
        // Badlands chunks hold Java's `mineshaft_mesa`, which no map seeks.
        BURIED_MINESHAFT_MAP => ring_search(from, rings, 1, &|r| {
            let centre = r * 16 + IVec2::splat(8);
            (g.mineshafts.starts_at(r) && !g.column(centre.x, centre.y).biome.is_badlands() && !skip(r)).then_some(r)
        }),
        BURIED_ANCIENT_CITY_MAP => ring_search(from, rings, super::ancient_city::SPACING, &|r| {
            let s = g.ancient_cities.start_chunk(r);
            (g.ancient_cities.stub(g, r).is_some() && !skip(s)).then_some(s)
        }),
        _ => None,
    }
}

/// `ChunkGenerator.getNearestGeneratedStructure` over rings `0..=rings`
/// for one random-spread placement of `spacing` chunks.
fn ring_search(from: IVec3, rings: i32, spacing: i32, start: &dyn Fn(IVec2) -> Option<IVec2>) -> Option<IVec2> {
    let chunk = IVec2::new(from.x, from.z).div_euclid(IVec2::splat(16));
    for radius in 0..=rings {
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x.abs() != radius && z.abs() != radius {
                    continue;
                }
                let sector = chunk + IVec2::new(x, z) * spacing;
                if let Some(s) = start(sector.div_euclid(IVec2::splat(spacing))) {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Original 16×16 art: the five map icons (a folded map with a coloured
/// mark) and their map decorations.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    const MARKS: [[u8; 3]; 5] = [[86, 58, 36], [214, 180, 96], [60, 150, 170], [120, 120, 128], [32, 72, 84]];
    if layer < DECORATION_TEX {
        let mark = MARKS[(layer - TEX) as usize];
        if !(1..=14).contains(&x) || !(2..=13).contains(&y) {
            return [0, 0, 0, 0];
        }
        let fold = x == 5 || x == 10;
        let target = (6..=9).contains(&x) && (6..=9).contains(&y);
        let c = if target && (x + y).is_multiple_of(2) {
            mark
        } else if fold {
            [190, 180, 150]
        } else {
            [222, 212, 178]
        };
        return [c[0], c[1], c[2], 255];
    }
    let (x, y) = (x as i32, y as i32);
    let i = layer - DECORATION_TEX;
    let filled = match i {
        // A mansion: a wide house with a roof and a tower.
        0 => {
            (2..14).contains(&x) && (8..14).contains(&y)
                || (4..12).contains(&x) && (5..8).contains(&y) && (x - 8).abs() <= y - 3
        }
        // A pyramid.
        1 => (4..14).contains(&y) && (x - 8).abs() <= (y - 3).min(6),
        // A ruined arch over water.
        2 => {
            (3..13).contains(&x) && (11..14).contains(&y)
                || (x == 4 || x == 11) && (5..11).contains(&y)
                || (4..12).contains(&x) && y == 5
        }
        // A pickaxe.
        3 => (x - y).abs() <= 1 && (3..13).contains(&x) || y == 3 && (3..13).contains(&x) && (x - 8).abs() >= 1,
        // A city's portal frame.
        _ => (3..13).contains(&x) && (3..13).contains(&y) && !((5..11).contains(&x) && (5..13).contains(&y)),
    };
    if !filled {
        return [0; 4];
    }
    let c = MARKS[i as usize];
    [c[0], c[1], c[2], 255]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use glam::{IVec2, IVec3};
    use serde_json::Value;

    use super::*;
    use crate::world::maps::MapStorage;
    use crate::world::terrain::{Dimension, Generation};

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/mansions.json")).unwrap()
    }

    /// The five items exist under Java's names, are maps, and are the
    /// explorer maps Java's loot sources, structure tags and decorations name.
    #[test]
    fn maps_match_java_both_ways() {
        let f = fixture();
        let java: Vec<&str> = f["maps"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(java, MAPS.iter().map(|m| m.java).collect::<Vec<_>>());
        let clonable: BTreeSet<&str> =
            f["clonable_maps"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        for m in &MAPS {
            assert_eq!(crate::java_registry::item(m.java, ""), Some(m.item), "{}", m.java);
            assert_eq!(Item::from_name(m.java), Some(m.item));
            assert!(m.item.is_map() && clonable.contains(m.java));
            assert_eq!(m.item.name(), m.name);
            assert!(crate::world::maps::has_map_id(&crate::inventory::Stack {
                map_id: Some(3),
                ..crate::inventory::Stack::new(m.item, 1)
            }));
            assert!(!crate::world::maps::extendable(m.item), "only filled maps zoom out");
            let tag = &f["structure_tags"][m.destination];
            assert!(tag.is_array(), "{}", m.destination);
        }
        // Every loot entry giving one of them: its destination and decoration.
        let mut seen = BTreeSet::new();
        for (table, entries) in f["map_loot_sources"].as_object().unwrap() {
            assert_eq!(table, "chests/abandoned_camp_secret_chest", "a new loot source");
            for e in entries.as_array().unwrap() {
                let m = get(Item::from_name(e["item"].as_str().unwrap()).unwrap()).unwrap();
                let modifier =
                    e["modifier"].as_array().unwrap().iter().find(|x| x["type"] == "exploration_map").unwrap();
                assert_eq!(modifier["destination"].as_str().unwrap(), format!("#minecraft:{}", m.destination));
                // `ExplorationMapFunction.DEFAULT_DECORATION` is the mansion.
                assert_eq!(modifier.get("decoration").and_then(Value::as_str).unwrap_or("mansion"), m.decoration);
                assert_eq!(modifier.get("search_radius").and_then(Value::as_i64).unwrap_or(50), 50);
                seen.insert(m.java);
            }
        }
        assert_eq!(seen.len(), 5);
        // Only the mansion map is traded, at the master cartographer.
        let trades: Vec<&String> = f["map_trades"].as_object().unwrap().keys().collect();
        assert_eq!(trades, ["cartographer/5/emerald_and_compass_woodland_mansion_map"]);
        let trade = &f["map_trades"]["cartographer/5/emerald_and_compass_woodland_mansion_map"];
        assert_eq!(trade["gives"]["id"], "woodland_mansion_map");
        assert_eq!(trade["given_item_modifier"][0]["search_radius"], 100);
        assert!(
            f["cartographer_trade_tags"]["level_5"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t == "cartographer/5/emerald_and_compass_woodland_mansion_map")
        );
    }

    #[test]
    fn icons_and_decorations_have_their_own_layers() {
        let mut layers = BTreeSet::new();
        for m in &MAPS {
            let icon = m.item.icon_layer().unwrap();
            assert!((TEX..DECORATION_TEX).contains(&icon));
            let decoration = decoration_layer(m.decoration).unwrap();
            assert!((DECORATION_TEX..TEX_END).contains(&decoration));
            for layer in [icon, decoration] {
                assert!(layers.insert(layer));
                assert!((0..16).any(|y| (0..16).any(|x| pixel(layer, x, y)[3] == 255)), "{layer} is blank");
            }
        }
        assert!(TEX_END as u32 <= crate::world::block::tex::COUNT);
        assert_eq!(decoration_layer("monument"), None);
    }

    /// Each map finds its destination; a second map skips the start the
    /// first referenced (`skip_existing_chunks`), and the references and
    /// maps survive a save.
    #[test]
    fn maps_find_their_destinations_and_skip_known_starts() {
        let g = Generator::with_generation(42, Dimension::Overworld, Generation::V4);
        let mut storage = MapStorage::default();
        let from = IVec3::new(0, 70, 0);
        for m in &MAPS {
            let rings = if m.item == WOODLAND_MANSION_MAP { 100 } else { 50 };
            let first = storage.explorer(&g, from, m.item, rings).unwrap_or_else(|| panic!("no {}", m.java));
            let data = storage.maps[&first.map_id.unwrap()].clone();
            let d = data.decorations.iter().find(|d| d.kind == m.decoration).unwrap().clone();
            let corner = IVec2::new(d.x as i32, d.z as i32);
            let start = corner.div_euclid(IVec2::splat(16));
            assert_eq!(corner, start * IVec2::splat(16), "the target is the start chunk's corner");
            assert_eq!(data.scale, 2);
            assert!(storage.references.contains(&(m.destination.to_owned(), start.x, start.y)));
            // The start really is one.
            match m.item {
                WOODLAND_MANSION_MAP => assert!(crate::world::mansion::start(&g, start).is_some()),
                DESERT_PYRAMID_MAP => assert!(g.temples.generates(&g, Kind::DesertPyramid, start)),
                WARM_OCEAN_RUINS_MAP => {
                    assert!(g.temples.generates(&g, Kind::OceanRuin, start) && g.temples.ocean_ruin_is_warm(&g, start))
                }
                BURIED_MINESHAFT_MAP => assert!(g.mineshafts.starts_at(start)),
                _ => assert!(
                    g.ancient_cities
                        .stub(&g, start.div_euclid(IVec2::splat(crate::world::ancient_city::SPACING)))
                        .is_some()
                ),
            }
            let second = storage.explorer(&g, from, m.item, rings).unwrap();
            let d2 = storage.maps[&second.map_id.unwrap()].decorations.iter().find(|d| d.kind == m.decoration).unwrap();
            assert_ne!((d2.x, d2.z), (d.x, d.z), "{} pointed at a referenced start", m.java);
            // Without references the search finds the first start again.
            let plain = crate::world::maps::locate(&g, from, m.item, rings).unwrap();
            assert_eq!(IVec2::new(plain.x, plain.z), corner);
        }
        let loaded = MapStorage::load(&storage.serialize()).unwrap();
        assert_eq!(loaded.references, storage.references);
        assert_eq!(loaded.maps.len(), storage.maps.len());
        // An old save without references loads with none.
        let mut old: Value = serde_json::from_str(&storage.serialize()).unwrap();
        old.as_object_mut().unwrap().remove("references");
        assert!(MapStorage::load(&old.to_string()).unwrap().references.is_empty());
        // A map stack keeps its item and id through the stack codec.
        let stack =
            crate::inventory::Stack { map_id: Some(7), ..crate::inventory::Stack::new(WOODLAND_MANSION_MAP, 1) };
        let text = crate::inventory::stack_to_string(Some(stack));
        assert_eq!(crate::inventory::stack_from_str(&text), Some(Some(stack)));
    }

    /// Generations before 4 have no mansions, so the mansion map finds none there.
    #[test]
    fn the_mansion_map_finds_nothing_before_generation_4() {
        let g = Generator::with_generation(42, Dimension::Overworld, Generation::V3);
        assert!(locate(&g, IVec3::ZERO, WOODLAND_MANSION_MAP, 30, &|_, _| false).is_none());
    }
}
