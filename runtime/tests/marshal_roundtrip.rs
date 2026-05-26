//! GAP-marshal: the AST encoder + decoder must round-trip `match`
//! patterns byte-identically — including the two shapes that were
//! historically dropped:
//!
//!   * SHAPE B — a NESTED constructor sub-pattern: `Node(Leaf(a), b)`
//!     (an uppercase-initial ctor INSIDE another ctor's payload). The
//!     old marshaler carried only one `MatchBind` level, so the inner
//!     `Leaf(a)` collapsed to `BindWildcard` and decoded back to a
//!     `Wildcard` — losing both the inner constructor name and its
//!     binds.
//!   * SHAPE A — a TOP-LEVEL bind arm: a bare lowercase identifier as
//!     the whole arm pattern (`other ⇒ 0`). `MatchPattern` had no
//!     `PatBind`, so it reflected (lossily) as `PatWildcard`.
//!
//! Pipeline under test (identical to `roundtrip_ast.rs`): parse source
//! → encode the user Program to a Z3 Datatype (`encode_ast.rs`) → pin
//! `output ∈ Program = <encoded>` and solve → read the model's `output`
//! back as `Value::Enum` → decode to a Rust `ast::Program`
//! (`decode_ast.rs`) → assert structural equality.
//!
//! These FAIL before the GAP-marshal fix (nested ctor → `Wildcard`,
//! top-level bind → `Wildcard`) and PASS after.

use std::path::Path;
use evident_runtime::EvidentRuntime;
use evident_runtime::ast::{self, Expr, MatchPattern};
use evident_runtime::translate::ast_decoder;

const STDLIB_AST: &str = "../stdlib/ast.ev";

/// Parse `user_src`, encode its Program, pin it to a synthetic
/// `output ∈ Program`, solve, and decode the model's `output` back to a
/// Rust `Program`. Same Z3 round-trip the reflection path uses.
fn round_trip(user_src: &str) -> ast::Program {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.mark_system_loads_complete();
    rt.load_source(user_src).unwrap();

    let prog_value = rt.encode_program_value().unwrap();
    rt.load_source("claim _marshal_round_trip\n    output ∈ Program\n").unwrap();
    let r = rt.query_with_program_value(
        "_marshal_round_trip", "output", prog_value,
    ).unwrap();
    assert!(r.satisfied, "identity round-trip should be SAT");
    let bound = r.bindings.get("output").expect("model should bind `output`");
    ast_decoder::decode_program(bound)
        .expect("decoder should reconstruct a Program from the bound value")
}

/// The RHS of the (single) `lhs = rhs` constraint in `schema`. The
/// demos below put the `match` on the RHS of an equality, so this digs
/// it out for inspection.
fn constraint_rhs<'a>(prog: &'a ast::Program, schema: &str) -> &'a Expr {
    let s = prog.schemas.iter().find(|s| s.name == schema)
        .unwrap_or_else(|| panic!("schema `{schema}` should round-trip"));
    for item in &s.body {
        if let ast::BodyItem::Constraint(Expr::Binary(_, _, rhs)) = item {
            return rhs.as_ref();
        }
    }
    panic!("no `lhs = rhs` constraint found in `{schema}`");
}

fn ctor(name: &str, binds: Vec<MatchPattern>) -> MatchPattern {
    MatchPattern::Ctor { name: name.into(), binds }
}
fn bind(name: &str) -> MatchPattern { MatchPattern::Bind(name.into()) }

// ── SHAPE B: nested constructor sub-pattern ─────────────────────────

#[test]
fn roundtrip_nested_ctor_pattern() {
    // `Node(Leaf(a), b)` — `Leaf(a)` is a ctor INSIDE `Node`'s first
    // payload slot. The second slot `b` is a plain bind.
    let src = "\
enum Tree = Leaf(Int) | Node(Tree, Tree)

claim t
    x ∈ Tree
    r ∈ Int
    r = match x
        Node(Leaf(a), b) ⇒ a
        _                ⇒ 0
";
    let decoded = round_trip(src);
    let rhs = constraint_rhs(&decoded, "t");
    let Expr::Match(scr, arms) = rhs else {
        panic!("expected Match on the RHS, got {rhs:?}");
    };
    assert!(matches!(scr.as_ref(), Expr::Identifier(n) if n == "x"));
    assert_eq!(arms.len(), 2);
    // The crux: the nested `Leaf(a)` must survive, not collapse to a
    // wildcard.
    assert_eq!(
        arms[0].pattern,
        ctor("Node", vec![ctor("Leaf", vec![bind("a")]), bind("b")]),
        "nested ctor sub-pattern must round-trip (was lossy: Leaf(a) → Wildcard)"
    );
    assert_eq!(arms[1].pattern, MatchPattern::Wildcard);
}

