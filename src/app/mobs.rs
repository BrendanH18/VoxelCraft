//! Game-side glue for mobs: updating them, player melee, entity events and
//! the `--spawn` / `--wait` debug options.

use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Call, Sound, Voice};
use crate::entity::{self, Entities, EntityEvent, MobKind, MobSound, PlayerId, Target};
use crate::physics;

use super::Game;

pub(super) struct Mobs {
    pub entities: Entities,
    /// The current left-button hold started on a mob: don't break blocks
    /// until it's released.
    pub(super) attack_held: bool,
    /// `--spawn` requests, applied once the world has loaded.
    pending: Vec<(MobKind, IVec3, Option<crate::entity::armor::Equipped>)>,
    /// `--wait`: seconds to keep simulating before a `--screenshot`.
    wait: f64,
    wait_start: Option<Instant>,
}

impl Mobs {
    pub fn new(seed: u64, spawn: Vec<(MobKind, IVec3, Option<crate::entity::armor::Equipped>)>, wait: f64) -> Self {
        Self { entities: Entities::new(seed), attack_held: false, pending: spawn, wait, wait_start: None }
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

    /// Right-click a sheep with dye or shears, shared with gamepad players.
    pub(super) fn use_sheep(&mut self) -> bool {
        let Some(index) = self.mob_target() else { return false };
        let Some(item) = self.held_item() else { return false };
        let Some(shears) = self.mobs.entities.use_on_sheep(index, item) else { return false };
        if self.session.mode.is_survival() {
            if shears {
                self.session.inventory.wear(self.actions.selected, 1);
            } else {
                self.session.inventory.take_one(self.actions.selected);
            }
        }
        true
    }

    /// Left-button press: melee through the shared session transaction.
    /// Returns `true` if something was hit, in which case no block should be broken.
    pub(super) fn attack(&mut self) -> bool {
        if self.attack_lan_player() {
            return true;
        }
        let sprinting = self.sprinting_for_attack();
        let result = self.session.attack(self.actions.selected, &self.world, &mut self.mobs.entities, sprinting);
        use voxelcraft::session::combat::{Outcome, Target as Hit};
        let Ok(Outcome::Hit { target, at, critical, tool_broken, .. }) = result else { return false };
        self.mobs.attack_held = true;
        let (volume, pitch) = match target {
            Hit::FenceKnot => (0.0, (1.0, 1.0)),
            Hit::Cart => (0.7, (0.8, 1.0)),
            Hit::Fireball => (0.7, (0.6, 0.8)),
            Hit::Fight => (0.8, (0.9, 1.1)),
            Hit::Mob(_) if critical => (0.8, (1.25, 1.4)),
            Hit::Mob(_) => (0.8, (0.9, 1.1)),
        };
        if volume > 0.0 {
            self.audio.play(Sound::Hit, Some(at), volume, pitch);
        }
        if let Some(item) = tool_broken {
            self.tool_broke(item);
        }
        true
    }

    /// Sprint state of whoever is acting: a controller/LAN hand's own input, or the host's keys.
    pub(super) fn sprinting_for_attack(&self) -> bool {
        if self.puppet {
            self.agents
                .players
                .values()
                .find(|b| b.id == self.session.id)
                .is_some_and(|b| b.agent.movement_input().sprint)
                && (self.session.mode.invulnerable() || self.session.vitals.hunger.can_sprint())
        } else {
            self.movement_input(self.arrival.is_some()).sprint
        }
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
        self.mobs.attack_held = false;
    }

    /// Block breaking is suppressed while aiming at a mob, and for the rest
    /// of a click that started as an attack.
    pub(super) fn attacking(&self) -> bool {
        self.mobs.attack_held
            || self.mob_target().is_some()
            || self.fight_target().is_some()
            || self.lan_player_target().is_some()
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
        self.mobs.entities.moon_brightness =
            [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75][self.day_count.rem_euclid(8) as usize];
        self.mobs.entities.trader_spawning =
            self.gamerules.bool("doTraderSpawning") && self.gamerules.bool("doMobSpawning");
        self.mobs.entities.villager_griefing = self.gamerules.bool("mobGriefing");
        self.mobs.entities.mob_loot = self.gamerules.bool("doMobLoot");
        self.mobs.entities.village_time = self.day_time;
        self.mobs.entities.village_day = self.day_count;
        for m in &mut self.mobs.entities.mobs {
            if let Some(v) = &mut m.villager {
                v.trading =
                    self.inventory_open && self.container == super::Container::Trading(v.id) || self.pads.trading(v.id);
            }
        }
        if self.held_item() != Some(crate::item::Item::FISHING_ROD) || self.session.vitals.is_dead() {
            self.mobs.entities.drop_bobber(self.session.id);
        }
        super::seat_rider(&self.mobs.entities, &mut self.session.player, crate::entity::PlayerId::HOST);
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            super::seat_rider(&self.mobs.entities, &mut bot.agent.session.player, bot.agent.session.id);
        }
        if self.difficulty == crate::simulation::difficulty::Difficulty::Peaceful {
            self.mobs.entities.despawn_hostiles();
        }
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse);
            self.audio.play(Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.95, 1.05));
        }
        crate::entity::golem::finish_golems(&mut self.world, &mut self.mobs.entities);
        for (cell, count) in std::mem::take(&mut self.world.hatched_turtles) {
            self.mobs.entities.hatch_turtles(cell, count);
        }
        let mut players = vec![Target {
            alive: !self.session.vitals.is_dead(),
            look: self.session.player.forward().as_dvec3(),
            thorns: Target::thorns_of(&self.session.inventory.armor),
            held_enchants: self
                .session
                .inventory
                .get(self.actions.selected)
                .map_or(Default::default(), |s| s.active_enchants()),
            shape: self.session.player.collision_shape(),
            gold_armor: crate::entity::nether::wears_gold(&self.session.inventory.armor),
            held_item: self.held_item(),
            on_ground: self.session.player.on_ground,
            in_water: self.session.player.in_water,
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
            spawn_light: crate::simulation::lighting::SpawnLight::at(
                (self.day_time.rem_euclid(1.) * 24000.).floor() as u32,
                self.weather.strength,
                if self.weather.thundering { self.weather.strength } else { 0. },
                self.weather.thundering,
            ),
            players,
            // The Nether has no sun to burn the undead.
            daylight: if self.dimension.has_sky() {
                self.weather.dim(super::sky_state(self.day_time).daylight)
            } else {
                0.0
            },
            raining: self.weather.raining,
            spawning: self.gamerules.bool("doMobSpawning"),
            dimension: self.dimension,
        };
        let mut smashed = Vec::new();
        for event in self.mobs.entities.update_difficulty(dt, &self.world, &ctx, self.difficulty) {
            match event {
                EntityEvent::ProjectileBlockHit { cell, pos, normal, arrow } => {
                    self.world.hit_redstone_target(cell, pos, normal, arrow);
                    self.world.touch_redstone_ore(cell);
                }
                EntityEvent::PlayerHit { player: PlayerId::HOST, damage, knockback, cause } => {
                    // Knockback only lands with damage, so hurt immunity
                    // also stops repeated shoves.
                    if self.damage_player_armored(self.difficulty.mob_damage(damage), cause) > 0.0 {
                        self.session.player.hurt_from(knockback.as_dvec3());
                        let resistance = self.session.inventory.knockback_resistance();
                        self.session.player.vel +=
                            crate::simulation::survival::knockback_taken(knockback.as_dvec3(), resistance);
                    }
                }
                EntityEvent::PlayerHit { player, damage, knockback, cause } => {
                    if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt(
                            self.difficulty.mob_damage(damage),
                            cause,
                            knockback.as_dvec3(),
                            &mut self.mobs.entities,
                        );
                    }
                }
                EntityEvent::PlayerFall { player, damage } => {
                    if self.gamerules.bool("fallDamage") {
                        if player == PlayerId::HOST {
                            self.damage_player(damage, crate::simulation::survival::CAUSE_FALL);
                        } else if let Some(bot) = self.agents.by_id_mut(player) {
                            bot.agent.hurt_fall(damage, &mut self.mobs.entities);
                        }
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
                EntityEvent::PlayerSting { player, damage, cause } => {
                    if player == PlayerId::HOST {
                        self.damage_player_armored(damage, cause);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt(damage, cause, DVec3::ZERO, &mut self.mobs.entities);
                    }
                }
                EntityEvent::PlayerEffect { player: PlayerId::HOST, effect, amplifier, ticks } => {
                    if self.session.mode.is_survival() {
                        let damage = self.session.vitals.apply_effect(effect, amplifier, ticks);
                        if damage > 0.0 {
                            self.damage_player(damage, "was killed by magic");
                        }
                    }
                }
                EntityEvent::PlayerEffect { player, effect, amplifier, ticks } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
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
                    self.audio.play(Sound::Break(crate::audio::sounds::Material::Glass), Some(pos), 1.0, (0.9, 1.1));
                }
                EntityEvent::PlayerMagic { player: PlayerId::HOST, amount } => {
                    if self.session.mode.is_survival() {
                        if amount > 0.0 {
                            self.damage_player(amount, "was killed by magic");
                        } else {
                            self.session.vitals.health =
                                (self.session.vitals.health - amount).min(crate::simulation::survival::MAX_HEALTH);
                        }
                    }
                }
                EntityEvent::PlayerMagic { player, amount } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.session.mode.is_survival()
                    {
                        if amount > 0.0 {
                            bot.agent.hurt(amount, "was killed by magic", DVec3::ZERO, &mut self.mobs.entities);
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
                    if self.gamerules.bool("mobGriefing") {
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
                    let (sound, gain) = match sound {
                        MobSound::Bell => (Sound::Bell, 1.0),
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
                        MobSound::Ambient(kind) => (Sound::Mob(voice(kind), Call::Ambient), 0.7),
                        MobSound::Hurt(kind) => (Sound::Mob(voice(kind), Call::Hurt), 0.9),
                        MobSound::Death(kind) => (Sound::Mob(voice(kind), Call::Death), 0.9),
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
                    self.audio.play(sound, Some(pos), gain, pitch);
                }
                EntityEvent::MobShot { pos, .. } => {
                    self.audio.play(Sound::Hit, Some(pos + DVec3::Y * 0.5), 0.8, (0.9, 1.1))
                }
                EntityEvent::Shoot { .. }
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
        if self.gamerules.bool("mobGriefing") && !smashed.is_empty() {
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

fn voice(kind: MobKind) -> Voice {
    match kind {
        MobKind::Pig => Voice::Pig,
        MobKind::Cow => Voice::Cow,
        MobKind::Sheep => Voice::Sheep,
        MobKind::Chicken => Voice::Chicken,
        MobKind::Zombie | MobKind::Husk | MobKind::Drowned | MobKind::ZombieVillager => Voice::Zombie,
        MobKind::Skeleton | MobKind::WitherSkeleton => Voice::Skeleton,
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
        MobKind::Horse | MobKind::Donkey | MobKind::Mule => Voice::Horse,
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
        MobKind::Rabbit => Voice::Rabbit,
        MobKind::Goat => Voice::Goat,
    }
}

impl Game {
    /// Blows a hole in the world and hurts everything around `center`.
    /// `cause` completes the death message, as for [`Game::damage_player`].
    pub(super) fn explode(&mut self, center: DVec3, power: f32, cause: &str, credit_player: bool) {
        let griefing_mob = cause.contains("creeper") || cause.contains("ghast");
        let mob_can_grief = !griefing_mob || self.gamerules.bool("mobGriefing");
        if mob_can_grief {
            self.world.explode_with_drops(center, power as f64, self.gamerules.bool("doTileDrops"));
        }
        if credit_player {
            self.mobs.entities.explode_credited(center, power);
        } else {
            self.mobs.entities.explode(center, power);
        }
        // TNT caught in the blast goes off soon after.
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse);
        }
        self.audio.play(Sound::Explosion, Some(center), 1.0, (0.9, 1.05));
        let mid = self.session.player.pos + DVec3::Y * 0.9;
        if let Some((damage, impact)) = entity::explosion_damage(power, mid.distance(center))
            && let damage = self.difficulty.mob_damage(damage)
            && self.damage_player_armored(damage, cause) > 0.0
        {
            let away = (mid - center).normalize_or(DVec3::Y);
            self.session.player.hurt_from(away);
            let push = away * (impact as f64 * 14.0) + DVec3::Y * 4.0;
            self.session.player.vel +=
                crate::simulation::survival::knockback_taken(push, self.session.inventory.knockback_resistance());
        }
        if self.arrival.is_some() {
            return;
        }
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            let mid = bot.agent.session.player.pos + DVec3::Y * 0.9;
            if let Some((damage, impact)) = entity::explosion_damage(power, mid.distance(center)) {
                let away = (mid - center).normalize_or(DVec3::Y);
                let push = away * (impact as f64 * 14.0) + DVec3::Y * 4.0;
                bot.agent.hurt(self.difficulty.mob_damage(damage), cause, push, &mut self.mobs.entities);
            }
        }
    }

    /// F3 line with entity counts.
    pub(super) fn mobs_debug_line(&self) -> String {
        let e = &self.mobs.entities;
        let (passive, hostile): (Vec<_>, Vec<_>) = MobKind::ALL.iter().partition(|k| !k.is_hostile());
        let total = |kinds: Vec<&MobKind>| kinds.into_iter().map(|&k| e.count(k)).sum::<usize>();
        format!(
            "Entities: {} (passive: {}, hostile: {}, arrows: {}, items: {}), {} rendered, {} falling blocks",
            e.mobs.len(),
            total(passive),
            total(hostile),
            e.arrows.len(),
            e.items.len(),
            e.rendered,
            self.world.falling_blocks().len()
        )
    }
}
