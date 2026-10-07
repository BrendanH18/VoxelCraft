//! Shared everyday item actions for desktop, controller and device-free players.
use crate::{entity::Entities, inventory::Inventory, item::Item, player::Player, world::World};

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
    false
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
}
