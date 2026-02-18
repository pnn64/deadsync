#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
layout (location = 2) in vec4 a_color;
layout (location = 3) in vec4 i_model_col0;
layout (location = 4) in vec4 i_model_col1;
layout (location = 5) in vec4 i_model_col2;
layout (location = 6) in vec4 i_model_col3;

out vec2 v_uv;
out vec4 v_color;

uniform mat4 u_model_view_proj;

void main() {
    mat4 model = mat4(i_model_col0, i_model_col1, i_model_col2, i_model_col3);
    v_uv = a_uv;
    v_color = a_color;
    gl_Position = u_model_view_proj * model * vec4(a_pos, 0.0, 1.0);
}
