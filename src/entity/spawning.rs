//! Natural population and cage activation; independent of rendering and input.
use super::*;
impl Entities {
    pub(super) fn natural_spawn<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) {
        // Overworld ground monsters follow Java's per-chunk cycle on real
        // light, caves included (see `monster_spawns`).
        let java_monsters =
            ctx.dimension == Dimension::Overworld && difficulty != crate::simulation::difficulty::Difficulty::Peaceful;
        if java_monsters {
            self.overworld_monsters(world, ctx);
        }
        for center in ctx.players.iter().map(|t| t.pos) {
            for kind in MobKind::ALL {
                let cap = kind.spawn_cap(ctx.dimension);
                // The Nether picks by its biomes' spawn lists below.
                let nether = ctx.dimension == Dimension::Nether;
                if (difficulty == crate::simulation::difficulty::Difficulty::Peaceful && kind.is_hostile())
                    || self.count_near(kind, center) >= cap
                    || (nether && nether_mob(kind).is_none())
                    || (ctx.dimension == Dimension::Overworld && monster_spawns::cycle_kind(kind))
                    || (!nether
                        && (!kind.spawns_in(ctx.dimension) || !self.rng.chance(kind.spawn_chance(ctx.dimension))))
                {
                    continue;
                }
                let angle = self.rng.range(0.0, TAU) as f64;
                let dist = self.rng.range(SPAWN_MIN_DIST as f32, SPAWN_MAX_DIST as f32) as f64;
                let x = (center.x + angle.cos() * dist).floor() as i32;
                let z = (center.z + angle.sin() * dist).floor() as i32;
                if ctx.dimension == Dimension::Overworld
                    && !self.rng.chance(kind.biome_chance(if kind == MobKind::TropicalFish && center.y < 40.0 {
                        world.cave_biome(IVec3::new(x, center.y as i32, z))
                    } else {
                        world.biome(x, z)
                    }))
                {
                    continue;
                }
                let mut group = None;
                if nether {
                    let Some((spawn, chance)) = nether_spawn(world, kind, x, z) else { continue };
                    if !self.rng.chance(chance) {
                        continue;
                    }
                    group = Some(spawn.group);
                }
                let spot = match ctx.dimension {
                    Dimension::Overworld if kind.is_aquatic() => {
                        aquatic::spawn_spot(world, kind, x, z, center.y as i32, &mut self.rng)
                    }
                    Dimension::Overworld if kind == MobKind::Drowned => self.drowned_spot(world, x, z, ctx.daylight),
                    Dimension::Overworld if kind == MobKind::Slime => {
                        self.slime_spot(world, x, z, ctx.spawn_light.sky_darken)
                    }
                    Dimension::Overworld => spawn_spot(world, kind, x, z, ctx.daylight),
                    Dimension::Nether if kind == MobKind::Strider => nether::lava_spot(world, x, z),
                    Dimension::Nether => cavern_spot(world, kind, x, z, self.rng.range(40.0, 118.0) as i32),
                    Dimension::End => cavern_spot(world, kind, x, z, self.rng.range(30.0, 90.0) as i32),
                };
                let Some(pos) = spot else { continue };
                if !in_spawn_ring(center, pos) || !clear_of_players(ctx, self.world_spawn, pos) {
                    continue;
                }
                self.spawn(kind, pos);
                self.natural_animal(world.biome(x, z));
                let herd_coat = self.natural_horse_coat(None);
                self.note_zombie_villager(kind);
                // Animals come in small herds, zombified piglins and End
                // endermen in packs.
                if !kind.is_hostile() || ctx.dimension != Dimension::Overworld {
                    // Nether groups follow their biome's spawn list (wastes ghasts and
                    // magma cubes come in fours, delta ghasts alone).
                    let extra = match group {
                        Some((lo, hi)) => lo as i32 - 1 + (self.rng.next_f32() * (hi - lo + 1) as f32) as i32,
                        None => (self.rng.next_f32() * 3.0) as i32,
                    };
                    for _ in 0..extra {
                        let (dx, dz) = ((self.rng.range(-3.0, 3.0)) as i32, (self.rng.range(-3.0, 3.0)) as i32);
                        // A group straddling a biome edge stays within its spawn list.
                        if nether && nether_spawn(world, kind, x + dx, z + dz).is_none() {
                            continue;
                        }
                        let spot = if kind.is_aquatic() {
                            aquatic::spawn_spot(world, kind, x + dx, z + dz, pos.y as i32, &mut self.rng)
                        } else if kind == MobKind::Strider {
                            nether::lava_spot(world, x + dx, z + dz)
                        } else if ctx.dimension.has_sky() {
                            spawn_spot(world, kind, x + dx, z + dz, ctx.daylight)
                        } else {
                            cavern_spot(world, kind, x + dx, z + dz, pos.y as i32 + 2)
                        };
                        if self.count_near(kind, center) < cap
                            && let Some(p) = spot
                            && clear_of_players(ctx, self.world_spawn, p)
                        {
                            self.spawn(kind, p);
                            self.natural_animal(world.biome(x + dx, z + dz));
                            self.natural_horse_coat(herd_coat);
                            self.note_zombie_villager(kind);
                        }
                    }
                }
            }
            if ctx.dimension == Dimension::Nether && difficulty != crate::simulation::difficulty::Difficulty::Peaceful {
                self.fortress_spawn(world, ctx, center);
            }
        }
    }

    /// Java's drowned rules: in water, in rivers one attempt in 15, in oceans
    /// one in 40 and only deeper than five blocks below sea level. Deep water
    /// is dark enough in daylight; shallow water needs night.
    pub(super) fn drowned_spot<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        x: i32,
        z: i32,
        daylight: f32,
    ) -> Option<DVec3> {
        use crate::world::terrain::SEA_LEVEL;
        let river = world.biome(x, z).is_river();
        if !self.rng.chance(if river { 1.0 / 15.0 } else { 1.0 / 40.0 }) {
            return None;
        }
        let top = (SEA_LEVEL - 8..=SEA_LEVEL + 1)
            .rev()
            .find(|&y| world.block(IVec3::new(x, y, z)).is_some_and(Block::holds_water))?;
        let y = if river { top } else { top - 5 - (self.rng.next_f32() * 6.0) as i32 };
        let dark = daylight < HOSTILE_SPAWN_DAYLIGHT || top - y >= 4;
        let wet = |dy: i32| world.block(IVec3::new(x, y + dy, z)).is_some_and(Block::holds_water);
        (dark && wet(0) && wet(1)).then(|| DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5))
    }

    /// Swamp slimes need Java's `getMaxLocalRawBrightness` (darkened sky or
    /// block light) at most a roll of 0..=7; slime chunks spawn below y 40.
    pub(super) fn slime_spot<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        x: i32,
        z: i32,
        sky_darken: u8,
    ) -> Option<DVec3> {
        if world.biome(x, z).is_swamp() && self.rng.chance(0.5 * self.moon_brightness) {
            let pos = spawn_spot(world, MobKind::Slime, x, z, 0.0)?;
            let y = pos.y as i32;
            let cell = pos.floor().as_ivec3();
            let sky = world.sky_light_level(cell)?;
            let light = sky.saturating_sub(sky_darken).max(world.block_light(cell));
            if y > 50 && y < 70 && light <= (self.rng.next_f32() * 8.0) as u8 {
                return Some(pos);
            }
        }
        if slime::slime_chunk(world.seed(), x.div_euclid(16), z.div_euclid(16)) && self.rng.chance(0.1) {
            cavern_spot(world, MobKind::Slime, x, z, 39).filter(|p| p.y < 40.0)
        } else {
            None
        }
    }

    /// Java's fortress spawn list, used for spots inside fortress pieces:
    /// blazes, wither skeletons, zombified piglins, magma cubes and skeletons in groups.
    /// Light doesn't matter.
    pub(super) fn fortress_spawn<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, center: DVec3) {
        let total: u32 = FORTRESS_SPAWNS.iter().map(|s| s.1).sum();
        let mut r = (self.rng.next_f32() * total as f32) as u32;
        let &(kind, _, lo, hi) = FORTRESS_SPAWNS
            .iter()
            .find(|s| {
                let hit = r < s.1;
                r = r.saturating_sub(s.1);
                hit
            })
            .unwrap_or(&FORTRESS_SPAWNS[0]);
        let cap = kind.spawn_cap(Dimension::Nether);
        if self.count_near(kind, center) >= cap {
            return;
        }
        let angle = self.rng.range(0.0, TAU) as f64;
        let dist = self.rng.range(SPAWN_MIN_DIST as f32, SPAWN_MAX_DIST as f32) as f64;
        let x = (center.x + angle.cos() * dist).floor() as i32;
        let z = (center.z + angle.sin() * dist).floor() as i32;
        let top = self.rng.range(48.0, 100.0) as i32;
        let Some(pos) = cavern_spot(world, kind, x, z, top) else { return };
        if !in_spawn_ring(center, pos)
            || !clear_of_players(ctx, self.world_spawn, pos)
            || !world.in_fortress(pos.floor().as_ivec3())
        {
            return;
        }
        self.spawn(kind, pos);
        let group = self.rng.range(lo as f32, hi as f32 + 1.0) as u32;
        for _ in 1..group {
            let (dx, dz) = (self.rng.range(-3.0, 3.0) as i32, self.rng.range(-3.0, 3.0) as i32);
            if self.count_near(kind, center) < cap
                && let Some(p) = cavern_spot(world, kind, x + dx, z + dz, pos.y as i32 + 1)
                && world.in_fortress(p.floor().as_ivec3())
                && clear_of_players(ctx, self.world_spawn, p)
            {
                self.spawn(kind, p);
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

    /// Living mobs of `kind` that count against the cap of a player at
    /// `center` (those within despawn range).
    pub(super) fn count_near(&self, kind: MobKind, center: DVec3) -> usize {
        let r2 = DESPAWN_DIST * DESPAWN_DIST;
        self.mobs.iter().filter(|m| m.kind == kind && m.alive() && m.pos.distance_squared(center) <= r2).count()
    }
}
