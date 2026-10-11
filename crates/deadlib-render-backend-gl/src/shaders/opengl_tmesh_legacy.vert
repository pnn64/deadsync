#version 120
attribute vec3 a_pos;
attribute vec2 a_uv;
attribute vec4 a_color;
attribute vec2 a_tex_matrix_scale;
attribute vec4 a_normal;
uniform vec4 u_tint;
uniform vec2 u_uv_scale;
uniform vec2 u_uv_offset;
uniform vec2 u_uv_tex_shift;
uniform float u_texture_mask;
uniform float u_cull_mode;
uniform mat4 u_model;
uniform vec4 i_sphere_row0;
uniform vec4 i_sphere_row1;
uniform vec4 i_sphere_row2;
uniform vec4 i_additive_uv;
varying vec2 v_uv;
varying vec4 v_color;
varying float v_texture_mask;
varying float v_cull_mode;
varying vec2 v_additive_uv;
varying float v_additive;
uniform mat4 u_model_view_proj;

vec2 sphere_uv() {
    mat3 a = mat3(vec3(i_sphere_row0.x, i_sphere_row1.x, i_sphere_row2.x), vec3(i_sphere_row0.y, i_sphere_row1.y, i_sphere_row2.y), vec3(i_sphere_row0.z, i_sphere_row1.z, i_sphere_row2.z));
    mat3 cof = mat3(cross(a[1], a[2]), cross(a[2], a[0]), cross(a[0], a[1]));
    vec3 n = cof * a_normal.xyz * sign(dot(a[0], cof[0]));
    vec4 p = vec4(a_pos, 1.0);
    vec3 eye = vec3(dot(i_sphere_row0, p), dot(i_sphere_row1, p), dot(i_sphere_row2, p));
    vec3 r = reflect(eye / max(length(eye), 1e-20), n / max(length(n), 1e-20));
    return r.xy / max(2.0 * length(r + vec3(0.0, 0.0, 1.0)), 1e-20) + vec2(0.5);
}
void main() {
    mat4 model = u_model;
    gl_Position = u_model_view_proj * model * vec4(a_pos, 1.0);

    float mode = a_normal.w;
    vec2 primary = a_uv;
    vec2 secondary = a_uv;
    if (mod(mode, 4.0) != 0.0 && u_texture_mask < 0.5) {
        vec2 sphere = sphere_uv();
        if (mod(mode, 2.0) != 0.0) primary = sphere;
        if (mod(floor(mode / 2.0), 2.0) != 0.0) secondary = sphere;
    }
    v_uv = primary * u_uv_scale + u_uv_offset + u_uv_tex_shift * (a_tex_matrix_scale - vec2(1.0));
    v_additive_uv = secondary * i_additive_uv.xy + i_additive_uv.zw;
    v_color = a_color * u_tint;
    v_texture_mask = u_texture_mask;
    v_cull_mode = u_cull_mode;
    v_additive = float(mode >= 4.0);
}
