//! Game-side glue for mobs: updating them, player melee, entity events and
//! the `--spawn` / `--wait` debug options.

use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::entity::{self, Entities, EntityEvent, MobKind};
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
            self.mobs.entities.attack(i, self.player.forward().as_dvec3());
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
                EntityEvent::PlayerHit { damage, knockback } => {
                    // Knockback only lands with damage, so hurt immunity
                    // also stops repeated shoves.
                    if self.damage_player(damage, "was slain by a zombie") > 0.0 {
                        self.player.vel += knockback.as_dvec3();
                    }
                }
            }
        }
    }
}

impl Game {
    /// F3 line with entity counts.
    pub(super) fn mobs_debug_line(&self) -> String {
        let e = &self.mobs.entities;
        format!(
            "Entities: {} (pigs: {}, zombies: {}), {} rendered, {} falling blocks",
            e.mobs.len(),
            e.count(MobKind::Pig),
            e.count(MobKind::Zombie),
            e.rendered,
            self.world.falling_blocks().len()
        )
    }
}
