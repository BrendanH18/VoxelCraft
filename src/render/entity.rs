//! Entity pass: every mob's plain boxes in one instanced draw (the vertex
//! shader expands each [`BoxInstance`] into its faces), then the textured,
//! translucent and flat geometry from a per-frame buffer of camera-relative
//! triangles. Both are built by `crate::entity::model`.

use super::{DEPTH_FORMAT, Renderer};

pub use crate::entity::model::EntityVertex;
use crate::entity::model::{BOX_VERTICES, BoxInstance, EntityMesh};

pub(super) struct EntityPass {
    /// Opaque box instances.
    boxes: wgpu::RenderPipeline,
    box_buffer: wgpu::Buffer,
    box_capacity: usize,
    box_count: u32,
    pipeline: wgpu::RenderPipeline,
    /// Alpha-blended, depth-tested but not depth-written (slime shells and
    /// ground shadows).
    translucent: wgpu::RenderPipeline,
    /// Vertices before this index are opaque.
    opaque: u32,
    skin: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    capacity: usize,
    count: u32,
    /// v0.15 Track D: the Glowing mask pipelines and the glowing entities'
    /// geometry (see `super::outline`).
    mask_boxes: wgpu::RenderPipeline,
    mask_verts: wgpu::RenderPipeline,
    glow_box_buffer: wgpu::Buffer,
    glow_box_capacity: usize,
    glow_box_count: u32,
    glow_buffer: wgpu::Buffer,
    glow_capacity: usize,
    glow_count: u32,
}

impl EntityPass {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals: &wgpu::BindGroupLayout,
        blocks: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        paged_blocks: Option<u32>,
    ) -> Self {
        let skin_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("humanoid skin layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("entity pipeline layout"),
            bind_group_layouts: &[Some(globals), Some(blocks), Some(&skin_layout)],
            immediate_size: 0,
        });
        let layers = crate::entity::armor::ARRAY_LAYERS;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("player skin and armor"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut pixels = crate::entity::player_model::skin_pixels();
        pixels.extend(crate::entity::armor::layer_pixels());
        debug_assert_eq!(pixels.len(), layers as usize * 64 * 64 * 4);
        queue.write_texture(
            texture.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(64) },
            texture.size(),
        );
        let skin = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("humanoid skin"),
            layout: &skin_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture.create_view(&Default::default())),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("entity shader"),
            source: super::block_shader(include_str!("shaders/entity.wgsl"), paged_blocks),
        });
        let vertices = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<EntityVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Unorm8x4
            ],
        };
        let instances = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BoxInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4,
                5 => Unorm8x4, 6 => Unorm8x4, 7 => Unorm8x4, 8 => Unorm8x4
            ],
        };
        let make = |label, entry_point, buffer, depth_write, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    buffers: &[Some(buffer)],
                },
                primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let make_mask = |label, entry_point, buffer| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    buffers: &[Some(buffer)],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_mask"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: super::outline::MASK_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let mask_boxes = make_mask("glow mask boxes", "vs_box", instances.clone());
        let mask_verts = make_mask("glow mask", "vs_main", vertices.clone());
        let boxes = make("entity boxes", "vs_box", instances, true, None);
        let pipeline = make("entities", "vs_main", vertices.clone(), true, None);
        let translucent =
            make("translucent entities", "vs_main", vertices, false, Some(wgpu::BlendState::ALPHA_BLENDING));
        let capacity = 4096;
        let box_capacity = 1024;
        Self {
            boxes,
            box_buffer: Self::create_buffer(device, "entity boxes", box_capacity * std::mem::size_of::<BoxInstance>()),
            box_capacity,
            box_count: 0,
            pipeline,
            translucent,
            opaque: 0,
            skin,
            buffer: Self::create_buffer(device, "entity vertices", capacity * std::mem::size_of::<EntityVertex>()),
            capacity,
            count: 0,
            mask_boxes,
            mask_verts,
            glow_box_buffer: Self::create_buffer(device, "glow boxes", 64 * std::mem::size_of::<BoxInstance>()),
            glow_box_capacity: 64,
            glow_box_count: 0,
            glow_buffer: Self::create_buffer(device, "glow vertices", 256 * std::mem::size_of::<EntityVertex>()),
            glow_capacity: 256,
            glow_count: 0,
        }
    }

    /// Whether any glowing entity is in this frame's geometry.
    pub(super) fn glowing(&self) -> bool {
        self.glow_box_count > 0 || self.glow_count > 0
    }

    /// Draws the glowing entities into the outline mask.
    pub(super) fn draw_mask(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(2, &self.skin, &[]);
        if self.glow_box_count > 0 {
            pass.set_pipeline(&self.mask_boxes);
            pass.set_vertex_buffer(0, self.glow_box_buffer.slice(..));
            pass.draw(0..BOX_VERTICES, 0..self.glow_box_count);
        }
        if self.glow_count > 0 {
            pass.set_pipeline(&self.mask_verts);
            pass.set_vertex_buffer(0, self.glow_buffer.slice(..));
            pass.draw(0..self.glow_count, 0..1);
        }
    }

    fn create_buffer(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.box_count > 0 {
            pass.set_pipeline(&self.boxes);
            pass.set_bind_group(2, &self.skin, &[]);
            pass.set_vertex_buffer(0, self.box_buffer.slice(..));
            pass.draw(0..BOX_VERTICES, 0..self.box_count);
        }
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(2, &self.skin, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.opaque, 0..1);
        if self.opaque < self.count {
            pass.set_pipeline(&self.translucent);
            pass.draw(self.opaque..self.count, 0..1);
        }
    }
}

