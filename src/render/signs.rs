//! Signs and shelf items, drawn outside the chunk meshes like Java's block
//! entity renderers: sign boards and posts from Java 26.3's sign block
//! models (`template_sign_rot_*`, `template_wall_sign`,
//! `template_hanging_sign_*`), their text placed as `StandingSignRenderer`
//! and `HangingSignRenderer` place it, and the items on shelves as
//! `ShelfRenderer` places them. Everything here is rebuilt each frame for
//! the signs and shelves within [`RANGE`] blocks.

use glam::{DVec3, IVec3, Vec3};

use super::BlockModel;
use super::block_model::BlockVertex;
use crate::entity::model::EntityVertex;
use crate::world::World;
use crate::world::block::{Block, tex as block_tex};
use crate::world::wood::entities::{SignEntity, SignText, text_color};
use crate::world::wood::signs::{Mount, Sign};
use crate::world::wood::{self, Kind, shelves};

/// Java's default block entity view distance.
pub const RANGE: f64 = 64.0;
/// Java draws the glow outline within 16 blocks (`OUTLINE_RENDER_DISTANCE`).
const OUTLINE_RANGE: f64 = 16.0;

/// A box in 1/16 block in the sign's own frame (front toward +z), with the
/// texture layer of its faces.
#[derive(Clone, Copy, Debug)]
struct Part {
    min: [f32; 3],
    max: [f32; 3],
    layer: u16,
}

/// A sign body to draw: its parts, turned `yaw` radians about the cell's
/// vertical centre line (front toward +z at 0, as Java's yaw).
#[derive(Clone, Debug)]
pub struct SignModel {
    pub cell: IVec3,
    pub yaw: f32,
    parts: Vec<Part>,
    pub sky_light: f32,
    pub block_light: f32,
}

fn planks_layer(kind: Kind) -> u16 {
    kind.planks().info().tex[0]
}

fn stripped_layer(kind: Kind) -> u16 {
    kind.stripped_log().info().tex[0]
}

/// The body of `sign`, from the Java block models (chains of hanging signs
/// are thin boxes here rather than crossed planes).
fn parts(sign: Sign) -> Vec<Part> {
    let p = |min: [f32; 3], max: [f32; 3], layer: u16| Part { min, max, layer };
    let planks = planks_layer(sign.kind);
    let third = 1.0 / 3.0;
    match sign.mount {
        Mount::Standing { .. } => vec![
            p([7.0 + third, 0.0, 7.0 + third], [8.0 + 2.0 * third, 9.0 + third, 8.0 + 2.0 * third], planks),
            p([0.0, 9.0 + third, 7.0 + third], [16.0, 17.0 + third, 8.0 + 2.0 * third], planks),
        ],
        Mount::Wall { .. } => vec![p([0.0, 4.0 + third, third], [16.0, 12.0 + third, 1.0 + 2.0 * third], planks)],
        Mount::Hanging { attached, .. } => {
            let board = stripped_layer(sign.kind);
            let chain = block_tex::CHAIN;
            let mut out = vec![p([1.0, 0.0, 7.0], [15.0, 10.0, 9.0], board)];
            if attached {
                out.push(p([3.5, 10.0, 7.75], [4.5, 16.0, 8.25], chain));
                out.push(p([11.5, 10.0, 7.75], [12.5, 16.0, 8.25], chain));
            } else {
                out.push(p([3.0, 10.0, 7.5], [5.0, 16.0, 8.5], chain));
                out.push(p([11.0, 10.0, 7.5], [13.0, 16.0, 8.5], chain));
            }
            out
        }
        Mount::WallHanging { .. } => {
            let board = stripped_layer(sign.kind);
            vec![
                p([1.0, 0.0, 7.0], [15.0, 10.0, 9.0], board),
                p([0.0, 14.0, 6.0], [16.0, 16.0, 10.0], board),
                p([2.5, 10.0, 7.5], [3.5, 14.0, 8.5], block_tex::CHAIN),
                p([12.5, 10.0, 7.5], [13.5, 14.0, 8.5], block_tex::CHAIN),
            ]
        }
    }
}

