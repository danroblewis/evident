//! AST → Z3 expressions. Entry point for the `translate` module: just
//! sub-module declarations and re-exports of the public API. The
//! actual code lives in `translate/{types,datatypes,extract,
//! preprocess,declare,exprs,inline,eval}.rs`.
//!
//! See `runtime/PROGRESS.md` for the layout rationale.

mod datatypes;
mod decode_ast;
mod declare;
mod encode_ast;
mod eval;
mod exprs;
mod extract;
mod inline;
mod preprocess;
mod types;

pub mod ast_decoder {
    //! Public surface of the Z3-model → Rust-AST decoder. Mirrors
    //! the encoder's shape; used by self-hosted desugar passes
    //! that need to read back a transformed Program.
    pub use super::decode_ast::{decode_program, decode_effect, decode_effect_list,
                                  decode_ffi_arg, decode_arg_list,
                                  decode_result, decode_result_list,
                                  DecodeError};
}

pub mod ast_encoder {
    //! Public surface of the AST → Z3 datatype encoder. Used by
    //! `EvidentRuntime::encode_program_value` and the
    //! `evident dump-ast` CLI.
    pub use super::encode_ast::{encode_program, encode_body_items_into_seq,
                                 encode_effect_result, encode_effect_result_list,
                                 EncodeError};
}

// External API. Anything used by another module in this crate
// (`runtime`, `executor`, `main`) is re-exported here.
pub use eval::{build_cache, evaluate, evaluate_with_core, evaluate_with_extra_assertion,
                evaluate_with_extra_assertions,
                evaluate_with_program_and_body,
                run_cached, sample_cached_inner};
pub use preprocess::{structural_names, structural_signature, StructuralSignature};
pub mod preprocess_api { pub use super::preprocess::collect_referenced_names; }
pub use types::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, FieldKind, Value};
