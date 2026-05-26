//! `toposort` — effect-dispatch ordering. **Sole implementation: the
//! self-hosted Evident `Toposort<String>` claim.**
//!
//! Session PORT-toposort cut effect-ordering over to Evident-only: the
//! Rust `topo_sort_with_random_tiebreak` (Kahn's algorithm with a
//! randomized ready-frontier, in `runtime/src/effect_loop/toposort.rs`) is
//! **deleted**, and the dispatcher orders a tick's Effects through
//! [`EvidentToposort`]. There is no Rust-algorithm fallback. The Evident
//! path already existed behind `EVIDENT_TOPOSORT_IMPL=evident`; this
//! promotes it to the default and removes the env gate.
//!
//! [`EvidentToposort`] owns a **dedicated** [`EvidentRuntime`] with only
//! `stdlib/toposort.ev` loaded. The dedicated context is load-bearing: it
//! isolates the toposort solve from the user FSM's own complex solve (which,
//! shared in one Z3 context, ran 12–16s — see the abandonment note in the
//! old `collect.rs`).
//!
//! ## The integer-rank encoding (why it's fast)
//!
//! The dispatcher's node names are arbitrary identities — the toposort only
//! needs to tell vertices apart, not read their strings. So this shim maps
//! the node names to the contiguous integers `0..n-1` and queries the
//! `ToposortRanks` claim, whose edges index a rank array DIRECTLY
//! (`pos[e.from] < pos[e.to]`): one array select per endpoint, O(1) per
//! edge. The generic domain-typed `Toposort<T>` is correct but uses
//! `position_of` — a depth-n chained ITE evaluated twice per edge — which is
//! O(n·#edges) Z3 terms and took *13–42s* on a Mario-scale graph (70 nodes,
//! 96 edges). `ToposortRanks` solves the same graph in **~19ms**. The
//! string↔int mapping and the rank→order inversion are pure Rust leaf
//! marshaling ("Evident owns the algorithm, Rust owns the leaves"); the
//! integers never escape this module, so the runtime's effect-ordering API
//! stays node-identity in / node-identity out.
//!
//! ## Why this is setup-only, not a per-tick cost
//!
//! The dispatcher memoizes orderings by `(sorted nodes, sorted edges)` shape
//! (`DISPATCH_ORDER_CACHE` in `effect_loop/toposort.rs`). A program's effect
//! graph is shape-stable across ticks — Mario emits the identical effect set
//! every frame — so tick 0 pays the one Evident solve and every later tick
//! is a `HashMap` lookup that never reaches this module. Per-tick steady
//! state is unchanged by the cutover; the move is a one-time tick-0 cost
//! (engine build + the single solve), in line with the AOT-over-runtime
//! priority.
//!
//! ## What stays in Rust, and why
//!
//! One leaf: **cycle recovery**. A cyclic dependency graph has no
//! topological order, so the solve is UNSAT and [`toposort`] returns `None`.
//! The dispatcher's policy on a cycle is "keep the program running by
//! dispatching the nodes in input order and warning on stderr" (so a bad
//! user-declared `Seq(Effect)` ordering doesn't silently halt the program).
//! That recovery is a Rust policy decision about what to do when no ordering
//! exists — not part of the sort algorithm — so it stays in the caller
//! (`effect_loop::toposort::cycle_recovery`). The acyclic case (the only one
//! that produces an ordering) is fully self-hosted.
//!
//! ## No bootstrap guard needed
//!
//! Unlike `validate` / `desugar` / `subscriptions` — which run *on the load
//! path* and so re-enter themselves while their engine loads its pass file —
//! toposort runs only on the *dispatch* path. Building the engine loads
//! `stdlib/toposort.ev` (and runs the usual load passes over it), but nothing
//! in the load path calls back into effect-dispatch ordering, so there is no
//! re-entrancy to guard against.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::core::Value;
use crate::runtime::EvidentRuntime;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `toposort`'s Rust-level signature, independent of which impl backs it.
/// Kept for uniformity with the rest of the [`super`] swap-interface family
/// (`pretty`, `validate`, `subscriptions`); toposort now has a single impl.
pub trait ToposortImpl: Portable {
    /// Order `nodes` so every `(from, to)` edge has `from` earlier than
    /// `to`. `Some(ordering)` on an acyclic graph; `None` if the graph has
    /// a cycle (UNSAT) or the result can't be decoded.
    fn toposort(&self, nodes: &[String], edges: &[(String, String)]) -> Option<Vec<String>>;
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — queries the stdlib `Toposort<String>` claim
// ─────────────────────────────────────────────────────────────────────

/// Pass-driven dispatch ordering. Holds a dedicated [`EvidentRuntime`] with
/// `stdlib/toposort.ev` loaded; build once and reuse so the claim's
/// translation/JIT is cached across ticks (and so the dedicated Z3 context
/// stays isolated from the user FSM's solve).
pub struct EvidentToposort {
    rt: EvidentRuntime,
}

impl EvidentToposort {
    /// The runtime-internal integer-rank toposort. `stdlib/toposort.ev`
    /// declares it (and `sat_ranks_*` tests anchor its monomorphic deps).
    const CLAIM: &'static str = "ToposortRanks";