fn light(world: &World, cell: IVec3) -> (f32, f32) {
    let centre = cell.as_dvec3() + DVec3::splat(0.5);
    (crate::entity::sky_light(world, centre), world.block_light(cell) as f32 / 15.0)
}

/// Sign bodies within [`RANGE`] of `camera`.
pub fn models(world: &World, camera: DVec3) -> Vec<SignModel> {
    let mut out = Vec::new();
    for (&cell, _) in world.wood_entities().signs() {
        if (cell.as_dvec3() + DVec3::splat(0.5)).distance_squared(camera) > RANGE * RANGE {
            continue;
        }
        let Some(sign) = world.get_block(cell).and_then(wood::signs::sign) else { continue };
        let (sky_light, block_light) = light(world, cell);
        out.push(SignModel { cell, yaw: sign.yaw_degrees().to_radians(), parts: parts(sign), sky_light, block_light });
    }
    out
}

/// Face shading, ordered +X, -X, +Y, -Y, +Z, -Z (as in `chunk.wgsl`).
const FACE_SHADE: [f32; 6] = [0.8, 0.8, 1.0, 0.55, 0.68, 0.68];

/// Triangles of `models`, camera-relative, for the block model pass.
pub fn vertices(models: &[SignModel], camera: DVec3, out: &mut Vec<BlockVertex>) {
    for m in models {
        let origin = (m.cell.as_dvec3() - camera).as_vec3();
        let (sin, cos) = m.yaw.sin_cos();
        let place = |c: Vec3| {
            let (x, z) = (c.x - 0.5, c.z - 0.5);
            origin + Vec3::new(x * cos - z * sin + 0.5, c.y, x * sin + z * cos + 0.5)
        };
        for part in &m.parts {
            let lo = Vec3::from(part.min) / 16.0;
            let hi = Vec3::from(part.max) / 16.0;
            for (face, &shade) in FACE_SHADE.iter().enumerate() {
                let (d, positive) = (face / 2, face % 2 == 0);
                let (u, v) = ((d + 1) % 3, (d + 2) % 3);
                let corner = |a: bool, b: bool| {
                    let mut p = [0.0f32; 3];
                    p[d] = if positive { hi[d] } else { lo[d] };
                    p[u] = if a { hi[u] } else { lo[u] };
                    p[v] = if b { hi[v] } else { lo[v] };
                    Vec3::from_array(p)
                };
                let c = [corner(false, false), corner(true, false), corner(true, true), corner(false, true)];
                for i in [0, 1, 2, 2, 3, 0] {
                    let p = c[i];
                    // UVs as the chunk shader's planar mapping, in the sign's frame.
                    let uv = match d {
                        0 => [p.z, 1.0 - p.y],
                        1 => [p.x, p.z],
                        _ => [p.x, 1.0 - p.y],
                    };
                    out.push(BlockVertex {
                        pos: place(p).to_array(),
                        uv,
                        layer: part.layer as u32,
                        light: [m.sky_light, shade, m.block_light],
                    });
                }
            }
        }
    }
}

// ---- Text ------------------------------------------------------------------

/// Advance of a glyph in font pixels, as the UI font (`ui::glyph_advance`).
fn advance(c: char) -> i32 {
    super::ui::glyph_advance(c) as i32
}

/// Java's `Font.width`.
pub fn text_width(s: &str) -> i32 {
    s.chars().map(advance).sum::<i32>() - 1
}

/// The part of `line` that fits `max` font pixels (the renderer's
/// `font.split(line, maxWidth)` first line, cut by character).
pub fn fitted(line: &str, max: i32) -> &str {
    let mut width = -1;
    for (i, c) in line.char_indices() {
        width += advance(c);
        if width > max {
            return &line[..i];
        }
    }
    line
}

