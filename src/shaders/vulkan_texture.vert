#version 450

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_uv;

layout(location = 0) out vec2 out_uv;

layout(push_constant) uniform constants {
    mat4 mvp;
} PushConstants;

void main() {
    gl_Position = PushConstants.mvp * vec4(in_position, 0.0, 1.0);
    out_uv = in_uv;
}