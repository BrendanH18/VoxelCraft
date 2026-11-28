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
/// v0.18 Track 7: a picture from the UI image atlas (the wordmark, world
/// icons), uv in atlas units.
pub const IMAGE: f32 = -3.0;

/// The UI image atlas's side in pixels: the wordmark in the top-left
/// 256 x 64, the default world icon beside it, then [`ICON_SLOTS`] world
/// icons in 64-pixel cells.
pub const IMAGE_ATLAS: u32 = 512;
/// Where an image sits in the atlas, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
pub const WORDMARK_RECT: ImageRect = ImageRect { x: 0, y: 0, w: 256, h: 64 };
pub const DEFAULT_ICON_RECT: ImageRect = ImageRect { x: 256, y: 0, w: 64, h: 64 };
/// World icon cells below the wordmark row.
pub const ICON_SLOTS: usize = 56;
pub fn icon_slot(slot: usize) -> ImageRect {
    let slot = slot % ICON_SLOTS;
    ImageRect { x: (slot % 8) as u32 * 64, y: 64 + (slot / 8) as u32 * 64, w: 64, h: 64 }
}

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

/// Black at `alpha` as Java blends it: Java's GUI mixes sRGB values, so a
/// black overlay keeps `1 - alpha` of the sRGB colour behind it. Blending
/// here is in linear light, where keeping that much takes an alpha of
/// `1 - (1 - alpha)^2.2`. For screen dimming and list backings given as
/// Java's alpha.
pub fn shade(alpha: f32) -> Color {
    [0.0, 0.0, 0.0, blend_alpha(alpha)]
}

/// The inverse of [`srgb_to_linear`]: the sRGB value (0..1) the GPU writes
/// for linear light `v`.
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// The exponent of the blending model below: how a share of sRGB value
/// becomes a share of linear light (the sRGB curve is close to this power
/// across the range). It models Java's blending; it converts no colour.
pub const BLEND_GAMMA: f32 = 2.2;

