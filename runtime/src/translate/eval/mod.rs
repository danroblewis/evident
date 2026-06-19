//! Public orchestrator entry points, in two families:
//!
//!   * **One-shot query** — `evaluate`, `evaluate_with_extra_assertion`,
//!     `evaluate_with_extra_assertions`, `evaluate_with_program_and_body`.
//!     Each builds a fresh Solver, asserts the
//!     schema's body (plus any caller extras), runs `check`, returns
//!     a `QueryResult`. Variants exist for the different shapes of
//!     "extra constraints" callers want to layer on (CLI `--given`,
//!     test scaffolding, multi-FSM coordinator).
//!
//!   * **Per-step cached query** — `build_cache` (compile once) +
//!     `run_cached` (step many times re-using the compiled solver).
//!     Used by the effect loop to amortize translate cost across ticks.
//!
//! Submodule layout (each depends only on those above it):
//!   * `solver`     — solver tuning, numeric helpers, env priming,
//!                    declare-and-assert convenience.
//!   * `decode`     — model → `Value` extractors (`extract_binding`,
//!                    enum + seq decoders).
//!   * `cached`     — `build_cache`, `run_cached`.
//!   * `extra`      — `evaluate_with_extra_assertion(s)`,
//!                    `evaluate_with_program_and_body`.
//!
//! `evaluate` itself lives in this file — it's THE entry point and
//! the shape every other variant is a copy of.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, SatResult};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
use super::declare::{apply_seq_lengths, apply_set_candidates};
use super::extract::{assert_seq_given, assert_set_given, extract_seq, extract_seq_composite, extract_set, unescape_z3_string};
use super::inline::inline_body_items;
use super::preprocess::{apply_pinned_ints, collect_pinned_ints};

mod solver;
mod decode;
mod cached;
mod extra;

use solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64, real_value_to_f64};
use decode::extract_enum_value;

pub use cached::{build_cache, run_cached};
pub use extra::{evaluate_with_extra_assertion, evaluate_with_extra_assertions, evaluate_with_program_and_body};
pub(crate) use decode::extract_binding;

// Re-export to sibling translate modules. `extract_seq_enum` is
// called by `super::eval::extract_seq_enum(...)` from
// `translate::extract`'s composite-Seq path; preserve the
// pre-split `pub(super)` visibility.
pub(super) use decode::extract_seq_enum;

/// Evaluate a single schema with optional pre-bound values, using the
/// `schemas` table to resolve user-defined types referenced inside the
/// schema body.
///
/// Sub-schema expansion: `task ∈ Task` doesn't create a Z3 const named
/// `task`. It recursively declares one Z3 const per leaf field of Task,
/// keyed under the dotted prefix `task.field` in the env. Field access
/// (parsed as `Identifier("task.field")` once we hit FieldAccess support)
/// resolves through the env directly. For v0.1 we have a flat
/// `Identifier(String)` so the parser must produce dotted names —
/// currently it only sees bare idents, but the Membership case below
/// expands them in the env regardless.
pub fn evaluate(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> EvalResult {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Pass 1: declare variables and add per-type constraints. User-defined
    // schema types expand into their leaf fields under a dotted prefix.
    // ..Passthrough imports declarations from the named claim too — any
    // variable name not already in env gets a fresh Z3 const, names that
    // collide with the parent are reused (names-match composition).
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name, .. } => {
                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name, .. } = sub {
                            if !env.contains_key(name) {
                                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
                            }
                        }
                    }
                } else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                }
            }
            BodyItem::ClaimCall { .. } => {
                // Declarations from the claim's body are added in pass 2
                // (where we have the inner env to bind into); no work here.
            }
            BodyItem::SubclaimDecl(_) => {
                // Subclaims contribute no constraints to the parent —
                // they're registered into the runtime's schemas table at
                // load time so other items can reference them.
            }
            // (Bare-identifier-as-passthrough desugared upstream — see
            // the matching note in build_cache above.)
            BodyItem::Constraint(_) => {}
        }
    }

    // Pass 1.5: pin literal-int vars from `given` + body equalities +
    // #seq length propagation. Quantifier ranges over those names then
    // unroll because translate_int yields literal IntVals.
    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    // Populate Set candidates from given Value::Set* before body
    // translation — `#s` reads `candidates.len()`.
    apply_set_candidates(&env, given);

    // Pass 2: translate body constraints and assert. Passthrough items
    // also contribute their included claim's constraints under the
    // current env. ClaimCall items translate their claim's body in a
    // fresh env where each mapping slot is pre-bound. Both passthrough
    // and ClaimCall recurse into nested claim composition (one helper
    // unifies all four entry shapes).
    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    // Pass 3: assert ground facts for each given binding. Names that
    // aren't declared in the schema are silently ignored (matches the
    // Python runtime's behavior).
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            // PinnedInt was already folded in via apply_pinned_ints from
            // this same given value — assertion is redundant. If values
            // disagree, force UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            (Var::EnumVar { ast, .. }, val @ Value::Enum { .. }) => {
                if let Some(reg) = enums {
                    if let Some(dt) = super::encode_ast::value_enum_to_datatype(val, ctx, reg) {
                        solver.assert(&ast._eq(&dt));
                    }
                }
            }
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }

    let check_result = solver.check();
    let satisfied = matches!(check_result, SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                match var {
                    Var::IntVar(i) => {
                        if let Some(val) = model.eval(i, true) {
                            if let Some(n) = val.as_i64() {
                                bindings.insert(name.clone(), Value::Int(n));
                            }
                        }
                    }
                    Var::BoolVar(b) => {
                        if let Some(val) = model.eval(b, true) {
                            if let Some(bv) = val.as_bool() {
                                bindings.insert(name.clone(), Value::Bool(bv));
                            }
                        }
                    }
                    Var::RealVar(r) => {
                        if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                            bindings.insert(name.clone(), Value::Real(real_value_to_f64(num, den)));
                        }
                    }
                    Var::StrVar(s) => {
                        if let Some(val) = model.eval(s, true) {
                            if let Some(sv) = val.as_string() {
                                bindings.insert(name.clone(), Value::Str(unescape_z3_string(&sv)));
                            }
                        }
                    }
                    Var::SeqVar { arr, len, elem } => {
                        if let Some(v) = extract_seq(arr, len, *elem, &model, ctx) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::PinnedInt(v) => {
                        bindings.insert(name.clone(), Value::Int(*v));
                    }
                    Var::SetVar { set, elem, candidates } => {
                        if let Some(v) = extract_set(set, *elem, candidates, &model, ctx) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::DatatypeSetVar { .. } => { /* unsupported in v1 */ }
                    Var::DatatypeSeqVar { arr, len, dt, fields, type_name } => {
                        let extracted = if fields.is_empty() {
                            extract_seq_enum(arr, len, type_name, *dt, &model, ctx, enums)
                        } else {
                            extract_seq_composite(arr, len, fields.as_slice(), *dt, &model, ctx, enums)
                        };
                        if let Some(v) = extracted {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::EnumVar { ast, enum_name, dt } => {
                        if let Some(v) = extract_enum_value(ast, enum_name, dt, &model, ctx, enums) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::EnumValue { .. } => { /* literal */ }
                    Var::EnumCtor { .. }  => { /* constructor */ }
                }
            }
        }
    }
    EvalResult { satisfied, bindings }
}
