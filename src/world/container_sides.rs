//! v0.17 Track C2: Java 26.3's sided container access for hoppers,
//! droppers and hopper minecarts (`WorldlyContainer.getSlotsForFace`,
//! `canPlaceItemThroughFace` and `canTakeItemThroughFace`, with each
//! container's `canPlaceItem` and `canTakeItem`), checked against
//! `tests/fixtures/java/26.3/dispensing.json` by `tests/hopper_sides.rs`.
//!
//! A `face` is the side of the container being reached, as Java's
//! `Direction`: the unit step from the container toward whoever reaches
//! it. A hopper above pushes through `UP` and one below pulls through
//! `DOWN`; a hopper beside pushes through that side.
//!
//! Plain containers (chests, barrels, dispensers, droppers, hoppers,
//! shelves) offer every slot through every face in slot order. Worldly
//! containers (furnaces, brewing stands, shulker boxes, the composter)
//! name their slots per face, in the order hoppers try them: a hopper under
//! a furnace takes the output (slot 2) before an empty bucket in the fuel
//! slot (slot 1).
use glam::IVec3;

use super::World;
use crate::inventory::Stack;
use crate::item::Item;

/// How a container block exposes its slots.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Every face reaches every slot (`HopperBlockEntity.getSlots` for a
    /// non-worldly container): chests (a double chest is one 54-slot
    /// container), barrels, copper chests, dispensers, droppers and hoppers.
    Plain { slots: u8 },
    /// `ShulkerBoxBlockEntity`: every slot through every face, but no
    /// shulker box goes in.
    ShulkerBox,
    /// `AbstractFurnaceBlockEntity` (furnace, smoker, blast furnace):
    /// input 0, fuel 1, output 2.
    Furnace,
    /// `BrewingStandBlockEntity`: bottles 0-2, ingredient 3, fuel 4.
    BrewingStand,
    /// `ComposterBlock.InputContainer` (levels 0-6): one item through the top.
    ComposterInput,
    /// `ComposterBlock.EmptyContainer` (level 7): a container with no slots,
    /// which still stops a hopper below from collecting items.
    ComposterFull,
    /// `ComposterBlock.OutputContainer` (level 8): bone meal out of the bottom.
    ComposterOutput,
    /// `JukeboxBlockEntity`: one playable disc into an empty slot.
    Jukebox,
    /// `ChiseledBookShelfBlockEntity`: six one-book slots.
    ChiseledBookshelf,
    /// `ShelfBlockEntity`: three slots taking anything.
    Shelf,
    /// Track C1's crafter (`CrafterBlockEntity`). Its face rules are Track
    /// C1's: [`can_place`] refuses everything until that track supplies
    /// them, and [`World::container_kind`] doesn't return it yet.
    Crafter,
}

const SLOTS: [u8; 54] = {
    let mut out = [0; 54];
    let mut i = 0;
    while i < 54 {
        out[i] = i as u8;
        i += 1;
    }
    out
};

/// Slot count (`Container.getContainerSize`).
pub const fn size(kind: Kind) -> usize {
    match kind {
        Kind::Plain { slots } => slots as usize,
        Kind::ShulkerBox => 27,
        Kind::Furnace | Kind::Shelf => 3,
        Kind::BrewingStand => 5,
        Kind::ComposterInput | Kind::ComposterOutput | Kind::Jukebox => 1,
        Kind::ComposterFull => 0,
        Kind::ChiseledBookshelf => 6,
        Kind::Crafter => 9,
    }
}

/// Every slot in order: where a hopper puts an item entity it collects
/// (Java's `addItem` with no direction).
pub fn all_slots(kind: Kind) -> &'static [u8] {
    &SLOTS[..size(kind)]
}

/// `getSlotsForFace(face)` for worldly containers, every slot in order for
/// the rest.
pub fn slots_for_face(kind: Kind, face: IVec3) -> &'static [u8] {
    let (up, down) = (face == IVec3::Y, face == IVec3::NEG_Y);
    match kind {
        Kind::Furnace if up => &[0],
        Kind::Furnace if down => &[2, 1],
        Kind::Furnace => &[1],
        Kind::BrewingStand if up => &[3],
        Kind::BrewingStand if down => &[0, 1, 2, 3],
        Kind::BrewingStand => &[0, 1, 2, 4],
        Kind::ComposterInput if up => &[0],
        Kind::ComposterOutput if down => &[0],
        Kind::ComposterInput | Kind::ComposterOutput | Kind::ComposterFull => &[],
        _ => &SLOTS[..size(kind)],
    }
}

