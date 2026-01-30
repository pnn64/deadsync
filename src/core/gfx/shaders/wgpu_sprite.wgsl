struct Proj {
    proj: mat4x4<f32>,
};

@group(0) @binding(0) var u_sampler: sampler;
@group(0) @binding(1) var u_tex: texture_2d<f32>;
var<push_constant> u_proj: Proj;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) center: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) rot: vec2<f32>,
    @location(5) tint: vec4<f32>,
    @location(6) uv_scale: vec2<f32>,
    @location(7) uv_offset: vec2<f32>,
    @location(8) edge_fade: vec4<f32>,
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) edge_fade: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    let local = vec2<f32>(input.pos.x * input.size.x, input.pos.y * input.size.y);
    let s = input.rot.x;
    let c = input.rot.y;
    let rotated = vec2<f32>(c * local.x - s * local.y, s * local.x + c * local.y);
    let world = input.center + rotated;

    var out: VertexOut;
    out.pos = u_proj.proj * vec4<f32>(world, 0.0, 1.0);
    out.uv = input.uv * input.uv_scale + input.uv_offset;
    out.tint = input.tint;
    out.edge_fade = input.edge_fade;
    return out;
}

fn edge_factor(t: f32, feather_l: f32, feather_r: f32) -> f32 {
    var l = 1.0;
    var r = 1.0;
    if feather_l > 0.0 {
        l = clamp((t - 0.0) / feather_l, 0.0, 1.0);
    }
    if feather_r > 0.0 {
        r = clamp((1.0 - t) / feather_r, 0.0, 1.0);
    }
    return min(l, r);
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(u_tex, u_sampler, input.uv);
    let fade_x = edge_factor(input.uv.x, input.edge_fade.x, input.edge_fade.y);
    let fade_y = edge_factor(input.uv.y, input.edge_fade.z, input.edge_fade.w);
    let fade = min(fade_x, fade_y);
    var color = texel * input.tint;
    color.a = color.a * fade;
    return color;
}

