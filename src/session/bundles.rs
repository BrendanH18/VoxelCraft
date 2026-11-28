//! v1.0 gap track 6: holding use with a bundle throws its contents out, one
//! stack at a time (Java 26.3 `BundleItem.use`, `onUseTick`,
//! `dropContent`), shared by the desktop, controller, LAN and CLI paths.
//!
//! `use` starts using the bundle in either hand for its 200-tick use
//! duration (`getUseDuration`). Each use tick calls `onUseTick` with the
//! ticks remaining before counting down: the first tick (200 remaining)
//! throws the front stack, then every tick with fewer than 190 remaining
//! and an even count does (188, 186, ...), until use ends at zero or the
//! button is let go. A throw is `Player.drop(stack, true)`, the same toss
//! as dropping the selected item. Using a bundle counts as using an item
//! (the movement slow-down) and posts `item_interact_start` and `_finish`.
use super::PlayerSession;
use crate::{
    content::bundle,
    entity::Entities,
    inventory::{HOTBAR_SLOTS, Stack},
    item::Item,
    world::vibration::GameEvent,
};

/// `BundleItem.getUseDuration`.
pub const USE_TICKS: u32 = 200;
/// `TICKS_AFTER_FIRST_THROW`.
const AFTER_FIRST_THROW: u32 = 10;
/// `TICKS_BETWEEN_THROWS`.
const BETWEEN_THROWS: u32 = 2;

#[derive(Default)]
pub(super) struct State {
    /// The hotbar slot selected when use began, and whether the bundle is
    /// the off-hand's.
    slot: Option<usize>,
    pub(super) off: bool,
    /// Use ticks run so far.
    ticks: u32,
    /// The bundle's colour, for `isSameItem`.
    item: Option<Item>,
}

/// One stack thrown out of a bundle: where the player was, for the
/// remove-one and drop-contents sounds and the main hand's swing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emptied {
    pub stack: Stack,
    pub at: glam::DVec3,
}

impl PlayerSession {
    /// A bundle is in use.
    pub fn bundle_in_use(&self) -> bool {
        self.bundle_use.slot.is_some()
    }

    fn bundle_hand(&self, slot: usize) -> Option<(bool, Item)> {
        let main = self.inventory.get(slot).map(|s| s.item);
        if let Some(item) = main.filter(|&i| bundle::is_bundle(i)) {
            return Some((false, item));
        }
        // The off hand's bundle is used only when the main hand's item has
        // no use of its own that comes first (as for the brush).
        let off = self.inventory.offhand.map(|s| s.item).filter(|&i| bundle::is_bundle(i))?;
        let main_uses = main.is_some_and(|i| {
            i == Item::SHIELD
                || i.food().is_some() && self.can_eat(i)
                || i.is_drink()
                || matches!(i, Item::BOW | Item::CROSSBOW | Item::CHARGED_CROSSBOW | Item::BRUSH)
        });
        (!main_uses).then_some((true, off))
    }

    /// `BundleItem.use`: starts using the bundle in the main hand, or the off
    /// hand's when the main hand's item takes no use first. Returns whether
    /// one is now in use.
    pub fn start_bundle_use(&mut self, slot: usize, entities: &mut Entities) -> bool {
        if self.bundle_in_use() {
            return true;
        }
        if slot >= HOTBAR_SLOTS || !self.mode.can_interact() || self.vitals.is_dead() || self.sleeping.is_some() {
            return false;
        }
        let Some((off, item)) = self.bundle_hand(slot) else { return false };
        self.bundle_use = State { slot: Some(slot), off, ticks: 0, item: Some(item) };
        self.use_vibration(entities, GameEvent::ItemInteractStart);
        true
    }

    /// `stopUsingItem`: lets the bundle go.
    pub fn stop_bundle_use(&mut self, entities: &mut Entities) {
        if self.bundle_in_use() {
            self.use_vibration(entities, GameEvent::ItemInteractFinish);
        }
        self.bundle_use = State::default();
    }

    /// One fixed tick of holding use (`updatingUsingItem`): throws the front
    /// stack on the ticks `onUseTick` does. Letting go, switching slot or
    /// hand item, or reaching the end of the use stops it.
    pub fn tick_bundle_use(&mut self, slot: usize, using: bool, entities: &mut Entities) -> Option<Emptied> {
        let state = &self.bundle_use;
        let item = state.item?;
        let held = if state.off { self.inventory.offhand } else { self.inventory.get(slot) };
        if !using
            || state.slot != Some(slot)
            || held.is_none_or(|s| s.item != item)
            || !self.mode.can_interact()
            || self.vitals.is_dead()
            || self.sleeping.is_some()
        {
            self.stop_bundle_use(entities);
            return None;
        }
        let remaining = USE_TICKS - state.ticks;
        let throws = remaining == USE_TICKS
            || remaining < USE_TICKS - AFTER_FIRST_THROW && remaining.is_multiple_of(BETWEEN_THROWS);
        let emptied = if throws { self.throw_from_bundle(slot, entities) } else { None };
        self.bundle_use.ticks += 1;
        if self.bundle_use.ticks >= USE_TICKS {
            // `completeUsingItem`: the use runs out.
            self.stop_bundle_use(entities);
        }
        emptied
    }

