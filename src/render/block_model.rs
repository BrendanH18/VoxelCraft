//! Free-standing textured blocks (falling sand and gravel, dropped items):
//! cubes, two crossed planes for plants, or a flat item icon, built on the
//! CPU each frame from the block texture array and drawn in one call. Lit
//! like entities: an estimated sky light, the time of day and per-face
//! shading.

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec3};

use super::DEPTH_FORMAT;
use crate::world::block::{Block, RenderKind};
use crate::world::shape;

/// A block drawn outside the chunk meshes.
#[derive(Clone, Copy, Debug)]
pub struct BlockModel {
    /// World position of the minimum corner.
    pub min: DVec3,
    /// Edge length in blocks.
    pub size: f32,
    pub block: Block,
    /// Sky light at the block, 0..1.
    pub sky_light: f32,
    /// Torch light at the block, 0..1.
    pub block_light: f32,
    /// Turn about the vertical axis through the centre, in radians.
    pub yaw: f32,
    /// Draw this texture layer as a flat, upright square (an item icon)
    /// instead of the block.
    pub icon: Option<u16>,
    /// A full turn about the centre instead of `yaw` (an item in a frame).
    pub orient: Option<glam::Quat>,
    /// Per-stack item colour (white for blocks and untinted items).
    pub tint: [u8; 4],
    /// A banner's pattern layers (v0.15 Track B), drawn over its flag.
    pub patterns: Option<voxelcraft::inventory::components::StackPayload>,
    /// v0.16 Track B: a placed shulker box's lid, 0 shut to 1 open; the
    /// block is then drawn as `ShulkerBoxRenderer` draws it. v0.18 Track 9:
    /// a chest's lid the same way (`ChestRenderer`), and a barrel in use with
    /// its open top.
    pub lid: Option<f32>,
    /// v1.0 gap track 6b: an item with the enchantment glint
    /// (`ItemStack.hasFoil`).
    pub glint: bool,
    /// v1.0 gap track 6b: an item Java draws translucent (stained glass and
    /// its panes, tinted glass, ice, slime and honey blocks), blended
    /// instead of cut out.
    pub translucent: bool,
    /// v1.0 gap track 6b: an item drawn with its 3D mesh
    /// (`content::item_models`) in this display context: placed at `min`
    /// (the item's origin, not a corner), turned by `orient` or `yaw` and
    /// scaled by `size`.
    pub mesh: Option<(voxelcraft::inventory::Stack, voxelcraft::content::item_models::Context)>,
    /// v1.0 gap track 6c: an item placed by a whole matrix from its model
    /// space (a block is 0..1) to camera-relative blocks, its display
    /// transform included (held in a third-person hand, on a shelf):
    /// `min`, `size`, `yaw` and `orient` are then unused, and a mesh's
    /// context is ignored.
    pub transform: Option<glam::Mat4>,
}

/// v1.0 gap track 6c: a held item ([`crate::entity::model::HeldItemModel`])
/// as a model placed by its matrix: its 3D mesh, its model block (a pose's
/// statue first) or its sprite (a compass or clock frame first), tinted as
/// the stack is, with the glint and translucency of dropped items.
pub fn held_model(h: &crate::entity::model::HeldItemModel) -> BlockModel {
    let stack = h.stack;
    let mesh = (voxelcraft::content::item_models::has_mesh(stack.item)
        || voxelcraft::content::item_models::has_in_hand_mesh(stack.item))
    .then_some((stack, voxelcraft::content::item_models::Context::ThirdPersonRightHand));
    let block = if mesh.is_some() { None } else { h.block.or_else(|| crate::item::model_block(&stack)) };
    let icon = match (mesh, block) {
        (None, None) => h.icon.or_else(|| stack.item.flat_layer()),
        _ => None,
    };
    let tint =
        h.tint.or_else(|| crate::inventory::components::dye_tint(&stack)).map_or([255; 4], |[r, g, b]| [r, g, b, 255]);
    BlockModel {
        min: DVec3::ZERO,
        size: 1.0,
        block: block.unwrap_or(Block::AIR),
        sky_light: h.sky_light,
        block_light: h.block_light,
        yaw: 0.0,
        icon,
        orient: None,
        tint,
        patterns: None,
        lid: None,
        glint: stack.has_foil(),
        translucent: crate::item::translucent(&stack),
        mesh,
        transform: Some(h.transform),
    }
}

