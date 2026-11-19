use super::*;
use crate::content::banner_patterns as bp;
use crate::content::books;
use crate::world::terrain::Generator;
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w
}

const P: IVec3 = IVec3::new(3, 10, 3);

fn written(pages: usize) -> Stack {
    let pages: Vec<String> = (0..pages).map(|i| format!("page {i}")).collect();
    let quill = books::edit(Stack::new(books::WRITABLE_BOOK, 1), &pages).unwrap();
    books::sign(quill, &pages, "Notes", "Alex").unwrap()
}

#[test]
fn lectern_states_cover_facing_book_and_power() {
    let mut seen = std::collections::BTreeSet::new();
    for f in Facing::ALL {
        for book in [false, true] {
            for powered in [false, true] {
                let b = lectern_state(f, book, powered);
                assert_eq!(lectern(b.0), Some((f, book, powered)));
                assert!(seen.insert(b.0));
                assert_eq!(b.name(), "lectern");
                assert_eq!(b.base(), Block::LECTERN);
                assert_eq!(b.oriented(), Some((Block::LECTERN, f)));
                assert_eq!(b.drop(), Some(crate::item::Item::from_block(Block::LECTERN)));
            }
        }
    }
    assert_eq!(seen.len(), 16);
    assert!(seen.iter().all(|&id| (937..=940).contains(&id) || (LECTERN_FIRST..=LECTERN_LAST).contains(&id)));
}

#[test]
fn books_go_on_lecterns_turn_pages_pulse_and_come_back() {
    let mut w = world();
    w.set_block(P, lectern_state(Facing::North, false, false));
    assert_eq!(w.lectern_comparator(P), Some(0));
    assert!(!w.place_lectern_book(P, Stack::new(crate::item::Item::BOOK, 1)), "a plain book isn't a lectern book");
    let book = written(3);
    assert!(w.place_lectern_book(P, Stack { count: 5, ..book }));
    assert_eq!(w.get_block(P), Some(lectern_state(Facing::North, true, false)));
    assert_eq!(w.lectern_at(P).unwrap().book.count, 1, "one book goes on");
    assert!(!w.place_lectern_book(P, book), "a lectern holds one book");
    assert_eq!(w.lectern_comparator(P), Some(1), "page 1 of 3");
    assert!(w.set_lectern_page(P, 1));
    assert_eq!(w.lectern_comparator(P), Some(8));
    assert_eq!(w.get_block(P), Some(lectern_state(Facing::North, true, true)), "a page turn powers it");
    assert_eq!(w.redstone_power(P + IVec3::NEG_Y), 15, "the pulse strongly powers the block below");
    assert_eq!(w.page_turns, [PageTurn(P)]);
    assert!(!w.set_lectern_page(P, 1), "the same page changes nothing");
    assert!(w.set_lectern_page(P, 99), "pages clamp to the book");
    assert_eq!(w.lectern_at(P).unwrap().page, 2);
    assert_eq!(w.lectern_comparator(P), Some(15));
    for _ in 0..3 {
        w.tick_redstone();
    }
    assert_eq!(w.get_block(P), Some(lectern_state(Facing::North, true, false)), "two ticks later it's off");
    let taken = w.take_lectern_book(P).unwrap();
    assert_eq!(taken, book);
    assert_eq!(w.get_block(P), Some(lectern_state(Facing::North, false, false)));
    assert_eq!(w.lectern_comparator(P), Some(0));
}

/// v0.15 integration review: a save during the two-tick pulse keeps the
/// pulse's scheduled end, so the reloaded lectern turns itself off.
#[test]
fn a_lectern_saved_mid_pulse_turns_off_after_reload() {
    let mut w = world();
    w.set_block(P, lectern_state(Facing::North, false, false));
    assert!(w.place_lectern_book(P, written(3)));
    assert!(w.set_lectern_page(P, 1));
    assert_eq!(w.get_block(P), Some(lectern_state(Facing::North, true, true)));
    let saved = w.redstone_to_string();
    let mut loaded = world();
    loaded.set_block(P, lectern_state(Facing::North, true, true));
    loaded.load_redstone(&saved);
    for _ in 0..3 {
        loaded.tick_redstone();
    }
    assert_eq!(loaded.get_block(P), Some(lectern_state(Facing::North, true, false)));
}

#[test]
fn breaking_a_lectern_drops_its_book() {
    let mut w = world();
    w.set_block(P, lectern_state(Facing::East, false, false));
    let book = written(1);
    assert!(w.place_lectern_book(P, book));
    w.drops.clear();
    w.set_block(P, Block::AIR);
    assert_eq!(w.drops, [(P + IVec3::Y, book)]);
    assert!(w.lectern_at(P).is_none());
}

#[test]
fn banners_keep_patterns_through_placing_saving_and_breaking() {
    let mut w = world();
    let mut item = bp::loom_result(
        Stack::new(bp::banner_item(crate::color::DyeColor::Blue), 1),
        Stack::new(crate::color::DyeColor::Yellow.dye(), 1),
        crate::inventory::components::BannerPattern::from_name("cross").unwrap(),
    )
    .unwrap();
    item = item.with_name("Flag").unwrap();
    w.set_block(P, crate::world::colour_blocks::banner::standing(crate::color::DyeColor::Blue as u8, 3));
    w.restore_banner_item(P, &item);
    assert_eq!(w.banner_item_at(P), Some(item));
    let saved = w.workstations_to_string();
    let mut other = world();
    other.set_block(P, crate::world::colour_blocks::banner::standing(crate::color::DyeColor::Blue as u8, 3));
    other.load_workstations(&saved);
    assert_eq!(other.banner_item_at(P), Some(item));
    assert_eq!(other.workstations_to_string(), saved);
    other.drops.clear();
    other.set_block(P, Block::AIR);
    other.spill_block(P, crate::world::colour_blocks::banner::standing(crate::color::DyeColor::Blue as u8, 3));
    assert_eq!(other.drops, [(P, item)]);
    // The ominous banner round-trips as itself.
    let ominous = bp::ominous_banner();
    w.set_block(P, crate::world::colour_blocks::banner::standing(0, 0));
    w.restore_banner_item(P, &ominous);
    assert_eq!(w.banner_item_at(P), Some(ominous));
    assert_eq!(w.banner_item_at(P).unwrap().display_name(), "ominous banner");
}

#[test]
fn lecterns_save_their_book_and_page() {
    let mut w = world();
    w.set_block(P, lectern_state(Facing::South, false, false));
    w.place_lectern_book(P, written(4));
    w.set_lectern_page(P, 2);
    let saved = w.workstations_to_string();
    let mut other = world();
    other.set_block(P, lectern_state(Facing::South, true, false));
    other.load_workstations(&saved);
    assert_eq!(other.lectern_at(P), w.lectern_at(P));
    assert_eq!(other.lectern_comparator(P), w.lectern_comparator(P));
    other.load_workstations("l:1,2,3=junk;x|b:zz=;-|l:4,5,6");
    assert_eq!(other.workstations_to_string(), saved);
}
