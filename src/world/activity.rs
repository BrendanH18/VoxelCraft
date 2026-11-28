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
        self.simulation_host_away = false;
        self.simulation_host = host.map(|p| java_chunk(p.floor().as_ivec3()));
        self.simulation_host_pos = host;
        self.simulation_others.clear();
        self.simulation_others.extend(players);
        let centers: Vec<_> = host
            .into_iter()
            .chain(self.simulation_others.iter().copied())
            .map(|p| java_chunk(p.floor().as_ivec3()))
            .collect();
        self.set_activity_centers(centers);
    }

    /// v1.0 gap Track 8f: the host watching the End credits has left the
    /// level (`ServerPlayer.showEndCredits`: `removePlayerImmediately`
    /// takes their chunk tickets with them), so only `others` keep chunks
    /// ticking; with nobody else there nothing near them ticks. While
    /// `away`, the host-position refreshes the world's own ticks make
    /// ([`World::simulation_player`]) are ignored. `spectators` as for
    /// [`World::set_simulation_spectators`], the host first.
    pub fn set_simulation_players_with_host(
        &mut self,
        host: Option<DVec3>,
        others: impl IntoIterator<Item = DVec3>,
        spectators: impl IntoIterator<Item = bool>,
    ) {
        let mut spectators = spectators.into_iter();
        match host {
            Some(host) => {
                self.set_simulation_players(std::iter::once(host).chain(others));
                self.set_simulation_spectators(spectators);
            }
            None => {
                let others: Vec<DVec3> = others.into_iter().collect();
                let centers = others.iter().map(|p| java_chunk(p.floor().as_ivec3())).collect();
                self.simulation_host_away = true;
                self.simulation_host = None;
                self.simulation_host_pos = None;
                self.simulation_others = others;
                self.set_activity_centers(centers);
                // Fire spread reads the others' flags from the first.
                spectators.next();
                self.set_simulation_spectators(spectators);
            }
        }
    }

    fn set_activity_centers(&mut self, mut centers: Vec<IVec2>) {
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
        if self.simulation_host_away {
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    /// v1.0 gap Track 8f: while the host watches the End credits their
    /// chunks stop ticking (alone, nothing does), the world's own host
    /// refreshes don't bring them back, and a LAN player still keeps theirs.
    #[test]
    fn a_host_watching_the_credits_keeps_no_chunks_ticking() {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 2, 0), Arc::new(ChunkData::Uniform(crate::world::block::Block::AIR)), false);
        let (here, cell) = (DVec3::new(8.5, 70.0, 8.5), IVec3::new(8, 70, 8));
        w.set_simulation_players_with_host(Some(here), [], [false]);
        assert!(w.is_entity_ticking(cell) && w.is_block_ticking(cell));
        w.set_simulation_players_with_host(None, [], [false]);
        assert!(!w.is_entity_ticking(cell) && !w.is_block_ticking(cell));
        w.simulation_player(here);
        assert!(!w.is_block_ticking(cell), "the world's own refresh doesn't put the host back");
        let far = DVec3::new(5000.0, 70.0, 5000.0);
        w.set_simulation_players_with_host(None, [far, here], [false, true, false]);
        assert!(w.is_entity_ticking(cell), "a guest beside them keeps the chunk");
        w.set_simulation_players_with_host(Some(here), [], [false]);
        assert!(w.is_entity_ticking(cell));
    }
}
