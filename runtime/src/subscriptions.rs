//! Static subscription / read-set inference for the multi-FSM
//! scheduler. See `docs/design/fsm-subscriptions.md` for the full
//! design.
//!
//! Phase 1 (this module): walk a `SchemaDecl`'s body and collect
//! the set of `world.X` reads (read-set) and `world_next.X` writes
//! (write-set). Pure analysis — no behavior change in the runtime.
//! Phase 2 will use these sets to drive delta scheduling.
//!
//! Limitations of this first cut:
//!   * `..ClaimName` passthrough and `ClaimCall` are NOT recursively
//!     resolved here — we treat them as opaque. Callers that need
//!     the fully-resolved set must walk transitively themselves.
//!     For the v1 multi-FSM scheduler, top-level FSM claims don't
//!     compose this way, so the local read-set matches the runtime
//!     behavior. We'll lift this restriction in Phase 5.
//!   * Subclaim bodies ARE walked (subclaims are in the parent's
//!     scope and inline at translate time, so their world reads are
//!     the parent's).

use std::collections::HashSet;

use crate::ast::{BodyItem, Expr, Mapping, MatchPattern, Pins, SchemaDecl};

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

/// Walk one claim and collect its world access sets.
pub fn world_access_sets(claim: &SchemaDecl) -> AccessSets {
    let mut sets = AccessSets::default();
    walk_body(&claim.body, &mut sets);
    sets
}

fn walk_body(body: &[BodyItem], sets: &mut AccessSets) {
    for item in body {
        match item {
            BodyItem::Membership { pins, .. } => walk_pins(pins, sets),
            BodyItem::Passthrough(_) => {}  // see module doc
            BodyItem::SubclaimDecl(s) => walk_body(&s.body, sets),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings { walk_expr(&m.value, sets); }
            }
            BodyItem::Constraint(e) => walk_expr(e, sets),
        }
    }
}

fn walk_pins(pins: &Pins, sets: &mut AccessSets) {
    match pins {
        Pins::None => {}
        Pins::Named(ms) => for m in ms { walk_expr(&m.value, sets); },
        Pins::Positional(es) => for e in es { walk_expr(e, sets); },
    }
}

fn walk_expr(e: &Expr, sets: &mut AccessSets) {
    match e {
        Expr::Identifier(name) => {
            if let Some(field) = name.strip_prefix("world_next.") {
                // Take only the first dotted segment after world_next.
                // (`world_next.player.pos.x` writes the `player`
                // top-level field, conservatively.)
                let first = first_segment(field);
                sets.writes.insert(first.to_string());
            } else if let Some(field) = name.strip_prefix("world.") {
                let first = first_segment(field);
                sets.reads.insert(first.to_string());
            }
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
            for x in es { walk_expr(x, sets); },
        Expr::Range(a, b) => { walk_expr(a, sets); walk_expr(b, sets); }
        Expr::InExpr(a, b) => { walk_expr(a, sets); walk_expr(b, sets); }
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            walk_expr(range, sets); walk_expr(body, sets);
        }
        Expr::Call(_, args) => for a in args { walk_expr(a, sets); },
        Expr::Cardinality(inner) | Expr::Not(inner) => walk_expr(inner, sets),
        Expr::Index(a, b) => { walk_expr(a, sets); walk_expr(b, sets); }
        Expr::Field(recv, _) => walk_expr(recv, sets),
        Expr::Binary(_, a, b) => { walk_expr(a, sets); walk_expr(b, sets); }
        Expr::Ternary(c, t, f) => {
            walk_expr(c, sets); walk_expr(t, sets); walk_expr(f, sets);
        }
        Expr::Match(scrut, arms) => {
            walk_expr(scrut, sets);
            for arm in arms { walk_expr(&arm.body, sets); }
            // Patterns can't reference world directly — they're
            // structural matches over the scrutinee — so no walk
            // needed for arm.pattern.
            let _ = MatchPattern::Wildcard; // anchor for future changes
        }
        Expr::Matches(e, _) => walk_expr(e, sets),
    }
    // Mapping appears only inside Pins/ClaimCall, handled above.
    let _ = std::any::type_name::<Mapping>();
}

