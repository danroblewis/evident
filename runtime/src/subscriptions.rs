//! Static subscription / read-set inference types for the multi-FSM
//! scheduler. See `docs/design/fsm-subscriptions.md` for the full
//! design.
//!
//! ## Session XX — the walk lives in Evident now
//!
//! The canonical Rust walk (`world_access_sets` + its `walk_body` /
//! `walk_pins` / `walk_expr` / `first_segment` traversal) was **deleted**
//! in session XX. The whole walk is now the self-hosted stack-FSM
//! `stdlib/passes/subscriptions.ev`, driven through
//! [`crate::portable::subscriptions`] — the runtime's SOLE subscriptions
//! implementation. The scheduler computes a claim's `(reads, writes)` via
//! [`crate::portable::subscriptions::access_sets`], which marshals the
//! claim body into a `Value`, runs the `subscriptions_walk` FSM to a
//! drained-stack halt (`effect_loop::run_nested`), and classifies the
//! reachable identifiers by their `world.` / `world_next.` prefix.
//!
//! This module keeps only the two pieces that did NOT move:
//!   * [`AccessSets`] — the read/write-set value type the scheduler and
//!     the Evident shim both produce and consume.
//!   * [`body_references_identifier`] — a *different* analysis (does a
//!     body reference a named effect constructor, e.g. `ReadLine`?), used
//!     at load time to detect fd-resource conflicts. It is not a
//!     world-access walk and has no Evident twin.

use std::collections::HashSet;

use crate::core::ast::{BodyItem, Expr, Pins, SchemaDecl};

/// Read-set + write-set for one FSM claim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessSets {
    /// Field names X such that `world.X` appears anywhere in the
    /// claim body. NOT including `world` itself bare (which would
    /// indicate a whole-record read — currently the language always
    /// dot-accesses individual fields).
    pub reads:  HashSet<String>,
    /// Field names X such that `world_next.X` appears as the LHS of
    /// any constraint. Right now we conservatively count any
    /// reference to `world_next.X` as a write — refinement to
    /// "only LHS of equality" can come later if needed.
    pub writes: HashSet<String>,
}

/// Returns true iff the claim's body references the named
/// effect constructor (e.g. "ReadLine", "Exit"). Used at load
/// time to detect conflicts — e.g. a program that has a stdin
/// plugin auto-installed AND emits Effect::ReadLine would race
/// for fd 0; the runtime rejects that combination.
///
/// This is a plain identifier-presence check, NOT the world-access
/// walk (that moved to `stdlib/passes/subscriptions.ev` — see the
/// module doc). It stays in Rust because it answers a different
/// question (does name N appear anywhere) and has no Evident port.
pub fn body_references_identifier(claim: &SchemaDecl, ident: &str) -> bool {
    fn walk(items: &[BodyItem], ident: &str) -> bool {
        for item in items {
            match item {
                BodyItem::Membership { pins, .. } => {
                    if walk_pins(pins, ident) { return true; }
                }
                BodyItem::Passthrough(_) => {}
                BodyItem::SubclaimDecl(s) => {
                    if walk(&s.body, ident) { return true; }
                }
                BodyItem::ClaimCall { mappings, .. } => {
                    for m in mappings {
                        if walk_expr(&m.value, ident) { return true; }
                    }
                }
                BodyItem::Constraint(e) => {
                    if walk_expr(e, ident) { return true; }
                }
                // No effect constructors live in a halts_within directive.
                BodyItem::HaltsWithin { .. } => {}
            }
        }
        false
    }
    fn walk_pins(pins: &Pins, ident: &str) -> bool {
        match pins {
            Pins::None => false,
            Pins::Named(ms) => ms.iter().any(|m| walk_expr(&m.value, ident)),
            Pins::Positional(es) => es.iter().any(|e| walk_expr(e, ident)),
        }
    }
    fn walk_expr(e: &Expr, ident: &str) -> bool {
        match e {
            Expr::Identifier(s) => s == ident,
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => false,
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                es.iter().any(|x| walk_expr(x, ident)),
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) | Expr::Binary(_, a, b) =>
                walk_expr(a, ident) || walk_expr(b, ident),
            Expr::Forall(_, range, body) | Expr::Exists(_, range, body) =>
                walk_expr(range, ident) || walk_expr(body, ident),
            Expr::Call(name, args) =>
                name == ident || args.iter().any(|a| walk_expr(a, ident)),
            Expr::Cardinality(inner) | Expr::Not(inner) => walk_expr(inner, ident),
            Expr::Field(recv, _) => walk_expr(recv, ident),
            Expr::Ternary(c, t, f) =>
                walk_expr(c, ident) || walk_expr(t, ident) || walk_expr(f, ident),
            Expr::Match(scrut, arms) =>
                walk_expr(scrut, ident) || arms.iter().any(|a| walk_expr(&a.body, ident)),
            Expr::Matches(e, _) => walk_expr(e, ident),
            Expr::RunFsm { init, .. } => walk_expr(init, ident),
        }
    }
    walk(&claim.body, ident)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn claim_named(src: &str, name: &str) -> SchemaDecl {
        let prog = parse(src).expect("parse");
        prog.schemas.iter().find(|s| s.name == name)
            .unwrap_or_else(|| panic!("claim `{name}` not found"))
            .clone()
    }

    #[test]
    fn references_effect_constructor() {
        let src = "\
type World
    a ∈ Int

claim emitter
    world, world_next ∈ World
    effects ∈ Seq(Effect)
    effects = ⟨ReadLine⟩
";
        let c = claim_named(src, "emitter");
        assert!(body_references_identifier(&c, "ReadLine"));
        assert!(!body_references_identifier(&c, "Exit"));
    }
}
