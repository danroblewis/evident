//! Public orchestrator entry points, in two families:
//!
//!   * **One-shot query** — `evaluate`, `evaluate_with_extra_assertion`,
//!     `evaluate_with_extra_assertions`, `evaluate_with_program_and_body`,
//!     `evaluate_with_core`. Each builds a fresh Solver, asserts the
//!     schema's body (plus any caller extras), runs `check`, returns
//!     a `QueryResult`. Variants exist for the different shapes of
//!     "extra constraints" callers want to layer on (CLI `--given`,
//!     test scaffolding, multi-FSM coordinator).
//!
//!   * **Per-step cached query** — `build_cache` (compile once) +
//!     `run_cached` (step many times re-using the compiled solver) +
//!     `sample_cached_inner` (n-distinct-models for `sample`). Used
//!     by the effect loop to amortize translate cost across ticks.
//!
//! File layout (top-to-bottom, each section depends only on those above):
//!   1. Helpers — Real-literal / numeric conversions, solver tuning,
//!      env priming, the declare-and-assert convenience.
//!   2. Cached-query path — `build_cache` / `run_cached` /
//!      `sample_cached_inner`, used by the multi-FSM scheduler.
//!   3. One-shot evaluate variants — used by `query` / `check` / `sample`.
//!   4. Local model-extraction helpers — leaf-level Var → Value plumbing
//!      shared by both query paths.

use std::collections::{HashMap, HashSet};
use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::{Context, Params, SatResult, Solver};

use crate::ast::*;
use super::types::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
use super::declare::{apply_seq_lengths, apply_set_candidates, declare_var};
use super::extract::{assert_seq_given, assert_set_given, extract_seq, extract_seq_composite, extract_set, unescape_z3_string};
use super::inline::inline_body_items;
use super::preprocess::{apply_pinned_ints, collect_pinned_ints};

// ── Section 1: Helpers ────────────────────────────────────────────────

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

