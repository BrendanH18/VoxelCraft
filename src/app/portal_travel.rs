//! v1.0 final Track F2: mobs, items, projectiles, minecarts and boats
//! through Nether portals, End portals and End gateways, and the host's
//! vehicle taking them with it.
//!
//! [`crate::entity::portal_travel`] takes an entity that goes out of the
//! level. Through an End gateway it stays in the End and arrives at once.
//! Bound for another dimension, which doesn't run while the host is away,
//! it waits as a [`PortalArrival`] in the root level file until that
//! dimension next loads. Arriving there, before the host does, each one
//! makes the arrival Java made at the time of the trip (the other
//! dimension hasn't changed meanwhile, so the same `PortalForcer` search
//! finds, or `createPortal` builds, the same exit portal) and joins the
//! level; if its chunk unloads it goes to the chunk's region record as any
//! other entity does.

use glam::{DVec3, IVec3};

use crate::entity::portal_travel::Departure;
use crate::world::block::Block;
use crate::world::portal::{BORDER, PortalKind, PortalTrip};
use crate::world::region::Props;
use crate::world::terrain::Dimension;

use super::Game;
use super::dimension::OwedPortal;

/// The root level file's key for [`PortalArrival`]s, written only when
/// some are waiting.
pub(super) const PORTAL_ARRIVALS_KEY: &str = "portal_arrivals";
/// At most this many travellers wait; later ones stay where they are.
const MAX_PORTAL_ARRIVALS: usize = 4096;

/// Where a traveller to another dimension comes out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Route {
    /// Through a Nether portal: the exit portal around the trip's target
    /// (found or built), at the same place in it.
    Nether(PortalTrip),
    /// The exit portal the search found at this cell, outside the loaded area.
    NetherAt(IVec3, PortalTrip),
    /// Into the End: the obsidian platform, facing west, with this motion.
    EndSpawn { vel: DVec3 },
    /// Out of the End: the world spawn's top (`Entity.adjustSpawnLocation`).
    WorldSpawn { vel: DVec3 },
    /// A vehicle saved while the host it carries through an End gateway
    /// waits to arrive: the gateway's exit, found already.
    At { pos: DVec3, vel: DVec3 },
}

/// A traveller waiting for its dimension to load.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PortalArrival {
    pub to: Dimension,
    pub route: Route,
    /// The root entity's `EntityDimensions` and yaw as it left.
    pub width: f64,
    pub height: f64,
    pub yaw: f32,
    pub batch: Props,
}

fn dim_code(d: Dimension) -> &'static str {
    match d {
        Dimension::Overworld => "overworld",
        Dimension::Nether => "nether",
        Dimension::End => "end",
    }
}

fn v3(v: DVec3) -> serde_json::Value {
    serde_json::json!([v.x, v.y, v.z])
}

fn read_v3(v: &serde_json::Value) -> Option<DVec3> {
    Some(DVec3::new(v[0].as_f64()?, v[1].as_f64()?, v[2].as_f64()?))
}

fn read_i3(v: &serde_json::Value) -> Option<IVec3> {
    let i = |n: &serde_json::Value| n.as_i64().and_then(|n| i32::try_from(n).ok());
    Some(IVec3::new(i(&v[0])?, i(&v[1])?, i(&v[2])?))
}

/// The [`PORTAL_ARRIVALS_KEY`] text: a JSON list.
pub(super) fn arrivals_text(arrivals: &[PortalArrival]) -> String {
    let one = |a: &PortalArrival| {
        let mut v = serde_json::json!({
            "to": dim_code(a.to),
            "width": a.width,
            "height": a.height,
            "yaw": a.yaw,
            "batch": a.batch.iter().map(|(k, v)| serde_json::json!([k, v])).collect::<Vec<_>>(),
        });
        let trip = |v: &mut serde_json::Value, t: &PortalTrip| {
            v["target"] = serde_json::json!([t.target.x, t.target.y, t.target.z]);
            v["along_x"] = t.along_x.into();
            v["offset"] = v3(t.offset);
            v["vel"] = v3(t.vel);
        };
        match &a.route {
            Route::Nether(t) => {
                v["route"] = "nether".into();
                trip(&mut v, t);
            }
            Route::NetherAt(at, t) => {
                v["route"] = "nether".into();
                trip(&mut v, t);
                v["at"] = serde_json::json!([at.x, at.y, at.z]);
            }
            Route::EndSpawn { vel } => {
                v["route"] = "end_spawn".into();
                v["vel"] = v3(*vel);
            }
            Route::WorldSpawn { vel } => {
                v["route"] = "world_spawn".into();
                v["vel"] = v3(*vel);
            }
            Route::At { pos, vel } => {
                v["route"] = "at".into();
                v["pos"] = v3(*pos);
                v["vel"] = v3(*vel);
            }
        }
        v
    };
    serde_json::Value::Array(arrivals.iter().map(one).collect()).to_string()
}

