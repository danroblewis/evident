mod core;
pub mod functionize;
pub mod trampoline;
pub mod ffi;
mod lexer;
mod parser;
pub mod encode;
mod runtime;

#[cfg(test)]
mod tests;

pub use runtime::EvidentRuntime;
pub use core::{QueryResult, RuntimeError, Value};

pub use core::ast;
