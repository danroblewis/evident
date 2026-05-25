//! Cross-validation: the Rust generic-monomorphization collector vs the
//! Evident `stdlib/passes/generics.ev` collector, both reached through
//! the `portable::generics` swap interface.
//!
//! `generics` is the AST→AST monomorphization pass. Only its WALK half —
//! the traversal that locates generic-use type-position strings — is
//! self-hostable; the PARSE + SUBSTITUTE half is substring/tokenize work
//! Evident can't express (no substring/char/split op; only `=` `≠` `++`).
//! See `runtime/src/portable/generics.rs` and `docs/self-hosting.md`.
//!
//! So this test pins the swappable unit — the collector — two ways:
//!
//!   1. **`collect_uses` triple-set equality** — `RustGenerics`
//!      (canonical `collect_generic_uses`) and `EvidentGenerics`
//!      (`generics_walk` FSM) find the SAME set of generic-use triples on
//!      hand-built fixtures, the full `examples/` corpus, and the
//!      generic-heavy stdlib files (`toposort.ev`, `combinatorics.ev`).
//!
//!   2. **`monomorphize` byte-identical output** — driving the SHARED
//!      fixed-point + substitution + copy construction
//!      (`monomorphize_generics_with`) with each impl's collector produces
//!      byte-identical schema maps. This is the "byte-identical rewritten
//!      AST" check: same concrete schema copies (`Edge<Int>`,
//!      `Holder<Int>`, …), same bodies, transitively, to a fixed point.
//!
//! Because the subst + construct body is shared and only the collector is
//! swapped, (1) ⟹ (2): equal triple sets force equal rewrites. (2) is
//! included anyway as the direct end-to-end proof on real construction.

use std::collections::HashMap;
use std::path::Path;

use evident_runtime::ast::{BinOp, BodyItem, Expr, Keyword, Pins, SchemaDecl};
use evident_runtime::portable::generics::{EvidentGenerics, GenericsImpl, RustGenerics};

const STDLIB: &str = "../stdlib";

fn rust() -> RustGenerics { RustGenerics }
fn evident() -> EvidentGenerics {
    EvidentGenerics::new(Path::new(STDLIB)).expect("load stdlib/passes/generics.ev")
}

// ── AST builders ──────────────────────────────────────────────────────

fn schema(keyword: Keyword, name: &str, type_params: Vec<&str>, body: Vec<BodyItem>) -> SchemaDecl {
    SchemaDecl {
        keyword,
        name: name.to_string(),
        type_params: type_params.into_iter().map(|s| s.to_string()).collect(),
        body,
        param_count: 0,
        external: false,
    }
}

fn member(name: &str, type_name: &str) -> BodyItem {
    BodyItem::Membership { name: name.to_string(), type_name: type_name.to_string(), pins: Pins::None }
}
fn ident(n: &str) -> Expr { Expr::Identifier(n.to_string()) }
fn call(n: &str, args: Vec<Expr>) -> Expr { Expr::Call(n.to_string(), args) }
fn constraint(e: Expr) -> BodyItem { BodyItem::Constraint(e) }

// ── Comparison helpers ────────────────────────────────────────────────

/// Compare two triple lists as SETS (the canonical iterates a HashMap, so
/// order is unspecified — only the set is meaningful).
fn assert_same_uses(rust_uses: &[(String, String, String)], ev_uses: &[(String, String, String)], ctx: &str) {
    let mut r: Vec<_> = rust_uses.to_vec();
    let mut e: Vec<_> = ev_uses.to_vec();
    r.sort();
    e.sort();
    r.dedup();
    e.dedup();
    assert_eq!(r, e, "generic-use sets diverge for {ctx}\n  rust: {r:?}\n  ev:   {e:?}");
}

/// Compare two schema maps byte-identically: same key set, and each
/// SchemaDecl's `Debug` form identical (AST types derive Debug, not Eq).
fn assert_same_map(rust_map: &HashMap<String, SchemaDecl>, ev_map: &HashMap<String, SchemaDecl>, ctx: &str) {
    let mut rk: Vec<_> = rust_map.keys().cloned().collect();
    let mut ek: Vec<_> = ev_map.keys().cloned().collect();
    rk.sort();
    ek.sort();
    assert_eq!(rk, ek, "monomorphized key sets diverge for {ctx}");
    for k in &rk {
        let r = format!("{:?}", rust_map[k]);
        let e = format!("{:?}", ev_map[k]);
        assert_eq!(r, e, "monomorphized schema `{k}` diverges for {ctx}");
    }
}

// ── Identity ──────────────────────────────────────────────────────────

