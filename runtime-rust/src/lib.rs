//! Evident runtime — Rust port (experimental).
//!
//! See `runtime-rust/PROGRESS.md` for current status and `NOTES.md` for
//! Evident-language gotchas worth knowing.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod translate;
pub mod runtime;
pub mod executor;

pub use runtime::{EvidentRuntime, QueryResult, Value};
