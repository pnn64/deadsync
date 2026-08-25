struct Proj {
    proj: mat4x4<f32>,
};

@group(0) @binding(0) var u_sampler: sampler;
@group(0) @binding(1) var u_y: texture_2d<f32>;
@group(0) @binding(2) var u_u: texture_2d<f32>;
@group(0) @binding(3) var u_v: texture_2d<f32>;
struct YuvConversion {
    levels: vec4<f32>,
    coeffs: vec4<f32>,
};
@group(0) @binding(4) var<uniform> u_conversion: YuvConversion;
var<immediate> u_proj: Proj;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) center: vec4<f32>,
    @location(3) size: vec2<f32>,
    @location(4) rot: vec2<f32>,
    @location(5) tint: vec4<f32>,
    @location(6) uv_scale: vec2<f32>,
    @location(7) uv_offset: vec2<f32>,
    @location(8) local_offset: vec2<f32>,
    @location(9) local_offset_rot_sin_cos: vec2<f32>,
    @location(10) edge_fade: vec4<f32>,
    @location(11) texture_mask: f32,
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) edge_fade: vec4<f32>,
    @location(3) texture_mask: f32,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    let local = vec2<f32>(input.pos.x * input.size.x, input.pos.y * input.size.y);
    let s = input.rot.x;
    let c = input.rot.y;
    let rotated = vec2<f32>(c * local.x - s * local.y, s * local.x + c * local.y);
    let os = input.local_offset_rot_sin_cos.x;
    let oc = input.local_offset_rot_sin_cos.y;
    let offset_world = vec2<f32>(
        oc * input.local_offset.x - os * input.local_offset.y,
        os * input.local_offset.x + oc * input.local_offset.y
    );
    let world = vec3<f32>(input.center.xy + rotated + offset_world, input.center.z);

    var out: VertexOut;
    out.pos = u_proj.proj * vec4<f32>(world, 1.0);
    out.uv = input.uv * input.uv_scale + input.uv_offset;
    out.tint = input.tint;
    out.edge_fade = input.edge_fade;
    out.texture_mask = input.texture_mask;
    return out;
}

fn edge_factor(t: f32, feather_l: f32, feather_r: f32) -> f32 {
    var l = 1.0;
    var r = 1.0;
    if feather_l > 0.0 {
        l = clamp(t / feather_l, 0.0, 1.0);
    }
    if feather_r > 0.0 {
        r = clamp((1.0 - t) / feather_r, 0.0, 1.0);
    }
    return min(l, r);
}

fn sample_yuv(uv: vec2<f32>) -> vec4<f32> {
    let y = textureSample(u_y, u_sampler, uv).r * u_conversion.levels.x
        + u_conversion.levels.y;
    let u = textureSample(u_u, u_sampler, uv).r * u_conversion.levels.z
        + u_conversion.levels.w;
    let v = textureSample(u_v, u_sampler, uv).r * u_conversion.levels.z
        + u_conversion.levels.w;
    return vec4<f32>(
        y + u_conversion.coeffs.x * v,
        y + u_conversion.coeffs.y * u + u_conversion.coeffs.z * v,
        y + u_conversion.coeffs.w * u,
        1.0
    );
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let texel = sample_yuv(input.uv);
    let fade_x = edge_factor(input.uv.x, input.edge_fade.x, input.edge_fade.y);
    let fade_y = edge_factor(input.uv.y, input.edge_fade.z, input.edge_fade.w);
    var color = texel * input.tint;
    if input.texture_mask > 0.5 {
        color = vec4<f32>(input.tint.rgb, input.tint.a);
    }
    color.a = color.a * min(fade_x, fade_y);
    return color;
}
