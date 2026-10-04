//! Immediate-mode 2D UI geometry: rectangles, text and block icons,
//! built in pixel coordinates and drawn in one call.

use bytemuck::{Pod, Zeroable};

use crate::world::block::Block;
use crate::world::shape;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    /// Block texture layer (>= 0), or one of [`SOLID`] / [`GLYPH`].
    pub layer: f32,
    pub color: [f32; 4],
}

pub const SOLID: f32 = -1.0;
pub const GLYPH: f32 = -2.0;

/// Font atlas: 16x8 grid of 8x8 glyphs covering ASCII 0..128.
pub const FONT_ATLAS_W: u32 = 128;
pub const FONT_ATLAS_H: u32 = 64;

pub type Color = [f32; 4];
pub const WHITE: Color = [1.0, 1.0, 1.0, 1.0];

/// Rasterises the font atlas as R8 coverage.
pub fn font_atlas() -> Vec<u8> {
    let mut px = vec![0u8; (FONT_ATLAS_W * FONT_ATLAS_H) as usize];
    for (c, rows) in font8x8::legacy::BASIC_LEGACY.iter().enumerate() {
        let (gx, gy) = ((c % 16) * 8, (c / 16) * 8);
        for (y, row) in rows.iter().enumerate() {
            for x in 0..8 {
                if row >> x & 1 == 1 {
                    px[(gy + y) * FONT_ATLAS_W as usize + gx + x] = 255;
                }
            }
        }
    }
    px
}

/// Advance width of a glyph in font pixels (proportional, 1px gap).
fn glyph_advance(c: char) -> f32 {
    if c == ' ' {
        return 4.0;
    }
    let rows = font8x8::legacy::BASIC_LEGACY.get(c as usize).copied().unwrap_or([0; 8]);
    let bits = rows.iter().fold(0u8, |a, r| a | r);
    if bits == 0 {
        return 4.0;
    }
    let first = bits.trailing_zeros();
    let last = 7 - bits.leading_zeros();
    (last - first + 2) as f32
}

fn glyph_left(c: char) -> f32 {
    let rows = font8x8::legacy::BASIC_LEGACY.get(c as usize).copied().unwrap_or([0; 8]);
    let bits = rows.iter().fold(0u8, |a, r| a | r);
    if bits == 0 { 0.0 } else { bits.trailing_zeros() as f32 }
}

pub struct Ui {
    pub verts: Vec<UiVertex>,
    pub width: f32,
    pub height: f32,
    /// Physical pixels per UI pixel (like Minecraft's GUI scale).
    pub scale: f32,
}

impl Ui {
    pub fn new(width: f32, height: f32, dpi: f32) -> Self {
        Self { verts: Vec::with_capacity(4096), width, height, scale: Self::scale_for(dpi) }
    }

    /// A UI with an explicit scale (see [`Ui::fitted_scale`]).
    pub fn with_scale(width: f32, height: f32, scale: f32) -> Self {
        Self { verts: Vec::with_capacity(4096), width, height, scale }
    }

    /// Like Minecraft's automatic GUI scale: the display's scale, lowered
    /// (not below 1) until the view is at least 320x180 UI pixels, so small
    /// split-screen views keep their whole HUD on screen.
    pub fn fitted_scale(width: f32, height: f32, dpi: f32) -> f32 {
        let mut scale = Self::scale_for(dpi);
        while scale > 1.0 && (width / scale < 320.0 || height / scale < 180.0) {
            scale -= 1.0;
        }
        scale
    }

    /// Physical pixels per UI pixel for a display scale factor.
    pub fn scale_for(dpi: f32) -> f32 {
        (dpi * 1.5).round().max(1.0)
    }

    /// UI-pixel dimensions of the screen.
    pub fn size(&self) -> (f32, f32) {
        (self.width / self.scale, self.height / self.scale)
    }

    fn ndc(&self, x: f32, y: f32) -> [f32; 2] {
        let (x, y) = ((x * self.scale).round(), (y * self.scale).round());
        [x / self.width * 2.0 - 1.0, 1.0 - y / self.height * 2.0]
    }

    /// Arbitrary quad in UI pixels, corners clockwise from top-left.
    pub fn quad(&mut self, p: [[f32; 2]; 4], uv: [[f32; 2]; 4], layer: f32, color: Color) {
        let v: [UiVertex; 4] =
            std::array::from_fn(|i| UiVertex { pos: self.ndc(p[i][0], p[i][1]), uv: uv[i], layer, color });
        self.verts.extend_from_slice(&[v[0], v[1], v[2], v[0], v[2], v[3]]);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.quad([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], [[0.0; 2]; 4], SOLID, color);
    }

    /// Square HUD icon (heart, bubble, ...) from a block texture or item icon layer.
    pub fn icon(&mut self, x: f32, y: f32, size: f32, layer: impl Into<u16>, color: Color) {
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.quad([[x, y], [x + size, y], [x + size, y + size], [x, y + size]], uv, layer.into() as f32, color);
    }

