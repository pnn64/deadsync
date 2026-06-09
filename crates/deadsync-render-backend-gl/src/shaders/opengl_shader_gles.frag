#version 300 es
precision mediump float;

in vec2 v_tex_coord;
in vec2 v_quad;
flat in vec4 v_tint;
flat in vec4 v_edge_fade;
flat in float v_texture_mask;
out vec4 FragColor;

uniform sampler2D u_texture;

float edge_fade_factor(vec2 q, vec4 e) {
    float f = 1.0;
    if (e.x > 0.0) f *= clamp(q.x / e.x, 0.0, 1.0);
    if (e.y > 0.0) f *= clamp((1.0 - q.x) / e.y, 0.0, 1.0);
    if (e.z > 0.0) f *= clamp(q.y / e.z, 0.0, 1.0);
    if (e.w > 0.0) f *= clamp((1.0 - q.y) / e.w, 0.0, 1.0);
    return f;
}

void main() {
    vec4 s = texture(u_texture, v_tex_coord);
    float f = edge_fade_factor(v_quad, v_edge_fade);
    vec4 color = s * v_tint;
    if (v_texture_mask > 0.5) {
        color = vec4(v_tint.rgb, s.a * v_tint.a);
    }
    color.a *= f;
    FragColor = color;
}
