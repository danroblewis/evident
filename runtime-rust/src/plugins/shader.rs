//! `SDLShaderPlugin` — renders an Evident `shader Name` decl as a
//! GLSL fragment shader on a fullscreen triangle.
//!
//! Lifecycle:
//!   1. `initialize`: create an SDL window with a GL 3.3 core
//!      profile context. Idempotent (re-init after a swap reuses
//!      the existing window).
//!   2. First `before_step` after a successful main solve: read
//!      `output.shader_name` from the bindings, look up the shader
//!      in the runtime, transpile + compile + link, cache the
//!      program + uniform locations.
//!   3. Per `after_step`: pull each uniform's value out of bindings,
//!      `glUniform*` it, draw two triangles forming a fullscreen
//!      quad, swap buffers.
//!
//! The plugin is opaque to Z3 — the shader's body never enters any
//! solver call. It's a pure consumer of bindings.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::ptr;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sdl2::keyboard::Scancode;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{EventPump, Sdl};

use crate::glsl::{transpile, TranspiledShader};
use crate::translate::Value;
use crate::executor::Plugin;

/// Types this plugin handles. `SDLShaderOutput` triggers the
/// transpile + render pipeline; `SDLInput` / `SDLWindow` are the
/// usual SDL contributions, replicated here so a shader-rendering
/// program doesn't need the rect-renderer SDLPlugin running too
/// (only one plugin can own the SDL EventPump). `cmd_execute`
/// suppresses SDLPlugin when this plugin is active.
pub const SDL_SHADER_TYPES: &[&str] =
    &["SDLShaderOutput", "SDLInput", "SDLWindow"];

/// SDL+GL state held across the lifetime of a run. The Sdl context
/// must outlive the GLContext (which must outlive the Window) — all
/// stored here as Options so re-initialization is idempotent.
pub struct SDLShaderPlugin {
    width:  u32,
    height: u32,
    title:  String,

    sdl:        Option<Sdl>,
    window:     Option<Window>,
    gl_context: Option<GLContext>,
    /// Owned event pump. SDL allows only one EventPump per Sdl
    /// instance — creating a fresh one each frame silently errors
    /// (`Result::Err` we used to swallow), which left close events
    /// unpolled and the X button non-functional.
    event_pump: Option<EventPump>,

    /// Map from declared var name → its type name, populated at
    /// `initialize`. Used by `before_step` to know which SDLInput /
    /// SDLWindow vars to contribute givens for, and by `after_step`
    /// to find which `<var>.shader_name` binding to read.
    var_types: HashMap<String, String>,

    // Input state mirrored from the SDL event pump each frame.
    mouse_x: i32,
    mouse_y: i32,
    click:   bool,
    quit:    bool,
    last_instant:   Instant,
    last_screen_xy: Option<(i32, i32)>,

    /// Compiled shader program + uniform locations. Lazily filled on
    /// the first `after_step` once we know which shader to use.
    program: Option<CompiledShader>,

    /// Schema-table snapshot used to expand record uniforms. Needed
    /// at compile time; the plugin pulls it from the runtime via
    /// the executor's `set_runtime_handle` (see below).
    runtime_handle: RuntimeHandle,

    /// Fullscreen-triangle VAO/VBO. Created lazily alongside the
    /// program. One quad serves every program.
    vao: u32,
    vbo: u32,
}

/// Compiled shader + its uniform locations, keyed by source name
/// (`state.hero.x`). The plugin reads bindings by source name and
/// uploads to the corresponding GL location.
struct CompiledShader {
    program:           u32,
    uniform_locations: HashMap<String, (i32, &'static str)>,
}

/// Trampoline: the executor calls `set_runtime` once per active
/// plugin so this plugin can pull the schema table at transpile
/// time. Wrapped in an Arc so cloning the handle (across plugin
/// re-init) doesn't deep-copy the schemas.
#[derive(Default)]
pub struct RuntimeHandle {
    /// Name → list of (leaf_field_name, leaf_type_name). Populated
    /// at `set_runtime`.
    pub type_leaves: std::sync::Arc<HashMap<String, Vec<(String, String)>>>,
    /// shader_name → ShaderDecl.
    pub shaders:     std::sync::Arc<HashMap<String, crate::ast::ShaderDecl>>,
}

impl SDLShaderPlugin {
    pub fn new(width: u32, height: u32, title: impl Into<String>) -> Self {
        SDLShaderPlugin {
            width, height, title: title.into(),
            sdl: None, window: None, gl_context: None,
            event_pump: None,
            var_types: HashMap::new(),
            mouse_x: 0, mouse_y: 0, click: false, quit: false,
            last_instant: Instant::now(),
            last_screen_xy: None,
            program: None,
            runtime_handle: RuntimeHandle::default(),
            vao: 0, vbo: 0,
        }
    }

