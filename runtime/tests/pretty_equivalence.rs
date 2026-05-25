//! Cross-validation: the Rust `pretty` impl vs the Evident
//! `stdlib/passes/pretty.ev` impl, both reached through the
//! `portable::pretty` swap interface.
//!
//! `pretty` was the first transform ported to the portable Rust⇄Evident
//! pattern (see docs/self-hosting.md). The pass is now an **ordered
//! Emit/Expand stack-FSM** (`pretty_walk`) that routes around the
//! recursion gap (COUNTEREXAMPLES #15) exactly as `subscriptions_walk`
//! does — so `EvidentPretty` renders **recursive** sub-`Expr`s, not just
//! flat leaves. This test proves the seam end-to-end: marshal a Rust AST
//! node with the shared marshaler → drive the FSM to halt → decode the
//! String → compare against the native renderer.
//!
//! Two groups:
//!   1. **Faithful subset** — every pure-ASCII shape, RECURSIVELY (calls
//!      + arg lists, nested field/index, ternaries, `matches` + its
//!      pattern, ASCII-operator binaries with the right parenthesization).
//!      Asserts byte-identity.
//!   2. **Known boundaries** — shapes that still diverge, pinned so a
//!      future runtime fix surfaces here as a failing "still-diverges"
//!      assertion (the signal to promote them):
//!        * Unicode operator glyphs (#16) — the pass WALKS the sub-exprs
//!          (recursion routed) but the glyph bytes mangle through Z3, so
//!          the render diverges only in those bytes. We assert both the
//!          divergence AND that the operands appear (recursion is routed).
//!        * Numbers / Bool — no int→string in a pass + the JIT bool-payload
//!          bug (#17); these render to an ASCII sentinel.

use std::path::Path;

use evident_runtime::ast::{BinOp, BodyItem, Expr, Mapping, MatchArm, MatchPattern, Pins};
use evident_runtime::portable::pretty::{EvidentPretty, PrettyImpl, RustPretty};

const STDLIB: &str = "../stdlib";

fn rust() -> RustPretty { RustPretty }
fn evident() -> EvidentPretty {
    EvidentPretty::new(Path::new(STDLIB)).expect("load stdlib/passes/pretty.ev")
}

// ── helpers to build AST nodes ──
fn ident(n: &str) -> Expr { Expr::Identifier(n.to_string()) }
fn boxed(e: Expr) -> Box<Expr> { Box::new(e) }
fn bin(op: BinOp, l: Expr, r: Expr) -> Expr { Expr::Binary(op, boxed(l), boxed(r)) }
fn call(n: &str, args: Vec<Expr>) -> Expr { Expr::Call(n.to_string(), args) }
fn field(recv: Expr, f: &str) -> Expr { Expr::Field(boxed(recv), f.to_string()) }
fn index(s: Expr, i: Expr) -> Expr { Expr::Index(boxed(s), boxed(i)) }
fn passthrough(n: &str) -> BodyItem { BodyItem::Passthrough(n.to_string()) }
fn claimcall(n: &str) -> BodyItem {
    BodyItem::ClaimCall { name: n.to_string(), mappings: vec![] }
}
fn constraint(e: Expr) -> BodyItem { BodyItem::Constraint(e) }

// ─────────────────────────────────────────────────────────────────────
// 1. Faithful subset — Rust output == Evident output, RECURSIVELY
// ─────────────────────────────────────────────────────────────────────

