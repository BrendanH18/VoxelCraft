//! Java 26.3 `PistonBaseBlock`: `checkIfExtend`, `triggerEvent` and
//! `moveBlocks`. Piston events go through the level's block-event queue
//! (`World::queue_block_event`, run by `World::run_block_events` at the end
//! of the tick), which dispatches them to [`World::piston_trigger_event`].
//! Edits use Java's `Level.setBlock` flags and explicit updates, so the
//! neighbour and shape updates a move causes follow Java's order.
use glam::IVec3;

use super::super::{
    World,
    block::Block,
    redstone_blocks::{self as r, Component},
    redstone_order::{UPDATE_SHAPE_ORDER, java_direction_value, java_hash_bucket},
    vibration::{EventSource, GameEvent},
};
use super::moving::SPIT_PROGRESS;
use super::reaction::{PushReaction, is_piston, push_reaction};
use super::resolver::Resolver;

// `TRIGGER_EXTEND`, `TRIGGER_CONTRACT` and `TRIGGER_DROP`.
use super::super::automation::{PISTON_EXTEND as EXTEND, PISTON_RETRACT as CONTRACT, PISTON_RETRACT_SHORT as DROP};

/// Java's `Level.setBlock` flag values the piston code passes.
pub(super) mod flags {
    /// `UPDATE_ALL`.
    pub const ALL: u16 = 3;
    /// `UPDATE_CLIENTS`.
    pub const CLIENTS: u16 = 2;
    /// `UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE`: a destroyed block.
    pub const DESTROYED: u16 = 18;
    /// `UPDATE_INVISIBLE | UPDATE_MOVE_BY_PISTON | 256`: a cell set moving.
    pub const MOVING: u16 = 324;
    /// `UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE | UPDATE_MOVE_BY_PISTON`: a vacated cell.
    pub const VACATED: u16 = 82;
    /// `UPDATE_INVISIBLE | UPDATE_KNOWN_SHAPE | 256`: a retracting base, or
    /// the arm removed before a pull.
    pub const KNOWN: u16 = 276;
    /// `UPDATE_ALL | UPDATE_MOVE_BY_PISTON`: an extended base, a landed block.
    pub const PLACED: u16 = 67;
}

/// Our facing from Java's `Direction.get3DDataValue` (the event's `b1`).
fn facing_of_java(value: i32) -> Option<u8> {
    (0..6u8).find(|&f| java_direction_value(r::direction(f)) == value & 7)
}

