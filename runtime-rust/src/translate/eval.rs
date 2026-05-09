//! The four public orchestrator entry points: `evaluate` (one-shot
//! query), `build_cache` + `run_cached` (per-step cached query for the
//! executor), `sample_cached_inner` (n-distinct-models for sampling).

use std::collections::{HashMap, HashSet};
use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::{Context, Params, SatResult, Solver};

/// Set `smt.arith.solver` to `arith_solver` on `solver`. Pass `0` to
/// skip (lets Z3 use its built-in default). The chosen value depends
/// on workload — the runtime's auto-tuner decides which to use; this
/// helper is the dumb mechanism. See `runtime::SolveHistory` for the
/// policy.
/// Build a Z3 Real literal from an f64 source value.
///
/// Splits `f.to_string()` (Rust's shortest-round-trip Display form,
/// so `3.14` formats as `"3.14"`) into pure-integer numerator and
/// denominator strings: `"3.14"` → `("314", "100")` → exact
/// rational `157/50` in Z3. Z3's numeral parser accepts integer
/// num/den directly, but is finicky about decimals in `"num/den"`
/// concatenation, so we do the split ourselves.
///
/// Edge cases: NaN / inf fall back to 0 (constraint solvers don't
/// have useful NaN semantics).
fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let (num, den) = f64_to_int_rational(f);
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}

/// `3.14` → `("314", "100")`. `-3.14` → `("-314", "100")`.
/// `42` → `("42", "1")`. Used by `real_from_f64` to feed Z3 only
/// integer numerator/denominator strings.
fn f64_to_int_rational(f: f64) -> (String, String) {
    let s = f.to_string();
    if let Some(dot) = s.find('.') {
        let (int_part, frac_with_dot) = s.split_at(dot);
        let frac = &frac_with_dot[1..];
        let num = format!("{}{}", int_part, frac);
        let den = format!("1{}", "0".repeat(frac.len()));
        (num, den)
    } else {
        (s, "1".to_string())
    }
}

/// Convert a Z3 model's Real binding to f64. Z3 returns the exact
/// rational `(num, den)`; we project to f64 for the public Value
/// shape. Lossy in general; fine for the binding-display + tolerance-
/// based equality use cases.
fn real_value_to_f64(num: i64, den: i64) -> f64 {
    if den == 0 { 0.0 } else { num as f64 / den as f64 }
}

fn apply_solver_tuning(ctx: &Context, solver: &Solver, arith_solver: u32) {
    if arith_solver == 0 { return; }
    let mut params = Params::new(ctx);
    params.set_u32("smt.arith.solver", arith_solver);
    solver.set_params(&params);
}

/// For every enum in the registry, pre-populate `env` with one
/// `Var::EnumValue` per variant name. Lets bare identifiers like
/// `Mon`, `Tue`, … resolve via the existing env-lookup path in
/// `translate_*` without any new code in exprs.rs.
///
/// Variant names are globally unique across all enums (enforced at
/// `register_enum`), so there's no clash risk. If a variant collides
/// with a user-declared variable name, the user's declaration in the
/// schema body will overwrite this entry — schema-local takes
/// precedence over the language-level constant.
fn populate_enum_variants<'ctx>(
    env: &mut HashMap<String, super::types::Var<'ctx>>,
    enums: Option<&EnumRegistry>,
) where 'ctx: 'static {
    let Some(reg) = enums else { return };
    for (enum_name, (dt, variants)) in reg.by_name.borrow().iter() {
        for (idx, variant) in variants.iter().enumerate() {
            if variant.fields.is_empty() {
                // Nullary variant — pre-apply the constructor and stash
                // the Datatype value directly. Lets bare identifiers
                // resolve via env-lookup with no special-casing.
                let ctor = &dt.variants[idx].constructor;
                let ast = ctor.apply(&[]).as_datatype()
                    .expect("nullary enum variant must yield a Datatype value");
                env.insert(variant.name.clone(), super::types::Var::EnumValue { ast });
            } else {
                env.insert(variant.name.clone(), super::types::Var::EnumCtor {
                    dt: *dt,
                    variant_idx: idx,
                    field_types: variant.fields.iter()
                        .map(|f| f.type_name.clone()).collect(),
                });
            }
            let _ = enum_name;
        }
    }
}

