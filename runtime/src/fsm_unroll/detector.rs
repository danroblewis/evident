//! Affine-step detector — gate the log-unroll technique behind a quick
//! probe that classifies the FSM body as affine (composition collapses
//! to closed form) or branching (composition grows ~2× per doubling).
//!
//! Z's measurement (`docs/perf/log-unroll-feasibility.md`) found the
//! two regimes separate cleanly, but **only after 3 doublings** — one
//! doubling is not enough:
//!
//! | shape           | f1 | f2 | f4 | f8 | f2/f1 | f4/f2 | f8/f4 |
//! |-----------------|----|----|----|----|-------|-------|-------|
//! | pure counter    |  3 |  3 |  3 |  3 | 1.00  | 1.00  | 1.00  |
//! | linear recur.   |  5 |  5 |  5 |  5 | 1.00  | 1.00  | 1.00  |
//! | Fibonacci       |  3 |  6 | 11 | 11 | 2.00  | 1.83  | 1.00  |
//! | conditional upd |  6 |  9 | 15 | 27 | 1.50  | 1.67  | 1.80  |
//! | 3-state machine |  9 | 16 | 33 | 69 | 1.78  | 2.06  | 2.09  |
//!
//! Reading the f2/f1 column alone misclassifies both ways: it puts
//! Fibonacci (affine, 2.00) in the branching bucket and the conditional
//! update (branching, exactly 1.50) in the affine bucket. The
//! **last-doubling ratio after probing to F^8** is the clean
//! discriminant — affine shapes have collapsed to 1.00 by then while
//! branching shapes stay ≥ 1.67. That's why the composer probes 3
//! doublings before deciding. Threshold 1.5 sits in the gap.

use std::collections::HashSet;

use z3::ast::{Ast, Bool, Dynamic};

/// Count unique AST nodes across `assertions`. Z3 ASTs are hash-consed
/// DAGs (the `z3` crate's `Hash`/`Eq` are keyed on `Z3_get_ast_id`), so
/// a recursive walk that dedups by node identity counts shared subterms
/// once — the honest "how big is this term collection".
///
/// This is the same metric as Z's `count_nodes` in
/// `runtime/tests/log_unroll_measurement.rs`, replicated here so the
/// runtime can decide without test-time machinery.
pub(super) fn count_nodes<'ctx>(assertions: &[Bool<'ctx>]) -> usize {
    let mut seen: HashSet<Dynamic<'ctx>> = HashSet::new();
    let mut stack: Vec<Dynamic<'ctx>> = assertions
        .iter()
        .map(|b| Dynamic::from_ast(b))
        .collect();
    while let Some(d) = stack.pop() {
        if seen.insert(d.clone()) {
            for c in d.children() {
                stack.push(c);
            }
        }
    }
    seen.len()
}

/// Classifier verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    /// Ratio ≤ 1.5 — body collapses under composition. Proceed.
    Affine,
    /// Ratio > 1.5 — body grows ~linearly under composition. Refuse.
    Branching,
}

/// The number of doublings the composer probes before classifying.
/// 3 doublings reach F^8 — the depth at which Z's affine shapes have
/// all collapsed to ratio 1.0 (Fibonacci only reveals itself here) and
/// the branching shapes are still ≥ 1.67. See the table in the module
/// doc.
pub(super) const PROBE_DOUBLINGS: u32 = 3;

/// The composer probes up to this power (2^PROBE_DOUBLINGS) before
/// deciding, capped at the largest power ≤ N.
pub(super) const PROBE_POWER: u64 = 1 << PROBE_DOUBLINGS; // F^8

/// The affine/branching boundary on the last-doubling node-count ratio.
/// Z's regimes are affine ≤ 1.0 / branching ≥ 1.67 by F^8, so the
/// precise value in (1.0, 1.67) doesn't matter; 1.5 is the midpoint.
pub(super) const RATIO_THRESHOLD: f64 = 1.5;

/// Classify on the most recent doubling's node-count ratio (the
/// `last_nodes / prev_nodes` of the F^{2k}/F^k step that reached the
/// probe depth). Called once, after the composer has probed
/// [`PROBE_POWER`] doublings.
pub(super) fn classify(last_ratio: f64) -> Verdict {
    if last_ratio <= RATIO_THRESHOLD { Verdict::Affine } else { Verdict::Branching }
}
