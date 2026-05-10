//! Integration tests for the `match` expression.
//!
//! Coverage:
//!   - Int / String / Bool / Real result types
//!   - Nullary variants
//!   - Wildcard arm
//!   - Bindings discarded with `_`
//!   - UNSAT when match value pinned to wrong branch result
//!
//! v1 limitation: scrutinee must be a bare Identifier whose env entry
//! is a `Var::EnumVar`. Payload bindings restricted to Int / Bool /
//! String / Real (enum-typed payloads must use `_`).

use evident_runtime::{EvidentRuntime, Value};

#[test]
fn match_int_payload_picks_arm() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Result = Ok(Int) | Err(String)

claim t
    r ∈ Result
    score ∈ Int
    r = Ok(7)
    score = match r
        Ok(n)  ⇒ n * 10
        Err(_) ⇒ 0
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("score"), Some(&Value::Int(70)));
}

#[test]
fn match_string_payload_picks_other_arm() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Result = Ok(Int) | Err(String)

claim t
    r ∈ Result
    score ∈ Int
    r = Err(\"boom\")
    score = match r
        Ok(n)  ⇒ n * 10
        Err(_) ⇒ -99
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("score"), Some(&Value::Int(-99)));
}

#[test]
fn match_string_result() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Result = Ok(Int) | Err(String)

claim t
    r ∈ Result
    label ∈ String
    r = Ok(99)
    label = match r
        Ok(_)  ⇒ \"ok\"
        Err(_) ⇒ \"err\"
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    match r.bindings.get("label") {
        Some(Value::Str(s)) => assert_eq!(s, "ok"),
        other => panic!("expected Str(\"ok\"), got {other:?}"),
    }
}

#[test]
fn match_nullary_with_wildcard() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Day = Mon | Tue | Wed | Thu | Fri | Sat | Sun

claim t
    today ∈ Day
    busy ∈ Bool
    today = Mon
    busy = match today
        Sat ⇒ false
        Sun ⇒ false
        _   ⇒ true
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("busy"), Some(&Value::Bool(true)));
}

#[test]
fn match_int_payload_with_binding_used_in_arithmetic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Result = Ok(Int) | Err(String)

claim t
    r ∈ Result
    out ∈ Int
    r = Ok(5)
    out = match r
        Ok(n)  ⇒ n + 100
        Err(_) ⇒ 0
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("out"), Some(&Value::Int(105)));
}

#[test]
fn match_pinned_to_wrong_branch_is_unsat() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Result = Ok(Int) | Err(String)

claim t
    r ∈ Result
    score ∈ Int
    r = Ok(7)
    score = match r
        Ok(n)  ⇒ n * 10
        Err(_) ⇒ 0
    score = 99
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(!r.satisfied);
}

/// v1 limitation: a multi-line `match` can't sit inside `(...)`
/// because the lexer suppresses Newline tokens inside paren groups
/// — the indented arms collapse onto one line and the parser's
/// "expected newline + indented arms" check fires. Workaround:
/// extract the match into its own equation, then use the bound name.
#[test]
fn match_into_intermediate_then_arithmetic() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("\
enum Result = Ok(Int) | Err(String)

claim t
    r ∈ Result
    score ∈ Int
    total ∈ Int
    r = Ok(3)
    score = match r
        Ok(n)  ⇒ n
        Err(_) ⇒ 0
    total = score + 10
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("total"), Some(&Value::Int(13)));
}