    /// Inject the runtime's type+shader tables. The executor calls
    /// this once after `initialize` so the plugin has everything it
    /// needs to transpile shaders without holding a runtime
    /// reference (which would entangle lifetimes).
    pub fn set_runtime(&mut self, handle: RuntimeHandle) {
        self.runtime_handle = handle;
    }

    fn open_window(&mut self) -> Result<(), String> {
        if self.window.is_some() {
            return Ok(());
        }
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        // Request GL 3.3 core profile — matches the `#version 330
        // core` the transpiler emits.
        let gl_attr = video.gl_attr();
        gl_attr.set_context_profile(GLProfile::Core);
        gl_attr.set_context_version(3, 3);

        let window = video
            .window(&self.title, self.width, self.height)
            .opengl()
            .position_centered()
            .build()
            .map_err(|e| e.to_string())?;
        let gl_context = window.gl_create_context()?;
        gl::load_with(|name| video.gl_get_proc_address(name) as *const _);

        // Fullscreen triangle. Two-triangle quad covering [-1, 1]².
        // gl_FragCoord runs over the framebuffer in pixels; we'll
        // pass the normalized coord through `pixel` (vec2 in [0,1]).
        let verts: [f32; 12] = [
            -1.0, -1.0,  1.0, -1.0,  -1.0,  1.0,
             1.0, -1.0,  1.0,  1.0,  -1.0,  1.0,
        ];
        let mut vao = 0u32;
        let mut vbo = 0u32;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (verts.len() * std::mem::size_of::<f32>()) as isize,
                verts.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                0, 2, gl::FLOAT, gl::FALSE,
                (2 * std::mem::size_of::<f32>()) as i32,
                ptr::null(),
            );
        }
        let event_pump = sdl.event_pump()?;
        self.vao = vao;
        self.vbo = vbo;
        self.event_pump = Some(event_pump);
        self.sdl        = Some(sdl);
        self.gl_context = Some(gl_context);
        self.window     = Some(window);
        Ok(())
    }

    /// Compile + link the shader for `shader_name`. Stores the
    /// program + uniform-location table on `self.program`.
    fn compile_shader_named(&mut self, shader_name: &str) -> Result<(), String> {
        let shader_decl = self.runtime_handle.shaders.get(shader_name)
            .ok_or_else(|| format!("shader {:?} not found in runtime", shader_name))?;
        let transpiled: TranspiledShader = transpile(shader_decl, &self.runtime_handle.type_leaves)
            .map_err(|e| format!("transpile {shader_name}: {e}"))?;

        let vert_src = include_str!("shader.vert");
        let vert = compile_shader_stage(gl::VERTEX_SHADER, vert_src)?;
        let frag = compile_shader_stage(gl::FRAGMENT_SHADER, &transpiled.source)?;
        let program = link_program(vert, frag)?;
        unsafe {
            gl::DeleteShader(vert);
            gl::DeleteShader(frag);
        }

        let mut uniform_locations: HashMap<String, (i32, &'static str)> = HashMap::new();
        for u in &transpiled.uniforms {
            let cname = CString::new(u.glsl_name.clone()).unwrap();
            let loc = unsafe { gl::GetUniformLocation(program, cname.as_ptr()) };
            // GL returns -1 for unused / optimized-away uniforms;
            // store anyway so the upload path can no-op cleanly.
            uniform_locations.insert(u.source_name.clone(), (loc, u.glsl_type));
        }

        self.program = Some(CompiledShader { program, uniform_locations });
        Ok(())
    }
}

