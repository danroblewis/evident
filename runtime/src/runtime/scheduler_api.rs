//! Per-tick query entry points for the multi-FSM scheduler.

use super::errors::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use std::collections::HashMap;

impl EvidentRuntime {
    /// Pin one or more enum-typed (Datatype) variables across a
    /// single query. Each entry of `pins` is `(var_name, value)`.
    /// Used by the multi-FSM scheduler to fix `state` and
    /// `last_results` per tick — see the "execution-layer
    /// extension surface" section in the module docs.
    pub fn query_with_pinned_datatypes(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
    ) -> Result<QueryResult, RuntimeError> {
        self.query_with_pins_and_given(claim_name, pins, &HashMap::new())
    }

    /// Like `query_with_pinned_datatypes` but also accepts a
    /// `given` map for scalar pins (Int/Bool/String/Real values).
    /// Used by the multi-FSM scheduler to thread `world_next.*`
    /// writer values into reader `world.*` slots within the same
    /// tick — see the "execution-layer extension surface"
    /// section in the module docs.
    pub fn query_with_pins_and_given(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
        given: &HashMap<String, Value>,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        // Function-izer fast path on the SCHEDULER side. The
        // scheduler passes realistic per-tick given values (state,
        // last_results, _world.X). State-pair FSMs ALSO get a
        // `pins` array with the state pinned as a Z3 Datatype —
        // we used to bail in that case, but the scheduler now also
        // surfaces the state's Value form in `given` (see
        // `effect_loop.rs::run_with_ctx` around the
        // `current_state_v` insertion). So the function-izer can
        // fire even with non-empty pins; the pinned Datatype is
        // simply redundant with the given Value. If function-izer
        // rejects, fall through to Z3 with `pins` intact.
        // Z3 functionizer + Cranelift JIT, enabled by default. JIT
        // compiles the extracted Z3Program to native code; on miss
        // (extract or codegen refused) falls through to the slow
        // path below. Disable with EVIDENT_FUNCTIONIZE=0.
        let functionize_on = std::env::var("EVIDENT_FUNCTIONIZE")
            .map(|s| s != "0").unwrap_or(true);
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

        // Slow-path cache: if the function-izer already built a
        // CachedSchema and stored it (because it refused to produce
        // a JIT program), reuse it here instead of rebuilding
        // the body. Each tick is push → assert pins/given → check
        // → extract model → pop. For Mario's display this cuts the
        // per-tick cost from ~14ms (fresh translation) to ~2-3ms
        // (just the solve + extract).
        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (claim_name.to_string(), given_keys);
        if let Some(cached) = self.slow_path_cache.borrow().get(&cache_key).cloned() {
            if std::env::var("EVIDENT_TRACE_SLOW_PATH").is_ok() {
                eprintln!("[slow/cached] {claim_name}");
            }
            use z3::ast::Ast;
            cached.solver.push();
            // Apply the typed Datatype pins (state).
            for (var_name, value) in pins {
                if let Some(crate::translate::Var::EnumVar { ast, .. }) = cached.env.get(*var_name) {
                    cached.solver.assert(&ast._eq(value));
                }
            }
            // Apply Value::Enum givens here (run_cached doesn't, to
            // keep its lifetime parametric). We have 'static context
            // so we can re-encode the enum value as a Datatype and
            // assert equality on the EnumVar's ast.
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
