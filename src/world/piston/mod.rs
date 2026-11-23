//! Java 26.3 pistons: `PistonBaseBlock` block events, `PistonStructureResolver`,
//! `MovingPistonBlock` / `PistonMovingBlockEntity` and the entity pushes
//! they make. Behaviour and differences: docs/redstone.md#pistons-and-observers.
mod entities;
mod events;
mod java_table;
mod moving;
mod reaction;
mod resolver;

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

pub use entities::{PUSH_OFFSET, PistonPush, PushBody, STICKY_TOP, TICK_MOVEMENT, push_body, push_player};
pub use moving::{Moving, SHORT_HEAD, SPIT_PROGRESS, STEP};
pub use reaction::{
    PistonClass, PushReaction, has_block_entity, is_sticky, piston_class, push_reaction, stick_together,
};
pub use resolver::{MAX_PUSH, Resolver};

use super::{World, block::Block, redstone_blocks as r};

#[derive(Default)]
pub(crate) struct PistonState {
    /// `PistonMovingBlockEntity`s by cell.
    moving: FxHashMap<IVec3, Moving>,
    /// Next ticker order.
    sequence: u64,
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
    /// changed). Heads (`PistonHeadBlock.updateShape` and
    /// `affectNeighborsAfterRemoval`) and observers react through the
    /// level's shape updates and removal effects (`world::redstone`).
    pub(crate) fn track_piston(&mut self, p: IVec3, old: Block, new: Block) {
        if old == r::MOVING && new != r::MOVING {
            self.pistons.moving.remove(&p);
        }
    }
}
