//! Immediate-mode 2D UI geometry: rectangles, text and block icons,
//! built in pixel coordinates and drawn in one call.

use bytemuck::{Pod, Zeroable};

use crate::world::block::Block;

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

    /// Isometric cube icon filling a `size` square.
    pub fn block_icon(&mut self, x: f32, y: f32, size: f32, block: Block) {
        let t = block.info().tex;
        // Half width `s`; the top diamond is `s` tall, each side face `s` tall.
        let (cx, s) = (x + size / 2.0, size / 2.0);
        let q = s / 2.0;
        let (l, r, top, bot) = (cx - s, cx + s, y, y + size);
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let shade = |f: f32| [f, f, f, 1.0];
        self.quad([[l, top + q], [cx, top], [r, top + q], [cx, top + s]], uv, t[2] as f32, WHITE);
        self.quad([[l, top + q], [cx, top + s], [cx, bot], [l, bot - q]], uv, t[5] as f32, shade(0.8));
        self.quad([[cx, top + s], [r, top + q], [r, bot - q], [cx, bot]], uv, t[0] as f32, shade(0.62));
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

    /// Text without a shadow (e.g. titles on light panels).
    pub fn text_flat(&mut self, x: f32, y: f32, s: &str, color: Color) -> f32 {
        self.text_raw(x, y, s, color)
    }

    fn text_raw(&mut self, x: f32, y: f32, s: &str, color: Color) -> f32 {
        let mut pen = x;
        for c in s.chars() {
            let c = if (c as u32) < 128 { c } else { '?' };
            if c != ' ' {
                let (gx, gy) = ((c as u32 % 16) as f32 * 8.0, (c as u32 / 16) as f32 * 8.0);
                let (u0, v0) = (gx / FONT_ATLAS_W as f32, gy / FONT_ATLAS_H as f32);
                let (u1, v1) = ((gx + 8.0) / FONT_ATLAS_W as f32, (gy + 8.0) / FONT_ATLAS_H as f32);
                let left = pen - glyph_left(c);
                self.quad(
                    [[left, y], [left + 8.0, y], [left + 8.0, y + 8.0], [left, y + 8.0]],
                    [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                    GLYPH,
                    color,
                );
            }
            pen += glyph_advance(c);
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
