// FTI — Foreign Type Interface registry.
//
// Each entry is: (Evident type name, install function). The install
// function constructs the bridge, starts it, and returns the
// EventSource together with the keys it writes (so the caller can
// mark them as plugin-owned).
//
// Adding a new FTI type: implement an `install_<name>` function and
// add a row to the `INSTALLERS` table. No other Rust file needs
// changes — `effect_loop` reads the table for both the type-name
// recognition (`is_fti_type`) and the install dispatch.

use std::sync::mpsc::Sender;

use crate::ast::Pins;
use crate::event_sources::{
    EventSource, FrameTimer, GlProgramSource,
    SchedulerEvent, SdlWindowSource,
};

pub struct FtiInstall {
    pub source: Box<dyn EventSource>,
    pub keys:   Vec<String>,
}

pub struct FtiContext {
    pub claim_name:   String,
    pub param_name:   String,
    pub env_tick_ms:  Option<u64>,
}

pub type FtiInstallFn = fn(
    ctx:      &FtiContext,
    pins:     &Pins,
    event_tx: Sender<SchedulerEvent>,
) -> Result<FtiInstall, String>;

// Types listed here use bespoke Rust bridges — typically thread-
// driven (FrameTimer / Timer wake the scheduler at intervals) or
// requiring platform-specific glue. Types with a declarative
// `install ∈ Seq(InstallStep)` member skip this registry and use
// the generic dispatcher in `event_sources/declarative_install.rs`.
const INSTALLERS: &[(&str, FtiInstallFn)] = &[
    ("FrameClock", install_frame_clock),
    ("Timer",      install_timer),
    ("SDL_Window", install_sdl_window),
    ("GL_Program", install_gl_program),
];

pub fn fti_install_fn(type_name: &str) -> Option<FtiInstallFn> {
    INSTALLERS.iter()
        .find(|(name, _)| *name == type_name)
        .map(|(_, f)| *f)
}

pub fn is_fti_type(type_name: &str) -> bool {
    fti_install_fn(type_name).is_some()
}

/// Stdlib paths whose types are auto-installed by `INSTALLERS` when
/// a program declares matching world fields. The runtime treats
/// `import "..."` of these paths as optional: if the file exists at
/// the expected location, it loads normally; otherwise the import
/// silently no-ops because the FTI registry already provides the
/// types these files declare.
///
/// Lives here (not in `runtime.rs`) because the no-op policy is a
/// property of the FTI registry — these are paths the FTI bridges
/// stand in for. Adding a new shimmed stdlib file means: confirm
/// the relevant `INSTALLERS` entry covers everything the file
/// declares, then add the path here.
const SHIMMED_STDLIB_PATHS: &[&str] = &[
    "packages/sdl.ev",
    "stdlib/io.ev",
];

/// True if `import_path` is a stdlib file whose types are
/// already provided by the FTI registry, so a missing file at
/// the expected path should silently no-op rather than error.
pub fn is_shimmed_stdlib(import_path: &str) -> bool {
    SHIMMED_STDLIB_PATHS.contains(&import_path)
}

// ── Pin readers ────────────────────────────────────────────────

fn pin_int(pins: &Pins, field: &str) -> Option<i64> {
    use crate::ast::{Expr, Mapping};
    let Pins::Named(ms) = pins else { return None };
    ms.iter().find_map(|Mapping { slot, value }|
        (slot == field).then(|| match value {
            Expr::Int(n) => Some(*n),
            _ => None,
        }).flatten())
}

fn pin_str(pins: &Pins, field: &str) -> Option<String> {
    use crate::ast::{Expr, Mapping};
    let Pins::Named(ms) = pins else { return None };
    ms.iter().find_map(|Mapping { slot, value }|
        (slot == field).then(|| match value {
            Expr::Str(s) => Some(s.clone()),
            _ => None,
        }).flatten())
}

fn pin_bool(pins: &Pins, field: &str) -> Option<bool> {
    use crate::ast::{Expr, Mapping};
    let Pins::Named(ms) = pins else { return None };
    ms.iter().find_map(|Mapping { slot, value }|
        (slot == field).then(|| match value {
            Expr::Bool(b) => Some(*b),
            _ => None,
        }).flatten())
}

