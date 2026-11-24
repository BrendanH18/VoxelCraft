// v0.18 Track 7: the title panorama. A full-screen triangle; each pixel's
// view direction picks one of the six faces (`voxelcraft::title::panorama`,
// north, east, south, west, up, down) and samples it at a fixed mip level,
// so face seams never pick up the derivative jump of a cube's edges.
// Screens over the panorama blur it: a disc of samples around the pixel,
// each from a softer mip level, standing in for Java's screen blur.

struct Panorama {
    inv_view_proj: mat4x4<f32>,
    // x: mip level, y: opacity, zw: the blur disc's radius in NDC (0: sharp)
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> p: Panorama;
@group(0) @binding(1) var faces: texture_2d_array<f32>;
@group(0) @binding(2) var faces_sampler: sampler;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_panorama(@builtin(vertex_index) i: u32) -> Out {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: Out;
    out.clip = vec4<f32>(xy, 0.0, 1.0);
    out.ndc = xy;
    return out;
}

// Mirrors `panorama::lookup`: the face of the largest axis, then the point
// on it in the face's right/up frame.
fn lookup(d: vec3<f32>) -> vec3<f32> {
    let a = abs(d);
    var face = 0u;
    var forward = vec3<f32>(0.0, 0.0, -1.0);
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 {
            face = 1u;
            forward = vec3<f32>(1.0, 0.0, 0.0);
        } else {
            face = 3u;
            forward = vec3<f32>(-1.0, 0.0, 0.0);
        }
    } else if a.y >= a.z {
        if d.y > 0.0 {
            face = 4u;
            forward = vec3<f32>(0.0, 1.0, 0.0);
            up = vec3<f32>(0.0, 0.0, 1.0);
        } else {
            face = 5u;
            forward = vec3<f32>(0.0, -1.0, 0.0);
            up = vec3<f32>(0.0, 0.0, -1.0);
        }
    } else if d.z > 0.0 {
        face = 2u;
        forward = vec3<f32>(0.0, 0.0, 1.0);
    }
    let right = cross(forward, up);
    let q = d / dot(d, forward);
    return vec3<f32>((dot(q, right) + 1.0) * 0.5, (1.0 - dot(q, up)) * 0.5, f32(face));
}

fn sample_at(ndc: vec2<f32>) -> vec3<f32> {
    let far = p.inv_view_proj * vec4<f32>(ndc, 0.5, 1.0);
    let hit = lookup(normalize(far.xyz / far.w));
    return textureSampleLevel(faces, faces_sampler, hit.xy, u32(hit.z), p.params.x).rgb;
}

// The centre, then two rings of six (the outer turned by 30 degrees).
const TAPS = array<vec2<f32>, 13>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.45, 0.0), vec2<f32>(0.225, 0.39), vec2<f32>(-0.225, 0.39),
    vec2<f32>(-0.45, 0.0), vec2<f32>(-0.225, -0.39), vec2<f32>(0.225, -0.39),
    vec2<f32>(0.779, 0.45), vec2<f32>(0.0, 0.9), vec2<f32>(-0.779, 0.45),
    vec2<f32>(-0.779, -0.45), vec2<f32>(0.0, -0.9), vec2<f32>(0.779, -0.45),
);

@fragment
fn fs_panorama(in: Out) -> @location(0) vec4<f32> {
    if p.params.z <= 0.0 {
        return vec4<f32>(sample_at(in.ndc), p.params.y);
    }
    var taps = TAPS;
    var sum = vec3<f32>(0.0);
    for (var i = 0u; i < 13u; i++) {
        sum += sample_at(in.ndc + taps[i] * p.params.zw);
    }
    return vec4<f32>(sum / 13.0, p.params.y);
}
