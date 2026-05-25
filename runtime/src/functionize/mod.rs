//! Functionizer concrete implementations + the default factory.
//!
//! The traits themselves (`Functionizer`, `CompiledFunction`) live in
//! `crate::core::functionizer`; this module owns the concrete
//! implementations (currently just Cranelift) and the
//! `default()` factory that `EvidentRuntime::new()` uses.
//!
//! Selection happens at runtime construction:
//!
//! ```ignore
//! let rt = EvidentRuntime::new();                 // default: Cranelift
//! let rt = EvidentRuntime::with_functionizer(    // custom
//!     Box::new(my_strategy));
//! ```

pub mod cranelift;
pub mod symbolic;
pub mod llm;
pub mod satisfier;
// GLSL fragment-shader functionizer — macOS only (headless CGL context).
// Opt-in; the default factory below never returns it.
#[cfg(target_os = "macos")]
pub mod glsl;

// Re-export the traits so existing `crate::core::Functionizer`
// / `crate::core::CompiledFunction` paths keep resolving.
pub use crate::core::{CompiledFunction, Functionizer};

/// Default functionizer (Cranelift JIT). Used by `EvidentRuntime::new()`.
pub fn default() -> Box<dyn Functionizer> {
    Box::new(cranelift::CraneliftFunctionizer)
}
