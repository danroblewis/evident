//! OpenGL shader-program FTI bridge. See `event_sources/mod.rs`
//! for trait and shared helpers.

use std::sync::mpsc::Sender;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

/// GL shader program FTI bridge. Synchronous install: compile
/// vertex + fragment shaders, link program, call glUseProgram,
/// write program ID. Requires a current GL context (set by
/// SDL_Window FTI's earlier install in the same FSM).
pub struct GlProgramSource {
    vertex_src:   String,
    fragment_src: String,
    handle_field: String,
    write_queue:  WriteQueue,
}

impl GlProgramSource {
    pub fn new(vertex_src: impl Into<String>,
               fragment_src: impl Into<String>,
               handle_field: impl Into<String>) -> Self {
        GlProgramSource {
            vertex_src:   vertex_src.into(),
            fragment_src: fragment_src.into(),
            handle_field: handle_field.into(),
            write_queue:  new_write_queue(),
        }
    }

    /// Synchronous install: compiles + links on the calling
    /// thread (which has the current GL context). Returns
    /// the program ID (or 0 on failure).
    pub fn start_inline(&mut self, tx: Sender<SchedulerEvent>) -> Result<u32, String> {
        use libloading::{Library, Symbol};
        use std::ffi::CString;
        use std::os::raw::{c_char, c_int, c_uint};

        // OpenGL framework on macOS; libGL on Linux.
        let paths = [
            "/System/Library/Frameworks/OpenGL.framework/OpenGL",
            "/usr/lib/x86_64-linux-gnu/libGL.so.1",
            "/usr/lib/libGL.so",
        ];
        let lib = paths.iter()
            .find_map(|p| unsafe { Library::new(p) }.ok())
            .ok_or_else(|| "couldn't find OpenGL library".to_string())?;

        type GlCreateShader   = unsafe extern "C" fn(c_uint) -> c_uint;
        type GlShaderSource   = unsafe extern "C" fn(c_uint, c_int, *const *const c_char, *const c_int);
        type GlCompileShader  = unsafe extern "C" fn(c_uint);
        type GlCreateProgram  = unsafe extern "C" fn() -> c_uint;
        type GlAttachShader   = unsafe extern "C" fn(c_uint, c_uint);
        type GlLinkProgram    = unsafe extern "C" fn(c_uint);
        type GlUseProgram     = unsafe extern "C" fn(c_uint);
        type GlDeleteShader   = unsafe extern "C" fn(c_uint);
        type GlGetShaderiv    = unsafe extern "C" fn(c_uint, c_uint, *mut c_int);
        type GlGetShaderInfoLog = unsafe extern "C" fn(c_uint, c_int, *mut c_int, *mut c_char);
        type GlGetProgramiv     = unsafe extern "C" fn(c_uint, c_uint, *mut c_int);
        type GlGetProgramInfoLog = unsafe extern "C" fn(c_uint, c_int, *mut c_int, *mut c_char);

        let create_shader: Symbol<GlCreateShader>   = unsafe { lib.get(b"glCreateShader\0") }
            .map_err(|e| format!("glCreateShader: {e}"))?;
        let shader_source: Symbol<GlShaderSource>   = unsafe { lib.get(b"glShaderSource\0") }
            .map_err(|e| format!("glShaderSource: {e}"))?;
        let compile_shader: Symbol<GlCompileShader> = unsafe { lib.get(b"glCompileShader\0") }
            .map_err(|e| format!("glCompileShader: {e}"))?;
        let create_program: Symbol<GlCreateProgram> = unsafe { lib.get(b"glCreateProgram\0") }
            .map_err(|e| format!("glCreateProgram: {e}"))?;
        let attach_shader: Symbol<GlAttachShader>   = unsafe { lib.get(b"glAttachShader\0") }
            .map_err(|e| format!("glAttachShader: {e}"))?;
        let link_program: Symbol<GlLinkProgram>     = unsafe { lib.get(b"glLinkProgram\0") }
            .map_err(|e| format!("glLinkProgram: {e}"))?;
        let use_program: Symbol<GlUseProgram>       = unsafe { lib.get(b"glUseProgram\0") }
            .map_err(|e| format!("glUseProgram: {e}"))?;
        let delete_shader: Symbol<GlDeleteShader>   = unsafe { lib.get(b"glDeleteShader\0") }
            .map_err(|e| format!("glDeleteShader: {e}"))?;
        let get_shader_iv: Symbol<GlGetShaderiv>    = unsafe { lib.get(b"glGetShaderiv\0") }
            .map_err(|e| format!("glGetShaderiv: {e}"))?;
        let get_shader_log: Symbol<GlGetShaderInfoLog> = unsafe { lib.get(b"glGetShaderInfoLog\0") }
            .map_err(|e| format!("glGetShaderInfoLog: {e}"))?;
        let get_program_iv: Symbol<GlGetProgramiv> = unsafe { lib.get(b"glGetProgramiv\0") }
            .map_err(|e| format!("glGetProgramiv: {e}"))?;
        let get_program_log: Symbol<GlGetProgramInfoLog> = unsafe { lib.get(b"glGetProgramInfoLog\0") }
            .map_err(|e| format!("glGetProgramInfoLog: {e}"))?;

        let compile = |kind: c_uint, src: &str| -> Result<c_uint, String> {
            let id = unsafe { create_shader(kind) };
            if id == 0 { return Err("glCreateShader returned 0".into()); }
            let src_c = CString::new(src).map_err(|_| "shader src has nul")?;
            let src_ptr = src_c.as_ptr();
            unsafe {
                shader_source(id, 1, &src_ptr, std::ptr::null());
                compile_shader(id);
            }
            // Check compile status (GL_COMPILE_STATUS = 0x8B81).
            let mut status: c_int = 0;
            unsafe { get_shader_iv(id, 0x8B81, &mut status); }
            if status == 0 {
                let mut log = vec![0i8; 1024];
                let mut len: c_int = 0;
                unsafe { get_shader_log(id, 1024, &mut len, log.as_mut_ptr() as *mut c_char); }
                let log_str: String = log.iter().take(len as usize)
                    .map(|&b| b as u8 as char).collect();
                return Err(format!("shader compile failed: {log_str}"));
            }
            Ok(id)
        };

        // GL_VERTEX_SHADER=0x8B31, GL_FRAGMENT_SHADER=0x8B30.
        let vs = compile(0x8B31, &self.vertex_src)?;
        let fs = compile(0x8B30, &self.fragment_src)?;
        let prog = unsafe { create_program() };
        if prog == 0 { return Err("glCreateProgram returned 0".into()); }
        unsafe {
            attach_shader(prog, vs);
            attach_shader(prog, fs);
            link_program(prog);
        }
        // Check link status (GL_LINK_STATUS = 0x8B82). Silent
        // link failure is the classic black-screen footgun.
        let mut link_status: c_int = 0;
        unsafe { get_program_iv(prog, 0x8B82, &mut link_status); }
        if link_status == 0 {
            let mut log = vec![0i8; 1024];
            let mut len: c_int = 0;
            unsafe { get_program_log(prog, 1024, &mut len, log.as_mut_ptr() as *mut c_char); }
            let log_str: String = log.iter().take(len as usize)
                .map(|&b| b as u8 as char).collect();
            return Err(format!("program link failed: {log_str}"));
        }
        unsafe {
            use_program(prog);
            delete_shader(vs);
            delete_shader(fs);
        }

        {
            let mut q = self.write_queue.lock().unwrap();
            q.push_back((self.handle_field.clone(), Value::Int(prog as i64)));
        }
        let _ = tx.send(SchedulerEvent::Tick { name: "gl_program".into() });

        // No keepalive thread needed — the lib stays loaded
        // because we leak it (drop suppressed via the
        // Box::leak pattern below would be cleaner, but
        // forgetting the binding works too). For simplicity
        // we just let the borrow extend through `lib` going
        // out of scope; the underlying GL framework remains
        // mapped because SDL_Window's bridge holds it open.
        // Actually we need to either leak this lib or hold
        // onto it. Let's leak it via Box::leak.
        let leaked: &'static Library = Box::leak(Box::new(lib));
        let _ = leaked;
        Ok(prog)
    }
}

impl EventSource for GlProgramSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        // Always synchronous; uses the current GL context.
        self.start_inline(tx)?;
        Ok(())
    }
    fn stop(&mut self) {}
    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }
    fn write_fields(&self) -> Vec<String> {
        vec![self.handle_field.clone()]
    }
}
