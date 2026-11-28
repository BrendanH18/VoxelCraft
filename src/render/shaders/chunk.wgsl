// Chunk terrain shader. Quads are 12-byte records in a storage buffer (see
// src/mesh.rs) that the vertex shader expands into corners (vertex
// pulling): with the shared quad index buffer and base_vertex = first quad
// * 4, vertex_index / 4 is the quad and vertex_index % 4 the corner. The
// per-draw instance attribute is the chunk origin relative to the camera.

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
    // x: night vision strength 0..1, y: Brightness (less Darkness's blend),
    // z: Darkness's pulsing lightmap scale.
    effects: vec4<f32>,
    // rgb: visual/ambient_light_color; w: use the skyless dimension lightmap.
    ambient: vec4<f32>,
    // Java's fog: environmental start/end (spherical), render-distance start/end (cylindrical).
    fog: vec4<f32>,
    // x: sky fog end, y: cloud fog end.
    fog_ends: vec4<f32>,
    // rgb: sRGB visual/sky_light_color (Java's gamma-space lightmap).
    sky_light: vec4<f32>,
    sunrise: vec4<f32>,
    cloud_color: vec4<f32>,
    // rgb: the fog colour, sRGB-encoded on the CPU (to_srgb(fog_color)).
    fog_srgb: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

// The exact sRGB transfer the target encodes with, so a colour taken
// from linear to sRGB and back is unchanged and a fully fogged pixel
// shows Java's fog colour byte for byte.
fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}
fn from_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
}
// Java 26.3's fog.glsl: spherical and cylindrical distance, and the larger
// of the environmental and render-distance fogs, blended in sRGB as Java
// blends it (exact sRGB, matching the fog colour's conversion).
fn fog_distances(rel: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(length(rel), max(length(rel.xz), abs(rel.y)));
}
fn linear_fog(d: f32, start: f32, end: f32) -> f32 {
    if d <= start { return 0.0; }
    if d >= end { return 1.0; }
    return (d - start) / (end - start);
}
fn fog_value(d: vec2<f32>) -> f32 {
    return max(linear_fog(d.x, g.fog.x, g.fog.y), linear_fog(d.y, g.fog.z, g.fog.w));
}
// v1.0 Track 2: the fog colour's encode is a per-frame constant (g.fog_srgb),
// and a fully fogged fragment is the fog colour itself.
fn apply_fog(color: vec3<f32>, d: vec2<f32>) -> vec3<f32> {
    let f = fog_value(d);
    if f <= 0.0 { return color; }
    if f >= 1.0 { return g.fog_color.rgb; }
    return from_srgb(mix(to_srgb(color), g.fog_srgb.rgb, f));
}

// v1.0 gap Track 5c: lightmap.fsh's ambient term, added before sky and
// block light: the dimension's `visual/ambient_light_color`, or Night
// Vision's `visual/night_vision_color` (#999999, in every dimension) times
// its strength (g.effects.x), whichever is brighter.
const NIGHT_VISION_COLOR: f32 = 153.0 / 255.0;
fn ambient_light() -> vec3<f32> {
    return max(g.ambient.rgb, vec3<f32>(NIGHT_VISION_COLOR * g.effects.x));
}
// The lightmap of an unclamped light sum, as lightmap.fsh finishes it:
// Darkness's pulsing `DarknessScale` (g.effects.z) comes off before the
// clamp, then the Brightness option (lowered by Darkness's blend on the
// CPU) clamps and curves it.
fn lightmap(raw: vec3<f32>) -> vec3<f32> {
    return brightness(raw - vec3<f32>(g.effects.z));
}

