//! Value translators: scalar (`Expr` → Z3 String/Int/Real) plus record-vector
//! lifting (componentwise eq/cmp/arith over short record types like IVec2,
//! Color, … by enumerating leaf paths and substituting per-leaf).

use std::collections::HashMap;
use z3::ast::{Bool, Int, Real, String as Z3Str};
use z3::Context;

use crate::core::{FieldKind, SeqElem, Var};

use super::*;

// ───────────────────────── record-vector lifting ─────────────────────────

pub(super) fn lift_record_op<'ctx>(
    op: &BinOp,
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    if !matches!(op,
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    ) {
        return None;
    }

    let mut lhs_records = Vec::new();
    let mut rhs_records = Vec::new();
    collect_record_refs(lhs, env, schemas, &mut lhs_records);
    collect_record_refs(rhs, env, schemas, &mut rhs_records);
    if lhs_records.is_empty() || rhs_records.is_empty() { return None; }
    let mut all_records = lhs_records;
    all_records.extend(rhs_records);

    let leaves = lhs_record_leaves(&all_records[0], env, schemas)?;
    for rec in all_records.iter().skip(1) {
        let rec_leaves = lhs_record_leaves(rec, env, schemas)?;
        if rec_leaves != leaves { return None; }
    }

    let mut clauses = Vec::with_capacity(leaves.len());
    for leaf in &leaves {
        let lhs_leaf = substitute_record_refs(lhs, leaf, env, schemas)?;
        let rhs_leaf = substitute_record_refs(rhs, leaf, env, schemas)?;
        let leaf_op = Expr::Binary(
            op.clone(),
            Box::new(lhs_leaf),
            Box::new(rhs_leaf),
        );
        clauses.push(translate_bool(&leaf_op, ctx, env, schemas)?);
    }
    let refs: Vec<&Bool> = clauses.iter().collect();
    Some(match op {

        BinOp::Neq => Bool::or(ctx, &refs),

        _ => Bool::and(ctx, &refs),
    })
}

pub(super) fn lhs_record_leaves<'ctx>(
    lhs: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Vec<String>> {
    match lhs {

        Expr::Call(type_name, _args) => {
            let schema = schemas.get(type_name)?;
            let mut leaves = schema_leaf_paths(schema, schemas);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) { return None; }
            let prefix = format!("{}.", name);
            let mut leaves: Vec<String> = env.keys()
                .filter_map(|k| k.strip_prefix(&prefix).map(String::from))
                .collect();
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Field(receiver, field) => {

            let Expr::Index(seq_expr, _) = receiver.as_ref() else { return None };
            let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
            let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return None };
            let nested_sub = fields.iter().find_map(|f| match f {
                FieldKind::Nested { name, sub_fields, .. } if name == field => Some(sub_fields),
                _ => None,
            })?;
            let mut leaves = enumerate_nested_leaves(nested_sub);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        Expr::Index(receiver, _) => {

            let Expr::Identifier(seq_name) = receiver.as_ref() else { return None };
            let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return None };
            let mut leaves = enumerate_nested_leaves(fields);
            if leaves.is_empty() { return None; }
            leaves.sort();
            Some(leaves)
        }
        _ => None,
    }
}

pub(super) fn schema_leaf_paths(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<String> {
    let mut out = Vec::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            if let Some(sub) = schemas.get(type_name) {
                for leaf in schema_leaf_paths(sub, schemas) {
                    out.push(format!("{}.{}", name, leaf));
                }
            } else {
                out.push(name.clone());
            }
        }
    }
    out
}

pub(super) fn enumerate_nested_leaves(fields: &[FieldKind]) -> Vec<String> {
    let mut out = Vec::new();
    for f in fields {
        match f {
            FieldKind::Primitive { name, .. } => out.push(name.clone()),
            FieldKind::Nested { name, sub_fields, .. } => {
                for sub in enumerate_nested_leaves(sub_fields) {
                    out.push(format!("{}.{}", name, sub));
                }
            }
            FieldKind::SeqField { name, .. } => {

                out.push(name.clone());
            }
        }
    }
    out
}