/// Where a sign side's text sits: its centre, its right and its out-of-face
/// direction, and the size of a font pixel, all in world space.
fn text_frame(sign: Sign, cell: IVec3, front: bool) -> (DVec3, DVec3, DVec3, f64) {
    let yaw = (sign.yaw_degrees() as f64).to_radians();
    let (sin, cos) = yaw.sin_cos();
    let right = DVec3::new(cos, 0.0, sin);
    let out = DVec3::new(-sin, 0.0, cos);
    // (height of the line block's middle, distance in front of the centre),
    // as the renderers' text transformations.
    let (y, z, scale) = match sign.mount {
        Mount::Standing { .. } => (0.5 + 1.0 / 3.0, 0.046_666_667, 1.0 / 96.0),
        Mount::Wall { .. } => (0.5 - 0.3125 + 1.0 / 3.0, -0.4375 + 0.046_666_667, 1.0 / 96.0),
        Mount::Hanging { .. } | Mount::WallHanging { .. } => (0.9375 - 0.3125 - 0.32, 0.073, 0.014_062_5),
    };
    let base = cell.as_dvec3() + DVec3::new(0.5, y, 0.5);
    if front {
        (base + out * z, right, out, scale)
    } else {
        // The back side turns half round about the centre line; a wall
        // sign's back is pushed into its wall with the board.
        let back_z = match sign.mount {
            Mount::Wall { .. } => -0.4375 - 0.046_666_667,
            _ => -z,
        };
        (base + out * back_z, -right, -out, scale)
    }
}

struct Pen<'a> {
    out: &'a mut Vec<EntityVertex>,
    camera: DVec3,
    origin: DVec3,
    right: DVec3,
    normal: DVec3,
    scale: f64,
}

impl Pen<'_> {
    /// A rectangle `w` by `h` font pixels whose top left is (`x`, `y`),
    /// y down, `lift` font pixels toward the reader.
    fn rect(&mut self, (x, y, w, h): (f64, f64, f64, f64), lift: f64, color: [u8; 3], light: [u8; 4], torch: u8) {
        let at = |px: f64, py: f64| {
            (self.origin + self.right * (px * self.scale) - DVec3::Y * (py * self.scale) + self.normal * (lift * 0.002)
                - self.camera)
                .as_vec3()
        };
        let corners = [at(x, y + h), at(x + w, y + h), at(x + w, y), at(x, y)];
        for i in [0, 1, 2, 0, 2, 3] {
            self.out.push(EntityVertex {
                pos: corners[i].to_array(),
                uv: [0.0; 2],
                color: [color[0], color[1], color[2], 0],
                light,
                torch: [torch, 0, 0, 0],
            });
        }
    }

    /// One line of text, its glyph rows merged into runs.
    fn line(&mut self, line: &str, x0: f64, y0: f64, offset: (f64, f64), lift: f64, style: ([u8; 3], [u8; 4], u8)) {
        let (color, light, torch) = style;
        let mut pen = x0;
        for c in line.chars() {
            let c = if c.is_ascii() { c } else { '?' };
            let glyph = font8x8::legacy::BASIC_LEGACY[c as usize];
            let left = glyph.iter().fold(0u8, |a, r| a | r);
            let left = if left == 0 { 0 } else { left.trailing_zeros() } as f64;
            for (row, bits) in glyph.into_iter().enumerate() {
                let mut x = 0;
                while x < 8 {
                    if bits & (1 << x) == 0 {
                        x += 1;
                        continue;
                    }
                    let start = x;
                    while x < 8 && bits & (1 << x) != 0 {
                        x += 1;
                    }
                    let px = pen - left + start as f64 + offset.0;
                    let py = y0 + row as f64 + offset.1;
                    self.rect((px, py, (x - start) as f64, 1.0), lift, color, light, torch);
                }
            }
            pen += advance(c) as f64;
        }
    }
}

