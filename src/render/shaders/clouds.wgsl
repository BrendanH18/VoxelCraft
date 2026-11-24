// Clouds (v0.18 Track 8): one packed face per instance, expanded to a
// 12 x 4 x 12 cell face. Java 26.3's `clouds.vsh` shades faces by
// direction and fades them out with distance (`clouds.fsh`); the colour is
// the environment's cloud colour, an sRGB value like Java's.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    zenith_color: vec4<f32>,
    sun: vec4<f32>,
    params: vec4<f32>,
    // xyz: the layer's offset from the camera (-x in cell, base y, -z in cell).
    clouds: vec4<f32>,
    environment: vec4<f32>,
    effects: vec4<f32>,
    ambient: vec4<f32>,
    fog: vec4<f32>,
    // x: sky fog end, y: cloud fog end.
    fog_ends: vec4<f32>,
    sky_light: vec4<f32>,
    sunrise: vec4<f32>,
    // sRGB cloud colour and alpha.
    cloud_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) dist: f32,
};

const CELL: vec3<f32> = vec3<f32>(12.0, 4.0, 12.0);
// `clouds.vsh`'s face colours: down, up, north, south, west, east.
const SHADES = array<f32, 6>(0.7, 1.0, 0.8, 0.8, 0.9, 0.9);

// Unit-cube corners of face `dir`, counter-clockwise seen from outside.
fn corner(dir: u32, i: u32) -> vec3<f32> {
    // Two triangles: 0 1 2, 2 3 0.
    let k = array<u32, 6>(0u, 1u, 2u, 2u, 3u, 0u)[i];
    let a = f32(k == 1u || k == 2u);
    let b = f32(k >= 2u);
    switch dir {
        case 0u: { return vec3<f32>(a, 0.0, b); }
        case 1u: { return vec3<f32>(b, 1.0, a); }
        case 2u: { return vec3<f32>(b, a, 0.0); }
        case 3u: { return vec3<f32>(a, b, 1.0); }
        case 4u: { return vec3<f32>(0.0, b, a); }
        default: { return vec3<f32>(1.0, a, b); }
    }
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @location(0) face: u32) -> VsOut {
    let x = f32(i32(face & 1023u) - 512);
    let z = f32(i32((face >> 10u) & 1023u) - 512);
    let dir = (face >> 20u) & 7u;
    let inside = (face & (1u << 23u)) != 0u;
    let top_colour = (face & (1u << 24u)) != 0u;
    // Inside faces wind the other way so they face into the cell.
    let i = select(vi, 5u - vi, inside);
    let pos = corner(dir, i) * CELL + vec3<f32>(x, 0.0, z) * CELL + g.clouds.xyz;
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    let shade = select(SHADES[dir], 1.0, top_colour);
    out.color = vec4<f32>(g.cloud_color.rgb * shade, g.cloud_color.a);
    out.dist = length(pos);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let fade = 1.0 - clamp(in.dist / max(g.fog_ends.y, 1e-3), 0.0, 1.0);
    // The colour is sRGB like Java's; the target encodes linear values.
    let c = max(in.color.rgb, vec3<f32>(0.0));
    let lin = select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
    return vec4<f32>(lin, in.color.a * fade);
}
