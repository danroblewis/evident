//! Round-trip test for the AST encoder + decoder.
//!
//! Pipeline under test:
//!   1. Parse user source → Rust ast::Program
//!   2. Encode Program → Z3 Datatype value (encode_ast.rs)
//!   3. Run a query that binds `output ∈ Program; output = <encoded>`
//!   4. Read back the model's `output` value as Value::Enum
//!   5. Decode Value::Enum → Rust ast::Program (decode_ast.rs)
//!   6. Assert structural equality with the original
//!
//! If this passes, the decoder is correct enough to support
//! self-hosted desugar passes (which would replace step 3's
//! identity with a real transformation).

use std::path::Path;
use evident_runtime::EvidentRuntime;
use evident_runtime::translate::ast_decoder;

const STDLIB_AST: &str = "../stdlib/ast.ev";

/// Helper: load a user source string, encode its Program, run a
/// trivial pass `output = <encoded program>`, decode the model's
/// `output` binding, return the decoded Program.
fn round_trip(user_src: &str) -> evident_runtime::ast::Program {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.mark_system_loads_complete();
    rt.load_source(user_src).unwrap();

    // Build a synthetic claim that ties `output ∈ Program` to the
    // encoded user Program. Then read the model's `output`.
    let prog_value = rt.encode_program_value().unwrap();
    let synth_claim = "claim _identity_round_trip\n    output ∈ Program\n";
    rt.load_source(synth_claim).unwrap();
    let r = rt.query_with_program_value(
        "_identity_round_trip", "output", prog_value,
    ).unwrap();
    assert!(r.satisfied,
        "identity round-trip should be SAT — output = encoded program");
    let bound = r.bindings.get("output")
        .expect("model should bind `output`");
    ast_decoder::decode_program(bound)
        .expect("decoder should reconstruct a Program from the bound value")
}

/// Compare two Programs for structural equality on the parts the
/// encoder/decoder cover (schemas + enums; not imports/traces/shaders
/// which the encoder doesn't pass through stdlib/ast.ev).
fn assert_program_eq(actual: &evident_runtime::ast::Program,
                     expected_subset: &evident_runtime::ast::Program) {
    assert_eq!(actual.schemas.len(), expected_subset.schemas.len(),
        "schema count mismatch:\n  actual: {} schemas\n  expected: {} schemas",
        actual.schemas.len(), expected_subset.schemas.len());
    for (a, e) in actual.schemas.iter().zip(expected_subset.schemas.iter()) {
        assert_eq!(a.name, e.name, "schema name mismatch");
        assert_eq!(format!("{:?}", a.keyword), format!("{:?}", e.keyword),
            "keyword mismatch for `{}`", a.name);
        assert_eq!(a.body.len(), e.body.len(),
            "body length mismatch for `{}`", a.name);
        // Body items: compare via Debug repr — exact structural eq.
        for (ai, ei) in a.body.iter().zip(e.body.iter()) {
            assert_eq!(format!("{:?}", ai), format!("{:?}", ei),
                "body item mismatch in `{}`", a.name);
        }
    }
    assert_eq!(actual.enums.len(), expected_subset.enums.len());
    for (a, e) in actual.enums.iter().zip(expected_subset.enums.iter()) {
        assert_eq!(a.name, e.name);
        assert_eq!(format!("{:?}", a.variants), format!("{:?}", e.variants));
    }
}

#[test]
fn roundtrip_minimal_membership() {
    let src = "claim t\n    x ∈ Int\n";
    let decoded = round_trip(src);

    // Build the expected Program independently.
    let mut expected = evident_runtime::ast::Program::default();
    expected.schemas.push(evident_runtime::ast::SchemaDecl {
        keyword: evident_runtime::ast::Keyword::Claim,
        name: "t".into(),
        body: vec![
            evident_runtime::ast::BodyItem::Membership {
                name: "x".into(),
                type_name: "Int".into(),
                pins: evident_runtime::ast::Pins::None,
            },
        ],
    });

    assert_program_eq(&decoded, &expected);
}

