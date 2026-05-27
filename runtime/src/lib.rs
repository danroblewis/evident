//! Evident runtime — minimal Rust core (~11K lines target, side-effects via FFI).

mod core;
pub mod chc;
pub mod decompose;
pub mod effect_dispatch;
pub mod z3_eval;
pub mod z3_profile;
pub mod functionize;
// Internal: translate pass uses this via `crate::fsm_unroll`; not public API.
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
pub mod smtlib_fsm;
pub mod stdlib_path;
pub mod subscriptions;
mod event_sources;
mod fti;

pub use runtime::EvidentRuntime;
pub use core::{QueryResult, RuntimeError, Value};

pub use core::ast;

/// Parse source to a raw `Program` (pre-pass AST). Used by correctness tests that need
/// the pre-desugar schema to feed a pass and pin its output.
pub fn parse_program(src: &str) -> Result<ast::Program, String> {
    parser::parse(src).map_err(|e| e.to_string())
}
