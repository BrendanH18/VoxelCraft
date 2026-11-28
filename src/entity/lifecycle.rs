//! Entity tick coordination, lifetime and event collection.
use super::*;
impl Entities {
    pub fn update<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) -> Vec<EntityEvent> {
        self.update_difficulty(dt, world, ctx, crate::simulation::difficulty::Difficulty::Normal)
    }

    /// [`Entities::update`] under this world's difficulty.
    pub fn update_difficulty<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) -> Vec<EntityEvent> {
        self.snapshot_positions();
        for m in &mut self.mobs {
            m.simulation_active = world.entity_ticking(m.pos.floor().as_ivec3());
            if let Some(v) = &mut m.villager {
                v.active = m.simulation_active;
            }
        }
        self.difficulty = difficulty;
        let local_difficulty = difficulty.effective(self.clock_time, 0, self.moon_brightness);
        let mut events = Vec::new();
        events.append(&mut self.pending_sounds);
        if difficulty == crate::simulation::difficulty::Difficulty::Peaceful {
            self.despawn_hostiles();
        }
        // The normal caller advances one fixed 50 ms tick. Recount on each
        // spawn tick; never reuse a SpawnState across five ticks of entity changes.
        self.spawn_timer += dt.max(0.0);
        while self.spawn_timer >= 0.05 - 1e-9 {
            self.spawn_timer -= 0.05;
            // Generation herds belong to chunk generation, which `doMobSpawning` doesn't gate.
            if ctx.dimension == Dimension::Overworld && world.light_settled() {
                self.populate_generation(world);
            }
            if ctx.spawning {
                self.natural_spawn(world, ctx, difficulty);
            }
            self.spawn_ticks = self.spawn_ticks.wrapping_add(1);
        }
        if difficulty != crate::simulation::difficulty::Difficulty::Peaceful {
            if self.spawner_blocks {
                self.run_spawners(dt as f32, world, ctx);
            }
            self.populate_bastions(dt as f32, world, ctx);
        }

        self.spawn_phantoms(dt, world, ctx);
        self.sense_phantoms();
        self.populate_structures(dt as f32, world, ctx, difficulty);
        self.populate_end_cities(dt as f32, world, ctx);
        self.trader_tick(dt as f32, world, ctx);
        self.tick_decor(world);
        self.village_upkeep(dt as f32, world);
        self.villager_pickup(world);
        self.populate_village_residents(world, difficulty);
        self.cat_spawner_tick(dt, world, ctx);
        self.assign_hunts(ctx);
        self.endermite_upkeep(dt, world);
        self.silverfish_upkeep(dt, world, &mut events);
        self.sulfur_cube_upkeep(dt, world, ctx, &mut events);
        self.illager_upkeep(dt, world, ctx, &mut events);
        self.raid_tick(dt, world, ctx, &mut events);
        self.happy_ghast_upkeep(dt, world, ctx);
        self.nautilus_upkeep(dt, world, ctx, &mut events);
        self.shulker_upkeep(dt, world, ctx, &mut events);
        self.trader_upkeep();
        // `Entity.hasMovedHorizontallyRecently`, from last update's positions.
        let moved: Vec<(PlayerId, bool)> = ctx
            .players
            .iter()
            .map(|t| {
                let last = self.player_spots.iter().find(|(id, _)| *id == t.id).map(|&(_, p)| p);
                (t.id, last.is_some_and(|p| (t.pos - p).with_y(0.0).length() > 1e-5))
            })
            .collect();
        self.player_spots.clear();
        self.player_spots.extend(ctx.players.iter().map(|t| (t.id, t.pos)));
        self.resolve_hurt_by(ctx);
        self.call_reinforcements(world, ctx);
        self.nether_sense(dt as f32, world, ctx);
        self.aquatic_sense(dt as f32, world);
        self.grow_loaded_turtles(dt as f32, world, ctx);
        self.frog_upkeep(dt, world, ctx);
        self.polar_sense(dt, world, ctx);
        self.neutral_sense(world, ctx);
        self.panda_upkeep(dt, world, ctx, &mut events);
        self.armadillo_upkeep(dt, world, ctx, &mut events);
        self.ocelot_upkeep(dt, ctx);
        self.fox_upkeep(ctx);
        self.fox_village_stroll(world, ctx);
        self.tick_allays(dt, world, ctx, &mut events);
        self.tick_animals(dt as f32, world, ctx, &mut events);
        self.tick_leashes(dt as f32, world, ctx);
        self.tick_boat_leashes(dt as f32, world, ctx);
        self.tick_llamas(ctx);
        self.tick_undead_mounts(dt, world, ctx);
        self.piglin_rides(world, ctx);
        self.sense_goals(world);
        // v1.0 gap Track 2c: what the mobs' look goals and brains see.
        self.sense_looks(world);
        self.sense_schools();
        self.sense_dolphins(world, &moved);
        // v0.16 Track C: the Wither's server AI before the mobs move.
        self.wither_tick(dt, world, ctx, &mut events);
        // v1.0 gap Track 2b: lowered mob spears stab before the mobs move.
        self.mob_spear_stabs(world, ctx, &mut events);
        if self.still.is_some() {
            self.still_ticks = self.still_ticks.saturating_add(1);
        }
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &self.mobs[i];
            if !m.simulation_active {
                i += 1;
                continue;
            }
            let resident = matches!(
                m.kind,
                MobKind::Allay
                    | MobKind::CopperGolem
                    | MobKind::Villager
                    | MobKind::IronGolem
                    | MobKind::SnowGolem
                    | MobKind::WanderingTrader
                    | MobKind::TraderLlama
            ) || m.kind.never_distance_despawns()
                || m.kind == MobKind::Warden
                // v0.16 Track C: `WitherBoss.checkDespawn` only leaves in Peaceful.
                || m.kind == MobKind::Wither
                || m.ocelot.as_ref().is_some_and(|s| s.trusting || s.ticks <= 2400)
                || m.persistent
                || m.creaking.as_ref().is_some_and(|s| s.home.is_some())
                || m.built
                || m.riding.is_some()
                || m.convert_left > 0.0
                || m.villager.as_ref().is_some_and(|v| v.xp > 0)
                // `Raider.removeWhenFarAway`: never while in a raid.
                || m.illager.as_ref().is_some_and(|s| s.raider.raid.is_some());
            // `PatrollingMonster.removeWhenFarAway`: a patroller only goes
            // beyond 128 blocks.
            let patrolling = m.illager.as_ref().is_some_and(|s| s.raider.patrolling);
            // Java's `Mob.checkDespawn`: a despawnable mob goes at once beyond
            // its category's despawn distance, and at random (1 in 800 a tick)
            // beyond 32 blocks once it has done nothing for 600 ticks.
            let near = ctx.nearest_player_dist2(m.pos);
            let far = m.kind.category().despawn_distance();
            let calm = category::MobCategory::NO_DESPAWN_DISTANCE;
            let idle_out = m.idle > 30.0
                && near.is_some_and(|d| d > calm * calm)
                && (!patrolling || near.is_some_and(|d| d > super::raid::PATROL_DESPAWN_SQR));
            let gone = m.trader.as_ref().is_some_and(|t| t.despawn <= 0.0)
                || m.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                || !resident
                    && (near.is_some_and(|d| d > far * far) || idle_out && self.rng.chance(dt as f32 * 20.0 / 800.0));
            if near.is_some_and(|d| d < calm * calm) {
                self.mobs[i].idle = 0.0;
            }
            if let Some(v) = &mut self.mobs[i].villager {
                v.active = true;
            }
            let m = &self.mobs[i];
            if gone {
                if m.dying.is_some_and(|t| t >= mob::DEATH_TIME) {
                    let mut burst = crate::particles::Burst::new(
                        crate::particles::Kind::Poof,
                        m.pos + DVec3::Y * m.shape().height * 0.5,
                        20,
                    );
                    burst.spread = DVec3::new(m.shape().half_width, m.shape().height * 0.5, m.shape().half_width);
                    burst.forced = true;
                    burst.velocity_spread = DVec3::splat(0.02);
                    self.particles.push(crate::particles::Request::Burst(burst));
                }
                let dead = self.mobs.swap_remove(i);
                if dead.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                    && let Some(stack) = dead
                        .wolf_armor
                        .as_ref()
                        .and_then(|s| s.armor)
                        .filter(|s| !s.enchants.has(crate::enchant::Enchantment::VanishingCurse))
                {
                    self.items.push(ItemEntity::new(stack, dead.pos, DVec3::ZERO, 0.5, &mut self.rng));
                }
                if dead.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                    && let Some(s) = &dead.copper_golem
                {
                    for stack in [s.cargo, s.antenna].into_iter().flatten() {
                        self.items.push(ItemEntity::new(stack, dead.pos, DVec3::ZERO, 0.5, &mut self.rng));
                    }
                }
                if dead.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                    && let Some(s) = &dead.allay
                {
                    for stack in
                        [s.cargo, s.liked.filter(|s| !s.enchants.has(crate::enchant::Enchantment::VanishingCurse))]
                            .into_iter()
                            .flatten()
                    {
                        self.items.push(ItemEntity::new(stack, dead.pos, DVec3::ZERO, 0.5, &mut self.rng));
                    }
                }
                // Whatever a fox carries in its mouth or a dolphin plays with
                // (`setGuaranteedDrop`) drops, enchantments and names intact.
                if dead.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                    && let Some(stack) =
                        dead.animal.as_ref().and_then(|a| a.mouth).or_else(|| dead.dolphin().and_then(|d| d.hand))
                {
                    self.items.push(ItemEntity::new(stack, dead.pos, DVec3::ZERO, 0.5, &mut self.rng));
                }
                if dead.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                    && let Some(s) = &dead.mount
                {
                    for stack in s
                        .slots
                        .iter()
                        .flatten()
                        .copied()
                        .chain(s.chest.then(|| crate::inventory::Stack::new(Block::CHEST, 1)))
                    {
                        self.items.push(ItemEntity::new(stack, dead.pos, DVec3::ZERO, 0.5, &mut self.rng));
                    }
                }
                if dead.dying.is_some_and(|t| t >= mob::DEATH_TIME) && dead.kind.is_cube() && dead.size > 1 {
                    // `SulfurCube.getSplitCount`: two babies (none once primed).
                    let count = if dead.kind == MobKind::SulfurCube {
                        if dead.sulfur_cube.as_ref().is_some_and(|s| s.primed()) { 0 } else { 2 }
                    } else {
                        2 + (self.rng.next_f32() * 3.0) as usize
                    };
                    for n in 0..count {
                        let offset =
                            DVec3::new((n % 2) as f64 - 0.5, 0.5, (n / 2) as f64 - 0.5) * dead.size as f64 * 0.25;
                        let mut child = Mob::new(dead.kind, dead.pos + offset, self.rng.range(0.0, TAU));
                        if dead.kind == MobKind::SulfurCube {
                            // `setUpSplitCube`: `setBaby(true)`.
                            child.set_sulfur_size(dead.size / 2);
                            child.persistent = dead.persistent;
                        } else {
                            child.set_size(dead.size / 2);
                        }
                        self.mobs.push(child);
                    }
                }
                continue;
            }
            // Fliers never land: they hold still after ten ticks.
            if let Some(face) = self.still
                && (self.mobs[i].on_ground || self.still_ticks > 10)
            {
                self.mobs[i].hold_still(world, face);
                self.mobs[i].burning = self.still_burning;
                i += 1;
                continue;
            }
            let before_tame = self.mobs[i].mount.as_ref().map(|s| (s.tame, s.rider));
            let grounded = self.mobs[i].on_ground;
            self.mobs[i].difficulty = difficulty;
            self.mobs[i].griefing = self.villager_griefing;
            self.mobs[i].local_difficulty = local_difficulty;
            self.mobs[i].trader_night = (0.5..0.96).contains(&self.village_time);
            self.mobs[i].update(dt, world, ctx, &mut self.rng, &mut events);
            let m = &self.mobs[i];
            if let Some((tame, rider)) = before_tame
                && let Some(s) = &m.mount
                && !tame
                && (s.tame || rider.is_some() && s.rider.is_none())
            {
                let mut burst = crate::particles::Burst::new(
                    if s.tame { crate::particles::Kind::Heart } else { crate::particles::Kind::Angry },
                    m.pos + DVec3::Y * m.shape().height,
                    7,
                );
                burst.spread = DVec3::new(0.5, 0.5, 0.5);
                self.particles.push(crate::particles::Request::Burst(burst));
            }
            if m.kind == MobKind::MagmaCube && !grounded && m.on_ground && m.alive() {
                let mut b = crate::particles::Burst::new(crate::particles::Kind::Flame, m.pos, m.size as u16 * 8);
                b.spread = DVec3::new(m.shape().half_width, 0.0, m.shape().half_width);
                self.particles.push(crate::particles::Request::Burst(b));
            }
            i += 1;
        }
        self.resolve_breeding();
        self.illager_after(dt, world, ctx, &mut events);
        self.mobs_pick_up_loot(&mut events);
        self.mob_index.rebuild(&self.mobs);
        self.watch_doors(dt as f32, &mut events);
        self.resolve_strikes(ctx, &mut events);
        self.separate(dt);
        self.position_undead_riders();
        self.nether_upkeep(ctx, &mut events);
        self.dolphin_items();
        // `TurtleLayEggGoal` places its clutch (`setBlockAndUpdate`) in the
        // update that clears the turtle's eggs, so a save can't fall between.
        // Two clutches on one cell in one update: the later turtle's
        // overwrites the earlier, as consecutive `setBlockAndUpdate`s do.
        let mut clutches: Vec<(IVec3, u8)> = Vec::new();
        for m in &mut self.mobs {
            if let Some((cell, eggs)) = m.take_laid_eggs() {
                clutches.retain(|&(c, _)| c != cell);
                clutches.push((cell, eggs));
            }
        }
        for (cell, eggs) in clutches {
            events.push(EntityEvent::AnimalBlock {
                cell,
                from: Block::AIR,
                to: crate::world::overworld_blocks::turtle_eggs(eggs),
            });
        }
        let mut frog_clutches = std::collections::HashSet::new();
        for m in &mut self.mobs {
            if let Some(cell) = m.frog.as_mut().and_then(|s| s.laid.take())
                && frog_clutches.insert(cell)
            {
                m.animal.as_mut().unwrap().pregnant = false;
                m.frog.as_mut().unwrap().sounds |= 1 << super::frog::FrogSound::Lay as u8;
                events.push(EntityEvent::AnimalBlock {
                    cell,
                    from: Block::AIR,
                    to: crate::world::frog_blocks::FROGSPAWN,
                });
            }
        }
        for m in &mut self.mobs {
            if let Some(s) = &mut m.frog {
                let sounds = std::mem::take(&mut s.sounds);
                for sound in super::frog::FrogSound::ALL {
                    if sounds & (1 << sound as u8) != 0 {
                        events.push(EntityEvent::Sound { sound: MobSound::FrogAction(sound), pos: m.pos });
                    }
                }
            }
        }
        for m in &mut self.mobs {
            if let Some(stack) = m.panda.as_mut().and_then(|s| s.spit.take()) {
                self.items.push(ItemEntity::new(stack, m.pos, DVec3::ZERO, 0.5, &mut self.rng));
            }
        }
        // v1.0 Track 7: dragons no fight owns (spawn eggs).
        self.wild_dragons.griefing_off = !self.villager_griefing;
        if let Some(fight) = &mut self.fight {
            fight.griefing_off = !self.villager_griefing;
        }
        let dragons_from = events.len();
        self.wild_dragons.update(dt, world, ctx, &mut self.rng, &self.crystals, self.fight.as_ref(), &mut events);
        if let Some(fight) = &mut self.fight {
            // The End's crystals tick with the fight.
            fight.update_beside(dt, world, ctx, &mut self.rng, &mut self.crystals, &self.wild_dragons, &mut events);
        } else if !self.crystals.is_empty() {
            self.crystal_time += dt;
            while self.crystal_time >= 0.05 - 1e-9 {
                self.crystal_time -= 0.05;
                self.crystals.tick(world, false, &mut events);
            }
        } else {
            self.crystal_time = 0.0;
        }
        // v1.0 gap Track 8: `EnderDragon.aiStep`'s `processFlappingMovement`
        // posts `flap` at the dragon with each wing beat it sounds.
        for e in &events[dragons_from..] {
            if let EntityEvent::Sound { sound: super::MobSound::DragonFlap, pos } = *e {
                let none = crate::world::vibration::EventSource::NONE;
                self.sculk.post(pos, crate::world::vibration::GameEvent::Flap, none, None);
            }
        }

        // Mob bow/crossbow shots become arrows, blaze shots fireballs.
        for e in &events {
            match *e {
                EntityEvent::Shoot { from, target, cause, shooter } => {
                    let mut arrow = Arrow::aimed(from, target, cause, &mut self.rng);
                    arrow.shooter = Some(shooter);
                    arrow.custom_effect =
                        self.mobs.iter().find(|m| m.uid == shooter).and_then(|m| m.kind.arrow_effect());
                    if let Some(m) = self.mobs.iter().find(|m| m.uid == shooter) {
                        arrow.enchants = m
                            .equipment
                            .main_hand()
                            .filter(|s| s.item == crate::item::Item::BOW)
                            .map_or(Default::default(), |s| s.enchants);
                    }
                    arrow.tipped = arrow.custom_effect.is_some();
                    // `Projectile.tick`'s first tick: `projectile_shoot` from its owner.
                    let source = crate::world::vibration::EventSource::mob(shooter);
                    self.sculk.post(from, crate::world::vibration::GameEvent::ProjectileShoot, source, None);
                    self.arrows.push(arrow);
                }
                EntityEvent::MobArrow { from, vel, cause, shooter, enchants } => {
                    let mut arrow = Arrow::mob_shot(from, vel, cause, shooter);
                    arrow.enchants = enchants;
                    let source = crate::world::vibration::EventSource::mob(shooter);
                    self.sculk.post(from, crate::world::vibration::GameEvent::ProjectileShoot, source, None);
                    self.arrows.push(arrow);
                }
                EntityEvent::ThrowTrident { from, target, shooter } => {
                    let stack = self
                        .mobs
                        .iter()
                        .find(|m| m.uid == shooter)
                        .and_then(|m| m.equipment.main_hand().copied())
                        .filter(|s| s.item == crate::weapons::TRIDENT)
                        .unwrap_or(crate::inventory::Stack::new(crate::weapons::TRIDENT, 1));
                    let uncertainty = crate::weapons::trident::mob_uncertainty(self.difficulty as u8);
                    let eye = from + DVec3::Y * f64::from(0.1f32);
                    let t =
                        super::trident::Trident::from_drowned(stack, shooter, eye, target, uncertainty, &mut self.rng);
                    let source = t.event_source();
                    self.sculk.post(t.pos, crate::world::vibration::GameEvent::ProjectileShoot, source, None);
                    self.tridents.push(t);
                }
                EntityEvent::ThrowPotion { from, vel, potion } => {
                    // The witch isn't named, so the shot has no source.
                    self.sculk.shot(from, crate::world::vibration::EventSource::NONE);
                    self.potions.push(potion::ThrownPotion::new(potion, None, from, vel));
                }
                EntityEvent::ShootWindCharge { from, dir, inaccuracy, shooter } => {
                    self.sculk.shot(from, crate::world::vibration::EventSource::mob(shooter));
                    self.thrown.push(thrown::Thrown::breeze_charge(from, dir, inaccuracy, shooter, &mut self.rng));
                }
                EntityEvent::ChargedBlast { center } => self.charged_blasts.push(center),
                EntityEvent::Fireball { from, dir, large, shooter } => {
                    let mut ball =
                        if large { fireball::Fireball::large(from, dir) } else { fireball::Fireball::new(from, dir) };
                    ball.owner = Some(super::trident::Owner::Mob(shooter));
                    if large {
                        let mut burst = crate::particles::Burst::new(crate::particles::Kind::LargeSmoke, from, 6);
                        burst.spread = DVec3::splat(0.4);
                        self.particles.push(crate::particles::Request::Burst(burst));
                    }
                    self.sculk.shot(from, crate::world::vibration::EventSource::NONE);
                    self.fireballs.push(ball);
                }
                _ => {}
            }
        }
        events.retain(|e| {
            !matches!(
                e,
                EntityEvent::Shoot { .. }
                    | EntityEvent::MobArrow { .. }
                    | EntityEvent::Fireball { .. }
                    | EntityEvent::ChargedBlast { .. }
                    | EntityEvent::ThrowPotion { .. }
                    | EntityEvent::ThrowTrident { .. }
                    | EntityEvent::ShootWindCharge { .. }
            )
        });
        // v1.0 gap Track 4c: the crystals and dragon parts as they now
        // stand, for every projectile but arrows (which strike them at
        // once); their hits are applied once they have all moved.
        let boss = super::dragon::Boxes::new(self.fight.as_ref(), &self.wild_dragons, &self.crystals);
        self.update_wither_skulls(world, ctx, &boss, &mut events);
        {
            let (mobs, sculk) = (&self.mobs, &mut self.sculk);
            let carts: Vec<(u32, DVec3, DVec3)> = if self.fireballs.is_empty() {
                Vec::new()
            } else {
                self.minecarts.iter().map(|c| (c.id, c.aabb().0, c.aabb().1)).collect()
            };
            self.fireballs.retain_mut(|f| {
                if !world.entity_ticking(f.pos.floor().as_ivec3()) {
                    return true;
                }
                let before = f.pos;
                let keep = f.update(dt, world, ctx, mobs, &carts, &boss, &mut events);
                if !keep {
                    let none = crate::world::vibration::EventSource::NONE;
                    sculk.land(world, f.pos, f.pos - before, none);
                }
                keep
            });
        }
        if self.arrows.iter().any(|a| a.piercing()) {
            self.ensure_mob_ids();
        }
        let others = if self.arrows.is_empty() { Vec::new() } else { self.arrow_targets() };
        {
            let (mobs, rng, sculk) = (&mut self.mobs, &mut self.rng, &mut self.sculk);
            let mut targets = super::dragon::Targets {
                fight: self.fight.as_mut(),
                wild: &mut self.wild_dragons,
                crystals: &mut self.crystals,
            };
            self.arrows.retain_mut(|a| {
                if !world.entity_ticking(a.pos.floor().as_ivec3()) {
                    return true;
                }
                let was_stuck = a.is_stuck();
                let before = a.pos;
                let keep = a.update(dt, world, ctx, mobs, &others, Some(&mut targets), rng, &mut events);
                // `Projectile.onHit`: `projectile_land` on the block it stuck
                // in, or where it hit an entity and went.
                if !was_stuck && (a.is_stuck() || !keep) {
                    let source = sculk_events::Sculk::projectile_source(a.owner, a.shooter);
                    let heading = if a.is_stuck() { a.dir.as_dvec3() } else { a.pos - before };
                    sculk.land(world, a.pos, heading, source);
                }
                keep
            });
            // Thrown things are removed when they hit (`Projectile.onHit`).
            let mut gateways = self.fight.as_mut().map(|f| &mut f.gateway_entities);
            self.pearls.retain_mut(|p| {
                // v1.0 gap Track 8f: a pearl keeps its own chunk ticking for
                // its owner (`registerAndUpdateEnderPearlTicket`, radius 2),
                // so it flies on wherever the terrain is loaded.
                let cell = p.pos.floor().as_ivec3();
                if !world.entity_ticking(cell) && !world.loaded(cell) {
                    return true;
                }
                let keep = p.update(dt, world, mobs, rng, &mut events, gateways.as_deref_mut(), &boss);
                if !keep {
                    sculk.land(world, p.pos, p.vel, sculk_events::Sculk::projectile_source(Some(p.owner), None));
                }
                keep
            });
            self.potions.retain_mut(|p| {
                if !world.entity_ticking(p.pos.floor().as_ivec3()) {
                    return true;
                }
                let keep = p.update(dt, world, ctx, mobs, &boss, rng, &mut events);
                if !keep {
                    sculk.land(world, p.pos, p.vel, sculk_events::Sculk::projectile_source(p.owner, None));
                }
                keep
            });
            self.thrown.retain_mut(|t| {
                if !world.entity_ticking(t.pos.floor().as_ivec3()) {
                    return true;
                }
                let keep = t.update(dt, world, mobs, &self.mob_index, &ctx.players, &boss, rng, &mut events);
                if !keep {
                    let source = sculk_events::Sculk::projectile_source(t.owner, t.shooter);
                    sculk.land(world, t.pos, t.pos - t.previous_pos, source);
                }
                keep
            });
        }
        self.pearl_endermites(&events, ctx);
        self.update_tridents(dt, world, ctx, &boss, &mut events);
        // v0.16 Track B: firework rockets.
        self.update_fireworks(world, ctx, &boss, &mut events);
        {
            let (rng, mobs, index) = (&mut self.rng, &self.mobs, &self.mob_index);
            let mut i = 0;
            while i < self.bobbers.len() {
                if !world.entity_ticking(self.bobbers[i].pos.floor().as_ivec3())
                    || self.bobbers[i].update(dt, world, rng, ctx, mobs, index, &boss)
                {
                    // v1.0 gap Track 8d: the block its flight hit.
                    let b = &mut self.bobbers[i];
                    if let Some(cell) = b.struck.take() {
                        events.push(super::projectile::block_hit(
                            cell,
                            b.pos,
                            b.burning(),
                            false,
                            Some(super::trident::Owner::Player(b.owner)),
                        ));
                    }
                    i += 1;
                } else {
                    self.bobbers.swap_remove(i);
                }
            }
        }
        for e in &events {
            if let EntityEvent::LingeringCloud { pos, potion, owner } = *e {
                self.clouds.push(super::lingering::Cloud::new(pos, potion, owner));
            }
        }
        events.retain(|e| !matches!(e, EntityEvent::LingeringCloud { .. }));
        self.ensure_mob_ids();
        self.clouds.retain_mut(|cloud| {
            !world.entity_ticking(cloud.pos.floor().as_ivec3())
                || cloud.tick(dt, ctx, &mut self.mobs, &boss, &mut self.rng, &mut events)
        });
        self.apply_dragon_hits(&mut events);
        self.update_eyes(dt, world, &mut events);
        self.apply_entity_shots(&events, ctx);
        self.resolve_mob_hits(&mut events);
        for e in &events {
            match *e {
                EntityEvent::MobShot { kind, pos, killed, burning, player_kill, owner } => {
                    if killed {
                        self.drop_loot_with_fire(kind, pos, 0, burning, player_kill);
                    }
                    if kind == MobKind::Warden
                        && !killed
                        && let Some(owner) = owner
                        && let Some(i) = self.mobs.iter().position(|m| m.kind == kind && m.pos == pos && m.alive())
                    {
                        self.warden_hurt(i, crate::world::vibration::EntityRef::Player(owner), false);
                    }
                    // Java's `VILLAGER_HURT`/`VILLAGER_KILLED` reputation events fire for any
                    // damage a player causes, including arrows and splash potions.
                    if kind == MobKind::Villager
                        && let Some(owner) = owner
                        && let Some(i) = self.mobs.iter().position(|m| m.kind == kind && m.pos == pos)
                    {
                        self.note_villager_hurt(i, owner, killed);
                    }
                    if let Some(owner) = owner
                        && let Some(i) =
                            self.mobs.iter().position(|m| m.kind == kind && m.pos == pos && m.nether.is_some())
                    {
                        self.nether_hurt(i, nether::Foe::Player(owner));
                    }
                    // `HurtByTargetGoal`: projectiles credit their shooter too.
                    if let Some(owner) = owner
                        && !killed
                        && (matches!(
                            kind,
                            MobKind::Dolphin
                                | MobKind::Blaze
                                | MobKind::Panda
                                | MobKind::Armadillo
                                | MobKind::Endermite
                        ) || super::illager::State::for_kind(kind).is_some()
                            || super::neutral::is_neutral(kind))
                        && let Some(i) = self.mobs.iter().position(|m| m.kind == kind && m.pos == pos && m.alive())
                    {
                        self.hurt_by_player_target(i, owner);
                    }
                    // Java's OwnerHurtTargetGoal: a player's wolves join in on what they shoot.
                    if let Some(owner) = owner
                        && !killed
                        && let Some(i) = self.mobs.iter().position(|m| m.kind == kind && m.pos == pos && m.alive())
                    {
                        self.animals_owner_attack(i, owner);
                    }
                }
                EntityEvent::OwnerHurt { player, attacker } => self.mob_hurt_player(player, attacker),
                EntityEvent::DragonXp { pos, points } => {
                    if self.mob_loot {
                        self.spawn_xp(pos, points);
                    }
                }
                EntityEvent::MobKilled { kind, pos, burning, player_kill, looting } => {
                    self.drop_loot_with_fire(kind, pos, looting, burning, player_kill);
                }
                EntityEvent::LaidEgg { pos, variant } => {
                    // `Chicken.aiStep`: `entity_place` at the hen (not named).
                    let none = crate::world::vibration::EventSource::NONE;
                    self.sculk.post(pos, crate::world::vibration::GameEvent::EntityPlace, none, None);
                    // `gameplay/chicken_lay`: the variant's egg.
                    self.drop_from_block(crate::inventory::Stack::new(variant.egg(), 1), pos.floor().as_ivec3());
                }
                EntityEvent::Hatched { pos, count, variant } => {
                    for _ in 0..count {
                        self.spawn_baby(MobKind::Chicken, pos);
                        self.mobs.last_mut().unwrap().set_farm_variant(variant);
                    }
                }
                EntityEvent::ExperienceBurst { pos, toward, points } => self.spawn_xp_toward(pos, toward, points),
                EntityEvent::DropEquipment { stack, pos } => self.scatter(stack, pos),
                EntityEvent::ThrowItem { stack, pos, vel } => {
                    self.items.push(item::ItemEntity::new(stack, pos, vel, 0.5, &mut self.rng));
                }
                EntityEvent::ItemParticle { pos, vel, item } => {
                    self.particles.push(crate::particles::Request::Item { pos, vel, item });
                }
                _ => {}
            }
        }
        events.retain(|e| {
            !matches!(
                e,
                EntityEvent::DragonXp { .. }
                    | EntityEvent::MobKilled { .. }
                    | EntityEvent::LaidEgg { .. }
                    | EntityEvent::Hatched { .. }
                    | EntityEvent::ExperienceBurst { .. }
                    | EntityEvent::DropEquipment { .. }
                    | EntityEvent::ThrowItem { .. }
                    | EntityEvent::ItemParticle { .. }
            )
        });
        let mut destroyed = Vec::new();
        self.items.retain_mut(|item| {
            if !world.entity_ticking(item.pos.floor().as_ivec3()) {
                return true;
            }
            let alive = item.update(dt, world);
            // Burnt, not despawned: a bundle spills what it holds.
            if !alive && item.age < item::LIFETIME {
                destroyed.push(*item);
            }
            alive
        });
        for item in destroyed {
            self.spill_destroyed(&item);
        }
        self.orbs.retain_mut(|orb| !world.entity_ticking(orb.pos.floor().as_ivec3()) || orb.update(dt, world, ctx));
        let tnt_explodes = self.tnt_explodes;
        let mobs = &self.mobs;
        let alive = |uid| mobs.iter().find(|m| m.uid == uid && m.alive()).map(|m| m.kind);
        self.tnt.retain_mut(|t| {
            !world.entity_ticking(t.pos.floor().as_ivec3()) || t.update(dt, world, &mut events, tnt_explodes, alive)
        });
        self.merge_timer -= dt as f32;
        if self.merge_timer <= 0.0 {
            self.merge_timer = MERGE_INTERVAL;
            item::merge_active(&mut self.items, |p| world.entity_ticking(p.floor().as_ivec3()));
            orb::merge_active(&mut self.orbs, |p| world.entity_ticking(p.floor().as_ivec3()));
        }

        let dtf = dt as f32;
        self.puffs.retain_mut(|p| {
            p.age += dtf;
            p.pos += p.vel * dt;
            p.vel *= 1.0 - (dt * 3.0).min(1.0);
            p.vel.y += 1.5 * dt;
            p.age < p.life
        });
        for e in &events {
            if let EntityEvent::Vibration { pos, event, source } = *e {
                self.sculk.post(pos, event, source, None);
            }
        }
        events.retain(|e| !matches!(e, EntityEvent::Vibration { .. }));
        events
    }
}

