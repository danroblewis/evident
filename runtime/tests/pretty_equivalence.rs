//! Cross-validation: the Rust `pretty` impl vs the Evident
//! `stdlib/passes/pretty.ev` impl, both reached through the
//! `portable::pretty` swap interface.
//!
//! `pretty` is the first transform ported to the portable Rust⇄Evident
//! pattern (see docs/self-hosting.md). This test proves the seam
//! end-to-end: marshal a Rust AST node → query the Evident pass → decode
//! the String → compare against the native renderer.
//!
//! `EvidentPretty` is byte-identical to `RustPretty` only on the ASCII,
//! non-recursive subset (the rest is blocked on two runtime gaps —
//! recursion and Unicode-in-string-literals — documented in
//! docs/self-hosting.md and examples/COUNTEREXAMPLES.md). The first
//! group asserts equality on that faithful subset; the second pins the
//! known boundary so a future runtime fix that closes a gap shows up
//! here as a failing "still-diverges" assertion.

use std::path::Path;

use evident_runtime::ast::{BinOp, BodyItem, Expr, Mapping, Pins};
use evident_runtime::portable::pretty::{EvidentPretty, PrettyImpl, RustPretty};

const STDLIB: &str = "../stdlib";

fn rust() -> RustPretty { RustPretty }
fn evident() -> EvidentPretty {
    EvidentPretty::new(Path::new(STDLIB)).expect("load stdlib/passes/pretty.ev")
}

// ── helpers to build AST nodes ──
fn ident(n: &str) -> Expr { Expr::Identifier(n.to_string()) }
fn passthrough(n: &str) -> BodyItem { BodyItem::Passthrough(n.to_string()) }
fn claimcall(n: &str) -> BodyItem {
    BodyItem::ClaimCall { name: n.to_string(), mappings: vec![] }
}
fn constraint(e: Expr) -> BodyItem { BodyItem::Constraint(e) }

// ── 1. Faithful subset: Rust output == Evident output ──

#[test]
fn body_items_faithful_subset_match() {
    let r = rust();
    let e = evident();

    // Each of these is ASCII, non-recursive — the subset the Evident
    // pass reproduces byte-for-byte. 10+ representative items.
    let items: Vec<BodyItem> = vec![
        passthrough("Foo"),
        passthrough("LineReader"),
        passthrough("LineWriter"),
        claimcall("valid_conf"),
        claimcall("no_conflicts"),
        claimcall("within_budget"),
        constraint(ident("counter")),
        constraint(ident("on_ground")),
        constraint(ident("halting")),
        constraint(ident("state.dots")),       // dotted identifier
        constraint(ident("world.pos")),
        constraint(ident("spawnable_only")),
    ];

    for item in &items {
        let want = r.body_item(item);
        let got = e.body_item(item);
        assert_eq!(want, got, "body_item mismatch for {item:?}");
    }
}

#[test]
fn exprs_faithful_subset_match() {
    let r = rust();
    let e = evident();
    let exprs: Vec<Expr> = vec![
        ident("x"),
        ident("counter"),
        ident("foo.bar.baz"),
        ident("state.player.pos"),
    ];
    for ex in &exprs {
        assert_eq!(r.expr(ex), e.expr(ex), "expr mismatch for {ex:?}");
    }
}

#[test]
fn impl_names() {
    use evident_runtime::portable::Portable;
    assert_eq!(RustPretty.impl_name(), "rust");
    assert_eq!(evident().impl_name(), "evident");
}

// ── 2. Known boundary: these still diverge (gap markers) ──
//
// If a future runtime change closes a gap, the Evident output will start
// matching the Rust output and these assertions will fail — that's the
// signal to promote the shape into the faithful set above.

#[test]
fn membership_diverges_unicode_gap() {
    // Rust emits the Unicode ∈; the pass can't (Z3 byte-string handling).
    let item = BodyItem::Membership {
        name: "x".to_string(),
        type_name: "Int".to_string(),
        pins: Pins::None,
    };
    let r = rust().body_item(&item);
    let e = evident().body_item(&item);
    assert_eq!(r, "x ∈ Int");
    assert_ne!(e, r, "membership unexpectedly faithful — promote it");
    assert_eq!(e, "<unsupported-membership>");
}

#[test]
fn binary_expr_diverges_recursion_gap() {
    // Rust recurses into operands; the pass can't (recursion gap).
    let item = constraint(Expr::Binary(
        BinOp::Lt,
        Box::new(ident("counter")),
        Box::new(Expr::Int(0)),
    ));
    let r = rust().body_item(&item);
    let e = evident().body_item(&item);
    assert_eq!(r, "counter < 0");
    assert_ne!(e, r, "binary expr unexpectedly faithful — promote it");
    assert_eq!(e, "<unsupported-expr>");
}

#[test]
fn nonempty_claimcall_diverges() {
    // Empty-mapping claim calls are faithful; mappings need ↦ (Unicode)
    // and recursion over the mapping list.
    let item = BodyItem::ClaimCall {
        name: "manage_event".to_string(),
        mappings: vec![Mapping { slot: "schedule".to_string(), value: ident("assignments") }],
    };
    let r = rust().body_item(&item);
    let e = evident().body_item(&item);
    assert_eq!(r, "manage_event (schedule ↦ assignments)");
    // The pass renders only the name (its BIClaimCall arm ignores ms).
    assert_eq!(e, "manage_event");
    assert_ne!(e, r);
}
