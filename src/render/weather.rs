//! Weather pass: rain and snow drawn as camera-facing sheets on the
//! columns around the player, with the drops and flakes themselves
//! generated in the fragment shader. Alpha-blended and drawn after water.

use bytemuck::{Pod, Zeroable};

use super::{DEPTH_FORMAT, Renderer};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct WeatherVertex {
    /// Camera-relative position.
    pub pos: [f32; 3],
    /// x: 0..1 across the sheet, y: world height (scrolls the drops).
    pub uv: [f32; 2],
    /// Per-column random offset, so neighbouring sheets don't match.
    pub seed: f32,
    /// x: 255 for snow, y: sky light, z: opacity.
    pub params: [u8; 4],
}

pub(super) struct WeatherPass {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: usize,
    count: u32,
}

impl WeatherPass {
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weather shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/weather.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("weather"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<WeatherVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, 1 => Float32x2, 2 => Float32, 3 => Unorm8x4
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 4096;
        Self { pipeline, buffer: Self::create_buffer(device, capacity), capacity, count: 0 }
    }

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather vertices"),
            size: (capacity * std::mem::size_of::<WeatherVertex>()) as u64,
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
    /// Sets the rain and snow sheets for the next frame (camera-relative to
    /// the next `FrameParams::camera`).
    pub fn set_weather(&mut self, verts: &[WeatherVertex]) {
        let w = &mut self.weather;
        if verts.len() > w.capacity {
            w.capacity = verts.len().next_power_of_two();
            w.buffer = WeatherPass::create_buffer(&self.device, w.capacity);
        }
        if !verts.is_empty() {
            self.queue.write_buffer(&w.buffer, 0, bytemuck::cast_slice(verts));
        }
        w.count = verts.len() as u32;
    }
}
