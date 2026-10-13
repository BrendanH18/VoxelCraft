//! Actor-owned mining progress and block destruction; adapters present outcomes.
//!
//! Timing follows Java's client `MultiPlayerGameMode` start/continue split as
//! driven by `Minecraft.startAttack`/`continueAttack`: the first held tick
//! starts (and, in Creative or for an instant block, destroys) and then
//! continues in the same tick; later ticks continue. A destroy delay of five
//! ticks follows every Creative destruction and every completed timed
//! Survival destruction. Destruction itself follows the server's
//! `ServerPlayerGameMode.destroyBlock`.
use super::PlayerSession;
use crate::{
    enchant::Enchantment,
    entity::{Entities, PlayerId},
    inventory::{HOTBAR_SLOTS, Stack},
    item::Item,
    mining,
    particles::Request,
    rules::GameRules,
    simulation::survival,
    world::{World, block::Block, overworld_blocks},
};
use glam::IVec3;

/// Java's `MultiPlayerGameMode.destroyDelay` after a Creative or completed timed destruction.
pub const DESTROY_DELAY_TICKS: u8 = 5;

#[derive(Default)]
pub(super) struct State {
    /// Java's `destroyBlockPos` and `destroyingItem`; the block state is not part of the key.
    target: Option<(IVec3, Option<Stack>)>,
    progress: f32,
    /// The attack input has been held since the last release (start already ran).
    held: bool,
    /// Remaining destroy-delay ticks; survives release like Java's field.
    delay: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Hit {
        player: PlayerId,
        cell: IVec3,
        block: Block,
        progress: f32,
    },
    Broken {
        player: PlayerId,
        cell: IVec3,
        block: Block,
        tool_broken: Option<Item>,
    },
    TeleportedEgg {
        player: PlayerId,
        destination: Option<IVec3>,
    },
    /// A destroy-delay tick: the held input neither progresses nor destroys.
    Delayed {
        player: PlayerId,
        remaining: u8,
    },
}

/// Java's `ItemStack.isSameItemSameComponents`: item and components, not count.
fn same_item(a: Option<Stack>, b: Option<Stack>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Stack { count: 1, ..a } == Stack { count: 1, ..b },
        _ => false,
    }
}

impl PlayerSession {
    /// Releases the attack input (Java `stopDestroyBlock`): progress is lost,
    /// but a pending destroy delay is kept.
    pub fn cancel_mining(&mut self) {
        self.mining = State { delay: self.mining.delay, ..State::default() };
    }

    /// Remaining destroy-delay ticks.
    pub fn mining_delay(&self) -> u8 {
        self.mining.delay
    }

    pub fn breaking(&self, world: &World) -> Option<(IVec3, f32)> {
        let (pos, _) = self.mining.target?;
        let present = world.get_block(pos).is_some_and(|b| b != Block::AIR);
        (present && self.mining.progress > 0.).then_some((pos, self.mining.progress.min(1.)))
    }

