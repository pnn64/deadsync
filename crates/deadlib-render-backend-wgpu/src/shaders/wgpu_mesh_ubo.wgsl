struct Proj {
    proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u_proj: Proj;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.pos = u_proj.proj * vec4<f32>(input.pos, 0.0, 1.0);
    // Engine cameras use [-w, w] depth; wgpu clips to [0, w].
    out.pos.z = (out.pos.z + out.pos.w) * 0.5;
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return input.color;
}
