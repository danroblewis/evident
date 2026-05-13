//! SDL2 window bridge. See `event_sources/mod.rs` for trait and
//! shared helpers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::Value;
use super::{drain, new_write_queue, EventSource, SchedulerEvent, WriteQueue};

/// SDL_Window resource bridge. On start: load libSDL2, call
/// SDL_Init + SDL_CreateWindow, write the window pointer (as
/// i64) to the configured `handle` field. On stop: call
/// SDL_DestroyWindow + SDL_Quit.
///
/// Lifecycle: the SDL library and window pointer are held by
/// the source; both live until the source is dropped (i.e. the
/// runtime exits or the source is explicitly stopped).
///
/// Caveat: SDL functions must be called from the same thread
/// that initialized SDL on macOS. The current implementation
/// uses a single bridge thread for both init and cleanup,
/// avoiding the cross-thread issue. User FSMs that call SDL
/// functions via Effect::LibCall (for glClear, swap, etc.)
/// will execute on the dispatch thread, which is DIFFERENT —
/// this works because OpenGL doesn't have the same single-
/// thread restriction once a context is current. Window
/// management calls (resize, fullscreen toggle) from user
/// code may behave oddly; for v1 keep window state read-only.
pub struct SdlWindowSource {
    title:            String,
    width:            i32,
    height:           i32,
    handle_field:     String,
    gl_handle_field:  Option<String>,
    vao_field:        Option<String>,
    renderer_field:   Option<String>,
    write_queue:      WriteQueue,
    stop_flag:        Arc<AtomicBool>,
    handle:           Option<JoinHandle<()>>,
}

impl SdlWindowSource {
    pub fn new(title: impl Into<String>,
               width: i32,
               height: i32,
               handle_field: impl Into<String>) -> Self {
        SdlWindowSource {
            title:           title.into(),
            width, height,
            handle_field:    handle_field.into(),
            gl_handle_field: None,
            vao_field:       None,
            renderer_field:  None,
            write_queue:     new_write_queue(),
            stop_flag:       Arc::new(AtomicBool::new(false)),
            handle:          None,
        }
    }

    pub fn with_gl_context_field(mut self, field: impl Into<String>) -> Self {
        self.gl_handle_field = Some(field.into());
        self
    }

    pub fn with_vao_field(mut self, field: impl Into<String>) -> Self {
        self.vao_field = Some(field.into());
        self
    }

    /// Configure a renderer field. When set, `start_inline` calls
    /// SDL_CreateRenderer after the window is created and writes
    /// the renderer pointer to the field — giving user FSMs a
    /// persistent renderer handle across ticks (which raw FFI
    /// calls otherwise can't carry — see COUNTEREXAMPLES.md #9).
    pub fn with_renderer_field(mut self, field: impl Into<String>) -> Self {
        self.renderer_field = Some(field.into());
        self
    }

