//! Shared fluid-bucket and bottle execution; presentation stays in the client.
//! Reference: Java 26.3 `BucketItem`, `MobBucketItem`, `BottleItem` and
//! `ServerPlayerGameMode.useItemOn`; see tests/fixtures/java/26.3/README.md.
use glam::{DVec3, IVec3};

use crate::{
    content::interaction::block_use_precedes_item,
    entity::Entities,
    inventory::Inventory,
    item::Item,
    player::Player,
    potion::Potion,
    rules::GameMode,
    world::{World, block::Block, terrain::Dimension},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Filled {
        at: IVec3,
        fluid: Block,
    },
    Poured {
        at: IVec3,
        fluid: Block,
    },
    /// A fish bucket's water and creature were released (`BUCKET_EMPTY_FISH`).
    Released {
        at: IVec3,
    },
    Evaporated {
        at: IVec3,
    },
    BottleFilled {
        at: IVec3,
    },
    Creature,
}

/// One right-click with a held item after entity interactions have declined it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// The held item is not a bucket or bottle.
    NotHeld,
    /// The targeted block's own use acts first; the caller performs it.
    Block(IVec3),
    /// `BucketItem.use`/`BottleItem.use` ran; `None` is PASS or FAIL.
    Item(Option<Effect>),
}

pub fn handles(item: Item) -> bool {
    matches!(item, Item::BUCKET | Item::WATER_BUCKET | Item::LAVA_BUCKET | Item::GLASS_BOTTLE)
        || crate::entity::aquatic::bucket_kind(item).is_some()
}

/// The shared input priority for buckets and bottles. Every adapter runs its
/// entity interactions (animal use, milking, mounting, carts) first, then this:
/// Java's block use (doors, levers, buttons, a full composter, containers)
/// precedes `Item.use` unless the player sneaks, then the bucket or bottle acts.
#[allow(clippy::too_many_arguments)]
pub fn use_held(
    player: &Player,
    inventory: &mut Inventory,
    slot: usize,
    mode: GameMode,
    sneaking: bool,
    world: &mut World,
    entities: &mut Entities,
    reach: f64,
) -> Use {
    let Some(held) = inventory.get(slot).map(|stack| stack.item).filter(|&item| handles(item)) else {
        return Use::NotHeld;
    };
    if let Some((pos, _)) = world.raycast(player.eye(), player.forward().as_dvec3(), reach)
        && block_use_precedes_item(world, pos, Some(held), sneaking, (player.eye(), player.forward().as_dvec3()))
    {
        return Use::Block(pos);
    }
    Use::Item(use_bucket(player, inventory, slot, mode, world, entities, reach))
}

/// Execute one bucket or bottle use (`Item.use`). Spectators cannot use items;
/// Adventure players may capture creatures and fill bottles, but
/// `BucketItem.use` fails without `Player.mayUseItemAt` (`Abilities.mayBuild`).
pub fn use_bucket(
    player: &Player,
    inventory: &mut Inventory,
    slot: usize,
    mode: GameMode,
    world: &mut World,
    entities: &mut Entities,
    reach: f64,
) -> Option<Effect> {
    if !mode.can_interact() {
        return None;
    }
    let creative = mode.is_creative();
    let stack = inventory.get(slot)?;
    let held = stack.item;
    let eye = player.eye();
    let dir = player.forward().as_dvec3();
    if entities.capture_water_creature(world, inventory, slot, creative, eye, dir, reach) {
        return Some(Effect::Creature);
    }
    let creature = crate::entity::aquatic::bucket_kind(held).is_some();
    if held != Item::GLASS_BOTTLE && !mode.can_build() {
        return None;
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
            let fluid = if held == Item::WATER_BUCKET { Block::WATER } else { Block::LAVA };
            let effect = pour(world, eye, dir, reach, fluid, player.sneaking)?;
            // Creative keeps its filled bucket and does not receive an empty one.
            if !creative {
                super::exchange(inventory, slot, Item::BUCKET, false, entities, player);
            }
            Some(effect)
        }
        _ if creature => {
            let effect = pour(world, eye, dir, reach, Block::WATER, player.sneaking)?;
            let (at, evaporated) = match effect {
                Effect::Poured { at, .. } => (at, false),
                Effect::Evaporated { at } => (at, true),
                _ => unreachable!("pouring only reports poured or evaporated fluid"),
            };
            // `MobBucketItem.checkExtraContent` spawns even where the water evaporated.
            entities.release_water_creature(stack, at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
            if !creative {
                super::exchange(inventory, slot, Item::BUCKET, false, entities, player);
            }
            // `BucketItem.emptyContents` plays the empty sound only when the water stays.
            Some(if evaporated { effect } else { Effect::Released { at } })
        }
        _ => None,
    }
}

/// VoxelCraft's Java `LiquidBlockContainer` blocks: kelp, seagrass, sea
/// pickles and live coral, all of which always hold water here, so Java's
/// `canPlaceLiquid` is false and `placeLiquid` changes nothing.
fn holds_poured_water(b: Block) -> bool {
    b.is_waterlogged()
}

/// `BucketItem.use` with a raycast that ignores fluids (`ClipContext.Fluid.NONE`).
fn pour(world: &mut World, eye: DVec3, dir: DVec3, reach: f64, fluid: Block, sneaking: bool) -> Option<Effect> {
    let (hit, normal) = world.raycast(eye, dir, reach)?;
    empty_contents(world, hit, normal, fluid, sneaking)
}

/// Java `BucketItem.use` target choice and `emptyContents` for a block hit
/// at `hit` through the face `normal`. Water targets a water-holding hit block
/// in place; otherwise the cell on the hit face. An unplaceable first choice
/// (or sneaking at a water holder) retries the hit face. A replaceable or
/// non-solid, non-fluid block there is destroyed with its drops first.
pub fn empty_contents(world: &mut World, hit: IVec3, normal: IVec3, fluid: Block, sneaking: bool) -> Option<Effect> {
    let water = fluid == Block::WATER;
    let face = hit + normal;
    let first = if water && holds_poured_water(world.get_block(hit)?) { hit } else { face };
    let placeable = |world: &World, at: IVec3, from_hit: bool| {
        world.get_block(at).is_some_and(|b| b == Block::AIR || (b.fluid_replaceable() && !(sneaking && from_hit)))
    };
    let at = if placeable(world, first, true) {
        first
    } else if placeable(world, face, false) {
        face
    } else {
        return None;
    };
    if water && world.generator.dimension == Dimension::Nether {
        return Some(Effect::Evaporated { at });
    }
    let old = world.get_block(at)?;
    if water && holds_poured_water(old) {
        return Some(Effect::Poured { at, fluid });
    }
    if old != Block::AIR && !old.is_fluid() {
        // `Level.destroyBlock(pos, true)`: the replaced block drops its loot.
        world.spill_block(at, old);
    }
    world.set_block(at, fluid);
    Some(Effect::Poured { at, fluid })
}
