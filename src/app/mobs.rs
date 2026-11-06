//! Game-side glue for mobs: updating them, player melee, entity events and
//! the `--spawn` / `--wait` debug options.

use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::audio::SoundSource;
use crate::audio::sounds::{Call, Sound, Voice};
use crate::entity::{self, Entities, EntityEvent, MobKind, MobSound, PlayerId, Target};
use crate::physics;

use super::Game;

pub(super) struct Mobs {
    pub entities: Entities,
    /// The current left-button hold started on a mob: don't break blocks
    /// until it's released.
    /// `--spawn` requests, applied once the world has loaded.
    pending: Vec<(MobKind, IVec3, Option<crate::entity::armor::Equipped>)>,
    /// `--wait`: seconds to keep simulating before a `--screenshot`.
    wait: f64,
    wait_start: Option<Instant>,
}

impl Mobs {
    pub fn new(seed: u64, spawn: Vec<(MobKind, IVec3, Option<crate::entity::armor::Equipped>)>, wait: f64) -> Self {
        Self { entities: Entities::new(seed), pending: spawn, wait, wait_start: None }
    }

    /// For `--screenshot`: true once `--wait` seconds have passed since the
    /// first call.
    pub fn waited(&mut self) -> bool {
        let start = *self.wait_start.get_or_insert_with(Instant::now);
        start.elapsed().as_secs_f64() >= self.wait
    }
}

impl Game {
    /// Index of the living mob under the crosshair, if it's within reach and
    /// in front of the targeted block.
    pub(super) fn mob_target(&self) -> Option<usize> {
        let eye = self.session.player.eye();
        let dir = self.session.player.forward().as_dvec3();
        let block_dist = self
            .target()
            .and_then(|(b, _)| physics::ray_aabb(eye, dir, b.as_dvec3(), b.as_dvec3() + DVec3::ONE))
            .unwrap_or(self.session.entity_reach());
        self.mobs.entities.raycast(eye, dir, block_dist.min(self.session.entity_reach())).map(|(i, _)| i)
    }

    /// The host's sprint state for an attack.
    pub(super) fn sprinting_for_attack(&self) -> bool {
        self.session.player.sprinting
    }

    /// The End crystal or dragon part under the crosshair, if it's within
    /// reach and in front of the targeted block and any mob.
    fn fight_target(&self) -> Option<crate::entity::dragon::Hit> {
        let eye = self.session.player.eye();
        let dir = self.session.player.forward().as_dvec3();
        let reach = self.session.entity_reach();
        let block_dist = self
            .target()
            .and_then(|(b, _)| physics::ray_aabb(eye, dir, b.as_dvec3(), b.as_dvec3() + DVec3::ONE))
            .unwrap_or(reach);
        self.mobs.entities.fight_raycast(eye, dir, block_dist.min(reach)).map(|(hit, _)| hit)
    }

    pub(super) fn release_attack(&mut self) {
        self.body.attack_held = false;
    }

    /// Block breaking is suppressed while aiming at a mob, and for the rest
    /// of a click that started as an attack.
    pub(super) fn attacking(&self) -> bool {
        self.body.attack_held
            || self.mob_target().is_some()
            || self.fight_target().is_some()
            || self.lan_player_target(&super::lan::PlayerRef::Host).is_some()
    }

