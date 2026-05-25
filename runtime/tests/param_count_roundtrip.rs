//! Gap A: `SchemaDecl::param_count` round-trips through the shared
//! marshaler. `param_count` is the first-line-param insertion index
//! (`claim X(a, b)` → 2) that `inject`/`desugar` splice after; before
//! this it was dropped — `MakeSchemaDecl` had no slot, so the decoder
//! reconstructed `param_count: 0`.

use std::path::Path;
use evident_runtime::ast;
use evident_runtime::translate::ast_encoder;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::translate::ast_decoder;

const STDLIB_AST: &str = "../stdlib/ast.ev";

/// The shared `*_to_value` marshaler now CARRIES `param_count` as the
/// third `MakeSchemaDecl` field — the slot a stack-FSM inject/desugar
/// pass reads to find the insertion index. (The body is a named
/// cons-list here, distinct from the Z3-extract `SeqEnum` shape that
/// `decode_schema_decl` reads; that family split predates this change,
/// so we assert the marshaled shape directly.)
#[test]
fn param_count_carried_by_value_marshaler() {
    let sd = ast::SchemaDecl {
        keyword: ast::Keyword::Claim,
        name: "Toposort".into(),
        type_params: vec![],
        body: vec![
            ast::BodyItem::Membership { name: "n".into(),     type_name: "Nat".into(),       pins: ast::Pins::None },
            ast::BodyItem::Membership { name: "items".into(), type_name: "Seq(Int)".into(),  pins: ast::Pins::None },
            ast::BodyItem::Membership { name: "extra".into(), type_name: "Int".into(),       pins: ast::Pins::None },
        ],
        param_count: 2,   // n, items are first-line params; extra is a helper-local
        external: false,
    };

    let v = ast_encoder::schema_decl_to_value(&sd);
    let Value::Enum { enum_name, variant, fields } = &v else {
        panic!("expected Enum, got {v:?}");
    };
    assert_eq!(enum_name, "SchemaDecl");
    assert_eq!(variant, "MakeSchemaDecl");
    assert_eq!(fields.len(), 4, "kw, name, param_count, body");
    assert_eq!(fields[1], Value::Str("Toposort".into()));
    assert_eq!(fields[2], Value::Int(2),
        "param_count must be the third marshaled field (Gap A)");
}

/// param_count = 0 (no first-line params) is carried too — guards
/// against an off-by-one or a hardcoded default.
#[test]
fn zero_param_count_carried() {
    let sd = ast::SchemaDecl {
        keyword: ast::Keyword::Type,
        name: "Plain".into(),
        type_params: vec![],
        body: vec![ast::BodyItem::Membership {
            name: "x".into(), type_name: "Int".into(), pins: ast::Pins::None,
        }],
        param_count: 0,
        external: false,
    };
    let v = ast_encoder::schema_decl_to_value(&sd);
    if let Value::Enum { fields, .. } = &v {
        assert_eq!(fields[2], Value::Int(0));
    } else { panic!("expected Enum"); }
}

/// Whole-program round-trip through the Z3 `encode`/`decode` path:
/// a `claim X(a, b)` parsed (param_count = 2) recovers param_count = 2
/// after encode → solve → extract → decode.
#[test]
fn param_count_round_trips_through_z3_encode_decode() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST)).unwrap();
    rt.mark_system_loads_complete();
    // Two first-line params (a, b) + one body membership (c).
    rt.load_source("claim widget(a ∈ Int, b ∈ Int)\n    c ∈ Int\n").unwrap();

    let prog_value = rt.encode_program_value().unwrap();
    rt.load_source("claim _pc_round_trip\n    output ∈ Program\n").unwrap();
    let r = rt.query_with_program_value("_pc_round_trip", "output", prog_value).unwrap();
    assert!(r.satisfied);
    let bound = r.bindings.get("output").expect("bound output");
    let prog = ast_decoder::decode_program(bound).expect("decode_program");
    let widget = prog.schemas.iter().find(|s| s.name == "widget")
        .expect("widget schema round-trips");
    assert_eq!(widget.param_count, 2,
        "claim widget(a, b) has param_count 2; it must survive the Z3 round-trip");
    assert_eq!(widget.body.len(), 3, "a, b, c");
}
