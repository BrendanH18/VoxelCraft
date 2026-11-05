//! Shared copper-golem entity uses and statue block callback priority.
use super::{
    PlayerSession,
    interact::{UseContext, Used},
};
use crate::{
    entity::{Entities, copper_golem::Sound},
    inventory::Stack,
    item::{Item, ToolKind},
    world::{World, copper_golem_statue as statue},
};
use glam::{DVec3, IVec3};

impl PlayerSession {
    pub(super) fn use_copper_golem(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        broken: &mut Option<Item>,
    ) -> Option<Used> {
        let i = self.mob_target(world, entities)?;
        let m = &mut entities.mobs[i];
        if !m.alive() {
            return None;
        }
        let state = m.copper_golem.as_mut()?;
        let pos = m.pos;
        let held = self.inventory.get(slot);
        if held.is_none() {
            return entities.return_copper_golem_cargo(i, self.player.pos).then_some(Used::Mob);
        }
        let held = held?;
        let stage = state.stage;
        let mut wear = false;
        let used = if held.item == Item::HONEYCOMB && state.wax() {
            if self.mode.is_survival() {
                self.inventory.take_one(slot);
            }
            Used::Waxed { cell: pos.floor().as_ivec3() }
        } else if held.item.as_tool().is_some_and(|(kind, _)| kind == ToolKind::Axe) && state.scrape() {
            wear = true;
            Used::Scraped { cell: pos.floor().as_ivec3() }
        } else if held.item == Item::SHEARS {
            let flower = state.shear()?;
            entities.drop_copper_golem_equipment(flower, pos + DVec3::Y * 1.5);
            entities.copper_golem_sound(pos, Sound::Shear, stage);
            wear = true;
            Used::Mob
        } else if state.equip_antenna(Stack { count: 1, ..held }).is_some() {
            if self.mode.is_survival() {
                self.inventory.take_one(slot);
            }
            Used::Mob
        } else {
            return None;
        };
        if wear && self.mode.is_survival() && self.inventory.wear(slot, 1) {
            *broken = Some(held.item);
        }
        Some(used)
    }

    pub(super) fn use_copper_statue(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        p: IVec3,
        ctx: &UseContext,
        broken: &mut Option<Item>,
    ) -> Option<Used> {
        let held = self.inventory.get(slot).map(|s| s.item);
        if ctx.sneaking && (held.is_some() || self.inventory.offhand.is_some()) {
            return None;
        }
        let s = world.get_block(p).and_then(statue::decode)?;
        if let Some(item) = held.filter(|i| i.as_tool().is_some_and(|(k, _)| k == ToolKind::Axe)) {
            // Waxed/aged statues pass to the axe item transformer.
            if s.scrape().is_some() {
                return None;
            }
            let revived = entities.revive_copper_golem(world, p);
            if self.mode.is_survival() && self.inventory.wear(slot, 1) {
                *broken = Some(item);
            }
            return revived.then_some(Used::Statue { cell: p });
        }
        // Unwaxed blocks pass to honeycomb; waxed ones consume it as a pose click.
        if !s.waxed && held == Some(Item::HONEYCOMB) {
            return None;
        }
        if world.set_block(p, statue::block(s.cycle_pose())) {
            entities.copper_golem_sound(p.as_dvec3() + DVec3::splat(0.5), Sound::Statue, s.stage);
            Some(Used::Statue { cell: p })
        } else {
            None
        }
    }
}