    /// Applies `--spawn` requests (with the other `--place` edits).
    pub(super) fn spawn_pending_mobs(&mut self) {
        for (kind, mut pos, armor) in std::mem::take(&mut self.mobs.pending) {
            if pos.y == i32::MIN {
                pos.y = self.world.generator.column(pos.x, pos.z).height + 1;
            }
            self.mobs.entities.spawn(kind, pos.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
            if let Some(equipped) = armor
                && let Some(mob) = self.mobs.entities.mobs.last_mut()
            {
                mob.armor = [Some(equipped.kind); 4];
                mob.armor_glint = if equipped.glint { 0b1111 } else { 0 };
            }
        }
    }

    pub(super) fn update_mobs(&mut self, dt: f64) {
        // Entities leave the level with their chunk and return when it loads.
        self.mobs.entities.sync_chunk_entities(&mut self.world);
        self.mobs.entities.moon_brightness =
            [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75][self.day_count.rem_euclid(8) as usize];
        self.mobs.entities.clock_time =
            self.day_count.saturating_mul(24_000) + (self.day_time * 24_000.0).round() as i64;
        // The world spawn (Java's respawn data) is an Overworld position.
        self.mobs.entities.world_spawn =
            (self.dimension == crate::world::terrain::Dimension::Overworld).then_some(self.world_spawn);
        self.mobs.entities.trader_spawning =
            self.gamerules.bool("spawn_wandering_traders") && self.gamerules.bool("spawn_mobs");
        self.mobs.entities.phantom_spawning = self.gamerules.bool("spawn_phantoms");
        self.mobs.entities.monster_spawning = self.gamerules.bool("spawn_monsters");
        self.mobs.entities.spawner_blocks = self.gamerules.bool("spawner_blocks_work");
        self.mobs.entities.villager_griefing = self.gamerules.bool("mob_griefing");
        self.mobs.entities.mob_loot = self.gamerules.bool("mob_drops");
        self.mobs.entities.neutral_forgive_dead_players = self.gamerules.bool("forgive_dead_players");
        self.mobs.entities.thundering = self.dimension.has_sky() && self.weather.thunder_level() > 0.9;
        self.mobs.entities.neutral_universal_anger = self.gamerules.bool("universal_anger");
        self.mobs.entities.village_time = self.day_time;
        self.mobs.entities.village_day = self.day_count;
        for m in &mut self.mobs.entities.mobs {
            if let Some(v) = &mut m.villager {
                v.trading = self.inventory_open && self.body.container == super::Container::Trading(v.id)
                    || self.pads.trading(v.id);
            }
        }
        // Java keeps the hook while a rod is in either hand.
        let off_rod = self.session.inventory.offhand.is_some_and(|s| s.item == crate::item::Item::FISHING_ROD);
        if !(self.held_item() == Some(crate::item::Item::FISHING_ROD) || off_rod) || self.session.vitals.is_dead() {
            self.mobs.entities.drop_bobber(self.session.id);
        }
        super::seat_rider(&self.mobs.entities, &mut self.session.player, crate::entity::PlayerId::HOST);
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            super::seat_rider(&self.mobs.entities, &mut bot.agent.session.player, bot.agent.session.id);
        }
        if self.difficulty == crate::simulation::difficulty::Difficulty::Peaceful {
            self.mobs.entities.despawn_hostiles();
        }
        // `ThrownEnderpearl.tick` discards a dead owner's pearls.
        if self.gamerules.bool("ender_pearls_vanish_on_death") {
            let host_dead = self.session.vitals.is_dead();
            let agents = &self.agents;
            self.mobs.entities.discard_pearls_of_dead(|owner| {
                if owner == PlayerId::HOST {
                    host_dead
                } else {
                    agents.players.values().any(|b| b.id == owner && b.agent.session.vitals.is_dead())
                }
            });
        }
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse);
            self.audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.95, 1.05));
        }
        crate::entity::golem::finish_golems(&mut self.world, &mut self.mobs.entities);
        crate::entity::frog::finish_hatches(&mut self.world, &mut self.mobs.entities);
        for (cell, count) in std::mem::take(&mut self.world.hatched_turtles) {
            self.mobs.entities.hatch_turtles(cell, count);
        }
        let mut players = vec![Target {
            spectator: self.session.mode == voxelcraft::rules::GameMode::Spectator,
            alive: !self.session.vitals.is_dead(),
            look: self.session.player.forward().as_dvec3(),
            thorns: Target::thorns_of(&self.session.inventory.armor),
            held_enchants: self
                .session
                .inventory
                .get(self.body.actions.selected)
                .map_or(Default::default(), |s| s.active_enchants()),
            shape: self.session.player.collision_shape(),
            gold_armor: crate::entity::nether::wears_gold(&self.session.inventory.armor),
            held_item: self.held_item(),
            on_ground: self.session.player.on_ground,
            health: self.session.vitals.health,
            effects: self.session.vitals.effects.mask(),
            in_water: self.session.player.in_water,
            swimming: self.session.player.swimming,
            time_since_rest: self.session.vitals.time_since_rest,
            sprinting: self.session.player.sprinting,
            passenger: self.session.player.vehicle.is_some(),
            ..Target::new(
                PlayerId::HOST,
                self.session.player.pos,
                self.session.mode.targetable() && !self.session.vitals.is_dead(),
            )
        }];
        // Agents keep source-dimension positions until arrival relocates them.
        if self.arrival.is_none() {
            players.extend(self.agents.targets());
        }
        let ctx = entity::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::for_dimension(
                self.dimension,
                (self.day_time.rem_euclid(1.) * 24000.).floor() as u32,
                self.weather.rain_level(),
                self.weather.thunder_level(),
            ),
            players,
            // The Nether has no sun to burn the undead.
            daylight: if self.dimension.has_sky() {
                self.weather.dim(super::sky_state(self.day_time).daylight)
            } else {
                0.0
            },
            raining: self.weather.is_raining() && self.dimension.has_sky(),
            spawning: self.gamerules.bool("spawn_mobs"),
            dimension: self.dimension,
        };
        self.world.set_bee_clock(self.dimension, (self.day_time.rem_euclid(1.) * 24000.).floor() as u32);
        self.mobs.entities.tick_lightning(dt, &mut self.world, &ctx, self.weather.is_thundering());
        let mut smashed = Vec::new();
        let mut arrow_effects = crate::entity::ArrowEffectGate::default();
        for event in self.mobs.entities.update_world(dt, &mut self.world, &ctx, self.difficulty) {
            match event {
                EntityEvent::ArrowEffectsStart { player } => arrow_effects.begin(player),
                EntityEvent::ArrowEffectsEnd => arrow_effects.end(),
                EntityEvent::ProjectileBlockHit { cell, pos, normal, arrow, burning } => {
                    self.world.hit_redstone_target(cell, pos, normal, arrow);
                    self.world.touch_redstone_ore(cell);
                    if burning {
                        self.world.light_candle(cell);
                    }
                }
                EntityEvent::PlayerHit { player: PlayerId::HOST, damage, knockback, java, cause } => {
                    // Knockback only lands with damage, so hurt immunity
                    // also stops repeated shoves.
                    let toward = match java.and_then(|hit| hit.away()) {
                        Some(away) => Some(-away),
                        None => Some(-knockback.as_dvec3()),
                    };
                    let accepted = self.damage_player_from(self.difficulty.mob_damage(damage), cause, toward);
                    arrow_effects.record_hit(PlayerId::HOST, if self.session.vitals.is_dead() { 0. } else { accepted });
                    if accepted > 0. {
                        if let Some(hit) = java {
                            let entities = &mut self.mobs.entities;
                            self.session.knocked_back(&hit, &mut || f64::from(entities.roll_range(0.0, 1.0)));
                        } else {
                            self.session.player.hurt_from(knockback.as_dvec3());
                            let resistance = self.session.inventory.knockback_resistance();
                            self.session.player.vel +=
                                crate::simulation::survival::knockback_taken(knockback.as_dvec3(), resistance);
                        }
                    }
                }
                EntityEvent::PlayerHit { player, damage, knockback, java, cause } => {
                    arrow_effects.record_hit(player, 0.);
                    if let Some(bot) = self.agents.by_id_mut(player) {
                        let damage = self.difficulty.mob_damage(damage);
                        let accepted = match java {
                            Some(hit) => bot.agent.hurt_knocked(damage, cause, &hit, &mut self.mobs.entities),
                            None => bot.agent.hurt(damage, cause, knockback.as_dvec3(), &mut self.mobs.entities),
                        };
                        arrow_effects
                            .record_hit(player, if bot.agent.session.vitals.is_dead() { 0. } else { accepted });
                    }
                }
                EntityEvent::PlayerLightning { player } => {
                    if player == PlayerId::HOST {
                        self.damage_player_armored(5., "was struck by lightning");
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt(5., "was struck by lightning", DVec3::ZERO, &mut self.mobs.entities);
                    }
                }
                EntityEvent::PlayerFall { player, damage } => {
                    if player == PlayerId::HOST {
                        self.damage_player(damage, crate::simulation::survival::CAUSE_FALL);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt_fall(damage, &mut self.mobs.entities);
                    }
                }
                EntityEvent::PlayerBeam { player, physical, magic } => {
                    let physical = self.difficulty.mob_damage(physical);
                    if player == PlayerId::HOST {
                        let reduced = crate::simulation::survival::armor_reduce(
                            physical,
                            self.session.inventory.armor_points(),
                            self.session.inventory.armor_toughness(),
                        );
                        if self.damage_player(reduced + magic, "was slain by a guardian") > 0.0 {
                            self.session.inventory.wear_armor(physical);
                        }
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt_beam(physical, magic, &mut self.mobs.entities);
                    }
                }
                EntityEvent::BeeSting { uid, player, poison } => {
                    let source = self.mobs.entities.mobs.iter().find(|m| m.uid == uid).map(|m| m.pos);
                    let damage = self.difficulty.mob_damage(2.);
                    let applied = if player == PlayerId::HOST {
                        let toward = source.map(|p| p - self.session.player.pos);
                        let hit = self.damage_player_from(damage, "was stung by a bee", toward) > 0.;
                        if hit && poison > 0 {
                            self.session.vitals.apply_effect(crate::simulation::effects::Effect::Poison, 0, poison);
                        }
                        hit
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        let toward = source.map(|p| p - bot.agent.session.player.pos);
                        let blocked = toward
                            .is_some_and(|d| bot.agent.session.shield_intercept(bot.agent.selected, damage, d).blocked);
                        let hit = !blocked
                            && bot.agent.hurt(damage, "was stung by a bee", DVec3::ZERO, &mut self.mobs.entities) > 0.;
                        if hit && poison > 0 {
                            bot.agent.session.vitals.apply_effect(
                                crate::simulation::effects::Effect::Poison,
                                0,
                                poison,
                            );
                        }
                        hit
                    } else {
                        false
                    };
                    self.mobs.entities.confirm_bee_sting(uid, applied);
                }
                EntityEvent::PlayerSting { player, damage, cause, poison } => {
                    // Pufferfish contact (`mobAttack`) and guardian thorns both
                    // have a living, non-player cause, so difficulty scales them.
                    let damage = self.difficulty.mob_damage(damage);
                    if player == PlayerId::HOST {
                        if self.damage_player_armored(damage, cause) > 0.0 && poison > 0 {
                            self.session.vitals.apply_effect(crate::simulation::effects::Effect::Poison, 0, poison);
                        }
                    } else if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.hurt(damage, cause, DVec3::ZERO, &mut self.mobs.entities) > 0.0
                        && poison > 0
                    {
                        bot.agent.session.vitals.apply_effect(crate::simulation::effects::Effect::Poison, 0, poison);
                    }
                }
                EntityEvent::PlayerEffect { player: PlayerId::HOST, effect, amplifier, ticks } => {
                    if arrow_effects.allows() && self.session.mode.is_survival() {
                        let damage = self.session.vitals.apply_effect(effect, amplifier, ticks);
                        if damage > 0.0 {
                            self.damage_player(damage, "was killed by magic");
                        }
                    }
                }
                EntityEvent::PlayerEffect { player, effect, amplifier, ticks } => {
                    if arrow_effects.allows()
                        && let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.mode.is_survival()
                    {
                        let damage = bot.agent.session.vitals.apply_effect(effect, amplifier, ticks);
                        if damage > 0.0 {
                            bot.agent.hurt(damage, "was killed by magic", DVec3::ZERO, &mut self.mobs.entities);
                        }
                    }
                }
                EntityEvent::PotionSplashed { pos, colour } => {
                    let mut burst = crate::particles::Burst::new(crate::particles::Kind::Effect, pos, 40);
                    burst.spread = DVec3::splat(0.4);
                    burst.velocity_spread = DVec3::splat(0.12);
                    burst.color =
                        Some([colour[0] as f32 / 255.0, colour[1] as f32 / 255.0, colour[2] as f32 / 255.0, 1.0]);
                    self.world.particles.push(crate::particles::Request::Burst(burst));
                    // `SPLASH_POTION_BREAK` plays in `NEUTRAL`.
                    let glass = Sound::Break(crate::audio::sounds::Material::Glass);
                    self.audio.play_in(SoundSource::Neutral, glass, Some(pos), 1.0, (0.9, 1.1));
                }
                EntityEvent::PlayerMagic { player: PlayerId::HOST, amount } => {
                    if arrow_effects.allows() && self.session.mode.is_survival() {
                        if amount > 0.0 {
                            self.damage_player(amount, "was killed by magic");
                        } else {
                            self.session.vitals.health =
                                (self.session.vitals.health - amount).min(crate::simulation::survival::MAX_HEALTH);
                        }
                    }
                }
                EntityEvent::PlayerMagic { player, amount } => {
                    if arrow_effects.allows()
                        && let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.mode.is_survival()
                    {
                        if amount > 0.0 {
                            bot.agent.hurt_magic(amount, &mut self.mobs.entities);
                        } else {
                            bot.agent.session.vitals.health =
                                (bot.agent.session.vitals.health - amount).min(crate::simulation::survival::MAX_HEALTH);
                        }
                    }
                }
                EntityEvent::Explosion { center, power, cause, credit_player } => {
                    self.explode(center, power, cause, credit_player)
                }
                EntityEvent::PearlLanded { owner, pos } => {
                    let mut burst = crate::particles::Burst::new(crate::particles::Kind::Portal, pos + DVec3::Y, 32);
                    burst.spread = DVec3::Y;
                    burst.velocity_spread = DVec3::new(1.0, 0.0, 1.0);
                    self.world.particles.push(crate::particles::Request::Burst(burst));
                    self.pearl_landed(owner, pos);
                }
                EntityEvent::Ignite { player: PlayerId::HOST, secs } => {
                    if self.session.mode.is_survival() {
                        self.session.vitals.ignite(secs);
                    }
                }
                EntityEvent::Ignite { player, secs } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.mode.is_survival()
                    {
                        bot.agent.session.vitals.ignite(secs);
                    }
                }
                EntityEvent::IgniteBlock { cell } => {
                    if self.gamerules.bool("mob_griefing") {
                        self.world.ignite(cell);
                    }
                }
                EntityEvent::Fireball { .. } | EntityEvent::ThrowPotion { .. } => {}
                EntityEvent::Sound { sound, pos } => {
                    // Brutes and zoglins are deeper takes on piglin and hoglin voices.
                    let deep = matches!(
                        sound,
                        MobSound::Ambient(MobKind::PiglinBrute | MobKind::Zoglin)
                            | MobSound::Hurt(MobKind::PiglinBrute | MobKind::Zoglin)
                            | MobSound::Death(MobKind::PiglinBrute | MobKind::Zoglin)
                    );
                    let source = mob_sound_source(sound);
                    let (sound, gain) = match sound {
                        MobSound::Thunder => {
                            self.audio.play_in(SoundSource::Weather, Sound::Thunder, None, 1., (0.8, 1.));
                            continue;
                        }
                        MobSound::FrogAction(action) => {
                            use voxelcraft::entity::frog::FrogSound as F;
                            match action {
                                F::Tongue => (Sound::Bow, 2.0),
                                F::Eat => (Sound::Pop, 2.0),
                                F::Jump => (Sound::Swim, 1.0),
                                F::Land => (Sound::Swim, 2.0),
                                F::Lay => (Sound::Swim, 1.0),
                                F::Hatch => (Sound::Pop, 1.0),
                                F::Grow => (Sound::Pop, 0.15),
                            }
                        }
                        MobSound::CopperGolem(action, stage) => (Sound::CopperGolem(action, stage), 0.7),
                        MobSound::Hurt(MobKind::CopperGolem) => {
                            (Sound::CopperGolem(voxelcraft::entity::copper_golem::Sound::Hurt, 0), 0.9)
                        }
                        MobSound::Death(MobKind::CopperGolem) => {
                            (Sound::CopperGolem(voxelcraft::entity::copper_golem::Sound::Death, 0), 0.9)
                        }
                        MobSound::Bell => (Sound::Bell, 1.0),
                        // Procedural stand-ins for Java's `wandering_trader.disappeared`/`reappeared`.
                        MobSound::TraderDrink(true) => (Sound::Teleport, 0.5),
                        MobSound::TraderDrink(false) => (Sound::Pop, 0.8),
                        MobSound::BoatPaddle(water) => (
                            Sound::Step(if water {
                                crate::audio::sounds::Material::Water
                            } else {
                                crate::audio::sounds::Material::Wood
                            }),
                            0.6,
                        ),
                        MobSound::Fuse => (Sound::Fuse, 1.0),
                        MobSound::Bow => (Sound::Bow, 1.0),
                        MobSound::Ambient(kind) => (Sound::Mob(voice(kind), Call::Ambient), 0.7 * mob_volume(kind)),
                        MobSound::Hurt(kind) => (Sound::Mob(voice(kind), Call::Hurt), 0.9 * mob_volume(kind)),
                        MobSound::Death(kind) => (Sound::Mob(voice(kind), Call::Death), 0.9 * mob_volume(kind)),
                        MobSound::Scream => (Sound::Scream, 1.0),
                        MobSound::Teleport => (Sound::Teleport, 0.8),
                        MobSound::Fireball => (Sound::Fireball, 0.9),
                        MobSound::EyeDeath => (Sound::EyeDeath, 0.8),
                        MobSound::DragonFlap => (Sound::DragonFlap, 1.0),
                        MobSound::DragonGrowl => (Sound::DragonGrowl, 1.0),
                        MobSound::DragonShoot => (Sound::Fireball, 1.0),
                        MobSound::DragonSmash => (Sound::Explosion, 0.25),
                        // Java plays the death to the whole dimension.
                        MobSound::DragonDeath => {
                            self.audio.play(Sound::DragonDeath, None, 1.0, (1.0, 1.0));
                            continue;
                        }
                    };
                    let pitch = if sound == Sound::Fireball && gain == 1.0 {
                        (0.6, 0.7)
                    } else if deep {
                        (0.72, 0.82)
                    } else {
                        (0.9, 1.1)
                    };
                    self.audio.play_in(source.unwrap_or(sound.source()), sound, Some(pos), gain, pitch);
                }
                EntityEvent::MobShot { pos, .. } | EntityEvent::EntityShot { pos, .. } => {
                    // An arrow landing is the arrow entity's sound, `NEUTRAL`.
                    self.audio.play_in(SoundSource::Neutral, Sound::Hit, Some(pos + DVec3::Y * 0.5), 0.8, (0.9, 1.1))
                }
                EntityEvent::Shoot { .. }
                | EntityEvent::LingeringCloud { .. }
                | EntityEvent::MobHit { .. }
                | EntityEvent::OwnerHurt { .. }
                | EntityEvent::DragonXp { .. }
                | EntityEvent::MobKilled { .. }
                | EntityEvent::LaidEgg { .. }
                | EntityEvent::Hatched { .. } => {}
                EntityEvent::AnimalBlock { cell, from, to } => {
                    if self.world.get_block(cell) == Some(from) {
                        self.world.set_block(cell, to);
                    }
                }
                EntityEvent::VillagerDoor { cell } => {
                    if self.world.get_block(cell).is_some_and(|b| {
                        matches!(b.shaped(), Some(crate::world::block::Shaped::Door { open: false, upper: false, .. }))
                    }) {
                        self.toggle_door(cell, glam::Vec3::ZERO);
                    }
                }
                EntityEvent::VillagerHarvest { cell } => {
                    if let Some(block) = self.world.get_block(cell)
                        && let Some((crop, age)) = block.as_crop()
                        && age == crop.max_age()
                    {
                        self.world.spill_block(cell, block);
                        self.world.set_block(cell, crate::world::block::Block::crop(crop, 0));
                    }
                }
                EntityEvent::VillagerDoorClose { cell } => {
                    if self.world.get_block(cell).is_some_and(|b| {
                        matches!(b.shaped(), Some(crate::world::block::Shaped::Door { open: true, upper: false, .. }))
                    }) {
                        self.toggle_door(cell, glam::Vec3::ZERO);
                    }
                }
                EntityEvent::BreakBlock { cell } => smashed.push(cell),
                EntityEvent::Shove { player: PlayerId::HOST, velocity } => {
                    if self.session.mode.is_survival() && !self.session.vitals.is_dead() {
                        let push = crate::simulation::survival::knockback_taken(
                            velocity.as_dvec3(),
                            self.session.inventory.knockback_resistance(),
                        );
                        shove(&mut self.session.player.vel, push);
                    }
                }
                EntityEvent::Shove { player, velocity } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.mode.is_survival()
                    {
                        let push = crate::simulation::survival::knockback_taken(
                            velocity.as_dvec3(),
                            bot.agent.session.inventory.knockback_resistance(),
                        );
                        shove(&mut bot.agent.session.player.vel, push);
                    }
                }
                EntityEvent::BuildGateway { pos } => self.world.build_gateway(pos),
                EntityEvent::PearlGateway { owner, cell, pos } => {
                    if owner == PlayerId::HOST {
                        if !self.session.vitals.is_dead() {
                            self.enter_gateway(cell);
                        }
                    } else {
                        // Agents can't travel by gateway yet: the pearl
                        // lands at its mouth instead.
                        self.pearl_landed(owner, pos);
                    }
                }
                EntityEvent::DragonKilled { first } => {
                    self.world.open_exit_portal(first);
                    self.audio.play(Sound::PortalSpawn, None, 1.0, (1.0, 1.0));
                }
            }
        }
        if self.difficulty == crate::simulation::difficulty::Difficulty::Peaceful {
            self.mobs.entities.despawn_hostiles();
        }
        // All the blocks the dragon flew through this tick, in one edit.
        if self.gamerules.bool("mob_griefing") && !smashed.is_empty() {
            smashed.sort_unstable_by_key(|p| (p.x, p.y, p.z));
            smashed.dedup();
            self.world.break_blocks(&smashed);
        }
    }
}

