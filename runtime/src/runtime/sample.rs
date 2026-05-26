//! Distinct-model enumeration via the `sample` API.

use crate::core::RuntimeError;
use super::{EvidentRuntime, Value};
use crate::translate::{build_cache, sample_cached_inner};
use std::collections::HashMap;

impl EvidentRuntime {
    /// Return up to `n` distinct satisfying models via blocking clauses.
    /// Limitation: blocking skips Seq/Set bindings, so Seq-only outputs may return duplicates.
    pub fn sample(&self, name: &str, given: &HashMap<String, Value>, n: usize)
        -> Result<Vec<HashMap<String, Value>>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();
        // Fresh non-shared solver: arith.solver=2 is pathologically slow with
        // cumulative blocking clauses, and sample's push/pop shouldn't taint the per-frame solver.
        let names = crate::translate::structural_names(&schema.body);
        let structural_given: HashMap<String, Value> = given.iter()
            .filter(|(k, _)| names.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // 0 = don't call set_params; leaves Z3 at its default arith path (avoids solver=2 pathology).
        let cached = build_cache(
            &schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &structural_given, 0);
        Ok(sample_cached_inner(&cached, given, n, self.z3_ctx, Some(&self.enums)))
    }
}