    /// Macos-friendly variant: call SDL_Init + SDL_CreateWindow
    /// SYNCHRONOUSLY on the calling thread (which should be the
    /// runtime's main thread). Pushes the resulting handle into
    /// the queue immediately. Skip the background thread; cleanup
    /// happens on Drop. Returns the window pointer (or 0 on failure)
    /// so the caller can also note it for later use.
    pub fn start_inline(&mut self, tx: Sender<SchedulerEvent>) -> Result<i64, String> {
        use libloading::{Library, Symbol};
        use std::ffi::CString;
        use std::os::raw::{c_char, c_int, c_void};
        let paths = [
            "/opt/homebrew/lib/libSDL2.dylib",
            "/usr/local/lib/libSDL2.dylib",
            "/usr/lib/x86_64-linux-gnu/libSDL2.so",
            "/usr/lib/libSDL2.so",
        ];
        let lib = paths.iter()
            .find_map(|p| unsafe { Library::new(p) }.ok())
            .ok_or_else(|| "couldn't find libSDL2 in standard paths".to_string())?;

        type SdlInit = unsafe extern "C" fn(u32) -> c_int;
        type SdlCreateWindow = unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut c_void;

        let sdl_init: Symbol<SdlInit> = unsafe { lib.get(b"SDL_Init\0") }
            .map_err(|e| format!("SDL_Init lookup: {e}"))?;
        let sdl_create_window: Symbol<SdlCreateWindow> = unsafe { lib.get(b"SDL_CreateWindow\0") }
            .map_err(|e| format!("SDL_CreateWindow lookup: {e}"))?;

        let init_rc = unsafe { sdl_init(0x20) };
        if init_rc != 0 {
            return Err(format!("SDL_Init returned {init_rc}"));
        }

        // macOS: NSApplicationLoad() bootstraps Cocoa for a
        // command-line tool. SDL's video init does this too,
        // but calling it explicitly + asking the app to be
        // "regular" (not "accessory" or "prohibited") may
        // tighten the GL drawable lifecycle that's blocking
        // dispatch-time renders. Best-effort — ignore if
        // AppKit isn't loadable.
        #[cfg(target_os = "macos")]
        {
            type NsApplicationLoad = unsafe extern "C" fn() -> bool;
            if let Ok(appkit) = unsafe {
                Library::new("/System/Library/Frameworks/AppKit.framework/AppKit")
            } {
                if let Ok(nsapp_load) = unsafe {
                    appkit.get::<Symbol<NsApplicationLoad>>(b"NSApplicationLoad\0")
                } {
                    unsafe { nsapp_load(); }
                }
                let _: &'static Library = Box::leak(Box::new(appkit));
            }
        }

        // GL attributes MUST be set BEFORE SDL_CreateWindow or
        // they're silently ignored. Without these, the context
        // defaults to legacy GL 2.1 on macOS, and #version 330
        // core shaders fail to link / produce nothing visible.
        // Only relevant if the caller wants a GL context — if
        // not, attribute calls are harmless.
        if self.gl_handle_field.is_some() {
            type SdlGlSetAttribute = unsafe extern "C" fn(c_int, c_int) -> c_int;
            if let Ok(set_attr) = unsafe { lib.get::<Symbol<SdlGlSetAttribute>>(b"SDL_GL_SetAttribute\0") } {
                unsafe {
                    set_attr(17, 3);  // CONTEXT_MAJOR_VERSION = 3
                    set_attr(18, 3);  // CONTEXT_MINOR_VERSION = 3
                    set_attr(21, 1);  // CONTEXT_PROFILE_MASK = CORE
                    set_attr(5, 1);   // DOUBLEBUFFER = 1
                }
            }
        }

        let title_c = CString::new(self.title.clone()).unwrap_or_default();
        let win_ptr = unsafe {
            sdl_create_window(
                title_c.as_ptr(),
                0x2FFF0000u32 as i32, 0x2FFF0000u32 as i32,
                self.width, self.height,
                2,  // SDL_WINDOW_OPENGL
            )
        };
        if win_ptr.is_null() {
            return Err("SDL_CreateWindow returned null".to_string());
        }
        // Explicitly show + raise. On macOS, terminal-launched
        // SDL windows can stay hidden behind other apps until
        // the activation policy is set; SDL_RaiseWindow nudges
        // them to the front. Both calls are no-ops if the
        // window is already visible.
        type SdlVoidWin = unsafe extern "C" fn(*mut c_void);
        if let Ok(show) = unsafe { lib.get::<Symbol<SdlVoidWin>>(b"SDL_ShowWindow\0") } {
            unsafe { show(win_ptr); }
        }
        if let Ok(raise) = unsafe { lib.get::<Symbol<SdlVoidWin>>(b"SDL_RaiseWindow\0") } {
            unsafe { raise(win_ptr); }
        }

        // GL context (optional). Attributes were already set
        // above, before SDL_CreateWindow.
        let gl_ptr = if self.gl_handle_field.is_some() {
            type SdlGlCreateContext = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
            type SdlGlMakeCurrent   = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
            let create_ctx: Symbol<SdlGlCreateContext> =
                unsafe { lib.get(b"SDL_GL_CreateContext\0") }
                    .map_err(|e| format!("SDL_GL_CreateContext lookup: {e}"))?;
            let ctx_ptr = unsafe { create_ctx(win_ptr) };
            if ctx_ptr.is_null() {
                return Err("SDL_GL_CreateContext returned null".to_string());
            }
            if let Ok(make_current) = unsafe { lib.get::<Symbol<SdlGlMakeCurrent>>(b"SDL_GL_MakeCurrent\0") } {
                unsafe { make_current(win_ptr, ctx_ptr) };
            }
            ctx_ptr as i64
        } else {
            0i64
        };

        // Default VAO (optional) + viewport. Core profile draws
        // need a bound VAO; on Apple's GL-on-Metal driver the
        // default viewport is 0×0 until you set it. The actual
        // GL FFI lives in the sibling `gl_context` helper so
        // this file doesn't open libGL itself.
        let vao_id = if self.vao_field.is_some() {
            super::gl_context::setup_default_vao_and_viewport(self.width, self.height)
                .unwrap_or(0) as i64
        } else { 0 };

        // SDL_Renderer (optional). When the user's SDL_Window
        // declaration includes a `renderer` field, create one
        // bound to this window. This is the missing piece that
        // makes per-tick rendering work (COUNTEREXAMPLES.md #9):
        // the renderer pointer is in the world snapshot from
        // here on, so each tick's FFI calls can pass it via
        // ArgHandle(world.renderer).
        let renderer_ptr = if self.renderer_field.is_some() {
            type SdlCreateRenderer = unsafe extern "C"
                fn(*mut c_void, c_int, u32) -> *mut c_void;
            let create_ren: Symbol<SdlCreateRenderer> =
                unsafe { lib.get(b"SDL_CreateRenderer\0") }
                    .map_err(|e| format!("SDL_CreateRenderer lookup: {e}"))?;
            // index=-1 (any driver), flags=0 (default; accelerated).
            let r = unsafe { create_ren(win_ptr, -1, 0) };
            if r.is_null() {
                return Err("SDL_CreateRenderer returned null".to_string());
            }
            r as i64
        } else { 0 };

        // Push the handles to the write queue so the runtime
        // applies them to the snapshot via the normal drain path.
        {
            let mut q = self.write_queue.lock().unwrap();
            q.push_back((self.handle_field.clone(), Value::Int(win_ptr as i64)));
            if let Some(gl_field) = &self.gl_handle_field {
                q.push_back((gl_field.clone(), Value::Int(gl_ptr)));
            }
            if let Some(vao_field) = &self.vao_field {
                q.push_back((vao_field.clone(), Value::Int(vao_id)));
            }
            if let Some(r_field) = &self.renderer_field {
                q.push_back((r_field.clone(), Value::Int(renderer_ptr)));
            }
        }
        let _ = tx.send(SchedulerEvent::Tick { name: "sdl".into() });

        // Hold the library alive in a long-lived background thread
        // so its Drop doesn't run prematurely. The thread parks
        // until stop_flag is set; cleanup of window pointer is
        // also done from this thread.
        // Hold the library alive in a long-lived background
        // thread that just waits for the stop signal. SDL
        // teardown (DestroyWindow, Quit) intentionally NOT
        // called — on macOS they need the main thread, and the
        // runtime is exiting anyway when the source is dropped.
        // The OS reclaims the window on process exit.
        let stop = self.stop_flag.clone();
        let _ = win_ptr;  // suppress unused (we want to keep the library alive)
        let handle = std::thread::Builder::new()
            .name("evident-sdl-keepalive".into())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                drop(lib);
            })
            .map_err(|e| format!("sdl keepalive spawn: {e}"))?;
        self.handle = Some(handle);

        Ok(win_ptr as i64)
    }
}

