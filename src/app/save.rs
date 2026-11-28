//! World save assembly and persistence, separate from event dispatch.
use super::*;
use crate::world::region::REGION_DIR;
/// v1.0 gap Track 8f: a player's ender pearls in their player data (Java's
/// `ender_pearls`).
pub(super) const PEARLS_KEY: &str = "ender_pearls";

impl Game {
    /// Saves the world and waits until it is on disk: quitting, changing
    /// dimension, `/save` and the LAN host's save all finish here before
    /// the world closes.
    pub(super) fn save(&mut self) {
        self.save_world(true);
    }

    /// v1.0 Track 3b: the periodic save. With region storage, the chunk
    /// records and level files are written by the I/O thread; write errors
    /// reach the player from [`Game::poll_region_storage`].
    pub(super) fn autosave(&mut self) {
        self.save_world(false);
    }

    /// Reports what the region I/O thread found since the last frame: write
    /// errors as a failed save, unreadable chunks as load damage.
    pub(super) fn poll_region_storage(&mut self) {
        let errors = self.world.take_region_errors();
        if !errors.is_empty() {
            let e = errors.join("; ");
            log::error!("save failed: {e}");
            self.show_popup(&format!("Could not save the world: {e}"));
            self.save_error = Some(e);
        }
        let notices = self.world.take_region_notices();
        if !notices.is_empty() {
            self.report_load_damage(&notices.join(" "));
        }
    }

