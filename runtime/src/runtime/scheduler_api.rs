//! Per-tick query entry points for the multi-FSM scheduler.

use crate::core::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use std::collections::HashMap;

impl EvidentRuntime {
    /// Collect the effects this tick would DISPATCH, in dispatch order — exactly
    /// the scheduler's `collect_dispatchable_effects`. `primary_var` is the FSM's
    /// `effects` slot for **mode 1** (dispatch is the literal `Seq` order);
    /// `None` selects **mode 2** (scrape every `Effect`-valued binding and
    /// toposort by the `Seq(Effect)` ordering-edge declarations). Exposed so the
    /// behavior-contract harness can witness mode-2 ordering against the real
    /// runtime, rather than reading a single pre-ordered binding.
    pub fn collect_tick_effects(
        &self,
        claim_name: &str,
        bindings: &HashMap<String, Value>,
        primary_var: Option<&str>,
    ) -> Vec<crate::core::ast::Effect> {
        crate::effect_loop::collect_dispatchable_effects(self, claim_name, bindings, primary_var)
    }

    /// Pin enum-typed (Datatype) variables for one query; used by the scheduler
    /// to fix `state` and `last_results` per tick.
    pub fn query_with_pinned_datatypes(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
    ) -> Result<QueryResult, RuntimeError> {
        self.query_with_pins_and_given(claim_name, pins, &HashMap::new())
    }

    /// Like `query_with_pinned_datatypes` but also accepts scalar givens;
    /// threads `world_next.*` writer values into reader `world.*` slots within the same tick.
    pub fn query_with_pins_and_given(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
        given: &HashMap<String, Value>,
    ) -> Result<QueryResult, RuntimeError> {
        // SMT-LIB-driven FSM (strategy 2): route the per-tick solve to the
        // SMT-LIB path instead of the Evident-AST evaluator. Empty registry by
        // default, so the Evident-source path below is untouched.
        {
            let reg = self.smtlib_fsms.borrow();
            if let Some(fsm) = reg.get(claim_name) {
                return Ok(crate::smtlib_fsm::solve_tick(self, fsm, pins, given));
            }
        }
        let base = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        // Tier-3: if the body has `run(F, init)`, drive it to a final value before solving.
        // resolve_runs returns None for F itself (no `run`) so there's no mutual recursion.
        let resolved = self.resolve_runs(base, given)?;
        let schema = resolved.as_ref().unwrap_or(base);
        // JIT fires even with non-empty pins: Datatype pin is redundant with `current_state_v` in given.
        // Skip for `run`-containing bodies: cache keys on given-KEYS, but `run` output depends on VALUES.
        let had_run = resolved.is_some();
        let functionize_on = !had_run
            && std::env::var("EVIDENT_FUNCTIONIZE").map(|s| s != "0").unwrap_or(true);
        if functionize_on {
            if let Some(result) = self.try_functionize_z3(claim_name, schema, given) {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz/z3] HIT (scheduler) {}", claim_name);
                }
                return Ok(result);
            }
        }
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);

        // Slow-path cache: reuse CachedSchema when JIT refused; push→assert→check→pop per tick.
        // Cuts per-tick cost from ~14ms (fresh translation) to ~2-3ms.
        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (claim_name.to_string(), given_keys);
        let cached_lookup = if had_run { None }
            else { self.slow_path_cache.borrow().get(&cache_key).cloned() };
        if let Some(cached) = cached_lookup {
            if std::env::var("EVIDENT_TRACE_SLOW_PATH").is_ok() {
                eprintln!("[slow/cached] {claim_name}");
            }
            use z3::ast::Ast;
            cached.solver.push();
            // Assert typed Datatype pins (state).
            for (var_name, value) in pins {
                if let Some(crate::translate::Var::EnumVar { ast, .. }) = cached.env.get(*var_name) {
                    cached.solver.assert(&ast._eq(value));
                }
            }
            // Assert Value::Enum givens: run_cached skips these (lifetime-parametric);
            // we have 'static context so we can re-encode and assert here.
            for (name, value) in given {
                if let (Some(crate::translate::Var::EnumVar { ast, .. }), Value::Enum { .. }) =
                    (cached.env.get(name), value)
                {
                    if let Some(dt) = crate::translate::value_enum_to_datatype(
                        value, self.z3_ctx, &self.enums)
                    {
                        cached.solver.assert(&ast._eq(&dt));
                    }
                }
            }
            let r = crate::translate::run_cached(&cached, given, self.z3_ctx, Some(&self.enums));
            cached.solver.pop(1);
            return Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings });
        }

        if std::env::var("EVIDENT_TRACE_SLOW_PATH").is_ok() {
            eprintln!("[slow] {claim_name}: dispatching to evaluate_with_extra_assertions");
        }
        let r = crate::translate::evaluate_with_extra_assertions(
            schema,
            given,
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            arith,
            pins,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}
