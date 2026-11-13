//! Cheap guards on the saved ID spaces, for content added in parallel:
//! block texture layers, non-block item ids and the append-only orderings
//! whose positions saves or the LAN protocol carry. `examples/id_map.rs`
//! prints the map these protect. The block registry is a function of the
//! state id, so a state can't be registered twice; the lists that can hold
//! an entry twice are checked here.
use std::collections::BTreeSet;

use voxelcraft::{
    enchant::{Enchantment, Enchants},
    entity::MobKind,
    item::{BLOCK_ITEM_BASE, FIRST_ITEM, Item},
    potion::Potion,
    simulation::effects::{Effect, Effects},
    world::block::{INFO, RenderKind, tex},
};

/// A face that names a layer past `tex::COUNT` samples outside the texture
/// array: a module that adds layers also raises the count.
#[test]
fn block_faces_name_generated_texture_layers() {
    const { assert!(tex::COUNT <= tex::CAPACITY) };
    for (id, info) in INFO.iter().enumerate().filter(|(_, i)| i.kind != RenderKind::Invisible) {
        for layer in info.tex {
            assert!(u32::from(layer) < tex::COUNT, "state {id} ({}) names layer {layer} of {}", info.name, tex::COUNT);
        }
    }
}

#[test]
fn non_block_items_are_listed_once_below_the_block_item_base() {
    let ids = FIRST_ITEM..BLOCK_ITEM_BASE;
    let mut listed = BTreeSet::new();
    for item in Item::all_items() {
        assert!(ids.contains(&item.0), "{} ({}) is outside the non-block item ids", item.0, item.name());
        assert!(item.is_valid(), "{} is listed without a definition", item.0);
        assert!(listed.insert(item.0), "{} ({}) is listed twice", item.0, item.name());
    }
    // Ids that name something without being listed: shears as the
    // integration branch saved them, and the two blocks with an item id.
    let unlisted: Vec<u16> = ids.filter(|&id| Item(id).is_valid() && !listed.contains(&id)).collect();
    assert_eq!(unlisted, [607, Item::SCAFFOLDING.0, Item::HONEY_BLOCK.0], "defined in Item::info, not in all_items");
}

/// Drinkable and splash potions are item ids computed from the potion's
/// index; a potion appended without new ids would land on another's.
#[test]
fn every_potion_has_its_own_drinkable_and_splash_item() {
    let mut ids = BTreeSet::new();
    for potion in Potion::all() {
        let (drink, splash) = (Item::potion(potion), Item::splash_potion(potion));
        assert_eq!((drink.as_potion(), drink.as_splash_potion()), (Some(potion), None), "{}", potion.info().id);
        assert_eq!((splash.as_splash_potion(), splash.as_potion()), (Some(potion), None), "{}", potion.info().id);
        assert!(ids.insert(drink.0) && ids.insert(splash.0), "{} shares an item id", potion.info().id);
    }
}

#[test]
fn append_only_orderings_keep_distinct_positions_that_fit_their_encodings() {
    // Mob kinds: the LAN index is the position, saves use the name.
    for (index, kind) in MobKind::ALL.into_iter().enumerate() {
        assert_eq!((kind.lan_index(), MobKind::from_lan_index(index)), (index, Some(kind)), "{kind:?} is listed twice");
        assert_eq!(MobKind::from_name(kind.name()), Some(kind), "{} names two mob kinds", kind.name());
    }
    // Effects: the mask bit and icon layer are the declaration position,
    // saves use the id.
    for (index, effect) in Effect::ALL.into_iter().enumerate() {
        assert_eq!(effect as usize, index, "Effect::ALL must follow declaration order");
        assert_eq!(Effect::from_id(effect.id()), Some(effect), "{} names two effects", effect.id());
    }
    let lasting: Vec<Effect> = Effect::ALL.into_iter().filter(|e| !e.is_instant()).collect();
    let mut all = Effects::default();
    for &effect in &lasting {
        all.add(effect, 0, 600);
    }
    assert_eq!(all.mask().count_ones() as usize, lasting.len(), "the effect mask has run out of bits: widen it");
    // Enchantments: a stack's levels are saved by bit position.
    for enchantment in Enchantment::ALL {
        let one = Enchants::NONE.with(enchantment, 7);
        assert_eq!(one.iter().collect::<Vec<_>>(), [(enchantment, 7)], "the level bits of {enchantment:?} don't fit");
        assert_eq!(Enchants::from_hex(&one.to_hex()), Some(one), "{enchantment:?} doesn't survive a save");
    }
}
