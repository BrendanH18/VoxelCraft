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
    world::{World, block::Block, terrain::Dimension, waterlog},
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
        Item::GLASS_BOTTLE => {
            // `BottleItem.use`: any water source fills it, including the
            // water a waterlogged block holds; the water stays.
            let (at, _) = world.raycast_sources(eye, dir, reach)?;
            if !waterlog::holds_source(world.get_block(at)?) {
                return None;
            }
            super::exchange(inventory, slot, Item::potion(Potion::WATER), creative, entities, player);
            Some(Effect::BottleFilled { at })
        }
        Item::BUCKET => {
            let (at, _) = world.raycast_sources(eye, dir, reach)?;
            let (result, fluid) = pick_up(world, at)?;
            super::exchange(inventory, slot, result, creative, entities, player);
            Some(Effect::Filled { at, fluid })
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

/// `BucketPickup.pickupBlock` for the block at `at`: a water or lava source
/// becomes air (`LiquidBlock`), a waterlogged block gives up its water
/// (`SimpleWaterloggedBlock`). Returns the filled bucket and the fluid taken.
/// Kelp, seagrass and flowing fluid can't be picked up.
pub fn pick_up(world: &mut World, at: IVec3) -> Option<(Item, Block)> {
    match world.get_block(at)? {
        Block::WATER => {
            world.set_block(at, Block::AIR);
            Some((Item::WATER_BUCKET, Block::WATER))
        }
        Block::LAVA => {
            world.set_block(at, Block::AIR);
            Some((Item::LAVA_BUCKET, Block::LAVA))
        }
        b if waterlog::wet(b.0) => {
            world.drain_waterlogged(at);
            Some((Item::WATER_BUCKET, Block::WATER))
        }
        _ => None,
    }
}

/// `BucketItem.use` with a raycast that ignores fluids (`ClipContext.Fluid.NONE`).
fn pour(world: &mut World, eye: DVec3, dir: DVec3, reach: f64, fluid: Block, sneaking: bool) -> Option<Effect> {
    let (hit, normal) = world.raycast(eye, dir, reach)?;
    empty_contents(world, hit, normal, fluid, sneaking)
}

/// Java `BucketItem.use` target choice and `emptyContents` for a block hit
/// at `hit` through the face `normal`. Water goes into the hit block itself
/// when it is a `LiquidBlockContainer` (a waterloggable block, kelp or
/// seagrass); otherwise into the cell on the hit face. A first choice that
/// can't take the fluid (or a sneaking click on one that could) retries the
/// hit face.
pub fn empty_contents(world: &mut World, hit: IVec3, normal: IVec3, fluid: Block, sneaking: bool) -> Option<Effect> {
    let first =
        if fluid == Block::WATER && waterlog::liquid_container(world.get_block(hit)?) { hit } else { hit + normal };
    empty_at(world, first, fluid, Some((hit + normal, sneaking)))
}

/// Java `BucketItem.emptyContents` at `at`. `retry` is the hit-face cell
/// and whether the player sneaks, for a player's click (`hitResult`); a
/// dispenser passes `None`.
///
/// The fluid goes in when the cell is air, or when it can take it (a
/// non-solid or replaceable block, or water into a waterloggable block)
/// and the player isn't sneaking. Water evaporates in the Nether, fills a
/// waterloggable block (`placeLiquid`; a full one, kelp and seagrass take
/// the bucket without change), and otherwise replaces the block, which
/// drops its loot first.
pub fn empty_at(world: &mut World, at: IVec3, fluid: Block, retry: Option<(IVec3, bool)>) -> Option<Effect> {
    let water = fluid == Block::WATER;
    let old = world.get_block(at)?;
    let may_replace = old.fluid_replaceable();
    // `canPlaceLiquid`: a waterloggable block takes water, wet or dry (a wet
    // one's `placeLiquid` then changes nothing, and the bucket still empties,
    // as in Java); kelp and seagrass take nothing.
    let place_liquid = may_replace || (water && old.waterloggable());
    let sneaking = retry.is_some_and(|(_, sneaking)| sneaking);
    if !(old == Block::AIR || (place_liquid && !sneaking)) {
        return match retry {
            Some((face, _)) => empty_at(world, face, fluid, None),
            None => None,
        };
    }
    if water && world.generator.dimension == Dimension::Nether {
        return Some(Effect::Evaporated { at });
    }
    if water && waterlog::liquid_container(old) {
        world.fill_waterlogged(at);
        return Some(Effect::Poured { at, fluid });
    }
    if may_replace && old != Block::AIR && !old.is_fluid() {
        // `Level.destroyBlock(pos, true)`: the replaced block drops its loot.
        world.spill_block(at, old);
    }
    world.set_block(at, fluid);
    Some(Effect::Poured { at, fluid })
}
