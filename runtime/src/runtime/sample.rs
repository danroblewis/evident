//! Distinct-model enumeration via the `sample` API.

use crate::core::RuntimeError;
use super::{EvidentRuntime, Value};
use crate::translate::{build_cache, sample_cached_inner};
use std::collections::HashMap;

impl EvidentRuntime {
    /// Return up to `n` distinct satisfying models. Uses the cached
    /// solver: one push for the per-query givens, then accumulating
    /// blocking clauses (¬(b1=v1 ∧ … ∧ bn=vn) for each scalar binding)
    /// across iterations until either `n` distinct models or UNSAT.
    /// All blocking clauses + givens are popped before returning so the
    /// cached solver is unchanged from the caller's perspective.
    ///
    /// Limitation (v1): blocking only covers Bool, Int, Str bindings.
    /// Seq/Set values are skipped from the blocking conjunction, so
    /// schemas whose only varying outputs are sequences will return
    /// duplicates. See `sample_cached_inner` in translate.rs.
    pub fn sample(&self, name: &str, given: &HashMap<String, Value>, n: usize)
        -> Result<Vec<HashMap<String, Value>>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();
        // Sample uses its own fresh, non-shared cached solver. Two reasons:
        //   1. `arith.solver=2` (the runtime's per-frame default and a
        //      candidate in the auto-tuner) is pathologically slow on
        //      sample_cached_inner's cumulative blocking-clause workload.
        //   2. The blocking clauses asserted inside sample's outer push
        //      shouldn't influence the per-frame solver state that the
        //      auto-tuner is timing.
        // Sample is rare and amortizes the build_cache cost across N
        // models, so the lack of cross-call caching is acceptable.
        let names = crate::translate::structural_names(&schema.body);
        let structural_given: HashMap<String, Value> = given.iter()
            .filter(|(k, _)| names.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Sample's "safe" config: leave Z3 at its default arith path.
        // 0 means "don't call set_params". Empirically this avoids the
        // solver=2 blocking-clause pathology.
        let cached = build_cache(
            &schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &structural_given, 0);
        Ok(sample_cached_inner(&cached, given, n, self.z3_ctx, Some(&self.enums)))
    }
}
