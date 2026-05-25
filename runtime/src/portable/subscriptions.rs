//! `subscriptions` — static world-access-set inference for the multi-FSM
//! scheduler. **Sole implementation: the self-hosted Evident pass.**
//!
//! Session XX cut subscriptions over to Evident-only: the canonical Rust
//! walk (`crate::subscriptions::world_access_sets` + its `walk_*`
//! traversal) is **deleted**, and the multi-FSM scheduler now computes
//! every claim's `(reads, writes)` through [`EvidentSubscriptions`]. There
//! is no Rust-walk fallback.
//!
//! [`EvidentSubscriptions`] owns an [`EvidentRuntime`] with
//! `stdlib/passes/subscriptions.ev` loaded. The WHOLE walk runs in Evident
//! as an FSM-with-stack (`subscriptions_walk`): this shim only marshals the
//! claim body into a `Value` via the SHARED marshaler
//! ([`crate::translate::ast_encoder::body_item_to_value`]), drives the FSM
//! to a drained-stack halt via [`crate::effect_loop::run_nested`], and
//! classifies the reachable identifiers. **No Rust-side tree walk, no
//! bespoke encoder** — the recursion and accumulation live in the pass.
//!
//! ## What stays in Rust, and why
//!
//! The FSM owns the traversal and the accumulation, but NOT the
//! `world.`/`world_next.` classification: that needs `strip_prefix` /
//! `first_segment`, and Evident has no substring/prefix operator. So the
//! FSM emits the RAW dotted identifier strings it reaches and [`classify`]
//! does the prefix split here — a few unavoidable lines.
//!
//! ## The scheduler entry point
//!
//! Production code calls the free [`access_sets`] function, which holds a
//! per-thread lazily-built [`EvidentSubscriptions`] engine: the pass is
//! loaded and JIT-cached once per thread, then reused for every claim. The
//! engine locates `stdlib/` via the one [`crate::stdlib_path::stdlib_dir`]
//! resolver (session WW), so a relocated/installed binary finds the pass
//! without a CWD assumption.
//!
//! ## No bootstrap cycle
//!
//! Computing subscriptions for the user's FSMs runs `subscriptions_walk`
//! via [`crate::effect_loop::run_nested`] — the tier-3 blocking
//! interpreter. `run_nested` drives a single FSM with per-tick Z3 solves
//! (`query_with_pins_and_given`); it **never** calls `access_sets` or any
//! scheduler-level subscription inference. And `subscriptions_walk` itself
//! reads no `world.X` (its state is `SW`, a plain stack machine), so even
//! its own access-set is empty. The recursion therefore terminates: the
//! pass that computes subscriptions does not itself need subscriptions.
//! See `runtime/tests/subscriptions_correctness.rs::bootstrap_*`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::core::ast::SchemaDecl;
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::subscriptions::AccessSets;
use crate::translate::ast_decoder::{decode_list, decode_str};
use crate::translate::ast_encoder::body_item_to_value;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `subscriptions`' Rust-level signature, independent of which impl backs
/// it. Kept for uniformity with the rest of the [`super`] swap-interface
/// family (`pretty`, `validate`); subscriptions now has a single impl.
pub trait SubscriptionsImpl: Portable {
    /// Walk one claim and collect its world access sets.
    fn access_sets(&self, claim: &SchemaDecl) -> AccessSets;
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — runs stdlib/passes/subscriptions.ev as a stack-FSM
// ─────────────────────────────────────────────────────────────────────

/// Runs the analysis by encoding the claim body with the shared marshaler
/// and driving the `subscriptions_walk` FSM to halt. Holds its own runtime
/// with the pass loaded; build once and reuse so the FSM's per-tick solve
/// is JIT-cached across calls.
pub struct EvidentSubscriptions {
    rt: EvidentRuntime,
}

impl EvidentSubscriptions {
    /// The whole-walk FSM in `stdlib/passes/subscriptions.ev`.
    const WALK_FSM: &'static str = "subscriptions_walk";

