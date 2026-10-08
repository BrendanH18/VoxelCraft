//! Game-side glue for mobs: updating them, player melee, entity events and
//! the `--spawn` / `--wait` debug options.

use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Call, Sound, Voice};
use crate::entity::{self, Entities, EntityEvent, MobKind, MobSound, PlayerId, Target};
use crate::physics;

use super::{Game, REACH};

pub(super) struct Mobs {
    pub entities: Entities,
    /// Seconds until the player can hit again.
    pub(super) attack_cooldown: f64,
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
        Self {
            entities: Entities::new(seed),
            attack_cooldown: 0.0,
            attack_held: false,
            pending: spawn,
            wait,
            wait_start: None,
        }
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
        let eye = self.player.eye();
        let dir = self.player.forward().as_dvec3();
        let block_dist = self
            .target()
            .and_then(|(b, _)| physics::ray_aabb(eye, dir, b.as_dvec3(), b.as_dvec3() + DVec3::ONE))
            .unwrap_or(REACH);
        self.mobs.entities.raycast(eye, dir, block_dist.min(REACH)).map(|(i, _)| i)
    }

    /// Right-click a sheep with dye or shears, shared with gamepad players.
    pub(super) fn use_sheep(&mut self) -> bool {
        let Some(index) = self.mob_target() else { return false };
        let Some(item) = self.held_item() else { return false };
        let Some(shears) = self.mobs.entities.use_on_sheep(index, item) else { return false };
        if self.mode.is_survival() {
            if shears {
                self.inventory.wear(self.actions.selected, 1);
            } else {
                self.inventory.take_one(self.actions.selected);
            }
        }
        true
    }

    /// Left-button press: hits the mob under the crosshair. Returns `true`
    /// if a mob was targeted, in which case no block should be broken.
    pub(super) fn attack(&mut self) -> bool {
        let eye = self.player.eye();
        let dir = self.player.forward().as_dvec3();
        if self.mobs.entities.large_fireball(eye, dir, REACH).is_some() {
            self.mobs.attack_held = true;
            if self.mobs.attack_cooldown <= 0.0 {
                self.mobs.entities.punch_fireball(eye, dir, REACH);
                self.mobs.attack_cooldown = crate::mining::attack_cooldown(self.held_item());
                self.audio.play(Sound::Hit, Some(eye + dir * 2.0), 0.7, (0.6, 0.8));
                self.wear_held(true);
                if self.mode.is_survival() {
                    self.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
                }
            }
            return true;
        }
        if self.strike_fight() {
            return true;
        }
        let Some(i) = self.mob_target() else { return false };
        self.mobs.attack_held = true;
        if self.mobs.attack_cooldown <= 0.0 {
            self.mobs.attack_cooldown = crate::mining::attack_cooldown(self.held_item());
            // A hit while falling is a critical one, like Minecraft.
            let p = &self.player;
            let critical = !p.on_ground && p.vel.y < 0.0 && !p.in_water && !p.flying;
            // Controller input belongs to the puppet's agent, not the host's keys.
            let sprint = if self.puppet {
                self.agents
                    .players
                    .values()
                    .find(|b| b.id == self.actor)
                    .is_some_and(|b| b.agent.movement_input().sprint)
                    && (self.mode.invulnerable() || self.vitals.hunger.can_sprint())
            } else {
                self.movement_input(self.arrival.is_some()).sprint
            };
            let sweep = (p.on_ground && !critical && !sprint).then_some(p.pos);
            let held = self.inventory.get(self.actions.selected);
            let bonus = self.vitals.effects.attack_bonus();
            let dir = self.player.forward().as_dvec3();
            self.mobs.entities.melee(i, dir, held, bonus, critical, sweep);
            let at = self.mobs.entities.mobs[i].pos + DVec3::Y * 0.5;
            let pitch = if critical { (1.25, 1.4) } else { (0.9, 1.1) };
            self.audio.play(Sound::Hit, Some(at), 0.8, pitch);
            self.wear_held(true);
            if self.mode.is_survival() {
                self.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
            }
        }
        true
    }

    /// The End crystal or dragon part under the crosshair, if it's within
    /// reach and in front of the targeted block and any mob.
    fn fight_target(&self) -> Option<crate::entity::dragon::Hit> {
        let eye = self.player.eye();
        let dir = self.player.forward().as_dvec3();
        let block_dist = self
            .target()
            .and_then(|(b, _)| physics::ray_aabb(eye, dir, b.as_dvec3(), b.as_dvec3() + DVec3::ONE))
            .unwrap_or(REACH);
        self.mobs.entities.fight_raycast(eye, dir, block_dist.min(REACH)).map(|(hit, _)| hit)
    }

    /// Melee on an End crystal (it blows up) or the dragon.
    fn strike_fight(&mut self) -> bool {
        let Some(hit) = self.fight_target() else { return false };
        self.mobs.attack_held = true;
        if self.mobs.attack_cooldown <= 0.0 {
            self.mobs.attack_cooldown = crate::mining::attack_cooldown(self.held_item());
            let p = &self.player;
            let critical = !p.on_ground && p.vel.y < 0.0 && !p.in_water && !p.flying;
            let base = crate::mining::attack_damage(self.held_item()) + self.vitals.effects.attack_bonus();
            let sharpness =
                self.inventory.get(self.actions.selected).map_or(Default::default(), |s| s.active_enchants());
            let damage = base.max(0.0) * if critical { 1.5 } else { 1.0 }
                + crate::enchant::damage_bonus(sharpness, crate::enchant::Creature::Other);
            let by = self.actor;
            if self.mobs.entities.strike(hit, damage, by) {
                let impact = self.player.eye() + self.player.forward().as_dvec3() * 2.0;
                for kind in [
                    critical.then_some(crate::particles::Kind::Crit),
                    (crate::enchant::damage_bonus(sharpness, crate::enchant::Creature::Other) > 0.0)
                        .then_some(crate::particles::Kind::MagicCrit),
                ]
                .into_iter()
                .flatten()
                {
                    let mut b = crate::particles::Burst::new(kind, impact, 16);
                    b.spread = DVec3::splat(0.4);
                    self.world.particles.push(crate::particles::Request::Tracking(b));
                }
                self.audio.play(
                    Sound::Hit,
                    Some(self.player.eye() + self.player.forward().as_dvec3() * 2.0),
                    0.8,
                    (0.9, 1.1),
                );
                self.wear_held(true);
            }
            if self.mode.is_survival() {
                self.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
            }
        }
        true
    }

    pub(super) fn release_attack(&mut self) {
        self.mobs.attack_held = false;
    }

    /// Block breaking is suppressed while aiming at a mob, and for the rest
    /// of a click that started as an attack.
    pub(super) fn attacking(&self) -> bool {
        self.mobs.attack_held || self.mob_target().is_some() || self.fight_target().is_some()
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
        self.mobs.entities.mob_loot = self.gamerules.bool("doMobLoot");
        self.mobs.entities.village_time = self.day_time;
        self.mobs.entities.village_day = self.day_count;
        for m in &mut self.mobs.entities.mobs {
            if let Some(v) = &mut m.villager {
                v.trading =
                    self.inventory_open && self.container == super::Container::Trading(v.id) || self.pads.trading(v.id);
            }
        }
        if self.held_item() != Some(crate::item::Item::FISHING_ROD) || self.vitals.is_dead() {
            self.mobs.entities.drop_bobber(self.actor);
        }
        if self.difficulty == crate::simulation::difficulty::Difficulty::Peaceful {
            self.mobs.entities.despawn_hostiles();
        }
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse);
            self.audio.play(Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.95, 1.05));
        }
        crate::entity::golem::finish_golems(&mut self.world, &mut self.mobs.entities);
        self.mobs.attack_cooldown -= dt;
        let mut players = vec![Target {
            alive: !self.vitals.is_dead(),
            look: self.player.forward().as_dvec3(),
            thorns: Target::thorns_of(&self.inventory.armor),
            held_enchants: self
                .inventory
                .get(self.actions.selected)
                .map_or(Default::default(), |s| s.active_enchants()),
            shape: self.player.collision_shape(),
            ..Target::new(PlayerId::HOST, self.player.pos, self.mode.targetable() && !self.vitals.is_dead())
        }];
        // Agents keep source-dimension positions until arrival relocates them.
        if self.arrival.is_none() {
            players.extend(self.agents.targets());
        }
        let ctx = entity::Ctx {
            players,
            // The Nether has no sun to burn the undead.
            daylight: if self.dimension.has_sky() {
                self.weather.dim(super::sky_state(self.day_time).daylight)
            } else {
                0.0
            },
            raining: self.weather.raining,
            spawning: self.difficulty != crate::simulation::difficulty::Difficulty::Peaceful
                && self.gamerules.bool("doMobSpawning"),
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
                        self.player.hurt_from(knockback.as_dvec3());
                        let resistance = self.inventory.knockback_resistance();
                        self.player.vel +=
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
                EntityEvent::PlayerEffect { player: PlayerId::HOST, effect, amplifier, ticks } => {
                    if self.mode.is_survival() {
                        let damage = self.vitals.apply_effect(effect, amplifier, ticks);
                        if damage > 0.0 {
                            self.damage_player(damage, "was killed by magic");
                        }
                    }
                }
                EntityEvent::PlayerEffect { player, effect, amplifier, ticks } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.mode.is_survival()
                    {
                        let damage = bot.agent.vitals.apply_effect(effect, amplifier, ticks);
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
                    if self.mode.is_survival() {
                        if amount > 0.0 {
                            self.damage_player(amount, "was killed by magic");
                        } else {
                            self.vitals.health =
                                (self.vitals.health - amount).min(crate::simulation::survival::MAX_HEALTH);
                        }
                    }
                }
                EntityEvent::PlayerMagic { player, amount } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.mode.is_survival()
                    {
                        if amount > 0.0 {
                            bot.agent.hurt(amount, "was killed by magic", DVec3::ZERO, &mut self.mobs.entities);
                        } else {
                            bot.agent.vitals.health =
                                (bot.agent.vitals.health - amount).min(crate::simulation::survival::MAX_HEALTH);
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
                    if self.mode.is_survival() {
                        self.vitals.ignite(secs);
                    }
                }
                EntityEvent::Ignite { player, secs } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.mode.is_survival()
                    {
                        bot.agent.vitals.ignite(secs);
                    }
                }
                EntityEvent::IgniteBlock { cell } => {
                    if self.gamerules.bool("mobGriefing") {
                        self.world.ignite(cell);
                    }
                }
                EntityEvent::Fireball { .. } | EntityEvent::ThrowPotion { .. } => {}
                EntityEvent::Sound { sound, pos } => {
                    let (sound, gain) = match sound {
                        MobSound::Bell => (Sound::Bell, 1.0),
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
                    let pitch = if sound == Sound::Fireball && gain == 1.0 { (0.6, 0.7) } else { (0.9, 1.1) };
                    self.audio.play(sound, Some(pos), gain, pitch);
                }
                EntityEvent::MobShot { pos, .. } => {
                    self.audio.play(Sound::Hit, Some(pos + DVec3::Y * 0.5), 0.8, (0.9, 1.1))
                }
                EntityEvent::Shoot { .. }
                | EntityEvent::DragonXp { .. }
                | EntityEvent::MobKilled { .. }
                | EntityEvent::LaidEgg { .. }
                | EntityEvent::Hatched { .. } => {}
                EntityEvent::VillagerDoor { cell } => {
                    if self.world.get_block(cell).is_some_and(|b| {
                        matches!(b.shaped(), Some(crate::world::block::Shaped::Door { open: false, upper: false, .. }))
                    }) {
                        self.toggle_door(cell, glam::Vec3::ZERO);
                    }
                }
                EntityEvent::BreakBlock { cell } => smashed.push(cell),
                EntityEvent::Shove { player: PlayerId::HOST, velocity } => {
                    if self.mode.is_survival() && !self.vitals.is_dead() {
                        let push = crate::simulation::survival::knockback_taken(
                            velocity.as_dvec3(),
                            self.inventory.knockback_resistance(),
                        );
                        shove(&mut self.player.vel, push);
                    }
                }
                EntityEvent::Shove { player, velocity } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && bot.agent.mode.is_survival()
                    {
                        let push = crate::simulation::survival::knockback_taken(
                            velocity.as_dvec3(),
                            bot.agent.inventory.knockback_resistance(),
                        );
                        shove(&mut bot.agent.player.vel, push);
                    }
                }
                EntityEvent::BuildGateway { pos } => self.world.build_gateway(pos),
                EntityEvent::PearlGateway { owner, cell, pos } => {
                    if owner == PlayerId::HOST {
                        if !self.vitals.is_dead() {
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
        MobKind::TraderLlama => Voice::Cow,
        MobKind::IronGolem => Voice::Cow,
        MobKind::SnowGolem => Voice::Slime,
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
        let mid = self.player.pos + DVec3::Y * 0.9;
        if let Some((damage, impact)) = entity::explosion_damage(power, mid.distance(center))
            && let damage = self.difficulty.mob_damage(damage)
            && self.damage_player_armored(damage, cause) > 0.0
        {
            let away = (mid - center).normalize_or(DVec3::Y);
            self.player.hurt_from(away);
            let push = away * (impact as f64 * 14.0) + DVec3::Y * 4.0;
            self.player.vel +=
                crate::simulation::survival::knockback_taken(push, self.inventory.knockback_resistance());
        }
        if self.arrival.is_some() {
            return;
        }
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            let mid = bot.agent.player.pos + DVec3::Y * 0.9;
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
