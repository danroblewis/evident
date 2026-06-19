//! Per-step cached query path. Used by the multi-FSM scheduler to
//! amortize the translate cost across ticks.
//!
//!   * `build_cache`             — translate the schema's body once
//!                                  into a fresh solver; returns a
//!                                  `CachedSchema` callers reuse.
//!   * `run_cached`              — per-tick: push, assert givens,
//!                                  check, extract model, pop. Reuses
//!                                  all cached constraint translation.
//!   * `sample_cached_inner`     — n-distinct-models sampling on top
//!                                  of a cache; one outer push +
//!                                  per-iteration blocking clauses.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, SatResult};

use crate::core::ast::*;
use crate::core::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
use super::super::declare::{apply_seq_lengths, apply_set_candidates};
use super::super::extract::{assert_seq_given, assert_set_given, extract_seq, extract_seq_composite, extract_set, unescape_z3_string};
use super::super::inline::inline_body_items;
use super::super::preprocess::{apply_pinned_ints, collect_pinned_ints};
use super::solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64, real_value_to_f64};
use super::decode::{extract_enum_value, extract_seq_enum};

/// Translate the schema's body once into a fresh solver and return a
/// `CachedSchema` that subsequent queries can reuse via push/pop.
///
/// `given` is the set of values that should be folded into the cache
/// at build time — typically the structural subset (names appearing
/// in quantifier bounds), so the cache contains the right unrolled
/// shape. Non-structural givens can be left in or out; they won't
/// change the cache's correctness, but if included they're folded as
/// `Var::PinnedInt` and any subsequent `run_cached` with a different
/// value for that name will hit the PinnedInt-mismatch UNSAT path.
/// The runtime's cache layer takes care of this by passing only the
/// structural subset and rebuilding on signature change.
pub fn build_cache(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    given: &HashMap<String, Value>,
    arith_solver: u32,
) -> CachedSchema<'static> {
    // Mirror evaluate_with_extra_assertions: install the thread-local
    // EnumRegistry so the translator can resolve enum constructors
    // (e.g. `LibCall(..., ⟨⟩)`) appearing in body items. Without this,
    // those constraints silently drop and outputs end up undefined.
    let _enum_guard = super::super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Same two passes as evaluate(), but writing into the cache's
    // solver instead of a fresh one each time.
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
                }
            }
            // (Bare-identifier-as-passthrough is desugared upstream by
            // stdlib/passes/desugar_passthrough.ev; by the time we
            // walk body items here, those constraints are already
            // BodyItem::Passthrough nodes handled above.)
            _ => {}
        }
    }

    // Pass 1.5: pin literal-int vars + propagate seq lengths. `given`
    // contributes both Int values (for pinned) and Seq* lengths (for
    // seq_lens), so a structural value the runtime decided to bake into
    // the cache (e.g. `cells_count = 80` from a config menu) makes
    // every `∀ i ∈ {0..cells_count - 1}` unroll correctly.
    let seq_lens = super::super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    // Populate Set var candidates from given Value::Set* — needed before
    // body translation so `#s` cardinality folds to a literal count.
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    CachedSchema { env, solver, arith_solver }
}

