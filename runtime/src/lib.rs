mod core;
pub mod functionize;
pub mod trampoline;
pub mod ffi;
mod lexer;
mod parser;
pub mod encode;
mod session;

#[cfg(test)]
mod tests;

pub use session::EvidentRuntime;
pub use core::{QueryResult, RuntimeError, Value};

pub use core::ast;
