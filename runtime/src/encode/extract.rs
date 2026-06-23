use std::collections::HashMap;
use z3::ast::{Array, Ast, Bool, Int, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::core::{EnumRegistry, FieldKind, SeqElem, Value, Var};

pub(crate) fn unescape_z3_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'u' {

            if i + 2 < bytes.len() && bytes[i + 2] == b'{' {
                if let Some(close_rel) = bytes[i + 3..].iter().position(|&b| b == b'}') {
                    let hex_end = i + 3 + close_rel;
                    let hex = &s[i + 3..hex_end];
                    if !hex.is_empty() && hex.len() <= 6 {
                        if let Ok(cp) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(cp) {
                                out.push(ch);
                                i = hex_end + 1;
                                continue;
                            }
                        }
                    }
                }
            }
        }

        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

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
                out.push(unescape_z3_string(&model.eval(&v, true)?.as_string()?));
            }
            Some(Value::SeqStr(out))
        }
    }
}

pub(super) fn extract_composite_value<'ctx>(
    elem: &z3::ast::Datatype<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'_>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<HashMap<String, Value>> {
    let mut field_map: HashMap<String, Value> = HashMap::new();

    let mut acc_pos: usize = 0;
    for fk in fields.iter() {
        let value = match fk {
            FieldKind::Primitive { prim_type, .. } => {
                if acc_pos >= dt.variants[0].accessors.len() { break; }
                let raw = dt.variants[0].accessors[acc_pos].apply(&[elem]);
                acc_pos += 1;
                match prim_type.as_str() {
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
                        Value::Str(unescape_z3_string(&model.eval(&z, true)?.as_string()?))
                    }
                    _ => return None,
                }
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if acc_pos >= dt.variants[0].accessors.len() { break; }
                let raw = dt.variants[0].accessors[acc_pos].apply(&[elem]);
                acc_pos += 1;
                let nested_elem = raw.as_datatype()?;
                let nested_map =
                    extract_composite_value(&nested_elem, sub_fields, *nested_dt, model, ctx, enums)?;
                Value::Composite(nested_map)
            }
            FieldKind::SeqField { name, arr_idx, len_idx, elem: seq_elem, .. } => {
                use crate::core::SeqFieldElem;
                if *len_idx >= dt.variants[0].accessors.len() { break; }
                let arr_dyn = dt.variants[0].accessors[*arr_idx].apply(&[elem]);
                let len_dyn = dt.variants[0].accessors[*len_idx].apply(&[elem]);
                acc_pos = *len_idx + 1;
                let arr = arr_dyn.as_array()?;
                let len_z3 = len_dyn.as_int()?;
                let len = model.eval(&len_z3, true)?.as_i64()?;
                let _ = name;
                let extracted = match seq_elem {
                    SeqFieldElem::Primitive(prim) => {
                        match prim {
                            crate::core::SeqElem::Int => {
                                let mut out: Vec<i64> = Vec::with_capacity(len as usize);
                                for k in 0..len {
                                    let idx = Int::from_i64(ctx, k);
                                    let cell = arr.select(&idx).as_int()?;
                                    out.push(model.eval(&cell, true)?.as_i64()?);
                                }
                                Value::SeqInt(out)
                            }
                            crate::core::SeqElem::Bool => {
                                let mut out: Vec<bool> = Vec::with_capacity(len as usize);
                                for k in 0..len {
                                    let idx = Int::from_i64(ctx, k);
                                    let cell = arr.select(&idx).as_bool()?;
                                    out.push(model.eval(&cell, true)?.as_bool()?);
                                }
                                Value::SeqBool(out)
                            }
                            crate::core::SeqElem::Str => {
                                let mut out: Vec<String> = Vec::with_capacity(len as usize);
                                for k in 0..len {
                                    let idx = Int::from_i64(ctx, k);
                                    let cell = arr.select(&idx).as_string()?;
                                    out.push(unescape_z3_string(
                                        &model.eval(&cell, true)?.as_string()?));
                                }
                                Value::SeqStr(out)
                            }
                        }
                    }
                    SeqFieldElem::Enum { enum_name, dt: enum_dt } => {
                        let len_int = Int::from_i64(ctx, len);
                        let extracted = super::eval::extract_seq_enum(
                            &arr, &len_int, enum_name, *enum_dt, model, ctx, enums);
                        extracted?
                    }
                    SeqFieldElem::Composite { dt: inner_dt, sub_fields, .. } => {
                        let mut out: Vec<HashMap<String, Value>> = Vec::with_capacity(len as usize);
                        for k in 0..len {
                            let idx = Int::from_i64(ctx, k);
                            let cell = arr.select(&idx);
                            let inner_elem = cell.as_datatype()?;
                            let nested =
                                extract_composite_value(&inner_elem, sub_fields, *inner_dt, model, ctx, enums)?;
                            out.push(nested);
                        }
                        Value::SeqComposite(out)
                    }
                };
                extracted
            }
        };
        field_map.insert(fk.name().to_string(), value);
    }
    Some(field_map)
}