impl Renderer {
    /// Sets the entity geometry for the next view drawn (camera-relative to
    /// its `FrameParams::camera`).
    pub fn set_entities(&mut self, mesh: &EntityMesh) {
        self.held_models.clear();
        self.held_models.extend(mesh.held_items().iter().map(super::block_model::held_model));
        let e = &mut self.entities;
        let boxes = mesh.boxes();
        if boxes.len() > e.box_capacity {
            e.box_capacity = boxes.len().next_power_of_two();
            e.box_buffer = EntityPass::create_buffer(
                &self.device,
                "entity boxes",
                e.box_capacity * std::mem::size_of::<BoxInstance>(),
            );
        }
        if !boxes.is_empty() {
            self.queue.write_buffer(&e.box_buffer, 0, bytemuck::cast_slice(boxes));
        }
        e.box_count = boxes.len() as u32;
        // v0.18 Track 6: the mesh keeps its blended triangles (slime shells,
        // ground shadows) in their own list, so both go up as they are and
        // the translucent pipeline draws the second range.
        let (opaque, blended) = (&mesh[..], mesh.translucent());
        let total = opaque.len() + blended.len();
        if total > e.capacity {
            e.capacity = total.next_power_of_two();
            e.buffer = EntityPass::create_buffer(
                &self.device,
                "entity vertices",
                e.capacity * std::mem::size_of::<EntityVertex>(),
            );
        }
        if !opaque.is_empty() {
            self.queue.write_buffer(&e.buffer, 0, bytemuck::cast_slice(opaque));
        }
        if !blended.is_empty() {
            let offset = std::mem::size_of_val(opaque) as u64;
            self.queue.write_buffer(&e.buffer, offset, bytemuck::cast_slice(blended));
        }
        e.opaque = opaque.len() as u32;
        e.count = total as u32;
        // v0.15 Track D: the glowing entities, for the outline mask.
        let (glow_boxes, glow) = (mesh.glow_boxes(), mesh.glow_verts());
        if glow_boxes.len() > e.glow_box_capacity {
            e.glow_box_capacity = glow_boxes.len().next_power_of_two();
            e.glow_box_buffer = EntityPass::create_buffer(
                &self.device,
                "glow boxes",
                e.glow_box_capacity * std::mem::size_of::<BoxInstance>(),
            );
        }
        if !glow_boxes.is_empty() {
            self.queue.write_buffer(&e.glow_box_buffer, 0, bytemuck::cast_slice(glow_boxes));
        }
        e.glow_box_count = glow_boxes.len() as u32;
        if glow.len() > e.glow_capacity {
            e.glow_capacity = glow.len().next_power_of_two();
            e.glow_buffer = EntityPass::create_buffer(
                &self.device,
                "glow vertices",
                e.glow_capacity * std::mem::size_of::<EntityVertex>(),
            );
        }
        if !glow.is_empty() {
            self.queue.write_buffer(&e.glow_buffer, 0, bytemuck::cast_slice(glow));
        }
        e.glow_count = glow.len() as u32;
    }
}
