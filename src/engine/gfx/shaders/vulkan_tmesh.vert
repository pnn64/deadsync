#version 450

layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec4 a_color;
layout(location = 3) in vec2 a_tex_matrix_scale;
layout(location = 4) in vec4 i_model_col0;
layout(location = 5) in vec4 i_model_col1;
layout(location = 6) in vec4 i_model_col2;
layout(location = 7) in vec4 i_model_col3;
layout(location = 8) in vec4 i_tint;
layout(location = 9) in vec2 i_uv_scale;
layout(location = 10) in vec2 i_uv_offset;
layout(location = 11) in vec2 i_uv_tex_shift;
layout(location = 12) in float i_texture_mask;

layout(push_constant) uniform ProjPush {
    mat4 proj;
} pc;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) flat out float v_texture_mask;

void main() {
    mat4 model = mat4(i_model_col0, i_model_col1, i_model_col2, i_model_col3);
    gl_Position = pc.proj * model * vec4(a_pos, 1.0);
    v_uv = a_uv * i_uv_scale + i_uv_offset
         + i_uv_tex_shift * (a_tex_matrix_scale - vec2(1.0, 1.0));
    v_color = a_color * i_tint;
    v_texture_mask = i_texture_mask;
}
