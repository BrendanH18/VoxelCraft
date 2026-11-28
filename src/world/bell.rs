//! Bell use and redstone edges, delivered through the shared entity bridge.
//!
//! v1.0 final track F3: a rung bell swings (Java 26.3 `BellBlockEntity`,
//! `BellRenderer`, `BellModel.setupAnim`). Ringing sends block event 1 with
//! the direction it was struck from (`BellBlockEntity.onHit`, the clicked
//! face, or the bell's facing for redstone and villagers); the bell then
//! shakes for 50 ticks, its body turned about its pivot (8, 12, 8)
//! sixteenths by `sin(t / pi) / (4 + t / 3)` radians, about x for a north
//! or south strike and about z for east or west. A shaking bell is left out
//! of its chunk mesh and drawn as a block-entity model, as chest lids are
//! (`world::lids`); LAN guests receive the shakes in their snapshots.
use super::{World, block::Block, block::Facing};
use glam::{DVec3, IVec3};

/// `BellBlockEntity.DURATION`: how long a struck bell shakes, in ticks.
pub const SHAKE_TICKS: u32 = 50;

/// `BellBlockEntity`'s `shaking` state: ticks since it was struck and the
/// direction it was struck from (`clickDirection`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shake {
    pub ticks: u32,
    pub direction: Facing,
}

impl Shake {
    /// `BellModel.setupAnim`: the body's turn about x and z, in radians,
    /// `partial` of a tick after the last.
    pub fn swing(self, partial: f32) -> [f32; 2] {
        let t = self.ticks as f32 + partial.clamp(0.0, 1.0);
        let base = (t / std::f32::consts::PI).sin() / (4.0 + t / 3.0);
        match self.direction {
            Facing::North => [-base, 0.0],
            Facing::South => [base, 0.0],
            Facing::East => [0.0, -base],
            Facing::West => [0.0, base],
        }
    }
}

/// The bell's facing (Java `BellBlock.FACING`): VoxelCraft's one bell state
/// is Java's `bell_floor` model unturned, `facing=north`.
pub const FACING: Facing = Facing::North;

/// The pivot of the bell's body (`BellModel`'s `bell_body`), in blocks.
pub const PIVOT: [f32; 3] = [0.5, 0.75, 0.5];

/// Whether the bell box `lo`..`hi` (sixteenths) is part of the swinging
/// body (the cup and its lip) rather than the posts and beam.
pub fn is_body(lo: [u8; 3], hi: [u8; 3]) -> bool {
    lo[0] >= 4 && hi[0] <= 12 && hi[1] <= 12
}

impl World {
    /// v1.0 final track F3: `BellBlockEntity.onHit(direction)` and
    /// `triggerEvent(1, direction)`: the bell at `pos` (re)starts shaking.
    pub fn shake_bell(&mut self, pos: IVec3, direction: Facing) {
        if self.get_block(pos).is_none_or(|b| b.base() != Block::BELL) {
            return;
        }
        let new = self.bell_shakes.insert(pos, Shake { ticks: 0, direction }).is_none();
        if new {
            self.remesh_geometry(pos);
        }
    }

    /// `BellBlockEntity.tick`'s shake: one tick on; a bell done shaking (or
    /// gone) is drawn in its chunk again.
    pub fn tick_bells(&mut self) {
        if self.bell_shakes.is_empty() {
            return;
        }
        let mut done = Vec::new();
        for (p, s) in self.bell_shakes.iter_mut() {
            s.ticks += 1;
            if s.ticks >= SHAKE_TICKS {
                done.push(*p);
            }
        }
        done.extend(
            self.bell_shakes.keys().filter(|p| self.get_block(**p).is_none_or(|b| b.base() != Block::BELL)).copied(),
        );
        for p in done {
            if self.bell_shakes.remove(&p).is_some() {
                self.remesh_geometry(p);
            }
        }
    }

