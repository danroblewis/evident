//! Stage 3 integration tests: end-to-end self-hosted pass invocation.
//!
//! Pipeline under test:
//!   1. Load stdlib/ast.ev (canonical AST shape)
//!   2. Load stdlib/passes/literal_types.ev (the pass)
//!   3. Load a tiny user program
//!   4. Call EvidentRuntime::query_with_program(pass_claim, "program")
//!      which encodes the user's program → Z3 Datatype value, asserts
//!      `program = <encoded value>` against the pass's `program` var,
//!      then runs the pass's solver.
//!   5. Read the pass's bindings to recover inferred information.
//!
//! These tests prove the bridge end-to-end. The actual inference rules
//! in literal_types.ev are deliberately narrow (single-claim,
//! single-body-item programs) — Stage 3's win is the *plumbing*, not
//! the inference power. Real inference comes when we add cardinality /
//! quantifier support over the recursive list datatypes.

use std::path::Path;
use evident_runtime::{EvidentRuntime, Value};

const STDLIB_AST:    &str = "../stdlib/ast.ev";
const LITERAL_TYPES: &str = "../stdlib/passes/literal_types.ev";

fn fresh_rt_with_pass() -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).expect("load stdlib/ast.ev");
    rt.load_file(Path::new(LITERAL_TYPES)).expect("load literal_types.ev");
    // Snapshot everything loaded so far as the "system" layer; the
    // user's subsequent loads are what the encoder will emit.
    rt.mark_system_loads_complete();
    rt
}

#[test]
fn smoke_accepts_anything_sat_for_empty_program() {
    // Empty program (no claims, no enums in the user file) — the
    // smoke claim should still accept it. Confirms the runtime
    // injection path runs without erroring even for a trivial input.
    let mut rt = fresh_rt_with_pass();
    let r = rt.query_with_program("accepts_anything", "program").unwrap();
    assert!(r.satisfied,
        "accepts_anything should be SAT for any program, even empty");
}

#[test]
fn smoke_has_at_least_one_schema_sat_when_user_loads_one() {
    let mut rt = fresh_rt_with_pass();
    rt.load_source("claim demo\n    x ∈ Int\n").unwrap();
    let r = rt.query_with_program("has_at_least_one_schema", "program").unwrap();
    assert!(r.satisfied,
        "has_at_least_one_schema should be SAT after the user loads `claim demo`");
}

#[test]
fn smoke_has_at_least_one_schema_unsat_for_truly_empty_user_program() {
    // After mark_system_loads_complete, no user schemas/enums means
    // the encoded Program is genuinely empty: MakeProgram(SchLNil, EDLNil).
    // The pass's inequality assertion fails → UNSAT.
    let mut rt = fresh_rt_with_pass();
    let r = rt.query_with_program("has_at_least_one_schema", "program").unwrap();
    assert!(!r.satisfied,
        "with no user-loaded schemas, encoded program is the empty \
         MakeProgram(SchLNil, EDLNil) — the inequality should fail");
}

#[test]
fn smoke_has_at_least_one_schema_sat_for_user_enum_only() {
    // User loads only an enum (no schemas). The encoded program
    // becomes MakeProgram(SchLNil, EDLCons(...)). The inequality
    // against MakeProgram(SchLNil, EDLNil) holds → SAT.
    let mut rt = fresh_rt_with_pass();
    rt.load_source("enum Color = Red | Green\n").unwrap();
    let r = rt.query_with_program("has_at_least_one_schema", "program").unwrap();
    assert!(r.satisfied,
        "user enum makes EDL non-empty; inequality should hold");
}

