//! `encode_bool` — the central dispatcher. Handles built-in predicate calls
//! (contains / distinct), quantifiers (coindexed / edges / ranges / set-subset),
//! and every binary/equality form, delegating compound-value equalities to the
//! equation translators and record comparisons to the record-lift.

use std::collections::HashMap;
use z3::ast::{Bool, Int, String as Z3Str};
use z3::Context;

use crate::core::{SeqElem, Var};

use super::*;

pub(in crate::encode) fn encode_bool<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {

    if let Expr::Call(name, args) = e {
        if name == "contains" && args.len() == 2 {
            let Expr::Identifier(seq_name) = &args[0] else { return None };
            let var = env.get(seq_name)?;

            if let Some((arr, len, elem)) = var.as_seq() {
                let n = len.simplify().as_i64()?;
                let mut clauses: Vec<Bool> = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    let eq = match elem {
                        SeqElem::Int => {
                            let v = encode_int(&args[1], ctx, env)?;
                            cell.as_int()?._eq(&v)
                        }
                        SeqElem::Bool => {
                            let v = encode_bool(&args[1], ctx, env, schemas)?;
                            cell.as_bool()?._eq(&v)
                        }
                        SeqElem::Str => {
                            let v = encode_str(&args[1], ctx, env)?;
                            cell.as_string()?._eq(&v)
                        }
                    };
                    clauses.push(eq);
                }
                let refs: Vec<&Bool> = clauses.iter().collect();
                return Some(if refs.is_empty() {
                    Bool::from_bool(ctx, false)
                } else {
                    Bool::or(ctx, &refs)
                });
            }

            if let Some((arr, len, _, _, _)) = var.as_datatype_seq() {
                let n = len.simplify().as_i64()?;

                let mut clauses: Vec<Bool> = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);

                    let arg = args[1].clone();
                    let eq_expr = Expr::Binary(
                        crate::core::ast::BinOp::Eq,
                        Box::new(Expr::Index(
                            Box::new(args[0].clone()),
                            Box::new(Expr::Int(i)),
                        )),
                        Box::new(arg),
                    );
                    if let Some(b) = encode_bool(&eq_expr, ctx, env, schemas) {
                        clauses.push(b);
                    } else {
                        let _ = cell;
                        return None;
                    }
                }
                let refs: Vec<&Bool> = clauses.iter().collect();
                return Some(if refs.is_empty() {
                    Bool::from_bool(ctx, false)
                } else {
                    Bool::or(ctx, &refs)
                });
            }
            return None;
        }
        if name == "distinct" {

            if args.is_empty() { return Some(Bool::from_bool(ctx, true)); }

            if args.len() == 1 {
                let Expr::Identifier(sname) = &args[0] else { return None };
                let var = env.get(sname)?;
                let (_, len, _) = var.as_seq()?;
                let n = len.simplify().as_i64()?;
                if n <= 1 { return Some(Bool::from_bool(ctx, true)); }
                let exploded: Vec<Expr> = (0..n).map(|i|
                    Expr::Index(
                        Box::new(Expr::Identifier(sname.clone())),
                        Box::new(Expr::Int(i)))).collect();
                return encode_bool(
                    &Expr::Call("distinct".into(), exploded),
                    ctx, env, schemas);
            }
            if let Some(ints) = args.iter()
                .map(|a| encode_int(a, ctx, env))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Int> = ints.iter().collect();
                return Some(Int::distinct(ctx, &refs));
            }
            if let Some(bools) = args.iter()
                .map(|a| encode_bool(a, ctx, env, schemas))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Bool> = bools.iter().collect();
                return Some(Bool::distinct(ctx, &refs));
            }
            if let Some(strs) = args.iter()
                .map(|a| encode_str(a, ctx, env))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Z3Str> = strs.iter().collect();
                return Some(Z3Str::distinct(ctx, &refs));
            }
            return None;
        }
    }
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(encode_bool(inner, ctx, env, schemas)?.not()),

        Expr::Ternary(c, a, b) => {
            let cond = encode_bool(c, ctx, env, schemas)?;
            let then_v = encode_bool(a, ctx, env, schemas)?;
            let else_v = encode_bool(b, ctx, env, schemas)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = encode_match_arms(scr, arms, ctx, env,
                |body, e| encode_bool(body, ctx, e, schemas))?;
            fold_arms_to_ite(compiled)
        }

        Expr::Matches(e, pattern) => {
            use crate::core::ast::MatchPattern;
            match pattern {
                MatchPattern::Wildcard => Some(Bool::from_bool(ctx, true)),
                MatchPattern::Ctor { name, .. } => {
                    let scr_name = match e.as_ref() {
                        Expr::Identifier(n) if !n.contains('.') => n,
                        _ => return None,
                    };
                    let (scr_dt, dt) = match env.get(scr_name)? {
                        Var::EnumVar { ast, dt, .. } => (ast.clone(), *dt),
                        _ => return None,
                    };
                    let var_idx = dt.variants.iter()
                        .position(|v| v.constructor.name() == *name)?;
                    dt.variants[var_idx].tester.apply(&[&scr_dt]).as_bool()
                }
            }
        }

        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Bool { return None; }
            let i = encode_int(idx_expr, ctx, env)?;
            arr.select(&i).as_bool()
        }

        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "Bool" { raw.as_bool() } else { None }
        }

        Expr::InExpr(lhs, rhs) => {

            if let Expr::Identifier(name) = rhs.as_ref() {
                if let Some((set, elem)) = env.get(name).and_then(|v| v.as_set()) {
                    return match elem {
                        SeqElem::Int => {
                            let x = encode_int(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Bool => {
                            let x = encode_bool(lhs, ctx, env, schemas)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Str => {
                            let x = encode_str(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                    };
                }

                if let Some((set, _, dt, fields, _)) =
                    env.get(name).and_then(|v| v.as_datatype_set())
                {
                    if let Expr::Identifier(ident) = lhs.as_ref() {
                        let dyn_val = build_composite_dynamic(ident, dt, fields, ctx, env)?;
                        return Some(set.member(&dyn_val));
                    }
                }
            }

            let items = match rhs.as_ref() {
                Expr::SetLit(items) => items.clone(),
                _ => return None,
            };
            let mut clauses: Vec<Bool> = Vec::with_capacity(items.len());
            for it in &items {
                let eq = Expr::Binary(BinOp::Eq, lhs.clone(), Box::new(it.clone()));
                if let Some(b) = encode_bool(&eq, ctx, env, schemas) {
                    clauses.push(b);
                }
            }
            if clauses.is_empty() { return Some(Bool::from_bool(ctx, false)); }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::or(ctx, &refs))
        }

        Expr::Forall(vars, range, body) | Expr::Exists(vars, range, body) => {
            let mut clauses: Vec<Bool> = Vec::new();

            if let Expr::Call(name, args) = range.as_ref() {
                match (name.as_str(), args.len()) {
                    ("coindexed", n_seqs) if n_seqs >= 1 => {
                        if vars.len() != n_seqs {
                            return None;

                        }

                        let mut seq_lens: Vec<i64> = Vec::with_capacity(n_seqs);
                        for arg in args {
                            let Expr::Identifier(seq_name) = arg else { return None };
                            let seq_var = env.get(seq_name)?;
                            let len = if let Some((_, len, _, _, _)) = seq_var.as_datatype_seq() {
                                len.simplify().as_i64()?
                            } else if let Some((_, len, _)) = seq_var.as_seq() {
                                len.simplify().as_i64()?
                            } else {
                                return None;
                            };
                            seq_lens.push(len);
                        }
                        let n = *seq_lens.iter().min()?;
                        for i in 0..n {
                            let mut env2 = env.clone();
                            for (var, arg) in vars.iter().zip(args.iter()) {
                                let Expr::Identifier(seq_name) = arg else { return None };
                                let seq_var = env.get(seq_name)?;
                                let idx = Int::from_i64(ctx, i);
                                if let Some((arr, _, _, dt, fields)) = seq_var.as_datatype_seq() {
                                    let elem_dyn = arr.select(&idx);
                                    if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                                        return None;
                                    }
                                } else if let Some((arr, _, elem)) = seq_var.as_seq() {
                                    let cell = arr.select(&idx);
                                    let v = match elem {
                                        SeqElem::Int  => cell.as_int().map(Var::IntVar),
                                        SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                                        SeqElem::Str  => cell.as_string().map(Var::StrVar),
                                    };
                                    env2.insert(var.clone(), v?);
                                } else {
                                    return None;
                                }
                            }
                            if let Some(b) = encode_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                        let refs: Vec<&Bool> = clauses.iter().collect();
                        return Some(if matches!(e, Expr::Forall(..)) {
                            Bool::and(ctx, &refs)
                        } else if refs.is_empty() {
                            Bool::from_bool(ctx, false)
                        } else {
                            Bool::or(ctx, &refs)
                        });
                    }
                    ("edges", 1) => {

                        if vars.len() != 2 { return None; }
                        let arg = &args[0];
                        let Expr::Identifier(seq_name) = arg else { return None };
                        let seq_var = env.get(seq_name)?;
                        let (n, bind): (i64, Box<dyn Fn(&mut HashMap<String, Var<'ctx>>, i64, &str) -> bool>) =
                            if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {
                                let arr = arr.clone(); let fields = fields.to_vec();
                                let n = len.simplify().as_i64()?;
                                (n, Box::new(move |env2, i, var| {
                                    let idx = Int::from_i64(ctx, i);
                                    let elem_dyn = arr.select(&idx);
                                    bind_composite_fields(env2, &elem_dyn, &fields, dt, var)
                                }))
                            } else if let Some((arr, len, elem)) = seq_var.as_seq() {
                                let arr = arr.clone();
                                let n = len.simplify().as_i64()?;
                                (n, Box::new(move |env2, i, var| {
                                    let idx = Int::from_i64(ctx, i);
                                    let cell = arr.select(&idx);
                                    let v = match elem {
                                        SeqElem::Int  => cell.as_int().map(Var::IntVar),
                                        SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                                        SeqElem::Str  => cell.as_string().map(Var::StrVar),
                                    };
                                    match v {
                                        Some(v) => { env2.insert(var.to_string(), v); true }
                                        None => false,
                                    }
                                }))
                            } else {
                                return None;
                            };
                        for i in 0..(n - 1) {
                            let mut env2 = env.clone();
                            if !bind(&mut env2, i,     &vars[0]) { return None; }
                            if !bind(&mut env2, i + 1, &vars[1]) { return None; }
                            if let Some(b) = encode_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                        let refs: Vec<&Bool> = clauses.iter().collect();
                        return Some(if matches!(e, Expr::Forall(..)) {
                            Bool::and(ctx, &refs)
                        } else if refs.is_empty() {
                            Bool::from_bool(ctx, false)
                        } else {
                            Bool::or(ctx, &refs)
                        });
                    }
                    _ => return None,
                }
            }

            if vars.len() != 1 { return None; }
            let var = &vars[0];

            if let Some((lo, hi)) = literal_range(range, ctx, env) {
                for i in lo..=hi {
                    let mut env2 = env.clone();
                    env2.insert(var.clone(), Var::IntVar(Int::from_i64(ctx, i)));
                    if let Some(b) = encode_bool(body, ctx, &env2, schemas) {
                        clauses.push(b);
                    }
                }

            } else if let Some(handle) = (!matches!(range.as_ref(), Expr::Identifier(_)))
                .then(|| resolve_seq_handle(range.as_ref(), ctx, env))
                .flatten()
            {

                let n = handle.len().simplify().as_i64()?;
                match &handle {
                    SeqHandleRef::Composite { arr, dt, fields, .. } => {
                        for i in 0..n {
                            let mut env2 = env.clone();
                            let idx = Int::from_i64(ctx, i);
                            let elem_dyn = arr.select(&idx);
                            if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                                return None;
                            }
                            if let Some(b) = encode_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                    }
                    SeqHandleRef::Primitive { arr, elem, .. } => {
                        for i in 0..n {
                            let mut env2 = env.clone();
                            let idx = Int::from_i64(ctx, i);
                            let cell = arr.select(&idx);
                            let v = match elem {
                                SeqElem::Int  => cell.as_int().map(Var::IntVar),
                                SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                                SeqElem::Str  => cell.as_string().map(Var::StrVar),
                            };
                            let v = v?;
                            env2.insert(var.clone(), v);
                            if let Some(b) = encode_bool(body, ctx, &env2, schemas) {
                                clauses.push(b);
                            }
                        }
                    }
                }
            } else if let Expr::Identifier(seq_name) = range.as_ref() {
                let seq_var = env.get(seq_name)?;
                if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {

                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env.clone();
                        let idx = Int::from_i64(ctx, i);
                        let elem_dyn = arr.select(&idx);
                        if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                            return None;
                        }
                        if let Some(b) = encode_bool(body, ctx, &env2, schemas) {
                            clauses.push(b);
                        }
                    }
                } else if let Some((arr, len, elem)) = seq_var.as_seq() {

                    let n = len.simplify().as_i64()?;
                    for i in 0..n {
                        let mut env2 = env.clone();
                        let idx = Int::from_i64(ctx, i);
                        let cell = arr.select(&idx);
                        let v = match elem {
                            SeqElem::Int  => cell.as_int().map(Var::IntVar),
                            SeqElem::Bool => cell.as_bool().map(Var::BoolVar),
                            SeqElem::Str  => cell.as_string().map(Var::StrVar),
                        };
                        let v = v?;
                        env2.insert(var.clone(), v);
                        if let Some(b) = encode_bool(body, ctx, &env2, schemas) {
                            clauses.push(b);
                        }
                    }
                } else if let Some((set, _elem)) = seq_var.as_set() {

                    if let Some(other_set) = match_set_subset_body(body, var, env) {
                        let b = set.set_subset(other_set);
                        return Some(if matches!(e, Expr::Forall(..)) {
                            b
                        } else {
                            b.not().not()

                        });
                    }
                    return None;
                } else if let Some((set, _, _, _, _)) = seq_var.as_datatype_set() {

                    if let Some(other_set) = match_set_subset_body(body, var, env) {
                        let b = set.set_subset(other_set);
                        return Some(if matches!(e, Expr::Forall(..)) { b } else { b });
                    }
                    return None;
                } else {

                    return None;
                }
            } else {

                return None;
            }

            let refs: Vec<&Bool> = clauses.iter().collect();
            if matches!(e, Expr::Forall(..)) {
                Some(Bool::and(ctx, &refs))
            } else {
                if refs.is_empty() { Some(Bool::from_bool(ctx, false)) }
                else                { Some(Bool::or(ctx, &refs)) }
            }
        }
        Expr::Binary(op, lhs, rhs) => match op {

            BinOp::And => {
                let l = encode_bool(lhs, ctx, env, schemas)?;
                let r = encode_bool(rhs, ctx, env, schemas)?;
                Some(Bool::and(ctx, &[&l, &r]))
            }
            BinOp::Or => {
                let l = encode_bool(lhs, ctx, env, schemas)?;
                let r = encode_bool(rhs, ctx, env, schemas)?;
                Some(Bool::or(ctx, &[&l, &r]))
            }
            BinOp::Implies => {
                let l = encode_bool(lhs, ctx, env, schemas)?;
                let r = encode_bool(rhs, ctx, env, schemas)?;
                Some(l.implies(&r))
            }

            BinOp::Eq | BinOp::Neq => {

                if let Some(b) = encode_cons_chain_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = encode_cons_chain_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }

                if let Some(b) = encode_seq_lit_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = encode_seq_lit_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }

                if let Some(b) = encode_set_lit_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = encode_set_lit_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }

                if let Some(b) = encode_seq_eq(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }

                if let Some(b) = encode_seq_index_assign(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = encode_seq_index_assign(rhs, lhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (encode_bool(lhs, ctx, env, schemas), encode_bool(rhs, ctx, env, schemas))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (encode_int(lhs, ctx, env), encode_int(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }

                if let (Some(l), Some(r)) =
                    (encode_real(lhs, ctx, env), encode_real(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (encode_str(lhs, ctx, env), encode_str(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }

                let target_hint = match lhs.as_ref() {
                    Expr::Identifier(n) => env.get(n).and_then(|v| match v {
                        Var::EnumVar { enum_name, dt, .. } => Some((enum_name.clone(), *dt)),
                        _ => None,
                    }),
                    _ => None,
                };
                let pair = with_target_enum_hint(target_hint.clone(), || {
                    let l = resolve_enum_ast(lhs, ctx, env, schemas);
                    let r = resolve_enum_ast(rhs, ctx, env, schemas);
                    (l, r)
                });
                if let (Some(l), Some(r)) = pair {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }

                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }

            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if let (Some(l), Some(r)) =
                    (encode_int(lhs, ctx, env), encode_int(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Lt => l.lt(&r),
                        BinOp::Le => l.le(&r),
                        BinOp::Gt => l.gt(&r),
                        BinOp::Ge => l.ge(&r),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (encode_real(lhs, ctx, env), encode_real(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Lt => l.lt(&r),
                        BinOp::Le => l.le(&r),
                        BinOp::Gt => l.gt(&r),
                        BinOp::Ge => l.ge(&r),
                        _ => unreachable!(),
                    });
                }

                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            _ => None,
        }
        _ => None,
    }
}

pub(super) fn literal_range<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<(i64, i64)> {
    if let Expr::Range(lo, hi) = e {
        let lo_z3 = encode_int(lo, ctx, env)?;
        let hi_z3 = encode_int(hi, ctx, env)?;
        let lo_v = lo_z3.simplify().as_i64()?;
        let hi_v = hi_z3.simplify().as_i64()?;
        return Some((lo_v, hi_v));
    }
    None
}
