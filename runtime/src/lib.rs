//! Evident runtime — minimal Rust core (~11K lines target, side-effects via FFI).

mod core;
pub mod effect_dispatch;
pub mod effect_loop;
mod ffi;
mod lexer;
mod parser;
pub mod portable;
pub mod translate;
mod runtime;
pub mod stdlib_path;
pub mod subscriptions;
mod event_sources;
mod fti;
// Single global serialization point for Z3 `Context` creation (thread-safety).
mod z3_ctx;

pub use runtime::EvidentRuntime;
pub use core::{QueryResult, RuntimeError, Value};

pub use core::ast;

/// Parse source to a raw `Program` (pre-pass AST). Used by correctness tests that need
/// the pre-desugar schema to feed a pass and pin its output.
pub fn parse_program(src: &str) -> Result<ast::Program, String> {
    parser::parse(src).map_err(|e| e.to_string())
}