impl Entities {
    /// Mob `attacker` hurt `player` (their `lastHurtByMob`): Java's
    /// `OwnerHurtByTargetGoal` sends their wolves at it, and (v1.0 gap
    /// Track 2f) foxes that trust them defend against it. v1.0 gap Track
    /// 8f: also the mob behind a blast that hurt them.
    ///
    /// [`EntityEvent::OwnerHurt`]; v1.0 gap Track 2h: also called by the game for a blast a mob is behind (a
    /// ghast's fireball, a creeper), as `hurtServer` records any living
    /// source entity.
    pub fn mob_hurt_player(&mut self, player: PlayerId, attacker: u32) {
        self.animals_defend_owner(attacker, player);
        self.player_hurts = self.player_hurts.wrapping_add(1);
        // v1.0 gap Track 2f: the player's `lastHurtByMob`, which foxes that
        // trust it defend against.
        self.player_hurt_by.insert(player, (fox::Attacker::Mob(attacker), self.player_hurts, 0));
    }

    /// v1.0 gap Track 8f: a blast hurt mob `i`, and `by` was behind it
    /// (`DamageSource.getEntity`, for primed TNT its owner): `hurtServer`
    /// makes them its attacker (`setLastHurtByMob`, `lastHurtByPlayer`),
    /// as a projectile they shot does.
    pub(super) fn blamed_for_blast(&mut self, i: usize, by: super::trident::Owner, killed: bool) {
        use super::trident::Owner;
        let Some((kind, own_uid)) = self.mobs.get(i).map(|m| (m.kind, m.uid)) else { return };
        match by {
            Owner::Player(owner) => {
                if kind == MobKind::Villager {
                    self.note_villager_hurt(i, owner, killed);
                }
                if killed {
                    return;
                }
                if kind == MobKind::Warden {
                    self.warden_hurt(i, crate::world::vibration::EntityRef::Player(owner), false);
                }
                if self.mobs[i].nether.is_some() {
                    self.nether_hurt(i, nether::Foe::Player(owner));
                }
                self.hurt_by_player_target(i, owner);
                self.animals_owner_attack(i, owner);
            }
            // A mob isn't its own attacker (a creeper's blast is its own).
            Owner::Mob(uid) if uid != own_uid && !killed => {
                let Some(attacker) = self.mobs.iter().find(|a| a.uid == uid && a.alive()) else { return };
                if kind == MobKind::Wither {
                    if super::wither::friend(attacker.kind) {
                        return;
                    }
                    self.wither_hurt_by_mob(i, uid);
                }
                self.hurt_by_mob(i, uid, false);
            }
            Owner::Mob(_) => {}
        }
    }
}