    /// Max-iteration guard for the nested walk. One AST node costs a
    /// small constant number of FSM ticks, so a body of N nodes halts in
    /// O(N) ticks; the cap is set far above any realistic claim so a
    /// legitimate walk never hits it (a non-terminating walk would be a
    /// pass bug, surfaced as a loud `MaxItersExceeded`).
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/subscriptions.ev` from `stdlib_dir` into a fresh
    /// runtime. `stdlib_dir` is the repo's `stdlib/` directory. The pass
    /// is self-contained (it declares its own cons-list copy of the AST
    /// enums matching the shared marshaler), so no other stdlib file is
    /// needed.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("subscriptions.ev"))
            .map_err(|e| format!("load passes/subscriptions.ev: {e}"))?;
        Ok(Self { rt })
    }
}

impl Portable for EvidentSubscriptions {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl SubscriptionsImpl for EvidentSubscriptions {
    fn access_sets(&self, claim: &SchemaDecl) -> AccessSets {
        let mut sets = AccessSets::default();
        // Drive the walk-FSM once per top-level body item. Each run's
        // stack is one item's subtree, so the per-tick state marshaled
        // through `run_nested` stays small — the difference between an
        // O(N) and an O(N²) total marshaling cost on a big claim like
        // Mario's `game`. This is a flat driver over the top-level items
        // (NOT a tree walk): every recursion — into sub-expressions AND
        // into subclaim bodies — happens inside the FSM. reads/writes is
        // a set union over items, so per-item-then-union is byte-identical
        // to walking the whole body in one pass.
        for item in &claim.body {
            // Shared marshaler: ast.rs BodyItem → Value::Enum tree (the
            // canonical cons-list shape). Wrapped as the FSM's unified
            // walk node `Work::WBody(BodyItem)` — `run_nested`'s coerce
            // seeds it into `SWSeed(Work)`.
            let seed = work_node("WBody", body_item_to_value(item));
            self.walk_item(&seed, &claim.name, &mut sets);
        }
        sets
    }
}

impl EvidentSubscriptions {
    /// Drive `subscriptions_walk` over one seeded `Work` node and fold the
    /// reachable identifiers it returns into `sets` (classified by their
    /// `world.`/`world_next.` prefix). The FSM returns `SWDone(NameList)` —
    /// a cons-list of RAW dotted identifier strings.
    fn walk_item(&self, seed: &Value, claim_name: &str, sets: &mut AccessSets) {
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed.clone(), Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "SWDone" && fields.len() == 1 => {
                // Shared cons-list decoder: NameList → Vec<String>.
                match decode_list(&fields[0], "NameList", "NameNil", "NameCons", decode_str) {
                    Ok(names) => for name in names { classify(&name, sets); },
                    Err(e) => eprintln!("[subscriptions/evident] decode of `{claim_name}` \
                        result failed: {e}"),
                }
            }
            Ok(other) => eprintln!("[subscriptions/evident] walk of `{claim_name}` \
                returned a non-SWDone state: {other:?}"),
            Err(e) => eprintln!("[subscriptions/evident] walk of `{claim_name}` failed: {e}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point — a per-thread cached engine
// ─────────────────────────────────────────────────────────────────────

thread_local! {
    /// One [`EvidentSubscriptions`] engine per thread, built lazily on the
    /// first [`access_sets`] call. `EvidentRuntime` is `!Send`/`!Sync`
    /// (Z3 context, Cranelift module, `Rc`/`RefCell` interior), so a
    /// thread-local — not a global — is the right cache: the scheduler runs
    /// single-threaded, so it pays the pass-load + JIT-compile cost exactly
    /// once.
    static ENGINE: RefCell<Option<Rc<EvidentSubscriptions>>> = const { RefCell::new(None) };
}

/// World access sets for one claim, computed by the self-hosted Evident
/// `subscriptions_walk` pass. **This is the runtime's sole subscriptions
/// entry point** — the scheduler ([`crate::effect_loop`]) calls it to wake
/// FSMs on read-set deltas and to scope multi-writer snapshots.
///
/// Builds and caches a per-thread [`EvidentSubscriptions`] engine on first
/// use (see [`ENGINE`]). The engine locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver.
///
/// # Panics
///
/// If `stdlib/passes/subscriptions.ev` cannot be located or loaded. There
/// is no Rust-walk fallback (session XX), so an unloadable pass is a hard
/// error — the same robust resolution the rest of the runtime relies on
/// (session WW). The error names every checked path and the
/// `EVIDENT_STDLIB` override.
pub fn access_sets(claim: &SchemaDecl) -> AccessSets {
    let engine = ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(build_engine()));
        }
        // Clone the Rc out so the thread-local borrow is released before
        // we run the walk (`access_sets` → `run_nested` does not re-enter
        // this thread-local, but releasing keeps the invariant obvious).
        slot.as_ref().unwrap().clone()
    });
    engine.access_sets(claim)
}

/// Locate `stdlib/` and load the subscriptions pass into a fresh engine.
/// Panics with the resolver's path-list diagnostic on failure — see
/// [`access_sets`].
fn build_engine() -> EvidentSubscriptions {
    let dir = crate::stdlib_path::stdlib_dir().unwrap_or_else(|e| panic!(
        "subscriptions: cannot locate stdlib to load the subscriptions \
         pass (the sole impl since session XX): {e}"));
    EvidentSubscriptions::new(&dir).unwrap_or_else(|e| panic!(
        "subscriptions: failed to load passes/subscriptions.ev from {}: {e}",
        dir.display()))
}

// ─────────────────────────────────────────────────────────────────────
// Classification — the one piece Evident can't express (no substring op)
// ─────────────────────────────────────────────────────────────────────

/// Classify one raw dotted identifier into the read/write sets: a
/// `world_next.X…` access writes the top-level field `X`, a `world.X…`
/// access reads it, anything else (a bare local, a non-world name)
/// contributes nothing. Duplicates collapse into the `HashSet`s.
fn classify(name: &str, sets: &mut AccessSets) {
    if let Some(field) = name.strip_prefix("world_next.") {
        sets.writes.insert(first_segment(field).to_string());
    } else if let Some(field) = name.strip_prefix("world.") {
        sets.reads.insert(first_segment(field).to_string());
    }
}

/// First dotted segment of `s` (`player.pos.x` → `player`). Conservative
/// top-level-field attribution.
fn first_segment(s: &str) -> &str {
    s.split('.').next().unwrap_or(s)
}

/// Wrap an already-marshaled AST `Value` as the FSM's unified `Work` node.
fn work_node(variant: &str, inner: Value) -> Value {
    Value::Enum {
        enum_name: "Work".to_string(),
        variant: variant.to_string(),
        fields: vec![inner],
    }
}
