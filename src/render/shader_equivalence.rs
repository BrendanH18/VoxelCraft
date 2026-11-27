//! v1.0 Track 2: the chunk shader's cheaper lighting and fog give the same
//! pixels. These tests mirror `shaders/chunk.wgsl` in `f32` (the mirror is
//! checked against the shader's text) and compare the v0.18 per-fragment
//! form with the v1.0 one, byte for byte on the sRGB target:
//!
//! - **Lightmap.** v0.18 ran `overworld_light` (three `pow`s) on every
//!   fragment. When a quad's four corners carry the same light, v1.0 runs it
//!   once per vertex and the fragment only multiplies by the corner shade.
//! - **Fog colour.** v0.18 encoded the constant fog colour to sRGB on every
//!   fragment (three `pow`s); v1.0 encodes it once per frame on the CPU
//!   (`Globals::fog_srgb`), and returns the fog colour itself where the fog
//!   is total.

use glam::Vec3;
use voxelcraft::atmosphere;

const SHADER: &str = include_str!("shaders/chunk.wgsl");

/// The uniforms the lighting reads.
#[derive(Clone, Copy)]
struct Uniforms {
    /// `g.params.z` (daylight) and `g.params.w` (rain).
    daylight: f32,
    rain: f32,
    sun: Vec3,
    enhanced: bool,
    sky_light: Vec3,
    ambient: Vec3,
    night_vision: f32,
    brightness: f32,
    /// `g.effects.z`: Darkness's lightmap scale.
    darkness: f32,
}

fn to_srgb(c: Vec3) -> Vec3 {
    let f = |x: f32| {
        let x = x.max(0.0);
        if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
    };
    Vec3::new(f(c.x), f(c.y), f(c.z))
}