// The Brightness option (Java's gamma, g.effects.y): lightmap.fsh moves each
// clamped light level toward its `notGamma` curve, which lifts dark levels.
fn brightness(lit: vec3<f32>) -> vec3<f32> {
    let c = clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0));
    let peak = max(max(c.r, c.g), max(c.b, 1e-4));
    let inverted = 1.0 - peak;
    let curved = c * ((1.0 - inverted * inverted * inverted * inverted) / peak);
    return mix(c, curved, g.effects.y);
}
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
const PAGE_SHIFT: u32 = 8u;
fn sample_block(uv: vec2<f32>, layer: u32) -> vec4<f32> {
    if !BLOCK_PAGING { return textureSample(blocks, blocks_sampler, uv, layer); }
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let local = layer & ((1u << PAGE_SHIFT) - 1u);
    switch layer >> PAGE_SHIFT {
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


@group(2) @binding(0) var<storage, read> quads: array<u32>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) shade: f32,
    // Spherical and cylindrical distance, for the fog.
    @location(3) dist: vec2<f32>,
    // x: sky light, y: block light (0..1), interpolated for smooth lighting.
    @location(4) light: vec2<f32>,
    @location(5) rel: vec3<f32>,
    @location(6) @interpolate(flat) normal: vec3<f32>,
    // v1.0 Track 2: rgb is the Overworld's converted lightmap and `sun_tint`
    // its sun-facing tint (`overworld_parts`) when all four corners of the
    // quad carry the same light, so both are the same at every fragment; w
    // is 1 then, else 0 and the fragment works them out from the
    // interpolated light.
    @location(7) @interpolate(flat) lightmap: vec4<f32>,
    @location(8) @interpolate(flat) sun_tint: vec3<f32>,
};

