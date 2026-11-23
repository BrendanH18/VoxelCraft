//! Java 26.3 `PistonStructureResolver`: which blocks a piston moves and
//! which it breaks, in the order `PistonBaseBlock.moveBlocks` uses them.
use glam::IVec3;

use super::super::{World, block::Block};
use super::reaction::{PistonClass, is_sticky, piston_class, stick_together};

/// `PistonStructureResolver.MAX_PUSH_DEPTH`, over the whole assembly.
pub const MAX_PUSH: usize = 12;

/// Java's `Direction.values()` order (down, up, north, south, west, east),
/// which decides the order sticky blocks gather their neighbours.
pub(super) const JAVA_DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

pub struct Resolver {
    piston: IVec3,
    /// The piston's facing.
    facing: IVec3,
    /// The facing when extending, its opposite when retracting.
    push: IVec3,
    extending: bool,
    start: IVec3,
    pub to_push: Vec<IVec3>,
    pub to_destroy: Vec<IVec3>,
}

impl Resolver {
    pub fn new(piston: IVec3, facing: IVec3, extending: bool) -> Self {
        let (push, start) = if extending { (facing, piston + facing) } else { (-facing, piston + facing * 2) };
        Self { piston, facing, push, extending, start, to_push: Vec::new(), to_destroy: Vec::new() }
    }

    pub fn push_direction(&self) -> IVec3 {
        self.push
    }

    fn block(w: &World, p: IVec3) -> Option<Block> {
        w.get_block(p)
    }

    fn air(w: &World, p: IVec3) -> bool {
        Self::block(w, p) == Some(Block::AIR)
    }

    fn sticky_at(w: &World, p: IVec3) -> bool {
        Self::block(w, p).is_some_and(is_sticky)
    }

    /// `resolve`: false when the structure can't move.
    pub fn resolve(&mut self, w: &World) -> bool {
        self.to_push.clear();
        self.to_destroy.clear();
        if !w.piston_pushable(self.start, self.push, false, self.facing) {
            let popped = Self::block(w, self.start).is_some_and(|b| piston_class(b) == PistonClass::Destroy);
            if self.extending && popped {
                self.to_destroy.push(self.start);
                return true;
            }
            return false;
        }
        if !self.add_line(w, self.start, self.push) {
            return false;
        }
        let mut i = 0;
        while i < self.to_push.len() {
            let p = self.to_push[i];
            if Self::sticky_at(w, p) && !self.add_branches(w, p) {
                return false;
            }
            i += 1;
        }
        true
    }

    /// `addBlockLine`: the block at `start`, the sticky blocks dragged behind
    /// it, and the blocks it pushes ahead.
    fn add_line(&mut self, w: &World, start: IVec3, connection: IVec3) -> bool {
        let Some(mut next) = Self::block(w, start) else { return true };
        if next == Block::AIR
            || !w.piston_pushable(start, self.push, false, connection)
            || start == self.piston
            || self.to_push.contains(&start)
        {
            return true;
        }
        let mut count = 1;
        if count + self.to_push.len() > MAX_PUSH {
            return false;
        }
        while is_sticky(next) {
            let behind = start - self.push * count as i32;
            let previous = next;
            let Some(b) = Self::block(w, behind) else { break };
            next = b;
            if next == Block::AIR
                || !stick_together(previous, next)
                || !w.piston_pushable(behind, self.push, false, -self.push)
                || behind == self.piston
            {
                break;
            }
            count += 1;
            if count + self.to_push.len() > MAX_PUSH {
                return false;
            }
        }
        let mut added = 0;
        for i in (0..count).rev() {
            self.to_push.push(start - self.push * i as i32);
            added += 1;
        }
        let mut i = 1;
        loop {
            let ahead = start + self.push * i;
            if let Some(collision) = self.to_push.iter().position(|&q| q == ahead) {
                self.reorder_at_collision(added, collision);
                for j in 0..=collision + added {
                    let q = self.to_push[j];
                    if Self::sticky_at(w, q) && !self.add_branches(w, q) {
                        return false;
                    }
                }
                return true;
            }
            if Self::air(w, ahead) {
                return true;
            }
            if !w.piston_pushable(ahead, self.push, true, self.push) || ahead == self.piston {
                return false;
            }
            if Self::block(w, ahead).is_some_and(|b| piston_class(b) == PistonClass::Destroy) {
                self.to_destroy.push(ahead);
                return true;
            }
            if self.to_push.len() >= MAX_PUSH {
                return false;
            }
            self.to_push.push(ahead);
            added += 1;
            i += 1;
        }
    }

    /// `reorderListAtCollision`: the line just added moves before the
    /// blocks it ran into.
    fn reorder_at_collision(&mut self, added: usize, collision: usize) {
        let len = self.to_push.len();
        self.to_push[collision..].rotate_right(added.min(len - collision));
    }

    /// `addBranchingBlocks`: neighbours across the push axis that stick.
    fn add_branches(&mut self, w: &World, from: IVec3) -> bool {
        let Some(from_block) = Self::block(w, from) else { return true };
        for d in JAVA_DIRECTIONS {
            if d.abs() == self.push.abs() {
                continue;
            }
            let n = from + d;
            if Self::block(w, n).is_some_and(|b| stick_together(b, from_block)) && !self.add_line(w, n, d) {
                return false;
            }
        }
        true
    }
}
