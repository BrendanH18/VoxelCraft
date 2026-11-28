//! v1.0 final F11: the dimensions the host isn't in, run as Java 26.3 runs
//! them ([verification record](../../docs/verification/2026-11-28-v1.0-final-f11-dimensions.md)).
//!
//! Java ticks every `ServerLevel` each server tick, in the order the levels
//! were registered (the Overworld, the Nether, the End). A level nobody is
//! in does only what its chunk tickets let it ([`voxelcraft::world::tickets`]):
//! a `PORTAL` ticket for 300 ticks where something came through a portal,
//! an `ENDER_PEARL` ticket around a flying pearl. Scheduled block and fluid
//! ticks, random ticks and block events run in the chunks they reach; its
//! entities and block entities run too, until `emptyTime` reaches 300 ticks
//! after the last ticket that keeps the level active went. Nothing spawns
//! (natural and custom spawning need a player nearby). Chunks no ticket
//! reaches unload, their entities with them.
//!
//! Here the host's level is `self.world` and `self.mobs.entities`, as it
//! always was. Every other dimension that has been loaded is a background
//! [`Level`] with its own `World` (streaming only what its tickets reach,
//! meshing nothing) and `Entities`. Each game tick, a background level is
//! swapped into `self.world` and `self.mobs.entities` ([`Game::with_level`])
//! and ticked by [`Game::tick_unattended`], the parts of the game's own tick
//! that a level runs without players: `self.unattended` gates every use of
//! the host and the other players, and the audio is suppressed so nothing
//! it plays reaches anyone. Its particles and presentation queues are
//! dropped. Then it is swapped back. Travelling to a background level
//! swaps it in for good ([`Game::switch_dimension`]); the level left
//! becomes a background level.

use glam::{DVec3, IVec3};

use voxelcraft::world::tickets::{self, EmptyTime, TicketKind, Tickets};

use crate::world::World;
use crate::world::terrain::{Dimension, Generator};

use super::Game;
use super::dimension;

/// The level file key for a level's persistent tickets
/// (`TicketStorage`'s `chunk_tickets`), written only when it has some.
pub(super) const TICKETS_KEY: &str = "chunk_tickets";

/// A dimension the host isn't in.
pub(super) struct Level {
    pub dimension: Dimension,
    pub world: World,
    pub entities: crate::entity::Entities,
    pub tickets: Tickets,
    pub empty: EmptyTime,
}

/// `MinecraftServer.getAllLevels`' order: the Overworld, then the Nether,
/// then the End.
pub(super) fn order(d: Dimension) -> u8 {
    match d {
        Dimension::Overworld => 0,
        Dimension::Nether => 1,
        Dimension::End => 2,
    }
}

