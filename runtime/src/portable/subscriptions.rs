//! `subscriptions` — static world-access-set inference for the
//! multi-FSM scheduler, ported into the [`super`] swap interface.
//!
//! See [`crate::subscriptions`] for what the analysis computes (read/write
//! sets per claim) and where its output drives the runtime
//! ([`crate::effect_loop`] scheduler subscriptions). Two impls:
//!   * [`RustSubscriptions`] — wraps the canonical
//!     [`crate::subscriptions::world_access_sets`] directly.
//!   * [`EvidentSubscriptions`] — owns an [`EvidentRuntime`] with
//!     `stdlib/passes/subscriptions.ev` loaded; walks the AST in Rust
//!     and delegates each leaf identifier's classification (`world.X` →
//!     read, `world_next.X` → write) to the Evident pass via
//!     [`EvidentRuntime::query`].
//!
//! ## Why the shim drives the walk
//!
//! A whole-claim-as-input pass would need to recurse through the body's
//! `Expr` tree to find every `EIdentifier(_)` leaf. The runtime can't
//! self-host that recursion yet (see `docs/self-hosting.md` "Recursive
//! claims don't constrain their outputs"; `examples/COUNTEREXAMPLES.md`).
//! So the Evident pass owns the *classification semantics* — what makes
//! an identifier a read or a write — and the shim owns the tree walk.
//! The shim's walk logic mirrors [`crate::subscriptions::walk_body`] /
//! `walk_expr` so the two impls visit identical leaf sets.
//!
//! ## Faithful equivalence
//!
//! Both impls produce byte-identical `AccessSets` (HashSet<String>
//! equality) on every FSM-shaped claim across the demo corpus — see
//! `runtime/tests/subscriptions_equivalence.rs`. This is full
//! faithfulness on the analysis's actual semantic surface; no
//! "unsupported-shape" sentinel as in [`super::pretty`].

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::core::ast::{BodyItem, Expr, Mapping, Pins, SchemaDecl};
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
// Evident impl — calls stdlib/passes/subscriptions.ev
// ─────────────────────────────────────────────────────────────────────

/// Runs the analysis by delegating each leaf-identifier classification
/// to `stdlib/passes/subscriptions.ev::classify_world_ident`. Holds its
/// own runtime with the pass loaded; build once and reuse so the pass
/// is compiled/cached across calls.
///
/// Internally also caches the (`is_read`, `is_write`, `suffix`) result
/// per identifier string. Mario's `game` claim alone references the same
/// `world.X` strings dozens of times across guard arms; without caching
/// each call is a Z3 query.
pub struct EvidentSubscriptions {
    rt: EvidentRuntime,
    cache: std::cell::RefCell<HashMap<String, ClassResult>>,
}

#[derive(Clone, Debug)]
struct ClassResult {
    is_read:  bool,
    is_write: bool,
    /// Identifier with the matching prefix stripped — for a read,
    /// `ident.strip_prefix("world.")`; for a write,
    /// `ident.strip_prefix("world_next.")`. The shim takes the first
    /// dotted segment to recover the top-level field name (matching
    /// `subscriptions::first_segment`).
    suffix:   String,
}

impl EvidentSubscriptions {
    /// SAT iff `ident` begins with the literal `"world_next."`; on SAT
    /// the model binds `suffix_w` to the rest.
    const WRITE_MATCH_CLAIM: &'static str = "world_next_write_match";
    /// SAT iff `ident` begins with the literal `"world."`; on SAT the
    /// model binds `suffix_r` to the rest.
    const READ_MATCH_CLAIM: &'static str = "world_read_match";