/// v1.0 gap track 6b: a vertex flag in [`BlockVertex::layer`]'s top bit:
/// the fragment adds the enchantment glint (see `block_model.wgsl`).
pub const GLINT: u32 = 1 << 31;

/// A frame's model vertices: the opaque (cut-out) ones, then the
/// translucent ones, drawn blended after them.
#[derive(Default)]
pub(super) struct ModelVertices {
    pub(super) opaque: Vec<BlockVertex>,
    pub(super) translucent: Vec<BlockVertex>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct BlockVertex {
    /// Camera-relative position.
    pub(super) pos: [f32; 3],
    pub(super) uv: [f32; 2],
    pub(super) layer: u32,
    /// x: sky light, y: face shade, z: torch light.
    pub(super) light: [f32; 3],
    pub(super) tint: [u8; 4],
}

/// Face shading, ordered +X, -X, +Y, -Y, +Z, -Z (as in `chunk.wgsl`).
const FACE_SHADE: [f32; 6] = [0.8, 0.8, 1.0, 0.55, 0.68, 0.68];

/// Triangles for every model, camera-relative. `ticks` (game ticks, with
/// the fraction) sways banner flags; `masks` caches flat icons' shapes.
pub(super) fn vertices(
    models: &[BlockModel],
    camera: DVec3,
    ticks: f64,
    masks: &mut super::hand::SpriteMasks,
) -> ModelVertices {
    let mut all = ModelVertices { opaque: Vec::with_capacity(models.len() * 36), translucent: Vec::new() };
    for m in models {
        let out = if m.translucent { &mut all.translucent } else { &mut all.opaque };
        let start = out.len();
        model_vertices(out, m, camera, ticks, masks);
        if m.glint {
            out[start..].iter_mut().for_each(|v| v.layer |= GLINT);
        }
    }
    all
}

fn model_vertices(
    out: &mut Vec<BlockVertex>,
    m: &BlockModel,
    camera: DVec3,
    ticks: f64,
    masks: &mut super::hand::SpriteMasks,
) {
    if let Some(t) = m.transform {
        transformed_vertices(out, m, t, masks);
        return;
    }
    if let Some((stack, context)) = m.mesh {
        let Some(mesh) = voxelcraft::content::item_models::mesh(&stack) else { return };
        let anchor = (m.min - camera).as_vec3();
        let (sin, cos) = m.yaw.sin_cos();
        let turn = |p: Vec3| match m.orient {
            Some(q) => q * p,
            None => Vec3::new(p.x * cos - p.z * sin, p.y, p.x * sin + p.z * cos),
        };
        for q in mesh.placed(context) {
            let q =
                crate::world::colour_blocks::banner::Quad { corners: q.corners.map(turn), normal: turn(q.normal), ..q };
            push_quad(out, m, anchor, q);
        }
        return;
    }
    {
        let min = (m.min - camera).as_vec3();
        let s = m.size;
        if let Some(b) = crate::world::colour_blocks::banner::decode(m.block.0) {
            banner_vertices(out, m, b, min, ticks);
            return;
        }
        if let (Some(lid), Some(s)) = (m.lid, crate::world::shulker_box::decode(m.block)) {
            crate::world::shulker_box::quads(s, lid, |q| push_quad(out, m, min, q));
            return;
        }
        // v0.18 Track 9: a chest in use, its lid at `lid` (`ChestRenderer`).
        if let (Some(lid), Some((facing, bounds))) = (m.lid, crate::world::lids::chest_model(m.block)) {
            let faces = m.block.info().tex;
            crate::world::lids::chest_quads(facing, bounds, faces, lid, |q| push_quad(out, m, min, q));
            return;
        }
        // v0.16 Track C: placed heads (`SkullBlockRenderer`); a powered
        // piglin or dragon head animates with the game ticks.
        if m.icon.is_none()
            && m.orient.is_none()
            && m.size == 1.0
            && let Some(s) = crate::world::skulls::decode(m.block)
        {
            let anim = if s.powered { ticks as f32 } else { 0.0 };
            quad_vertices(out, m, &crate::world::skulls::quads(s, anim), min);
            return;
        }
        // v0.18 Track 9: a barrel in use shows its open top.
        // An item's front faces north, as Java's item models have it.
        let tex = m
            .lid
            .and_then(|_| crate::world::lids::barrel_faces(m.block))
            .unwrap_or_else(|| if m.lid.is_none() { m.block.item_facing() } else { m.block }.info().tex);
        let (sin, cos) = m.yaw.sin_cos();
        // Unit-cube corner -> camera-relative position, turned about the centre.
        let place = |c: Vec3| match m.orient {
            Some(q) => min + (q * (c - Vec3::splat(0.5)) + Vec3::splat(0.5)) * s,
            None => {
                let (x, z) = (c.x - 0.5, c.z - 0.5);
                min + Vec3::new(x * cos - z * sin + 0.5, c.y, x * sin + z * cos + 0.5) * s
            }
        };
        // v1.0 gap track 6d: Java's face UVs, as `chunk.wgsl` maps them.
        // v1.0 gap track 6f: a box face (its box in sixteenths) takes the
        // jar's item UVs where the item model has them (trapdoors' rims).
        let mut quad = |corners: [Vec3; 4], layer: u16, shade: f32, face: usize, bx: Option<([u8; 3], [u8; 3])>| {
            let mut uvs = corners.map(|c| crate::mesh::face_uv(face, c.to_array()));
            if let Some((lo, hi)) = bx {
                uvs = crate::mesh::item_face_uvs(m.block, face, lo, hi, uvs);
            }
            for i in [0, 1, 2, 2, 3, 0] {
                let c = corners[i];
                out.push(BlockVertex {
                    pos: place(c).to_array(),
                    uv: uvs[i],
                    layer: layer as u32,
                    light: [m.sky_light, shade, m.block_light],
                    tint: m.tint,
                });
            }
        };
        if let Some(layer) = m.icon {
            // v1.0 gap track 6b: Java's `item/generated` model, the sprite
            // extruded 1/16 of the model deep and centred (7.5 to 8.5), so
            // 1/32 of a block for a dropped item's half-size model.
            drop(quad);
            let mask = masks.get(layer);
            let turn = |p: Vec3| match m.orient {
                Some(q) => q * p,
                None => Vec3::new(p.x * cos - p.z * sin, p.y, p.x * sin + p.z * cos),
            };
            super::hand::extrude(mask, |c, uv| {
                let c = c.map(|p| Vec3::new(p.x, p.y, p.z + 0.5 - 1.0 / 32.0));
                let n = turn((c[1] - c[0]).cross(c[2] - c[0]).normalize_or_zero());
                let vertical = if n.y > 0.0 { FACE_SHADE[2] } else { FACE_SHADE[3] };
                let shade = n.y * n.y * vertical + n.x * n.x * FACE_SHADE[0] + n.z * n.z * FACE_SHADE[4];
                // The front and back faces keep the full light a flat icon had.
                let shade = if n.z.abs() > 0.99 && m.orient.is_none() { 1.0 } else { shade };
                for i in [0, 1, 2, 2, 3, 0] {
                    out.push(BlockVertex {
                        pos: place(c[i]).to_array(),
                        uv: uv[i],
                        layer: layer as u32,
                        light: [m.sky_light, shade, m.block_light],
                        tint: m.tint,
                    });
                }
            });
            return;
        }
        if m.block.kind() == RenderKind::Cross {
            // Both diagonal planes run 0 -> 1 in z, so z works as u (a west
            // face's mapping).
            let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
            quad([v(0., 0., 0.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 0.)], tex[0], 0.9, 1, None);
            quad([v(1., 0., 0.), v(0., 0., 1.), v(0., 1., 1.), v(1., 1., 0.)], tex[0], 0.9, 1, None);
            return;
        }
        // Falling blocks and every item model share the item's boxes.
        let boxes = shape::item_boxes(m.block);
        for (i, bx) in boxes.as_slice().iter().enumerate() {
            // Candle wicks, the beacon's plinth and core: their own layers.
            let tex = shape::item_box_layer(m.block, i).map_or(tex, |l| [l; 6]);
            let (lo, mut hi) = (bx.min.map(|c| c as f32 / 16.0), bx.max.map(|c| c as f32 / 16.0));
            if m.block == crate::world::frog_blocks::FROGSPAWN {
                hi[1] = crate::world::frog_blocks::HEIGHT as f32;
            }
            for (face, (&layer, &shade)) in tex.iter().zip(&FACE_SHADE).enumerate() {
                let (d, positive) = (face / 2, face % 2 == 0);
                let (u, v) = ((d + 1) % 3, (d + 2) % 3);
                let corner = |a: bool, b: bool| {
                    let mut p = [0.0f32; 3];
                    p[d] = if positive { hi[d] } else { lo[d] };
                    p[u] = if a { hi[u] } else { lo[u] };
                    p[v] = if b { hi[v] } else { lo[v] };
                    Vec3::from_array(p)
                };
                // Counter-clockwise seen from outside (the pipeline doesn't
                // cull, so this is only for consistency).
                let c = if positive {
                    [corner(false, false), corner(true, false), corner(true, true), corner(false, true)]
                } else {
                    [corner(false, false), corner(false, true), corner(true, true), corner(true, false)]
                };
                quad(c, layer, shade, face, Some((bx.min, bx.max)));
            }
        }
    }
}

/// v1.0 gap track 6c: an item placed by `t` ([`BlockModel::transform`]):
/// its 3D mesh, its extruded sprite (Java's `item/generated` model, 7.5 to
/// 8.5 sixteenths deep) or its block's boxes, every face shaded by how it
/// ends up turned.
fn transformed_vertices(
    out: &mut Vec<BlockVertex>,
    m: &BlockModel,
    t: glam::Mat4,
    masks: &mut super::hand::SpriteMasks,
) {
    let mut push = |corners: [Vec3; 4], normal: Vec3, uv: [[f32; 2]; 4], layer: u16, tint: [u8; 4]| {
        let n = t.transform_vector3(normal).normalize_or_zero();
        let vertical = if n.y > 0.0 { FACE_SHADE[2] } else { FACE_SHADE[3] };
        let shade = n.y * n.y * vertical + n.x * n.x * FACE_SHADE[0] + n.z * n.z * FACE_SHADE[4];
        for i in [0, 1, 2, 2, 3, 0] {
            out.push(BlockVertex {
                pos: t.transform_point3(corners[i]).to_array(),
                uv: uv[i],
                layer: layer as u32,
                light: [m.sky_light, shade, m.block_light],
                tint,
            });
        }
    };
    // v1.0 gap track 6d: a hand holds the trident's and spyglass's in-hand models.
    let hand = |c| c == voxelcraft::content::item_models::Context::ThirdPersonRightHand;
    if let Some(mesh) = m.mesh.and_then(|(stack, context)| {
        voxelcraft::content::item_models::mesh(&stack)
            .or_else(|| hand(context).then(|| voxelcraft::content::item_models::in_hand_mesh(&stack)).flatten())
    }) {
        for q in &mesh.quads {
            push(q.corners, q.normal, q.uv, q.layer, q.tint.unwrap_or(m.tint));
        }
        return;
    }
    if let Some(layer) = m.icon {
        let depth = Vec3::new(0.0, 0.0, 7.5 / 16.0);
        super::hand::extrude(masks.get(layer), |c, uv| {
            let normal = (c[1] - c[0]).cross(c[2] - c[0]);
            push(c.map(|p| p + depth), normal, uv, layer, m.tint);
        });
        return;
    }
    let block = m.block.item_facing();
    let tex = block.info().tex;
    if block.kind() == RenderKind::Cross {
        let v = Vec3::new;
        let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        for (c, n) in [
            ([v(0., 0., 0.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 0.)], v(-1., 0., 1.)),
            ([v(1., 0., 0.), v(0., 0., 1.), v(0., 1., 1.), v(1., 1., 0.)], v(1., 0., 1.)),
        ] {
            push(c, n, uv, tex[0], m.tint);
        }
        return;
    }
    // v1.0 gap track 6d: the block item's model as a slot draws it (Java's
    // face UVs, a head's skull box), so every context shows the same model.
    for q in super::ui::block_item_quads(block) {
        push(q.corners, q.normal, q.uv, q.layer, m.tint);
    }
}

/// v1.0 gap track 6c: the lowest and highest y of `stack`'s model placed by
/// `local` (`ItemStackRenderState.getModelBoundingBox`): its 3D mesh, the
/// boxes of `block`, or the whole square of its extruded sprite.
pub fn extent_y(stack: &voxelcraft::inventory::Stack, block: Option<Block>, local: glam::Mat4) -> (f32, f32) {
    let mut corners: Vec<Vec3> = Vec::new();
    if let Some(mesh) = voxelcraft::content::item_models::mesh(stack) {
        corners.extend(mesh.quads.iter().flat_map(|q| q.corners));
    } else if let Some(block) = block {
        corners.extend(super::ui::block_item_quads(block).iter().flat_map(|q| q.corners));
    } else {
        for i in 0..8 {
            let z = if i & 4 == 0 { 7.5 } else { 8.5 } / 16.0;
            corners.push(Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, z));
        }
    }
    corners
        .iter()
        .map(|&c| local.transform_point3(c).y)
        .fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)))
}