use crate::ast::*;
use super::types::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
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
    enums: Option<&EnumRegistry>,
    given: &HashMap<String, Value>,
    arith_solver: u32,
) -> CachedSchema<'static> {
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Same two passes as evaluate(), but writing into the cache's
    // solver instead of a fresh one each time.
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name, .. } => {
                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name, .. } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
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
    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let mut visited: HashSet<String> = HashSet::new();
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
                Var::RealVar(r) => {
                    if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                        let f = real_value_to_f64(num, den);
                        bindings.insert(name.clone(), Value::Real(f));
                        block_terms.push(r._eq(&real_from_f64(ctx, f)));
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
                Var::EnumVar { ast, enum_name, dt } => {
                    if let Some(v) = extract_enum_value(ast, enum_name, dt, &model, enums) {
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
                    Var::RealVar(r) => {
                        if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                            bindings.insert(name.clone(), Value::Real(real_value_to_f64(num, den)));
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
                    Var::EnumVar { ast, enum_name, dt } => {
                        if let Some(v) = extract_enum_value(ast, enum_name, dt, &model, enums) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::EnumValue { .. } => { /* literal, no model state */ }
                    Var::EnumCtor { .. }  => { /* constructor reference, no model state */ }
                }
            }
        }
    }
    cached.solver.pop(1);
    EvalResult { satisfied, bindings, unsat_core_items: None }
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
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> EvalResult {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
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
                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name, .. } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
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
                    Var::RealVar(r) => {
                        if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                            bindings.insert(name.clone(), Value::Real(real_value_to_f64(num, den)));
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
                    Var::EnumVar { ast, enum_name, dt } => {
                        if let Some(v) = extract_enum_value(ast, enum_name, dt, &model, enums) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::EnumValue { .. } => { /* literal */ }
                    Var::EnumCtor { .. }  => { /* constructor */ }
                }
            }
        }
    }
    EvalResult { satisfied, bindings, unsat_core_items: None }
}

/// Stage 3 helper: like `evaluate`, but additionally asserts that
/// the variable named `extra_var` (which must have been declared in
/// the schema body as an enum-typed Membership) equals `extra_value`.
/// Used by `EvidentRuntime::query_with_program` to inject an
/// encoded `Program` value into a self-hosted pass.
///
/// Implementation: copy of `evaluate` with one extra `solver.assert`
/// after pass 2 and before the satisfiability check. Cleaner than
/// extending `evaluate` with an Option<extra_assertion> closure
/// because the AST-value injection is a niche operation that
/// shouldn't pollute the main entry point's signature.
pub fn evaluate_with_extra_assertion(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
    extra_var: &str,
    extra_value: z3::ast::Datatype<'static>,
) -> EvalResult {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Pass 1: declare. (Same as evaluate.)
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
        }
    }

    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let mut visited: HashSet<String> = HashSet::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    // Inject the extra value. Look up the variable, must be EnumVar
    // (i.e. enum-typed Membership). Anything else gets a warning;
    // assertion is silently skipped to avoid forcing UNSAT.
    if let Some(Var::EnumVar { ast, .. }) = env.get(extra_var) {
        solver.assert(&ast._eq(&extra_value));
    } else {
        eprintln!("warning: query_with_program: variable `{}` is not enum-typed in `{}`",
                  extra_var, schema.name);
    }

    let satisfied = matches!(solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, enums);
            }
        }
    }
    EvalResult { satisfied, bindings, unsat_core_items: None }
}

/// Like `evaluate_with_extra_assertion` but pins multiple enum-typed
/// variables in one solve. Used by the effect loop to pin both
/// `state` and `last_results` per step.
pub fn evaluate_with_extra_assertions(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
    pins: &[(&str, z3::ast::Datatype<'static>)],
) -> EvalResult {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
        }
    }

    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let mut visited: HashSet<String> = HashSet::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    for (var_name, value) in pins {
        if let Some(Var::EnumVar { ast, .. }) = env.get(*var_name) {
            solver.assert(&ast._eq(value));
        } else {
            eprintln!("warning: pin: variable `{}` is not enum-typed in `{}`",
                      var_name, schema.name);
        }
    }

    let satisfied = matches!(solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, enums);
            }
        }
    }
    EvalResult { satisfied, bindings, unsat_core_items: None }
}

