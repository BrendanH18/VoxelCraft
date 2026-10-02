// Rain and snow sheets. Each sheet is one column of falling weather; the
// streaks and flakes are drawn procedurally, on a 16-texel grid so they
// match the blocky art.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    zenith_color: vec4<f32>,
    sun: vec4<f32>,
    // x: fog start, y: fog end, z: daylight, w: rain strength
    params: vec4<f32>,
    clouds: vec4<f32>,
    environment: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) seed: f32,
    // x: snow, y: sky light, z: opacity
    @location(2) params: vec4<f32>,
    @location(3) dist: f32,
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) seed: f32,
    @location(3) params: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.seed = seed;
    out.params = params;
    out.dist = length(pos);
    return out;
}

fn hash1(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

fn curve(l: f32) -> f32 {
    return l / (4.0 - 3.0 * l);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = g.sun.w;
    let snow = in.params.x > 0.5;
    // Four lanes of drops per sheet, each with its own speed and phase.
    let lanes = 4.0;
    let lane = floor(in.uv.x * lanes);
    let h = hash1(in.seed * 13.7 + lane * 3.1);
    // Texel position within the lane (16 texels per block, 4 per lane).
    let tx = floor(fract(in.uv.x * lanes) * 4.0);
    var alpha = 0.0;
    var color = vec3<f32>(0.0);
    if snow {
        // Flakes drift down slowly and sway from side to side.
        let fall = in.uv.y + t * (1.2 + h * 0.6) + h * 9.0;
        let cell = floor(fall * 2.0);
        let ch = hash1(cell * 7.3 + in.seed + lane * 11.0);
        let sway = floor((sin(t * 1.5 + ch * 6.28) * 0.5 + 0.5) * 3.0);
        let ty = floor(fract(fall * 2.0) * 8.0);
        if ch > 0.65 && tx == sway && ty == 3.0 {
            alpha = 0.9;
            color = vec3<f32>(0.95, 0.97, 1.0);
        }
    } else {
        // Long thin streaks falling fast.
        let fall = (in.uv.y + t * (11.0 + h * 4.0)) * 0.5 + h * 7.0;
        let cell = floor(fall);
        let ch = hash1(cell * 5.9 + in.seed + lane * 17.0);
        let ty = fract(fall);
        if ch > 0.55 && tx == floor(fract(ch * 7.0) * 4.0) && ty < 0.3 {
            alpha = 0.45 * (1.0 - ty / 0.3 * 0.5);
            color = vec3<f32>(0.62, 0.7, 0.85);
        }
    }
    // Thin out right in front of the eyes.
    alpha *= smoothstep(0.8, 3.0, in.dist);
    if alpha <= 0.0 {
        discard;
    }
    let light = curve(in.params.y) * g.params.z * 0.9 + 0.1;
    var c = pow(color, vec3<f32>(2.2)) * light;
    let f = clamp((in.dist - g.params.x) / (g.params.y - g.params.x), 0.0, 1.0);
    c = mix(c, g.fog_color.rgb, f);
    return vec4<f32>(c, alpha * in.params.z);
}