/// Reads [`arrivals_text`]; damaged entries are skipped.
pub(super) fn parse_arrivals(text: &str) -> Vec<PortalArrival> {
    let root: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let one = |v: &serde_json::Value| {
        let to = match v["to"].as_str()? {
            "overworld" => Dimension::Overworld,
            "nether" => Dimension::Nether,
            "end" => Dimension::End,
            _ => return None,
        };
        let vel = read_v3(&v["vel"]).unwrap_or(DVec3::ZERO);
        let route = match v["route"].as_str()? {
            "nether" => {
                let trip = PortalTrip {
                    target: read_i3(&v["target"])?,
                    along_x: v["along_x"].as_bool().unwrap_or(true),
                    offset: read_v3(&v["offset"]).unwrap_or(DVec3::new(0.5, 0.0, 0.0)),
                    vel,
                    spectator: false,
                };
                match read_i3(&v["at"]) {
                    Some(at) => Route::NetherAt(at, trip),
                    None => Route::Nether(trip),
                }
            }
            "end_spawn" => Route::EndSpawn { vel },
            "world_spawn" => Route::WorldSpawn { vel },
            "at" => Route::At { pos: read_v3(&v["pos"])?, vel },
            _ => return None,
        };
        let batch = v["batch"]
            .as_array()?
            .iter()
            .filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_str()?.to_string())))
            .collect();
        Some(PortalArrival {
            to,
            route,
            width: v["width"].as_f64().unwrap_or(0.6),
            height: v["height"].as_f64().unwrap_or(1.8),
            yaw: v["yaw"].as_f64().unwrap_or(0.0) as f32,
            batch,
        })
    };
    root.as_array().into_iter().flatten().filter_map(one).take(MAX_PORTAL_ARRIVALS).collect()
}

/// `EndPortalBlock.getPortalDestination` for anything but a player: the
/// bottom centre of `ServerLevel.END_SPAWN_POINT`, a block above where a
/// player arrives (Java lowers only a `ServerPlayer` by one), so it drops
/// onto the platform.
pub(super) fn end_spawn_arrival() -> DVec3 {
    crate::world::end::SPAWN.as_dvec3() + DVec3::new(0.5, 1.0, 0.5)
}

impl Game {
    /// v1.0 final Track F2: the travellers to save: those waiting, and the
    /// vehicle carrying the host while the host waits to arrive, routed as
    /// the host's arrival is.
    pub(super) fn saved_portal_arrivals(&self) -> Vec<PortalArrival> {
        use super::dimension::Arrival;
        let mut out = self.portal_arrivals.clone();
        if let Some(v) = &self.carried_vehicle {
            let route = match self.arrival {
                Some(Arrival::Portal(trip)) => Some((self.dimension, Route::Nether(trip))),
                Some(Arrival::PortalAt(at, trip)) => Some((self.dimension, Route::NetherAt(at, trip))),
                Some(Arrival::EndSpawn { .. }) => Some((Dimension::End, Route::EndSpawn { vel: v.vel })),
                Some(Arrival::SpawnTop { vel }) => Some((Dimension::Overworld, Route::WorldSpawn { vel })),
                Some(Arrival::Gateway { exit, .. }) => {
                    Some((self.dimension, Route::At { pos: exit.as_dvec3() + DVec3::new(0.5, 1.0, 0.5), vel: v.vel }))
                }
                _ => None,
            };
            let (to, route) = route.unwrap_or((self.dimension, Route::At { pos: v.pos, vel: DVec3::ZERO }));
            out.push(PortalArrival { to, route, width: v.width, height: v.height, yaw: v.yaw, batch: v.batch.clone() });
        }
        out
    }

    /// `NetherPortalBlock.getPortalDestination`'s trip for a traveller of
    /// `width` by `height` at `pos` (moving at `vel`) leaving through the
    /// portal at `entry` for `to`: the scaled position at its own height,
    /// clamped to the world border, and where it stood in the portal.
    pub(super) fn nether_trip(
        &self,
        to: Dimension,
        entry: IVec3,
        pos: DVec3,
        vel: DVec3,
        (width, height): (f64, f64),
    ) -> PortalTrip {
        let scale = if to == Dimension::Nether { 1.0 / 8.0 } else { 8.0 };
        let target = BORDER.clamp(DVec3::new(pos.x * scale, pos.y, pos.z * scale));
        let mut trip = PortalTrip::without_portal(target);
        if self.world.get_block(entry).is_some_and(|b| b.is_nether_portal()) {
            let (rect, along_x) = self.world.portal_rect(entry);
            trip.along_x = along_x;
            trip.offset = crate::world::portal::relative_position(rect, along_x, pos, width, height);
        }
        trip.vel = vel;
        trip
    }

