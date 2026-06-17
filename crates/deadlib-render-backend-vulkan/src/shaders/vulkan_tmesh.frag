#version 450

layout(set = 0, binding = 0) uniform sampler2D u_tex;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) flat in float v_texture_mask;

layout(location = 0) out vec4 outColor;

void main() {
    vec4 texel = texture(u_tex, v_uv);
    outColor = texel * v_color;
    if (v_texture_mask > 0.5) {
        outColor = vec4(v_color.rgb, texel.a * v_color.a);
    }
}
