//! Structural decomposition + component-classification queries.

use crate::core::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use std::collections::HashMap;

impl EvidentRuntime {
    /// Decompose the named claim into independent sub-models (Components).
    pub fn analyze_decomposition(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<Vec<crate::decompose::Component>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        Ok(crate::translate::analyze_decomposition(
            schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith))
    }

    /// Decompose + classify components as function-shaped vs search-shaped via 2-copy
    /// uniqueness check. Costs ~1+N Z3 calls (one initial + one per component).
    pub fn classify_components(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<Vec<crate::translate::ClassifiedComponent>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        Ok(crate::translate::classify_components(
            schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith))
    }

    /// Like `query`, but on UNSAT also returns the unsat-core (body indices).
    /// Used by `evident test`; givens are not tracked in the core.
    pub fn query_with_core(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<(QueryResult, Option<Vec<usize>>), RuntimeError>
    {
        let base = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        // Resolve nested FSMs before the cored solve.
        let resolved = self.resolve_runs(base, given)?;
        let schema = resolved.as_ref().unwrap_or(base);
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_core(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        let qr = QueryResult { satisfied: r.satisfied, bindings: r.bindings };
        Ok((qr, r.unsat_core_items))
    }

    /// Query with no pre-bound values.
    pub fn query_free(&self, name: &str) -> Result<QueryResult, RuntimeError> {
        self.query(name, &HashMap::new())
    }
}
