#version 330 core
in vec2 v_uv;
in vec4 v_color;
out vec4 FragColor;

uniform sampler2D u_texture;

void main() {
    vec2 uv = fract(v_uv);
    FragColor = texture(u_texture, uv) * v_color;
}
