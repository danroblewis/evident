//! Read/write-set types for the multi-FSM scheduler. World-access walk lives in
//! `stdlib/passes/subscriptions.ev`; this module keeps only `AccessSets` + `body_references_identifier`.

use std::collections::HashSet;

use crate::core::ast::{BodyItem, Expr, Pins, SchemaDecl};

/// Read-set + write-set for one FSM claim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessSets {
    /// Field names X where `world.X` appears in the body.
    pub reads:  HashSet<String>,
    /// Field names X where `world_next.X` appears; conservatively counts any reference as a write.
    pub writes: HashSet<String>,
}

/// Returns true iff the claim body references `ident` (e.g. "ReadLine").
/// Used at load time to detect fd-resource conflicts (stdin plugin + ReadLine emitter → reject).
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
                BodyItem::HaltsWithin { .. } => {} // no effect constructors here
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
