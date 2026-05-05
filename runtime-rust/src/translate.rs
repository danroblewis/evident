//! AST → Z3 expressions. Entry point for the `translate` module: just
//! sub-module declarations and re-exports of the public API. The
//! actual code lives in `translate/{types,datatypes,extract,
//! preprocess,declare,exprs,inline,eval}.rs`.
//!
//! See `runtime-rust/PROGRESS.md` for the layout rationale.

mod datatypes;
mod declare;
mod eval;
mod exprs;
mod extract;
mod inline;
mod preprocess;
mod types;

// External API. Anything used by another module in this crate
// (`runtime`, `executor`, `main`) is re-exported here.
pub use eval::{build_cache, evaluate, run_cached, sample_cached_inner};
pub use types::{CachedSchema, DatatypeRegistry, EvalResult, FieldKind, Value};
