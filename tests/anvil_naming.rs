//! v0.18 Track 2: the anvil's name box as every screen uses it. Each
//! player's session holds what its name box says for the item in the left
//! input; the desktop field, the controller keyboard and a LAN guest's
//! `anvil_name` command all set it, and the shared anvil result reads it
//! (`AnvilMenu.setItemName` and `createResult`).
use glam::DVec3;
use voxelcraft::{
    enchant::{self, TOO_EXPENSIVE},
    entity::PlayerId,
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    session::PlayerSession,
};

fn sword() -> Stack {
    Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1)
}

fn session_with(left: Option<Stack>, right: Option<Stack>) -> PlayerSession {
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::ZERO);
    s.work = [left, right, None];
    s
}

/// What the anvil offers this session, as the screens compute it.
fn result(s: &PlayerSession) -> Option<enchant::AnvilResult> {
    enchant::anvil_menu(s.work[0]?, s.work[1], s.anvil_name(), false)
}

#[test]
fn a_name_typed_for_the_left_item_renames_it_for_a_level() {
    let mut s = session_with(Some(sword()), None);
    assert!(result(&s).is_none(), "the box still shows the item's own name");
    assert!(s.set_anvil_name("Fang"));
    let r = result(&s).expect("a rename");
    assert_eq!(r.output.name.as_str(), Some("Fang"));
    assert_eq!((r.cost, r.uses), (1, Some(0)), "one level, the right input left alone");
    // Typing the item's own name back costs nothing (`AnvilScreen.onNameChanged`).
    assert!(s.set_anvil_name(sword().display_name()));
    assert!(result(&s).is_none());
    // An empty box on an unnamed item does nothing either.
    assert!(s.set_anvil_name(""));
    assert!(result(&s).is_none());
}

#[test]
fn a_blank_box_clears_a_custom_name_and_the_box_follows_the_left_item() {
    let named = sword().with_name("Old").unwrap();
    let mut s = session_with(Some(named), None);
    assert!(s.set_anvil_name(""));
    let r = result(&s).expect("clearing the name");
    assert_eq!(r.output.name.as_str(), None);
    assert_eq!(r.cost, 1);
    // A different left item resets the box (`AnvilScreen.slotChanged`).
    s.work[0] = Some(sword());
    assert_eq!(s.anvil_name(), None);
    assert!(result(&s).is_none());
    // No item, no box; and closing the screen forgets it.
    let mut empty = session_with(None, None);
    assert!(!empty.set_anvil_name("Nothing"));
    assert_eq!(empty.anvil_name(), None);
    let mut s = session_with(Some(sword()), None);
    s.set_anvil_name("Fang");
    s.clear_anvil_name();
    assert_eq!(s.anvil_name(), None);
}

#[test]
fn names_follow_validate_name_and_combine_with_repairs() {
    let mut s = session_with(Some(sword()), None);
    // `AnvilMenu.validateName`: section signs and control characters go;
    // more than 50 characters is refused and keeps the last good name.
    assert!(s.set_anvil_name("A\u{a7}cB\u{7}"));
    assert_eq!(s.anvil_name(), Some("AcB"));
    assert!(!s.set_anvil_name(&"x".repeat(51)));
    assert_eq!(s.anvil_name(), Some("AcB"));
    assert!(s.set_anvil_name(&"x".repeat(50)));
    // A rename on top of a repair costs one more level than the repair.
    let mut worn = sword();
    worn.damage = 1000;
    let diamonds = Stack::new(Item::DIAMOND, 2);
    let mut s = session_with(Some(worn), Some(diamonds));
    let repair = result(&s).expect("a repair").cost;
    s.set_anvil_name("Mended");
    let both = result(&s).expect("a repair and a rename");
    assert_eq!(both.cost, repair + 1);
    assert_eq!(both.output.name.as_str(), Some("Mended"));
    assert!(both.cost < TOO_EXPENSIVE);
}
