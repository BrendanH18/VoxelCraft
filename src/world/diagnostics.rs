//! v1.0 Track 3: the sizes of the world's queues, caches and retained
//! state, read by the long-session soak (`crate::soak`) to find anything
//! that grows without bound.
//!
//! Every count is an entry count (cells, chunks, events), not bytes, except
//! where the name ends in `_bytes`. Persistent state that Java keeps on disk
//! with its chunk (furnaces, chests, modified chunks) is listed too: it
//! grows with the part of the world a player has changed, which is by
//! design, but the soak reports it so the rate is known.

use super::World;

/// Named entry counts, in a fixed order.
pub type Counts = Vec<(&'static str, usize)>;

impl World {
    /// Every queue, cache and retained collection the world holds, by name.
    pub fn diagnostics(&self) -> Counts {
        let mut out: Counts = Vec::with_capacity(160);
        let saved_bytes: usize = self.saved.values().map(|d| d.heap_bytes()).sum();
        let loaded_bytes: usize = self.chunks.values().map(|s| s.data.heap_bytes()).sum();
        out.extend([
            ("chunks.loaded", self.chunks.len()),
            ("chunks.loaded_modified", self.chunks.values().filter(|s| s.modified).count()),
            ("chunks.loaded_bytes", loaded_bytes),
            ("chunks.saved_unloaded", self.saved.len()),
            ("chunks.saved_unloaded_bytes", saved_bytes),
            ("chunks.columns", self.columns.len()),
            ("stream.gen_in_flight", self.gen_in_flight.len()),
            ("stream.mesh_in_flight", self.mesh_in_flight),
            ("stream.dirty", self.dirty.len()),
            ("stream.sync_meshes", self.sync_meshes.len()),
            ("stream.load_list", self.load_list.len()),
            ("stream.light_mesh_changed", self.light_mesh_changed.len()),
            ("stream.mesh_uploads", self.mesh_uploads.len()),
            ("stream.mesh_removals", self.mesh_removals.len()),
            ("entity_loads", self.entity_loads.len()),
        ]);
        // v1.0 Track 3b: region storage.
        let (pending, written, bytes) = self.regions.as_ref().map_or((0, 0, 0), |s| s.stats());
        out.extend([
            ("regions.pending", pending),
            ("regions.written", written as usize),
            ("regions.written_bytes", bytes as usize),
            ("regions.evicted", self.region_records.evicted as usize),
            ("regions.level_owned", self.region_records.level_owned_len()),
            ("regions.entity_records", self.region_records.entity_batches()),
        ]);
        self.light_updates.soak_counts(&["light.block_queue", "light.block_flood_raw"], &mut out);
        self.sky_updates.soak_counts(&["light.sky_queue", "light.sky_flood_raw"], &mut out);
        self.fluids.soak_counts(&mut out);
        self.fire.soak_counts(&mut out);
        self.lightning.soak_counts(&mut out);
        self.redstone.soak_counts(&mut out);
        self.automation.soak_counts(&mut out);
        self.pistons.soak_counts(&mut out);
        self.sculk.soak_counts(&mut out);
        out.extend([
            ("falling", self.falling.len()),
            ("falling.stored", self.stored_falling.values().map(Vec::len).sum()),
            ("block_pushes", self.block_pushes.len()),
            ("leaf_decay", self.leaf_decay.len()),
            ("furnaces", self.furnaces.len()),
            ("chests", self.chests.len()),
            ("chest_positions", self.chest_positions.len()),
            ("copper_golem_names", self.copper_golem_names.len()),
            ("brewing_stands", self.brewing_stands.len()),
            ("creaking_active", self.creaking_active.len()),
            ("creaking_hearts", self.creaking_hearts.len()),
            ("spawners", self.spawners.len()),
            ("village_pois", self.village_pois.len()),
            ("village_homes", self.village_homes.len()),
            ("village_residents", self.village_residents.len()),
            ("banners", self.banners.len()),
            ("straw_sleepers", self.straw_sleepers.len()),
            // Events the game drains each tick or frame.
            ("events.drops", self.drops.len()),
            ("events.xp_drops", self.xp_drops.len()),
            ("events.brews_done", self.brews_done.len()),
            ("events.primed_tnt", self.primed_tnt.len()),
            ("events.released_silverfish", self.released_silverfish.len()),
            ("events.notes", self.notes.len()),
            ("events.allay_notes", self.allay_notes.len()),
            ("events.trip_sounds", self.trip_sounds.len()),
            ("events.golem_heads", self.golem_heads.len()),
            ("events.wither_skulls", self.wither_skulls.len()),
            ("events.beacon", self.beacon_events.len()),
            ("events.conduit", self.conduit_events.len()),
            ("events.potent", self.potent_events.len()),
            ("events.potent_settle", self.potent_settle.len()),
            ("events.hatched_turtles", self.hatched_turtles.len()),
            ("events.hatched_tadpoles", self.hatched_tadpoles.len()),
            ("events.sniffer_eggs", self.sniffer_egg_events.len()),
            ("events.dried_ghasts", self.dried_ghast_events.len()),
            ("events.chorus", self.chorus_events.len()),
            ("events.page_turns", self.page_turns.len()),
            ("events.jukebox", self.jukebox_events.len()),
            ("events.records", self.records.len()),
            ("events.records_started", self.records_started.len()),
            ("events.bell_rings", self.bell_rings.len()),
            ("events.particles", self.particles.len()),
        ]);
        out
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::{IVec2, IVec3};

    use super::super::World;
    use super::super::block::Block;
    use super::super::terrain::Generator;

    fn count(world: &World, name: &str) -> usize {
        world.diagnostics().into_iter().find(|(n, _)| *n == name).map(|(_, v)| v).expect(name)
    }

    /// Nine flat columns around the origin, ground at y 60.
    fn world() -> World {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        for x in -1..=1 {
            for z in -1..=1 {
                world.load_test_column(IVec2::new(x, z), 60);
            }
        }
        world
    }

    #[test]
    fn an_unloading_chunk_takes_its_fluid_cells_with_it() {
        let mut world = world();
        let spring = IVec3::new(16, 70, 16);
        world.set_block(spring, Block::WATER);
        let pending = count(&world, "fluid.pending");
        let bubbles = count(&world, "fluid.bubbles");
        assert!(pending >= 7, "{pending}");
        // The test columns count as modified, so their cells wait for them.
        world.unload_test_column(IVec2::ZERO);
        assert_eq!((count(&world, "fluid.pending"), count(&world, "fluid.bubbles")), (0, 0));
        assert_eq!(count(&world, "fluid.suspended"), pending + bubbles);
        // Nothing is scheduled where no chunk is loaded (`LevelTicks.schedule`).
        world.wake_fluids(spring);
        assert_eq!(count(&world, "fluid.pending"), 0);
        world.load_test_column(IVec2::ZERO, 60);
        assert_eq!((count(&world, "fluid.pending"), count(&world, "fluid.bubbles")), (pending, bubbles));
        assert_eq!(count(&world, "fluid.suspended"), 0);
    }

    #[test]
    fn an_unloading_chunk_takes_its_scheduled_fires_with_it() {
        let mut world = world();
        let fire = IVec3::new(40, 61, 40);
        world.set_block(fire - IVec3::Y, Block::NETHERRACK);
        world.set_block(fire, Block::FIRE);
        assert_eq!((count(&world, "fire.pending"), count(&world, "fire.scheduled")), (1, 1));
        world.unload_test_column(IVec2::ONE);
        assert_eq!(count(&world, "fire.pending"), 0);
        assert_eq!(count(&world, "fire.columns"), 0, "no container is left to peek at every tick");
        // Its fire comes back with the chunk.
        world.load_test_column(IVec2::ONE, 60);
        assert_eq!(world.get_block(fire), Some(Block::FIRE));
        assert_eq!(count(&world, "fire.pending"), 1);
    }
}
