//! v0.15 Track D: the Glowing outline. [`super::entity::EntityPass`] draws
//! the glowing entities' geometry into a one-channel mask before the main
//! pass (only on frames with a glowing entity in view); this full-screen
//! pass then turns the pixels just outside that mask white, after the world
//! and without depth, so the outline shows through walls.

use super::DEPTH_FORMAT;

/// The mask's format: one 8-bit channel.
pub(super) const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

pub(super) struct Outline {
    pub(super) pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    /// The mask, its bind group and its size, made when first needed.
    mask: Option<(wgpu::TextureView, wgpu::BindGroup, (u32, u32))>,
}

impl Outline {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glow mask layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glow outline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glow outline shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/outline.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glow outline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_outline"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout, mask: None }
    }

    /// The mask target of `size` and the bind group that reads it.
    pub(super) fn mask(&mut self, device: &wgpu::Device, size: (u32, u32)) -> (&wgpu::TextureView, &wgpu::BindGroup) {
        if self.mask.as_ref().is_none_or(|m| m.2 != size) {
            let view = device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("glow mask"),
                    size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: MASK_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glow mask"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
            });
            self.mask = Some((view, bind, size));
        }
        let m = self.mask.as_ref().unwrap();
        (&m.0, &m.1)
    }

    /// The bind group of the mask made by [`Outline::mask`].
    pub(super) fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.mask.as_ref().map(|m| &m.1)
    }
}
