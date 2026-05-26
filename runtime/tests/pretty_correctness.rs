//! Correctness pins for the self-hosted `pretty` renderer — the
//! `stdlib/passes/pretty.ev` `pretty_walk` stack-FSM reached through
//! [`EvidentPretty`]. This is now the SOLE renderer (the native `RustPretty`
//! was retired in session pretty-evident), so these tests pin its output
//! DIRECTLY rather than cross-checking against a Rust twin.
//!
//! `pretty_walk` is an ordered Emit/Expand stack-FSM that routes around the
//! recursion gap (#15) exactly as `subscriptions_walk` does, so it renders
//! recursive sub-`Expr`s, not just leaves. The two gaps that once bounded
//! byte-fidelity are CLOSED:
//!   * Unicode glyphs (#16) — the encode side now escapes non-ASCII to
//!     `\u{..}` before `Z3_mk_string` and the decode side recovers them, so
//!     operator glyphs (`∈ ∀ ∃ ∧ ∨ ⇒ ≠ ≤ ≥ ¬ ⟨⟩ ↦ …`) round-trip exactly.
//!   * int→string — `str_from_int` (Z3 `Z3_mk_int_to_str`, sign reattached)
//!     renders `EInt` + `BIHaltsWithin`; `EBool` renders via a ternary.
//!
//! The one documented residual is `EReal` (Z3 has no real→string matching
//! Rust's shortest-round-trip f64 Display) — pinned to the `<real>` sentinel
//! at the bottom.

use std::path::Path;

use evident_runtime::ast::{BinOp, BodyItem, Expr, Mapping, MatchArm, MatchPattern, Pins};
use evident_runtime::portable::pretty::{EvidentPretty, PrettyImpl};

const STDLIB: &str = "../stdlib";

fn pretty() -> EvidentPretty {
    EvidentPretty::new(Path::new(STDLIB)).expect("load stdlib/passes/pretty.ev")
}

// ── helpers to build AST nodes ──
fn ident(n: &str) -> Expr { Expr::Identifier(n.to_string()) }
fn boxed(e: Expr) -> Box<Expr> { Box::new(e) }
fn bin(op: BinOp, l: Expr, r: Expr) -> Expr { Expr::Binary(op, boxed(l), boxed(r)) }
fn call(n: &str, args: Vec<Expr>) -> Expr { Expr::Call(n.to_string(), args) }
fn field(recv: Expr, f: &str) -> Expr { Expr::Field(boxed(recv), f.to_string()) }
fn index(s: Expr, i: Expr) -> Expr { Expr::Index(boxed(s), boxed(i)) }

// ─────────────────────────────────────────────────────────────────────
// 1. Expressions — recursive, including the now-faithful number/glyph shapes
// ─────────────────────────────────────────────────────────────────────