    /// Isometric block icon filling a `size` square: a cube, a lowered one
    /// for low blocks, or the boxes of a shaped block.
    pub fn block_icon(&mut self, x: f32, y: f32, size: f32, block: Block) {
        let t = block.info().tex;
        if block.flat_icon() {
            // Plants, torches and ladders show their flat sprite, like items.
            self.icon(x - 1.0, y - 1.0, size + 2.0, t[0], WHITE);
            return;
        }
        let mut boxes = shape::item_shape(block);
        if boxes.is_empty() {
            boxes = shape::Boxes::from_box(shape::Box16 { min: [0; 3], max: [16, 16 - block.top_drop(), 16] });
        }
        // Half width `s`: the top diamond is `s` tall, each side face `s` tall.
        let (l, s) = (x, size / 2.0);
        let q = s / 2.0;
        // A point of the unit cube in icon pixels: +X runs down-right, +Z
        // up-right, +Y up. The top, -Z (left) and +X (right) faces show.
        let at = |p: [f32; 3]| [l + s * (p[0] + p[2]), y + q * (1.0 + p[0] - p[2]) + s * (1.0 - p[1])];
        let shade = |f: f32| [f, f, f, 1.0];
        // Far boxes first, so nearer ones paint over them.
        let mut order: Vec<_> = boxes.as_slice().to_vec();
        order.sort_by_key(|b| {
            b.min[0] as i32 + b.max[0] as i32 + b.min[1] as i32 + b.max[1] as i32 - b.min[2] as i32 - b.max[2] as i32
        });
        for b in order {
            let (lo, hi) = (b.min.map(|c| c as f32 / 16.0), b.max.map(|c| c as f32 / 16.0));
            let top = [[lo[0], hi[1], lo[2]], [lo[0], hi[1], hi[2]], [hi[0], hi[1], hi[2]], [hi[0], hi[1], lo[2]]];
            let left = [[lo[0], hi[1], lo[2]], [hi[0], hi[1], lo[2]], [hi[0], lo[1], lo[2]], [lo[0], lo[1], lo[2]]];
            let right = [[hi[0], hi[1], lo[2]], [hi[0], hi[1], hi[2]], [hi[0], lo[1], hi[2]], [hi[0], lo[1], lo[2]]];
            // UVs as on the cube: (z, x) on top, (x or z, 1 - y) on the sides.
            let face = |c: [[f32; 3]; 4], uv: fn([f32; 3]) -> [f32; 2]| (c.map(at), c.map(uv));
            for ((p, uv), layer, colour) in [
                (face(top, |p| [p[2], p[0]]), t[2], WHITE),
                (face(left, |p| [p[0], 1.0 - p[1]]), t[5], shade(0.8)),
                (face(right, |p| [p[2], 1.0 - p[1]]), t[0], shade(0.62)),
            ] {
                self.quad(p, uv, layer as f32, colour);
            }
        }
    }

    pub fn text_width(s: &str) -> f32 {
        s.chars().map(glyph_advance).sum::<f32>() - 1.0
    }

    /// Draws text with Minecraft-style drop shadow; returns its width.
    pub fn text(&mut self, x: f32, y: f32, s: &str, color: Color) -> f32 {
        let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]];
        self.text_raw(x + 1.0, y + 1.0, s, shadow);
        self.text_raw(x, y, s, color)
    }

    /// Shadowed text magnified `k` times (titles); returns its width.
    pub fn text_scaled(&mut self, x: f32, y: f32, s: &str, color: Color, k: f32) -> f32 {
        let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]];
        self.glyphs(x + k, y + k, s, shadow, k);
        self.glyphs(x, y, s, color, k)
    }

    /// Text without a shadow (e.g. titles on light panels).
    pub fn text_flat(&mut self, x: f32, y: f32, s: &str, color: Color) -> f32 {
        self.text_raw(x, y, s, color)
    }

    fn text_raw(&mut self, x: f32, y: f32, s: &str, color: Color) -> f32 {
        self.glyphs(x, y, s, color, 1.0)
    }

    fn glyphs(&mut self, x: f32, y: f32, s: &str, color: Color, k: f32) -> f32 {
        let mut pen = x;
        for c in s.chars() {
            let c = if (c as u32) < 128 { c } else { '?' };
            if c != ' ' {
                let (gx, gy) = ((c as u32 % 16) as f32 * 8.0, (c as u32 / 16) as f32 * 8.0);
                let (u0, v0) = (gx / FONT_ATLAS_W as f32, gy / FONT_ATLAS_H as f32);
                let (u1, v1) = ((gx + 8.0) / FONT_ATLAS_W as f32, (gy + 8.0) / FONT_ATLAS_H as f32);
                let (left, size) = (pen - glyph_left(c) * k, 8.0 * k);
                self.quad(
                    [[left, y], [left + size, y], [left + size, y + size], [left, y + size]],
                    [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                    GLYPH,
                    color,
                );
            }
            pen += glyph_advance(c) * k;
        }
        pen - x
    }

    /// Text on a translucent backing panel, like Minecraft's F3 screen.
    pub fn label(&mut self, x: f32, y: f32, s: &str, color: Color) {
        let w = Self::text_width(s);
        self.rect(x - 1.0, y - 1.0, w + 2.0, 10.0, [0.0, 0.0, 0.0, 0.4]);
        self.text_raw(x, y, s, color);
    }
}

#[cfg(test)]
mod scale_tests {
    use super::Ui;

    #[test]
    fn small_views_lower_the_gui_scale() {
        // A full 1600x900 window at 2x keeps the display's scale.
        assert_eq!(Ui::fitted_scale(1600.0, 900.0, 2.0), 3.0);
        // Stacked halves and quarters drop a step to stay 320x180.
        assert_eq!(Ui::fitted_scale(1600.0, 450.0, 2.0), 2.0);
        assert_eq!(Ui::fitted_scale(800.0, 450.0, 2.0), 2.0);
        // Never below one physical pixel per UI pixel.
        assert_eq!(Ui::fitted_scale(200.0, 100.0, 2.0), 1.0);
    }
}
