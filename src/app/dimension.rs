//! The overworld, the Nether and the End: lighting portals, travelling
//! through them, and keeping each dimension's blocks, containers and
//! dropped items in its own save.
//!
//! Saves keep the overworld where they always have (the world folder), the
//! Nether in a `nether` folder inside it and the End in `end`. The root
//! `level.txt` holds the player and everything global, plus the overworld's
//! furnaces, chests and items; `nether/level.txt` holds the Nether's.

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{DVec3, IVec3};

use crate::audio::SoundSource;
use crate::audio::sounds::{Material, Sound};
use crate::world::World;
use crate::world::block::Block;
use crate::world::portal::{BORDER, PortalTrip};
use crate::world::storage::{self, SavedChunks, Storage};
use crate::world::terrain::{Dimension, Generator};

use super::Game;

/// `Player.getDimensionChangingDelay`: 10 ticks, in seconds.
const PORTAL_COOLDOWN: f64 = 10.0 / 20.0;

/// Level properties that belong to one dimension rather than the player.
pub(super) const DIMENSION_KEYS: [&str; 40] = [
    "furnaces",
    "chests",
    "items",
    "orbs",
    "spawners",
    "brewing",
    "dragon",
    "villagers",
    "redstone",
    "automation",
    "minecarts",
    "decor",
    "nether_mobs",
    "falling",
    "wood",
    "utilities",
    "beehives",
    "archaeology",
    "jukeboxes",
    "creaking_hearts",
    "potion_clouds",
    "copper_golem_statues",
    "sculk",
    "tridents",
    "trial",
    "ominous_item_spawners",
    "workstations",
    "raids",
    // v0.16 Track C.
    "beacons",
    // v0.16 Track F.
    "conduits",
    // v0.16 Track H.
    "potent_sulfur",
    "wither_skulls",
    // v0.16 Track G.
    "end_crystals",
    // v0.17 Track C1.
    "crafters",
    // v1.0 Track 3b: region storage.
    "region_chunks",
    "fluid_ticks",
    "stored_index",
    "entity_ids",
    // v1.0 Track 7: dragons hatched from eggs.
    "ender_dragons",
    // v1.0 review fixes: the level file's save generation.
    "save_generation",
];
/// How far around the arrival point to look for an existing portal.
const SEARCH_RADIUS: i32 = 16;

/// Java's `EntityDimensions` of a collision shape: width and height, as
/// floats.
fn java_size(shape: crate::physics::Shape) -> (f64, f64) {
    (f64::from((shape.half_width * 2.0) as f32), f64::from(shape.height as f32))
}

/// Where the player is headed while the destination streams in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Arrival {
    /// Through a portal, to the nearest portal around the trip's target
    /// (one is built along the entry portal's axis if there's none), at the
    /// same place in it (v1.0 gap Track 5).
    Portal(PortalTrip),
    /// v0.16 Track G: the portal the search found, out of the loaded area:
    /// wait for it to load, then step in.
    PortalAt(IVec3, PortalTrip),
    /// Back from the dead: the bed or world spawn.
    Respawn,
    /// Back from the dead to the Nether respawn anchor at this cell.
    Anchor(IVec3),
    EndSpawn,
    /// Through an End gateway at `from` to the one at `exit`, which is
    /// built (after the `EndIslandFeature` island at `island`, if any) and
    /// linked back if it's new. The traveller keeps `vel` (Java's
    /// `Relative.DELTA`).
    Gateway {
        exit: IVec3,
        from: IVec3,
        island: Option<IVec3>,
        link: bool,
        vel: DVec3,
    },
}

/// Where a dimension's save lives inside the world folder.
pub(super) fn storage_for(root: &Storage, dim: Dimension) -> Storage {
    match dim {
        Dimension::Overworld => Storage::new(root.dir()),
        Dimension::Nether => Storage::new(root.dir().join("nether")),
        Dimension::End => Storage::new(root.dir().join("end")),
    }
}

/// A dimension's saved chunks and properties. The overworld's properties
/// live in the root level file, which the caller already has. The third
/// value tells the player about damage the load worked around (v1.0
/// Track 1): chunks that couldn't be read regenerate, as Java's
/// `RegionFile` does, and a dimension level file that can't be read is
/// replaced by the copy its last save kept. Damaged files are copied aside
/// first, so nothing is silently lost.
pub(super) fn load_dimension(
    root: &Storage,
    dim: Dimension,
    overworld_props: &BTreeMap<String, String>,
) -> (SavedChunks, BTreeMap<String, String>, Option<String>) {
    let storage = storage_for(root, dim);
    if !storage.exists() && !storage.has_chunks_without_level() {
        return Default::default();
    }
    let mut notices = Vec::new();
    let chunks = match storage.load_chunks_recovering() {
        Ok((chunks, None)) => chunks,
        Ok((chunks, Some(damage))) => {
            let kept = damage.kept_as.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
            notices.push(format!(
                "Some of the {}'s saved chunks couldn't be read ({}): {} of {} were kept and the rest regenerate.{}",
                dim.name(),
                damage.reason,
                damage.recovered,
                damage.saved,
                kept.map(|k| format!(" The damaged file was kept as {k}.")).unwrap_or_default()
            ));
            chunks
        }
        Err(e) => {
            log::error!("failed to load {} chunks: {e}", dim.name());
            notices.push(format!(
                "The {}'s saved chunks couldn't be read ({e}), so its terrain regenerates. The file is kept.",
                dim.name()
            ));
            Default::default()
        }
    };
    let props = match dim {
        Dimension::Overworld => overworld_props.clone(),
        Dimension::Nether | Dimension::End => match storage.load_level() {
            Ok(level) => level.props,
            Err(e) => {
                log::error!("failed to load the {} level file: {e}", dim.name());
                let kept =
                    storage.dir().join("level.txt").exists().then(|| storage.keep_damaged("level.txt")).flatten();
                let kept = kept.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
                let kept = kept.map(|k| format!(" The damaged file was kept as {k}.")).unwrap_or_default();
                match storage.load_level_old() {
                    Ok(level) => {
                        notices.push(format!(
                            "The {}'s level file couldn't be read ({e}), so its last save's copy was used.{kept}",
                            dim.name()
                        ));
                        level.props
                    }
                    Err(_) => {
                        notices.push(format!(
                            "The {}'s level file couldn't be read ({e}): its containers and entities are lost.{kept}",
                            dim.name()
                        ));
                        Default::default()
                    }
                }
            }
        },
    };
    (chunks, props, (!notices.is_empty()).then(|| notices.join(" ")))
}

