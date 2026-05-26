//! Correctness of the self-hosted Seq(Effect) ordering-chain extraction — the
//! runtime's SOLE chain-extraction implementation since session PORT-seqchains.
//!
//! The canonical Rust walk (`effect_loop::seq_chains::extract_seq_effect_chains`
//! + its `node_name` matcher) is now deleted, so there is no oracle to compare
//! against: this test pins the EXPECTED chains for a corpus of bodies covering
//! every `node_name` shape the deleted walk recognized — bare identifiers,
//! synthetic `name[i]` index nodes, synthetic `outer[i].field[j]` field-index
//! nodes — plus the all-elements-resolve gate and body-order preservation.
//!
//! A regression in `stdlib/passes/seq_chains.ev` (the FSM walk), the shared
//! marshaler it consumes, or the Rust-side `node_name` resolution surfaces here.
//!
//! Three concerns:
//!   1. `corpus_resolved_chains` — pinned resolved chains across the corpus,
//!      via the production entry `seq_chains::extract_seq_effect_chains`.
//!   2. `raw_chains_via_engine` — the FSM's raw `Expr` output (body order,
//!      element order), via an explicitly-constructed engine.
//!   3. `cache_*` — the per-body cache returns identical results and is keyed
//!      by body, not shared across distinct bodies.

use std::collections::HashSet;
use std::path::Path;

use evident_runtime::ast::{BinOp, BodyItem, Expr, Pins};
use evident_runtime::portable::seq_chains::{self, EvidentSeqChains, SeqChainsImpl};
use evident_runtime::portable::Portable;

const STDLIB: &str = "../stdlib";

// ── AST builders ──────────────────────────────────────────────────────

fn ident(s: &str) -> Expr { Expr::Identifier(s.to_string()) }

/// `base[i]` — an `Index` over an `Int` literal.
fn idx(base: Expr, i: i64) -> Expr {
    Expr::Index(Box::new(base), Box::new(Expr::Int(i)))
}

/// `outer[i].field[j]` — the synthetic `Seq(Composite-with-Seq-Effect-field)`
/// element shape (e.g. `plat_effs[0].effs[0]`).
fn field_idx(outer: &str, i: i64, field: &str, j: i64) -> Expr {
    idx(Expr::Field(Box::new(idx(ident(outer), i)), field.to_string()), j)
}

fn seqlit(items: Vec<Expr>) -> Expr { Expr::SeqLit(items) }

/// `lhs = rhs` body constraint.
fn eq(lhs: Expr, rhs: Expr) -> BodyItem {
    BodyItem::Constraint(Expr::Binary(BinOp::Eq, Box::new(lhs), Box::new(rhs)))
}

/// `lhs < rhs` body constraint — recognized by `node_name`? No: only `=`.
fn lt(lhs: Expr, rhs: Expr) -> BodyItem {
    BodyItem::Constraint(Expr::Binary(BinOp::Lt, Box::new(lhs), Box::new(rhs)))
}

fn membership(name: &str, ty: &str) -> BodyItem {
    BodyItem::Membership { name: name.to_string(), type_name: ty.to_string(), pins: Pins::None }
}

fn set<'a>(names: &'a [String]) -> HashSet<&'a String> {
    names.iter().collect()
}

