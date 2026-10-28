//! Right-clicking this module's blocks (each block's `useItemOn` and
//! `useWithoutItem`), shared by every input path through
//! `session::interact`.

use glam::IVec3;

use super::{
    Campfire, Contents, anchor_charges, campfire, campfire_of, cauldron, cauldron_of, is_pot, potted, potted_plant,
    respawn_anchor,
};
use crate::entity::Entities;
use crate::inventory::{Inventory, Stack};
use crate::item::{Item, ToolKind};
use crate::player::Player;
use crate::rules::GameMode;
use crate::world::{World, block::Block, terrain::Dimension};

/// What a use did, for adapters' sounds and the host's follow-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// The block took the click but nothing changed (Java's `CONSUME`).
    Consumed,
    PotPlanted,
    PotEmptied,
    /// An item went into a decorated pot (`DECORATED_POT_INSERT`).
    PotFilled,
    /// A bucket or bottle traded with a cauldron (`fill`: into the cauldron).
    Cauldron {
        fill: bool,
        lava: bool,
        snow: bool,
        bottle: bool,
    },
    FoodPlaced,
    Lit,
    Doused,
    AnchorCharged,
    /// An ender chest with room above it opens the player's ender inventory.
    EnderChest,
    /// A charged anchor in the Nether: the host sets the player's spawn.
    AnchorSpawn,
    /// A charged anchor outside the Nether: the block is gone and the host
    /// makes Java's power-5 fiery explosion there ([`World::take_anchor_blasts`]),
    /// whoever used it.
    AnchorExploded,
}

/// `RespawnAnchorBlock.isRespawnFuel`.
fn anchor_fuel(item: Option<Item>) -> bool {
    item == Some(Block::GLOWSTONE.into())
}

/// Whether the block at `pos` acts for `held` before the item's own use
/// (so buckets fill cauldrons instead of scooping the water inside).
pub fn takes_use(world: &World, pos: IVec3, held: Option<Item>) -> bool {
    let Some(b) = world.get_block(pos) else { return false };
    is_pot(b)
        || super::decorated_pot_of(b).is_some()
        || super::ender_chest_of(b).is_some()
        || super::cauldron::takes(b, held)
        || (campfire_of(b).is_some() && held.is_some_and(|i| super::campfire::cooked(i).is_some()))
        || anchor_charges(b).is_some_and(|c| (anchor_fuel(held) && c < 4) || c > 0)
}

/// Runs the block use at `pos` for the item in `slot`; `None` when the
/// block doesn't take it (placement and other item uses follow).
#[allow(clippy::too_many_arguments)]
pub fn use_block(
    world: &mut World,
    entities: &mut Entities,
    player: &Player,
    inventory: &mut Inventory,
    slot: usize,
    mode: GameMode,
    pos: IVec3,
    face: IVec3,
) -> Option<Effect> {
    let b = world.get_block(pos)?;
    let held = inventory.get(slot).map(|s| s.item);
    let creative = mode.is_creative();
    let take = |inventory: &mut Inventory| {
        if !creative {
            inventory.take_one(slot);
        }
    };
    if is_pot(b) {
        // `FlowerPotBlock.useItemOn`: a pottable plant goes into an empty
        // pot (a full one takes the click); anything else, or nothing,
        // takes the plant out (`useWithoutItem`).
        let planted = held.and_then(Item::block).and_then(potted);
        if let Some(full) = planted {
            if b != super::FLOWER_POT {
                return Some(Effect::Consumed);
            }
            world.set_block(pos, full);
            take(inventory);
            return Some(Effect::PotPlanted);
        }
        let plant = potted_plant(b)?;
        world.set_block(pos, super::FLOWER_POT);
        if inventory.add(plant, 1) != 0 {
            entities.throw(Stack::new(plant, 1), player.eye(), player.forward().as_dvec3());
        }
        return Some(Effect::PotEmptied);
    }
    if super::ender_chest_of(b).is_some() {
        return Some(if super::ender_chest_opens(world, pos) { Effect::EnderChest } else { Effect::Consumed });
    }
    if super::decorated_pot_of(b).is_some() {
        // `DecoratedPotBlock.useItemOn`: one of the held item goes in when
        // the pot is empty or holds less than a stack of the same; otherwise
        // (or empty-handed) it only wobbles.
        if let Some(stack) = inventory.get(slot)
            && world.decorated_pot_insert(pos, Stack { count: 1, ..stack })
        {
            take(inventory);
            return Some(Effect::PotFilled);
        }
        return Some(Effect::Consumed);
    }
    if let Some(contents) = cauldron_of(b) {
        let item = held?;
        let (next, result) = super::cauldron::exchange(contents, item)?;
        world.set_block(pos, cauldron(next));
        crate::survival_items::exchange(inventory, slot, result, creative, entities, player);
        let fill = matches!(item, Item::WATER_BUCKET | Item::LAVA_BUCKET | Item::POWDER_SNOW_BUCKET)
            || item.as_potion().is_some();
        let kind = if fill { next } else { contents };
        return Some(Effect::Cauldron {
            fill,
            lava: kind == Contents::Lava,
            snow: matches!(kind, Contents::PowderSnow(_)),
            bottle: item == Item::GLASS_BOTTLE || item.as_potion().is_some(),
        });
    }
    if let Some(c) = campfire_of(b) {
        let item = held?;
        // `CampfireBlock.useItemOn`: food with a campfire recipe goes on.
        if super::campfire::cooked(item).is_some() {
            if world.campfire_place_food(pos, item) {
                take(inventory);
                return Some(Effect::FoodPlaced);
            }
            return Some(Effect::Consumed);
        }
        // `FlintAndSteelItem`/`FireChargeItem.useOn`: light a dry, unlit fire.
        if (item == Item::FLINT_AND_STEEL || item == Item::FIRE_CHARGE)
            && super::campfire::can_light(b)
            && mode.can_build()
        {
            world.set_block(pos, campfire(Campfire { lit: true, ..c }));
            if !creative {
                if item == Item::FIRE_CHARGE {
                    inventory.take_one(slot);
                } else {
                    inventory.wear(slot, 1);
                }
            }
            return Some(Effect::Lit);
        }
        // `ShovelItem.useOn`: dowse a lit fire (never from below).
        if item.as_tool().is_some_and(|(kind, _)| kind == ToolKind::Shovel)
            && c.lit
            && face != IVec3::NEG_Y
            && mode.can_build()
        {
            world.set_block(pos, campfire(Campfire { lit: false, ..c }));
            if !creative {
                inventory.wear(slot, crate::mining::wear(item, false));
            }
            return Some(Effect::Doused);
        }
        return None;
    }
    if let Some(charges) = anchor_charges(b) {
        // `RespawnAnchorBlock.useItemOn`: glowstone adds a charge.
        if anchor_fuel(held) && charges < 4 {
            world.set_block(pos, respawn_anchor(charges + 1));
            take(inventory);
            return Some(Effect::AnchorCharged);
        }
        if charges == 0 {
            return None;
        }
        // `useWithoutItem`: outside the Nether it blows up.
        if world.generator.dimension != Dimension::Nether {
            world.set_block(pos, Block::AIR);
            world.utilities.anchor_blasts.push(pos);
            return Some(Effect::AnchorExploded);
        }
        return Some(Effect::AnchorSpawn);
    }
    None
}
