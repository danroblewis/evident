//! Halt detection + state Value ↔ Z3 Datatype encoding.
//!
//! Pure-data helpers used by both single-FSM and multi-FSM
//! schedulers to (a) detect when a model's state value indicates
//! the program should halt, (b) re-encode a decoded state Value
//! back into a Z3 Datatype so the next step can pin it, and
//! (c) seed a spawned FSM's state from an Int argument when the
//! state enum's first variant takes a single Int payload.

use crate::runtime::EvidentRuntime;
use crate::core::Value;
use super::fsm::MainShape;

/// Check whether a model `Value` corresponds to a halt sentinel —
/// for v1 that's any variant whose name is exactly "Done" or "Halt".
/// (Future: user-declared halt predicate.)
pub(super) fn model_matches_value(v: &Value, _state_type: &str) -> bool {
    matches!(v, Value::Enum { variant, .. } if variant == "Done" || variant == "Halt")
}

/// Re-encode a state Value as a Z3 Datatype for the next step's pin.
/// Handles nullary AND payload variants by recursively encoding
/// each field. Primitive payloads (Int, Bool, String, Real) are
/// encoded as Z3 literals; nested enum payloads recurse.
/// (Pin-readers moved to `crate::fti` — used only by FTI install.)

/// Seed a spawned FSM's state to `FirstVariant(arg)` when the
/// state enum's first variant takes a single Int payload. Used
/// by `Effect::SpawnFsm(claim, arg)` — lets the parent pass
/// an instance ID (or other Int parameter) into the spawned
/// FSM's body, which can `match state` to read it.
///
/// Returns None if the first variant doesn't have exactly one
/// Int payload (caller falls back to `seed_state`).
pub(super) fn seed_state_with_arg(
    rt: &EvidentRuntime,
    shape: &MainShape,
    arg: i64,
) -> Option<(Option<z3::ast::Datatype<'static>>, Option<Value>)> {
    let state_type = shape.state_type.as_ref()?;
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let (sort, decl_variants) = by_name.get(state_type)?;
    let first_sort = sort.variants.first()?;
    let first_decl = decl_variants.first()?;
    if first_sort.constructor.arity() != 1 { return None; }
    // Check the field type is Int. The decl_variants entry has
    // payload type info.
    if first_decl.fields.len() != 1 { return None; }
    if first_decl.fields[0].type_name != "Int" { return None; }
    // Encode `FirstVariant(arg)`.
    let value = Value::Enum {
        enum_name: state_type.clone(),
        variant:   first_decl.name.clone(),
        fields:    vec![Value::Int(arg)],
    };
    let dt = encode_state_value(rt, &value);
    Some((dt, Some(value)))
}

pub(super) fn encode_state_value(rt: &EvidentRuntime, v: &Value) -> Option<z3::ast::Datatype<'static>> {
    use z3::ast::{Int as Z3Int, Bool as Z3Bool, String as Z3Str, Dynamic, Ast};
    let Value::Enum { enum_name, variant, fields } = v else { return None };
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let (sort, _decl) = by_name.get(enum_name)?;
    let var_idx = sort.variants.iter().position(|v| v.constructor.name() == *variant)?;
    let ctor = &sort.variants[var_idx].constructor;
    if fields.is_empty() {
        return ctor.apply(&[]).as_datatype();
    }
    // Payload — encode each field as a Dynamic so vtable dispatch
    // through &dyn Ast works correctly. Earlier attempts using
    // Box<dyn Ast> ran into a Z3 null-pointer return from apply,
    // probably from variance issues with the dyn trait object.
    let ctx = rt.z3_context();
    let owned: Vec<Dynamic<'static>> = fields.iter().filter_map(|f| {
        let dyn_v: Dynamic<'static> = match f {
            Value::Int(n)  => Dynamic::from_ast(&Z3Int::from_i64(ctx, *n)),
            Value::Bool(b) => Dynamic::from_ast(&Z3Bool::from_bool(ctx, *b)),
            Value::Str(s)  => Dynamic::from_ast(&Z3Str::from_str(ctx, s).ok()?),
            Value::Real(r) => {
                let i = (*r * 1_000_000.0) as i64;
                Dynamic::from_ast(&z3::ast::Real::from_real(ctx, i as i32, 1_000_000))
            }
            Value::Enum { .. } => {
                let dt = encode_state_value(rt, f)?;
                Dynamic::from_ast(&dt)
            }
            _ => return None,
        };
        Some(dyn_v)
    }).collect();
    if owned.len() != fields.len() { return None; }
    let refs: Vec<&dyn Ast> = owned.iter().map(|v| v as &dyn Ast).collect();
    ctor.apply(&refs).as_datatype()
}
