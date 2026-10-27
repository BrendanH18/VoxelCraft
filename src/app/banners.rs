//! Banners draw as free-standing models, as Java's `BannerRenderer` draws
//! them, within its block entity view distance of the camera.

use glam::DVec3;

use super::Game;
use crate::render::BlockModel;

/// `BlockEntityRenderer.getViewDistance`'s default, in blocks.
const VIEW_DISTANCE: f64 = 64.0;

impl Game {
    /// Banners within the view distance of `camera` (each split-screen view
    /// has its own).
    pub(super) fn banner_models(&self, camera: DVec3) -> Vec<BlockModel> {
        let eye = camera;
        self.world
            .banners()
            .filter(|(cell, _)| (cell.as_dvec3() + DVec3::splat(0.5)).distance_squared(eye) < VIEW_DISTANCE.powi(2))
            .map(|(cell, block)| {
                let centre = cell.as_dvec3() + DVec3::splat(0.5);
                BlockModel {
                    min: cell.as_dvec3(),
                    size: 1.0,
                    block,
                    sky_light: crate::entity::sky_light(&self.world, centre),
                    block_light: self.torch_light(centre),
                    yaw: 0.0,
                    icon: None,
                }
            })
            .collect()
    }
}
