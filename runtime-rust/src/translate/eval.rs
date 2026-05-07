//! The four public orchestrator entry points: `evaluate` (one-shot
//! query), `build_cache` + `run_cached` (per-step cached query for the
//! executor), `sample_cached_inner` (n-distinct-models for sampling).

use std::collections::{HashMap, HashSet};
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, Params, SatResult, Solver};

/// Set `smt.arith.solver` to `arith_solver` on `solver`. Pass `0` to
/// skip (lets Z3 use its built-in default). The chosen value depends
/// on workload — the runtime's auto-tuner decides which to use; this
/// helper is the dumb mechanism. See `runtime::SolveHistory` for the
/// policy.
fn apply_solver_tuning(ctx: &Context, solver: &Solver, arith_solver: u32) {
    if arith_solver == 0 { return; }
    let mut params = Params::new(ctx);
    params.set_u32("smt.arith.solver", arith_solver);
    solver.set_params(&params);
}

use crate::ast::*;
use super::types::{CachedSchema, DatatypeRegistry, EvalResult, Value, Var};
use super::declare::declare_var;
use super::extract::{assert_seq_given, extract_seq, extract_seq_composite};
use super::inline::inline_body_items;
use super::preprocess::{apply_pinned_ints, apply_seq_lengths, collect_pinned_ints, collect_seq_lengths};

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
    given: &HashMap<String, Value>,
    arith_solver: u32,
) -> CachedSchema<'static> {
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();

    // Same two passes as evaluate(), but writing into the cache's
    // solver instead of a fresh one each time.
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
                            }
                        }
                    }
                }
            }
            // Bare-identifier names-match passthrough: a `BodyItem::Constraint(
            // Identifier(name))` whose `name` is a known claim/type behaves
            // exactly like `..ClaimName`. The parser leaves bare idents as
            // Constraint(Identifier(...)) because it can't disambiguate at
            // parse time (the same shape might be a Bool variable). We
            // resolve here, where `schemas` is in scope.
            BodyItem::Constraint(Expr::Identifier(name)) => {
                if let Some(claim) = schemas.get(name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 1.5: pin literal-int vars + propagate seq lengths. `given`
    // contributes both Int values (for pinned) and Seq* lengths (for
    // seq_lens), so a structural value the runtime decided to bake into
    // the cache (e.g. `cells_count = 80` from a config menu) makes
    // every `∀ i ∈ {0..cells_count - 1}` unroll correctly.
    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let mut visited: HashSet<String> = HashSet::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, &mut visited);

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
) -> Vec<HashMap<String, Value>> {
    cached.solver.push();

    // Apply per-query givens (mirrors run_cached).
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx) {
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
                Var::StrVar(s) => {
                    if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
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
                Var::SetVar { .. } => {
                    // Same as run_cached: SetVars aren't enumerable; skip.
                }
                Var::DatatypeSeqVar { arr, len, dt, fields, .. } => {
                    if let Some(v) = extract_seq_composite(
                        arr, len, fields.as_slice(), *dt, &model, ctx)
                    {
                        bindings.insert(name.clone(), v);
                    }
                    // Blocking on composite seq elements is non-trivial
                    // (same shape as primitive seqs); skipped for v1.
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
) -> EvalResult {
    // Solver params were set once in build_cache; no per-call tuning here.
    cached.solver.push();
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            // PinnedInt was already folded in via apply_pinned_ints from
            // this same given value, so the assertion is redundant. If
            // the values disagree (caller passes a different int after a
            // body equality pinned the var), force UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx) {
                    cached.solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }
    let satisfied = matches!(cached.solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
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
                    Var::StrVar(s) => {
                        if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                            bindings.insert(name.clone(), Value::Str(v));
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
                    Var::SetVar { .. } => {
                        // Z3 sets are characteristic functions over an
                        // (often infinite) element domain. We don't try
                        // to enumerate; bindings just omit set vars.
                    }
                    Var::DatatypeSeqVar { arr, len, dt, fields, .. } => {
                        if let Some(v) = extract_seq_composite(
                            arr, len, fields.as_slice(), *dt, &model, ctx)
                        {
                            bindings.insert(name.clone(), v);
                        }
                    }
                }
            }
        }
    }
    cached.solver.pop(1);
    EvalResult { satisfied, bindings }
}

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
    arith_solver: u32,
) -> EvalResult {
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();

    // Pass 1: declare variables and add per-type constraints. User-defined
    // schema types expand into their leaf fields under a dotted prefix.
    // ..Passthrough imports declarations from the named claim too — any
    // variable name not already in env gets a fresh Z3 const, names that
    // collide with the parent are reused (names-match composition).
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
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
            // Bare-identifier names-match passthrough (see build_cache for
            // the rationale): a `Constraint(Identifier(name))` whose name
            // is a known claim/type is treated as `..ClaimName`. Adds any
            // of the claim's own variables that aren't already in env.
            BodyItem::Constraint(Expr::Identifier(name)) if schemas.contains_key(name) => {
                if let Some(claim) = schemas.get(name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry));
                            }
                        }
                    }
                }
            }
            BodyItem::Constraint(_) => {}
        }
    }

    // Pass 1.5: pin literal-int vars from `given` + body equalities +
    // #seq length propagation. Quantifier ranges over those names then
    // unroll because translate_int yields literal IntVals.
    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    // Pass 2: translate body constraints and assert. Passthrough items
    // also contribute their included claim's constraints under the
    // current env. ClaimCall items translate their claim's body in a
    // fresh env where each mapping slot is pre-bound. Both passthrough
    // and ClaimCall recurse into nested claim composition (one helper
    // unifies all four entry shapes).
    let mut visited: HashSet<String> = HashSet::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, &mut visited);

    // Pass 3: assert ground facts for each given binding. Names that
    // aren't declared in the schema are silently ignored (matches the
    // Python runtime's behavior).
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            // PinnedInt was already folded in via apply_pinned_ints from
            // this same given value — assertion is redundant. If values
            // disagree, force UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx) {
                    solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }

    let satisfied = matches!(solver.check(), SatResult::Sat);
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
                    Var::StrVar(s) => {
                        if let Some(val) = model.eval(s, true) {
                            if let Some(sv) = val.as_string() {
                                bindings.insert(name.clone(), Value::Str(sv));
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
                    Var::SetVar { .. } => {
                        // Z3 sets are characteristic functions over an
                        // (often infinite) element domain. We don't try
                        // to enumerate; bindings just omit set vars.
                    }
                    Var::DatatypeSeqVar { arr, len, dt, fields, .. } => {
                        if let Some(v) = extract_seq_composite(
                            arr, len, fields.as_slice(), *dt, &model, ctx)
                        {
                            bindings.insert(name.clone(), v);
                        }
                    }
                }
            }
        }
    }
    EvalResult { satisfied, bindings }
}
