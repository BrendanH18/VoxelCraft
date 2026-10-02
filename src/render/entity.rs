//! Entity pass: all mobs in one draw from a per-frame vertex buffer of
//! camera-relative box-model triangles (built by `crate::entity::model`).

use super::{DEPTH_FORMAT, Renderer};

pub use crate::entity::model::EntityVertex;

pub(super) struct EntityPass {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: usize,
    count: u32,
}

impl EntityPass {
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("entity shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/entity.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("entities"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<EntityVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Unorm8x4
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
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
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 4096;
        Self { pipeline, buffer: Self::create_buffer(device, capacity), capacity, count: 0 }
    }

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("entity vertices"),
            size: (capacity * std::mem::size_of::<EntityVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

impl Renderer {
    /// Sets the entity geometry for the next frame (camera-relative to the
    /// next `FrameParams::camera`).
    pub fn set_entities(&mut self, verts: &[EntityVertex]) {
        let e = &mut self.entities;
        if verts.len() > e.capacity {
            e.capacity = verts.len().next_power_of_two();
            e.buffer = EntityPass::create_buffer(&self.device, e.capacity);
        }
        if !verts.is_empty() {
            self.queue.write_buffer(&e.buffer, 0, bytemuck::cast_slice(verts));
        }
        e.count = verts.len() as u32;
    }
}
