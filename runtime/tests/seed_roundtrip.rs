//! SEED-marshal: the `*_to_value` SEED marshaler must round-trip `match`
//! patterns faithfully — the symmetric completion of GAP-marshal (which
//! fixed the *Z3* encode/decode path; `marshal_roundtrip.rs`).
//!
//! The SEED marshaler (`encode_ast::*_to_value`) is what `run_nested` uses
//! to build a self-hosted pass's INPUT value. Historically it collapsed a
//! nested-ctor sub-pattern (`Node(Leaf(a), b)`) to `BindWildcard` and a
//! top-level bind arm (`other ⇒ …`) to `PatWildcard`. So an AST-rebuilding
//! pass could preserve patterns in its OUTPUT (Z3 decode was faithful) but
//! LOST them on INPUT — the last asymmetry keeping the rebuild-pass class
//! on a Rust shim.
//!
//! This pins both halves of the fix:
//!   1. **The marshaler emits the rich shape.** `body_item_list_to_value`
//!      on a body carrying `Node(Leaf(a), b)` + a top-level bind produces
//!      `BindCtor` / `PatBind` to depth — not the lossy wildcards. (RED
//!      before the marshaler fix: the seed carried `BindWildcard` /
//!      `PatWildcard`.)
//!   2. **A grown pass accepts the rich seed through a real solve.** The
//!      seed is driven through `run_nested` over an echo FSM whose runtime
//!      loads `stdlib/passes/desugar.ev` (grown in lockstep to declare
//!      `BindCtor` / `PatBind`). The pattern survives the per-tick
//!      encode→solve→decode round-trip — proving the grown enum lets
//!      `value_enum_to_datatype` encode the seed instead of silently
//!      dropping it.

use std::path::Path;

use evident_runtime::ast::{BodyItem, Expr, MatchArm, MatchPattern};
use evident_runtime::effect_loop::run_nested;
use evident_runtime::translate::ast_encoder::body_item_list_to_value;
use evident_runtime::{EvidentRuntime, Value};

const DESUGAR_PASS: &str = "../stdlib/passes/desugar.ev";

// ── AST builders ────────────────────────────────────────────────────────

fn ctor(name: &str, binds: Vec<MatchPattern>) -> MatchPattern {
    MatchPattern::Ctor { name: name.into(), binds }
}
fn bind(name: &str) -> MatchPattern { MatchPattern::Bind(name.into()) }
fn arm(pattern: MatchPattern, body: Expr) -> MatchArm {
    MatchArm { pattern, body: Box::new(body) }
}

/// A body of one constraint whose RHS is a `match` with:
///   * a NESTED-ctor arm `Node(Leaf(a), b) ⇒ a` (SHAPE B), and
///   * a TOP-LEVEL bind arm `other ⇒ 0` (SHAPE A).
/// The two shapes the old seed marshaler dropped.
fn body_with_rich_patterns() -> Vec<BodyItem> {
    let m = Expr::Match(
        Box::new(Expr::Identifier("x".into())),
        vec![
            arm(ctor("Node", vec![ctor("Leaf", vec![bind("a")]), bind("b")]),
                Expr::Identifier("a".into())),
            arm(bind("other"), Expr::Int(0)),
        ],
    );
    vec![BodyItem::Constraint(m)]
}

// ── Value-tree navigation ───────────────────────────────────────────────

fn enum_<'a>(v: &'a Value) -> (&'a str, &'a [Value]) {
    match v {
        Value::Enum { variant, fields, .. } => (variant.as_str(), fields.as_slice()),
        other => panic!("expected Value::Enum, got {other:?}"),
    }
}

/// Assert `v` is `Enum{variant}` and return its fields.
fn variant<'a>(v: &'a Value, want: &str) -> &'a [Value] {
    let (got, fields) = enum_(v);
    assert_eq!(got, want, "expected variant `{want}`, got `{got}`");
    fields
}

/// Collect every variant name appearing anywhere in the value tree.
fn collect_variants(v: &Value, out: &mut Vec<String>) {
    if let Value::Enum { variant, fields, .. } = v {
        out.push(variant.clone());
        for f in fields {
            collect_variants(f, out);
        }
    }
}

/// Navigate `BodyItemList(BILCons(BIConstraint(EMatch(_, MatchArmList)), _))`
/// to the arm cons-list, returning each arm's `MatchPattern` value.
fn arm_patterns(body_value: &Value) -> Vec<Value> {
    // BodyItemList → BILCons(item, _)
    let cons = variant(body_value, "BILCons");
    let item = &cons[0];
    // BIConstraint(expr)
    let expr = &variant(item, "BIConstraint")[0];
    // EMatch(scrutinee, MatchArmList)
    let arm_list = &variant(expr, "EMatch")[1];
    // Walk MALCons spine, pull each arm's pattern (field 0 of MakeMatchArm).
    let mut out = Vec::new();
    let mut cur = arm_list;
    loop {
        let (v, fields) = enum_(cur);
        match v {
            "MALNil" => break,
            "MALCons" => {
                let armv = &fields[0];
                let pat = &variant(armv, "MakeMatchArm")[0];
                out.push(pat.clone());
                cur = &fields[1];
            }
            other => panic!("unexpected MatchArmList variant `{other}`"),
        }
    }
    out
}

