//! Game-side glue for mobs: updating them, player melee, entity events and
//! the `--spawn` / `--wait` debug options.

use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::audio::sounds::Sound;
use crate::entity::{self, Entities, EntityEvent, MobKind, MobSound};
use crate::physics;

use super::{Game, GameMode, REACH};

pub(super) struct Mobs {
    pub entities: Entities,
    /// Seconds until the player can hit again.
    attack_cooldown: f64,
    /// The current left-button hold started on a mob: don't break blocks
    /// until it's released.
    attack_held: bool,
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
        let Some(i) = self.mob_target() else { return false };
        self.mobs.attack_held = true;
        if self.mobs.attack_cooldown <= 0.0 {
            self.mobs.attack_cooldown = entity::ATTACK_COOLDOWN;
            let damage = crate::mining::attack_damage(self.held_item());
            let killed = self.mobs.entities.attack(i, self.player.forward().as_dvec3(), damage);
            self.wear_held(true);
            if self.mode == GameMode::Survival {
                self.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
            }
            if let Some(kind) = killed
                && self.mode == GameMode::Survival
            {
                // Loot goes straight to the inventory until items can lie in the world.
                for (item, count) in self.mobs.entities.drops(kind) {
                    self.inventory.add(item, count);
                }
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
        self.mobs.attack_held || self.mob_target().is_some()
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
        self.mobs.attack_cooldown -= dt;
        let ctx = entity::Ctx {
            player_pos: self.player.pos,
            player_targetable: self.mode == GameMode::Survival,
            daylight: super::sky_state(self.day_time).daylight,
            spawning: true,
        };
        for event in self.mobs.entities.update(dt, &self.world, &ctx) {
            match event {
                EntityEvent::PlayerHit { damage, knockback, cause } => {
                    // Knockback only lands with damage, so hurt immunity
                    // also stops repeated shoves.
                    if self.damage_player(damage, cause) > 0.0 {
                        self.player.vel += knockback.as_dvec3();
                    }
                }
                EntityEvent::Explosion { center, power } => self.explode(center, power),
                EntityEvent::Sound { sound, pos } => {
                    let sound = match sound {
                        MobSound::Fuse => Sound::Fuse,
                        MobSound::Bow => Sound::Bow,
                    };
                    self.audio.play(sound, Some(pos), 1.0, (0.95, 1.05));
                }
                EntityEvent::Shoot { .. } => {}
            }
        }
    }
}

impl Game {
    /// Blows a hole in the world and hurts everything around `center`.
    pub(super) fn explode(&mut self, center: DVec3, power: f32) {
        self.world.explode(center, power as f64);
        self.mobs.entities.explode(center, power);
        self.audio.play(Sound::Explosion, Some(center), 1.0, (0.9, 1.05));
        let mid = self.player.pos + DVec3::Y * 0.9;
        if let Some((damage, impact)) = entity::explosion_damage(power, mid.distance(center))
            && self.damage_player(damage, "was blown up by a creeper") > 0.0
        {
            let away = (mid - center).normalize_or(DVec3::Y);
            self.player.vel += away * (impact as f64 * 14.0) + DVec3::Y * 4.0;
        }
    }

    /// F3 line with entity counts.
    pub(super) fn mobs_debug_line(&self) -> String {
        let e = &self.mobs.entities;
        let (passive, hostile): (Vec<_>, Vec<_>) = MobKind::ALL.iter().partition(|k| !k.is_hostile());
        let total = |kinds: Vec<&MobKind>| kinds.into_iter().map(|&k| e.count(k)).sum::<usize>();
        format!(
            "Entities: {} (passive: {}, hostile: {}, arrows: {}), {} rendered, {} falling blocks",
            e.mobs.len(),
            total(passive),
            total(hostile),
            e.arrows.len(),
            e.rendered,
            self.world.falling_blocks().len()
        )
    }
}
