//! AST → Z3 expressions: sub-module declarations and public API re-exports.

mod datatypes;
mod declare;
mod encode_ast;
mod eval;
mod exprs;
mod extract;
mod inline;
mod preprocess;

pub use eval::{build_cache, evaluate, run_cached};
pub(crate) use extract::z3_string;
pub use preprocess::{structural_names, structural_signature, StructuralSignature};
pub use crate::core::{CachedSchema, DatatypeRegistry};
