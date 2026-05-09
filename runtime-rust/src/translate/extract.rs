//! Reading model values back out of a satisfied Z3 solver. Each
//! function maps one variable kind (or one composite element) to its
//! `Value`. Also `assert_seq_given` for the inverse: pinning a Seq
//! variable to a `Value::Seq*` shape from a `given` map.

use std::collections::HashMap;
use z3::ast::{Array, Ast, Bool, Int, String as Z3Str};
use z3::{Context, DatatypeSort};

use super::types::{FieldKind, SeqElem, Value, Var};

/// Decode Z3's `as_string()` output back to a Rust string. Z3
/// represents non-printable characters (and a few others) using
/// `\u{xxxx}` escape sequences; without unescaping, a `"abc\n"`
/// shader source survives the solver round-trip as the seven
/// literal characters `"abc\u{a}"`, which the GLSL compiler then
/// rejects. This function reverses that escape — for every other
/// character it's the identity.
pub(super) fn unescape_z3_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'u' {
            // Try to parse `\u{HEX}`. If anything looks off, fall
            // through and emit the backslash literally so we don't
            // corrupt strings that happen to contain `\u` for real.
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
        // Default: copy the byte. We re-derive `char` boundaries
        // by reading the UTF-8 sequence starting at i.
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod unescape_tests {
    use super::unescape_z3_string;
    #[test]
    fn newline_escape_decoded() {
        assert_eq!(unescape_z3_string("abc\\u{a}def"), "abc\ndef");
    }
    #[test]
    fn multi_escape_decoded() {
        assert_eq!(
            unescape_z3_string("a\\u{9}b\\u{20}c"),  // \t and space
            "a\tb c",
        );
    }
    #[test]
    fn high_codepoint_decoded() {
        // U+1F600 is 😀 (4-byte UTF-8)
        assert_eq!(unescape_z3_string("hi \\u{1f600}!"), "hi 😀!");
    }
    #[test]
    fn no_escape_passthrough() {
        assert_eq!(unescape_z3_string("plain ascii"), "plain ascii");
    }
    #[test]
    fn malformed_passthrough() {
        // Missing closing brace — emit literally.
        assert_eq!(unescape_z3_string("\\u{xyz"), "\\u{xyz");
    }
}

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
                out.push(unescape_z3_string(&model.eval(&v, true)?.as_string()?));
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
                    Value::Str(unescape_z3_string(&model.eval(&z, true)?.as_string()?))
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

/// Build a Z3 Datatype `Dynamic` from a `Value::Composite` field map +
/// the type's `FieldKind` list. Mirror image of `extract_composite_value`:
/// extraction reads the model + accessors to assemble a flat field map;
/// this builds a constructor application from a flat field map back into
/// a Datatype value.
///
/// The recursion handles nested record fields. For `BouncingDot` whose
/// `pos` field is itself an `IVec2`, the field map carries
/// `Value::Composite({x, y})` for `pos`, which this function passes
/// back to itself with the nested type's `(dt, sub_fields)`.
///
/// Without this, round-tripping a state through `given` between
/// executor frames silently failed for any user type with nested record
/// fields — `assert_seq_given` returned None, the caller printed
/// "type mismatch for given", and the next-frame solver ran with
/// state.dots free → garbage output.
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
        };
        field_dyns.push(dynamic);
    }
    let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
    Some(ctor.apply(&dyn_refs))
}