/// Whether the container is worldly (`WorldlyContainer`): its face decides
/// which slots exist and what passes through them.
pub const fn worldly(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::ShulkerBox
            | Kind::Furnace
            | Kind::BrewingStand
            | Kind::ComposterInput
            | Kind::ComposterFull
            | Kind::ComposterOutput
    )
}

/// `canPlaceItem(slot, item)` and, for worldly containers,
/// `canPlaceItemThroughFace(slot, item, face)`. `current` is what the slot
/// holds now. `face` is `None` for an item entity a hopper collects (Java
/// passes a null direction; no worldly container collects items).
pub fn can_place(kind: Kind, slot: usize, item: Item, current: Option<&Stack>, face: Option<IVec3>) -> bool {
    match kind {
        Kind::Plain { .. } | Kind::Shelf => true,
        Kind::ShulkerBox => !crate::content::shulker_box::is_shulker_box(item),
        // `AbstractFurnaceBlockEntity.canPlaceItem`: nothing into the
        // output; any item as input; fuel (`COOKING_FUEL`), or a bucket
        // while the fuel slot holds no bucket.
        Kind::Furnace => match slot {
            0 => true,
            1 => {
                super::furnace::burn_time(item).is_some()
                    || item == Item::BUCKET && current.is_none_or(|s| s.item != Item::BUCKET)
            }
            _ => false,
        },
        // `BrewingStandBlockEntity.canPlaceItem`: blaze powder
        // (`BREWING_FUEL`) as fuel, a brewing reagent as the ingredient,
        // a potion or glass bottle into an empty bottle slot.
        Kind::BrewingStand => match slot {
            4 => item == Item::BLAZE_POWDER,
            3 => super::brewing::is_ingredient(item),
            _ => current.is_none() && super::brewing::fits_bottle_slot(item),
        },
        // `InputContainer.canPlaceItemThroughFace`: compostables, from above.
        Kind::ComposterInput => face == Some(IVec3::Y) && crate::content::compost::chance(item) > 0.0,
        Kind::ComposterFull | Kind::ComposterOutput => false,
        // `JukeboxBlockEntity.canPlaceItem`: `JUKEBOX_PLAYABLE`, slot empty.
        Kind::Jukebox => current.is_none() && super::jukebox::song_of(item).is_some(),
        // `ListBackedContainer.canPlaceItem` with `acceptsItemType`
        // (`#bookshelf_books`) and a stack limit of one.
        Kind::ChiseledBookshelf => current.is_none() && super::wood::bookshelf::is_book(item),
        // Track C1 seam: `CrafterBlockEntity.canPlaceItem` (disabled slots,
        // the smallest-stack rule) belongs to the crafter track.
        Kind::Crafter => false,
    }
}

/// `canTakeItem` and, for worldly containers, `canTakeItemThroughFace`.
/// The jukebox's and chiseled bookshelf's `canTakeItem` also ask the taker
/// for room; the insertion that follows already refuses a taker without it.
pub fn can_take(kind: Kind, slot: usize, item: Item, face: IVec3) -> bool {
    match kind {
        // `#furnace_fuel_bottom_takeable` (bucket, water bucket) from the
        // fuel slot through the bottom; anything else that face offers.
        Kind::Furnace => !(face == IVec3::NEG_Y && slot == 1) || matches!(item, Item::BUCKET | Item::WATER_BUCKET),
        // Only an empty glass bottle comes out of the ingredient slot.
        Kind::BrewingStand => slot != 3 || item == Item::GLASS_BOTTLE,
        Kind::ComposterInput | Kind::ComposterFull => false,
        // `OutputContainer.canTakeItemThroughFace`: bone meal, from below.
        Kind::ComposterOutput => face == IVec3::NEG_Y && item == Item::BONE_MEAL,
        _ => true,
    }
}

impl World {
    /// The container at `p` as hoppers, droppers and hopper minecarts see
    /// it (`HopperBlockEntity.getBlockContainer`), or `None`. An ender
    /// chest isn't a container, and neither is a chest lent to an open
    /// ender chest screen.
    pub fn container_kind(&self, p: IVec3) -> Option<Kind> {
        let b = self.get_block(p)?;
        if self.lent_chest() == Some(p) || super::utilities::ender_chest_of(b).is_some() {
            return None;
        }
        if let Some(level) = super::composter::level(b) {
            return Some(match level {
                8 => Kind::ComposterOutput,
                7 => Kind::ComposterFull,
                _ => Kind::ComposterInput,
            });
        }
        if super::jukebox::is_jukebox(b) {
            return Some(Kind::Jukebox);
        }
        if self.chest(p).is_some() {
            return Some(if super::shulker_box::owns(b.0) {
                Kind::ShulkerBox
            } else {
                Kind::Plain { slots: self.container_slots(p) as u8 }
            });
        }
        if self.furnace(p).is_some() {
            return Some(Kind::Furnace);
        }
        if self.brewing_stand(p).is_some() {
            return Some(Kind::BrewingStand);
        }
        if super::wood::bookshelf::shelf(b).is_some() {
            return Some(Kind::ChiseledBookshelf);
        }
        if super::wood::shelves::shelf(b).is_some() {
            return Some(Kind::Shelf);
        }
        // Track C1 seam: the crafter answers `Kind::Crafter` here once it
        // exists, with its slots read through `container_stack`.
        None
    }

