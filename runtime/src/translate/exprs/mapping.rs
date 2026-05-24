//! ClaimCall mapping resolution. `resolve_mapping` turns a mapping-value
//! expression into one-or-more `(env-key, Var)` bindings to install when
//! entering a ClaimCall; `expr_as_var` is its leaf case;
//! `resolve_field_chain_to_bindings` drills into `Seq(Composite)`
//! elements along a dotted field path.

use std::collections::HashMap;
use z3::ast::{Bool, Int, String as Z3Str};
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::{FieldKind, Var};

use super::bool::translate_bool;
use super::scalar::{real_from_f64, translate_int, translate_real, translate_str};
use super::seq_eq::bind_composite_fields;

/// Resolve a mapping-value expression to one-or-more `(env-key, Var)`
/// bindings to install in the inner env when entering a ClaimCall.
///
/// Three resolution paths, tried in order:
///   1. Sub-schema mapping: the value is a dotted identifier (e.g.
///      `state.player`) AND no env binding exists for that exact name,
///      but multiple env keys share it as a prefix (`state.player.x`,
///      `state.player.y`, …). Each matched leaf is bound under
///      `slot.field`. This matches the Python translator's behavior
///      for `state mapsto state.player`.
///   2. Leaf identifier or literal: `expr_as_var` produces a single
///      `Var`, bound to `slot` directly.
///   3. Otherwise → empty (caller logs a warning).
pub(crate) fn resolve_mapping<'ctx>(
    slot: &str,
    value: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<(String, Var<'ctx>)> {
    if let Expr::Identifier(name) = value {
        // If the exact name is in env, prefer leaf binding.
        if env.contains_key(name) {
            return vec![(slot.to_string(), env[name].clone())];
        }
        // Otherwise try sub-schema expansion: gather every env key
        // beginning with `name.` and re-key under `slot.field`.
        let prefix = format!("{}.", name);
        let mut out = Vec::new();
        for (k, v) in env {
            if let Some(field) = k.strip_prefix(&prefix) {
                out.push((format!("{}.{}", slot, field), v.clone()));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // Inline record literal: `Type(arg1, arg2, …)` where `Type` is a
    // known schema (a record type). Expand per-field, binding each
    // arg to `slot.field_name`. Unspecified fields stay free — same
    // partial-pinning semantics as `name ∈ Type(args)` declarations.
    //
    // Without this branch, `set_draw_color(ren, Color(220, 40, 60), eff)`
    // would warn "positional arg didn't resolve" and leave the claim's
    // `color.*` fields unconstrained. Same fix applies whether the
    // call site uses positional invocation or `mapsto` (`color ↦
    // Color(220, 40, 60)`).
    if let Expr::Call(type_name, args) = value {
        if let Some(schema) = schemas.get(type_name) {
            let fields: Vec<(String, String)> = schema.body.iter()
                .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
                    Some((name.clone(), type_name.clone()))
                } else { None })
                .collect();
            if args.len() <= fields.len() {
                let mut out = Vec::new();
                let mut ok = true;
                for (arg, (field_name, field_type)) in args.iter().zip(fields.iter()) {
                    let key = format!("{}.{}", slot, field_name);
                    // Tuple → sub-record coercion. When the arg is a
                    // bare `(a, b, c)` and the field's type is a known
                    // record schema, treat the tuple as positional
                    // args for that schema. Same rule applies inside
                    // record literals as for top-level claim args.
                    let coerced_storage: Expr;
                    let arg_ref: &Expr = match arg {
                        Expr::Tuple(items) if schemas.contains_key(field_type) => {
                            coerced_storage = Expr::Call(
                                field_type.clone(), items.clone());
                            &coerced_storage
                        }
                        other => other,
                    };
                    let v: Option<Var<'ctx>> = match field_type.as_str() {
                        "Int" | "Nat" | "Pos" =>
                            translate_int(arg_ref, ctx, env).map(Var::IntVar),
                        "Bool" =>
                            translate_bool(arg_ref, ctx, env, schemas).map(Var::BoolVar),
                        "String" =>
                            translate_str(arg_ref, ctx, env).map(Var::StrVar),
                        "Real" =>
                            translate_real(arg_ref, ctx, env).map(Var::RealVar),
                        _ => {
                            // Composite field — recurse. Handles both
                            // sub-record literals (`Foo(Bar(1, 2), 3)`)
                            // and identifier passthrough by sub-schema
                            // expansion (handled by the Identifier
                            // branch above).
                            let nested = resolve_mapping(&key, arg_ref, ctx, env, schemas);
                            if !nested.is_empty() {
                                out.extend(nested);
                                continue;
                            }
                            None
                        }
                    };
                    if let Some(var) = v {
                        out.push((key, var));
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok && !out.is_empty() {
                    return out;
                }
            }
        }
    }
    // `seq[i]` where seq is a `Seq(Composite)` — select the i-th
    // element's Datatype value and bind each of its fields under
    // `slot.field_name`. Mirrors how a bare Identifier referencing a
    // flat-expanded composite resolves. Without this branch, calls
    // like `win.draw_rect(mario.rects[0], hat_effs)` couldn't pass
    // `r` as the rect arg.
    if let Expr::Index(seq_expr, idx_expr) = value {
        if let Expr::Identifier(seq_name) = seq_expr.as_ref() {
            if let Some(var) = env.get(seq_name) {
                if let Some((arr, _, _, dt, fields)) = var.as_datatype_seq() {
                    if let Some(i) = translate_int(idx_expr, ctx, env) {
                        let elem_dyn = arr.select(&i);
                        // Build a temporary env into which bind_composite_fields
                        // writes leaves under `slot.field_name`. Then lift those
                        // entries out into the (slot.X → Var) pairs.
                        let mut tmp: HashMap<String, Var<'ctx>> = HashMap::new();
                        if bind_composite_fields(&mut tmp, &elem_dyn, fields, dt, slot) {
                            return tmp.into_iter().collect();
                        }
                    }
                }
            }
        }
    }

    // `Field(Index(seq, i), field)` (possibly nested) — reaching a
    // sub-field of a Seq element. Walk outward to find the Index root,
    // then drill in along the field path applying composite accessors.
    // At the leaf: bind either the primitive directly, the inner Seq
    // (if SeqField), or all the composite's leaves under `slot`.
    //
    // Used by the ∀-expansion path: `∀ (p, b) ∈ coindexed(platforms,
    // plat_effs) : win.draw_rect(Rect(p.color, …), b.effs)` expands
    // each iteration's `b.effs` arg to `Field(Index(plat_effs, i),
    // "effs")` and `p.color` to `Field(Index(platforms, i), "color")`.
    if matches!(value, Expr::Field(_, _)) {
        let mut path: Vec<String> = Vec::new();
        let mut cur = value;
        let (seq_name, idx_expr) = loop {
            match cur {
                Expr::Field(recv, fname) => {
                    path.push(fname.clone());
                    cur = recv.as_ref();
                }
                Expr::Index(seq, idx) => {
                    if let Expr::Identifier(name) = seq.as_ref() {
                        break (name.clone(), idx.clone());
                    }
                    return Vec::new();
                }
                _ => return Vec::new(),
            }
        };
        path.reverse();
        let Some(out) = resolve_field_chain_to_bindings(
            &seq_name, &idx_expr, &path, slot, ctx, env) else {
            return Vec::new();
        };
        return out;
    }
    if let Some(v) = expr_as_var(value, ctx, env) {
        return vec![(slot.to_string(), v)];
    }
    Vec::new()
}

/// Drill into a `Seq(Composite)` element along a dotted field path and
/// produce bindings under `slot`. Handles three terminating shapes:
///
///   * primitive leaf — single binding `slot → IntVar/BoolVar/StrVar`.
///   * composite sub-field — `bind_composite_fields` with prefix `slot`.
///   * `SeqField` — single binding `slot → SeqVar/DatatypeSeqVar` from
///     the inner Seq's accessors.
///
/// Used by the ∀-expansion: per-iteration substituted args like
/// `Field(Field(Index(platforms, 0), "aabb"), "pos")` reach a sub-record;
/// `Field(Index(plat_effs, 0), "effs")` reaches a Seq.
fn resolve_field_chain_to_bindings<'ctx>(
    seq_name: &str,
    idx_expr: &Expr,
    path: &[String],
    slot: &str,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Vec<(String, Var<'ctx>)>> {
    use crate::core::SeqFieldElem;
    let var = env.get(seq_name)?;
    let (arr, _, _, root_dt, root_fields) = var.as_datatype_seq()?;
    let i = translate_int(idx_expr, ctx, env)?;
    let elem_dyn = arr.select(&i);

    // Walk inward along the path. At each step we have a current
    // Dynamic (elem_dyn) + a current (dt, fields) describing it.
    let mut cur_dyn = elem_dyn;
    let mut cur_dt: &DatatypeSort = root_dt;
    let mut cur_fields: &[FieldKind] = root_fields;
    for (depth, fname) in path.iter().enumerate() {
        let pos = cur_fields.iter().position(|fk| fk.name() == fname)?;
        let fk = &cur_fields[pos];
        let is_last = depth == path.len() - 1;
        match fk {
            FieldKind::Primitive { prim_type, .. } => {
                if !is_last { return None; }
                if pos >= cur_dt.variants[0].accessors.len() { return None; }
                let raw = cur_dt.variants[0].accessors[pos].apply(
                    &[&cur_dyn.as_datatype()?]);
                let var: Option<Var<'ctx>> = match prim_type.as_str() {
                    "Int" | "Nat" | "Pos" => raw.as_int().map(Var::IntVar),
                    "Bool" => raw.as_bool().map(Var::BoolVar),
                    "String" => raw.as_string().map(Var::StrVar),
                    _ => None,
                };
                return Some(vec![(slot.to_string(), var?)]);
            }
            FieldKind::Nested { dt: nested_dt, sub_fields, .. } => {
                if pos >= cur_dt.variants[0].accessors.len() { return None; }
                let raw = cur_dt.variants[0].accessors[pos].apply(
                    &[&cur_dyn.as_datatype()?]);
                if is_last {
                    // Bind all of the nested composite's leaves under
                    // `slot.X.Y…`.
                    let mut tmp: HashMap<String, Var<'ctx>> = HashMap::new();
                    if !bind_composite_fields(&mut tmp, &raw, sub_fields, nested_dt, slot) {
                        return None;
                    }
                    return Some(tmp.into_iter().collect());
                }
                cur_dyn = raw;
                cur_dt = nested_dt;
                cur_fields = sub_fields;
            }
            FieldKind::SeqField { arr_idx, len_idx, elem: seq_elem, .. } => {
                if !is_last { return None; }
                if *len_idx >= cur_dt.variants[0].accessors.len() { return None; }
                let elem_d = cur_dyn.as_datatype()?;
                let arr_d = cur_dt.variants[0].accessors[*arr_idx].apply(&[&elem_d]);
                let len_d = cur_dt.variants[0].accessors[*len_idx].apply(&[&elem_d]);
                let inner_arr = arr_d.as_array()?;
                let inner_len = len_d.as_int()?;
                let var = match seq_elem {
                    SeqFieldElem::Primitive(e) => Var::SeqVar {
                        arr: inner_arr, len: inner_len, elem: *e,
                    },
                    SeqFieldElem::Enum { dt, enum_name } => Var::DatatypeSeqVar {
                        arr: inner_arr, len: inner_len,
                        type_name: enum_name.clone(),
                        dt: *dt, fields: Vec::new(),
                    },
                    SeqFieldElem::Composite { dt, type_name, sub_fields } => Var::DatatypeSeqVar {
                        arr: inner_arr, len: inner_len,
                        type_name: type_name.clone(),
                        dt: *dt, fields: sub_fields.clone(),
                    },
                };
                return Some(vec![(slot.to_string(), var)]);
            }
        }
    }
    None
}

/// Resolve a leaf expression to a single `Var`. Used both for ClaimCall
/// scalar mappings and as the tail-case of `resolve_mapping`.
fn expr_as_var<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Var<'ctx>> {
    match e {
        Expr::Identifier(name) => env.get(name).cloned(),
        Expr::Int(n)  => Some(Var::IntVar(Int::from_i64(ctx, *n))),
        Expr::Bool(b) => Some(Var::BoolVar(Bool::from_bool(ctx, *b))),
        Expr::Real(f) => Some(Var::RealVar(real_from_f64(ctx, *f))),
        Expr::Str(s)  => Z3Str::from_str(ctx, s).ok().map(Var::StrVar),
        _ => None,
    }
}
