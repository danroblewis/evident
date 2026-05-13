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
    EventSource, FrameTimer, SchedulerEvent,
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

// Types listed here use bespoke Rust bridges — only thread-driven
// timers, which can't be expressed as a one-shot install Seq. Every
// other external type (SDL_Window, GL_Program, Hostname, …) uses
// the declarative `install ∈ Seq(InstallStep)` path dispatched via
// `event_sources/declarative_install.rs`. See those types in
// `packages/sdl/` and `stdlib/runtime.ev` for their install Seqs.
const INSTALLERS: &[(&str, FtiInstallFn)] = &[
    ("FrameClock", install_frame_clock),
    ("Timer",      install_timer),
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