impl EventSource for SdlWindowSource {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String> {
        if self.handle.is_some() {
            return Err("SdlWindowSource already started".to_string());
        }
        let title = self.title.clone();
        let width = self.width;
        let height = self.height;
        let handle_field = self.handle_field.clone();
        let write_queue = self.write_queue.clone();
        let stop_flag = self.stop_flag.clone();
        let handle = std::thread::Builder::new()
            .name("evident-sdl".into())
            .spawn(move || {
                use libloading::{Library, Symbol};
                use std::ffi::CString;
                use std::os::raw::{c_char, c_int, c_void};
                // Try common SDL2 paths.
                let paths = [
                    "/opt/homebrew/lib/libSDL2.dylib",
                    "/usr/local/lib/libSDL2.dylib",
                    "/usr/lib/x86_64-linux-gnu/libSDL2.so",
                    "/usr/lib/libSDL2.so",
                ];
                let lib = paths.iter()
                    .find_map(|p| unsafe { Library::new(p) }.ok());
                let Some(lib) = lib else {
                    eprintln!("[SdlWindowSource] couldn't find libSDL2 \
                               in standard paths; window not created");
                    let _ = tx.send(SchedulerEvent::Tick { name: "sdl".into() });
                    return;
                };

                // SDL_Init(SDL_INIT_VIDEO=0x20)
                type SdlInit  = unsafe extern "C" fn(u32) -> c_int;
                type SdlCreateWindow = unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut c_void;
                type SdlDestroyWindow = unsafe extern "C" fn(*mut c_void);
                type SdlQuit  = unsafe extern "C" fn();

                let sdl_init: Symbol<SdlInit> = match unsafe { lib.get(b"SDL_Init\0") } {
                    Ok(s) => s,
                    Err(e) => { eprintln!("[SdlWindowSource] SDL_Init lookup: {e}"); return; }
                };
                let sdl_create_window: Symbol<SdlCreateWindow> = match unsafe { lib.get(b"SDL_CreateWindow\0") } {
                    Ok(s) => s,
                    Err(e) => { eprintln!("[SdlWindowSource] SDL_CreateWindow lookup: {e}"); return; }
                };
                let sdl_destroy_window: Symbol<SdlDestroyWindow> = match unsafe { lib.get(b"SDL_DestroyWindow\0") } {
                    Ok(s) => s,
                    Err(_) => { eprintln!("[SdlWindowSource] SDL_DestroyWindow lookup failed"); return; }
                };
                let sdl_quit: Symbol<SdlQuit> = match unsafe { lib.get(b"SDL_Quit\0") } {
                    Ok(s) => s,
                    Err(_) => { eprintln!("[SdlWindowSource] SDL_Quit lookup failed"); return; }
                };

                let init_rc = unsafe { sdl_init(0x20) };
                if init_rc != 0 {
                    eprintln!("[SdlWindowSource] SDL_Init returned {init_rc}");
                    return;
                }
                let title_c = CString::new(title.clone()).unwrap_or_default();
                // Position SDL_WINDOWPOS_CENTERED = 0x2FFF0000.
                // Flags 0x2 = SDL_WINDOW_OPENGL.
                let win_ptr = unsafe {
                    sdl_create_window(
                        title_c.as_ptr(),
                        0x2FFF0000u32 as i32, 0x2FFF0000u32 as i32,
                        width, height,
                        2,
                    )
                };
                if win_ptr.is_null() {
                    eprintln!("[SdlWindowSource] SDL_CreateWindow returned null");
                    unsafe { sdl_quit() };
                    return;
                }
                {
                    let mut q = write_queue.lock().unwrap();
                    q.push_back((handle_field.clone(), Value::Int(win_ptr as i64)));
                }
                let _ = tx.send(SchedulerEvent::Tick { name: "sdl".into() });

                // Wait for stop signal.
                while !stop_flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }

                // Cleanup.
                unsafe { sdl_destroy_window(win_ptr) };
                unsafe { sdl_quit() };
                drop(lib);
            })
            .map_err(|e| format!("SdlWindowSource spawn: {e}"))?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    fn drain_writes(&mut self) -> Vec<(String, Value)> {
        drain(&self.write_queue)
    }

    fn write_fields(&self) -> Vec<String> {
        vec![self.handle_field.clone()]
    }
}

impl Drop for SdlWindowSource {
    fn drop(&mut self) { self.stop(); }
}
