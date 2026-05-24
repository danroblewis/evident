//! AST `Expr` → Z3 expression translators (Int / Bool / String / Real)
//! and the helpers they share.
//!
//! This module was split out of a single 3116-line `exprs.rs`. Each
//! sibling file owns one of the original file's numbered sections:
//!
//!   * `mapping.rs`     — ClaimCall mapping resolution: `resolve_mapping`,
//!                        `expr_as_var`, the Seq-element field-chain binders.
//!   * `enums.rs`       — Enum / Cons-chain helpers: `resolve_enum_ast`,
//!                        `build_cons_chain`, Seq-payload constructor args.
//!   * `seq_field.rs`   — `SeqHandleRef` + `resolve_seq_handle` /
//!                        `resolve_seq_field`.
//!   * `scalar.rs`      — per-sort translators `translate_str` / `_int` /
//!                        `_real` plus the Real-literal helper `real_from_f64`.
//!   * `record_lift.rs` — record / vector lifting: `lift_record_op`, leaf
//!                        enumeration, record-ref substitution.
//!   * `seq_eq.rs`      — Seq/Set-equality translation: `translate_seq_lit_eq`,
//!                        `translate_seq_index_assign`, composite-Seq plumbing.
//!   * `bool.rs`        — `translate_bool`, the big Bool dispatcher.
//!   * `quant.rs`       — quantifier unrolling (`∀` / `∃` over ranges,
//!                        seqs, `coindexed`, `edges`), split from `bool.rs`.
//!   * `match_expr.rs`  — match-expression translator: `translate_match_arms`,
//!                        `fold_arms_to_ite`.
//!   * `range.rs`       — literal-range folder: `literal_range`.
//!
//! This file (`mod.rs`) keeps the thread-local translation context the
//! siblings share — the active EnumRegistry pointer and the
//! SeqLit-target enum hint, plus their RAII guards.

use z3::DatatypeSort;

use crate::core::EnumRegistry;

mod mapping;
mod enums;
mod seq_field;
mod scalar;
mod record_lift;
mod seq_eq;
mod bool;
mod quant;
mod match_expr;
mod range;

// Public surface consumed by sibling `translate/` modules. `inline.rs`
// imports `resolve_mapping` + `translate_bool`; the `eval/*` modules use
// `EnumRegistryGuard` (a `pub` item defined below, so no re-export
// needed for it).
pub(super) use mapping::resolve_mapping;
pub(super) use bool::translate_bool;

// ── Section 1: Thread-local context (active enums + target hint) ─────

thread_local! {
    /// Active EnumRegistry for the current translation. Set by
    /// `with_enums(...)` (called from each `evaluate*` entry point in
    /// eval.rs) and restored on drop. Read by `translate_match_arms`
    /// to look up the DatatypeSort of a payload field whose declared
    /// type is itself an enum (so the binding can become a proper
    /// `Var::EnumVar` for further pattern matching).
    ///
    /// Stored as a raw `*const EnumRegistry` because the registry's
    /// lifetime is tied to `EvidentRuntime` (which lives for the whole
    /// translation), but we can't carry a `'static` reference through
    /// thread-locals. The pointer is set/cleared via the RAII guard
    /// `EnumRegistryGuard`; readers borrow it back as `&EnumRegistry`
    /// inside the guard's lifetime.
    static ACTIVE_ENUMS: std::cell::Cell<Option<*const EnumRegistry>> =
        const { std::cell::Cell::new(None) };
}

/// RAII guard: stash an EnumRegistry pointer in thread-local for the
/// duration of a translation. Restores the previous value on drop so
/// nested calls compose correctly.
pub struct EnumRegistryGuard {
    prev: Option<*const EnumRegistry>,
}

impl EnumRegistryGuard {
    pub fn new(enums: Option<&EnumRegistry>) -> Self {
        let new_ptr = enums.map(|r| r as *const EnumRegistry);
        let prev = ACTIVE_ENUMS.with(|c| {
            let was = c.get();
            c.set(new_ptr);
            was
        });
        Self { prev }
    }
}

impl Drop for EnumRegistryGuard {
    fn drop(&mut self) {
        ACTIVE_ENUMS.with(|c| c.set(self.prev));
    }
}

/// Run `f` with the active EnumRegistry borrowed if one is set.
pub(super) fn with_active_enums<R>(f: impl FnOnce(Option<&EnumRegistry>) -> R) -> R {
    let ptr = ACTIVE_ENUMS.with(|c| c.get());
    // SAFETY: `ptr` was set by an EnumRegistryGuard whose Drop hasn't
    // run yet (translation is single-threaded, the guard outlives the
    // call stack that uses it).
    let opt = ptr.map(|p| unsafe { &*p });
    f(opt)
}

thread_local! {
    /// Currently expected enum type for SeqLit-as-Cons-chain lowering
    /// inside enum-typed contexts. Set by `translate_bool`'s Eq path
    /// when the LHS is enum-typed; read by `resolve_enum_ast`'s
    /// SeqLit arm. Holds (enum_name, dt).
    static TARGET_ENUM_HINT: std::cell::RefCell<Option<(String, &'static DatatypeSort<'static>)>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with `target` as the current SeqLit-target hint. Restores
/// the previous value on return so nested calls compose.
pub(super) fn with_target_enum_hint<R>(
    target: Option<(String, &'static DatatypeSort<'static>)>,
    f: impl FnOnce() -> R,
) -> R {
    let prev = TARGET_ENUM_HINT.with(|c| c.replace(target));
    let r = f();
    TARGET_ENUM_HINT.with(|c| { *c.borrow_mut() = prev; });
    r
}

pub(super) fn current_target_enum() -> Option<(String, &'static DatatypeSort<'static>)> {
    TARGET_ENUM_HINT.with(|c| c.borrow().clone())
}
