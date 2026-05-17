//! Evident runtime — Rust port (minimal core).
//!
//! See docs/design/minimal-runtime.md and docs/plans/roadmap.md for
//! the architectural goals: ~11K Rust lines, side-effects via FFI,
//! everything else as Evident libraries.

pub mod ast;
pub mod decompose;
pub mod effect_dispatch;
pub mod functionize;
pub mod z3_eval;
pub mod z3_profile;
pub mod cranelift_jit;
mod rust_vm;
mod value_builders;
pub mod effect_loop;
mod ffi;
mod lexer;
mod parser;
pub mod pretty;
pub mod translate;
mod runtime;
pub mod subscriptions;
mod event_sources;
mod fti;

pub use runtime::{EvidentRuntime, QueryResult, Value};
