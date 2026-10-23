//! The Distortion Effects option's state: Java 26.3's nausea blend factor
//! (`MobEffectInstance.BlendState`), the portal effect intensity and the
//! spinning angle (`LocalPlayer.tickSpinningEffect`) that drive the view
//! warp, and the green overlay that stands in for the warp when the option
//! is turned down (`Hud.extractConfusionOverlay`).

use glam::Mat4;

/// Nausea's blend: in over 150 ticks, out over 20 ticks, starting 60 ticks
/// before it ends (`setBlendDuration(150, 20, 60)`).
const NAUSEA_IN: f32 = 150.0;
const NAUSEA_OUT: f32 = 20.0;
const NAUSEA_ADVANCE: u32 = 60;
/// Degrees per tick the warp turns for a portal and for nausea.
const PORTAL_SPEED: f32 = 20.0;
const NAUSEA_SPEED: f32 = 7.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Distortion {
    /// Last tick's and this tick's nausea blend factor, 0..1.
    nausea: (f32, f32),
    /// Last tick's and this tick's portal intensity, 0..1.
    portal: (f32, f32),
    spin_time: f32,
    spin_speed: f32,
}

impl Distortion {
    /// One game tick: `nausea` is the effect's remaining ticks, if any, and
    /// `in_portal` whether the player stood in a Nether portal this tick.
    pub fn tick(&mut self, nausea: Option<u32>, in_portal: bool) {
        let target = if nausea.is_some_and(|t| t > NAUSEA_ADVANCE) { 1.0 } else { 0.0 };
        let step = if target > self.nausea.1 { 1.0 / NAUSEA_IN } else { 1.0 / NAUSEA_OUT };
        let n = self.nausea.1 + (target - self.nausea.1).clamp(-step, step);
        self.nausea = (self.nausea.1, n);
        // `handlePortalTransitionEffect`.
        let p = if in_portal { self.portal.1 + 0.0125 } else { self.portal.1 - 0.05 };
        self.portal = (self.portal.1, p.clamp(0.0, 1.0));
        let (portal, nausea) = (self.portal.1, self.nausea.1);
        if portal > 0.0 || nausea > 0.0 {
            self.spin_speed = (portal * PORTAL_SPEED + nausea * NAUSEA_SPEED) / (portal + nausea);
            self.spin_time += self.spin_speed;
        } else {
            self.spin_speed = 0.0;
        }
    }

    pub fn nausea(&self, alpha: f32) -> f32 {
        self.nausea.0 + (self.nausea.1 - self.nausea.0) * alpha
    }

    pub fn portal(&self, alpha: f32) -> f32 {
        self.portal.0 + (self.portal.1 - self.portal.0) * alpha
    }

    /// The view warp at `alpha` through the tick for the Distortion Effects
    /// option `scale` (0..1).
    pub fn warp(&self, alpha: f32, scale: f32) -> Mat4 {
        let intensity = self.portal(alpha).max(self.nausea(alpha)) * scale * scale;
        voxelcraft::camera::spin_warp(intensity, self.spin_time + alpha * self.spin_speed)
    }

    /// The strength of the green nausea overlay Java draws instead of the
    /// warp as the option goes down (none while a portal's purple shows).
    pub fn overlay(&self, alpha: f32, scale: f32) -> f32 {
        if self.portal(alpha) > 0.0 || scale >= 1.0 { 0.0 } else { self.nausea(alpha) * (1.0 - scale) }
    }

    /// Back to no effect (respawning, changing dimension).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nausea_blends_in_slowly_and_out_before_it_ends() {
        let mut d = Distortion::default();
        for t in (0..400).rev() {
            d.tick(Some(t + 1), false);
            if t == 400 - 75 {
                assert!((d.nausea(1.0) - 0.5).abs() < 0.01, "halfway in after 75 ticks");
            }
        }
        // The last 60 ticks fade out over 20.
        assert_eq!(d.nausea(1.0), 0.0);
        let mut d = Distortion::default();
        for _ in 0..200 {
            d.tick(Some(1000), false);
        }
        assert_eq!(d.nausea(1.0), 1.0);
        d.tick(Some(60), false);
        assert!((d.nausea(1.0) - 0.95).abs() < 1e-5);
    }

    #[test]
    fn the_warp_follows_the_option_and_the_overlay_replaces_it() {
        let mut d = Distortion::default();
        for _ in 0..200 {
            d.tick(Some(1000), false);
        }
        assert_ne!(d.warp(0.5, 1.0), Mat4::IDENTITY);
        assert_eq!(d.warp(0.5, 0.0), Mat4::IDENTITY, "Distortion Effects OFF");
        assert_eq!(d.overlay(0.5, 1.0), 0.0);
        assert_eq!(d.overlay(0.5, 0.0), 1.0);
        assert!((d.overlay(0.5, 0.25) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn portals_build_up_over_eighty_ticks_and_fade_faster() {
        let mut d = Distortion::default();
        for _ in 0..80 {
            d.tick(None, true);
        }
        assert!((d.portal(1.0) - 1.0).abs() < 1e-4);
        for _ in 0..20 {
            d.tick(None, false);
        }
        assert_eq!(d.portal(1.0), 0.0);
    }
}
