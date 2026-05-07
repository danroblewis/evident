#version 330 core
// Pass-through vertex shader for SDLShaderPlugin's fullscreen quad.
// Maps clip-space [-1, 1]² → normalized pixel coord [0, 1]² so the
// fragment shader's `pixel` varying is the convention.
layout(location = 0) in vec2 v_pos;
out vec2 pixel;
void main() {
    pixel = v_pos * 0.5 + 0.5;
    gl_Position = vec4(v_pos, 0.0, 1.0);
}
