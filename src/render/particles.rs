//! One fixed-buffer instanced draw for depth-writing, alpha-tested billboards.
//! Drawing before water lets translucent surfaces tint particles behind them
//! while foreground particles occlude water. No particle sorting is needed.

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec3};

use super::{DEPTH_FORMAT, Renderer};
use crate::particles::{CAPACITY, Kind, Motion, Pool, Texture};
use crate::world::{
    World,
    block::{Block, tex},
};

const SPRITES: usize = 15;
const FRAMES: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    center: [f32; 3],
    size: f32,
    uv: [f32; 4],
    color: [f32; 4],
    /// sky, block, texture array layer, block/sprite flag
    light: [f32; 4],
}

pub(super) struct ParticlePass {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    camera: wgpu::Buffer,
    sprites: wgpu::BindGroup,
    staging: Vec<Instance>,
    count: u32,
}

impl ParticlePass {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals: &wgpu::BindGroupLayout,
        blocks: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        paged_blocks: bool,
    ) -> Self {
        let sprite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle sprites layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let view = Renderer::create_texture_array(
            device,
            queue,
            "particle sprites",
            (SPRITES * FRAMES) as u32,
            vec![sprites()],
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("particle sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle camera axes"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sprites = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle sprites"),
            layout: &sprite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: camera.as_entire_binding() },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle pipeline layout"),
            bind_group_layouts: &[Some(globals), Some(blocks), Some(&sprite_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle shader"),
            source: super::block_shader(include_str!("shaders/particles.wgsl"), paged_blocks),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"), layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64, step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs_main"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }), multiview_mask: None, cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle instances"),
            size: (CAPACITY * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, buffer, camera, sprites, staging: Vec::with_capacity(CAPACITY), count: 0 }
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        if self.count == 0 {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(2, &self.sprites, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..6, 0..self.count);
        1
    }
}

impl Renderer {
    pub fn set_particles(&mut self, pool: &Pool, world: &World, camera: DVec3, forward: Vec3, alpha: f64) {
        let pass = &mut self.particles;
        pass.staging.clear();
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward).normalize();
        let axes = [right.x, right.y, right.z, 0.0, up.x, up.y, up.z, 0.0];
        for p in pool.iter() {
            let pos = p.previous.lerp(p.pos, alpha);
            if pos.distance_squared(camera) > 128.0 * 128.0 {
                continue;
            }
            let t = ((p.age as f32 + alpha as f32) / p.lifetime.max(1) as f32).clamp(0.0, 1.0);
            let (layer, terrain) = match p.texture {
                Texture::Block(b) => {
                    let b = if b == Block::GRASS { Block::DIRT } else { b.base() };
                    let layer = tex::tinted(
                        b.info().tex[0],
                        world.foliage_at(p.origin.x as i32, p.origin.z as i32).unwrap_or(0),
                    );
                    (layer as f32, 1.0)
                }
                Texture::Sprite(s) => ((s as usize * FRAMES + (t * (FRAMES - 1) as f32) as usize) as f32, 0.0),
            };
            let scale = if matches!(p.texture, Texture::Block(_)) {
                1.0
            } else {
                match p.style {
                    Kind::Portal => 1.0 - (1.0 - t).powi(2),
                    Kind::Flame | Kind::SoulFlame => 1.0 - t * t * 0.5,
                    Kind::Lava => 1.0 - t * t,
                    Kind::Smoke | Kind::LargeSmoke => 1.0 - t,
                    Kind::Crit | Kind::MagicCrit | Kind::Heart | Kind::Angry | Kind::DragonBreath => {
                        (t * 32.0).clamp(0.0, 1.0)
                    }
                    _ => 1.0,
                }
            };
            let sky = (crate::entity::sky_light(world, pos)
                + if p.motion == Motion::Portal || p.motion == Motion::Glyph { t.powi(4) } else { 0.0 })
            .min(1.0);
            pass.staging.push(Instance {
                center: (pos - camera).as_vec3().to_array(),
                size: p.size * scale,
                uv: p.uv,
                color: p.color,
                light: [
                    if p.emissive { 1.0 } else { sky },
                    if p.emissive {
                        1.0
                    } else {
                        (world.block_light(pos.floor().as_ivec3()) as f32 / 15.0
                            + if matches!(p.style, Kind::Flame | Kind::SoulFlame) { t } else { 0.0 })
                        .min(1.0)
                    },
                    layer,
                    terrain + if p.emissive { 2.0 } else { 0.0 },
                ],
            });
        }
        pass.count = pass.staging.len() as u32;
        if pass.count > 0 {
            self.queue.write_buffer(&pass.camera, 0, bytemuck::cast_slice(&axes));
            self.queue.write_buffer(&pass.buffer, 0, bytemuck::cast_slice(&pass.staging));
        }
    }
}

