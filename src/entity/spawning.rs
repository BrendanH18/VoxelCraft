//! Natural population and cage activation; independent of rendering and input.
use super::*;
impl Entities {
    pub(super) fn natural_spawn<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) {
        self.spawn_categories(world, ctx, difficulty);
    }

    /// `ServerChunkCache.tickChunks`: fresh counts and one category filter
    /// per tick, then chunk order followed by category order.
    fn spawn_categories<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) {
        use category::MobCategory;
        if !world.light_settled() {
            return;
        }
        let players: Vec<_> = ctx.players.iter().filter(|t| !t.spectator).map(|t| t.pos).collect();
        if players.is_empty() {
            return;
        }
        let mut state = category::SpawnState::new(&players, &self.mobs, |c| {
            world.world_surface(c.x * 16 + 8, c.y * 16 + 8).is_some()
        });
        let enabled = MobCategory::SPAWNING.map(|c| {
            // Bats are the whole ambient list and aren't in VoxelCraft yet; the
            // pass would only spend biome lookups (see `spawn_lists` tests).
            c != MobCategory::Ambient
                && (c.friendly() || difficulty != crate::simulation::difficulty::Difficulty::Peaceful)
                && (!c.persistent() || self.spawn_ticks.is_multiple_of(400))
                && state.global_room(c)
        });
        if !enabled.iter().any(|&e| e) {
            return;
        }
        let mut structures = Vec::new();
        for p in &players {
            for b in world.structures_near(p.floor().as_ivec3(), 160) {
                if !structures
                    .iter()
                    .any(|other: &std::sync::Arc<crate::world::temples::Built>| other.bounds.min == b.bounds.min)
                {
                    structures.push(b);
                }
            }
        }
        let rules =
            monster_spawns::SpawnRules { dimension: ctx.dimension, light: ctx.spawn_light, moon: self.moon_brightness };
        // `isNaturalSpawningAllowed`: spawn attempts need an entity-ticking chunk,
        // although every chunk within eight still counts toward the global cap.
        let mut order = std::mem::take(&mut state.chunks);
        order.retain(|c| world.entity_ticking(IVec3::new(c.x * 16 + 8, 0, c.y * 16 + 8)));
        for i in (1..order.len()).rev() {
            order.swap(i, self.rng.next_int(i as u32 + 1) as usize);
        }
        for c in order {
            for (category, enabled) in MobCategory::SPAWNING.into_iter().zip(enabled) {
                if !enabled || !state.local_room(category, c) {
                    continue;
                }
                let x = c.x * 16 + self.rng.next_int(16) as i32;
                let z = c.y * 16 + self.rng.next_int(16) as i32;
                let Some(top) = world.world_surface(x, z) else { continue };
                let min = ctx.dimension.min_y();
                if top < min {
                    continue;
                }
                let y = min + self.rng.next_int((top + 2 - min) as u32) as i32;
                if y <= min {
                    continue;
                }
                self.category_packs(world, ctx, IVec3::new(x, y, z), rules, &mut state, category, &structures);
            }
        }
    }

    /// Generation is asynchronous and uses 32-block voxel chunks. Consume
    /// each 16-block column when it first becomes fully loaded, with a saved
    /// marker so unload/reload and cleared herds cannot regenerate animals.
    pub(super) fn populate_generation<W: MobWorld + ?Sized>(&mut self, world: &W) {
        let mut ready = Vec::new();
        world.generation_columns(&mut |c| {
            if !self.generation_populated.contains(&c) {
                ready.push(c);
            }
        });
        ready.sort_unstable_by_key(|c| (c.y, c.x));
        for c in ready {
            self.generation_populated.insert(c);
            self.spawn_mobs_for_chunk_generation(world, c);
        }
    }

    /// `NaturalSpawner.spawnMobsForChunkGeneration`: uncapped creature
    /// herds, weighted by the source biome, up to four attempts per member.
    pub fn spawn_mobs_for_chunk_generation<W: MobWorld + ?Sized>(&mut self, world: &W, chunk: glam::IVec2) {
        use category::MobCategory;
        let origin = chunk * 16;
        let biome = world.biome(origin.x, origin.y);
        let list = spawn_lists::biome_list(biome, MobCategory::Creature);
        if list.is_empty() {
            return;
        }
        // Java seeds each chunk's herds from the world seed (`setDecorationSeed`),
        // so a column's herd doesn't depend on when or in what order it loaded.
        let seed = world.seed()
            ^ (chunk.x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (chunk.y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
        let shared = std::mem::replace(&mut self.rng, Rng::new(seed));
        self.generation_herds(world, chunk, biome, list);
        self.rng = shared;
    }

    fn generation_herds<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        chunk: glam::IVec2,
        biome: crate::world::terrain::Biome,
        list: &'static [monster_spawns::MonsterSpawn],
    ) {
        let origin = chunk * 16;
        let rules = monster_spawns::SpawnRules {
            dimension: Dimension::Overworld,
            light: crate::simulation::lighting::SpawnLight::DAY,
            moon: self.moon_brightness,
        };
        while self.rng.next_f32() < spawn_lists::generation_probability(biome) {
            let Some(entry) = monster_spawns::pick(list, &mut self.rng) else { break };
            let count = entry.min + self.rng.next_int(entry.max - entry.min + 1);
            let (mut x, mut z) = (origin.x + self.rng.next_int(16) as i32, origin.y + self.rng.next_int(16) as i32);
            let (start_x, start_z) = (x, z);
            let Some(kind) = entry.mob.kind() else { continue };
            let mut group = None;
            for _ in 0..count {
                for _ in 0..4 {
                    // MOTION_BLOCKING_NO_LEAVES: leaves don't become the herd's ground.
                    let Some(mut top) = world.world_surface(x, z) else { break };
                    while top > Dimension::Overworld.min_y()
                        && world
                            .block(IVec3::new(x, top, z))
                            .is_some_and(|b| b.is_leaves() || (!b.is_solid() && !b.is_fluid()))
                    {
                        top -= 1;
                    }
                    let cell = IVec3::new(x, top + 1, z);
                    let width = kind.shape().half_width * 2.0;
                    let pos = DVec3::new(
                        (x as f64).clamp(origin.x as f64 + width, (origin.x + 16) as f64 - width),
                        cell.y as f64,
                        (z as f64).clamp(origin.y as f64 + width, (origin.y + 16) as f64 - width),
                    );
                    let ok = spawn_rules::spot_ok(world, kind, pos.floor().as_ivec3(), rules, &mut self.rng)
                        && !physics::overlaps_solid(world, pos, kind.shape());
                    if ok {
                        group = self.spawn_natural(kind, pos, biome, group);
                    }
                    x += self.rng.next_int(5) as i32 - self.rng.next_int(5) as i32;
                    z += self.rng.next_int(5) as i32 - self.rng.next_int(5) as i32;
                    while x < origin.x || x >= origin.x + 16 || z < origin.y || z >= origin.y + 16 {
                        x = start_x + self.rng.next_int(5) as i32 - self.rng.next_int(5) as i32;
                        z = start_z + self.rng.next_int(5) as i32 - self.rng.next_int(5) as i32;
                    }
                    if ok {
                        break;
                    }
                }
            }
        }
    }

    /// Java's spawner logic: a spawner with a player within 16 blocks waits
    /// out its delay, then tries four spots up to 4 blocks away (and a block
    /// up or down) with room for its mob, unless six are already around.
    /// Light and ground don't matter. Nearby cages give off smoke and flame
    /// on the existing visual roll (about six times a second) so this tick
    /// does not change the spawner RNG stream.
    pub(super) fn run_spawners<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx) {
        let spawners = world.spawners();
        self.spawner_delays.retain(|p, _| spawners.iter().any(|(q, _)| q == p));
        for (cell, kind) in spawners {
            let centre = cell.as_dvec3() + DVec3::splat(0.5);
            if !ctx.players.iter().any(|t| t.alive && t.pos.distance_squared(centre) < SPAWNER_RANGE * SPAWNER_RANGE) {
                continue;
            }
            if self.rng.chance(dt * 6.0) {
                let p = centre + DVec3::new(self.rng.range(-0.4, 0.4) as f64, self.rng.range(-0.4, 0.4) as f64, 0.0);
                for kind in [crate::particles::Kind::Smoke, crate::particles::Kind::Flame] {
                    self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(kind, p, 1)));
                }
            }
            let delay = self.spawner_delays.entry(cell).or_insert(1.0);
            *delay -= dt;
            if *delay > 0.0 {
                continue;
            }
            *delay = 0.0;
            let reach = SPAWNER_REACH + 0.5;
            let crowd = self
                .mobs
                .iter()
                .filter(|m| m.kind == kind && m.alive() && (m.pos - centre).abs().cmple(DVec3::splat(reach)).all())
                .count();
            let mut spawned = crowd >= SPAWNER_CROWD;
            for _ in 0..SPAWNER_TRIES.min(SPAWNER_CROWD.saturating_sub(crowd)) {
                let mut r = || (self.rng.next_f32() - self.rng.next_f32()) as f64 * SPAWNER_REACH;
                let (dx, dz) = (r(), r());
                let dy = (self.rng.next_f32() * 3.0).floor() as f64 - 1.0;
                let pos = DVec3::new(centre.x + dx, cell.y as f64 + dy, centre.z + dz);
                let shape = kind.shape();
                if !world.loaded(pos.floor().as_ivec3())
                    || physics::overlaps_solid(world, pos, shape)
                    || physics::is_fluid_at(world, pos)
                {
                    continue;
                }
                self.spawn(kind, pos);
                spawned = true;
            }
            if spawned {
                let next = self.rng.range(SPAWNER_DELAY.0, SPAWNER_DELAY.1);
                self.spawner_delays.insert(cell, next);
            }
        }
    }
}