/// Quads of a block-entity model, lit by its cell and shaded by how each
/// face is turned.
fn quad_vertices(
    out: &mut Vec<BlockVertex>,
    m: &BlockModel,
    quads: &[crate::world::colour_blocks::banner::Quad],
    min: Vec3,
) {
    for q in quads {
        let n = q.normal;
        let vertical = if n.y > 0.0 { FACE_SHADE[2] } else { FACE_SHADE[3] };
        let shade = n.y * n.y * vertical + n.x * n.x * FACE_SHADE[0] + n.z * n.z * FACE_SHADE[4];
        for i in [0, 1, 2, 2, 3, 0] {
            out.push(BlockVertex {
                pos: (min + q.corners[i] * m.size).to_array(),
                uv: q.uv[i],
                layer: q.layer as u32,
                light: [m.sky_light, shade, m.block_light],
                tint: q.tint.unwrap_or(m.tint),
            });
        }
    }
}

/// A banner's pole, bar and flag (`colour_blocks::banner::quads`), lit by
/// its cell and shaded by how each face is turned.
fn banner_vertices(
    out: &mut Vec<BlockVertex>,
    m: &BlockModel,
    b: crate::world::colour_blocks::Banner,
    min: Vec3,
    ticks: f64,
) {
    use crate::world::colour_blocks::banner;
    let cell = m.min.floor().as_ivec3();
    let layers = match m.patterns.map(|p| p.get()) {
        Some(voxelcraft::inventory::components::Payload::Banner(p)) => p.layers.as_slice(),
        _ => &[],
    };
    for q in banner::quads(b, banner::phase(cell, ticks)).into_iter().chain(banner::pattern_quads(
        b,
        banner::phase(cell, ticks),
        layers,
    )) {
        push_quad(out, m, min, q);
    }
}