    /// v1.0 final Track F2: each tick before the entities move, the host's
    /// entities' portal steps ([`crate::entity::Entities::portal_departures`]),
    /// and the trips of those that go. A LAN client's entities are the host's.
    pub(super) fn entity_portals(&mut self) {
        if self.lan.client.is_some() || self.arrival.is_some() || self.credits.is_some() {
            return;
        }
        if self.portal_arrivals.iter().any(|a| a.to == self.dimension) {
            self.resolve_portal_arrivals(false);
        }
        let nether_allowed = self.gamerules.portal_allows_trip_from(self.dimension);
        let in_end = self.dimension == Dimension::End;
        let mut fight = self.mobs.entities.fight.take();
        // `EndGatewayBlock.entityInside`: a gateway not cooling down starts
        // its cooldown and lets the entity in.
        let open = |cell: IVec3| {
            in_end
                && fight.as_mut().is_some_and(|f| {
                    if f.gateway_entities.cooling(cell) {
                        return false;
                    }
                    f.gateway_entities.trigger(cell);
                    true
                })
        };
        let (departures, refused) = self.mobs.entities.portal_departures(&self.world, nether_allowed, open);
        self.mobs.entities.fight = fight;
        // A refused trip still owes the Nether its exit portal, as the
        // host's does (v1.0 gap Track 8c).
        for r in refused {
            let trip = self.nether_trip(Dimension::Nether, r.entry, r.pos, DVec3::ZERO, (0.0, 0.0));
            self.owe_portal(OwedPortal { target: trip.target, along_x: trip.along_x });
        }
        for d in departures {
            self.send_through(d);
        }
    }

    /// Sends a departed entity on: through a gateway at once, or to wait
    /// for its dimension.
    fn send_through(&mut self, d: Departure) {
        let size = (d.width, d.height);
        let java_yaw = crate::knockback::java_yaw_degrees(d.yaw);
        let (to, route) = match (d.kind, self.dimension) {
            // As for the host, a Nether portal in the End leads nowhere here.
            (PortalKind::Nether, Dimension::End) => {
                self.mobs.entities.arrive(&d.batch, d.pos, None, 0.0, false);
                return;
            }
            (PortalKind::Nether, from) => {
                let to = from.other();
                (to, Route::Nether(self.nether_trip(to, d.entry, d.pos, d.vel, size)))
            }
            (PortalKind::End, Dimension::End) => (Dimension::Overworld, Route::WorldSpawn { vel: d.vel }),
            (PortalKind::End, _) => {
                (Dimension::End, Route::EndSpawn { vel: crate::world::portal::end_arrival_motion(d.vel, java_yaw) })
            }
            (PortalKind::Gateway, _) => {
                self.entity_through_gateway(d);
                return;
            }
        };
        if self.portal_arrivals.len() >= MAX_PORTAL_ARRIVALS {
            log::warn!("too many entities wait to arrive in other dimensions: this one stays");
            self.mobs.entities.arrive(&d.batch, d.pos, None, 0.0, false);
            return;
        }
        log::info!("an entity went through a portal to the {}", to.name());
        self.portal_arrivals.push(PortalArrival {
            to,
            route,
            width: d.width,
            height: d.height,
            yaw: d.yaw,
            batch: d.batch,
        });
    }

