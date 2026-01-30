#version 450

layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec4 a_color;

layout(push_constant) uniform ProjPush {
    mat4 proj;
} pc;

layout(location = 0) out vec4 v_color;

void main() {
    gl_Position = pc.proj * vec4(a_pos, 0.0, 1.0);
    v_color = a_color;
}

