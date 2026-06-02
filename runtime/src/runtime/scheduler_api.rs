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
        let base = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let resolved = self.resolve_runs(base, given)?;
        let schema = resolved.as_ref().unwrap_or(base);
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
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
