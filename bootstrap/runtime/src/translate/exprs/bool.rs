//! `translate_bool`: Bool-sort dispatcher for builtins (`contains`, `distinct`), recognizers,
//! Seq/Set membership, quantifiers, and binary ops (routed through seq-eq/set-eq/record-lift/enum).

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::Context;

use crate::core::ast::*;
use crate::core::{SeqElem, Var};

use super::enums::resolve_enum_ast;
use super::match_expr::{fold_arms_to_ite, translate_match_arms};
use super::record_lift::lift_record_op;
use super::scalar::{translate_int, translate_real, translate_str};
use super::seq_eq::{
    build_composite_dynamic, translate_cons_chain_eq, translate_seq_eq,
    translate_seq_index_assign, translate_seq_lit_eq, translate_set_lit_eq,
};
use super::seq_field::{resolve_seq_field, resolve_seq_handle, SeqHandleRef};
use super::with_target_enum_hint;

pub(crate) fn translate_bool<'ctx>(
    e: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    if let Expr::Call(name, args) = e {
        // String builtins (str_contains/starts_with/ends_with) checked before seq `contains`.
        if let Some(b) = super::string_ops::translate_str_bool(name, args, ctx, env) {
            return Some(b);
        }
        if name == "contains" && args.len() == 2 {
            let Expr::Identifier(seq_name) = &args[0] else { return None };
            let var = env.get(seq_name)?;
            // Primitive Seq path (SeqInt / SeqBool / SeqStr).
            if let Some((arr, len, elem)) = var.as_seq() {
                let n = len.simplify().as_i64()?;
                let mut clauses: Vec<Bool> = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    let eq = match elem {
                        SeqElem::Int => {
                            let v = translate_int(&args[1], ctx, env)?;
                            cell.as_int()?._eq(&v)
                        }
                        SeqElem::Bool => {
                            let v = translate_bool(&args[1], ctx, env, schemas)?;
                            cell.as_bool()?._eq(&v)
                        }
                        SeqElem::Str => {
                            let v = translate_str(&args[1], ctx, env)?;
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
            // Datatype Seq path (Seq(UserType) or Seq(EnumType)).
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
                    if let Some(b) = translate_bool(&eq_expr, ctx, env, schemas) {
                        clauses.push(b);
                    } else {
                        let _ = cell; // silence unused
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
            // Single Seq arg: unroll to elements. None on unpinned Seq (drops loudly).
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
                return translate_bool(
                    &Expr::Call("distinct".into(), exploded),
                    ctx, env, schemas);
            }
            if let Some(ints) = args.iter()
                .map(|a| translate_int(a, ctx, env))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Int> = ints.iter().collect();
                return Some(Int::distinct(ctx, &refs));
            }
            if let Some(bools) = args.iter()
                .map(|a| translate_bool(a, ctx, env, schemas))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Bool> = bools.iter().collect();
                return Some(Bool::distinct(ctx, &refs));
            }
            if let Some(strs) = args.iter()
                .map(|a| translate_str(a, ctx, env))
                .collect::<Option<Vec<_>>>()
            {
                let refs: Vec<&Z3Str> = strs.iter().collect();
                return Some(Z3Str::distinct(ctx, &refs));
            }
            return None;
        }
        // Z3 Bool operators that don't have direct Evident surface syntax.
        // Adding a new one is one match-arm here, following the distinct
        // precedent above (the user's "all Z3 predicates" extension recipe).
        if name == "xor" && args.len() == 2 {
            let a = translate_bool(&args[0], ctx, env, schemas)?;
            let b = translate_bool(&args[1], ctx, env, schemas)?;
            return Some(a.xor(&b));
        }
        if name == "iff" && args.len() == 2 {
            let a = translate_bool(&args[0], ctx, env, schemas)?;
            let b = translate_bool(&args[1], ctx, env, schemas)?;
            return Some(a.iff(&b));
        }
        if name == "ite" && args.len() == 3 {
            let cond = translate_bool(&args[0], ctx, env, schemas)?;
            let t = translate_bool(&args[1], ctx, env, schemas)?;
            let e = translate_bool(&args[2], ctx, env, schemas)?;
            return Some(cond.ite(&t, &e));
        }
    }
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(translate_bool(inner, ctx, env, schemas)?.not()),

        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, schemas)?;
            let then_v = translate_bool(a, ctx, env, schemas)?;
            let else_v = translate_bool(b, ctx, env, schemas)?;
            Some(cond.ite(&then_v, &else_v))
        }
        Expr::Match(scr, arms) => {
            let compiled = translate_match_arms(scr, arms, ctx, env,
                |body, e| translate_bool(body, ctx, e, schemas))?;
            fold_arms_to_ite(compiled)
        }
        // `e matches Pattern` — recognizer; payload binds ignored (use `match` to extract).
        Expr::Matches(e, pattern) => {
            use crate::core::ast::MatchPattern;
            match pattern {
                MatchPattern::Wildcard | MatchPattern::Bind(_) =>
                    Some(Bool::from_bool(ctx, true)),
                // Tests outer variant tag only; nested sub-patterns ignored.
                MatchPattern::Ctor { name, .. } => {
                    // The scrutinee is an Identifier, BARE OR DOTTED. A dotted
                    // name (`c.t`) is record-field access: `declare_var_named`
                    // flattens an enum-typed record field into an env entry
                    // keyed by the dotted name, so `env.get("c.t")` is the
                    // field's EnumVar. (Pre-fix this arm guarded on
                    // `!n.contains('.')` and dropped field-access scrutinees
                    // vacuously-SAT — context-bundles.md gap, fixture 155.)
                    // The `Var::EnumVar` match below is the real gate: a
                    // non-enum scrutinee still falls to `_ => return None`.
                    let scr_name = match e.as_ref() {
                        Expr::Identifier(n) => n,
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
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_bool()
        }
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "Bool" { raw.as_bool() } else { None }
        }

        // `x ∈ {a,b,c}` → disjunction; `x ∈ s` (SetVar) → native membership; `x ∈ s` (String) → contains.
        Expr::InExpr(lhs, rhs) => {
            if let Expr::Identifier(name) = rhs.as_ref() {
                if let Some((set, elem)) = env.get(name).and_then(|v| v.as_set()) {
                    return match elem {
                        SeqElem::Int => {
                            let x = translate_int(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Bool => {
                            let x = translate_bool(lhs, ctx, env, schemas)?;
                            Some(set.member(&x))
                        }
                        SeqElem::Str => {
                            let x = translate_str(lhs, ctx, env)?;
                            Some(set.member(&x))
                        }
                    };
                }
                // Composite-element Set: build composite Dynamic, use native set.member.
                if let Some((set, _, dt, fields, _)) =
                    env.get(name).and_then(|v| v.as_datatype_set())
                {
                    if let Expr::Identifier(ident) = lhs.as_ref() {
                        let dyn_val = build_composite_dynamic(ident, dt, fields, ctx, env)?;
                        return Some(set.member(&dyn_val));
                    }
                }
            }
            // String containment falls through after Set paths.
            if let (Some(needle), Some(hay)) =
                (translate_str(lhs, ctx, env), translate_str(rhs, ctx, env))
            {
                return Some(hay.contains(&needle));
            }
            // Set-literal RHS: reduce to OR of equalities.
            let items = match rhs.as_ref() {
                Expr::SetLit(items) => items.clone(),
                _ => return None,
            };
            let mut clauses: Vec<Bool> = Vec::with_capacity(items.len());
            for it in &items {
                let eq = Expr::Binary(BinOp::Eq, lhs.clone(), Box::new(it.clone()));
                if let Some(b) = translate_bool(&eq, ctx, env, schemas) {
                    clauses.push(b);
                }
            }
            if clauses.is_empty() { return Some(Bool::from_bool(ctx, false)); }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::or(ctx, &refs))
        }

        Expr::Forall(vars, range, body) | Expr::Exists(vars, range, body) =>
            super::quant::translate_quantifier(e, vars, range, body, ctx, env, schemas),
        Expr::Binary(op, lhs, rhs) => match op {
            // Boolean combinators
            BinOp::And => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(Bool::and(ctx, &[&l, &r]))
            }
            BinOp::Or => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(Bool::or(ctx, &[&l, &r]))
            }
            BinOp::Implies => {
                let l = translate_bool(lhs, ctx, env, schemas)?;
                let r = translate_bool(rhs, ctx, env, schemas)?;
                Some(l.implies(&r))
            }
            BinOp::Eq | BinOp::Neq => {
                // Try Cons/Nil-chain, SeqLit, SetLit, whole-Seq, seq-index-assign,
                // Bool/Int/Real/String, then enum and record broadcast.
                if let Some(b) = translate_cons_chain_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_cons_chain_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_lit_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_lit_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_set_lit_eq(lhs, rhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_set_lit_eq(rhs, lhs, ctx, env, schemas) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_eq(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_index_assign(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let Some(b) = translate_seq_index_assign(rhs, lhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_bool(lhs, ctx, env, schemas), translate_bool(rhs, ctx, env, schemas))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_int(lhs, ctx, env), translate_int(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Real: coerces Int to Real when needed.
                if let (Some(l), Some(r)) =
                    (translate_real(lhs, ctx, env), translate_real(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                if let (Some(l), Some(r)) =
                    (translate_str(lhs, ctx, env), translate_str(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                // Enum equality; set LHS as SeqLit-target hint so ⟨…⟩ in RHS lowers correctly.
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
                    (translate_int(lhs, ctx, env), translate_int(rhs, ctx, env))
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
                    (translate_real(lhs, ctx, env), translate_real(rhs, ctx, env))
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
