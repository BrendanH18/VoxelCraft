// Block selection outline (world space) and 2D HUD (screen space).

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
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var blocks_sampler: sampler;
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
fn sample_block_level(uv: vec2<f32>, layer: u32) -> vec4<f32> {
    if !BLOCK_PAGING { return textureSampleLevel(blocks, blocks_sampler, uv, layer, 0.0); }
    let local = layer & 255u;
    switch layer >> 8u {
        case 1u: { return textureSampleLevel(blocks_1, blocks_sampler, uv, local, 0.0); }
        case 2u: { return textureSampleLevel(blocks_2, blocks_sampler, uv, local, 0.0); }
        case 3u: { return textureSampleLevel(blocks_3, blocks_sampler, uv, local, 0.0); }
        case 4u: { return textureSampleLevel(blocks_4, blocks_sampler, uv, local, 0.0); }
        case 5u: { return textureSampleLevel(blocks_5, blocks_sampler, uv, local, 0.0); }
        case 6u: { return textureSampleLevel(blocks_6, blocks_sampler, uv, local, 0.0); }
        case 7u: { return textureSampleLevel(blocks_7, blocks_sampler, uv, local, 0.0); }
        default: { return textureSampleLevel(blocks, blocks_sampler, uv, local, 0.0); }
    }
}

// Item icons, addressed as layers from 2048 on (see `tex::ITEM_BASE`).
@group(1) @binding(2) var items: texture_2d_array<f32>;
@group(2) @binding(0) var font: texture_2d<f32>;

@vertex
fn vs_line(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return g.view_proj * vec4<f32>(pos, 1.0);
}

@fragment
fn fs_line() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.7);
}

struct DecalOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
};

@vertex
fn vs_decal(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) layer: u32) -> DecalOut {
    var out: DecalOut;
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.layer = layer;
    return out;
}

@fragment
fn fs_decal(in: DecalOut) -> @location(0) vec4<f32> {
    return sample_block(in.uv, in.layer);
}

struct UiOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) layer: f32,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_ui(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: f32,
    @location(3) color: vec4<f32>,
) -> UiOut {
    var out: UiOut;
    out.clip = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    out.layer = layer;
    out.color = color;
    return out;
}

@fragment
fn fs_ui(in: UiOut) -> @location(0) vec4<f32> {
    // layer -2: font glyph, -1: flat colour, >= 0: tinted block texture,
    // >= 2048: tinted item icon. A negative alpha fills the texture's shape
    // with the colour instead (the enchantment glint).
    if in.layer >= 0.0 && in.color.a < 0.0 {
        let layer = i32(in.layer + 0.5);
        var a = 0.0;
        if layer >= 2048 {
            a = textureSampleLevel(items, blocks_sampler, in.uv, layer - 2048, 0.0).a;
        } else {
            a = sample_block_level(in.uv, u32(layer)).a;
        }
        return vec4<f32>(in.color.rgb, -in.color.a * a);
    }
    if in.layer < -1.5 {
        let coverage = textureSampleLevel(font, blocks_sampler, in.uv, 0.0).r;
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }
    if in.layer < 0.0 {
        return in.color;
    }
    let layer = i32(in.layer + 0.5);
    if layer >= 2048 {
        return textureSampleLevel(items, blocks_sampler, in.uv, layer - 2048, 0.0) * in.color;
    }
    return sample_block_level(in.uv, u32(layer)) * in.color;
}
