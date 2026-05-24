//! Quantifier unrolling — the `∀ vars ∈ <range> : body` / `∃ …` arm of
//! the Bool dispatcher, factored out of `translate_bool`. Handles the
//! five range shapes (integer range, composite seq, primitive seq,
//! `coindexed(...)`, `edges(...)`) plus the Set-subset pattern, each
//! unrolled to a Z3 conjunction (∀) or disjunction (∃).

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int};
use z3::Context;

use crate::core::ast::*;
use crate::core::{SeqElem, Var};

use super::bool::translate_bool;
use super::range::literal_range;
use super::seq_eq::{bind_composite_fields, match_set_subset_body};
use super::seq_field::{resolve_seq_handle, SeqHandleRef};

/// Translate `Expr::Forall(vars, range, body)` / `Expr::Exists(...)`.
/// `e` is the original quantifier node (used to distinguish ∀ from ∃).
///
/// Range shapes:
///   1. Integer range `{lo..hi}` — unrolls lo..=hi, binds the
///      single var to each Int. Single-var binding only.
///   2. Composite seq `state.dots` (Seq(UserType)) — unrolls
///      0..len, binds `var.field` to each leaf of state.dots[i].
///      Single-var only.
///   3. Primitive seq `s` (Seq(Int|Bool|String)) — unrolls
///      0..len, binds the single var to each element.
///   4. `coindexed(A, B, C)` — N-arity zip. Tuple binding required;
///      each iteration binds vars[k] to seqs[k][i] (positionally
///      across all sequences).
///   5. `edges(seq)` — consecutive-pair iteration. 2-tuple binding;
///      each iteration binds vars[0] to seq[i], vars[1] to seq[i+1].
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

    // Form 4: coindexed(A, B, …) — tuple-binding required.
    if let Expr::Call(name, args) = range {
        match (name.as_str(), args.len()) {
            ("coindexed", n_seqs) if n_seqs >= 1 => {
                if vars.len() != n_seqs {
                    return None; // arity mismatch — let the caller's
                                 // dropped-constraint path surface it
                }
                // All sequences must have the same pinned length.
                // Build the (Var-handle, length) per sequence so we
                // can iterate and bind each var per index.
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
                // edges(seq) — adjacent-pair iteration, requires
                // a 2-tuple binding. Each step binds vars[0] to
                // seq[i] and vars[1] to seq[i+1] for i in 0..n-1.
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
            _ => return None,    // unknown function in quantifier range
        }
    }

    // Forms 1–3 require a single-name binding.
    if vars.len() != 1 { return None; }
    let var = &vars[0];

    // Form 1: integer range.
    if let Some((lo, hi)) = literal_range(range, ctx, env) {
        for i in lo..=hi {
            let mut env2 = env.clone();
            env2.insert(var.clone(), Var::IntVar(Int::from_i64(ctx, i)));
            if let Some(b) = translate_bool(body, ctx, &env2, schemas) {
                clauses.push(b);
            }
        }
    // Form 2 / 3: iterate over a Seq variable.
    } else if let Some(handle) = (!matches!(range, Expr::Identifier(_)))
        .then(|| resolve_seq_handle(range, ctx, env))
        .flatten()
    {
        // Forall over a non-Identifier seq expression — typically
        // `∀ x ∈ outer[i].seq_field : …`. Reuses the same
        // primitive-vs-composite element machinery as the
        // Identifier path below, but pulls (arr, len) from the
        // resolved handle.
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
            // Composite seq: iterate elements, bind <var>.<field>
            // for each declared field in env on each iteration.
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
            // Primitive seq: bind `var` to the element directly.
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
            // Primitive-element Set: detect the subset pattern
            // `∀ x ∈ a : x ∈ b` and emit Z3 native set_subset.
            // Used for both pinned and free Sets — works without
            // iteration. Anything else over a primitive Set is
            // unsupported in v1.
            if let Some(other_set) = match_set_subset_body(body, var, env) {
                let b = set.set_subset(other_set);
                return Some(if matches!(e, Expr::Forall(..)) {
                    b
                } else {
                    b.not().not()    // ∃ x ∈ a : x ∈ b is "a ∩ b ≠ ∅"
                                      // — different semantics; we don't
                                      // model existence here.
                });
            }
            return None;
        } else if let Some((set, _, _, _, _)) = seq_var.as_datatype_set() {
            // Composite-element Set: same subset pattern as the
            // primitive case. The pattern is `∀ e ∈ a : e ∈ b`
            // where the body's `e` was a flat-expanded composite;
            // both `a` and `b` must be DatatypeSetVars over the
            // same datatype.
            if let Some(other_set) = match_set_subset_body(body, var, env) {
                let b = set.set_subset(other_set);
                return Some(if matches!(e, Expr::Forall(..)) { b } else { b });
            }
            return None;
        } else {
            // Identifier in scope but not a seq — can't iterate.
            return None;
        }
    } else {
        // Range expression we don't recognize.
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
