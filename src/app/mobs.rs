//! Game-side glue for mobs: updating them, player melee, entity events and
//! the `--spawn` / `--wait` debug options.

use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::audio::SoundSource;
use crate::audio::sounds::{Call, Sound, Voice};
use crate::entity::{self, Entities, EntityEvent, MobKind, MobSound, PlayerId, Target};
use crate::physics;
use crate::simulation::damage::Cause;

use super::Game;

pub(super) struct Mobs {
    pub entities: Entities,
    /// The current left-button hold started on a mob: don't break blocks
    /// until it's released.
    /// `--spawn` requests, applied once the world has loaded.
    pending: Vec<(MobKind, IVec3, Option<crate::entity::armor::Equipped>)>,
    /// `--spawn allay,x,y,z,item`: liked items by index into `pending`
    /// (main-hand items for other mobs).
    held: Vec<(usize, crate::item::Item)>,
    /// `--spawn kind,x,y,z,baby`: babies by index into `pending`.
    babies: Vec<usize>,
    /// v1.0 gap Track 7e: `type[:profession[:level]]` of spawned villagers.
    villagers: Vec<(usize, String)>,
    /// `--wait`: seconds to keep simulating before a `--screenshot`.
    wait: f64,
    wait_start: Option<Instant>,
    /// `--still-mobs` (see `Entities::still`).
    still: bool,
    /// v1.0 gap Track 8d: `--burning`.
    burning: bool,
    /// Sign text laid out once and redrawn from the cache each frame.
    pub sign_text: crate::render::signs::TextCache,
}

impl Mobs {
    pub fn new(seed: u64, spawn: Vec<(MobKind, IVec3, Option<crate::entity::armor::Equipped>)>, wait: f64) -> Self {
        Self {
            entities: Entities::new(seed),
            pending: spawn,
            held: Vec::new(),
            babies: Vec::new(),
            villagers: Vec::new(),
            wait,
            wait_start: None,
            sign_text: Default::default(),
            still: false,
            burning: false,
        }
    }

    pub fn holding(self, held: Vec<(usize, crate::item::Item)>) -> Self {
        Self { held, ..self }
    }

    pub fn babies(self, babies: Vec<usize>) -> Self {
        Self { babies, ..self }
    }

    pub fn villagers(self, villagers: Vec<(usize, String)>) -> Self {
        Self { villagers, ..self }
    }

    pub fn still(self, still: bool) -> Self {
        Self { still, ..self }
    }

    /// v1.0 gap Track 8d: `--burning`: still mobs hold still on fire.
    pub fn burning(self, burning: bool) -> Self {
        Self { burning, ..self }
    }

