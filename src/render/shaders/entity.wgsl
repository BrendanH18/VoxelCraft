// Mob box models. Vertices are camera-relative and pre-transformed on the
// CPU; lighting matches the terrain shader (sky light x daylight, face
// shading, fog).

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    zenith_color: vec4<f32>,
    sun: vec4<f32>,
    // x: fog start, y: fog end, z: daylight (skylight multiplier), w: unused
    params: vec4<f32>,
    clouds: vec4<f32>,
    environment: vec4<f32>,
    // x: night vision strength 0..1.
    effects: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var blocks_sampler: sampler;
@group(1) @binding(2) var items: texture_2d_array<f32>;
@group(1) @binding(3) var blocks_1: texture_2d_array<f32>;
@group(1) @binding(4) var blocks_2: texture_2d_array<f32>;
@group(1) @binding(5) var blocks_3: texture_2d_array<f32>;
@group(1) @binding(6) var blocks_4: texture_2d_array<f32>;
@group(1) @binding(7) var blocks_5: texture_2d_array<f32>;
@group(1) @binding(8) var blocks_6: texture_2d_array<f32>;
@group(1) @binding(9) var blocks_7: texture_2d_array<f32>;
// Specialized at startup: one array when supported, portable pages otherwise.
const BLOCK_PAGING: bool = false;
fn sample_block(uv: vec2<f32>, layer: u32) -> vec4<f32> {
    if !BLOCK_PAGING { return textureSample(blocks, blocks_sampler, uv, layer); }
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let local = layer & 255u;
    switch layer >> 8u {
        case 1u: { return textureSampleGrad(blocks_1, blocks_sampler, uv, local, dx, dy); }
        case 2u: { return textureSampleGrad(blocks_2, blocks_sampler, uv, local, dx, dy); }
        case 3u: { return textureSampleGrad(blocks_3, blocks_sampler, uv, local, dx, dy); }
        case 4u: { return textureSampleGrad(blocks_4, blocks_sampler, uv, local, dx, dy); }
        case 5u: { return textureSampleGrad(blocks_5, blocks_sampler, uv, local, dx, dy); }
        case 6u: { return textureSampleGrad(blocks_6, blocks_sampler, uv, local, dx, dy); }
        case 7u: { return textureSampleGrad(blocks_7, blocks_sampler, uv, local, dx, dy); }
        default: { return textureSampleGrad(blocks, blocks_sampler, uv, local, dx, dy); }
    }
}
@group(2) @binding(0) var skin: texture_2d_array<f32>;

// Night vision (Java's lightmap): brightens every light level toward full,
// keeping its hue; g.effects.x is its strength.
fn night_vision(lit: vec3<f32>) -> vec3<f32> {
    let peak = max(max(lit.r, lit.g), max(lit.b, 1e-3));
    return mix(lit, lit / peak, g.effects.x);
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // x: sky light, y: face shade, z: hurt, w: emissive
    @location(2) light: vec4<f32>,
    @location(3) dist: f32,
    @location(5) rel: vec3<f32>,
    @location(4) torch: f32,
    @location(6) @interpolate(flat) material: u32,
    @location(7) @interpolate(flat) layer: u32,
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) light: vec4<f32>,
    @location(4) torch: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    out.torch = torch.x;
    out.material = u32(round(torch.y * 255.0));
    out.layer = u32(round(torch.z * 255.0)) + u32(round(torch.w * 255.0)) * 256u;
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.color = color;
    out.light = light;
    out.dist = length(pos);
    out.rel = pos;
    return out;
}

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

// Same curve as terrain: dim levels fall off quickly.
fn curve(l: f32) -> f32 {
    return l / (4.0 - 3.0 * l);
}

fn apply_fog(color: vec3<f32>, dist: f32) -> vec3<f32> {
    let f = clamp((dist - g.params.x) / (g.params.y - g.params.x), 0.0, 1.0);
    return mix(color, g.fog_color.rgb, f * f * (3.0 - 2.0 * f));
}

