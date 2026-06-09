#version 110
attribute vec2 a_pos;
attribute vec4 a_color;

varying vec4 v_color;

uniform mat4 u_model_view_proj;

void main() {
    v_color = a_color;
    gl_Position = u_model_view_proj * vec4(a_pos, 0.0, 1.0);
}