fn key(ctx: &FtiContext, field: &str) -> String {
    format!("{}.{}.{}", ctx.claim_name, ctx.param_name, field)
}

// ── Bridges ────────────────────────────────────────────────────

fn install_frame_clock(
    ctx: &FtiContext, _pins: &Pins, event_tx: Sender<SchedulerEvent>,
) -> Result<FtiInstall, String> {
    let ms = ctx.env_tick_ms.unwrap_or(100);
    let key = key(ctx, "tick_count");
    let mut bridge = FrameTimer::new(ms, "fti").with_count_field(&key);
    bridge.start(event_tx)
        .map_err(|e| format!("FrameClock bridge `{}.{}`: {e}",
                             ctx.claim_name, ctx.param_name))?;
    Ok(FtiInstall { source: Box::new(bridge), keys: vec![key] })
}

fn install_timer(
    ctx: &FtiContext, pins: &Pins, event_tx: Sender<SchedulerEvent>,
) -> Result<FtiInstall, String> {
    let ms = pin_int(pins, "interval_ms").unwrap_or(100) as u64;
    let key = key(ctx, "tick_count");
    let mut bridge = FrameTimer::new(ms, "fti").with_count_field(&key);
    bridge.start(event_tx)
        .map_err(|e| format!("Timer bridge `{}.{}`: {e}",
                             ctx.claim_name, ctx.param_name))?;
    Ok(FtiInstall { source: Box::new(bridge), keys: vec![key] })
}

fn install_sdl_window(
    ctx: &FtiContext, pins: &Pins, event_tx: Sender<SchedulerEvent>,
) -> Result<FtiInstall, String> {
    let title  = pin_str(pins, "title").unwrap_or_else(|| "Evident".into());
    let width  = pin_int(pins, "width").unwrap_or(640) as i32;
    let height = pin_int(pins, "height").unwrap_or(480) as i32;
    // GL mode is opt-in (`gl ↦ true`). Default is renderer mode
    // because SDL_Renderer and a manual GL context on the same
    // window conflict — the renderer comes up but RenderClear/
    // Present don't actually paint pixels visible to the user.
    // Demos that want raw GL (gl_swap_window etc.) pin `gl ↦ true`.
    let gl_mode = pin_bool(pins, "gl").unwrap_or(false);
    let h_key  = key(ctx, "handle");
    let g_key  = key(ctx, "gl_handle");
    let v_key  = key(ctx, "vao");
    let r_key  = key(ctx, "renderer");
    let mut bridge = SdlWindowSource::new(title, width, height, &h_key);
    let mut keys = vec![h_key.clone()];
    if gl_mode {
        bridge = bridge
            .with_gl_context_field(&g_key)
            .with_vao_field(&v_key);
        keys.push(g_key);
        keys.push(v_key);
    } else {
        bridge = bridge.with_renderer_field(&r_key);
        keys.push(r_key);
    }
    // Inline start: SDL on macOS requires CreateWindow on the
    // main thread. The runtime is single-threaded here.
    bridge.start_inline(event_tx)
        .map_err(|e| format!("SDL_Window bridge `{}.{}`: {e}",
                             ctx.claim_name, ctx.param_name))?;
    Ok(FtiInstall {
        source: Box::new(bridge),
        keys,
    })
}

fn install_gl_program(
    ctx: &FtiContext, pins: &Pins, event_tx: Sender<SchedulerEvent>,
) -> Result<FtiInstall, String> {
    let vsrc = pin_str(pins, "vertex_src").unwrap_or_default();
    let fsrc = pin_str(pins, "fragment_src").unwrap_or_default();
    let key = key(ctx, "handle");
    let mut bridge = GlProgramSource::new(vsrc, fsrc, &key);
    bridge.start_inline(event_tx)
        .map_err(|e| format!("GL_Program bridge `{}.{}`: {e}",
                             ctx.claim_name, ctx.param_name))?;
    Ok(FtiInstall { source: Box::new(bridge), keys: vec![key] })
}
