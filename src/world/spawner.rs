//! Monster spawners: which mob each spawner cage makes. The cages are
//! blocks; this table (like furnaces and chests) remembers their mob, and
//! the entity simulation does the spawning near players (see
//! `entity::Entities`).

use glam::IVec3;

use super::World;
use super::block::Block;
use super::terrain::Dimension;
use crate::entity::MobKind;

impl World {
    /// The mob the spawner at `p` makes, if there is one.
    pub fn spawner(&self, p: IVec3) -> Option<MobKind> {
        self.spawners.get(&p).copied()
    }

    /// Every spawner in a loaded chunk.
    pub fn spawners(&self) -> Vec<(IVec3, MobKind)> {
        self.spawners.iter().filter(|(p, _)| self.is_loaded(**p)).map(|(&p, &k)| (p, k)).collect()
    }

    /// Sets which mob an existing spawner makes. Returns whether there was
    /// one at `p`.
    pub fn set_spawner(&mut self, p: IVec3, kind: MobKind) -> bool {
        let Some(slot) = self.spawners.get_mut(&p) else { return false };
        *slot = kind;
        true
    }

    /// A spawner placed without saying what it spawns makes the dimension's
    /// usual one: blazes in the Nether (fortresses), endermen in the End,
    /// zombies elsewhere (dungeons).
    fn default_spawner(&self) -> MobKind {
        match self.generator.dimension {
            Dimension::Nether => MobKind::Blaze,
            Dimension::End => MobKind::Enderman,
            Dimension::Overworld => MobKind::Zombie,
        }
    }

    /// Keeps the spawner table in step with a block change at `p`.
    pub(super) fn track_spawner(&mut self, p: IVec3, old: Block, new: Block) {
        if old == Block::SPAWNER && new != Block::SPAWNER {
            self.spawners.remove(&p);
        } else if new == Block::SPAWNER && old != Block::SPAWNER {
            let kind = self.default_spawner();
            self.spawners.entry(p).or_insert(kind);
        }
    }

    /// `x,y,z=mob|...` for the level file.
    pub fn spawners_to_string(&self) -> String {
        let mut entries: Vec<String> = self
            .spawners
            .iter()
            .map(|(p, k)| format!("{},{},{}={}", p.x, p.y, p.z, k.name().replace(' ', "_")))
            .collect();
        entries.sort();
        entries.join("|")
    }

    /// Restores spawners saved by [`World::spawners_to_string`]; malformed
    /// entries are skipped.
    pub fn load_spawners(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let Some((pos, kind)) = entry.split_once('=') else { continue };
            let c: Vec<i32> = pos.split(',').filter_map(|v| v.parse().ok()).collect();
            if let (&[x, y, z], Some(kind)) = (&c[..], MobKind::from_name(kind)) {
                self.spawners.insert(IVec3::new(x, y, z), kind);
            }
        }
    }
}
