//! v0.16 Track D (GEN-06): `abandoned_camp_map`, the explorer map only the
//! abandoned camps' common chests give.
//!
//! `chests/abandoned_camp_common_chest`'s third pool rolls one of eight
//! entries, each an `abandoned_camp_map` with `set_name` (its
//! `filled_map.<biome>_camp_map` name) and an `exploration_map` modifier
//! (decoration `abandoned_camp`, destination `#on_abandoned_camp_<biome>`,
//! Java's default zoom 2, search radius 50 and `skip_existing_chunks`), and
//! a condition that the chest is *not* in that biome. The eight destinations
//! are the bamboo jungle, cherry grove, birch forest, dappled forest, flower
//! forest, pale garden, swamp and windswept forest camps. A rolled map keeps
//! its destination in the stack's `entity_data` (the biome's index + 1)
//! until the map is drawn.

use glam::{IVec2, IVec3};

use super::temples::Kind;
use super::terrain::{Biome, Generator};
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

pub const CAMP_MAP: Item = Item(1708);
/// Texture layers 796 (the item icon) and 797 (the map decoration), the
/// first after Track D's block textures.
pub const TEX: u16 = 796;
pub const DECORATION_TEX: u16 = TEX + 1;
pub const TEX_END: u16 = TEX + 2;
const _: () = assert!(TEX == crate::world::gen5_blocks::t::END && TEX_END <= 949);

/// The `MapDecorationTypes` id drawn at the target.
pub const DECORATION: &str = "abandoned_camp";

/// The eight destinations, in the loot pool's order: the camp biome and
/// the map's name.
pub const DESTINATIONS: [(Biome, &str); 8] = [
    (Biome::BambooJungle, "Bamboo Camp Map"),
    (Biome::CherryGrove, "Cherry Grove Camp Map"),
    (Biome::BirchForest, "Birch Forest Camp Map"),
    (Biome::DappledForest, "Dappled Forest Camp Map"),
    (Biome::FlowerForest, "Flower Forest Camp Map"),
    (Biome::PaleGarden, "Pale Garden Camp Map"),
    (Biome::Swamp, "Swamp Camp Map"),
    (Biome::WindsweptForest, "Windswept Forest Camp Map"),
];

pub fn item_info(item: Item) -> Option<ItemInfo> {
    (item == CAMP_MAP).then_some(ItemInfo {
        name: "abandoned camp map",
        kind: ItemKind::Material,
        max_stack: 64,
        sprite: Sprite::Map(true),
    })
}

/// The destination index a tag names (`on_abandoned_camp_<biome>`).
pub fn destination_of_tag(tag: &str) -> Option<usize> {
    let biome = tag.trim_start_matches('#').trim_start_matches("minecraft:").strip_prefix("on_abandoned_camp_")?;
    DESTINATIONS.iter().position(|(b, _)| b.name() == biome)
}

/// The structure tag of destination `i` (the map references' key).
pub fn tag(i: usize) -> String {
    format!("on_abandoned_camp_{}", DESTINATIONS[i].0.name())
}

/// `findNearestMapStructure` for a camp of `biome`: rings of the camp
/// placement's regions out from `from`, the first start on the first ring
/// that has one, skipping starts `known` holds. Returns the start chunk.
pub fn locate(g: &Generator, from: IVec3, biome: Biome, rings: i32, known: &dyn Fn(IVec2) -> bool) -> Option<IVec2> {
    let t = &g.temples;
    super::explorer_maps::ring_search(from, rings, super::temples::camp_templates::SPACING, &|r| {
        let s = t.start_chunk(Kind::AbandonedCamp, r);
        (t.generates(g, Kind::AbandonedCamp, s)
            && super::temples::camp_templates::start_biome(g, s).is_some_and(|c| c.biome == biome)
            && t.camp_at(g, r)
            && !known(s))
        .then_some(s)
    })
}

/// Original 16×16 art: the map icon (a folded map marked with a tent) and
/// its decoration (a small tent).
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    const MARK: [u8; 3] = [168, 74, 40];
    let tent = |x: i32, y: i32, cx: i32, top: i32, h: i32| (top..top + h).contains(&y) && (x - cx).abs() <= y - top;
    let (xi, yi) = (x as i32, y as i32);
    if layer == TEX {
        if !(1..=14).contains(&x) || !(2..=13).contains(&y) {
            return [0, 0, 0, 0];
        }
        let c = if tent(xi, yi, 8, 5, 5) {
            if (xi - 8).abs() <= 0 && yi >= 8 { [60, 40, 30] } else { MARK }
        } else if x == 5 || x == 10 {
            [190, 180, 150]
        } else {
            [222, 212, 178]
        };
        return [c[0], c[1], c[2], 255];
    }
    if tent(xi, yi, 8, 4, 9) {
        let c = if (xi - 8).abs() <= 1 && yi >= 10 { [50, 30, 20] } else { MARK };
        return [c[0], c[1], c[2], 255];
    }
    [0, 0, 0, 0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_is_registered_and_its_destinations_match_the_loot_table() {
        assert_eq!(crate::java_registry::item("abandoned_camp_map", ""), Some(CAMP_MAP));
        assert!(CAMP_MAP.is_map());
        assert_eq!(CAMP_MAP.icon_layer(), Some(TEX));
        assert_eq!(crate::world::explorer_maps::decoration_layer(DECORATION), Some(DECORATION_TEX));
        let tables: serde_json::Value = serde_json::from_str(include_str!("loot_tables.json")).unwrap();
        let pool = &tables["tables"]["abandoned_camp_common_chest"]["pools"][2]["entries"];
        let entries = pool.as_array().unwrap();
        assert_eq!(entries.len(), DESTINATIONS.len());
        for (i, e) in entries.iter().enumerate() {
            assert_eq!(e["name"], "minecraft:abandoned_camp_map");
            let m = &e["modifier"];
            assert_eq!(m[1]["destination"], format!("#minecraft:{}", tag(i)));
            assert_eq!(m[1]["decoration"], "minecraft:abandoned_camp");
            assert_eq!(destination_of_tag(m[1]["destination"].as_str().unwrap()), Some(i));
            assert_eq!(
                e["condition"]["term"]["predicate"]["biomes"],
                format!("minecraft:{}", DESTINATIONS[i].0.name()),
                "not in the destination's own biome"
            );
            let key = m[0]["name"]["translate"].as_str().unwrap();
            let words: Vec<String> =
                key.trim_start_matches("filled_map.").split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect();
            assert_eq!(words.join(" "), DESTINATIONS[i].1);
        }
    }
}
