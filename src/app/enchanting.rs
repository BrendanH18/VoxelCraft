//! The enchanting table screen, as Java has it: an item slot, a lapis
//! slot and three offers whose costs grow with the bookshelves around the
//! table. The offers come from the player's enchantment seed, so they stay
//! the same until something is enchanted.

use glam::IVec3;

use crate::enchant::{self, Offer};
use crate::inventory::Stack;

use super::{Container, Game};

/// Shared transient inputs. Enchanting and anvils use the first two;
/// smithing uses template, base and addition.
pub(super) type WorkSlots = [Option<Stack>; 3];

impl Game {
    /// Right-click on an enchanting table.
    pub(super) fn open_enchanting(&mut self, pos: IVec3) {
        if self.inventory_open {
            return;
        }
        self.body.container = Container::Enchanting(pos);
        self.toggle_inventory();
    }

    /// The three offers for what's in the host's item slot.
    pub(super) fn enchant_offers(&self) -> [Offer; 3] {
        super::screen::enchant_offers(self.body.container, &self.session, &self.world)
    }

    /// Whether the host can take offer `i` now.
    pub(super) fn can_take_offer(&self, i: usize, offer: Offer) -> bool {
        super::screen::can_take_offer(&self.session, i, offer)
    }

    /// Empties the input slots (closing the screen returns them).
    pub(super) fn take_work(&mut self) -> Vec<Stack> {
        self.session.work.iter_mut().filter_map(Option::take).collect()
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
