#version 330 core
in vec2 v_uv;
in vec4 v_color;
flat in float v_texture_mask;
out vec4 FragColor;

uniform sampler2D u_texture;

void main() {
    vec2 uv = fract(v_uv);
    vec4 s = texture(u_texture, uv);
    FragColor = s * v_color;
    if (v_texture_mask > 0.5) {
        FragColor = vec4(v_color.rgb, s.a * v_color.a);
    }
}
