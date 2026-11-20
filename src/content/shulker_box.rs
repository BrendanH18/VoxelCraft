//! Java 26.3 shulker box items (v0.16 Track B): the plain box and its
//! sixteen dyed forms, block items of `world::shulker_box`. A box item
//! carries its contents as Java's `container` component
//! ([`Payload::Container`]), copied from the block entity when the box
//! breaks (`copy_components` in `blocks/*shulker_box`) and back when it is
//! placed. Shulker boxes don't go inside shulker boxes or bundles
//! (`BlockItem.canFitInsideContainerItems`, `ShulkerBoxSlot.mayPlace`,
//! `ShulkerBoxBlockEntity.canPlaceItemThroughFace`).

use crate::color::DyeColor;
use crate::inventory::Stack;
use crate::inventory::components::Payload;
use crate::item::Item;
use crate::world::chest::SLOTS;
use crate::world::shulker_box as block;

/// `ItemContainerContents.MAX_SIZE`.
pub const MAX_CONTAINER_SLOTS: usize = 256;
/// `ItemContainerContents.addToTooltip` names this many stacks, then counts
/// the rest.
pub const TOOLTIP_STACKS: usize = 5;

/// The item of a box of colour `colour` (`None` for the plain box).
pub fn item(colour: Option<DyeColor>) -> Item {
    Item::from_block(block::item_state(colour))
}

/// Whether `item` is one of the seventeen shulker boxes (`#shulker_boxes`).
pub fn is_shulker_box(item: Item) -> bool {
    colour(item).is_some()
}

/// A shulker box item's colour: `Some(None)` for the plain box, `None` for
/// any other item.
pub fn colour(item: Item) -> Option<Option<DyeColor>> {
    let b = item.block()?;
    block::decode(b).filter(|s| block::item_state(s.colour) == b).map(|s| s.colour)
}

/// Every box in `#shulker_boxes` order: plain, then `DyeColor` order.
pub fn all() -> impl Iterator<Item = Item> {
    std::iter::once(None).chain(DyeColor::ALL.map(Some)).map(item)
}

/// `Item.canFitInsideContainerItems`: false for shulker boxes only.
pub fn can_fit_inside_container(stack: &Stack) -> bool {
    !is_shulker_box(stack.item)
}

/// The 27 slots a box item holds (empty past what its component lists).
pub fn contents(stack: &Stack) -> [Option<Stack>; SLOTS] {
    let mut out = [None; SLOTS];
    if let Some(Payload::Container(slots)) = stack.payload() {
        for (cell, s) in out.iter_mut().zip(slots) {
            *cell = *s;
        }
    }
    out
}

/// `ItemContainerContents.fromItems`: `stack` holding `slots` (no
/// component when they are all empty).
pub fn with_contents(stack: Stack, slots: &[Option<Stack>]) -> Stack {
    let last = slots.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
    Stack { payload: Payload::Container(slots[..last].to_vec()).intern(), ..stack }
}

/// `ItemContainerContents.addToTooltip`: the first five stacks as "name x
/// count", then "and N more...". (26.3 names the line
/// `item.container.item_count`, which en_us leaves untranslated; the
/// shulker box's older `%s x %s` wording is used.)
pub fn tooltip_lines(stack: &Stack) -> Vec<String> {
    let Some(Payload::Container(slots)) = stack.payload() else { return Vec::new() };
    let items: Vec<&Stack> = slots.iter().flatten().collect();
    let mut out: Vec<String> =
        items.iter().take(TOOLTIP_STACKS).map(|s| format!("{} x {}", s.display_name(), s.count)).collect();
    if items.len() > TOOLTIP_STACKS {
        out.push(format!("and {} more...", items.len() - TOOLTIP_STACKS));
    }
    out
}

/// The sixteen `crafting_transmute` recipes (`<colour>_shulker_box.json`):
/// any shulker box and one dye make that colour's box with every component
/// kept. `None` when the grid isn't one box and one dye; `Some(None)` when
/// Java's `matches` refuses it (the box is that colour already).
pub fn transmute(occupied: &[Stack]) -> Option<Option<Stack>> {
    let [a, b] = occupied else { return None };
    let (shulker, dye) = if is_shulker_box(a.item) { (a, b) } else { (b, a) };
    if !is_shulker_box(shulker.item) {
        return None;
    }
    let c = dye.item.dye_color()?;
    let result = Stack { item: item(Some(c)), count: 1, ..*shulker };
    Some((!result.stacks_with(shulker)).then_some(result))
}

/// `CauldronInteractions.shulkerBoxInteraction`: a dyed box comes out plain
/// (`transmuteCopy(SHULKER_BOX)`, contents kept). `None` for anything else,
/// the plain box included (it isn't in the water map).
pub fn washed(stack: Stack) -> Option<Stack> {
    colour(stack.item)??;
    Some(Stack { item: item(None), count: 1, ..stack })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seventeen_boxes_with_colours_and_contents() {
        let boxes: Vec<Item> = all().collect();
        assert_eq!(boxes.len(), 17);
        assert_eq!(colour(boxes[0]), Some(None));
        assert_eq!(colour(boxes[16]), Some(Some(DyeColor::Black)));
        assert!(boxes.iter().all(|&b| b.max_stack() == 1));
        assert_eq!(boxes[0].name(), "shulker box");
        assert_eq!(boxes[4].name(), "light blue shulker box");
        let mut slots = [None; SLOTS];
        slots[3] = Some(Stack::new(Item::DIAMOND, 5));
        slots[20] = Some(Stack::new(Item::APPLE, 1));
        let full = with_contents(Stack::new(boxes[0], 1), &slots);
        assert_eq!(contents(&full), slots);
        assert_eq!(tooltip_lines(&full), ["diamond x 5", "apple x 1"]);
        assert_eq!(with_contents(Stack::new(boxes[0], 1), &[None; SLOTS]), Stack::new(boxes[0], 1));
        let text = crate::inventory::stack_to_string(Some(full));
        assert_eq!(crate::inventory::stack_from_str(&text), Some(Some(full)));
        assert!(!can_fit_inside_container(&full));
        assert!(can_fit_inside_container(&Stack::new(Item::DIAMOND, 1)));
    }

    #[test]
    fn dyeing_and_washing_keep_contents() {
        let mut slots = [None; SLOTS];
        slots[0] = Some(Stack::new(Item::STICK, 64));
        let plain = with_contents(Stack::new(item(None), 1), &slots);
        let red = transmute(&[plain, Stack::new(DyeColor::Red.dye(), 1)]).unwrap().unwrap();
        assert_eq!((red.item, contents(&red)), (item(Some(DyeColor::Red)), slots));
        assert_eq!(transmute(&[red, Stack::new(DyeColor::Red.dye(), 1)]), Some(None));
        assert_eq!(washed(red), Some(plain));
        assert_eq!(washed(plain), None);
        let lines = tooltip_lines(&with_contents(Stack::new(item(None), 1), &[Some(Stack::new(Item::STICK, 1)); 7]));
        assert_eq!(lines.last().unwrap(), "and 2 more...");
    }
}
