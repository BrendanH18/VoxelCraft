// v0.15 Track D: the Glowing outline (Java's entity outline post effect,
// drawn here as one full-screen pass): a pixel outside a glowing entity's
// silhouette mask but within two pixels of it turns the outline colour
// (white, the colour of an entity with no team). It is drawn after the
// world without depth, so it shows through walls.

@group(0) @binding(0) var mask: texture_2d<f32>;

struct FullOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> FullOut {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: FullOut;
    out.pos = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

const REACH: i32 = 2;

@fragment
fn fs_outline(in: FullOut) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(mask));
    let p = clamp(vec2<i32>(floor(in.pos.xy)), vec2<i32>(0), size - 1);
    if textureLoad(mask, p, 0).r > 0.5 {
        discard;
    }
    var near = 0.0;
    for (var dy = -REACH; dy <= REACH; dy++) {
        for (var dx = -REACH; dx <= REACH; dx++) {
            let q = clamp(p + vec2<i32>(dx, dy), vec2<i32>(0), size - 1);
            near = max(near, textureLoad(mask, q, 0).r);
        }
    }
    if near < 0.5 {
        discard;
    }
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
