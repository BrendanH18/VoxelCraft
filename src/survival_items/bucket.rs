//! Shared fluid-bucket and bottle execution; presentation stays in the client.
use glam::IVec3;

use crate::{
    entity::Entities,
    inventory::Inventory,
    item::Item,
    player::Player,
    potion::Potion,
    world::{World, block::Block, terrain::Dimension},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Filled { at: IVec3, fluid: Block },
    Poured { at: IVec3, fluid: Block },
    Evaporated { at: IVec3 },
    BottleFilled { at: IVec3 },
    Creature,
}

pub fn handles(item: Item) -> bool {
    matches!(item, Item::BUCKET | Item::WATER_BUCKET | Item::LAVA_BUCKET | Item::GLASS_BOTTLE)
        || crate::entity::aquatic::bucket_kind(item).is_some()
}

/// Execute one action after the caller validates its player/game mode.
/// A missed creature capture falls through to ordinary water placement.
pub fn use_bucket(
    player: &Player,
    inventory: &mut Inventory,
    slot: usize,
    creative: bool,
    world: &mut World,
    entities: &mut Entities,
    reach: f64,
) -> Option<Effect> {
    let held = inventory.get(slot)?.item;
    let eye = player.eye();
    let dir = player.forward().as_dvec3();
    if entities.use_water_creature_bucket(world, inventory, slot, creative, eye, dir, reach) {
        return Some(Effect::Creature);
    }
    match held {
        Item::BUCKET | Item::GLASS_BOTTLE => {
            let (at, _) = world.raycast_sources(eye, dir, reach)?;
            let fluid = world.get_block(at)?;
            let result = match (held, fluid) {
                (Item::BUCKET, Block::WATER) => Item::WATER_BUCKET,
                (Item::BUCKET, Block::LAVA) => Item::LAVA_BUCKET,
                (Item::GLASS_BOTTLE, Block::WATER) => Item::potion(Potion::WATER),
                _ => return None,
            };
            if held == Item::BUCKET {
                world.set_block(at, Block::AIR);
            }
            super::exchange(inventory, slot, result, creative, entities, player);
            Some(if held == Item::BUCKET { Effect::Filled { at, fluid } } else { Effect::BottleFilled { at } })
        }
        Item::WATER_BUCKET | Item::LAVA_BUCKET => {
            let (hit, normal) = world.raycast(eye, dir, reach)?;
            let at = if world.get_block(hit)?.is_replaceable() { hit } else { hit + normal };
            if !world.get_block(at)?.is_replaceable() {
                return None;
            }
            let fluid = if held == Item::WATER_BUCKET { Block::WATER } else { Block::LAVA };
            let evaporates = fluid == Block::WATER && world.generator.dimension == Dimension::Nether;
            if !evaporates {
                world.set_block(at, fluid);
            }
            // Creative keeps its filled bucket and does not receive an empty one.
            if !creative {
                super::exchange(inventory, slot, Item::BUCKET, false, entities, player);
            }
            Some(if evaporates { Effect::Evaporated { at } } else { Effect::Poured { at, fluid } })
        }
        _ => None,
    }
}
