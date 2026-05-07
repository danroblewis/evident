#version 330 core
// Pass-through vertex shader for SDLShaderPlugin's fullscreen quad.
//
// `pixel` is emitted in PIXEL space with a TOP-LEFT origin so it
// lines up directly with `input.mouse` and other SDL coordinate
// inputs. The y flip happens here (`1.0 - v_pos.y`) — the OpenGL
// convention is bottom-left, but every other coord the host hands
// us (mouse, window position) is top-left. Matching them avoids a
// per-shader correction.
//
// `iResolution_x` / `iResolution_y` are auto-injected uniforms that
// the SDLShaderPlugin sets each frame to the viewport size. The
// fragment shader can reference them as `iResolution.x` /
// `iResolution.y` (the transpiler recognizes the name and resolves
// to the underscored uniform).
layout(location = 0) in vec2 v_pos;
out vec2 pixel;
uniform float iResolution_x;
uniform float iResolution_y;
void main() {
    pixel = vec2((v_pos.x + 1.0) * 0.5 * iResolution_x,
                 (1.0 - v_pos.y) * 0.5 * iResolution_y);
    gl_Position = vec4(v_pos, 0.0, 1.0);
}
