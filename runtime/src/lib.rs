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
mod value_builders;
pub mod effect_loop;
mod ffi;
mod lexer;
mod parser;
pub mod pretty;
pub mod translate;
mod runtime;
mod fti;

pub use runtime::EvidentRuntime;
pub use core::{QueryResult, RuntimeError, Value};

// Preserve `evident_runtime::ast::*` for external callers.
pub use core::ast;
