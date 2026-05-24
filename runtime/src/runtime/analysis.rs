//! Structural decomposition + component-classification queries.

use crate::core::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use std::collections::HashMap;

impl EvidentRuntime {
    /// Structural decomposition pass: re-separate the named claim into
    /// the independent sub-models it was composed from. Returns a list
    /// of `Component`s, each holding the variable names in that
    /// independent piece. See `docs/design/compile-claims-to-functions.md`
    /// ("Decomposition") for the architectural framing.
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

    /// Decomposition + per-component functionality verdict via the
    /// 2-copy uniqueness check. Returns a list of `ClassifiedComponent`s
    /// flagging which components are function-shaped (outputs uniquely
    /// determined by inputs) vs search-shaped. Cost: roughly 1+N Z3
    /// calls (the initial solve plus one check per component); each
    /// component-level check is small.
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

    /// Like `query`, but on UNSAT also returns the unsat-core: indices
    /// into the schema's `body` for the constraints Z3 identified as
    /// the conflicting subset. Used by `evident test` to highlight
    /// which assertions made a `sat_*` test fail. Givens are not
    /// tracked — the core only includes schema body items.
    pub fn query_with_core(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<(QueryResult, Option<Vec<usize>>), RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_core(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        let qr = QueryResult { satisfied: r.satisfied, bindings: r.bindings };
        Ok((qr, r.unsat_core_items))
    }

    /// Convenience: query without any pre-bound values.
    pub fn query_free(&self, name: &str) -> Result<QueryResult, RuntimeError> {
        self.query(name, &HashMap::new())
    }
}
