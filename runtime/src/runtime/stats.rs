//! Aggregate functionizer + JIT statistics.

use std::collections::HashMap;

/// Aggregate functionizer + JIT statistics across all
/// (claim, given-keys) cache-miss attempts in this runtime's
/// lifetime. Inspected via `EvidentRuntime::functionize_stats()`.
#[derive(Default, Clone, Debug)]
pub struct FunctionizeStats {
    pub claims: HashMap<String, PerClaimStats>,
}

#[derive(Default, Clone, Debug)]
pub struct PerClaimStats {
    /// Number of cache-miss analyses we ran on this claim.
    pub analyses: u32,
    /// Cache-hit count (where the cached JIT program was used).
    pub cache_hits: u32,
    /// Number of analyses where Z3's simplify decided the body
    /// is UNSAT (short-circuit return).
    pub decided_unsat: u32,
    /// `simplified_assertions` from Z3's tactic pipeline,
    /// summed across analyses. Divide by `analyses` for the
    /// per-call average.
    pub simplified_total: u32,
    /// Steps in the extracted Z3Program, summed. A step is
    /// either a scalar substitution, a Seq construction, or a
    /// guarded equality chain. These are constraints we
    /// "removed" from Z3 — the value is computed directly.
    pub steps_total: u32,
    /// Consistency checks in the program, summed. Equalities
    /// between non-output vars (e.g. `state = Init` when state
    /// is given) — we verify them at eval but they don't
    /// drive output computation.
    pub checks_total: u32,
    /// Bool predicates in the program, summed. Non-equality
    /// assertions like `n > 0` from Nat bounds. Verified at
    /// eval; unevaluable ones are SKIPPED (trusting Z3's prior
    /// validation).
    pub predicates_total: u32,
    /// `Some(true)` if extract_program succeeded for the
    /// most recent analysis; `Some(false)` if not; None if
    /// no analysis ran yet.
    pub last_extract_ok: Option<bool>,
    /// Number of analyses where the functionizer successfully
    /// compiled at least one component of the claim. Miss → the whole
    /// claim slow-paths.
    pub compiled: u32,
    /// Total independent components the claim decomposed into,
    /// summed across analyses.
    pub components: u32,
    /// Components that compiled to a callable artifact, summed across
    /// analyses. `components_compiled < components` means partial
    /// compilation — the rest are solved by the cached scoped Z3 solver.
    pub components_compiled: u32,
}
