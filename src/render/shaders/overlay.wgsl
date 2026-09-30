// Block selection outline (world space) and 2D HUD (screen space).

struct Globals {
    view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var blocks_sampler: sampler;

@vertex
fn vs_line(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return g.view_proj * vec4<f32>(pos, 1.0);
}

@fragment
fn fs_line() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.7);
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
    // Negative layer: flat colour. Otherwise a block texture tinted by colour.
    if in.layer < 0.0 {
        return in.color;
    }
    let tex = textureSampleLevel(blocks, blocks_sampler, in.uv, i32(in.layer + 0.5), 0.0);
    return tex * in.color;
}
