//! Drawing banner patterns on screen (v0.15 Track B): the loom's choices
//! and preview, and patterned banner and shield icons. Each layer is the
//! pattern's two mask layers (`world::workstations::pattern_layer`), tinted
//! with its dye, over the base colour, as `BannerRenderer` stacks them.

use crate::color::DyeColor;
use crate::inventory::Stack;
use crate::item::Item;
use crate::render::ui::{Color, Ui};
use voxelcraft::content::banner_patterns as bp;
use voxelcraft::inventory::components::BannerLayer;
use voxelcraft::world::workstations::pattern_layer;

/// A dye as a UI colour: its sRGB value, which `Ui` converts to linear
/// like every UI colour (`render::ui::linear`).
pub(super) fn dye_color(c: DyeColor) -> Color {
    let [r, g, b] = c.rgb().map(|v| f32::from(v) / 255.0);
    [r, g, b, 1.0]
}

/// A flag `w` × `h` at (`x`, `y`): the base colour, then each layer's mask.
pub(super) fn draw_flag(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, base: DyeColor, layers: &[BannerLayer]) {
    ui.rect(x, y, w, h, dye_color(base));
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for l in layers {
        for half in 0..2u8 {
            let top = y + h / 2.0 * f32::from(half);
            let quad = [[x, top], [x + w, top], [x + w, top + h / 2.0], [x, top + h / 2.0]];
            ui.quad(quad, uv, f32::from(pattern_layer(l.pattern, half)), dye_color(l.color));
        }
    }
}

/// Draws a patterned banner or shield's icon over the plain one at
/// (`x`, `y`) (an 18-pixel slot). Plain stacks draw nothing extra.
pub(super) fn draw_icon_patterns(ui: &mut Ui, x: f32, y: f32, stack: &Stack) {
    let p = bp::patterns(stack);
    if p.layers.is_empty() && p.base.is_none() {
        return;
    }
    if let Some(base) = bp::banner_color(stack.item) {
        // Over the banner icon's cloth (`colour_textures::banner_icon`).
        draw_flag(ui, x + 6.0, y + 3.0, 6.0, 12.0, base, &p.layers);
    } else if stack.item == Item::SHIELD {
        // Over the shield's face, inside its rim.
        draw_flag(ui, x + 5.0, y + 4.0, 8.0, 10.0, p.base.unwrap_or(DyeColor::White), &p.layers);
    }
}