/// StructureSpawnOverride / ChunkGenerator.getMobsAt. The structures' actual
/// VoxelCraft bounds stand in for Java piece/structure bounds (see evidence).
pub(super) fn spawn_list<W: MobWorld + ?Sized>(
    world: &W,
    category: category::MobCategory,
    dimension: Dimension,
    cell: IVec3,
    structures: &[std::sync::Arc<crate::world::temples::Built>],
) -> &'static [monster_spawns::MonsterSpawn] {
    use crate::world::temples::Kind;
    use category::MobCategory as C;
    use monster_spawns::{JavaMonster as J, e};
    const WITCH: [monster_spawns::MonsterSpawn; 1] = [e(J::Witch, 1, 1, 1)];
    const CAT: [monster_spawns::MonsterSpawn; 1] = [e(J::Known(MobKind::Cat), 1, 1, 1)];
    const GUARDIAN: [monster_spawns::MonsterSpawn; 1] = [e(J::Known(MobKind::Guardian), 1, 2, 4)];
    const PILLAGER: [monster_spawns::MonsterSpawn; 1] = [e(J::Known(MobKind::Pillager), 1, 1, 1)];
    const FORTRESS: [monster_spawns::MonsterSpawn; 5] = [
        e(J::Known(MobKind::Blaze), 10, 2, 3),
        e(J::Known(MobKind::ZombifiedPiglin), 5, 4, 4),
        e(J::Known(MobKind::WitherSkeleton), 8, 5, 5),
        e(J::Skeleton, 2, 5, 5),
        e(J::Known(MobKind::MagmaCube), 3, 4, 4),
    ];
    if dimension == Dimension::Nether {
        if category == C::Monster && world.in_fortress(cell) {
            return &FORTRESS;
        }
        return spawn_lists::dimension_list(
            world.nether_biome_at(cell).unwrap_or(crate::world::nether_biome::NetherBiome::NetherWastes),
            category,
        );
    }
    if dimension == Dimension::End {
        return spawn_lists::end_list(category);
    }
    for b in structures {
        if !b.spawns.contains(cell) {
            continue;
        }
        match (b.kind, category) {
            (Kind::SwampHut, C::Monster) => return &WITCH,
            (Kind::SwampHut, C::Creature) => return &CAT,
            (Kind::Monument, C::Monster) => return &GUARDIAN,
            (Kind::Monument, C::UndergroundWaterCreature | C::Axolotls) => return &[],
            (Kind::Outpost, C::Monster) => return &PILLAGER,
            _ => {}
        }
    }
    if category == C::Monster {
        monster_spawns::monster_list(world.cave_biome(cell))
    } else {
        spawn_lists::biome_list(world.cave_biome(cell), category)
    }
}
