#version 450
layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) in float v_texture_mask;
layout(location = 3) in float v_cull_mode;
layout(location = 4) in vec2 v_additive_uv;
layout(location = 5) in float v_additive;
layout(set = 0, binding = 0) uniform sampler2D u_texture;
layout(set = 1, binding = 0) uniform sampler2D u_additive;
layout(location = 0) out vec4 out_color;

void main() {
    if ((v_cull_mode > 0.5 && v_cull_mode < 1.5 && !gl_FrontFacing) || (v_cull_mode > 1.5 && gl_FrontFacing)) discard;
    vec4 texel = texture(u_texture, v_uv);
    vec4 color = texel * v_color;
    if (v_texture_mask > 0.5) {
        color = vec4(v_color.rgb, texel.a * v_color.a);
    } else if (v_additive > 0.5) {
        vec4 reflection = texture(u_additive, v_additive_uv);
        color = vec4(min(color.rgb + reflection.rgb, vec3(1.0)), color.a * reflection.a);
    }
    if (color.a <= 1.0 / 256.0) discard;
    out_color = color;
}
