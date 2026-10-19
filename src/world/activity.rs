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

    /// Storage columns reaching the block-ticking border (level 32), or None
    /// while no players are registered and every loaded cell ticks.
    pub(super) fn block_columns(&self) -> Option<Vec<(IVec2, IVec2)>> {
        let radius = self.distance + 1;
        let centers = self.centers.as_ref()?;
        Some(centers.iter().map(|&c| ((c - IVec2::splat(radius)) >> 1, (c + IVec2::splat(radius)) >> 1)).collect())
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
        self.simulation_host_pos = host;
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

    /// Mark which of the players given to [`World::set_simulation_players`]
    /// (same order) are Spectators. `ChunkMap.playerIsCloseEnoughTo` ignores
    /// them, so they do not keep distant fire burning.
    pub fn set_simulation_spectators(&mut self, spectators: impl IntoIterator<Item = bool>) {
        self.simulation_spectators.clear();
        self.simulation_spectators.extend(spectators);
    }

    /// Refresh the host position without losing other sessions registered by
    /// streaming. Headless single-player callers use the same path.
    pub(crate) fn simulation_player(&mut self, player: DVec3) {
        self.simulation_host_pos = Some(player);
        if self.simulation_host == Some(java_chunk(player.floor().as_ivec3())) {
            return;
        }
        let others = self.simulation_others.clone();
        self.set_simulation_players(std::iter::once(player).chain(others));
    }

    /// Java's `ServerLevel.canSpreadFireAround`: with
    /// `fire_spread_radius_around_player` of -1 any block may, at 0 none may,
    /// otherwise a player must be closer than the radius. Fixtures without
    /// registered players tick every fire.
    pub fn fire_may_spread(&self, p: IVec3) -> bool {
        match self.rules.fire_spread_radius {
            -1 => true,
            radius if radius <= 0 => false,
            radius => {
                let at = p.as_dvec3();
                self.activity.centers.is_none()
                    || self.simulation_host_pos.iter().chain(&self.simulation_others).enumerate().any(|(i, q)| {
                        !self.simulation_spectators.get(i).copied().unwrap_or(false)
                            && q.distance(at) < f64::from(radius)
                    })
            }
        }
    }

    pub fn is_entity_ticking(&self, p: IVec3) -> bool {
        self.activity.contains(p, false) && self.is_loaded(p)
    }

    pub fn is_block_ticking(&self, p: IVec3) -> bool {
        self.activity.contains(p, true) && self.is_loaded(p)
    }
}