/// v1.0 Track 3b: opens the dimension's region storage and attaches it to
/// `world`, which then keeps its unloaded chunks there. `props` are the
/// dimension's level file properties; without `region_chunks` they are from
/// before region storage and hold every chunk's state. If such a world also
/// has region files beside its `chunks.bin`, a session crashed before its
/// first save wrote them: they are moved aside as `region.unsaved-<secs>`,
/// and the world opens as that save left it. `writable` is false for
/// scripted runs, which read records but never write them. Returns damage
/// to tell the player about.
pub(super) fn attach_regions(
    world: &mut World,
    root: &Storage,
    dim: Dimension,
    props: &BTreeMap<String, String>,
    writable: bool,
) -> Option<String> {
    use crate::world::region::{REGION_DIR, RegionStore};
    let storage = storage_for(root, dim);
    let dir = storage.dir().join(REGION_DIR);
    let owned = props.get(crate::world::REGION_KEY).map(|t| World::parse_region_chunks(t));
    if writable && owned.is_none() && storage.dir().join(crate::world::storage::CHUNKS_FILE).is_file() && dir.is_dir() {
        let to = storage.dir().join(format!("{REGION_DIR}.unsaved-{}", storage::unix_secs()));
        log::warn!("moving region files no level file names aside to {}", to.display());
        if let Err(e) = std::fs::rename(&dir, &to) {
            log::error!("couldn't move {} aside: {e}", dir.display());
        }
    }
    match RegionStore::open(&dir, writable) {
        Ok((store, damage)) => {
            // v1.0 Track 1b: a stable line for the save cycle's collection
            // (`tools/save_fixtures/collect.py`), with the records the
            // region tables name, whether or not the chunks load.
            log::info!("region storage: {} chunk records in the {}", store.stored_at_open().len(), dim.name());
            let generation = props.get(crate::world::GENERATION_KEY).and_then(|g| g.parse().ok()).unwrap_or(0);
            world.attach_regions(store, owned, generation, writable);
            let notices: Vec<String> = damage.iter().map(|d| d.notice(dim.name())).collect();
            (!notices.is_empty()).then(|| notices.join(" "))
        }
        Err(e) => {
            log::error!("failed to open the {} region files: {e}", dim.name());
            Some(format!(
                "The {}'s region files couldn't be read ({e}), so its saved chunks regenerate. The files are kept.",
                dim.name()
            ))
        }
    }
}