/// Walk a `BindList(BLCons|BLNil)` spine into its `MatchBind` heads.
fn bind_list(v: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let mut cur = v;
    loop {
        let (var, fields) = enum_(cur);
        match var {
            "BLNil" => break,
            "BLCons" => { out.push(fields[0].clone()); cur = &fields[1]; }
            other => panic!("unexpected BindList variant `{other}`"),
        }
    }
    out
}

// ── 1. Marshaler emits the rich shape (RED before the marshaler fix) ─────

#[test]
fn seed_marshaler_emits_bindctor_and_patbind() {
    let body = body_with_rich_patterns();
    let value = body_item_list_to_value(&body);

    let mut variants = Vec::new();
    collect_variants(&value, &mut variants);
    assert!(
        variants.iter().any(|v| v == "BindCtor"),
        "nested-ctor sub-pattern Node(Leaf(a), b) must marshal to BindCtor, \
         not collapse to BindWildcard. Variants seen: {variants:?}"
    );
    assert!(
        variants.iter().any(|v| v == "PatBind"),
        "top-level bind arm `other ⇒ 0` must marshal to PatBind, not collapse \
         to PatWildcard. Variants seen: {variants:?}"
    );
}

#[test]
fn seed_marshaler_pattern_is_byte_faithful() {
    let body = body_with_rich_patterns();
    let value = body_item_list_to_value(&body);
    let pats = arm_patterns(&value);
    assert_eq!(pats.len(), 2, "two arms");

    // arm[0]: PatCtor("Node", [ BindCtor("Leaf", [BindName("a")]), BindName("b") ])
    let node = variant(&pats[0], "PatCtor");
    assert_eq!(node[0], Value::Str("Node".into()));
    let node_binds = bind_list(&node[1]);
    assert_eq!(node_binds.len(), 2);
    // first sub-bind: BindCtor("Leaf", [BindName("a")])
    let leaf = variant(&node_binds[0], "BindCtor");
    assert_eq!(leaf[0], Value::Str("Leaf".into()));
    let leaf_binds = bind_list(&leaf[1]);
    assert_eq!(leaf_binds.len(), 1);
    let inner = variant(&leaf_binds[0], "BindName");
    assert_eq!(inner[0], Value::Str("a".into()));
    // second sub-bind: BindName("b")
    let b = variant(&node_binds[1], "BindName");
    assert_eq!(b[0], Value::Str("b".into()));

    // arm[1]: PatBind("other")
    let other = variant(&pats[1], "PatBind");
    assert_eq!(other[0], Value::Str("other".into()));
}

// ── 2. A grown pass accepts the rich seed through run_nested ─────────────

/// An echo FSM, declared atop `desugar.ev`'s (now-grown) AST enums. It
/// passes its `BodyItemList` payload through exactly one per-tick solve —
/// `EchoStart(b) → EchoEnd(b)` advances (forcing `b` through the
/// encode→solve→decode round-trip), `EchoEnd(b)` halts — so the returned
/// body has genuinely round-tripped, not just been handed back from Rust.
const ECHO_FSM: &str = "\
enum EchoState =
    EchoStart(BodyItemList)
    EchoEnd(BodyItemList)

fsm echo_body(st ∈ EchoState, halt ∈ Bool)
    st = match _st
        EchoStart(b) ⇒ EchoEnd(b)
        EchoEnd(b)   ⇒ EchoEnd(b)
    halt = match _st
        EchoEnd(_) ⇒ true
        _          ⇒ false
";

#[test]
fn rich_seed_survives_run_nested_echo() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(DESUGAR_PASS))
        .expect("load stdlib/passes/desugar.ev");
    rt.mark_system_loads_complete();
    rt.load_source(ECHO_FSM).expect("load echo FSM");

    let body = body_with_rich_patterns();
    let seed = Value::Enum {
        enum_name: "EchoState".into(),
        variant: "EchoStart".into(),
        fields: vec![body_item_list_to_value(&body)],
    };

    let final_state = run_nested(&rt, "echo_body", seed, 100)
        .expect("echo FSM should run to halt");
    // EchoEnd(b) — pull the round-tripped body out.
    let echoed_body = &variant(&final_state, "EchoEnd")[0];

    let mut variants = Vec::new();
    collect_variants(echoed_body, &mut variants);
    assert!(
        variants.iter().any(|v| v == "BindCtor"),
        "nested-ctor sub-pattern must survive the run_nested round-trip — a \
         grown pass (desugar.ev) must accept the BindCtor seed, not drop it. \
         Variants seen: {variants:?}"
    );
    assert!(
        variants.iter().any(|v| v == "PatBind"),
        "top-level bind must survive the run_nested round-trip. \
         Variants seen: {variants:?}"
    );

    // And byte-faithful: the echoed body's arm patterns match the input.
    let pats = arm_patterns(echoed_body);
    assert_eq!(pats.len(), 2);
    assert_eq!(variant(&pats[0], "PatCtor")[0], Value::Str("Node".into()));
    assert_eq!(variant(&pats[1], "PatBind")[0], Value::Str("other".into()));
}
