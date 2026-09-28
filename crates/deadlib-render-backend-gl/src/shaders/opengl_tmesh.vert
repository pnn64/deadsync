#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec4 a_color;
layout(location = 3) in vec2 a_tex_matrix_scale;
layout(location = 4) in vec4 a_normal;
layout(location = 5) in vec4 i_model_col0;
layout(location = 6) in vec4 i_model_col1;
layout(location = 7) in vec4 i_model_col2;
layout(location = 8) in vec4 i_model_col3;
layout(location = 9) in vec4 i_tint;
layout(location = 10) in vec4 i_uv_params;
layout(location = 11) in vec4 i_flags;
layout(location = 12) in vec4 i_sphere_row0;
layout(location = 13) in vec4 i_sphere_row1;
layout(location = 14) in vec4 i_sphere_row2;
layout(location = 15) in vec4 i_additive_uv;
out vec2 v_uv;
out vec4 v_color;
out float v_texture_mask;
out float v_cull_back;
out vec2 v_additive_uv;
out float v_additive;
uniform mat4 u_model_view_proj;

vec2 sphere_uv() {
    mat3 a = transpose(mat3(i_sphere_row0.xyz, i_sphere_row1.xyz, i_sphere_row2.xyz));
    mat3 cof = mat3(cross(a[1], a[2]), cross(a[2], a[0]), cross(a[0], a[1]));
    vec3 n = cof * a_normal.xyz * sign(dot(a[0], cof[0]));
    vec4 p = vec4(a_pos, 1.0);
    vec3 eye = vec3(dot(i_sphere_row0, p), dot(i_sphere_row1, p), dot(i_sphere_row2, p));
    vec3 r = reflect(eye / max(length(eye), 1e-20), n / max(length(n), 1e-20));
    return r.xy / max(2.0 * length(r + vec3(0.0, 0.0, 1.0)), 1e-20) + vec2(0.5);
}
void main() {
    mat4 model = mat4(i_model_col0, i_model_col1, i_model_col2, i_model_col3);
    gl_Position = u_model_view_proj * model * vec4(a_pos, 1.0);

    int mode = int(a_normal.w);
    vec2 primary = a_uv;
    vec2 secondary = a_uv;
    if ((mode & 3) != 0 && i_flags.z < 0.5) {
        vec2 sphere = sphere_uv();
        if ((mode & 1) != 0) primary = sphere;
        if ((mode & 2) != 0) secondary = sphere;
    }
    v_uv = primary * i_uv_params.xy + i_uv_params.zw + i_flags.xy * (a_tex_matrix_scale - vec2(1.0));
    v_additive_uv = secondary * i_additive_uv.xy + i_additive_uv.zw;
    v_color = a_color * i_tint;
    v_texture_mask = i_flags.z;
    v_cull_back = i_flags.w;
    v_additive = float((mode & 4) != 0);
}
