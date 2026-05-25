//! `translate_bool` — the Bool-sort dispatcher. Handles builtins
//! (`contains`, `distinct`), recognizers (`matches`), Seq/Set membership,
//! quantifier unrolling (`∀` / `∃` over ranges, seqs, coindexed, edges),
//! and every binary operator, routing equality/comparison through the
//! seq-eq, set-eq, record-lift, and enum helpers as needed.

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
    // `distinct(a, b, c, …)` — Z3's all-different primitive. Two
    // call shapes:
    //   * Variadic over scalar args: `distinct(a, b, c)`. All args
    //     translate to the same Z3 sort. v1 supports Int / Bool /
    //     String; picks the first sort that translates every arg.
    //   * Single Seq arg with pinned length: `distinct(seq)`.
    //     Unrolls to `distinct(seq[0], seq[1], …, seq[n-1])`
    //     and recurses through the variadic path.
    // 0 or 1 args is trivially true.
    // `contains(seq, x)` — true if x ∈ seq. The `x ∈ seq` infix
    // form is silently dropped today for element-in-Seq; this
    // builtin makes the operation explicit and translates. For a
    // pinned-length Seq, unrolls to a disjunction of element
    // equalities `seq[0] = x ∨ seq[1] = x ∨ … ∨ seq[n-1] = x`.
    if let Expr::Call(name, args) = e {
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
                // Translate x as a Call/Identifier that resolves to a
                // datatype value via the existing seq-element handling.
                // For simplicity: build seq[i] = x for each i.
                let mut clauses: Vec<Bool> = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let idx = Int::from_i64(ctx, i);
                    let cell = arr.select(&idx);
                    // Compare via the cell's _eq against translated x.
                    // For datatype types, we need translate_x_as_datatype;
                    // best-effort via the existing translate_bool's Eq path
                    // by constructing `cell_value = arg`.
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
            // 0 args: trivially true (no pair to differ).
            if args.is_empty() { return Some(Bool::from_bool(ctx, true)); }
            // 1 arg: must be a pinned-length Seq variable.
            // Returning None on failure (not vacuous true) so a
            // `distinct(s)` over an unpinned Seq surfaces as a
            // dropped constraint instead of silently passing.
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
    }
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(translate_bool(inner, ctx, env, schemas)?.not()),

        // `cond ? a : b` with Bool branches → Z3 ITE.
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
        // `e matches Pattern` — constructor recognizer. Returns Bool.
        // Wildcard pattern → always true. Ctor pattern → is_Ctor(e).
        // Payload binds in the pattern are IGNORED (use `match` to
        // bind, or `e = Ctor(literal)` to compare payload values).
        Expr::Matches(e, pattern) => {
            use crate::core::ast::MatchPattern;
            match pattern {
                // A wildcard or a bare binding matches any value.
                MatchPattern::Wildcard | MatchPattern::Bind(_) =>
                    Some(Bool::from_bool(ctx, true)),
                // `e matches Ctor(...)` tests only the outer variant tag;
                // nested sub-patterns are ignored here (use `match` to
                // deep-test/extract). Same shallow contract as before.
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

        // `seq[i]` where seq holds Bool elements. Accepts both bare
        // Identifier seqs and Seq-field accesses via the unified
        // `resolve_seq_handle` helper (handles e.g. `groups[0].flags[2]`).
        Expr::Index(seq_expr, idx_expr) => {
            let handle = resolve_seq_handle(seq_expr.as_ref(), ctx, env)?;
            let SeqHandleRef::Primitive { arr, elem, .. } = handle else { return None };
            if elem != SeqElem::Bool { return None; }
            let i = translate_int(idx_expr, ctx, env)?;
            arr.select(&i).as_bool()
        }
        // `pts[i].active` where pts is Seq(UserType) and `active` is a
        // Bool field.
        Expr::Field(_, _) => {
            let (raw, ftype) = resolve_seq_field(e, ctx, env)?;
            if ftype == "Bool" { raw.as_bool() } else { None }
        }

        // `x ∈ {a, b, c}` → x = a ∨ x = b ∨ x = c.
        // `x ∈ s` where s is a Set var → s.member(x).
        Expr::InExpr(lhs, rhs) => {
            // Set-var RHS (Identifier whose env entry is SetVar): use Z3's
            // native set membership.
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
                // Composite-element Set: LHS must be an Identifier whose
                // flat-expanded fields exist in env (same shape as for
                // `Seq(Composite)` element references). Build the
                // composite Dynamic and use Z3 native set.member.
                if let Some((set, _, dt, fields, _)) =
                    env.get(name).and_then(|v| v.as_datatype_set())
                {
                    if let Expr::Identifier(ident) = lhs.as_ref() {
                        let dyn_val = build_composite_dynamic(ident, dt, fields, ctx, env)?;
                        return Some(set.member(&dyn_val));
                    }
                }
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

        // `∀ vars ∈ <range> : body` / `∃ …`. Range shapes:
        //
        //   1. Integer range `{lo..hi}` — unrolls lo..=hi, binds the
        //      single var to each Int. Single-var binding only.
        //   2. Composite seq `state.dots` (Seq(UserType)) — unrolls
        //      0..len, binds `var.field` to each leaf of state.dots[i].
        //      Single-var only.
        //   3. Primitive seq `s` (Seq(Int|Bool|String)) — unrolls
        //      0..len, binds the single var to each element.
        //   4. `coindexed(A, B, C)` — N-arity zip. Tuple binding required;
        //      each iteration binds vars[k] to seqs[k][i] (positionally
        //      across all sequences).
        //   5. `edges(seq)` — consecutive-pair iteration. 2-tuple binding;
        //      each iteration binds vars[0] to seq[i], vars[1] to seq[i+1].
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
            // Eq/Neq work over Bool, Int, or String. Try in that order.
            BinOp::Eq | BinOp::Neq => {
                // Cons/Nil-shaped enum SeqLit: `effs = ⟨a, b, c⟩` where
                // `effs` is e.g. EffectList (any enum with a 0-arity
                // variant + a 2-arity self-recursive variant).
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
                // First: handle `seq_var = ⟨e1, e2, …⟩` (sequence literal
                // assignment). This pins both length and per-element values
                // and lives outside the Bool/Int/Str scalar paths because
                // it produces a conjunction over the elements rather than
                // a single _eq.
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
                // `set_var = {a, b, c}` — exact set membership. Mirror of
                // translate_seq_lit_eq but for SetVar + SetLit. Records
                // candidates for the extract path.
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
                // `A = B` (whole-Seq equality between two named Seq
                // vars). Desugars to element-wise equality + length
                // match. Try lhs/rhs in only one direction since the
                // helper is symmetric in operand roles.
                if let Some(b) = translate_seq_eq(lhs, rhs, ctx, env) {
                    return Some(match op {
                        BinOp::Eq  => b,
                        BinOp::Neq => b.not(),
                        _ => unreachable!(),
                    });
                }
                // `seq[i] = composite_var` (single-element composite-seq
                // assignment). Try both orientations.
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
                // Real path: at least one side is Real (RealVar or Real
                // literal); the other side may be Int and gets coerced.
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
                // Enum equality: `today = Mon` where `today` is an
                // EnumVar and `Mon` is an EnumValue (or vice versa, or
                // both EnumValues). Both sides must reference enum-
                // typed identifiers in env. Different enums on the two
                // sides aren't allowed — caller has a type error.
                //
                // If LHS is an enum-typed Identifier, set it as the
                // SeqLit-target hint so any ⟨…⟩ inside RHS (including
                // inside match arm bodies) lowers to the correct
                // Cons/Nil chain.
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
                // Record-op broadcast: handles `=`, `≠` between
                // record-typed expressions on either side, including
                // arithmetic (`vec_lo = vec - offset`).
                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            // Numeric comparisons. Try Int first; fall back to Real
            // (with Int→Real coercion) so `realvar < 3` and
            // `realvar < 3.14` both work.
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
                // Record-op broadcast: `<`, `≤`, `>`, `≥` between
                // record-typed expressions are componentwise. Same
                // helper as Eq/Neq — operator threads through.
                // Handles `vec_lo ≤ vec` and arithmetic-laden forms
                // like `dot.pos - offset_lo ≤ player.pos`.
                lift_record_op(op, lhs, rhs, ctx, env, schemas)
            }
            _ => None,
        }
        _ => None,
    }
}
