//! Java 26.3 harnesses (`Items.HARNESS`): sixteen dyed body-slot items that
//! only a happy ghast can wear. Item ids 1585..=1600 in `DyeColor` order
//! (v0.17 build-5000 reservation).
//!
//! This is the item side: the items, their crafting and re-dyeing
//! (`crafting::harness`) and the `Equippable.harness` data the happy ghast
//! will read. A player can't wear one (`Equippable.swapWithEquipmentSlot`
//! fails for entities outside `can_equip_harness`), so using one in the air
//! does nothing. Checked against `tests/fixtures/java/26.3/harnesses.json`
//! in `tests/java_harnesses.rs`.

use crate::color::DyeColor;
use crate::entity::equipment::Slot;
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

/// The white harness; the other colours follow in `DyeColor` order.
pub const ITEM_FIRST: u16 = 1585;
/// One past the black harness.
pub const ITEM_END: u16 = ITEM_FIRST + 16;
const _: () = assert!(ITEM_END == 1601);

/// `Items.HARNESS`'s `stacksTo(1)`.
pub const MAX_STACK: u8 = 1;

/// `Equippable.harness`: the slot it is worn in.
pub const SLOT: Slot = Slot::Body;
/// `SoundEvents.HARNESS_EQUIP`, played when one is put on.
pub const EQUIP_SOUND: &str = "entity.happy_ghast.equip";
/// `SoundEvents.HARNESS_UNEQUIP`, played when shears take one off.
pub const SHEARING_SOUND: &str = "entity.happy_ghast.unequip";
/// The `can_equip_harness` entity tag.
pub const ALLOWED_ENTITIES: &[&str] = &["happy_ghast"];
/// `setEquipOnInteract(true)`: using one on a happy ghast puts it on.
pub const EQUIP_ON_INTERACT: bool = true;
/// `setCanBeSheared(true)`: shears take it off again.
pub const CAN_BE_SHEARED: bool = true;

const NAMES: [&str; 16] = [
    "white harness",
    "orange harness",
    "magenta harness",
    "light blue harness",
    "yellow harness",
    "lime harness",
    "pink harness",
    "gray harness",
    "light gray harness",
    "cyan harness",
    "purple harness",
    "blue harness",
    "brown harness",
    "green harness",
    "red harness",
    "black harness",
];

/// The harness of colour `c`.
pub const fn harness(c: DyeColor) -> Item {
    Item(ITEM_FIRST + c as u16)
}

/// The colour of a harness item.
pub fn colour(item: Item) -> Option<DyeColor> {
    item.0.checked_sub(ITEM_FIRST).filter(|&i| i < 16).map(|i| DyeColor::ALL[i as usize])
}

/// Every harness, in `DyeColor` order (the `harnesses` item tag's order).
pub fn all() -> impl Iterator<Item = Item> {
    (ITEM_FIRST..ITEM_END).map(Item)
}

/// Whether `entity` (a Java entity id) can wear a harness.
pub fn can_equip(entity: &str) -> bool {
    ALLOWED_ENTITIES.contains(&entity.strip_prefix("minecraft:").unwrap_or(entity))
}

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    let c = colour(item)?;
    Some(ItemInfo {
        name: NAMES[c as usize],
        kind: ItemKind::Material,
        max_stack: MAX_STACK,
        sprite: Sprite::Harness(c.rgb()),
    })
}