pub(super) fn extract_seq_composite<'ctx>(
    arr: &Array<'ctx>,
    len: &Int<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'_>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    let mut out: Vec<HashMap<String, Value>> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let idx = Int::from_i64(ctx, i);
        let elem_dyn = arr.select(&idx);
        let elem = elem_dyn.as_datatype()?;
        let field_map = extract_composite_value(&elem, fields, dt, model, ctx, enums)?;
        out.push(field_map);
    }
    Some(Value::SeqComposite(out))
}

/// Parse an indexed given key `<base>[<idx>]` into `(base, idx)`. Returns None
/// for any key that isn't exactly that shape (e.g. a bare name, or a field pin).
pub(super) fn parse_indexed_key(name: &str) -> Option<(&str, usize)> {
    let open = name.find('[')?;
    if !name.ends_with(']') { return None; }
    let base = &name[..open];
    if base.is_empty() { return None; }
    let idx: usize = name[open + 1..name.len() - 1].parse().ok()?;
    Some((base, idx))
}

/// Apply an indexed-element given pin like `col[0] = 1` on a `Seq` variable.
/// `name` is the full key (`col[0]`); `env` supplies the base Seq var.
/// Returns the `Bool` constraint to assert, or None if `name` isn't an indexed
/// key, the base isn't a Seq, or the element type doesn't match the value.
pub(super) fn assert_indexed_given<'ctx>(
    env: &HashMap<String, Var<'ctx>>,
    name: &str,
    value: &Value,
    ctx: &'ctx Context,
) -> Option<Bool<'ctx>> {
    let (base, idx) = parse_indexed_key(name)?;
    let (arr, _len, elem) = env.get(base)?.as_seq()?;
    let cell = arr.select(&Int::from_i64(ctx, idx as i64));
    match (elem, value) {
        (SeqElem::Int, Value::Int(n)) =>
            Some(cell.as_int()?._eq(&Int::from_i64(ctx, *n))),
        (SeqElem::Bool, Value::Bool(b)) =>
            Some(cell.as_bool()?._eq(&Bool::from_bool(ctx, *b))),
        (SeqElem::Str, Value::Str(s)) => {
            let want = Z3Str::from_str(ctx, s).ok()?;
            Some(cell.as_string()?._eq(&want))
        }
        _ => None,
    }
}

pub(super) fn assert_seq_given<'ctx>(
    var: &Var<'ctx>,
    value: &Value,
    ctx: &'ctx Context,
    enums: Option<&crate::core::EnumRegistry>,
) -> Option<Bool<'ctx>> {
    if let (Var::DatatypeSeqVar { arr, len, dt, fields, .. }, Value::SeqEnum(items)) =
        (var, value)
    {
        if fields.is_empty() {

            let _ = enums;
            let mut conjuncts: Vec<Bool> = Vec::with_capacity(items.len() + 1);
            conjuncts.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            for (i, element) in items.iter().enumerate() {
                let elem_dyn = value_enum_to_dyn_with_dt(element, dt, ctx)?;
                let idx = Int::from_i64(ctx, i as i64);
                let cell = arr.select(&idx);
                conjuncts.push(cell._eq(&elem_dyn));
            }
            let refs: Vec<&Bool> = conjuncts.iter().collect();
            return Some(Bool::and(ctx, &refs));
        }
    }
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
            for (i, element) in items.iter().enumerate() {
                let elem_ast = composite_value_to_dyn(element, fields, dt, ctx)?;
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

fn value_enum_to_dyn_with_dt<'ctx>(
    v: &Value,
    dt: &DatatypeSort<'ctx>,
    ctx: &'ctx Context,
) -> Option<z3::ast::Dynamic<'ctx>> {
    let Value::Enum { variant, fields, .. } = v else { return None };
    let var_idx = dt.variants.iter()
        .position(|vv| vv.constructor.name() == *variant)?;
    let ctor = &dt.variants[var_idx].constructor;
    if fields.is_empty() {
        return Some(z3::ast::Dynamic::from_ast(&ctor.apply(&[]).as_datatype()?));
    }

    let owned: Vec<z3::ast::Dynamic<'ctx>> = fields.iter().filter_map(|f| {
        match f {
            Value::Int(n) =>
                Some(z3::ast::Dynamic::from_ast(&Int::from_i64(ctx, *n))),
            Value::Bool(b) =>
                Some(z3::ast::Dynamic::from_ast(&Bool::from_bool(ctx, *b))),
            Value::Str(s) =>
                Some(z3::ast::Dynamic::from_ast(&Z3Str::from_str(ctx, s).ok()?)),
            Value::Real(r) => {
                let i = (*r * 1_000_000.0) as i64;
                Some(z3::ast::Dynamic::from_ast(
                    &z3::ast::Real::from_real(ctx, i as i32, 1_000_000)))
            }

            _ => None,
        }
    }).collect();
    if owned.len() != fields.len() { return None; }
    let refs: Vec<&dyn z3::ast::Ast<'ctx>> = owned.iter()
        .map(|v| v as &dyn z3::ast::Ast<'ctx>).collect();
    Some(z3::ast::Dynamic::from_ast(&ctor.apply(&refs).as_datatype()?))
}

