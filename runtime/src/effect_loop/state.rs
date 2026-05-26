//! Halt detection and state Value ↔ Z3 Datatype encoding for the scheduler.

use crate::runtime::EvidentRuntime;
use crate::core::Value;
use super::fsm::MainShape;

/// True if `v` is a halt-sentinel variant (name exactly "Done" or "Halt").
pub(super) fn model_matches_value(v: &Value, _state_type: &str) -> bool {
    matches!(v, Value::Enum { variant, .. } if variant == "Done" || variant == "Halt")
}

/// Seed a spawned FSM's state to `FirstVariant(arg)` if the first variant takes one Int payload.
/// Returns None if the shape doesn't match (caller falls back to `seed_state`).
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
    if first_decl.fields.len() != 1 { return None; }
    if first_decl.fields[0].type_name != "Int" { return None; }
    let value = Value::Enum {
        enum_name: state_type.clone(),
        variant:   first_decl.name.clone(),
        fields:    vec![Value::Int(arg)],
    };
    let dt = encode_state_value(rt, &value);
    Some((dt, Some(value)))
}

pub(super) fn encode_state_value(rt: &EvidentRuntime, v: &Value) -> Option<z3::ast::Datatype<'static>> {
    use z3::ast::{Int as Z3Int, Bool as Z3Bool, Dynamic, Ast};
    let Value::Enum { enum_name, variant, fields } = v else { return None };
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let (sort, _decl) = by_name.get(enum_name)?;
    let var_idx = sort.variants.iter().position(|v| v.constructor.name() == *variant)?;
    let ctor = &sort.variants[var_idx].constructor;
    if fields.is_empty() {
        return ctor.apply(&[]).as_datatype();
    }
    // Use Dynamic (not Box<dyn Ast>): Box<dyn Ast> caused Z3 null-pointer from apply.
    let ctx = rt.z3_context();
    let owned: Vec<Dynamic<'static>> = fields.iter().filter_map(|f| {
        let dyn_v: Dynamic<'static> = match f {
            Value::Int(n)  => Dynamic::from_ast(&Z3Int::from_i64(ctx, *n)),
            Value::Bool(b) => Dynamic::from_ast(&Z3Bool::from_bool(ctx, *b)),
            Value::Str(s)  => Dynamic::from_ast(&crate::translate::z3_string(ctx, s).ok()?),
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
