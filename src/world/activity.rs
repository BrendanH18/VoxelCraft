//! Player simulation tickets in Java's 16-block chunk coordinates. Storage
//! and render chunks are 32 blocks wide and must not determine activity.
use glam::{DVec3, IVec2, IVec3};

use super::World;

pub const DEFAULT_SIMULATION_DISTANCE: i32 = 10;
pub const SIMULATION_DISTANCE_RANGE: (i32, i32) = (5, 32);

#[derive(Clone)]
pub(super) struct Activity {
    /// None until a caller supplies players. Direct subsystem fixtures may
    /// tick loaded cells without creating a streaming/player session.
    centers: Option<Vec<IVec2>>,
    distance: i32,
}

impl Default for Activity {
    fn default() -> Self {
        Self { centers: None, distance: DEFAULT_SIMULATION_DISTANCE }
    }
}

fn java_chunk(p: IVec3) -> IVec2 {
    IVec2::new(p.x >> 4, p.z >> 4)
}

impl Activity {
    pub(super) fn contains(&self, p: IVec3, block: bool) -> bool {
        let c = java_chunk(p);
        let radius = self.distance + i32::from(block);
        self.centers.as_ref().is_none_or(|centers| centers.iter().any(|&s| (c - s).abs().max_element() <= radius))
    }

    /// Storage columns intersecting the full ticket border (level 33).
    pub(super) fn columns(&self) -> impl Iterator<Item = (IVec2, IVec2)> + '_ {
        let radius = self.distance + 2;
        self.centers.iter().flatten().map(move |&c| ((c - IVec2::splat(radius)) >> 1, (c + IVec2::splat(radius)) >> 1))
    }

    pub(super) fn keeps(&self, chunk: IVec3) -> bool {
        self.columns().any(|(lo, hi)| chunk.x >= lo.x && chunk.z >= lo.y && chunk.x <= hi.x && chunk.z <= hi.y)
    }
}

impl World {
    /// Simulation distance uses Java's 16-block chunks, independently of the
    /// render distance. Level 31 ticks entities; level 32 also ticks blocks
    /// and block entities one chunk farther out.
    pub fn simulation_distance(&self) -> i32 {
        self.activity.distance
    }

    pub fn set_simulation_distance(&mut self, distance: i32) {
        let distance = distance.clamp(SIMULATION_DISTANCE_RANGE.0, SIMULATION_DISTANCE_RANGE.1);
        if self.activity.distance != distance {
            self.activity.distance = distance;
            self.center = None;
            self.refresh_redstone_activity();
        }
    }

    /// Supply every active player (desktop, controller, agent or LAN). The
    /// union never accelerates work when two players share the same chunks.
    pub fn set_simulation_players(&mut self, players: impl IntoIterator<Item = DVec3>) {
        let mut players = players.into_iter();
        let host = players.next();
        self.simulation_host = host.map(|p| java_chunk(p.floor().as_ivec3()));
        self.simulation_others.clear();
        self.simulation_others.extend(players);
        let mut centers: Vec<_> = host
            .into_iter()
            .chain(self.simulation_others.iter().copied())
            .map(|p| java_chunk(p.floor().as_ivec3()))
            .collect();
        centers.sort_unstable_by_key(|c| (c.x, c.y));
        centers.dedup();
        if self.activity.centers.as_ref() != Some(&centers) {
            self.activity.centers = Some(centers);
            self.center = None;
            self.refresh_redstone_activity();
        }
    }

    /// Refresh the host position without losing other sessions registered by
    /// streaming. Headless single-player callers use the same path.
    pub(crate) fn simulation_player(&mut self, player: DVec3) {
        if self.simulation_host == Some(java_chunk(player.floor().as_ivec3())) {
            return;
        }
        let others = self.simulation_others.clone();
        self.set_simulation_players(std::iter::once(player).chain(others));
    }

    pub fn is_entity_ticking(&self, p: IVec3) -> bool {
        self.activity.contains(p, false) && self.is_loaded(p)
    }

    pub fn is_block_ticking(&self, p: IVec3) -> bool {
        self.activity.contains(p, true) && self.is_loaded(p)
    }
}