/// One model quad, lit by its cell and shaded by how it is turned.
fn push_quad(out: &mut Vec<BlockVertex>, m: &BlockModel, min: Vec3, q: crate::world::colour_blocks::banner::Quad) {
    // FACE_SHADE blended by the normal: up, down, then x and z sides.
    let n = q.normal;
    let vertical = if n.y > 0.0 { FACE_SHADE[2] } else { FACE_SHADE[3] };
    let shade = n.y * n.y * vertical + n.x * n.x * FACE_SHADE[0] + n.z * n.z * FACE_SHADE[4];
    for i in [0, 1, 2, 2, 3, 0] {
        out.push(BlockVertex {
            pos: (min + q.corners[i] * m.size).to_array(),
            uv: q.uv[i],
            layer: q.layer as u32,
            light: [m.sky_light, shade, m.block_light],
            tint: q.tint.unwrap_or(m.tint),
        });
    }
}

pub(super) struct BlockModelPass {
    pipeline: wgpu::RenderPipeline,
    /// The first-person hand: depth pulled in front of everything.
    hand_pipeline: wgpu::RenderPipeline,
    /// v1.0 gap track 6b: translucent items (stained glass), blended over
    /// the rest without writing depth, in the world and in the hand.
    translucent_pipeline: wgpu::RenderPipeline,
    hand_translucent_pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: usize,
    /// Vertex ranges, in buffer order: world models, the hand, then the
    /// translucent world models and the translucent hand.
    ranges: [std::ops::Range<u32>; 4],
    sprite_masks: super::hand::SpriteMasks,
}