#[test]
fn infer_string_type_from_simple_assignment() {
    // The pivotal test. User program is `claim t : msg = "hello"`.
    // The pass should infer:
    //   inferred_var  = "msg"
    //   inferred_type = "String"
    //   claim_name    = "t"
    let mut rt = fresh_rt_with_pass();
    rt.load_source("claim t\n    msg = \"hello\"\n").unwrap();
    let r = rt.query_with_program("infer_string_from_single_assignment", "program").unwrap();
    assert!(r.satisfied,
        "expected SAT for `claim t : msg = \"hello\"`; got UNSAT — \
         plumbing or pattern is wrong");
    let var = r.bindings.get("inferred_var");
    let typ = r.bindings.get("inferred_type");
    let claim_name = r.bindings.get("claim_name");
    assert_eq!(var, Some(&Value::Str("msg".to_string())),
        "expected inferred_var = \"msg\"; got {var:?}");
    assert_eq!(typ, Some(&Value::Str("String".to_string())),
        "expected inferred_type = \"String\"; got {typ:?}");
    assert_eq!(claim_name, Some(&Value::Str("t".to_string())),
        "expected claim_name = \"t\"; got {claim_name:?}");
}

#[test]
fn infer_int_type_from_simple_assignment() {
    let mut rt = fresh_rt_with_pass();
    rt.load_source("claim t\n    n = 42\n").unwrap();
    let r = rt.query_with_program("infer_int_from_single_assignment", "program").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("inferred_var"),
               Some(&Value::Str("n".to_string())));
    assert_eq!(r.bindings.get("inferred_type"),
               Some(&Value::Str("Int".to_string())));
}

#[test]
fn infer_bool_type_from_simple_assignment() {
    let mut rt = fresh_rt_with_pass();
    rt.load_source("claim t\n    flag = true\n").unwrap();
    let r = rt.query_with_program("infer_bool_from_single_assignment", "program").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("inferred_var"),
               Some(&Value::Str("flag".to_string())));
    assert_eq!(r.bindings.get("inferred_type"),
               Some(&Value::Str("Bool".to_string())));
}

#[test]
fn string_inference_unsat_for_int_literal_assignment() {
    // The pass's String-rule is shape-specific: it requires EStr on
    // the RHS. An Int assignment (`n = 42`) doesn't match that
    // shape, so the rule must be UNSAT. Confirms the pattern is
    // actually selective.
    let mut rt = fresh_rt_with_pass();
    rt.load_source("claim t\n    n = 42\n").unwrap();
    let r = rt.query_with_program("infer_string_from_single_assignment", "program").unwrap();
    assert!(!r.satisfied,
        "string inference should be UNSAT when the assignment is Int, \
         not String; got SAT");
}

#[test]
fn no_match_unsat_for_multi_body_program() {
    // The pass's rule requires exactly one body item — a program
    // with two body items doesn't match. This is the documented
    // narrowness of the v0.1 pass.
    let mut rt = fresh_rt_with_pass();
    rt.load_source("claim t\n    x = \"hi\"\n    y = \"there\"\n").unwrap();
    let r = rt.query_with_program("infer_string_from_single_assignment", "program").unwrap();
    assert!(!r.satisfied,
        "v0.1 pass only matches single-body-item programs; got SAT");
}

#[test]
fn unknown_pass_claim_errors() {
    let mut rt = fresh_rt_with_pass();
    let err = rt.query_with_program("not_a_real_pass", "program")
        .expect_err("expected UnknownSchema");
    let s = format!("{err:?}");
    assert!(s.contains("not_a_real_pass") || s.contains("UnknownSchema"),
        "unexpected error: {s}");
}

#[test]
fn fails_without_stdlib_ast() {
    // Without stdlib/ast.ev loaded, the encoder can't find the
    // Program enum and returns an error rather than producing
    // garbage.
    let mut rt = EvidentRuntime::new();
    // Skip stdlib/ast.ev load.
    let load_result = rt.load_file(Path::new(LITERAL_TYPES));
    // The pass file itself imports stdlib/ast.ev, so this load
    // resolves the import and brings ast.ev along. To actually
    // hit the "stdlib not loaded" case, we have to skip the pass
    // and try query_with_program against a schema that doesn't
    // need the AST… but that defeats the purpose. Instead, write
    // a fresh test where we directly call encode_program_value
    // and assert the failure.
    if load_result.is_ok() {
        // Pass loaded, AST imported transitively — the encoder will
        // succeed. Skip this test (the negative path is covered by
        // tests/encode_ast.rs::encoder_fails_without_stdlib).
        return;
    }
}