#[test]
fn expr_renders_match_expected() {
    let p = pretty();
    // (node, expected render). Pure-ASCII, Unicode-glyph, and numeric shapes
    // are ALL faithful now — the renderer reproduces them byte-for-byte,
    // including the recursion into sub-expressions.
    let cases: Vec<(Expr, &str)> = vec![
        // leaves
        (ident("x"), "x"),
        (ident("counter"), "counter"),
        (ident("foo.bar.baz"), "foo.bar.baz"),
        (Expr::Str("hi".to_string()), "\"hi\""),
        // numbers (str_from_int) + bool (ternary) — formerly sentinels
        (Expr::Int(0), "0"),
        (Expr::Int(42), "42"),
        (Expr::Int(10042), "10042"),
        (Expr::Int(-7), "-7"),     // sign reattached (str.from_int is naturals-only)
        (Expr::Bool(true), "true"),
        (Expr::Bool(false), "false"),
        // ASCII binary operators — recurse into both operands
        (bin(BinOp::Eq,  ident("a"), ident("b")), "a = b"),
        (bin(BinOp::Lt,  ident("counter"), ident("limit")), "counter < limit"),
        (bin(BinOp::Add, ident("a"), ident("b")), "a + b"),
        (bin(BinOp::Sub, ident("a"), ident("b")), "a - b"),
        (bin(BinOp::Mul, ident("a"), ident("b")), "a * b"),
        (bin(BinOp::Div, ident("a"), ident("b")), "a / b"),
        (bin(BinOp::Concat, ident("a"), ident("b")), "a ++ b"),
        // Unicode binary operators — formerly diverged on the glyph (#16)
        (bin(BinOp::Neq, ident("lo"), ident("hi")), "lo ≠ hi"),
        (bin(BinOp::Le,  ident("lo"), ident("hi")), "lo ≤ hi"),
        (bin(BinOp::Ge,  ident("lo"), ident("hi")), "lo ≥ hi"),
        (bin(BinOp::And, ident("lo"), ident("hi")), "lo ∧ hi"),
        (bin(BinOp::Or,  ident("lo"), ident("hi")), "lo ∨ hi"),
        (bin(BinOp::Implies, ident("lo"), ident("hi")), "lo ⇒ hi"),
        // nested binary → inner Binary operand is parenthesized
        (bin(BinOp::Mul, bin(BinOp::Add, ident("a"), ident("b")), ident("c")), "(a + b) * c"),
        (bin(BinOp::Add, ident("a"), bin(BinOp::Mul, ident("b"), ident("c"))), "a + (b * c)"),
        // a number embedded in a recursive structure
        (call("f", vec![ident("x"), Expr::Int(5)]), "f(x, 5)"),
        // calls recurse into the argument list
        (call("f", vec![ident("x"), ident("y")]), "f(x, y)"),
        (call("coindexed", vec![ident("a"), ident("b"), ident("c")]), "coindexed(a, b, c)"),
        (call("g", vec![]), "g()"),
        // set / tuple / seq literals (comma-separated, recursive)
        (Expr::SetLit(vec![ident("a"), ident("b"), ident("c")]), "{a, b, c}"),
        (Expr::Tuple(vec![ident("a"), ident("b")]), "(a, b)"),
        (Expr::SeqLit(vec![ident("lo"), ident("hi")]), "⟨lo, hi⟩"),
        // membership / quantifier / negation (Unicode)
        (Expr::InExpr(boxed(ident("lo")), boxed(ident("hi"))), "lo ∈ hi"),
        (Expr::Not(boxed(ident("inner"))), "¬(inner)"),
        (Expr::Forall(vec!["var".to_string()], boxed(ident("src")), boxed(ident("body"))),
            "∀ var ∈ src : body"),
        (Expr::Exists(vec!["a".to_string(), "b".to_string()], boxed(ident("s")), boxed(ident("p"))),
            "∃ (a, b) ∈ s : p"),
        // nested field + index
        (field(ident("state"), "dots"), "state.dots"),
        (index(field(ident("state"), "dots"), ident("i")), "state.dots[i]"),
        // cardinality
        (Expr::Cardinality(boxed(ident("items"))), "#items"),
        // ternary
        (Expr::Ternary(boxed(ident("c")), boxed(ident("a")), boxed(ident("b"))), "(c ? a : b)"),
        // matches + its pattern, with binds
        (Expr::Matches(boxed(ident("e")), MatchPattern::Ctor {
            name: "ECall".to_string(),
            binds: vec![MatchPattern::Bind("nm".to_string()), MatchPattern::Wildcard],
        }), "(e matches ECall(nm, _))"),
        (Expr::Matches(boxed(ident("e")), MatchPattern::Wildcard), "(e matches _)"),
        // match with arms (` ⇒ ` and ` | ` — formerly diverged on the glyph)
        (Expr::Match(boxed(ident("e")), vec![
            MatchArm {
                pattern: MatchPattern::Ctor { name: "Ok".to_string(),
                    binds: vec![MatchPattern::Bind("n".to_string())] },
                body: boxed(ident("n")),
            },
            MatchArm { pattern: MatchPattern::Wildcard, body: boxed(ident("fallback")) },
        ]), "match e { Ok(n) ⇒ n | _ ⇒ fallback }"),
        // run(...)
        (Expr::RunFsm { fsm: "walk".to_string(), init: boxed(ident("root")) }, "run(walk, root)"),
        // range over identifier bounds
        (Expr::Range(boxed(ident("lo")), boxed(ident("hi"))), "{lo..hi}"),
        // a deep mix
        (call("f", vec![
            index(field(ident("state"), "dots"), ident("i")),
            bin(BinOp::Add, ident("a"), ident("b")),
        ]), "f(state.dots[i], a + b)"),
    ];

    for (node, want) in &cases {
        assert_eq!(&p.expr(node), want, "expr mismatch for {node:?}");
    }
}

// ─────────────────────────────────────────────────────────────────────
// 2. Body items — including the now-faithful Unicode shapes
// ─────────────────────────────────────────────────────────────────────

#[test]
fn body_item_renders_match_expected() {
    let p = pretty();
    let cases: Vec<(BodyItem, &str)> = vec![
        (BodyItem::Passthrough("Foo".to_string()), "..Foo"),
        (BodyItem::Passthrough("LineReader".to_string()), "..LineReader"),
        (BodyItem::ClaimCall { name: "valid_conf".to_string(), mappings: vec![] }, "valid_conf"),
        (BodyItem::Constraint(ident("counter")), "counter"),
        (BodyItem::Constraint(bin(BinOp::Lt, ident("counter"), ident("limit"))), "counter < limit"),
        (BodyItem::Constraint(call("f", vec![ident("x"), ident("y")])), "f(x, y)"),
        // Unicode shapes — formerly diverged (#16)
        (BodyItem::Membership {
            name: "x".to_string(), type_name: "Int".to_string(), pins: Pins::None,
        }, "x ∈ Int"),
        (BodyItem::ClaimCall {
            name: "manage_event".to_string(),
            mappings: vec![Mapping { slot: "schedule".to_string(), value: ident("assignments") }],
        }, "manage_event (schedule ↦ assignments)"),
    ];

    for (item, want) in &cases {
        assert_eq!(&p.body_item(item), want, "body_item mismatch for {item:?}");
    }
}

#[test]
fn impl_name_is_evident() {
    use evident_runtime::portable::Portable;
    assert_eq!(pretty().impl_name(), "evident");
}

// ─────────────────────────────────────────────────────────────────────
// 3. The one documented residual: EReal
// ─────────────────────────────────────────────────────────────────────
//
// Z3 has no real→string op matching Rust's shortest-round-trip f64 Display
// (`Z3_mk_int_to_str` is integers only; an exact Z3 rational can't reproduce
// "3.14"). `EReal` therefore renders to the `<real>` sentinel. This is
// harmless: no `.ev` source in the repo uses a real literal. See
// COUNTEREXAMPLES.md #16. If a real→string capability ever lands, promote
// this into the faithful set above.

#[test]
fn real_renders_sentinel() {
    let p = pretty();
    assert_eq!(p.expr(&Expr::Real(3.14)), "<real>");
    // The structure AROUND a real is still faithful — only the real itself
    // is a sentinel.
    assert_eq!(p.expr(&call("f", vec![ident("x"), Expr::Real(2.0)])), "f(x, <real>)");
}
