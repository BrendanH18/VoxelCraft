//! v0.18 Track 7: the title screen's panorama, six pre-rendered faces of
//! VoxelCraft terrain (`assets/panorama/`, made by the game itself with
//! `--panorama`; see `docs/development.md#the-title-panorama`) drawn from
//! the cube's centre with Java's field of view, tilt and spin
//! (`voxelcraft::title::panorama`).
//!
//! The faces are decoded and their mip levels built off the main thread
//! ([`decode`]); the renderer keeps the texture once uploaded, as Java keeps
//! its cube map, so returning to the title doesn't decode them again.

use glam::Vec3;
use voxelcraft::title::panorama;

use super::DEPTH_FORMAT;

/// The faces' asset keys, `panorama_0` .. `panorama_5` (north, east, south,
/// west, up, down).
pub const FACE_KEYS: [&str; 6] = [
    "voxelcraft:panorama_0",
    "voxelcraft:panorama_1",
    "voxelcraft:panorama_2",
    "voxelcraft:panorama_3",
    "voxelcraft:panorama_4",
    "voxelcraft:panorama_5",
];

/// Decoded faces: every mip level, each holding the six faces back to back.
pub struct Faces {
    pub size: u32,
    pub levels: Vec<Vec<u8>>,
}

impl Faces {
    /// Bytes the texture takes on the GPU (every level, RGBA8).
    pub fn gpu_bytes(&self) -> u64 {
        self.levels.iter().map(|l| l.len() as u64).sum()
    }
}

/// Decodes the embedded faces and builds their mip chains. Fails if a face
/// is missing, isn't square or differs in size from the first.
pub fn decode() -> Result<Faces, String> {
    let mut size = 0;
    let mut base = Vec::new();
    for key in FACE_KEYS {
        let bytes = voxelcraft::assets::resource(key).ok_or_else(|| format!("{key}: missing resource"))?;
        let (w, h, rgba) = decode_png(bytes).map_err(|e| format!("{key}: {e}"))?;
        if w != h || (size != 0 && w != size) || !w.is_power_of_two() {
            return Err(format!("{key}: faces must be equal power-of-two squares, got {w} x {h}"));
        }
        size = w;
        base.extend_from_slice(&rgba);
    }
    let mut levels = vec![base];
    let mut s = size;
    while s > 1 {
        let next = half(levels.last().unwrap(), s);
        s /= 2;
        levels.push(next);
    }
    Ok(Faces { size, levels })
}

/// An RGB or RGBA 8-bit PNG as RGBA.
fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let pixels = &buf[..info.buffer_size()];
    let rgba = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()
        }
        (png::ColorType::Rgba, png::BitDepth::Eight) => pixels.to_vec(),
        other => return Err(format!("expected 8-bit RGB or RGBA, got {other:?}")),
    };
    Ok((info.width, info.height, rgba))
}

/// The next mip level of six `size`-square faces: each texel the mean of
/// four (Java's `MipmapStrategy.MEAN`), averaged in linear light.
fn half(level: &[u8], size: u32) -> Vec<u8> {
    let s = size as usize;
    let h = s / 2;
    let mut out = vec![0u8; h * h * 4 * 6];
    let lin = |c: u8| SRGB_TO_LINEAR[c as usize];
    for face in 0..6 {
        let src = &level[face * s * s * 4..];
        let dst = &mut out[face * h * h * 4..];
        for y in 0..h {
            for x in 0..h {
                let at = |dx: usize, dy: usize| ((2 * y + dy) * s + 2 * x + dx) * 4;
                let o = (y * h + x) * 4;
                for c in 0..4 {
                    let sum: f32 = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)]
                        .into_iter()
                        .map(|i| if c == 3 { src[i + c] as f32 / 255.0 } else { lin(src[i + c]) })
                        .sum();
                    let v = sum / 4.0;
                    dst[o + c] = if c == 3 { (v * 255.0).round() as u8 } else { linear_to_srgb(v) };
                }
            }
        }
    }
    out
}

static SRGB_TO_LINEAR: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|i| {
        let c = i as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    })
});