pub(super) fn substitute_record_refs<'ctx>(
    expr: &Expr,
    leaf: &str,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Expr> {
    match expr {

        Expr::Call(type_name, args) => {
            let schema = schemas.get(type_name)?;

            let fields: Vec<(&str, &str)> = schema.body.iter()
                .filter_map(|item| match item {
                    BodyItem::Membership { name, type_name, .. } =>
                        Some((name.as_str(), type_name.as_str())),
                    _ => None,
                })
                .collect();

            let (first, rest) = match leaf.split_once('.') {
                Some((a, b)) => (a, Some(b)),
                None => (leaf, None),
            };
            let pos = fields.iter().position(|(n, _)| *n == first)?;
            if pos >= args.len() { return None; }

            let coerced: Expr;
            let arg_ref: &Expr = match &args[pos] {
                Expr::Tuple(items) if schemas.contains_key(fields[pos].1) => {
                    coerced = Expr::Call(fields[pos].1.to_string(), items.clone());
                    &coerced
                }
                other => other,
            };
            match rest {
                None => Some(arg_ref.clone()),

                Some(rest_path) => substitute_record_refs(arg_ref, rest_path, env, schemas),
            }
        }
        Expr::Identifier(name) => {
            if env.contains_key(name) {

                return Some(expr.clone());
            }
            let prefix = format!("{}.", name);
            if env.keys().any(|k| k.starts_with(&prefix)) {

                let mut extended = name.clone();
                for p in leaf.split('.') {
                    extended.push('.');
                    extended.push_str(p);
                }
                if env.contains_key(&extended) { Some(Expr::Identifier(extended)) }
                else { None }
            } else {

                Some(expr.clone())
            }
        }
        Expr::Field(receiver, field) => {

            if is_field_of_index_record(receiver, field, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }

            Some(expr.clone())
        }
        Expr::Index(receiver, _) => {

            if is_seq_element_record(receiver, env) {
                let mut result = expr.clone();
                for p in leaf.split('.') {
                    result = Expr::Field(Box::new(result), p.to_string());
                }
                return Some(result);
            }

            Some(expr.clone())
        }
        Expr::Binary(op, a, b) => {
            let a2 = substitute_record_refs(a, leaf, env, schemas)?;
            let b2 = substitute_record_refs(b, leaf, env, schemas)?;
            Some(Expr::Binary(op.clone(), Box::new(a2), Box::new(b2)))
        }
        Expr::Not(x) => substitute_record_refs(x, leaf, env, schemas).map(|y| Expr::Not(Box::new(y))),

        _ => Some(expr.clone()),
    }
}

pub(super) fn is_field_of_index_record<'ctx>(
    receiver: &Expr,
    field: &str,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Index(seq_expr, _) = receiver else { return false };
    let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return false };
    let Some(Var::DatatypeSeqVar { fields, .. }) = env.get(seq_name) else { return false };
    fields.iter().any(|f| matches!(f, FieldKind::Nested { name, .. } if name == field))
}

pub(super) fn is_seq_element_record<'ctx>(
    receiver: &Expr,
    env: &HashMap<String, Var<'ctx>>,
) -> bool {
    let Expr::Identifier(seq_name) = receiver else { return false };
    matches!(env.get(seq_name), Some(Var::DatatypeSeqVar { .. }))
}

pub(super) fn collect_record_refs<'ctx>(
    expr: &Expr,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
    out: &mut Vec<Expr>,
) {
    match expr {

        Expr::Call(type_name, _) if schemas.contains_key(type_name) => {
            out.push(expr.clone());
        }
        Expr::Identifier(name) => {
            if !env.contains_key(name)
                && env.keys().any(|k| k.starts_with(&format!("{}.", name)))
            {
                out.push(expr.clone());
            }
        }
        Expr::Field(receiver, field) => {
            if is_field_of_index_record(receiver, field, env) {
                out.push(expr.clone());
            }
        }
        Expr::Index(receiver, _) => {
            if is_seq_element_record(receiver, env) {
                out.push(expr.clone());
            }
        }
        Expr::Binary(_, a, b) => {
            collect_record_refs(a, env, schemas, out);
            collect_record_refs(b, env, schemas, out);
        }
        Expr::Not(x) => collect_record_refs(x, env, schemas, out),
        _ => {}
    }
}

// ───────────────────────── scalar translators ─────────────────────────