fn first_segment(s: &str) -> &str {
    s.split('.').next().unwrap_or(s)
}

/// Returns true iff the claim's body references the named
/// effect constructor (e.g. "ReadLine", "Exit"). Used at load
/// time to detect conflicts — e.g. a program that has a stdin
/// plugin auto-installed AND emits Effect::ReadLine would race
/// for fd 0; the runtime rejects that combination.
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
        }
    }
    walk(&claim.body, ident)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn claim_named<'a>(src: &'a str, name: &str) -> SchemaDecl {
        let prog = parse(src).expect("parse");
        prog.schemas.iter().find(|s| s.name == name)
            .unwrap_or_else(|| panic!("claim `{name}` not found"))
            .clone()
    }

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn writer_only_has_no_reads() {
        let src = "\
type World
    a ∈ Int
    b ∈ Bool

claim setup
    world, world_next ∈ World
    world_next.a = 42
    world_next.b = true
";
        let c = claim_named(src, "setup");
        let s = world_access_sets(&c);
        assert_eq!(s.reads, HashSet::<String>::new());
        assert_eq!(s.writes, set(&["a", "b"]));
    }

    #[test]
    fn reader_collects_read_set() {
        let src = "\
type World
    a ∈ Int
    b ∈ Bool
    c ∈ Int

claim render
    world ∈ World
    msg ∈ Int
    msg = (world.b ? world.a : 0)
";
        let c = claim_named(src, "render");
        let s = world_access_sets(&c);
        assert_eq!(s.reads,  set(&["a", "b"]));  // c not referenced
        assert_eq!(s.writes, HashSet::<String>::new());
    }

    #[test]
    fn nested_field_path_reads_top_level() {
        // world.player.pos.x conservatively counts as a read of `player`.
        let src = "\
type World
    player ∈ Int
    score  ∈ Int

claim render
    world ∈ World
    out ∈ Int
    out = world.player + world.score
";
        let c = claim_named(src, "render");
        let s = world_access_sets(&c);
        assert_eq!(s.reads, set(&["player", "score"]));
    }

    #[test]
    fn reads_inside_match_arms_and_quantifiers() {
        let src = "\
type World
    a ∈ Int
    b ∈ Bool

enum S = One | Two

claim handler
    world ∈ World
    state ∈ S
    out ∈ Int
    out = match state
        One ⇒ world.a
        Two ⇒ (world.b ? 1 : 0)
";
        let c = claim_named(src, "handler");
        let s = world_access_sets(&c);
        assert_eq!(s.reads,  set(&["a", "b"]));
        assert_eq!(s.writes, HashSet::<String>::new());
    }

    #[test]
    fn writer_with_match_writes_one_field() {
        let src = "\
type World
    a ∈ Int

enum S = X | Y

claim w
    world, world_next ∈ World
    state ∈ S
    world_next.a = match state
        X ⇒ 1
        Y ⇒ 2
";
        let c = claim_named(src, "w");
        let s = world_access_sets(&c);
        assert_eq!(s.writes, set(&["a"]));
        assert_eq!(s.reads,  HashSet::<String>::new());
    }

    #[test]
    fn passthrough_writer_passes_through_field() {
        // `world_next.a = world.a` is the read+write idiom for
        // passthrough on a non-Done branch. Both should be tracked.
        let src = "\
type World
    a ∈ Int
    b ∈ Int

enum S = X | Y

claim w
    world, world_next ∈ World
    state ∈ S
    world_next.a = (state matches X ? 99 : world.a)
    world_next.b = world.b
";
        let c = claim_named(src, "w");
        let s = world_access_sets(&c);
        assert_eq!(s.reads,  set(&["a", "b"]));
        assert_eq!(s.writes, set(&["a", "b"]));
    }
}
