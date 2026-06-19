//! AST → Z3 expressions. Entry point for the `translate` module: just
//! sub-module declarations and re-exports of the public API. The
//! actual code lives in `translate/{types,datatypes,extract,
//! preprocess,declare,exprs,inline,eval,effect_codec}.rs`.
//!
//! Public API surface (the *only* items external callers may use).
//! Adding to either list is a deliberate expansion, not an oversight:
//!
//!   * From `eval` — the orchestrator entry points:
//!       `evaluate`,
//!       `evaluate_with_extra_assertion`,
//!       `evaluate_with_extra_assertions`,
//!       `build_cache`, `run_cached`.
//!     The `_with_*` variants exist because the runtime facade has
//!     several callers (CLI givens, multi-FSM scheduler extras) and
//!     each needs a slightly different extra-assertion shape.
//!
//!   * From `preprocess` — the `collect_referenced_names` helper
//!     consumed by `commands/test.rs`'s diagnostic path.
//!
//!   * From `types` — the typed-binding + model-output data types:
//!     `CompiledModel`, `DatatypeRegistry`, `EnumRegistry`,
//!     `EvalResult`, `FieldKind`, `Value`. `EnumRegistry` is part
//!     of the API because the runtime facade owns one and passes
//!     references into `evaluate*`.
//!
//!   * `effect_encoder` / `effect_decoder` namespaces — the
//!     Effect/Result value codec the executor uses to encode
//!     `Result`/`Value::Enum` world fields for the `given` map and to
//!     decode `Effect`s / `Result`s / FFI args / install steps back
//!     from a solved model. Kept as namespaces (rather than flat
//!     `pub use`) because callers consume them as
//!     `effect_encoder::...` / `effect_decoder::...` — the qualified
//!     form makes the codec boundary visible at the call site.

mod datatypes;
mod declare;
mod effect_codec;
mod eval;
mod exprs;
mod extract;
mod inline;
mod preprocess;

pub mod effect_decoder {
    //! Public surface of the Effect/Result value decoder. Used by the
    //! executor to read back `Effect`s, `Result`s, FFI args, and the
    //! declarative `Seq(InstallStep)` install path from a model.
    pub use super::effect_codec::{decode_effect, decode_effect_list,
                                  decode_ffi_arg, decode_arg_list,
                                  decode_result,
                                  decode_install_step, decode_install_step_list,
                                  InstallStep,
                                  DecodeError};
}

pub mod effect_encoder {
    //! Public surface of the Effect/Result value encoder. Used by the
    //! executor to build `Result` values and to re-encode
    //! `Value::Enum` world fields for the `given` map.
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
