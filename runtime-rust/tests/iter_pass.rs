//! Stage 5.5 integration tests: the iteration pass.
//!
//! Pipeline under test:
//!   1. Load stdlib/ast.ev + stdlib/passes/iter_types.ev
//!   2. mark_system_loads_complete
//!   3. Load user file
//!   4. Call query_with_program_and_body(rule, "program", "body")
//!      which encodes the program AND the user's first claim's body
//!      as a Seq(BodyItem), pinning both into the pass's vars.
//!
//! The iteration rules use ∃ over the seq indices, so the pass
//! works for any user program length — not just single-body or
//! 2-body shapes that literal_types.ev requires.

use std::path::Path;
use evident_runtime::{EvidentRuntime, Value};

const STDLIB_AST:  &str = "../stdlib/ast.ev";
const ITER_TYPES:  &str = "../stdlib/passes/iter_types.ev";

fn fresh_rt_with_iter_pass() -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_file(Path::new(ITER_TYPES)).unwrap();
    rt.mark_system_loads_complete();
    rt
}

#[test]
fn has_membership_finds_single_decl() {
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("claim t\n    x ∈ Int\n").unwrap();
    let r = rt.query_with_program_and_body(
        "has_membership_of_var", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("x".to_string())));
    assert_eq!(r.bindings.get("target_type"),
               Some(&Value::Str("Int".to_string())));
}

#[test]
fn has_membership_finds_in_third_position() {
    // The win of iteration: a 3-body program where the Membership
    // is the LAST item still gets picked up. literal_types.ev's
    // extract_first_membership requires the head; this rule
    // searches via ∃.
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim t
    a = \"hi\"
    b = 1
    score ∈ Nat
").unwrap();
    let r = rt.query_with_program_and_body(
        "has_membership_of_var", "program", "body",
    ).unwrap();
    assert!(r.satisfied,
        "iteration should find the Membership in the third position");
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("score".to_string())));
    assert_eq!(r.bindings.get("target_type"),
               Some(&Value::Str("Nat".to_string())));
}

#[test]
fn has_membership_unsat_for_constraint_only_program() {
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("claim t\n    x = 5\n").unwrap();
    let r = rt.query_with_program_and_body(
        "has_membership_of_var", "program", "body",
    ).unwrap();
    assert!(!r.satisfied,
        "no Membership in body → ∃ is UNSAT");
}

#[test]
fn has_string_assignment_finds_in_body() {
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim t
    x ∈ Int
    msg ∈ String
    msg = \"hello\"
").unwrap();
    let r = rt.query_with_program_and_body(
        "has_string_assignment", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("msg".to_string())));
    assert_eq!(r.bindings.get("string_lit"),
               Some(&Value::Str("hello".to_string())));
}

#[test]
fn has_int_assignment_iterates_past_others() {
    // Mixed bag: the Int assignment is buried in the middle.
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim t
    x ∈ Int
    msg ∈ String
    x = 42
    msg = \"hi\"
").unwrap();
    let r = rt.query_with_program_and_body(
        "has_int_assignment", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("x".to_string())));
    assert_eq!(r.bindings.get("int_lit"),
               Some(&Value::Int(42)));
}

#[test]
fn has_string_unsat_when_only_int_assigned() {
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("claim t\n    n ∈ Int\n    n = 5\n").unwrap();
    let r = rt.query_with_program_and_body(
        "has_string_assignment", "program", "body",
    ).unwrap();
    assert!(!r.satisfied);
}

#[test]
fn iteration_works_on_empty_user_program() {
    // No user claim → body has length 0 → ∃ over empty range
    // unrolls to false → UNSAT for the existential rules. Should
    // NOT panic.
    let mut rt = fresh_rt_with_iter_pass();
    let r = rt.query_with_program_and_body(
        "has_membership_of_var", "program", "body",
    ).unwrap();
    assert!(!r.satisfied,
        "empty body → no body items → ∃ is UNSAT");
}

#[test]
fn iteration_isolates_between_user_loads() {
    // First call: program has a Membership.
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("claim a\n    x ∈ Int\n").unwrap();
    let r1 = rt.query_with_program_and_body(
        "has_membership_of_var", "program", "body",
    ).unwrap();
    assert!(r1.satisfied);
    // Each call rebuilds env, so the second call sees the same
    // user state — same answer.
    let r2 = rt.query_with_program_and_body(
        "has_membership_of_var", "program", "body",
    ).unwrap();
    assert_eq!(r1.bindings.get("target_var"),
               r2.bindings.get("target_var"));
}

#[test]
fn has_bool_assignment_finds_in_body() {
    // Bool variant — wasn't in the initial test set.
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim t
    x ∈ Int
    flag ∈ Bool
    flag = true
").unwrap();
    let r = rt.query_with_program_and_body(
        "has_bool_assignment", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("flag".to_string())));
    assert_eq!(r.bindings.get("bool_lit"),
               Some(&Value::Bool(true)));
}

