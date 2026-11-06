//! Actor-owned continuous brushing, used by desktop/controller/LAN/CLI.
use super::PlayerSession;
use crate::{inventory::HOTBAR_SLOTS, item::Item, world::World};
/// One brush stroke (Java `BrushItem.onUseTick` at use ticks 5, 15, ...):
/// the targeted cell, its block before the stroke, and whether the stroke
/// finished extracting a suspicious block (level event 3008).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub pos: glam::IVec3,
    pub block: crate::world::block::Block,
    pub completed: bool,
}
/// Java spawns `nextInt(7, 12)` dust particles per stroke; this is its mean.
const DUST_PARTICLES: usize = 9;
#[derive(Default)]
pub(super) struct State {
    slot: Option<usize>,
    pub(super) off: bool,
    ticks: u32,
}
impl PlayerSession {
    pub fn brushing(&self) -> bool {
        self.brushing.slot.is_some()
    }
    pub fn start_brushing(&mut self, slot: usize, world: &World) -> bool {
        if self.brushing() {
            return true;
        }
        if slot >= HOTBAR_SLOTS || !self.mode.can_interact() || self.vitals.is_dead() || self.sleeping.is_some() {
            return false;
        }
        let main = self.inventory.get(slot).map(|s| s.item);
        let off = main != Some(Item::BRUSH);
        if off
            && (self.inventory.offhand.map(|s| s.item) != Some(Item::BRUSH)
                || main.is_some_and(|i| {
                    i == Item::SHIELD
                        || i.food().is_some() && self.can_eat(i)
                        || i.is_drink()
                        || matches!(i, Item::BOW | Item::CROSSBOW | Item::CHARGED_CROSSBOW)
                }))
        {
            return false;
        }
        if self.block_target(world).is_none() || self.block_takes_use(slot, world, self.player.sneaking) {
            return false;
        }
        self.brushing = State { slot: Some(slot), off, ticks: 0 };
        true
    }
    pub fn cancel_brushing(&mut self) {
        self.brushing = State::default();
    }
    /// Java BrushItem strokes at use ticks 5,15,...; block cooldown also
    /// prevents two players accelerating extraction. Returns the stroke.
    pub fn tick_brushing(&mut self, slot: usize, using: bool, world: &mut World) -> Option<Stroke> {
        let held = if self.brushing.off { self.inventory.offhand } else { self.inventory.get(slot) };
        if !using
            || self.brushing.slot != Some(slot)
            || held.is_none_or(|s| s.item != Item::BRUSH)
            || !self.mode.can_interact()
            || self.vitals.is_dead()
            || self.sleeping.is_some()
        {
            self.cancel_brushing();
            return None;
        }
        let Some((pos, face)) = self.block_target(world) else {
            self.cancel_brushing();
            return None;
        };
        self.brushing.ticks += 1;
        if self.brushing.ticks > 200 {
            self.cancel_brushing();
            return None;
        }
        if self.brushing.ticks % 10 != 5 {
            return None;
        }
        let block = world.get_block(pos).unwrap_or(crate::world::block::Block::AIR);
        // Dust off the brushed face for any block (`spawnDustParticles`).
        for _ in 0..DUST_PARTICLES {
            world.particles.push(crate::particles::Request::Hit { cell: pos, block, face });
        }
        let completed = world.brush_block(pos, face);
        if completed && !self.mode.is_creative() {
            if self.brushing.off {
                self.inventory.swap_offhand(slot);
                self.inventory.wear(slot, 1);
                self.inventory.swap_offhand(slot);
            } else {
                self.inventory.wear(slot, 1);
            }
        }
        Some(Stroke { pos, block, completed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Entities, PlayerId},
        inventory::Stack,
        session::interact::{UseContext, Used},
        world::{
            archaeology::{SAND, Table},
            block::Block,
            chunk::ChunkData,
            terrain::Generator,
        },
    };
    use glam::{DVec3, IVec3};
    use std::sync::Arc;
    #[test]
    fn both_hands_use_ticks_five_through_ninety_five_and_wear_only_on_completion() {
        for off in [false, true] {
            let mut w = World::new_remote(Arc::new(Generator::new(42)), 2);
            w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
            let at = IVec3::new(8, 145, 8);
            w.set_block(at - IVec3::Y, Block::STONE);
            w.set_block(at, SAND);
            w.set_archaeology_loot(at, Table::Cold, 42);
            let mut s = PlayerSession::new(PlayerId(1), DVec3::new(6.5, 144., 8.5));
            if off {
                s.inventory.offhand = Some(Stack::new(Item::BRUSH, 1));
            } else {
                s.inventory.slots[0] = Some(Stack::new(Item::BRUSH, 1));
            }
            let mut e = Entities::new(42);
            let used = s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).unwrap().unwrap();
            assert_eq!(used.used, Used::BrushStarted);
            for tick in 1..=95 {
                w.maps.game_ticks = tick;
                let stroke = s.tick_brushing(0, true, &mut w);
                assert_eq!(stroke.is_some(), tick % 10 == 5);
                if tick < 95 {
                    assert!(crate::world::archaeology::decode(w.get_block(at).unwrap()).is_some());
                }
            }
            assert_eq!(w.get_block(at), Some(Block::SAND));
            let held = if off { s.inventory.offhand } else { s.inventory.get(0) }.unwrap();
            assert_eq!(held.damage, 1);
            assert_eq!(w.drops.len(), 1);
            s.tick_brushing(0, false, &mut w);
            assert!(!s.brushing());
        }
    }
    #[test]
    fn cli_held_use_extracts_the_same_reward_without_food() {
        use crate::agent::{Agent, Command};
        let mut w = World::new_remote(Arc::new(Generator::new(42)), 2);
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        let at = IVec3::new(8, 145, 8);
        w.set_block(at - IVec3::Y, Block::STONE);
        w.set_block(IVec3::new(6, 143, 8), Block::STONE);
        w.set_block(at, SAND);
        w.set_archaeology_loot(at, Table::Cold, 42);
        let mut cli = Agent::new(DVec3::new(6.5, 144., 8.5));
        cli.session.inventory.slots[0] = Some(Stack::new(Item::BRUSH, 1));
        let mut e = Entities::new(42);
        cli.execute(Command::Eat, &mut w, &mut e, &[]).unwrap();
        for tick in 1..=95 {
            w.maps.game_ticks = tick;
            cli.tick(&mut w, &mut e);
        }
        assert_eq!(w.get_block(at), Some(Block::SAND));
        assert_eq!(cli.session.inventory.get(0).unwrap().damage, 1);
        assert_eq!(w.drops.len(), 1);
        assert_eq!(cli.session.vitals.hunger.food, 20.);
        cli.execute(Command::Run(Default::default(), 1, false), &mut w, &mut e, &[]).unwrap();
        assert!(!cli.session.brushing(), "a replacement movement/wait command releases the brush");
        assert!(!cli.session.using_item(), "cancelled brushing must not keep movement slowed");
    }
    #[test]
    fn brush_in_air_passes_to_offhand_shield() {
        let mut w = World::new_remote(Arc::new(Generator::new(42)), 2);
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        let mut s = PlayerSession::new(PlayerId(1), DVec3::new(6.5, 144., 8.5));
        s.inventory.slots[0] = Some(Stack::new(Item::BRUSH, 1));
        s.inventory.offhand = Some(Stack::new(Item::SHIELD, 1));
        assert!(!s.start_brushing(0, &w));
        assert!(s.start_blocking(0));
        assert_eq!(s.shield_hand(0), Some(crate::session::hands::Hand::Off));
    }
}
