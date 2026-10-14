//! World save assembly and persistence, separate from event dispatch.
use super::*;
impl Game {
    pub(super) fn save(&mut self) {
        // LAN clients do not own the world; benchmark scenes never write saves.
        if self.lan.client.is_some() || self.benchmark.is_some() {
            self.last_save = Instant::now();
            return;
        }
        let mut props = std::collections::BTreeMap::new();
        props.insert("agents".into(), self.serialize_lan_profiles());
        props.insert("pads".into(), self.pads.serialize());
        props.insert("mode".to_string(), self.session.mode.name().to_string());
        props.insert("difficulty".to_string(), self.difficulty.name().to_string());
        props.insert("hardcore".to_string(), self.hardcore.to_string());
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
        let h = self.session.vitals.hunger;
        props.insert("hunger".to_string(), format!("{:.2},{:.2},{:.3}", h.food, h.saturation, h.exhaustion));
        if let Some(cause) = &self.session.vitals.death {
            props.insert("death".to_string(), cause.clone());
        }
        props.insert("dimension".to_string(), self.dimension.name().to_string());
        props.insert("time".to_string(), format!("{:.5}", self.day_time));
        props.insert("day_count".to_string(), self.day_count.to_string());
        props.insert("weather".to_string(), self.weather.serialize());
        if self.credits_seen {
            props.insert("credits_seen".to_string(), "1".to_string());
        }
        if let Some(b) = self.session.spawn_bed {
            props.insert("bed".to_string(), format!("{},{},{}", b.x, b.y, b.z));
        }
        if let Some(p) = self.session.spawn_point {
            props.insert("spawn_point".to_string(), format!("{},{},{}", p.x, p.y, p.z));
        }
        let seed = self.world.generator.seed;
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
            Ok(()) => log::info!(
                "saved {} modified {} chunks to {}",
                chunks.len(),
                self.dimension.name(),
                self.storage.dir().display()
            ),
            Err(e) => log::error!("save failed: {e}"),
        }
        self.last_save = Instant::now();
    }
}