/// Original procedural 16px sprites, eight stages each. No external assets.
fn sprites() -> Vec<u8> {
    let mut data = vec![0; SPRITES * FRAMES * 16 * 16 * 4];
    for kind in 0..SPRITES {
        for frame in 0..FRAMES {
            for y in 0..16 {
                for x in 0..16 {
                    let dx = x as f32 - 7.5;
                    let dy = y as f32 - 7.5;
                    let r = (dx * dx + dy * dy).sqrt();
                    let noise = ((x * 37 + y * 17 + frame * 13) % 11) as f32;
                    let on = match kind {
                        // Smoke/poof: a small grey puff that thins as it ages,
                        // not a filled disc (those read as black blobs).
                        0 => r < 4.2 - frame as f32 * 0.35 && noise > 1.5 + frame as f32 * 0.7,
                        10 | 11 => r < 7.0 - frame as f32 * 0.45 + noise * 0.13 && noise > frame as f32 * 0.65,
                        1 | 14 => dy > -6.0 && dy < 6.0 && dx.abs() < (dy + 8.0) * 0.38 && noise > 1.0,
                        // Spores and ash: a speck of a few pixels.
                        13 => r < 1.9,
                        2 => (dx.abs() < 1.5 || dy.abs() < 1.5 || (dx.abs() - dy.abs()).abs() < 1.0) && r < 6.0,
                        3 => (4.0..6.0).contains(&r) || (dx < -1.0 && dy < -1.0 && r < 4.5),
                        4 => dy.abs() < 1.5 && dx.abs() < 7.0 || dx.abs() < 2.0 && (dy + 3.0).abs() < 2.0,
                        5 => dx.abs() + dy.abs() < 6.0 && noise > 2.0,
                        6 => {
                            x > 3 && x < 12 && y > 2 && y < 13 && (x.is_multiple_of(3) || (y + frame).is_multiple_of(4))
                        }
                        7 => {
                            ((dx - 3.0).powi(2) + (dy + 3.0).powi(2) < 12.0
                                || (dx + 3.0).powi(2) + (dy + 3.0).powi(2) < 12.0
                                || dy >= -2.0 && dx.abs() + dy < 7.0)
                                && dy < 7.0
                        }
                        8 => (dx.abs() - dy.abs()).abs() < 1.5 && r < 6.0,
                        9 => r < 6.0 && r > 3.0 && (dx > 0.0 || dy > 0.0) || (dy + 3.0).abs() < 1.5 && dx.abs() < 4.0,
                        _ => dx.abs() < 2.5 && dy.abs() < 4.0,
                    };
                    if !on {
                        continue;
                    }
                    let color = if kind == 1 {
                        [255, (170.0 + dy * 10.0).clamp(70.0, 250.0) as u8, 35, 255]
                    } else if kind == 14 {
                        [60, (190.0 + dy * 8.0).clamp(120.0, 255.0) as u8, 255, 255]
                    } else {
                        let c = (220.0 + noise * 3.0).min(255.0) as u8;
                        [c, c, c, 255]
                    };
                    let i = (((kind * FRAMES + frame) * 16 + y) * 16 + x) * 4;
                    data[i..i + 4].copy_from_slice(&color);
                }
            }
        }
    }
    data
}
