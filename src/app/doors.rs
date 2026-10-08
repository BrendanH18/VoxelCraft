//! Shaped blocks in play: doors two blocks tall, ladders hung on walls, and
//! opening and closing doors and fence gates.

use glam::IVec3;

use crate::audio::sounds::Sound;
use crate::item::Item;
use crate::world::block::{Block, Facing, Shaped};
use crate::world::forms::{self, WoodForm};

use super::Game;

impl Game {
    /// Right-click on a door or fence gate opens or closes it (both halves
    /// of a door) for a player looking along `forward`. Returns whether it did.
    pub(super) fn toggle_door(&mut self, pos: IVec3, forward: glam::Vec3) -> bool {
        let Some(b) = self.world.get_block(pos) else { return false };
        if matches!(
            voxelcraft::world::redstone_blocks::component(b),
            Some(voxelcraft::world::redstone_blocks::Component::IronDoor { .. })
        ) {
            return true;
        }
        let open = match b.shaped() {
            Some(Shaped::Door { mut facing, mut open, upper }) => {
                let other = if upper { pos - IVec3::Y } else { pos + IVec3::Y };
                let partner = self.world.get_block(other).filter(|o| o.is_door());
                // A wood door's upper half does not store facing, so the
                // lower half is the one that flips.
                if upper
                    && let Some(lower) = partner
                    && let Some(Shaped::Door { facing: f, open: o, upper: false }) = lower.shaped()
                {
                    facing = f;
                    open = o;
                }
                self.world.set_block(pos, b.toggled(facing));
                if let Some(o) = partner {
                    self.world.set_block(other, o.toggled(facing));
                }
                !open
            }
            Some(Shaped::Gate { facing, open }) => {
                // A gate swings away from whoever opens it.
                let toward = Facing::toward(forward);
                let facing = if !open && toward.along_x() == facing.along_x() { toward } else { facing };
                self.world.set_block(pos, b.toggled(facing));
                !open
            }
            _ => return false,
        };
        self.audio.play(Sound::Door(open), Some(pos.as_dvec3() + 0.5), 0.8, (0.9, 1.1));
        true
    }

    /// Places a door with its lower half at `at`, facing the player.
    /// Returns whether it went down.
    pub(super) fn place_door(&mut self, at: IVec3, item: Item) -> bool {
        let up = at + IVec3::Y;
        let free =
            |p: IVec3| self.world.get_block(p).is_some_and(|b| b.is_replaceable()) && !self.player.intersects_block(p);
        if up.y >= crate::world::chunk::WORLD_HEIGHT
            || !free(at)
            || !free(up)
            || !self.world.get_block(at - IVec3::Y).is_some_and(|b| b.is_opaque())
        {
            return false;
        }
        let facing = Facing::toward(self.player.forward());
        let (lower, upper) = if item == Item::OAK_DOOR {
            (Block::door(facing, false, false), Block::door(facing, false, true))
        } else if let Some(block) = item.block()
            && let Some(WoodForm::Door { index, .. }) = forms::wood_form(block.0)
        {
            (forms::wood_id(index, 14 + facing as u16), forms::wood_id(index, 22))
        } else {
            return false;
        };
        self.world.set_block(at, lower);
        self.world.set_block(up, upper);
        self.audio.block_place(Block::PLANKS, at);
        true
    }

    /// Hangs a ladder on the side `normal` of the wall at `wall`. Returns
    /// whether it went up.
    pub(super) fn place_ladder(&mut self, wall: IVec3, normal: IVec3) -> bool {
        let at = wall + normal;
        let Some(facing) = Facing::from_offset(normal) else { return false };
        let free = self.world.get_block(at).is_some_and(|b| b.is_replaceable());
        if !free || !self.world.get_block(wall).is_some_and(|b| b.is_opaque()) {
            return false;
        }
        let ladder = Block::LADDER.with_facing(facing);
        if !self.world.set_block(at, ladder) {
            return false;
        }
        self.audio.block_place(ladder, at);
        true
    }

    /// After one half of a door at `pos` broke, removes the other. Only the
    /// lower half drops the door; breaking the upper one spills it.
    pub(super) fn break_door_partner(&mut self, pos: IVec3, half: Block) {
        // A lower half takes the upper one with it as it loses its support.
        if !half.is_door_upper() {
            return;
        }
        let below = pos - IVec3::Y;
        let Some(lower) = self.world.get_block(below).filter(|b| b.is_door() && !b.is_door_upper()) else { return };
        self.world.set_block(below, Block::AIR);
        if self.mode.is_survival() {
            self.world.spill_block(below, lower);
        }
    }
}
