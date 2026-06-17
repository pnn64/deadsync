#version 110
varying vec2 v_tex_coord;
varying vec2 v_quad;

uniform sampler2D u_texture;
uniform vec4 u_tint;
uniform vec4 u_edge_fade;
uniform float u_texture_mask;

float edge_fade_factor(vec2 q, vec4 e) {
    float f = 1.0;
    if (e.x > 0.0) f *= clamp(q.x / e.x, 0.0, 1.0);
    if (e.y > 0.0) f *= clamp((1.0 - q.x) / e.y, 0.0, 1.0);
    if (e.z > 0.0) f *= clamp(q.y / e.z, 0.0, 1.0);
    if (e.w > 0.0) f *= clamp((1.0 - q.y) / e.w, 0.0, 1.0);
    return f;
}

void main() {
    vec4 s = texture2D(u_texture, v_tex_coord);
    float f = edge_fade_factor(v_quad, u_edge_fade);
    vec4 color = s * u_tint;
    if (u_texture_mask > 0.5) {
        color = vec4(u_tint.rgb, s.a * u_tint.a);
    }
    color.a *= f;
    gl_FragColor = color;
}
