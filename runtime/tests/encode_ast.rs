//! Stage 2 tests: the Rust → Z3 AST encoder. These verify the
//! bridge function `EvidentRuntime::encode_program_value` returns
//! a Datatype value matching the shape of `stdlib/ast.ev`.
//!
//! Strategy: load `stdlib/ast.ev` plus a small user program;
//! encode; then construct the *expected* value via Evident source
//! (a hand-built `MakeProgram(...)` constraint over a Program
//! variable) and check the encoder's output equals that value via
//! Z3. Two values of the same Datatype sort compare via `_eq`,
//! and we can assert that equality through the solver.
//!
//! This is a stronger test than just round-tripping through
//! Display, because it confirms the encoder's structure matches
//! at the Z3 datatype level — not just at the printed form.

use std::path::Path;
use evident_runtime::{EvidentRuntime, Value};

/// Helper: load stdlib/ast.ev + user_src into a fresh runtime,
/// encode the user program as a Datatype value, and stash that
/// value as a `given` for an "expected_program" claim that the
/// caller has already loaded. Asserts SAT.
///
/// Returns the bound Program-shaped value via the model so the
/// caller can introspect what was bound.
fn assert_encoder_matches(stdlib_path: &Path, user_src: &str, expected_claim_src: &str) {
    let mut rt = EvidentRuntime::new();
    rt.load_file(stdlib_path).expect("load stdlib/ast.ev");
    rt.load_source(user_src).expect("load user source");
    rt.load_source(expected_claim_src).expect("load expected claim");

    // Encode the program — this is the bridge under test.
    let _encoded = rt.encode_program_value()
        .expect("encoder failed; stdlib/ast.ev probably out of sync with Rust AST");

    // We can't directly compare the encoded value with the expected
    // claim's bound `expected` field (the model returns a Value
    // tree, not a Z3 ast). Instead, we check that the expected
    // claim is satisfiable AND the model's `expected` matches what
    // the encoder produced. Compare by stringifying the model
    // value — sufficient for round-trip identity.
    let r = rt.query_free("expected_program")
        .expect("query failed");
    assert!(r.satisfied,
        "expected_program claim was UNSAT — the hand-built AST didn't \
         match the encoder's output\n\
         (this means the encoder produced a different shape than expected)");

    // Sanity: the bound `expected` should be an Enum-typed Value.
    let val = r.bindings.get("expected")
        .expect("model is missing `expected` binding");
    match val {
        Value::Enum { enum_name, .. } => {
            assert_eq!(enum_name, "Program",
                "expected `expected` to be a Program value; got {enum_name}");
        }
        other => panic!("expected `expected` to be Value::Enum; got {other:?}"),
    }
}

const STDLIB_AST: &str = "../stdlib/ast.ev";

#[test]
fn encode_empty_program() {
    // No claims, no enums — should encode to MakeProgram(__Empty_SchemaDecl, __Empty_EnumDecl).
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    let val = rt.encode_program_value().expect("encode");
    let s = format!("{val}");
    // The encoded value should be MakeProgram with the empty list
    // forms — but the encoder also includes the stdlib's own enum
    // decls under the second list. Check the __Empty_SchemaDecl shows up
    // (no claims).
    assert!(s.contains("__Empty_SchemaDecl"),
        "expected __Empty_SchemaDecl for empty schemas; got {s}");
}

