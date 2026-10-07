struct Globals {
    view_proj: mat4x4<f32>, inv_view_proj: mat4x4<f32>, fog_color: vec4<f32>,
    zenith_color: vec4<f32>, sun: vec4<f32>, params: vec4<f32>, clouds: vec4<f32>,
    environment: vec4<f32>, effects: vec4<f32>,
};
struct Axes { right: vec4<f32>, up: vec4<f32> };
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var block_sampler: sampler;
@group(2) @binding(0) var sprites: texture_2d_array<f32>;
@group(2) @binding(1) var sprite_sampler: sampler;
@group(2) @binding(2) var<uniform> axes: Axes;
struct Output {
    @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>, @location(2) @interpolate(flat) light: vec4<f32>,
    @location(3) dist: f32,
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
    out.color = color; out.light = light; out.dist = length(center);
    return out;
}
fn curve(l: f32) -> f32 { return l / (4. - 3. * l); }
@fragment fn fs_main(in: Output) -> @location(0) vec4<f32> {
    var texel: vec4<f32>;
    if in.light.w > 0.5 {
        texel = textureSample(blocks, block_sampler, in.uv, i32(in.light.z));
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
    var lit = max(vec3<f32>(sky), torch) * 0.96 + 0.04;
    let peak = max(max(lit.r, lit.g), max(lit.b, 0.001));
    lit = mix(lit, lit / peak, g.effects.x);
    let base = texel.rgb * pow(in.color.rgb, vec3<f32>(2.2)) * lit;
    let fog = clamp((in.dist - g.params.x) / (g.params.y - g.params.x), 0., 1.);
    return vec4<f32>(mix(base, g.fog_color.rgb, fog * fog * (3. - 2. * fog)), 1.);
}
