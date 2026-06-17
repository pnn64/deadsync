#version 450

// Vertex buffers
layout(location = 0) in vec2 a_pos;      // unit quad: [-0.5..0.5]
layout(location = 1) in vec2 a_uv;

// Per-instance (binding = 1) — 72 bytes total
layout(location = 2) in vec4 i_center;      // screen/world space center xyz + pad
layout(location = 3) in vec2 i_size;        // scale along X/Y (lengths of model columns)
layout(location = 4) in vec2 i_rot_sin_cos; // (sinθ, cosθ)
layout(location = 5) in vec4 i_tint;
layout(location = 6) in vec2 i_uv_scale;
layout(location = 7) in vec2 i_uv_offset;
layout(location = 8) in vec2 i_local_offset;
layout(location = 9) in vec2 i_local_offset_rot_sin_cos; // (sinθ, cosθ)
layout(location = 10) in vec4 i_edge_fade;   // (fadeLeft, fadeRight, fadeBottom, fadeTop), in UV units
layout(location = 11) in float i_texture_mask;

// Push constants
layout(push_constant) uniform ProjPush {
    mat4 proj;
} pc;

// Varyings
layout(location = 0) out vec2 v_uv;
layout(location = 1) flat out vec4 v_tint;
layout(location = 2) flat out vec4 v_edgeFade;
layout(location = 3) flat out float v_texture_mask;

void main() {
    // Scale local quad half-extents by instance size
    vec2 local = vec2(a_pos.x * i_size.x, a_pos.y * i_size.y);

    // Rotate with sin/cos pair (R = [ cos -sin; sin cos ])
    float s = i_rot_sin_cos.x;
    float c = i_rot_sin_cos.y;
    vec2 rotated = vec2(c * local.x - s * local.y,
                        s * local.x + c * local.y);

    float so = i_local_offset_rot_sin_cos.x;
    float co = i_local_offset_rot_sin_cos.y;
    vec2 offset_world = vec2(
        co * i_local_offset.x - so * i_local_offset.y,
        so * i_local_offset.x + co * i_local_offset.y
    );

    vec3 world = vec3(i_center.xy + rotated + offset_world, i_center.z);

    gl_Position = pc.proj * vec4(world, 1.0);

    v_uv       = a_uv * i_uv_scale + i_uv_offset;
    v_tint     = i_tint;
    v_edgeFade = i_edge_fade;
    v_texture_mask = i_texture_mask;
}
