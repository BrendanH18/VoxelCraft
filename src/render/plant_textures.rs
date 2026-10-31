//! Procedural textures for the v0.13 biome plants (layers from
//! `world::biome_plants::TEX`): original artwork. Leaf litter is a scatter of
//! autumn leaves and wildflowers a scatter of blooms, both denser with each
//! segment; the dry grasses are straw-coloured and the firefly bush carries
//! glowing yellow-green specks.

use super::{Rgba, SIZE, rnd, shade};
use crate::world::biome_plants::t;

const CLEAR: Rgba = [0, 0, 0, 0];
/// Autumn leaf colours for the litter.
const LEAVES: [[u8; 3]; 4] = [[160, 98, 38], [128, 82, 34], [176, 124, 44], [104, 96, 36]];
/// Bloom colours for the wildflowers: yellow, white, blue and pink.
const BLOOMS: [[u8; 3]; 4] = [[238, 206, 52], [240, 238, 232], [96, 126, 226], [236, 140, 196]];
const STRAW: [u8; 3] = [186, 162, 96];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        l if (t::LEAF_LITTER..t::LEAF_LITTER + 4).contains(&l) => {
            let density = 0.3 + 0.2 * (l - t::LEAF_LITTER) as f32;
            if rnd(layer, x / 2, y / 2, 3) < density {
                let c = LEAVES[(rnd(layer, x / 2, y / 2, 5) * 4.0) as usize % 4];
                shade(c, 0.82 + r * 0.3)
            } else {
                CLEAR
            }
        }
        l if (t::WILDFLOWERS..t::WILDFLOWERS + 4).contains(&l) => {
            let count = (l - t::WILDFLOWERS) as usize + 1;
            // Stems and leaves on the ground, with `count` blooms.
            let bloom = (0..count).find(|&i| {
                let cx = 2 + (rnd(l, i, 0, 8) * 12.0) as usize;
                let cy = 2 + (rnd(l, i, 1, 8) * 12.0) as usize;
                x.abs_diff(cx) + y.abs_diff(cy) <= 1
            });
            if let Some(i) = bloom {
                let centre = x.abs_diff(2 + (rnd(l, i, 0, 8) * 12.0) as usize) == 0
                    && y.abs_diff(2 + (rnd(l, i, 1, 8) * 12.0) as usize) == 0;
                if centre { shade([248, 214, 70], 1.0) } else { shade(BLOOMS[i % 4], 0.9 + r * 0.2) }
            } else if rnd(l, x / 2, y / 2, 3) < 0.25 + 0.05 * count as f32 {
                shade([58, 122, 44], 0.75 + r * 0.3)
            } else {
                CLEAR
            }
        }
        t::BUSH => {
            // A rounded little shrub of leaves low in the cell.
            let d = ((x as f32 - 7.5).powi(2) / 49.0) + ((y as f32 - 11.0).powi(2) / 25.0);
            if d < 1.0 && rnd(layer, x, y, 3) < 0.85 { shade([54, 110, 40], 0.7 + r * 0.4) } else { CLEAR }
        }
        t::FIREFLY_BUSH => {
            let d = ((x as f32 - 7.5).powi(2) / 49.0) + ((y as f32 - 10.0).powi(2) / 36.0);
            if d >= 1.0 || rnd(layer, x, y, 3) > 0.85 {
                CLEAR
            } else if rnd(layer, x / 2, y / 2, 6) < 0.12 {
                shade([214, 240, 96], 1.1)
            } else {
                shade([46, 96, 52], 0.7 + r * 0.4)
            }
        }
        t::SHORT_DRY_GRASS | t::TALL_DRY_GRASS | t::TALL_DRY_GRASS_TOP => {
            let height = match layer {
                t::SHORT_DRY_GRASS => 7,
                t::TALL_DRY_GRASS => SIZE,
                _ => 11,
            };
            // Blades: columns that rise to a hashed height, leaning slightly.
            let blade = rnd(layer, x, 9, 4);
            let tip = SIZE - height + (blade * 5.0) as usize;
            let present = (x % 3 != 1 || rnd(layer, x, 2, 7) > 0.5) && y >= tip;
            if present { shade(STRAW, 0.78 + r * 0.34) } else { CLEAR }
        }
        t::CACTUS_FLOWER => {
            // A pink bloom with a yellow heart on a short green base.
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 9.0);
            let d = (dx * dx + dy * dy).sqrt();
            if d < 1.8 {
                shade([250, 224, 72], 1.0)
            } else if d < 4.4 && (dx.abs() + dy.abs()) % 3.0 > 0.4 {
                shade([238, 96, 168], 0.85 + r * 0.25)
            } else if y >= 13 && (6..=9).contains(&x) {
                shade([50, 120, 50], 0.8 + r * 0.2)
            } else {
                CLEAR
            }
        }
        _ => [255, 0, 255, 255],
    }
}
