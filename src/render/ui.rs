//! Immediate-mode 2D UI geometry: rectangles, text and block icons,
//! built in pixel coordinates and drawn in one call.

use bytemuck::{Pod, Zeroable};

use crate::world::block::Block;
use crate::world::shape;
use voxelcraft::font;

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

pub type Color = [f32; 4];
pub const WHITE: Color = [1.0, 1.0, 1.0, 1.0];

/// An opaque UI colour from its sRGB hex value, the number a capture shows
/// (`rgb(0xC6C6C6)` is Java's panel grey).
pub const fn rgb(hex: u32) -> Color {
    [((hex >> 16) & 0xFF) as f32 / 255.0, ((hex >> 8) & 0xFF) as f32 / 255.0, (hex & 0xFF) as f32 / 255.0, 1.0]
}

/// One sRGB channel (0..1) as linear light: the sRGB transfer function the
/// GPU inverts when it writes to the sRGB surface, so a value converted here
/// shows on screen as written.
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

/// The shared UI colour conversion (art guide, "Colour pipeline"): every
/// UI and HUD colour, flat or a texture tint, is written as sRGB and
/// becomes linear here, as [`Ui::quad`] does for everything it draws.
/// Alpha stays linear. A tint multiplies the texture in linear light,
/// which is close to multiplying the sRGB values as Java's GUI does.
pub fn linear(c: Color) -> Color {
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]]
}

/// The font atlas: every glyph of [`voxelcraft::font`] as R8 coverage.
pub fn font_atlas() -> (Vec<u8>, (u32, u32)) {
    let f = font::font();
    (f.atlas(), f.atlas_size())
}

/// Shadow colour of text: Java's `ARGB.scaleRGB(color, 0.25)`.
fn shadow(color: Color) -> Color {
    [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]]
}

pub struct Ui {
    pub verts: Vec<UiVertex>,
    pub width: f32,
    pub height: f32,
    /// Physical pixels per UI pixel (like Minecraft's GUI scale).
    pub scale: f32,
    /// The last colour [`linear`] converted and its result: text and panels
    /// repeat one colour for many quads.
    converted: (Color, Color),
}

impl Ui {
    /// A UI with an explicit scale (see [`Ui::gui_scale`] and [`Ui::view_scale`]).
    pub fn with_scale(width: f32, height: f32, scale: f32) -> Self {
        Self { verts: Vec::with_capacity(4096), width, height, scale, converted: (WHITE, WHITE) }
    }

    /// The largest GUI scale that leaves at least 320x240 UI pixels, at
    /// least 1 (Java's `Window.calculateScale(0, false)`). It's the top of
    /// the GUI Scale option and what Auto picks.
    pub fn max_gui_scale(width: f32, height: f32) -> u32 {
        let mut scale = 1;
        while width / (scale + 1) as f32 >= 320.0 && height / (scale + 1) as f32 >= 240.0 {
            scale += 1;
        }
        scale
    }

    /// Java's GUI scale for a window `width` x `height` physical pixels:
    /// `setting` (0 is Auto, the largest that fits), never more than
    /// [`Ui::max_gui_scale`].
    pub fn gui_scale(width: f32, height: f32, setting: u32) -> f32 {
        let max = Self::max_gui_scale(width, height);
        (if setting == 0 { max } else { setting.min(max) }) as f32
    }

