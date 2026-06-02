//! AST → Z3 expressions: sub-module declarations and public API re-exports.
//! Actual code lives in `translate/{datatypes,extract,preprocess,declare,exprs,inline,eval,encode_ast,decode_ast}.rs`.

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
    //! Z3-model → Rust-AST decoder; `decode_list`/`decode_str` are the shared cons-list reader.
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
    //! AST → `Value`/Z3-datatype marshaler. `encode_*` targets Z3 Datatype (given-pinning);
    //! `*_to_value` targets `Value::Enum` (shared marshaler for stack-FSM self-hosted passes).
    pub use super::encode_ast::{encode_program, encode_body_items_into_seq,
                                 encode_effect_result,
                                 effect_results_to_value,
                                 value_enum_to_datatype,
                                 EncodeError,
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

pub use eval::{build_cache, evaluate,
                evaluate_with_extra_assertion,
                evaluate_with_extra_assertions,
                evaluate_with_program_and_body,
                run_cached};
pub(crate) use extract::z3_string;
pub use preprocess::{collect_referenced_names, structural_names, structural_signature,
                     StructuralSignature};
pub use crate::core::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, FieldKind, Value, Var};
pub use encode_ast::value_enum_to_datatype;