#[test]
fn impl_names() {
    use evident_runtime::portable::Portable;
    assert_eq!(RustGenerics.impl_name(), "rust");
    assert_eq!(evident().impl_name(), "evident");
}

// ── collect_uses: hand-built fixtures ────────────────────────────────

#[test]
fn collect_membership_passthrough_call_claimcall() {
    let ev = evident();
    // Each of the four type-position kinds the walk collects, plus an
    // identifier and an int literal that must collect NOTHING.
    let mut map: HashMap<String, SchemaDecl> = HashMap::new();
    map.insert("user".into(), schema(Keyword::Claim, "user", vec![], vec![
        member("e", "Edge<Int>"),                                   // Membership type_name
        BodyItem::Passthrough("Mixin<Rect>".into()),                // Passthrough name
        constraint(call("Toposort<Int>", vec![ident("items")])),     // Call name + ident arg (dropped)
        BodyItem::ClaimCall {                                         // ClaimCall name + mapping value
            name: "Wrap<Bool>".into(),
            mappings: vec![evident_runtime::ast::Mapping {
                slot: "x".into(),
                value: call("Pair<Int, String>", vec![]),
            }],
        },
        constraint(Expr::Binary(BinOp::Eq, Box::new(ident("a")), Box::new(Expr::Int(7)))), // nothing
    ]));
    assert_same_uses(&rust().collect_uses(&map), &ev.collect_uses(&map),
                     "four-kinds fixture");
}

#[test]
fn collect_nested_and_seq_wrapped() {
    let ev = evident();
    // Seq/Set wrappers and nested generic args — the parse recurses; the
    // walk must surface the OUTER string for the shared parse to descend.
    let mut map: HashMap<String, SchemaDecl> = HashMap::new();
    map.insert("nested".into(), schema(Keyword::Type, "nested", vec![], vec![
        member("xs", "Seq(Edge<Int>)"),
        member("ys", "Set(Pair<Rect, Effect>)"),
        member("z", "Edge<Pair<Int, String>>"),
    ]));
    assert_same_uses(&rust().collect_uses(&map), &ev.collect_uses(&map),
                     "nested/seq fixture");
}

#[test]
fn collect_recurses_subclaims_and_exprs() {
    let ev = evident();
    // A subclaim body + generic uses buried inside compound expressions
    // (Ternary, Match arm, SeqLit, Forall body, Field, Index).
    use evident_runtime::ast::{MatchArm, MatchPattern};
    let sub = schema(Keyword::Subclaim, "Inner", vec![], vec![
        member("p", "Edge<Bool>"),
        constraint(call("Combination<Int>", vec![])),
    ]);
    let mut map: HashMap<String, SchemaDecl> = HashMap::new();
    map.insert("outer".into(), schema(Keyword::Claim, "outer", vec![], vec![
        BodyItem::SubclaimDecl(sub),
        constraint(Expr::Ternary(
            Box::new(ident("c")),
            Box::new(call("Toposort<Int>", vec![])),
            Box::new(call("Toposort<String>", vec![])),
        )),
        constraint(Expr::SeqLit(vec![call("Holder<Rect>", vec![]), ident("noop")])),
        constraint(Expr::Match(
            Box::new(ident("state")),
            vec![
                MatchArm { pattern: MatchPattern::Ctor { name: "K".into(), binds: vec![] },
                           body: Box::new(call("Wrap<Int>", vec![])) },
                MatchArm { pattern: MatchPattern::Wildcard,
                           body: Box::new(ident("x")) },
            ],
        )),
        constraint(Expr::Forall(
            vec!["i".into()],
            Box::new(Expr::Range(Box::new(Expr::Int(0)), Box::new(Expr::Int(3)))),
            Box::new(call("Permutation<Real>", vec![ident("s")])),
        )),
    ]));
    assert_same_uses(&rust().collect_uses(&map), &ev.collect_uses(&map),
                     "subclaim/compound-expr fixture");
}

#[test]
fn collect_identifier_and_runfsm_collect_nothing() {
    let ev = evident();
    // An identifier spelled like a generic, and a RunFsm whose init looks
    // generic — neither is a type position, so neither is collected. Both
    // impls return the empty set.
    let mut map: HashMap<String, SchemaDecl> = HashMap::new();
    map.insert("none".into(), schema(Keyword::Claim, "none", vec![], vec![
        constraint(ident("Edge<Int>")),
        constraint(Expr::RunFsm { fsm: "f".into(), init: Box::new(call("Edge<Rect>", vec![])) }),
    ]));
    let r = rust().collect_uses(&map);
    assert!(r.is_empty(), "canonical should collect nothing here, got {r:?}");
    assert_same_uses(&r, &ev.collect_uses(&map), "identifier/runfsm fixture");
}

