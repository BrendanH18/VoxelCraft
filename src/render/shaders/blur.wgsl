// v0.18 Track 2: the menu background blur (Java's `blur` post effect):
// three horizontal and vertical box blur passes of `radius` pixels. Each
// pass takes `radius + 1` bilinear samples two texels apart, the last one
// at half weight, so a pass averages 2 * radius + 1 texels. Samples stay
// inside the view's rectangle so split-screen views don't bleed together.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;

struct Blur {
    // Step direction in texels: (1, 0) or (0, 1).
    dir: vec2<f32>,
    radius: f32,
    pad: f32,
    // The view's rectangle in texels: min x, min y, max x, max y.
    rect: vec4<f32>,
};

@group(0) @binding(2) var<uniform> blur: Blur;

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

fn tap(p: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    let q = clamp(p, blur.rect.xy + 0.5, blur.rect.zw - 0.5);
    return textureSampleLevel(source, linear_sampler, q / size, 0.0);
}

@fragment
fn fs_blur(in: FullOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(source));
    let r = blur.radius;
    var sum = vec4<f32>(0.0);
    var a = -r + 0.5;
    loop {
        if a > r {
            break;
        }
        sum += tap(in.pos.xy + blur.dir * a, size);
        a += 2.0;
    }
    sum += tap(in.pos.xy + blur.dir * r, size) * 0.5;
    return vec4<f32>((sum / (r + 0.5)).rgb, 1.0);
}