/// Stage 5.5: like `evaluate_with_extra_assertion` but injects two
/// related values — the encoded `Program` datatype AND a flat
/// `Seq(BodyItem)` for the user's first claim's body. Lets a
/// self-hosted pass iterate over arbitrary-length user programs
/// via `∀ i ∈ {0..#body-1} : …`.
///
/// The seq-injection asserts both `#body = items.len()` and
/// `body[i] = encoded(items[i])` for each i, so the variable's
/// model is fully pinned to the user's source.
pub fn evaluate_with_program_and_body(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: &EnumRegistry,
    arith_solver: u32,
    program_var: &str,
    program_value: z3::ast::Datatype<'static>,
    body_var: &str,
    body_items: &[BodyItem],
) -> EvalResult {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(Some(enums));
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, Some(enums));

    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), Some(enums));
        }
    }

    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let mut visited: HashSet<String> = HashSet::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, Some(enums), &mut visited);

    // Program injection.
    if let Some(Var::EnumVar { ast, .. }) = env.get(program_var) {
        solver.assert(&ast._eq(&program_value));
    } else {
        eprintln!("warning: program var `{}` is not enum-typed in `{}`",
                  program_var, schema.name);
    }
    // Body Seq injection.
    if let Some(Var::DatatypeSeqVar { arr, len, fields, .. }) = env.get(body_var) {
        if !fields.is_empty() {
            eprintln!("warning: body var `{}` is record-typed seq, not enum-typed; skipping",
                      body_var);
        } else {
            match super::encode_ast::encode_body_items_into_seq(
                body_items, arr, len, ctx, enums,
            ) {
                Ok(asserts) => for a in &asserts { solver.assert(a); },
                Err(e) => eprintln!("warning: body encode failed: {e}"),
            }
        }
    } else {
        eprintln!("warning: body var `{}` is not a Seq(enum) in `{}`",
                  body_var, schema.name);
    }

    let satisfied = matches!(solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, Some(enums));
            }
        }
    }
    EvalResult { satisfied, bindings, unsat_core_items: None }
}

/// Same as `evaluate`, but tags every Z3 assertion derived from a
/// top-level body item with a unique tracker bool so an UNSAT result
/// produces a usable `unsat_core_items` (indices into `schema.body`).
///
/// Givens are NOT tracked — the user's `given` values are external
/// inputs, not constraints we can ask the user to fix. The core
/// reflects the conflict among the schema's own body items only.
///
/// On SAT, behaves exactly like `evaluate` (the trackers are still
/// asserted but Z3 ignores them); the cost is one extra implication
/// per assertion. On UNSAT, the core is mapped from tracker name
/// (`__core_<i>__`) back to body item index.
pub fn evaluate_with_core(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> EvalResult {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = Solver::new(ctx);
    apply_solver_tuning(ctx, &solver, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Pass 1: declarations (same as evaluate).
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name, .. } => {
                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name, .. } = sub {
                            if !env.contains_key(name) {
                                declare_var(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
                            }
                        }
                    }
                }
            }
            // (Bare-identifier-as-passthrough desugared upstream — see
            // build_cache notes.)
            _ => {}
        }
    }

    let seq_lens = collect_seq_lengths(&schema.body, given);
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    // Allocate one tracker bool per top-level body item, with a name
    // that encodes the index so we can map the core back to source.
    let trackers: Vec<Bool<'static>> = (0..schema.body.len())
        .map(|i| Bool::new_const(ctx, format!("__core_{i}__")))
        .collect();

    let mut visited: HashSet<String> = HashSet::new();
    super::inline::inline_body_items_tracked(
        &schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited, &trackers,
    );

    // Givens — same as evaluate, NOT tracked.
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx) {
                    solver.assert(&b);
                }
            }
        }
    }

    // check_assumptions activates the trackers and asks Z3 to find a
    // satisfying assignment under them. On UNSAT, get_unsat_core
    // returns the subset that's actually needed for the conflict.
    let result = solver.check_assumptions(&trackers);
    let satisfied = matches!(result, SatResult::Sat);
    let mut bindings = HashMap::new();
    let mut core_items: Option<Vec<usize>> = None;

    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, enums);
            }
        }
    } else {
        let core = solver.get_unsat_core();
        let mut indices: Vec<usize> = core.iter()
            .filter_map(|b| {
                let s = format!("{b}");
                s.strip_prefix("__core_")
                    .and_then(|rest| rest.strip_suffix("__"))
                    .and_then(|n| n.parse().ok())
            })
            .collect();
        indices.sort();
        indices.dedup();
        core_items = Some(indices);
    }
    EvalResult { satisfied, bindings, unsat_core_items: core_items }
}