/// The linear-light alpha at which black darkens what's behind it as
/// Java's sRGB blend at `alpha` does: Java keeps `1 - alpha` of the sRGB
/// value, which is `(1 - alpha)^2.2` of its light (within a byte over the
/// whole range). [`shade`] and the shared UI's veils use it.
pub fn blend_alpha(alpha: f32) -> f32 {
    1.0 - (1.0 - alpha.clamp(0.0, 1.0)).powf(BLEND_GAMMA)
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

    #[inline(always)]
    fn ndc(&self, x: f32, y: f32) -> [f32; 2] {
        let (x, y) = ((x * self.scale).round(), (y * self.scale).round());
        [x / self.width * 2.0 - 1.0, 1.0 - y / self.height * 2.0]
    }

    /// Converts a colour [`Ui::quad`] hasn't just seen, kept out of line so
    /// the quad itself stays small.
    #[cold]
    #[inline(never)]
    fn convert(&mut self, color: Color) {
        self.converted = (color, linear(color));
    }

    /// Arbitrary quad in UI pixels, corners clockwise from top-left.
    /// `color` is sRGB (see [`linear`]). Always inlined: callers build
    /// thousands of quads a frame, and a call per quad costs more than the
    /// quad (v0.18 track 1 measured 2.5 times the time per text quad).
    #[inline(always)]
    pub fn quad(&mut self, p: [[f32; 2]; 4], uv: [[f32; 2]; 4], layer: f32, color: Color) {
        if self.converted.0 != color {
            self.convert(color);
        }
        let color = self.converted.1;
        let v: [UiVertex; 4] =
            std::array::from_fn(|i| UiVertex { pos: self.ndc(p[i][0], p[i][1]), uv: uv[i], layer, color });
        self.verts.extend_from_slice(&[v[0], v[1], v[2], v[0], v[2], v[3]]);
    }

    /// Draws `part` of the UI image atlas (in atlas pixels) over x, y, w, h.
    pub fn image(&mut self, x: f32, y: f32, w: f32, h: f32, part: [f32; 4], color: Color) {
        let a = IMAGE_ATLAS as f32;
        let [px, py, pw, ph] = part;
        // A hair inside the edges, so nearest sampling never takes a neighbour.
        let (u0, v0, u1, v1) = ((px + 0.01) / a, (py + 0.01) / a, (px + pw - 0.01) / a, (py + ph - 0.01) / a);
        self.quad(
            [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            IMAGE,
            color,
        );
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.quad([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], [[0.0; 2]; 4], SOLID, color);
    }

    /// A rectangle shading from `top` to `bottom` (Java's `fillGradient`),
    /// both sRGB like every UI colour.
    pub fn gradient(&mut self, x: f32, y: f32, w: f32, h: f32, top: Color, bottom: Color) {
        let p = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
        // sRGB in, as for `quad`.
        let (top, bottom) = (linear(top), linear(bottom));
        let colour = [top, top, bottom, bottom];
        let v: [UiVertex; 4] = std::array::from_fn(|i| UiVertex {
            pos: self.ndc(p[i][0], p[i][1]),
            uv: [0.0; 2],
            layer: SOLID,
            color: colour[i],
        });
        self.verts.extend_from_slice(&[v[0], v[1], v[2], v[0], v[2], v[3]]);
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

    /// v1.0 gap track 6c: a block item in the `size`-pixel item area at
    /// (x, y) of a slot (16 pixels of an 18-pixel slot, inset by one), as
    /// Java's `GuiItemAtlas` draws it: the item's boxes with their faces'
    /// textures, placed by its model's GUI display transform
    /// ([`Ui::gui_model`]). Until 2026-11-27 icons were an isometric cube
    /// mirrored left to right from Java's view.
    pub fn block_icon(&mut self, x: f32, y: f32, size: f32, block: Block, glint: bool) {
        if block.flat_icon() {
            // Plants, torches and ladders show their flat sprite, like items
            // (v1.0 gap track 6b: an item sprite of its own where it has one).
            let layer = crate::item::Item::from_block(block).flat_layer().unwrap_or(block.info().tex[0]);
            self.icon(x, y, size, layer, WHITE);
            return;
        }
        let stack = voxelcraft::inventory::Stack::new(crate::item::Item::from_block(block.base()), 1);
        let display = voxelcraft::content::item_display::drawn(&stack);
        self.gui_model(x, y, size, &block_item_quads(block), &display, glint);
    }

    /// v1.0 gap track 6b: an item mesh (`content::item_models`) in the
    /// `size`-pixel item area at (x, y), placed with its GUI display
    /// transform ([`Ui::gui_model`]).
    pub fn item_mesh(&mut self, x: f32, y: f32, size: f32, mesh: &voxelcraft::content::item_models::Mesh, glint: bool) {
        self.gui_model(x, y, size, &mesh.quads, &mesh.display, glint);
    }

    /// v1.0 gap track 6c: model-space quads (a block is 0..1) in the
    /// `size`-pixel item area at (x, y), as `GuiItemAtlas` draws an item:
    /// the GUI display transform about the area's centre, one block across
    /// the area, x right, y up and z toward the viewer (Java scales the
    /// pose by (16, -16, 16) and flips the atlas back); faces turned away
    /// are dropped and far ones drawn first, each lit by Java's GUI lights
    /// for its turn ([`gui_shade`](voxelcraft::content::item_display::gui_shade):
    /// `ITEMS_3D` for block models, `ITEMS_FLAT` for `gui_light: front`).
    /// `glint` lays the enchantment glint over every face.
    pub fn gui_model(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        quads: &[voxelcraft::world::colour_blocks::banner::Quad],
        display: &voxelcraft::content::item_display::Display,
        glint: bool,
    ) {
        use voxelcraft::content::item_display::{Context, gui_shade};
        let m = Context::Gui.matrix(display);
        let (cx, cy) = (x + size / 2.0, y + size / 2.0);
        let mut placed: Vec<_> = quads
            .iter()
            .map(|q| (q.corners.map(|c| m.transform_point3(c)), m.transform_vector3(q.normal).normalize_or_zero(), q))
            .filter(|(_, n, _)| n.z > 1e-4)
            .collect();
        let depth = |c: &[glam::Vec3; 4]| c.iter().map(|c| c.z).sum::<f32>();
        placed.sort_by(|a, b| depth(&a.0).total_cmp(&depth(&b.0)));
        for (corners, normal, q) in placed {
            let shade = gui_shade(normal, display.front_light);
            let [r, g, b] = q.tint.map_or([1.0; 3], |t| [t[0], t[1], t[2]].map(|v| f32::from(v) / 255.0));
            let p = corners.map(|c| [cx + c.x * size, cy - c.y * size]);
            self.quad(p, q.uv, q.layer as f32, [r * shade, g * shade, b * shade, 1.0]);
            if glint {
                self.glint_quad(p, q.uv, q.layer);
            }
        }
    }

    /// The enchantment glint over one face of a 3D item (as the flat icons'
    /// glint in `hud`): a faint purple sheen over the face's texture shape
    /// with a brighter band sweeping down it along the texture's v.
    fn glint_quad(&mut self, p: [[f32; 2]; 4], uv: [[f32; 2]; 4], layer: u16) {
        let sheen = [0.55, 0.25, 1.0, -0.3];
        self.quad(p, uv, layer as f32, sheen);
        let band = 0.3;
        let top = (glint_clock() * 0.6).fract() * (1.0 + band) - band;
        let (v0, v1) = (top.max(0.0), (top + band).min(1.0));
        if v1 <= v0 {
            return;
        }
        // The face is a parallelogram with its texture mapped affinely: v
        // runs along one pair of opposite edges (0 to 3 and 1 to 2, or 0 to
        // 1 and 3 to 2); the band is cut out of it along that pair.
        let (a, b, c, d) = if (uv[3][1] - uv[0][1]).abs() > 1e-6 { (0, 3, 1, 2) } else { (0, 1, 3, 2) };
        let (va, vb) = (uv[a][1], uv[b][1]);
        if (vb - va).abs() < 1e-6 {
            return;
        }
        let at = |t: f32, from: usize, to: usize| {
            let k = ((t - va) / (vb - va)).clamp(0.0, 1.0);
            let mix = |x: [f32; 2], y: [f32; 2]| [x[0] + (y[0] - x[0]) * k, x[1] + (y[1] - x[1]) * k];
            (mix(p[from], p[to]), mix(uv[from], uv[to]))
        };
        let (lo, hi) = (v0.max(va.min(vb)), v1.min(va.max(vb)));
        if hi <= lo {
            return;
        }
        let (a0, b0) = (at(lo, a, b), at(lo, c, d));
        let (a1, b1) = (at(hi, a, b), at(hi, c, d));
        self.quad([a0.0, b0.0, b1.0, a1.0], [a0.1, b0.1, b1.1, a1.1], layer as f32, [0.8, 0.6, 1.0, -0.35]);
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
        let glyphs = f.glyphs();
        let (aw, ah) = f.atlas_size();
        let (du, dv) = (1.0 / aw as f32, 1.0 / ah as f32);
        let top = y + font::TOP as f32 * k;
        let bottom = top + font::CELL_H as f32 * k;
        let mut pen = x;
        for c in s.chars() {
            let index = f.atlas_index(c);
            let g = &glyphs[index];
            let advance = g.advance() as f32 * k;
            let w = g.width() as f32;
            if w > 0.0 {
                let gx = (index as u32 % font::ATLAS_COLUMNS * font::CELL_W) as f32;
                let gy = (index as u32 / font::ATLAS_COLUMNS * font::CELL_H) as f32;
                let (mut x0, mut x1) = (pen, pen + w * k);
                let (mut u0, mut u1) = (gx, gx + w);
                if let Some((min_x, max_x)) = clip {
                    if x1 <= min_x || x0 >= max_x {
                        pen += advance;
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
                let (u0, u1) = (u0 * du, u1 * du);
                let (v0, v1) = (gy * dv, (gy + font::CELL_H as f32) * dv);
                self.quad(
                    [[x0, top], [x1, top], [x1, bottom], [x0, bottom]],
                    [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                    GLYPH,
                    color,
                );
            }
            pen += advance;
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

/// Seconds since the first glint was drawn: the glint's clock.
pub fn glint_clock() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32()
}

/// v1.0 gap track 6c: the quads of a block item's model in Java's item
/// model space (a block is 0..1): the item's boxes ([`shape::item_boxes`]
/// of [`Block::item_facing`]) with the block's face layers and Java's face
/// UVs (each face's texture upright and unmirrored seen from outside, as
/// `FaceBakery`'s default UVs map it). A head's full cube is Java's 8 × 8 × 8
/// skull box on the floor of the block (`SkullModel`).
pub fn block_item_quads(block: Block) -> Vec<voxelcraft::world::colour_blocks::banner::Quad> {
    use glam::Vec3;
    let block = block.item_facing();
    let tex = block.info().tex;
    let head = crate::world::skulls::decode(block.canonical()).is_some();
    if matches!(block.shaped(), Some(voxelcraft::world::block::Shaped::Fence)) {
        return fence_inventory_quads(tex);
    }
    let mut out = Vec::with_capacity(6);
    for (i, b) in shape::item_boxes(block).as_slice().iter().enumerate() {
        // Candle wicks, the beacon's plinth and core: their own layers.
        let tex = shape::item_box_layer(block, i).map_or(tex, |l| [l; 6]);
        let (lo, hi) = (Vec3::from_array(b.min.map(f32::from)) / 16.0, Vec3::from_array(b.max.map(f32::from)) / 16.0);
        let place = |p: Vec3| if head { Vec3::new(0.25 + p.x * 0.5, p.y * 0.5, 0.25 + p.z * 0.5) } else { p };
        for (face, &layer) in tex.iter().enumerate() {
            let (d, positive) = (face / 2, face % 2 == 0);
            let (u, v) = ((d + 1) % 3, (d + 2) % 3);
            let corner = |a: bool, b: bool| {
                let mut p = [0.0f32; 3];
                p[d] = if positive { hi[d] } else { lo[d] };
                p[u] = if a { hi[u] } else { lo[u] };
                p[v] = if b { hi[v] } else { lo[v] };
                Vec3::from_array(p)
            };
            let c = if positive {
                [corner(false, false), corner(true, false), corner(true, true), corner(false, true)]
            } else {
                [corner(false, false), corner(false, true), corner(true, true), corner(true, false)]
            };
            // v1.0 gap track 6f: trapdoors' rims take the jar's item UVs.
            let uv = voxelcraft::mesh::item_face_uvs(
                block,
                face,
                b.min,
                b.max,
                c.map(|p| voxelcraft::mesh::face_uv(face, p.to_array())),
            );
            let mut normal = Vec3::ZERO;
            normal[d] = if positive { 1.0 } else { -1.0 };
            out.push(voxelcraft::world::colour_blocks::banner::Quad {
                corners: c.map(place),
                uv,
                layer,
                normal,
                tint: None,
            });
        }
    }
    out
}

/// v1.0 gap track 6d: Java's `fence_inventory` model, element by element
/// (from, to, and each face's explicit UV in texels, `None` for a face the
/// model leaves out), faces +X, -X, +Y, -Y, +Z, -Z: two posts at z 0-4 and
/// 12-16, and two bars from z -2 to 18 in three pieces each, reaching 2/16
/// beyond the block.
const FENCE_INVENTORY: [voxelcraft::content::item_models::Element; 8] = {
    const fn f(u0: f32, v0: f32, u1: f32, v1: f32) -> Option<[f32; 4]> {
        Some([u0, v0, u1, v1])
    }
    [
        // Left post: east, west, up, down, south, north.
        (
            [6., 0., 0.],
            [10., 16., 4.],
            [
                f(0., 0., 4., 16.),
                f(0., 0., 4., 16.),
                f(6., 0., 10., 4.),
                f(6., 0., 10., 4.),
                f(6., 0., 10., 16.),
                f(6., 0., 10., 16.),
            ],
        ),
        // Right post.
        (
            [6., 0., 12.],
            [10., 16., 16.],
            [
                f(12., 0., 16., 16.),
                f(12., 0., 16., 16.),
                f(6., 12., 10., 16.),
                f(6., 12., 10., 16.),
                f(6., 0., 10., 16.),
                f(6., 0., 10., 16.),
            ],
        ),
        // Top bar, and its ends beyond the block.
        (
            [7., 12., 0.],
            [9., 15., 16.],
            [f(0., 1., 16., 4.), f(0., 1., 16., 4.), f(7., 0., 9., 16.), f(7., 0., 9., 16.), None, None],
        ),
        (
            [7., 12., -2.],
            [9., 15., 0.],
            [f(0., 1., 2., 4.), f(14., 1., 16., 4.), f(7., 14., 9., 16.), f(7., 0., 9., 2.), None, f(7., 1., 9., 4.)],
        ),
        (
            [7., 12., 16.],
            [9., 15., 18.],
            [f(14., 1., 16., 4.), f(0., 1., 2., 4.), f(7., 0., 9., 2.), f(7., 14., 9., 16.), f(7., 1., 9., 4.), None],
        ),
        // Lower bar, and its ends.
        (
            [7., 6., 0.],
            [9., 9., 16.],
            [f(0., 7., 16., 10.), f(0., 7., 16., 10.), f(7., 0., 9., 16.), f(7., 0., 9., 16.), None, None],
        ),
        (
            [7., 6., -2.],
            [9., 9., 0.],
            [
                f(0., 7., 2., 10.),
                f(14., 7., 16., 10.),
                f(7., 14., 9., 16.),
                f(7., 0., 9., 2.),
                None,
                f(7., 7., 9., 10.),
            ],
        ),
        (
            [7., 6., 16.],
            [9., 9., 18.],
            [
                f(14., 7., 16., 10.),
                f(0., 7., 2., 10.),
                f(7., 0., 9., 2.),
                f(7., 14., 9., 16.),
                f(7., 7., 9., 10.),
                None,
            ],
        ),
    ]
};

/// [`FENCE_INVENTORY`]'s quads with the fence's face layers.
fn fence_inventory_quads(tex: [u16; 6]) -> Vec<voxelcraft::world::colour_blocks::banner::Quad> {
    voxelcraft::content::item_models::element_quads(&FENCE_INVENTORY, tex)
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn colours_are_written_as_srgb_and_shown_as_written() {
        // The GPU encodes linear output for the sRGB surface with the sRGB
        // curve; its inverse here brings every byte back unchanged.
        let encode = linear_to_srgb;
        for byte in 0..=255u8 {
            let v = byte as f32 / 255.0;
            assert_eq!((encode(srgb_to_linear(v)) * 255.0).round() as u8, byte);
        }
        assert_eq!(rgb(0xC6C6C6), [198.0 / 255.0, 198.0 / 255.0, 198.0 / 255.0, 1.0]);
        // Alpha is not converted; the panel grey becomes about 0.57 linear.
        let panel = linear([0.78, 0.78, 0.78, 0.5]);
        assert!((panel[0] - 0.5705).abs() < 1e-3 && panel[3] == 0.5, "{panel:?}");
        // Ui::quad converts what it's given, flat colours and tints alike.
        let mut ui = Ui::with_scale(100.0, 100.0, 1.0);
        ui.rect(0.0, 0.0, 1.0, 1.0, rgb(0x404040));
        ui.icon(0.0, 0.0, 16.0, 3u16, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(ui.verts[0].color, linear(rgb(0x404040)));
        assert_eq!(ui.verts[6].color[0], srgb_to_linear(0.5));
    }

    #[test]
    fn shades_darken_as_javas_alpha_does() {
        // Over a mid grey, Java's 112/255 black leaves 56% of the sRGB value.
        let behind = 0.5f32;
        let a = shade(112.0 / 255.0)[3];
        let blended = srgb_to_linear(behind) * (1.0 - a);
        let encode = linear_to_srgb;
        assert!((encode(blended) - behind * (1.0 - 112.0 / 255.0)).abs() < 0.015, "{}", encode(blended));
        assert_eq!(shade(0.0)[3], 0.0);
        assert_eq!(shade(1.0)[3], 1.0);
    }

    #[test]
    fn text_quads_follow_the_font() {
        let mut ui = Ui::with_scale(400.0, 300.0, 1.0);
        let w = ui.text_flat(10.0, 20.0, "Aé ", WHITE);
        assert_eq!(w, font::width("Aé ") as f32, "the pen moves by the advances");
        assert_eq!(Ui::text_width("Aé "), font::ink_width("Aé ") as f32);
        // Two glyph quads (the space has none), each from the line's top
        // less 3 rows to 12 rows down, as wide as the glyph's ink.
        assert_eq!(ui.verts.len(), 12);
        let x = |v: &UiVertex| ((v.pos[0] + 1.0) / 2.0 * 400.0).round();
        let y = |v: &UiVertex| ((1.0 - v.pos[1]) / 2.0 * 300.0).round();
        assert_eq!((x(&ui.verts[0]), y(&ui.verts[0])), (10.0, 17.0));
        assert_eq!((x(&ui.verts[2]), y(&ui.verts[2])), (15.0, 29.0));
        assert_eq!(x(&ui.verts[6]), 16.0, "é starts after A's advance");
        // Clipping cuts glyphs at the edges and drops those outside.
        let mut clipped = Ui::with_scale(400.0, 300.0, 1.0);
        clipped.text_clipped(0.0, 0.0, "WWWW", WHITE, (3.0, 14.0));
        let xs: Vec<f32> = clipped.verts.iter().map(x).collect();
        assert!(xs.iter().all(|&v| (3.0..=14.0).contains(&v)), "{xs:?}");
    }

    /// Headless cost of the UI paths this track changed (`cargo test
    /// --profile ci --bin voxelcraft text_cost -- --ignored --nocapture`):
    /// 40 shadowed lines of 57 characters in alternating colours (each line
    /// converts two new colours), 3920 rectangles, one colour conversion
    /// and one line's width.
    #[test]
    #[ignore]
    fn text_cost() {
        let line = "Simulation Distance: 10 Chunks - Ærøskøbing Łódź Žďár ...";
        let colours = [WHITE, rgb(0xA0A0A0), rgb(0xFFFFA0), rgb(0x404040)];
        let runs = 2000;
        let start = std::time::Instant::now();
        let mut quads = 0;
        let mut ui = Ui::with_scale(1920.0, 1080.0, 2.0);
        for _ in 0..runs {
            ui.verts.clear();
            for i in 0..40 {
                ui.text(4.0, i as f32 * 10.0, line, colours[i % 4]);
            }
            quads = ui.verts.len() / 6;
            std::hint::black_box(&ui.verts);
        }
        let per_frame = start.elapsed() / runs;
        let glyphs = 40 * line.chars().count();
        let start = std::time::Instant::now();
        let mut ui = Ui::with_scale(1920.0, 1080.0, 2.0);
        for _ in 0..runs {
            ui.verts.clear();
            for i in 0..3920 {
                ui.rect((i % 300) as f32, (i / 300) as f32, 5.0, 12.0, colours[i / 98 % 4]);
            }
            std::hint::black_box(&ui.verts);
        }
        println!("text_cost: 3920 rectangles {:?} per frame", start.elapsed() / runs);
        let start = std::time::Instant::now();
        for i in 0..1_000_000u32 {
            std::hint::black_box(linear(std::hint::black_box([i as f32 * 1e-6, 0.5, 0.25, 1.0])));
        }
        let convert = start.elapsed() / 1_000_000;
        let start = std::time::Instant::now();
        for _ in 0..runs {
            std::hint::black_box(Ui::text_width(std::hint::black_box(line)));
        }
        println!("text_cost: linear() {convert:?} per colour, text_width {:?} per line", start.elapsed() / runs);
        println!(
            "text_cost: {glyphs} characters, {quads} quads: {per_frame:?} per frame, {:.1} ns per character",
            per_frame.as_nanos() as f64 / glyphs as f64
        );
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

#[cfg(test)]
mod item_view_tests {
    use super::*;

    /// v1.0 gap track 6d: a fence item is Java's `fence_inventory`: two
    /// posts and two bars running north to south, the bars reaching 2/16
    /// beyond the block, 40 faces, each with its explicit UV (the left
    /// post's east face samples texels 0-4 across, its top 6-10).
    #[test]
    fn fence_items_are_javas_fence_inventory() {
        let quads = block_item_quads(crate::world::block::Block::OAK_FENCE);
        assert_eq!(quads.len(), 40);
        let (lo, hi) = quads
            .iter()
            .flat_map(|q| q.corners)
            .fold((glam::Vec3::MAX, glam::Vec3::MIN), |(lo, hi), c| (lo.min(c), hi.max(c)));
        assert!((lo - glam::Vec3::new(6.0, 0.0, -2.0) / 16.0).abs().max_element() < 1e-6, "{lo}");
        assert!((hi - glam::Vec3::new(10.0, 16.0, 18.0) / 16.0).abs().max_element() < 1e-6, "{hi}");
        let east = &quads[0];
        let us: Vec<f32> = east.uv.iter().map(|uv| uv[0] * 16.0).collect();
        assert!(us.iter().all(|u| (*u - 0.0).abs() < 1e-4 || (*u - 4.0).abs() < 1e-4), "{us:?}");
        // Seen from outside the east face runs unmirrored: u grows toward -z.
        let at_north = east.corners.iter().zip(&east.uv).find(|(c, _)| c.z < 1e-6).unwrap().1[0];
        assert!((at_north * 16.0 - 4.0).abs() < 1e-4);
        let top = &quads[2];
        assert!(top.uv.iter().all(|uv| (6.0 - 1e-4..=10.0 + 1e-4).contains(&(uv[0] * 16.0))), "{:?}", top.uv);
    }

    /// The x (UI pixels) of the quads drawn with `layer`, by their vertices.
    fn xs(ui: &Ui, layer: u16) -> Vec<f32> {
        ui.verts.iter().filter(|v| v.layer == layer as f32).map(|v| (v.pos[0] + 1.0) / 2.0 * ui.width).collect()
    }

    /// v1.0 gap track 6c: a furnace's front (its item model's north face)
    /// shows on the right of a slot and its east side on the left, as
    /// Java's `block/block` GUI turn (30, 225, 0) places them; stairs show
    /// their profile on the left (`block/stairs`' 135); a head its face on
    /// the right (`item/template_skull`'s 45, the face south). The top is
    /// lit fully, the left 0.65 and the right 0.4 (Java's GUI lights).
    #[test]
    fn slot_icons_take_javas_orientation_and_light() {
        use crate::world::block::tex;
        let mut ui = Ui::with_scale(64.0, 64.0, 1.0);
        ui.block_icon(0.0, 0.0, 64.0, Block::FURNACE, false);
        let front = xs(&ui, tex::FURNACE_FRONT);
        assert!(!front.is_empty() && front.iter().all(|&x| x >= 32.0 - 1e-3), "{front:?}");
        let shades: Vec<f32> = ui.verts.iter().map(|v| linear_to_srgb(v.color[0])).collect();
        for expected in [1.0, 0.6505, 0.4] {
            assert!(shades.iter().any(|s| (s - expected).abs() < 2e-3), "{expected} in {shades:?}");
        }
        let mut head = Ui::with_scale(64.0, 64.0, 1.0);
        let skull = crate::world::skulls::item_block(crate::world::skulls::Kind::Zombie);
        head.block_icon(0.0, 0.0, 64.0, skull, false);
        let face = skull.info().tex[4];
        let x = xs(&head, face);
        assert!(!x.is_empty() && x.iter().all(|&x| x >= 32.0 - 1e-3), "{x:?}");
    }

    /// v1.0 gap track 6c: an enchanted 3D item glints in a slot: each face
    /// is drawn again as the glint's texture-shaped sheen.
    #[test]
    fn enchanted_3d_items_glint_in_a_slot() {
        let mut plain = Ui::with_scale(64.0, 64.0, 1.0);
        plain.block_icon(0.0, 0.0, 64.0, Block::STONE, false);
        let mut glinting = Ui::with_scale(64.0, 64.0, 1.0);
        glinting.block_icon(0.0, 0.0, 64.0, Block::STONE, true);
        assert!(glinting.verts.len() > plain.verts.len());
        assert!(glinting.verts.iter().any(|v| v.color[3] < 0.0), "the sheen is drawn through the texture's shape");
    }
}
