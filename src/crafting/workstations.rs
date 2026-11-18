//! v0.15 Track B recipes (Java 26.3 `data/minecraft/recipe`): `spyglass`,
//! `writable_book` and the pattern items (`<x>_banner_pattern`: paper and
//! the charge's ingredient). `creeper_banner_pattern` needs a creeper head,
//! which isn't registered (`registry_gaps.json`), so it has no recipe yet.
//! The special grid recipes (banner duplication, shield decoration, book
//! cloning, map extending) are in [`super::Grid::result_in`].

use super::{Recipe, b, ingredient, shaped, shapeless};
use crate::content::banner_patterns::{PATTERN_ITEMS, item as pattern};
use crate::item::Item;
use crate::world::block::Block;

pub(super) fn add(r: &mut Vec<Recipe>) {
    r.push(shaped(
        &["#", "X", "X"],
        &[('#', ingredient(Item::AMETHYST_SHARD)), ('X', ingredient(Item::COPPER_INGOT))],
        Item::SPYGLASS,
        1,
    ));
    r.push(shapeless(
        &[ingredient(Item::BOOK), ingredient(Item::INK_SAC), ingredient(Item::FEATHER)],
        Item::WRITABLE_BOOK,
        1,
    ));
    let charge = |java: &str| -> Option<Item> {
        Some(match java {
            "flower_banner_pattern" => b(crate::world::overworld_blocks::OXEYE_DAISY),
            "skull_banner_pattern" => Item::WITHER_SKULL,
            "mojang_banner_pattern" => Item::ENCHANTED_GOLDEN_APPLE,
            "field_masoned_banner_pattern" => b(Block::BRICKS),
            "bordure_indented_banner_pattern" => b(crate::world::overworld_blocks::VINE),
            _ => return None,
        })
    };
    for (i, p) in PATTERN_ITEMS.iter().enumerate() {
        if let Some(c) = charge(p.java) {
            r.push(shapeless(&[ingredient(Item::PAPER), ingredient(c)], pattern(i), 1));
        }
    }
}
