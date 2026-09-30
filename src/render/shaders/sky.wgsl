// Procedural sky (gradient, sun, moon, stars) and the cloud layer.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    // Horizon colour; also the fog colour so terrain fades into the sky.
    fog_color: vec4<f32>,
    zenith_color: vec4<f32>,
    // xyz: direction towards the sun, w: time in seconds.
    sun: vec4<f32>,
    // x: fog start, y: fog end, z: daylight, w: unused
    params: vec4<f32>,
    // xy: camera xz wrapped to the cloud pattern period, z: cloud plane y
    // relative to the camera, w: cloud draw radius.
    clouds: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn hash3(p: vec3<f32>) -> f32 {
    return hash2(p.xy + hash2(p.yz + p.x * 17.0) * 91.7);
}

// ---------------------------------------------------------------- sky

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    // Full-screen triangle at the far plane (depth 0 with reverse-Z), so the
    // depth test limits it to pixels no geometry covered.
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    let ndc = uv * 2.0 - 1.0;
    var out: SkyOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let far = g.inv_view_proj * vec4<f32>(in.ndc, 0.0001, 1.0);
    let dir = normalize(far.xyz / far.w);
    let sun = normalize(g.sun.xyz);
    let daylight = g.params.z;

    // Horizon-to-zenith gradient; below the horizon stays horizon-coloured.
    let up = clamp(dir.y, 0.0, 1.0);
    var color = mix(g.fog_color.rgb, g.zenith_color.rgb, pow(up, 0.6));

    // Warm glow around the sun near the horizon at dawn/dusk.
    let toward_sun = max(dot(dir, sun), 0.0);
    let low_sun = 1.0 - smoothstep(0.0, 0.45, abs(sun.y));
    let horizon = 1.0 - smoothstep(0.0, 0.5, abs(dir.y));
    color += vec3<f32>(1.0, 0.45, 0.15) * pow(toward_sun, 6.0) * low_sun * horizon * 0.7;

    // Stars rotate with the sun and fade in at night.
    let night = 1.0 - smoothstep(0.15, 0.5, daylight);
    if night > 0.0 && dir.y > -0.1 {
        // Rotate into a frame that turns with the sun (around Z).
        let c = sun.x;
        let s = sun.y;
        let sd = vec3<f32>(c * dir.x + s * dir.y, -s * dir.x + c * dir.y, dir.z);
        let cell = floor(sd * 220.0);
        let h = hash3(cell);
        if h > 0.9975 {
            let twinkle = 0.7 + 0.3 * sin(g.sun.w * 3.0 + h * 1000.0);
            color += vec3<f32>(night * twinkle * (h - 0.9975) * 400.0);
        }
    }

    // Square sun and moon, like Minecraft.
    let right = normalize(cross(sun, vec3<f32>(0.0, 0.0, 1.0)));
    let top = cross(right, sun);
    let ds = dot(dir, sun);
    if ds > 0.0 {
        let p = vec2<f32>(dot(dir, right), dot(dir, top)) / ds;
        let d = max(abs(p.x), abs(p.y));
        if d < 0.06 {
            color = mix(vec3<f32>(1.0, 0.95, 0.75), vec3<f32>(1.0, 1.0, 0.9), step(d, 0.045)) * 1.2;
        } else {
            color += vec3<f32>(1.0, 0.8, 0.5) * pow(ds, 200.0) * 0.4;
        }
    } else {
        let p = vec2<f32>(dot(dir, right), dot(dir, top)) / -ds;
        let d = max(abs(p.x), abs(p.y));
        if d < 0.045 {
            // Moon with a few darker craters.
            let cell = floor((p + 0.045) / 0.015);
            let crater = hash2(cell + 7.0) > 0.7;
            color = select(vec3<f32>(0.85, 0.87, 0.92), vec3<f32>(0.6, 0.62, 0.7), crater);
        }
    }
    return vec4<f32>(color, 1.0);
}

// ------------------------------------------------------------- clouds

struct CloudOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
};

@vertex
fn vs_clouds(@builtin(vertex_index) i: u32) -> CloudOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let xz = corners[i] * g.clouds.w;
    let rel = vec3<f32>(xz.x, g.clouds.z, xz.y);
    var out: CloudOut;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    out.rel = rel;
    return out;
}

const CLOUD_CELL: f32 = 12.0;

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn fs_clouds(in: CloudOut) -> @location(0) vec4<f32> {
    let drift = vec2<f32>(g.sun.w * 1.2, 0.0);
    let world = in.rel.xz + g.clouds.xy + drift;
    // Evaluate coverage once per cell for crisp blocky edges.
    let cell = floor(world / CLOUD_CELL);
    let n = value_noise(cell / 5.0) * 0.65 + value_noise(cell / 2.0) * 0.35;
    if n < 0.58 {
        discard;
    }
    let dist = length(in.rel.xz);
    let fade = 1.0 - smoothstep(g.clouds.w * 0.6, g.clouds.w, dist);
    // Daylight bottoms out at 0.12 (moonlight); map that to near-black clouds.
    let k = clamp((g.params.z - 0.12) / 0.88, 0.0, 1.0);
    let lit = mix(vec3<f32>(0.012, 0.014, 0.025), vec3<f32>(1.0, 1.0, 1.0), k * k);
    let color = mix(lit, g.fog_color.rgb, smoothstep(0.0, g.clouds.w, dist) * 0.6);
    return vec4<f32>(color, 0.8 * fade);
}
