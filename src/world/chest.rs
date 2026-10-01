//! Chests: 27 slots of storage kept in [`World`] by position (block
//! entities), like furnaces. They appear when a chest is placed, spill into
//! [`World::drops`] when it's removed, and are saved in the level file.

use glam::IVec3;

use super::World;
use super::block::Block;
use crate::inventory::{Stack, stack_from_str, stack_to_string};

pub const SLOTS: usize = 27;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Chest {
    pub slots: [Option<Stack>; SLOTS],
}

impl Default for Chest {
    fn default() -> Self {
        Self { slots: [None; SLOTS] }
    }
}

impl Chest {
    /// Slots as comma-separated stacks (see [`stack_to_string`]).
    pub fn serialize(&self) -> String {
        self.slots.iter().map(|&s| stack_to_string(s)).collect::<Vec<_>>().join(",")
    }

    pub fn deserialize(text: &str) -> Option<Self> {
        let parts: Vec<&str> = text.split(',').collect();
        if parts.len() != SLOTS {
            return None;
        }
        let mut chest = Self::default();
        for (slot, part) in chest.slots.iter_mut().zip(parts) {
            *slot = stack_from_str(part)?;
        }
        Some(chest)
    }
}

pub fn is_chest(b: Block) -> bool {
    b.base() == Block::CHEST
}

impl World {
    pub fn chest(&self, p: IVec3) -> Option<&Chest> {
        self.chests.get(&p)
    }

    pub fn chest_mut(&mut self, p: IVec3) -> Option<&mut Chest> {
        self.chests.get_mut(&p)
    }

    /// Keeps the chest table in step with a block change at `p` (turning a
    /// chest keeps its contents).
    pub(super) fn track_chest(&mut self, p: IVec3, old: Block, new: Block) {
        if is_chest(old) && !is_chest(new) {
            if let Some(chest) = self.chests.remove(&p) {
                self.drops.extend(chest.slots.into_iter().flatten().map(|s| (p, s)));
            }
        } else if is_chest(new) {
            self.chests.entry(p).or_default();
        }
    }

    /// `x,y,z=slots|...` for the level file.
    pub fn chests_to_string(&self) -> String {
        self.chests
            .iter()
            .map(|(p, c)| format!("{},{},{}={}", p.x, p.y, p.z, c.serialize()))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Restores chests saved by [`World::chests_to_string`]; malformed
    /// entries are skipped.
    pub fn load_chests(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let Some((pos, slots)) = entry.split_once('=') else { continue };
            let c: Vec<i32> = pos.split(',').filter_map(|v| v.parse().ok()).collect();
            if let (&[x, y, z], Some(chest)) = (&c[..], Chest::deserialize(slots)) {
                self.chests.insert(IVec3::new(x, y, z), chest);
            }
        }
    }
}
