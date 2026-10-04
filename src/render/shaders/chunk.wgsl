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
    // x: night vision strength 0..1.
    effects: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

// Night vision (Java's lightmap): brightens every light level toward full,
// keeping its hue; g.effects.x is its strength.
fn night_vision(lit: vec3<f32>) -> vec3<f32> {
    let peak = max(max(lit.r, lit.g), max(lit.b, 1e-3));
    return mix(lit, lit / peak, g.effects.x);
}
@group(1) @binding(0) var blocks: texture_2d_array<f32>;
@group(1) @binding(1) var blocks_sampler: sampler;
@group(2) @binding(0) var<storage, read> quads: array<u32>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) shade: f32,
    @location(3) dist: f32,
    // x: sky light, y: block light (0..1), interpolated for smooth lighting.
    @location(4) light: vec2<f32>,
    @location(5) rel: vec3<f32>,
    @location(6) @interpolate(flat) normal: vec3<f32>,
};

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
    let base = vec3<u32>(w0 & 63u, (w0 >> 6u) & 63u, (w0 >> 12u) & 63u);

    var local: vec3<f32>;
    var uv: vec2<f32>;
    if face >= 6u {
        // Cross-shaped block: a unit diagonal plane through the cell, (0,0)
        // to (1,1) in xz for face 6 and (1,0) to (0,1) for face 7.
        let t = select(0u, 1u, c == 1u || c == 2u);
        let up = select(0u, 1u, c >= 2u);
        let corner = base + vec3<u32>(select(t, 1u - t, face == 7u), up, t);
        local = vec3<f32>(corner);
        uv = vec2<f32>(f32(t), f32(1u - up));
    } else {
        let d = face >> 1u;
        let u = (d + 1u) % 3u;
        let v = (d + 2u) % 3u;
        if (w1 >> 31u) == 1u {
            // Detail quad (shaped blocks): part of the cell `base`, bounds
            // and plane offset in 1/16 block.
            let lo = vec2<u32>((w0 >> 21u) & 31u, (w0 >> 26u) & 31u);
            let hi = vec2<u32>((w1 >> 16u) & 31u, (w1 >> 21u) & 31u);
            var p = vec3<f32>(base);
            p[d] += f32((w1 >> 26u) & 31u) / 16.0;
            p[u] += f32(select(lo.x, hi.x, c == 1u || c == 2u)) / 16.0;
            p[v] += f32(select(lo.y, hi.y, c >= 2u)) / 16.0;
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
    out.layer = w1 & 255u;
    out.shade = face_shade[face] * ao_curve[ao];
    out.dist = length(rel);
    out.rel = rel;
    var normals = array<vec3<f32>, 8>(
        vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(-1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, -1.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, -1.0),
        vec3<f32>(-0.707, 0.0, 0.707), vec3<f32>(0.707, 0.0, 0.707));
    out.normal = normals[face];
    out.light = vec2<f32>(f32(light & 15u), f32(light >> 4u)) / 15.0;
    return out;
}

// Minecraft-like light curve: dim levels fall off quickly.
fn curve(l: vec2<f32>) -> vec2<f32> {
    return l / (4.0 - 3.0 * l);
}

fn lighting(in: VsOut) -> vec3<f32> {
    let l = curve(in.light);
    let sky = l.x * g.params.z * daylight_tint(in.normal);
    let torch = l.y * vec3<f32>(1.0, 0.86, 0.66);
    return night_vision(max(sky, torch) * 0.96 + 0.04) * in.shade;
}

fn apply_fog(color: vec3<f32>, dist: f32) -> vec3<f32> {
    let f = clamp((dist - g.params.x) / (g.params.y - g.params.x), 0.0, 1.0);
    return mix(color, g.fog_color.rgb, f * f * (3.0 - 2.0 * f));
}

// Directional skylight; caves retain their block lighting and dimensions
// without a sun keep their steady ambient illumination.
fn daylight_tint(normal: vec3<f32>) -> vec3<f32> {
    if g.environment.y < 0.5 || g.environment.x > 0.5 {
        return vec3<f32>(1.0);
    }
    let moon = g.sun.y < 0.0;
    let dir = select(g.sun.xyz, -g.sun.xyz, moon);
    let elevation = abs(dir.y);
    let dusk = 1.0 - smoothstep(0.05, 0.45, elevation);
    let warm = mix(vec3<f32>(1.0), vec3<f32>(1.12, 0.80, 0.58), dusk);
    let tint = select(warm, vec3<f32>(0.72, 0.82, 1.05), moon);
    let diffuse = max(dot(normal, dir), 0.0);
    return mix(tint * (0.62 + 0.38 * diffuse), vec3<f32>(0.85), g.params.w);
}

@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(blocks, blocks_sampler, in.uv, in.layer);
    return vec4<f32>(apply_fog(tex.rgb * lighting(in), in.dist), 1.0);
}

@fragment
fn fs_cutout(in: VsOut) -> @location(0) vec4<f32> {
    // Fire occupies layers 89..95 and animates entirely on the GPU.
    let fire = in.layer == 89u;
    let layer = select(in.layer, 89u + u32(g.sun.w * 10.0) % 7u, fire);
    let tex = textureSample(blocks, blocks_sampler, in.uv, layer);
    if tex.a < 0.5 {
        discard;
    }
    return vec4<f32>(apply_fog(tex.rgb * select(lighting(in), vec3<f32>(1.0), fire), in.dist), 1.0);
}

@fragment
fn fs_translucent(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(blocks, blocks_sampler, in.uv, in.layer);
    var color = tex.rgb * lighting(in);
    var alpha = tex.a;
    if g.environment.y > 0.5 && in.layer == 5u && in.normal.y > 0.5 && in.rel.y < 0.0 {
        // Analytic normals keep greedy quad edges joined. Spatial periods
        // divide the camera wrap; temporal periods divide the time wrap.
        let p = in.rel.xz + g.environment.zw;
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
        let reflection = mix(g.fog_color.rgb, g.zenith_color.rgb, smoothstep(0.0, 0.7, reflected.y));
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
