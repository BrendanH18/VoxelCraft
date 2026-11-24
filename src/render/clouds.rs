//! Cloud pass (v0.18 Track 8): Java 26.3's flat and block-shaped clouds
//! ([`crate::atmosphere::clouds`]). The face list is rebuilt on the CPU only
//! when the camera moves into another 12-block cell, crosses the layer, or
//! the Clouds or Cloud Distance option changes, like `CloudRenderer`; each
//! frame just draws it, six vertices per face from one packed `u32`.
//!
//! Java draws clouds with order-independent transparency. Here a depth-only
//! pass lays down the nearest cloud surface and the colour pass blends only
//! that surface, so the faces behind it never show through (Java's pre-OIT
//! method).

use voxelcraft::atmosphere::{
    Clouds,
    clouds::{self, Pattern, Relative, View},
};

use super::DEPTH_FORMAT;

/// What the frame wants drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudParams {
    pub mode: Clouds,
    /// Cloud Distance in Java chunks.
    pub range: u32,
    /// Game ticks for the drift (with the partial tick).
    pub ticks: f64,
    pub height: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    cell: (i32, i32),
    relative: Relative,
    mode: Clouds,
    radius: i32,
}

pub(super) struct CloudPass {
    depth_pipeline: wgpu::RenderPipeline,
    color_pipeline: wgpu::RenderPipeline,
    pattern: Pattern,
    faces: Vec<u32>,
    key: Option<Key>,
    buffer: wgpu::Buffer,
    capacity: usize,
    count: u32,
}

impl CloudPass {
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cloud shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/clouds.wgsl").into()),
        });
        let pipeline = |label: &str, depth_write: bool, compare, color: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![0 => Uint32],
                    })],
                },
                primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(compare),
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
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: if color { wgpu::ColorWrites::ALL } else { wgpu::ColorWrites::empty() },
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // Reverse-Z: nearer is greater.
        let depth_pipeline = pipeline("cloud depth", true, wgpu::CompareFunction::Greater, false);
        let color_pipeline = pipeline("clouds", false, wgpu::CompareFunction::Equal, true);
        let capacity = 4096;
        Self {
            depth_pipeline,
            color_pipeline,
            pattern: Pattern::generate(),
            faces: Vec::new(),
            key: None,
            buffer: Self::create_buffer(device, capacity),
            capacity,
            count: 0,
        }
    }

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cloud faces"),
            size: (capacity * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Places the layer for this frame's camera and rebuilds the faces if
    /// the camera's cell, side of the layer or the options changed. Returns
    /// the shader's cloud offset: `(-x in cell, base y, -z in cell)`.
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: glam::DVec3,
        p: Option<CloudParams>,
    ) -> [f32; 3] {
        let Some(p) = p.filter(|p| p.mode != Clouds::Off) else {
            self.count = 0;
            self.key = None;
            return [0.0; 3];
        };
        let view = View::new(camera.x, camera.y, camera.z, p.ticks, p.height);
        let radius = clouds::radius_cells(p.range);
        let key = Key { cell: view.cell, relative: view.relative, mode: p.mode, radius };
        if self.key != Some(key) {
            self.key = Some(key);
            clouds::build(&self.pattern, &view, p.mode, radius, &mut self.faces);
            if self.faces.len() > self.capacity {
                self.capacity = self.faces.len().next_power_of_two();
                self.buffer = Self::create_buffer(device, self.capacity);
            }
            if !self.faces.is_empty() {
                queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.faces));
            }
            self.count = self.faces.len() as u32;
        }
        [-view.in_cell.0, view.base, -view.in_cell.1]
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.set_pipeline(&self.depth_pipeline);
        pass.draw(0..6, 0..self.count);
        pass.set_pipeline(&self.color_pipeline);
        pass.draw(0..6, 0..self.count);
    }
}