impl World {
    /// `PistonBaseBlock.checkIfExtend`, from placement and neighbour changes:
    /// queues a block event for the end of the tick.
    pub(crate) fn check_piston(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p) else { return };
        let Some(Component::Piston { facing, extended, .. }) = r::component(b) else { return };
        let dir = r::direction(facing);
        let java_dir = java_direction_value(dir);
        let powered = self.piston_powered(p, facing);
        if powered && !extended {
            if Resolver::new(p, dir, true).resolve(self) {
                self.queue_block_event(p, b, EXTEND, java_dir);
            }
        } else if !powered && extended {
            let pushed = p + dir * 2;
            let drop = self.get_block(pushed) == Some(r::MOVING)
                && self.moving_piston(pushed).is_some_and(|m| {
                    m.facing == facing
                        && m.extending
                        && (m.progress < SPIT_PROGRESS
                            || self.redstone.tick == m.last_ticked
                            || self.pistons.handling_tick)
                });
            self.queue_block_event(p, b, if drop { DROP } else { CONTRACT }, java_dir);
        }
    }

    /// `getNeighborSignal`: any side but the front, and quasi-connectivity
    /// through the cell above (all but its downward side).
    pub(super) fn piston_powered(&self, p: IVec3, facing: u8) -> bool {
        const SIDES: [IVec3; 6] = super::resolver::JAVA_DIRECTIONS;
        let front = r::direction(facing);
        SIDES.into_iter().any(|d| d != front && self.signal_from(p + d, -d, true) > 0)
            || SIDES.into_iter().any(|d| d != IVec3::NEG_Y && self.signal_from(p + IVec3::Y + d, -d, true) > 0)
    }

    /// `PistonBaseBlock.triggerEvent` for a queued piston event (`b` is the
    /// piston state, `b1` the facing's `get3DDataValue`).
    pub(crate) fn piston_trigger_event(&mut self, p: IVec3, b: Block, b0: i32, b1: i32) -> bool {
        let Some(Component::Piston { facing, sticky, .. }) = r::component(b) else { return false };
        let dir = r::direction(facing);
        let extended = r::piston(facing, sticky, true);
        let powered = self.piston_powered(p, facing);
        // A retraction that finds power again only marks the piston extended.
        if powered && (b0 == CONTRACT || b0 == DROP) {
            self.set_block_flags(p, extended, flags::CLIENTS);
            return false;
        }
        if !powered && b0 == EXTEND {
            return false;
        }
        if b0 == EXTEND {
            if !self.move_piston_blocks(p, facing, sticky, true) {
                return false;
            }
            self.set_block_flags(p, extended, flags::PLACED);
            // v0.17 Track E1: the extended base is the context.
            self.emit_block_event(p, GameEvent::BlockActivate, EventSource::NONE, Some(extended));
            return true;
        }
        // Retraction: an arm still extending completes at once.
        self.final_tick(p + dir);
        let moved = r::piston(facing_of_java(b1).unwrap_or(facing), sticky, false);
        self.add_moving(p, moved, facing, false, true, flags::KNOWN);
        self.update_neighbors_at(p);
        for d in UPDATE_SHAPE_ORDER {
            self.neighbor_shape_changed(p + d, -d);
        }
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
        // v0.17 Track E1: the moving piston is the context.
        self.emit_block_event(p, GameEvent::BlockDeactivate, EventSource::NONE, Some(r::MOVING));
        true
    }

    /// `level.removeBlock(arm, false)`: the cell's fluid stays.
    fn remove_piston_arm(&mut self, arm: IVec3) {
        self.set_block_flags(arm, Block::AIR, flags::ALL);
    }

    /// `PistonBaseBlock.moveBlocks`.
    fn move_piston_blocks(&mut self, p: IVec3, facing: u8, sticky: bool, extending: bool) -> bool {
        let dir = r::direction(facing);
        let arm = p + dir;
        if !extending && matches!(self.get_block(arm).and_then(r::component), Some(Component::PistonHead { .. })) {
            self.edit_exact_flags(arm, Block::AIR, flags::KNOWN);
        }
        let mut resolver = Resolver::new(p, dir, extending);
        if !resolver.resolve(self) {
            return false;
        }
        let push = resolver.push_direction();
        // States before anything changes (`toPushShapes`, `deleteAfterMove`).
        let shapes: Vec<Block> = resolver.to_push.iter().map(|&q| self.get_block(q).unwrap_or(Block::AIR)).collect();
        let mut destroyed = Vec::with_capacity(resolver.to_destroy.len());
        for &q in resolver.to_destroy.iter().rev() {
            let b = self.get_block(q).unwrap_or(Block::AIR);
            if !b.is_fluid() {
                self.spill_block(q, b);
            }
            self.edit_exact_flags(q, Block::AIR, flags::DESTROYED);
            // v0.17 Track E1: the destroyed block is the context.
            self.emit_block_event(q, GameEvent::BlockDestroy, EventSource::NONE, Some(b));
            destroyed.push((q, b));
        }
        let mut vacated: Vec<(IVec3, Block)> = resolver.to_push.iter().copied().zip(shapes.iter().copied()).collect();
        for (i, &q) in resolver.to_push.iter().enumerate().rev() {
            let to = q + push;
            vacated.retain(|&(v, _)| v != to);
            self.add_moving(to, shapes[i], facing, extending, false, flags::MOVING);
        }
        if extending {
            vacated.retain(|&(v, _)| v != arm);
            self.add_moving(arm, r::piston_head(facing, sticky), facing, true, true, flags::MOVING);
        }
        // `deleteAfterMove` is a `HashMap`: Java visits it in bucket order.
        vacated.sort_by_key(|&(q, _)| java_hash_bucket(q));
        // `moveBlocks` leaves air, not the moved block's water.
        for &(q, _) in &vacated {
            self.edit_exact_flags(q, Block::AIR, flags::VACATED);
        }
        for &(q, old) in &vacated {
            self.wire_indirect_shapes(q, old);
            for d in UPDATE_SHAPE_ORDER {
                self.neighbor_shape_changed(q + d, -d);
            }
        }
        for &(q, old) in &destroyed {
            self.affect_neighbors_after_removal(q, old, false);
            self.wire_indirect_shapes(q, old);
            self.update_neighbors_at(q);
        }
        for &q in resolver.to_push.iter().rev() {
            self.update_neighbors_at(q);
        }
        if extending {
            self.update_neighbors_at(arm);
        }
        // VoxelCraft's support reactions of non-redstone blocks (cane, sand,
        // plants), which stand in for their Java `updateShape`.
        for &(q, _) in destroyed.iter().chain(&vacated) {
            self.settle(q);
        }
        if extending {
            self.settle(arm);
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