/// Raises `vel` to at least `push` along its direction, so a shove that
/// repeats every tick doesn't build up.
fn shove(vel: &mut DVec3, push: DVec3) {
    let len = push.length();
    if len > 0.0 {
        let along = vel.dot(push / len);
        if along < len {
            *vel += push / len * (len - along);
        }
    }
}

/// The category Java plays a mob sound in, where it isn't the sound's own
/// ([`Sound::source`]): the creature's voice decides hostile or friendly,
/// and these are the exceptions.
fn mob_sound_source(sound: MobSound) -> Option<SoundSource> {
    match sound {
        MobSound::FrogAction(voxelcraft::entity::frog::FrogSound::Lay | voxelcraft::entity::frog::FrogSound::Hatch) => {
            Some(SoundSource::Blocks)
        }
        MobSound::FrogAction(_) => Some(SoundSource::Neutral),
        // `AbstractVillager`, boats and the dragon's smashing: `NEUTRAL`, `NEUTRAL`, `HOSTILE`.
        MobSound::TraderDrink(_) | MobSound::BoatPaddle(_) => Some(SoundSource::Neutral),
        MobSound::Thunder => Some(SoundSource::Weather),
        MobSound::DragonSmash => Some(SoundSource::Hostile),
        // Skeletons and pillagers shoot as hostile mobs, not as the player.
        MobSound::Bow => Some(SoundSource::Hostile),
        // The snow golem borrows the slime's voice but is a friendly mob.
        MobSound::Ambient(MobKind::SnowGolem)
        | MobSound::Hurt(MobKind::SnowGolem)
        | MobSound::Death(MobKind::SnowGolem) => Some(SoundSource::Neutral),
        _ => None,
    }
}