/// The dimensions to open as background levels, oldest need first: those
/// travellers are bound for, the Nether while it owes portals, and those
/// pearls of players in the game wait in; none already `live`.
pub(super) fn waiting_levels(
    arrivals: &[super::portal_travel::PortalArrival],
    owed_portals: bool,
    pearls: &[Dimension],
    live: &[Dimension],
) -> Vec<Dimension> {
    let mut out: Vec<Dimension> = Vec::new();
    let wanted =
        arrivals.iter().map(|a| a.to).chain(owed_portals.then_some(Dimension::Nether)).chain(pearls.iter().copied());
    for d in wanted {
        if !live.contains(&d) && !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

/// The Java chunk of a block position.
pub(super) fn chunk_at(p: IVec3) -> glam::IVec2 {
    tickets::chunk_of(p.x, p.z)
}

impl Game {
    /// Swaps `level` with the host's: world, entities, dimension, tickets
    /// and empty time. The game-wide maps and loot sequences stay with
    /// `self.world`. Its own inverse.
    pub(super) fn swap_level(&mut self, level: &mut Level) {
        std::mem::swap(&mut self.world, &mut level.world);
        std::mem::swap(&mut self.mobs.entities, &mut level.entities);
        std::mem::swap(&mut self.dimension, &mut level.dimension);
        std::mem::swap(&mut self.tickets, &mut level.tickets);
        std::mem::swap(&mut self.empty_time, &mut level.empty);
        self.world.maps = std::mem::take(&mut level.world.maps);
        self.world.loot_sequences = std::mem::take(&mut level.world.loot_sequences);
    }

    /// Runs `f` with background level `i` swapped in as the current one,
    /// unattended: the host and the other players aren't there, and
    /// nothing it plays is heard.
    pub(super) fn with_level<R>(&mut self, i: usize, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut level = self.levels.remove(i);
        self.swap_level(&mut level);
        let was = std::mem::replace(&mut self.unattended, true);
        let muted = self.audio.suppress(true);
        let r = f(self);
        self.audio.suppress(muted);
        self.unattended = was;
        self.swap_level(&mut level);
        self.levels.insert(i, level);
        r
    }

    /// The background level of `d`, if it is loaded.
    pub(super) fn level_index(&self, d: Dimension) -> Option<usize> {
        self.levels.iter().position(|l| l.dimension == d)
    }

    /// Whether the host's level and every background level may run here: a
    /// LAN client's world is the host's.
    fn runs_levels(&self) -> bool {
        self.lan.client.is_none()
    }

    /// Loads dimension `d` from its save as a background level (nothing of
    /// it loads until a ticket asks). Returns its index.
    pub(super) fn open_level(&mut self, d: Dimension) -> usize {
        if let Some(i) = self.level_index(d) {
            return i;
        }
        let (chunks, props, notice) = dimension::load_dimension(&self.storage, d, &self.overworld_props);
        let (seed, generation) = (self.world.generator.seed, self.world.generator.generation);
        let mut world = World::new(
            std::sync::Arc::new(Generator::with_generation(seed, d, generation)),
            chunks,
            self.settings.render_distance,
        );
        world.set_simulation_distance(self.settings.simulation_distance);
        world.set_background(true);
        let region_notice =
            dimension::attach_regions(&mut world, &self.storage, d, &props, self.settings_path.is_some());
        log::info!("the {} runs in the background", d.name());
        self.levels.push(Level {
            dimension: d,
            world,
            entities: crate::entity::Entities::new(seed),
            tickets: Tickets::default(),
            empty: EmptyTime::default(),
        });
        let i = self.levels.len() - 1;
        self.with_level(i, |g| g.restore_dimension(&props));
        if let Some(notice) = notice.into_iter().chain(region_notice).reduce(|a, b| format!("{a} {b}")) {
            self.report_load_damage(&notice);
        }
        i
    }

    /// Opens the background levels something waits for: travellers bound
    /// there ([`super::portal_travel::PortalArrival`]), portals a refused
    /// trip owes the Nether, and pearls of players in the game waiting for
    /// the level they fly in. Java's levels are always loaded; these load
    /// once they have something to do.
    pub(super) fn open_waiting_levels(&mut self) {
        if !self.runs_levels() {
            return;
        }
        let agents = &self.agents.players;
        let online = |id: crate::entity::PlayerId| {
            id == crate::entity::PlayerId::HOST || agents.values().any(|b| b.active && b.id == id)
        };
        let pearls = self.owned_pearls.waiting_in(online);
        let live: Vec<Dimension> =
            std::iter::once(self.dimension).chain(self.levels.iter().map(|l| l.dimension)).collect();
        for d in waiting_levels(&self.portal_arrivals, !self.owed_portals.is_empty(), &pearls, &live) {
            self.open_level(d);
        }
    }

    /// v1.0 final F11: when a world opens, the levels whose saved tickets
    /// (`chunk_tickets`, Java's `activateAllDeactivatedTickets`) are still
    /// running, and those something waits for.
    pub(super) fn open_ticketed_levels(&mut self) {
        if !self.runs_levels() {
            return;
        }
        for d in [Dimension::Overworld, Dimension::Nether, Dimension::End] {
            if d == self.dimension {
                continue;
            }
            let ticketed = match d {
                Dimension::Overworld => self.overworld_props.contains_key(TICKETS_KEY),
                _ => {
                    let storage = dimension::storage_for(&self.storage, d);
                    storage.exists() && storage.load_level().is_ok_and(|l| l.props.contains_key(TICKETS_KEY))
                }
            };
            if ticketed {
                self.open_level(d);
            }
        }
        self.open_waiting_levels();
    }

    /// Ticks the background levels that come before (`before`) or after the
    /// host's level in Java's level order.
    pub(super) fn tick_levels(&mut self, before: bool) {
        if !self.runs_levels() {
            return;
        }
        let here = order(self.dimension);
        let mut i = 0;
        while i < self.levels.len() {
            if (order(self.levels[i].dimension) < here) == before {
                self.with_level(i, |g| g.tick_unattended());
            }
            i += 1;
        }
    }

    /// Each tick, before anything else in a level: its tickets count down
    /// (`purgeStaleTickets`, only where their chunk is loaded), and what
    /// they reach loads and ticks.
    pub(super) fn update_level_tickets(&mut self) {
        let world = &self.world;
        self.tickets.purge(|c| world.column_loaded(c.x * 16, c.y * 16));
        self.world.set_tickets(&self.tickets);
    }

    /// After the entities: the tickets pearls renewed this tick
    /// (`registerAndUpdateEnderPearlTicket`), which also reset the level's
    /// empty time.
    pub(super) fn take_pearl_tickets(&mut self) {
        for chunk in std::mem::take(&mut self.mobs.entities.pearl_tickets) {
            self.tickets.add_with_radius(TicketKind::EnderPearl, chunk, 2);
            self.empty_time.reset();
        }
    }

    /// `Entity.placePortalTicket`: a `PORTAL` ticket (radius 3) where
    /// something arrived through a portal, which keeps the chunks around it
    /// loaded and ticking for 300 ticks.
    pub(super) fn place_portal_ticket(&mut self, at: IVec3) {
        self.tickets.add_with_radius(TicketKind::Portal, chunk_at(at), 3);
    }

    /// Holds the ground around `target` (within `r` blocks) loaded until
    /// the work waiting on it is done (Java loads it at once).
    pub(super) fn hold_ground(&mut self, target: IVec3, r: i32) {
        self.tickets.add_with_radius(TicketKind::Load, chunk_at(target), (r + 15) / 16);
    }

    /// Whether the columns at `target` and `r` blocks around it are loaded.
    pub(super) fn ground_loaded(&self, target: IVec3, r: i32) -> bool {
        [(-r, -r), (r, -r), (-r, r), (r, r), (0, 0)]
            .iter()
            .all(|&(dx, dz)| self.world.column_loaded(target.x + dx, target.z + dz))
    }

    /// The work that waits for ground in this level: portals a refused trip
    /// owes the Nether, then travellers bound here, oldest first. Once none
    /// waits, the ground they held is let go.
    pub(super) fn resolve_waiting(&mut self) {
        self.build_ready_owed_portals();
        if self.portal_arrivals.iter().any(|a| a.to == self.dimension) {
            self.resolve_portal_arrivals(false);
        }
        let waits = self.portal_arrivals.iter().any(|a| a.to == self.dimension)
            || (self.dimension == Dimension::Nether && !self.owed_portals.is_empty());
        if !waits {
            self.tickets.remove_kind(TicketKind::Load);
        }
    }

    /// One `ServerLevel.tick` of a level nobody is in, swapped in by
    /// [`Game::with_level`]: its tickets, then the world's ticks where they
    /// reach, then (until `emptyTime` reaches 300) its block entities and
    /// entities.
    fn tick_unattended(&mut self) {
        let dt = crate::simulation::TICK_SECONDS;
        self.update_level_tickets();
        self.world.set_simulation_players_with_host(None, std::iter::empty(), std::iter::empty());
        self.world.update_background();
        // `hasActiveTickets`, then `emptyTime`.
        let running = self.empty_time.tick(self.tickets.keep_active());
        self.sync_owned_pearls();
        self.world.set_tile_drops(self.gamerules.bool("block_drops"));
        self.mobs.entities.entity_drops = self.gamerules.bool("entity_drops");
        self.mobs.entities.tnt_explodes = self.gamerules.bool("tnt_explodes");
        self.world.rules =
            crate::world::WorldRules { difficulty: self.difficulty.id(), ..self.gamerules.world_rules() };
        self.world.raining = self.weather.is_raining() && self.dimension.has_sky();
        let ticks = (self.day_time.rem_euclid(1.) * 24000.).floor() as u32;
        let darken = crate::simulation::lighting::SpawnLight::for_dimension(
            self.dimension,
            ticks,
            self.weather.rain_level(),
            self.weather.thunder_level(),
        )
        .sky_darken;
        let sun_angle =
            if self.dimension == Dimension::Overworld { crate::simulation::lighting::sun_angle(ticks) } else { 0.0 };
        self.world.set_redstone_daylight(self.day_time, darken, sun_angle);
        self.world.redstone_contacts_from(std::iter::empty(), &self.mobs.entities);
        self.world.set_shulker_viewers(&[]);
        self.world.set_container_viewers(&[]);
        self.world.set_chest_viewers(Vec::new());
        self.mobs.entities.set_vehicle_viewers(Vec::new());
        crate::simulation::tick_unattended_world_rules(
            &mut self.world,
            self.gamerules.int("fire_spread_radius_around_player") != 0,
            self.gamerules.int("random_tick_speed") as u32,
            running,
        );
        let block_pushes = self.world.take_block_pushes();
        self.mobs.entities.apply_block_pushes(&block_pushes);
        if !self.world.piston_pushes().is_empty() {
            self.mobs.entities.apply_piston_pushes(&self.world);
        }
        self.mobs.entities.stalactite_hits(self.world.stalactite_hits());
        if running {
            self.world.tick_automation_entities(&mut self.mobs.entities);
            self.tick_trial();
            self.tick_beacons();
            self.tick_conduits();
            self.tick_potent_sulfur();
            self.explode_anchors();
            self.update_mobs(dt);
        } else {
            // `entityManager.tick` runs whatever `emptyTime` says: entities
            // leave with their chunks and come back with them.
            self.mobs.entities.sync_chunk_entities(&mut self.world);
            self.resolve_waiting();
        }
        self.spawn_world_drops();
        self.take_pearl_tickets();
        self.discard_presentation();
    }

    /// What a level nobody is in would have shown or played: dropped.
    fn discard_presentation(&mut self) {
        let w = &mut self.world;
        w.notes.clear();
        w.records.clear();
        w.records_started.clear();
        w.page_turns.clear();
        w.chorus_events.clear();
        w.soul_sounds.clear();
        w.trip_sounds.clear();
        w.brews_done.clear();
        w.particles.drain().for_each(drop);
        drop(w.take_sculk_sounds());
        drop(w.take_lid_cues());
        drop(w.take_shulker_cues());
        drop(w.take_crafter_events());
        drop(w.take_composter_cues());
        drop(w.take_bone_meal_sounds());
        w.mesh_uploads.clear();
        w.mesh_removals.clear();
        self.mobs.entities.clear_pending_sounds();
    }

    /// v1.0 final F11: a pearl of the host's landed in a level the host
    /// isn't in (`ThrownEnderpearl.onHit` teleports its owner across when
    /// `isAllowedToTeleportOwner`: `canUsePortal(true)`, alive and not
    /// sleeping). The host goes once the levels have ticked.
    pub(super) fn pearl_owner_away(&mut self, owner: crate::entity::PlayerId, pos: DVec3) {
        if owner == crate::entity::PlayerId::HOST {
            self.pearl_trip = Some((self.dimension, pos));
        } else {
            // Other players follow the host between dimensions here.
            log::info!("a pearl of a player in another dimension landed in the {}: they stay", self.dimension.name());
        }
    }

    /// Takes the host to where their pearl landed in another level.
    pub(super) fn take_pearl_trip(&mut self) {
        let Some((to, pos)) = self.pearl_trip.take() else { return };
        if to == self.dimension
            || self.session.vitals.is_dead()
            || self.session.sleeping.is_some()
            || self.arrival.is_some()
            || self.credits.is_some()
        {
            return;
        }
        // `ServerPlayer.teleport` to another level leaves its vehicle.
        if self.session.player.vehicle.take().is_some() {
            self.mobs.entities.dismount(&self.world, crate::entity::PlayerId::HOST);
        }
        self.switch_dimension(to, dimension::Arrival::Pearl { pos });
    }

    /// The pearls `owner` has thrown, wherever they fly, as their player
    /// data saves them.
    pub(super) fn saved_pearls(&self, owner: crate::entity::PlayerId) -> Option<serde_json::Value> {
        let levels = std::iter::once((self.dimension, &self.mobs.entities))
            .chain(self.levels.iter().map(|l| (l.dimension, &l.entities)));
        self.owned_pearls.save_levels(levels, owner)
    }
}

#[cfg(test)]
mod tests {
    use super::super::dimension::{OwedPortal, owed_portals_text, parse_owed_portals};
    use super::super::portal_travel::{PortalArrival, Route, arrivals_text, parse_arrivals};
    use super::*;
    use crate::world::portal::PortalTrip;

    /// v1.0 final F11: a save from before F11 with entities waiting for
    /// other dimensions (Track F2's `portal_arrivals`), portals owed to the
    /// Nether (Track 8c's `owed_nether_portals`) and pearls waiting in a
    /// level (Track 8f's `ender_pearls` with their dimension) still reads,
    /// and each opens the level it waits for, where it is then resolved
    /// as soon as its ground loads.
    #[test]
    fn old_saves_waiting_work_opens_the_levels_it_waits_for() {
        let trip = PortalTrip::without_portal(IVec3::new(40, 70, -8));
        let arrivals = vec![
            PortalArrival {
                to: Dimension::Nether,
                route: Route::Nether(trip),
                width: 0.9,
                height: 1.4,
                yaw: 0.0,
                batch: vec![("nether_mobs".into(), "{}".into())],
            },
            PortalArrival {
                to: Dimension::End,
                route: Route::EndSpawn { vel: DVec3::ZERO },
                width: 0.25,
                height: 0.25,
                yaw: 0.0,
                batch: Vec::new(),
            },
        ];
        let arrivals = parse_arrivals(&arrivals_text(&arrivals));
        let owed =
            parse_owed_portals(&owed_portals_text(&[OwedPortal { target: IVec3::new(5, 64, 5), along_x: true }]));
        assert_eq!(owed.len(), 1);
        let mut pearls = crate::entity::saved_projectiles::OwnedPearls::default();
        let saved = serde_json::json!([{"owner": 0, "pos": [0.5, 80.0, 0.5], "motion": [1.0, 0.0, 0.0], "dimension": "overworld"}]);
        pearls.load(&saved, crate::entity::PlayerId::HOST);
        let waiting = pearls.waiting_in(|_| true);
        // In the End, with nothing else loaded: the Nether and the
        // Overworld open; the End is the host's.
        let open = waiting_levels(&arrivals, !owed.is_empty(), &waiting, &[Dimension::End]);
        assert_eq!(open, [Dimension::Nether, Dimension::Overworld]);
        // Already running: nothing more opens.
        assert!(
            waiting_levels(&arrivals, true, &waiting, &[Dimension::End, Dimension::Nether, Dimension::Overworld])
                .is_empty()
        );
        // Pearls of a player who isn't in the game wait with them.
        assert!(pearls.waiting_in(|_| false).is_empty());
    }

    #[test]
    fn levels_tick_in_javas_order() {
        assert!(order(Dimension::Overworld) < order(Dimension::Nether));
        assert!(order(Dimension::Nether) < order(Dimension::End));
    }
}