#[test]
fn exprs_faithful_recursive_match() {
    let r = rust();
    let e = evident();

    // Each of these renders to pure ASCII at every level — the subset the
    // stack-FSM reproduces byte-for-byte, INCLUDING the recursion into
    // sub-expressions (the headline win over the pre-#15-routing port).
    let exprs: Vec<Expr> = vec![
        // leaves
        ident("x"),
        ident("counter"),
        ident("foo.bar.baz"),
        ident("state.player.pos"),
        Expr::Str("hi".to_string()),
        // ASCII binary operators — recurse into both operands
        bin(BinOp::Eq,  ident("a"), ident("b")),
        bin(BinOp::Lt,  ident("counter"), ident("limit")),
        bin(BinOp::Gt,  ident("x"), ident("y")),
        bin(BinOp::Add, ident("a"), ident("b")),
        bin(BinOp::Sub, ident("a"), ident("b")),
        bin(BinOp::Mul, ident("a"), ident("b")),
        bin(BinOp::Div, ident("a"), ident("b")),
        bin(BinOp::Concat, ident("a"), ident("b")),
        // nested binary → the inner Binary operand is parenthesized,
        // exactly as RustPretty does
        bin(BinOp::Mul, bin(BinOp::Add, ident("a"), ident("b")), ident("c")),
        bin(BinOp::Add, ident("a"), bin(BinOp::Mul, ident("b"), ident("c"))),
        // calls recurse into their argument list
        call("f", vec![ident("x"), ident("y")]),
        call("coindexed", vec![ident("a"), ident("b"), ident("c")]),
        call("g", vec![]),
        // set / tuple literals (comma-separated, recursive)
        Expr::SetLit(vec![ident("a"), ident("b"), ident("c")]),
        Expr::Tuple(vec![ident("a"), ident("b")]),
        // nested field + index
        field(ident("state"), "dots"),
        index(field(ident("state"), "dots"), ident("i")),
        index(call("f", vec![ident("x")]), ident("0")),
        // cardinality
        Expr::Cardinality(boxed(ident("items"))),
        // ternary (recursive)
        Expr::Ternary(boxed(ident("c")), boxed(ident("a")), boxed(ident("b"))),
        // matches + its (ASCII) pattern, with binds
        Expr::Matches(boxed(ident("e")), MatchPattern::Ctor {
            name: "ECall".to_string(),
            binds: vec![MatchPattern::Bind("nm".to_string()), MatchPattern::Wildcard],
        }),
        Expr::Matches(boxed(ident("e")), MatchPattern::Wildcard),
        // run(...)
        Expr::RunFsm { fsm: "walk".to_string(), init: boxed(ident("root")) },
        // range over identifier bounds
        Expr::Range(boxed(ident("lo")), boxed(ident("hi"))),
        // a deep mix: f(state.dots[i], a + b)
        call("f", vec![
            index(field(ident("state"), "dots"), ident("i")),
            bin(BinOp::Add, ident("a"), ident("b")),
        ]),
    ];

    for ex in &exprs {
        assert_eq!(r.expr(ex), e.expr(ex), "expr mismatch for {ex:?}");
    }
}

#[test]
fn body_items_faithful_subset_match() {
    let r = rust();
    let e = evident();

    let items: Vec<BodyItem> = vec![
        passthrough("Foo"),
        passthrough("LineReader"),
        claimcall("valid_conf"),       // empty mappings → just the name
        claimcall("no_conflicts"),
        constraint(ident("counter")),  // delegates to the Expr walk
        constraint(ident("state.dots")),
        // a constraint over a RECURSIVE expr — the routing reaches through
        // the BodyItem into the Expr stack-FSM
        constraint(bin(BinOp::Lt, ident("counter"), ident("limit"))),
        constraint(call("f", vec![ident("x"), ident("y")])),
    ];

    for item in &items {
        let want = r.body_item(item);
        let got = e.body_item(item);
        assert_eq!(want, got, "body_item mismatch for {item:?}");
    }
}

#[test]
fn impl_names() {
    use evident_runtime::portable::Portable;
    assert_eq!(RustPretty.impl_name(), "rust");
    assert_eq!(evident().impl_name(), "evident");
}

// ─────────────────────────────────────────────────────────────────────
// 2. Known boundaries — recursion routed, but the render still diverges
// ─────────────────────────────────────────────────────────────────────
//
// If a future runtime change closes a gap, the Evident output will start
// matching the Rust output and the `assert_ne!` will fail — that's the
// signal to promote the shape into the faithful set above.

/// Unicode operator glyphs (#16). The pass RENDERS these — it walks the
/// sub-exprs and emits the real glyph — but Z3 byte-string handling
/// mangles the glyph, so the bytes diverge there and only there. We pin
/// (a) the divergence and (b) that the operands still appear, proving the
/// recursion gap is routed even for the glyph shapes.
#[test]
fn unicode_glyph_shapes_diverge_but_recurse() {
    let r = rust();
    let e = evident();

    // (rust_expected, node, operand fragments) — each operand renders to
    // ASCII so we can assert it survives into the Evident render, proving
    // the recursion is routed even though the glyph bytes mangle. The
    // operands here are deliberately multi-char (not single letters that
    // could collide with the mangled glyph's escape text).
    let cases: Vec<(&str, Expr, Vec<&str>)> = vec![
        ("lo ≠ hi",         bin(BinOp::Neq, ident("lo"), ident("hi")),      vec!["lo", "hi"]),
        ("lo ≤ hi",         bin(BinOp::Le,  ident("lo"), ident("hi")),      vec!["lo", "hi"]),
        ("lo ≥ hi",         bin(BinOp::Ge,  ident("lo"), ident("hi")),      vec!["lo", "hi"]),
        ("lo ∧ hi",         bin(BinOp::And, ident("lo"), ident("hi")),      vec!["lo", "hi"]),
        ("lo ∨ hi",         bin(BinOp::Or,  ident("lo"), ident("hi")),      vec!["lo", "hi"]),
        ("lo ⇒ hi",         bin(BinOp::Implies, ident("lo"), ident("hi")),  vec!["lo", "hi"]),
        ("⟨lo, hi⟩",        Expr::SeqLit(vec![ident("lo"), ident("hi")]),   vec!["lo", "hi"]),
        ("lo ∈ hi",         Expr::InExpr(boxed(ident("lo")), boxed(ident("hi"))), vec!["lo", "hi"]),
        ("¬(inner)",        Expr::Not(boxed(ident("inner"))),               vec!["inner"]),
        ("∀ var ∈ src : body",
            Expr::Forall(vec!["var".to_string()], boxed(ident("src")), boxed(ident("body"))),
            vec!["var", "src", "body"]),
    ];

    for (want, node, frags) in &cases {
        let got_r = r.expr(node);
        assert_eq!(&got_r, want, "RustPretty changed for {node:?}");
        let got_e = e.expr(node);
        assert_ne!(got_e, got_r,
            "{node:?} unexpectedly faithful — promote it into the faithful set (#16 fixed?)");
        // Recursion routed: every ASCII operand made it into the render.
        for frag in frags {
            assert!(got_e.contains(frag),
                "operand {frag:?} missing from {got_e:?} — recursion NOT routed for {node:?}");
        }
    }
}

