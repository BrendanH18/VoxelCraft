//! wgpu renderer.
//!
//! Chunk meshes live in one vertex buffer per chunk (opaque, cutout and
//! translucent quads back to back) and all share a single quad index buffer.
//! Each frame the visible chunks are frustum culled and sorted, and their
//! camera-relative origins are written into one instance buffer; every
//! chunk draw then selects its origin with `first_instance`. Positions stay
//! camera-relative end to end, so precision holds far from the origin.
//!
//! Depth is reverse-Z with an infinite far plane.

pub mod arena;
pub mod block_model;
pub mod entity;
pub mod hand;
mod item_sprites;
pub mod textures;
pub mod ui;
pub mod weather;

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, IVec3, Mat4, Vec3, Vec4};
use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::mesh::{CROSS, CUTOUT, FACE_ORDER, MeshData, OPAQUE, PASSES, TRANSLUCENT};
use crate::world::block::tex;
use crate::world::chunk::{CHUNK_SIZE, CHUNK_SIZE_I};
pub use block_model::BlockModel;
use ui::UiVertex;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Worst case (3D checkerboard): half the blocks visible on all six sides.
const MAX_QUADS_PER_CHUNK: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE / 2 * 6;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    fog_color: [f32; 4],
    zenith_color: [f32; 4],
    /// xyz: direction to the sun, w: time in seconds.
    sun: [f32; 4],
    /// x: fog start, y: fog end, z: daylight.
    params: [f32; 4],
    /// xy: wrapped cloud pattern origin, z: cloud plane y relative to the
    /// camera, w: cloud radius.
    clouds: [f32; 4],
    /// x: dimension (0 Overworld, 1 Nether, 2 End); y: enhanced graphics; zw: camera xz wrapped at 128 blocks.
    environment: [f32; 4],
}

const CLOUD_HEIGHT: f64 = 192.0;
/// Cloud pattern wraps after this many blocks (keeps shader math precise).
const CLOUD_PERIOD: f64 = 12.0 * 5.0 * 1024.0;
const CLOUD_SPEED: f64 = 1.2;

struct ChunkMesh {
    /// Where the quads live in the arena.
    alloc: arena::Allocation,
    quads: u32,
    /// First quad of each (pass, face) group, `pass * 6 + face`; the last
    /// entry is the total.
    offsets: [u32; PASSES * 6 + 1],
}

/// Face directions (bit per face, +X -X +Y -Y +Z -Z) that can face a
/// camera at `origin` = chunk min corner minus camera position. Positive
/// faces lie on planes 1..=32 of the chunk (water tops dip up to 2 blocks
/// lower), negative ones on 0..=31; a face is back-facing when the camera
/// is behind its plane.
fn facing_faces(origin: Vec3) -> u8 {
    let c = -origin;
    let s = CHUNK_SIZE as f32;
    let faces = [c.x > 0.0, c.x < s, c.y > -2.0, c.y < s, c.z > 0.0, c.z < s];
    faces.iter().enumerate().fold(0, |m, (face, &f)| m | (f as u8) << face)
}

/// Maps a face bitmask to a pass's group bitmask (see [`FACE_ORDER`]).
fn group_mask(faces: u8, pass: usize) -> u8 {
    FACE_ORDER[pass].iter().enumerate().fold(0, |m, (group, &face)| m | (faces >> face & 1) << group)
}

/// Everything the renderer needs to know about the current frame.
pub struct FrameParams {
    pub camera: DVec3,
    pub forward: Vec3,
    pub fov_y: f32,
    pub sky_color: [f64; 3],
    pub fog_color: [f32; 3],
    pub fog_start: f32,
    pub fog_end: f32,
    /// Skylight multiplier, 1 at noon.
    pub daylight: f32,
    pub zenith_color: [f32; 3],
    pub sun_dir: Vec3,
    pub dimension: crate::world::terrain::Dimension,
    pub enhanced_graphics: bool,
    /// Seconds since start (animations).
    pub time: f32,
    /// Targeted block and its outline's corners within the cell (beds,
    /// slabs and shaped blocks are smaller).
    pub highlight: Option<(IVec3, [f32; 3], [f32; 3])>,
    /// Block being broken and the crack texture layer to overlay on it.
    pub crack: Option<(IVec3, u8)>,
    /// Free-standing blocks (falling sand and gravel).
    pub block_models: Vec<BlockModel>,
    /// The first-person hand (`None` in third person or with the HUD hidden).
    pub hand: Option<hand::Hand>,
    /// HUD geometry, drawn last.
    pub ui: Vec<UiVertex>,
    /// Rain strength 0..1: hides the sun, moon and stars and thickens the
    /// clouds.
    pub rain: f32,
}

