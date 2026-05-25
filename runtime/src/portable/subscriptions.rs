//! `subscriptions` — static world-access-set inference for the
//! multi-FSM scheduler, ported into the [`super`] swap interface.
//!
//! See [`crate::subscriptions`] for what the analysis computes (read/write
//! sets per claim) and where its output drives the runtime
//! ([`crate::effect_loop`] scheduler subscriptions). Two impls:
//!   * [`RustSubscriptions`] — wraps the canonical
//!     [`crate::subscriptions::world_access_sets`] directly. The default,
//!     the oracle.
//!   * [`EvidentSubscriptions`] — owns an [`EvidentRuntime`] with
//!     `stdlib/passes/subscriptions.ev` loaded. The WHOLE walk runs in
//!     Evident as an FSM-with-stack (`subscriptions_walk`): this shim only
//!     marshals the claim body into a `Value` via the SHARED marshaler,
//!     drives the FSM to a drained-stack halt via
//!     [`crate::effect_loop::run_nested`], and classifies the reachable
//!     identifiers. **No Rust-side tree walk, no bespoke encoder** — the
//!     recursion and accumulation live in the pass.
//!
//! ## Session UU: the shared marshaler retrofit
//!
//! QQ proved that self-hosting the walk did NOT shrink the runtime,
//! because this shim hand-rolled a bespoke `AST → WNode` encoder — itself
//! a recursive AST traversal isomorphic to the walk it replaced, so the
//! marshaling tax was re-paid per port. UU deletes that encoder. The shim
//! now feeds the FSM the output of the ONE shared marshaler
//! ([`crate::translate::ast_encoder::body_item_to_value`], the `*_to_value`
//! family): the FSM walks the FULL canonical AST directly (the same
//! `Expr`/`BodyItem`/`Pins`/… shapes `stdlib/ast.ev` defines, list fields
//! as poppable Cons enums). A future port reuses the same marshaler — no
//! new encoder — so the *marginal* port is `+Evident pass, −Rust walk,
//! +~3 lines` (encode → [`run_nested`] → decode).
//!
//! ### What stays in Rust, and why
//!
//! The FSM owns the traversal and the accumulation, but NOT the
//! `world.`/`world_next.` classification: that needs `strip_prefix` /
//! `first_segment`, and Evident has no substring/prefix operator. So the
//! FSM emits the RAW dotted identifier strings it reaches and
//! [`classify`] does the prefix split here — a few unavoidable lines,
//! mirroring the canonical [`crate::subscriptions`] `walk_expr` leaf
//! logic 1:1. (QQ kept classification in the FSM only because its bespoke
//! encoder pre-split identifiers into segments — exactly the per-pass
//! encoder UU removes.)
//!
//! ## Faithful equivalence
//!
//! Both impls produce byte-identical `AccessSets` (HashSet<String>
//! equality) on every FSM-shaped claim across the demo corpus including
//! Mario — see `runtime/tests/subscriptions_equivalence.rs`. The Evident
//! walk visits the same identifier leaves as the canonical Rust walk
//! (same AST structure → same leaves) and the shim classifies them with
//! the identical prefix rule; the name lists it returns are deduped into
//! the `HashSet`s here, so element order is irrelevant.

use std::path::Path;

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
/// it. Mirrors the public function the original `subscriptions.rs` exposes.
pub trait SubscriptionsImpl: Portable {
    /// Walk one claim and collect its world access sets.
    fn access_sets(&self, claim: &SchemaDecl) -> AccessSets;
}

// ─────────────────────────────────────────────────────────────────────
// Rust impl — the canonical analysis
// ─────────────────────────────────────────────────────────────────────

/// The native analysis. Total, fast, always correct — the default.
pub struct RustSubscriptions;

impl Portable for RustSubscriptions {
    fn impl_name(&self) -> &'static str { "rust" }
}

impl SubscriptionsImpl for RustSubscriptions {
    fn access_sets(&self, claim: &SchemaDecl) -> AccessSets {
        crate::subscriptions::world_access_sets(claim)
    }
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
// Classification — the one piece Evident can't express (no substring op)
// ─────────────────────────────────────────────────────────────────────

/// Classify one raw dotted identifier into the read/write sets, mirroring
/// `crate::subscriptions::walk_expr`'s `Identifier` arm 1:1: a
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
/// top-level-field attribution, matching the canonical analysis.
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

// ─────────────────────────────────────────────────────────────────────
// Selection
// ─────────────────────────────────────────────────────────────────────

/// Pick an impl by `EVIDENT_SUBSCRIPTIONS_IMPL` (`rust` | `evident`),
/// defaulting to the Rust impl. `evident` requires `EVIDENT_STDLIB_DIR`
/// to point at the repo `stdlib/`; if loading fails it falls back to
/// Rust.
pub fn default_impl() -> Box<dyn SubscriptionsImpl> {
    if std::env::var("EVIDENT_SUBSCRIPTIONS_IMPL").as_deref() == Ok("evident") {
        if let Ok(dir) = std::env::var("EVIDENT_STDLIB_DIR") {
            if let Ok(ev) = EvidentSubscriptions::new(Path::new(&dir)) {
                return Box::new(ev);
            }
        }
    }
    Box::new(RustSubscriptions)
}