    /// Whether `--still-mobs` pins this capture's mobs.
    pub fn still_mobs(&self) -> bool {
        self.still
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
        if self.mobs.still {
            self.mobs.entities.still = Some(self.session.player.eye());
            self.mobs.entities.still_burning = self.mobs.burning;
        }
        let held = std::mem::take(&mut self.mobs.held);
        let babies = std::mem::take(&mut self.mobs.babies);
        let villagers = std::mem::take(&mut self.mobs.villagers);
        for (i, (kind, mut pos, armor)) in std::mem::take(&mut self.mobs.pending).into_iter().enumerate() {
            if pos.y == i32::MIN {
                pos.y = self.world.generator.column(pos.x, pos.z).height + 1;
            }
            self.mobs.entities.spawn(kind, pos.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
            // v0.18 Track 6: a pinned mob placed standing on a full block is
            // held from its first tick, before it can hop, swing or wander.
            if self.mobs.still
                && crate::physics::full_collision(&self.world, pos - IVec3::Y)
                && let Some(mob) = self.mobs.entities.mobs.last_mut()
            {
                mob.on_ground = true;
            }
            if let Some(equipped) = armor
                && let Some(mob) = self.mobs.entities.mobs.last_mut()
            {
                mob.equipment.wear_set(equipped.kind, equipped.glint);
            }
            if let Some(&(_, item)) = held.iter().find(|(at, _)| *at == i)
                && let Some(mob) = self.mobs.entities.mobs.last_mut()
            {
                let stack = crate::inventory::Stack::new(item, 1);
                match mob.allay.as_mut() {
                    Some(allay) => allay.liked = Some(stack),
                    None => mob.equipment.set(crate::entity::equipment::Slot::MainHand, Some(stack)),
                }
            }
            // v1.0 gap Track 7e: a dressed villager for the outfit scenes.
            if let Some((_, spec)) = villagers.iter().find(|(at, _)| *at == i)
                && let Some(v) = self.mobs.entities.mobs.last_mut().and_then(|m| m.villager.as_mut())
            {
                let mut fields = spec.split(':');
                v.map_variant = fields.next().map(str::to_owned);
                if let Some(p) = fields.next() {
                    let p = entity::villager::Profession::ALL.into_iter().find(|j| j.name().eq_ignore_ascii_case(p));
                    v.set_profession(p.unwrap_or_default());
                }
                v.level = fields.next().and_then(|l| l.parse().ok()).unwrap_or(v.level).clamp(1, 5);
            }
            // v1.0 gap Track 7b: a baby for the capture scenes' baby models.
            if babies.contains(&i)
                && let Some(mob) = self.mobs.entities.mobs.last_mut()
            {
                // Animals take their baby age from this on their first tick.
                mob.baby = true;
            }
        }
    }

    /// What the entities see of the players and the world this update.
    pub(super) fn entity_ctx(&self) -> entity::Ctx {
        let mut players = vec![Target {
            invulnerable: self.session.mode.invulnerable(),
            last_hurt: self.session.vitals.cooling_down(),
            difficulty: self.difficulty,
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
            armor_pieces: self.session.inventory.armor.iter().flatten().count() as u8,
            head: self.session.inventory.armor[0].and_then(|s| crate::world::skulls::kind_of_item(s.item)),
            sneaking: self.session.player.sneaking,
            darkness_ticks: self
                .session
                .vitals
                .effects
                .get(voxelcraft::simulation::effects::Effect::Darkness)
                .map_or(0, |e| e.ticks),
            in_water: self.session.player.in_water,
            swimming: self.session.player.swimming,
            time_since_rest: self.session.vitals.time_since_rest,
            sprinting: self.session.player.sprinting,
            blocking: self.session.blocking(),
            fall_flying: self.session.player.fall_flying,
            passenger: self.session.player.vehicle.is_some(),
            vel: self.session.player.vel,
            ..Target::new(
                PlayerId::HOST,
                self.session.player.pos,
                self.session.mode.targetable() && !self.session.vitals.is_dead(),
            )
        }];
        // v1.0 gap Track 8e: a player watching the End credits has left the
        // level (`ServerPlayer.showEndCredits`: `removePlayerImmediately`),
        // so no mob, projectile or spawn rule finds them.
        if self.credits.is_some() {
            players.clear();
        }
        // Agents keep source-dimension positions until arrival relocates them.
        if self.arrival.is_none() {
            players.extend(self.agents.targets());
        }
        for p in &mut players {
            p.difficulty = self.difficulty;
        }
        entity::Ctx {
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
        }
    }

    pub(super) fn update_mobs(&mut self, dt: f64) {
        // Entities leave the level with their chunk and return when it loads.
        self.mobs.entities.sync_chunk_entities(&mut self.world);
        // Return gateways generated since: new block entities, age 0.
        for g in std::mem::take(&mut self.world.generated_gateways) {
            if let Some(fight) = &mut self.mobs.entities.fight {
                fight.gateway_entities.placed(g);
            }
        }
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
        // v0.15 Track E: the `raids` and `spawn_patrols` rules.
        self.mobs.entities.raids.enabled = self.gamerules.bool("raids");
        self.mobs.entities.raids.patrols = self.gamerules.bool("spawn_patrols");
        self.mobs.entities.monster_spawning = self.gamerules.bool("spawn_monsters");
        self.mobs.entities.spawner_blocks = self.gamerules.bool("spawner_blocks_work");
        self.mobs.entities.villager_griefing = self.gamerules.bool("mob_griefing");
        self.mobs.entities.max_entity_cramming = self.gamerules.int("max_entity_cramming");
        self.mobs.entities.mob_loot = self.gamerules.bool("mob_drops");
        self.mobs.entities.neutral_forgive_dead_players = self.gamerules.bool("forgive_dead_players");
        self.mobs.entities.thundering = self.dimension.has_sky() && self.weather.thunder_level() > 0.9;
        self.mobs.entities.neutral_universal_anger = self.gamerules.bool("universal_anger");
        self.mobs.entities.village_time = self.day_time;
        self.mobs.entities.village_day = self.day_count;
        // v1.0 gap Track 2d: each merchant's customer (`tradingPlayer`): the
        // host, a controller player or a LAN player with its screen open.
        let mut customers: Vec<(u64, crate::entity::PlayerId)> = Vec::new();
        if self.inventory_open
            && let super::Container::Trading(id) = self.body.container
        {
            customers.push((id, crate::entity::PlayerId::HOST));
        }
        let agent = |name: &str| self.agents.players.get(name).map(|b| b.agent.session.id);
        customers.extend(self.pads.trading_seats().filter_map(|(id, name)| Some((id, agent(name)?))));
        customers.extend(self.lan_trading().filter_map(|(id, name)| Some((id, agent(name)?))));
        for m in &mut self.mobs.entities.mobs {
            if let Some(v) = &mut m.villager {
                v.customer = customers.iter().find(|c| c.0 == v.id).map(|c| c.1);
                v.trading = v.customer.is_some() || self.pads.trading(v.id);
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
        // Fire, redstone and dispensers light TNT for no one (`TntBlock.prime`
        // with no source, `DispenseItemBehavior`'s `PrimedTnt(.., null)`).
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse, None);
            self.audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.95, 1.05));
        }
        // v0.16 Track E: silverfish out of broken infested blocks.
        self.mobs.entities.release_silverfish(&mut self.world.released_silverfish);
        crate::entity::golem::finish_golems(&mut self.world, &mut self.mobs.entities);
        self.finish_withers();
        crate::entity::frog::finish_hatches(&mut self.world, &mut self.mobs.entities);
        for (cell, count) in std::mem::take(&mut self.world.hatched_turtles) {
            self.mobs.entities.hatch_turtles(cell, count);
        }
        let ctx = self.entity_ctx();
        self.world.set_bee_clock(self.dimension, (self.day_time.rem_euclid(1.) * 24000.).floor() as u32);
        self.mobs.entities.tick_lightning(dt, &mut self.world, &ctx, self.weather.is_thundering());
        let mut smashed = Vec::new();
        let mut arrow_effects = crate::entity::ArrowEffectGate::default();
        for event in self.mobs.entities.update_world(dt, &mut self.world, &ctx, self.difficulty) {
            match event {
                EntityEvent::ArrowEffectsStart { player } => arrow_effects.begin(player),
                EntityEvent::ArrowEffectsEnd => arrow_effects.end(),
                EntityEvent::ProjectileBlockHit { cell, pos, normal, arrow, burning, mob_owned, by } => {
                    self.world.hit_redstone_target(cell, pos, normal, arrow);
                    self.world.touch_redstone_ore(cell);
                    if burning {
                        self.world.light_candle(cell);
                        self.burning_projectile_hit(cell, mob_owned, by);
                    }
                    // v0.16 Track A: `ChorusFlowerBlock.onProjectileHit`: a
                    // projectile that may break blocks breaks the flower.
                    if self.gamerules.bool("projectiles_can_break_blocks") {
                        self.world.chorus_flower_shot(cell);
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
                        } else if self.session.vitals.last_hit_fresh() {
                            // Only a full hit takes `hurtServer`'s push.
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
                // v1.0 gap Track 2b: a mob's spear contact that knocks back
                // or dismounts without (or besides) damage.
                EntityEvent::PlayerShove { player, pushes } => {
                    let entities = &mut self.mobs.entities;
                    if player == PlayerId::HOST {
                        self.session.shoved(pushes, &mut || f64::from(entities.roll_range(0.0, 1.0)));
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.session.shoved(pushes, &mut || f64::from(entities.roll_range(0.0, 1.0)));
                    }
                }
                EntityEvent::PlayerDismount { player } => {
                    if player == PlayerId::HOST {
                        if self.session.player.vehicle.take().is_some()
                            && let Some(pos) = self.mobs.entities.dismount(&self.world, PlayerId::HOST)
                        {
                            self.session.player.pos = pos;
                            self.session.player.vel = DVec3::ZERO;
                        }
                    } else if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.player.vehicle.take().is_some()
                        && let Some(pos) = self.mobs.entities.dismount(&self.world, player)
                    {
                        bot.agent.session.player.pos = pos;
                        bot.agent.session.player.vel = DVec3::ZERO;
                    }
                }
                EntityEvent::PlayerSonicBoom { player: PlayerId::HOST, damage, push, from } => {
                    // `SonicBoom.tick`: unarmoured, unenchanted damage that a
                    // raised shield facing the warden still stops, then the push.
                    let damage = self.difficulty.mob_damage(damage);
                    let toward = from - self.session.player.pos;
                    let slot = self.body.actions.selected;
                    if !self.session.shield_intercept(slot, damage, toward).blocked
                        && self.damage_player(damage, Cause::SONIC_BOOM) > 0.0
                    {
                        let resistance = self.session.inventory.knockback_resistance();
                        self.session.player.vel += crate::simulation::survival::knockback_taken(
                            push.as_dvec3() * crate::knockback::TICK_RATE,
                            resistance,
                        );
                    }
                }
                EntityEvent::PlayerSonicBoom { player, damage, push, from } => {
                    if let Some(bot) = self.agents.by_id_mut(player) {
                        let damage = self.difficulty.mob_damage(damage);
                        bot.agent.hurt_sonic_boom(damage, push.as_dvec3(), from, &mut self.mobs.entities);
                    }
                }
                EntityEvent::PlayerLightning { player } => {
                    if player == PlayerId::HOST {
                        self.damage_player_armored(5., Cause::LIGHTNING);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt(5., Cause::LIGHTNING, DVec3::ZERO, &mut self.mobs.entities);
                    }
                }
                // v0.16 Track B: a held rocket pushes a gliding player.
                EntityEvent::RocketBoost { player } => {
                    if player == PlayerId::HOST {
                        self.session.player.rocket_boost();
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.session.player.rocket_boost();
                    }
                }
                // v0.16 Track B: `fireworks` damage, never scaled by
                // difficulty, with the rocket's knockback toward it.
                EntityEvent::PlayerFirework { player, damage, from } => {
                    let push = |victim: DVec3| crate::knockback::Hit {
                        pushes: [Some(crate::knockback::Push::hurt(victim, from)), None],
                        punch: None,
                        throw: None,
                    };
                    if player == PlayerId::HOST {
                        let hit = push(self.session.player.pos);
                        let toward = from - self.session.player.pos;
                        if self.damage_player_from(damage, Cause::FIREWORKS, Some(toward)) > 0.0 {
                            let entities = &mut self.mobs.entities;
                            self.session.knocked_back(&hit, &mut || f64::from(entities.roll_range(0.0, 1.0)));
                        }
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        let hit = push(bot.agent.session.player.pos);
                        bot.agent.hurt_knocked(damage, Cause::FIREWORKS, &hit, &mut self.mobs.entities);
                    }
                }
                // Its sounds and particles came from the entity pass.
                EntityEvent::FireworkBurst { .. } => {}
                EntityEvent::PlayerFall { player, damage } => {
                    if player == PlayerId::HOST {
                        self.damage_player(damage, Cause::FALL);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt_fall(damage, &mut self.mobs.entities);
                    }
                }
                EntityEvent::PlayerBeam { player, physical, magic } => {
                    let physical = self.difficulty.mob_damage(physical);
                    if player == PlayerId::HOST {
                        // v1.0 gap Track 8d: `GuardianAttackGoal.tick`'s two
                        // hits, `indirectMagic` then the attack, which the
                        // hurt cooldown lets in only by its excess.
                        self.damage_player(magic, Cause::GUARDIAN_MAGIC);
                        self.damage_player_armored(physical, Cause::GUARDIAN_BEAM);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt_beam(physical, magic, &mut self.mobs.entities);
                    }
                }
                EntityEvent::BeeSting { uid, player, poison } => {
                    let source = self.mobs.entities.mobs.iter().find(|m| m.uid == uid).map(|m| m.pos);
                    let damage = self.difficulty.mob_damage(2.);
                    let applied = if player == PlayerId::HOST {
                        let toward = source.map(|p| p - self.session.player.pos);
                        let hit = self.damage_player_from(damage, Cause::BEE_STING, toward) > 0.;
                        if hit && poison > 0 {
                            self.session.vitals.apply_effect(crate::simulation::effects::Effect::Poison, 0, poison);
                        }
                        hit
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        let toward = source.map(|p| p - bot.agent.session.player.pos);
                        let blocked = toward
                            .is_some_and(|d| bot.agent.session.shield_intercept(bot.agent.selected, damage, d).blocked);
                        let hit = !blocked
                            && bot.agent.hurt(damage, Cause::BEE_STING, DVec3::ZERO, &mut self.mobs.entities) > 0.;
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
                    if arrow_effects.allows()
                        && self.session.mode.is_survival()
                        && !elder_curse_held(&self.session.vitals.effects, effect, amplifier, ticks)
                    {
                        let damage = self.session.vitals.apply_effect(effect, amplifier, ticks);
                        if damage > 0.0 {
                            self.damage_player(damage, Cause::MAGIC);
                        }
                    }
                }
                EntityEvent::PlayerEffect { player, effect, amplifier, ticks } => {
                    if arrow_effects.allows()
                        && let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.mode.is_survival()
                        && !elder_curse_held(&bot.agent.session.vitals.effects, effect, amplifier, ticks)
                    {
                        let damage = bot.agent.session.vitals.apply_effect(effect, amplifier, ticks);
                        if damage > 0.0 {
                            bot.agent.hurt(damage, Cause::MAGIC, DVec3::ZERO, &mut self.mobs.entities);
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
                EntityEvent::PlayerMagic { player: PlayerId::HOST, amount, cause } => {
                    if arrow_effects.allows() && self.session.mode.is_survival() {
                        if amount > 0.0 {
                            self.damage_player(amount, cause);
                        } else {
                            self.session.vitals.health =
                                (self.session.vitals.health - amount).min(self.session.vitals.max_health());
                        }
                    }
                }
                EntityEvent::PlayerMagic { player, amount, cause } => {
                    if arrow_effects.allows()
                        && let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.mode.is_survival()
                    {
                        if amount > 0.0 {
                            bot.agent.hurt_magic_by(amount, cause, &mut self.mobs.entities);
                        } else {
                            bot.agent.session.vitals.health =
                                (bot.agent.session.vitals.health - amount).min(bot.agent.session.vitals.max_health());
                        }
                    }
                }
                EntityEvent::Explosion { center, power, cause, credit_player } => {
                    self.explosion_event(center, power, cause, credit_player)
                }
                EntityEvent::CartExplosion { center, power, spares_rails } => {
                    self.world.emit_game_event(
                        center,
                        crate::world::vibration::GameEvent::Explode,
                        crate::world::vibration::EventSource::NONE,
                        None,
                    );
                    self.explode_blocks(center, power, Cause::TNT, false, true, spares_rails);
                }
                EntityEvent::WindBurst(burst) => self.wind_burst(&burst),
                // v0.16 Track C: the Wither's blasts, broken blocks and roses.
                EntityEvent::WitherBlast { center, power, dangerous, owner } => {
                    self.wither_explode(center, power, dangerous, owner)
                }
                EntityEvent::WitherBreak { min, max, pos } => self.wither_break(min, max, pos),
                EntityEvent::WitherRose { pos } => self.wither_rose(pos),
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
                EntityEvent::IgniteBlock { cell, mob_owned } => {
                    if !mob_owned || self.gamerules.bool("mob_griefing") {
                        self.world.ignite(cell);
                    }
                }
                EntityEvent::Fireball { .. }
                | EntityEvent::ChargedBlast { .. }
                | EntityEvent::MobArrow { .. }
                | EntityEvent::ThrowPotion { .. }
                | EntityEvent::ThrowTrident { .. }
                | EntityEvent::ShootWindCharge { .. } => {}
                EntityEvent::MobGrief { cell, block, drops } => {
                    // `destroyBlock(pos, true, ravager)` or the door goal's
                    // `removeBlock`, both under `mobGriefing`.
                    if self.gamerules.bool("mob_griefing") && self.world.get_block(cell) == Some(block) {
                        if drops {
                            self.world.spill_block(cell, block);
                        }
                        self.world.set_block(cell, crate::world::block::Block::AIR);
                        self.audio.block_break(block, cell);
                    }
                }
                EntityEvent::DoorBroken { cell } => {
                    // v1.0 gap Track 2: `BreakDoorGoal.tick` at 240. The goal
                    // checked `mobGriefing` when it started, so it isn't checked
                    // again. `removeBlock` takes the cell silently; the door's
                    // other half loses its support and `destroyBlock`s with
                    // particles, its break sound and its loot (the door item from
                    // a lower half); then `levelEvent(1021)`.
                    use crate::world::block::{Block, Shaped};
                    let door = |b: Option<Block>| b.filter(|b| b.is_door());
                    if door(self.world.get_block(cell)).is_some() {
                        self.world.set_block(cell, Block::AIR);
                        for other in [cell + IVec3::Y, cell - IVec3::Y] {
                            let Some(block) = door(self.world.get_block(other)) else { continue };
                            if matches!(block.shaped(), Some(Shaped::Door { upper: false, .. })) {
                                self.world.spill_block(other, block);
                            }
                            self.world.set_block(other, Block::AIR);
                            self.world.particles.push(crate::particles::Request::Break { cell: other, block });
                            self.audio.block_break(block, other);
                            let none = crate::world::vibration::EventSource::NONE;
                            let destroy = crate::world::vibration::GameEvent::BlockDestroy;
                            self.world.emit_block_event(other, destroy, none, Some(block));
                        }
                    }
                    let sound = Sound::Illager(voxelcraft::entity::illager::Cue::DoorBreak);
                    let at = cell.as_dvec3() + DVec3::splat(0.5);
                    self.audio.play_in(SoundSource::Hostile, sound, Some(at), 2.0, (0.8, 1.2));
                }
                EntityEvent::TridentBrokeDripstone { cell, mob_owned } => {
                    // `Projectile.mayInteract` (a mob's needs `mobGriefing`) and `mayBreak`.
                    let may = self.gamerules.bool("projectiles_can_break_blocks")
                        && (!mob_owned || self.gamerules.bool("mob_griefing"));
                    if may && let Some(block) = self.world.trident_breaks_dripstone(cell) {
                        self.audio.block_break(block, cell);
                    }
                }
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
                        MobSound::DecorBroken(kind) => {
                            let effect = voxelcraft::entity::decor::Effect::Broken(kind);
                            super::utilities::decor_feedback(&mut self.audio, pos, effect);
                            continue;
                        }
                        MobSound::Allay(action) => (Sound::Allay(action), 0.8),
                        MobSound::Warden(cue) => (Sound::Warden(cue), warden_volume(cue)),
                        MobSound::Hurt(MobKind::Warden) => (Sound::Warden(voxelcraft::entity::warden::Cue::Hurt), 1.0),
                        MobSound::Death(MobKind::Warden) => {
                            (Sound::Warden(voxelcraft::entity::warden::Cue::Death), 1.0)
                        }
                        MobSound::Weapon(cue) => {
                            super::items::weapon_sound(&mut self.audio, cue, pos);
                            continue;
                        }
                        MobSound::Breeze(cue) => (Sound::Breeze(cue), 1.0),
                        // `levelEvent(1019)` plays at volume 2.
                        MobSound::Illager(cue @ voxelcraft::entity::illager::Cue::DoorAttack) => {
                            (Sound::Illager(cue), 2.0)
                        }
                        MobSound::Illager(cue) => (Sound::Illager(cue), 1.0),
                        MobSound::Raid(cue) => (Sound::Raid(cue), 1.0),
                        MobSound::Flight(cue) => (Sound::Flight(self.far_firework(cue, pos)), 1.0),
                        // `globalLevelEvent(1023)`: the whole dimension hears the Wither arrive.
                        MobSound::Wither(voxelcraft::entity::wither::Cue::Spawn) => {
                            let sound = Sound::Wither(voxelcraft::entity::wither::Cue::Spawn);
                            self.audio.play_in(SoundSource::Hostile, sound, None, 1.0, (1.0, 1.0));
                            continue;
                        }
                        MobSound::Wither(cue) => (Sound::Wither(cue), 1.0),
                        // v0.16 Track A: the nearest shared sounds for the shulker's cues.
                        MobSound::Shulker(cue) => {
                            use voxelcraft::entity::shulker::Cue;
                            match cue {
                                Cue::Open => (Sound::Door(true), 0.6),
                                Cue::Close => (Sound::Door(false), 0.6),
                                Cue::Shoot => (Sound::Bow, 1.0),
                                Cue::Teleport => (Sound::Teleport, 1.0),
                                Cue::BulletHit | Cue::BulletHurt => (Sound::Hit, 1.0),
                            }
                        }
                        MobSound::SulfurCube(cue) => (Sound::SulfurCube(cue), 0.8),
                        // v1.0 gap Track 3b: `makeSound` at volume 1.
                        MobSound::Mount(cue) => (Sound::Mount(cue), 1.0),
                        // v1.0 gap Track 2f: the fox's own sounds; its night
                        // screech plays at volume 2.
                        MobSound::Fox(cue) => {
                            use voxelcraft::entity::fox::Cue;
                            let ambient = 0.7 * mob_volume(MobKind::Fox);
                            let volume = match cue {
                                Cue::Sleep => ambient,
                                Cue::Screech => 2.0 * ambient,
                                _ => 1.0,
                            };
                            (Sound::Fox(cue), volume)
                        }
                        // v1.0 gap Track 2g: the launch at volume 1, the
                        // landing's step at 2.
                        MobSound::Goat(cue) => {
                            let landing = cue == voxelcraft::entity::animals::long_jump::Cue::Step;
                            (Sound::Goat(cue), if landing { 2.0 } else { 1.0 })
                        }
                        // v1.0 gap Track 2h: `makeSound` at the villager's
                        // volume, 1.
                        MobSound::VillagerWork(job) => (Sound::VillagerWork(job), 1.0),
                        // v1.0 gap Track 2h: the game's chewing stand-in for
                        // `PANDA_EAT` (Java's volume is 0.5 or 1).
                        MobSound::PandaEat => (Sound::Step(crate::audio::sounds::Material::Snow), 0.75),
                        MobSound::CopperGolem(action, stage) => (Sound::CopperGolem(action, stage), 0.7),
                        MobSound::Hurt(MobKind::CopperGolem) => {
                            (Sound::CopperGolem(voxelcraft::entity::copper_golem::Sound::Hurt, 0), 0.9)
                        }
                        MobSound::Death(MobKind::CopperGolem) => {
                            (Sound::CopperGolem(voxelcraft::entity::copper_golem::Sound::Death, 0), 0.9)
                        }
                        MobSound::Sniffer(cue) => {
                            use voxelcraft::entity::sniffer::Cue;
                            (Sound::Sniffer(cue), if matches!(cue, Cue::EggCrack | Cue::EggHatch) { 0.7 } else { 1.0 })
                        }
                        // `emitDiggingParticles`: the block's hit sound at volume and pitch 0.5.
                        MobSound::SnifferDigHit(block) => {
                            // v0.18 Track 9: the block's own hit sound.
                            let hit = crate::audio::block_sounds::play(block, crate::audio::BlockCue::Hit);
                            if let Some((sound, ..)) = hit {
                                self.audio.play_in(SoundSource::Neutral, sound, Some(pos), 0.5, (0.5, 0.5));
                            }
                            continue;
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
                        // v1.0 gap Track 8f: `makeSound(VILLAGER_NO)` at volume 1
                        // and the voice pitch, `(r - r) * 0.2 + 1` (1.5 for a child).
                        MobSound::VillagerNo(baby) => {
                            let pitch = if baby { (1.3, 1.7) } else { (0.8, 1.2) };
                            self.audio.play_in(SoundSource::Neutral, Sound::VillagerNo, Some(pos), 1.0, pitch);
                            continue;
                        }
                        MobSound::Ambient(kind) => (Sound::Mob(voice(kind), Call::Ambient), 0.7 * mob_volume(kind)),
                        MobSound::Hurt(kind) => (Sound::Mob(voice(kind), Call::Hurt), 0.9 * mob_volume(kind)),
                        MobSound::Death(kind) => (Sound::Mob(voice(kind), Call::Death), 0.9 * mob_volume(kind)),
                        MobSound::Scream => (Sound::Scream, 1.0),
                        MobSound::Teleport => (Sound::Teleport, 0.8),
                        MobSound::Fireball => (Sound::Fireball, 0.9),
                        MobSound::EyeDeath => (Sound::EyeDeath, 0.8),
                        MobSound::DragonFlap => (Sound::DragonFlap, 1.0),
                        MobSound::DragonGrowl => (Sound::DragonGrowl, 1.0),
                        MobSound::DragonRoar => {
                            self.audio.play_in(SoundSource::Hostile, Sound::DragonGrowl, Some(pos), 2.5, (0.8, 1.1));
                            continue;
                        }
                        MobSound::DragonShoot => (Sound::Fireball, 1.0),
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
                | EntityEvent::MobStab(_)
                | EntityEvent::OwnerHurt { .. }
                | EntityEvent::DragonXp { .. }
                | EntityEvent::MobKilled { .. }
                | EntityEvent::LaidEgg { .. }
                | EntityEvent::Hatched { .. }
                | EntityEvent::ExperienceBurst { .. }
                | EntityEvent::DropEquipment { .. }
                | EntityEvent::ThrowItem { .. }
                | EntityEvent::ItemParticle { .. }
                // Applied by the entities within their tick.
                | EntityEvent::DragonStruck { .. }
                | EntityEvent::DragonHealed { .. }
                | EntityEvent::Vibration { .. } => {}
                EntityEvent::AnimalBlock { cell, from, to } => {
                    self.world.animal_block(cell, from, to);
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
                        // `HarvestFarmland`: `destroyBlock` (break particles
                        // and sound, the drops for the farmer to pick up);
                        // v1.0 gap Track 2g: the farmer replants from its seeds.
                        self.world.spill_block(cell, block);
                        self.world.set_block(cell, crate::world::block::Block::AIR);
                        self.world.particles.push(crate::particles::Request::Break { cell, block });
                        self.audio.block_break(block, cell);
                        let none = crate::world::vibration::EventSource::NONE;
                        let destroy = crate::world::vibration::GameEvent::BlockDestroy;
                        self.world.emit_block_event(cell, destroy, none, Some(block));
                    }
                }
                EntityEvent::VillagerPlant { cell, block } => {
                    if self.world.get_block(cell) == Some(crate::world::block::Block::AIR)
                        && self.world.get_block(cell - glam::IVec3::Y).is_some_and(|b| b.is_farmland())
                        && self.world.set_block(cell, block)
                    {
                        // `setBlockAndUpdate`, `block_place`, `item.crop.plant`.
                        let none = crate::world::vibration::EventSource::NONE;
                        let place = crate::world::vibration::GameEvent::BlockPlace;
                        self.world.emit_block_event(cell, place, none, Some(block));
                        self.audio.block_place(block, cell);
                    }
                }
                EntityEvent::VillagerBonemeal { cell } => {
                    // `BoneMealItem.growCrop` and level event 1505 (v1.0
                    // gap Track 2h: its particles and sound are the world's).
                    self.world.use_bone_meal(cell);
                }
                EntityEvent::VillagerCompost { cell, from, to } => self.world.villager_compost(cell, from, to),
                EntityEvent::RingBell { pos, mob } => {
                    self.world.ring_bell_by(pos, crate::world::vibration::EventSource::mob(mob));
                }
                EntityEvent::VillagerDoorClose { cell } => {
                    if self.world.get_block(cell).is_some_and(|b| {
                        matches!(b.shaped(), Some(crate::world::block::Shaped::Door { open: true, upper: false, .. }))
                    }) {
                        self.toggle_door(cell, glam::Vec3::ZERO);
                    }
                }
                EntityEvent::BreakBlock { cell } => smashed.push(cell),
                // Level event 2008 (`LevelEventHandler`): one explosion particle.
                // v1.0 gap Track 2f: level event 2001 at a block that stays.
                EntityEvent::DestroyEffect { cell } => {
                    if let Some(block) = self.world.get_block(cell).filter(|&b| b != crate::world::block::Block::AIR) {
                        self.world.particles.push(crate::particles::Request::Break { cell, block });
                        self.audio.block_break(block, cell);
                    }
                }
                EntityEvent::DragonSmashed { cell } => {
                    let pos = cell.as_dvec3() + DVec3::splat(0.5);
                    self.world.particles.push(crate::particles::Request::Explosion { pos, large: false });
                }
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
                // v0.16 Track G: End crystals and the dragon respawn ritual.
                EntityEvent::CrystalFire { cell } => {
                    self.world.crystal_fire(cell);
                }
                EntityEvent::ExitPortal { active } => self.world.set_exit_portal(active, false),
                EntityEvent::RespawnPillar { index } => self.respawn_pillar(index),
                EntityEvent::QuietBlast { center, power } => {
                    self.world.emit_game_event(
                        center,
                        crate::world::vibration::GameEvent::Explode,
                        crate::world::vibration::EventSource::NONE,
                        None,
                    );
                    self.explode_with(center, power, Cause::END_CRYSTAL, false, false);
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
        // `AbstractVillager` and boats: `NEUTRAL`.
        MobSound::TraderDrink(_) | MobSound::BoatPaddle(_) => Some(SoundSource::Neutral),
        // v1.0 gap Track 2h: the panda's `playSound`, `NEUTRAL`.
        MobSound::PandaEat => Some(SoundSource::Neutral),
        MobSound::Thunder => Some(SoundSource::Weather),
        // Skeletons and pillagers shoot as hostile mobs, not as the player.
        MobSound::Bow => Some(SoundSource::Hostile),
        MobSound::Shulker(_) => Some(SoundSource::Hostile),
        // The snow golem borrows the slime's voice but is a friendly mob.
        MobSound::Ambient(MobKind::SnowGolem)
        | MobSound::Hurt(MobKind::SnowGolem)
        | MobSound::Death(MobKind::SnowGolem) => Some(SoundSource::Neutral),
        _ => None,
    }
}

/// Relative loudness of the warden's calls: Java plays most at volume 5
/// (10 for listening, steps and attacks, 3 for roars and booms); the mixer's
/// gain stays within 1 and the distance falloff does the rest.
fn warden_volume(cue: voxelcraft::entity::warden::Cue) -> f32 {
    use voxelcraft::entity::warden::Cue;
    match cue {
        Cue::Heartbeat | Cue::TendrilClicks => 0.8,
        Cue::Step => 0.7,
        _ => 1.0,
    }
}

// Java Bat.getSoundVolume; samples stay normalized like the other voices.
fn mob_volume(kind: MobKind) -> f32 {
    if kind == MobKind::Bat { 0.1 } else { 1.0 }
}

/// v0.16 Track C: `NOTE_BLOCK_IMITATE_*`, the sound a note block plays with
/// a mob head on it: the mob's own ambient call (the dragon's growl).
pub(super) fn head_note(i: crate::world::gadgets::Instrument) -> Option<Sound> {
    use crate::world::gadgets::Instrument as I;
    let kind = match i {
        I::Zombie => MobKind::Zombie,
        I::Skeleton => MobKind::Skeleton,
        I::Creeper => MobKind::Creeper,
        I::WitherSkeleton => MobKind::WitherSkeleton,
        I::Piglin => MobKind::Piglin,
        I::Dragon => return Some(Sound::DragonGrowl),
        _ => return None,
    };
    Some(Sound::Mob(voice(kind), Call::Ambient))
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
        MobKind::Sniffer => Voice::Sniffer,
        // The warden's own calls are `Sound::Warden`; this voice is unused.
        MobKind::Warden => Voice::Creaking,
        MobKind::Breeze => Voice::Breeze,
        MobKind::Endermite => Voice::Endermite,
        MobKind::Vindicator => Voice::Vindicator,
        MobKind::Evoker => Voice::Evoker,
        MobKind::Vex => Voice::Vex,
        MobKind::Ravager => Voice::Ravager,
        MobKind::Illusioner => Voice::Illusioner,
        MobKind::HappyGhast => Voice::HappyGhast,
        MobKind::Nautilus => Voice::Nautilus,
        MobKind::ZombieNautilus => Voice::ZombieNautilus,
        MobKind::CamelHusk => Voice::CamelHusk,
        MobKind::Parched => Voice::Parched,
        MobKind::Wither => Voice::Wither,
        MobKind::Shulker => Voice::Shulker,
        MobKind::SulfurCube => Voice::SulfurCube,
        MobKind::Allay => Voice::Allay,
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
    /// A wind burst ([`voxelcraft::entity::wind::wind_burst`]) over every
    /// player in this world: wind charges, and the shared entry point the
    /// mace's Wind Burst can call.
    pub(super) fn wind_burst(&mut self, burst: &voxelcraft::entity::wind::WindBurst) {
        // A player watching the credits isn't in the level (v1.0 gap Track 8e).
        let mut sessions: Vec<&mut voxelcraft::session::PlayerSession> =
            if self.credits.is_none() { vec![&mut self.session] } else { Vec::new() };
        if self.arrival.is_none() {
            sessions.extend(self.agents.players.values_mut().filter(|b| b.active).map(|b| &mut b.agent.session));
        }
        voxelcraft::entity::wind::wind_burst(&mut self.world, &mut self.mobs.entities, &mut sessions, burst);
        self.audio.play_in(
            SoundSource::Neutral,
            Sound::Breeze(voxelcraft::entity::breeze::Cue::Burst),
            Some(burst.center),
            1.0,
            (0.9, 1.1),
        );
    }

    /// Before a save: End crystal hits still queued for the next tick are
    /// resolved now, as Java resolves them at once (`EndCrystal.hurtServer`),
    /// so a save never keeps a hit crystal whole. Their blasts go off here
    /// and may hit more crystals, so this repeats (each pass removes one).
    pub(super) fn settle_crystal_hits(&mut self) {
        while self.mobs.entities.crystals.hits_pending() {
            let ctx = self.entity_ctx();
            let mut events = Vec::new();
            self.mobs.entities.settle_crystal_hits(&ctx, &mut events);
            for event in events {
                match event {
                    EntityEvent::Explosion { center, power, cause, credit_player } => {
                        self.explosion_event(center, power, cause, credit_player)
                    }
                    EntityEvent::ExitPortal { active } => self.world.set_exit_portal(active, false),
                    _ => {}
                }
            }
            // What the blasts broke drops now, before the save is written.
            self.spawn_world_drops();
            self.mobs.entities.release_silverfish(&mut self.world.released_silverfish);
        }
    }

    /// v1.0 gap Track 8b: a burning projectile's `onProjectileHit` on a
    /// campfire (`CampfireBlock`: lit unless waterlogged) or TNT
    /// (`TntBlock`: primed and removed while `tnt_explodes`), if it may
    /// interact there (`Projectile.mayInteract`: a mob's only with
    /// `mob_griefing`). v1.0 gap Track 8e: the TNT's owner is the
    /// projectile's owner, `by` (`TntBlock.onProjectileHit`).
    fn burning_projectile_hit(&mut self, cell: IVec3, mob_owned: bool, by: Option<crate::entity::trident::Owner>) {
        if mob_owned && !self.gamerules.bool("mob_griefing") {
            return;
        }
        if self.world.light_campfire(cell) {
            return;
        }
        use crate::world::block::Block;
        if self.world.get_block(cell) == Some(Block::TNT) && self.world.rules.tnt_explodes {
            self.world.set_block(cell, Block::AIR);
            self.mobs.entities.prime_tnt(cell, false, by);
            self.audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.95, 1.05));
        }
    }

    /// An entity's [`EntityEvent::Explosion`]: `ServerExplosion.explode`
    /// posts `explode` at its centre, then the blast.
    fn explosion_event(&mut self, center: DVec3, power: f32, cause: Cause, credit_player: bool) {
        self.world.emit_game_event(
            center,
            crate::world::vibration::GameEvent::Explode,
            crate::world::vibration::EventSource::NONE,
            None,
        );
        self.explode(center, power, cause, credit_player);
    }

    /// Blows a hole in the world and hurts everything around `center`.
    /// `cause` completes the death message, as for [`Game::damage_player`].
    pub(super) fn explode(&mut self, center: DVec3, power: f32, cause: Cause, credit_player: bool) {
        self.explode_with(center, power, cause, credit_player, true);
    }

    /// [`Game::explode`], breaking no blocks unless `breaks_blocks`
    /// (`ExplosionInteraction.NONE`).
    pub(super) fn explode_with(
        &mut self,
        center: DVec3,
        power: f32,
        cause: Cause,
        credit_player: bool,
        breaks_blocks: bool,
    ) {
        self.explode_blocks(center, power, cause, credit_player, breaks_blocks, false);
    }

    /// [`Game::explode_with`]; `spares_rails` is a primed TNT minecart's
    /// blast, which leaves rails and the blocks under them
    /// (`MinecartTNT.getBlockExplosionResistance`, `shouldBlockExplode`).
    fn explode_blocks(
        &mut self,
        center: DVec3,
        power: f32,
        cause: Cause,
        credit_player: bool,
        breaks_blocks: bool,
        spares_rails: bool,
    ) {
        // `ServerLevel.explode`: the interaction decides whether blocks break
        // and whether their drops decay.
        let mut blast = self.gamerules.explosion(voxelcraft::rules::ExplosionSource::of(cause));
        blast.destroys_blocks &= breaks_blocks;
        // `ServerExplosion.explode`: find the cells, hurt entities while the
        // walls about to go still shield them, then break the blocks.
        let cells = match (blast.destroys_blocks, spares_rails) {
            (false, _) => Vec::new(),
            (true, false) => self.world.exploded_cells(center, power),
            (true, true) => self.world.rail_sparing_exploded_cells(center, power),
        };
        // v1.0 gap Track 8f: whoever was behind the blast (TNT's owner, a
        // ghast) is the attacker of every mob and player it hurts.
        let flags = (credit_player, blast.destroys_blocks);
        self.mobs.entities.explode_by(&self.world, center, power, flags, cause.by);
        let blamed = match cause.by {
            Some(crate::entity::trident::Owner::Mob(uid)) => Some(uid),
            _ => None,
        };
        // A player watching the credits isn't in the level (v1.0 gap Track 8e).
        if self.credits.is_none()
            && let Some((damage, push)) = self.session.blast(&self.world, center, power)
        {
            let toward = Some(center - self.session.player.pos);
            if self.damage_player_from(self.difficulty.mob_damage(damage), cause, toward) > 0.0 {
                self.session.player.hurt_from(push);
                if let Some(uid) = blamed {
                    self.mobs.entities.mob_hurt_player(crate::entity::PlayerId::HOST, uid);
                }
            }
            self.session.player.vel += push;
        }
        if self.arrival.is_none() {
            for bot in self.agents.players.values_mut().filter(|b| b.active) {
                if let Some((damage, push)) = bot.agent.session.blast(&self.world, center, power) {
                    let taken =
                        bot.agent.hurt(self.difficulty.mob_damage(damage), cause, DVec3::ZERO, &mut self.mobs.entities);
                    if taken > 0.0
                        && let Some(uid) = blamed
                    {
                        self.mobs.entities.mob_hurt_player(bot.id, uid);
                    }
                    bot.agent.session.player.vel += push;
                }
            }
        }
        if blast.destroys_blocks {
            // TNT never sets fires, whoever lit it (its text may name a ghast).
            let fire = !cause.tnt && self.gamerules.explosion_fire(cause.text);
            self.world.detonate(&cells, power, self.gamerules.bool("block_drops"), blast.drop_decay, fire);
        }
        // TNT caught in the blast goes off soon after, lit by whoever was
        // behind it (`TntBlock.wasExploded`: `explosion.getIndirectSourceEntity()`,
        // v1.0 gap Track 8e).
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse, cause.by);
        }
        self.audio.play(Sound::Explosion, Some(center), 1.0, (0.9, 1.05));
    }

    /// v0.16 Track G: `SUMMONING_PILLARS` rebuilds pillar `index`: its top
    /// cleared, a power 5 blast with no source at the cap (breaking
    /// blocks), the spike rebuilt, and a new invulnerable crystal beaming
    /// at (0, 128, 0).
    fn respawn_pillar(&mut self, index: usize) {
        let Some(p) = self.world.generator.end().and_then(|e| e.pillars().get(index).copied()) else { return };
        self.world.clear_spike_top(&p);
        let center = DVec3::new(f64::from(p.x) + 0.5, f64::from(p.top), f64::from(p.z) + 0.5);
        self.world.emit_game_event(
            center,
            crate::world::vibration::GameEvent::Explode,
            crate::world::vibration::EventSource::NONE,
            None,
        );
        self.explode(center, crate::entity::dragon::PILLAR_BLAST, Cause::EXPLOSION, false);
        self.world.build_spike(&p);
        let beam = Some(crate::entity::dragon::RESPAWN_BEAM);
        self.mobs.entities.crystals.spawn(p.crystal(), beam, true, true);
    }

    /// F3 line with entity counts.
    /// F3's entity statistics, kept to two short lines (like Java's
    /// `E:` line) so they fit beside the left column.
    pub(super) fn mobs_debug_lines(&self) -> [String; 2] {
        let e = &self.mobs.entities;
        let (passive, hostile): (Vec<_>, Vec<_>) = MobKind::ALL.iter().partition(|k| !k.is_hostile());
        let total = |kinds: Vec<&MobKind>| kinds.into_iter().map(|&k| e.count(k)).sum::<usize>();
        [
            format!(
                "Mobs: {}/{} drawn (P {}, H {}), arrows {}, items {}",
                e.rendered,
                e.mobs.len(),
                total(passive),
                total(hostile),
                e.arrows.len(),
                e.items.len()
            ),
            format!(
                "Falling: {} (+{} stored), stored: {} in {} chunks",
                self.world.falling_blocks().len(),
                self.world.stored_falling_blocks(),
                e.stored_entities(),
                e.stored_chunks()
            ),
        ]
    }
}

/// v0.16 Track F: `MobEffectUtil.addEffectToPlayersAround` as the elder
/// guardian calls it (Mining Fatigue III for 6000 ticks, display limit
/// 1200) skips a player whose Mining Fatigue is already at least III with
/// more than 1199 ticks left. The elder's pulse is the only source of that
/// exact effect.
fn elder_curse_held(
    effects: &voxelcraft::simulation::effects::Effects,
    effect: voxelcraft::simulation::effects::Effect,
    amplifier: u8,
    ticks: u32,
) -> bool {
    use voxelcraft::simulation::effects::Effect;
    effect == Effect::MiningFatigue
        && amplifier == 2
        && ticks == 6000
        && effects.get(Effect::MiningFatigue).is_some_and(|a| a.amplifier >= 2 && a.ticks > 1199)
}
