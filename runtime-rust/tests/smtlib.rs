//! SMT-LIB import/export tests.
//!
//! Three categories: (1) export shape — given Evident source,
//! verify the SMT-LIB output contains the expected forms; (2)
//! import shape — given SMT-LIB text, verify the resulting Evident
//! body items are correct; (3) roundtrip — Evident → SMT-LIB →
//! Evident solves to the same SAT/UNSAT as the original.

use evident_runtime::ast::{BinOp, BodyItem, Expr};
use evident_runtime::smtlib;
use evident_runtime::EvidentRuntime;
use std::collections::HashMap;

fn first_claim_named(rt: &EvidentRuntime, name: &str) -> evident_runtime::ast::SchemaDecl {
    rt.get_schema(name).expect("claim by name").clone()
}

// ── Export tests ────────────────────────────────────────────────────────────

#[test]
fn export_basic_int_constraints() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim c\n    x ∈ Int\n    x = 5\n").unwrap();
    let s = smtlib::export(&first_claim_named(&rt, "c")).unwrap();
    assert!(s.contains("(declare-const x Int)"), "{s}");
    assert!(s.contains("(assert (= x 5))"),     "{s}");
    assert!(s.contains("(check-sat)"),          "{s}");
}

#[test]
fn export_nat_emits_nonneg_assertion() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim c\n    n ∈ Nat\n").unwrap();
    let s = smtlib::export(&first_claim_named(&rt, "c")).unwrap();
    assert!(s.contains("(declare-const n Int)"), "{s}");
    assert!(s.contains("(assert (>= n 0))"),     "{s}");
}

#[test]
fn export_pos_emits_positive_assertion() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim c\n    n ∈ Pos\n").unwrap();
    let s = smtlib::export(&first_claim_named(&rt, "c")).unwrap();
    assert!(s.contains("(assert (> n 0))"), "{s}");
}

#[test]
fn export_logical_ops() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim c\n    a ∈ Bool\n    b ∈ Bool\n    a ∧ b\n").unwrap();
    let s = smtlib::export(&first_claim_named(&rt, "c")).unwrap();
    assert!(s.contains("(assert (and a b))"), "{s}");
}

#[test]
fn export_implication() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim c\n    a ∈ Bool\n    b ∈ Bool\n    a ⇒ b\n").unwrap();
    let s = smtlib::export(&first_claim_named(&rt, "c")).unwrap();
    assert!(s.contains("(assert (=> a b))"), "{s}");
}

#[test]
fn export_unsupported_type_errors() {
    // Sub-record types aren't in v1 scope.
    let mut rt = EvidentRuntime::new();
    rt.load_source("type Pair\n    a ∈ Int\n    b ∈ Int\n\
                    claim c\n    p ∈ Pair\n").unwrap();
    let err = smtlib::export(&first_claim_named(&rt, "c")).unwrap_err();
    assert!(format!("{err}").contains("not in scope"),
        "expected scope error, got: {err}");
}

// ── Import tests ────────────────────────────────────────────────────────────

#[test]
fn import_declare_const() {
    let items = smtlib::import("(declare-const x Int)").unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        BodyItem::Membership { name, type_name, .. } => {
            assert_eq!(name, "x");
            assert_eq!(type_name, "Int");
        }
        other => panic!("expected Membership, got {other:?}"),
    }
}

#[test]
fn import_zero_arg_declare_fun() {
    let items = smtlib::import("(declare-fun y () Real)").unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        BodyItem::Membership { name, type_name, .. } => {
            assert_eq!(name, "y");
            assert_eq!(type_name, "Real");
        }
        other => panic!("expected Membership, got {other:?}"),
    }
}

#[test]
fn import_higher_arity_declare_fun_errors() {
    let err = smtlib::import("(declare-fun f (Int) Bool)").unwrap_err();
    assert!(format!("{err}").contains("higher-arity"),
        "expected higher-arity error, got: {err}");
}

#[test]
fn import_unsupported_sort_errors() {
    let err = smtlib::import("(declare-const v (_ BitVec 32))").unwrap_err();
    assert!(format!("{err}").contains("not supported")
            || format!("{err}").contains("not in scope"),
        "expected unsupported-sort error, got: {err}");
}

