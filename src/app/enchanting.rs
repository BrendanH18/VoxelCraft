//! The enchanting table screen, as Java has it: an item slot, a lapis
//! slot and three offers whose costs grow with the bookshelves around the
//! table. The offers come from the player's enchantment seed, so they stay
//! the same until something is enchanted.

use glam::{DVec3, IVec3};

use crate::audio::sounds::Sound;
use crate::enchant::{self, Offer};
use crate::inventory::{Stack, move_into};
use crate::item::Item;

use super::hud::SlotRef;
use super::{Container, Game};

/// A crafting station's two input slots: the enchanting table's item and
/// lapis, or the anvil's left and right inputs.
pub(super) type WorkSlots = [Option<Stack>; 2];

impl Game {
    /// Right-click on an enchanting table.
    pub(super) fn open_enchanting(&mut self, pos: IVec3) {
        if self.inventory_open {
            return;
        }
        self.container = Container::Enchanting(pos);
        self.toggle_inventory();
    }

    /// The three offers for what's in the item slot.
    pub(super) fn enchant_offers(&self) -> [Offer; 3] {
        let Container::Enchanting(pos) = self.container else { return Default::default() };
        match self.work[0] {
            Some(stack) if enchant::table_accepts(stack) => {
                enchant::offers(self.vitals.xp.seed, stack.item, enchant::bookshelves(&self.world, pos))
            }
            _ => Default::default(),
        }
    }

    /// Whether offer `i` can be taken now: enough levels for its cost and
    /// lapis for its number (creative needs neither).
    pub(super) fn can_take_offer(&self, i: usize, offer: Offer) -> bool {
        let creative = self.mode.is_creative();
        let lapis = self.work[1].map_or(0, |s| s.count) as usize;
        let level = self.vitals.xp.level;
        offer.cost > 0 && (creative || (lapis > i && level >= offer.cost && level > i as u32))
    }

    /// A click on the table's slots or offers. The item slot holds one
    /// item, the lapis slot only lapis (Java's slot rules).
    pub(super) fn table_click(&mut self, slot: SlotRef, right: bool) {
        let cursor = &mut self.inventory.cursor;
        match slot {
            SlotRef::EnchantItem => match (*cursor, self.work[0]) {
                (Some(c), None) => {
                    self.work[0] = Some(Stack { count: 1, ..c });
                    *cursor = (c.count > 1).then_some(Stack { count: c.count - 1, ..c });
                }
                (Some(c), Some(held)) if c.count == 1 => {
                    self.work[0] = Some(c);
                    *cursor = Some(held);
                }
                (None, held) => {
                    *cursor = held;
                    self.work[0] = None;
                }
                _ => {}
            },
            SlotRef::EnchantLapis => {
                if cursor.is_none_or(|c| c.item == Item::LAPIS_LAZULI) {
                    crate::inventory::click_slot(&mut self.work[1], cursor, right);
                }
            }
            SlotRef::EnchantOffer(i) => self.take_offer(i),
            _ => {}
        }
    }

    /// Java's `EnchantmentMenu.clickMenuButton`: enchants the item with
    /// offer `i`'s roll, costing `i + 1` levels and lapis, and draws a new
    /// enchantment seed.
    pub(super) fn take_offer(&mut self, i: usize) {
        let Container::Enchanting(pos) = self.container else { return };
        let offer = self.enchant_offers()[i];
        let Some(mut stack) = self.work[0] else { return };
        if !self.can_take_offer(i, offer) {
            return;
        }
        let list = enchant::offer_enchants(self.vitals.xp.seed, stack.item, i, offer.cost);
        if list.is_empty() {
            return;
        }
        if stack.item == Item::BOOK {
            stack.item = Item::ENCHANTED_BOOK;
        }
        for (e, level) in list {
            stack.enchants.set(e, level);
        }
        self.work[0] = Some(stack);
        if self.mode.is_survival() {
            self.vitals.xp.add_levels(-(i as i64 + 1));
            if let Some(lapis) = &mut self.work[1] {
                lapis.count -= i as u8 + 1;
                if lapis.count == 0 {
                    self.work[1] = None;
                }
            }
        }
        self.vitals.xp.seed = (enchant::roll() * u32::MAX as f32) as u32 as i32;
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        self.audio.play(Sound::LevelUp, Some(at), 0.6, (1.5, 1.7));
    }

    /// Shift-click from the inventory onto the table: lapis to its slot,
    /// anything else into the empty item slot. Returns what didn't move.
    pub(super) fn move_to_table(&mut self, stack: Stack) -> Option<Stack> {
        if stack.item == Item::LAPIS_LAZULI {
            return move_into(stack, std::slice::from_mut(&mut self.work[1]), &[0]);
        }
        if self.work[0].is_some() {
            return Some(stack);
        }
        self.work[0] = Some(Stack { count: 1, ..stack });
        (stack.count > 1).then_some(Stack { count: stack.count - 1, ..stack })
    }

    /// Empties the input slots (closing the screen returns them).
    pub(super) fn take_work(&mut self) -> Vec<Stack> {
        self.work.iter_mut().filter_map(Option::take).collect()
    }
}

/// Java's enchanting screen shows each offer as a few words in the
/// standard galactic alphabet; these are its words, picked by the seed.
pub fn rune_words(seed: i32, slot: usize) -> String {
    const WORDS: [&str; 24] = [
        "the",
        "elder",
        "scrolls",
        "klaatu",
        "berata",
        "niktu",
        "xyzzy",
        "bless",
        "curse",
        "light",
        "darkness",
        "fire",
        "air",
        "earth",
        "water",
        "ignite",
        "snuff",
        "imbue",
        "galvanize",
        "enchant",
        "sphere",
        "spirit",
        "beast",
        "fhtagn",
    ];
    let mut rng = enchant::JavaRandom::new(seed as i64 ^ (slot as i64 * 0x9E37));
    let n = 2 + rng.next_bounded(2) as usize;
    (0..n).map(|_| WORDS[rng.next_bounded(WORDS.len() as i32) as usize]).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rune_words_are_stable() {
        assert_eq!(rune_words(5, 0), rune_words(5, 0));
        assert!(rune_words(5, 1).split(' ').count() >= 2);
    }
}
