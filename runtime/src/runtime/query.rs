//! `query` + `query_cached` — Z3-only solve path (no JIT).

use super::autotune::SolveHistory;
use crate::core::{QueryResult, RuntimeError};
use super::{EvidentRuntime, Value};
use crate::translate::{build_cache, run_cached, structural_signature};
use std::collections::HashMap;
use std::time::Instant;

impl EvidentRuntime {
    /// Evaluate the named schema; `given` pre-binds variables.
    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let base = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let resolved = self.resolve_runs(base, given)?;
        let schema = resolved.as_ref().unwrap_or(base);
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Like `query` but reuses a cached Z3 solver (push/pop per call). Cache is rebuilt when
    /// structural givens (quantifier bounds) change; non-structural changes re-assert in place.
    pub fn query_cached(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<QueryResult, RuntimeError>
    {
        let base = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let schema = match self.resolve_runs(base, given)? {
            Some(rewritten) => rewritten,
            None => base.clone(),
        };
        let cur_sig = structural_signature(&schema.body, given);

        let arith_solver = {
            let mut hist = self.solve_history.borrow_mut();
            hist.entry(name.to_string()).or_insert_with(SolveHistory::new)
                .current_config()
        };

        let mut cache = self.cache.borrow_mut();
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

        let t0 = Instant::now();
        let r = run_cached(&entry.0, given, self.z3_ctx, Some(&self.enums));
        let dt = t0.elapsed();
        drop(cache);
        if let Some(_new_cfg) = self.solve_history.borrow_mut()
            .get_mut(name).and_then(|h| h.record(dt))
        {
            self.cache.borrow_mut().remove(name);
        }
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}
