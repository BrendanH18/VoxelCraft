//! Original hand-designed sediment scratches, with no imported artwork.
use super::{Rgba, noisy, shade};
use crate::world::archaeology::t;
pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let local = layer - t::BASE;
    let dust = local % 4;
    let gravel = local >= 4;
    let colour = if gravel { [139, 132, 128] } else { [218, 198, 143] };
    let scratch = (x * 7 + y * 3) % 17 < 1 + dust as usize && !(x + y).is_multiple_of(5);
    if scratch {
        shade(colour, 0.62 + f32::from(dust) * 0.04)
    } else {
        noisy(layer, x, y, colour, if gravel { 0.19 } else { 0.09 })
    }
}
