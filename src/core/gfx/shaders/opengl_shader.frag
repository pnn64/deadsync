#version 330 core
in vec2 v_tex_coord;
in vec2 v_quad;
out vec4 FragColor;

uniform vec4  u_color;
uniform sampler2D u_texture;
uniform vec4  u_edge_fade; 

float edge_fade_factor(vec2 q, vec4 e) {
    // ... existing code ...
    float f = 1.0;
    if (e.x > 0.0) f *= clamp(q.x / e.x, 0.0, 1.0);           
    if (e.y > 0.0) f *= clamp((1.0 - q.x) / e.y, 0.0, 1.0);   
    if (e.z > 0.0) f *= clamp(q.y / e.z, 0.0, 1.0);           
    if (e.w > 0.0) f *= clamp((1.0 - q.y) / e.w, 0.0, 1.0);   
    return f;
}

void main() {
    vec4 s = texture(u_texture, fract(v_tex_coord));
    
    float f = edge_fade_factor(v_quad, u_edge_fade);
    s.a *= f;
    FragColor = s * u_color; 
}