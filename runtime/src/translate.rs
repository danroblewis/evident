//! AST → Z3 expressions. Entry point for the `translate` module: just
//! sub-module declarations and re-exports of the public API. The
//! actual code lives in `translate/{types,datatypes,extract,
//! preprocess,declare,exprs,inline,eval,encode_ast,decode_ast}.rs`.
//!
//! Public API surface (the *only* items external callers may use).
//! Adding to either list is a deliberate expansion, not an oversight:
//!
//!   * From `eval` — the orchestrator entry points:
//!       `evaluate`, `evaluate_with_core`,
//!       `evaluate_with_extra_assertion`,
//!       `evaluate_with_extra_assertions`,
//!       `evaluate_with_program_and_body`,
//!       `build_cache`, `run_cached`, `sample_cached_inner`.
//!     The `_with_*` variants exist because the runtime facade has
//!     several callers (CLI givens, multi-FSM scheduler extras,
//!     unsat-core extraction) and each needs a slightly different
//!     extra-assertion shape.
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
//!   * `ast_encoder` / `ast_decoder` namespaces — the AST↔Z3-Datatype
//!     bridge for self-hosted compiler passes. Kept as namespaces
//!     (rather than flat `pub use`) because callers consume them as
//!     `ast_encoder::encode_program(...)` / `ast_decoder::decode_*` —
//!     the qualified form makes the bridge boundary visible at the
//!     call site.

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
    //! Public surface of the Z3-model → Rust-AST decoder. Mirrors
    //! the encoder's shape; used by self-hosted desugar passes
    //! that need to read back a transformed Program.
    //!
    //! `decode_list` + `decode_str` are THE shared cons-list reader
    //! (session UU): the read-side twin of `ast_encoder`'s `*_to_value`
    //! marshaler, used by any port that drives a stack-FSM over the
    //! marshaler's output and decodes a cons-list accumulator back.
    pub use super::decode_ast::{decode_program, decode_schema_decl,
                                  decode_effect, decode_effect_list,
                                  decode_ffi_arg, decode_arg_list,
                                  decode_result,
                                  decode_install_step, decode_install_step_list,
                                  decode_list, decode_str, decode_expr,
                                  InstallStep,
                                  DecodeError};
}

pub mod ast_encoder {
    //! Public surface of the AST → `Value`/Z3-datatype marshaler.
    //!
    //! Two families share this namespace:
    //!   * `encode_*` — AST → Z3 `Datatype` (the `given`-pinning path
    //!     used by `EvidentRuntime::encode_program_value`, `dump-ast`,
    //!     and reflection's Z3 assertions).
    //!   * `*_to_value` — AST → `Value::Enum` tree (THE shared marshaler,
    //!     session UU). A `pub` family so every self-hosted port reuses
    //!     one encoder instead of hand-rolling its own (which QQ showed
    //!     re-pays the marshaling tax per pass). Its list-typed fields are
    //!     poppable Cons enums (`BodyItemList`, `ExprList`, …), directly
    //!     consumable by a stack-FSM walk; pair with `ast_decoder`'s
    //!     `decode_list` to read accumulators back.
    pub use super::encode_ast::{encode_program, encode_body_items_into_seq,
                                 encode_effect_result,
                                 effect_results_to_value,
                                 value_enum_to_datatype,
                                 EncodeError,
                                 // ── the shared AST → Value marshaler ──
                                 program_to_value, schema_decl_to_value,
                                 schema_list_to_value, body_item_to_value,
                                 body_item_list_to_value, pins_to_value,
                                 mapping_to_value, mapping_list_to_value,
                                 expr_to_value, expr_list_to_value,
                                 string_list_to_value, binop_to_value,
                                 keyword_to_value, match_arm_to_value,
                                 match_arm_list_to_value, match_pattern_to_value,
                                 bind_list_to_value, enum_decl_to_value,
                                 enum_decl_list_to_value, enum_variant_to_value,
                                 enum_variant_list_to_value, enum_field_to_value,
                                 enum_field_list_to_value};
}

pub use eval::{analyze_decomposition, build_cache, classify_components,
                ClassifiedComponent, evaluate, evaluate_with_core,
                evaluate_with_extra_assertion,
                evaluate_with_extra_assertions,
                evaluate_with_program_and_body,
                run_cached, sample_cached_inner};
pub(crate) use eval::extract_binding;
pub use preprocess::{collect_referenced_names, structural_names, structural_signature,
                     StructuralSignature};
pub use crate::core::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, FieldKind, Value, Var};
pub use encode_ast::value_enum_to_datatype;
