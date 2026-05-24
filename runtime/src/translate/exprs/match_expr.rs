//! Match-expression translator.
//!
//! `match scrutinee
//!      Ctor(b1, ...) ⇒ body
//!      _             ⇒ fallback`
//!
//! translates to a nested Z3 `Bool::ite(...)` chain over the
//! constructor-recognizer (tester) booleans. Each non-wildcard arm's
//! body is translated with payload bindings extended into a cloned env.
//!
//! v1 limitations:
//!   - Scrutinee must be a bare Identifier (Var::EnumVar in env).
//!   - Payload bindings are restricted to Int / Bool / String / Real
//!     fields. Enum-typed payloads can use `_` to discard but not bind.
//!   - Exhaustiveness isn't enforced — if no arm matches at runtime,
//!     the last arm's body is used as the trailing else (which may
//!     fire incorrectly if the user omitted variants).

use std::collections::HashMap;
use z3::ast::Bool;
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::Var;

use super::scalar::translate_int;
use super::with_active_enums;

/// One compiled arm: an optional tester boolean (None = wildcard) and
/// the translated body in a per-arm extended env. Type T is the body's
/// Z3 sort (Int / Bool / Z3Str / Real / Datatype).
pub(super) type CompiledArm<'ctx, T> = (Option<Bool<'ctx>>, T);

/// Resolve the scrutinee + walk arms, returning a Vec of (tester, body).
/// Body translation is delegated to `body_translator` so the same
/// machinery serves Int / Bool / Str / Real / Enum match results.
pub(super) fn translate_match_arms<'ctx, T>(
    scr: &Expr,
    arms: &[crate::core::ast::MatchArm],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    body_translator: impl Fn(&Expr, &HashMap<String, Var<'ctx>>) -> Option<T>,
) -> Option<Vec<CompiledArm<'ctx, T>>> {
    use crate::core::ast::MatchPattern;
    // Scrutinee shapes supported:
    //   * Bare Identifier resolving to Var::EnumVar.
    //   * Index(Identifier(seq), idx) where `seq` is a Var::DatatypeSeqVar
    //     with empty fields (i.e. Seq(EnumType)) — element pulled via
    //     arr.select(idx). Lets `match last_results[0]` reach the same
    //     arm machinery as bare-identifier matches.
    let (scr_dt, dt, scr_enum_name) = match scr {
        Expr::Identifier(n) if !n.contains('.') => {
            match env.get(n)? {
                Var::EnumVar { ast, dt, enum_name } =>
                    (ast.clone(), *dt, enum_name.clone()),
                Var::EnumValue { .. } => return None,
                _ => return None,
            }
        }
        Expr::Index(seq_expr, idx_expr) => {
            let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
            if seq_name.contains('.') { return None; }
            let (arr, dt, type_name) = match env.get(seq_name)? {
                Var::DatatypeSeqVar { arr, dt, type_name, fields, .. }
                    if fields.is_empty() =>
                        (arr.clone(), *dt, type_name.clone()),
                _ => return None,
            };
            let idx = translate_int(idx_expr, ctx, env)?;
            let elem_dt = arr.select(&idx).as_datatype()?;
            (elem_dt, dt, type_name)
        }
        _ => return None,
    };
    let mut compiled: Vec<CompiledArm<T>> = Vec::new();
    for arm in arms {
        match &arm.pattern {
            MatchPattern::Wildcard => {
                let body = body_translator(&arm.body, env)?;
                compiled.push((None, body));
            }
            MatchPattern::Ctor { name, binds } => {
                let var_idx = dt.variants.iter()
                    .position(|v| v.constructor.name() == *name)?;
                let z3_var = &dt.variants[var_idx];
                if binds.len() != z3_var.accessors.len() { return None; }
                let tester = z3_var.tester.apply(&[&scr_dt]).as_bool()?;
                let mut env2 = env.clone();
                let scr_enum_name = scr_enum_name.clone();
                let field_decls: Vec<crate::core::ast::EnumField> = with_active_enums(|enums| {
                    enums.and_then(|er| {
                        er.by_name.borrow().get(&scr_enum_name)
                            .and_then(|(_, variants)| {
                                variants.iter()
                                    .find(|v| v.name == *name)
                                    .map(|v| v.fields.clone())
                            })
                    }).unwrap_or_default()
                });
                for (j, bind_opt) in binds.iter().enumerate() {
                    let Some(bind_name) = bind_opt else { continue };
                    let acc = &z3_var.accessors[j];
                    let raw = acc.apply(&[&scr_dt]);
                    // Try each primitive sort first.
                    let var = if let Some(i) = raw.as_int() { Var::IntVar(i) }
                        else if let Some(b) = raw.as_bool() { Var::BoolVar(b) }
                        else if let Some(s) = raw.as_string() { Var::StrVar(s) }
                        else if let Some(r) = raw.as_real() { Var::RealVar(r) }
                        else if let Some(payload_dt) = raw.as_datatype() {
                            // Enum-typed payload. The field's type name
                            // comes from the EnumField list we looked up
                            // above. For self-recursion the type matches
                            // the scrutinee; for cross-enum we look up
                            // the field's type in the EnumRegistry.
                            let field_type = field_decls.get(j)
                                .map(|f| f.type_name.clone())
                                .unwrap_or_else(|| scr_enum_name.clone());
                            let payload_dt_sort: &'static DatatypeSort<'static> =
                                with_active_enums(|enums| {
                                    enums.and_then(|er| {
                                        er.by_name.borrow().get(&field_type)
                                            .map(|(d, _)| *d)
                                    })
                                }).unwrap_or(dt);  // fall back to scrutinee's dt
                            Var::EnumVar {
                                ast: payload_dt,
                                enum_name: field_type,
                                dt: payload_dt_sort,
                            }
                        }
                        else { return None; };
                    env2.insert(bind_name.clone(), var);
                }
                let body = body_translator(&arm.body, &env2)?;
                compiled.push((Some(tester), body));
            }
        }
    }
    Some(compiled)
}

/// Fold compiled arms bottom-up into a nested ITE. Last arm's body
/// becomes the trailing else; any earlier wildcard arm short-circuits
/// (its body becomes the new accumulator).
pub(super) fn fold_arms_to_ite<'ctx, T>(
    mut compiled: Vec<CompiledArm<'ctx, T>>,
) -> Option<T>
where
    T: z3::ast::Ast<'ctx>,
{
    if compiled.is_empty() { return None; }
    let (_, last_body) = compiled.pop()?;
    let mut acc = last_body;
    for (tester_opt, body) in compiled.into_iter().rev() {
        match tester_opt {
            None       => { acc = body; }
            Some(tester) => { acc = tester.ite(&body, &acc); }
        }
    }
    Some(acc)
}
