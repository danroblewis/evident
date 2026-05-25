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
        let mut env2 = env.clone();
        let mut testers: Vec<Bool<'ctx>> = Vec::new();
        compile_pattern(&arm.pattern, &scr_dt, dt, &scr_enum_name,
                        &mut env2, &mut testers)?;
        // The arm's guard is the conjunction of every recognizer tester
        // collected while walking the (possibly nested) pattern. A
        // pattern that contributes no testers (`Wildcard` or a top-level
        // `Bind`) is a catch-all — its guard is `None`, which
        // `fold_arms_to_ite` treats as the trailing else.
        let combined: Option<Bool<'ctx>> = match testers.len() {
            0 => None,
            1 => Some(testers.pop().unwrap()),
            _ => {
                let refs: Vec<&Bool<'ctx>> = testers.iter().collect();
                Some(Bool::and(ctx, &refs))
            }
        };
        let body = body_translator(&arm.body, &env2)?;
        compiled.push((combined, body));
    }
    Some(compiled)
}

/// Recursively match `pat` against the Z3 datatype value `scr_dt` (of
/// sort `dt`, enum `enum_name`). Appends a recognizer tester per
/// constructor level to `testers` (their conjunction is the arm's
/// guard) and inserts payload bindings into `env`. A nested constructor
/// sub-pattern recurses, conjoining its own tester — so
/// `Node(Leaf(n), r)` matches only when the outer value is a `Node` AND
/// its first field is a `Leaf`, binding `n` and `r`.
///
/// Returns `None` on a shape mismatch (unknown variant, arity mismatch
/// against the constructor's physical accessors, or an unsupported
/// payload kind such as a `Seq`-typed field) — the whole `match` then
/// drops as untranslatable, the same loud failure as before.
fn compile_pattern<'ctx>(
    pat: &MatchPattern,
    scr_dt: &z3::ast::Datatype<'ctx>,
    dt: &'static DatatypeSort<'static>,
    enum_name: &str,
    env: &mut HashMap<String, Var<'ctx>>,
    testers: &mut Vec<Bool<'ctx>>,
) -> Option<()> {
    match pat {
        MatchPattern::Wildcard => Some(()),
        MatchPattern::Bind(name) => {
            // Bind the whole value (catch-all). Carry the enum sort so
            // the bound name can itself be `match`ed downstream.
            env.insert(name.clone(), Var::EnumVar {
                ast: scr_dt.clone(),
                enum_name: enum_name.to_string(),
                dt,
            });
            Some(())
        }
        MatchPattern::Ctor { name, binds } => {
            let var_idx = dt.variants.iter()
                .position(|v| v.constructor.name() == *name)?;
            let z3_var = &dt.variants[var_idx];
            // One bind per physical accessor. Seq-typed fields expand to
            // two accessors (arr, len) / one __SeqOf_T cons accessor, so
            // this guard refuses Seq-payload patterns (unchanged from the
            // prior behavior — Seq-in-match isn't supported yet).
            if binds.len() != z3_var.accessors.len() { return None; }
            testers.push(z3_var.tester.apply(&[scr_dt]).as_bool()?);
            let field_decls: Vec<crate::core::ast::EnumField> = with_active_enums(|enums| {
                enums.and_then(|er| {
                    er.by_name.borrow().get(enum_name)
                        .and_then(|(_, variants)| variants.iter()
                            .find(|v| v.name == *name)
                            .map(|v| v.fields.clone()))
                })
            }).unwrap_or_default();
            for (j, sub) in binds.iter().enumerate() {
                let raw = z3_var.accessors[j].apply(&[scr_dt]);
                compile_field(sub, &raw, field_decls.get(j), enum_name, dt,
                              env, testers)?;
            }
            Some(())
        }
    }
}

/// Match a sub-pattern against one payload field's raw Z3 value.
/// Primitive fields bind to their scalar Var; enum fields either bind
/// (as `EnumVar`, carrying the field's enum sort) or recurse into a
/// nested constructor pattern.
fn compile_field<'ctx>(
    sub: &MatchPattern,
    raw: &z3::ast::Dynamic<'ctx>,
    field_decl: Option<&crate::core::ast::EnumField>,
    parent_enum: &str,
    parent_dt: &'static DatatypeSort<'static>,
    env: &mut HashMap<String, Var<'ctx>>,
    testers: &mut Vec<Bool<'ctx>>,
) -> Option<()> {
    match sub {
        MatchPattern::Wildcard => Some(()),
        MatchPattern::Bind(name) => {
            let var = if let Some(i) = raw.as_int() { Var::IntVar(i) }
                else if let Some(b) = raw.as_bool() { Var::BoolVar(b) }
                else if let Some(s) = raw.as_string() { Var::StrVar(s) }
                else if let Some(r) = raw.as_real() { Var::RealVar(r) }
                else if let Some(payload_dt) = raw.as_datatype() {
                    let (ftype, fsort) =
                        field_enum_sort(field_decl, parent_enum, parent_dt);
                    Var::EnumVar { ast: payload_dt, enum_name: ftype, dt: fsort }
                }
                else { return None; };
            env.insert(name.clone(), var);
            Some(())
        }
        MatchPattern::Ctor { .. } => {
            // A nested constructor sub-pattern only matches an enum field.
            let payload_dt = raw.as_datatype()?;
            let (ftype, fsort) =
                field_enum_sort(field_decl, parent_enum, parent_dt);
            compile_pattern(sub, &payload_dt, fsort, &ftype, env, testers)
        }
    }
}

/// Resolve a payload field's enum type name + sort. For self-recursion
/// (a field whose type is the parent enum itself) this is the parent's
/// own sort; for a cross-enum field we look the type up in the active
/// `EnumRegistry`, falling back to the parent's sort if unknown.
fn field_enum_sort(
    field_decl: Option<&crate::core::ast::EnumField>,
    parent_enum: &str,
    parent_dt: &'static DatatypeSort<'static>,
) -> (String, &'static DatatypeSort<'static>) {
    let ftype = field_decl
        .map(|f| f.type_name.clone())
        .unwrap_or_else(|| parent_enum.to_string());
    let fsort = with_active_enums(|enums| {
        enums.and_then(|er| er.by_name.borrow().get(&ftype).map(|(d, _)| *d))
    }).unwrap_or(parent_dt);
    (ftype, fsort)
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