/// Text of the signs within [`RANGE`] of `camera`, as entity geometry:
/// dyed text darkened to 40% and lit by the world, glowing text at full
/// colour and full brightness with a dark outline (Java's
/// `AbstractSignRenderer.submitSignText`).
pub fn text(world: &World, camera: DVec3, out: &mut Vec<EntityVertex>) {
    for (&cell, entity) in world.wood_entities().signs() {
        let distance = (cell.as_dvec3() + DVec3::splat(0.5)).distance(camera);
        if distance > RANGE {
            continue;
        }
        let Some(sign) = world.get_block(cell).and_then(wood::signs::sign) else { continue };
        let (sky, block) = light(world, cell);
        for front in [true, false] {
            side_text(sign, cell, entity, front, distance <= OUTLINE_RANGE, (sky, block), camera, out);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn side_text(
    sign: Sign,
    cell: IVec3,
    entity: &SignEntity,
    front: bool,
    near: bool,
    (sky, block): (f32, f32),
    camera: DVec3,
    out: &mut Vec<EntityVertex>,
) {
    let text: &SignText = entity.side(front);
    if !text.has_message() {
        return;
    }
    let (origin, right, normal, scale) = text_frame(sign, cell, front);
    // Only the side facing the camera can be seen.
    if (camera - origin).dot(normal) <= 0.0 {
        return;
    }
    let colour = text_color(text.color);
    let dark = if text.color == crate::color::DyeColor::Black && text.glowing {
        [0xF0, 0xEB, 0xCC]
    } else {
        colour.map(|c| (c as f32 * 0.4) as u8)
    };
    let lit = [(sky.clamp(0.0, 1.0) * 255.0) as u8, 255, 0, 0];
    let torch = (block.clamp(0.0, 1.0) * 255.0) as u8;
    let emissive = [255, 255, 0, 255];
    let line_height = sign.line_height() as f64;
    let midpoint = 4.0 * line_height / 2.0;
    let mut pen = Pen { out, camera, origin, right, normal, scale };
    for (i, line) in text.lines.iter().enumerate() {
        let line = fitted(line, sign.max_line_width() as i32);
        if line.is_empty() {
            continue;
        }
        let x0 = -(text_width(line) as f64) / 2.0;
        let y0 = i as f64 * line_height - midpoint;
        if text.glowing {
            if near || text.color == crate::color::DyeColor::Black {
                for d in [
                    (-1.0, 0.0),
                    (1.0, 0.0),
                    (0.0, -1.0),
                    (0.0, 1.0),
                    (-1.0, -1.0),
                    (1.0, 1.0),
                    (-1.0, 1.0),
                    (1.0, -1.0),
                ] {
                    pen.line(line, x0, y0, d, 1.0, (dark, emissive, 0));
                }
            }
            pen.line(line, x0, y0, (0.0, 0.0), 2.0, (colour, emissive, 0));
        } else {
            pen.line(line, x0, y0, (0.0, 0.0), 1.0, (dark, lit, torch));
        }
    }
}

// ---- Shelf items -------------------------------------------------------

/// The items on shelves within [`RANGE`] of `camera`, a quarter of a block
/// each (Java's `ShelfRenderer`).
pub fn shelf_items(world: &World, camera: DVec3, out: &mut Vec<BlockModel>) {
    for (&cell, items) in world.wood_entities().shelves() {
        if (cell.as_dvec3() + DVec3::splat(0.5)).distance_squared(camera) > RANGE * RANGE {
            continue;
        }
        let Some(shelf) = world.get_block(cell).and_then(shelves::shelf) else { continue };
        let (sky_light, block_light) = light(world, cell);
        for (slot, stack) in items.iter().enumerate() {
            let Some(stack) = stack else { continue };
            let (anchor, yaw) = shelves::item_anchor(shelf, slot);
            let block = stack.item.block().filter(|b| !b.flat_icon());
            let icon = match block {
                Some(_) => None,
                None => stack.item.block().map(|b| b.info().tex[0]).or_else(|| stack.item.icon_layer()),
            };
            let size = 0.25;
            out.push(BlockModel {
                min: cell.as_dvec3() + anchor - DVec3::splat(size / 2.0),
                size: size as f32,
                block: block.unwrap_or(Block::AIR),
                sky_light,
                block_light,
                yaw: yaw.to_radians(),
                icon,
                orient: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::world::block::Facing;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;

    const AT: IVec3 = IVec3::new(8, 145, 8);

    fn world() -> World {
        let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        w
    }

    #[test]
    fn lines_are_cut_to_the_sign_width() {
        let long = "W".repeat(40);
        let cut = fitted(&long, 90);
        assert!(!cut.is_empty() && cut.len() < long.len());
        assert!(text_width(cut) <= 90 && text_width(&long[..cut.len() + 1]) > 90);
        assert_eq!(fitted("Hi", 60), "Hi");
    }

    #[test]
    fn sign_bodies_stay_in_their_cell_and_turn_with_the_sign() {
        let mounts = [
            Mount::Standing { rotation: 5 },
            Mount::Wall { facing: Facing::East },
            Mount::Hanging { rotation: 3, attached: true },
            Mount::WallHanging { facing: Facing::North },
        ];
        for mount in mounts {
            let sign = Sign { kind: Kind::Cherry, mount, wet: false };
            let parts = parts(sign);
            let model =
                SignModel { cell: AT, yaw: sign.yaw_degrees().to_radians(), parts, sky_light: 1.0, block_light: 0.0 };
            let mut v = Vec::new();
            vertices(std::slice::from_ref(&model), AT.as_dvec3(), &mut v);
            assert_eq!(v.len(), model.parts.len() * 36, "{mount:?}");
            // A turned body may reach past its cell only by the standing
            // board's diagonal overhang.
            for p in &v {
                assert!(p.pos.iter().all(|&c| (-0.25..=1.25).contains(&c)), "{mount:?}: {:?}", p.pos);
            }
        }
    }

    #[test]
    fn text_is_drawn_on_the_side_the_camera_sees() {
        let mut w = world();
        w.set_block(AT - IVec3::Y, Block::STONE);
        let sign = Sign { kind: Kind::Oak, mount: Mount::Standing { rotation: 0 }, wet: false };
        w.set_block(AT, sign.block());
        w.set_sign_text(AT, true, ["Hello".into(), String::new(), String::new(), String::new()]).unwrap();
        let centre = AT.as_dvec3() + DVec3::splat(0.5);
        // Rotation 0 faces south (+z): seen from there, not from the north.
        let mut front = Vec::new();
        text(&w, centre + DVec3::new(0.0, 0.5, 4.0), &mut front);
        assert!(!front.is_empty());
        let mut back = Vec::new();
        text(&w, centre + DVec3::new(0.0, 0.5, -4.0), &mut back);
        assert!(back.is_empty(), "the back side has no text");
        let mut far = Vec::new();
        text(&w, centre + DVec3::new(0.0, 0.5, RANGE + 1.0), &mut far);
        assert!(far.is_empty(), "beyond the block entity range");
        assert_eq!(models(&w, centre).len(), 1);
    }

    #[test]
    fn shelf_items_sit_on_the_shelf_a_quarter_block_each() {
        use crate::inventory::Stack;
        use crate::item::Item;
        let mut w = world();
        let shelf = shelves::Shelf {
            kind: Kind::Spruce,
            facing: Facing::South,
            powered: false,
            chain: shelves::Chain::Unconnected,
            wet: false,
        };
        w.set_block(AT, shelf.block());
        let mut hotbar = [None; 9];
        hotbar[0] = Some(Stack::new(Item::from_block(Block::STONE), 1));
        hotbar[1] = Some(Stack::new(Item::STICK, 1));
        // Clicks on the front (south) face: slot 0 on the left, slot 2 on the right.
        let front = |x: f64| AT.as_dvec3() + DVec3::new(x, 0.5, 1.0);
        w.use_shelf(AT, IVec3::Z, front(0.1), &mut hotbar, 0, false).unwrap();
        w.use_shelf(AT, IVec3::Z, front(0.9), &mut hotbar, 1, false).unwrap();
        let mut models = Vec::new();
        shelf_items(&w, AT.as_dvec3(), &mut models);
        assert_eq!(models.len(), 2);
        assert_eq!((models[0].block, models[0].icon), (Block::STONE, None));
        assert!(models[1].icon.is_some(), "a stick draws as its sprite");
        for m in &models {
            assert_eq!(m.size, 0.25);
            let centre = m.min + DVec3::splat(0.125) - AT.as_dvec3();
            assert!(centre.min_element() > 0.0 && centre.max_element() < 1.0, "{centre}");
        }
        // Left of the front as the player sees it (west, -x) for slot 0.
        assert!(models[0].min.x < models[1].min.x);
    }
}
