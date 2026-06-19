mod datatypes;
mod declare;
mod effect_codec;
mod eval;
mod exprs;
mod extract;
mod inline;
mod preprocess;

pub mod effect_decoder {

    pub use super::effect_codec::{decode_effect, decode_effect_list,
                                  decode_ffi_arg, decode_arg_list,
                                  decode_result,
                                  decode_install_step, decode_install_step_list,
                                  InstallStep,
                                  DecodeError};
}

pub mod effect_encoder {

    pub use super::effect_codec::{effect_results_to_value,
                                  value_enum_to_datatype};
}

pub use eval::{build_cache,
                evaluate,
                evaluate_with_extra_assertion,
                evaluate_with_extra_assertions,
                run_cached};
pub(crate) use eval::extract_binding;
pub use preprocess::collect_referenced_names;
pub use crate::core::{CompiledModel, DatatypeRegistry, EnumRegistry, EvalResult, FieldKind, Value, Var};
pub use effect_codec::value_enum_to_datatype;
