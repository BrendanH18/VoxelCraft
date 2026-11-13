//! The reward emerging from a partly brushed suspicious block, placed as
//! Java 26.3's `BrushableBlockRenderer` places it. Drawn with the shared
//! item sprites and block models; no imported artwork.
use glam::{DVec3, IVec3};

use super::BlockModel;
use crate::world::{World, archaeology, block::Block};

/// Java item models are one block wide; the renderer scales them by half.
const SIZE: f64 = 0.5;

/// Java's `translations`: the item centre inside the cell for the brushed
/// face and dust stage, before the renderer's 0.5 lift.
fn translation(face: IVec3, dust: u8) -> DVec3 {
    let f = f64::from(dust as f32 / 10.0 * 0.75);
    let mut t = DVec3::new(0.5, 0.0, 0.5);
    match face.to_array() {
        [1, 0, 0] => t.x = 0.73 + f,
        [-1, 0, 0] => t.x = 0.25 - f,
        [0, 1, 0] => t.y = 0.25 + f,
        [0, -1, 0] => t.y = -0.23 - f,
        [0, 0, -1] => t.z = 0.25 - f,
        [0, 0, 1] => t.z = 0.73 + f,
        _ => {}
    }
    t
}

/// Rewards of partly brushed blocks within `range` of `camera`.
pub fn emerging_items(world: &World, camera: DVec3, range: f64, out: &mut Vec<BlockModel>) {
    for (cell, stack, face) in world.emerging_items() {
        let Some((_, dust)) = world.get_block(cell).and_then(archaeology::decode) else { continue };
        if dust == 0 || (cell.as_dvec3() + DVec3::splat(0.5)).distance_squared(camera) > range * range {
            continue;
        }
        // Lit from the brushed face's neighbour, as Java's renderer is.
        let lit = cell + face;
        let sky_light = crate::entity::sky_light(world, lit.as_dvec3() + DVec3::splat(0.5));
        let block_light = world.block_light(lit) as f32 / 15.0;
        let block = stack.item.block().filter(|b| !b.flat_icon());
        let icon = match block {
            Some(_) => None,
            None => stack.item.block().map(|b| b.info().tex[0]).or_else(|| stack.item.icon_layer()),
        };
        // Java turns 75° then 11° (plus 90° on the east-west axis) about +Y;
        // a positive `BlockModel` yaw turns the other way.
        let degrees = 75.0 + 11.0 + if face.x != 0 { 90.0 } else { 0.0 };
        let centre = cell.as_dvec3() + translation(face, dust) + DVec3::new(0.0, 0.5, 0.0);
        out.push(BlockModel {
            min: centre - DVec3::splat(SIZE / 2.0),
            size: SIZE as f32,
            block: block.unwrap_or(Block::AIR),
            sky_light,
            block_light,
            yaw: -(degrees as f32).to_radians(),
            icon,
            orient: None,
            tint: crate::inventory::components::dye_tint_bytes(stack),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{archaeology::Table, chunk::ChunkData, terrain::Generator};
    use std::sync::Arc;

    #[test]
    fn rewards_emerge_from_the_brushed_face_as_dust_grows() {
        let at = IVec3::new(8, 145, 8);
        let mut w = World::new_remote(Arc::new(Generator::new(42)), 2);
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        w.set_block(at - IVec3::Y, Block::STONE);
        w.set_block(at, archaeology::SAND);
        w.set_archaeology_loot(at, Table::Cold, 42);
        let mut models = Vec::new();
        emerging_items(&w, at.as_dvec3(), 64.0, &mut models);
        assert!(models.is_empty(), "nothing shows before the first stroke");
        let mut last = f64::MIN;
        for stroke in 0..9 {
            w.maps.game_ticks = stroke * 10;
            w.brush_block(at, IVec3::X);
            models.clear();
            emerging_items(&w, at.as_dvec3(), 64.0, &mut models);
            assert_eq!(models.len(), 1);
            let centre = models[0].min + DVec3::splat(SIZE / 2.0) - at.as_dvec3();
            assert!(centre.x >= last && centre.x > 0.73, "{centre}");
            assert!((centre.y - 0.5).abs() < 1e-9 && (centre.z - 0.5).abs() < 1e-9);
            last = centre.x;
        }
        assert!((last - (0.73 + 0.225)).abs() < 1e-6, "dust 3 pushes the reward 0.225 further out");
        models.clear();
        emerging_items(&w, at.as_dvec3() + DVec3::splat(100.0), 64.0, &mut models);
        assert!(models.is_empty(), "out of range");
    }

    #[test]
    fn java_translations_for_every_face() {
        assert_eq!(translation(IVec3::NEG_X, 0), DVec3::new(0.25, 0.0, 0.5));
        assert_eq!(translation(IVec3::NEG_Z, 0), DVec3::new(0.5, 0.0, 0.25));
        assert!((translation(IVec3::NEG_Y, 2).y - (-0.23 - 0.15)).abs() < 1e-6);
        assert!((translation(IVec3::Y, 1).y - 0.325).abs() < 1e-6);
        assert!((translation(IVec3::Z, 3).z - 0.955).abs() < 1e-6);
    }
}