fn from_srgb(c: Vec3) -> Vec3 {
    let f = |x: f32| {
        let x = x.max(0.0);
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    Vec3::new(f(c.x), f(c.y), f(c.z))
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn brightness(u: &Uniforms, lit: Vec3) -> Vec3 {
    let c = lit.clamp(Vec3::ZERO, Vec3::ONE);
    let peak = c.x.max(c.y).max(c.z.max(1e-4));
    let inverted = 1.0 - peak;
    let curved = c * ((1.0 - inverted * inverted * inverted * inverted) / peak);
    c.lerp(curved, u.brightness)
}

fn night_vision(u: &Uniforms, lit: Vec3, raw: Vec3) -> Vec3 {
    let peak = lit.x.max(lit.y).max(lit.z.max(1e-3));
    let over = (raw - Vec3::ONE).max(Vec3::ZERO);
    brightness(u, lit.lerp(lit / peak, u.night_vision) + over - Vec3::splat(u.darkness))
}

fn daylight_tint(u: &Uniforms, normal: Vec3) -> Vec3 {
    if !u.enhanced {
        return Vec3::ONE;
    }
    let moon = u.sun.y < 0.0;
    let dir = if moon { -u.sun } else { u.sun };
    let dusk = 1.0 - smoothstep(0.05, 0.45, dir.y.abs());
    let warm = Vec3::ONE.lerp(Vec3::new(1.12, 0.80, 0.58), dusk);
    let tint = if moon { Vec3::ONE } else { warm };
    let diffuse = normal.dot(dir).max(0.0);
    (tint * (0.62 + 0.38 * diffuse)).lerp(Vec3::splat(0.85), u.rain)
}

fn overworld_light(u: &Uniforms, sky_level: f32, block_level: f32, normal: Vec3, shade: f32) -> Vec3 {
    let (lit, sun) = overworld_parts(u, sky_level, block_level, normal);
    lit * shade * sun
}

fn overworld_parts(u: &Uniforms, sky_level: f32, block_level: f32, normal: Vec3) -> (Vec3, Vec3) {
    let sky = (sky_level / (4.0 - 3.0 * sky_level)) * u.daylight * u.sky_light;
    let p = (2.0 * block_level - 1.0) * (2.0 * block_level - 1.0);
    let hue = Vec3::new(1.0, 0.847, 0.549).lerp(Vec3::ONE, 0.9 * p);
    let block = hue * (block_level / (4.0 - 3.0 * block_level)) * 1.4;
    let raw = u.ambient + sky + block;
    let java = raw.clamp(Vec3::ZERO, Vec3::ONE);
    let s = sky.x.max(sky.y).max(sky.z);
    let b = block.x.max(block.y).max(block.z);
    let share = (s / (s + b).max(1e-4)).clamp(0.0, 1.0);
    let sun = Vec3::ONE.lerp(daylight_tint(u, normal), share);
    (from_srgb(night_vision(u, java, raw)), sun)
}

/// What the sRGB target stores, as bytes.
fn bytes(c: Vec3) -> [u8; 3] {
    to_srgb(c).to_array().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The shader lines the mirror copies, so a change to one shows up here.
#[test]
fn the_mirror_follows_the_shader() {
    for line in [
        "let sky = (sky_level / (4.0 - 3.0 * sky_level)) * g.params.z * g.sky_light.rgb;",
        "let hue = mix(vec3<f32>(1.0, 0.847, 0.549), vec3<f32>(1.0), 0.9 * p);",
        "let block = hue * (block_level / (4.0 - 3.0 * block_level)) * 1.4;",
        "let share = clamp(s / max(s + b, 1e-4), 0.0, 1.0);",
        "return o.lit * shade * o.sun;",
        "return OverworldLight(from_srgb(night_vision(java, raw)), sun);",
        "let java = clamp(raw, vec3<f32>(0.0), vec3<f32>(1.0));",
        "let over = max(raw - vec3<f32>(1.0), vec3<f32>(0.0));",
        "return brightness(mix(lit, lit / peak, g.effects.x) + over - vec3<f32>(g.effects.z));",
        "return mix(tint * (0.62 + 0.38 * diffuse), vec3<f32>(0.85), g.params.w);",
        "let uniform_light = w2 == (w2 & 255u) * 0x01010101u;",
        "let o = overworld_parts(out.light.x, out.light.y, out.normal);",
        "return in.lightmap.rgb * in.shade * in.sun_tint;",
        "if f >= 1.0 { return g.fog_color.rgb; }",
        "return from_srgb(mix(to_srgb(color), g.fog_srgb.rgb, f));",
    ] {
        assert!(SHADER.contains(line), "chunk.wgsl no longer has: {line}");
    }
}

fn settings() -> Vec<Uniforms> {
    let mut out = Vec::new();
    let suns = [Vec3::new(0.3, 0.9, 0.2).normalize(), Vec3::new(0.9, 0.12, 0.1).normalize(), Vec3::new(0.2, -0.8, 0.1)];
    for (daylight, sky_light) in
        [(1.0, Vec3::ONE), (0.24, Vec3::new(0.478, 0.478, 1.0)), (0.6, Vec3::new(0.8, 0.8, 1.0))]
    {
        for sun in suns {
            for (rain, enhanced) in [(0.0, true), (1.0, true), (0.0, false)] {
                for (night_vision, brightness, darkness) in
                    [(0.0, 0.0, 0.0), (0.0, 0.5, 0.0), (1.0, 1.0, 0.0), (0.4, 0.0, 0.0), (0.0, 0.2, 0.3)]
                {
                    out.push(Uniforms {
                        daylight,
                        rain,
                        sun,
                        enhanced,
                        sky_light,
                        ambient: Vec3::splat(10.0 / 255.0),
                        night_vision,
                        brightness,
                        darkness,
                    });
                }
            }
        }
    }
    out
}

/// A quad whose corners all carry light byte `light`: the vertex form, then
/// the interpolated corner shade, against the fragment form at the
/// interpolated light, at many points of the quad, for every light byte,
/// normal and lighting setting.
///
/// How the hardware interpolates four equal corner values decides whether
/// the old form saw exactly the corner value. Interpolating as
/// `v0 + w1 (v1 - v0) + w2 (v2 - v0)` gives it exactly, and then the two
/// forms agree bit for bit. Summing `v w0 + v w1 + v w2` can be off by an
/// ulp, which moved the old form's output off its exact value; against
/// that the new form differs in a handful of rounding ties, by one byte.
#[test]
fn uniformly_lit_quads_light_the_same_per_vertex() {
    let normals = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z, Vec3::new(-0.707, 0.0, 0.707)];
    let shades = [0.8 * 0.42, 0.55 * 0.62, 1.0 * 0.82, 0.68, 1.0];
    // (fragments, differing exactly-interpolated, differing summed, largest summed difference)
    let (mut compared, mut exact_differing, mut summed_differing, mut worst) = (0u64, 0u64, 0u64, 0f32);
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 40) as f32 / (1u64 << 24) as f32
    };
    for u in settings() {
        for light in 0..=255u32 {
            let corner = Vec3::new((light & 15) as f32 / 15.0, (light >> 4) as f32 / 15.0, 0.0);
            for normal in normals {
                let (lit, sun) = overworld_parts(&u, corner.x, corner.y, normal);
                for _ in 0..4 {
                    let (a, b) = (next(), next());
                    let (w0, w1) = (a.min(b), (a.max(b) - a.min(b)));
                    let w2 = 1.0 - w0 - w1;
                    let (s0, s1, s2) = (shades[light as usize % 5], shades[(light as usize + 1) % 5], 1.0);
                    let shade = s0 + w1 * (s1 - s0) + w2 * (s2 - s0);
                    let new = lit * shade * sun;
                    // Edge-function interpolation of equal values: the corner value.
                    let exact = overworld_light(&u, corner.x, corner.y, normal, shade);
                    // Barycentric sums.
                    let level = corner.x * w0 + corner.x * w1 + corner.x * w2;
                    let block = corner.y * w0 + corner.y * w1 + corner.y * w2;
                    let summed = overworld_light(&u, level, block, normal, shade);
                    compared += 1;
                    exact_differing += u64::from(exact != new);
                    worst = worst.max((summed - new).abs().max_element());
                    summed_differing += u64::from(bytes(summed) != bytes(new));
                }
            }
        }
    }
    eprintln!(
        "lightmap: {compared} fragments; exactly interpolated: {exact_differing} differ in any bit; \
         summed: {summed_differing} with another byte, largest linear difference {worst:e}"
    );
    assert_eq!(exact_differing, 0, "{exact_differing} of {compared} fragments changed");
    assert!(summed_differing * 10_000 < compared, "{summed_differing} of {compared}");
    assert!(worst < 1e-5, "{worst}");
}

