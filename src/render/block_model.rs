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
}

/// Face shading, ordered +X, -X, +Y, -Y, +Z, -Z (as in `chunk.wgsl`).
const FACE_SHADE: [f32; 6] = [0.8, 0.8, 1.0, 0.55, 0.68, 0.68];

/// Triangles for every model, camera-relative.
pub fn vertices(models: &[BlockModel], camera: DVec3) -> Vec<BlockVertex> {
    let mut out = Vec::with_capacity(models.len() * 36);
    for m in models {
        let min = (m.min - camera).as_vec3();
        let s = m.size;
        let tex = m.block.info().tex;
        let (sin, cos) = m.yaw.sin_cos();
        // Unit-cube corner -> camera-relative position, turned about the centre.
        let place = |c: Vec3| {
            let (x, z) = (c.x - 0.5, c.z - 0.5);
            min + Vec3::new(x * cos - z * sin + 0.5, c.y, x * sin + z * cos + 0.5) * s
        };
        // UVs as in `chunk.wgsl`: sides map (horizontal, down), tops map (x, z).
        let mut quad = |corners: [Vec3; 4], layer: u16, shade: f32, axis: usize| {
            for i in [0, 1, 2, 2, 3, 0] {
                let c = corners[i];
                let uv = match axis {
                    0 => [c.z, 1.0 - c.y],
                    1 => [c.x, c.z],
                    _ => [c.x, 1.0 - c.y],
                };
                out.push(BlockVertex {
                    pos: place(c).to_array(),
                    uv,
                    layer: layer as u32,
                    light: [m.sky_light, shade, m.block_light],
                });
            }
        };
        if let Some(layer) = m.icon {
            let v = |x: f32, y: f32| Vec3::new(x, y, 0.5);
            quad([v(0., 0.), v(1., 0.), v(1., 1.), v(0., 1.)], layer, 1.0, 2);
            continue;
        }
        if m.block.kind() == RenderKind::Cross {
            // Both diagonal planes run 0 -> 1 in z, so z works as u.
            let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
            quad([v(0., 0., 0.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 0.)], tex[0], 0.9, 0);
            quad([v(1., 0., 0.), v(0., 0., 1.), v(0., 1., 1.), v(1., 1., 0.)], tex[0], 0.9, 0);
            continue;
        }
        let mut boxes = shape::item_shape(m.block);
        if boxes.is_empty() {
            boxes = shape::Boxes::from_box(shape::Box16::FULL);
        }
        for bx in boxes.as_slice() {
            let (lo, hi) = (bx.min.map(|c| c as f32 / 16.0), bx.max.map(|c| c as f32 / 16.0));
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
                quad(c, layer, shade, d);
            }
        }
    }
    out
}

pub(super) struct BlockModelPass {
    pipeline: wgpu::RenderPipeline,
    /// The first-person hand: depth pulled in front of everything.
    hand_pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: usize,
    /// Vertices of world models, then of the hand.
    count: u32,
    hand_count: u32,
    sprite_masks: super::hand::SpriteMasks,
}

impl BlockModelPass {
    pub(super) fn new(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        paged_blocks: bool,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("block model shader"),
            source: super::block_shader(include_str!("shaders/block_model.wgsl"), paged_blocks),
        });
        let pipeline = |label, entry_point| {
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
                            0 => Float32x3, 1 => Float32x2, 2 => Uint32, 3 => Float32x3
                        ],
                    })],
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let capacity = 36 * 16;
        Self {
            pipeline: pipeline("block models", "vs_main"),
            hand_pipeline: pipeline("hand", "vs_hand"),
            buffer: Self::create_buffer(device, capacity),
            capacity,
            count: 0,
            hand_count: 0,
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

    /// Uploads this frame's models and hand.
    pub(super) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        models: &[BlockModel],
        hand: Option<(&super::hand::Hand, Vec3, f32, f32)>,
        camera: DVec3,
    ) {
        let mut verts = vertices(models, camera);
        self.count = verts.len() as u32;
        if let Some((hand, forward, fov_y, aspect)) = hand {
            verts.extend(super::hand::vertices(hand, forward, fov_y, aspect, &mut self.sprite_masks));
        }
        self.hand_count = verts.len() as u32 - self.count;
        if verts.len() > self.capacity {
            self.capacity = verts.len().next_power_of_two();
            self.buffer = Self::create_buffer(device, self.capacity);
        }
        if !verts.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&verts));
        }
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count + self.hand_count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        if self.count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.draw(0..self.count, 0..1);
        }
        if self.hand_count > 0 {
            pass.set_pipeline(&self.hand_pipeline);
            pass.draw(self.count..self.count + self.hand_count, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubes_and_plants_get_their_triangles() {
        let at = |block| BlockModel {
            min: DVec3::new(10.0, 64.0, 5.0),
            size: 1.0,
            block,
            sky_light: 1.0,
            block_light: 0.0,
            yaw: 0.0,
            icon: None,
        };
        let icon = BlockModel { icon: Some(crate::world::block::tex::ITEM_BASE), yaw: 1.0, ..at(Block::AIR) };
        let v = vertices(&[at(Block::SAND), at(Block::POPPY), icon], DVec3::new(10.0, 64.0, 5.0));
        assert_eq!(v.len(), 36 + 12 + 6);
        // Camera-relative and within the unit cube (a turned icon too).
        assert!(v.iter().all(|v| v.pos.iter().all(|&c| (-1e-6..=1.0 + 1e-6).contains(&c))));
        assert_eq!(v[36].layer, crate::world::block::tex::POPPY as u32);
        assert_eq!(v[48].layer, crate::world::block::tex::ITEM_BASE as u32);
    }
}
