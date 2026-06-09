#version 110
attribute vec2 a_pos;
attribute vec2 a_tex_coord;

varying vec2 v_tex_coord;
varying vec2 v_quad;

uniform mat4 u_model_view_proj;
uniform vec4 u_center;
uniform vec2 u_size;
uniform vec2 u_rot_sin_cos;
uniform vec2 u_uv_scale;
uniform vec2 u_uv_offset;
uniform vec2 u_local_offset;
uniform vec2 u_local_offset_rot_sin_cos;

void main() {
    v_quad = a_tex_coord;

    vec2 local = vec2(a_pos.x * u_size.x, a_pos.y * u_size.y);
    float s = u_rot_sin_cos.x;
    float c = u_rot_sin_cos.y;
    vec2 rotated = vec2(
        c * local.x - s * local.y,
        s * local.x + c * local.y
    );

    float so = u_local_offset_rot_sin_cos.x;
    float co = u_local_offset_rot_sin_cos.y;
    vec2 local_offset_world = vec2(
        co * u_local_offset.x - so * u_local_offset.y,
        so * u_local_offset.x + co * u_local_offset.y
    );

    vec3 world = vec3(u_center.xy + rotated + local_offset_world, u_center.z);
    gl_Position = u_model_view_proj * vec4(world, 1.0);
    v_tex_coord = a_tex_coord * u_uv_scale + u_uv_offset;
}