#[test]
fn iteration_handles_long_body() {
    // A body with 6 items — well past anything literal_types.ev can
    // pattern-match. Iteration should still find the assignment.
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim t
    a ∈ Int
    b ∈ Bool
    c ∈ String
    a = 1
    b = false
    c = \"target\"
").unwrap();
    let r = rt.query_with_program_and_body(
        "has_string_assignment", "program", "body",
    ).unwrap();
    assert!(r.satisfied,
        "iteration should find the String assignment in a 6-body program");
    assert_eq!(r.bindings.get("target_var"),
               Some(&Value::Str("c".to_string())));
    assert_eq!(r.bindings.get("string_lit"),
               Some(&Value::Str("target".to_string())));
}

#[test]
fn iteration_picks_one_of_many_matching_assignments() {
    // Multiple String assignments exist; ∃ binds to whichever Z3
    // picks first. The exact choice is a solver implementation
    // detail — we just check that bindings are consistent (the
    // bound (target_var, string_lit) match a real assignment in
    // the body).
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim t
    a = \"first\"
    b = \"second\"
    c = \"third\"
").unwrap();
    let r = rt.query_with_program_and_body(
        "has_string_assignment", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    let var = match r.bindings.get("target_var") {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("expected target_var as Str; got {other:?}"),
    };
    let lit = match r.bindings.get("string_lit") {
        Some(Value::Str(s)) => s.clone(),
        other => panic!("expected string_lit as Str; got {other:?}"),
    };
    let expected = [("a", "first"), ("b", "second"), ("c", "third")];
    let valid = expected.iter()
        .any(|(v, l)| *v == var && *l == lit);
    assert!(valid,
        "(target_var={var:?}, string_lit={lit:?}) doesn't match \
         any real assignment in the body");
}

#[test]
fn iteration_user_program_with_multiple_claims_uses_first() {
    // The runtime injects only the FIRST user claim's body. The
    // pass operates on that. Subsequent claims are visible in the
    // Program value but not in the flat body Seq.
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim alpha
    msg = \"only_in_alpha\"

claim beta
    n ∈ Int
").unwrap();
    let r = rt.query_with_program_and_body(
        "has_string_assignment", "program", "body",
    ).unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("string_lit"),
               Some(&Value::Str("only_in_alpha".to_string())),
        "iteration should find the alpha-claim's String assignment");

    // Beta's Int Membership is NOT in body (body is alpha's body).
    // Verify by trying to find a Membership — should be UNSAT.
    let r2 = rt.query_with_program_and_body(
        "has_membership_of_var", "program", "body",
    ).unwrap();
    assert!(!r2.satisfied,
        "beta's Membership is in claim 2, not the injected body");
}

#[test]
fn unknown_pass_claim_for_iter_errors() {
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("claim t\n    x ∈ Int\n").unwrap();
    let err = rt.query_with_program_and_body(
        "no_such_rule", "program", "body",
    ).expect_err("expected UnknownSchema");
    let s = format!("{err:?}");
    assert!(s.contains("no_such_rule") || s.contains("UnknownSchema"),
        "unexpected error: {s}");
}

// ── Stage 8: per-claim iteration via the runtime API ───────────

#[test]
fn nth_claim_body_reaches_each_claim() {
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("\
claim alpha
    msg = \"in_alpha\"
claim beta
    score ∈ Nat
    score = 100
").unwrap();
    assert_eq!(rt.user_claim_count(), 2);
    assert_eq!(rt.user_claim_name(0), Some("alpha".to_string()));
    assert_eq!(rt.user_claim_name(1), Some("beta".to_string()));

    // alpha's body has a string assignment; iter on body 0 finds it.
    let r0 = rt.query_with_program_and_nth_claim_body(
        "has_string_assignment", "program", "body", 0,
    ).unwrap().unwrap();
    assert!(r0.satisfied);
    assert_eq!(r0.bindings.get("string_lit"),
               Some(&Value::Str("in_alpha".to_string())));

    // beta's body has a Membership; iter on body 1 finds it.
    let r1 = rt.query_with_program_and_nth_claim_body(
        "has_membership_of_var", "program", "body", 1,
    ).unwrap().unwrap();
    assert!(r1.satisfied);
    assert_eq!(r1.bindings.get("target_var"),
               Some(&Value::Str("score".to_string())));
}

#[test]
fn nth_claim_body_returns_none_for_out_of_range() {
    let mut rt = fresh_rt_with_iter_pass();
    rt.load_source("claim only_one\n    x = 1\n").unwrap();
    let r = rt.query_with_program_and_nth_claim_body(
        "has_int_assignment", "program", "body", 5,
    ).unwrap();
    assert!(r.is_none(),
        "out-of-range claim_idx should return Ok(None)");
}

#[test]
fn user_claim_count_zero_when_no_user_loads() {
    let rt = fresh_rt_with_iter_pass();
    assert_eq!(rt.user_claim_count(), 0,
        "no user-loaded claims after mark_system_loads_complete");
}
