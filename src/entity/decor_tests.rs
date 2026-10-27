//! Placement, use, breaking and saving of item frames, paintings and
//! armor stands.
use super::*;
use crate::world::{World, terrain::Generator};
use glam::IVec2;
use std::sync::Arc;

const GROUND: i32 = 64;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.load_test_column(IVec2::new(x, z), GROUND);
        }
    }
    w
}

/// A stone wall along x = 0..8, y = 65..69, its east face open at x = 8.
fn walled() -> World {
    let mut w = world();
    for y in GROUND + 1..GROUND + 5 {
        for z in 0..8 {
            w.set_block(IVec3::new(7, y, z), Block::STONE);
        }
    }
    w
}

fn inventory(item: Item, count: u8) -> Inventory {
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(item, count));
    inv
}

/// Looking west at the wall's east face, at height `y`, z = 2.5.
fn aim(y: f64) -> (DVec3, DVec3) {
    (DVec3::new(9.5, y, 2.5), DVec3::NEG_X)
}

#[test]
fn frames_hang_on_solid_faces_hold_and_turn_items_and_pop_them() {
    let w = walled();
    let mut e = Entities::new(1);
    let wall = IVec3::new(7, GROUND + 2, 2);
    // Nothing to hang on in the open air.
    assert_eq!(e.place_decor(&w, Item::ITEM_FRAME, wall + IVec3::X * 3, IVec3::X, 0.0, &[]), Some(None));
    assert_eq!(e.place_decor(&w, Item::STICK, wall, IVec3::X, 0.0, &[]), None);
    let placed = e.place_decor(&w, Item::ITEM_FRAME, wall, IVec3::X, 0.0, &[]);
    assert_eq!(placed, Some(Some(Effect::Placed(Kind::Frame { glow: false }))));
    assert_eq!((e.decor[0].cell, e.decor[0].facing), (wall + IVec3::X, IVec3::X));
    // A second frame on the same face can't overlap it; a floor frame can go up.
    assert_eq!(e.place_decor(&w, Item::GLOW_ITEM_FRAME, wall, IVec3::X, 0.0, &[]), Some(None));
    let floor = IVec3::new(2, GROUND, 2);
    assert!(e.place_decor(&w, Item::GLOW_ITEM_FRAME, floor, IVec3::Y, 0.0, &[]).unwrap().is_some());
    // Its box: 12/16 square, a sixteenth deep, against the wall.
    let (min, max) = e.decor[0].aabb();
    assert!((min - DVec3::new(8.0, 66.125, 2.125)).abs().max_element() < 1e-9, "{min}");
    assert!((max - DVec3::new(8.0625, 66.875, 2.875)).abs().max_element() < 1e-9, "{max}");

    let (eye, dir) = aim(GROUND as f64 + 2.5);
    let mut inv = inventory(Item::DIAMOND, 3);
    assert_eq!(e.use_decor(eye, dir, 3.0, &mut inv, 0, false).map(|u| u.1), Some(Effect::ItemAdded));
    assert_eq!(inv.slots[0].map(|s| s.count), Some(2));
    assert_eq!(e.decor[0].item.map(|s| (s.item, s.count)), Some((Item::DIAMOND, 1)));
    // A full frame turns its item an eighth each click, eight to a turn.
    for turn in 1..=9 {
        assert_eq!(e.use_decor(eye, dir, 3.0, &mut inv, 0, false).map(|u| u.1), Some(Effect::ItemRotated));
        assert_eq!(e.decor[0].rotation, turn % ROTATIONS);
    }
    // A hit pops the item out and leaves the frame; the next breaks it.
    assert_eq!(e.hurt_decor(eye, dir, 3.0, false).map(|h| h.1), Some(Effect::ItemRemoved));
    assert_eq!(e.items.last().map(|i| i.stack.item), Some(Item::DIAMOND));
    assert_eq!(e.hurt_decor(eye, dir, 3.0, false).map(|h| h.1), Some(Effect::Broken(Kind::Frame { glow: false })));
    assert_eq!(e.items.last().map(|i| i.stack.item), Some(Item::ITEM_FRAME));
    assert_eq!(e.decor.len(), 1, "the floor frame stays");
}

#[test]
fn creative_hits_drop_nothing() {
    let w = walled();
    let mut e = Entities::new(1);
    let wall = IVec3::new(7, GROUND + 2, 2);
    e.place_decor(&w, Item::ITEM_FRAME, wall, IVec3::X, 0.0, &[]);
    let (eye, dir) = aim(GROUND as f64 + 2.5);
    let mut inv = inventory(Item::APPLE, 1);
    e.use_decor(eye, dir, 5.0, &mut inv, 0, true);
    assert_eq!(inv.slots[0].map(|s| s.count), Some(1), "creative keeps the item");
    e.hurt_decor(eye, dir, 5.0, true);
    e.hurt_decor(eye, dir, 5.0, true);
    assert!(e.decor.is_empty() && e.items.is_empty());
}