#[derive(Default, Clone, Copy)]
pub struct RenderStats {
    pub meshes: usize,
    pub visible: usize,
    pub draw_calls: usize,
    pub quads: u64,
    /// GPU memory reserved for chunk quads, and the part in use.
    pub gpu_bytes: u64,
    pub gpu_used_bytes: u64,
    /// Time spent blocked acquiring the swapchain image (vsync wait).
    pub acquire_ms: f64,
}

/// Plane-based view frustum (left, right, bottom, top); the far plane is
/// infinite and near-plane culling isn't worth it for chunks.
struct Frustum {
    planes: [Vec4; 4],
}

impl Frustum {
    fn new(m: Mat4) -> Self {
        let (r0, r1, r3) = (m.row(0), m.row(1), m.row(3));
        let n = |p: Vec4| p / p.truncate().length();
        Self { planes: [n(r3 + r0), n(r3 - r0), n(r3 + r1), n(r3 - r1)] }
    }

    fn intersects(&self, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|p| {
            let v = Vec3::new(
                if p.x > 0.0 { max.x } else { min.x },
                if p.y > 0.0 { max.y } else { min.y },
                if p.z > 0.0 { max.z } else { min.z },
            );
            p.truncate().dot(v) + p.w >= 0.0
        })
    }
}

/// Calls `f(first_quad, quads)` for each contiguous run of face groups in
/// `mask`, given one pass's 7 group offsets. Empty groups never split a run.
fn face_runs(offsets: &[u32], mask: u8, mut f: impl FnMut(u32, u32)) {
    let mut mask = mask;
    for g in 0..6 {
        if offsets[g] == offsets[g + 1] {
            mask |= 1 << g;
        }
    }
    let mut g = 0;
    while g < 6 {
        if mask & 1 << g == 0 {
            g += 1;
            continue;
        }
        let first = g;
        while g < 6 && mask & 1 << g != 0 {
            g += 1;
        }
        let quads = offsets[g] - offsets[first];
        if quads > 0 {
            f(offsets[first], quads);
        }
    }
}

/// A window image being drawn this frame (or the offscreen target).
pub struct Frame {
    surface: Option<wgpu::SurfaceTexture>,
    view: wgpu::TextureView,
    /// A view has been drawn, so later ones must not clear the image.
    drawn: bool,
}

/// A rectangle of the window in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    pub fn full((width, height): (u32, u32)) -> Self {
        Self { x: 0, y: 0, width: width.max(1), height: height.max(1) }
    }

    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }

    /// Splits a window into `count` (1..=4) player views like split-screen
    /// Minecraft: two players get halves (top/bottom, or left/right when
    /// `side_by_side`), three or four get quarters, with the third view of
    /// three spanning the bottom.
    pub fn split((width, height): (u32, u32), count: usize, side_by_side: bool) -> Vec<Self> {
        let (w, h) = (width.max(2), height.max(2));
        let (hw, hh) = (w / 2, h / 2);
        let rect = |x, y, width, height| Self { x, y, width, height };
        match count {
            0 | 1 => vec![Self::full((w, h))],
            2 if side_by_side => vec![rect(0, 0, hw, h), rect(hw, 0, w - hw, h)],
            2 => vec![rect(0, 0, w, hh), rect(0, hh, w, h - hh)],
            3 => vec![rect(0, 0, hw, hh), rect(hw, 0, w - hw, hh), rect(0, hh, w, h - hh)],
            _ => {
                vec![rect(0, 0, hw, hh), rect(hw, 0, w - hw, hh), rect(0, hh, hw, h - hh), rect(hw, hh, w - hw, h - hh)]
            }
        }
    }
}

