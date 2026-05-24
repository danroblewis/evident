//! Dispatch ordering — Kahn's algorithm with a randomized
//! ready-frontier, plus the dogfood `Toposort<String>` path.
//!
//! The Rust implementation is the default; setting
//! `EVIDENT_TOPOSORT_IMPL=evident` routes through the stdlib
//! claim instead (slow alongside complex user solves — see
//! `collect.rs` comment).
//!
//! Memoization: `DISPATCH_ORDER_CACHE` keyed on the canonical
//! (sorted nodes, sorted edges) input. Mario's effect set is
//! identical every frame, so tick 0 pays the toposort and every
//! subsequent tick hits the cache.

use crate::ast::Effect;
use crate::runtime::EvidentRuntime;
use crate::translate::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-wide memo of dispatch orderings keyed on the canonical
/// (sorted nodes, sorted edges) input. After the first solve for a
/// given shape, subsequent ticks are a HashMap lookup — same idea as
/// "compile the constraint model" but on a smaller scale. Mario's
/// effect set is identical every frame, so after tick 0 the cache hits.
pub(super) static DISPATCH_ORDER_CACHE: Mutex<Option<HashMap<DispatchKey, Vec<String>>>>
    = Mutex::new(None);

pub(super) type DispatchKey = (Vec<String>, Vec<(String, String)>);

/// Self-hosted dispatch ordering: call the stdlib `Toposort<String>`
/// claim via `rt.query`. The runtime's own constraint primitive
/// solves its own dispatch ordering — the dogfood path.
///
/// Returns `None` on UNSAT or if the schema isn't loaded (caller
/// falls back to the Rust path). Selected by
/// `EVIDENT_TOPOSORT_IMPL=evident`.
pub(super) fn evident_toposort(
    rt: &EvidentRuntime,
    nodes: &[String],
    edges: &[(String, String)],
) -> Option<Vec<String>> {
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("items".into(), Value::SetStr(nodes.to_vec()));
    let edge_vals: Vec<HashMap<String, Value>> = edges.iter().map(|(f, t)| {
        let mut m = HashMap::new();
        m.insert("from".into(), Value::Str(f.clone()));
        m.insert("to".into(),   Value::Str(t.clone()));
        m
    }).collect();
    given.insert("edges".into(), Value::SeqComposite(edge_vals));

    let r = rt.query("Toposort<String>", &given).ok()?;
    if !r.satisfied { return None; }
    match r.bindings.get("sorted") {
        Some(Value::SeqStr(v)) => Some(v.clone()),
        _ => None,
    }
}

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

/// Kahn's algorithm with a randomized "ready" frontier — each step
/// picks uniformly at random from nodes whose dependencies have all
/// been emitted. Both the initial ready set and ties at each step
/// shuffle, so two runs of the same input produce different valid
/// linearizations (the bug-surfacing property).
///
/// Cycles are reported on stderr and the not-yet-emitted nodes get
/// appended in arbitrary order — keeps the program running rather
/// than silently halting on bad user-declared edges.
pub(super) fn topo_sort_with_random_tiebreak(
    nodes: &[String],
    edges: &[(String, String)],
    rng: &mut rand::rngs::StdRng,
) -> Vec<String> {
    use rand::seq::SliceRandom;
    use std::collections::HashSet;

    let mut in_degree: HashMap<&str, usize> = nodes.iter()
        .map(|n| (n.as_str(), 0))
        .collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in edges {
        // Skip edges referencing names not in the node set — the
        // caller already filtered, this is just defensive.
        if !in_degree.contains_key(to.as_str()) { continue; }
        if !in_degree.contains_key(from.as_str()) { continue; }
        adj.entry(from.as_str()).or_default().push(to.as_str());
        *in_degree.get_mut(to.as_str()).unwrap() += 1;
    }

    let mut ready: Vec<&str> = in_degree.iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&n, _)| n)
        .collect();
    ready.shuffle(rng);

    let mut out: Vec<String> = Vec::with_capacity(nodes.len());
    while let Some(_) = ready.first() {
        // Pop a random ready node (swap_remove on the last after shuffle
        // gives uniform sampling without resorting each iteration).
        ready.shuffle(rng);
        let n = ready.pop().unwrap();
        out.push(n.to_string());
        if let Some(succs) = adj.get(n) {
            for &m in succs {
                let d = in_degree.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 { ready.push(m); }
            }
        }
    }

    if out.len() < nodes.len() {
        // Cycle: dump the remaining nodes in arbitrary order so the
        // program keeps running. The user can spot the cycle by
        // inspecting which effects fired late.
        eprintln!("warning: cycle in declared Effect ordering edges — \
                   {} of {} nodes emitted before stall; remaining nodes \
                   appended in input order",
                  out.len(), nodes.len());
        let emitted: HashSet<String> = out.iter().cloned().collect();
        for n in nodes {
            if !emitted.contains(n) {
                out.push(n.clone());
            }
        }
    }

    out
}