#[test]
fn roundtrip_membership_plus_constraint() {
    let src = "claim t\n    x ∈ Int\n    x = 5\n";
    let decoded = round_trip(src);

    assert_eq!(decoded.schemas.len(), 1);
    let s = &decoded.schemas[0];
    assert_eq!(s.name, "t");
    assert_eq!(s.body.len(), 2);
    // body[0] = Membership(x, Int, None)
    match &s.body[0] {
        evident_runtime::ast::BodyItem::Membership { name, type_name, .. } => {
            assert_eq!(name, "x");
            assert_eq!(type_name, "Int");
        }
        other => panic!("expected Membership, got {other:?}"),
    }
    // body[1] = Constraint(Binary(Eq, Identifier("x"), Int(5)))
    match &s.body[1] {
        evident_runtime::ast::BodyItem::Constraint(e) => {
            match e {
                evident_runtime::ast::Expr::Binary(op, lhs, rhs) => {
                    assert!(matches!(op, evident_runtime::ast::BinOp::Eq));
                    assert!(matches!(lhs.as_ref(),
                        evident_runtime::ast::Expr::Identifier(s) if s == "x"));
                    assert!(matches!(rhs.as_ref(),
                        evident_runtime::ast::Expr::Int(5)));
                }
                other => panic!("expected Binary(Eq, ...), got {other:?}"),
            }
        }
        other => panic!("expected Constraint, got {other:?}"),
    }
}

#[test]
fn roundtrip_user_enum_decl() {
    let src = "enum Color = Red | Green | Blue\n";
    let decoded = round_trip(src);
    // Find the user's Color enum (stdlib's enums are also present).
    let color = decoded.enums.iter().find(|e| e.name == "Color")
        .expect("user-declared Color enum should round-trip");
    let names: Vec<&str> = color.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["Red", "Green", "Blue"]);
    assert!(color.variants.iter().all(|v| v.fields.is_empty()),
        "all variants are nullary");
}

#[test]
fn roundtrip_payload_enum() {
    let src = "enum Result = Ok(Int) | Err(String)\n";
    let decoded = round_trip(src);
    let result = decoded.enums.iter().find(|e| e.name == "Result")
        .expect("user-declared Result enum should round-trip");
    assert_eq!(result.variants.len(), 2);
    assert_eq!(result.variants[0].name, "Ok");
    assert_eq!(result.variants[0].fields.len(), 1);
    assert_eq!(result.variants[0].fields[0].type_name, "Int");
    assert_eq!(result.variants[1].name, "Err");
    assert_eq!(result.variants[1].fields[0].type_name, "String");
}

#[test]
fn roundtrip_quantifier_expression() {
    let src = "\
claim t
    s ∈ Seq(Int)
    #s = 3
    ∀ i ∈ {0..2} : s[i] > 0
";
    let decoded = round_trip(src);
    let s = &decoded.schemas[0];
    // Find the EForall in the body — it's the third item.
    let forall_item = &s.body[2];
    match forall_item {
        evident_runtime::ast::BodyItem::Constraint(e) => {
            match e {
                evident_runtime::ast::Expr::Forall(vars, _, _) => {
                    assert_eq!(vars, &vec!["i".to_string()]);
                }
                other => panic!("expected Forall, got {other:?}"),
            }
        }
        other => panic!("expected Constraint, got {other:?}"),
    }
}

#[test]
fn roundtrip_nested_call_args() {
    // Tests that decode_expr_list + decode_expr nest correctly for
    // EBinary inside a record literal (ECall with Expr args).
    let src = "\
type Point
    x ∈ Int
    y ∈ Int

claim t
    p ∈ Point
    p = Point(3, 4)
";
    let decoded = round_trip(src);
    // The `p = Point(3, 4)` constraint should round-trip. Find it.
    let claim_t = decoded.schemas.iter().find(|s| s.name == "t").unwrap();
    let pin_constraint = claim_t.body.iter().find(|i| matches!(
        i, evident_runtime::ast::BodyItem::Constraint(_)
    )).expect("expected the p = ... constraint");
    if let evident_runtime::ast::BodyItem::Constraint(
        evident_runtime::ast::Expr::Binary(_, _, rhs)
    ) = pin_constraint {
        match rhs.as_ref() {
            evident_runtime::ast::Expr::Call(name, args) => {
                assert_eq!(name, "Point");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }
}

#[test]
fn roundtrip_pins_named_form() {
    let src = "\
type Vec2
    x ∈ Int
    y ∈ Int

claim t
    v ∈ Vec2 (x ↦ 1, y ↦ 2)
";
    let decoded = round_trip(src);
    let claim_t = decoded.schemas.iter().find(|s| s.name == "t").unwrap();
    let memb = &claim_t.body[0];
    match memb {
        evident_runtime::ast::BodyItem::Membership { pins, .. } => {
            match pins {
                evident_runtime::ast::Pins::Named(maps) => {
                    assert_eq!(maps.len(), 2);
                    assert_eq!(maps[0].slot, "x");
                    assert_eq!(maps[1].slot, "y");
                }
                other => panic!("expected Named pins, got {other:?}"),
            }
        }
        other => panic!("expected Membership, got {other:?}"),
    }
}
