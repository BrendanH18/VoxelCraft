//! Java 26.3 pistons: `PistonBaseBlock` block events, `PistonStructureResolver`,
//! `MovingPistonBlock` / `PistonMovingBlockEntity` and the entity pushes
//! they make. Behaviour and differences: docs/redstone.md#pistons-and-observers.
mod entities;
mod events;
mod moving;
mod reaction;
mod resolver;

use std::collections::VecDeque;

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

pub use entities::{PUSH_OFFSET, PistonPush, PushBody, STICKY_TOP, TICK_MOVEMENT, push_body, push_player};
pub use events::BlockEvent;
pub use moving::{Moving, SHORT_HEAD, SPIT_PROGRESS, STEP};
pub use reaction::{
    PistonClass, PushReaction, has_block_entity, is_sticky, piston_class, push_reaction, stick_together,
};
pub use resolver::{MAX_PUSH, Resolver};

use super::{
    World,
    block::Block,
    redstone_blocks::{self as r, Component},
};

#[derive(Default)]
pub(crate) struct PistonState {
    /// `PistonMovingBlockEntity`s by cell.
    moving: FxHashMap<IVec3, Moving>,
    /// Next ticker order.
    sequence: u64,
    /// Queued piston block events (see [`events`]).
    events: VecDeque<BlockEvent>,
    /// Java's `ServerLevel.isHandlingTick`: block ticks and block events.
    pub(crate) handling_tick: bool,
    /// The entity pushes the last moving-block tick made, and their boxes.
    pushes: Vec<PistonPush>,
    push_boxes: Vec<(DVec3, DVec3)>,
    order_scratch: Vec<(u64, IVec3)>,
}

impl World {
    /// Edit hook: a moving cell replaced from outside loses its block
    /// entity (`preRemoveSideEffects` runs `finalTick`, which finds the cell
    /// changed); heads follow `PistonHeadBlock.canSurvive` and
    /// `affectNeighborsAfterRemoval`; a powered observer placed without a
    /// pending tick turns off (`ObserverBlock.onPlace`).
    pub(crate) fn track_piston(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        if old == r::MOVING && new != r::MOVING {
            self.pistons.moving.remove(&p);
        }
        // A head whose base cell changed.
        for facing in 0..6u8 {
            let head = p + r::direction(facing);
            if let Some(Component::PistonHead { facing: f, sticky }) = self.get_block(head).and_then(r::component)
                && f == facing
                && !self.head_survives(head, facing, sticky)
            {
                self.edit(head, Block::AIR, false);
            }
        }
        // A removed head takes the piston it extended from, with its drops.
        if let Some(Component::PistonHead { facing, sticky }) = r::component(old)
            && !matches!(r::component(new), Some(Component::PistonHead { .. }))
        {
            let base = p - r::direction(facing);
            let fitting = r::piston(facing, sticky, true);
            if self.get_block(base) == Some(fitting) {
                self.spill_block(base, fitting);
                self.edit(base, Block::AIR, false);
            }
        }
        if let Some(Component::Observer { facing, on: true }) = r::component(new)
            && !matches!(r::component(old), Some(Component::Observer { .. }))
            && !self.has_scheduled_redstone(p)
        {
            self.edit(p, r::observer(facing, false), false);
        }
    }
}
