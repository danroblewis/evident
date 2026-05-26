//! Affine-step detector: probes F^8; ratio ≤ 1.5 → affine (log-unroll collapses), > 1.5 → branching (refuses).
//! Needs F^8: Fibonacci misclassifies at F^2 (ratio 2.0) but collapses by F^8/F^4 (ratio 1.0).

use std::collections::HashSet;

use z3::ast::{Ast, Bool, Dynamic};

/// Unique AST node count across `assertions` (Z3 ASTs are hash-consed DAGs).
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    Affine,    // ratio ≤ 1.5 — collapses
    Branching, // ratio > 1.5 — refuses
}

pub(super) const PROBE_DOUBLINGS: u32 = 3;
pub(super) const PROBE_POWER: u64 = 1 << PROBE_DOUBLINGS; // F^8
// Affine ≤ 1.0, branching ≥ 1.67 by F^8; 1.5 sits in the gap.
pub(super) const RATIO_THRESHOLD: f64 = 1.5;

pub(super) fn classify(last_ratio: f64) -> Verdict {
    if last_ratio <= RATIO_THRESHOLD { Verdict::Affine } else { Verdict::Branching }
}
