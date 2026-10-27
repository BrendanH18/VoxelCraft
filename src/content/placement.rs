//! Existing door-family states, shared by gameplay adapters without new IDs.
use crate::{
    item::Item,
    world::{
        block::{Block, Facing},
        forms::{self, WoodForm},
        redstone_blocks,
    },
};

pub fn door_halves(item: Item, facing: Facing) -> Option<[Block; 2]> {
    if item == Item::OAK_DOOR {
        return Some([Block::door(facing, false, false), Block::door(facing, false, true)]);
    }
    let block = item.places()?;
    if let Some(halves) = crate::world::copper::door_halves(block, facing) {
        return Some(halves);
    }
    if matches!(redstone_blocks::component(block), Some(redstone_blocks::Component::IronDoor { .. })) {
        return Some([
            redstone_blocks::iron_door(facing, false, false),
            redstone_blocks::iron_door(facing, false, true),
        ]);
    }
    let WoodForm::Door { index, .. } = forms::wood_form(block.0)? else { return None };
    Some([forms::wood_id(index, 14 + facing as u16), forms::wood_id(index, 22)])
}