    /// Validate and advance one fixed tick of held attack input against a
    /// raycast the engine computes. Call once per tick while held and
    /// [`PlayerSession::cancel_mining`] on release.
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
        let creative = self.mode.is_creative();
        // Java `Item.canDestroyBlock`: tools without `can_destroy_blocks_in_creative`.
        if creative && held.is_some_and(|s| !mining::can_destroy_in_creative(s.item)) {
            return Err("held item cannot break blocks in Creative");
        }
        if !std::mem::replace(&mut self.mining.held, true) {
            // `startDestroyBlock`, then `continueDestroyBlock` in the same tick.
            if creative {
                let out = self.destroy(pos, block, slot, world, entities, rules);
                self.mining.delay = DESTROY_DELAY_TICKS - 1;
                return Ok(out);
            }
            if let Some(out) = self.start(pos, block, held, slot, world, entities, rules) {
                return Ok(out);
            }
        } else if self.mining.delay == 0 && !creative && !self.same_target(pos, held) {
            // A changed position or item restarts without progress this tick.
            return Ok(match self.start(pos, block, held, slot, world, entities, rules) {
                Some(out) => out,
                None => Outcome::Hit { player: self.id, cell: pos, block, progress: 0. },
            });
        }
        if self.mining.delay > 0 {
            self.mining.delay -= 1;
            return Ok(Outcome::Delayed { player: self.id, remaining: self.mining.delay });
        }
        if creative {
            self.mining.delay = DESTROY_DELAY_TICKS;
            return Ok(self.destroy(pos, block, slot, world, entities, rules));
        }
        let digger = mining::Digger {
            held,
            helmet: self.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.player.head_in_water(world),
            on_ground: self.player.on_ground || self.player.flying,
        };
        let effects = &self.vitals.effects;
        self.mining.progress +=
            mining::dig_progress_with(block, digger, effects.dig_speed_factor(), effects.mining_factor());
        // Lit redstone ore is retained while it is being mined.
        world.touch_redstone_ore(pos);
        if self.mining.progress < 1. {
            world.particles.push(Request::Hit { cell: pos, block, face });
            return Ok(Outcome::Hit { player: self.id, cell: pos, block, progress: self.mining.progress });
        }
        let out = self.destroy(pos, block, slot, world, entities, rules);
        self.mining.progress = 0.;
        self.mining.delay = DESTROY_DELAY_TICKS;
        Ok(out)
    }

    fn same_target(&self, pos: IVec3, held: Option<Stack>) -> bool {
        self.mining.target.is_some_and(|(p, item)| p == pos && same_item(item, held))
    }

    /// Survival `startDestroyBlock`: attack the block, then destroy an
    /// instant block (no delay) or begin tracking a timed one at zero.
    #[allow(clippy::too_many_arguments)]
    fn start(
        &mut self,
        pos: IVec3,
        block: Block,
        held: Option<Stack>,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        rules: &GameRules,
    ) -> Option<Outcome> {
        if block == Block::DRAGON_EGG
            && let Some(destination) = world.teleport_egg(pos)
        {
            self.mining.target = None;
            self.mining.progress = 0.;
            return Some(Outcome::TeleportedEgg { player: self.id, destination: Some(destination) });
        }
        world.touch_redstone_ore(pos);
        let block = world.get_block(pos).unwrap_or(block);
        let digger = mining::Digger {
            held,
            helmet: self.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.player.head_in_water(world),
            on_ground: self.player.on_ground || self.player.flying,
        };
        let effects = &self.vitals.effects;
        if mining::dig_progress_with(block, digger, effects.dig_speed_factor(), effects.mining_factor()) >= 1. {
            self.mining.target = None;
            self.mining.progress = 0.;
            return Some(self.destroy(pos, block, slot, world, entities, rules));
        }
        self.mining.target = Some((pos, held));
        self.mining.progress = 0.;
        None
    }

    /// Server `destroyBlock`: partner removal, removal, Survival wear and,
    /// with the correct tool, `playerDestroy` drops, XP and exhaustion.
    fn destroy(
        &mut self,
        pos: IVec3,
        block: Block,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        rules: &GameRules,
    ) -> Outcome {
        let held = self.inventory.get(slot);
        let survival = self.mode.is_survival();
        let drops = survival && rules.bool("doTileDrops");
        let harvest = mining::can_harvest(block, held.map(|s| s.item));
        let tool = held.map_or(Default::default(), |s| s.active_enchants());
        let melts = survival
            && block == Block::ICE
            && !tool.has(Enchantment::SilkTouch)
            && world.generator.dimension != crate::world::terrain::Dimension::Nether
            && world.get_block(pos - IVec3::Y).is_some_and(crate::content::mining::ice_support);
        // `playerWillDestroy`: doors without the correct tool (or in Creative)
        // and Creative double plants remove their lower half without a drop.
        let below = pos - IVec3::Y;
        let lower = world.get_block(below);
        let door_lower = lower.filter(|b| block.is_door_upper() && b.is_door() && !b.is_door_upper());
        let plant_lower = overworld_blocks::double_of(block)
            .filter(|&(base, upper)| upper && lower == Some(base))
            .map(|(base, _)| base);
        let silent_lower = (door_lower.is_some() && (!survival || !harvest)) || (plant_lower.is_some() && !survival);
        // Neighbor drops use the same gamerule.
        world.set_tile_drops(rules.bool("doTileDrops") && !silent_lower);
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
        if let Some(lower) = door_lower
            && world.get_block(below) == Some(lower)
        {
            world.set_block(below, Block::AIR);
            if drops && !silent_lower {
                world.spill_with_item(below, lower, held);
            }
        }
        world.set_tile_drops(rules.bool("doTileDrops"));
        if drops && harvest {
            world.spill_with_item(pos, block, held);
            entities.drop_mined_xp(block, pos, tool);
        }
        if block.is_bed() {
            world.break_bed_partner(pos, block, drops);
        }
        let tool_broken = if survival {
            if harvest {
                self.vitals.hunger.exhaust(survival::EXHAUST_MINE);
            }
            held.filter(|s| block.hardness() > 0. && self.inventory.wear(slot, mining::wear(s.item, false)))
                .map(|s| s.item)
        } else {
            None
        };
        Outcome::Broken { player: self.id, cell: pos, block, tool_broken }
    }
}