    /// Load `toposort.ev` from `stdlib_dir` into a fresh runtime.
    /// `stdlib_dir` is the repo's `stdlib/` directory. `toposort.ev` pulls
    /// in its `combinatorics.ev` / `permutation.ev` transitive imports
    /// itself.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("toposort.ev"))
            .map_err(|e| format!("load toposort.ev: {e}"))?;
        Ok(Self { rt })
    }
}

impl Portable for EvidentToposort {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl ToposortImpl for EvidentToposort {
    fn toposort(&self, nodes: &[String], edges: &[(String, String)]) -> Option<Vec<String>> {
        let n = nodes.len();
        if n == 0 { return Some(Vec::new()); }

        // Leaf marshaling: map each node name to its contiguous index. Node
        // names are unique by construction (binding names + `name[i]`
        // synthetics), so position IS a stable identity.
        let idx: HashMap<&str, i64> = nodes.iter().enumerate()
            .map(|(i, name)| (name.as_str(), i as i64)).collect();

        // Edges become Edge<Int> over the node indices. Defensively skip an
        // edge whose endpoint isn't a known node (the dispatcher only emits
        // intra-node edges, but the old Rust path skipped strays too).
        let edge_vals: Vec<HashMap<String, Value>> = edges.iter()
            .filter_map(|(f, t)| {
                let (fi, ti) = (*idx.get(f.as_str())?, *idx.get(t.as_str())?);
                let mut m = HashMap::new();
                m.insert("from".into(), Value::Int(fi));
                m.insert("to".into(),   Value::Int(ti));
                Some(m)
            }).collect();

        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("n".into(),     Value::Int(n as i64));
        given.insert("edges".into(), Value::SeqComposite(edge_vals));

        let r = self.rt.query(Self::CLAIM, &given).ok()?;
        if !r.satisfied { return None; }     // cyclic graph → UNSAT
        let Some(Value::SeqInt(pos)) = r.bindings.get("pos") else { return None };
        if pos.len() != n { return None; }

        // `pos[k]` is node k's output rank; invert to an ordering by sorting
        // the node indices on ascending rank. (Distinct ranks ⇒ stable.)
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&k| pos[k]);
        Some(order.into_iter().map(|k| nodes[k].clone()).collect())
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point — a per-thread cached engine
// ─────────────────────────────────────────────────────────────────────

thread_local! {
    /// One [`EvidentToposort`] engine per thread, built lazily on the first
    /// [`toposort`] call. `EvidentRuntime` is `!Send`/`!Sync` (Z3 context,
    /// Cranelift module, `Rc`/`RefCell` interior), so a thread-local — not a
    /// global — is the right cache: the scheduler runs single-threaded, so it
    /// pays the pass-load cost exactly once and isolates the toposort solve
    /// in its own Z3 context.
    static ENGINE: RefCell<Option<Rc<EvidentToposort>>> = const { RefCell::new(None) };
}

/// Order a tick's dispatchable Effect nodes, computed by the self-hosted
/// Evident `Toposort<String>` claim. **This is the runtime's sole
/// effect-ordering entry point** — `effect_loop::collect` calls it once per
/// unique effect-graph shape (cached thereafter).
///
/// Returns `Some(ordering)` on an acyclic graph; `None` if the declared
/// ordering edges form a cycle (UNSAT). The caller recovers from `None` via
/// `effect_loop::toposort::cycle_recovery` — see the module docs.
///
/// Builds and caches a per-thread [`EvidentToposort`] engine on first use
/// (see [`ENGINE`]). The engine locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver (session WW).
///
/// # Panics
///
/// If `stdlib/toposort.ev` cannot be located or loaded. There is no
/// Rust-algorithm fallback (session PORT-toposort), so an unloadable pass is
/// a hard error — the same robust resolution the rest of the runtime relies
/// on (session WW). The error names every checked path and the
/// `EVIDENT_STDLIB` override.
pub fn toposort(nodes: &[String], edges: &[(String, String)]) -> Option<Vec<String>> {
    let engine = ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(build_engine()));
        }
        slot.as_ref().unwrap().clone()
    });
    engine.toposort(nodes, edges)
}

/// Locate `stdlib/` and load the toposort claim into a fresh engine. Panics
/// with the resolver's path-list diagnostic on failure — see [`toposort`].
fn build_engine() -> EvidentToposort {
    let dir = crate::stdlib_path::stdlib_dir().unwrap_or_else(|e| panic!(
        "toposort: cannot locate stdlib to load the toposort claim \
         (the sole impl since session PORT-toposort): {e}"));
    EvidentToposort::new(&dir).unwrap_or_else(|e| panic!(
        "toposort: failed to load toposort.ev from {}: {e}",
        dir.display()))
}