/// Build a solver, optionally wrapping it with a Z3 tactic preprocessing
/// chain. `EVIDENT_TACTICS` env var picks the chain:
///
///   - unset (default)  → "solve-eqs". Substitutes equality-defined
///     variables before solving. 1.3-1.6× speedup across our workloads
///     (`bench_tactics` example). Sound — never converts SAT to UNSAT.
///   - "off"            → plain `Solver::new(ctx)`; no tactic. Baseline.
///   - "simplify"       → `simplify` only.
///   - "standard"       → `simplify` + `propagate-values` + `solve-eqs`.
///   - "aggressive"     → standard + `elim-uncnstr` + `propagate-ineqs`.
///   - comma-separated  → custom chain, e.g. "simplify,solve-eqs".
///
/// All chains have `smt` appended as the terminal solving tactic —
/// preprocessors like `simplify` alone return `Unknown` without it.
///
/// Tactics run as preprocessing inside the solver; substitutions
/// happen automatically. Model extraction goes through the original
/// variable names because Z3's tactic-derived solver handles the
/// model conversion under the hood.
pub(super) fn make_tuned_solver<'ctx>(ctx: &'ctx Context, arith_solver: u32) -> Solver<'ctx> {
    let chain = std::env::var("EVIDENT_TACTICS").ok();
    // Default to "solve-eqs" — empirically best speedup with no
    // soundness regression across our workloads.
    let chain_spec = chain.as_deref().unwrap_or("solve-eqs");
    let solver = match chain_spec {
        "" | "off" => Solver::new(ctx),
        spec => {
            let mut names: Vec<&str> = match spec {
                "simplify"   => vec!["simplify"],
                "standard"   => vec!["simplify", "propagate-values", "solve-eqs"],
                "aggressive" => vec!["simplify", "propagate-values", "solve-eqs",
                                     "elim-uncnstr", "propagate-ineqs"],
                custom => custom.split(',').map(|s| s.trim()).collect(),
            };
            // ALWAYS append a terminal solving tactic. Preprocessors like
            // `simplify` produce a normalized formula but don't decide
            // SAT/UNSAT — calling `check()` returns `Unknown`. The
            // canonical terminal is `smt` (Z3's default SMT strategy).
            // Tactics that already include solving (`solve-eqs`, `der`,
            // etc.) cascade through to a decision; appending `smt`
            // again is a no-op for those.
            if !names.last().map(|n| *n == "smt").unwrap_or(false) {
                names.push("smt");
            }
            let mut t = z3::Tactic::new(ctx, names[0]);
            for n in &names[1..] {
                t = t.and_then(&z3::Tactic::new(ctx, n));
            }
            t.solver()
        }
    };
    apply_solver_tuning(ctx, &solver, arith_solver);
    solver
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
    env: &mut HashMap<String, Var<'ctx>>,
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
                env.insert(variant.name.clone(), Var::EnumValue { ast });
            } else {
                env.insert(variant.name.clone(), Var::EnumCtor {
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

/// Allocate a typed Z3 const for `(name, type_name)` and immediately
/// issue any type-implied invariants on the solver. `declare_var`'s
/// own concern is allocation only — it returns a list of `Bool`
/// constraints (Nat / Pos / Seq-length non-negativity) that the caller
/// must assert. This helper bundles the common case.
fn declare_and_assert(
    ctx: &'static Context,
    solver: &Solver<'static>,
    env: &mut HashMap<String, Var<'static>>,
    name: &str,
    type_name: &str,
    schemas: &HashMap<String, SchemaDecl>,
    registry: Option<&DatatypeRegistry>,
    enums: Option<&EnumRegistry>,
) {
    let post = declare_var(ctx, env, name, type_name, schemas, registry, enums);
    for c in &post { solver.assert(c); }
}

// ── Section 2: Cached-query path (build_cache / run_cached / sample) ─

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
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
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
    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
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
    crate::z3_profile::record_check_stats(&cached.solver, None, check_dt);
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

// ── Section 3: One-shot evaluate variants (query / check / sample) ───

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
/// Analysis-only entry: build the solver exactly the way `evaluate`
/// does, then read out the asserted Bool constraints and free-variable
/// names *without* calling `solver.check()`. Intended for compile-time
/// structural passes — currently the decomposition pass that re-separates
/// composed claims into independent sub-models. See
/// `crate::decompose` for the union-find walker, and
/// `docs/design/compile-claims-to-functions.md` ("Decomposition") for
/// the architectural framing.
///
/// `given_keys` are treated as broadcast constants — they don't link
/// components, so the returned `var_names` excludes them.
pub fn analyze_decomposition(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> Vec<crate::decompose::Component> {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // The setup phases below mirror `evaluate`'s build phase exactly —
    // declare, pin, inline. We deliberately skip the final `solver.check()`
    // and the model-extraction loop.
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
            _ => {}
        }
    }

    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    // Pin given values (same as evaluate's Pass 3) so given-keyed vars
    // get assertions that we can then exclude from var_names.
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(_), _) => {}
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
                }
            }
        }
    }

    // Collect free-variable names: every entry in env EXCLUDING
    //   - given-keyed names (those are broadcast constants), and
    //   - enum variant constants like `Init`, `Done` populated by
    //     `populate_enum_variants` (those are runtime-constant
    //     values, not user-declared variables).
    let var_names: Vec<String> = env.iter()
        .filter(|(n, _)| !given.contains_key(n.as_str()))
        .filter(|(_, v)| !matches!(v, Var::EnumValue { .. } | Var::EnumCtor { .. }))
        .map(|(n, _)| n.clone())
        .collect();

    let assertions = solver.get_assertions();
    crate::decompose::decompose(ctx, &assertions, &var_names)
}

/// A component plus a functionality verdict — whether the component's
/// variables are uniquely determined by the inputs (given + already-
/// determined components). UNSAT of the "another distinct model exists"
/// check means functional; SAT or UNKNOWN means non-functional.
#[derive(Debug, Clone)]
pub struct ClassifiedComponent {
    pub component: crate::decompose::Component,
    /// `true` iff the 2-copy uniqueness check is UNSAT for this
    /// component's variables — i.e., no two distinct satisfying models
    /// disagree on this component's variables, holding `given` fixed.
    /// `false` means non-functional OR Z3 returned Unknown.
    pub functional: bool,
}

