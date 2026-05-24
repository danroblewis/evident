//! Analysis tooling: which variables a claim takes as input vs solves
//! *for*, and — of the solved-for variables — which one, if pinned,
//! would most reduce the Z3 solve cost.
//!
//! Three entry points:
//!
//!   * `given_vars(schema)`        — the claim-line interface (always
//!                                    supplied by the caller).
//!   * `solved_for_vars(schema, given_keys)` — body-declared variables
//!                                    that aren't params and aren't
//!                                    already given. Cheap, AST-only.
//!   * `bottleneck_vars(schema, given, top_n)` — the real work: solve
//!                                    once to get a model, then for
//!                                    each candidate leaf variable pin
//!                                    it to its model value and re-solve,
//!                                    ranking by how much the pin sped
//!                                    the solve.
//!
//! The bottleneck finder reuses one `build_cache` across every pinning
//! solve (push → assert pin → check → pop), so the per-candidate cost
//! is just one extra assertion + solve, never a re-translation. To
//! cancel the drift that incremental solving introduces (Z3 keeps
//! learned clauses across `check()` calls even through push/pop), each
//! candidate is measured against a *local* baseline solved immediately
//! before it — that's why `baseline_solve_us` is per-entry rather than
//! a single global number.
//!
//! See `commands/profile.rs` for the CLI that combines all three into
//! a report.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::SatResult;

use crate::core::ast::{BodyItem, SchemaDecl};
use crate::core::{CachedSchema, RuntimeError, Value, Var};
use super::lenient::LenientGuard;
use super::EvidentRuntime;

/// One row of the bottleneck ranking: how a single candidate
/// variable's solve cost compares pinned vs unpinned.
#[derive(Debug, Clone)]
pub struct BottleneckEntry {
    /// Env-leaf variable name (`frame`, `_world.player.pos.x`, …).
    pub var_name: String,
    /// Microseconds for the local baseline solve (nothing extra pinned),
    /// measured immediately before the pinned solve to cancel drift.
    pub baseline_solve_us: u128,
    /// Microseconds for the solve with this variable pinned to its
    /// baseline-model value.
    pub pinned_solve_us: u128,
    /// `baseline - pinned`. Positive = pinning this var sped the solve.
    /// Can be negative (the pin cost more than it saved, or noise).
    pub savings_us: i128,
    /// Per-key Z3 statistic delta (`baseline - pinned`). Keys are Z3's
    /// statistic names (`conflicts`, `decisions`, `propagations`, …).
    /// Only non-zero deltas are kept. Empty when the solver reported no
    /// statistics for either solve.
    pub z3_stats_delta: HashMap<String, i64>,
}

/// Result of one timed solve against a cached schema.
struct SolveTiming {
    /// Wall time of `Solver::check()` in microseconds.
    micros: u128,
    /// Z3 solver statistics keyed by name, as f64.
    stats: BTreeMap<String, f64>,
    satisfied: bool,
    /// Scalar (Int/Bool/String) model bindings — only populated when
    /// the caller asked to extract a model.
    scalars: HashMap<String, Value>,
}

impl EvidentRuntime {
    /// The claim-line interface variables — what a caller must supply.
    /// These are the first `param_count` body Memberships (the
    /// first-line `claim Foo(a ∈ X, …)` params). Always "given".
    pub fn given_vars(&self, schema: &str) -> Result<Vec<String>, RuntimeError> {
        let decl = self.schemas.get(schema)
            .ok_or_else(|| RuntimeError::UnknownSchema(schema.to_string()))?;
        Ok(decl.body.iter().take(decl.param_count)
            .filter_map(membership_name)
            .map(str::to_string)
            .collect())
    }

