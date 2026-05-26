//! Per-tick effect-dispatch ordering via the self-hosted Evident `Toposort<String>` claim.
//! Results are memoized by (sorted nodes, sorted edges) — shape-stable programs pay one solve.

use crate::core::ast::Effect;
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-wide memo keyed on (sorted nodes, sorted edges); hit on tick 1+ for stable shapes.
pub(super) static DISPATCH_ORDER_CACHE: Mutex<Option<HashMap<DispatchKey, Vec<String>>>>
    = Mutex::new(None);

pub(super) type DispatchKey = (Vec<String>, Vec<(String, String)>);

/// Map sorted node names back to dispatchable Effect values.
pub(super) fn resolve_synthetic_names_to_effects(
    names: &[String],
    node_values: &HashMap<String, Effect>,
) -> Vec<Effect> {
    names.iter()
        .filter_map(|n| node_values.get(n).cloned())
        .collect()
}

/// Cycle recovery: when toposort is UNSAT (cyclic edges), dispatch in input order and warn.
/// This is a policy decision (keep running vs. halt), not part of the sort algorithm.
pub(super) fn cycle_recovery(nodes: &[String]) -> Vec<String> {
    eprintln!("warning: cycle in declared Effect ordering edges — \
               no topological order exists ({} nodes); dispatching in \
               input order so the program keeps running", nodes.len());
    nodes.to_vec()
}

/// Order `nodes` respecting `edges` via the self-hosted Evident `ToposortRanks` claim.
/// Returns None on a cyclic graph; caller recovers via `cycle_recovery`.
pub(super) fn evident_toposort(
    nodes: &[String],
    edges: &[(String, String)],
) -> Option<Vec<String>> {
    crate::portable::toposort::toposort(nodes, edges)
}