#[test]
fn import_assert_arithmetic() {
    let items = smtlib::import(
        "(declare-const x Int)\n\
         (assert (= (+ x 5) 10))\n").unwrap();
    assert_eq!(items.len(), 2);
    match &items[1] {
        BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
            // lhs = (+ x 5), rhs = 10
            match (&**lhs, &**rhs) {
                (Expr::Binary(BinOp::Add, _, _), Expr::Int(10)) => {}
                other => panic!("expected (+ x 5) = 10, got {other:?}"),
            }
        }
        other => panic!("expected Constraint(=), got {other:?}"),
    }
}

#[test]
fn import_drops_solver_directives() {
    let items = smtlib::import(
        "(set-logic QF_LIA)\n\
         (set-info :status sat)\n\
         (declare-const x Int)\n\
         (check-sat)\n\
         (exit)\n").unwrap();
    // Only the declare-const should survive.
    assert_eq!(items.len(), 1);
}

#[test]
fn import_drops_comments() {
    let items = smtlib::import(
        "; this is a comment\n\
         (declare-const x Int)  ; trailing comment\n\
         ; another\n").unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn import_n_ary_and() {
    // SMT-LIB allows (and a b c d). Should fold into pairwise Ands.
    let items = smtlib::import(
        "(declare-const a Bool)\n\
         (declare-const b Bool)\n\
         (declare-const c Bool)\n\
         (assert (and a b c))\n").unwrap();
    let last = items.last().unwrap();
    let BodyItem::Constraint(e) = last else { panic!("expected Constraint") };
    // Should be Binary(And, Binary(And, a, b), c) by left-fold.
    let Expr::Binary(BinOp::And, _, _) = e else {
        panic!("expected outer And, got {e:?}");
    };
}

// ── Roundtrip tests ─────────────────────────────────────────────────────────

/// Solve the original Evident program AND the roundtripped one, verify
/// they agree on SAT/UNSAT. (Bindings need not be identical — solvers
/// can pick any witness.)
fn roundtrip_solves_same(src: &str, claim_name: &str) {
    let mut rt1 = EvidentRuntime::new();
    rt1.load_source(src).expect("original parses");
    let r1 = rt1.query(claim_name, &HashMap::new()).expect("original queries");

    let smt = smtlib::export(&first_claim_named(&rt1, claim_name))
        .expect("export ok");
    let items = smtlib::import(&smt).expect("re-import ok");
    let evident_text = smtlib::body_items_to_evident("rt", &items);

    let mut rt2 = EvidentRuntime::new();
    rt2.load_source(&evident_text)
        .unwrap_or_else(|e| panic!("roundtrip failed to parse:\n{evident_text}\nerror: {e}"));
    let r2 = rt2.query("rt", &HashMap::new()).expect("roundtrip queries");

    assert_eq!(r1.satisfied, r2.satisfied,
        "SAT/UNSAT mismatch after roundtrip\n--- original ---\n{src}\n\
         --- smt ---\n{smt}\n--- roundtrip ---\n{evident_text}");
}

#[test]
fn roundtrip_sat_simple() {
    roundtrip_solves_same(
        "claim c\n    x ∈ Nat\n    x = 5\n",
        "c",
    );
}

#[test]
fn roundtrip_unsat() {
    roundtrip_solves_same(
        "claim c\n    x ∈ Nat\n    x > 100\n    x < 50\n",
        "c",
    );
}

#[test]
fn roundtrip_arithmetic() {
    roundtrip_solves_same(
        "claim c\n    x ∈ Int\n    y ∈ Int\n    x = 7\n    y = 11\n    x + y = 18\n",
        "c",
    );
}

#[test]
fn roundtrip_logical() {
    roundtrip_solves_same(
        "claim c\n    a ∈ Bool\n    b ∈ Bool\n    a = true\n    b = false\n    a ∨ b\n",
        "c",
    );
}

#[test]
fn roundtrip_bounded_forall() {
    // ∀ i ∈ {0..3} : n ≠ i, plus n ≤ 5 — only n = 4 or 5 satisfies.
    roundtrip_solves_same(
        "claim c\n    n ∈ Nat\n    n ≤ 5\n    ∀ i ∈ {0..3} : n ≠ i\n",
        "c",
    );
}

#[test]
fn roundtrip_bounded_exists() {
    // ∃ i ∈ {0..10} : i = n   — always satisfiable for n in range.
    roundtrip_solves_same(
        "claim c\n    n ∈ Nat\n    n = 7\n    ∃ i ∈ {0..10} : i = n\n",
        "c",
    );
}

#[test]
fn roundtrip_implication() {
    roundtrip_solves_same(
        "claim c\n    a ∈ Bool\n    b ∈ Bool\n    a = true\n    a ⇒ b\n    b = true\n",
        "c",
    );
}
