//! Evident runtime — Rust port (minimal core).
//!
//! See docs/design/minimal-runtime.md and docs/plans/roadmap.md for
//! the architectural goals: ~11K Rust lines, side-effects via FFI,
//! everything else as Evident libraries.

mod core;
pub mod decompose;
pub mod effect_dispatch;
pub mod z3_eval;
pub mod z3_profile;
pub mod functionize;
// Internal: consumed by the translate pass via `crate::fsm_unroll`.
// Not part of the public API — exercised end-to-end through
// `EvidentRuntime` (see runtime/tests/fsm_unroll.rs).
mod fsm_unroll;
mod value_builders;
pub mod effect_loop;
mod ffi;
mod lexer;
mod parser;
pub mod portable;
pub mod pretty;
pub mod translate;
mod runtime;
pub mod stdlib_path;
pub mod subscriptions;
mod event_sources;
mod fti;

pub use runtime::EvidentRuntime;
pub use core::{QueryResult, RuntimeError, Value};

// Preserve `evident_runtime::ast::*` for external callers.
pub use core::ast;

/// Parse Evident source into a raw `Program` — the AST *before* any
/// load-time pass (desugar, inject, …) runs. Exposed for correctness
/// tests (e.g. `runtime/tests/desugar_correctness.rs`) that need the
/// pre-desugar schemas to feed a pass implementation and pin its output.
pub fn parse_program(src: &str) -> Result<ast::Program, String> {
    parser::parse(src).map_err(|e| e.to_string())
}