// Java Bat.getSoundVolume; samples stay normalized like the other voices.
fn mob_volume(kind: MobKind) -> f32 {
    if kind == MobKind::Bat { 0.1 } else { 1.0 }
}

fn voice(kind: MobKind) -> Voice {
    match kind {
        MobKind::Pig => Voice::Pig,
        MobKind::Cow | MobKind::Mooshroom => Voice::Cow,
        MobKind::Bat => Voice::Bat,
        MobKind::Frog => Voice::Frog,
        MobKind::PolarBear => Voice::PolarBear,
        MobKind::Panda => Voice::Panda,
        MobKind::Armadillo => Voice::Armadillo,
        MobKind::Ocelot => Voice::Ocelot,
        MobKind::CopperGolem => Voice::Skeleton,
        MobKind::Creaking => Voice::Creaking,
        MobKind::Tadpole => Voice::Fish,
        MobKind::Sheep => Voice::Sheep,
        MobKind::Chicken => Voice::Chicken,
        MobKind::Zombie | MobKind::Husk | MobKind::Drowned | MobKind::ZombieVillager => Voice::Zombie,
        MobKind::Phantom | MobKind::Skeleton | MobKind::Stray | MobKind::Bogged | MobKind::WitherSkeleton => {
            Voice::Skeleton
        }
        MobKind::Creeper => Voice::Creeper,
        MobKind::Spider | MobKind::CaveSpider => Voice::Spider,
        MobKind::ZombifiedPiglin => Voice::Zombie,
        MobKind::Enderman => Voice::Enderman,
        MobKind::Blaze => Voice::Blaze,
        MobKind::Silverfish => Voice::Spider,
        MobKind::Slime | MobKind::MagmaCube => Voice::Slime,
        MobKind::Ghast => Voice::Ghast,
        MobKind::Witch => Voice::Witch,
        MobKind::Villager | MobKind::WanderingTrader => Voice::Villager,
        MobKind::TraderLlama | MobKind::Llama | MobKind::Camel => Voice::Cow,
        MobKind::SkeletonHorse | MobKind::ZombieHorse | MobKind::Horse | MobKind::Donkey | MobKind::Mule => {
            Voice::Horse
        }
        MobKind::IronGolem => Voice::Cow,
        MobKind::SnowGolem => Voice::Slime,
        MobKind::Piglin | MobKind::PiglinBrute => Voice::Piglin,
        MobKind::Hoglin | MobKind::Zoglin => Voice::Hoglin,
        MobKind::Strider => Voice::Strider,
        MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish | MobKind::Pufferfish | MobKind::Turtle => Voice::Fish,
        MobKind::Squid | MobKind::GlowSquid => Voice::Squid,
        MobKind::Dolphin => Voice::Dolphin,
        MobKind::Axolotl => Voice::Axolotl,
        MobKind::Guardian | MobKind::ElderGuardian => Voice::Guardian,
        MobKind::Cat => Voice::Cat,
        MobKind::Pillager => Voice::Pillager,
        MobKind::Wolf => Voice::Wolf,
        MobKind::Fox => Voice::Fox,
        MobKind::Parrot => Voice::Parrot,
        MobKind::Bee => Voice::Bee,
        MobKind::Rabbit => Voice::Rabbit,
        MobKind::Goat => Voice::Goat,
    }
}

