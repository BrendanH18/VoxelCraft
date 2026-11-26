//! v1.0 Track 3: the sizes of the entity simulation's lists, queues and
//! retained sets, read by the long-session soak (`crate::soak`).

use super::{Entities, Mob};
use crate::world::diagnostics::Counts;

impl Entities {
    /// Every entity list, queue and retained set, by name (entry counts).
    pub fn diagnostics(&self) -> Counts {
        let mut out: Counts = Vec::with_capacity(80);
        out.extend([
            ("entities.mobs", self.mobs.len()),
            ("entities.items", self.items.len()),
            ("entities.orbs", self.orbs.len()),
            ("entities.arrows", self.arrows.len()),
            ("entities.projectiles", {
                self.pearls.len()
                    + self.potions.len()
                    + self.clouds.len()
                    + self.thrown.len()
                    + self.tridents.len()
                    + self.bobbers.len()
                    + self.eyes.len()
                    + self.fireballs.len()
                    + self.fireworks.len()
                    + self.wither_skulls.len()
                    + self.shulker_bullets.len()
                    + self.fangs.len()
            }),
            ("entities.firework_twinkles", self.firework_twinkles.len()),
            ("entities.charged_blasts", self.charged_blasts.len()),
            ("entities.item_spawners", self.item_spawners.len()),
            ("entities.puffs", self.puffs.len()),
            ("entities.lightning", self.lightning.len()),
            ("entities.tnt", self.tnt.len()),
            ("entities.minecarts", self.minecarts.len()),
            ("entities.decor", self.decor.len()),
            ("entities.particles", self.particles.len()),
            ("entities.raids", self.raids.raids.len()),
            ("entities.villager_births", self.villager_births.len()),
            ("entities.claimed_beds", self.claimed_beds.len()),
            ("entities.claimed_jobs", self.claimed_jobs.len()),
            ("entities.claimed_meetings", self.claimed_meetings.len()),
            ("entities.pending_sounds", self.pending_sounds.len()),
            ("entities.door_watch", self.door_watch.len()),
            ("entities.trader_leaders", self.trader_leaders.len()),
            ("entities.effect_deaths", self.effect_deaths.len()),
            ("entities.spawner_delays", self.spawner_delays.len()),
            ("entities.generation_populated", self.generation_populated.len()),
            ("entities.bastions_populated", self.bastions_populated.len()),
            ("entities.structures_populated", self.structures_populated.len()),
            ("entities.nether_view", self.nether_view.len()),
            ("entities.player_spots", self.player_spots.len()),
            ("entities.hurt_by", self.hurt_by.len()),
            ("entities.casters", self.casters.len()),
            ("entities.poses", self.poses.len()),
            ("entities.mob_index", self.mob_index.soak_len()),
            ("entities.separation", self.separation.soak_len()),
            ("entities.sculk_catalysts", self.sculk.catalysts.len()),
            ("entities.sculk_deaths", self.sculk.deaths.len()),
            ("entities.sculk_events", self.sculk.events.len()),
            ("entities.mob_paths", self.mobs.iter().map(Mob::soak_path_len).sum()),
        ]);
        self.storage.soak_counts(&mut out);
        out
    }
}
