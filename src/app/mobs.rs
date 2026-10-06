//! Game-side glue for mobs: updating them, player melee, entity events and
//! the `--spawn` / `--wait` debug options.

use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Call, Sound, Voice};
use crate::entity::{self, Entities, EntityEvent, MobKind, MobSound, PlayerId, Target};
use crate::physics;

use super::{Game, GameMode, REACH};

pub(super) struct Mobs {
    pub entities: Entities,
    /// Seconds until the player can hit again.
    pub(super) attack_cooldown: f64,
    /// The current left-button hold started on a mob: don't break blocks
    /// until it's released.
    pub(super) attack_held: bool,
    /// `--spawn` requests, applied once the world has loaded.
    pending: Vec<(MobKind, IVec3)>,
    /// `--wait`: seconds to keep simulating before a `--screenshot`.
    wait: f64,
    wait_start: Option<Instant>,
}

impl Mobs {
    pub fn new(seed: u64, spawn: Vec<(MobKind, IVec3)>, wait: f64) -> Self {
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

    /// Left-button press: hits the mob under the crosshair. Returns `true`
    /// if a mob was targeted, in which case no block should be broken.
    pub(super) fn attack(&mut self) -> bool {
        if self.strike_fight() {
            return true;
        }
        let Some(i) = self.mob_target() else { return false };
        self.mobs.attack_held = true;
        if self.mobs.attack_cooldown <= 0.0 {
            self.mobs.attack_cooldown = entity::ATTACK_COOLDOWN;
            // A hit while falling is a critical one, like Minecraft.
            let p = &self.player;
            let critical = !p.on_ground && p.vel.y < 0.0 && !p.in_water && !p.flying;
            let base = crate::mining::attack_damage(self.held_item()) + self.vitals.effects.attack_bonus();
            let damage = base.max(0.0) * if critical { 1.5 } else { 1.0 };
            let killed = self.mobs.entities.attack(i, self.player.forward().as_dvec3(), damage);
            let at = self.mobs.entities.mobs[i].pos + DVec3::Y * 0.5;
            let pitch = if critical { (1.25, 1.4) } else { (0.9, 1.1) };
            self.audio.play(Sound::Hit, Some(at), 0.8, pitch);
            self.wear_held(true);
            if self.mode == GameMode::Survival {
                self.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
            }
            if let Some(kind) = killed {
                let pos = self.mobs.entities.mobs[i].pos;
                self.mobs.entities.drop_loot(kind, pos);
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
            self.mobs.attack_cooldown = entity::ATTACK_COOLDOWN;
            let p = &self.player;
            let critical = !p.on_ground && p.vel.y < 0.0 && !p.in_water && !p.flying;
            let base = crate::mining::attack_damage(self.held_item()) + self.vitals.effects.attack_bonus();
            let damage = base.max(0.0) * if critical { 1.5 } else { 1.0 };
            let by = self.actor;
            if self.mobs.entities.strike(hit, damage, by) {
                self.audio.play(
                    Sound::Hit,
                    Some(self.player.eye() + self.player.forward().as_dvec3() * 2.0),
                    0.8,
                    (0.9, 1.1),
                );
                self.wear_held(true);
            }
            if self.mode == GameMode::Survival {
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
        for (kind, mut pos) in std::mem::take(&mut self.mobs.pending) {
            if pos.y == i32::MIN {
                pos.y = self.world.generator.column(pos.x, pos.z).height + 1;
            }
            self.mobs.entities.spawn(kind, pos.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        }
    }

    pub(super) fn update_mobs(&mut self, dt: f64) {
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse);
            self.audio.play(Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.95, 1.05));
        }
        self.mobs.attack_cooldown -= dt;
        let mut players = vec![Target {
            alive: !self.vitals.is_dead(),
            look: self.player.forward().as_dvec3(),
            ..Target::new(PlayerId::HOST, self.player.pos, self.mode == GameMode::Survival && !self.vitals.is_dead())
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
            spawning: true,
            dimension: self.dimension,
        };
        for event in self.mobs.entities.update(dt, &self.world, &ctx) {
            match event {
                EntityEvent::PlayerHit { player: PlayerId::HOST, damage, knockback, cause } => {
                    // Knockback only lands with damage, so hurt immunity
                    // also stops repeated shoves.
                    if self.damage_player_armored(damage, cause) > 0.0 {
                        self.player.vel += knockback.as_dvec3();
                    }
                }
                EntityEvent::PlayerHit { player, damage, knockback, cause } => {
                    if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.hurt(damage, cause, knockback.as_dvec3(), &mut self.mobs.entities);
                    }
                }
                EntityEvent::Explosion { center, power, cause } => self.explode(center, power, cause),
                EntityEvent::PearlLanded { owner, pos } => self.pearl_landed(owner, pos),
                EntityEvent::Ignite { player: PlayerId::HOST, secs } => {
                    if self.mode == GameMode::Survival {
                        self.vitals.ignite(secs);
                    }
                }
                EntityEvent::Ignite { player, secs } => {
                    if let Some(bot) = self.agents.by_id_mut(player)
                        && !bot.agent.creative
                    {
                        bot.agent.vitals.ignite(secs);
                    }
                }
                EntityEvent::IgniteBlock { cell } => {
                    self.world.ignite(cell);
                }
                EntityEvent::Fireball { .. } => {}
                EntityEvent::Sound { sound, pos } => {
                    let (sound, gain) = match sound {
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
                EntityEvent::Shoot { .. } | EntityEvent::DragonXp { .. } => {}
                EntityEvent::BreakBlock { cell } => {
                    self.world.set_block(cell, crate::world::block::Block::AIR);
                }
                EntityEvent::BuildGateway { pos } => self.world.build_gateway(pos),
                EntityEvent::PearlGateway { owner, cell } => {
                    if owner == PlayerId::HOST && !self.vitals.is_dead() {
                        self.enter_gateway(cell);
                    }
                }
                EntityEvent::DragonKilled { first } => {
                    self.world.open_exit_portal(first);
                    self.audio.play(Sound::PortalSpawn, None, 1.0, (1.0, 1.0));
                }
            }
        }
    }
}

fn voice(kind: MobKind) -> Voice {
    match kind {
        MobKind::Pig => Voice::Pig,
        MobKind::Cow => Voice::Cow,
        MobKind::Sheep => Voice::Sheep,
        MobKind::Chicken => Voice::Chicken,
        MobKind::Zombie => Voice::Zombie,
        MobKind::Skeleton => Voice::Skeleton,
        MobKind::Creeper => Voice::Creeper,
        MobKind::Spider => Voice::Spider,
        MobKind::ZombifiedPiglin => Voice::Zombie,
        MobKind::Enderman => Voice::Enderman,
        MobKind::Blaze => Voice::Blaze,
        MobKind::Silverfish => Voice::Spider,
    }
}

impl Game {
    /// Blows a hole in the world and hurts everything around `center`.
    /// `cause` completes the death message, as for [`Game::damage_player`].
    pub(super) fn explode(&mut self, center: DVec3, power: f32, cause: &str) {
        self.world.explode(center, power as f64);
        self.mobs.entities.explode(center, power);
        // TNT caught in the blast goes off soon after.
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse);
        }
        self.audio.play(Sound::Explosion, Some(center), 1.0, (0.9, 1.05));
        let mid = self.player.pos + DVec3::Y * 0.9;
        if let Some((damage, impact)) = entity::explosion_damage(power, mid.distance(center))
            && self.damage_player_armored(damage, cause) > 0.0
        {
            let away = (mid - center).normalize_or(DVec3::Y);
            self.player.vel += away * (impact as f64 * 14.0) + DVec3::Y * 4.0;
        }
        if self.arrival.is_some() {
            return;
        }
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            let mid = bot.agent.player.pos + DVec3::Y * 0.9;
            if let Some((damage, impact)) = entity::explosion_damage(power, mid.distance(center)) {
                let away = (mid - center).normalize_or(DVec3::Y);
                let push = away * (impact as f64 * 14.0) + DVec3::Y * 4.0;
                bot.agent.hurt(damage, cause, push, &mut self.mobs.entities);
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
