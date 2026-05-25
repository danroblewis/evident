//! Correctness of the self-hosted generic monomorphization
//! (`stdlib/passes/generics.ev` + `runtime/src/portable/generics.rs`), the
//! SOLE implementation since session REVIVE-generics — the canonical Rust
//! `monomorphize_generics` is deleted.
//!
//! There is no Rust pass left to cross-validate against (the prior
//! `generics_equivalence.rs`), so this pins the monomorphized output as
//! DIRECT expectations on the generic-using corpus shapes: which concrete
//! `SchemaDecl` copies materialize (`Edge<Int>`, `Edge<Rect>`, …), their
//! substituted bodies, and the fixed-point + error behavior. It exercises
//! all four self-hosted halves end to end:
//!   * WALK     — `generics_walk` finds the type-position strings.
//!   * PARSE    — `split_head` splits `"Edge<Rect>"` → head + arg.
//!   * SUBST    — `subst_one` rewrites `"Seq(T)"` → `"Seq(Rect)"`.
//!   * CONSTRUCT— the shim splices substituted bodies onto clones to a
//!                fixed point, preserving `param_count` (GAPB).
//!
//! The real stdlib files (`toposort.ev`, `combinatorics.ev`) are covered
//! end-to-end by `runtime/tests/toposort.rs` and the `cargo test --test
//! demos` run, both of which load through this same production path.

use std::collections::HashMap;
use std::path::Path;

use evident_runtime::ast::{BodyItem, Expr, Keyword, Pins, SchemaDecl};
use evident_runtime::portable::generics::EvidentGenerics;
use evident_runtime::portable::Portable;

const STDLIB: &str = "../stdlib";

fn engine() -> EvidentGenerics {
    EvidentGenerics::new(Path::new(STDLIB)).expect("load stdlib/passes/generics.ev")
}

// ── AST builders ──────────────────────────────────────────────────────

fn schema(keyword: Keyword, name: &str, type_params: Vec<&str>, body: Vec<BodyItem>, param_count: usize) -> SchemaDecl {
    SchemaDecl {
        keyword,
        name: name.to_string(),
        type_params: type_params.into_iter().map(|s| s.to_string()).collect(),
        body,
        param_count,
        external: false,
    }
}
fn member(name: &str, type_name: &str) -> BodyItem {
    BodyItem::Membership { name: name.to_string(), type_name: type_name.to_string(), pins: Pins::None }
}
fn constraint(e: Expr) -> BodyItem { BodyItem::Constraint(e) }
fn ident(n: &str) -> Expr { Expr::Identifier(n.to_string()) }
fn call(n: &str, args: Vec<Expr>) -> Expr { Expr::Call(n.to_string(), args) }

/// Run monomorphization on a fixture set; return (schemas, order).
fn mono(decls: Vec<SchemaDecl>) -> (HashMap<String, SchemaDecl>, Vec<String>) {
    let mut schemas: HashMap<String, SchemaDecl> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for d in decls {
        order.push(d.name.clone());
        schemas.insert(d.name.clone(), d);
    }
    engine().monomorphize(&mut schemas, &mut order).expect("monomorphize");
    (schemas, order)
}

/// The Membership type_names of a schema's body, in order.
fn member_types(s: &SchemaDecl) -> Vec<String> {
    s.body.iter().filter_map(|b| match b {
        BodyItem::Membership { type_name, .. } => Some(type_name.clone()),
        _ => None,
    }).collect()
}

// ── Identity ──────────────────────────────────────────────────────────

#[test]
fn impl_name_is_evident() {
    assert_eq!(engine().impl_name(), "evident");
}

// ── A single direct generic use: `Edge<Int>` ─────────────────────────

#[test]
fn edge_int_materializes_with_substituted_body() {
    // type Edge<T>(from, to ∈ T) ; claim user { e ∈ Edge<Int> }
    let edge = schema(Keyword::Type, "Edge", vec!["T"],
                      vec![member("from", "T"), member("to", "T")], 2);
    let user = schema(Keyword::Claim, "user", vec![], vec![member("e", "Edge<Int>")], 0);
    let (schemas, _) = mono(vec![edge, user]);

    let m = schemas.get("Edge<Int>").expect("Edge<Int> materialized");
    // PARSE + SUBST: T ↦ Int in both Memberships.
    assert_eq!(member_types(m), vec!["Int", "Int"]);
    // CONSTRUCT: name renamed, type_params cleared, param_count preserved (GAPB).
    assert_eq!(m.name, "Edge<Int>");
    assert!(m.type_params.is_empty());
    assert_eq!(m.param_count, 2);
    // The generic template is kept (queried as monomorphic copies separately).
    assert!(schemas.contains_key("Edge"));
}

// ── Two distinct args produce two distinct copies ────────────────────

#[test]
fn edge_int_and_rect_are_separate_copies() {
    let edge = schema(Keyword::Type, "Edge", vec!["T"],
                      vec![member("from", "T"), member("to", "T")], 2);
    let rect = schema(Keyword::Type, "Rect", vec![],
                      vec![member("w", "Int"), member("h", "Int")], 2);
    let user = schema(Keyword::Claim, "user", vec![],
                      vec![member("a", "Edge<Int>"), member("b", "Edge<Rect>")], 0);
    let (schemas, _) = mono(vec![edge, rect, user]);

    assert_eq!(member_types(schemas.get("Edge<Int>").unwrap()), vec!["Int", "Int"]);
    assert_eq!(member_types(schemas.get("Edge<Rect>").unwrap()), vec!["Rect", "Rect"]);
}