/// Same as `analyze_decomposition`, plus a per-component verdict:
/// is this component function-shaped (outputs uniquely determined
/// by inputs)?
///
/// The check is: solver.check() → if SAT, capture the model, then
/// assert "at least one variable in the component differs from its
/// model value" and check again. UNSAT means functional; SAT means
/// another distinct model exists → non-functional.
///
/// Per-component cost: 1 push, 1 assert, 1 check, 1 pop. Cheap
/// relative to a full solve.
pub fn classify_components(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> Vec<ClassifiedComponent> {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Mirror analyze_decomposition's build phase.
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
            _ => {}
        }
    }
    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(_), _) => {}
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
                }
            }
        }
    }

    let var_names: Vec<String> = env.iter()
        .filter(|(n, _)| !given.contains_key(n.as_str()))
        .filter(|(_, v)| !matches!(v, Var::EnumValue { .. } | Var::EnumCtor { .. }))
        .map(|(n, _)| n.clone())
        .collect();
    let assertions = solver.get_assertions();
    let components = crate::decompose::decompose(ctx, &assertions, &var_names);

    // If the body is UNSAT under given, mark every component as
    // NON-functional. Don't say "vacuously functional" — that's
    // technically true (no two distinct models exist when no model
    // exists) but it lets downstream callers (the function-izer's
    // rt.query hook) skip the solve entirely and produce SAT=true
    // with wrong bindings. Force the caller to go through Z3 so it
    // correctly returns UNSAT.
    let initial = solver.check();
    if !matches!(initial, SatResult::Sat) {
        return components.into_iter().map(|c|
            ClassifiedComponent { component: c, functional: false }
        ).collect();
    }
    let model = match solver.get_model() {
        Some(m) => m,
        None => return components.into_iter().map(|c|
            ClassifiedComponent { component: c, functional: false }
        ).collect(),
    };

    // Per-component 2-copy check: assert that AT LEAST ONE variable
    // in this component differs from its current model value. UNSAT
    // means no such alternative model exists → functional.
    let mut out = Vec::with_capacity(components.len());
    for component in components {
        // Build the "differs from model" disjunction. Skip variables
        // whose types we can't easily compare against a Z3 value
        // (Seq, Set, Datatype) for v1; treat presence of such vars
        // as "couldn't classify, assume non-functional."
        let mut disjuncts: Vec<Bool<'static>> = Vec::new();
        let mut skip_due_to_unsupported_var = false;
        for name in &component.vars {
            let Some(var) = env.get(name) else { continue };
            match var {
                Var::IntVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::BoolVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::RealVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::StrVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::PinnedInt(_) => {} // Already a literal; can't differ.
                Var::EnumVar { ast, .. } => {
                    if let Some(val) = model.eval(ast, true) {
                        disjuncts.push(ast._eq(&val).not());
                    }
                }
                Var::EnumValue { .. } | Var::EnumCtor { .. } => {} // Enum constant; can't differ.
                Var::SeqVar { arr, len, .. } | Var::DatatypeSeqVar { arr, len, .. } => {
                    // A Seq's "value" is its length + array contents at
                    // in-range indices. The array can have arbitrary
                    // values outside [0, len), so naive `arr ≠ model_arr`
                    // is trivially SAT (Z3 picks a different out-of-range
                    // index). Encode the in-range existential explicitly:
                    //   len ≠ model_len  ∨  ∃ k ∈ [0, len). arr[k] ≠ model_arr[k]
                    let len_val = model.eval(len, true);
                    let arr_val = model.eval(arr, true);
                    if let Some(lv) = &len_val {
                        disjuncts.push(len._eq(lv).not());
                    }
                    if let Some(av) = &arr_val {
                        let k = z3::ast::Int::fresh_const(ctx, "fz_k");
                        let zero = z3::ast::Int::from_i64(ctx, 0);
                        let lo = zero.le(&k);
                        let hi = k.lt(len);
                        let in_range = z3::ast::Bool::and(ctx, &[&lo, &hi]);
                        let elem_diff = arr.select(&k)._eq(&av.select(&k)).not();
                        // Existential `∃k. P(k)` over Int is encoded as
                        // a fresh `k` AST plus `P(k)` asserted; Z3 treats
                        // free constants existentially in the assertion
                        // context.
                        disjuncts.push(z3::ast::Bool::and(ctx, &[&in_range, &elem_diff]));
                    }
                }
                Var::SetVar { set, .. } | Var::DatatypeSetVar { set, .. } => {
                    if let Some(val) = model.eval(set, true) {
                        disjuncts.push(set._eq(&val).not());
                    }
                }
            }
        }
        let functional = if skip_due_to_unsupported_var {
            false
        } else if disjuncts.is_empty() {
            // No differable vars (component is all pinned ints or only
            // contains unsupported types we skipped). Treat as functional.
            true
        } else {
            solver.push();
            let disj_refs: Vec<&Bool<'static>> = disjuncts.iter().collect();
            solver.assert(&Bool::or(ctx, &disj_refs));
            let r = solver.check();
            solver.pop(1);
            matches!(r, SatResult::Unsat)
        };
        out.push(ClassifiedComponent { component, functional });
    }
    out
}

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

    let check_t0 = std::time::Instant::now();
    let check_result = solver.check();
    crate::z3_profile::record_check_stats(&solver, Some(&schema.name), check_t0.elapsed());
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
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Pass 1: declare. (Same as evaluate.)
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
            _ => {}
        }
    }

    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    // Populate Set var candidates from given Value::Set* — needed before
    // body translation so `#s` cardinality folds to a literal count.
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
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

    let check_t0 = std::time::Instant::now();
    let check_result = solver.check();
    crate::z3_profile::record_check_stats(&solver, Some(&schema.name), check_t0.elapsed());
    let satisfied = matches!(check_result, SatResult::Sat);
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
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name, .. } => {
                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
            }
            BodyItem::Passthrough(claim_name) => {
                // Same as `evaluate` — pull in the passthrough'd
                // claim's Memberships so Pass 1.5's seq-length /
                // pinned-int collection can apply to them before
                // any `∀` over the inherited Seq tries to unroll.
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
            _ => {}
        }
    }

    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    // Populate Set var candidates from given Value::Set* — needed before
    // body translation so `#s` cardinality folds to a literal count.
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    for (var_name, value) in pins {
        if let Some(Var::EnumVar { ast, .. }) = env.get(*var_name) {
            solver.assert(&ast._eq(value));
        } else {
            eprintln!("warning: pin: variable `{}` is not enum-typed in `{}`",
                      var_name, schema.name);
        }
    }

    // Apply scalar `given` values (Int/Bool/String/Real). Same loop
    // as `evaluate`'s pass 3 — needed for the multi-FSM scheduler to
    // pin world.* fields each tick. Without this, callers using
    // query_with_pins_and_given for sub-field pins silently get free
    // (model-picked) values for the supposedly-given fields.
    //
    // `Value::Enum` values are also accepted here, so plugins that
    // write enum-typed world fields (currently the reflection
    // bridge's `world.program`) flow through the same path. The
    // value is re-encoded as a Z3 Datatype against the registry,
    // then asserted equal to the EnumVar's ast — same shape as the
    // explicit `pins` list above, just discovered through the
    // `given` map instead.
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  =>
                solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            (Var::EnumVar { ast, .. }, val @ Value::Enum { .. }) => {
                if let Some(reg) = enums {
                    if let Some(dt) = super::encode_ast::value_enum_to_datatype(val, ctx, reg) {
                        solver.assert(&ast._eq(&dt));
                    } else {
                        eprintln!("warning: given `{name}`: enum value did not encode \
                                   (registry missing variant?); leaving free");
                    }
                }
            }
            _ => {
                // Seq pin: (DatatypeSeqVar, SeqEnum) / (SeqVar, SeqInt) etc.
                // The multi-FSM scheduler routes `last_results ∈ Seq(Result)`
                // through here; without this the pin is silently dropped and
                // the FSM solves with an unconstrained Seq.
                if let Some(b) = super::extract::assert_seq_given(var, value, ctx, enums) {
                    solver.assert(&b);
                }
            }
        }
    }

    let check_t0 = std::time::Instant::now();
    let check_result = solver.check();
    crate::z3_profile::record_check_stats(&solver, Some(&schema.name), check_t0.elapsed());
    let satisfied = matches!(check_result, SatResult::Sat);
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
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, Some(enums));

    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name, .. } => {
                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), Some(enums));
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name, .. } = sub {
                            if !env.contains_key(name) {
                                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), Some(enums));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let mut visited: HashMap<String, usize> = HashMap::new();
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
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Pass 1: declarations (same as evaluate).
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
            // (Bare-identifier-as-passthrough desugared upstream — see
            // build_cache notes.)
            _ => {}
        }
    }

    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    // Allocate one tracker bool per top-level body item, with a name
    // that encodes the index so we can map the core back to source.
    let trackers: Vec<Bool<'static>> = (0..schema.body.len())
        .map(|i| Bool::new_const(ctx, format!("__core_{i}__")))
        .collect();

    let mut visited: HashMap<String, usize> = HashMap::new();
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
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
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

