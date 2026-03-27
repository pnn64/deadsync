#version 450

layout(set = 0, binding = 0) uniform sampler2D u_tex;

layout(location = 0) in vec2 v_uv;
layout(location = 1) flat in vec4 v_tint;
layout(location = 2) flat in vec4 v_edgeFade; // (left, right, bottom, top) in UV units

layout(location = 0) out vec4 outColor;

// Returns a fade factor in [0,1] along one axis given UV coord `t` in [0,1]
float edgeFactor1D(float t, float featherLeft, float featherRight) {
    float fL = 1.0;
    float fR = 1.0;
    if (featherLeft  > 0.0) fL = clamp((t      - 0.0) / featherLeft, 0.0, 1.0);
    if (featherRight > 0.0) fR = clamp((1.0 - t)      / featherRight, 0.0, 1.0);
    return min(fL, fR);
}

void main() {
    vec4 texel = texture(u_tex, v_uv);

    float fadeX = edgeFactor1D(v_uv.x, v_edgeFade.x, v_edgeFade.y);
    float fadeY = edgeFactor1D(v_uv.y, v_edgeFade.z, v_edgeFade.w);
    float fade  = min(fadeX, fadeY);

    outColor = texel * v_tint;
    outColor.a *= fade;
}
