//! Recipes of the v0.12 wood track (`world::wood`), from Java 26.3's
//! `data/minecraft/recipe` files.

use super::{PLANKS, Recipe, STICK, WOOD_SLAB, b, ingredient, shaped, shapeless};
use crate::world::block::{Block, Facing};
use crate::world::wood::{self, Kind, Pillar, bookshelf, shelves, signs, switches};

pub(super) fn add(r: &mut Vec<Recipe>) {
    // `oak_wood.json` and the rest: four logs make three wood; the same for
    // stripped logs and stripped wood (group `bark`).
    for w in 0..9u16 {
        let log = ingredient(b(wood::LOGS[w as usize]));
        r.push(shaped(&["##", "##"], &[('#', log)], b(wood::pillar(w, Pillar::Wood, 0)), 3));
        let stripped = ingredient(b(wood::pillar(w, Pillar::StrippedLog, 0)));
        r.push(shaped(&["##", "##"], &[('#', stripped)], b(wood::pillar(w, Pillar::StrippedWood, 0)), 3));
    }
    // Trapdoors (2), buttons and pressure plates of the seven woods, and the
    // polished blackstone button and plate (`spruce_trapdoor.json` and so on).
    for f in 0..switches::FAMILIES {
        let mat = ingredient(b(switches::material(f)));
        if f < 7 {
            r.push(shaped(&["###", "###"], &[('#', mat)], b(switches::trapdoor(f)), 2));
        }
        r.push(shapeless(&[mat], b(switches::button(f)), 1));
        r.push(shaped(&["##"], &[('#', mat)], b(switches::plate(f)), 1));
    }
    // Signs (3), hanging signs (6) and shelves (6) of every wood
    // (`oak_sign.json`, `oak_hanging_sign.json`, `oak_shelf.json`).
    let chain = ingredient(b(Block::CHAIN));
    for kind in Kind::ALL {
        let planks = ingredient(b(kind.planks()));
        let stripped = ingredient(b(kind.stripped_log()));
        let sign = |mount| signs::Sign { kind, mount, wet: false }.block();
        r.push(shaped(
            &["###", "###", " X "],
            &[('#', planks), ('X', STICK)],
            b(sign(signs::Mount::Standing { rotation: 0 })),
            3,
        ));
        r.push(shaped(
            &["X X", "###", "###"],
            &[('#', stripped), ('X', chain)],
            b(sign(signs::Mount::Hanging { rotation: 0, attached: false })),
            6,
        ));
        let shelf = shelves::Shelf {
            kind,
            facing: Facing::North,
            powered: false,
            chain: shelves::Chain::Unconnected,
            wet: false,
        };
        r.push(shaped(&["###", "   ", "###"], &[('#', stripped)], b(shelf.block()), 6));
    }
    // `chiseled_bookshelf.json`: planks around a row of wooden slabs.
    r.push(shaped(&["###", "XXX", "###"], &[('#', PLANKS), ('X', WOOD_SLAB)], b(bookshelf::CHISELED_BOOKSHELF), 1));
}