// ── Seq wrapper + nested generic + fixed point ───────────────────────

#[test]
fn seq_wrapper_drives_nested_fixed_point() {
    // type Edge<T>(from, to ∈ T)
    // claim Holder<T> { items ∈ Seq(Edge<T>) }
    // claim user { h ∈ Holder<Int> }
    let edge = schema(Keyword::Type, "Edge", vec!["T"],
                      vec![member("from", "T"), member("to", "T")], 2);
    let holder = schema(Keyword::Claim, "Holder", vec!["T"],
                        vec![member("items", "Seq(Edge<T>)")], 1);
    let user = schema(Keyword::Claim, "user", vec![], vec![member("h", "Holder<Int>")], 0);
    let (schemas, _) = mono(vec![edge, holder, user]);

    // Holder<Int>: SUBST through the Seq wrapper → Seq(Edge<Int>).
    assert_eq!(member_types(schemas.get("Holder<Int>").unwrap()), vec!["Seq(Edge<Int>)"]);
    // Fixed point: the new Seq(Edge<Int>) drove Edge<Int> in a later pass.
    assert_eq!(member_types(schemas.get("Edge<Int>").unwrap()), vec!["Int", "Int"]);
    // Edge<T> also materializes (identity subst T↦T) from Holder<T>'s body.
    assert!(schemas.contains_key("Edge<T>"));
}

// ── Generic use found in a constraint Call name (positional invocation) ─

#[test]
fn generic_call_name_in_constraint_is_monomorphized() {
    let edge = schema(Keyword::Type, "Edge", vec!["T"],
                      vec![member("from", "T"), member("to", "T")], 2);
    // claim user { _ : Edge<Int>(a, b) }  — positional generic invocation.
    let user = schema(Keyword::Claim, "user", vec![],
                      vec![constraint(call("Edge<Int>", vec![ident("a"), ident("b")]))], 0);
    let (schemas, _) = mono(vec![edge, user]);
    assert!(schemas.contains_key("Edge<Int>"));
    assert_eq!(member_types(schemas.get("Edge<Int>").unwrap()), vec!["Int", "Int"]);
}

// ── A bare identifier spelled like a generic collects NOTHING ────────

#[test]
fn identifier_lookalike_is_not_monomorphized() {
    let edge = schema(Keyword::Type, "Edge", vec!["T"],
                      vec![member("from", "T"), member("to", "T")], 2);
    // The identifier "Edge<Int>" (not a type-position name) must NOT create
    // a copy — the WALK only collects the four type-position kinds.
    let user = schema(Keyword::Claim, "user", vec![],
                      vec![constraint(ident("Edge<Int>"))], 0);
    let (schemas, _) = mono(vec![edge, user]);
    assert!(!schemas.contains_key("Edge<Int>"),
            "a bare identifier must not trigger monomorphization");
}

// ── Error: type arguments on a non-generic type ──────────────────────

#[test]
fn type_args_on_non_generic_is_error() {
    // `Foo` has no type_params but is used as `Foo<Bar>`.
    let foo = schema(Keyword::Type, "Foo", vec![], vec![member("x", "Int")], 1);
    let user = schema(Keyword::Claim, "user", vec![], vec![member("f", "Foo<Bar>")], 0);

    let mut schemas: HashMap<String, SchemaDecl> = HashMap::new();
    let mut order = Vec::new();
    for d in [foo, user] { order.push(d.name.clone()); schemas.insert(d.name.clone(), d); }
    let err = engine().monomorphize(&mut schemas, &mut order).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("isn't declared as generic"), "got: {msg}");
}

// ── Error: wrong number of type arguments ────────────────────────────

#[test]
fn wrong_arg_count_is_error() {
    // Pair<A, B> used with a single arg.
    let pair = schema(Keyword::Type, "Pair", vec!["A", "B"],
                      vec![member("fst", "A"), member("snd", "B")], 2);
    let user = schema(Keyword::Claim, "user", vec![], vec![member("p", "Pair<Int>")], 0);

    let mut schemas: HashMap<String, SchemaDecl> = HashMap::new();
    let mut order = Vec::new();
    for d in [pair, user] { order.push(d.name.clone()); schemas.insert(d.name.clone(), d); }
    let err = engine().monomorphize(&mut schemas, &mut order).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("expects 2 type argument(s), got 1"), "got: {msg}");
}

// ── Multi-param generic: both params substituted ─────────────────────

#[test]
fn multi_param_generic_substitutes_each() {
    let pair = schema(Keyword::Type, "Pair", vec!["A", "B"],
                      vec![member("fst", "A"), member("snd", "B")], 2);
    let user = schema(Keyword::Claim, "user", vec![], vec![member("p", "Pair<Int, String>")], 0);
    let (schemas, _) = mono(vec![pair, user]);
    let m = schemas.get("Pair<Int, String>").expect("Pair<Int, String> materialized");
    assert_eq!(member_types(m), vec!["Int", "String"]);
}

// ── No generics → no-op (and engine still loads cleanly) ─────────────

#[test]
fn non_generic_program_is_unchanged() {
    let a = schema(Keyword::Type, "Point", vec![], vec![member("x", "Int"), member("y", "Int")], 2);
    let b = schema(Keyword::Claim, "user", vec![], vec![member("p", "Point")], 0);
    let (schemas, _) = mono(vec![a, b]);
    // Exactly the two input schemas, nothing added.
    assert_eq!(schemas.len(), 2);
    assert!(schemas.contains_key("Point") && schemas.contains_key("user"));
}