impl Plugin for SDLShaderPlugin {
    fn handles_types(&self) -> &'static [&'static str] { SDL_SHADER_TYPES }

    fn initialize(&mut self, matched_vars: Vec<(String, String)>) {
        self.var_types.clear();
        for (n, t) in matched_vars {
            self.var_types.insert(n, t);
        }
        if let Err(e) = self.open_window() {
            eprintln!("SDLShader init failed: {e}");
        }
    }

    fn before_step(&mut self) -> Option<HashMap<String, Value>> {
        // Drain events first — close button + escape stop the run.
        // The pump is owned by this plugin (created once in
        // open_window). Re-creating per frame silently fails because
        // SDL allows only one EventPump per Sdl instance.
        self.click = false;
        if let Some(pump) = self.event_pump.as_mut() {
            for event in pump.poll_iter() {
                use sdl2::event::Event;
                use sdl2::keyboard::Keycode;
                match event {
                    Event::Quit { .. } => { self.quit = true; return None; }
                    Event::KeyDown { keycode: Some(Keycode::Escape), .. } => return None,
                    Event::MouseMotion { x, y, .. } => { self.mouse_x = x; self.mouse_y = y; }
                    Event::MouseButtonDown { x, y, .. } => {
                        self.mouse_x = x; self.mouse_y = y; self.click = true;
                    }
                    _ => {}
                }
            }
        }

        // Continuous keyboard state.
        let pump = self.event_pump.as_ref()?;
        let kb = pump.keyboard_state();
        let right = kb.is_scancode_pressed(Scancode::Right);
        let left  = kb.is_scancode_pressed(Scancode::Left);
        let up    = kb.is_scancode_pressed(Scancode::Up);
        let down  = kb.is_scancode_pressed(Scancode::Down);

        // dt + wall clock.
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_instant).as_millis() as i64;
        let dt_ms = dt_ms.min(100);
        self.last_instant = now;
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // Window position + delta (for SDLWindow vars only).
        let canvas_window = self.window.as_ref()?;
        let (screen_x, screen_y) = canvas_window.position();
        let (size_w, size_h)     = canvas_window.size();
        let (wdx, wdy) = match self.last_screen_xy {
            Some((px, py)) => (screen_x - px, screen_y - py),
            None => (0, 0),
        };
        self.last_screen_xy = Some((screen_x, screen_y));

        let mut out: HashMap<String, Value> = HashMap::new();
        for (var, ty) in &self.var_types {
            match ty.as_str() {
                "SDLInput" => {
                    out.insert(format!("{var}.right_held"), Value::Bool(right));
                    out.insert(format!("{var}.left_held"),  Value::Bool(left));
                    out.insert(format!("{var}.up_held"),    Value::Bool(up));
                    out.insert(format!("{var}.down_held"),  Value::Bool(down));
                    out.insert(format!("{var}.mouse.x"),    Value::Int(self.mouse_x as i64));
                    out.insert(format!("{var}.mouse.y"),    Value::Int(self.mouse_y as i64));
                    out.insert(format!("{var}.click"),      Value::Bool(self.click));
                    out.insert(format!("{var}.quit"),       Value::Bool(self.quit));
                    out.insert(format!("{var}.time"),       Value::Int(unix_ms));
                    out.insert(format!("{var}.dt"),         Value::Int(dt_ms));
                }
                "SDLWindow" => {
                    out.insert(format!("{var}.screen.x"), Value::Int(screen_x as i64));
                    out.insert(format!("{var}.screen.y"), Value::Int(screen_y as i64));
                    out.insert(format!("{var}.size.x"),   Value::Int(size_w as i64));
                    out.insert(format!("{var}.size.y"),   Value::Int(size_h as i64));
                    out.insert(format!("{var}.drag.x"),   Value::Int(wdx as i64));
                    out.insert(format!("{var}.drag.y"),   Value::Int(wdy as i64));
                }
                _ => {} // SDLShaderOutput contributes nothing here
            }
        }
        Some(out)
    }

    fn after_step(&mut self, bindings: &HashMap<String, Value>) -> bool {
        // First successful step: figure out which shader to compile.
        // Find the (single) SDLShaderOutput var and read its
        // `shader_name` binding.
        if self.program.is_none() {
            let Some(var) = self.var_types.iter()
                .find(|(_, t)| t.as_str() == "SDLShaderOutput")
                .map(|(n, _)| n.clone()) else {
                eprintln!("SDLShader: no SDLShaderOutput var matched");
                return false;
            };
            let shader_name = match bindings.get(&format!("{var}.shader_name")) {
                Some(Value::Str(s)) => s.clone(),
                _ => {
                    eprintln!("SDLShader: missing or non-string {var}.shader_name binding");
                    return false;
                }
            };
            if let Err(e) = self.compile_shader_named(&shader_name) {
                eprintln!("SDLShader: {e}");
                return false;
            }
        }

        // Drive the GL pipeline: clear, use program, set uniforms,
        // draw, swap. Width/height are pulled live from the SDL
        // window so a future resize event would just work.
        let Some(prog) = &self.program else { return true };
        let (vw, vh) = self.window.as_ref()
            .map(|w| w.size())
            .unwrap_or((self.width, self.height));
        unsafe {
            gl::Viewport(0, 0, vw as i32, vh as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(prog.program);
            // Auto-uniforms first — these are the built-in
            // viewport uniforms the vertex shader needs and any
            // user fragment shader can also consume as
            // `iResolution.x` / `iResolution.y`.
            if let Some((loc, _)) = prog.uniform_locations.get("iResolution.x") {
                if *loc >= 0 { gl::Uniform1f(*loc, vw as f32); }
            }
            if let Some((loc, _)) = prog.uniform_locations.get("iResolution.y") {
                if *loc >= 0 { gl::Uniform1f(*loc, vh as f32); }
            }
            for (source_name, (loc, glsl_type)) in &prog.uniform_locations {
                if *loc < 0 { continue; }
                if source_name.starts_with("iResolution.") { continue; }
                let Some(v) = bindings.get(source_name) else { continue };
                upload_uniform(*loc, glsl_type, v);
            }
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
        }
        if let Some(window) = &self.window {
            window.gl_swap_window();
        }
        true
    }
}

