// FTI (Foreign Type Interface) registry: type-name → install function.
// To add a new type: implement `install_<name>` and add a row to `INSTALLERS`.

use std::sync::mpsc::Sender;

use crate::core::ast::Pins;
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

// Only thread-driven timers here; all other external types use the declarative install path.
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

/// Stdlib paths silently no-op'd on import when the FTI registry already provides their types.
const SHIMMED_STDLIB_PATHS: &[&str] = &[
    "packages/sdl.ev",
    "stdlib/io.ev",
];

pub fn is_shimmed_stdlib(import_path: &str) -> bool {
    SHIMMED_STDLIB_PATHS.contains(&import_path)
}


fn pin_int(pins: &Pins, field: &str) -> Option<i64> {
    use crate::core::ast::{Expr, Mapping};
    let Pins::Named(ms) = pins else { return None };
    ms.iter().find_map(|Mapping { slot, value }|
        (slot == field).then(|| match value {
            Expr::Int(n) => Some(*n),
            _ => None,
        }).flatten())
}

fn pin_str(pins: &Pins, field: &str) -> Option<String> {
    use crate::core::ast::{Expr, Mapping};
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

