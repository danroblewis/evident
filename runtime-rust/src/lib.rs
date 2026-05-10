//! Evident runtime — Rust port (minimal core).
//!
//! See docs/design/minimal-runtime.md and docs/plans/roadmap.md for
//! the architectural goals: ~11K Rust lines, side-effects via FFI,
//! everything else as Evident libraries.

pub mod ast;
pub mod effect_dispatch;
pub mod effect_loop;
pub mod ffi;
pub mod lexer;
pub mod parser;
pub mod pretty;
pub mod translate;
pub mod runtime;
pub mod subscriptions;
pub mod event_sources;
pub mod fti;

pub use runtime::{EvidentRuntime, QueryResult, Value};