impl Game {
    /// Blows a hole in the world and hurts everything around `center`.
    /// `cause` completes the death message, as for [`Game::damage_player`].
    pub(super) fn explode(&mut self, center: DVec3, power: f32, cause: &str, credit_player: bool) {
        // `ServerLevel.explode`: the interaction decides whether blocks break
        // and whether their drops decay.
        let blast = self.gamerules.explosion(voxelcraft::rules::ExplosionSource::of_cause(cause));
        // `ServerExplosion.explode`: find the cells, hurt entities while the
        // walls about to go still shield them, then break the blocks.
        let cells = if blast.destroys_blocks { self.world.exploded_cells(center, power) } else { Vec::new() };
        self.mobs.entities.explode_with(&self.world, center, power, credit_player, blast.destroys_blocks);
        if let Some((damage, push)) = self.session.blast(&self.world, center, power) {
            let toward = Some(center - self.session.player.pos);
            if self.damage_player_from(self.difficulty.mob_damage(damage), cause, toward) > 0.0 {
                self.session.player.hurt_from(push);
            }
            self.session.player.vel += push;
        }
        if self.arrival.is_none() {
            for bot in self.agents.players.values_mut().filter(|b| b.active) {
                if let Some((damage, push)) = bot.agent.session.blast(&self.world, center, power) {
                    bot.agent.hurt(self.difficulty.mob_damage(damage), cause, DVec3::ZERO, &mut self.mobs.entities);
                    bot.agent.session.player.vel += push;
                }
            }
        }
        if blast.destroys_blocks {
            let fire = self.gamerules.explosion_fire(cause);
            self.world.detonate(&cells, power, self.gamerules.bool("block_drops"), blast.drop_decay, fire);
        }
        // TNT caught in the blast goes off soon after.
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse);
        }
        self.audio.play(Sound::Explosion, Some(center), 1.0, (0.9, 1.05));
    }

    /// F3 line with entity counts.
    pub(super) fn mobs_debug_line(&self) -> String {
        let e = &self.mobs.entities;
        let (passive, hostile): (Vec<_>, Vec<_>) = MobKind::ALL.iter().partition(|k| !k.is_hostile());
        let total = |kinds: Vec<&MobKind>| kinds.into_iter().map(|&k| e.count(k)).sum::<usize>();
        format!(
            "Entities: {} (passive: {}, hostile: {}, arrows: {}, items: {}), {} rendered, {} falling blocks, \
             {} stored in {} unloaded chunks",
            e.mobs.len(),
            total(passive),
            total(hostile),
            e.arrows.len(),
            e.items.len(),
            e.rendered,
            self.world.falling_blocks().len(),
            e.stored_entities() + self.world.stored_falling_blocks(),
            e.stored_chunks()
        )
    }
}
