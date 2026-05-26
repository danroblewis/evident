//! Schema-mutation rewrites via `stdlib/passes/introspect.ev` — the first
//! AST-*rebuild* port: the FSM takes a whole `SchemaDecl` + a mutation request
//! and RETURNS the rebuilt `SchemaDecl`. The body walk + reconstruction
//! self-host; the bounds/idempotency leaves + dual-update bookkeeping stay in
//! the Rust caller (runtime/src/runtime/introspect.rs).
//!
//! Load-time tooling only (sole caller is the passthrough-desugar auto-apply
//! in commands/common.rs) — never on the per-tick, translate, or scheduler
//! path, so no hot-path or bootstrap concern. `cached_runner` is therefore
//! safe (introspect's mutations are not reachable while loading the pass).
//!
//! The shared marshaler's `SchemaDecl` is 4-field (`MakeSchemaDecl(Keyword,
//! String, Int, BodyItemList)`) and omits `type_params` / `external`. The
//! mutations never change those, and every parsed subclaim carries the
//! defaults, so taking only the rebuilt `.body` off the decoded schema —
//! and leaving the original's header fields in place — round-trips
//! byte-identically. A whole-schema replace would drop those two fields.

use super::{run_done_payload, EvidentRunner};
use crate::core::ast::{BodyItem, Pins, SchemaDecl};
use crate::core::Value;
use crate::translate::ast_decoder::decode_schema_decl;
use crate::translate::ast_encoder::{body_item_to_value, schema_decl_to_value};

cached_runner!(runner, "passes/introspect.ev", "introspect_replace");

/// Replace `s.body[idx]` with `new_item` via the Evident rebuild FSM, in place.
/// Caller guarantees `idx < s.body.len()` (the Rust bounds leaf).
pub fn replace_body_item(s: &mut SchemaDecl, idx: usize, new_item: &BodyItem) {
    let runner = runner();
    let seed = replace_req(s, idx, new_item);
    apply_rebuilt_body(&runner, "introspect_replace", seed, "RDone", s);
}

/// Insert `name ∈ type_name` (bare `Pins::None`) at the head of `s.body` via
/// the Evident prepend FSM. Caller guarantees `name` isn't already declared
/// (the Rust idempotency leaf).
pub fn prepend_membership(s: &mut SchemaDecl, name: &str, type_name: &str) {
    let runner = runner();
    let item = BodyItem::Membership {
        name: name.to_string(),
        type_name: type_name.to_string(),
        pins: Pins::None,
    };
    let seed = prepend_req(s, &item);
    apply_rebuilt_body(&runner, "introspect_prepend", seed, "PDone", s);
}

/// Drive `fsm` to its `<done>(SchemaDecl)` halt and copy the decoded body onto
/// `s`, preserving `s`'s `type_params` / `external`. On any failure leave `s`
/// untouched (the pass is proven on the corpus; a failure is a bug, surfaced
/// via eprintln — the same stance the desugar/validate cutovers take).
fn apply_rebuilt_body(
    runner: &EvidentRunner,
    fsm: &str,
    seed: Value,
    done: &str,
    s: &mut SchemaDecl,
) {
    let Some(payload) = run_done_payload(runner, fsm, seed, done, "introspect/evident") else {
        return;
    };
    match decode_schema_decl(&cons_to_seq(&payload)) {
        Ok(rebuilt) => s.body = rebuilt.body,
        Err(e) => eprintln!("[introspect/evident] {fsm} decode failed: {e}"),
    }
}

/// THE marshaler asymmetry, bridged. `schema_decl_to_value` (and the FSM, whose
/// list enums are `Nil`/`Cons` so a stack walk can pop a head) emit lists as
/// cons-list `Value::Enum` spines; the shared `decode_*` family reads the
/// `Value::SeqEnum` shape Z3 extracts from `stdlib/ast.ev`'s `Seq(T)` fields.
/// They are NOT inverses. This rewrites every AST list spine in the FSM output
/// to `SeqEnum` (or `SeqStr` for the lone `StringList`), so the decoded shape
/// matches what `decode_schema_decl` expects — without a parallel cons-list
/// decoder for the whole AST.
fn cons_to_seq(v: &Value) -> Value {
    let Value::Enum { enum_name, variant, fields } = v else {
        return v.clone();
    };
    if let Some(seq) = list_spine_to_seq(enum_name, v) {
        return seq;
    }
    Value::Enum {
        enum_name: enum_name.clone(),
        variant: variant.clone(),
        fields: fields.iter().map(cons_to_seq).collect(),
    }
}

/// If `enum_name` is one of the AST cons-list enums, walk its `Nil`/`Cons`
/// spine and return a `SeqEnum` of normalized heads — or `SeqStr` for
/// `StringList` (`EForall`/`EExists` vars, which `decode_string_list` reads as
/// `Value::SeqStr`). `None` for non-list enums.
fn list_spine_to_seq(enum_name: &str, v: &Value) -> Option<Value> {
    let (nil, cons) = match enum_name {
        "BodyItemList" => ("BILNil", "BILCons"),
        "ExprList"     => ("ELNil", "ELCons"),
        "MappingList"  => ("MLNil", "MLCons"),
        "MatchArmList" => ("MALNil", "MALCons"),
        "BindList"     => ("BLNil", "BLCons"),
        "StringList"   => ("SLNil", "SLCons"),
        _ => return None,
    };
    let mut heads = Vec::new();
    let mut cur = v;
    loop {
        let Value::Enum { variant, fields, .. } = cur else { return None };
        if variant == nil {
            break;
        }
        if variant == cons && fields.len() == 2 {
            heads.push(cons_to_seq(&fields[0]));
            cur = &fields[1];
        } else {
            return None;
        }
    }
    if enum_name == "StringList" {
        let strs: Option<Vec<String>> = heads.iter()
            .map(|h| match h { Value::Str(s) => Some(s.clone()), _ => None })
            .collect();
        return strs.map(Value::SeqStr);
    }
    Some(Value::SeqEnum(heads))
}

fn replace_req(s: &SchemaDecl, idx: usize, new_item: &BodyItem) -> Value {
    Value::Enum {
        enum_name: "ReplaceReq".to_string(),
        variant: "MakeReplaceReq".to_string(),
        fields: vec![
            schema_decl_to_value(s),
            Value::Int(idx as i64),
            body_item_to_value(new_item),
        ],
    }
}

fn prepend_req(s: &SchemaDecl, new_item: &BodyItem) -> Value {
    Value::Enum {
        enum_name: "PrependReq".to_string(),
        variant: "MakePrependReq".to_string(),
        fields: vec![schema_decl_to_value(s), body_item_to_value(new_item)],
    }
}
