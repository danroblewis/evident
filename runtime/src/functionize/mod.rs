//! Functionizer implementations + default factory. Traits: `crate::core::functionizer`.
//! ```ignore
//! let rt = EvidentRuntime::new();
//! let rt = EvidentRuntime::with_functionizer(Box::new(my_strategy));
//! ```

pub mod cranelift;
pub mod symbolic;
pub mod llm;
pub mod satisfier;
// GLSL functionizer — macOS only (headless CGL). Opt-in; not returned by default().
#[cfg(target_os = "macos")]
pub mod glsl;

pub use crate::core::{CompiledFunction, Functionizer};

/// Default functionizer (Cranelift JIT). Used by `EvidentRuntime::new()`.
pub fn default() -> Box<dyn Functionizer> {
    Box::new(cranelift::CraneliftFunctionizer)
}
