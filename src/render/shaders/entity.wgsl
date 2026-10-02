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
};

@group(0) @binding(0) var<uniform> g: Globals;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // x: sky light, y: face shade, z: hurt, w: emissive
    @location(2) light: vec4<f32>,
    @location(3) dist: f32,
    @location(4) torch: f32,
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
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.color = color;
    out.light = light;
    out.dist = length(pos);
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Per-texel brightness noise gives the flat colours a pixel-art texture.
    let n = hash2(floor(in.uv) + 0.5) - 0.5;
    // Colours are authored in sRGB, like the block textures.
    var base = pow(in.color.rgb, vec3<f32>(2.2)) * (1.0 + n * in.color.a);

    // Sky light scaled by daylight, or warm torch light, whichever is brighter.
    let sky = vec3<f32>(curve(in.light.x) * g.params.z);
    let torch = curve(in.torch) * vec3<f32>(1.0, 0.86, 0.66);
    let lit = mix((max(sky, torch) * 0.96 + 0.04) * in.light.y, vec3<f32>(1.0), in.light.w);
    var c = base * lit;
    // Hurt: Minecraft-style red overlay.
    c = mix(c, vec3<f32>(0.8, 0.0, 0.0) * max(lit, vec3<f32>(0.25)), in.light.z * 0.6);
    return vec4<f32>(apply_fog(c, in.dist), 1.0);
}
