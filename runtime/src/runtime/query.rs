//! `query`, `query_cached`, and the per-component Z3-AST functionizer
//! fast path.
//!
//! ## Per-component compilation
//!
//! A claim's simplified body is decomposed into independent
//! sub-models (`decompose_simplified` — connected components over the
//! free variables). Each component is compiled to its own callable
//! artifact in isolation; a construct one component can't emit no
//! longer blocks the rest. The components that *do* refuse to compile
//! are gathered into one cached, scoped Z3 solver (only their
//! constraints, not the whole claim) and solved per call via
//! `run_cached`. The whole arrangement is a `ClaimPlan`, cached per
//! `(claim, given-keys)`.

use super::autotune::SolveHistory;
use crate::core::{CachedSchema, CompiledFunction, QueryResult, RuntimeError, Var, Z3Step};
use super::lenient::LenientGuard;
use super::{EvidentRuntime, Value};
use crate::translate::{build_cache, run_cached, structural_signature};
use crate::z3_eval::{collect_touched_names, extract_program_partial,
                     recompose_record_seqs, simplify_assertions};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;
use z3::ast::{Ast, Bool};
use z3::{Context, Params, SatResult, Solver, Tactic};
use z3_sys::DeclKind;

/// Does this component carry a *defining* constraint — anything beyond
/// a bare type-bound comparison (`>=`, `>`, `<=`, `<`)? Equalities,
/// guarded implications (`or`), `select`/`len` pins, etc. all define or
/// relate the component's outputs, so the scoped slow solve can recover
/// them. A component whose every assertion is a plain comparison
/// constrains nothing beyond its declared type — its outputs are free
/// (e.g. because a defining constraint was dropped by the translator).
fn component_has_defining_assertion(assertions: &[Bool<'static>]) -> bool {
    !assertions.iter().all(|a| {
        a.safe_decl().ok()
            .map(|d| matches!(d.kind(),
                DeclKind::GE | DeclKind::GT | DeclKind::LE | DeclKind::LT))
            .unwrap_or(false)
    })
}

/// A per-claim execution plan: zero or more compiled components plus
/// an optional combined slow-path solve for the components that
/// refused to compile. Cached in `EvidentRuntime::fn_cache` per
/// `(claim, given-keys)` and run by `EvidentRuntime::execute_plan`.
pub(crate) struct ClaimPlan {
    /// One callable artifact per JIT-able component. Each produces a
    /// disjoint slice of the claim's outputs from `given`.
    pub(super) compiled: Vec<Rc<dyn CompiledFunction>>,
    /// Combined slow path: a cached solver holding only the assertions
    /// of the components that didn't compile (plus given-only
    /// consistency assertions), and the names of the outputs it
    /// produces. `None` when every component compiled.
    pub(super) slow: Option<SlowPart>,
    /// Statically-resolved integer vars (Z3 `PinnedInt`s), which sit in
    /// no component. Injected into every result so the bindings match
    /// the monolithic path, which emitted them as constant steps.
    pub(super) pinned_ints: Vec<(String, Value)>,
}

/// The cached scoped Z3 solve for the uncompiled components.
pub(crate) struct SlowPart {
    /// Full env (for given-pinning + model extraction) paired with a
    /// solver carrying *only* the uncompiled components' assertions.
    cached: CachedSchema<'static>,
    /// Output var names this solve is responsible for — the union of
    /// the uncompiled components' variables. Other env entries the
    /// solver happens to model are ignored.
    outputs: Vec<String>,
}

/// What `compile_one_component` decided for a component.
enum ComponentOutcome {
    /// Compiled to a callable artifact.
    Compiled(Rc<dyn CompiledFunction>),
    /// Couldn't compile, but is safe to solve in the scoped slow part
    /// (a `Guarded` step, a Set output, a codegen refusal, …).
    Slow,
    /// Gap-fill was refused: a needed output has no safe definition.
    /// This is the case the monolithic path returned `None` for, so we
    /// abandon functionizing the whole claim and let the non-lenient
    /// `evaluate` handle it — which solves a genuinely-free output
    /// correctly, or surfaces a dropped-constraint error rather than
    /// masking it with a baked/solved value from the lenient cache.
    Bail,
}

/// Minimal union-find for `decompose_simplified`. (The one in
/// `crate::decompose` is private and re-normalizes its input; here we
/// partition the already-simplified assertions directly.)
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind { parent: (0..n).collect(), rank: vec![0; n] }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r { r = self.parent[r]; }
        let mut y = x;
        while self.parent[y] != r {
            let next = self.parent[y];
            self.parent[y] = r;
            y = next;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb { return; }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}

/// Decompose the (already-simplified) assertions into independent
/// components over `outputs`. Two outputs join the same component when
/// some assertion mentions both. Returns, per component, the output
/// names it owns and the indices of `simplified` assertions touching
/// it; plus the indices of assertions that touch no output at all
/// (given-only consistency constraints).
///
/// Operating on `simplified` directly (rather than
/// `analyze_decomposition`, which rebuilds the solver and re-runs
/// `simplify`) keeps the component partition and the assertion
/// buckets derived from the *same* formula set, so every assertion's
/// output-touch-set lands in exactly one component.
fn decompose_simplified(
    simplified: &[Bool<'static>],
    outputs: &[String],
) -> (Vec<Vec<String>>, Vec<Vec<usize>>, Vec<usize>) {
    let index_of: HashMap<&str, usize> = outputs.iter().enumerate()
        .map(|(i, n)| (n.as_str(), i)).collect();
    let mut uf = UnionFind::new(outputs.len());
    // For each assertion, the sorted/deduped output-var indices it touches.
    let mut per_assert: Vec<Vec<usize>> = Vec::with_capacity(simplified.len());
    for a in simplified {
        let mut touched: HashSet<String> = HashSet::new();
        collect_touched_names(a, &mut touched);
        // A Seq output `s` splits into Z3-internal `s__arr` / `s__len`
        // consts; map those back to the base name so a length pin
        // (`#s = 4` → `s__len = 4`) joins the SAME component as the
        // element pins (`s[0] = …` → `select s …`). Otherwise the seq
        // loses its length and the component infers it from the pinned
        // elements alone.
        let mut idxs: Vec<usize> = touched.iter()
            .filter_map(|n| {
                let base = n.strip_suffix("__len")
                    .or_else(|| n.strip_suffix("__arr"))
                    .unwrap_or(n.as_str());
                index_of.get(base).copied()
            })
            .collect();
        idxs.sort_unstable();
        idxs.dedup();
        for w in idxs.windows(2) { uf.union(w[0], w[1]); }
        per_assert.push(idxs);
    }
    // Bucket output indices by root, in first-appearance order for
    // deterministic component ordering.
    let mut root_to_comp: HashMap<usize, usize> = HashMap::new();
    let mut comp_vars: Vec<Vec<String>> = Vec::new();
    for i in 0..outputs.len() {
        let r = uf.find(i);
        let comp = *root_to_comp.entry(r).or_insert_with(|| {
            comp_vars.push(Vec::new());
            comp_vars.len() - 1
        });
        comp_vars[comp].push(outputs[i].clone());
    }
    let mut comp_assertions: Vec<Vec<usize>> = vec![Vec::new(); comp_vars.len()];
    let mut global: Vec<usize> = Vec::new();
    for (ai, idxs) in per_assert.iter().enumerate() {
        match idxs.first() {
            Some(&first_out) => {
                let r = uf.find(first_out);
                let comp = root_to_comp[&r];
                comp_assertions[comp].push(ai);
            }
            None => global.push(ai),
        }
    }
    (comp_vars, comp_assertions, global)
}

/// Build a solver tuned the same way `make_tuned_solver` does (tactic
/// chain from `EVIDENT_TACTICS`, default `solve-eqs`; `smt.arith.solver`
/// param). Re-implemented here because that helper is `pub(super)` to
/// `translate::eval` — the per-component slow solver needs the same
/// tuning as the cached slow path it replaces.
fn build_tuned_solver(ctx: &'static Context, arith_solver: u32) -> Solver<'static> {
    let chain = std::env::var("EVIDENT_TACTICS").ok();
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
            if !names.last().map(|n| *n == "smt").unwrap_or(false) { names.push("smt"); }
            let mut t = Tactic::new(ctx, names[0]);
            for n in &names[1..] { t = t.and_then(&Tactic::new(ctx, n)); }
            t.solver()
        }
    };
    if arith_solver != 0 {
        let mut params = Params::new(ctx);
        params.set_u32("smt.arith.solver", arith_solver);
        solver.set_params(&params);
    }
    solver
}

impl EvidentRuntime {
    /// Per-component Z3-AST functionizer. Decomposes the claim's
    /// simplified body into independent sub-models, compiles each one
    /// it can to native code, and gathers the rest into a single
    /// cached scoped Z3 solve. Returns `Some(QueryResult)` when the
    /// plan executed (compiled components ran + any slow part was
    /// SAT), `None` to fall through to a full Z3 solve.
    ///
    /// Cached per `(claim, given-keys)` as a `ClaimPlan`; subsequent
    /// calls just re-run the plan (JIT calls at ~µs + one scoped solve).
    pub(super) fn try_functionize_z3(&self, name: &str, schema: &crate::core::ast::SchemaDecl,
                          given: &HashMap<String, Value>) -> Option<QueryResult>
    {
        // Cache key: name + sorted given_keys. The plan is generic
        // over given VALUES — compiled components read inputs per call
        // and the slow part re-pins `given` each call — so a stable
        // set of given_keys per FSM keeps the cached plan correct
        // across ticks.
        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (name.to_string(), given_keys.clone());

        // Cache hit: re-run the cached plan. `None` cached means the
        // claim can't be functionized — fall through to slow-path Z3.
        if let Some(entry) = self.fn_cache.borrow().get(&cache_key).cloned() {
            let Some(plan) = entry else { return None };
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().cache_hits += 1;
            return self.execute_plan(&plan, given);
        }

        // Cache miss: build a CachedSchema, capture the body
        // assertions (without given values pinned so the
        // extracted program is generic over input values), apply
        // Z3's tactic chain, and extract per-output assignments.
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        // The Z3 translator fatal-exits on dropped constraints
        // (constraints it can't express). For schemas with such
        // gaps (e.g. enum ctors carrying Seq payloads), the slow
        // path is the only correct option — fall through there.
        if crate::z3_eval::has_known_translator_gap(&schema.body) {
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }
        // Pass the ACTUAL given to build_cache so apply_pinned_ints
        // can resolve symbolic bounds (∀ i ∈ {0..n - 1}) into
        // statically-known ranges before the translator runs.
        // Without these pins, body shapes like ∀-over-symbolic-Range
        // would trip the translator's dropped-constraint fatal-exit.
        //
        // R27: temporarily enable EVIDENT_LENIENT for the
        // build_cache call so untranslatable body items (like
        // SDL_Window's `install ∈ Seq(InstallStep) = ⟨...⟩` with
        // payloaded LibCalls) become warnings rather than
        // fatal-exit. extract_program will produce a partial
        // program; if it's incomplete for the outputs we need,
        // we fall through to the slow path which handles these
        // cases via the silently-skipping inheritance path
        // (inline.rs line 906).
        let _lenient_guard = LenientGuard::enable();
        // Pass an empty given to build_cache so the extracted program
        // is generic over input values. If we passed `given` here,
        // apply_pinned_ints would bake `_count`/state/etc. into the
        // body as constants, and the cached program would be wrong
        // for any other tick's values. Structural pins (Seq lengths)
        // still propagate because they come from the schema body
        // itself (`#platforms = 4`), not from given.
        let empty_given: HashMap<String, Value> = HashMap::new();
        let cached = crate::translate::build_cache(
            schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &empty_given, arith);
        drop(_lenient_guard);
        // get_assertions ties the Bool lifetime to the solver, but
        // the underlying Z3 ASTs are reference-counted by the
        // 'static Context — they outlive the solver wrapper. Same
        // pattern as `effect_loop.rs` uses for BodyItem slices.
        let assertions_local = cached.solver.get_assertions();
        let assertions: Vec<z3::ast::Bool<'static>> = unsafe {
            std::mem::transmute::<Vec<z3::ast::Bool<'_>>, Vec<z3::ast::Bool<'static>>>(
                assertions_local)
        };
        let simplify_result = simplify_assertions(self.z3_ctx, &assertions);
        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.analyses += 1;
            per.simplified_total += simplify_result.formulas.len() as u32;
        }
        if simplify_result.unsat {
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().decided_unsat += 1;
            return Some(QueryResult { satisfied: false, bindings: HashMap::new() });
        }
        let simplified = &simplify_result.formulas;

        // Outputs: vars actually constrained by the simplified
        // body. Many env entries (world.player.vel.x when this
        // FSM doesn't read it, FTI bridge leaves, type-level
        // siblings of an unused field) appear in env but have no
        // body assertion — Z3 would pick any value. For the
        // function-izer, those vars are NOT outputs; the
        // scheduler's downstream paths either carry through from
        // world_snapshot or just don't need them.
        //
        // We compute the constraint-touched set by walking the
        // simplified assertions and collecting every 0-arity
        // App name that appears anywhere. An output is then:
        //   - in env (declared at build_cache time)
        //   - NOT in given (input)
        //   - NOT a PinnedInt/EnumValue/EnumCtor constant
        //   - actually appears in the simplified body
        let mut touched: std::collections::HashSet<String> = std::collections::HashSet::new();
        for a in simplified {
            crate::z3_eval::collect_touched_names(a, &mut touched);
        }
        let outputs: Vec<String> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter(|(_, v)| !matches!(v,
                crate::translate::Var::EnumValue { .. }
                | crate::translate::Var::EnumCtor { .. }
                | crate::translate::Var::PinnedInt(_)))
            .filter(|(name, _)| touched.contains(name.as_str()))
            .map(|(n, _)| n.clone())
            .collect();
        // Pinned ints — vars whose value was statically resolvable
        // at build_cache time. Synthesize Scalar steps for them so
        // the cached program produces these bindings without any
        // re-derivation needed at hit time.
        let pinned_steps: Vec<crate::core::Z3Step<'static>> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::translate::Var::PinnedInt(i) => Some(crate::core::Z3Step::Scalar {
                    var:  n.clone(),
                    expr: z3::ast::Dynamic::from_ast(&z3::ast::Int::from_i64(self.z3_ctx, *i)),
                }),
                _ => None,
            })
            .collect();
        // The same pinned ints as plain bindings — injected into every
        // result regardless of which component (if any) consumed them,
        // so the output set matches the monolithic path.
        let pinned_ints: Vec<(String, Value)> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::translate::Var::PinnedInt(i) => Some((n.clone(), Value::Int(*i))),
                _ => None,
            })
            .collect();

        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
            eprintln!("[fz/z3] {}: simplified body has {} assertions, outputs = {:?}",
                name, simplified.len(), outputs);
            for a in simplified {
                eprintln!("    {a}");
            }
        }
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
            eprintln!("[fz/z3] {} extract pass: {} outputs, {} simplified",
                name, outputs.len(), simplified.len());
            if name == "keyboard" || std::env::var("EVIDENT_FZ_DUMP_BODY").is_ok() {
                eprintln!("[fz/z3] {name} outputs: {outputs:?}");
                for a in simplified {
                    eprintln!("  {a}");
                }
            }
        }
        if outputs.is_empty() {
            // No constrained outputs — the body is just type
            // bounds / predicates with nothing to compute. We
            // can't claim to produce bindings the caller needs;
            // fall through to the slow path which extracts the
            // model directly.
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }
        // ── Per-component compilation ──────────────────────────────
        // Decompose the simplified body into independent sub-models,
        // compile each one we can, and gather the rest into one cached
        // scoped slow solve. A construct one component can't emit no
        // longer blocks the others.
        let (comp_vars, comp_assert_idx, global_idx) =
            decompose_simplified(simplified, &outputs);
        let n_components = comp_vars.len();

        let mut compiled: Vec<Rc<dyn CompiledFunction>> = Vec::new();
        let mut uncompiled_outputs: Vec<String> = Vec::new();
        let mut uncompiled_assert_idx: Vec<usize> = Vec::new();
        let mut n_compiled = 0u32;
        let mut bail = false;
        for (ci, cvars) in comp_vars.iter().enumerate() {
            let casserts: Vec<Bool<'static>> =
                comp_assert_idx[ci].iter().map(|&i| simplified[i].clone()).collect();
            match self.compile_one_component(name, cvars, &casserts, &cached, given, &pinned_steps) {
                ComponentOutcome::Compiled(c) => { compiled.push(c); n_compiled += 1; }
                ComponentOutcome::Slow => {
                    uncompiled_outputs.extend(cvars.iter().cloned());
                    uncompiled_assert_idx.extend(comp_assert_idx[ci].iter().copied());
                }
                ComponentOutcome::Bail => { bail = true; break; }
            }
        }

        // A gap-fill refusal abandons functionizing this claim: cache the
        // built body for the scheduler's slow path to reuse, mark the
        // plan absent, and fall through to the non-lenient `evaluate`
        // (matches the pre-decomposition behavior — see `ComponentOutcome::Bail`).
        if bail {
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().last_extract_ok = Some(false);
            let cached_static: CachedSchema<'static> = cached;
            self.slow_path_cache.borrow_mut()
                .insert(cache_key.clone(), Rc::new(cached_static));
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }

        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.last_extract_ok = Some(true);
            per.components += n_components as u32;
            per.components_compiled += n_compiled;
            if n_compiled > 0 { per.compiled += 1; }
        }
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
            || std::env::var("EVIDENT_FUNCTIONIZE_STATS").is_ok()
        {
            eprintln!("[fz/stats] {}: components={} compiled={} slow_outputs={} simplified={}",
                name, n_components, n_compiled, uncompiled_outputs.len(), simplified.len());
        }

        // Build the combined slow part for the components that didn't
        // compile (plus given-only consistency assertions). Only the
        // uncompiled components' constraints go in — the compiled ones
        // are handled natively — so this solve is strictly smaller than
        // the full claim, and it's cached: each tick is push → assert
        // given → check → extract → pop, no re-translation. When every
        // component compiled there's no slow part and the plan is a
        // pure JIT call.
        let slow = if uncompiled_outputs.is_empty() {
            None
        } else {
            let mut slow_assertions: Vec<Bool<'static>> = uncompiled_assert_idx.iter()
                .map(|&i| simplified[i].clone()).collect();
            for &i in &global_idx { slow_assertions.push(simplified[i].clone()); }
            let slow_solver = build_tuned_solver(self.z3_ctx, arith);
            for a in &slow_assertions { slow_solver.assert(a); }
            // Move the env out of the full cache; the full-body solver
            // is dropped (we only needed it for gap-fill models above).
            let CachedSchema { env, .. } = cached;
            Some(SlowPart {
                cached: CachedSchema { env, solver: slow_solver, arith_solver: arith },
                outputs: uncompiled_outputs,
            })
        };

        let plan = Rc::new(ClaimPlan { compiled, slow, pinned_ints });
        self.fn_cache.borrow_mut().insert(cache_key, Some(plan.clone()));
        self.execute_plan(&plan, given)
    }

    /// Compile one decomposed component to a callable artifact, scoped
    /// to its own outputs + assertions. Mirrors the monolithic
    /// extract → recompose → gap-fill → compile pipeline, but scoped to
    /// this component. Returns a `ComponentOutcome`:
    ///   * `Compiled` — native artifact ready;
    ///   * `Slow`     — solve in the scoped slow part (Set output,
    ///                  `Guarded`/codegen refusal, extract cycle);
    ///   * `Bail`     — gap-fill refused; the whole claim must fall to
    ///                  the non-lenient `evaluate`.
    fn compile_one_component(
        &self,
        name: &str,
        comp_outputs: &[String],
        comp_assertions: &[Bool<'static>],
        cached: &CachedSchema<'static>,
        given: &HashMap<String, Value>,
        pinned_steps: &[Z3Step<'static>],
    ) -> ComponentOutcome {
        let _ = name;
        if comp_outputs.is_empty() { return ComponentOutcome::Slow; }
        // The JIT can't represent a Set output — a Z3 Set is a
        // characteristic function (`Array elem → Bool`), and the codegen
        // would misread its `store`-chain as a value-Seq. Send any
        // Set-bearing component to the slow path, where run_cached's
        // `extract_set` produces the right value from the candidate list.
        for v in comp_outputs {
            if matches!(cached.env.get(v),
                Some(Var::SetVar { .. }) | Some(Var::DatatypeSetVar { .. }))
            {
                return ComponentOutcome::Slow;
            }
        }
        let comp_out_vec: Vec<String> = comp_outputs.to_vec();
        let Some((mut program, mut missing)) =
            extract_program_partial(comp_assertions, &comp_out_vec)
        else {
            // Extraction cycle — the scoped slow solve handles it.
            return ComponentOutcome::Slow;
        };
        // Recompose record-element Seq outputs (Z3's simplify breaks a
        // whole-element ctor pin into per-field accessor pins).
        if !missing.is_empty() {
            recompose_record_seqs(
                comp_assertions, &mut missing, &mut program, &self.datatypes, self.z3_ctx);
        }
        if !missing.is_empty() {
            // Scoped unsafe-free check: would baking a model value be
            // unsafe for any var this component touches? It is, when the
            // var is neither given, a computed output, nor a constant —
            // its empty-given model value would be Z3's free choice,
            // wrong on later ticks.
            let mut touched: HashSet<String> = HashSet::new();
            for a in comp_assertions { collect_touched_names(a, &mut touched); }
            let output_set: HashSet<&str> = comp_out_vec.iter().map(|s| s.as_str()).collect();
            let missing_set: HashSet<&str> = missing.iter().map(|s| s.as_str()).collect();
            let mut unsafe_free = false;
            for n in &touched {
                let in_given = given.contains_key(n);
                let is_covered = output_set.contains(n.as_str())
                    && !missing_set.contains(n.as_str());
                let in_env = cached.env.contains_key(n);
                let is_const = cached.env.get(n).map(|v| matches!(v,
                    Var::PinnedInt(_) | Var::EnumValue { .. } | Var::EnumCtor { .. }))
                    .unwrap_or(false);
                if in_env && !in_given && !is_covered && !is_const {
                    unsafe_free = true;
                    break;
                }
            }
            if unsafe_free {
                // Can't safely bake. If the component carries a *defining*
                // constraint (an equality / guarded implication / select
                // pin — anything beyond a bare type-bound comparison),
                // the missing outputs are determined; the scoped slow
                // solve recovers them from the real `given`. If every
                // assertion is just a type bound, the output is genuinely
                // unconstrained (e.g. its defining constraint was dropped
                // by the translator) — bail so the non-lenient `evaluate`
                // surfaces that as an error instead of masking it with an
                // arbitrary value.
                if component_has_defining_assertion(comp_assertions) {
                    return ComponentOutcome::Slow;
                }
                return ComponentOutcome::Bail;
            }
            // Safe to gap-fill from a model of the full cached body. The
            // missing outputs are constants (record-Seq literals like
            // Mario's `platforms`), so the full body's model values for
            // them are correct regardless of `given`.
            if !matches!(cached.solver.check(), SatResult::Sat) {
                return ComponentOutcome::Bail;
            }
            let Some(model) = cached.solver.get_model() else {
                return ComponentOutcome::Bail;
            };
            let mut prebaked: Vec<Z3Step<'static>> = Vec::with_capacity(missing.len());
            for var_name in &missing {
                let Some(var) = cached.env.get(var_name) else {
                    return ComponentOutcome::Bail;
                };
                let mut tmp: HashMap<String, Value> = HashMap::new();
                crate::translate::extract_binding(
                    var_name, var, &model, self.z3_ctx, &mut tmp, Some(&self.enums));
                let Some(value) = tmp.remove(var_name) else {
                    return ComponentOutcome::Bail;
                };
                prebaked.push(Z3Step::PreBaked { var: var_name.clone(), value });
            }
            let mut all = prebaked;
            all.append(&mut program.steps);
            program.steps = all;
        }
        // Count the absorbed work (per-claim totals, summed over
        // components).
        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.steps_total      += program.steps.len() as u32;
            per.checks_total     += program.checks.len() as u32;
            per.predicates_total += program.predicates.len() as u32;
        }
        // Prepend pinned-int steps so component exprs that reference a
        // statically-known constant (e.g. `x_max = LEVEL_W - p_size.x`)
        // resolve it from env instead of loading an absent input.
        let mut all = pinned_steps.to_vec();
        all.append(&mut program.steps);
        program.steps = all;
        match self.functionizer.compile(&program, &self.enums, &self.datatypes) {
            // Codegen refused (e.g. a `Guarded` step) — the scoped slow
            // solve produces the right value, so this is not a Bail.
            Some(c) => ComponentOutcome::Compiled(c),
            None => ComponentOutcome::Slow,
        }
    }

    /// Run a cached `ClaimPlan`: call each compiled component, then
    /// solve the combined slow part (if any) with `given` pinned, and
    /// merge. Returns `None` (→ caller falls through to a full Z3
    /// solve) if a compiled component bails or the slow part is UNSAT.
    fn execute_plan(&self, plan: &ClaimPlan, given: &HashMap<String, Value>)
        -> Option<QueryResult>
    {
        let mut out: HashMap<String, Value> = HashMap::new();
        // Statically-pinned ints sit in no component; emit them first so
        // every result carries them (matches the monolithic path).
        for (k, v) in &plan.pinned_ints {
            if !given.contains_key(k) { out.insert(k.clone(), v.clone()); }
        }
        for c in &plan.compiled {
            let bindings = c.call(given)?;
            for (k, v) in bindings {
                if !given.contains_key(&k) { out.insert(k, v); }
            }
        }
        if let Some(slow) = &plan.slow {
            // Enum-typed givens (e.g. an FSM's `state`) aren't pinned by
            // run_cached's scalar/seq/set arms — pin them in an outer
            // frame and keep them out of the map run_cached sees (else
            // it would log a spurious "type mismatch" per tick).
            slow.cached.solver.push();
            let mut scalar_given: HashMap<String, Value> =
                HashMap::with_capacity(given.len());
            for (n, v) in given {
                match (slow.cached.env.get(n), v) {
                    (Some(Var::EnumVar { ast, .. }), Value::Enum { .. }) => {
                        if let Some(dt) = crate::translate::value_enum_to_datatype(
                            v, self.z3_ctx, &self.enums)
                        {
                            slow.cached.solver.assert(&ast._eq(&dt));
                        }
                    }
                    _ => { scalar_given.insert(n.clone(), v.clone()); }
                }
            }
            let r = run_cached(&slow.cached, &scalar_given, self.z3_ctx, Some(&self.enums));
            slow.cached.solver.pop(1);
            if !r.satisfied { return None; }
            for vn in &slow.outputs {
                if let Some(v) = r.bindings.get(vn) {
                    out.insert(vn.clone(), v.clone());
                }
            }
        }
        for (k, v) in given { out.insert(k.clone(), v.clone()); }
        Some(QueryResult { satisfied: true, bindings: out })
    }

    /// Evaluate the named schema and return whether it's satisfiable
    /// plus a model. `given` pre-binds variables to concrete values
    /// (mirrors the Python `query(schema, given=...)` parameter).
    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;

        // Functionizer fast path: extract a Z3Program from the body
        // and JIT-compile to native code. On miss (extract refused
        // or JIT codegen refused) we fall through to a full Z3 solve.
        let functionize_on = std::env::var("EVIDENT_FUNCTIONIZE")
            .map(|s| s != "0").unwrap_or(true);
        if functionize_on {
            if let Some(result) = self.try_functionize_z3(name, schema, given) {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz/z3] HIT {}", name);
                }
                return Ok(result);
            }
        }

        // One-shot query: don't auto-tune (no chance to learn over many
        // calls). Use the env override if set, default 2 (the value
        // that wins on Z3 4.8.12 for our typical workload).
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Faster query — translates the schema once on first call and
    /// reuses the resulting Z3 solver across subsequent calls
    /// (push/pop per query). Mirrors Python's `query(name, given,
    /// cached=True)` and the `evaluate_cached` optimization.
    ///
    /// **Structural-signature invalidation.** The cache stores the
    /// subset of the previous `given` keyed on names that appear in
    /// quantifier bounds — the structural signature. If this query's
    /// signature differs (e.g. a config value that drives an unroll
    /// count just changed), the cache is dropped and rebuilt against
    /// the new given. Non-structural changes (player position, etc.)
    /// reuse the cache and just re-assert the new value per-query.
    ///
    /// Bindings, satisfaction result, and overall semantics are
    /// identical to `query()`. Faster when called many times against
    /// the same schema with mostly-stable structural givens (e.g. an
    /// executor stepping a state machine 60×/sec where lengths and
    /// bound names don't change).
    pub fn query_cached(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<QueryResult, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();   // cheap: SchemaDecl is small + Arc-friendly clones
        let cur_sig = structural_signature(&schema.body, given);

        // Auto-tuner: which arith.solver should the cache use right now?
        let arith_solver = {
            let mut hist = self.solve_history.borrow_mut();
            hist.entry(name.to_string()).or_insert_with(SolveHistory::new)
                .current_config()
        };

        let mut cache = self.cache.borrow_mut();
        // Rebuild if (a) no entry, (b) structural signature changed, or
        // (c) cached config doesn't match the auto-tuner's current pick.
        let needs_rebuild = match cache.get(name) {
            Some((cached, cached_sig)) =>
                cached_sig != &cur_sig || cached.arith_solver != arith_solver,
            None => true,
        };
        if needs_rebuild {
            if cache.contains_key(name) {
                *self.cache_rebuilds.borrow_mut() += 1;
            }
            let names = crate::translate::structural_names(&schema.body);
            let structural_given: HashMap<String, Value> = given.iter()
                .filter(|(k, _)| names.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let new_cached = build_cache(
                &schema, &self.schemas, self.z3_ctx, &self.datatypes,
                Some(&self.enums), &structural_given, arith_solver);
            cache.insert(name.to_string(), (new_cached, cur_sig));
        }
        let entry = cache.get(name).unwrap();

        // Time the actual solve so the auto-tuner can decide whether to
        // advance to the next pricing window.
        let t0 = Instant::now();
        let r = run_cached(&entry.0, given, self.z3_ctx, Some(&self.enums));
        let dt = t0.elapsed();
        drop(cache);  // release before we may invalidate below

        // Record the timing. If the tuner says to switch configs,
        // evict so the next call rebuilds under the new value.
        if let Some(_new_cfg) = self.solve_history.borrow_mut()
            .get_mut(name).and_then(|h| h.record(dt))
        {
            self.cache.borrow_mut().remove(name);
        }
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}