#[test]
fn roundtrip_deeply_nested_ctor_pattern() {
    // Two levels of nesting: `Node(Node(Leaf(a), Leaf(b)), c)`.
    let src = "\
enum Tree = Leaf(Int) | Node(Tree, Tree)

claim t
    x ∈ Tree
    r ∈ Int
    r = match x
        Node(Node(Leaf(a), Leaf(b)), c) ⇒ a
        _                               ⇒ 0
";
    let decoded = round_trip(src);
    let rhs = constraint_rhs(&decoded, "t");
    let Expr::Match(_, arms) = rhs else { panic!("expected Match, got {rhs:?}") };
    assert_eq!(
        arms[0].pattern,
        ctor("Node", vec![
            ctor("Node", vec![
                ctor("Leaf", vec![bind("a")]),
                ctor("Leaf", vec![bind("b")]),
            ]),
            bind("c"),
        ]),
        "two-level nested ctor sub-patterns must round-trip"
    );
}

#[test]
fn roundtrip_nested_ctor_in_matches_operator() {
    // `e matches Node(Leaf(_), _)` — the `Matches` form carries a
    // single `MatchPattern` (not a list of arms). Its nested ctor must
    // round-trip too.
    let src = "\
enum Tree = Leaf(Int) | Node(Tree, Tree)

claim t
    x ∈ Tree
    flag ∈ Bool
    flag = (x matches Node(Leaf(_), _))
";
    let decoded = round_trip(src);
    let rhs = constraint_rhs(&decoded, "t");
    let Expr::Matches(scr, pat) = rhs else {
        panic!("expected Matches on RHS, got {rhs:?}");
    };
    assert!(matches!(scr.as_ref(), Expr::Identifier(n) if n == "x"));
    assert_eq!(
        *pat,
        ctor("Node", vec![
            ctor("Leaf", vec![MatchPattern::Wildcard]),
            MatchPattern::Wildcard,
        ]),
        "nested ctor in `matches` must round-trip (Leaf(_) → Wildcard was lossy)"
    );
}

// ── SHAPE A: top-level bind arm ─────────────────────────────────────

#[test]
fn roundtrip_top_level_bind_arm() {
    // `other ⇒ 0` — the WHOLE arm pattern is a bare lowercase
    // identifier (a top-level bind, not a wildcard, not a ctor).
    let src = "\
enum Tree = Leaf(Int) | Node(Tree, Tree)

claim t
    x ∈ Tree
    r ∈ Int
    r = match x
        Leaf(n) ⇒ n
        other   ⇒ 0
";
    let decoded = round_trip(src);
    let rhs = constraint_rhs(&decoded, "t");
    let Expr::Match(_, arms) = rhs else { panic!("expected Match, got {rhs:?}") };
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].pattern, ctor("Leaf", vec![bind("n")]));
    assert_eq!(
        arms[1].pattern, bind("other"),
        "top-level bind arm must round-trip (was lossy: Bind(other) → Wildcard)"
    );
}

// ── `Match` nested inside a larger Expr ─────────────────────────────

#[test]
fn roundtrip_match_nested_in_arm_body() {
    // The outer `Match` is the larger `Expr`; an inner `Match` sits in
    // its first arm's BODY (the test_37 tree-walk shape). Exercises
    // decode recursing into a Match reached through another Match's
    // arms — AND the nested ctor `Node(l, c)` carrying a bind list.
    let src = "\
enum Tree = Leaf(Int) | Node(Tree, Tree)

claim t
    x ∈ Tree
    r ∈ Int
    r = match x
        Node(l, c) ⇒ match l
            Leaf(a) ⇒ a
            _       ⇒ 0
        _ ⇒ 0
";
    let decoded = round_trip(src);
    let rhs = constraint_rhs(&decoded, "t");
    let Expr::Match(_, arms) = rhs else {
        panic!("expected outer Match on RHS, got {rhs:?}");
    };
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].pattern, ctor("Node", vec![bind("l"), bind("c")]));
    // The first arm's body is itself a Match.
    let Expr::Match(inner_scr, inner_arms) = arms[0].body.as_ref() else {
        panic!("expected a Match nested in arm[0].body, got {:?}", arms[0].body);
    };
    assert!(matches!(inner_scr.as_ref(), Expr::Identifier(n) if n == "l"));
    assert_eq!(inner_arms[0].pattern, ctor("Leaf", vec![bind("a")]));
    assert_eq!(inner_arms[1].pattern, MatchPattern::Wildcard);
}

// ── Single-level binds + wildcards still round-trip (regression) ────

#[test]
fn roundtrip_simple_binds_and_wildcards_unchanged() {
    let src = "\
enum Result = Ok(Int) | Err(String)

claim t
    res ∈ Result
    score ∈ Int
    score = match res
        Ok(n)  ⇒ n
        Err(_) ⇒ 0
";
    let decoded = round_trip(src);
    let rhs = constraint_rhs(&decoded, "t");
    let Expr::Match(_, arms) = rhs else { panic!("expected Match, got {rhs:?}") };
    assert_eq!(arms[0].pattern, ctor("Ok", vec![bind("n")]));
    assert_eq!(arms[1].pattern, ctor("Err", vec![MatchPattern::Wildcard]));
}