fn chains(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect()
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ── One corpus case: a body + its effect-node-set + expected resolved chains ──
struct Case {
    name:     &'static str,
    body:     Vec<BodyItem>,
    nodes:    Vec<String>,
    expected: Vec<Vec<String>>,
}

fn corpus() -> Vec<Case> {
    vec![
        // Bare-identifier ordering declaration, SeqLit on the rhs.
        Case { name: "ident_chain_rhs",
               body: vec![eq(ident("xs"), seqlit(vec![ident("a"), ident("b"), ident("c")]))],
               nodes: names(&["a", "b", "c"]),
               expected: chains(&[&["a", "b", "c"]]) },
        // SeqLit on the lhs is recognized too.
        Case { name: "ident_chain_lhs",
               body: vec![eq(seqlit(vec![ident("a"), ident("b")]), ident("xs"))],
               nodes: names(&["a", "b"]),
               expected: chains(&[&["a", "b"]]) },
        // Synthetic `name[i]` index nodes (dispatch bundle elements).
        Case { name: "index_synthetic",
               body: vec![eq(ident("hat_effs"),
                             seqlit(vec![idx(ident("hat_effs"), 0), idx(ident("hat_effs"), 1)]))],
               nodes: names(&["hat_effs[0]", "hat_effs[1]"]),
               expected: chains(&[&["hat_effs[0]", "hat_effs[1]"]]) },
        // Synthetic `outer[i].field[j]` field-index nodes.
        Case { name: "field_index_synthetic",
               body: vec![eq(ident("phase"),
                             seqlit(vec![field_idx("plat_effs", 0, "effs", 0),
                                         field_idx("plat_effs", 0, "effs", 1)]))],
               nodes: names(&["plat_effs[0].effs[0]", "plat_effs[0].effs[1]"]),
               expected: chains(&[&["plat_effs[0].effs[0]", "plat_effs[0].effs[1]"]]) },
        // One unresolved element drops the WHOLE chain (not a clean ordering).
        Case { name: "partial_unresolved_dropped",
               body: vec![eq(ident("xs"), seqlit(vec![ident("a"), ident("zzz")]))],
               nodes: names(&["a"]),
               expected: vec![] },
        // A non-SeqLit equality contributes nothing.
        Case { name: "non_seqlit_eq",
               body: vec![eq(ident("x"), Expr::Int(5))],
               nodes: names(&[]),
               expected: vec![] },
        // A non-`=` binary over a SeqLit contributes nothing.
        Case { name: "non_eq_binary",
               body: vec![lt(ident("x"), seqlit(vec![ident("a")]))],
               nodes: names(&["a"]),
               expected: vec![] },
        // Non-constraint body items are inert; the SeqLit constraint still fires.
        Case { name: "membership_inert",
               body: vec![membership("hat_effs", "Seq(Effect)"),
                          eq(ident("p"), seqlit(vec![ident("a")]))],
               nodes: names(&["a"]),
               expected: chains(&[&["a"]]) },
        // Two chains come out in BODY order (the shim reverses the FSM's
        // newest-first accumulator).
        Case { name: "two_chains_body_order",
               body: vec![eq(ident("p"), seqlit(vec![ident("a")])),
                          eq(ident("q"), seqlit(vec![ident("b")]))],
               nodes: names(&["a", "b"]),
               expected: chains(&[&["a"], &["b"]]) },
        // A dropped chain doesn't disturb the body order of the survivors.
        Case { name: "mixed_resolved_and_dropped",
               body: vec![eq(ident("p"), seqlit(vec![ident("a"), ident("b")])),
                          eq(ident("q"), seqlit(vec![ident("c"), ident("nope")]))],
               nodes: names(&["a", "b", "c"]),
               expected: chains(&[&["a", "b"]]) },
        // An empty body yields no chains.
        Case { name: "empty_body",
               body: vec![],
               nodes: names(&[]),
               expected: vec![] },
    ]
}

// ── 1. Corpus — pinned resolved chains via the production entry ──

#[test]
fn corpus_resolved_chains() {
    seq_chains::reset_cache();
    // Build and HOLD every case so each body has a distinct live pointer (the
    // cache key), avoiding intra-test allocator reuse.
    let cases = corpus();
    let mut checked = 0;
    for c in &cases {
        let got = seq_chains::extract_seq_effect_chains(&c.body, &set(&c.nodes));
        assert_eq!(got, c.expected,
            "case `{}`:\n  expected {:?}\n  got      {:?}", c.name, c.expected, got);
        checked += 1;
    }
    assert!(checked >= 10, "expected ≥10 corpus cases; checked {checked}");
}

// ── 2. The FSM's raw Expr output, via an explicit engine ──

#[test]
fn raw_chains_via_engine() {
    // `Expr` is not `PartialEq`; compare the Debug rendering. This pins that
    // the FSM emits chains in BODY order with element order preserved, BEFORE
    // any `node_name` resolution.
    let eng = EvidentSeqChains::new(Path::new(STDLIB))
        .expect("load stdlib/passes/seq_chains.ev");
    let body = vec![
        eq(ident("p"), seqlit(vec![ident("a"), ident("b")])),
        membership("x", "Int"),
        eq(seqlit(vec![ident("c")]), ident("q")),
    ];
    let got = eng.raw_chains(&body);
    let want = vec![
        vec![ident("a"), ident("b")],
        vec![ident("c")],
    ];
    assert_eq!(format!("{got:?}"), format!("{want:?}"),
        "raw chains should be body-order, element-order-preserving");
}

// ── 3. Cache behavior ──

#[test]
fn cache_is_idempotent_per_body() {
    seq_chains::reset_cache();
    let body = vec![eq(ident("p"), seqlit(vec![ident("a"), ident("b")]))];
    let nodes = names(&["a", "b"]);
    let a = seq_chains::extract_seq_effect_chains(&body, &set(&nodes));
    let b = seq_chains::extract_seq_effect_chains(&body, &set(&nodes));  // cache hit
    assert_eq!(a, b);
    assert_eq!(a, chains(&[&["a", "b"]]));
}

#[test]
fn cache_distinguishes_distinct_bodies() {
    seq_chains::reset_cache();
    // Two distinct, simultaneously-live bodies must not share a cache entry.
    let body1 = vec![eq(ident("p"), seqlit(vec![ident("a")]))];
    let body2 = vec![eq(ident("q"), seqlit(vec![ident("x"), ident("y")]))];
    let n1 = names(&["a"]);
    let n2 = names(&["x", "y"]);
    let c1 = seq_chains::extract_seq_effect_chains(&body1, &set(&n1));
    let c2 = seq_chains::extract_seq_effect_chains(&body2, &set(&n2));
    assert_eq!(c1, chains(&[&["a"]]));
    assert_eq!(c2, chains(&[&["x", "y"]]));
    // Re-querying body1 still returns body1's chains (not body2's).
    assert_eq!(seq_chains::extract_seq_effect_chains(&body1, &set(&n1)), chains(&[&["a"]]));
}

// ── 4. Trivial sanity — impl-name plumbing ──

#[test]
fn impl_name_is_evident() {
    let eng = EvidentSeqChains::new(Path::new(STDLIB)).expect("load pass");
    assert_eq!(eng.impl_name(), "evident");
}

// A Mario-phase_chain-shaped body: a 40-element ordering chain over
// synthetic field-index nodes + 30 inert items, mimicking a real claim —
// confirms the stack-FSM walk handles a long chain + a cache re-query.
#[test]
fn walks_large_chain_and_caches() {
    seq_chains::reset_cache();
    let mut chain_elems = Vec::new();
    let mut nodes = Vec::new();
    for i in 0..40 {
        chain_elems.push(field_idx("plat_effs", i, "effs", 0));
        nodes.push(format!("plat_effs[{i}].effs[0]"));
    }
    let mut body = vec![eq(ident("phase_chain"), seqlit(chain_elems))];
    for k in 0..30 { body.push(membership(&format!("v{k}"), "Int")); }
    let s = set(&nodes);

    let cold = seq_chains::extract_seq_effect_chains(&body, &s);
    assert_eq!(cold.len(), 1);
    assert_eq!(cold[0].len(), 40);
    // A re-query (warm cache hit) returns the identical chain.
    assert_eq!(seq_chains::extract_seq_effect_chains(&body, &s), cold);
}