    fn save_world(&mut self, wait: bool) {
        // LAN clients do not own the world; benchmark scenes never write saves.
        if self.lan.client.is_some() || self.benchmark.is_some() {
            self.last_save = Instant::now();
            return;
        }
        self.settle_crystal_hits();
        let mut props = std::collections::BTreeMap::new();
        props.insert("loot_sequences".into(), self.world.loot_sequences.serialize());
        props.insert("maps".into(), self.world.maps.serialize());
        props.insert("agents".into(), self.serialize_lan_profiles());
        props.insert("pads".into(), self.pads.serialize());
        props.insert("mode".to_string(), self.session.mode.name().to_string());
        props.insert("difficulty".to_string(), self.difficulty.name().to_string());
        props.insert("hardcore".to_string(), self.hardcore.to_string());
        props.insert("difficulty_locked".to_string(), self.difficulty_locked.to_string());
        props.insert("gamerules".to_string(), self.gamerules.serialize());
        props.insert("lan_rules".to_string(), self.lan.rules_text());
        props.insert(
            "world_spawn".to_string(),
            format!("{},{},{}", self.world_spawn.x, self.world_spawn.y, self.world_spawn.z),
        );
        props.insert("name".to_string(), self.world_name.clone());
        props.insert("terrain".to_string(), self.terrain_version.to_string());
        // Save what's held or on the crafting grid as if the screen closed.
        let mut inventory = self.session.inventory.clone();
        inventory.return_stacks(self.craft.cells.iter().chain(&self.session.work).flatten().copied());
        props.insert("inventory".to_string(), inventory.serialize());
        props.insert("health".to_string(), self.session.vitals.health.to_string());
        props.insert("air".to_string(), format!("{:.2}", self.session.vitals.air));
        props.insert("xp".to_string(), self.session.vitals.xp.serialize());
        props.insert("effects".to_string(), self.session.vitals.effects.serialize());
        props.insert("time_since_rest".into(), self.session.vitals.time_since_rest.to_string());
        props.insert("ticks_frozen".to_string(), self.session.vitals.ticks_frozen.to_string());
        props.insert("warden_spawn_tracker".into(), self.session.vitals.warden.save().to_string());
        let h = self.session.vitals.hunger;
        props.insert("hunger".to_string(), format!("{:.2},{:.2},{:.3}", h.food, h.saturation, h.exhaustion));
        if let Some(cause) = &self.session.vitals.death {
            props.insert("death".to_string(), cause.clone());
        }
        // v1.0 gap Track 8f: the host's pearls (`ServerPlayer.saveEnderPearls`).
        let host = crate::entity::PlayerId::HOST;
        if let Some(pearls) = self.owned_pearls.save(&self.mobs.entities, self.dimension, host) {
            props.insert(PEARLS_KEY.to_string(), pearls.to_string());
        }
        props.insert("dimension".to_string(), self.dimension.name().to_string());
        props.insert("time".to_string(), format!("{:.5}", self.day_time));
        props.insert("day_count".to_string(), self.day_count.to_string());
        props.insert("weather".to_string(), self.weather.serialize());
        if self.credits_seen {
            props.insert("credits_seen".to_string(), "1".to_string());
        }
        if !self.owed_portals.is_empty() {
            props.insert(dimension::OWED_PORTALS_KEY.to_string(), dimension::owed_portals_text(&self.owed_portals));
        }
        if let Some(b) = self.session.spawn_bed {
            props.insert("bed".to_string(), format!("{},{},{}", b.x, b.y, b.z));
        }
        props.insert("ender_chest".to_string(), self.ender_for_save().serialize());
        if let Some(a) = self.session.spawn_anchor {
            props.insert("spawn_anchor".to_string(), format!("{},{},{}", a.x, a.y, a.z));
        }
        if let Some(text) = self.session.last_death_to_string() {
            props.insert("last_death".to_string(), text);
        }
        if let Some(text) = self.session.raid_omen_to_string() {
            props.insert("raid_omen_position".to_string(), text);
        }
        props.insert("stats".to_string(), self.session.stats.serialize());
        props.insert("recipe_book".to_string(), self.session.recipe_book.serialize());
        if let Some(p) = self.session.spawn_point {
            props.insert("spawn_point".to_string(), format!("{},{},{}", p.x, p.y, p.z));
        }
        let seed = self.world.generator.seed;
        record_generation(&mut props, self.world.generator.generation);
        if self.world.writes_records() {
            self.save_regions(seed, props, wait);
            self.last_save = Instant::now();
            return;
        }
        // A dimension with region files keeps its chunks there. If they
        // couldn't be opened, saving without them would write a `chunks.bin`
        // and a level file without `region_chunks`, and the next launch
        // would move the region files aside as unsaved: refuse instead.
        let regions = dimension::storage_for(&self.storage, self.dimension).dir().join(REGION_DIR);
        if may_hold_regions(&regions) {
            let e = "this dimension's region files couldn't be opened, so nothing was written over them";
            log::error!("save failed: {e} ({})", regions.display());
            self.show_popup(&format!("Could not save the world: {e}"));
            self.save_error = Some(e.to_string());
            self.last_save = Instant::now();
            return;
        }
        let chunks = self.world.modified_chunks();
        // The overworld saves with the player; the Nether in its own folder.
        let result = match self.dimension {
            Dimension::Overworld => {
                props.extend(self.dimension_props());
                let player = Some((self.session.player.pos, self.session.player.yaw, self.session.player.pitch));
                self.storage.save(&LevelInfo { seed, player, props }, &chunks)
            }
            Dimension::Nether | Dimension::End => {
                let nether = LevelInfo { seed, player: None, props: self.dimension_props() };
                dimension::storage_for(&self.storage, self.dimension).save(&nether, &chunks).and_then(|()| {
                    props.extend(self.overworld_props.clone());
                    let player = Some((self.session.player.pos, self.session.player.yaw, self.session.player.pitch));
                    self.storage.save_level(&LevelInfo { seed, player, props })
                })
            }
        };
        match result {
            Ok(()) => {
                self.save_error = None;
                log::info!(
                    "saved {} modified {} chunks to {}",
                    chunks.len(),
                    self.dimension.name(),
                    self.storage.dir().display()
                )
            }
            Err(e) => {
                log::error!("save failed: {e}");
                self.show_popup(&format!("Could not save the world: {e}"));
                self.save_error = Some(e.to_string());
            }
        }
        self.last_save = Instant::now();
    }
}

