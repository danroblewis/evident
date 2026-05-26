//! Dispatch ordering — the per-tick effect-graph cache + the leaf
//! marshaling around the self-hosted Evident toposort.
//!
//! **The ordering algorithm is self-hosted.** Effect-dispatch ordering
//! routes solely through the Evident `Toposort<String>` claim, via
//! [`crate::portable::toposort`] (session PORT-toposort). The previous Rust
//! Kahn's-algorithm-with-randomized-tiebreak — and the
//! `EVIDENT_TOPOSORT_IMPL` env gate that selected it — are **deleted**. This
//! module keeps only the pieces the dispatcher owns: the per-tick
//! shape-cache, the node-name → Effect marshaling, and the cycle-recovery
//! policy (what to do when the declared edges have no topological order).
//!
//! Memoization: `DISPATCH_ORDER_CACHE` keyed on the canonical
//! (sorted nodes, sorted edges) input. A program's effect set is shape-stable
//! across ticks (Mario's is identical every frame), so tick 0 pays the one
//! Evident solve and every subsequent tick hits this cache — the cutover is
//! setup-only, not a per-tick cost. See `portable::toposort`.

use crate::core::ast::Effect;
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-wide memo of dispatch orderings keyed on the canonical
/// (sorted nodes, sorted edges) input. After the first solve for a
/// given shape, subsequent ticks are a HashMap lookup — same idea as
/// "compile the constraint model" but on a smaller scale. Mario's
/// effect set is identical every frame, so after tick 0 the cache hits
/// and the Evident toposort solve never runs again.
pub(super) static DISPATCH_ORDER_CACHE: Mutex<Option<HashMap<DispatchKey, Vec<String>>>>
    = Mutex::new(None);

pub(super) type DispatchKey = (Vec<String>, Vec<(String, String)>);

/// Map sorted node names (real binding names + `name[i]` synthetic
/// names for Seq(Effect) elements) back to dispatchable Effect values.
pub(super) fn resolve_synthetic_names_to_effects(
    names: &[String],
    node_values: &HashMap<String, Effect>,
) -> Vec<Effect> {
    names.iter()
        .filter_map(|n| node_values.get(n).cloned())
        .collect()
}

/// Cycle recovery — the one piece that stays Rust, and why.
///
/// A cyclic dependency graph has no topological order, so the Evident
/// `ToposortRanks` solve is UNSAT and [`crate::portable::toposort`]
/// returns `None`. The dispatcher's policy is to keep the program running
/// rather than halt on a bad user-declared `Seq(Effect)` ordering: dispatch
/// the nodes in input order and warn on stderr so the user can spot the
/// cycle by inspecting which effects fired. This is a Rust policy decision
/// about what to do when no ordering *exists* — not part of the sort
/// algorithm — so it lives here, off the self-hosted walk's output.
pub(super) fn cycle_recovery(nodes: &[String]) -> Vec<String> {
    eprintln!("warning: cycle in declared Effect ordering edges — \
               no topological order exists ({} nodes); dispatching in \
               input order so the program keeps running", nodes.len());
    nodes.to_vec()
}

/// Self-hosted dispatch ordering: order `nodes` so every `(from, to)` edge
/// has `from` earlier than `to`, via the Evident `ToposortRanks` claim.
/// `None` on a cyclic graph (the caller recovers via [`cycle_recovery`]).
/// Thin re-export so call sites keep the `super::toposort::` path; the engine
/// lives in [`crate::portable::toposort`].
pub(super) fn evident_toposort(
    nodes: &[String],
    edges: &[(String, String)],
) -> Option<Vec<String>> {
    crate::portable::toposort::toposort(nodes, edges)
}
