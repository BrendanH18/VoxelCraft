//! Java 26.3 `PistonBaseBlock`: `checkIfExtend`, its block events and
//! `triggerEvent`, and `moveBlocks`.
//!
//! Integration seam: piston block events go through
//! [`World::queue_piston_event`] and are run by [`World::run_piston_block_events`]
//! (Java's `ServerLevel.blockEvent` / `runBlockEvents`). A level-wide
//! block-event queue replaces both by queueing [`BlockEvent`]s itself and
//! calling [`World::piston_trigger_event`] for each.
use glam::IVec3;

use super::super::{
    World,
    block::Block,
    redstone_blocks::{self as r, Component},
};
use super::moving::SPIT_PROGRESS;
use super::reaction::{PushReaction, is_piston, push_reaction};
use super::resolver::Resolver;

/// `TRIGGER_EXTEND`, `TRIGGER_CONTRACT` and `TRIGGER_DROP`.
pub const EXTEND: u8 = 0;
pub const CONTRACT: u8 = 1;
pub const DROP: u8 = 2;

/// Block events one tick may run before the rest carry over (Java has no
/// bound; VoxelCraft keeps every tick budgeted).
const MAX_EVENTS: usize = 65_536;

/// Java's `BlockEventData` for a piston: the block (piston or sticky
/// piston), `b0` (the trigger) and `b1` (the facing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockEvent {
    pub pos: IVec3,
    pub sticky: bool,
    pub b0: u8,
    pub facing: u8,
}

impl World {
    /// `level.blockEvent`: queues an event once (equal events merge).
    pub(crate) fn queue_piston_event(&mut self, e: BlockEvent) {
        if !self.pistons.events.contains(&e) {
            self.pistons.events.push_back(e);
        }
    }

    /// `ServerLevel.runBlockEvents`: runs queued events in order, including
    /// ones they queue; events in cells that don't tick wait.
    pub(crate) fn run_piston_block_events(&mut self) {
        let mut waiting = Vec::new();
        let mut ran = 0;
        while let Some(e) = self.pistons.events.pop_front() {
            if ran == MAX_EVENTS {
                self.pistons.events.push_front(e);
                break;
            }
            ran += 1;
            if !self.is_block_ticking(e.pos) {
                waiting.push(e);
                continue;
            }
            self.piston_trigger_event(e);
            self.drain_redstone_updates();
        }
        self.pistons.events.extend(waiting);
    }

    /// `doBlockEvent`: the event runs only while the cell still holds that
    /// kind of piston. Returns `triggerEvent`'s result.
    pub(crate) fn piston_trigger_event(&mut self, e: BlockEvent) -> bool {
        match self.get_block(e.pos).and_then(r::component) {
            Some(Component::Piston { facing, sticky, .. }) if sticky == e.sticky => {
                self.trigger_piston(e.pos, facing, sticky, e.b0, e.facing)
            }
            _ => false,
        }
    }

    /// `PistonBaseBlock.checkIfExtend`, from placement and neighbour changes.
    pub(crate) fn check_piston(&mut self, p: IVec3) {
        let Some(Component::Piston { facing, sticky, extended }) = self.get_block(p).and_then(r::component) else {
            return;
        };
        let powered = self.piston_powered(p, facing);
        if powered && !extended {
            if Resolver::new(p, r::direction(facing), true).resolve(self) {
                self.queue_piston_event(BlockEvent { pos: p, sticky, b0: EXTEND, facing });
            }
        } else if !powered && extended {
            let pushed = p + r::direction(facing) * 2;
            let drop = self.get_block(pushed) == Some(r::MOVING)
                && self.moving_piston(pushed).is_some_and(|m| {
                    m.facing == facing
                        && m.extending
                        && (m.progress < SPIT_PROGRESS
                            || self.redstone.tick == m.last_ticked
                            || self.pistons.handling_tick)
                });
            self.queue_piston_event(BlockEvent { pos: p, sticky, b0: if drop { DROP } else { CONTRACT }, facing });
        }
    }

    /// `getNeighborSignal`: any side but the front, the piston's own cell
    /// read downward, and quasi-connectivity through the cell above.
    pub(super) fn piston_powered(&self, p: IVec3, facing: u8) -> bool {
        const SIDES: [IVec3; 6] = super::resolver::JAVA_DIRECTIONS;
        let front = r::direction(facing);
        SIDES.into_iter().any(|d| d != front && self.signal_from(p + d, -d, true) > 0)
            || SIDES.into_iter().any(|d| d != IVec3::NEG_Y && self.signal_from(p + IVec3::Y + d, -d, true) > 0)
    }