impl Game {
    /// v1.0 Track 3b: a save with region storage. Entities stored in
    /// unloaded chunks and the level file's state for chunks that aren't
    /// loaded go to their records, loaded chunks whose blocks changed write
    /// them, and the level files (the dimension's, then the player's) are
    /// written by the I/O thread once those records are on disk, followed by
    /// moving a migrated `chunks.bin` aside. `wait` blocks until all of it
    /// is written.
    fn save_regions(&mut self, seed: u64, mut props: std::collections::BTreeMap<String, String>, wait: bool) {
        let started = Instant::now();
        let stored = self.mobs.entities.take_stored_batches(&mut self.world);
        let records = self.world.save_records(stored);
        let player = Some((self.session.player.pos, self.session.player.yaw, self.session.player.pitch));
        let dimension = dimension::storage_for(&self.storage, self.dimension);
        let root = Storage::new(self.storage.dir());
        let (own, player_level) = match self.dimension {
            Dimension::Overworld => {
                props.extend(self.dimension_props());
                (LevelInfo { seed, player, props }, None)
            }
            Dimension::Nether | Dimension::End => {
                let own = LevelInfo { seed, player: None, props: self.dimension_props() };
                props.extend(self.overworld_props.clone());
                (own, Some(LevelInfo { seed, player, props }))
            }
        };
        let Some(store) = self.world.regions() else { return };
        store.after_writes(move || {
            dimension.save_level(&own)?;
            if let Some(level) = player_level {
                root.save_level(&level)?;
            }
            dimension.retire_chunks_file()
        });
        let result = if wait { self.world.flush_regions() } else { Ok(()) };
        match result {
            Ok(()) => {
                if wait {
                    self.save_error = None;
                }
                log::info!(
                    "saved {records} {} chunk records to {} ({:.1} ms on the main thread{})",
                    self.dimension.name(),
                    self.storage.dir().display(),
                    started.elapsed().as_secs_f64() * 1000.0,
                    if wait { ", written" } else { "; writing" }
                );
            }
            Err(e) => {
                log::error!("save failed: {e}");
                self.show_popup(&format!("Could not save the world: {e}"));
                self.save_error = Some(e.to_string());
            }
        }
    }
}

/// Records the world's generator in its `level.txt` properties, for
/// `startup::world_generation` to read back.
pub(super) fn record_generation(props: &mut std::collections::BTreeMap<String, String>, generation: Generation) {
    props.insert(Generation::KEY.to_string(), generation.number().to_string());
}

/// Whether `regions` (a dimension's region folder) is there, or can't be
/// checked: either way the legacy `chunks.bin` save must not run.
fn may_hold_regions(regions: &std::path::Path) -> bool {
    !matches!(regions.try_exists(), Ok(false))
}

impl Game {
    /// v1.0 gap Track 8f: the pearls saved with the host's and each
    /// profile's player data, waiting until their owner is in their level.
    pub(super) fn load_owned_pearls(&mut self, root: &std::collections::BTreeMap<String, String>) {
        if let Some(v) = root.get(PEARLS_KEY).and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()) {
            self.owned_pearls.load(&v, crate::entity::PlayerId::HOST);
        }
        let Some(serde_json::Value::Array(profiles)) =
            root.get("agents").and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        else {
            return;
        };
        for p in &profiles {
            if let Some(bot) = p["name"].as_str().and_then(|n| self.agents.players.get(n)) {
                self.owned_pearls.load(&p[PEARLS_KEY], bot.id);
            }
        }
    }

    /// v1.0 gap Track 8f: pearls are in the level only while their owner
    /// is (the host, or an active profile).
    pub(super) fn sync_owned_pearls(&mut self) {
        if self.lan.client.is_some() {
            return;
        }
        let agents = &self.agents.players;
        let present = |id: crate::entity::PlayerId| {
            id == crate::entity::PlayerId::HOST || agents.values().any(|b| b.active && b.id == id)
        };
        self.owned_pearls.sync(&mut self.mobs.entities, self.dimension, present);
    }
}

#[cfg(test)]
mod tests {
    /// Review fix S4, round 2: a region folder that can't be checked (here a
    /// symlink loop, which `Path::exists` reports as missing) counts as one
    /// that is there.
    #[cfg(unix)]
    #[test]
    fn a_region_folder_that_cant_be_checked_refuses_the_legacy_save() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-save-regions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let regions = dir.join("region");
        assert!(!super::may_hold_regions(&regions), "no folder: the legacy save may run");
        std::os::unix::fs::symlink(&regions, &regions).unwrap();
        assert!(!regions.exists());
        assert!(super::may_hold_regions(&regions));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

