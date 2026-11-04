//! Lightning rod attraction, pulses and copper cleaning (Java 26.3).
use super::{
    World,
    block::Block,
    copper::{self, Copper, Family, Stage},
};
use glam::{IVec2, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};
#[derive(Default)]
pub(super) struct State {
    rods: FxHashMap<IVec2, FxHashSet<IVec3>>,
}
pub fn rod(b: Block) -> Option<Copper> {
    copper::state(b).filter(|c| c.family == Family::Rod)
}
pub fn rod_signal(b: Block, direction: IVec3, strong: bool) -> Option<u8> {
    rod(b).map(|c| if c.local >= 12 && (!strong || direction == -copper::rod_direction(c.local)) { 15 } else { 0 })
}
impl World {
    pub(super) fn track_lightning_rod(&mut self, p: IVec3, old: Block, new: Block) {
        let key = IVec2::new(p.x >> 5, p.z >> 5);
        if rod(new).is_some() {
            self.lightning.rods.entry(key).or_default().insert(p);
            if rod(new).is_some_and(|c| c.local >= 12) {
                self.schedule_redstone(p, 8, 0);
            }
        } else if rod(old).is_some()
            && let Some(rods) = self.lightning.rods.get_mut(&key)
        {
            rods.remove(&p);
        }
    }
    pub(super) fn unload_lightning_chunk(&mut self, c: IVec3) {
        let key = IVec2::new(c.x, c.z);
        if let Some(rods) = self.lightning.rods.get_mut(&key) {
            rods.retain(|p| p.y >> 5 != c.y);
            if rods.is_empty() {
                self.lightning.rods.remove(&key);
            }
        }
    }
    /// Closest sky-visible rod in a 128 block sphere, using WORLD_SURFACE.
    pub fn lightning_rod_target(&self, center: IVec3) -> Option<IVec3> {
        let key = IVec2::new(center.x >> 5, center.z >> 5);
        let mut closest = None;
        for x in -4..=4 {
            for z in -4..=4 {
                let Some(rods) = self.lightning.rods.get(&(key + IVec2::new(x, z))) else { continue };
                for &p in rods {
                    let d = (p.as_dvec3() - center.as_dvec3()).length_squared();
                    if d <= 128. * 128.
                        && rod(self.get_block(p).unwrap_or(Block::AIR)).is_some()
                        && self.world_surface_height(p.x, p.z) == Some(p.y)
                        && closest.is_none_or(|(best, _)| d < best)
                    {
                        closest = Some((d, p));
                    }
                }
            }
        }
        closest.map(|(_, p)| p + IVec3::Y)
    }
    /// Initial flash only: power the struck rod and clean its copper. Fire
    /// is suppressed for visual-only trap bolts, but rods/cleaning are not.
    pub fn lightning_strike_blocks(&mut self, pos: glam::DVec3) {
        let p = (pos - glam::DVec3::Y * 1e-6).floor().as_ivec3();
        if let Some(c) = self.get_block(p).and_then(rod) {
            self.set_block(p, copper::block(Family::Rod, c.stage, c.local % 12 + 12));
        }
        self.clean_lightning_copper(p);
    }
    pub(super) fn reset_lightning_rod(&mut self, p: IVec3, b: Block) {
        if let Some(c) = rod(b).filter(|c| c.local >= 12) {
            self.set_block(p, copper::block(Family::Rod, c.stage, c.local % 12));
        }
    }
    pub fn clean_lightning_copper(&mut self, p: IVec3) {
        let Some(c) = self.get_block(p).and_then(copper::state).filter(|c| c.family.weathers()) else { return };
        self.restage_copper(p, c, Stage::Unaffected, true);
        let walks = 3 + self.roll() % 3;
        for _ in 0..walks {
            let mut at = p;
            let steps = 1 + self.roll() % 8;
            for _ in 0..steps {
                let mut next = None;
                for _ in 0..10 {
                    let q = at
                        + IVec3::new(
                            (self.roll() % 3) as i32 - 1,
                            (self.roll() % 3) as i32 - 1,
                            (self.roll() % 3) as i32 - 1,
                        );
                    if let Some(c) = self.get_block(q).and_then(copper::state).filter(|c| c.family.weathers()) {
                        if let Some(stage) = c.stage.previous() {
                            self.restage_copper(q, c, stage, true);
                        }
                        next = Some(q);
                        break;
                    }
                }
                let Some(q) = next else { break };
                at = q;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{chunk::ChunkData, terrain::Generator};
    use std::sync::Arc;
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
        for y in w.generator.dimension.chunk_rows() {
            w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        w
    }
    #[test]
    fn rods_attract_only_when_exposed_and_nearest_within_128() {
        let mut w = world();
        let p = IVec3::new(8, 140, 8);
        let b = copper::block(Family::Rod, Stage::Oxidized, 0);
        w.set_block(p, b);
        assert_eq!(w.lightning_rod_target(p + IVec3::X * 50), Some(p + IVec3::Y));
        assert_eq!(w.lightning_rod_target(p + IVec3::X * 129), None);
        w.set_block(p + IVec3::Y * 10, Block::GLASS);
        assert_eq!(w.lightning_rod_target(p), None);
        w.set_block(p + IVec3::Y * 10, Block::AIR);
        w.set_block(p + IVec3::X * 3, b);
        assert_eq!(w.lightning_rod_target(p + IVec3::X * 4), Some(p + IVec3::X * 3 + IVec3::Y));
    }
    #[test]
    fn all_rod_directions_water_and_oxidation_keep_properties_through_eight_tick_pulse() {
        for stage in Stage::ALL {
            for direction in 0..6 {
                for wet in [0, 6] {
                    let mut w = world();
                    let p = IVec3::new(8, 140, 8);
                    let local = direction + wet;
                    w.set_block(p, copper::block(Family::Rod, stage, local));
                    w.lightning_strike_blocks(p.as_dvec3() + glam::DVec3::new(0.5, 1., 0.5));
                    let b = w.get_block(p).unwrap();
                    assert_eq!(
                        rod(b),
                        Some(Copper { family: Family::Rod, stage: Stage::Unaffected, local: local + 12 })
                    );
                    assert_eq!(rod_signal(b, IVec3::X, false), Some(15));
                    let back = -copper::rod_direction(local);
                    assert_eq!(rod_signal(b, back, true), Some(15));
                    assert_eq!(rod_signal(b, -back, true), Some(0));
                    let scheduled = w.redstone_to_string();
                    w.load_redstone(&scheduled);
                    for _ in 0..7 {
                        w.tick_redstone();
                    }
                    assert!(rod(w.get_block(p).unwrap()).unwrap().local >= 12);
                    w.tick_redstone();
                    assert_eq!(rod(w.get_block(p).unwrap()).unwrap().local, local);
                }
            }
        }
    }
    #[test]
    fn copper_walk_is_local_and_keeps_state_and_fire_obeys_player_radius() {
        let mut w = world();
        let p = IVec3::new(8, 140, 8);
        let b = copper::block(Family::Slab, Stage::Oxidized, 3);
        w.set_block(p, b);
        w.set_block(p + IVec3::X * 12, b);
        w.clean_lightning_copper(p);
        assert_eq!(w.get_block(p), Some(copper::block(Family::Slab, Stage::Unaffected, 3)));
        assert_eq!(w.get_block(p + IVec3::X * 12), Some(b));
        w.rules.fire_spread_radius = 0;
        w.set_block(p, Block::STONE);
        assert_eq!(w.lightning_fire(p + IVec3::Y, 4), 0);
        w.rules.fire_spread_radius = 128;
        w.rules.difficulty = 1;
        assert_eq!(w.lightning_fire(p + IVec3::Y, 4), 0);
        assert_eq!(w.lightning_flash_fire(p + IVec3::Y, 0), 1);
    }
}
