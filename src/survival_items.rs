//! Shared everyday item actions for desktop, controller and device-free players.
use crate::{
    entity::{Entities, PlayerId},
    inventory::Inventory,
    item::Item,
    player::Player,
    world::World,
};

pub fn use_mob(
    player: &Player,
    inventory: &mut Inventory,
    slot: usize,
    creative: bool,
    world: &World,
    entities: &mut Entities,
) -> bool {
    let Some(held) = inventory.get(slot) else { return false };
    let eye = player.eye();
    let dir = player.forward().as_dvec3();
    let reach = world.raycast(eye, dir, 6.0).map_or(6.0, |(p, _)| eye.distance(p.as_dvec3() + glam::DVec3::splat(0.5)));
    let Some((index, _)) = entities.raycast(eye, dir, reach) else { return false };
    if held.item == Item::SHEARS && entities.shear(index) {
        if !creative {
            inventory.wear(slot, 1);
        }
        return true;
    }
    if held.item == Item::BUCKET && entities.mobs[index].kind == crate::entity::MobKind::Cow {
        exchange(inventory, slot, Item::MILK_BUCKET, creative, entities, player);
        return true;
    }
    false
}

/// Throws one snowball or egg. Returns whether the held item was one of those.
pub fn throw_held(
    player: &Player,
    inventory: &mut Inventory,
    slot: usize,
    creative: bool,
    owner: PlayerId,
    entities: &mut Entities,
) -> bool {
    let Some(held) = inventory.get(slot) else { return false };
    if held.item != Item::SNOWBALL && held.item != Item::EGG {
        return false;
    }
    let carry = if player.on_ground { player.vel.with_y(0.0) } else { player.vel };
    entities.throw_projectile(held.item, owner, player.eye(), player.forward().as_dvec3(), carry);
    if !creative {
        inventory.take_one(slot);
    }
    true
}

/// Java's `ItemUtils.createFilledResult`: creative keeps the original and gains
/// `result` once; survival consumes one and returns `result` to the hand,
/// inventory, or the ground.
pub fn exchange(
    inventory: &mut Inventory,
    slot: usize,
    result: Item,
    creative: bool,
    entities: &mut Entities,
    player: &Player,
) {
    if creative {
        if !inventory.slots.iter().flatten().any(|stack| stack.item == result) {
            let _ = inventory.add(result, 1);
        }
        return;
    }
    inventory.take_one(slot);
    if inventory.get(slot).is_none() {
        inventory.slots[slot] = Some(crate::inventory::Stack::new(result, 1));
    } else if inventory.add(result, 1) != 0 {
        entities.throw(crate::inventory::Stack::new(result, 1), player.eye(), player.forward().as_dvec3());
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        entity::{Entities, MobKind},
        inventory::Stack,
        item::Item,
        mining,
        world::block::Block,
    };
    #[test]
    fn shears_stats_speed_drops_and_one_time_shearing() {
        assert_eq!(Item::SHEARS.durability(), Some(238));
        assert_eq!(Item::from_name("shears"), Some(Item::SHEARS));
        assert!(mining::can_harvest(Block::COBWEB, Some(Item::SHEARS)));
        assert_eq!(mining::break_time(Block::COBWEB, Some(Item::SHEARS)), 0.4);
        assert!(mining::break_time(Block::LEAVES, Some(Item::SHEARS)) < 0.05);
        assert!((mining::break_time(Block::WOOL, Some(Item::SHEARS)) - 0.24).abs() < 1e-6);
        let mut entities = Entities::new(7);
        entities.spawn(MobKind::Sheep, glam::DVec3::ZERO);
        assert!(entities.shear(0));
        assert!(!entities.shear(0));
        let Stack { item, count, .. } = entities.items[0].stack;
        assert_eq!(item, Block::WOOL.into());
        assert!((1..=3).contains(&count));
    }

    #[test]
    fn creative_milking_keeps_the_bucket_and_adds_milk_once() {
        use crate::entity::Entities;
        use crate::inventory::Inventory;
        use crate::player::Player;
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(Item::BUCKET, 1));
        let player = Player::new(glam::DVec3::ZERO);
        let mut entities = Entities::new(1);
        super::exchange(&mut inventory, 0, Item::MILK_BUCKET, true, &mut entities, &player);
        assert_eq!(inventory.get(0).unwrap().item, Item::BUCKET);
        assert!(inventory.slots.iter().flatten().any(|stack| stack.item == Item::MILK_BUCKET));
        let milk = inventory.slots.iter().flatten().filter(|stack| stack.item == Item::MILK_BUCKET).count();
        super::exchange(&mut inventory, 0, Item::MILK_BUCKET, true, &mut entities, &player);
        assert_eq!(inventory.slots.iter().flatten().filter(|stack| stack.item == Item::MILK_BUCKET).count(), milk);
    }

    #[test]
    fn snowballs_and_eggs_stack_to_sixteen_and_have_names() {
        assert_eq!(Item::from_name("snowball"), Some(Item::SNOWBALL));
        assert_eq!(Item::from_name("egg"), Some(Item::EGG));
        assert_eq!(Item::SNOWBALL.max_stack(), 16);
        assert_eq!(Item::EGG.max_stack(), 16);
        assert!(Item::creative_palette().any(|item| item == Item::SNOWBALL));
        assert!(Item::creative_palette().any(|item| item == Item::EGG));
    }
}
