//! v0.18 Track 2: the menu background blur. While a menu that blurs is
//! open (Menu Background Blur above 0), the world and HUD are drawn into
//! [`Blur::scene`] instead of the frame; six full-screen passes (Java's
//! `blur` post effect: horizontal then vertical box blurs, three times)
//! blur it back onto the frame, and the menu is drawn over that. The two
//! targets are made the first time a menu blurs and kept at the window's
//! size; frames without a blurring menu don't touch them.

/// Java's `post_effect/blur.json`: three horizontal and vertical pairs.
pub(super) const PASSES: usize = 6;

/// The `Blur` uniform: direction, radius and the view's rectangle.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    dir: [f32; 2],
    radius: f32,
    pad: f32,
    rect: [f32; 4],
}

/// Uniform slots are this far apart (wgpu's default dynamic offset alignment).
const STRIDE: u64 = 256;

struct Targets {
    size: (u32, u32),
    /// What the world and HUD are drawn into, and the blur's ping-pong
    /// partner.
    scene: wgpu::TextureView,
    swap: wgpu::TextureView,
    /// Read `scene` horizontally, and `swap` vertically.
    read_scene: wgpu::BindGroup,
    read_swap: wgpu::BindGroup,
}

pub(super) struct Blur {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

impl Blur {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
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
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blur.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_blur"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur uniforms"),
            size: STRIDE * 2,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, layout, sampler, uniforms, format, targets: None }
    }

    /// Makes (or remakes, for a new `size`) the targets and sets the
    /// radius and the view's rectangle for this view.
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: (u32, u32),
        radius: u32,
        rect: [f32; 4],
    ) {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(self.make_targets(device, size));
        }
        for (i, dir) in [[1.0, 0.0], [0.0, 1.0]].into_iter().enumerate() {
            let params = Params { dir, radius: radius as f32, pad: 0.0, rect };
            queue.write_buffer(&self.uniforms, i as u64 * STRIDE, bytemuck::bytes_of(&params));
        }
    }

    /// The target the world and HUD are drawn into (after [`Blur::prepare`]).
    pub(super) fn scene(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|t| &t.scene)
    }

    fn make_targets(&self, device: &wgpu::Device, (w, h): (u32, u32)) -> Targets {
        let target = |label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let (scene, swap) = (target("blur scene"), target("blur swap"));
        let read = |view: &wgpu::TextureView, slot: u64| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur read"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.uniforms,
                            offset: slot * STRIDE,
                            size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
                        }),
                    },
                ],
            })
        };
        let (read_scene, read_swap) = (read(&scene, 0), read(&swap, 1));
        Targets { size: (w, h), scene, swap, read_scene, read_swap }
    }

    /// Blurs the scene drawn since [`Blur::begin`] onto `out`, within the
    /// viewport `vp` (x, y, width, height in pixels).
    pub(super) fn finish(&self, encoder: &mut wgpu::CommandEncoder, out: &wgpu::TextureView, vp: [u32; 4]) {
        let Some(t) = &self.targets else { return };
        for pass in 0..PASSES {
            // Even passes read the scene horizontally into the swap target;
            // odd ones read it back vertically, the last straight onto the frame.
            let (read, write) = if pass % 2 == 0 {
                (&t.read_scene, &t.swap)
            } else if pass + 1 == PASSES {
                (&t.read_swap, out)
            } else {
                (&t.read_swap, &t.scene)
            };
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: write,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let [x, y, w, h] = vp;
            rp.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            rp.set_scissor_rect(x, y, w, h);
            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, read, &[]);
            rp.draw(0..3, 0..1);
        }
    }
}