pub(super) fn extract_set<'ctx>(
    set: &z3::ast::Set<'ctx>,
    elem: SeqElem,
    candidates: &std::cell::RefCell<Option<Vec<Value>>>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
) -> Option<Value> {
    let borrow = candidates.borrow();
    let cands = borrow.as_ref()?;
    match elem {
        SeqElem::Int => {
            let mut seen = std::collections::BTreeSet::new();
            let mut out: Vec<i64> = Vec::new();
            for v in cands {
                let Value::Int(n) = v else { continue };
                let member_bool = set.member(&Int::from_i64(ctx, *n));
                let evaluated = model.eval(&member_bool, true).and_then(|b| b.as_bool());
                if evaluated == Some(true) && seen.insert(*n) {
                    out.push(*n);
                }
            }
            Some(Value::SetInt(out))
        }
        SeqElem::Bool => {
            let mut seen = std::collections::BTreeSet::new();
            let mut out: Vec<bool> = Vec::new();
            for v in cands {
                let Value::Bool(b) = v else { continue };
                let member_bool = set.member(&Bool::from_bool(ctx, *b));
                let evaluated = model.eval(&member_bool, true).and_then(|x| x.as_bool());
                if evaluated == Some(true) && seen.insert(*b) {
                    out.push(*b);
                }
            }
            Some(Value::SetBool(out))
        }
        SeqElem::Str => {
            let mut seen = std::collections::BTreeSet::new();
            let mut out: Vec<String> = Vec::new();
            for v in cands {
                let Value::Str(s) = v else { continue };
                let z = Z3Str::from_str(ctx, s).ok()?;
                let member_bool = set.member(&z);
                let evaluated = model.eval(&member_bool, true).and_then(|b| b.as_bool());
                if evaluated == Some(true) && seen.insert(s.clone()) {
                    out.push(s.clone());
                }
            }
            Some(Value::SetStr(out))
        }
    }
}

pub(super) fn assert_set_given<'ctx>(
    var: &Var<'ctx>,
    value: &Value,
    ctx: &'ctx Context,
) -> Option<Bool<'ctx>> {
    use z3::ast::Set as Z3Set;
    use z3::Sort;
    match (var, value) {
        (Var::SetVar { set, elem: SeqElem::Int, candidates }, Value::SetInt(items)) => {
            let mut lit = Z3Set::empty(ctx, &Sort::int(ctx));
            for n in items { lit = lit.add(&Int::from_i64(ctx, *n)); }
            *candidates.borrow_mut() = Some(items.iter().map(|n| Value::Int(*n)).collect());
            Some(set._eq(&lit))
        }
        (Var::SetVar { set, elem: SeqElem::Bool, candidates }, Value::SetBool(items)) => {
            let mut lit = Z3Set::empty(ctx, &Sort::bool(ctx));
            for b in items { lit = lit.add(&Bool::from_bool(ctx, *b)); }
            *candidates.borrow_mut() = Some(items.iter().map(|b| Value::Bool(*b)).collect());
            Some(set._eq(&lit))
        }
        (Var::SetVar { set, elem: SeqElem::Str, candidates }, Value::SetStr(items)) => {
            let mut lit = Z3Set::empty(ctx, &Sort::string(ctx));
            for s in items {
                let z = Z3Str::from_str(ctx, s).ok()?;
                lit = lit.add(&z);
            }
            *candidates.borrow_mut() = Some(items.iter().map(|s| Value::Str(s.clone())).collect());
            Some(set._eq(&lit))
        }
        _ => None,
    }
}

fn composite_value_to_dyn<'ctx>(
    map: &HashMap<String, Value>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'ctx>,
    ctx: &'ctx Context,
) -> Option<z3::ast::Dynamic<'ctx>> {
    let ctor = &dt.variants[0].constructor;
    let mut field_dyns: Vec<z3::ast::Dynamic> = Vec::with_capacity(fields.len());
    for fk in fields.iter() {
        let dynamic = match fk {
            FieldKind::Primitive { name, prim_type } => {
                let v = map.get(name)?;
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
            FieldKind::Nested { name, dt: nested_dt, sub_fields, .. } => {
                let v = map.get(name)?;
                let Value::Composite(nested_map) = v else { return None };
                composite_value_to_dyn(nested_map, sub_fields, *nested_dt, ctx)?
            }
            FieldKind::SeqField { .. } => {

                return None;
            }
        };
        field_dyns.push(dynamic);
    }
    let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
    Some(ctor.apply(&dyn_refs))
}
