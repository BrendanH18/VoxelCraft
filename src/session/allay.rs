//! Shared actor use, including controller, agent and LAN authoritative actions.
use super::{PlayerSession, interact::Used};
use crate::{
    entity::{Entities, allay::Sound},
    inventory::Stack,
    item::Item,
    world::World,
};
impl PlayerSession {
    pub(super) fn use_allay(&mut self, slot: usize, world: &World, entities: &mut Entities) -> Option<Used> {
        let i = self.mob_target(world, entities)?;
        if !entities.mobs[i].alive() || entities.mobs[i].allay.is_none() {
            return None;
        }
        let held = self.inventory.get(slot);
        let pos = entities.mobs[i].pos;
        if held.is_some_and(|s| s.item == Item::AMETHYST_SHARD) && entities.duplicate_allay(i) {
            if self.mode.is_survival() {
                self.inventory.take_one(slot);
            }
            return Some(Used::Mob);
        }
        let state = entities.mobs[i].allay.as_mut().unwrap();
        let sound = if let Some(held) = held.filter(|_| state.liked.is_none()) {
            state.liked = Some(Stack { count: 1, ..held });
            state.player = Some(self.id);
            if self.mode.is_survival() {
                self.inventory.take_one(slot);
            }
            Sound::Give
        } else if held.is_none() && state.liked.is_some() {
            let liked = state.liked.take().unwrap();
            state.player = None;
            self.inventory.add_stack(liked);
            if let Some(stack) = state.cargo.take() {
                entities.drop_from_block(stack, pos.floor().as_ivec3());
            }
            Sound::Take
        } else {
            return None;
        };
        entities.allay_sound(pos, sound);
        Some(Used::Mob)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{MobKind, PlayerId},
        rules::GameMode,
        session::interact::{UseContext, Used},
        world::terrain::Generator,
    };
    use glam::DVec3;
    use std::sync::Arc;
    #[test]
    fn shared_survival_use_gifts_component_item_links_actor_and_returns_both_slots() {
        let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
        world.load_test_column(glam::IVec2::ZERO, 0);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Allay, DVec3::new(2., 11.4, 1.));
        let mut actor = PlayerSession::new(PlayerId(7), DVec3::new(0.5, 10., 1.));
        actor.set_mode(GameMode::Survival);
        let mut stack = Stack::new(Item::DIAMOND, 2);
        stack.name = crate::inventory::StackName::new("Keepsake").unwrap();
        actor.inventory.slots[0] = Some(stack);
        let ctx = UseContext { sneaking: false, others: &[] };
        let result = actor.use_held(0, &mut world, &mut e, &ctx).unwrap().unwrap();
        assert_eq!(result.used, Used::Mob);
        assert_eq!(result.player, PlayerId(7));
        assert_eq!(actor.inventory.get(0).unwrap().count, 1);
        let s = e.mobs[0].allay.as_mut().unwrap();
        assert_eq!(s.player, Some(PlayerId(7)));
        assert_eq!(s.liked, Some(Stack { count: 1, ..stack }));
        s.cargo = Some(Stack { count: 11, ..stack });
        actor.inventory.slots[0] = None;
        assert_eq!(actor.use_held(0, &mut world, &mut e, &ctx).unwrap().unwrap().used, Used::Mob);
        assert_eq!(actor.inventory.get(0), Some(Stack { count: 1, ..stack }));
        assert!(e.mobs[0].allay.as_ref().unwrap().player.is_none());
        assert!(e.mobs[0].allay.as_ref().unwrap().cargo.is_none());
        assert_eq!(e.items[0].stack, Stack { count: 11, ..stack });
    }
    #[test]
    fn shared_dancing_shard_use_consumes_once_and_cooldown_blocks_duplication() {
        let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
        world.load_test_column(glam::IVec2::ZERO, 0);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Allay, DVec3::new(2., 11.4, 1.));
        e.mobs[0].allay.as_mut().unwrap().set_jukebox_playing(glam::IVec3::new(1, 10, 1), true);
        e.mobs[0].allay.as_mut().unwrap().liked = Some(Stack::new(Item::DIAMOND, 1));
        let mut actor = PlayerSession::new(PlayerId(9), DVec3::new(0.5, 10., 1.));
        actor.set_mode(GameMode::Survival);
        actor.inventory.slots[0] = Some(Stack::new(Item::AMETHYST_SHARD, 2));
        let ctx = UseContext { sneaking: false, others: &[] };
        assert_eq!(actor.use_held(0, &mut world, &mut e, &ctx).unwrap().unwrap().used, Used::Mob);
        assert_eq!(e.mobs.len(), 2);
        assert_eq!(actor.inventory.get(0).unwrap().count, 1);
        assert!(!e.duplicate_allay(0));
    }
}
#[cfg(test)]
mod hand_tests {
    use super::*;
    use crate::{
        entity::{MobKind, PlayerId},
        rules::GameMode,
        session::interact::UseContext,
        world::terrain::Generator,
    };
    use glam::DVec3;
    use std::sync::Arc;
    #[test]
    fn shared_offhand_gifting_and_empty_main_hand_return_keep_hand_priority() {
        let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
        world.load_test_column(glam::IVec2::ZERO, 0);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Allay, DVec3::new(2., 11.4, 1.));
        let mut actor = PlayerSession::new(PlayerId(11), DVec3::new(0.5, 10., 1.));
        actor.set_mode(GameMode::Survival);
        actor.inventory.offhand = Some(Stack::new(Item::DIAMOND, 2));
        let ctx = UseContext { sneaking: false, others: &[] };
        assert!(actor.use_held(0, &mut world, &mut e, &ctx).unwrap().unwrap().off_hand);
        assert_eq!(actor.inventory.offhand.unwrap().count, 1);
        assert_eq!(e.mobs[0].allay.as_ref().unwrap().player, Some(PlayerId(11)));
        // Full main hand plus empty offhand cannot take the sample.
        actor.inventory.slots[0] = Some(Stack::new(Item::AMETHYST_SHARD, 1));
        actor.inventory.offhand = None;
        assert!(actor.use_held(0, &mut world, &mut e, &ctx).unwrap().is_none());
        assert!(e.mobs[0].allay.as_ref().unwrap().liked.is_some());
        actor.inventory.slots[0] = None;
        actor.inventory.offhand = Some(Stack::new(Item::AMETHYST_SHARD, 1));
        assert!(!actor.use_held(0, &mut world, &mut e, &ctx).unwrap().unwrap().off_hand);
        assert_eq!(actor.inventory.get(0), Some(Stack::new(Item::DIAMOND, 1)));
        assert_eq!(actor.inventory.offhand.unwrap().item, Item::AMETHYST_SHARD);
        assert!(e.mobs[0].allay.as_ref().unwrap().liked.is_none());
    }
}