/// The fog blend with the colour encoded once on the CPU, and the total
/// fog's shortcut, against the per-fragment encode, over fog colours the
/// atmosphere produces, surface colours and fog amounts.
#[test]
fn the_fog_blend_is_the_same_with_the_fog_colour_encoded_once() {
    let (mut compared, mut differing, mut worst) = (0u64, 0u64, 0f32);
    let mut worst_encode = 0f32;
    for fr in (0..=255).step_by(15) {
        for fg in (0..=255).step_by(17) {
            for fb in (0..=255).step_by(51) {
                // The game hands the renderer the fog colour in linear.
                let srgb = [fr as f32 / 255.0, fg as f32 / 255.0, fb as f32 / 255.0];
                let fog = Vec3::from(atmosphere::linear(srgb));
                let encoded = Vec3::from(atmosphere::encode(fog.to_array()));
                worst_encode = worst_encode.max((encoded - to_srgb(fog)).abs().max_element());
                for c in [0.0, 0.002, 0.03, 0.2, 0.55, 0.9, 1.0] {
                    let color = Vec3::new(c, (c * 0.7 + 0.1).min(1.0), c * c);
                    for f in [0.001, 0.05, 0.25, 0.5, 0.8, 0.999, 1.0] {
                        let old = from_srgb(to_srgb(color).lerp(to_srgb(fog), f));
                        let new = if f >= 1.0 { fog } else { from_srgb(to_srgb(color).lerp(encoded, f)) };
                        compared += 1;
                        worst = worst.max((old - new).abs().max_element());
                        differing += u64::from(bytes(old) != bytes(new));
                    }
                }
            }
        }
    }
    eprintln!(
        "fog: {compared} fragments, {differing} with another byte, largest linear difference {worst:e}, \
         largest encode difference {worst_encode:e}"
    );
    assert_eq!(differing, 0, "{differing} of {compared} fragments changed (largest difference {worst:e})");
    assert!(worst < 1e-5 && worst_encode < 1e-6, "{worst} {worst_encode}");
}

/// Darkness against Java 26.3's `lightmap.fsh`: without night vision the
/// mirror is `clamp(ambient + sky + block - DarknessScale)` moved toward
/// `notGamma` by the Brightness the CPU lowered, so dim light goes black at
/// the pulse's peak while light well above full (daylight with a torch)
/// stays bright; with no Darkness every light is as before.
#[test]
fn darkness_comes_off_the_lightmap_before_its_clamp() {
    let not_gamma = |c: Vec3| {
        let peak = c.max_element();
        let inv = 1.0 - peak;
        c * ((1.0 - inv * inv * inv * inv) / peak)
    };
    let base = Uniforms {
        daylight: 1.0,
        rain: 0.0,
        sun: Vec3::Y,
        enhanced: false,
        sky_light: Vec3::ONE,
        ambient: Vec3::ZERO,
        night_vision: 0.0,
        brightness: 0.5,
        darkness: 0.0,
    };
    for (daylight, scale, gamma) in [(1.0, 0.45, 0.0), (0.24, 0.3, 0.5), (0.6, 0.12, 0.2), (1.0, 0.0, 0.5)] {
        let u = Uniforms { daylight, darkness: scale, brightness: gamma, ..base };
        for sky in 0..16 {
            for block in 0..16 {
                let (s, b) = (sky as f32 / 15.0, block as f32 / 15.0);
                let (lit, _) = overworld_parts(&u, s, b, Vec3::Y);
                // lightmap.fsh, in Java's own order.
                let sky_light = (s / (4.0 - 3.0 * s)) * daylight * u.sky_light;
                let p = (2.0 * b - 1.0) * (2.0 * b - 1.0);
                let tint = Vec3::new(1.0, 0.847, 0.549).lerp(Vec3::ONE, 0.9 * p);
                let color = (u.ambient + sky_light + tint * (b / (4.0 - 3.0 * b)) * 1.4 - Vec3::splat(scale))
                    .clamp(Vec3::ZERO, Vec3::ONE);
                let java = if color.max_element() > 0.0 { color.lerp(not_gamma(color), gamma) } else { color };
                assert!(
                    (lit - from_srgb(java)).abs().max_element() < 1e-5,
                    "sky {sky} block {block} scale {scale}: {lit} against {}",
                    from_srgb(java)
                );
            }
        }
    }
    // At the peak, dark corners go black and daylight beside a torch doesn't.
    let peak = Uniforms { darkness: 0.45, brightness: 0.0, ..base };
    assert_eq!(overworld_parts(&peak, 4.0 / 15.0, 0.0, Vec3::Y).0, Vec3::ZERO);
    assert_eq!(overworld_parts(&peak, 1.0, 1.0, Vec3::Y).0, Vec3::ONE);
}
