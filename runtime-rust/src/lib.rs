//! Evident runtime — Rust port (experimental).
//!
//! See `runtime-rust/PROGRESS.md` for current status and `NOTES.md` for
//! Evident-language gotchas worth knowing.

pub mod ast;
pub mod effect_dispatch;
pub mod effect_loop;
pub mod ffi;
pub mod lexer;
pub mod parser;
pub mod pretty;
pub mod translate;
pub mod runtime;
pub mod executor;
pub mod plugins;
pub mod trace_runner;
pub mod glsl;
pub mod smtlib;

pub use runtime::{EvidentRuntime, QueryResult, Value};
