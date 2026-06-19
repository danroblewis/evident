//! AST → Z3 expressions. Entry point for the `translate` module: just
//! sub-module declarations and re-exports of the public API. The
//! actual code lives in `translate/{types,datatypes,extract,
//! preprocess,declare,exprs,inline,eval,encode_ast,decode_ast}.rs`.
//!
//! Public API surface (the *only* items external callers may use).
//! Adding to either list is a deliberate expansion, not an oversight:
//!
//!   * From `eval` — the orchestrator entry points:
//!       `evaluate`,
//!       `evaluate_with_extra_assertion`,
//!       `evaluate_with_extra_assertions`,
//!       `evaluate_with_program_and_body`,
//!       `build_cache`, `run_cached`.
//!     The `_with_*` variants exist because the runtime facade has
//!     several callers (CLI givens, multi-FSM scheduler extras) and
//!     each needs a slightly different extra-assertion shape.
//!
//!   * From `preprocess` — pre-translation helpers consumed by the
//!     runtime cache layer and by `commands/test.rs`'s diagnostic
//!     path: `structural_names`, `structural_signature`,
//!     `StructuralSignature`, `collect_referenced_names`.
//!
//!   * From `types` — the typed-binding + model-output data types:
//!     `CachedSchema`, `DatatypeRegistry`, `EnumRegistry`,
//!     `EvalResult`, `FieldKind`, `Value`. `EnumRegistry` is part
//!     of the API because the runtime facade owns one and passes
//!     references into `evaluate*`.
//!
//!   * `ast_encoder` / `ast_decoder` namespaces — the value↔Z3-Datatype
//!     bridge the executor uses to encode `Effect`/`Result`/`BodyItem`
//!     fragments and decode them back from a solved model. Kept as
//!     namespaces (rather than flat `pub use`) because callers consume
//!     them as `ast_encoder::...` / `ast_decoder::...` — the qualified
//!     form makes the bridge boundary visible at the call site.

mod datatypes;
mod decode_ast;
mod declare;
mod encode_ast;
mod eval;
mod exprs;
mod extract;
mod inline;
mod preprocess;

pub mod ast_decoder {
    //! Public surface of the Z3-model → Rust-AST decoder. Used by the
    //! executor to read back `Effect`s, `Result`s, FFI args, and the
    //! declarative `Seq(InstallStep)` install path from a model.
    pub use super::decode_ast::{decode_effect, decode_effect_list,
                                  decode_ffi_arg, decode_arg_list,
                                  decode_result,
                                  decode_install_step, decode_install_step_list,
                                  InstallStep,
                                  DecodeError};
}

pub mod ast_encoder {
    //! Public surface of the AST → Z3 datatype encoder. Used by the
    //! executor to encode `BodyItem`s / `Effect`s / `Result`s and to
    //! re-encode `Value::Enum` world fields for the `given` map.
    pub use super::encode_ast::{encode_body_items_into_seq,
                                 encode_effect_result,
                                 effect_results_to_value,
                                 value_enum_to_datatype,
                                 EncodeError};
}

pub use eval::{build_cache,
                evaluate,
                evaluate_with_extra_assertion,
                evaluate_with_extra_assertions,
                evaluate_with_program_and_body,
                run_cached};
pub(crate) use eval::extract_binding;
pub use preprocess::{collect_referenced_names, structural_names, structural_signature,
                     StructuralSignature};
pub use crate::core::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, FieldKind, Value, Var};
pub use encode_ast::value_enum_to_datatype;
