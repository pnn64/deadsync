#version 300 es
precision highp float;

layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_tex_coord;
layout (location = 2) in vec2 i_center;
layout (location = 3) in vec2 i_size;
layout (location = 4) in vec2 i_rot_sin_cos;
layout (location = 5) in vec4 i_tint;
layout (location = 6) in vec2 i_uv_scale;
layout (location = 7) in vec2 i_uv_offset;
layout (location = 8) in vec2 i_local_offset;
layout (location = 9) in vec2 i_local_offset_rot_sin_cos;
layout (location = 10) in vec4 i_edge_fade;

out vec2 v_tex_coord;
out vec2 v_quad;
flat out vec4 v_tint;
flat out vec4 v_edge_fade;

uniform mat4 u_model_view_proj;

void main() {
    v_quad = a_tex_coord;

    vec2 local = vec2(a_pos.x * i_size.x, a_pos.y * i_size.y);
    float s = i_rot_sin_cos.x;
    float c = i_rot_sin_cos.y;
    vec2 rotated = vec2(
        c * local.x - s * local.y,
        s * local.x + c * local.y
    );

    float so = i_local_offset_rot_sin_cos.x;
    float co = i_local_offset_rot_sin_cos.y;
    vec2 local_offset_world = vec2(
        co * i_local_offset.x - so * i_local_offset.y,
        so * i_local_offset.x + co * i_local_offset.y
    );

    vec2 world = i_center + rotated + local_offset_world;
    gl_Position = u_model_view_proj * vec4(world, 0.0, 1.0);
    v_tex_coord = a_tex_coord * i_uv_scale + i_uv_offset;
    v_tint = i_tint;
    v_edge_fade = i_edge_fade;
}
