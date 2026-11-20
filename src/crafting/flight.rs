//! v0.16 Track B recipes (Java 26.3 `data/minecraft/recipe`):
//! `firework_rocket_simple.json` (paper and gunpowder make three rockets),
//! the sixteen `<colour>_shulker_box.json` `crafting_transmute` recipes
//! (any shulker box and a dye; the grid keeps the box's contents through
//! `content::shulker_box::transmute`), and `shulker_box.json` (two shulker
//! shells around a chest) once Track A's shulker shell is registered. The
//! special firework recipes (`firework_star`, `firework_star_fade`,
//! `firework_rocket`) are grid-only (`content::fireworks::grid_special`).

use super::{Recipe, b, color_ingredients, shaped, shapeless};
use crate::color::DyeColor;
use crate::content::fireworks::FIREWORK_ROCKET;
use crate::item::Item;
use crate::world::block::Block;
use crate::world::shulker_box::{COLOURS, FIRST};

/// Java's `#shulker_boxes`: the plain box, then `DyeColor` order.
static SHULKER_BOXES: [Item; 17] = {
    let mut out = [Item(0); 17];
    let mut i = 0;
    while i < COLOURS as usize {
        out[i] = b(Block(FIRST + i as u16 * 6));
        i += 1;
    }
    out
};

pub(super) fn add(r: &mut Vec<Recipe>) {
    static DYES: [[Item; 1]; 16] = color_ingredients(0);
    r.push(shapeless(&[&[Item::GUNPOWDER], &[Item::PAPER]], FIREWORK_ROCKET, 3));
    for (i, c) in DyeColor::ALL.into_iter().enumerate() {
        r.push(shapeless(&[&SHULKER_BOXES, &DYES[i]], crate::content::shulker_box::item(Some(c)), 1));
    }
    if let Some(shell) = shulker_shell() {
        static SHELL: std::sync::OnceLock<[Item; 1]> = std::sync::OnceLock::new();
        const CHEST: &[Item] = &[b(Block::CHEST)];
        let shell: &'static [Item] = SHELL.get_or_init(|| [shell]);
        r.push(shaped(&["-", "#", "-"], &[('-', shell), ('#', CHEST)], SHULKER_BOXES[0], 1));
    }
}

/// Track A's `shulker_shell`, when this build registers it.
pub fn shulker_shell() -> Option<Item> {
    Item::from_name("shulker_shell").filter(|i| i.block().is_none())
}