// ── monomorphize: byte-identical rewritten AST (real construction) ────

#[test]
fn monomorphize_simple_and_transitive() {
    let ev = evident();
    // Generic templates: Edge<T> and Holder<T> (which references Edge<T>).
    // A user references Holder<Int> + Edge<Int> directly. Expanding
    // Holder<Int> introduces Seq(Edge<Int>) → forces Edge<Int> on the next
    // fixed-point iteration. Exercises collect → parse → subst → construct
    // → iterate, end to end.
    let edge = schema(Keyword::Type, "Edge", vec!["T"], vec![
        member("from", "T"),
        member("to", "T"),
    ]);
    let holder = schema(Keyword::Claim, "Holder", vec!["T"], vec![
        member("xs", "Seq(Edge<T>)"),
        constraint(call("Edge<T>", vec![ident("a"), ident("b")])),
    ]);
    let user = schema(Keyword::Claim, "user", vec![], vec![
        member("h", "Holder<Int>"),
        member("e", "Edge<Int>"),
    ]);

    let base: HashMap<String, SchemaDecl> = [
        ("Edge".to_string(), edge),
        ("Holder".to_string(), holder),
        ("user".to_string(), user),
    ].into_iter().collect();
    let order = vec!["Edge".to_string(), "Holder".to_string(), "user".to_string()];

    let mut r_map = base.clone();
    let mut r_order = order.clone();
    rust().monomorphize(&mut r_map, &mut r_order).expect("rust monomorphize");

    let mut e_map = base.clone();
    let mut e_order = order.clone();
    ev.monomorphize(&mut e_map, &mut e_order).expect("evident monomorphize");

    // Both produced the same concrete copies (Edge<Int>, Holder<Int>).
    assert!(r_map.contains_key("Edge<Int>"), "expected Edge<Int> to be materialized");
    assert!(r_map.contains_key("Holder<Int>"), "expected Holder<Int> to be materialized");
    assert_same_map(&r_map, &e_map, "transitive-monomorphization fixture");
    // The order vectors must also match (same schemas appended, same order
    // since the shared loop appends in collector-set order — but compare
    // as sets to be robust to HashMap iteration).
    let mut ro = r_order.clone(); ro.sort();
    let mut eo = e_order.clone(); eo.sort();
    assert_eq!(ro, eo, "schema_order diverges after monomorphization");
}

// ── Loaded corpus: walk every real generic-bearing body ──────────────

/// Files known to exercise generics richly, loaded explicitly so the test
/// has signal even if `examples/` carries no generics.
const GENERIC_STDLIB: &[&str] = &["toposort.ev", "combinatorics.ev"];

#[test]
fn corpus_collect_uses_agree() {
    let ev = evident();
    let mut checked_nonempty = 0usize;

    // Generic-heavy stdlib files: load each, compare collect_uses on the
    // (post-monomorphization) schema map. These bodies contain Edge<Int>,
    // Toposort<Int>, Permutation<Int>, Edge<String>, … — real shapes.
    for file in GENERIC_STDLIB {
        let mut rt = evident_runtime::EvidentRuntime::new();
        rt.load_file(&Path::new(STDLIB).join(file))
            .unwrap_or_else(|e| panic!("load stdlib/{file}: {e}"));
        let map = rt.schemas_map().clone();
        let r = rust().collect_uses(&map);
        let e = ev.collect_uses(&map);
        assert_same_uses(&r, &e, &format!("stdlib/{file}"));
        if !r.is_empty() { checked_nonempty += 1; }
    }
    assert!(checked_nonempty == GENERIC_STDLIB.len(),
            "expected every generic stdlib file to yield ≥1 use; got {checked_nonempty}");

    // The full examples/ corpus — breadth across every shape the parser
    // emits. Most carry no generics (empty set on both), which still
    // exercises the walk over real bodies and asserts both impls agree.
    let examples_dir = Path::new("../examples");
    let mut paths: Vec<_> = std::fs::read_dir(examples_dir)
        .expect("read examples/")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|s| s.to_str())
            .map(|n| n.starts_with("test_") && n.ends_with(".ev"))
            .unwrap_or(false))
        .collect();
    paths.sort();

    let mut files_checked = 0usize;
    for path in paths {
        let mut rt = evident_runtime::EvidentRuntime::new();
        if rt.load_file(&path).is_err() { continue; }  // skip files needing FFI/etc.
        let map = rt.schemas_map().clone();
        let r = rust().collect_uses(&map);
        let e = ev.collect_uses(&map);
        assert_same_uses(&r, &e, &format!("{}", path.display()));
        files_checked += 1;
    }
    assert!(files_checked >= 20, "only checked {files_checked} example files — corpus shrunk?");
}