// Directional skylight; caves retain their block lighting and dimensions
// without a sun keep their steady ambient illumination.
fn daylight_tint(normal: vec3<f32>) -> vec3<f32> {
    if g.environment.y < 0.5 || g.environment.x > 0.5 {
        return vec3<f32>(1.0);
    }
    let moon = g.sun.y < 0.0;
    let dir = select(g.sun.xyz, -g.sun.xyz, moon);
    let elevation = abs(dir.y);
    let dusk = 1.0 - smoothstep(0.05, 0.45, elevation);
    let warm = mix(vec3<f32>(1.0), vec3<f32>(1.12, 0.80, 0.58), dusk);
    let tint = select(warm, vec3<f32>(0.72, 0.82, 1.05), moon);
    let diffuse = max(dot(normal, dir), 0.0);
    return mix(tint * (0.62 + 0.38 * diffuse), vec3<f32>(0.85), g.params.w);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let area_normal = cross(dpdx(in.rel), dpdy(in.rel));
    let length_sq = dot(area_normal, area_normal);
    var face_normal = vec3<f32>(0.0, 1.0, 0.0);
    if length_sq > 1e-20 {
        face_normal = area_normal * inverseSqrt(length_sq);
    }
    let normal = select(face_normal, -face_normal, dot(face_normal, -in.rel) < 0.0);
    // Per-texel brightness noise gives the flat colours a pixel-art texture.
    let n = hash2(floor(in.uv) + 0.5) - 0.5;
    // Colours are authored in sRGB, like the block textures.
    var base = pow(in.color.rgb, vec3<f32>(2.2)) * (1.0 + n * in.color.a);

    // Held blocks and item icons share the terrain bind group. Both are
    // sampled up front so derivatives stay in uniform control flow.
    let held_block = sample_block(in.uv, min(in.layer, 2047u));
    let held_icon = textureSample(items, blocks_sampler, in.uv, max(in.layer, 2048u) - 2048u);
    let texel = clamp(vec2<i32>(floor(in.uv)), vec2<i32>(0), vec2<i32>(63));
    if in.material == 1u {
        let tex = textureLoad(skin, texel, 0, 0);
        if tex.a < 0.5 { discard; }
        base = tex.rgb;
    } else if in.material == 3u {
        // Armor sheet. Vertex colour is the leather dye (white for the rest).
        let tex = textureLoad(skin, texel, i32(in.layer), 0);
        if tex.a < 0.5 { discard; }
        base = tex.rgb * pow(in.color.rgb, vec3<f32>(2.2));
    } else if in.material == 2u {
        // Block layers are below tex::ITEM_BASE (2048); icons are at and above it.
        let tex = select(held_block, held_icon, in.layer >= 2048u);
        if tex.a < 0.5 { discard; }
        base = tex.rgb;
    }

    // Sky light scaled by daylight, or warm torch light, whichever is brighter.
    let sky = curve(in.light.x) * g.params.z * daylight_tint(normal);
    let torch = curve(in.torch) * vec3<f32>(1.0, 0.86, 0.66);
    let lit = mix(night_vision(max(sky, torch) * 0.96 + 0.04) * in.light.y, vec3<f32>(1.0), in.light.w);
    var c = base * lit;
    // Hurt: Minecraft-style red overlay.
    c = mix(c, vec3<f32>(0.8, 0.0, 0.0) * max(lit, vec3<f32>(0.25)), in.light.z * 0.6);
    // Enchantment glint: a scrolling band, cheap enough to stay in this pass.
    if in.material == 3u && in.color.a > 0.5 {
        let stripe = abs(fract(in.uv.x * 0.11 - in.uv.y * 0.11 + g.sun.w * 0.35) - 0.5);
        c += vec3<f32>(0.55, 0.42, 0.9) * smoothstep(0.40, 0.50, 0.5 - stripe);
    }
    return vec4<f32>(apply_fog(c, in.dist), 1.0);
}