pub(super) fn translate_str<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Z3Str<'ctx>> {
    match e {
        Expr::Str(s) => Z3Str::from_str(ctx, s).ok(),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_str().cloned()),

        Expr::Binary(BinOp::Concat, lhs, rhs) => {
            let l = translate_str(lhs, ctx, env)?;
            let r = translate_str(rhs, ctx, env)?;
            Some(Z3Str::concat(ctx, &[&l, &r]))
        }

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Str { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_string()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "String" {
                raw.as_string()
            } else {
                None
            }
        }

        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_str(a, ctx, env)?;
            let else_v = translate_str(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_str(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

pub(super) fn translate_int<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Int<'ctx>> {

    if let Expr::Call(name, args) = e {
        match (name.as_str(), args.len()) {
            ("min", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.le(&b).ite(&a, &b));
            }
            ("max", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.ge(&b).ite(&a, &b));
            }
            ("abs", 1) => {
                let x = translate_int(&args[0], ctx, env)?;
                let zero = Int::from_i64(ctx, 0);
                let neg = Int::sub(ctx, &[&zero, &x]);
                return Some(x.ge(&zero).ite(&x, &neg));
            }
            ("mod", 2) => {
                let a = translate_int(&args[0], ctx, env)?;
                let b = translate_int(&args[1], ctx, env)?;
                return Some(a.modulo(&b));
            }
            ("clamp", 3) => {
                let x  = translate_int(&args[0], ctx, env)?;
                let lo = translate_int(&args[1], ctx, env)?;
                let hi = translate_int(&args[2], ctx, env)?;

                let inner = x.le(&hi).ite(&x, &hi);
                return Some(inner.ge(&lo).ite(&inner, &lo));
            }

            ("position_of", 2) => {
                let Expr::Identifier(sname) = &args[0] else { return None };
                let var = env.get(sname)?;
                let (arr, len, elem) = var.as_seq()?;
                let n = len.simplify().as_i64()?;
                let mut result = Int::from_i64(ctx, -1);
                for i in (0..n).rev() {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    let eq = match elem {
                        SeqElem::Int => {
                            let v = translate_int(&args[1], ctx, env)?;
                            cell.as_int()?._eq(&v)
                        }
                        SeqElem::Bool => {
                            let v = match &args[1] {
                                Expr::Bool(b) => Bool::from_bool(ctx, *b),
                                Expr::Identifier(n) => env.get(n)?.as_bool()?.clone(),
                                _ => return None,
                            };
                            cell.as_bool()?._eq(&v)
                        }
                        SeqElem::Str => {
                            let v = translate_str(&args[1], ctx, env)?;
                            cell.as_string()?._eq(&v)
                        }
                    };
                    result = eq.ite(&idx, &result);
                }
                return Some(result);
            }
            _ => {}
        }
    }
    match e {
        Expr::Int(n) => Some(Int::from_i64(ctx, *n)),
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::IntVar(i)) => Some(i.clone()),
            Some(Var::PinnedInt(v)) => Some(Int::from_i64(ctx, *v)),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_int(lhs, ctx, env)?;
            let r = translate_int(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Int::add(ctx, &[&l, &r]),
                BinOp::Sub => Int::sub(ctx, &[&l, &r]),
                BinOp::Mul => Int::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }

        Expr::Cardinality(inner) => {
            if let Some(handle) = resolve_seq_handle(inner.as_ref(), ctx, env) {
                return Some(handle.len().clone());
            }
            if let Expr::Identifier(name) = inner.as_ref() {
                if let Some(var) = env.get(name) {
                    if let Some((_, _, candidates)) = var.as_set_with_candidates() {
                        if let Some(cands) = candidates.borrow().as_ref() {
                            return Some(Int::from_i64(ctx, cands.len() as i64));
                        }
                    }
                    if let Some((_, _, _, _, candidates)) = var.as_datatype_set() {
                        if let Some(cands) = candidates.borrow().as_ref() {
                            return Some(Int::from_i64(ctx, cands.len() as i64));
                        }
                    }
                }
            }
            None
        }

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Int { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_int()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if matches!(ftype.as_str(), "Int" | "Nat" | "Pos") {
                raw.as_int()
            } else {
                None
            }
        }

        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_int(a, ctx, env)?;
            let else_v = translate_int(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }

        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_int(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

pub(super) fn translate_real<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Real<'ctx>> {
    match e {
        Expr::Real(f) => Some(real_from_f64(ctx, *f)),
        Expr::Int(n)  => Some(Real::from_int(&Int::from_i64(ctx, *n))),
        Expr::Identifier(name) => match env.get(name) {
            Some(Var::RealVar(r)) => Some(r.clone()),
            Some(Var::IntVar(i))  => Some(Real::from_int(i)),
            Some(Var::PinnedInt(v)) => Some(Real::from_int(&Int::from_i64(ctx, *v))),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_real(lhs, ctx, env)?;
            let r = translate_real(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Real::add(ctx, &[&l, &r]),
                BinOp::Sub => Real::sub(ctx, &[&l, &r]),
                BinOp::Mul => Real::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }

        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, &HashMap::new())?;
            let then_v = translate_real(a, ctx, env)?;
            let else_v = translate_real(b, ctx, env)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_real(body, ctx, e))?;
            fold_arms_to_ite(compiled)
        }
        _ => None,
    }
}

pub(super) fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let s = f.to_string();
    let (num, den) = if let Some(dot) = s.find('.') {
        let (int_part, frac_with_dot) = s.split_at(dot);
        let frac = &frac_with_dot[1..];
        (format!("{}{}", int_part, frac),
         format!("1{}", "0".repeat(frac.len())))
    } else {
        (s, "1".to_string())
    };
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}
