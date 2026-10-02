// Free-standing textured blocks (see src/render/block_model.rs): lit like
// entities, with the terrain's light curve, face shading and fog.

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

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    // x: sky light, y: face shade, z: torch light
    @location(2) light: vec3<f32>,
    @location(3) dist: f32,
};

fn transform(pos: vec3<f32>, uv: vec2<f32>, layer: u32, light: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.layer = layer;
    out.light = light;
    out.dist = length(pos);
    return out;
}

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: u32,
    @location(3) light: vec3<f32>,
) -> VsOut {
    return transform(pos, uv, layer, light);
}

// The first-person hand: like `vs_main`, but with depth pulled into the
// nearest quarter of the (reversed) range, so it's always in front of the
// world, which never comes within 0.18 of the eye.
@vertex
fn vs_hand(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: u32,
    @location(3) light: vec3<f32>,
) -> VsOut {
    var out = transform(pos, uv, layer, light);
    let depth = out.clip.z / out.clip.w;
    out.clip.z = out.clip.w * (0.75 + 0.25 * clamp(depth * 4.0, 0.0, 1.0));
    out.dist = 0.0;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(blocks, blocks_sampler, in.uv, in.layer);
    if tex.a < 0.5 {
        discard;
    }
    let l = in.light.xz / (4.0 - 3.0 * in.light.xz);
    let torch = l.y * vec3<f32>(1.0, 0.86, 0.66);
    let lit = (max(vec3<f32>(l.x * g.params.z), torch) * 0.96 + 0.04) * in.light.y;
    let f = clamp((in.dist - g.params.x) / (g.params.y - g.params.x), 0.0, 1.0);
    let c = mix(tex.rgb * lit, g.fog_color.rgb, f * f * (3.0 - 2.0 * f));
    return vec4<f32>(c, 1.0);
}
