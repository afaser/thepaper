struct Params {
    src_size: vec2<f32>,
    dst_size: vec2<f32>,
    y_off: f32,
    y_scale: f32,
    c_scale: f32,
    radius: f32,
    mat0: vec4<f32>,
    mat1: vec4<f32>,
    mat2: vec4<f32>,
    dir: vec2<f32>,
    dst_srgb: f32,
    _pad: f32,
};

var<immediate> p: Params;

@group(0) @binding(0) var tex_y: texture_2d<f32>;
@group(0) @binding(1) var tex_uv: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let pos = positions[i];
    var out: VsOut;
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 1.0 - (pos.y * 0.5 + 0.5));
    return out;
}

fn cover_uv(uv: vec2<f32>) -> vec2<f32> {
    let scale = max(p.dst_size.x / p.src_size.x, p.dst_size.y / p.src_size.y);
    let visible = p.dst_size / scale;
    return (uv - vec2<f32>(0.5)) * (visible / p.src_size) + vec2<f32>(0.5);
}

fn yuv_to_rgb(y: f32, uv: vec2<f32>) -> vec3<f32> {
    let yv = (y - p.y_off) * p.y_scale;
    let u = (uv.x - 0.5) * p.c_scale;
    let v = (uv.y - 0.5) * p.c_scale;
    let rgb = vec3<f32>(
        p.mat0.x * yv + p.mat0.y * u + p.mat0.z * v,
        p.mat1.x * yv + p.mat1.y * u + p.mat1.z * v,
        p.mat2.x * yv + p.mat2.y * u + p.mat2.z * v,
    );
    return clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn sample_yuv(uv: vec2<f32>) -> vec3<f32> {
    let y = textureSample(tex_y, samp, uv).r;
    let uv_c = textureSample(tex_uv, samp, uv).rg;
    return yuv_to_rgb(y, uv_c);
}

fn encode(rgb: vec3<f32>) -> vec3<f32> {
    if (p.dst_srgb > 0.5) {
        return pow(rgb, vec3<f32>(2.2));
    }
    return rgb;
}

fn gauss(i: f32) -> f32 {
    let sigma = max(p.radius * 0.5, 0.5);
    return exp(-0.5 * i * i / (sigma * sigma));
}

fn blur_yuv(uv: vec2<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i = 0; i <= 6; i = i + 1) {
        let w = gauss(f32(i));
        let off = p.dir * f32(i);
        let a = sample_yuv(uv + off);
        let b = sample_yuv(uv - off);
        if (i == 0) {
            sum = sum + a * w;
            wsum = wsum + w;
        } else {
            sum = sum + (a + b) * w;
            wsum = wsum + 2.0 * w;
        }
    }
    return clamp(sum / wsum, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn blur_rgba(uv: vec2<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i = 0; i <= 6; i = i + 1) {
        let w = gauss(f32(i));
        let off = p.dir * f32(i);
        let a = textureSample(tex_y, samp, uv + off).rgb;
        let b = textureSample(tex_y, samp, uv - off).rgb;
        if (i == 0) {
            sum = sum + a * w;
            wsum = wsum + w;
        } else {
            sum = sum + (a + b) * w;
            wsum = wsum + 2.0 * w;
        }
    }
    return clamp(sum / wsum, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_live(in: VsOut) -> @location(0) vec4<f32> {
    let rgb = sample_yuv(cover_uv(in.uv));
    return vec4<f32>(encode(rgb), 1.0);
}

@fragment
fn fs_blur_yuv(in: VsOut) -> @location(0) vec4<f32> {
    let rgb = blur_yuv(cover_uv(in.uv));
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_blur_rgba(in: VsOut) -> @location(0) vec4<f32> {
    let rgb = blur_rgba(in.uv);
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_blur_draw(in: VsOut) -> @location(0) vec4<f32> {
    let rgb = textureSample(tex_y, samp, cover_uv(in.uv)).rgb;
    return vec4<f32>(encode(rgb), 1.0);
}
