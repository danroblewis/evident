//! Reading model values back out of a satisfied Z3 solver. Each
//! function maps one variable kind (or one composite element) to its
//! `Value`. Also `assert_seq_given` for the inverse: pinning a Seq
//! variable to a `Value::Seq*` shape from a `given` map.

use std::collections::HashMap;
use z3::ast::{Array, Ast, Bool, Int, String as Z3Str};
use z3::{Context, DatatypeSort};

use super::types::{FieldKind, SeqElem, Value, Var};

/// Read a Seq value out of the model: read the length, then read each
/// `arr.select(i)` for i ∈ 0..length and assemble into the right
/// `Value::Seq*` variant. Indices past the length are unconstrained
/// in Z3 (Arrays are total functions); we just don't read them.
pub(super) fn extract_seq<'ctx>(
    arr: &Array<'ctx>,
    len: &Int<'ctx>,
    elem: SeqElem,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    match elem {
        SeqElem::Int => {
            let mut out = Vec::with_capacity(n as usize);
            for i in 0..n {
                let idx = Int::from_i64(ctx, i);
                let v = arr.select(&idx).as_int()?;
                out.push(model.eval(&v, true)?.as_i64()?);
            }
            Some(Value::SeqInt(out))
        }
        SeqElem::Bool => {
            let mut out = Vec::with_capacity(n as usize);
            for i in 0..n {
                let idx = Int::from_i64(ctx, i);
                let v = arr.select(&idx).as_bool()?;
                out.push(model.eval(&v, true)?.as_bool()?);
            }
            Some(Value::SeqBool(out))
        }
        SeqElem::Str => {
            let mut out = Vec::with_capacity(n as usize);
            for i in 0..n {
                let idx = Int::from_i64(ctx, i);
                let v = arr.select(&idx).as_string()?;
                out.push(model.eval(&v, true)?.as_string()?);
            }
            Some(Value::SeqStr(out))
        }
    }
}

/// Walk the accessors of a single Datatype value and assemble a flat
/// `HashMap<String, Value>` of its fields. Recurses for nested
/// composite fields: a `FieldKind::Nested` yields a `Value::Composite`
/// whose own map is built by another call to this helper on the
/// nested `(dt, sub_fields)` pair.
///
/// Caller is responsible for ensuring `dt` and `fields` were built
/// together (same order). The accessor index aligns with `fields[i]`.
pub(super) fn extract_composite_value<'ctx>(
    elem: &z3::ast::Datatype<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'_>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<HashMap<String, Value>> {
    let mut field_map: HashMap<String, Value> = HashMap::new();
    for (fi, fk) in fields.iter().enumerate() {
        if fi >= dt.variants[0].accessors.len() { break; }
        let accessor = &dt.variants[0].accessors[fi];
        let raw = accessor.apply(&[elem]);
        let value = match fk {
            FieldKind::Primitive { prim_type, .. } => match prim_type.as_str() {
                "Int" | "Nat" | "Pos" => {
                    let z = raw.as_int()?;
                    Value::Int(model.eval(&z, true)?.as_i64()?)
                }
                "Bool" => {
                    let z = raw.as_bool()?;
                    Value::Bool(model.eval(&z, true)?.as_bool()?)
                }
                "String" => {
                    let z = raw.as_string()?;
                    Value::Str(model.eval(&z, true)?.as_string()?)
                }
                _ => return None,
            },
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                let nested_elem = raw.as_datatype()?;
                let nested_map =
                    extract_composite_value(&nested_elem, sub_fields, *nested_dt, model, ctx)?;
                Value::Composite(nested_map)
            }
        };
        field_map.insert(fk.name().to_string(), value);
    }
    Some(field_map)
}

/// Read a `Seq(UserType)` value out of the model: read the length,
/// then for each `i ∈ 0..length` select the array element (a
/// Datatype value) and call `extract_composite_value` to assemble
/// its field map. Push each element map into a `Vec` and wrap in
/// `Value::SeqComposite`.
pub(super) fn extract_seq_composite<'ctx>(
    arr: &Array<'ctx>,
    len: &Int<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'_>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    let mut out: Vec<HashMap<String, Value>> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let idx = Int::from_i64(ctx, i);
        let elem_dyn = arr.select(&idx);
        let elem = elem_dyn.as_datatype()?;
        let field_map = extract_composite_value(&elem, fields, dt, model, ctx)?;
        out.push(field_map);
    }
    Some(Value::SeqComposite(out))
}

/// Build a `Bool` constraint asserting that the named Seq variable
/// equals the given Value::Seq* (length + per-index element equality).
/// Returns None when the var/value shapes don't match — caller should
/// then warn or fall through.
///
/// Supports:
///   - Var::SeqVar (primitive elements: Int / Bool / String) +
///     Value::SeqInt / SeqBool / SeqStr
///   - Var::DatatypeSeqVar + Value::SeqComposite — builds a Datatype
///     constructor application per element from the field map's
///     primitive values (recursively for nested composites)
pub(super) fn assert_seq_given<'ctx>(
    var: &Var<'ctx>,
    value: &Value,
    ctx: &'ctx Context,
) -> Option<Bool<'ctx>> {
    match (var, value) {
        (Var::SeqVar { arr, len, elem: SeqElem::Int }, Value::SeqInt(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            for (i, n) in items.iter().enumerate() {
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx).as_int()?;
                conjuncts.push(cell._eq(&Int::from_i64(ctx, *n)));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::SeqVar { arr, len, elem: SeqElem::Bool }, Value::SeqBool(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            for (i, b) in items.iter().enumerate() {
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx).as_bool()?;
                conjuncts.push(cell._eq(&Bool::from_bool(ctx, *b)));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::SeqVar { arr, len, elem: SeqElem::Str }, Value::SeqStr(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            for (i, s) in items.iter().enumerate() {
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx).as_string()?;
                let want = Z3Str::from_str(ctx, s).ok()?;
                conjuncts.push(cell._eq(&want));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::DatatypeSeqVar { arr, len, dt, fields, .. }, Value::SeqComposite(items)) => {
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            // The Datatype has a single constructor with fields in
            // declaration order. Build an application per element.
            let ctor = &dt.variants[0].constructor;
            for (i, element) in items.iter().enumerate() {
                let mut field_dyns: Vec<z3::ast::Dynamic> = Vec::with_capacity(fields.len());
                for fk in fields.iter() {
                    let dynamic = match fk {
                        FieldKind::Primitive { name, prim_type } => {
                            let v = element.get(name)?;
                            match (prim_type.as_str(), v) {
                                ("Int" | "Nat" | "Pos", Value::Int(n)) =>
                                    z3::ast::Dynamic::from_ast(&Int::from_i64(ctx, *n)),
                                ("Bool", Value::Bool(b)) =>
                                    z3::ast::Dynamic::from_ast(&Bool::from_bool(ctx, *b)),
                                ("String", Value::Str(s)) => {
                                    let z = Z3Str::from_str(ctx, s).ok()?;
                                    z3::ast::Dynamic::from_ast(&z)
                                }
                                _ => return None,
                            }
                        }
                        FieldKind::Nested { .. } => return None, // skip for v1
                    };
                    field_dyns.push(dynamic);
                }
                let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
                let elem_ast = ctor.apply(&dyn_refs);
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx);
                conjuncts.push(cell._eq(&elem_ast));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        _ => None,
    }
}
