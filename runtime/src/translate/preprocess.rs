use std::collections::{HashMap, HashSet};

use crate::core::ast::*;
use crate::core::{Value, Var};

pub fn collect_referenced_names(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Identifier(n) => { out.insert(n.clone()); }
        Expr::Cardinality(inner) => {

            if let Expr::Identifier(name) = inner.as_ref() {
                out.insert(name.clone());
            }
            collect_referenced_names(inner, out);
        }
        Expr::Binary(_, lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::Not(inner) => collect_referenced_names(inner, out),
        Expr::Range(lo, hi) => {
            collect_referenced_names(lo, out);
            collect_referenced_names(hi, out);
        }
        Expr::Index(s, i) => {
            collect_referenced_names(s, out);
            collect_referenced_names(i, out);
        }
        Expr::Field(r, _) => collect_referenced_names(r, out),
        Expr::InExpr(lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::SetLit(items) | Expr::SeqLit(items) => {
            for it in items { collect_referenced_names(it, out); }
        }
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            collect_referenced_names(range, out);
            collect_referenced_names(body, out);
        }
        Expr::Call(_, args) => {
            for a in args { collect_referenced_names(a, out); }
        }
        _ => {}
    }
}

pub(super) fn collect_pinned_ints(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    seq_lengths: &HashMap<String, i64>,
) -> HashMap<String, i64> {
    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for item in body {
            if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {

                            if let Some(v) = eval_pure_int(b, &pinned, seq_lengths) {
                                pinned.insert(name.clone(), v);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }
    pinned
}

fn eval_pure_int(
    e: &Expr,
    pinned: &HashMap<String, i64>,
    seq_lengths: &HashMap<String, i64>,
) -> Option<i64> {
    match e {
        Expr::Int(n) => Some(*n),
        Expr::Identifier(name) => pinned.get(name).copied(),
        Expr::Cardinality(inner) => match inner.as_ref() {
            Expr::Identifier(name) => seq_lengths.get(name).copied(),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = eval_pure_int(lhs, pinned, seq_lengths)?;
            let r = eval_pure_int(rhs, pinned, seq_lengths)?;
            Some(match op {
                BinOp::Add => l.checked_add(r)?,
                BinOp::Sub => l.checked_sub(r)?,
                BinOp::Mul => l.checked_mul(r)?,
                BinOp::Div => if r == 0 { return None } else { l / r },
                _ => return None,
            })
        }
        _ => None,
    }
}

pub(super) fn collect_seq_lengths_with_schemas(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    schemas: Option<&HashMap<String, SchemaDecl>>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();

    for (k, v) in given {
        let len = match v {
            Value::SeqInt(v)       => v.len() as i64,
            Value::SeqBool(v)      => v.len() as i64,
            Value::SeqStr(v)       => v.len() as i64,
            Value::SeqComposite(v) => v.len() as i64,
            Value::SeqEnum(v)      => v.len() as i64,
            Value::SetInt(v)       => v.len() as i64,
            Value::SetBool(v)      => v.len() as i64,
            Value::SetStr(v)       => v.len() as i64,
            _ => continue,
        };
        out.insert(k.clone(), len);
    }

    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }

    let mut changed = true;
    while changed {
        changed = false;
        walk_constraints(body, schemas, &pinned, &mut out, &mut changed);

        scan_int_pins(body, schemas, &mut pinned, &out, &mut changed);
    }
    out
}

fn scan_int_pins(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    pinned: &mut HashMap<String, i64>,
    seq_lens: &HashMap<String, i64>,
    changed: &mut bool,
) {
    for item in body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {
                            if let Some(v) = eval_pure_int(b, pinned, seq_lens) {
                                pinned.insert(name.clone(), v);
                                *changed = true;
                            }
                        }
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(schemas) = schemas {
                    if let Some(claim) = schemas.get(claim_name) {
                        scan_int_pins(&claim.body, Some(schemas), pinned, seq_lens, changed);
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk_constraints(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    no_pinned: &HashMap<String, i64>,
    out: &mut HashMap<String, i64>,
    changed: &mut bool,
) {
    for item in body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {

                    if let Expr::Cardinality(inner) = a.as_ref() {
                        if let Expr::Identifier(name) = inner.as_ref() {
                            if !out.contains_key(name) {
                                if let Some(v) = eval_pure_int(b, no_pinned, out) {
                                    out.insert(name.clone(), v);
                                    *changed = true;
                                }
                            }
                        }
                    }

                    if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                        (a.as_ref(), b.as_ref())
                    {
                        if !out.contains_key(name) {
                            out.insert(name.clone(), items.len() as i64);
                            *changed = true;
                        }
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(schemas) = schemas {
                    if let Some(claim) = schemas.get(claim_name) {
                        walk_constraints(&claim.body, Some(schemas), no_pinned, out, changed);
                    }
                }
            }

            BodyItem::Membership { name: inst_name, type_name, .. } => {
                if let Some(schemas) = schemas {
                    if let Some(ty) = schemas.get(type_name) {
                        let field_set: std::collections::HashSet<String> = ty.body.iter()
                            .filter_map(|it| match it {
                                BodyItem::Membership { name, .. } => Some(name.clone()),
                                _ => None,
                            })
                            .collect();
                        walk_constraints_with_prefix(
                            &ty.body, Some(schemas), no_pinned, out, changed,
                            inst_name, &field_set);
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk_constraints_with_prefix(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    no_pinned: &HashMap<String, i64>,
    out: &mut HashMap<String, i64>,
    changed: &mut bool,
    prefix: &str,
    field_set: &std::collections::HashSet<String>,
) {
    for item in body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            for (a, b) in [(lhs, rhs), (rhs, lhs)] {

                if let Expr::Cardinality(inner) = a.as_ref() {
                    if let Expr::Identifier(name) = inner.as_ref() {
                        let first_seg = name.split('.').next().unwrap_or("");
                        if field_set.contains(first_seg) {
                            let dotted = format!("{}.{}", prefix, name);
                            if !out.contains_key(&dotted) {
                                if let Some(v) = eval_pure_int(b, no_pinned, out) {
                                    out.insert(dotted, v);
                                    *changed = true;
                                }
                            }
                        }
                    }
                }

                if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                    (a.as_ref(), b.as_ref())
                {
                    let first_seg = name.split('.').next().unwrap_or("");
                    if field_set.contains(first_seg) {
                        let dotted = format!("{}.{}", prefix, name);
                        if !out.contains_key(&dotted) {
                            out.insert(dotted, items.len() as i64);
                            *changed = true;
                        }
                    }
                }
            }
        }
    }

    let _ = schemas;
}

pub(super) fn apply_pinned_ints<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    pinned: &HashMap<String, i64>,
) {
    for (name, value) in pinned {
        if env.contains_key(name) {
            env.insert(name.clone(), Var::PinnedInt(*value));
        }
    }
}