/// Sample up to `n` distinct models from the cached schema's solver.
///
/// Strategy: one outer push for the per-query givens. Inside the outer
/// frame, loop:
///   1. solver.check(); if UNSAT, break.
///   2. Extract model into bindings; push onto result vec.
///   3. Build a blocking clause: ¬(AND of `binding == value` for every
///      *scalar* binding) — Bool, Int, Str. Sequence/set/composite
///      values are skipped from the clause for v1; schemas whose only
///      bindings are Seq* will return duplicates (documented limitation).
///   4. Assert the blocking clause inside the outer frame, so it
///      persists across iterations but is discarded by the outer pop.
///
/// Final pop restores the cached solver to exactly its build-time state.
pub fn sample_cached_inner<'ctx>(
    cached: &CachedSchema<'ctx>,
    given: &HashMap<String, Value>,
    n: usize,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Vec<HashMap<String, Value>> {
    cached.solver.push();

    // Apply per-query givens (mirrors run_cached).
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => cached.solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    cached.solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    cached.solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }

    let mut out: Vec<HashMap<String, Value>> = Vec::new();
    for _ in 0..n {
        if !matches!(cached.solver.check(), SatResult::Sat) {
            break;
        }
        let Some(model) = cached.solver.get_model() else { break };

        let mut bindings: HashMap<String, Value> = HashMap::new();
        // Collect scalar `(z3 expr, value)` pairs as we extract; we'll
        // turn them into a blocking clause at the end.
        let mut block_terms: Vec<Bool<'ctx>> = Vec::new();

        for (name, var) in cached.env.iter() {
            match var {
                Var::IntVar(i) => {
                    if let Some(v) = model.eval(i, true).and_then(|x| x.as_i64()) {
                        bindings.insert(name.clone(), Value::Int(v));
                        block_terms.push(i._eq(&Int::from_i64(ctx, v)));
                    }
                }
                Var::BoolVar(b) => {
                    if let Some(v) = model.eval(b, true).and_then(|x| x.as_bool()) {
                        bindings.insert(name.clone(), Value::Bool(v));
                        block_terms.push(b._eq(&Bool::from_bool(ctx, v)));
                    }
                }
                Var::RealVar(r) => {
                    if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                        let f = real_value_to_f64(num, den);
                        bindings.insert(name.clone(), Value::Real(f));
                        block_terms.push(r._eq(&real_from_f64(ctx, f)));
                    }
                }
                Var::StrVar(s) => {
                    if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                        let v = unescape_z3_string(&v);
                        bindings.insert(name.clone(), Value::Str(v.clone()));
                        if let Ok(lit) = Z3Str::from_str(ctx, &v) {
                            block_terms.push(s._eq(&lit));
                        }
                    }
                }
                Var::SeqVar { arr, len, elem } => {
                    if let Some(v) = extract_seq(arr, len, *elem, &model, ctx) {
                        bindings.insert(name.clone(), v);
                    }
                    // Seq blocking is non-trivial (would need ¬(arr[0]=v0
                    // ∧ … ∧ len=n)) — skipped for v1. Documented limitation.
                }
                Var::PinnedInt(v) => {
                    bindings.insert(name.clone(), Value::Int(*v));
                    // PinnedInts are constants, not solver vars — no
                    // useful blocking term to add.
                }
                Var::SetVar { set, elem, candidates } => {
                    if let Some(v) = extract_set(set, *elem, candidates, &model, ctx) {
                        bindings.insert(name.clone(), v);
                    }
                    // Set blocking is non-trivial (would need to negate
                    // membership for each candidate ∧ cardinality); skipped
                    // for v1.
                }
                Var::DatatypeSetVar { .. } => {
                    // Composite-element Set extraction is unsupported in v1
                    // — we'd need per-field model-eval over each candidate's
                    // accessor application. The constraint side (∈, =, ⊆,
                    // #) all work; check/all_solutions just omits the
                    // binding from the output.
                }
                Var::DatatypeSeqVar { arr, len, dt, fields, type_name } => {
                    let extracted = if fields.is_empty() {
                        extract_seq_enum(arr, len, type_name, *dt, &model, ctx, enums)
                    } else {
                        extract_seq_composite(arr, len, fields.as_slice(), *dt, &model, ctx, enums)
                    };
                    if let Some(v) = extracted {
                        bindings.insert(name.clone(), v);
                    }
                    // Blocking on composite/enum seq elements is non-trivial
                    // (same shape as primitive seqs); skipped for v1.
                }
                Var::EnumVar { ast, enum_name, dt } => {
                    if let Some(v) = extract_enum_value(ast, enum_name, dt, &model, ctx, enums) {
                        bindings.insert(name.clone(), v.clone());
                        // Push a positive `var = chosen` term — the
                        // outer code AND-s the term list and asserts
                        // NOT (this iteration's complete assignment).
                        // Use the model-evaluated value directly so
                        // payload variants block correctly (their
                        // constructors take arguments; can't call
                        // ctor.apply(&[]) the way nullary variants can).
                        if let Some(chosen) = model.eval(ast, true) {
                            block_terms.push(ast._eq(&chosen));
                        }
                    }
                }
                Var::EnumValue { .. } => {
                    // Variant literals have no model-side state; they're
                    // constants pre-populated into env at evaluate time.
                }
                Var::EnumCtor { .. } => {
                    // Constructor reference; no per-model state.
                }
            }
        }

        out.push(bindings);

        // If we have no scalar terms to block on at all, we'd loop
        // forever returning the same model. Bail.
        if block_terms.is_empty() {
            break;
        }
        let refs: Vec<&Bool<'ctx>> = block_terms.iter().collect();
        let conj = Bool::and(ctx, &refs);
        cached.solver.assert(&conj.not());
    }

    cached.solver.pop(1);
    out
}

/// Per-query work: push, assert givens against the cached env, check,
/// extract model, pop. Reuses all the constraint translation already
/// in the cache.
pub fn run_cached<'ctx>(
    cached: &CachedSchema<'ctx>,
    given: &HashMap<String, Value>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> EvalResult {
    // Solver params were set once in build_cache; no per-call tuning here.
    cached.solver.push();
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => cached.solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            // PinnedInt was already folded in via apply_pinned_ints from
            // this same given value, so the assertion is redundant. If
            // the values disagree (caller passes a different int after a
            // body equality pinned the var), force UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    cached.solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    cached.solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }
    let check_t0 = std::time::Instant::now();
    let check_result = cached.solver.check();
    let check_dt = check_t0.elapsed();
    let satisfied = matches!(check_result, SatResult::Sat);
    let mut bindings = HashMap::new();
    let extract_t0 = std::time::Instant::now();
    if satisfied {
        if let Some(model) = cached.solver.get_model() {
            for (name, var) in cached.env.iter() {
                match var {
                    Var::IntVar(i) => {
                        if let Some(v) = model.eval(i, true).and_then(|x| x.as_i64()) {
                            bindings.insert(name.clone(), Value::Int(v));
                        }
                    }
                    Var::BoolVar(b) => {
                        if let Some(v) = model.eval(b, true).and_then(|x| x.as_bool()) {
                            bindings.insert(name.clone(), Value::Bool(v));
                        }
                    }
                    Var::RealVar(r) => {
                        if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                            bindings.insert(name.clone(), Value::Real(real_value_to_f64(num, den)));
                        }
                    }
                    Var::StrVar(s) => {
                        if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                            bindings.insert(name.clone(), Value::Str(unescape_z3_string(&v)));
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
                    Var::EnumValue { .. } => { /* literal, no model state */ }
                    Var::EnumCtor { .. }  => { /* constructor reference, no model state */ }
                }
            }
        }
    }
    let _ = (check_dt, extract_t0);
    cached.solver.pop(1);
    EvalResult { satisfied, bindings, unsat_core_items: None }
}