// Reserved detail coordinate 31 is the half-sixteenth frogspawn height.
fn detail_coord(value:u32)->f32 {
    return select(f32(value),1.5,value==31u)/16.0;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @location(1) offset: vec3<f32>) -> VsOut {
    let q = (vi >> 2u) * 3u;
    let w0 = quads[q];
    let w1 = quads[q + 1u];
    let w2 = quads[q + 2u];
    let face = (w0 >> 18u) & 7u;
    let size = vec2<u32>(((w0 >> 21u) & 31u) + 1u, ((w0 >> 26u) & 31u) + 1u);

    // Corners are (0,0), (w,0), (w,h), (0,h) in the face's (u, v) plane.
    // Negative faces run in reverse for winding; `flip` rotates by one so
    // the index pattern's diagonal becomes the other one.
    var order = array<u32, 4>(0u, 1u, 2u, 3u);
    if (face & 1u) == 1u {
        order = array<u32, 4>(0u, 3u, 2u, 1u);
    }
    let c = order[((vi & 3u) + (w0 >> 31u)) & 3u];
    let ao = (w1 >> (8u + 2u * c)) & 3u;
    let light = (w2 >> (8u * c)) & 255u;
    let detail = (w1 & 0x80000000u) != 0u;
    let coord_mask = select(63u, 31u, detail);
    let base = vec3<u32>(w0 & coord_mask, (w0 >> 6u) & coord_mask, (w0 >> 12u) & coord_mask);

    var local: vec3<f32>;
    var uv: vec2<f32>;
    if face >= 6u {
        // Cross-shaped block: a unit diagonal plane through the cell, (0,0)
        // to (1,1) in xz for face 6 and (1,0) to (0,1) for face 7.
        let t = select(0u, 1u, c == 1u || c == 2u);
        let up = select(0u, 1u, c >= 2u);
        let across = select(t, 1u - t, face == 7u);
        // Bits 21-23: a cross on its side (wall amethyst buds), along x
        // (1) or z (2), pointing the negative way with bit 23.
        let axis = (w0 >> 21u) & 3u;
        let along = select(up, 1u - up, ((w0 >> 23u) & 1u) == 1u);
        var corner = base + vec3<u32>(across, up, t);
        if axis == 1u {
            corner = base + vec3<u32>(along, across, t);
        } else if axis == 2u {
            corner = base + vec3<u32>(across, t, along);
        }
        local = vec3<f32>(corner);
        uv = vec2<f32>(f32(t), f32(1u - up));
    } else {
        let d = face >> 1u;
        let u = (d + 1u) % 3u;
        let v = (d + 2u) % 3u;
        if (w1 >> 31u) == 1u {
            // Detail quad (shaped blocks): part of the cell `base`, bounds
            // and plane offset in 1/16 block.
            let tag = (w0 >> 21u) & 31u;
            let slope = face >= 2u && face <= 3u && tag >= 17u && tag <= 20u;
            let lo = vec2<u32>(select(tag, 0u, slope), (w0 >> 26u) & 15u);
            let hi = vec2<u32>((w1 >> 16u) & 31u, (w1 >> 21u) & 31u);
            var p = vec3<f32>(base);
            p[d] += detail_coord((w1 >> 26u) & 31u);
            p[u] += detail_coord(select(lo.x, hi.x, c == 1u || c == 2u));
            p[v] += detail_coord(select(lo.y, hi.y, c >= 2u));
            if slope {
                switch tag {
                    case 17u: { p.y += p.x - f32(base.x); }
                    case 18u: { p.y += 1.0 - (p.x - f32(base.x)); }
                    case 19u: { p.y += 1.0 - (p.z - f32(base.z)); }
                    default: { p.y += p.z - f32(base.z); }
                }
            }
            local = p;
        } else {
            var corner = base;
            if c == 1u || c == 2u {
                corner[u] += size.x;
            }
            if c >= 2u {
                corner[v] += size.y;
            }

            // Water surfaces: lower the quad's upper edge (never the bottom face).
            var top = base.y;
            if u == 1u {
                top += size.x;
            } else if v == 1u {
                top += size.y;
            }
            var drop = 0.0;
            if face != 3u && corner.y == top {
                drop = f32((w1 >> 16u) & 31u) / 16.0;
            }
            local = vec3<f32>(corner) - vec3<f32>(0.0, drop, 0.0);
        }

        // Planar UVs from the chunk-local position: merged quads tile the texture.
        if face < 2u {
            uv = vec2<f32>(local.z, -local.y);
        } else if face < 4u {
            uv = local.xz;
        } else {
            uv = vec2<f32>(local.x, -local.y);
        }
    }

    var face_shade = array<f32, 8>(0.8, 0.8, 1.0, 0.55, 0.68, 0.68, 0.9, 0.9);
    var ao_curve = array<f32, 4>(0.42, 0.62, 0.82, 1.0);

    let rel = offset + local;
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(rel, 1.0);
    out.uv = uv;
    let layer_high = select((w1 >> 21u) & 15u,
        ((w0 >> 5u) & 1u) | (((w0 >> 11u) & 1u) << 1u) | (((w0 >> 17u) & 1u) << 2u) | (((w0 >> 30u) & 1u) << 3u),
        detail);
    out.layer = (w1 & 255u) | (layer_high << 8u);
    out.shade = face_shade[face] * ao_curve[ao];
    out.dist = fog_distances(rel);
    out.rel = rel;
    var normals = array<vec3<f32>, 8>(
        vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(-1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, -1.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, -1.0),
        vec3<f32>(-0.707, 0.0, 0.707), vec3<f32>(0.707, 0.0, 0.707));
    out.normal = normals[face];
    out.light = vec2<f32>(f32(light & 15u), f32(light >> 4u)) / 15.0;
    // Four equal corner bytes: the light is constant across the quad.
    let uniform_light = w2 == (w2 & 255u) * 0x01010101u;
    out.lightmap = vec4<f32>(0.0);
    out.sun_tint = vec3<f32>(1.0);
    if uniform_light && g.ambient.w <= 0.0 {
        let o = overworld_parts(out.light.x, out.light.y, out.normal);
        out.lightmap = vec4<f32>(o.lit, 1.0);
        out.sun_tint = o.sun;
    }
    return out;
}

// Lightmap brightness curve (Java 26.3 lightmap.fsh).
fn curve(l: vec2<f32>) -> vec2<f32> {
    return l / (4.0 - 3.0 * l);
}

fn lighting(in: VsOut) -> vec3<f32> {
    if in.lightmap.w > 0.5 {
        return in.lightmap.rgb * in.shade * in.sun_tint;
    }
    let l = curve(in.light);
    let sky = l.x * g.params.z * daylight_tint(in.normal);
    if g.ambient.w > 0.0 {
        let torch = l.y * vec3<f32>(1.0, 0.86, 0.66);
        return lightmap(dimension_light(sky, torch)) * in.shade;
    }
    return overworld_light(in.light.x, in.light.y, in.normal, in.shade);
}