    /// `PistonBaseBlock.triggerEvent`.
    fn trigger_piston(&mut self, p: IVec3, facing: u8, sticky: bool, b0: u8, b1: u8) -> bool {
        let dir = r::direction(facing);
        let powered = self.piston_powered(p, facing);
        // A retraction that finds power again only marks the piston extended.
        if powered && (b0 == CONTRACT || b0 == DROP) {
            self.edit(p, r::piston(facing, sticky, true), false);
            return false;
        }
        if !powered && b0 == EXTEND {
            return false;
        }
        if b0 == EXTEND {
            if !self.move_piston_blocks(p, facing, sticky, true) {
                return false;
            }
            self.edit(p, r::piston(facing, sticky, true), false);
            return true;
        }
        // Retraction: an arm still extending completes at once.
        self.final_tick(p + dir);
        let moved = r::piston(b1.min(5), sticky, false);
        self.add_moving(p, moved, facing, false, true);
        if sticky {
            let two = p + dir * 2;
            let state = self.get_block(two);
            let mut piston_piece = false;
            if state == Some(r::MOVING)
                && let Some(m) = self.moving_piston(two)
                && m.facing == facing
                && m.extending
            {
                self.final_tick(two);
                piston_piece = true;
            }
            if !piston_piece {
                let pull = b0 == CONTRACT
                    && state.is_some_and(|b| {
                        b != Block::AIR && (push_reaction(b) == PushReaction::PushPull || is_piston(b))
                    })
                    && self.piston_pushable(two, -dir, false, dir);
                if pull {
                    self.move_piston_blocks(p, facing, sticky, false);
                } else {
                    self.remove_piston_arm(p + dir);
                }
            }
        } else {
            self.remove_piston_arm(p + dir);
        }
        true
    }

    /// `level.removeBlock(arm, false)`: the cell's fluid stays.
    fn remove_piston_arm(&mut self, arm: IVec3) {
        self.edit(arm, Block::AIR, false);
    }

    /// `PistonBaseBlock.moveBlocks`.
    fn move_piston_blocks(&mut self, p: IVec3, facing: u8, sticky: bool, extending: bool) -> bool {
        let dir = r::direction(facing);
        let arm = p + dir;
        if !extending && matches!(self.get_block(arm).and_then(r::component), Some(Component::PistonHead { .. })) {
            self.edit_exact(arm, Block::AIR, false);
        }
        let mut resolver = Resolver::new(p, dir, extending);
        if !resolver.resolve(self) {
            return false;
        }
        let push = resolver.push_direction();
        // States before anything changes (`toPushShapes`).
        let shapes: Vec<Block> = resolver.to_push.iter().map(|&q| self.get_block(q).unwrap_or(Block::AIR)).collect();
        for &q in resolver.to_destroy.iter().rev() {
            let Some(b) = self.get_block(q) else { continue };
            if !b.is_fluid() {
                self.spill_block(q, b);
            }
            self.edit_exact(q, Block::AIR, false);
        }
        let mut vacated: Vec<IVec3> = resolver.to_push.clone();
        for (i, &q) in resolver.to_push.iter().enumerate().rev() {
            let to = q + push;
            vacated.retain(|&v| v != to);
            self.add_moving(to, shapes[i], facing, extending, false);
        }
        if extending {
            vacated.retain(|&v| v != arm);
            self.add_moving(arm, r::piston_head(facing, sticky), facing, true, true);
        }
        // `moveBlocks` leaves air, not the moved block's water.
        for q in vacated {
            self.edit_exact(q, Block::AIR, false);
        }
        true
    }

    /// `PistonHeadBlock.canSurvive`: a head stands on an extended piston of
    /// its kind and facing, or on a moving cell moving its way.
    pub(crate) fn head_survives(&self, head: IVec3, facing: u8, sticky: bool) -> bool {
        let base = head - r::direction(facing);
        match self.get_block(base) {
            Some(b) if b == r::piston(facing, sticky, true) => true,
            Some(r::MOVING) => self.moving_piston(base).is_some_and(|m| m.facing == facing),
            _ => false,
        }
    }
}
