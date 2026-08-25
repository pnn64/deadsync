#version 110
varying vec2 v_tex_coord;
varying vec2 v_quad;

uniform sampler2D u_texture;
uniform sampler2D u_texture_u;
uniform sampler2D u_texture_v;
uniform int u_yuv420;
uniform vec4 u_yuv_levels;
uniform vec4 u_yuv_coeffs;
uniform vec4 u_tint;
uniform vec4 u_edge_fade;
uniform float u_texture_mask;

vec4 sample_texture(vec2 uv) {
    if (u_yuv420 == 0) return texture2D(u_texture, uv);
    float y = texture2D(u_texture, uv).r * u_yuv_levels.x + u_yuv_levels.y;
    float u = texture2D(u_texture_u, uv).r * u_yuv_levels.z + u_yuv_levels.w;
    float v = texture2D(u_texture_v, uv).r * u_yuv_levels.z + u_yuv_levels.w;
    return vec4(
        y + u_yuv_coeffs.x * v,
        y + u_yuv_coeffs.y * u + u_yuv_coeffs.z * v,
        y + u_yuv_coeffs.w * u,
        1.0);
}

float edge_fade_factor(vec2 q, vec4 e) {
    float f = 1.0;
    if (e.x > 0.0) f *= clamp(q.x / e.x, 0.0, 1.0);
    if (e.y > 0.0) f *= clamp((1.0 - q.x) / e.y, 0.0, 1.0);
    if (e.z > 0.0) f *= clamp(q.y / e.z, 0.0, 1.0);
    if (e.w > 0.0) f *= clamp((1.0 - q.y) / e.w, 0.0, 1.0);
    return f;
}

void main() {
    vec4 s = sample_texture(v_tex_coord);
    float f = edge_fade_factor(v_quad, u_edge_fade);
    vec4 color = s * u_tint;
    if (u_texture_mask > 0.5) {
        color = vec4(u_tint.rgb, s.a * u_tint.a);
    }
    color.a *= f;
    gl_FragColor = color;
}