    /// `EndGatewayBlock.getPortalDestination` for an entity: to the
    /// gateway's exit, keeping its motion and facing (`Relative.DELTA`,
    /// `ROTATION`), building the exit gateway (and its island) and linking
    /// it the first time. An exit the game can't reach yet (its ground not
    /// loaded) leaves the entity where it was.
    fn entity_through_gateway(&mut self, d: Departure) {
        let stay = |game: &mut Self, d: &Departure| {
            game.mobs.entities.arrive(&d.batch, d.pos, Some(d.vel), 0.0, false);
        };
        if self.dimension != Dimension::End || self.world.get_block(d.entry) != Some(Block::END_GATEWAY) {
            return stay(self, &d);
        }
        let Some(fight) = &self.mobs.entities.fight else { return stay(self, &d) };
        let known = fight.gateway_exit(d.entry);
        let exact = self.world.return_gateway_exit(d.entry);
        let loaded = |game: &Self, at: IVec3, r: i32| {
            [(-r, -r), (r, -r), (-r, r), (r, r), (0, 0)]
                .iter()
                .all(|&(dx, dz)| game.world.column_loaded(at.x + dx, at.z + dz))
        };
        let pos = match (known, exact) {
            (Some(exit), _) if loaded(self, exit, 6) => {
                if self.world.get_block(exit) != Some(Block::END_GATEWAY) {
                    self.world.build_gateway(exit);
                }
                self.world.gateway_arrival(exit)
            }
            (None, Some(exit)) if loaded(self, exit, 0) => exit.as_dvec3() + DVec3::new(0.5, 0.0, 0.5),
            (None, None) => match self.world.find_gateway_exit(d.entry) {
                Some(found) if loaded(self, found.exit, if found.island.is_some() { 16 } else { 6 }) => {
                    if let Some(origin) = found.island {
                        self.world.build_end_island(origin);
                    }
                    if self.world.get_block(found.exit) != Some(Block::END_GATEWAY) {
                        self.world.build_gateway(found.exit);
                    }
                    if let Some(fight) = &mut self.mobs.entities.fight {
                        fight.link_gateways(d.entry, found.exit);
                        fight.link_gateways(found.exit, d.entry);
                    }
                    self.world.gateway_arrival(found.exit)
                }
                _ => return stay(self, &d),
            },
            _ => return stay(self, &d),
        };
        self.mobs.entities.arrive(&d.batch, pos, Some(d.vel), 0.0, false);
    }

    /// Arriving in a dimension, the travellers waiting for it come first,
    /// oldest first, each once the ground around its arrival has loaded.
    /// With `hold` the host waits, held there so it streams in, behind the
    /// arrival screen; without it (a world opened in the dimension they
    /// wait for) they arrive as their ground loads. Returns whether one is
    /// still waiting.
    pub(super) fn resolve_portal_arrivals(&mut self, hold: bool) -> bool {
        loop {
            match self.resolve_portal_arrival(hold) {
                Some(true) => continue,
                Some(false) => return true,
                None => return false,
            }
        }
    }

    /// The first traveller waiting for this dimension: `Some(true)` if it
    /// arrived, `Some(false)` if it waits for its ground, `None` if none waits.
    fn resolve_portal_arrival(&mut self, hold: bool) -> Option<bool> {
        let i = self.portal_arrivals.iter().position(|a| a.to == self.dimension)?;
        let a = self.portal_arrivals[i].clone();
        let (target, r) = match a.route {
            Route::Nether(trip) => (trip.target, 16),
            Route::NetherAt(at, _) => (at, 2),
            Route::EndSpawn { .. } => (crate::world::end::SPAWN, 2),
            Route::WorldSpawn { .. } => (self.world_spawn, 0),
            Route::At { pos, .. } => (pos.floor().as_ivec3(), 0),
        };
        let ready = [(-r, -r), (r, -r), (-r, r), (r, r), (0, 0)]
            .iter()
            .all(|&(dx, dz)| self.world.column_loaded(target.x + dx, target.z + dz));
        if !ready {
            if hold {
                self.session.player.pos = target.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                self.session.player.vel = DVec3::ZERO;
            }
            return Some(false);
        }
        let (pos, vel, turn) = match a.route {
            Route::Nether(trip) | Route::NetherAt(_, trip) => {
                let exit = match a.route {
                    Route::NetherAt(at, _) => Some(self.world.portal_rect(at)),
                    _ => {
                        let radius = if self.dimension == Dimension::Nether {
                            crate::world::portal::NETHER_PORTAL_RADIUS
                        } else {
                            crate::world::portal::OVERWORLD_PORTAL_RADIUS
                        };
                        match self.world.find_portal(trip.target, radius) {
                            Some(at) if !self.world.column_loaded(at.x, at.z) => {
                                self.portal_arrivals[i].route = Route::NetherAt(at, trip);
                                return Some(false);
                            }
                            Some(at) => Some(self.world.portal_rect(at)),
                            None => self.world.create_portal(trip.target, trip.along_x).map(|r| (r, trip.along_x)),
                        }
                    }
                };
                self.nether_exit(exit, &trip, (a.width, a.height), a.yaw)
            }
            Route::EndSpawn { vel } => {
                self.build_end_platform();
                (end_spawn_arrival(), vel, std::f32::consts::PI - a.yaw)
            }
            Route::WorldSpawn { vel } => (self.spawn_top(), vel, 0.0),
            Route::At { pos, vel } => (pos, vel, 0.0),
        };
        self.portal_arrivals.remove(i);
        self.mobs.entities.arrive(&a.batch, pos, Some(vel), turn, false);
        Some(true)
    }