impl BlockModelPass {
    pub(super) fn new(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        paged_blocks: Option<u32>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("block model shader"),
            source: super::block_shader(include_str!("shaders/block_model.wgsl"), paged_blocks),
        });
        let pipeline = |label, entry_point, translucent: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<BlockVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3, 1 => Float32x2, 2 => Uint32, 3 => Float32x3, 4 => Unorm8x4
                        ],
                    })],
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(!translucent),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(if translucent { "fs_translucent" } else { "fs_main" }),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: translucent.then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let capacity = 36 * 16;
        Self {
            pipeline: pipeline("block models", "vs_main", false),
            hand_pipeline: pipeline("hand", "vs_hand", false),
            translucent_pipeline: pipeline("translucent block models", "vs_main", true),
            hand_translucent_pipeline: pipeline("translucent hand", "vs_hand", true),
            buffer: Self::create_buffer(device, capacity),
            capacity,
            ranges: Default::default(),
            sprite_masks: Default::default(),
        }
    }

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("block model vertices"),
            size: (capacity * std::mem::size_of::<BlockVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Uploads this frame's models, signs and hand.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        models: &[BlockModel],
        held: &[BlockModel],
        signs: &[super::signs::SignModel],
        hand: Option<(&super::hand::Hand, Vec3, f32, f32, glam::Mat4)>,
        camera: DVec3,
        ticks: f64,
    ) {
        let ModelVertices { opaque: mut verts, mut translucent } =
            vertices(models, camera, ticks, &mut self.sprite_masks);
        // v1.0 gap track 6c: items in third-person and mob hands.
        let more = vertices(held, camera, ticks, &mut self.sprite_masks);
        verts.extend(more.opaque);
        translucent.extend(more.translucent);
        super::signs::vertices(signs, camera, &mut verts);
        let world = verts.len() as u32;
        let mut hand_translucent = Vec::new();
        if let Some((hand, forward, fov_y, aspect, view_effect)) = hand {
            let v = super::hand::vertices(hand, forward, fov_y, aspect, view_effect, &mut self.sprite_masks);
            if hand.translucent { hand_translucent = v } else { verts.extend(v) }
        }
        let held = verts.len() as u32;
        verts.extend(translucent);
        let blended = verts.len() as u32;
        verts.extend(hand_translucent);
        self.ranges = [0..world, world..held, held..blended, blended..verts.len() as u32];
        if verts.len() > self.capacity {
            self.capacity = verts.len().next_power_of_two();
            self.buffer = Self::create_buffer(device, self.capacity);
        }
        if !verts.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&verts));
        }
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.ranges.iter().all(|r| r.is_empty()) {
            return;
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        let pipelines =
            [&self.pipeline, &self.hand_pipeline, &self.translucent_pipeline, &self.hand_translucent_pipeline];
        for (range, pipeline) in self.ranges.iter().zip(pipelines) {
            if !range.is_empty() {
                pass.set_pipeline(pipeline);
                pass.draw(range.clone(), 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubes_and_plants_get_their_triangles() {
        let at = |block| BlockModel {
            patterns: None,
            lid: None,
            min: DVec3::new(10.0, 64.0, 5.0),
            size: 1.0,
            block,
            sky_light: 1.0,
            block_light: 0.0,
            yaw: 0.0,
            icon: None,
            orient: None,
            tint: [255; 4],
            glint: false,
            translucent: false,
            mesh: None,
            transform: None,
        };
        let icon = BlockModel { icon: Some(crate::world::block::tex::ITEM_BASE), yaw: 1.0, ..at(Block::AIR) };
        let mut masks = super::super::hand::SpriteMasks::default();
        let v = vertices(&[at(Block::SAND), at(Block::POPPY), icon], DVec3::new(10.0, 64.0, 5.0), 0.0, &mut masks);
        let v = v.opaque;
        // v1.0 gap track 6b: the icon is extruded, its two faces and edges.
        assert!(v.len() > 36 + 12 + 12, "{}", v.len());
        // Camera-relative and within the unit cube (a turned icon too).
        assert!(v.iter().all(|v| v.pos.iter().all(|&c| (-1e-6..=1.0 + 1e-6).contains(&c))));
        assert_eq!(v[36].layer, crate::world::block::tex::POPPY as u32);
        assert_eq!(v[48].layer, crate::world::block::tex::ITEM_BASE as u32);
        // 1/32 deep for a half-size model, about the centre plane.
        let half = BlockModel { size: 0.5, ..icon };
        let v = vertices(&[half], DVec3::new(10.0, 64.0, 5.0), 0.0, &mut masks).opaque;
        let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            let (x, z) = (v.pos[0] - 0.25, v.pos[2] - 0.25);
            let depth = -x * 1f32.sin() + z * 1f32.cos();
            (lo.min(depth), hi.max(depth))
        });
        assert!((hi - lo - 1.0 / 32.0).abs() < 1e-4 && (hi + lo).abs() < 1e-4, "{lo} {hi}");
    }

    #[test]
    fn glinting_and_translucent_items_are_flagged_and_set_apart() {
        let item = |translucent, glint| BlockModel {
            patterns: None,
            lid: None,
            min: DVec3::ZERO,
            size: 0.25,
            block: Block::SAND,
            sky_light: 1.0,
            block_light: 0.0,
            yaw: 0.0,
            icon: None,
            orient: None,
            tint: [255; 4],
            glint,
            translucent,
            mesh: None,
            transform: None,
        };
        let mut masks = super::super::hand::SpriteMasks::default();
        let v = vertices(&[item(false, true), item(true, false)], DVec3::ZERO, 0.0, &mut masks);
        assert_eq!((v.opaque.len(), v.translucent.len()), (36, 36));
        assert!(v.opaque.iter().all(|v| v.layer & GLINT != 0));
        assert!(v.translucent.iter().all(|v| v.layer & GLINT == 0));
    }
}