pub struct Renderer {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    blocks_bg: wgpu::BindGroup,
    font_bg: wgpu::BindGroup,
    opaque_pipeline: wgpu::RenderPipeline,
    cutout_pipeline: wgpu::RenderPipeline,
    cross_pipeline: wgpu::RenderPipeline,
    translucent_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    decal_pipeline: wgpu::RenderPipeline,
    decal_buf: wgpu::Buffer,
    sky_pipeline: wgpu::RenderPipeline,
    cloud_pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    entities: entity::EntityPass,
    block_models: block_model::BlockModelPass,
    weather: weather::WeatherPass,
    quad_indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    line_buf: wgpu::Buffer,
    ui_buf: wgpu::Buffer,
    ui_capacity: usize,
    meshes: FxHashMap<IVec3, ChunkMesh>,
    arena: arena::QuadArena,
    vsync: bool,
    pub stats: RenderStats,
    pub gpu_name: String,
    /// (distance², chunk, camera-relative origin, faces that can face the camera).
    visible: Vec<(f32, IVec3, [f32; 3], u8)>,
    capture: Option<std::path::PathBuf>,
    offscreen: Option<wgpu::Texture>,
    /// Render to `offscreen` instead of the window (benchmarks).
    pub force_offscreen: bool,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, vsync: bool) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("no suitable GPU adapter");
        let info = adapter.get_info();
        log::info!("GPU: {} ({:?})", info.name, info.backend);
        let gpu_name = format!("{} ({:?})", info.name, info.backend);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .expect("request device");
        // Chunk quads are read from storage buffers in the vertex shader.
        if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
            panic!("GPU/backend doesn't support storage buffers in vertex shaders (required for chunk rendering)");
        }
        let arena = arena::QuadArena::new(&device, &device.limits());

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        // COPY_SRC lets `--screenshot` read the frame back.
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | (caps.usages & wgpu::TextureUsages::COPY_SRC);
        let config = wgpu::SurfaceConfiguration {
            usage,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: Self::present_mode(vsync),
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: Default::default(),
        };
        surface.configure(&device, &config);
        let depth = Self::create_depth(&device, &config);

        // --- Bind groups -------------------------------------------------
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }],
        });

        let blocks_view =
            Self::create_texture_array(&device, &queue, "block textures", tex::COUNT, textures::generate_mips());
        let items_view = Self::create_texture_array(
            &device,
            &queue,
            "item icons",
            crate::item::icon_count(),
            textures::generate_item_mips(),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blocks sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let blocks_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blocks layout"),
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
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let blocks_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blocks"),
            layout: &blocks_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&blocks_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&items_view) },
            ],
        });

        let font_view = Self::create_font_texture(&device, &queue);
        let font_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("font layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let font_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("font"),
            layout: &font_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&font_view) }],
        });
        let ui_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&blocks_layout), Some(&font_layout)],
            immediate_size: 0,
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&blocks_layout)],
            immediate_size: 0,
        });
        let chunk_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("chunk pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&blocks_layout), Some(arena.layout())],
            immediate_size: 0,
        });

        // --- Pipelines ---------------------------------------------------
        let chunk_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chunk shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/chunk.wgsl").into()),
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/overlay.wgsl").into()),
        });

        // Quads come from the arena's storage buffers (vertex pulling); the
        // only vertex input is the per-draw chunk origin.
        let chunk_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![1 => Float32x3],
        })];
        let make_chunk_pipeline = |label: &str,
                                   fs: &str,
                                   blend: Option<wgpu::BlendState>,
                                   depth_write: bool,
                                   cull_mode: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&chunk_layout),
                vertex: wgpu::VertexState {
                    module: &chunk_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &chunk_buffers,
                },
                primitive: wgpu::PrimitiveState { cull_mode, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &chunk_shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let back = Some(wgpu::Face::Back);
        let opaque_pipeline = make_chunk_pipeline("opaque", "fs_opaque", None, true, back);
        let cutout_pipeline = make_chunk_pipeline("cutout", "fs_cutout", None, true, back);
        // Cross planes are seen from both sides.
        let cross_pipeline = make_chunk_pipeline("cross", "fs_cutout", None, true, None);
        let translucent_pipeline =
            make_chunk_pipeline("translucent", "fs_translucent", Some(wgpu::BlendState::ALPHA_BLENDING), false, back);

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("outline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_line"),
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

        // Crack overlay: multiplies the block's colour by 2x the crack
        // texture, so mid-grey texels leave it unchanged.
        let decal_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crack decal"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_decal"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Uint32],
                })],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_decal"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Dst,
                            dst_factor: wgpu::BlendFactor::Src,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let decal_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crack decal"),
            size: 36 * 24,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sky.wgsl").into()),
        });
        let make_sky_pipeline = |label: &str, vs: &str, fs: &str, compare, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &sky_shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &sky_shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // The sky only fills pixels still at the cleared far depth.
        let sky_pipeline = make_sky_pipeline("sky", "vs_sky", "fs_sky", wgpu::CompareFunction::GreaterEqual, None);
        let cloud_pipeline = make_sky_pipeline(
            "clouds",
            "vs_clouds",
            "fs_clouds",
            wgpu::CompareFunction::Greater,
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );

        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud"),
            layout: Some(&ui_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_ui"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32x4],
                })],
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
                module: &overlay_shader,
                entry_point: Some("fs_ui"),
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

        let entities = entity::EntityPass::new(&device, &layout, format);
        let block_models = block_model::BlockModelPass::new(&device, &layout, format);
        let weather = weather::WeatherPass::new(&device, &layout, format);

        // --- Shared buffers ----------------------------------------------
        let indices: Vec<u32> =
            (0..MAX_QUADS_PER_CHUNK as u32).flat_map(|q| [0, 1, 2, 2, 3, 0].map(|i| q * 4 + i)).collect();
        let quad_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_capacity = 1024;
        let instances = Self::create_instance_buffer(&device, instance_capacity);
        let line_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline"),
            size: 24 * 12,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui_capacity = 256;
        let ui_buf = Self::create_ui_buffer(&device, ui_capacity);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            depth,
            globals_buf,
            globals_bg,
            blocks_bg,
            font_bg,
            opaque_pipeline,
            cutout_pipeline,
            cross_pipeline,
            translucent_pipeline,
            line_pipeline,
            decal_pipeline,
            decal_buf,
            sky_pipeline,
            cloud_pipeline,
            ui_pipeline,
            entities,
            block_models,
            weather,
            quad_indices,
            instances,
            instance_capacity,
            line_buf,
            ui_buf,
            ui_capacity,
            meshes: FxHashMap::default(),
            arena,
            vsync,
            stats: RenderStats::default(),
            gpu_name,
            visible: Vec::new(),
            capture: None,
            offscreen: None,
            force_offscreen: false,
        }
    }

    /// Blocks until the GPU has finished all submitted work.
    pub fn wait_idle(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    pub fn capture_pending(&self) -> bool {
        self.capture.is_some()
    }

    /// Saves the next rendered frame as a PNG.
    pub fn request_capture(&mut self, path: impl Into<std::path::PathBuf>) {
        self.capture = Some(path.into());
    }

    fn save_capture(&self, texture: &wgpu::Texture, encoder: wgpu::CommandEncoder, path: &std::path::Path) {
        let (w, h) = (self.config.width, self.config.height);
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = encoder;
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map capture buffer"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll device");

        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mapped = buffer.slice(..).get_mapped_range().expect("read capture buffer");
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let line = &mapped[(y * row) as usize..(y * row + w * 4) as usize];
            for px in line.as_chunks::<4>().0 {
                pixels.extend_from_slice(&if bgra { [px[2], px[1], px[0], 255] } else { [px[0], px[1], px[2], 255] });
            }
        }
        let result = std::fs::File::create(path).map_err(|e| e.to_string()).and_then(|f| {
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().and_then(|mut wr| wr.write_image_data(&pixels)).map_err(|e| e.to_string())
        });
        match result {
            Ok(()) => log::info!("saved screenshot {}", path.display()),
            Err(e) => log::error!("screenshot failed: {e}"),
        }
    }

    fn present_mode(vsync: bool) -> wgpu::PresentMode {
        if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync }
    }

    fn create_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk instances"),
            size: (capacity * 12) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_ui_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hud"),
            size: (capacity * std::mem::size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// A 16x16 texture array of `layers` layers with the given mip levels.
    fn create_texture_array(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        layers: u32,
        mips: Vec<Vec<u8>>,
    ) -> wgpu::TextureView {
        let size = textures::SIZE as u32;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers },
            mip_level_count: textures::MIP_LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, data) in mips.iter().enumerate() {
            let s = size >> level;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s * 4), rows_per_image: Some(s) },
                wgpu::Extent3d { width: s, height: s, depth_or_array_layers: layers },
            );
        }
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
    }

    fn create_font_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
        let size = wgpu::Extent3d { width: ui::FONT_ATLAS_W, height: ui::FONT_ATLAS_H, depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("font"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &ui::font_atlas(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(ui::FONT_ATLAS_W), rows_per_image: None },
            size,
        );
        texture.create_view(&Default::default())
    }

    pub fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = Self::create_depth(&self.device, &self.config);
    }

    pub fn vsync(&self) -> bool {
        self.vsync
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        self.vsync = vsync;
        self.config.present_mode = Self::present_mode(vsync);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn upload_mesh(&mut self, pos: IVec3, mesh: MeshData) {
        self.remove_mesh(pos);
        if mesh.is_empty() {
            return;
        }
        let alloc = self.arena.alloc(&self.device, &self.queue, &mesh);
        let mut offsets = [0; PASSES * 6 + 1];
        for (i, n) in mesh.face_quads.as_flattened().iter().enumerate() {
            offsets[i + 1] = offsets[i] + n;
        }
        self.meshes.insert(pos, ChunkMesh { alloc, quads: mesh.quads.len() as u32, offsets });
    }

    /// Forgets every chunk mesh, entity and weather sheet (leaving a world).
    pub fn clear_world(&mut self) {
        let all: Vec<IVec3> = self.meshes.keys().copied().collect();
        for pos in all {
            self.remove_mesh(pos);
        }
        self.set_entities(&[]);
        self.set_weather(&[]);
    }

    pub fn remove_mesh(&mut self, pos: IVec3) {
        if let Some(m) = self.meshes.remove(&pos) {
            self.arena.free(m.alloc, m.quads);
        }
    }

    /// Cube around a block, 6 faces x 2 triangles, as (pos, uv, layer).
    fn decal_vertices(block: IVec3, camera: DVec3, layer: u8) -> Vec<u8> {
        let e = 0.003;
        let min = (block.as_dvec3() - camera - DVec3::splat(e)).as_vec3();
        let s = 1.0 + 2.0 * e as f32;
        let mut out = Vec::with_capacity(36 * 24);
        for axis in 0..3 {
            for side in [0.0, 1.0] {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let corner = |a: f32, b: f32| {
                    let mut p = [0.0f32; 3];
                    p[axis] = side * s;
                    p[u] = a * s;
                    p[v] = b * s;
                    (min + Vec3::from_array(p), [a, b])
                };
                for (a, b) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    let (p, uv) = corner(a, b);
                    out.extend_from_slice(bytemuck::cast_slice(&p.to_array()));
                    out.extend_from_slice(bytemuck::cast_slice(&uv));
                    out.extend_from_slice(&(layer as u32).to_le_bytes());
                }
            }
        }
        out
    }

    fn outline_vertices(&self, block: IVec3, lo: [f32; 3], hi: [f32; 3], camera: DVec3) -> [[f32; 3]; 24] {
        let e = 0.004;
        let origin = (block.as_dvec3() - camera).as_vec3();
        let min = origin + Vec3::from_array(lo) - Vec3::splat(e);
        let max = origin + Vec3::from_array(hi) + Vec3::splat(e);
        let c = |x: bool, y: bool, z: bool| {
            [if x { max.x } else { min.x }, if y { max.y } else { min.y }, if z { max.z } else { min.z }]
        };
        let mut out = [[0.0; 3]; 24];
        let mut i = 0;
        for a in [false, true] {
            for b in [false, true] {
                for (p, q) in
                    [(c(false, a, b), c(true, a, b)), (c(a, false, b), c(a, true, b)), (c(a, b, false), c(a, b, true))]
                {
                    out[i] = p;
                    out[i + 1] = q;
                    i += 2;
                }
            }
        }
        out
    }

    /// Draws a frame covering the whole window; returns `false` if it was
    /// skipped (window hidden).
    pub fn render(&mut self, p: &FrameParams) -> bool {
        let Some(mut frame) = self.begin_frame() else { return false };
        self.draw_view(&mut frame, p, Viewport::full(self.size()));
        self.end_frame(frame);
        true
    }

    /// Acquires the window image for a frame drawn as one or more views
    /// ([`Renderer::draw_view`], then [`Renderer::end_frame`]). `None` if the
    /// frame should be skipped (window hidden).
    pub fn begin_frame(&mut self) -> Option<Frame> {
        let acquire_start = std::time::Instant::now();
        let frame = if self.force_offscreen {
            None
        } else {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => Some(f),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface.configure(&self.device, &self.config);
                    None
                }
                _ => None, // occluded / timeout
            }
        };
        // Without a surface frame (e.g. hidden window) captures and
        // benchmarks still render offscreen; otherwise back off instead of
        // spinning.
        if frame.is_none() {
            if !self.force_offscreen && self.capture.is_none() {
                std::thread::sleep(std::time::Duration::from_millis(8));
                return None;
            }
            let size = (self.config.width, self.config.height);
            if self.offscreen.as_ref().is_none_or(|t| (t.width(), t.height()) != size) {
                self.offscreen = Some(self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("offscreen"),
                    size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.config.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                }));
            }
        }
        let acquire_ms = acquire_start.elapsed().as_secs_f64() * 1000.0;
        let target = match &frame {
            Some(f) => &f.texture,
            None => self.offscreen.as_ref().unwrap(),
        };
        let view = target.create_view(&Default::default());
        self.stats = RenderStats { meshes: self.meshes.len(), acquire_ms, ..Default::default() };
        Some(Frame { surface: frame, view, drawn: false })
    }

    /// Draws one camera's view into `vp` (physical pixels). Each view is
    /// submitted on its own, so per-view buffers can be refilled between
    /// calls ([`Renderer::set_entities`], [`Renderer::set_weather`]). The
    /// first view of a frame clears the whole image.
    pub fn draw_view(&mut self, frame: &mut Frame, p: &FrameParams, vp: Viewport) {
        let view = &frame.view;
        // Camera-relative view-projection.
        // wgpu NDC is DirectX-style: Z in [0, 1], Y up.
        let proj = glam::camera::rh::proj::directx::perspective_infinite_reverse(p.fov_y, vp.aspect(), 0.05);
        let view_mat = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, p.forward, Vec3::Y);
        let view_proj = proj * view_mat;
        let frustum = Frustum::new(view_proj);

        // Cull and sort (front to back for early-z; translucents walk it
        // backwards).
        self.visible.clear();
        let size = CHUNK_SIZE_I as f64;
        for &pos in self.meshes.keys() {
            let origin = (pos.as_dvec3() * size - p.camera).as_vec3();
            if frustum.intersects(origin, origin + Vec3::splat(size as f32)) {
                let center = origin + Vec3::splat(size as f32 / 2.0);
                self.visible.push((center.length_squared(), pos, origin.to_array(), facing_faces(origin)));
            }
        }
        self.visible.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

        if self.visible.len() > self.instance_capacity {
            self.instance_capacity = self.visible.len().next_power_of_two();
            self.instances = Self::create_instance_buffer(&self.device, self.instance_capacity);
        }
        let instance_data: Vec<[f32; 3]> = self.visible.iter().map(|v| v.2).collect();
        if !instance_data.is_empty() {
            self.queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instance_data));
        }

        let cloud_origin = (DVec3::new(p.camera.x + p.time as f64 * CLOUD_SPEED, 0.0, p.camera.z))
            .rem_euclid(DVec3::splat(CLOUD_PERIOD));
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            fog_color: [p.fog_color[0], p.fog_color[1], p.fog_color[2], 1.0],
            zenith_color: [p.zenith_color[0], p.zenith_color[1], p.zenith_color[2], 1.0],
            environment: [
                match p.dimension {
                    crate::world::terrain::Dimension::Overworld => 0.0,
                    crate::world::terrain::Dimension::Nether => 1.0,
                    crate::world::terrain::Dimension::End => 2.0,
                },
                f32::from(p.enhanced_graphics),
                p.camera.x.rem_euclid(128.0) as f32,
                p.camera.z.rem_euclid(128.0) as f32,
            ],
            sun: [p.sun_dir.x, p.sun_dir.y, p.sun_dir.z, p.time % 3600.0],
            params: [p.fog_start, p.fog_end, p.daylight, p.rain],
            clouds: [
                cloud_origin.x as f32,
                cloud_origin.z as f32,
                (CLOUD_HEIGHT - p.camera.y) as f32,
                p.fog_end.max(64.0) * 1.6,
            ],
        };
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

        if let Some((b, layer)) = p.crack {
            self.queue.write_buffer(&self.decal_buf, 0, &Self::decal_vertices(b, p.camera, layer));
        }
        if let Some((b, lo, hi)) = p.highlight {
            let verts = self.outline_vertices(b, lo, hi, p.camera);
            self.queue.write_buffer(&self.line_buf, 0, bytemuck::cast_slice(&verts));
        }
        let hand = p.hand.as_ref().map(|h| (h, p.forward, p.fov_y, vp.aspect()));
        self.block_models.set(&self.device, &self.queue, &p.block_models, hand, p.camera);
        let hud = &p.ui;
        if hud.len() > self.ui_capacity {
            self.ui_capacity = hud.len().next_power_of_two();
            self.ui_buf = Self::create_ui_buffer(&self.device, self.ui_capacity);
        }
        if !hud.is_empty() {
            self.queue.write_buffer(&self.ui_buf, 0, bytemuck::cast_slice(hud));
        }

        let mut stats = std::mem::take(&mut self.stats);
        stats.visible += self.visible.len();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Later views keep what earlier ones drew; the sky
                        // fills their own background.
                        load: if frame.drawn {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: p.sky_color[0],
                                g: p.sky_color[1],
                                b: p.sky_color[2],
                                a: 1.0,
                            })
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(vp.x as f32, vp.y as f32, vp.width as f32, vp.height as f32, 0.0, 1.0);
            pass.set_scissor_rect(vp.x, vp.y, vp.width, vp.height);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.blocks_bg, &[]);
            pass.set_index_buffer(self.quad_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_vertex_buffer(0, self.instances.slice(..));

            // Each pass draws the face groups of every chunk that can face
            // the camera, merging adjacent groups into one draw.
            let mut draw_range =
                |pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, kind: usize, back_to_front: bool| {
                    pass.set_pipeline(pipeline);
                    // Only rebind when a chunk lives in a different arena page.
                    let mut page = None;
                    let mut draw = |i: usize| {
                        let (_, pos, _, faces) = self.visible[i];
                        let mesh = &self.meshes[&pos];
                        // Cross quads face every way; they all sit in group 0.
                        let mask = if kind == CROSS { 1 } else { group_mask(faces, kind) };
                        face_runs(&mesh.offsets[kind * 6..kind * 6 + 7], mask, |first_quad, quads| {
                            if page != Some(mesh.alloc.page) {
                                pass.set_bind_group(2, self.arena.bind_group(mesh.alloc.page), &[]);
                                page = Some(mesh.alloc.page);
                            }
                            // vertex_index = base_vertex + index selects the quad record.
                            let base = (mesh.alloc.offset + first_quad) * 4;
                            pass.draw_indexed(0..quads * 6, base as i32, i as u32..i as u32 + 1);
                            stats.draw_calls += 1;
                            stats.quads += quads as u64;
                        });
                    };
                    if back_to_front {
                        (0..self.visible.len()).rev().for_each(&mut draw);
                    } else {
                        (0..self.visible.len()).for_each(&mut draw);
                    }
                };
            draw_range(&mut pass, &self.opaque_pipeline, OPAQUE, false);
            draw_range(&mut pass, &self.cutout_pipeline, CUTOUT, false);
            draw_range(&mut pass, &self.cross_pipeline, CROSS, false);
            self.entities.draw(&mut pass);
            self.block_models.draw(&mut pass);

            // Sky after terrain so early-z skips covered pixels.
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            if p.dimension.has_sky() {
                pass.set_pipeline(&self.cloud_pipeline);
                pass.draw(0..6, 0..1);
            }

            if p.crack.is_some() {
                pass.set_pipeline(&self.decal_pipeline);
                pass.set_vertex_buffer(0, self.decal_buf.slice(..));
                pass.draw(0..36, 0..1);
            }
            if p.highlight.is_some() {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_vertex_buffer(0, self.line_buf.slice(..));
                pass.draw(0..24, 0..1);
            }

            pass.set_vertex_buffer(0, self.instances.slice(..));
            draw_range(&mut pass, &self.translucent_pipeline, TRANSLUCENT, true);
            self.weather.draw(&mut pass);

            if !hud.is_empty() {
                pass.set_pipeline(&self.ui_pipeline);
                pass.set_bind_group(2, &self.font_bg, &[]);
                pass.set_vertex_buffer(0, self.ui_buf.slice(..));
                pass.draw(0..hud.len() as u32, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        frame.drawn = true;
        stats.gpu_bytes = self.arena.capacity_bytes();
        stats.gpu_used_bytes = self.arena.used_bytes();
        self.stats = stats;
    }

    /// Saves a requested screenshot and presents the frame.
    pub fn end_frame(&mut self, frame: Frame) {
        let target = match &frame.surface {
            Some(f) => &f.texture,
            None => self.offscreen.as_ref().unwrap(),
        };
        match self.capture.take() {
            Some(path) if target.usage().contains(wgpu::TextureUsages::COPY_SRC) => {
                let encoder = self.device.create_command_encoder(&Default::default());
                self.save_capture(target, encoder, &path)
            }
            Some(_) => log::error!("surface doesn't support COPY_SRC; can't take screenshots"),
            None => {}
        }
        if let Some(surface) = frame.surface {
            self.window.pre_present_notify();
            self.queue.present(surface);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaders_parse_and_validate() {
        let shaders = [
            ("chunk", include_str!("shaders/chunk.wgsl")),
            ("block_model", include_str!("shaders/block_model.wgsl")),
            ("entity", include_str!("shaders/entity.wgsl")),
            ("overlay", include_str!("shaders/overlay.wgsl")),
            ("sky", include_str!("shaders/sky.wgsl")),
            ("weather", include_str!("shaders/weather.wgsl")),
        ];
        for (name, source) in shaders {
            let module =
                naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
        }
    }

    fn runs(offsets: &[u32], mask: u8) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        face_runs(offsets, mask, |a, b| out.push((a, b)));
        out
    }

    #[test]
    fn split_screen_viewports_tile_the_window() {
        assert_eq!(Viewport::split((1601, 901), 1, false), vec![Viewport::full((1601, 901))]);
        for count in 2..=4 {
            for side in [false, true] {
                let views = Viewport::split((1601, 901), count, side);
                assert_eq!(views.len(), count);
                let area: u32 = views.iter().map(|v| v.width * v.height).sum();
                assert_eq!(area, 1601 * 901, "{count} {side}");
                assert!(views.iter().all(|v| v.x + v.width <= 1601 && v.y + v.height <= 901));
            }
        }
        let [top, bottom] = Viewport::split((1600, 900), 2, false)[..] else { panic!() };
        assert_eq!((top.y, top.height, bottom.y, bottom.width), (0, 450, 450, 1600));
        let [left, right] = Viewport::split((1600, 900), 2, true)[..] else { panic!() };
        assert_eq!((left.width, right.x, right.height), (800, 800, 900));
    }

    #[test]
    fn face_runs_merge_adjacent_and_empty_groups() {
        let offsets = [0, 1, 3, 6, 10, 15, 21];
        assert_eq!(runs(&offsets, 0b111111), [(0, 21)]);
        assert_eq!(runs(&offsets, 0b101001), [(0, 1), (6, 4), (15, 6)]);
        assert_eq!(runs(&offsets, 0), []);
        // Group 1 is empty, so it bridges groups 0 and 2.
        let offsets = [0, 4, 4, 6, 6, 6, 9];
        assert_eq!(runs(&offsets, 0b000101), [(0, 6)]);
        assert_eq!(runs(&offsets, 0b100001), [(0, 4), (6, 3)]);
    }

    #[test]
    fn facing_faces_culls_faces_pointing_away() {
        // Chunk entirely in front of the camera on +X, above it, and level on Z.
        assert_eq!(facing_faces(Vec3::new(10.0, 40.0, -16.0)), 0b111010);
        // Camera inside the chunk sees every direction.
        assert_eq!(facing_faces(Vec3::splat(-16.0)), 0b111111);
        // Water tops can dip below the chunk's min corner.
        assert_ne!(facing_faces(Vec3::new(-16.0, 1.0, -16.0)) & 0b100, 0);
    }

    #[test]
    fn group_mask_follows_face_order() {
        assert_eq!(group_mask(0b000101, TRANSLUCENT), 0b000101);
        // +X +Y +Z -X -Y -Z
        assert_eq!(group_mask(0b000101, OPAQUE), 0b000011);
        assert_eq!(group_mask(0b101010, OPAQUE), 0b111000);
    }
}
