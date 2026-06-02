//! Evident runtime — minimal Rust core. Language syntax, claim composition, Z3 model.

mod core;
mod lexer;
mod parser;
mod translate;
mod runtime;
mod z3_ctx;

pub use runtime::EvidentRuntime;
pub use core::{QueryResult, RuntimeError, Value};
pub use core::ast;

/// Parse source to a raw `Program` (pre-pass AST).
pub fn parse_program(src: &str) -> Result<ast::Program, String> {
    parser::parse(src).map_err(|e| e.to_string())
}
