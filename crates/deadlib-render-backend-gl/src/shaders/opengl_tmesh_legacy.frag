#version 110
varying vec2 v_uv;
varying vec4 v_color;

uniform sampler2D u_texture;
uniform float u_texture_mask;
uniform float u_cull_back;

void main() {
    if (u_cull_back > 0.5 && !gl_FrontFacing) discard;
    vec2 uv = fract(v_uv);
    vec4 s = texture2D(u_texture, uv);
    gl_FragColor = s * v_color;
    if (u_texture_mask > 0.5) {
        gl_FragColor = vec4(v_color.rgb, s.a * v_color.a);
    }
}
