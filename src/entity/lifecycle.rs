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
        self.difficulty = difficulty;
        let mut events = Vec::new();
        events.append(&mut self.pending_sounds);
        if difficulty == crate::simulation::difficulty::Difficulty::Peaceful {
            self.despawn_hostiles();
        }
        if ctx.spawning {
            self.spawn_timer -= dt as f32;
            if self.spawn_timer <= 0.0 {
                self.spawn_timer = SPAWN_INTERVAL;
                self.natural_spawn(world, ctx, difficulty);
            }
        }
        if difficulty != crate::simulation::difficulty::Difficulty::Peaceful {
            self.run_spawners(dt as f32, world, ctx);
            self.populate_bastions(dt as f32, world, ctx);
        }

        self.populate_structures(dt as f32, world, ctx, difficulty);
        self.trader_tick(dt as f32, world, ctx);
        self.village_upkeep(dt as f32, world);
        self.assign_hunts(ctx);
        self.trader_upkeep();
        self.player_spots.clear();
        self.player_spots.extend(ctx.players.iter().map(|t| (t.id, t.pos)));
        self.nether_sense(dt as f32, world, ctx);
        self.aquatic_sense(dt as f32, world);
        self.grow_loaded_turtles(dt as f32, world, ctx);
        self.tick_animals(dt as f32, world, ctx, &mut events);
        self.tick_leashes(dt as f32, world, ctx);
        self.tick_llamas(ctx);
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &self.mobs[i];
            let resident = matches!(
                m.kind,
                MobKind::Villager
                    | MobKind::IronGolem
                    | MobKind::SnowGolem
                    | MobKind::WanderingTrader
                    | MobKind::TraderLlama
            ) || m.persistent
                || m.built
                || m.convert_left > 0.0
                || m.villager.as_ref().is_some_and(|v| v.xp > 0);
            let gone = m.trader.as_ref().is_some_and(|t| t.despawn <= 0.0)
                || m.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                || !resident
                    && (ctx.nearest_player_dist2(m.pos).is_some_and(|d| d > DESPAWN_DIST * DESPAWN_DIST)
                        || !world.loaded(m.pos.floor().as_ivec3()));
            if resident
                && !gone
                && (!world.loaded(m.pos.floor().as_ivec3())
                    || ctx.nearest_player_dist2(m.pos).is_some_and(|d| d > DESPAWN_DIST * DESPAWN_DIST))
            {
                if let Some(v) = &mut self.mobs[i].villager {
                    v.active = false;
                }
                i += 1;
                continue;
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
                // Whatever a fox carries in its mouth drops, enchantments and names intact.
                if dead.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                    && let Some(stack) = dead.animal.as_ref().and_then(|a| a.mouth)
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
                    let count = 2 + (self.rng.next_f32() * 3.0) as usize;
                    for n in 0..count {
                        let offset =
                            DVec3::new((n % 2) as f64 - 0.5, 0.5, (n / 2) as f64 - 0.5) * dead.size as f64 * 0.25;
                        let mut child = Mob::new(dead.kind, dead.pos + offset, self.rng.range(0.0, TAU));
                        child.set_size(dead.size / 2);
                        self.mobs.push(child);
                    }
                }
                continue;
            }
            let before_tame = self.mobs[i].mount.as_ref().map(|s| (s.tame, s.rider));
            let grounded = self.mobs[i].on_ground;
            self.mobs[i].difficulty = difficulty;
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
        self.mob_index.rebuild(&self.mobs);
        self.resolve_strikes(ctx, &mut events);
        self.separate(dt);
        self.nether_upkeep(ctx, &mut events);
        if let Some(fight) = &mut self.fight {
            fight.update(dt, world, ctx, &mut self.rng, &mut events);
        }

        // Mob bow/crossbow shots become arrows, blaze shots fireballs.
        for e in &events {
            match *e {
                EntityEvent::Shoot { from, target, cause, shooter } => {
                    let mut arrow = Arrow::aimed(from, target, cause, &mut self.rng);
                    arrow.shooter = Some(shooter);
                    self.arrows.push(arrow);
                }
                EntityEvent::ThrowPotion { from, vel, potion } => {
                    self.potions.push(potion::ThrownPotion::new(potion, None, from, vel));
                }
                EntityEvent::Fireball { from, dir, large } => {
                    let ball =
                        if large { fireball::Fireball::large(from, dir) } else { fireball::Fireball::new(from, dir) };
                    if large {
                        let mut burst = crate::particles::Burst::new(crate::particles::Kind::LargeSmoke, from, 6);
                        burst.spread = DVec3::splat(0.4);
                        self.particles.push(crate::particles::Request::Burst(burst));
                    }
                    self.fireballs.push(ball);
                }
                _ => {}
            }
        }
        events.retain(|e| {
            !matches!(e, EntityEvent::Shoot { .. } | EntityEvent::Fireball { .. } | EntityEvent::ThrowPotion { .. })
        });
        {
            let mobs = &self.mobs;
            self.fireballs.retain_mut(|f| f.update(dt, world, ctx, mobs, &mut events));
        }
        {
            let (mobs, rng, fight) = (&mut self.mobs, &mut self.rng, &mut self.fight);
            self.arrows.retain_mut(|a| a.update(dt, world, ctx, mobs, fight.as_mut(), rng, &mut events));
            self.pearls.retain_mut(|p| p.update(dt, world, mobs, rng, &mut events));
            self.potions.retain_mut(|p| p.update(dt, world, ctx, mobs, rng, &mut events));
            self.thrown.retain_mut(|t| t.update(dt, world, mobs, &self.mob_index, &ctx.players, rng, &mut events));
        }
        {
            let rng = &mut self.rng;
            let mut i = 0;
            while i < self.bobbers.len() {
                if self.bobbers[i].update(dt, world, rng, ctx) {
                    i += 1;
                } else {
                    self.bobbers.swap_remove(i);
                }
            }
        }
        self.update_eyes(dt, &mut events);
        for e in &events {
            match *e {
                EntityEvent::MobShot { kind, pos, killed, burning, player_kill, owner } => {
                    if killed && self.mob_loot {
                        self.drop_loot_with_fire(kind, pos, 0, burning, player_kill);
                    }
                    if kind == MobKind::ZombifiedPiglin {
                        self.anger_piglins(pos);
                    }
                    if let Some(owner) = owner
                        && let Some(i) =
                            self.mobs.iter().position(|m| m.kind == kind && m.pos == pos && m.nether.is_some())
                    {
                        self.nether_hurt(i, nether::Foe::Player(owner));
                    }
                    // Java's OwnerHurtTargetGoal: a player's wolves join in on what they shoot.
                    if let Some(owner) = owner
                        && !killed
                        && let Some(i) = self.mobs.iter().position(|m| m.kind == kind && m.pos == pos && m.alive())
                    {
                        self.animals_owner_attack(i, owner);
                    }
                }
                EntityEvent::OwnerHurt { player, attacker } => {
                    // Java's OwnerHurtByTargetGoal: wolves go for whatever hurt their owner.
                    self.animals_defend_owner(attacker, player);
                }
                EntityEvent::DragonXp { pos, points } => {
                    if self.mob_loot {
                        self.spawn_xp(pos, points);
                    }
                }
                EntityEvent::MobKilled { kind, pos, burning, player_kill, looting } => {
                    self.drop_loot_with_fire(kind, pos, looting, burning, player_kill);
                }
                EntityEvent::LaidEgg { pos } => {
                    self.drop_from_block(
                        crate::inventory::Stack::new(crate::item::Item::EGG, 1),
                        pos.floor().as_ivec3(),
                    );
                }
                EntityEvent::Hatched { pos, count } => {
                    for _ in 0..count {
                        self.spawn_baby(MobKind::Chicken, pos);
                    }
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
            )
        });
        self.items.retain_mut(|item| !world.loaded(item.pos.floor().as_ivec3()) || item.update(dt, world));
        self.orbs.retain_mut(|orb| !world.loaded(orb.pos.floor().as_ivec3()) || orb.update(dt, world, ctx));
        self.tnt.retain_mut(|t| t.update(dt, world, &mut events));
        self.merge_timer -= dt as f32;
        if self.merge_timer <= 0.0 {
            self.merge_timer = MERGE_INTERVAL;
            item::merge(&mut self.items);
            orb::merge(&mut self.orbs);
        }

        let dtf = dt as f32;
        self.puffs.retain_mut(|p| {
            p.age += dtf;
            p.pos += p.vel * dt;
            p.vel *= 1.0 - (dt * 3.0).min(1.0);
            p.vel.y += 1.5 * dt;
            p.age < p.life
        });
        events
    }
}
