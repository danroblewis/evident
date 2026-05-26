//! Quantifier unrolling (`∀`/`∃`) over integer ranges, primitive/composite Seqs,
//! `coindexed(...)`, `edges(...)`, and the Set-subset shortcut.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int};
use z3::Context;

use crate::core::ast::*;
use crate::core::{SeqElem, Var};

use super::bool::translate_bool;
use super::range::literal_range;
use super::seq_eq::{bind_composite_fields, match_set_subset_body};
use super::seq_field::{resolve_seq_handle, SeqHandleRef};

/// Translate `∀`/`∃` quantifier; `e` distinguishes the two. Unrolls into
/// a Z3 conjunction (∀) or disjunction (∃) over the given range shape.
pub(super) fn translate_quantifier<'ctx>(
    e: &Expr,
    vars: &[String],
    range: &Expr,
    body: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let mut clauses: Vec<Bool> = Vec::new();

    if let Expr::Call(name, args) = range {
        match (name.as_str(), args.len()) {
            ("coindexed", n_seqs) if n_seqs >= 1 => {
                if vars.len() != n_seqs { return None; }
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
                    if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
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
                // Adjacent-pair iteration: vars[0]=seq[i], vars[1]=seq[i+1].
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
                    if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
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
            if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                clauses.push(b);
            }
        }
    } else if let Some(handle) = (!matches!(range, Expr::Identifier(_)))
        .then(|| resolve_seq_handle(range, ctx, env))
        .flatten()
    {
        // Non-identifier Seq expression (e.g. `outer[i].seq_field`).
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
                    if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
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
                    if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                        clauses.push(b);
                    }
                }
            }
        }
    } else if let Expr::Identifier(seq_name) = range {
        let seq_var = env.get(seq_name)?;
        if let Some((arr, len, _, dt, fields)) = seq_var.as_datatype_seq() {
            let n = len.simplify().as_i64()?;
            for i in 0..n {
                let mut env2 = env.clone();
                let idx = Int::from_i64(ctx, i);
                let elem_dyn = arr.select(&idx);
                if !bind_composite_fields(&mut env2, &elem_dyn, fields, dt, var) {
                    return None; // shape mismatch — fail loudly
                }
                if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
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
                if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                    clauses.push(b);
                }
            }
        } else if let Some((set, _elem)) = seq_var.as_set() {
            // Detect `∀ x ∈ a : x ∈ b` and emit Z3 set_subset; other Set bodies unsupported.
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