    /// The variables this claim solves *for*: every body-declared
    /// Membership name that is neither a claim-line param nor present
    /// in `given_keys`. Passthrough (`..Other`) bodies are flattened
    /// transitively, so variables a mixin introduces count too.
    ///
    /// Names are returned in first-declaration order, de-duplicated.
    pub fn solved_for_vars(&self, schema: &str, given_keys: &[String])
        -> Result<Vec<String>, RuntimeError>
    {
        let decl = self.schemas.get(schema)
            .ok_or_else(|| RuntimeError::UnknownSchema(schema.to_string()))?;
        let params: HashSet<&str> = decl.body.iter().take(decl.param_count)
            .filter_map(membership_name).collect();
        let given: HashSet<&str> = given_keys.iter().map(String::as_str).collect();

        let mut out: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();
        self.collect_memberships(decl, decl.param_count, &mut out, &mut seen, &mut visited);

        Ok(out.into_iter()
            .filter(|n| !params.contains(n.as_str()) && !given.contains(n.as_str()))
            .collect())
    }

    /// Walk a schema body collecting Membership names, recursing
    /// through Passthrough mixins. `skip` is the number of leading
    /// body items to ignore (the param prefix on the top-level decl;
    /// 0 for recursed passthrough bodies). `visited` guards against
    /// passthrough cycles.
    fn collect_memberships(
        &self,
        decl: &SchemaDecl,
        skip: usize,
        out: &mut Vec<String>,
        seen: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) {
        for (i, item) in decl.body.iter().enumerate() {
            match item {
                BodyItem::Membership { name, .. } => {
                    if i < skip { continue; }
                    if seen.insert(name.clone()) { out.push(name.clone()); }
                }
                BodyItem::Passthrough(pname) => {
                    if !visited.insert(pname.clone()) { continue; }
                    if let Some(sub) = self.schemas.get(pname) {
                        self.collect_memberships(sub, 0, out, seen, visited);
                    }
                }
                _ => {}
            }
        }
    }

    /// Rank the claim's solved-for leaf variables by how much pinning
    /// each one speeds the solve. Returns up to `top_n` entries sorted
    /// by `savings_us` descending.
    ///
    /// Algorithm: build a cache once, solve to get a baseline model,
    /// then for each scalar (Int/Bool/String) leaf the model bound —
    /// that isn't already `given`, isn't a compile-time constant, and
    /// belongs to a solved-for membership — pin it to its model value
    /// and re-solve. Each candidate is measured against a fresh local
    /// baseline solved immediately before it.
    ///
    /// Errors if the claim is UNSAT under `given` (the bottleneck
    /// question is meaningless without a satisfiable baseline).
    pub fn bottleneck_vars(
        &self,
        schema_name: &str,
        given: &HashMap<String, Value>,
        top_n: usize,
    ) -> Result<Vec<BottleneckEntry>, RuntimeError> {
        let schema = self.schemas.get(schema_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(schema_name.to_string()))?
            .clone();
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);

        // Build the cache once. Lenient so SDL / LibCall body items the
        // translator can't express become warnings instead of a fatal
        // exit — the bottleneck question is still meaningful over the
        // constraints that DID translate (this is the same cache the
        // runtime's slow path uses for such claims).
        let cached = {
            let _g = LenientGuard::enable();
            crate::translate::build_cache(
                &schema, &self.schemas, self.z3_ctx, &self.datatypes,
                Some(&self.enums), given, arith)
        };

        // Baseline solve: get a model to pin candidate values from.
        let base = self.solve_timed(&cached, given, true);
        if !base.satisfied {
            return Err(RuntimeError::Io(format!(
                "claim {schema_name:?} is UNSAT under the given bindings; \
                 bottleneck analysis needs a satisfiable baseline")));
        }

        // Candidates: scalar env leaves that (a) the baseline model
        // bound, (b) aren't already given, (c) belong to a solved-for
        // membership (so params and given-pinned vars are excluded;
        // compile-time-constant `PinnedInt` leaves never appear in the
        // scalar model in the first place).
        let given_keys: Vec<String> = given.keys().cloned().collect();
        let solved: HashSet<String> =
            self.solved_for_vars(schema_name, &given_keys)?.into_iter().collect();
        let mut candidates: Vec<String> = base.scalars.keys()
            .filter(|k| !given.contains_key(k.as_str()))
            .filter(|k| solved.contains(k.split('.').next().unwrap_or(k)))
            .cloned()
            .collect();
        candidates.sort();

