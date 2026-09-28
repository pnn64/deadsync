#version 120
varying vec2 v_uv;
varying vec4 v_color;
varying float v_texture_mask;
varying float v_cull_back;
varying vec2 v_additive_uv;
varying float v_additive;
uniform sampler2D u_texture;
uniform sampler2D u_additive;


void main() {
    if (v_cull_back > 0.5 && !gl_FrontFacing) discard;
    vec4 texel = texture2D(u_texture, v_uv);
    vec4 color = texel * v_color;
    if (v_texture_mask > 0.5) {
        color = vec4(v_color.rgb, texel.a * v_color.a);
    } else if (v_additive > 0.5) {
        vec4 reflection = texture2D(u_additive, v_additive_uv);
        color = vec4(min(color.rgb + reflection.rgb, vec3(1.0)), color.a * reflection.a);
    }
    if (color.a <= 1.0 / 256.0) discard;
    gl_FragColor = color;
}