    /// `BundleItem.dropContent`: the front stack out of the bundle in use,
    /// tossed as `Player.drop(stack, true)` tosses.
    fn throw_from_bundle(&mut self, slot: usize, entities: &mut Entities) -> Option<Emptied> {
        let hand = if self.bundle_use.off { &mut self.inventory.offhand } else { &mut self.inventory.slots[slot] };
        let mut bundle = (*hand)?;
        let stack = bundle::take_one(&mut bundle)?;
        *hand = Some(bundle);
        entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
        Some(Emptied { stack, at: self.player.pos })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::components::Payload;

    fn session_with(bundle: Stack) -> PlayerSession {
        let mut s = PlayerSession::new(crate::entity::PlayerId(1), glam::DVec3::new(0.5, 70.0, 0.5));
        s.mode = crate::rules::GameMode::Survival;
        s.inventory.slots[0] = Some(bundle);
        s
    }

    fn filled(stacks: usize) -> Stack {
        // Four kinds of item, one stack each.
        let kinds = [Item::SADDLE, Item::BUCKET, Item::BOW, Item::SHEARS];
        let items: Vec<Stack> = (0..stacks).map(|i| Stack::new(kinds[i % 4], 1)).collect();
        Stack::new(bundle::BUNDLE, 1).with_payload(Payload::Bundle(items))
    }

    #[test]
    fn holding_use_throws_the_front_stack_on_javas_ticks() {
        // More stacks than one use can throw (the weight rule isn't
        // checked when contents are set directly).
        let bundle = filled(100);
        let contents = bundle.bundle_contents().to_vec();
        let mut s = session_with(bundle);
        let mut e = Entities::new(42);
        assert!(s.start_bundle_use(0, &mut e));
        assert!(s.using_item(), "using a bundle slows like any held use");
        let mut thrown = Vec::new();
        for tick in 0..USE_TICKS + 5 {
            if let Some(out) = s.tick_bundle_use(0, true, &mut e) {
                thrown.push((tick, out.stack));
            }
        }
        // `onUseTick` with 200 remaining, then with 188, 186, ... 2 left.
        let ticks: Vec<u32> = thrown.iter().map(|&(t, _)| t).collect();
        let java: Vec<u32> = std::iter::once(0).chain((2..190).rev().filter(|r| r % 2 == 0).map(|r| 200 - r)).collect();
        assert_eq!(ticks, java);
        assert_eq!(ticks.len(), 95);
        let stacks: Vec<Stack> = thrown.iter().map(|&(_, s)| s).collect();
        assert_eq!(stacks, contents[..95], "front first");
        assert!(!s.bundle_in_use(), "the use runs out after 200 ticks");
        assert_eq!(e.items.len(), 95, "every thrown stack lies on the ground");
        assert_eq!(s.inventory.slots[0].unwrap().bundle_contents(), &contents[95..]);
    }

    #[test]
    fn letting_go_or_switching_stops_and_an_off_hand_bundle_is_used() {
        let mut s = session_with(filled(4));
        let mut e = Entities::new(42);
        assert!(s.start_bundle_use(0, &mut e));
        assert!(s.tick_bundle_use(0, true, &mut e).is_some());
        assert!(s.tick_bundle_use(0, false, &mut e).is_none());
        assert!(!s.bundle_in_use());
        assert!(s.start_bundle_use(0, &mut e));
        assert!(s.tick_bundle_use(1, true, &mut e).is_none(), "another slot");
        assert!(!s.bundle_in_use());
        // An empty hand leaves the use to the off hand's bundle.
        s.inventory.offhand = s.inventory.slots[0].take();
        assert!(s.start_bundle_use(0, &mut e));
        let out = s.tick_bundle_use(0, true, &mut e).expect("the off hand's bundle throws");
        assert_eq!(s.inventory.offhand.unwrap().bundle_contents().len(), 2);
        assert_eq!(out.at, s.player.pos);
        s.stop_bundle_use(&mut e);
        // A shield in the main hand comes first.
        s.inventory.slots[0] = Some(Stack::new(Item::SHIELD, 1));
        assert!(!s.start_bundle_use(0, &mut e));
        // An empty bundle is still used, throwing nothing.
        s.inventory.slots[0] = Some(Stack::new(bundle::BUNDLE, 1));
        assert!(s.start_bundle_use(0, &mut e));
        assert!(s.tick_bundle_use(0, true, &mut e).is_none());
        assert!(s.bundle_in_use());
    }
}