    /// Load `ast.ev` + `passes/subscriptions.ev` from `stdlib_dir` into
    /// a fresh runtime. `stdlib_dir` is the repo's `stdlib/` directory.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("ast.ev"))
            .map_err(|e| format!("load ast.ev: {e}"))?;
        rt.load_file(&stdlib_dir.join("passes").join("subscriptions.ev"))
            .map_err(|e| format!("load passes/subscriptions.ev: {e}"))?;
        Ok(Self { rt, cache: std::cell::RefCell::new(HashMap::new()) })
    }

    /// Classify one identifier string by running the Evident pass. On
    /// cache hit returns the prior result; on miss runs the two
    /// match-claims (SAT = prefix matched) and memoizes.
    ///
    /// The two prefixes are mutually exclusive (`world.` has '.' at
    /// index 5; `world_next.` has '_'). Checking the write claim first
    /// gives the same result either way, but the order matches the
    /// pass file's stated ordering.
    fn classify(&self, ident: &str) -> ClassResult {
        if let Some(hit) = self.cache.borrow().get(ident) {
            return hit.clone();
        }
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("ident".to_string(), Value::Str(ident.to_string()));

        let r = if let Ok(qr) = self.rt.query(Self::WRITE_MATCH_CLAIM, &given) {
            if qr.satisfied {
                ClassResult {
                    is_read:  false,
                    is_write: true,
                    suffix:   take_string(qr.bindings.get("suffix_w")),
                }
            } else if let Ok(qr) = self.rt.query(Self::READ_MATCH_CLAIM, &given) {
                if qr.satisfied {
                    ClassResult {
                        is_read:  true,
                        is_write: false,
                        suffix:   take_string(qr.bindings.get("suffix_r")),
                    }
                } else {
                    ClassResult { is_read: false, is_write: false, suffix: String::new() }
                }
            } else {
                ClassResult { is_read: false, is_write: false, suffix: String::new() }
            }
        } else {
            ClassResult { is_read: false, is_write: false, suffix: String::new() }
        };

        self.cache.borrow_mut().insert(ident.to_string(), r.clone());
        r
    }
}

fn take_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

impl Portable for EvidentSubscriptions {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl SubscriptionsImpl for EvidentSubscriptions {
    fn access_sets(&self, claim: &SchemaDecl) -> AccessSets {
        let mut sets = AccessSets::default();
        walk_body(self, &claim.body, &mut sets);
        sets
    }
}

// ─────────────────────────────────────────────────────────────────────
// AST walk — mirrors crate::subscriptions::walk_body / walk_expr
// ─────────────────────────────────────────────────────────────────────
//
// Kept structurally identical to subscriptions.rs so the two visit the
// same leaf identifiers in the same order. The only difference is where
// classification happens: Rust calls a `first_segment` helper inline,
// Evident defers to the pass via `EvidentSubscriptions::classify`.

fn walk_body(ev: &EvidentSubscriptions, body: &[BodyItem], sets: &mut AccessSets) {
    for item in body {
        match item {
            BodyItem::Membership { pins, .. } => walk_pins(ev, pins, sets),
            BodyItem::Passthrough(_) => {}  // see subscriptions.rs module doc
            BodyItem::SubclaimDecl(s) => walk_body(ev, &s.body, sets),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings { walk_expr(ev, &m.value, sets); }
            }
            BodyItem::Constraint(e) => walk_expr(ev, e, sets),
        }
    }
}

fn walk_pins(ev: &EvidentSubscriptions, pins: &Pins, sets: &mut AccessSets) {
    match pins {
        Pins::None => {}
        Pins::Named(ms) => for m in ms { walk_expr(ev, &m.value, sets); },
        Pins::Positional(es) => for e in es { walk_expr(ev, e, sets); },
    }
}

fn walk_expr(ev: &EvidentSubscriptions, e: &Expr, sets: &mut AccessSets) {
    match e {
        Expr::Identifier(name) => {
            let r = ev.classify(name);
            if r.is_write {
                sets.writes.insert(first_segment(&r.suffix).to_string());
            } else if r.is_read {
                sets.reads.insert(first_segment(&r.suffix).to_string());
            }
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
            for x in es { walk_expr(ev, x, sets); },
        Expr::Range(a, b) => { walk_expr(ev, a, sets); walk_expr(ev, b, sets); }
        Expr::InExpr(a, b) => { walk_expr(ev, a, sets); walk_expr(ev, b, sets); }
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            walk_expr(ev, range, sets); walk_expr(ev, body, sets);
        }
        Expr::Call(_, args) => for a in args { walk_expr(ev, a, sets); },
        Expr::Cardinality(inner) | Expr::Not(inner) => walk_expr(ev, inner, sets),
        Expr::Index(a, b) => { walk_expr(ev, a, sets); walk_expr(ev, b, sets); }
        Expr::Field(recv, _) => walk_expr(ev, recv, sets),
        Expr::Binary(_, a, b) => { walk_expr(ev, a, sets); walk_expr(ev, b, sets); }
        Expr::Ternary(c, t, f) => {
            walk_expr(ev, c, sets); walk_expr(ev, t, sets); walk_expr(ev, f, sets);
        }
        Expr::Match(scrut, arms) => {
            walk_expr(ev, scrut, sets);
            for arm in arms { walk_expr(ev, &arm.body, sets); }
        }
        Expr::Matches(inner, _) => walk_expr(ev, inner, sets),
    }
    let _ = std::any::type_name::<Mapping>(); // anchor: subscriptions.rs parity
}

fn first_segment(s: &str) -> &str {
    s.split('.').next().unwrap_or(s)
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
