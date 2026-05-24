//! Aggregate functionizer + JIT statistics.

use std::collections::HashMap;

/// Aggregate functionizer + JIT statistics across all
/// (claim, given-keys) cache-miss attempts in this runtime's
/// lifetime. Inspected via `EvidentRuntime::functionize_stats()`
/// and printed automatically by `effect-run`'s timing summary.
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
    /// Cross-tick value-cache hits: calls where the `(claim,
    /// given-values)` result was already memoized, so we skipped the
    /// compiled-function call entirely and returned the prior bindings.
    /// Climbs on idle frames (identical inputs tick after tick); stays
    /// flat on active frames (each tick's input differs → cache miss).
    pub value_cache_hits: u32,
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

impl FunctionizeStats {
    pub fn print_summary(&self) {
        eprintln!("[fz/stats] ── summary ─────────────────────────────");
        let mut names: Vec<&String> = self.claims.keys().collect();
        names.sort();
        let mut total_a = 0u32;
        let mut total_h = 0u32;
        let mut total_vh = 0u32;
        let mut total_compiled = 0u32;
        let mut total_components = 0u32;
        let mut total_components_compiled = 0u32;
        let mut total_steps = 0u32;
        let mut total_checks = 0u32;
        let mut total_preds = 0u32;
        let mut total_simplified = 0u32;
        for n in &names {
            let s = &self.claims[*n];
            total_a += s.analyses;
            total_h += s.cache_hits;
            total_vh += s.value_cache_hits;
            total_compiled += s.compiled;
            total_components += s.components;
            total_components_compiled += s.components_compiled;
            total_steps += s.steps_total;
            total_checks += s.checks_total;
            total_preds += s.predicates_total;
            total_simplified += s.simplified_total;
        }
        eprintln!("[fz/stats] {} claims analyzed; {} analyses; {} cache hits; {} value-cache hits",
            names.len(), total_a, total_h, total_vh);
        eprintln!("[fz/stats] z3 simplified assertions: {} total ({:.1}/analysis)",
            total_simplified,
            if total_a > 0 { total_simplified as f64 / total_a as f64 } else { 0.0 });
        eprintln!("[fz/stats]   absorbed as steps:      {} ({:.1}%)",
            total_steps,
            if total_simplified > 0 { 100.0 * total_steps as f64 / total_simplified as f64 } else { 0.0 });
        eprintln!("[fz/stats]   kept as checks:         {} ({:.1}%)",
            total_checks,
            if total_simplified > 0 { 100.0 * total_checks as f64 / total_simplified as f64 } else { 0.0 });
        eprintln!("[fz/stats]   kept as predicates:     {} ({:.1}%)",
            total_preds,
            if total_simplified > 0 { 100.0 * total_preds as f64 / total_simplified as f64 } else { 0.0 });
        eprintln!("[fz/stats] functionizer compiled: {} of {} analyses ({:.0}%)",
            total_compiled, total_a,
            if total_a > 0 { 100.0 * total_compiled as f64 / total_a as f64 } else { 0.0 });
        eprintln!("[fz/stats] components compiled:  {} of {} ({:.0}%)",
            total_components_compiled, total_components,
            if total_components > 0 {
                100.0 * total_components_compiled as f64 / total_components as f64
            } else { 0.0 });
        eprintln!("[fz/stats] per-claim:");
        for n in &names {
            let s = &self.claims[*n];
            let extract = match s.last_extract_ok {
                Some(true)  => "z3-fz✓",
                Some(false) => "z3-fz✗",
                None        => "z3-fz·",  // didn't reach Z3 functionizer
            };
            let compiled_mark = if s.compiled > 0 { "fn✓" } else { "fn·" };
            eprintln!("[fz/stats]   {:<14} z3=[an={:>3} h={:>3} vh={:>4} sim={:>2} stp={:>2} chk={:>2} pr={:>2}] comp={}/{} {} {}",
                n, s.analyses, s.cache_hits, s.value_cache_hits, s.simplified_total, s.steps_total,
                s.checks_total, s.predicates_total,
                s.components_compiled, s.components, extract, compiled_mark);
        }
        eprintln!("[fz/stats] legend:  z3=[an=analyses h=plan-hits vh=value-cache-hits sim=simplified-assertions stp=absorbed-as-steps chk=checks pr=predicates]");
        eprintln!("[fz/stats]          z3-fz✓ = extracted a Z3Program | z3-fz✗ = extract failed | z3-fz· = never ran");
        eprintln!("[fz/stats]          comp=C/N = C of N decomposed components compiled (rest → cached scoped Z3 solve)");
        eprintln!("[fz/stats]          fn✓ = ≥1 component compiled | fn· = no component compiled (full slow-path Z3)");
    }
}
