//! v0.16 Track C glue: the Wither's blasts, block breaking and wither
//! roses in the world, and its summoning from placed skulls.
use glam::{DVec3, IVec3};

use crate::audio::sounds::Sound;
use crate::world::block::Block;
use voxelcraft::entity::wither;
use voxelcraft::world::wither_rose::WITHER_ROSE;

use super::Game;
use crate::render::ui::Ui;

/// The Wither's purple boss bar at `y` (`BossBarColor.PURPLE`).
pub(super) fn boss_bar_ui(ui: &mut Ui, progress: f32, sw: f32, y: f32) {
    const PURPLE: [f32; 4] = [0.66, 0.2, 0.86, 1.0];
    let (w, x) = (182.0, ((sw - 182.0) / 2.0).floor());
    let name = "Wither";
    ui.text(((sw - Ui::text_width(name)) / 2.0).floor(), y - 9.0, name, [1.0, 1.0, 1.0, 1.0]);
    ui.rect(x, y, w, 5.0, [0.0, 0.0, 0.0, 0.8]);
    ui.rect(x + 1.0, y + 1.0, w - 2.0, 3.0, [0.2, 0.06, 0.26, 0.9]);
    let fill = ((w - 2.0) * progress.clamp(0.0, 1.0)).round();
    if fill > 0.0 {
        ui.rect(x + 1.0, y + 1.0, fill, 3.0, PURPLE);
        ui.rect(x + 1.0, y + 1.0, fill, 1.0, [0.86, 0.6, 1.0, 1.0]);
    }
}

impl Game {
    /// `WitherSkullBlock.checkSpawn` for the skulls placed this tick.
    pub(super) fn finish_withers(&mut self) {
        let peaceful = self.difficulty == voxelcraft::simulation::difficulty::Difficulty::Peaceful;
        wither::finish_withers(&mut self.world, &mut self.mobs.entities, peaceful);
    }

    /// The Wither's charge-up blast or a skull's: a mob explosion
    /// (`ExplosionInteraction.MOB`) that spares the Wither, with a
    /// dangerous skull's 0.8 resistance cap.
    /// v1.0 gap Track 8e: `owner` is the Wither behind it (the blast's
    /// source, or the skull's owner), which TNT it sets off takes as its own.
    pub(super) fn wither_explode(&mut self, center: DVec3, power: f32, dangerous: bool, owner: Option<u32>) {
        self.world.emit_game_event(
            center,
            crate::world::vibration::GameEvent::Explode,
            crate::world::vibration::EventSource::NONE,
            None,
        );
        let blast = self.gamerules.explosion(voxelcraft::rules::ExplosionSource::Mob);
        let cells =
            if blast.destroys_blocks { self.world.wither_exploded_cells(center, power, dangerous) } else { Vec::new() };
        let mut events = Vec::new();
        self.mobs.entities.wither_blast_mobs(&self.world, center, power, blast.destroys_blocks, &mut events);
        let cause = wither::BLAST_CAUSE;
        // A player watching the credits isn't in the level (v1.0 gap Track 8e).
        if self.credits.is_none()
            && let Some((damage, push)) = self.session.blast(&self.world, center, power)
        {
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
            self.world.detonate(&cells, power, self.gamerules.bool("block_drops"), blast.drop_decay, false);
        }
        for (cell, short_fuse) in std::mem::take(&mut self.world.primed_tnt) {
            self.mobs.entities.prime_tnt(cell, short_fuse, owner.map(crate::entity::trident::Owner::Mob));
        }
        self.audio.play(Sound::Explosion, Some(center), 1.0, (0.9, 1.05));
        for e in events {
            if let voxelcraft::entity::EntityEvent::WitherRose { pos } = e {
                self.wither_rose(pos);
            }
        }
    }

    /// `destroyBlocksTick` running out: every block in the box the Wither
    /// can destroy breaks with its drops, and level event 1022 plays.
    pub(super) fn wither_break(&mut self, min: IVec3, max: IVec3, pos: DVec3) {
        if !self.gamerules.bool("mob_griefing") {
            return;
        }
        let mut destroyed = false;
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let cell = IVec3::new(x, y, z);
                    let Some(b) = self.world.get_block(cell).filter(|&b| wither::can_destroy(b)) else { continue };
                    if b.is_fluid() {
                        continue;
                    }
                    self.world.spill_block(cell, b);
                    if self.world.set_block(cell, Block::AIR) {
                        self.world.particles.push(crate::particles::Request::Break { cell, block: b });
                        destroyed = true;
                    }
                }
            }
        }
        if destroyed {
            self.audio.play(Sound::Wither(wither::Cue::BreakBlocks), Some(pos), 1.0, (0.9, 1.1));
        }
    }

    /// `LivingEntity.createWitherRose`: a rose where the Wither's victim
    /// died, if the cell is air and can hold one under `mobGriefing`;
    /// otherwise a rose item.
    pub(super) fn wither_rose(&mut self, pos: DVec3) {
        let cell = pos.floor().as_ivec3();
        let placed = self.gamerules.bool("mob_griefing")
            && self.world.get_block(cell) == Some(Block::AIR)
            && self.world.get_block(cell - IVec3::Y).is_some_and(voxelcraft::world::wither_rose::supports)
            && self.world.set_block(cell, WITHER_ROSE);
        if !placed {
            self.mobs.entities.scatter(voxelcraft::inventory::Stack::new(WITHER_ROSE, 1), pos);
        }
    }
}