/// `EMatch` arms join with ` ⇒ ` (Unicode) — diverges on the glyph, but
/// the scrutinee, patterns, and arm bodies all render (recursion routed).
#[test]
fn match_expr_diverges_on_arrow_but_recurses() {
    let node = Expr::Match(boxed(ident("e")), vec![
        MatchArm {
            pattern: MatchPattern::Ctor { name: "Ok".to_string(),
                binds: vec![MatchPattern::Bind("n".to_string())] },
            body: boxed(ident("n")),
        },
        MatchArm { pattern: MatchPattern::Wildcard, body: boxed(ident("fallback")) },
    ]);
    let got_r = rust().expr(&node);
    assert_eq!(got_r, "match e { Ok(n) ⇒ n | _ ⇒ fallback }");
    let got_e = evident().expr(&node);
    assert_ne!(got_e, got_r, "EMatch unexpectedly faithful — promote it (#16 fixed?)");
    // Recursion routed: scrutinee, the constructor pattern, the bound
    // name, the wildcard, and both arm bodies all rendered.
    for frag in ["match e", "Ok(n)", "_", "fallback"] {
        assert!(got_e.contains(frag), "fragment {frag:?} missing from {got_e:?}");
    }
}

/// `BIMembership` renders `name ∈ type` — diverges on the ∈ glyph (#16).
#[test]
fn membership_diverges_unicode_gap() {
    let item = BodyItem::Membership {
        name: "x".to_string(),
        type_name: "Int".to_string(),
        pins: Pins::None,
    };
    let r = rust().body_item(&item);
    let e = evident().body_item(&item);
    assert_eq!(r, "x ∈ Int");
    assert_ne!(e, r, "membership unexpectedly faithful — promote it (#16 fixed?)");
    // The ASCII parts (the field name and the type) still render.
    assert!(e.contains('x') && e.contains("Int"), "operands missing from {e:?}");
}

/// A non-empty claim call needs `↦` (Unicode) — diverges on the glyph,
/// but the name, slot, and value all render (recursion routed).
#[test]
fn nonempty_claimcall_diverges_on_mapsto_but_recurses() {
    let item = BodyItem::ClaimCall {
        name: "manage_event".to_string(),
        mappings: vec![Mapping { slot: "schedule".to_string(), value: ident("assignments") }],
    };
    let r = rust().body_item(&item);
    let e = evident().body_item(&item);
    assert_eq!(r, "manage_event (schedule ↦ assignments)");
    assert_ne!(e, r, "non-empty claim call unexpectedly faithful — promote it (#16 fixed?)");
    // Recursion routed: name, slot, and value expr all rendered.
    for frag in ["manage_event", "schedule", "assignments"] {
        assert!(e.contains(frag), "fragment {frag:?} missing from {e:?}");
    }
}

/// Numbers and Bool have no faithful render in a pass (no int→string —
/// `IntToStr` is an Effect, not available under `run()`; and the JIT
/// bool-payload bug #17). They render to an ASCII sentinel.
#[test]
fn numbers_and_bool_render_to_sentinel() {
    let e = evident();
    assert_eq!(rust().expr(&Expr::Int(0)),  "0");
    assert_eq!(e.expr(&Expr::Int(0)),       "<int>");
    assert_eq!(e.expr(&Expr::Int(42)),      "<int>");
    assert_eq!(e.expr(&Expr::Real(3.14)),   "<real>");
    assert_eq!(rust().expr(&Expr::Bool(true)), "true");
    assert_eq!(e.expr(&Expr::Bool(true)),   "<bool>");
    // An expr that CONTAINS a number diverges only at the number: the
    // structure around it is still faithfully recursive.
    let node = call("f", vec![ident("x"), Expr::Int(5)]);
    assert_eq!(rust().expr(&node), "f(x, 5)");
    assert_eq!(e.expr(&node),      "f(x, <int>)");
}