// Directional skylight; caves retain their block lighting and dimensions
// without a sun keep their steady ambient illumination.
fn daylight_tint(normal: vec3<f32>) -> vec3<f32> {
    if g.environment.x > 0.5 {
        return vec3<f32>(1.0);
    }
    if g.environment.y < 0.5 {
        return vec3<f32>(1.0);
    }
    let moon = g.sun.y < 0.0;
    let dir = select(g.sun.xyz, -g.sun.xyz, moon);
    let elevation = abs(dir.y);
    let dusk = 1.0 - smoothstep(0.05, 0.45, elevation);
    let warm = mix(vec3<f32>(1.0), vec3<f32>(1.12, 0.80, 0.58), dusk);
    // Moonlight takes its blue from `visual/sky_light_color` (overworld_light).
    let tint = select(warm, vec3<f32>(1.0), moon);
    let diffuse = max(dot(normal, dir), 0.0);
    return mix(tint * (0.62 + 0.38 * diffuse), vec3<f32>(0.85), g.params.w);
}

// v0.18 Track 8: Java 26.3's lightmap (`lightmap.fsh`) for the Overworld,
// in Java's gamma space: the dimension's ambient colour, plus sky light
// (the brightness curve times `visual/sky_light_factor` and
// `visual/sky_light_color`, blue at night), plus block light
// (`visual/block_light_tint` #FFD88C mixed toward white by
// 0.9 * (2l - 1)^2, times the curve and Java's block factor 1.4), clamped,
// with night vision and the Brightness option; then converted once for the
// sRGB target. Face and corner shading and VoxelCraft's sun-facing tint stay
// outside the conversion (the tint weighted by the sky's share), so fully
// sunlit faces look as before while shade, night and torch-lit interiors
// take Java's darkness and warmth.
struct OverworldLight {
    // The converted lightmap, and the sun-facing tint weighted by the sky's share.
    lit: vec3<f32>,
    sun: vec3<f32>,
};
fn overworld_light(sky_level: f32, block_level: f32, normal: vec3<f32>, shade: f32) -> vec3<f32> {
    let o = overworld_parts(sky_level, block_level, normal);
    return o.lit * shade * o.sun;
}
fn overworld_parts(sky_level: f32, block_level: f32, normal: vec3<f32>) -> OverworldLight {
    let sky = (sky_level / (4.0 - 3.0 * sky_level)) * g.params.z * g.sky_light.rgb;
    let p = (2.0 * block_level - 1.0) * (2.0 * block_level - 1.0);
    let hue = mix(vec3<f32>(1.0, 0.847, 0.549), vec3<f32>(1.0), 0.9 * p);
    let block = hue * (block_level / (4.0 - 3.0 * block_level)) * 1.4;
    let raw = ambient_light() + sky + block;
    let s = max(max(sky.r, sky.g), sky.b);
    let b = max(max(block.r, block.g), block.b);
    let share = clamp(s / max(s + b, 1e-4), 0.0, 1.0);
    let sun = mix(vec3<f32>(1.0), daylight_tint(normal), share);
    return OverworldLight(from_srgb(lightmap(raw)), sun);
}

// Java 26.3 adds the environment ambient colour to sky and block light.
// The Overworld retains its existing daylight presentation in this scoped fix.
fn dimension_light(sky: vec3<f32>, torch: vec3<f32>) -> vec3<f32> {
    if g.ambient.w > 0.0 {
        // Unclamped: `lightmap` clamps it after Darkness, as Java does.
        return ambient_light() + sky + torch;
    }
    // The Overworld's ambient #0a0a0a (about 0.04), or night vision's.
    return max(sky, torch) * 0.96 + max(ambient_light(), vec3<f32>(0.04));
}

// The sky's colour in direction `dir`, as sky.wgsl draws it: Java's sky
// plane 16 blocks up, fogged toward the horizon by distance up to the sky
// fog end, in sRGB.
fn sky_at(dir: vec3<f32>) -> vec3<f32> {
    var fogged = 1.0;
    if dir.y > 1e-4 {
        let end = max(g.fog_ends.x, 1.0);
        let spherical = 16.0 / dir.y;
        let horizontal = spherical * sqrt(max(1.0 - dir.y * dir.y, 0.0));
        fogged = select(clamp(spherical / end, 0.0, 1.0), 1.0, horizontal >= end);
    }
    return from_srgb(mix(to_srgb(g.zenith_color.rgb), to_srgb(g.fog_color.rgb), fogged));
}

