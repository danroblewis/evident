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
//!     marshals the claim body into a `WList` value, drives the FSM to a
//!     drained-agenda halt via [`crate::effect_loop::run_nested`], and
//!     decodes the `(reads, writes)` accumulators. **No Rust-side tree
//!     walk** — the recursion, accumulation, and `world.`/`world_next.`
//!     classification all live in the pass.
//!
//! ## Session QQ: the LOC inversion
//!
//! This is the first port that REMOVES Rust rather than adding it. The
//! previous shim duplicated `crate::subscriptions::{walk_body, walk_pins,
//! walk_expr}` in Rust and called a leaf-only Evident classifier per
//! identifier. Those Rust walk functions are gone; what remains is a
//! structural encoder (`encode_body`/`encode_expr`/…) that maps each
//! ast.rs node to a `WNode` behavioral class — mirroring how the
//! canonical `walk_expr` GROUPS variants by traversal shape (its
//! `|`-patterns) — plus a cons-list decoder. The traversal logic itself
//! is the FSM in `stdlib/passes/subscriptions.ev`.
//!
//! ## Faithful equivalence
//!
//! Both impls produce byte-identical `AccessSets` (HashSet<String>
//! equality) on every FSM-shaped claim across the demo corpus including
//! Mario — see `runtime/tests/subscriptions_equivalence.rs`. The Evident
//! walk visits the same identifier leaves as the canonical Rust walk and
//! classifies them identically; the name cons-lists it returns are
//! deduped into the `HashSet`s here, so element order is irrelevant.

use std::collections::HashSet;
use std::path::Path;

use crate::core::ast::{BodyItem, Expr, Pins, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::subscriptions::AccessSets;
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

/// Runs the analysis by encoding the claim body as a composite `Value`
/// and driving the `subscriptions_walk` FSM to halt. Holds its own
/// runtime with the pass loaded; build once and reuse so the FSM's
/// per-tick solve is JIT-cached across calls.
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
    /// is self-contained (it walks its own `WNode` encoding, not the
    /// `ast.ev` enums), so no other stdlib file is needed.
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
        // agenda is one shallow subtree, so the per-tick state marshaled
        // through `run_nested` stays small — the difference between an
        // O(N) and an O(N²) total marshaling cost on a big claim like
        // Mario's `game`. This is a flat driver over the top-level items
        // (NOT a tree walk): every recursion — into sub-expressions AND
        // into subclaim bodies — happens inside the FSM. reads/writes is
        // a set union over items, so per-item-then-union is byte-identical
        // to walking the whole body in one pass.
        for item in &claim.body {
            if let Some(node) = encode_body_item(item) {
                self.walk_node(&node, &claim.name, &mut sets);
            }
        }
        sets
    }
}