/// The furnace, chest and item properties out of a level's properties.
pub(super) fn dimension_props(props: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    props.iter().filter(|(k, _)| DIMENSION_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

impl Game {
    /// Tells the player what a damaged save lost (v1.0 Track 1): the whole
    /// notice goes to chat, and the action bar points there.
    pub(super) fn report_load_damage(&mut self, notice: &str) {
        log::warn!("{notice}");
        self.chat_line(notice);
        self.show_popup("Part of this world's save was damaged; open chat for details");
    }

    /// This dimension's furnaces, chests and dropped items, for saving.
    pub(super) fn dimension_props(&self) -> BTreeMap<String, String> {
        let mut props = BTreeMap::new();
        props.insert("redstone".to_string(), self.world.redstone_to_string());
        props.insert("automation".to_string(), self.world.automation_to_string());
        props.insert("falling".to_string(), self.world.falling_to_string());
        props.insert("furnaces".to_string(), self.world.furnaces_to_string());
        props.insert("brewing".to_string(), self.world.brewing_stands_to_string());
        props.insert("chests".to_string(), self.world.chests_to_string());
        props.insert("wood".to_string(), self.world.wood_to_string());
        props.insert("beehives".into(), self.world.beehives_to_string());
        props.insert("archaeology".into(), self.world.archaeology_to_string());
        props.insert("jukeboxes".into(), self.world.jukeboxes_to_string());
        props.insert("creaking_hearts".into(), self.world.creaking_hearts_to_string());
        props.insert("sculk".into(), self.world.sculk_to_string());
        props.insert("copper_golem_statues".into(), self.world.copper_golem_statues_to_string());
        props.insert("trial".into(), self.world.trial_to_string());
        props.insert("spawners".to_string(), self.world.spawners_to_string());
        props.insert("utilities".to_string(), self.world.utilities_to_string());
        props.insert("workstations".to_string(), self.world.workstations_to_string());
        props.insert("beacons".to_string(), self.world.beacons_to_string());
        props.insert("conduits".to_string(), self.world.conduits_to_string());
        props.insert("potent_sulfur".to_string(), self.world.potent_sulfur_to_string());
        props.insert("crafters".to_string(), self.world.crafters_to_string());
        self.mobs.entities.save_props(&mut props);
        // v1.0 Track 3b: what region storage needs from the level file.
        if self.world.writes_records() {
            props.insert(crate::world::REGION_KEY.to_string(), self.world.region_chunks_to_string());
            props.insert(crate::world::GENERATION_KEY.to_string(), self.world.save_generation().to_string());
            props.insert(crate::world::FLUID_TICKS_KEY.to_string(), self.world.fluid_ticks_to_string());
            props.insert("stored_index".to_string(), self.mobs.entities.stored_index_to_string());
            props.insert("entity_ids".to_string(), self.mobs.entities.entity_ids_to_string());
        }
        if let Some(fight) = &self.mobs.entities.fight {
            props.insert("dragon".to_string(), fight.serialize(&self.mobs.entities.crystals));
        }
        props
    }

    /// Sets up a freshly loaded dimension's containers and items.
    pub(super) fn restore_dimension(&mut self, props: &BTreeMap<String, String>) {
        self.session.player.vehicle = None;
        for bot in self.agents.players.values_mut() {
            bot.agent.session.player.vehicle = None;
        }
        if let Some(r) = props.get("redstone") {
            self.world.load_redstone(r);
        }
        if let Some(a) = props.get("automation") {
            self.world.load_automation(a);
        }
        if let Some(f) = props.get("falling") {
            self.world.load_falling(f);
        }
        if let Some(b) = props.get("brewing") {
            self.world.load_brewing_stands(b);
        }
        if let Some(f) = props.get("furnaces") {
            self.world.load_furnaces(f);
        }
        if let Some(c) = props.get("chests") {
            self.world.load_chests(c);
        }
        if let Some(a) = props.get("archaeology") {
            self.world.load_archaeology(a);
        }
        if let Some(j) = props.get("jukeboxes") {
            self.world.load_jukeboxes(j);
        }
        if let Some(h) = props.get("beehives") {
            self.world.load_beehives(h);
        }
        if let Some(s) = props.get("creaking_hearts") {
            self.world.load_creaking_hearts(s);
        }
        if let Some(s) = props.get("sculk") {
            self.world.load_sculk(s);
        }
        if let Some(s) = props.get("copper_golem_statues") {
            self.world.load_copper_golem_statues(s);
        }
        if let Some(s) = props.get("trial") {
            self.world.load_trial(s);
        }
        if let Some(w) = props.get("wood") {
            self.world.load_wood(w);
        }
        if let Some(s) = props.get("spawners") {
            self.world.load_spawners(s);
        }
        if let Some(u) = props.get("utilities") {
            self.world.load_utilities(u);
        }
        if let Some(w) = props.get("workstations") {
            self.world.load_workstations(w);
        }
        if let Some(b) = props.get("beacons") {
            self.world.load_beacons(b);
        }
        if let Some(c) = props.get("conduits") {
            self.world.load_conduits(c);
        }
        if let Some(c) = props.get("crafters") {
            self.world.load_crafters(c);
        }
        if let Some(g) = props.get("potent_sulfur") {
            self.world.load_potent_sulfur(g);
        }
        if let Some(f) = props.get(crate::world::FLUID_TICKS_KEY) {
            self.world.load_fluid_ticks(f);
        }
        // Entities in chunks that haven't streamed in yet move to per-chunk
        // storage on the first tick; ridden vehicles stay with their riders.
        // Ids saved after the level file in region records are reserved
        // first (see `Entities::recover_ids`).
        self.mobs.entities.recover_ids(&mut self.world);
        self.mobs.entities.load_props(props);
        if let Some(t) = props.get("stored_index") {
            self.mobs.entities.load_stored_index(t);
        }
        if let Some(t) = props.get("entity_ids") {
            self.mobs.entities.load_entity_ids(t);
        }
        if props.contains_key("minecarts") {
            if let Some(id) = self.mobs.entities.claim_rider(crate::entity::PlayerId::HOST) {
                self.session.player.vehicle = Some(id);
            }
            let ids: Vec<_> = self.agents.players.values().map(|b| b.agent.session.id).collect();
            for id in ids {
                if let Some(cart) = self.mobs.entities.claim_rider(id)
                    && let Some(bot) = self.agents.players.values_mut().find(|b| b.agent.session.id == id)
                {
                    bot.agent.session.player.vehicle = Some(cart);
                }
            }
        }
        if props.contains_key("nether_mobs") {
            self.session.player.vehicle =
                self.mobs.entities.mount_for_player(crate::entity::PlayerId::HOST).or(self.session.player.vehicle);
            for bot in self.agents.players.values_mut() {
                bot.agent.session.player.vehicle =
                    self.mobs.entities.mount_for_player(bot.agent.session.id).or(bot.agent.session.player.vehicle);
            }
        }
        self.mobs.entities.fight = self
            .world
            .generator
            .end()
            .map(|end| crate::entity::dragon::Fight::load(end, props.get("dragon").map(String::as_str)));
        // v0.16 Track G: older saves kept the pillar crystals in the fight.
        let entities = &mut self.mobs.entities;
        if let Some(fight) = &mut entities.fight {
            fight.restore_crystals(&mut entities.crystals, props.contains_key("end_crystals"));
        }
    }

    /// Saves this dimension and swaps in `to`, with the player waiting at
    /// `arrival` until the ground there has loaded. Returns false when the
    /// save failed: the current dimension stays loaded and the portal locks
    /// until the player steps out.
    pub(super) fn switch_dimension(&mut self, to: Dimension, arrival: Arrival) -> bool {
        // An open ender chest hands its contents back before the world goes.
        if let super::Container::Chest(pos) = self.body.container
            && self.inventory_open
        {
            self.close_ender_chest(pos);
        }
        // Scripted runs (screenshots, benchmarks) never write saves.
        if self.settings_path.is_some() {
            self.save();
            // A world that couldn't be saved stays loaded ("Could not save
            // the world" is already shown): travelling would drop what
            // wasn't written, and the Overworld's cached properties could
            // reach the root level file later without the records they
            // name. The player leaves the portal to try again.
            if self.save_error.is_some() {
                log::warn!("not travelling to the {}: this dimension couldn't be saved", to.name());
                self.portal_locked = true;
                return false;
            }
        }
        if self.dimension == Dimension::Overworld {
            if self.lan.client.is_none() {
                self.settle_crystal_hits();
            }
            self.overworld_props = self.dimension_props();
        }
        let (chunks, props, notice) = load_dimension(&self.storage, to, &self.overworld_props);
        let (seed, generation) = (self.world.generator.seed, self.world.generator.generation);
        log::info!("travelling to the {} ({} modified chunks)", to.name(), chunks.len());
        let loot_sequences = std::mem::take(&mut self.world.loot_sequences);
        let maps = std::mem::take(&mut self.world.maps);
        self.world = World::new(
            Arc::new(Generator::with_generation(seed, to, generation)),
            chunks,
            self.settings.render_distance,
        );
        self.world.maps = maps;
        self.world.loot_sequences = loot_sequences;
        self.world.set_simulation_distance(self.settings.simulation_distance);
        let region_notice = attach_regions(&mut self.world, &self.storage, to, &props, self.settings_path.is_some());
        self.mobs.entities = crate::entity::Entities::new(seed);
        self.renderer.clear_world();
        self.particles.clear();
        self.dimension = to;
        self.restore_dimension(&props);
        if let Some(notice) = notice.into_iter().chain(region_notice).reduce(|a, b| format!("{a} {b}")) {
            self.report_load_damage(&notice);
        }
        self.arrival = Some(arrival);
        self.portal_time = 0.0;
        self.session.sleeping = None;
        self.body.actions.reset();
        self.session.cancel_mining();
        self.session.cancel_consumption();
        self.session.player.vel = DVec3::ZERO;
        self.session.vitals.reset_fall();
        true
    }

    /// Goes through the portal the player is standing in.
    fn travel(&mut self) {
        let to = self.dimension.other();
        let scale = if to == Dimension::Nether { 1.0 / 8.0 } else { 8.0 };
        let player = &self.session.player;
        // `NetherPortalBlock.getPortalDestination`: the scaled position at
        // the traveller's own height, clamped to the world border; a new
        // portal follows the one entered, and the traveller arrives where
        // they stood in it (`getRelativePortalPosition`), keeping their motion.
        let target = BORDER.clamp(DVec3::new(player.pos.x * scale, player.pos.y, player.pos.z * scale));
        let (width, height) = java_size(player.collision_shape());
        let mut trip = PortalTrip::without_portal(target);
        if let Some(cell) = self.touched(Block::NETHER_PORTAL) {
            let (rect, along_x) = self.world.portal_rect(cell);
            trip.along_x = along_x;
            trip.offset = crate::world::portal::relative_position(rect, along_x, player.pos, width, height);
        }
        trip.vel = player.vel;
        self.switch_dimension(to, Arrival::Portal(trip));
    }

    /// While travelling: holds the player in place until the destination
    /// has streamed in, then puts them in (or builds) the portal there, or
    /// at their respawn point. Returns whether the player is still waiting.
    pub(super) fn update_arrival(&mut self) -> bool {
        let Some(arrival) = self.arrival else { return false };
        let target = match arrival {
            Arrival::Portal(trip) => trip.target,
            Arrival::PortalAt(p, _) => p,
            Arrival::EndSpawn => crate::world::end::SPAWN,
            Arrival::Gateway { exit, .. } => exit,
            // Load where `respawn_point` will put the player: the bed (checked
            // once its chunk is in), else the set spawn point or the
            // `spawnRadius` spot around the world spawn.
            Arrival::Respawn => self.session.spawn_bed.unwrap_or_else(|| self.respawn_point().0.floor().as_ivec3()),
            Arrival::Anchor(anchor) => anchor,
        };
        let r = match arrival {
            Arrival::Portal(..) => SEARCH_RADIUS,
            Arrival::PortalAt(..) => 2,
            Arrival::Respawn => 0,
            Arrival::Anchor(_) => 1,
            Arrival::EndSpawn => 2,
            // Room for the gateway's arrival search and a new island.
            Arrival::Gateway { island: None, .. } => 6,
            Arrival::Gateway { island: Some(_), .. } => 16,
        };
        let ready = [(-r, -r), (r, -r), (-r, r), (r, r), (0, 0)]
            .iter()
            .all(|&(dx, dz)| self.world.column_loaded(target.x + dx, target.z + dz));
        if !ready {
            self.session.player.pos = target.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            self.session.player.vel = DVec3::ZERO;
            return true;
        }
        // The bed's chunk is in, so `respawn_point` can now check the bed. If
        // it's gone, the bed is forgotten and the fallback spot may be in an
        // unloaded chunk: keep waiting, now targeting that spot.
        if matches!(arrival, Arrival::Respawn) && self.session.spawn_bed.is_some() {
            self.respawn_point();
            if self.session.spawn_bed.is_none() {
                return true;
            }
        }
        self.session.player.pos = match arrival {
            Arrival::Portal(trip) | Arrival::PortalAt(_, trip) => {
                let exit = match arrival {
                    Arrival::Portal(_) => {
                        // `PortalForcer`: an existing portal within 16 blocks
                        // into the Nether or 128 out of it, else a new one.
                        let radius = if self.dimension == Dimension::Nether {
                            crate::world::portal::NETHER_PORTAL_RADIUS
                        } else {
                            crate::world::portal::OVERWORLD_PORTAL_RADIUS
                        };
                        match self.world.find_portal(trip.target, radius) {
                            Some(at) if !self.world.column_loaded(at.x, at.z) => {
                                self.arrival = Some(Arrival::PortalAt(at, trip));
                                return true;
                            }
                            Some(at) => Some(self.world.portal_rect(at)),
                            None => self.world.create_portal(trip.target, trip.along_x).map(|r| (r, trip.along_x)),
                        }
                    }
                    Arrival::PortalAt(at, _) => Some(self.world.portal_rect(at)),
                    _ => unreachable!(),
                };
                // Arriving in a portal doesn't send you straight back.
                self.portal_locked = true;
                match exit {
                    Some((rect, along_x)) => {
                        // `createDimensionTransition`: the same place in the
                        // exit portal, free of collisions, turned a quarter
                        // if the portals' axes differ; pitch and motion kept.
                        let (width, height) = java_size(self.session.player.collision_shape());
                        let at = crate::world::portal::exit_position(rect, along_x, trip.offset, width, height);
                        if along_x != trip.along_x {
                            let yaw = self.session.player.yaw + std::f32::consts::FRAC_PI_2;
                            self.session.player.yaw = yaw.rem_euclid(std::f32::consts::TAU);
                        }
                        self.world.collision_free_position(at, width, height)
                    }
                    // Java finds no room even for a ledge (never in these
                    // dimensions' heights): arrive at the target.
                    None => {
                        log::error!("unable to create a portal at {}", trip.target);
                        trip.target.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
                    }
                }
            }
            Arrival::EndSpawn => {
                let spawn = crate::world::end::SPAWN;
                for z in -2..=2 {
                    for x in -2..=2 {
                        self.world.set_block(spawn + IVec3::new(x, -1, z), Block::OBSIDIAN);
                        for y in 0..3 {
                            self.world.set_block(spawn + IVec3::new(x, y, z), Block::AIR);
                        }
                    }
                }
                self.session.player.yaw = std::f32::consts::PI;
                self.session.player.pitch = 0.0;
                spawn.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
            }
            Arrival::Respawn => {
                let (at, yaw) = self.respawn_point();
                self.session.player.yaw = yaw;
                self.session.player.pitch = 0.0;
                at
            }
            Arrival::Anchor(anchor) => match self.world.use_anchor_respawn(anchor) {
                Some((at, yaw)) => {
                    self.session.player.yaw = yaw;
                    self.session.player.pitch = 0.0;
                    at
                }
                // Gone, out of charge or boxed in: the world spawn instead.
                None => {
                    self.session.spawn_anchor = None;
                    self.show_popup("You have no home bed or charged respawn anchor, or it was obstructed");
                    if self.switch_dimension(Dimension::Overworld, Arrival::Respawn) {
                        return true;
                    }
                    // The save failed and says so: the player stays here
                    // rather than retrying it every frame.
                    self.arrival = None;
                    return false;
                }
            },
            Arrival::Gateway { exit, from, island, link, .. } => {
                if let Some(origin) = island {
                    self.world.build_end_island(origin);
                }
                if self.world.get_block(exit) != Some(Block::END_GATEWAY) {
                    self.world.build_gateway(exit);
                }
                if link && let Some(fight) = &mut self.mobs.entities.fight {
                    fight.link_gateways(from, exit);
                    fight.link_gateways(exit, from);
                }
                self.audio.play_in(SoundSource::Ambient, Sound::Teleport, None, 0.8, (0.9, 1.0));
                // `handlePortal` sets the cooldown as it teleports.
                self.portal_cooldown = PORTAL_COOLDOWN;
                self.world.gateway_arrival(exit)
            }
        };
        self.relocate_agents();
        self.session.player.vel = match arrival {
            Arrival::Gateway { vel, .. } => vel,
            Arrival::Portal(trip) | Arrival::PortalAt(_, trip) => trip.vel,
            _ => DVec3::ZERO,
        };
        self.session.player.flying = self.session.player.flying && self.session.mode.can_fly();
        self.session.vitals.reset_fall();
        self.arrival = None;
        let sound = Sound::Place(Material::Glass);
        // Java's portal travel and trigger sounds are local `AMBIENT`.
        self.audio.play_in(SoundSource::Ambient, sound, None, 0.8, (0.5, 0.6));
        false
    }

    /// A cell of `block` the player overlaps.
    fn touched(&self, block: Block) -> Option<IVec3> {
        let (min, max) = self.session.player.collision_shape().aabb(self.session.player.pos);
        let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
        (lo.y..=hi.y)
            .flat_map(|y| (lo.z..=hi.z).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec3::new(x, y, z))))
            .find(|&p| self.world.get_block(p) == Some(block))
    }

    /// Takes the player through the End gateway at `cell`: to its known
    /// exit, or out to the outer islands the first time.
    pub(super) fn enter_gateway(&mut self, cell: IVec3) {
        let Some(fight) = &self.mobs.entities.fight else { return };
        // `EndGatewayBlock.getPortalDestination`: players keep their motion
        // and facing (`Relative.DELTA`, `ROTATION`).
        let vel = self.session.player.vel;
        let arrival = match fight.gateway_exit(cell) {
            Some(exit) => Arrival::Gateway { exit, from: cell, island: None, link: false, vel },
            None => {
                let Some(found) = self.world.find_gateway_exit(cell) else { return };
                Arrival::Gateway { exit: found.exit, from: cell, island: found.island, link: true, vel }
            }
        };
        self.audio.play_in(SoundSource::Ambient, Sound::Teleport, None, 0.8, (0.7, 0.8));
        self.arrival = Some(arrival);
        self.session.player.vel = DVec3::ZERO;
        self.session.vitals.reset_fall();
    }

    /// Whether the player overlaps a `portal` block's slice from `bottom`
    /// to `top` of the way up its cell.
    fn in_portal(&self, portal: Block, (bottom, top): (f64, f64)) -> bool {
        let (min, max) = self.session.player.collision_shape().aabb(self.session.player.pos);
        let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
        (lo.y..=hi.y).any(|y| {
            min.y < y as f64 + top
                && max.y > y as f64 + bottom
                && (lo.z..=hi.z)
                    .any(|z| (lo.x..=hi.x).any(|x| self.world.get_block(IVec3::new(x, y, z)) == Some(portal)))
        })
    }

    /// Standing in a Nether portal long enough takes you to the other
    /// dimension. Falling into an End portal takes you at once: to the End's
    /// obsidian platform, or out of the End to your spawn point.
    pub(super) fn update_portal(&mut self, dt: f64) {
        if self.credits.is_some() {
            return;
        }
        self.portal_cooldown = (self.portal_cooldown - dt).max(0.0);
        if self.dimension == Dimension::End
            && !self.session.vitals.is_dead()
            && let Some(cell) = self.touched(Block::END_GATEWAY)
        {
            // `EndGatewayBlock.entityInside`: a gateway cooling down does
            // nothing; otherwise it starts its cooldown (and beam) and the
            // player enters it, or, still on their own portal cooldown,
            // only restarts that (`setAsInsidePortal`).
            let Some(fight) = &mut self.mobs.entities.fight else { return };
            if fight.gateway_entities.cooling(cell) {
                return;
            }
            fight.gateway_entities.trigger(cell);
            if self.portal_cooldown > 0.0 {
                self.portal_cooldown = PORTAL_COOLDOWN;
            } else {
                self.enter_gateway(cell);
            }
            return;
        }
        // Java's End portal block spans 6..12 sixteenths: coming up from
        // below takes you only once you reach it.
        if !self.session.vitals.is_dead() && self.in_portal(Block::END_PORTAL, (6.0 / 16.0, 0.75)) {
            // A trip refused because the save failed waits until the player
            // steps out.
            if self.portal_locked {
                return;
            }
            if self.dimension == Dimension::End && self.begin_credits() {
                return;
            }
            let (to, arrival) = match self.dimension {
                Dimension::End => (Dimension::Overworld, Arrival::Respawn),
                _ => (Dimension::End, Arrival::EndSpawn),
            };
            self.switch_dimension(to, arrival);
            return;
        }
        if self.dimension == Dimension::End
            || !self.in_portal(Block::NETHER_PORTAL, (0.0, 1.0))
            || self.session.vitals.is_dead()
        {
            self.portal_locked = false;
            self.portal_time = (self.portal_time - dt as f32 * 2.0).max(0.0);
            return;
        }
        if self.portal_locked {
            return;
        }
        let before = self.portal_time;
        self.portal_time += dt as f32;
        if before == 0.0 {
            self.audio.play_in(SoundSource::Ambient, Sound::Fuse, None, 0.4, (0.4, 0.5));
        }
        // `ServerLevel.isAllowedToEnterPortal`: only the Nether is gated.
        let allowed = self.gamerules.portal_allows_trip_from(self.dimension);
        if self.portal_time >= self.portal_needed() && allowed {
            self.travel();
        }
    }

    /// Seconds in a portal before it takes you: Java's
    /// `NetherPortalBlock.getPortalTransitionTime` ticks, from the creative
    /// delay for invulnerable players and the default delay otherwise.
    pub(super) fn portal_needed(&self) -> f32 {
        self.gamerules.portal_delay_ticks(self.session.mode.invulnerable()) as f32
            * crate::simulation::TICK_SECONDS as f32
    }

    /// Beds explode in both the Nether and the End.
    pub(super) fn bed_explodes(&mut self, pos: IVec3) -> bool {
        if self.dimension.has_sky() {
            return false;
        }
        // `straw_bed_rule` here is `destroy_on_use`: it falls apart instead.
        if super::bed::is_straw(self.world.get_block(pos)) {
            if let Some((_, head, _)) = self.straw_bed_halves(pos) {
                self.destroy_straw_bed(head);
            }
            return true;
        }
        let half = self.world.get_block(pos).unwrap_or(Block::AIR);
        self.world.set_block(pos, Block::AIR);
        self.break_bed_partner(pos, half);
        self.explode(
            pos.as_dvec3() + DVec3::splat(0.5),
            5.0,
            voxelcraft::simulation::damage::Cause::BAD_RESPAWN_POINT,
            false,
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkData;
    use crate::world::storage::LevelInfo;

    #[test]
    fn every_entity_save_key_stays_with_its_dimension() {
        let mut props = BTreeMap::new();
        crate::entity::Entities::new(1).save_props(&mut props);
        for key in props.keys() {
            assert!(DIMENSION_KEYS.contains(&key.as_str()), "{key} would be dropped on reload");
        }
    }

    /// Review fix S3: keys written only when there is something to write
    /// (dragons hatched from eggs), and the region storage keys, stay with
    /// their dimension when the overworld's are cached.
    #[test]
    fn egg_dragons_and_region_keys_stay_with_their_dimension() {
        let mut entities = crate::entity::Entities::new(1);
        entities.wild_dragons.spawn(DVec3::new(0.5, 90.0, 0.5), 0.0);
        let mut props = BTreeMap::new();
        entities.save_props(&mut props);
        assert!(props.contains_key("ender_dragons"));
        for key in [
            crate::world::REGION_KEY,
            crate::world::FLUID_TICKS_KEY,
            crate::world::GENERATION_KEY,
            "stored_index",
            "entity_ids",
        ] {
            props.insert(key.to_string(), "1".to_string());
        }
        assert_eq!(dimension_props(&props), props);
    }

    #[test]
    fn workstation_entities_stay_with_their_dimension() {
        // v0.15 Track B: banner patterns and lectern books.
        let props = [("workstations".into(), "l:1,2,3=1558:1;0".into())].into_iter().collect();
        assert_eq!(dimension_props(&props), props);
    }

    #[test]
    fn copper_statue_names_stay_with_their_dimension() {
        let props = [("copper_golem_statues".into(), "[[[10,140,10],\"Friend\"]]".into())].into_iter().collect();
        assert_eq!(dimension_props(&props), props);
    }

    #[test]
    fn each_dimension_keeps_its_own_save() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-dims-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = Storage::new(&dir);
        let props = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs.iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let chunk = |b| (IVec3::new(1, 2, 3), Arc::new(ChunkData::Uniform(b)));

        // The overworld lives in the root folder, its furnaces with the player.
        let root_props = props(&[("mode", "survival"), ("chests", "overworld chests"), ("dimension", "nether")]);
        let level = LevelInfo { seed: 9, player: None, props: root_props.clone() };
        root.save(&level, &[chunk(Block::STONE)]).unwrap();
        let nether = storage_for(&root, Dimension::Nether);
        let level = LevelInfo { seed: 9, player: None, props: props(&[("chests", "nether chests")]) };
        nether.save(&level, &[chunk(Block::NETHERRACK)]).unwrap();

        let overworld_props = dimension_props(&root_props);
        assert_eq!(overworld_props, props(&[("chests", "overworld chests")]));
        let (chunks, p, _) = load_dimension(&root, Dimension::Overworld, &overworld_props);
        assert_eq!((chunks.len(), p.get("chests").map(String::as_str)), (1, Some("overworld chests")));
        assert!(matches!(*chunks[&IVec3::new(1, 2, 3)], ChunkData::Uniform(Block::STONE)));
        let (chunks, p, _) = load_dimension(&root, Dimension::Nether, &overworld_props);
        assert_eq!(p.get("chests").map(String::as_str), Some("nether chests"));
        assert!(matches!(*chunks[&IVec3::new(1, 2, 3)], ChunkData::Uniform(Block::NETHERRACK)));

        let end = storage_for(&root, Dimension::End);
        let level = LevelInfo { seed: 9, player: None, props: props(&[("chests", "end chests")]) };
        end.save(&level, &[chunk(Block::END_STONE)]).unwrap();
        let (chunks, p, _) = load_dimension(&root, Dimension::End, &overworld_props);
        assert_eq!(p.get("chests").map(String::as_str), Some("end chests"));
        assert!(matches!(*chunks[&IVec3::new(1, 2, 3)], ChunkData::Uniform(Block::END_STONE)));
        let (_, p, notice) = load_dimension(&root, Dimension::Nether, &overworld_props);
        assert_eq!(p.get("chests").map(String::as_str), Some("nether chests"));
        assert_eq!(notice, None);

        // Rewriting just the root level leaves the overworld's chunks alone.
        root.save_level(&LevelInfo { seed: 9, player: None, props: root_props }).unwrap();
        assert_eq!(root.load_chunks().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v1.0 Track 1: damage to a dimension's files is worked around and
    /// reported, never silently replaced by an empty dimension.
    #[test]
    fn damaged_dimension_files_load_what_they_can_and_say_so() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-dims-damaged-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = Storage::new(&dir);
        let nether = storage_for(&root, Dimension::Nether);
        let props = |chests: &str| -> BTreeMap<String, String> { [("chests".into(), chests.into())].into() };
        let chunks =
            [IVec3::new(0, 2, 0), IVec3::new(1, 2, 0)].map(|p| (p, Arc::new(ChunkData::Uniform(Block::NETHERRACK))));
        nether.save(&LevelInfo { seed: 9, player: None, props: props("first") }, &chunks).unwrap();
        nether.save(&LevelInfo { seed: 9, player: None, props: props("second") }, &chunks).unwrap();
        let none = BTreeMap::new();

        // The level file cut off by a crash: the copy the last save kept is used.
        let level = std::fs::read_to_string(nether.dir().join("level.txt")).unwrap();
        std::fs::write(nether.dir().join("level.txt"), &level[..level.len() - 3]).unwrap();
        let (loaded, p, notice) = load_dimension(&root, Dimension::Nether, &none);
        assert_eq!((loaded.len(), p.get("chests").map(String::as_str)), (2, Some("first")));
        let notice = notice.expect("the player is told");
        assert!(notice.contains("last save's copy") && notice.contains("level.txt.damaged-"), "{notice}");

        // Chunks cut off partway: the intact record is kept and the damaged
        // file copied aside.
        let bin = std::fs::read(nether.dir().join("chunks.bin")).unwrap();
        std::fs::write(nether.dir().join("chunks.bin"), &bin[..bin.len() - 2]).unwrap();
        std::fs::write(nether.dir().join("level.txt"), &level).unwrap();
        let (loaded, p, notice) = load_dimension(&root, Dimension::Nether, &none);
        assert_eq!((loaded.len(), p.get("chests").map(String::as_str)), (1, Some("second")));
        let notice = notice.expect("the player is told");
        assert!(notice.contains("1 of 2") && notice.contains("chunks.bin.damaged-"), "{notice}");

        // Both level files lost, only the chunks left: the chunks still load.
        std::fs::remove_file(nether.dir().join("level.txt")).unwrap();
        std::fs::remove_file(nether.dir().join("level.txt_old")).unwrap();
        std::fs::write(nether.dir().join("chunks.bin"), &bin).unwrap();
        let (loaded, p, notice) = load_dimension(&root, Dimension::Nether, &none);
        assert_eq!((loaded.len(), p.len()), (2, 0));
        assert!(notice.unwrap().contains("containers and entities are lost"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v1.0 Track 3b: a dimension's region files attach to its world; a
    /// cut-off one is reported to the player, and a pre-region world whose
    /// unsaved session left region files beside its `chunks.bin` gets them
    /// moved aside; scripted runs read regions without writing or copying.
    #[test]
    fn region_files_attach_and_damage_is_reported() {
        use crate::world::region::{ChunkRecord, Patch, REGION_DIR, RegionStore};
        let dir = std::env::temp_dir().join(format!("voxelcraft-dims-regions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = Storage::new(&dir);
        let nether = storage_for(&root, Dimension::Nether);
        let regions = nether.dir().join(REGION_DIR);
        {
            let (store, _) = RegionStore::open(&regions, true).unwrap();
            let record = |b| ChunkRecord { blocks: Some(Arc::new(ChunkData::Uniform(b))), ..Default::default() };
            store.queue([
                (IVec3::new(0, 2, 0), Patch::replace(record(Block::NETHERRACK))),
                (IVec3::new(1, 2, 0), Patch::replace(record(Block::GLOWSTONE))),
            ]);
            store.flush().unwrap();
        }
        let world = || World::new_headless(Arc::new(Generator::new(9)), Default::default(), 1);
        let owned: BTreeMap<String, String> = [(crate::world::REGION_KEY.to_string(), String::new())].into();
        let mut w = world();
        assert_eq!(attach_regions(&mut w, &root, Dimension::Nether, &owned, true), None);
        assert!(w.writes_records());
        assert_eq!(w.section_data(IVec3::new(1, 2, 0)).and_then(|d| d.uniform()), Some(Block::GLOWSTONE));
        drop(w);

        // Cut off: the intact chunk reads, the player hears about the other.
        let file = regions.join("r.0.0.vxr");
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() - 4096 + 10]).unwrap();
        let mut w = world();
        let notice = attach_regions(&mut w, &root, Dimension::Nether, &owned, false).expect("the player is told");
        assert!(notice.contains("nether") && notice.contains("1 of 2"), "{notice}");
        assert!(!w.writes_records(), "a scripted run only reads");
        assert_eq!(std::fs::read_dir(&regions).unwrap().count(), 1, "nor copies anything aside");
        assert_eq!(w.section_data(IVec3::new(0, 2, 0)).and_then(|d| d.uniform()), Some(Block::NETHERRACK));
        drop(w);
        let mut w = world();
        let notice = attach_regions(&mut w, &root, Dimension::Nether, &owned, true).unwrap();
        assert!(notice.contains("r.0.0.vxr.damaged-"), "{notice}");
        drop(w);

        // A pre-region level file with `chunks.bin` beside region files.
        nether.save(&LevelInfo { seed: 9, player: None, props: BTreeMap::new() }, &[]).unwrap();
        let mut w = world();
        attach_regions(&mut w, &root, Dimension::Nether, &BTreeMap::new(), true);
        drop(w);
        let aside: Vec<String> = std::fs::read_dir(nether.dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("region.unsaved-"))
            .collect();
        assert_eq!(aside.len(), 1, "the unsaved session's regions moved aside");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