/// Linear light to an sRGB code value, through a 4096-step table.
fn linear_to_srgb(v: f32) -> u8 {
    static TABLE: std::sync::LazyLock<[u8; 4096]> = std::sync::LazyLock::new(|| {
        std::array::from_fn(|i| {
            let v = i as f32 / 4095.0;
            let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            (s.clamp(0.0, 1.0) * 255.0).round() as u8
        })
    });
    TABLE[(v.clamp(0.0, 1.0) * 4095.0).round() as usize]
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    inv_view_proj: [[f32; 4]; 4],
    params: [f32; 4],
}

/// How the panorama is drawn this frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Degrees turned (`Panorama.spin`).
    pub spin: f32,
    /// Blur radius in screen pixels: 0 on the title menu, Java's Menu
    /// Background Blur on the screens over it.
    pub blur: u32,
}

pub(super) struct PanoramaPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// The faces' bind group and size, once uploaded.
    bound: Option<(wgpu::BindGroup, u32)>,
    /// GPU bytes of the uploaded faces.
    pub(super) bytes: u64,
}

impl PanoramaPass {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("panorama layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panorama pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panorama shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/panorama.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("panorama"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_panorama"),
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
                entry_point: Some("fs_panorama"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("panorama uniform"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Java's panorama texture is blurred (linear) and clamped.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("panorama sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self { pipeline, layout, uniform, sampler, bound: None, bytes: 0 }
    }

    pub(super) fn loaded(&self) -> bool {
        self.bound.is_some()
    }

    pub(super) fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, faces: &Faces) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("panorama"),
            size: wgpu::Extent3d { width: faces.size, height: faces.size, depth_or_array_layers: 6 },
            mip_level_count: faces.levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, data) in faces.levels.iter().enumerate() {
            let s = (faces.size >> level).max(1);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s * 4), rows_per_image: Some(s) },
                wgpu::Extent3d { width: s, height: s, depth_or_array_layers: 6 },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("panorama"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.bytes = faces.gpu_bytes();
        self.bound = Some((bind, faces.size));
    }

    /// Writes this frame's camera; false if no faces are loaded.
    pub(super) fn prepare(&self, queue: &wgpu::Queue, view: View, (width, height): (u32, u32)) -> bool {
        let Some((_, size)) = &self.bound else { return false };
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let proj = glam::camera::rh::proj::directx::perspective_infinite_reverse(
            panorama::FOV_DEGREES.to_radians(),
            aspect,
            0.05,
        );
        let (forward, up) = panorama::view(view.spin);
        let look = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, forward, up);
        // A fixed level: texels per screen pixel at the centre, or the blur's.
        let focal = height as f32 * 0.5 / (panorama::FOV_DEGREES.to_radians() * 0.5).tan();
        let sharp = (*size as f32 * 0.5 / focal).max(1.0).log2();
        let lod = sharp.max(panorama::blur_lod(view.blur, height as f32, *size));
        let uniform =
            Uniform { inv_view_proj: (proj * look).inverse().to_cols_array_2d(), params: [lod, 1.0, 0.0, 0.0] };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        true
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some((bind, _)) = &self.bound else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_faces_decode_to_equal_squares_with_full_mip_chains() {
        let started = std::time::Instant::now();
        let faces = decode().unwrap();
        let took = started.elapsed();
        assert_eq!(faces.size, panorama::FACE_SIZE);
        assert_eq!(faces.levels.len() as u32, faces.size.ilog2() + 1);
        for (level, data) in faces.levels.iter().enumerate() {
            let s = (faces.size >> level) as usize;
            assert_eq!(data.len(), s * s * 4 * 6, "level {level}");
        }
        // Opaque, and not one flat colour: real terrain.
        let base = &faces.levels[0];
        assert!(base.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        let first = &base[..4];
        assert!(base.as_chunks::<4>().0.iter().any(|p| p[..] != *first));
        println!(
            "panorama: {} x {} faces, {} GPU bytes, decoded in {took:?}",
            faces.size,
            faces.size,
            faces.gpu_bytes()
        );
    }

    #[test]
    fn mips_average_in_linear_light() {
        // A 2 x 2 face of black and white halves the light, not the code value.
        let face = [0u8, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
        let level: Vec<u8> = face.iter().copied().cycle().take(16 * 6).collect();
        let next = half(&level, 2);
        assert_eq!(next.len(), 4 * 6);
        assert_eq!(&next[..4], &[188, 188, 188, 255]);
    }
}