/// Pull one variable's value out of the model into the bindings map.
/// Mirrors the inline match in `evaluate`'s SAT branch — extracted so
/// `evaluate_with_core` doesn't have to duplicate it.
fn extract_binding(
    name: &str, var: &Var<'static>, model: &z3::Model<'_>, ctx: &'static Context,
    bindings: &mut HashMap<String, Value>,
    enums: Option<&EnumRegistry>,
) {
    match var {
        Var::IntVar(i) => {
            if let Some(val) = model.eval(i, true) {
                if let Some(n) = val.as_i64() {
                    bindings.insert(name.to_string(), Value::Int(n));
                }
            }
        }
        Var::BoolVar(b) => {
            if let Some(val) = model.eval(b, true) {
                if let Some(bv) = val.as_bool() {
                    bindings.insert(name.to_string(), Value::Bool(bv));
                }
            }
        }
        Var::RealVar(r) => {
            if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                bindings.insert(name.to_string(), Value::Real(real_value_to_f64(num, den)));
            }
        }
        Var::StrVar(s) => {
            if let Some(val) = model.eval(s, true) {
                if let Some(sv) = val.as_string() {
                    bindings.insert(name.to_string(), Value::Str(sv));
                }
            }
        }
        Var::SeqVar { arr, len, elem } => {
            if let Some(v) = extract_seq(arr, len, *elem, model, ctx) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::PinnedInt(v) => { bindings.insert(name.to_string(), Value::Int(*v)); }
        Var::SetVar { .. } => {}
        Var::DatatypeSeqVar { arr, len, dt, fields, .. } => {
            if let Some(v) = extract_seq_composite(
                arr, len, fields.as_slice(), *dt, model, ctx)
            {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::EnumVar { ast, enum_name, dt } => {
            if let Some(v) = extract_enum_value(ast, enum_name, dt, model, enums) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::EnumValue { .. } => { /* literal */ }
        Var::EnumCtor { .. }  => { /* constructor */ }
    }
}

/// Extract an enum-typed Z3 const from the model. Walks the
/// DatatypeSort's variants looking for the one whose `tester` returns
/// true on the model-evaluated value, then recursively extracts each
/// payload field. Recursion handles self-referential enums — the
/// EnumRegistry is consulted to find the field's enum (by type name)
/// when a payload field is itself an enum-typed value.
fn extract_enum_value(
    ast: &z3::ast::Datatype<'_>,
    enum_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'_>,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let evaluated = model.eval(ast, true)?;
    // Find the active variant via its tester.
    let mut active_idx: Option<usize> = None;
    for (i, variant) in dt.variants.iter().enumerate() {
        let test = variant.tester.apply(&[&evaluated]).as_bool()?;
        if let Some(true) = model.eval(&test, true).and_then(|b| b.as_bool()) {
            active_idx = Some(i);
            break;
        }
    }
    let idx = active_idx?;
    let variant = &dt.variants[idx];
    let variant_name = variant.constructor.name();

    // Look up the variant's declared field types so we can route each
    // accessor's Dynamic through the right `as_int` / `as_bool` /
    // `as_string` extractor (or recurse for nested enums).
    let mut field_values: Vec<Value> = Vec::new();
    if let Some(reg) = enums {
        if let Some((_, decl_variants)) = reg.by_name.borrow().get(enum_name) {
            if let Some(decl_variant) = decl_variants.get(idx) {
                for (acc_idx, decl_field) in decl_variant.fields.iter().enumerate() {
                    let accessor = &variant.accessors[acc_idx];
                    let raw = accessor.apply(&[&evaluated]);
                    let extracted = match decl_field.type_name.as_str() {
                        "Int" | "Nat" | "Pos" => raw.as_int()
                            .and_then(|i| model.eval(&i, true))
                            .and_then(|x| x.as_i64())
                            .map(Value::Int),
                        "Bool" => raw.as_bool()
                            .and_then(|b| model.eval(&b, true))
                            .and_then(|x| x.as_bool())
                            .map(Value::Bool),
                        "String" => raw.as_string()
                            .and_then(|s| model.eval(&s, true))
                            .and_then(|x| x.as_string())
                            .map(Value::Str),
                        // Self-reference or another enum: recurse.
                        ref_type => {
                            let target: &str = if ref_type == enum_name { enum_name }
                                               else { ref_type };
                            let nested_dt = reg.by_name.borrow().get(target)
                                .map(|(d, _)| *d);
                            if let Some(nested_dt) = nested_dt {
                                raw.as_datatype().and_then(|child_ast| {
                                    extract_enum_value(&child_ast, target,
                                                       nested_dt, model, enums)
                                })
                            } else { None }
                        }
                    };
                    if let Some(v) = extracted {
                        field_values.push(v);
                    }
                }
            }
        }
    }
    Some(Value::Enum {
        enum_name: enum_name.to_string(),
        variant: variant_name,
        fields: field_values,
    })
}