// ── Neighbor audit: param_count / Pins / BodyItem variants ──────────
//
// These don't touch match patterns; they pin the rest of the
// encoder/decoder shape so the GAP-marshal change can't silently
// regress a neighbor. (param_count also has its own dedicated suite in
// param_count_roundtrip.rs — this re-confirms it round-trips here too.)

fn schema<'a>(prog: &'a ast::Program, name: &str) -> &'a ast::SchemaDecl {
    prog.schemas.iter().find(|s| s.name == name)
        .unwrap_or_else(|| panic!("schema `{name}` should round-trip"))
}

#[test]
fn roundtrip_param_count_first_line_params() {
    // `claim widget(a, b)` → param_count 2 (the first-line interface),
    // plus a body membership `c`.
    let decoded = round_trip("claim widget(a ∈ Int, b ∈ Int)\n    c ∈ Int\n");
    let widget = schema(&decoded, "widget");
    assert_eq!(widget.param_count, 2,
        "first-line params (a, b) → param_count 2 must survive the round-trip");
    assert_eq!(widget.body.len(), 3, "a, b, c");
}

#[test]
fn roundtrip_pins_positional_and_none() {
    let src = "\
type Vec2
    x ∈ Int
    y ∈ Int

claim t
    plain ∈ Int
    v ∈ Vec2(7, 9)
";
    let decoded = round_trip(src);
    let t = schema(&decoded, "t");
    // body[0]: `plain ∈ Int` → Pins::None
    match &t.body[0] {
        ast::BodyItem::Membership { name, pins: ast::Pins::None, .. } =>
            assert_eq!(name, "plain"),
        other => panic!("expected `plain ∈ Int` with Pins::None, got {other:?}"),
    }
    // body[1]: `v ∈ Vec2(7, 9)` → Pins::Positional([7, 9])
    match &t.body[1] {
        ast::BodyItem::Membership { name, pins: ast::Pins::Positional(args), .. } => {
            assert_eq!(name, "v");
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0], Expr::Int(7)));
            assert!(matches!(args[1], Expr::Int(9)));
        }
        other => panic!("expected `v ∈ Vec2(7,9)` with Pins::Positional, got {other:?}"),
    }
}

#[test]
fn roundtrip_passthrough_body_item() {
    let src = "\
claim Helper
    z ∈ Int

claim t
    ..Helper
";
    let decoded = round_trip(src);
    let t = schema(&decoded, "t");
    match &t.body[0] {
        ast::BodyItem::Passthrough(name) => assert_eq!(name, "Helper"),
        other => panic!("expected Passthrough(Helper), got {other:?}"),
    }
}

#[test]
fn roundtrip_claimcall_body_item() {
    let src = "\
claim Prop
    a ∈ Int

claim t
    b ∈ Int
    Prop (a ↦ b)
";
    let decoded = round_trip(src);
    let t = schema(&decoded, "t");
    let call = t.body.iter().find_map(|i| match i {
        ast::BodyItem::ClaimCall { name, mappings } => Some((name, mappings)),
        _ => None,
    }).expect("expected a ClaimCall body item");
    assert_eq!(call.0, "Prop");
    assert_eq!(call.1.len(), 1);
    assert_eq!(call.1[0].slot, "a");
    assert!(matches!(&call.1[0].value, Expr::Identifier(n) if n == "b"));
}

#[test]
fn roundtrip_subclaim_body_item() {
    let src = "\
claim t
    p ∈ Int
    subclaim Inner
        q ∈ Int
        q = p
";
    let decoded = round_trip(src);
    let t = schema(&decoded, "t");
    let sub = t.body.iter().find_map(|i| match i {
        ast::BodyItem::SubclaimDecl(s) => Some(s),
        _ => None,
    }).expect("expected a SubclaimDecl body item");
    assert_eq!(sub.name, "Inner");
    assert!(matches!(sub.keyword, ast::Keyword::Subclaim));
    assert_eq!(sub.body.len(), 2, "q ∈ Int; q = p");
}

#[test]
fn roundtrip_halts_within_body_item() {
    // `halts_within(f, 10)` → BodyItem::HaltsWithin { fsm_name, n }.
    let src = "\
fsm decrement(count ∈ Int, halt ∈ Bool)
    count = _count - 1
    halt  = (_count ≤ 0)

claim t
    halts_within(decrement, 10)
";
    let decoded = round_trip(src);
    let t = schema(&decoded, "t");
    let hw = t.body.iter().find_map(|i| match i {
        ast::BodyItem::HaltsWithin { fsm_name, n } => Some((fsm_name, *n)),
        _ => None,
    }).expect("expected a HaltsWithin body item");
    assert_eq!(hw.0, "decrement");
    assert_eq!(hw.1, 10);
}
