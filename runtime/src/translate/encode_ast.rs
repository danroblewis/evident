//! Z3 Datatype construction for enum-typed pin values.
//!
//! `value_enum_to_datatype` walks a `Value::Enum` and rebuilds it as a Z3
//! algebraic datatype by looking up the constructor in the `EnumRegistry`.
//! Used by `evaluate_with_extra_assertions` to pin enum-typed `given`s.

use z3::ast::{Ast, Datatype, Real};
use z3::Context;

use crate::core::EnumRegistry;
use crate::core::Value;

pub fn value_enum_to_datatype<'ctx>(
    v:     &Value,
    ctx:   &'ctx Context,
    enums: &EnumRegistry,
) -> Option<Datatype<'ctx>>
where 'ctx: 'static
{
    use z3::ast::{Bool as Z3Bool, Dynamic, Int as Z3Int};
    let Value::Enum { enum_name, variant, fields } = v else { return None };
    let by_name = enums.by_name.borrow();
    let (sort, _decl) = by_name.get(enum_name)?;
    let var_idx = sort.variants.iter()
        .position(|v| v.constructor.name() == *variant)?;
    let ctor = &sort.variants[var_idx].constructor;
    if fields.is_empty() {
        return ctor.apply(&[]).as_datatype();
    }
    drop(by_name);
    let owned: Vec<Dynamic<'static>> = fields.iter().filter_map(|f| {
        let dyn_v: Dynamic<'static> = match f {
            Value::Int(n)  => Dynamic::from_ast(&Z3Int::from_i64(ctx, *n)),
            Value::Bool(b) => Dynamic::from_ast(&Z3Bool::from_bool(ctx, *b)),
            Value::Str(s)  => Dynamic::from_ast(&crate::translate::z3_string(ctx, s).ok()?),
            Value::Real(r) => {
                let i = (*r * 1_000_000.0) as i64;
                Dynamic::from_ast(&Real::from_real(ctx, i as i32, 1_000_000))
            }
            Value::Enum { .. } => {
                let dt = value_enum_to_datatype(f, ctx, enums)?;
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