#[test]
fn frames_fall_when_their_wall_goes() {
    let mut w = walled();
    let mut e = Entities::new(1);
    let wall = IVec3::new(7, GROUND + 2, 2);
    e.place_decor(&w, Item::ITEM_FRAME, wall, IVec3::X, 0.0, &[]);
    e.decor[0].item = Some(Stack::new(Item::BREAD, 1));
    w.set_block(wall, Block::AIR);
    // Checked every 100 ticks.
    for _ in 0..99 {
        e.tick_decor(&w);
    }
    assert_eq!(e.decor.len(), 1);
    e.tick_decor(&w);
    assert!(e.decor.is_empty());
    let dropped: Vec<Item> = e.items.iter().map(|i| i.stack.item).collect();
    assert_eq!(dropped, [Item::ITEM_FRAME, Item::BREAD]);
}

#[test]
fn paintings_take_the_largest_placeable_variant_that_fits() {
    let mut w = walled();
    let mut e = Entities::new(7);
    // The open wall is 8 wide and 4 tall: a 4x4 painting fits.
    let clicked = IVec3::new(7, GROUND + 2, 3);
    assert!(e.place_decor(&w, Item::PAINTING, clicked, IVec3::X, 0.0, &[]).unwrap().is_some());
    let Kind::Painting(v) = e.decor[0].kind else { panic!() };
    let (name, width, height, placeable) = PAINTINGS[v as usize];
    assert_eq!((width, height, placeable), (4, 4, true), "{name}");
    let (min, max) = e.decor[0].aabb();
    assert!((max - min - DVec3::new(0.0625, 4.0, 4.0)).abs().max_element() < 1e-9);
    // Every cell behind it is wall.
    assert!(e.decor[0].survives(&Wrapped(&w), &[]));
    // Paintings never go on floors or ceilings.
    assert_eq!(e.place_decor(&w, Item::PAINTING, IVec3::new(2, GROUND, 2), IVec3::Y, 0.0, &[]), Some(None));
    // A one-block niche takes a 1x1 painting.
    let mut e = Entities::new(9);
    for y in GROUND + 1..GROUND + 5 {
        for z in 0..8 {
            w.set_block(IVec3::new(8, y, z), if z == 2 && y == GROUND + 2 { Block::AIR } else { Block::STONE });
        }
    }
    assert!(e.place_decor(&w, Item::PAINTING, IVec3::new(7, GROUND + 2, 2), IVec3::X, 0.0, &[]).unwrap().is_some());
    let Kind::Painting(v) = e.decor[0].kind else { panic!() };
    assert_eq!((PAINTINGS[v as usize].1, PAINTINGS[v as usize].2), (1, 1));
}

