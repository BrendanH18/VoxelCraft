//! World upkeep for this module's blocks: the fixed tick, block-change and
//! neighbour hooks, coral wall fans, and saving.

use glam::IVec3;

use super::{campfire_of, decorated_pot_of, trapped_chest_of, wall_fan, wall_fan_of, wall_of, wall_ok};
use crate::inventory::{stack_from_str, stack_to_string};
use crate::world::{World, block::Block};

impl World {
    /// One fixed tick: campfires cook and smoke, drips land in cauldrons,
    /// and weather fills cauldrons.
    pub fn tick_utilities(&mut self) {
        self.tick_campfires();
        self.tick_drips();
        self.tick_tilts();
        self.cauldron_precipitation();
    }

    /// The block at `p` changed from `old` to `new`.
    pub(in crate::world) fn track_utilities(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        self.track_campfire(p, old, new);
        if decorated_pot_of(old).is_some()
            && decorated_pot_of(new).is_none()
            && let Some(stack) = self.utilities.decorated.remove(&p)
            && self.tile_drops
        {
            self.drops.push((p, stack));
        }
        if trapped_chest_of(old).is_some() && trapped_chest_of(new).is_none() {
            self.utilities.viewers.remove(&p);
        }
    }

    /// Neighbour updates around a changed cell: campfires over hay become
    /// signal fires, coral wall fans fall off a wall that's gone.
    pub(in crate::world) fn update_utilities(&mut self, p: IVec3) {
        if self.get_block(p + IVec3::Y).and_then(campfire_of).is_some() {
            self.update_campfire_signal(p + IVec3::Y);
        }
        for d in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            let q = p + d;
            let Some(b) = self.get_block(q) else { continue };
            let Some(wall) = wall_of(b) else { continue };
            if q + wall.offset() == p && !self.get_block(p).is_some_and(|w| wall_ok(b, w)) {
                // Wall fans drop nothing without silk touch.
                self.edit(q, if b.is_waterlogged() { Block::WATER } else { Block::AIR }, true);
            }
        }
    }

    /// `CoralWallFanBlock.tick`: a live wall fan with no water dies.
    pub(in crate::world) fn wall_fan_death(&mut self, p: IVec3, b: Block) -> bool {
        let Some((colour, false, facing, _)) = wall_fan_of(b) else { return false };
        self.edit(p, wall_fan(colour, true, facing, false), true);
        true
    }

    /// Random tick hook: stalactites drip into cauldrons below.
    pub(in crate::world) fn utilities_random_tick(&mut self, p: IVec3, b: Block) {
        self.dripstone_transfer(p, b);
    }

    /// `c|x,y,z=slots` campfires, `p|x,y,z=stack` decorated pots, and
    /// `d|x,y,z=ticks` pending drips and `t|x,y,z=ticks` tilting dripleaves
    /// (Java saves both as scheduled block ticks), for the level file.
    pub fn utilities_to_string(&self) -> String {
        let mut parts = Vec::new();
        let campfires = self.campfires_to_string();
        if !campfires.is_empty() {
            parts.push(campfires);
        }
        for (p, s) in &self.utilities.decorated {
            parts.push(format!("p{},{},{}={}", p.x, p.y, p.z, stack_to_string(Some(*s))));
        }
        for (kind, list) in [("d", &self.utilities.drips), ("t", &self.utilities.tilts)] {
            for (p, ticks) in list {
                parts.push(format!("{kind}{},{},{}={ticks}", p.x, p.y, p.z));
            }
        }
        parts.join("|")
    }

    /// Restores [`World::utilities_to_string`]; malformed entries are skipped.
    pub fn load_utilities(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let Some((head, body)) = entry.split_once('=') else { continue };
            let Some((kind, pos)) = head.split_at_checked(1) else { continue };
            let c: Vec<i32> = pos.split(',').filter_map(|v| v.parse().ok()).collect();
            let &[x, y, z] = &c[..] else { continue };
            let p = IVec3::new(x, y, z);
            match kind {
                "c" => self.load_campfire(p, body),
                "p" => {
                    if let Some(Some(stack)) = stack_from_str(body) {
                        self.utilities.decorated.insert(p, stack);
                    }
                }
                "d" | "t" => {
                    let Ok(ticks) = body.parse::<u16>() else { continue };
                    let list = if kind == "d" { &mut self.utilities.drips } else { &mut self.utilities.tilts };
                    if !list.iter().any(|&(q, _)| q == p) {
                        list.push((p, ticks.max(1)));
                    }
                }
                _ => {}
            }
        }
    }
}