    /// The stack in `slot` of the container at `p`.
    pub fn container_stack(&self, p: IVec3, slot: usize) -> Option<Stack> {
        match self.container_kind(p)? {
            Kind::ComposterOutput => Some(Stack::new(Item::BONE_MEAL, 1)),
            Kind::ComposterInput | Kind::ComposterFull | Kind::Crafter => None,
            Kind::Jukebox => self.jukebox(p).and_then(|j| j.record),
            Kind::Plain { .. } | Kind::ShulkerBox => self.chest_slot(p, slot),
            Kind::Furnace => {
                let f = self.furnace(p)?;
                [f.input, f.fuel, f.output].get(slot).copied().flatten()
            }
            Kind::BrewingStand => {
                let b = self.brewing_stand(p)?;
                [b.bottles[0], b.bottles[1], b.bottles[2], b.ingredient, b.fuel].get(slot).copied().flatten()
            }
            Kind::ChiseledBookshelf => {
                self.wood_entities().bookshelf(p).and_then(|e| e.books.get(slot).copied().flatten())
            }
            Kind::Shelf => self.wood_entities().shelf(p).and_then(|s| s.get(slot).copied().flatten()),
        }
    }

    /// Replaces the stack in `slot` of the container at `p`.
    pub(super) fn set_container_stack(&mut self, p: IVec3, slot: usize, stack: Option<Stack>) {
        let Some(kind) = self.container_kind(p) else { return };
        match kind {
            Kind::ComposterOutput => {
                if stack.is_none() {
                    self.take_compost(p);
                }
            }
            Kind::ComposterInput | Kind::ComposterFull | Kind::Crafter => {}
            Kind::Jukebox => {
                self.set_jukebox_record(p, stack);
            }
            Kind::Plain { .. } | Kind::ShulkerBox => {
                self.set_chest_slot(p, slot, stack);
            }
            Kind::Furnace => {
                if let Some(f) = self.furnace_mut(p) {
                    match slot {
                        0 => {
                            f.input = stack;
                            f.cook = 0.0;
                        }
                        1 => f.fuel = stack,
                        _ => f.output = stack,
                    }
                }
            }
            Kind::BrewingStand => {
                if let Some(b) = self.brewing_stand_mut(p) {
                    match slot {
                        0..=2 => b.bottles[slot] = stack,
                        3 => b.ingredient = stack,
                        _ => b.fuel = stack,
                    }
                }
            }
            Kind::ChiseledBookshelf => self.set_bookshelf_book(p, slot, stack),
            Kind::Shelf => self.set_shelf_slot(p, slot, stack),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn furnace_faces_follow_java_slot_order() {
        assert_eq!(slots_for_face(Kind::Furnace, IVec3::NEG_Y), &[2, 1]);
        assert_eq!(slots_for_face(Kind::Furnace, IVec3::Y), &[0]);
        assert_eq!(slots_for_face(Kind::Furnace, IVec3::X), &[1]);
        // A second bucket can't join one already in the fuel slot.
        let bucket = Stack::new(Item::BUCKET, 1);
        assert!(can_place(Kind::Furnace, 1, Item::BUCKET, None, Some(IVec3::X)));
        assert!(!can_place(Kind::Furnace, 1, Item::BUCKET, Some(&bucket), Some(IVec3::X)));
        assert!(can_take(Kind::Furnace, 1, Item::BUCKET, IVec3::NEG_Y));
        assert!(!can_take(Kind::Furnace, 1, Item::COAL, IVec3::NEG_Y));
        assert!(can_take(Kind::Furnace, 1, Item::COAL, IVec3::X));
    }

    #[test]
    fn plain_containers_offer_every_slot() {
        assert_eq!(slots_for_face(Kind::Plain { slots: 54 }, IVec3::Z).len(), 54);
        assert_eq!(slots_for_face(Kind::Plain { slots: 5 }, IVec3::NEG_Y), &[0, 1, 2, 3, 4]);
        assert!(slots_for_face(Kind::ComposterFull, IVec3::Y).is_empty());
    }
}
