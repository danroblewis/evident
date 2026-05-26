//! Collect the dispatchable Effects from a satisfying model.
//!
//! Two modes, picked by `primary_var`:
//!
//!   * **`effects` slot present** (the legacy / ordered shape): the
//!     `primary_var` Seq(Effect) IS the dispatch list. Only those
//!     elements dispatch, in that order. Other Effect-typed
//!     bindings (intermediate names used to BUILD `effects`, or
//!     names bound in a `match`-on-state branch the program doesn't
//!     intend to dispatch this tick) are IGNORED.
//!   * **no `effects` slot**: walk every binding in the model and
//!     dispatch any `Effect` / `Seq(Effect)` value found. Dedup by
//!     value equality (the same Effect across two Seqs runs once,
//!     in its first-encountered position).

use crate::core::ast::{Effect, BodyItem, Expr, BinOp};
use crate::runtime::EvidentRuntime;
use crate::translate::{Value, ast_decoder};
use std::collections::{HashMap, HashSet};

use super::seq_chains::extract_seq_effect_chains;
use super::toposort::{
    DISPATCH_ORDER_CACHE, DispatchKey,
    cycle_recovery, evident_toposort, resolve_synthetic_names_to_effects,
};

/// Collect the dispatchable Effects from a satisfying model.
///
/// Two modes, picked by `primary_var`:
///
///   * **`effects` slot present** (the legacy / ordered shape): the
///     `primary_var` Seq(Effect) IS the dispatch list. Only those
///     elements dispatch, in that order. Other Effect-typed
///     bindings (intermediate names used to BUILD `effects`, or
///     names bound in a `match`-on-state branch the program doesn't
///     intend to dispatch this tick) are IGNORED.
///   * **no `effects` slot**: walk every binding in the model and
///     dispatch any `Effect` / `Seq(Effect)` value found. Dedup by
///     value equality (the same Effect across two Seqs runs once,
///     in its first-encountered position).
///
/// The split exists because in idiomatic effect-driven programs,
/// users construct intermediate Effect bindings (`frame_clear`,
/// `init_eff`, …) whose values are MATERIAL even when the program
/// doesn't intend to run them this tick — Z3 always assigns SOME
/// value to a declared `Effect`-typed name. The `effects` slot
/// IS the gate that says "of all these Effect bindings, dispatch
/// these in this order." Programs without that gate are opting
/// into "every Effect in the model runs."
///
/// Ordering for the no-slot mode:
///   * Each Effect-typed binding is a *node*.
///   * Each `Seq(Effect)` binding's literal `⟨a, b, c⟩` value contributes
///     ordering *edges* (a→b, b→c) — the Seq itself isn't dispatched,
///     it just declares "a must run before b before c".
///   * Cross-Seq ordering with no declared edge is unconstrained —
///     unconstrained nodes get a randomized linearization so bugs
///     caused by accidental ordering surface naturally.
///   * Dispatch order comes from `Toposort<String>` (stdlib/toposort.ev),
///     called via `rt.query`. The runtime dogfoods its own constraint
///     primitive here. When perf becomes an issue, the future
///     model-compilation layer is the resolution path.
pub(crate) fn collect_dispatchable_effects(
    rt: &EvidentRuntime,
    claim_name: &str,
    bindings: &HashMap<String, Value>,
    primary_var: Option<&str>,
) -> Vec<Effect> {
    // Mode 1: `effects` slot present — dispatch ONLY that Seq.
    // Intermediate / off-branch Effect bindings stay in the model
    // but don't run. Preserves the legacy gate semantics.
    if let Some(pv) = primary_var {
        if let Some(Value::SeqEnum(items)) = bindings.get(pv) {
            return items.iter()
                .filter_map(|v| ast_decoder::decode_effect(v).ok())
                .collect();
        }
        // primary_var declared but no model binding — fall through
        // to the walk-everything path. (Shouldn't happen for a
        // satisfied fsm, but defensive.)
    }

    // Mode 2: collect Effect-typed nodes + Seq-literal edges, toposort,
    // dispatch in resulting order.
    //
    // Ordering is self-hosted: the order comes from the Evident
    // `Toposort<String>` claim, run on a dedicated runtime via
    // `portable::toposort` (session PORT-toposort). The dedicated runtime
    // is what makes it viable — the same dogfood call on the *user's*
    // runtime shared a Z3 context with the user FSM's solve and ran 12–16s;
    // in isolation it's ~0.2–0.4s, paid once per unique shape and then
    // cached below (`DISPATCH_ORDER_CACHE`), so per-tick steady state is a
    // HashMap lookup.
    // Nodes come from two sources:
    //   * Bare `Effect`-typed bindings — node name = binding name.
    //   * `Seq(Effect)`-typed bindings — one synthetic node per element,
    //     name = `<binding>[i]`. Adjacent elements get an implicit edge
    //     (intra-Seq order is part of the contract).
    //
    // Two kinds of `Seq(Effect)` bindings:
    //   * **Dispatch bundles** — populated by a subclaim invocation
    //     like `win.draw_rect(r, plat_0_effs)`. The SeqLit assignment
    //     lives inside the subclaim's body, not the outer schema's.
    //     The outer schema only sees the binding name; the runtime
    //     synthesizes `name[i]` nodes for each element with auto-
    //     edges (color before fill), since those nodes are the only
    //     dispatch handle.
    //   * **Ordering declarations** — written explicitly in the
    //     outer body as `name = ⟨ref1, ref2, …⟩` (e.g. `sky_effs`,
    //     `phase_chain`). The elements are references to other
    //     dispatchable bindings; their effect values are ALREADY
    //     dispatched via those references. Synthesizing nodes for
    //     them would create duplicate dispatches AND, since the
    //     same effect can legitimately appear multiple times in
    //     such a chain (e.g. when two rects share a color, the
    //     state-changing `set_color` effect must fire BEFORE EACH
    //     fill that wants that color), value-based dedup would
    //     wrongly drop the redundant set_color and let the wrong
    //     color leak through.
    //
    // So: ordering declarations contribute EDGES only, not nodes.
    // The chain-extraction walks their SeqLit and turns adjacent
    // references into ordering edges between existing nodes.
    // A `Seq(Effect)` binding has a body SeqLit when the user wrote
    // `name = ⟨…⟩` explicitly (ordering declarations like phase_chain,
    // or trivial bundles like sky_effs). Bindings WITHOUT a body
    // SeqLit are populated by subclaim invocations (e.g., the four
    // plat_X_effs from `win.draw_rect(...)` calls), where the intra-
    // bundle order is implicit and must come from auto-edges between
    // adjacent synthetic nodes.
    //
    // For ordering-declaration bindings, the chain extraction walks
    // their SeqLit and produces deduped ordering edges; auto-edges
    // would generate contradictions when the same canonical appears
    // multiple times in the chain. So we skip auto-edges for those.
    let has_body_seqlit: HashSet<&str> = match rt.get_schema(claim_name) {
        Some(schema) => schema.body.iter().filter_map(|item| match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                let lhs_name = match (lhs.as_ref(), rhs.as_ref()) {
                    (Expr::Identifier(n), Expr::SeqLit(_)) => Some(n.as_str()),
                    (Expr::SeqLit(_), Expr::Identifier(n)) => Some(n.as_str()),
                    _ => None,
                };
                lhs_name
            }
            _ => None,
        }).collect(),
        None => HashSet::new(),
    };

    let mut node_values: HashMap<String, Effect> = HashMap::new();
    let mut all_names: Vec<String> = Vec::new();
    let mut all_auto_edges: Vec<(String, String)> = Vec::new();
    for (name, v) in bindings {
        match v {
            Value::Enum { enum_name, .. } if enum_name == "Effect" => {
                if let Ok(e) = ast_decoder::decode_effect(v) {
                    node_values.insert(name.clone(), e);
                    all_names.push(name.clone());
                }
            }
            Value::SeqEnum(items) => {
                let is_effect_seq = !items.is_empty() && items.iter().all(|it|
                    matches!(it, Value::Enum { enum_name, .. } if enum_name == "Effect")
                );
                // Ordering declarations have a body SeqLit; their
                // elements are references to other dispatchable bindings,
                // not fresh dispatch handles. Skip node creation
                // entirely for these — the chain-extraction below will
                // produce the ordering edges between the referenced
                // existing nodes.
                if is_effect_seq && !has_body_seqlit.contains(name.as_str()) {
                    let mut prev: Option<String> = None;
                    for (i, item) in items.iter().enumerate() {
                        if let Ok(e) = ast_decoder::decode_effect(item) {
                            let syn = format!("{}[{}]", name, i);
                            node_values.insert(syn.clone(), e);
                            all_names.push(syn.clone());
                            if let Some(p) = prev.take() {
                                all_auto_edges.push((p, syn.clone()));
                            }
                            prev = Some(syn);
                        }
                    }
                }
            }
            // `Seq(Composite-with-Seq(Effect)-field)` — the
            // `Seq(EffectBundle)` shape from per-iteration ∀-render
            // patterns. Each outer element holds an inner Seq(Effect);
            // synthesize nodes `outer[i].field[j]` for every effect
            // element, plus intra-bundle auto-edges between adjacent
            // (i, j) and (i, j+1) within the same outer index.
            //
            // Cross-bundle order (`outer[i].field[last] → outer[i+1].
            // field[0]`) is NOT auto-emitted — that's an inter-iteration
            // ordering choice the user expresses via a phase_chain Seq.
            Value::SeqComposite(items) => {
                for (i, fields_map) in items.iter().enumerate() {
                    for (fname, fval) in fields_map {
                        let Value::SeqEnum(inner) = fval else { continue };
                        let is_effect_inner = !inner.is_empty() && inner.iter().all(|it|
                            matches!(it, Value::Enum { enum_name, .. }
                                if enum_name == "Effect")
                        );
                        if !is_effect_inner { continue; }
                        let mut prev: Option<String> = None;
                        for (j, item) in inner.iter().enumerate() {
                            if let Ok(e) = ast_decoder::decode_effect(item) {
                                let syn = format!("{}[{}].{}[{}]", name, i, fname, j);
                                node_values.insert(syn.clone(), e);
                                all_names.push(syn.clone());
                                if let Some(p) = prev.take() {
                                    all_auto_edges.push((p, syn.clone()));
                                }
                                prev = Some(syn);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if all_names.is_empty() { return Vec::new(); }

    // No value-dedup: two dispatch nodes that happen to produce the
    // same Effect value (e.g. `hat_effs[0]` and `shirt_effs[0]` both
    // emitting `set_color(red)` because hat and shirt share a color
    // literal) are SEPARATE dispatch points. The renderer's color
    // state changes between them (face_color sets tan in between),
    // so each call to set_color is load-bearing — collapsing them
    // onto a single dispatch would let the wrong color leak through.
    //
    // The original motivation for dedup was that phase_chain's
    // synthetic nodes mirrored other bundles' values. That problem
    // is gone now that ordering declarations don't synthesize nodes
    // at all (they contribute edges only). The remaining
    // value-duplicates across dispatch bundles are intentional
    // and must each fire.
    let mut nodes: Vec<String> = all_names.clone();
    let auto_edges: Vec<(String, String)> = all_auto_edges.clone();
    // Identity map (no aliasing) — kept so the chain-edge translation
    // below has a uniform lookup path even though it's currently a
    // no-op for the dispatch-bundle case.
    let alias_to_canonical: HashMap<String, String> =
        all_names.iter().map(|n| (n.clone(), n.clone())).collect();
    if std::env::var("EVIDENT_DISPATCH_TIMING").is_ok() {
        eprintln!("dispatch: {} nodes", nodes.len());
    }

    // Edge extraction from the FSM's AST: each `Seq(Effect)` literal
    // body Constraint contributes edges. Two-step process:
    //
    //   1. Walk each SeqLit, resolve every element to its canonical
    //      name, drop duplicates (keep first occurrence) — produces a
    //      canonical-dedup'd chain.
    //   2. Make pairwise edges between adjacent elements of the
    //      deduped chain.
    //
    // Why dedup inside the SeqLit rather than at edge time: the user's
    // chain visits each unique canonical at most once. If the same
    // canonical reappears later (e.g. `phase_chain` references
    // `plat_2_color_eff` which is set_color(brown) — same as
    // `plat_1_color_eff`), the later reference can't fire again —
    // canonicals dispatch once. The dedup'd chain is a linear order
    // through unique canonicals that the toposort respects without
    // any cycles.
    let alias_set: HashSet<&String> = all_names.iter().collect();
    let raw_chains = match rt.get_schema(claim_name) {
        Some(schema) => extract_seq_effect_chains(&schema.body, &alias_set),
        None => Vec::new(),
    };
    let mut edges: Vec<(String, String)> = Vec::new();
    for chain in raw_chains {
        let mut deduped: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for name in &chain {
            let canon = alias_to_canonical.get(name).cloned().unwrap_or_else(|| name.clone());
            if seen.insert(canon.clone()) {
                deduped.push(canon);
            }
        }
        for w in deduped.windows(2) {
            edges.push((w[0].clone(), w[1].clone()));
        }
    }
    edges.extend(auto_edges);

    // Random tie-break — unconstrained orderings get a fresh
    // linearization each run so accidental-ordering bugs surface.
    // Reproducible via EVIDENT_DISPATCH_SEED for debugging.
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let seed: u64 = std::env::var("EVIDENT_DISPATCH_SEED").ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64).unwrap_or(0)
        });
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    nodes.shuffle(&mut rng);

    // If no edges, randomized order is the answer.
    if edges.is_empty() {
        return resolve_synthetic_names_to_effects(&nodes, &node_values);
    }

    // Memoize: same (nodes, edges) shape → reuse the prior linearization.
    // For Mario the shape is identical every frame, so tick 0 pays the
    // toposort and every subsequent tick is a HashMap lookup.
    let mut canon_nodes = nodes.clone();
    canon_nodes.sort();
    let mut canon_edges = edges.clone();
    canon_edges.sort();
    let cache_key: DispatchKey = (canon_nodes, canon_edges);
    {
        let mut guard = DISPATCH_ORDER_CACHE.lock().unwrap();
        if let Some(map) = guard.as_ref() {
            if let Some(cached) = map.get(&cache_key) {
                return resolve_synthetic_names_to_effects(cached, &node_values);
            }
        } else {
            *guard = Some(HashMap::new());
        }
    }

    // Ordering is self-hosted (session PORT-toposort): the Evident
    // `Toposort<String>` claim, on a dedicated isolated runtime. A cyclic
    // graph is UNSAT → `None` → keep the program running via input-order
    // recovery (see `toposort::cycle_recovery`). This solve runs at most
    // once per unique shape; the cache above serves every later tick.
    let timing = std::env::var("EVIDENT_DISPATCH_TIMING").is_ok();
    let t0 = std::time::Instant::now();
    let sorted_names = evident_toposort(&nodes, &edges)
        .unwrap_or_else(|| cycle_recovery(&nodes));
    if timing {
        eprintln!("toposort[evident]: {} nodes, {} edges, {:.3}ms",
            nodes.len(), edges.len(),
            t0.elapsed().as_secs_f64() * 1000.0);
    }

    if let Ok(mut guard) = DISPATCH_ORDER_CACHE.lock() {
        if let Some(map) = guard.as_mut() {
            map.insert(cache_key, sorted_names.clone());
        }
    }

    resolve_synthetic_names_to_effects(&sorted_names, &node_values)
}
