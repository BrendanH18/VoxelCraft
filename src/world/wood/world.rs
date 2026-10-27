//! World rules of the wood track: signs that lose their support break,
//! shelves follow redstone power and chain to their neighbours, and shelves
//! and chiseled bookshelves trade items with players.

use glam::IVec3;

use super::super::World;
use super::super::block::Block;
use super::bookshelf;
use super::shelves::{self, Chain, MAX_CHAIN, Shelf};
use super::signs;
use crate::inventory::Stack;

const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

/// What a player's use of a shelf or chiseled bookshelf did, for sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelfUse {
    /// One stack went onto the shelf (`SHELF_PLACE_ITEM`).
    Placed,
    /// The held stack and the slot's swapped, or the slot's came off into
    /// an empty hand (`SHELF_SINGLE_SWAP`, `SHELF_TAKE_ITEM`).
    Swapped {
        took: bool,
    },
    /// A powered chain swapped its slots with the hotbar (`SHELF_MULTI_SWAP`).
    Hotbar,
    /// A powered chain with nothing to swap; the use is spent.
    Nothing,
    /// A book went into (or came out of) a chiseled bookshelf.
    BookIn {
        enchanted: bool,
    },
    BookOut {
        enchanted: bool,
    },
}

impl World {
    /// Signs next to `p` whose support was at `p` break when it no longer
    /// holds them (`updateShape` of each sign block).
    /// A sign that breaks is itself a lost support, so a stack of signs
    /// resting on signs breaks all the way along.
    pub(in crate::world) fn drop_unsupported_signs(&mut self, p: IVec3) {
        let mut work = vec![p];
        while let Some(p) = work.pop() {
            for d in SIDES {
                let at = p + d;
                let Some(b) = self.get_block(at) else { continue };
                let Some(s) = signs::sign(b) else { continue };
                if !signs::support_directions(s).iter().flatten().any(|&dir| dir == -d) {
                    continue;
                }
                if !signs::survives(s, at, &|q| self.get_block(q)) {
                    self.edit(at, if s.wet { Block::WATER } else { Block::AIR }, true);
                    self.spill_block(at, b);
                    work.push(at);
                }
            }
        }
    }

    // ---- Shelves --------------------------------------------------------

    /// A shelf that is powered and faces `facing` (Java's
    /// `isConnectableToThisBlock`).
    fn chainable(&self, p: IVec3, facing: super::super::block::Facing) -> Option<Shelf> {
        self.get_block(p).and_then(shelves::shelf).filter(|s| s.powered && s.facing == facing)
    }

    /// Java's `getAllBlocksConnectedTo`: the powered chain through `p`, left to right.
    pub fn shelf_chain(&self, p: IVec3) -> Vec<IVec3> {
        let Some(s) = self.get_block(p).and_then(shelves::shelf).filter(|s| s.powered) else { return Vec::new() };
        let mut out = std::collections::VecDeque::from([p]);
        for (dir, end) in [(s.left(), Chain::Left), (s.right(), Chain::Right)] {
            for step in 1..MAX_CHAIN as i32 {
                let q = p + dir * step;
                let Some(n) = self.chainable(q, s.facing) else { break };
                if n.chain.towards(end) {
                    if end == Chain::Left {
                        out.push_front(q);
                    } else {
                        out.push_back(q);
                    }
                }
                if n.chain.chain_end() {
                    break;
                }
            }
        }
        out.into()
    }

    fn set_chain(&mut self, p: IVec3, chain: Chain) {
        if let Some(s) = self.get_block(p).and_then(shelves::shelf)
            && s.chain != chain
        {
            self.edit(p, Shelf { chain, ..s }.block(), false);
        }
    }

    /// Java's `ShelfBlock.onPlace` (and `affectNeighborsAfterRemoval` for a
    /// shelf that went), run on every change of a shelf state.
    pub(in crate::world) fn shelf_changed(&mut self, p: IVec3, old: Block, new: Block) {
        let (was, is) = (shelves::shelf(old), shelves::shelf(new));
        match is {
            Some(s) if s.powered => {
                // `updateSelfAndNeighborsOnPoweringUp`, unless a neighbour's
                // chaining caused this change.
                let neighbour_update = s.chain.connected() || was.is_some_and(|o| o.powered && o.chain.connected());
                if neighbour_update {
                    return;
                }
                let mut part = Chain::Unconnected;
                let left = p + s.left();
                let right = p + s.right();
                let left_len = self.chainable(left, s.facing).map_or(0, |_| self.shelf_chain(left).len());
                let right_len = self.chainable(right, s.facing).map_or(0, |_| self.shelf_chain(right).len());
                let mut length = 1;
                if left_len > 0 && length + left_len <= MAX_CHAIN {
                    part = part.connect_left();
                    if let Some(n) = self.chainable(left, s.facing) {
                        self.set_chain(left, n.chain.connect_right());
                    }
                    length += left_len;
                }
                if right_len > 0 && length + right_len <= MAX_CHAIN {
                    part = part.connect_right();
                    if let Some(n) = self.chainable(right, s.facing) {
                        self.set_chain(right, n.chain.connect_left());
                    }
                }
                self.set_chain(p, part);
            }
            Some(s) => self.shelf_unchain_neighbours(p, s),
            None => {
                if let Some(o) = was {
                    self.shelf_unchain_neighbours(p, o);
                }
            }
        }
    }

    /// `updateNeighborsAfterPoweringDown`.
    fn shelf_unchain_neighbours(&mut self, p: IVec3, s: Shelf) {
        if let Some(n) = self.chainable(p + s.left(), s.facing) {
            self.set_chain(p + s.left(), n.chain.disconnect_right());
        }
        if let Some(n) = self.chainable(p + s.right(), s.facing) {
            self.set_chain(p + s.right(), n.chain.disconnect_left());
        }
    }

