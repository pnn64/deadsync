diagnostic(off, derivative_uniformity);
struct Proj { proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u_proj: Proj;
@group(1) @binding(0) var u_sampler: sampler;
@group(1) @binding(1) var u_texture: texture_2d<f32>;
@group(2) @binding(0) var u_additive_sampler: sampler;
@group(2) @binding(1) var u_additive: texture_2d<f32>;

struct VertexIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) tex_matrix_scale: vec2<f32>,
    @location(4) normal: vec4<f32>,
    @location(5) model_col0: vec4<f32>,
    @location(6) model_col1: vec4<f32>,
    @location(7) model_col2: vec4<f32>,
    @location(8) model_col3: vec4<f32>,
    @location(9) tint: vec4<f32>,
    @location(10) uv_params: vec4<f32>,
    @location(11) flags: vec4<f32>,
    @location(12) sphere_row0: vec4<f32>,
    @location(13) sphere_row1: vec4<f32>,
    @location(14) sphere_row2: vec4<f32>,
    @location(15) additive_uv: vec4<f32>,
};
struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) texture_mask: f32,
    @location(3) @interpolate(flat) cull_back: f32,
    @location(4) additive_uv: vec2<f32>,
    @location(5) @interpolate(flat) additive: f32,
};

// OpenGL GL_SPHERE_MAP generates coordinates at vertices, before interpolation.
fn sphere_uv(input: VertexIn) -> vec2<f32> {
    let a = transpose(mat3x3<f32>(input.sphere_row0.xyz, input.sphere_row1.xyz, input.sphere_row2.xyz));
    let cofactor = mat3x3<f32>(cross(a[1], a[2]), cross(a[2], a[0]), cross(a[0], a[1]));
    let n = cofactor * input.normal.xyz * sign(dot(a[0], cofactor[0]));
    let p = vec4<f32>(input.pos, 1.0);
    let eye = vec3<f32>(dot(input.sphere_row0, p), dot(input.sphere_row1, p), dot(input.sphere_row2, p));
    let nn = n / max(length(n), 1e-20);
    let r = reflect(eye / max(length(eye), 1e-20), nn);
    let m = 2.0 * length(r + vec3<f32>(0.0, 0.0, 1.0));
    return r.xy / max(m, 1e-20) + vec2<f32>(0.5);
}

@vertex fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    let model = mat4x4<f32>(input.model_col0, input.model_col1, input.model_col2, input.model_col3);
    out.pos = u_proj.proj * model * vec4<f32>(input.pos, 1.0);
    out.pos.z = (out.pos.z + out.pos.w) * 0.5;
    let mode = u32(input.normal.w);
    var primary = input.uv;
    var secondary = input.uv;
    if (mode & 3u) != 0u && input.flags.z < 0.5 {
        let sphere = sphere_uv(input);
        if (mode & 1u) != 0u { primary = sphere; }
        if (mode & 2u) != 0u { secondary = sphere; }
    }
    out.uv = primary * input.uv_params.xy + input.uv_params.zw
        + input.flags.xy * (input.tex_matrix_scale - vec2<f32>(1.0));
    out.additive_uv = secondary * input.additive_uv.xy + input.additive_uv.zw;
    out.color = input.color * input.tint;
    out.texture_mask = input.flags.z;
    out.cull_back = input.flags.w;
    out.additive = select(0.0, 1.0, (mode & 4u) != 0u);
    return out;
}

@fragment fn fs_main(input: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    if input.cull_back > 0.5 && !front { discard; }
    let texel = textureSample(u_texture, u_sampler, input.uv);
    var color = texel * input.color;
    if input.texture_mask > 0.5 {
        color = vec4<f32>(input.color.rgb, texel.a * input.color.a);
    } else if input.additive > 0.5 {
        let reflection = textureSample(u_additive, u_additive_sampler, input.additive_uv);
        // ITG TextureMode_Add is GL_ADD: add RGB, multiply alpha, then blend.
        color = vec4<f32>(min(color.rgb + reflection.rgb, vec3<f32>(1.0)), color.a * reflection.a);
    }
    if color.a <= (1.0 / 256.0) { discard; }
    return color;
}