@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    let tex = sample_block(in.uv, in.layer);
    return vec4<f32>(apply_fog(tex.rgb * lighting(in), in.dist), 1.0);
}

@fragment
fn fs_cutout(in: VsOut) -> @location(0) vec4<f32> {
    // Fire occupies layers 89..95 (soul fire 1300..1306) and animates entirely on the GPU.
    let soul = in.layer == 1300u;
    let fire = in.layer == 89u || soul;
    let layer = select(in.layer, select(89u, 1300u, soul) + u32(g.sun.w * 10.0) % 7u, fire);
    let tex = sample_block(in.uv, layer);
    if tex.a < 0.5 {
        discard;
    }
    return vec4<f32>(apply_fog(tex.rgb * select(lighting(in), vec3<f32>(1.0), fire), in.dist), 1.0);
}

// v1.0 gap Track 5c: the Nether portal's animated texture. Java's
// `nether_portal.png` runs 32 frames of one tick each (its `.mcmeta` sets no
// frame time); the game's single original layer moves on the same clock as
// two copies scrolled diagonally opposite ways, a texel every second frame,
// so the swirl churns and repeats every 32 ticks. The layer is on the first
// texture page with or without paging, so the second copy samples it there.
const PORTAL_LAYER: u32 = 81u;
fn portal_step() -> f32 {
    return f32((u32(g.sun.w * 20.0) % 32u) / 2u) / 16.0;
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    let portal = in.layer == PORTAL_LAYER;
    let step = portal_step();
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    var tex = sample_block(in.uv + select(vec2<f32>(0.0), vec2<f32>(step, step), portal), in.layer);
    if portal {
        let other = textureSampleGrad(blocks, blocks_sampler, in.uv + vec2<f32>(-step, step), PORTAL_LAYER, dx, dy);
        tex = (tex + other) * 0.5;
    }
    var color = tex.rgb * lighting(in);
    var alpha = tex.a;
    if g.environment.y > 0.5 && in.layer == 5u && in.normal.y > 0.5 && in.rel.y < 0.0 {
        // Analytic normals keep greedy quad edges joined. Spatial periods
        // divide the camera wrap; temporal periods divide the time wrap.
        // Sampled per texel (16 per block), so the ripples' highlights
        // keep the block art's pixel grid instead of smooth bands.
        let p = (floor((in.rel.xz + g.environment.zw) * 16.0) + 0.5) / 16.0;
        let t = g.sun.w;
        let a = dot(p, vec2<f32>(0.785398, 0.392699)) + t * 1.047198;
        let b = dot(p, vec2<f32>(-0.392699, 0.785398)) + t * 0.628319;
        let c = dot(p, vec2<f32>(0.196350, 0.196350)) - t * 0.418879;
        let slope = 0.12 * cos(a) * vec2<f32>(0.785398, 0.392699)
            + 0.07 * cos(b) * vec2<f32>(-0.392699, 0.785398)
            + 0.10 * cos(c) * vec2<f32>(0.196350, 0.196350);
        let normal = normalize(vec3<f32>(-slope.x, 1.0, -slope.y));
        let view = normalize(-in.rel);
        let reflected = reflect(-view, normal);
        let fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(normal, view), 0.0), 5.0);
        let access = curve(in.light).x;
        // Reflect the analytic sky, not scene geometry. Underground water
        // gets no sky reflection; underwater and side faces keep base shading.
        let reflection = sky_at(reflected);
        color = mix(color, reflection, fresnel * access);
        if g.environment.x < 0.5 {
            let moon = g.sun.y < 0.0;
            let dir = select(g.sun.xyz, -g.sun.xyz, moon);
            let glint = pow(max(dot(reflected, dir), 0.0), 180.0);
            let glow = select(vec3<f32>(1.0, 0.82, 0.55), vec3<f32>(0.5, 0.65, 1.0), moon);
            color += glow * glint * access * (1.0 - g.params.w) * 0.7;
        }
        alpha = mix(alpha, 0.92, fresnel * access);
    }
    return vec4<f32>(apply_fog(color, in.dist), alpha);
}