fn upload_uniform(loc: i32, glsl_type: &str, v: &Value) {
    unsafe {
        match (glsl_type, v) {
            ("float", Value::Real(r)) => gl::Uniform1f(loc, *r as f32),
            ("float", Value::Int(n))  => gl::Uniform1f(loc, *n as f32),
            ("int",   Value::Int(n))  => gl::Uniform1i(loc, *n as i32),
            ("bool",  Value::Bool(b)) => gl::Uniform1i(loc, if *b { 1 } else { 0 }),
            _ => { /* type mismatch: silently skip — better than crashing the run */ }
        }
    }
}

fn compile_shader_stage(kind: u32, src: &str) -> Result<u32, String> {
    let csrc = CString::new(src).map_err(|e| e.to_string())?;
    unsafe {
        let id = gl::CreateShader(kind);
        gl::ShaderSource(id, 1, &csrc.as_ptr(), ptr::null());
        gl::CompileShader(id);
        let mut ok = 0i32;
        gl::GetShaderiv(id, gl::COMPILE_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0i32;
            gl::GetShaderiv(id, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf: Vec<u8> = vec![0; len as usize];
            gl::GetShaderInfoLog(id, len, ptr::null_mut(), buf.as_mut_ptr() as *mut _);
            let log = CStr::from_ptr(buf.as_ptr() as *const _)
                .to_string_lossy().into_owned();
            gl::DeleteShader(id);
            return Err(format!("shader compile failed:\n{log}\n--- source ---\n{src}"));
        }
        Ok(id)
    }
}

fn link_program(vert: u32, frag: u32) -> Result<u32, String> {
    unsafe {
        let p = gl::CreateProgram();
        gl::AttachShader(p, vert);
        gl::AttachShader(p, frag);
        gl::LinkProgram(p);
        let mut ok = 0i32;
        gl::GetProgramiv(p, gl::LINK_STATUS, &mut ok);
        if ok == 0 {
            let mut len = 0i32;
            gl::GetProgramiv(p, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf: Vec<u8> = vec![0; len as usize];
            gl::GetProgramInfoLog(p, len, ptr::null_mut(), buf.as_mut_ptr() as *mut _);
            let log = CStr::from_ptr(buf.as_ptr() as *const _)
                .to_string_lossy().into_owned();
            gl::DeleteProgram(p);
            return Err(format!("program link failed:\n{log}"));
        }
        Ok(p)
    }
}
