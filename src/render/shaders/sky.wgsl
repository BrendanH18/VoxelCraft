// Procedural sky: Java 26.3's sky colour overhead fogged toward the fog
// colour (`sky.fsh`), with VoxelCraft's own sun, moon, stars and glow.
// Clouds are their own pass (clouds.wgsl).

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    // Linear fog colour: the horizon, and what terrain fades into.
    fog_color: vec4<f32>,
    // Linear sky colour overhead (Java's `visual/sky_color`).
    zenith_color: vec4<f32>,
    // xyz: direction towards the sun, w: time in seconds.
    sun: vec4<f32>,
    // z: daylight, w: rain strength
    params: vec4<f32>,
    clouds: vec4<f32>,
    environment: vec4<f32>,
    effects: vec4<f32>,
    ambient: vec4<f32>,
    fog: vec4<f32>,
    // x: sky fog end.
    fog_ends: vec4<f32>,
    sky_light: vec4<f32>,
    // Linear sunrise/sunset colour and strength.
    sunrise: vec4<f32>,
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

// Mixes two linear colours as their sRGB values (2.2 power, matching the
// conversion the uniforms went through).
fn srgb_mix(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> {
    let e = vec3<f32>(1.0 / 2.2);
    return pow(mix(pow(max(a, vec3<f32>(0.0)), e), pow(max(b, vec3<f32>(0.0)), e), t), vec3<f32>(2.2));
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
    if g.environment.x > 0.5 {
        if g.environment.x > 1.5 {
            // A static, mottled violet sky; no sun, moon, stars or clouds.
            let cell = floor(dir * 180.0);
            let grain = hash3(cell);
            let haze = pow(1.0 - abs(dir.y), 3.0);
            let color = mix(g.zenith_color.rgb, g.fog_color.rgb, haze) * (0.8 + grain * 0.4);
            return vec4<f32>(color, 1.0);
        }
        return vec4<f32>(g.fog_color.rgb, 1.0);
    }
    let sun = normalize(g.sun.xyz);
    let daylight = g.params.z;

    // Java's sky is a plane 16 blocks overhead in the sky colour, fogged by
    // its distance up to the sky fog end (`sky.fsh`): spherical distance
    // linearly, horizontal distance as a step. Below the horizon there is
    // only the clear colour, the fog. Blended in sRGB, as Java does.
    var fogged = 1.0;
    if dir.y > 0.0 {
        let end = max(g.fog_ends.x, 1.0);
        let spherical = 16.0 / dir.y;
        let horizontal = spherical * sqrt(max(1.0 - dir.y * dir.y, 0.0));
        fogged = select(clamp(spherical / end, 0.0, 1.0), 1.0, horizontal >= end);
    }
    var color = srgb_mix(g.zenith_color.rgb, g.fog_color.rgb, fogged);

    // Java's sunrise/sunset glow (`visual/sunrise_sunset_color`) around the
    // sun's side of the horizon.
    let toward_sun = max(dot(normalize(vec3<f32>(dir.x, 0.0, dir.z) + vec3<f32>(0.0, 1e-4, 0.0)), normalize(vec3<f32>(sun.x, 0.0, sun.z))), 0.0);
    let horizon = 1.0 - smoothstep(0.0, 0.45, abs(dir.y));
    color = srgb_mix(color, g.sunrise.rgb, g.sunrise.w * pow(toward_sun, 3.0) * horizon);

    // Stars rotate with the sun and fade in at night.
    let rain = g.params.w;
    let night = (1.0 - smoothstep(0.15, 0.5, daylight)) * (1.0 - rain);
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

    // Square sun and moon, like Minecraft (hidden behind rain clouds).
    let clear = color;
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
    return vec4<f32>(mix(color, clear, rain), 1.0);
}
