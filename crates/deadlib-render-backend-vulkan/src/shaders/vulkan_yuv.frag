#version 450

layout(set = 0, binding = 0) uniform sampler2D u_planes[3];
layout(push_constant) uniform Push {
    mat4 proj;
    vec4 levels;
    vec4 coeffs;
} u_conversion;

layout(location = 0) in vec2 v_uv;
layout(location = 1) flat in vec4 v_tint;
layout(location = 2) flat in vec4 v_edgeFade;
layout(location = 3) flat in float v_texture_mask;

layout(location = 0) out vec4 outColor;

float edgeFactor1D(float t, float featherLeft, float featherRight) {
    float fL = featherLeft > 0.0 ? clamp(t / featherLeft, 0.0, 1.0) : 1.0;
    float fR = featherRight > 0.0 ? clamp((1.0 - t) / featherRight, 0.0, 1.0) : 1.0;
    return min(fL, fR);
}

vec4 sampleYuv(vec2 uv) {
    float y = texture(u_planes[0], uv).r * u_conversion.levels.x
        + u_conversion.levels.y;
    float u = texture(u_planes[1], uv).r * u_conversion.levels.z
        + u_conversion.levels.w;
    float v = texture(u_planes[2], uv).r * u_conversion.levels.z
        + u_conversion.levels.w;
    return vec4(
        y + u_conversion.coeffs.x * v,
        y + u_conversion.coeffs.y * u + u_conversion.coeffs.z * v,
        y + u_conversion.coeffs.w * u,
        1.0);
}

void main() {
    vec4 texel = sampleYuv(v_uv);
    float fadeX = edgeFactor1D(v_uv.x, v_edgeFade.x, v_edgeFade.y);
    float fadeY = edgeFactor1D(v_uv.y, v_edgeFade.z, v_edgeFade.w);
    outColor = texel * v_tint;
    if (v_texture_mask > 0.5) {
        outColor = vec4(v_tint.rgb, v_tint.a);
    }
    outColor.a *= min(fadeX, fadeY);
}
