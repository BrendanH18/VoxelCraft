// Chunk terrain shader. Quads are 12-byte records in a storage buffer (see
// src/mesh.rs) that the vertex shader expands into corners (vertex
// pulling): with the shared quad index buffer and base_vertex = first quad
// * 4, vertex_index / 4 is the quad and vertex_index % 4 the corner. The
// per-draw instance attribute is the chunk origin relative to the camera.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    zenith_color: vec4<f32>,
    sun: vec4<f32>,
    // x: fog start, y: fog end, z: daylight (skylight multiplier), w: unused
    params: vec4<f32>,
    clouds: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var blocks_sampler: sampler;
@group(2) @binding(0) var<storage, read> quads: array<u32>;

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
fn vs_main(@builtin(vertex_index) vi: u32, @location(1) offset: vec3<f32>) -> VsOut {
    let q = (vi >> 2u) * 3u;
    let w0 = quads[q];
    let w1 = quads[q + 1u];
    let w2 = quads[q + 2u];
    let face = (w0 >> 18u) & 7u;
    let size = vec2<u32>(((w0 >> 21u) & 31u) + 1u, ((w0 >> 26u) & 31u) + 1u);

    // Corners are (0,0), (w,0), (w,h), (0,h) in the face's (u, v) plane.
    // Negative faces run in reverse for winding; `flip` rotates by one so
    // the index pattern's diagonal becomes the other one.
    var order = array<u32, 4>(0u, 1u, 2u, 3u);
    if (face & 1u) == 1u {
        order = array<u32, 4>(0u, 3u, 2u, 1u);
    }
    let c = order[((vi & 3u) + (w0 >> 31u)) & 3u];
    let d = face >> 1u;
    let u = (d + 1u) % 3u;
    let v = (d + 2u) % 3u;
    let base = vec3<u32>(w0 & 63u, (w0 >> 6u) & 63u, (w0 >> 12u) & 63u);
    var corner = base;
    if c == 1u || c == 2u {
        corner[u] += size.x;
    }
    if c >= 2u {
        corner[v] += size.y;
    }
    let ao = (w1 >> (8u + 2u * c)) & 3u;
    let light = (w2 >> (8u * c)) & 255u;

    // Water surfaces: lower the quad's upper edge (never the bottom face).
    var top = base.y;
    if u == 1u {
        top += size.x;
    } else if v == 1u {
        top += size.y;
    }
    var drop = 0.0;
    if face != 3u && corner.y == top {
        drop = f32((w1 >> 16u) & 31u) / 16.0;
    }
    let local = vec3<f32>(corner) - vec3<f32>(0.0, drop, 0.0);

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
    out.layer = w1 & 255u;
    out.shade = face_shade[face] * ao_curve[ao];
    out.dist = length(rel);
    out.light = vec2<f32>(f32(light & 15u), f32(light >> 4u)) / 15.0;
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