    /// The bells shaking now, drawn as models instead of in their chunk.
    pub fn bell_shakes(&self) -> impl Iterator<Item = (IVec3, Shake)> + '_ {
        self.bell_shakes.iter().map(|(&p, &s)| (p, s))
    }

    /// Cells of the chunk at `cpos` whose shaking bells its mesh leaves out.
    pub(crate) fn shaking_bells_in_chunk(&self, cpos: IVec3) -> Vec<IVec3> {
        if self.bell_shakes.is_empty() {
            return Vec::new();
        }
        self.bell_shakes.keys().filter(|p| super::chunk::chunk_of(**p) == cpos).copied().collect()
    }

    /// Shaking bells within `radius` of `at`, for a LAN client: `[x, y, z,
    /// ticks, direction]` (0 south, 1 north, 2 east, 3 west).
    pub fn bells_near(&self, at: DVec3, radius: f64) -> Vec<serde_json::Value> {
        self.bell_shakes
            .iter()
            .filter(|(p, _)| (p.as_dvec3() + DVec3::splat(0.5)).distance_squared(at) <= radius * radius)
            .take(256)
            .map(|(p, s)| {
                let d = Facing::ALL.iter().position(|f| *f == s.direction).unwrap_or(1);
                serde_json::json!([p.x, p.y, p.z, s.ticks, d])
            })
            .collect()
    }

    /// Applies [`World::bells_near`] on a LAN client, remeshing the chunks
    /// whose bells started or stopped shaking.
    pub fn apply_bells(&mut self, list: &serde_json::Value) {
        let Some(list) = list.as_array() else {
            if !self.bell_shakes.is_empty() {
                for p in std::mem::take(&mut self.bell_shakes).into_keys() {
                    self.remesh_geometry(p);
                }
            }
            return;
        };
        let mut shakes = rustc_hash::FxHashMap::default();
        for v in list {
            let Some(a) = v.as_array() else { continue };
            let n = |i: usize| a.get(i).and_then(serde_json::Value::as_i64);
            let (Some(x), Some(y), Some(z), Some(t), Some(d)) = (n(0), n(1), n(2), n(3), n(4)) else { continue };
            let direction = Facing::ALL[(d as usize).min(3)];
            let ticks = (t.clamp(0, i64::from(SHAKE_TICKS) - 1)) as u32;
            shakes.insert(IVec3::new(x as i32, y as i32, z as i32), Shake { ticks, direction });
        }
        let changed: Vec<IVec3> = shakes
            .keys()
            .filter(|p| !self.bell_shakes.contains_key(p))
            .chain(self.bell_shakes.keys().filter(|p| !shakes.contains_key(p)))
            .copied()
            .collect();
        self.bell_shakes = shakes;
        for p in changed {
            self.remesh_geometry(p);
        }
    }

    /// Whether a click along `eye` + `dir` rings the bell at `pos`. Java's
    /// `BellBlock.isProperHit` rejects hits on the top or bottom face and on
    /// the beam above 0.8124 of the block. VoxelCraft has a single bell state
    /// (no facing or attachment), so the side-face axis rules are not modelled
    /// and the ray tests the whole cell.
    pub fn bell_hit_rings(&self, pos: IVec3, eye: glam::DVec3, dir: glam::DVec3) -> bool {
        let min = pos.as_dvec3();
        let Some((t, normal)) = crate::physics::ray_aabb_face(eye, dir, min, min + glam::DVec3::ONE) else {
            return false;
        };
        normal.y == 0 && eye.y + dir.y * t - min.y <= 0.812_399_983_406_066_9
    }

    /// v1.0 final track F3: the face of the bell at `pos` a click along
    /// `eye` + `dir` strikes (`hitResult.getDirection()`), if a side.
    pub fn bell_hit_face(&self, pos: IVec3, eye: DVec3, dir: DVec3) -> Option<Facing> {
        let min = pos.as_dvec3();
        let (_, normal) = crate::physics::ray_aabb_face(eye, dir, min, min + DVec3::ONE)?;
        Facing::from_offset(normal)
    }

    pub fn ring_bell(&mut self, pos: IVec3) -> bool {
        self.ring_bell_by(pos, super::vibration::EventSource::NONE)
    }

    /// `BellBlock.attemptToRing(ringingEntity, ...)`, which posts
    /// `block_change` from whoever rang it.
    pub fn ring_bell_by(&mut self, pos: IVec3, source: super::vibration::EventSource) -> bool {
        if self.get_block(pos).is_none_or(|b| b.base() != Block::BELL) {
            return false;
        }
        if !self.bell_rings.contains(&pos) {
            self.bell_rings.push(pos);
        }
        // v1.0 final track F3: struck from its facing (`attemptToRing` with
        // no direction); a player's use then names the face it struck.
        self.shake_bell(pos, FACING);
        self.emit_block_event(pos, super::vibration::GameEvent::BlockChange, source, None);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Entities, MobKind},
        world::{chunk::ChunkData, redstone_blocks as r, terrain::Generator},
    };
    use std::sync::Arc;
    #[test]
    fn only_side_hits_below_the_beam_ring_the_bell() {
        use glam::DVec3;
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        let p = IVec3::new(4, 145, 8);
        w.set_block(p, Block::BELL);
        let c = p.as_dvec3();
        // Level with the bell body from the west: rings (hit height 0.5).
        assert!(w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.5, 0.5), DVec3::X));
        // Just below the 0.8124 beam line: rings. Above it: passes.
        assert!(w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.81, 0.5), DVec3::X));
        assert!(!w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.82, 0.5), DVec3::X));
        // Top and bottom faces never ring.
        assert!(!w.bell_hit_rings(p, c + DVec3::new(0.5, 3.0, 0.5), DVec3::NEG_Y));
        assert!(!w.bell_hit_rings(p, c + DVec3::new(0.5, -2.0, 0.5), DVec3::Y));
        // A miss does not ring.
        assert!(!w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.5, 3.5), DVec3::X));
    }

    #[test]
    fn use_and_redstone_rising_edges_send_residents_home() {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        let p = IVec3::new(4, 145, 8);
        w.set_block(p, Block::BELL);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Villager, p.as_dvec3() + glam::DVec3::X * 4.0);
        let home = p + IVec3::Z * 5;
        e.mobs[0].villager.as_mut().unwrap().home = Some(home);
        e.spawn(MobKind::Villager, p.as_dvec3() + glam::DVec3::X * 33.0);
        assert!(w.use_redstone(p));
        w.tick_automation_entities(&mut e);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().bell_hide, 15.0);
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().bell_hide, 0.0);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().goal, Some(home.as_dvec3() + glam::DVec3::new(0.5, 0.6, 0.5)));
        w.set_block(p + IVec3::X, r::REDSTONE_BLOCK);
        w.tick_redstone();
        assert_eq!(w.bell_rings.len(), 1);
        w.bell_rings.clear();
        for _ in 0..5 {
            w.tick_redstone();
        }
        assert!(w.bell_rings.is_empty());
        w.set_block(p + IVec3::X, Block::AIR);
        w.tick_redstone();
        w.set_block(p + IVec3::X, r::REDSTONE_BLOCK);
        w.tick_redstone();
        assert_eq!(w.bell_rings.len(), 1);
    }

    /// v1.0 final track F3: a ring shakes the bell from its facing for 50
    /// ticks (`BellBlockEntity`), a second ring restarts the count, and the
    /// shaking bell is left out of its chunk mesh until it stops.
    #[test]
    fn a_rung_bell_shakes_for_fifty_ticks_outside_its_chunk_mesh() {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        let p = IVec3::new(4, 145, 8);
        w.set_block(p, Block::BELL);
        let chunk = crate::world::chunk::chunk_of(p);
        assert!(w.shaking_bells_in_chunk(chunk).is_empty());
        assert!(w.ring_bell(p));
        assert_eq!(w.bell_shakes().collect::<Vec<_>>(), [(p, Shake { ticks: 0, direction: Facing::North })]);
        assert_eq!(w.shaking_bells_in_chunk(chunk), [p]);
        for _ in 0..20 {
            w.tick_bells();
        }
        // A player's strike on the west face restarts it from the west.
        w.shake_bell(p, Facing::West);
        assert_eq!(w.bell_shakes().next(), Some((p, Shake { ticks: 0, direction: Facing::West })));
        for _ in 0..SHAKE_TICKS - 1 {
            w.tick_bells();
        }
        assert_eq!(w.bell_shakes().next().map(|s| s.1.ticks), Some(SHAKE_TICKS - 1));
        w.tick_bells();
        assert_eq!(w.bell_shakes().count(), 0);
        assert!(w.shaking_bells_in_chunk(chunk).is_empty());
        // Only bells shake.
        w.shake_bell(p + IVec3::X, Facing::East);
        assert_eq!(w.bell_shakes().count(), 0);
    }

    /// `BellModel.setupAnim`: `sin(t / pi) / (4 + t / 3)`, about x for north
    /// and south, about z for east and west, negative toward north and east.
    #[test]
    fn the_swing_follows_bell_model() {
        let at = |ticks, direction, partial| Shake { ticks, direction }.swing(partial);
        assert_eq!(at(0, Facing::North, 0.0), [0.0, 0.0]);
        let t: f32 = 5.5;
        let base = (t / std::f32::consts::PI).sin() / (4.0 + t / 3.0);
        assert_eq!(at(5, Facing::North, 0.5), [-base, 0.0]);
        assert_eq!(at(5, Facing::South, 0.5), [base, 0.0]);
        assert_eq!(at(5, Facing::East, 0.5), [0.0, -base]);
        assert_eq!(at(5, Facing::West, 0.5), [0.0, base]);
        // The swing changes sign every pi^2 ticks and dies away.
        assert!(at(12, Facing::South, 0.0)[0] < 0.0 && at(20, Facing::South, 0.0)[0] > 0.0);
        assert!(at(49, Facing::South, 0.0)[0].abs() < 0.05);
        // The body and lip swing; the posts and beam stay.
        let boxes = crate::world::shape::item_boxes(Block::BELL);
        let body: Vec<bool> = boxes.as_slice().iter().map(|b| is_body(b.min, b.max)).collect();
        assert_eq!(body.iter().filter(|&&b| b).count(), 2, "{body:?}");
    }

    /// A LAN guest takes the host's shakes and remeshes as they start and stop.
    #[test]
    fn lan_guests_receive_shaking_bells() {
        let world = || {
            let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
            w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
            w
        };
        let (mut host, mut guest) = (world(), world());
        let p = IVec3::new(4, 145, 8);
        host.set_block(p, Block::BELL);
        guest.set_block(p, Block::BELL);
        host.shake_bell(p, Facing::East);
        host.tick_bells();
        guest.apply_bells(&serde_json::json!(host.bells_near(p.as_dvec3(), 16.0)));
        assert_eq!(guest.bell_shakes().collect::<Vec<_>>(), [(p, Shake { ticks: 1, direction: Facing::East })]);
        guest.apply_bells(&serde_json::json!([]));
        assert_eq!(guest.bell_shakes().count(), 0);
    }
}