#[test]
fn armor_stands_face_the_player_wear_armor_and_break_on_a_double_hit() {
    let mut w = world();
    let mut e = Entities::new(1);
    let ground = IVec3::new(3, GROUND, 3);
    assert_eq!(e.place_decor(&w, Item::ARMOR_STAND, ground, IVec3::NEG_Y, 0.0, &[]), Some(None));
    // A player standing in the spot blocks it.
    let spot = DVec3::new(3.5, GROUND as f64 + 1.0, 3.5);
    assert_eq!(e.place_decor(&w, Item::ARMOR_STAND, ground, IVec3::Y, 0.0, &[spot]), Some(None));
    // Facing back at a player looking (Java yaw) 100 degrees: 100 - 180 = -80, snapped to -90.
    assert!(e.place_decor(&w, Item::ARMOR_STAND, ground, IVec3::Y, 100.0, &[]).unwrap().is_some());
    assert_eq!(e.decor[0].yaw, -90.0);
    assert_eq!(e.decor[0].pos, spot);

    let (eye, dir) = (DVec3::new(3.5, GROUND as f64 + 2.4, 6.0), DVec3::NEG_Z);
    let mut inv = inventory(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Diamond), 1);
    assert_eq!(e.use_decor(eye, dir, 4.0, &mut inv, 0, false).map(|u| u.1), Some(Effect::Equipped));
    assert!(inv.slots[0].is_none());
    assert_eq!(
        e.decor[0].armor[ArmorPiece::Chestplate as usize].map(|s| s.item),
        Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Diamond))
    );
    // Other items find no hands to go in.
    let mut stick = inventory(Item::STICK, 1);
    assert_eq!(e.use_decor(eye, dir, 4.0, &mut stick, 0, false).map(|u| u.1), Some(Effect::Refused));
    // Empty-handed at chest height takes the chestplate back.
    let chest = (DVec3::new(3.5, GROUND as f64 + 2.2, 6.0), DVec3::NEG_Z);
    assert_eq!(e.use_decor(chest.0, chest.1, 4.0, &mut inv, 0, false).map(|u| u.1), Some(Effect::Equipped));
    assert_eq!(inv.slots[0].map(|s| s.item), Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Diamond)));
    e.use_decor(eye, dir, 4.0, &mut inv, 0, false);

    // A first hit wobbles; one within five ticks breaks it.
    assert_eq!(e.hurt_decor(eye, dir, 4.0, false).map(|h| h.1), Some(Effect::Wobbled));
    for _ in 0..6 {
        e.tick_decor(&w);
    }
    assert_eq!(e.hurt_decor(eye, dir, 4.0, false).map(|h| h.1), Some(Effect::Wobbled), "too late: wobbles again");
    e.tick_decor(&w);
    assert_eq!(e.hurt_decor(eye, dir, 4.0, false).map(|h| h.1), Some(Effect::Broken(Kind::Stand)));
    let mut dropped: Vec<Item> = e.items.iter().map(|i| i.stack.item).collect();
    dropped.sort_by_key(|i| i.0);
    let mut expected = vec![Item::ARMOR_STAND, Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Diamond)];
    expected.sort_by_key(|i| i.0);
    assert_eq!(dropped, expected);

    // Stands fall to the ground.
    w.set_block(ground + IVec3::new(2, 3, 0), Block::STONE);
    let mut e = Entities::new(1);
    e.place_decor(&w, Item::ARMOR_STAND, ground + IVec3::new(2, 3, 0), IVec3::Y, 0.0, &[]);
    w.set_block(ground + IVec3::new(2, 3, 0), Block::AIR);
    for _ in 0..60 {
        e.tick_decor(&w);
    }
    assert!((e.decor[0].pos.y - (GROUND as f64 + 1.0)).abs() < 0.01, "{}", e.decor[0].pos);
}

#[test]
fn decorations_save_reload_and_go_to_chunk_storage() {
    let w = walled();
    let mut e = Entities::new(3);
    let wall = IVec3::new(7, GROUND + 2, 2);
    e.place_decor(&w, Item::GLOW_ITEM_FRAME, wall, IVec3::X, 0.0, &[]);
    e.decor[0].item = Some(Stack::new(Item::GOLDEN_APPLE, 1));
    e.decor[0].rotation = 5;
    e.place_decor(&w, Item::PAINTING, IVec3::new(7, GROUND + 3, 6), IVec3::X, 0.0, &[]);
    e.place_decor(&w, Item::ARMOR_STAND, IVec3::new(2, GROUND, 2), IVec3::Y, 33.0, &[]);
    e.decor[2].armor[0] = Some(Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1));
    let saved = e.decor_to_string();
    let mut other = Entities::new(4);
    other.load_decor(&saved);
    other.load_decor("F,1,2|junk;S,nan,1,2,0,-,-,-,-;P,0,0,0,1,0,no_such_art");
    assert_eq!(other.decor.len(), 3);
    for (a, b) in e.decor.iter().zip(&other.decor) {
        assert_eq!(
            (a.kind, a.cell, a.facing, a.item, a.rotation, a.armor),
            (b.kind, b.cell, b.facing, b.item, b.rotation, b.armor)
        );
        assert!((a.pos - b.pos).length() < 1e-3 && a.yaw == b.yaw);
    }
    // Unloading their chunk stores them; the save still has them.
    let mut w = w;
    w.unload_test_column(IVec2::new(0, 0));
    other.sync_chunk_entities(&mut w);
    assert!(other.decor.is_empty());
    assert_eq!(other.stored_entities(), 3);
    assert_eq!(other.decor_to_string().split(';').count(), 3);
}

#[test]
fn every_painting_has_art_and_the_items_have_recipes() {
    for (v, (name, w, h, _)) in PAINTINGS.iter().enumerate() {
        let (cw, ch) = (*w as usize * 4, *h as usize * 4);
        let colours: std::collections::HashSet<[u8; 3]> =
            (0..cw * ch).map(|i| painting_colour(v as u8, cw, ch, i % cw, i / cw)).collect();
        assert!(colours.len() > 3, "{name} is more than a frame");
    }
    let makes = |item: Item| crate::crafting::recipes().iter().any(|r| r.result.item == item);
    for item in [Item::ITEM_FRAME, Item::GLOW_ITEM_FRAME, Item::PAINTING, Item::ARMOR_STAND] {
        assert!(makes(item), "{}", item.name());
        assert_eq!(Item::from_name(item.name()), Some(item));
        assert!(Item::creative_palette().any(|i| i == item));
    }
    assert_eq!(Item::ARMOR_STAND.max_stack(), 16);
}