    /// Java's `ShelfBlock.neighborChanged`: power follows the neighbours'
    /// signal; losing it unchains the shelf.
    pub(in crate::world) fn shelf_power(&mut self, p: IVec3) {
        let Some(s) = self.get_block(p).and_then(shelves::shelf) else { return };
        let signal = self.redstone_power(p) > 0;
        if s.powered != signal {
            let chain = if signal { s.chain } else { Chain::Unconnected };
            self.edit(p, Shelf { powered: signal, chain, ..s }.block(), false);
        }
    }

    /// Java's `ShelfBlock.useItemOn` for a player whose hotbar is `hotbar`
    /// with slot `selected` held: `None` when the click passes (not on the
    /// front face, or an empty hand on an empty unpowered slot).
    /// `infinite` is Java's `hasInfiniteMaterials` (creative).
    pub fn use_shelf(
        &mut self,
        p: IVec3,
        face: IVec3,
        point: glam::DVec3,
        hotbar: &mut [Option<Stack>],
        selected: usize,
        infinite: bool,
    ) -> Option<ShelfUse> {
        let s = self.get_block(p).and_then(shelves::shelf)?;
        let slot = shelves::hit_slot(s, p, face, point)?;
        let mut changed = vec![p];
        let used = if !s.powered {
            let held = hotbar[selected];
            let items = self.wood.shelves.entry(p).or_default();
            let removed = std::mem::replace(&mut items[slot], held);
            hotbar[selected] = if infinite && removed.is_none() { held } else { removed };
            if removed.is_some() {
                ShelfUse::Swapped { took: held.is_none() }
            } else if held.is_none() {
                return None;
            } else {
                ShelfUse::Placed
            }
        } else {
            let chain = self.shelf_chain(p);
            if chain.is_empty() {
                return Some(ShelfUse::Nothing);
            }
            let mut any = false;
            let n = chain.len();
            for (i, q) in chain.iter().enumerate() {
                let items = self.wood.shelves.entry(*q).or_default();
                for (k, item) in items.iter_mut().enumerate() {
                    // The chain's slots line up with the end of the hotbar.
                    let Some(inv) = (9 + k).checked_sub((n - i) * 3) else { continue };
                    let Some(held) = hotbar.get_mut(inv) else { continue };
                    if held.is_some() || item.is_some() {
                        std::mem::swap(held, item);
                        any = true;
                    }
                }
            }
            changed = chain;
            if any { ShelfUse::Hotbar } else { ShelfUse::Nothing }
        };
        self.wood.version += 1;
        // Every shelf whose contents changed updates its comparators.
        for q in changed {
            self.redstone_changed(q);
        }
        Some(used)
    }

    // ---- Chiseled bookshelves ---------------------------------------------

    /// Java's `ChiseledBookShelfBlock.useItemOn` then `useWithoutItem`:
    /// one of the held books goes into an empty slot under the click
    /// ([`ShelfUse::BookIn`]: the caller spends it), or the slot's book
    /// comes out (returned for the caller's inventory). `None` when the
    /// click passes to the held item.
    pub fn use_bookshelf(
        &mut self,
        p: IVec3,
        face: IVec3,
        point: glam::DVec3,
        held: Option<Stack>,
    ) -> Option<(ShelfUse, Option<Stack>)> {
        let b = self.get_block(p)?;
        let (facing, slots) = bookshelf::shelf(b)?;
        let slot = bookshelf::hit_slot(facing, p, face, point);
        let holding_book = held.is_some_and(|s| bookshelf::is_book(s.item));
        let mut out = None;
        let used = match slot {
            None => return None,
            Some(slot) if holding_book && slots & (1 << slot) == 0 => {
                let book = held?;
                let entity = self.wood.bookshelves.entry(p).or_default();
                entity.books[slot] = Some(Stack { count: 1, ..book });
                entity.last_slot = Some(slot as u8);
                self.edit(p, bookshelf::state(facing, slots | 1 << slot), false);
                ShelfUse::BookIn { enchanted: book.item == crate::item::Item::ENCHANTED_BOOK }
            }
            // An empty slot under an empty hand (or a non-book) spends the use.
            Some(slot) if slots & (1 << slot) == 0 => ShelfUse::Nothing,
            Some(slot) => {
                let entity = self.wood.bookshelves.entry(p).or_default();
                let book = entity.books[slot].take();
                entity.last_slot = Some(slot as u8);
                self.edit(p, bookshelf::state(facing, slots & !(1 << slot)), false);
                let enchanted = book.is_some_and(|b| b.item == crate::item::Item::ENCHANTED_BOOK);
                out = book;
                ShelfUse::BookOut { enchanted }
            }
        };
        self.wood.version += 1;
        self.redstone_changed(p);
        Some((used, out))
    }

    /// A comparator's reading of the block at `p`, read from the side
    /// `toward` points to: shelves give their filled-slot bits from behind,
    /// chiseled bookshelves the last slot used plus one.
    pub(in crate::world) fn wood_comparator_signal(&self, p: IVec3, toward: IVec3) -> Option<u8> {
        let b = self.get_block(p)?;
        if let Some(s) = shelves::shelf(b) {
            let items = self.wood.shelves.get(&p).copied().unwrap_or_default();
            return Some(shelves::comparator_signal(s, &items, toward));
        }
        if bookshelf::slots(b).is_some() {
            return Some(self.wood.bookshelves.get(&p).and_then(|e| e.last_slot).map_or(0, |s| s + 1));
        }
        None
    }
}
