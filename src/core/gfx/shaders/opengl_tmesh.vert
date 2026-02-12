#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
layout (location = 2) in vec4 a_color;

out vec2 v_uv;
out vec4 v_color;

uniform mat4 u_model_view_proj;

void main() {
    v_uv = a_uv;
    v_color = a_color;
    gl_Position = u_model_view_proj * vec4(a_pos, 0.0, 1.0);
}