// ── Section 4: Local model-extraction helpers ────────────────────────

/// Pull one variable's value out of the model into the bindings map.
/// Mirrors the inline match in `evaluate`'s SAT branch — extracted so
/// `evaluate_with_core` doesn't have to duplicate it.
pub(crate) fn extract_binding(
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
                    bindings.insert(name.to_string(), Value::Str(unescape_z3_string(&sv)));
                }
            }
        }
        Var::SeqVar { arr, len, elem } => {
            if let Some(v) = extract_seq(arr, len, *elem, model, ctx) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::PinnedInt(v) => { bindings.insert(name.to_string(), Value::Int(*v)); }
        Var::SetVar { set, elem, candidates } => {
            if let Some(v) = extract_set(set, *elem, candidates, model, ctx) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::DatatypeSetVar { .. } => { /* unsupported in v1 */ }
        Var::DatatypeSeqVar { arr, len, dt, fields, type_name } => {
            let extracted = if fields.is_empty() {
                extract_seq_enum(arr, len, type_name, *dt, model, ctx, enums)
            } else {
                extract_seq_composite(arr, len, fields.as_slice(), *dt, model, ctx, enums)
            };
            if let Some(v) = extracted {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::EnumVar { ast, enum_name, dt } => {
            if let Some(v) = extract_enum_value(ast, enum_name, dt, model, ctx, enums) {
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
pub(super) fn extract_enum_value<'ctx>(
    ast: &z3::ast::Datatype<'ctx>,
    enum_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
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
    //
    // Seq(T) payload fields are two-accessor-expanded in the Z3
    // datatype (one logical field → arr accessor + len accessor),
    // so we maintain a separate physical accessor offset that
    // advances by 1 for primitive/enum fields and by 2 for Seq.
    let mut field_values: Vec<Value> = Vec::new();
    if let Some(reg) = enums {
        if let Some((_, decl_variants)) = reg.by_name.borrow().get(enum_name) {
            if let Some(decl_variant) = decl_variants.get(idx) {
                let mut acc_idx: usize = 0;
                for decl_field in decl_variant.fields.iter() {
                    if let Some(inner) = crate::runtime::parse_seq_type(&decl_field.type_name) {
                        // Internal-Cons backing: single Datatype
                        // accessor; walk the __SeqOf_T chain to
                        // recover the elements.
                        let helper_name = crate::runtime::internal_cons_helper_name(inner);
                        let has_helper = reg.by_name.borrow().contains_key(&helper_name);
                        if has_helper {
                            let acc = &variant.accessors[acc_idx];
                            let cons_dyn = acc.apply(&[&evaluated]);
                            let extracted = extract_internal_cons_seq(
                                &helper_name, inner, &cons_dyn, model, ctx, enums);
                            if let Some(v) = extracted {
                                field_values.push(v);
                            }
                            acc_idx += 1;
                            continue;
                        }
                        // Two-accessor expansion: arr at acc_idx, len at acc_idx+1.
                        let arr_acc = &variant.accessors[acc_idx];
                        let len_acc = &variant.accessors[acc_idx + 1];
                        let arr_dyn = arr_acc.apply(&[&evaluated]);
                        let len_dyn = len_acc.apply(&[&evaluated]);
                        let extracted = extract_seq_payload(
                            inner, &arr_dyn, &len_dyn, model, ctx, enums);
                        if let Some(v) = extracted {
                            field_values.push(v);
                        }
                        acc_idx += 2;
                        continue;
                    }
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
                            .map(|s| Value::Str(unescape_z3_string(&s))),
                        "Real" => raw.as_real()
                            .and_then(|r| model.eval(&r, true))
                            .and_then(|x| x.as_real())
                            .map(|(num, den)| Value::Real(real_value_to_f64(num, den))),
                        // Self-reference or another enum: recurse.
                        ref_type => {
                            let target: &str = if ref_type == enum_name { enum_name }
                                               else { ref_type };
                            let nested_dt = reg.by_name.borrow().get(target)
                                .map(|(d, _)| *d);
                            if let Some(nested_dt) = nested_dt {
                                raw.as_datatype().and_then(|child_ast| {
                                    extract_enum_value(&child_ast, target,
                                                       nested_dt, model, ctx, enums)
                                })
                            } else { None }
                        }
                    };
                    if let Some(v) = extracted {
                        field_values.push(v);
                    }
                    acc_idx += 1;
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

/// Extract a Seq-typed enum-variant payload field given the (arr,
/// len) pair produced by the two-accessor expansion. Routes to
/// `extract_seq` for primitive element types or `extract_seq_enum`
/// for enum elements. Used by `extract_enum_value` when it
/// encounters a `Seq(T)` field in a variant's declared types.
fn extract_seq_payload<'ctx>(
    inner_type: &str,
    arr_dyn: &z3::ast::Dynamic<'ctx>,
    len_dyn: &z3::ast::Dynamic<'ctx>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    use super::types::SeqElem;
    let len = len_dyn.as_int()?;
    match inner_type {
        "Int" | "Nat" | "Pos" => {
            let arr = arr_dyn.as_array()?;
            super::extract::extract_seq(&arr, &len, SeqElem::Int, model, ctx)
        }
        "Bool" => {
            let arr = arr_dyn.as_array()?;
            super::extract::extract_seq(&arr, &len, SeqElem::Bool, model, ctx)
        }
        "String" => {
            let arr = arr_dyn.as_array()?;
            super::extract::extract_seq(&arr, &len, SeqElem::Str, model, ctx)
        }
        enum_type => {
            // Enum element: look up the DatatypeSort and walk
            // arr[0..len], calling extract_enum_value per element.
            let reg = enums?;
            let dt = reg.by_name.borrow().get(enum_type).map(|(d, _)| *d)?;
            let arr = arr_dyn.as_array()?;
            extract_seq_enum(&arr, &len, enum_type, dt, model, ctx, enums)
        }
    }
}

/// Walk a `__SeqOf_T`-shaped Cons chain in the model and extract
/// the element list. Used by `extract_enum_value` when a variant
/// field is `Seq(T)` and T has internal-Cons backing (the field is
/// a single Datatype slot pointing to `__SeqOf_T`).
///
/// `__SeqOf_T` has variants `__Empty_T` (0-ary terminator) and
/// `__Cell_T(head: T, tail: __SeqOf_T)`. Walk via tester +
/// accessors until Empty.
fn extract_internal_cons_seq<'ctx>(
    helper_name: &str,
    elem_type: &str,
    cons_dyn: &z3::ast::Dynamic<'ctx>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let reg = enums?;
    let by_name = reg.by_name.borrow();
    let (helper_dt, helper_variants) = by_name.get(helper_name)?;
    let helper_dt: &'static z3::DatatypeSort<'static> = *helper_dt;
    let empty_idx = helper_variants.iter().position(|v| v.fields.is_empty())?;
    let cell_idx = helper_variants.iter().position(|v| v.fields.len() == 2)?;
    let (elem_dt, _) = by_name.get(elem_type)?;
    let elem_dt: &'static z3::DatatypeSort<'static> = *elem_dt;
    drop(by_name);

    let empty_tester = &helper_dt.variants[empty_idx].tester;
    let cell_v = &helper_dt.variants[cell_idx];
    let head_acc = &cell_v.accessors[0];
    let tail_acc = &cell_v.accessors[1];

    let mut out: Vec<Value> = Vec::new();
    let mut cur = cons_dyn.clone();
    // Cap iteration so a model bug can't make us walk forever.
    for _ in 0..10_000 {
        let is_empty_bool = empty_tester.apply(&[&cur]).as_bool()?;
        let is_empty = model.eval(&is_empty_bool, true)?.as_bool()?;
        if is_empty {
            return Some(Value::SeqEnum(out));
        }
        let head_dyn = head_acc.apply(&[&cur]);
        let head_dt = head_dyn.as_datatype()?;
        let head_val = extract_enum_value(&head_dt, elem_type, elem_dt, model, ctx, enums)?;
        out.push(head_val);
        cur = tail_acc.apply(&[&cur]);
    }
    None
}

/// Read a `Seq(EnumType)` value out of the model. Mirror of
/// `extract_seq_composite` but for enum-typed elements: each array
/// element is a Datatype value of the enum's sort, decoded via
/// `extract_enum_value` (which handles variant detection + payload
/// recursion). Returned as `Value::SeqEnum(Vec<Value::Enum>)`.
pub(super) fn extract_seq_enum<'ctx>(
    arr: &z3::ast::Array<'ctx>,
    len: &Int<'ctx>,
    type_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    let mut out: Vec<Value> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let idx = Int::from_i64(ctx, i);
        let elem_dyn = arr.select(&idx);
        let elem = elem_dyn.as_datatype()?;
        let v = extract_enum_value(&elem, type_name, dt, model, ctx, enums)?;
        out.push(v);
    }
    Some(Value::SeqEnum(out))
}
