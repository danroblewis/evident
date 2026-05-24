//! `query`, `query_cached`, and the Z3-AST functionizer fast path.

use super::autotune::SolveHistory;
use super::errors::{QueryResult, RuntimeError};
use super::lenient::LenientGuard;
use super::{EvidentRuntime, Value};
use crate::translate::{build_cache, run_cached, structural_signature};
use std::collections::HashMap;
use std::time::Instant;

impl EvidentRuntime {
    /// Z3-AST functionizer. Runs Z3's tactic chain on the body
    /// (simplify + propagate-values), extracts per-output
    /// substitutions from the resulting Z3 ASTs, and JIT-compiles
    /// them to native code via Cranelift.
    ///
    /// Returns `Some(QueryResult)` on JIT-compile success + call
    /// success, `None` to fall through to a full Z3 solve.
    ///
    /// Per (claim, given-keys), the Z3Program is built once and
    /// cached. JIT-compiled code is also cached and runs at ~µs/call.
    pub(super) fn try_functionize_z3(&self, name: &str, schema: &crate::ast::SchemaDecl,
                          given: &HashMap<String, Value>) -> Option<QueryResult>
    {
        use crate::z3_eval::{simplify_assertions, extract_program};
        // Cache key: name + sorted given_keys. The program built
        // is somewhat sensitive to given VALUES (Z3 may constant-
        // fold them in), but for the common case where given_keys
        // are stable per FSM, the cached program remains correct
        // for any value of those inputs IF Z3 left the relevant
        // dispatch intact (e.g., `match state` becomes a guarded
        // program). When Z3 folded a given value into a constant,
        // the program will be wrong for different values of that
        // key — eval_program returns None in that case (predicate
        // check fails) and we fall through to the slow path.
        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (name.to_string(), given_keys.clone());

        // Functionizer cache lookup — compiled function is the only
        // fast path. Cache miss falls through to extract + compile
        // below. Compile failure caches None and falls through to
        // slow-path Z3.
        if let Some(entry) = self.fn_cache.borrow().get(&cache_key).cloned() {
            let Some(compiled) = entry else { return None };
            let Some(bindings) = compiled.call(given) else { return None };
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().cache_hits += 1;
            let mut out = HashMap::new();
            for (k, v) in bindings {
                if !given.contains_key(&k) { out.insert(k, v); }
            }
            for (k, v) in given { out.insert(k.clone(), v.clone()); }
            return Some(QueryResult { satisfied: true, bindings: out });
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
            self.functionize_z3_cache.borrow_mut().insert(cache_key, None);
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
        let pinned_steps: Vec<crate::z3_eval::Z3Step<'static>> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::translate::Var::PinnedInt(i) => Some(crate::z3_eval::Z3Step::Scalar {
                    var:  n.clone(),
                    expr: z3::ast::Dynamic::from_ast(&z3::ast::Int::from_i64(self.z3_ctx, *i)),
                }),
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
            self.functionize_z3_cache.borrow_mut().insert(cache_key, None);
            return None;
        }
        let (mut program, missing) = match crate::z3_eval::extract_program_partial(&simplified, &outputs) {
            Some(p) => p,
            None => {
                self.functionize_stats.borrow_mut()
                    .claims.entry(name.to_string()).or_default().last_extract_ok = Some(false);
                self.functionize_z3_cache.borrow_mut().insert(cache_key, None);
                return None;
            }
        };
        // Gap-fill missing outputs via model extraction. These are
        // typically constant record-Seqs whose per-element pins were
        // decomposed by Z3 into per-field accessor assertions
        // extract_program can't recompose (e.g. Mario's `platforms`,
        // `e_init`, `mario.rects`). The solver in `cached` already
        // holds the simplified body; one check() yields a model from
        // which we read each missing output's value and bake it as
        // a `Z3Step::PreBaked`.
        if !missing.is_empty() {
            use z3::SatResult;
            if !matches!(cached.solver.check(), SatResult::Sat) {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz/z3] {}: gap-fill check returned non-sat", name);
                }
                self.functionize_stats.borrow_mut()
                    .claims.entry(name.to_string()).or_default().last_extract_ok = Some(false);
                self.functionize_z3_cache.borrow_mut().insert(cache_key, None);
                return None;
            }
            let model = match cached.solver.get_model() {
                Some(m) => m,
                None => {
                    self.functionize_stats.borrow_mut()
                        .claims.entry(name.to_string()).or_default().last_extract_ok = Some(false);
                    self.functionize_z3_cache.borrow_mut().insert(cache_key, None);
                    return None;
                }
            };
            let mut prebaked: Vec<crate::z3_eval::Z3Step<'static>> = Vec::with_capacity(missing.len());
            // Refuse to gap-fill if the body references free Z3
            // variables that aren't in `given`. Gap-fill via model
            // bakes Z3's free choices for those variables into the
            // resulting PreBaked Value — but the runtime supplies
            // different values per tick (e.g. win.renderer from the
            // FTI bridge, world.X from neighboring FSMs). If we baked
            // Z3's choice (say renderer=33) into a SeqComposite of
            // plat_effs, the JIT-emitted SDL calls would target a
            // bogus renderer handle and nothing would render.
            //
            // Heuristic: collect every 0-arity App in the simplified
            // body. If any of them is in cached.env but NOT in given
            // and NOT an output of the program (i.e. truly free, not
            // computed), refuse gap-fill — the model's choice for
            // that var would corrupt the baked values.
            {
                let mut all_touched: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for a in simplified {
                    crate::z3_eval::collect_touched_names(a, &mut all_touched);
                }
                let output_set: std::collections::HashSet<&str> = outputs.iter()
                    .map(|s| s.as_str()).collect();
                let missing_set: std::collections::HashSet<&str> = missing.iter()
                    .map(|s| s.as_str()).collect();
                let mut unsafe_free: Vec<String> = Vec::new();
                for n in &all_touched {
                    let in_given = given.contains_key(n);
                    let is_covered_output =
                        output_set.contains(n.as_str())
                            && !missing_set.contains(n.as_str());
                    let in_env = cached.env.contains_key(n);
                    // PinnedInt / EnumValue / EnumCtor are constants —
                    // not free in the Z3 sense.
                    let is_const = cached.env.get(n).map(|v| matches!(v,
                        crate::translate::Var::PinnedInt(_)
                        | crate::translate::Var::EnumValue { .. }
                        | crate::translate::Var::EnumCtor { .. })
                    ).unwrap_or(false);
                    if in_env && !in_given && !is_covered_output && !is_const {
                        unsafe_free.push(n.clone());
                    }
                }
                if !unsafe_free.is_empty() {
                    if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                        let mut v = unsafe_free.clone();
                        v.sort();
                        eprintln!("[fz/z3] {name}: refusing gap-fill — body has {} \
                                  free non-given vars whose model values would be baked: {:?}",
                            unsafe_free.len(),
                            &v[..v.len().min(8)]);
                    }
                    self.functionize_stats.borrow_mut()
                        .claims.entry(name.to_string()).or_default().last_extract_ok = Some(false);
                    self.functionize_z3_cache.borrow_mut().insert(cache_key.clone(), None);
                    // Stash the already-built CachedSchema for the
                    // slow path to reuse — it's expensive to rebuild
                    // the body's Z3 assertions from AST every tick.
                    // The 'static-lifetime transmute is sound because
                    // self.z3_ctx is 'static and lives for the runtime's
                    // lifetime.
                    let cached_static: crate::translate::CachedSchema<'static> = cached;
                    self.slow_path_cache.borrow_mut().insert(
                        cache_key, std::rc::Rc::new(cached_static));
                    return None;
                }
            }
            for var_name in &missing {
                let Some(var) = cached.env.get(var_name) else {
                    if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                        eprintln!("[fz/z3] {name}: gap-fill: missing env entry for {var_name:?}");
                    }
                    self.functionize_z3_cache.borrow_mut().insert(cache_key, None);
                    return None;
                };
                let mut tmp: HashMap<String, Value> = HashMap::new();
                crate::translate::extract_binding(var_name, var, &model, self.z3_ctx, &mut tmp, Some(&self.enums));
                let Some(value) = tmp.remove(var_name) else {
                    if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                        eprintln!("[fz/z3] {name}: gap-fill: extract_binding produced no value for {var_name:?}");
                    }
                    self.functionize_z3_cache.borrow_mut().insert(cache_key, None);
                    return None;
                };
                prebaked.push(crate::z3_eval::Z3Step::PreBaked {
                    var: var_name.clone(),
                    value,
                });
            }
            // Place pre-baked steps at the front (no dependencies).
            let mut all_steps = prebaked;
            all_steps.append(&mut program.steps);
            program.steps = all_steps;
            if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                eprintln!("[fz/z3] {name}: gap-filled {} outputs via model extraction", missing.len());
            }
        }
        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.last_extract_ok = Some(true);
            per.steps_total      += program.steps.len() as u32;
            per.checks_total     += program.checks.len() as u32;
            per.predicates_total += program.predicates.len() as u32;
        }
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
            || std::env::var("EVIDENT_FUNCTIONIZE_STATS").is_ok()
        {
            eprintln!("[fz/stats] {}: simplified={} steps={} checks={} preds={} outputs={}",
                name, simplified.len(),
                program.steps.len(), program.checks.len(), program.predicates.len(),
                outputs.len());
        }
        // Prepend pinned-int steps (they have no dependencies; safe
        // to place at the front of the chain).
        let mut all_steps = pinned_steps;
        all_steps.append(&mut program.steps);
        program.steps = all_steps;
        let stored = Some(program.clone());
        self.functionize_z3_cache.borrow_mut().insert(cache_key.clone(), stored);

        // Hand the extracted program to the configured functionizer.
        // On success, the next call hits the fast-path cache above
        // and runs at the strategy's per-call cost. On failure, cache
        // None so subsequent calls skip straight to slow-path Z3.
        let compiled = self.functionizer.compile(&program, &self.enums);
        if compiled.is_some() {
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().compiled += 1;
        }
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
            || std::env::var("EVIDENT_FUNCTIONIZE_STATS").is_ok()
        {
            eprintln!("[fz/stats] {}: {}={}", name, self.functionizer.name(),
                if compiled.is_some() { "yes" } else { "no" });
        }
        self.fn_cache.borrow_mut().insert(cache_key, compiled.clone());

        // Use the compiled function on the first call too if it succeeded.
        let compiled = compiled?;
        let bindings = compiled.call(given)?;
        let mut out: HashMap<String, Value> = HashMap::new();
        for (k, v) in bindings { out.insert(k, v); }
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
