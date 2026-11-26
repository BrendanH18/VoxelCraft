//! Cloud pass (v0.18 Track 8): Java 26.3's flat and block-shaped clouds
//! ([`crate::atmosphere::clouds`]). The face list is rebuilt on the CPU only
//! when the camera moves into another 12-block cell, crosses the layer, or
//! the Clouds or Cloud Distance option changes, like `CloudRenderer`; each
//! frame just draws it, six vertices per face from one packed `u32`.
//!
//! v1.0 Track 2: one pass, as Java 26.3 draws clouds without its
//! order-independent transparency (`RenderPipelines.CLOUDS`: translucent
//! blending, `DepthStencilState.DEFAULT`'s greater-or-equal reverse-Z test
//! with depth writes, back faces culled; `FLAT_CLOUDS` culls nothing).
//! Faces come nearest ring first, so the nearest cloud surface writes its
//! depth before anything it hides is drawn, and the hidden faces fail the
//! depth test: the same nearest-surface result v0.18 got from a depth-only
//! pass followed by a colour pass, at half the vertex work. The buffer is
//! sized by Java's own bound for the radius (`getSizeForCloudDistance`).

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
    /// Fancy clouds' pipeline (back faces culled) and Fast clouds' (none).
    fancy_pipeline: wgpu::RenderPipeline,
    flat_pipeline: wgpu::RenderPipeline,
    mode: Clouds,
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
        let pipeline = |label: &str, cull_mode: Option<wgpu::Face>| {
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
                primitive: wgpu::PrimitiveState { cull_mode, ..Default::default() },
                // Reverse-Z: nearer is greater.
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
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
            })
        };
        let fancy_pipeline = pipeline("clouds", Some(wgpu::Face::Back));
        let flat_pipeline = pipeline("flat clouds", None);
        let capacity = clouds::max_faces(clouds::radius_cells(voxelcraft::atmosphere::DEFAULT_CLOUD_RANGE));
        Self {
            fancy_pipeline,
            flat_pipeline,
            mode: Clouds::Fancy,
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
            // Java's bound for the radius holds every face the build makes
            // (`build_never_exceeds_javas_bound`), so the buffer is sized
            // once per Cloud Distance, not grown by doubling.
            let bound = clouds::max_faces(radius);
            debug_assert!(self.faces.len() <= bound, "{} faces over the bound {bound}", self.faces.len());
            self.faces.truncate(bound);
            if bound != self.capacity {
                self.capacity = bound;
                self.buffer = Self::create_buffer(device, self.capacity);
            }
            if !self.faces.is_empty() {
                queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.faces));
            }
            self.count = self.faces.len() as u32;
            self.mode = p.mode;
        }
        [-view.in_cell.0, view.base, -view.in_cell.1]
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.set_pipeline(if self.mode == Clouds::Fast { &self.flat_pipeline } else { &self.fancy_pipeline });
        pass.draw(0..6, 0..self.count);
    }
}
