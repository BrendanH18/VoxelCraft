//! v0.16 Track B presentation: shulker boxes drawn as block-entity models
//! with their lids, their open and close sounds, and the local player's
//! elytra wind.

use glam::DVec3;

use super::Game;
use crate::audio::sounds::Sound;
use crate::render::BlockModel;
use voxelcraft::content::fireworks::Cue;

impl Game {
    /// Shulker boxes within `ShulkerBoxRenderer`'s view distance of
    /// `camera`, lids interpolated by `alpha`.
    pub(super) fn shulker_models(&self, camera: DVec3, alpha: f64) -> impl Iterator<Item = BlockModel> + '_ {
        self.world.shulker_boxes_near(camera).map(move |(cell, block)| {
            let centre = cell.as_dvec3() + DVec3::splat(0.5);
            BlockModel {
                min: cell.as_dvec3(),
                size: 1.0,
                block,
                sky_light: crate::entity::sky_light(&self.world, centre),
                block_light: self.torch_light(centre),
                yaw: 0.0,
                icon: None,
                orient: None,
                tint: [255; 4],
                patterns: None,
                lid: Some(self.world.shulker_lid(cell, alpha as f32)),
            }
        })
    }

    /// `FireworkParticles.Starter.isFarAwayFromCamera`: blasts and twinkles
    /// 16 blocks or more from the camera use their `_far` sounds.
    pub(super) fn far_firework(&self, cue: Cue, pos: DVec3) -> Cue {
        let far = self.session.player.eye().distance_squared(pos) >= 256.0;
        match cue {
            Cue::Blast if far => Cue::BlastFar,
            Cue::LargeBlast if far => Cue::LargeBlastFar,
            Cue::Twinkle if far => Cue::TwinkleFar,
            cue => cue,
        }
    }

    /// `ElytraOnPlayerSoundInstance` for the local player: after a second of
    /// gliding the wind fades in over a second, as loud as the speed squared
    /// over four, higher above 0.8. Played as overlapping stretches rather
    /// than one tickable loop.
    pub(super) fn elytra_wind(&mut self) {
        let p = &self.session.player;
        if !p.fall_flying || p.fall_fly_ticks < 20 || !p.fall_fly_ticks.is_multiple_of(10) {
            return;
        }
        let speed = ((p.vel / 20.0).length_squared() as f32).max(0.0);
        let mut volume = if speed >= 1.0e-7 { (speed / 4.0).clamp(0.0, 1.0) } else { 0.0 };
        if p.fall_fly_ticks < 40 {
            volume *= (p.fall_fly_ticks - 20) as f32 / 20.0;
        }
        if volume <= 0.0 {
            return;
        }
        let pitch = if volume > 0.8 { 1.0 + (volume - 0.8) } else { 1.0 };
        let at = p.pos;
        self.audio.play(Sound::Flight(Cue::ElytraFlying), Some(at), volume, (pitch, pitch));
    }

    /// Plays the shulker boxes' lid sounds since the last tick
    /// (`SHULKER_BOX_OPEN`/`CLOSE`: volume 0.5, pitch 0.9 to 1.0).
    pub(super) fn play_shulker_sounds(&mut self) {
        for cue in self.world.take_shulker_cues() {
            let sound = Sound::Flight(if cue.open { Cue::ShulkerOpen } else { Cue::ShulkerClose });
            self.audio.play(sound, Some(cue.pos.as_dvec3() + DVec3::splat(0.5)), 0.5, (0.9, 1.0));
        }
    }
}