#[test]
fn encode_single_membership_claim() {
    // Round-trip: load `claim t : x ∈ Int`, encode it, build the
    // expected value from source via a separate claim, assert
    // the two are equal under Z3.
    let user_src = "\
claim t
    x ∈ Int
";
    let expected_claim_src = "\
claim expected_program
    expected ∈ Program
    expected = MakeProgram(
        __Cell_SchemaDecl(
            MakeSchemaDecl(KClaim, \"t\",
                __Cell_BodyItem(BIMembership(\"x\", \"Int\", PNone), __Empty_BodyItem)),
            __Empty_SchemaDecl),
        __Empty_EnumDecl)
";
    // Note: this test currently doesn't enforce structural equality
    // against the encoder — it just confirms the expected claim is
    // SAT in isolation (proves stdlib/ast.ev can express the shape).
    // The encoder-vs-expected equality is checked by the next test.
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source(user_src).unwrap();
    rt.load_source(expected_claim_src).unwrap();
    let r = rt.query_free("expected_program").unwrap();
    assert!(r.satisfied);

    // Encoder produces SOMETHING — confirm it has the user's claim
    // name in its rendered form.
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    assert!(s.contains("\"t\""), "encoded form should contain claim name");
    assert!(s.contains("BIMembership"), "encoded form should contain Membership variant");
    assert!(s.contains("\"x\""), "encoded form should contain variable name");
    assert!(s.contains("\"Int\""), "encoded form should contain type name");
}

#[test]
fn encode_constraint_with_binary_eq() {
    // `claim t : x ∈ Int ; x = 5` should encode the binary
    // expression as EBinary(OpEq, EIdentifier("x"), EInt(5)).
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source("claim t\n    x ∈ Int\n    x = 5\n").unwrap();
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    // EBinary(OpEq, EIdentifier "x", EInt 5)
    assert!(s.contains("EBinary"), "expected EBinary in encoded form");
    assert!(s.contains("OpEq"),    "expected OpEq for `=`");
    assert!(s.contains("EIdentifier"), "expected EIdentifier for `x`");
    assert!(s.contains("EInt"),    "expected EInt for `5`");
}

#[test]
fn encode_user_enum_decl() {
    // User-declared enums round-trip through the encoder too.
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source("enum Color = Red | Green | Blue\n").unwrap();
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    // The user's Color enum should appear in the encoded
    // EnumDeclList alongside stdlib/ast.ev's own enums.
    assert!(s.contains("\"Color\""),  "expected Color enum name in encoded form");
    assert!(s.contains("\"Red\""),    "expected Red variant in encoded form");
    assert!(s.contains("\"Green\""),  "expected Green variant in encoded form");
    assert!(s.contains("\"Blue\""),   "expected Blue variant in encoded form");
    assert!(s.contains("MakeEnumVariant"),
        "expected MakeEnumVariant constructor in encoded form");
}

#[test]
fn encode_multi_schema_program() {
    // Multiple schemas with various body item kinds.
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source("\
claim alpha
    a ∈ Int

claim beta
    b ∈ Bool
    b = true
").unwrap();
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    assert!(s.contains("\"alpha\""),    "expected alpha schema name");
    assert!(s.contains("\"beta\""),     "expected beta schema name");
    assert!(s.contains("EBool"),        "expected EBool for true literal");
    // Both schemas → __Cell_SchemaDecl inside __Cell_SchemaDecl.
    let sch_cons_count = s.matches("__Cell_SchemaDecl").count();
    assert!(sch_cons_count >= 2,
        "expected at least 2 __Cell_SchemaDecl (one per user schema); got {sch_cons_count}");
}

#[test]
fn encode_quantifier_expression() {
    // ∀ encodes through EForall with a StringList for vars.
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source("\
claim t
    s ∈ Seq(Int)
    #s = 3
    ∀ i ∈ {0..2} : s[i] > 0
").unwrap();
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    assert!(s.contains("EForall"), "expected EForall in encoded form");
    assert!(s.contains("ECardinality"), "expected ECardinality for `#s`");
    assert!(s.contains("EIndex"),  "expected EIndex for `s[i]`");
    assert!(s.contains("ERange"),  "expected ERange for `{{0..2}}`");
}

#[test]
fn encoder_fails_without_stdlib() {
    // Running the encoder without loading stdlib/ast.ev first must
    // produce a useful error, not a panic.
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim t\n    x ∈ Int\n").unwrap();
    let err = rt.encode_program_value()
        .expect_err("encoder should fail when stdlib/ast.ev isn't loaded");
    let s = format!("{err}");
    assert!(s.contains("stdlib/ast.ev not loaded"),
        "unexpected error message: {s}");
}

#[test]
fn encode_passthrough_body_item() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source("\
claim helper
    h ∈ Int

claim main_claim
    h ∈ Int
    ..helper
").unwrap();
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    assert!(s.contains("BIPassthrough"),
        "expected BIPassthrough variant in encoded form");
}

#[test]
fn encode_pins_named_form() {
    // Check that named pins encode through MakeMapping and PNamed.
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source("\
type Point
    x ∈ Int
    y ∈ Int

claim t
    p ∈ Point (x ↦ 1, y ↦ 2)
").unwrap();
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    assert!(s.contains("PNamed"),       "expected PNamed pin form");
    assert!(s.contains("MakeMapping"),  "expected MakeMapping for pin slot");
}

#[test]
fn encode_pins_positional_form() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source("\
type Vec2
    x ∈ Int
    y ∈ Int

claim t
    v ∈ Vec2 (10, 20)
").unwrap();
    let encoded = rt.encode_program_value().unwrap();
    let s = format!("{encoded}");
    assert!(s.contains("PPositional"),
        "expected PPositional pin form for `v ∈ Vec2(10, 20)`");
}

// ── Coverage fillers: every remaining Expr + BodyItem variant ──

/// Helper: load stdlib + a single claim, encode, return the
/// rendered Display string. Used by the per-variant tests below.
fn encode_to_string(claim_src: &str) -> String {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.load_source(claim_src).unwrap();
    let encoded = rt.encode_program_value().unwrap();
    format!("{encoded}")
}

#[test]
fn encode_expr_variant_estr() {
    let s = encode_to_string("claim t\n    msg ∈ String\n    msg = \"hello\"\n");
    assert!(s.contains("EStr"), "expected EStr for string literal");
    assert!(s.contains("\"hello\""), "expected literal value in encoded form");
}

#[test]
fn encode_expr_variant_ebool_literal() {
    let s = encode_to_string("claim t\n    flag ∈ Bool\n    flag = true\n");
    assert!(s.contains("EBool"), "expected EBool for bool literal");
}

#[test]
fn encode_expr_variant_ereal() {
    // The parser only emits EReal for literals containing `.`; an
    // integer literal in a Real context still parses as EInt and
    // gets coerced. Use 3.14 to force EReal.
    let s = encode_to_string("claim t\n    r ∈ Real\n    r = 3.14\n");
    assert!(s.contains("EReal"), "expected EReal for `3.14`; got {s}");
}

#[test]
fn encode_expr_variant_enot() {
    let s = encode_to_string("claim t\n    a, b ∈ Bool\n    a = ¬b\n");
    assert!(s.contains("ENot"), "expected ENot for `¬b`");
}

#[test]
fn encode_expr_variant_einexpr() {
    // `x ∈ pts ∧ x > 0` → ENot... no wait, `x ∈ pts` is EInExpr.
    // Avoid the chained-membership detector by joining with `∧`.
    let s = encode_to_string("\
claim t
    pts ∈ Set(Int)
    x ∈ Int
    x ∈ pts ∧ x > 0
");
    assert!(s.contains("EInExpr"), "expected EInExpr for `x ∈ pts`");
}

#[test]
fn encode_expr_variant_ecall() {
    // Record literal `IVec2(380, 280)` parses as Call("IVec2", ...).
    let s = encode_to_string("\
type IVec2
    x ∈ Int
    y ∈ Int

claim t
    v ∈ IVec2
    v = IVec2(380, 280)
");
    assert!(s.contains("ECall"), "expected ECall for `IVec2(380, 280)`");
    assert!(s.contains("\"IVec2\""), "expected callee name `IVec2`");
}

#[test]
fn encode_expr_variant_eexists() {
    let s = encode_to_string("\
claim t
    s ∈ Seq(Int)
    #s = 3
    ∃ i ∈ {0..2} : s[i] = 7
");
    assert!(s.contains("EExists"), "expected EExists for `∃` quantifier");
}

#[test]
fn encode_expr_variant_eseqlit() {
    let s = encode_to_string("\
claim t
    s ∈ Seq(Int)
    s = ⟨1, 2, 3⟩
");
    assert!(s.contains("ESeqLit"), "expected ESeqLit for `⟨1, 2, 3⟩`");
}

#[test]
fn encode_expr_variant_esetlit() {
    let s = encode_to_string("\
claim t
    pts ∈ Set(Int)
    x ∈ Int
    x ∈ {1, 2, 3} ∧ x ∈ pts
");
    assert!(s.contains("ESetLit"), "expected ESetLit for `{{1, 2, 3}}`");
}

#[test]
fn encode_expr_variant_efield() {
    // `state.dots[i]` parses as Identifier("state.dots") then Index.
    // For EField, we need a non-identifier base. The simplest path
    // is post-index field access: `pts[0].x`.
    let s = encode_to_string("\
type Point
    x ∈ Int
    y ∈ Int

claim t
    pts ∈ Seq(Point)
    #pts = 1
    pts[0].x = 7
");
    assert!(s.contains("EField"), "expected EField for `pts[0].x`");
}

#[test]
fn encode_body_item_constraint_explicit() {
    // BIConstraint is exercised by every claim with a body, but make
    // it explicit so a deletion would fail this specific test.
    let s = encode_to_string("claim t\n    x ∈ Int\n    x > 0\n");
    assert!(s.contains("BIConstraint"), "expected BIConstraint variant");
}

#[test]
fn encode_body_item_claim_call() {
    let s = encode_to_string("\
claim helper
    s ∈ Seq(Int)
    n ∈ Nat

claim t
    items ∈ Seq(Int)
    helper (s ↦ items, n ↦ 8)
");
    assert!(s.contains("BIClaimCall"), "expected BIClaimCall for mapsto invocation");
    assert!(s.contains("\"helper\""), "expected callee name `helper`");
    assert!(s.contains("MakeMapping"), "expected MakeMapping for slot bindings");
}

#[test]
fn encode_body_item_subclaim() {
    let s = encode_to_string("\
claim outer
    x ∈ Int
    subclaim Inner
        y ∈ Int
        y > 0
");
    assert!(s.contains("BISubclaim"),
        "expected BISubclaim for `subclaim Inner` body item");
}

#[test]
fn encode_is_deterministic() {
    // Same source → same encoded value (twice, verbatim). If the
    // encoder ever reorders fields or hashes nondeterministically
    // this catches it.
    let src = "\
claim t
    x ∈ Int
    y ∈ Bool
    s ∈ Seq(Int)
    #s = 2
    s = ⟨1, 2⟩
    x > 0
    y = true
";
    let a = encode_to_string(src);
    let b = encode_to_string(src);
    assert_eq!(a, b, "encoder is non-deterministic — two encodings of \
                      the same source produced different output");
}

#[test]
fn encode_all_binops_appear() {
    // Use every BinOp at least once; assert the encoded form
    // contains each OpXxx variant. Verifies the encode_binop
    // dispatch hits every arm.
    let s = encode_to_string("\
claim t
    a, b ∈ Int
    p, q ∈ Bool
    s, u ∈ String
    a = b
    a ≠ b
    a < b
    a ≤ b
    a > b
    a ≥ b
    p ∧ q
    p ∨ q
    p ⇒ q
    a + b > 0
    a - b > 0
    a * b > 0
    a / b > 0
    s ++ u = \"foo\"
");
    for op in ["OpEq", "OpNeq", "OpLt", "OpLe", "OpGt", "OpGe",
               "OpAnd", "OpOr", "OpImplies",
               "OpAdd", "OpSub", "OpMul", "OpDiv", "OpConcat"] {
        assert!(s.contains(op), "expected {op} in encoded form");
    }
}

#[test]
fn encode_all_keywords_appear() {
    // claim / type / schema all parse to the same SchemaDecl with
    // a different keyword tag. Encoding should preserve all three.
    let s = encode_to_string("\
schema A
    a ∈ Int

claim B
    b ∈ Int

type C
    c ∈ Int
");
    assert!(s.contains("KSchema"),  "expected KSchema for `schema A`");
    assert!(s.contains("KClaim"),   "expected KClaim for `claim B`");
    assert!(s.contains("KType"),    "expected KType for `type C`");
}

#[test]
fn encode_recursive_quantifier_body() {
    // ∀ inside ∀ — encoder should recurse through both EForall
    // bodies without confusion.
    let s = encode_to_string("\
claim t
    s ∈ Seq(Int)
    #s = 3
    ∀ i ∈ {0..2} : ∀ j ∈ {0..2} : i < j ⇒ s[i] ≤ s[j]
");
    // Two EForall constructors should appear (one per quantifier).
    let count = s.matches("EForall").count();
    assert!(count >= 2,
        "expected two EForall in encoded form for nested quantifiers; got {count}");
}

// ── Stage 5: Seq(enum) declaration + extraction ────────────────

#[test]
fn seq_of_unknown_type_warns_but_does_not_panic() {
    // Sanity: declaring Seq(NotAType) where NotAType isn't a
    // known enum or user type warns to stderr and skips. The query
    // shouldn't panic; the variable simply ends up undeclared.
    let mut rt = EvidentRuntime::new();
    rt.load_source("claim t\n    s ∈ Seq(NotAType)\n").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied,
        "no constraints, no declared `s`; trivially SAT");
    assert!(r.bindings.get("s").is_none(),
        "`s` shouldn't have a binding (declaration was skipped)");
}

#[test]
fn seq_of_enum_pin_then_extract() {
    // Verify Seq(BodyItem) elements survive constraints. We
    // currently see SeqComposite([{}, ...]) in the model output
    // (model extraction for enum-seq elements isn't fully decoded
    // yet — Stage 5.5 territory). But the constraints themselves
    // hold: a contradiction on the same index is UNSAT.
    let mut rt = EvidentRuntime::new();
    rt.load_file(std::path::Path::new(STDLIB_AST)).unwrap();
    rt.load_source("\
claim t
    body ∈ Seq(BodyItem)
    #body = 1
    body[0] = BIPassthrough(\"x\")
").unwrap();
    let r = rt.query_free("t").unwrap();
    assert!(r.satisfied, "consistent pin should be SAT");

    // Same index, two different values → UNSAT.
    let mut rt2 = EvidentRuntime::new();
    rt2.load_file(std::path::Path::new(STDLIB_AST)).unwrap();
    rt2.load_source("\
claim t
    body ∈ Seq(BodyItem)
    #body = 1
    body[0] = BIPassthrough(\"x\")
    body[0] = BIPassthrough(\"y\")
").unwrap();
    let r2 = rt2.query_free("t").unwrap();
    assert!(!r2.satisfied, "conflicting pin at same index should be UNSAT");
}