        let mut entries: Vec<BottleneckEntry> = Vec::with_capacity(candidates.len());
        for v in &candidates {
            // Local baseline (no extra pin), measured adjacent to the
            // pinned solve so incremental-solving drift cancels out.
            let local = self.solve_timed(&cached, given, false);

            let mut pinned = given.clone();
            pinned.insert(v.clone(), base.scalars[v].clone());
            let p = self.solve_timed(&cached, &pinned, false);

            let mut delta: HashMap<String, i64> = HashMap::new();
            for (k, lv) in &local.stats {
                let pv = p.stats.get(k).copied().unwrap_or(0.0);
                let d = (lv - pv) as i64;
                if d != 0 { delta.insert(k.clone(), d); }
            }
            entries.push(BottleneckEntry {
                var_name: v.clone(),
                baseline_solve_us: local.micros,
                pinned_solve_us: p.micros,
                savings_us: local.micros as i128 - p.micros as i128,
                z3_stats_delta: delta,
            });
        }

        entries.sort_by(|a, b| b.savings_us.cmp(&a.savings_us)
            .then_with(|| a.var_name.cmp(&b.var_name)));
        entries.truncate(top_n);
        Ok(entries)
    }

    /// Push the cache's solver, assert `given` (scalars only), time the
    /// `check()`, read the solver's statistics, optionally extract the
    /// scalar model, then pop. Reuses the cached body assertions.
    fn solve_timed(
        &self,
        cached: &CachedSchema<'static>,
        given: &HashMap<String, Value>,
        extract: bool,
    ) -> SolveTiming {
        let ctx = self.z3_ctx;
        cached.solver.push();
        for (name, value) in given {
            let Some(var) = cached.env.get(name) else { continue };
            match (var, value) {
                (Var::IntVar(v), Value::Int(n)) =>
                    cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
                (Var::BoolVar(v), Value::Bool(b)) =>
                    cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
                (Var::StrVar(v), Value::Str(s)) => {
                    if let Ok(lit) = Z3Str::from_str(ctx, s) {
                        cached.solver.assert(&v._eq(&lit));
                    }
                }
                // A given that disagrees with a folded compile-time
                // constant forces UNSAT (mirrors `run_cached`).
                (Var::PinnedInt(p), Value::Int(n)) if *p == *n => {}
                (Var::PinnedInt(_), Value::Int(_)) =>
                    cached.solver.assert(&Bool::from_bool(ctx, false)),
                // Real / Seq / Set / Enum givens aren't pinned here —
                // scalar candidates are the only thing this tool pins,
                // and CLI `--given` only produces Int/Bool/String.
                _ => {}
            }
        }

        let t0 = Instant::now();
        let res = cached.solver.check();
        let micros = t0.elapsed().as_micros();
        let satisfied = matches!(res, SatResult::Sat);

        let mut stats: BTreeMap<String, f64> = BTreeMap::new();
        for entry in cached.solver.get_statistics().entries() {
            stats.insert(entry.key, stat_value_f64(&entry.value));
        }

        let mut scalars: HashMap<String, Value> = HashMap::new();
        if extract && satisfied {
            if let Some(model) = cached.solver.get_model() {
                for (n, var) in cached.env.iter() {
                    match var {
                        Var::IntVar(i) => {
                            if let Some(v) = model.eval(i, true).and_then(|x| x.as_i64()) {
                                scalars.insert(n.clone(), Value::Int(v));
                            }
                        }
                        Var::BoolVar(b) => {
                            if let Some(v) = model.eval(b, true).and_then(|x| x.as_bool()) {
                                scalars.insert(n.clone(), Value::Bool(v));
                            }
                        }
                        Var::StrVar(s) => {
                            if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                                scalars.insert(n.clone(), Value::Str(v));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        cached.solver.pop(1);
        SolveTiming { micros, stats, satisfied, scalars }
    }
}

/// Borrow a Membership's name, or `None` for any other body item.
fn membership_name(item: &BodyItem) -> Option<&str> {
    match item {
        BodyItem::Membership { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

/// Project a Z3 statistic value to f64 for uniform accumulation.
fn stat_value_f64(v: &z3::StatisticsValue) -> f64 {
    match v {
        z3::StatisticsValue::UInt(n) => *n as f64,
        z3::StatisticsValue::Double(d) => *d,
    }
}