    /// `createDimensionTransition` out of `exit` (the exit portal's
    /// rectangle and axis): the same place in it, free of collisions, the
    /// yaw and motion turned a quarter if the portals' axes differ. Returns
    /// the position, motion and the yaw's turn.
    pub(super) fn nether_exit(
        &self,
        exit: Option<(crate::world::portal::PortalRect, bool)>,
        trip: &PortalTrip,
        (width, height): (f64, f64),
        yaw: f32,
    ) -> (DVec3, DVec3, f32) {
        match exit {
            Some((rect, along_x)) => {
                let at = crate::world::portal::exit_position(rect, along_x, trip.offset, width, height);
                let (vel, turn) = if along_x != trip.along_x {
                    let java_yaw = crate::knockback::java_yaw_degrees(yaw);
                    (crate::world::portal::quarter_turn_motion(trip.vel, java_yaw), std::f32::consts::FRAC_PI_2)
                } else {
                    (trip.vel, 0.0)
                };
                (self.world.collision_free_position(at, width, height), vel, turn)
            }
            None => {
                log::error!("unable to create a portal at {}", trip.target);
                (trip.target.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), trip.vel, 0.0)
            }
        }
    }

    /// `Entity.adjustSpawnLocation`: the world spawn's column, one above
    /// its `MOTION_BLOCKING_NO_LEAVES` height (the highest block that
    /// blocks motion or holds fluid, leaves aside), at its centre.
    pub(super) fn spawn_top(&self) -> DVec3 {
        let s = self.world_spawn;
        let mut y = self.world.world_surface_height(s.x, s.z).unwrap_or(s.y);
        while y > self.world.min_y() {
            let b = self.world.get_block(IVec3::new(s.x, y, s.z)).unwrap_or(Block::AIR);
            if (b.is_solid() || b.is_fluid()) && !b.is_leaves() {
                break;
            }
            y -= 1;
        }
        DVec3::new(f64::from(s.x) + 0.5, f64::from(y + 1), f64::from(s.z) + 0.5)
    }

    /// `EndPlatformFeature.createEndPlatform(.., true)`: every block of the
    /// platform that isn't obsidian (its floor) or air (the three layers
    /// above) is replaced with `destroyBlock(pos, true)`, dropping what was
    /// there.
    pub(super) fn build_end_platform(&mut self) {
        let spawn = crate::world::end::SPAWN;
        for z in -2..=2 {
            for x in -2..=2 {
                for y in -1..3 {
                    let cell = spawn + IVec3::new(x, y, z);
                    let want = if y == -1 { Block::OBSIDIAN } else { Block::AIR };
                    let Some(was) = self.world.get_block(cell).filter(|&b| b != want) else { continue };
                    if was != Block::AIR {
                        self.world.spill_block(cell, was);
                        self.world.particles.push(crate::particles::Request::Break { cell, block: was });
                    }
                    self.world.set_block(cell, want);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_travellers_round_trip_the_root_level_file() {
        let trip = PortalTrip {
            target: IVec3::new(-12, 70, 300),
            along_x: false,
            offset: DVec3::new(0.25, 0.5, 0.5),
            vel: DVec3::new(0.1, -0.2, 0.0),
            spectator: false,
        };
        let batch: Props = vec![("nether_mobs".into(), "{\"mobs\":[]}".into())];
        let arrivals = vec![
            PortalArrival {
                to: Dimension::Nether,
                route: Route::Nether(trip),
                width: 0.6,
                height: 1.95,
                yaw: 1.5,
                batch: batch.clone(),
            },
            PortalArrival {
                to: Dimension::Overworld,
                route: Route::NetherAt(IVec3::new(5, 64, 9), trip),
                width: 0.98,
                height: 0.7,
                yaw: 0.0,
                batch: batch.clone(),
            },
            PortalArrival {
                to: Dimension::End,
                route: Route::EndSpawn { vel: DVec3::X },
                width: 0.25,
                height: 0.25,
                yaw: 0.0,
                batch: batch.clone(),
            },
            PortalArrival {
                to: Dimension::Overworld,
                route: Route::WorldSpawn { vel: DVec3::ZERO },
                width: 0.5,
                height: 0.5,
                yaw: 0.0,
                batch,
            },
        ];
        assert_eq!(parse_arrivals(&arrivals_text(&arrivals)), arrivals);
        assert!(parse_arrivals("not json").is_empty());
    }
}
