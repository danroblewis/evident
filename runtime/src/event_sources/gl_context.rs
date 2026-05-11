//! Shared OpenGL FFI helpers used by GL-aware bridges. Owns the
//! `libGL` / `OpenGL.framework` dlopen + the small set of GL
//! function lookups other bridges need beyond their own
//! library.
//!
//! This file is NOT a bridge (no `pub struct GlContextSource`,
//! no `impl EventSource`). It's a sibling helper that exists so
//! bridges that need to configure a GL context can do so without
//! each one re-implementing the same library lookup. It's the
//! sanctioned place for `gl[A-Z]` / `OpenGL.framework` Rust code
//! outside of `gl_program.rs`'s shader-pipeline body.
//!
//! Each function here returns success or quietly fails — calls
//! happen only after the SDL bridge has created and made current
//! a GL context, so lookup failures usually indicate a missing
//! GL framework rather than a runtime bug.

use libloading::{Library, Symbol};
use std::os::raw::{c_int, c_uint};

/// Standard GL library paths on supported platforms. Tried in
/// order; first hit wins.
const GL_LIB_PATHS: &[&str] = &[
    "/System/Library/Frameworks/OpenGL.framework/OpenGL",
    "/usr/lib/x86_64-linux-gnu/libGL.so.1",
    "/usr/lib/libGL.so",
];

/// Open the platform's GL library. Returns None if no path
/// matches; callers treat this as "GL not available, skip the
/// optional GL setup" rather than fatal.
fn open_gl_library() -> Option<Library> {
    GL_LIB_PATHS.iter().find_map(|p| unsafe { Library::new(p) }.ok())
}

/// Create + bind a default VAO and set the GL viewport to the
/// given dimensions. Apple's GL-on-Metal driver defaults the
/// viewport to 0×0 (so draws don't rasterize) and core-profile
/// draws require a bound VAO. Both calls are needed for the
/// usual "I made a GL context, now please render anything"
/// path.
///
/// Returns `Some(vao_id)` on success, `None` if GL couldn't be
/// loaded or the symbol lookup failed. The library is leaked
/// via `Box::leak` so the GL framework remains mapped for the
/// life of the process — needed because subsequent
/// `Effect::LibCall` calls into GL will need it open.
///
/// Must be called from a thread with a current GL context (set
/// by the caller, e.g. via `SDL_GL_MakeCurrent`).
pub fn setup_default_vao_and_viewport(width: i32, height: i32) -> Option<u32> {
    type GlGenVertexArrays = unsafe extern "C" fn(c_int, *mut c_uint);
    type GlBindVertexArray = unsafe extern "C" fn(c_uint);
    type GlViewport        = unsafe extern "C" fn(c_int, c_int, c_int, c_int);

    let gl_lib = open_gl_library()?;
    let gen_vao: Result<Symbol<GlGenVertexArrays>, _> =
        unsafe { gl_lib.get(b"glGenVertexArrays\0") };
    let bind_vao: Result<Symbol<GlBindVertexArray>, _> =
        unsafe { gl_lib.get(b"glBindVertexArray\0") };
    let viewport: Result<Symbol<GlViewport>, _> =
        unsafe { gl_lib.get(b"glViewport\0") };

    let id = if let (Ok(gen), Ok(bind)) = (gen_vao, bind_vao) {
        let mut id: c_uint = 0;
        unsafe { gen(1, &mut id as *mut c_uint); bind(id); }
        id as u32
    } else {
        0
    };
    if let Ok(vp) = viewport {
        unsafe { vp(0, 0, width, height); }
    }
    // Leak the library so it stays mapped for subsequent
    // GL calls from user FSMs / other bridges (`gl_program.rs`
    // does the same trick for its own handle).
    let _: &'static Library = Box::leak(Box::new(gl_lib));
    Some(id)
}