impl EvidentSubscriptions {
    /// Drive `subscriptions_walk` over one encoded node (seeded as a
    /// single-item agenda frame) and fold its `(reads, writes)` into
    /// `sets`. `run_nested` coerces the `WList` seed into the state
    /// enum's first single-payload variant (`SWSeed(WList)`).
    fn walk_node(&self, node: &Value, claim_name: &str, sets: &mut AccessSets) {
        let seed = wlist(vec![node.clone()]);
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "SWDone" && fields.len() == 2 => {
                decode_name_list(&fields[0], &mut sets.reads);
                decode_name_list(&fields[1], &mut sets.writes);
            }
            Ok(other) => eprintln!("[subscriptions/evident] walk of `{claim_name}` \
                returned a non-SWDone state: {other:?}"),
            Err(e) => eprintln!("[subscriptions/evident] walk of `{claim_name}` failed: {e}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Structural marshaling: ast.rs node → WNode `Value` tree
// ─────────────────────────────────────────────────────────────────────
//
// Each ast.rs Expr / BodyItem / Pins maps to one `WNode`, mirroring how
// `crate::subscriptions::{walk_body, walk_pins, walk_expr}` group
// variants by traversal shape (their `|`-patterns). Scalar fields those
// walks never read (BinOp, call/field/fsm names, match patterns, ∀-vars,
// mapping slots) are dropped — faithful to the canonical walk, which
// binds them `_`. This encoder makes NO read/write decision and NO
// recursion-to-fixpoint: it serializes one node's shape; the FSM walks it.
//
// Two leaf-set-preserving simplifications keep the encoded tree (and so
// the per-tick agenda) small:
//   * a node that can hold no identifier is dropped (`None`): literals,
//     `Passthrough`/`HaltsWithin`, a pins-free `Membership`;
//   * a node with a single identifier-bearing child collapses to that
//     child — a pass-through (`Field`/`Cardinality`/`Not`/`Matches`/
//     `RunFsm`/`Constraint`) adds nothing of its own, and an N-ary node
//     with one surviving child needs no list wrapper.
// Both preserve the exact set of identifier leaves the canonical walk
// reaches; only empty/pass-through scaffolding disappears. The FSM still
// carries the general `NLeaf`/`NOne`/`NThree` arms (exercised by the
// pass's inline tests); the encoder simply never needs to emit them.

fn ev(enum_name: &str, variant: &str, fields: Vec<Value>) -> Value {
    Value::Enum { enum_name: enum_name.to_string(), variant: variant.to_string(), fields }
}

fn nident(segs: Value) -> Value { ev("WNode", "NIdent", vec![segs]) }
fn n_two(a: Value, b: Value) -> Value { ev("WNode", "NTwo", vec![a, b]) }
fn n_list(items: Vec<Value>) -> Value { ev("WNode", "NList", vec![wlist(items)]) }

/// Build a `WList` cons-list (`WLCons`/`WLNil`) from already-encoded WNodes.
fn wlist(items: Vec<Value>) -> Value {
    let mut acc = ev("WList", "WLNil", vec![]);
    for head in items.into_iter().rev() {
        acc = ev("WList", "WLCons", vec![head, acc]);
    }
    acc
}

/// Split a dotted-collapsed identifier into a `Segs` cons-list, head-first.
/// "world.player.pos" → SegCons("world", SegCons("player", SegCons("pos", SegNil))).
fn encode_segments(name: &str) -> Value {
    let mut acc = ev("Segs", "SegNil", vec![]);
    for seg in name.split('.').collect::<Vec<_>>().into_iter().rev() {
        acc = ev("Segs", "SegCons", vec![Value::Str(seg.to_string()), acc]);
    }
    acc
}

/// Wrap surviving children, collapsing the trivial cases: none → `None`
/// (contributes nothing); exactly one → that child directly; otherwise an
/// `NList`.
fn list_node(children: Vec<Value>) -> Option<Value> {
    match children.len() {
        0 => None,
        1 => children.into_iter().next(),
        _ => Some(n_list(children)),
    }
}

/// Combine the two children of a binary-shaped node, collapsing to the
/// single survivor (or `None`) when one/both are empty.
fn two_node(a: Option<Value>, b: Option<Value>) -> Option<Value> {
    match (a, b) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x),
        (Some(x), Some(y)) => Some(n_two(x, y)),
    }
}

fn encode_body_item(item: &BodyItem) -> Option<Value> {
    match item {
        // walk_body: Membership { pins } => walk_pins(pins).
        BodyItem::Membership { pins, .. } => encode_pins(pins),
        // walk_body: Passthrough / HaltsWithin contribute no world access.
        BodyItem::Passthrough(_) | BodyItem::HaltsWithin { .. } => None,
        // walk_body: SubclaimDecl(s) => walk_body(s.body).
        BodyItem::SubclaimDecl(s) =>
            list_node(s.body.iter().filter_map(encode_body_item).collect()),
        // walk_body: ClaimCall { mappings } => walk each m.value.
        BodyItem::ClaimCall { mappings, .. } =>
            list_node(mappings.iter().filter_map(|m| encode_expr(&m.value)).collect()),
        // walk_body: Constraint(e) => walk_expr(e).
        BodyItem::Constraint(e) => encode_expr(e),
    }
}

fn encode_pins(pins: &Pins) -> Option<Value> {
    match pins {
        // walk_pins: None => nothing.
        Pins::None => None,
        // walk_pins: Named(ms) => walk each m.value.
        Pins::Named(ms) => list_node(ms.iter().filter_map(|m| encode_expr(&m.value)).collect()),
        // walk_pins: Positional(es) => walk each e.
        Pins::Positional(es) => list_node(es.iter().filter_map(encode_expr).collect()),
    }
}

fn encode_expr(e: &Expr) -> Option<Value> {
    match e {
        // The only classifying leaf — split the dotted name into segments.
        Expr::Identifier(name) => Some(nident(encode_segments(name))),
        // walk_expr: Int | Real | Bool | Str => {}.
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => None,
        // walk_expr: SetLit | SeqLit | Tuple => walk each element.
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
            list_node(es.iter().filter_map(encode_expr).collect()),
        // walk_expr: Range / InExpr / Index / Binary => walk a, b.
        Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
            two_node(encode_expr(a), encode_expr(b)),
        Expr::Binary(_, a, b) => two_node(encode_expr(a), encode_expr(b)),
        // walk_expr: Forall / Exists => walk range, body (vars dropped).
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) =>
            two_node(encode_expr(range), encode_expr(body)),
        // walk_expr: Call(_, args) => walk each arg (name dropped).
        Expr::Call(_, args) => list_node(args.iter().filter_map(encode_expr).collect()),
        // walk_expr: Cardinality / Not => walk inner.
        Expr::Cardinality(inner) | Expr::Not(inner) => encode_expr(inner),
        // walk_expr: Field(recv, _) => walk recv (field name dropped).
        Expr::Field(recv, _) => encode_expr(recv),
        // walk_expr: Ternary(c, t, f) => walk all three.
        Expr::Ternary(c, t, f) =>
            list_node([c.as_ref(), t.as_ref(), f.as_ref()]
                .into_iter().filter_map(encode_expr).collect()),
        // walk_expr: Match(scrut, arms) => walk scrut + each arm body
        // (patterns dropped). Order-insensitive — flattened into one list.
        Expr::Match(scrut, arms) => {
            let mut items: Vec<Value> = Vec::new();
            items.extend(encode_expr(scrut));
            for arm in arms { items.extend(encode_expr(&arm.body)); }
            list_node(items)
        }
        // walk_expr: Matches(inner, _) => walk inner (pattern dropped).
        Expr::Matches(inner, _) => encode_expr(inner),
        // walk_expr: RunFsm { init, .. } => walk init (fsm name dropped).
        Expr::RunFsm { init, .. } => encode_expr(init),
    }
    // Mapping is reached only inside Pins / ClaimCall, handled above.
}

/// Decode a `NameList` (`NameCons`/`NameNil`) cons-list into a set of
/// field names. Duplicates collapse — the canonical analysis returns a
/// HashSet too, so order and repeats are irrelevant.
fn decode_name_list(v: &Value, out: &mut HashSet<String>) {
    let mut cur = v;
    while let Value::Enum { variant, fields, .. } = cur {
        if variant != "NameCons" { break; } // NameNil terminates
        match fields.as_slice() {
            [Value::Str(name), tail] => {
                out.insert(name.clone());
                cur = tail;
            }
            _ => break,
        }
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
