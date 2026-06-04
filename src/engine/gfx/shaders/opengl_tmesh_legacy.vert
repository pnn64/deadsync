#version 110
attribute vec3 a_pos;
attribute vec2 a_uv;
attribute vec4 a_color;
attribute vec2 a_tex_matrix_scale;

varying vec2 v_uv;
varying vec4 v_color;

uniform mat4 u_model_view_proj;
uniform mat4 u_model;
uniform vec4 u_tint;
uniform vec2 u_uv_scale;
uniform vec2 u_uv_offset;
uniform vec2 u_uv_tex_shift;

void main() {
    v_uv = a_uv * u_uv_scale + u_uv_offset
         + u_uv_tex_shift * (a_tex_matrix_scale - vec2(1.0, 1.0));
    v_color = a_color * u_tint;
    gl_Position = u_model_view_proj * u_model * vec4(a_pos, 1.0);
}
