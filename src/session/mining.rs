//! Actor-owned mining progress and block destruction; adapters present outcomes.
use super::PlayerSession;
use crate::{
    enchant::Enchantment,
    entity::{Entities, PlayerId},
    inventory::HOTBAR_SLOTS,
    item::Item,
    mining,
    particles::Request,
    rules::GameRules,
    simulation::survival,
    world::{World, block::Block},
};
use glam::IVec3;

#[derive(Default)]
pub(super) struct State {
    target: Option<(IVec3, Block, usize)>,
    progress: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Hit { player: PlayerId, cell: IVec3, block: Block, progress: f32 },
    Broken { player: PlayerId, cell: IVec3, block: Block, tool_broken: Option<Item> },
    TeleportedEgg { player: PlayerId, destination: Option<IVec3> },
}

impl PlayerSession {
    pub fn cancel_mining(&mut self) {
        self.mining = State::default();
    }

    pub fn breaking(&self, world: &World) -> Option<(IVec3, f32)> {
        let (pos, block, _) = self.mining.target?;
        (world.get_block(pos) == Some(block)).then_some((pos, self.mining.progress.min(1.)))
    }

    /// Validate and advance one fixed tick against a raycast the engine computes.
    /// Creative is immediate; Survival uses the existing per-tick speed modifiers.
    pub fn tick_mining(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        rules: &GameRules,
    ) -> Result<Outcome, &'static str> {
        if !self.mode.can_build() || self.vitals.is_dead() || self.sleeping.is_some() {
            return Err("player cannot mine now");
        }
        if slot >= HOTBAR_SLOTS {
            return Err("selected slot out of range");
        }
        let feet = self.player.pos.floor().as_ivec3();
        if !self.player.pos.is_finite()
            || !self.player.yaw.is_finite()
            || !self.player.pitch.is_finite()
            || !world.is_loaded(feet)
            || !world.is_loaded(feet - IVec3::Y)
        {
            return Err("player terrain or view is unavailable");
        }
        let held = self.inventory.get(slot);
        if held.is_some_and(|s| s.count == 0 || s.count > s.item.max_stack()) {
            return Err("invalid held stack");
        }
        let (pos, face) = self.block_target(world).ok_or("no block in reach")?;
        let block = world.get_block(pos).ok_or("target is unloaded")?;
        if block == Block::DRAGON_EGG {
            self.cancel_mining();
            return Ok(Outcome::TeleportedEgg { player: self.id, destination: world.teleport_egg(pos) });
        }
        let key = Some((pos, block, slot));
        let digger = mining::Digger {
            held,
            helmet: self.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.player.head_in_water(world),
            on_ground: self.player.on_ground || self.player.flying,
        };
        let before = if self.mining.target == key { self.mining.progress } else { 0. };
        let progress = if self.mode.is_creative() {
            1.
        } else {
            before + mining::dig_progress(block, digger, self.vitals.effects.mining_factor())
        };
        world.touch_redstone_ore(pos);
        // Touching ore may replace the target with its lit state; track that state.
        self.mining = State { target: Some((pos, world.get_block(pos).unwrap_or(block), slot)), progress };
        if progress < 1. {
            world.particles.push(Request::Hit { cell: pos, block, face });
            return Ok(Outcome::Hit { player: self.id, cell: pos, block, progress });
        }
        let tool = held.map_or(Default::default(), |s| s.active_enchants());
        let melts = self.mode.is_survival()
            && block == Block::ICE
            && !tool.has(Enchantment::SilkTouch)
            && world.generator.dimension != crate::world::terrain::Dimension::Nether
            && world.get_block(pos - IVec3::Y).is_some_and(crate::content::mining::ice_support);
        // All eligibility checks precede mutations. Neighbor drops use the same gamerule.
        world.set_tile_drops(rules.bool("doTileDrops"));
        if held.is_some_and(|s| s.item == Item::SHEARS) {
            world.disarm_tripwire(pos);
        }
        world.set_block(pos, if melts { Block::WATER } else { Block::AIR });
        if melts {
            world.particles.push(Request::Break { cell: pos, block });
        }
        if crate::entity::nether::guarded_by_piglins(block) {
            entities.piglins_notice(self.id, self.player.pos, false, world);
        }
        if self.mode.is_survival() && rules.bool("doTileDrops") && mining::can_harvest(block, held.map(|s| s.item)) {
            world.spill_with_item(pos, block, held);
            entities.drop_mined_xp(block, pos, tool);
        }
        if block.is_bed() {
            world.break_bed_partner(pos, block, self.mode.is_survival() && rules.bool("doTileDrops"));
        }
        if block.is_door_upper() {
            let below = pos - IVec3::Y;
            if let Some(lower) = world.get_block(below).filter(|b| b.is_door() && !b.is_door_upper()) {
                world.set_block(below, Block::AIR);
                if self.mode.is_survival() && rules.bool("doTileDrops") {
                    world.spill_with_item(below, lower, held);
                }
            }
        }
        let tool_broken = if self.mode.is_survival() {
            self.vitals.hunger.exhaust(survival::EXHAUST_MINE);
            held.filter(|s| block.hardness() > 0. && self.inventory.wear(slot, mining::wear(s.item, false)))
                .map(|s| s.item)
        } else {
            None
        };
        self.cancel_mining();
        Ok(Outcome::Broken { player: self.id, cell: pos, block, tool_broken })
    }
}
