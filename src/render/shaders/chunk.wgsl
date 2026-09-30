// Chunk terrain shader. Each vertex is two packed u32s (see src/mesh.rs);
// the per-draw instance attribute is the chunk origin relative to the camera.

struct Globals {
    view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    // x: fog start, y: fog end, z: daylight (skylight multiplier), w: unused
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var blocks_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) shade: f32,
    @location(3) dist: f32,
    // x: sky light, y: block light (0..1), interpolated for smooth lighting.
    @location(4) light: vec2<f32>,
};

@vertex
fn vs_main(@location(0) data: vec2<u32>, @location(1) offset: vec3<f32>) -> VsOut {
    let packed = data.x;
    let local = vec3<f32>(
        f32(packed & 63u),
        f32((packed >> 6u) & 63u),
        f32((packed >> 12u) & 63u),
    );
    let face = (packed >> 18u) & 7u;
    let ao = (packed >> 21u) & 3u;

    var face_shade = array<f32, 6>(0.8, 0.8, 1.0, 0.55, 0.68, 0.68);
    var ao_curve = array<f32, 4>(0.42, 0.62, 0.82, 1.0);

    // Planar UVs from the chunk-local position: merged quads tile the texture.
    var uv: vec2<f32>;
    if face < 2u {
        uv = vec2<f32>(local.z, -local.y);
    } else if face < 4u {
        uv = local.xz;
    } else {
        uv = vec2<f32>(local.x, -local.y);
    }

    let rel = offset + local;
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    out.uv = uv;
    out.layer = (packed >> 23u) & 255u;
    out.shade = face_shade[face] * ao_curve[ao];
    out.dist = length(rel);
    out.light = vec2<f32>(f32(data.y & 15u), f32((data.y >> 4u) & 15u)) / 15.0;
    return out;
}

// Minecraft-like light curve: dim levels fall off quickly.
fn curve(l: vec2<f32>) -> vec2<f32> {
    return l / (4.0 - 3.0 * l);
}

fn lighting(in: VsOut) -> vec3<f32> {
    let l = curve(in.light);
    let sky = vec3<f32>(l.x * g.params.z);
    let torch = l.y * vec3<f32>(1.0, 0.86, 0.66);
    return (max(sky, torch) * 0.96 + 0.04) * in.shade;
}

fn apply_fog(color: vec3<f32>, dist: f32) -> vec3<f32> {
    let f = clamp((dist - g.params.x) / (g.params.y - g.params.x), 0.0, 1.0);
    return mix(color, g.fog_color.rgb, f * f * (3.0 - 2.0 * f));
}

@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(blocks, blocks_sampler, in.uv, in.layer);
    return vec4<f32>(apply_fog(tex.rgb * lighting(in), in.dist), 1.0);
}

@fragment
fn fs_cutout(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(blocks, blocks_sampler, in.uv, in.layer);
    if tex.a < 0.5 {
        discard;
    }
    return vec4<f32>(apply_fog(tex.rgb * lighting(in), in.dist), 1.0);
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(blocks, blocks_sampler, in.uv, in.layer);
    return vec4<f32>(apply_fog(tex.rgb * lighting(in), in.dist), tex.a);
}
