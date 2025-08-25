#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_tex_coord;

// Per-instance attributes (used when u_instanced == 1)
layout (location = 2) in vec2 i_center;
layout (location = 3) in vec2 i_size;
layout (location = 4) in vec2 i_uv_scale;
layout (location = 5) in vec2 i_uv_offset;

out vec2 v_tex_coord;
out vec2 v_quad; // a_tex_coord in quad space [0..1], unaffected by uv scale/offset

uniform mat4 u_model_view_proj;
uniform vec2 u_uv_scale;
uniform vec2 u_uv_offset;
uniform int  u_instanced; // 0: per-object MVP path, 1: per-instance path

void main() {
    v_quad = a_tex_coord;

    if (u_instanced == 1) {
        vec2 world = i_center + a_pos * i_size;
        gl_Position = u_model_view_proj * vec4(world, 0.0, 1.0);
        v_tex_coord = a_tex_coord * i_uv_scale + i_uv_offset;
    } else {
        gl_Position = u_model_view_proj * vec4(a_pos, 0.0, 1.0);
        v_tex_coord = a_tex_coord * u_uv_scale + u_uv_offset;
    }
}