    /// The scale for one split-screen view of a window: the window's GUI
    /// scale, lowered (not below 1) until the view is at least 320x180 UI
    /// pixels, so small views keep their whole HUD on screen. A view filling
    /// the window gets the window's scale.
    pub fn view_scale((vw, vh): (f32, f32), (ww, wh): (f32, f32), setting: u32) -> f32 {
        let mut scale = Self::gui_scale(ww, wh, setting);
        while scale > 1.0 && (vw / scale < 320.0 || vh / scale < 180.0) {
            scale -= 1.0;
        }
        scale
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
    /// `color` is sRGB (see [`linear`]).
    pub fn quad(&mut self, p: [[f32; 2]; 4], uv: [[f32; 2]; 4], layer: f32, color: Color) {
        let color = if self.converted.0 == color {
            self.converted.1
        } else {
            self.converted = (color, linear(color));
            self.converted.1
        };
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

    /// Fills the shape of an icon (its texture's alpha) with `color`.
    pub fn icon_shape(&mut self, x: f32, y: f32, size: f32, layer: u16, uv: [f32; 4], color: Color) {
        let [u0, v0, u1, v1] = uv;
        let (px0, py0, px1, py1) = (x + u0 * size, y + v0 * size, x + u1 * size, y + v1 * size);
        let uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let color = [color[0], color[1], color[2], -color[3]];
        self.quad([[px0, py0], [px1, py0], [px1, py1], [px0, py1]], uv, layer as f32, color);
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

    /// Width of the ink of `s` in UI pixels: the advances less the last
    /// gap ([`font::ink_width`]). Every text measurement goes through the
    /// font, so layout, drawing, carets and clipping agree.
    pub fn text_width(s: &str) -> f32 {
        font::ink_width(s) as f32
    }

    /// Draws text with Minecraft-style drop shadow; returns its width.
    pub fn text(&mut self, x: f32, y: f32, s: &str, color: Color) -> f32 {
        let d = font::SHADOW_OFFSET as f32;
        self.glyphs(x + d, y + d, s, shadow(color), 1.0, None);
        self.glyphs(x, y, s, color, 1.0, None)
    }

    /// Shadowed text magnified `k` times (titles); returns its width.
    pub fn text_scaled(&mut self, x: f32, y: f32, s: &str, color: Color, k: f32) -> f32 {
        let d = font::SHADOW_OFFSET as f32 * k;
        self.glyphs(x + d, y + d, s, shadow(color), k, None);
        self.glyphs(x, y, s, color, k, None)
    }

    /// Text without a shadow (e.g. titles on light panels).
    pub fn text_flat(&mut self, x: f32, y: f32, s: &str, color: Color) -> f32 {
        self.glyphs(x, y, s, color, 1.0, None)
    }

    /// Glyph quads for `s` with the line's top at `y`, magnified `k` times
    /// and cut to `clip` (min and max x) if given; returns the pen's travel.
    fn glyphs(&mut self, x: f32, y: f32, s: &str, color: Color, k: f32, clip: Option<(f32, f32)>) -> f32 {
        let f = font::font();
        let (aw, ah) = f.atlas_size();
        let (aw, ah) = (aw as f32, ah as f32);
        let mut pen = x;
        for c in s.chars() {
            let index = f.atlas_index(c);
            let g = &f.glyphs()[index];
            let w = g.width() as f32;
            if w > 0.0 {
                let gx = (index as u32 % font::ATLAS_COLUMNS * font::CELL_W) as f32;
                let gy = (index as u32 / font::ATLAS_COLUMNS * font::CELL_H) as f32;
                let (mut x0, mut x1) = (pen, pen + w * k);
                let (mut u0, mut u1) = (gx, gx + w);
                if let Some((min_x, max_x)) = clip {
                    if x1 <= min_x || x0 >= max_x {
                        pen += g.advance() as f32 * k;
                        continue;
                    }
                    if x0 < min_x {
                        u0 += (min_x - x0) / k;
                        x0 = min_x;
                    }
                    if x1 > max_x {
                        u1 -= (x1 - max_x) / k;
                        x1 = max_x;
                    }
                }
                let top = y + font::TOP as f32 * k;
                let bottom = top + font::CELL_H as f32 * k;
                let (v0, v1) = (gy / ah, (gy + font::CELL_H as f32) / ah);
                self.quad(
                    [[x0, top], [x1, top], [x1, bottom], [x0, bottom]],
                    [[u0 / aw, v0], [u1 / aw, v0], [u1 / aw, v1], [u0 / aw, v1]],
                    GLYPH,
                    color,
                );
            }
            pen += g.advance() as f32 * k;
        }
        pen - x
    }

    /// Shadowed text drawn only between `min_x` and `max_x`, glyphs cut at
    /// the edges (Java's scissored widget labels).
    pub fn text_clipped(&mut self, x: f32, y: f32, s: &str, color: Color, (min_x, max_x): (f32, f32)) {
        let d = font::SHADOW_OFFSET as f32;
        self.glyphs(x + d, y + d, s, shadow(color), 1.0, Some((min_x, max_x)));
        self.glyphs(x, y, s, color, 1.0, Some((min_x, max_x)));
    }

    /// Text on a translucent backing panel, like Minecraft's F3 screen.
    pub fn label(&mut self, x: f32, y: f32, s: &str, color: Color) {
        let w = Self::text_width(s);
        self.rect(x - 1.0, y - 1.0, w + 2.0, font::LINE_HEIGHT as f32 + 1.0, [0.0, 0.0, 0.0, 0.4]);
        self.text_flat(x, y, s, color);
    }
}

#[cfg(test)]
mod scale_tests {
    use super::Ui;

    #[test]
    fn auto_is_the_largest_scale_keeping_320_by_240() {
        // Java's rule: 1920x1080 is 4 (480x270), 1600x900 is 3 (533x300).
        assert_eq!(Ui::max_gui_scale(1920.0, 1080.0), 4);
        assert_eq!(Ui::max_gui_scale(1600.0, 900.0), 3);
        assert_eq!(Ui::max_gui_scale(2560.0, 1440.0), 6);
        assert_eq!(Ui::max_gui_scale(640.0, 480.0), 2);
        assert_eq!(Ui::max_gui_scale(639.0, 480.0), 1);
        assert_eq!(Ui::max_gui_scale(200.0, 100.0), 1, "never below 1");
        assert_eq!(Ui::gui_scale(1920.0, 1080.0, 0), 4.0, "Auto");
        assert_eq!(Ui::gui_scale(1920.0, 1080.0, 2), 2.0);
        assert_eq!(Ui::gui_scale(1920.0, 1080.0, 9), 4.0, "capped at what fits");
    }

    #[test]
    fn small_views_lower_the_gui_scale() {
        let window = (1600.0, 900.0);
        assert_eq!(Ui::view_scale(window, window, 0), 3.0);
        // Stacked halves and quarters drop a step to stay 320x180.
        assert_eq!(Ui::view_scale((1600.0, 450.0), window, 0), 2.0);
        assert_eq!(Ui::view_scale((800.0, 450.0), window, 0), 2.0);
        assert_eq!(Ui::view_scale((800.0, 450.0), window, 1), 1.0, "a smaller choice stays");
        // Never below one physical pixel per UI pixel.
        assert_eq!(Ui::view_scale((200.0, 100.0), window, 0), 1.0);
    }
}
