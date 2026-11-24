struct Globals {
    view_proj: mat4x4<f32>, inv_view_proj: mat4x4<f32>, fog_color: vec4<f32>,
    zenith_color: vec4<f32>, sun: vec4<f32>, params: vec4<f32>, clouds: vec4<f32>,
    environment: vec4<f32>, effects: vec4<f32>, ambient: vec4<f32>,
    fog: vec4<f32>, fog_ends: vec4<f32>, sky_light: vec4<f32>, sunrise: vec4<f32>,
};
struct Axes { right: vec4<f32>, up: vec4<f32> };
@group(0) @binding(0) var<uniform> g: Globals;

// The exact sRGB transfer the target encodes with, so a colour taken
// from linear to sRGB and back is unchanged and a fully fogged pixel
// shows Java's fog colour byte for byte.
fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}
fn from_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
}
// Java 26.3's fog.glsl: spherical and cylindrical distance, and the larger
// of the environmental and render-distance fogs, blended in sRGB as Java
// blends it (exact sRGB, matching the fog colour's conversion).
fn fog_distances(rel: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(length(rel), max(length(rel.xz), abs(rel.y)));
}
fn linear_fog(d: f32, start: f32, end: f32) -> f32 {
    if d <= start { return 0.0; }
    if d >= end { return 1.0; }
    return (d - start) / (end - start);
}
fn fog_value(d: vec2<f32>) -> f32 {
    return max(linear_fog(d.x, g.fog.x, g.fog.y), linear_fog(d.y, g.fog.z, g.fog.w));
}
fn apply_fog(color: vec3<f32>, d: vec2<f32>) -> vec3<f32> {
    let f = fog_value(d);
    if f <= 0.0 { return color; }
    return from_srgb(mix(to_srgb(color), to_srgb(g.fog_color.rgb), f));
}
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var block_sampler: sampler;
// Extra pages on adapters that cap texture arrays (see `block_shader`).
@group(1) @binding(3) var blocks_1: texture_2d_array<f32>;
@group(1) @binding(4) var blocks_2: texture_2d_array<f32>;
@group(1) @binding(5) var blocks_3: texture_2d_array<f32>;
@group(1) @binding(6) var blocks_4: texture_2d_array<f32>;
@group(1) @binding(7) var blocks_5: texture_2d_array<f32>;
@group(1) @binding(8) var blocks_6: texture_2d_array<f32>;
@group(1) @binding(9) var blocks_7: texture_2d_array<f32>;
const BLOCK_PAGING: bool = false;
const PAGE_SHIFT: u32 = 8u;
fn sample_block(uv: vec2<f32>, layer: u32) -> vec4<f32> {
    if !BLOCK_PAGING { return textureSample(blocks, block_sampler, uv, layer); }
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let local = layer & ((1u << PAGE_SHIFT) - 1u);
    switch layer >> PAGE_SHIFT {
        case 1u: { return textureSampleGrad(blocks_1, block_sampler, uv, local, dx, dy); }
        case 2u: { return textureSampleGrad(blocks_2, block_sampler, uv, local, dx, dy); }
        case 3u: { return textureSampleGrad(blocks_3, block_sampler, uv, local, dx, dy); }
        case 4u: { return textureSampleGrad(blocks_4, block_sampler, uv, local, dx, dy); }
        case 5u: { return textureSampleGrad(blocks_5, block_sampler, uv, local, dx, dy); }
        case 6u: { return textureSampleGrad(blocks_6, block_sampler, uv, local, dx, dy); }
        case 7u: { return textureSampleGrad(blocks_7, block_sampler, uv, local, dx, dy); }
        default: { return textureSampleGrad(blocks, block_sampler, uv, local, dx, dy); }
    }
}
@group(2) @binding(0) var sprites: texture_2d_array<f32>;
@group(2) @binding(1) var sprite_sampler: sampler;
@group(2) @binding(2) var<uniform> axes: Axes;
struct Output {
    @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>, @location(2) @interpolate(flat) light: vec4<f32>,
    @location(3) dist: vec2<f32>,
};
@vertex fn vs_main(
    @builtin(vertex_index) vertex: u32, @location(0) center: vec3<f32>,
    @location(1) size: f32, @location(2) uv: vec4<f32>, @location(3) color: vec4<f32>,
    @location(4) light: vec4<f32>,
) -> Output {
    let corners = array<vec2<f32>, 6>(vec2<f32>(0., 1.), vec2<f32>(1., 1.), vec2<f32>(1., 0.), vec2<f32>(1., 0.), vec2<f32>(0., 0.), vec2<f32>(0., 1.));
    let c = corners[vertex];
    let pos = center + axes.right.xyz * ((c.x * 2. - 1.) * size) + axes.up.xyz * ((1. - c.y * 2.) * size);
    var out: Output;
    out.clip = g.view_proj * vec4<f32>(pos, 1.);
    out.uv = mix(uv.xy, uv.zw, c);
    out.color = color; out.light = light; out.dist = fog_distances(center);
    return out;
}
fn curve(l: f32) -> f32 { return l / (4. - 3. * l); }
// Java 26.3 adds the environment ambient colour to sky and block light.
// The Overworld retains its existing daylight presentation in this scoped fix.
fn dimension_light(sky: vec3<f32>, torch: vec3<f32>) -> vec3<f32> {
    if g.ambient.w > 0.0 {
        return clamp(g.ambient.rgb + sky + torch, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return max(sky, torch) * 0.96 + 0.04;
}

@fragment fn fs_main(in: Output) -> @location(0) vec4<f32> {
    var texel: vec4<f32>;
    if (in.light.w % 2.) > 0.5 {
        texel = sample_block(in.uv, u32(in.light.z));
    } else {
        texel = textureSampleLevel(sprites, sprite_sampler, in.uv, i32(in.light.z), 0.);
    }
    if texel.a * in.color.a < 0.1 { discard; }
    // Dither fractional ambient-effect alpha, keeping the unified depth-writing pass.
    let screen = vec2<u32>(in.clip.xy);
    let threshold = f32((screen.x & 3u) * 4u + (screen.y & 3u)) / 16.;
    if in.color.a < threshold { discard; }
    let sky = curve(in.light.x) * g.params.z;
    let torch = curve(in.light.y) * vec3<f32>(1., 0.86, 0.66);
    var lit = dimension_light(vec3<f32>(sky), torch);
    if in.light.w > 1.5 { lit = vec3<f32>(1.); }
    let peak = max(max(lit.r, lit.g), max(lit.b, 0.001));
    lit = clamp(mix(lit, lit / peak, g.effects.x), vec3<f32>(0.), vec3<f32>(1.));
    // Brightness (Java's gamma, g.effects.y), as in chunk.wgsl.
    let top = max(max(lit.r, lit.g), max(lit.b, 1e-4));
    let inv = 1. - top;
    lit = mix(lit, lit * ((1. - inv * inv * inv * inv) / top), g.effects.y);
    let base = texel.rgb * pow(in.color.rgb, vec3<f32>(2.2)) * lit;
    return vec4<f32>(apply_fog(base, in.dist), 1.);
}
