use evident_runtime::EvidentRuntime;

fn expect_load_err(src: &str, want_substring: &str) {
    let mut rt = EvidentRuntime::new();
    let res = rt.load_source(src);
    let err = res.expect_err(&format!(
        "expected load to fail with message containing {want_substring:?}"));
    let s = format!("{err:?}");
    assert!(
        s.contains(want_substring),
        "error message {s:?} did not contain {want_substring:?}"
    );
}

#[test]
fn duplicate_enum_name_errors() {
    expect_load_err(
        "enum Day = Mon | Tue\nenum Day = Wed | Thu\n",
        "declared more than once",
    );
}

#[test]
fn duplicate_variant_in_same_batch_errors() {
    expect_load_err(
        "enum A = X | Y\nenum B = X | Z\n",
        "declared twice",
    );
}

#[test]
fn duplicate_variant_within_one_enum_errors() {
    expect_load_err(
        "enum A = X | X\n",
        "declared twice",
    );
}

#[test]
fn unknown_payload_type_errors() {
    expect_load_err(
        "enum Foo = Bar(NotARealType)\n",
        "unknown payload type",
    );
}

#[test]
fn duplicate_variant_against_previously_loaded_errors() {
    let mut rt = EvidentRuntime::new();
    rt.load_source("enum A = Red | Green | Blue\n").unwrap();
    let err = rt.load_source("enum B = Red | Yellow\n").expect_err(
        "expected second load to fail because Red was already declared");
    let s = format!("{err:?}");
    assert!(s.contains("declared twice"),
            "unexpected error: {s:?}");
}

#[test]
fn cross_batch_payload_reference_resolves() {

    let mut rt = EvidentRuntime::new();
    rt.load_source("enum BinOp = OpAdd | OpSub\n").unwrap();
    rt.load_source("enum Expr = ELit(Int) | EOp(BinOp, Expr, Expr)\n").unwrap();

    rt.load_source("claim t\n    e ∈ Expr\n    e = EOp(OpAdd, ELit(2), ELit(3))\n").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied);
}

#[test]
fn empty_enum_errors() {

    expect_load_err("enum Empty =\n", "expected variant name");
}

#[test]
fn nullary_pre_population_doesnt_collide_with_membership_var() {

    let mut rt = EvidentRuntime::new();
    rt.load_source("enum Day = Mon | Tue\n").unwrap();

    rt.load_source("claim t\n    Mon ∈ Int\n    Mon = 5\n").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied,
        "expected user-declared Mon to shadow the Day::Mon variant; got UNSAT");
}
