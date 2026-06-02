//! Body-item dispatch loop: Membership → declare, Constraint → assert, Passthrough / ClaimCall → recurse.
//! Bare-identifier passthroughs are rewritten to `Passthrough` nodes by the desugar pass before this runs.

use std::collections::HashMap;

use z3::{Context, Solver};
use z3::ast::Bool;

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use crate::translate::exprs::translate_bool;
use super::calls;
use super::dispatch::{CallDispatch, body_contains_subschema_call,
                      method_dispatch_call_compat, method_dispatch_name_compat,
                      resolve_call, resolve_call_name};
use super::guards::{guard_is_satisfiable, guarded_bool, track_assert};
use super::membership::inline_membership;
use super::recursion::{exit_frame, try_enter};
use super::subschema;

/// Translate body items into the solver. Recurses through Passthrough and ClaimCall bodies
/// so nested claims are not silently dropped.
#[allow(clippy::too_many_arguments)]
pub(in crate::translate) fn inline_body_items(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
) {
    inline_body_items_guarded(items, env, solver, schemas, ctx, registry, enums, visited, &None, None)
}

/// Like `inline_body_items` but tags each assertion with a tracker for `get_unsat_core`.
/// `trackers[i]` → `items[i]`; tail items are untracked if fewer trackers are supplied.
#[allow(clippy::too_many_arguments)]
pub(in crate::translate) fn inline_body_items_tracked(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    trackers: &[Bool<'static>],
) {
    for (idx, item) in items.iter().enumerate() {
        let tracker = trackers.get(idx);
        let slice = std::slice::from_ref(item);
        inline_body_items_guarded(
            slice, env, solver, schemas, ctx, registry, enums, visited, &None, tracker,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn inline_body_items_guarded(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    for item in items {
        match item {
            BodyItem::Membership { name, type_name, pins } => {
                inline_membership(
                    name, type_name, pins,
                    env, solver, schemas, ctx, registry, enums, guard, tracker,
                );
            }
            // Subschema dispatch (priority): `(args) ∈ recv.subclaim_name` — receiver fields rebound.
            BodyItem::Constraint(Expr::InExpr(lhs, rhs))
                if matches!(resolve_call_name(rhs.as_ref(), items, schemas),
                    Some(CallDispatch::Subschema { .. }))
                && matches!(lhs.as_ref(), Expr::Tuple(_)) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                    resolve_call_name(rhs.as_ref(), items, schemas) else { unreachable!() };
                let args: Vec<Expr> = match lhs.as_ref() {
                    Expr::Tuple(items) => items.clone(),
                    _ => unreachable!(),
                };
                subschema::inline_subschema_call(
                    &recv, &type_name, &claim_name, &args,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            BodyItem::Constraint(Expr::InExpr(lhs, rhs))
                if method_dispatch_name_compat(rhs.as_ref(), items, schemas).is_some()
                && matches!(lhs.as_ref(), Expr::Tuple(_)) =>
            {
                calls::inline_tuple_in_claim(
                    lhs, rhs, items,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            // Subschema dispatch (priority): `recv.subclaim_name(args)` — same rebind logic.
            BodyItem::Constraint(Expr::Call(name, args))
                if matches!(resolve_call(name, items, schemas),
                    Some(CallDispatch::Subschema { .. })) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                    resolve_call(name, items, schemas) else { unreachable!() };
                subschema::inline_subschema_call(
                    &recv, &type_name, &claim_name, args,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            BodyItem::Constraint(Expr::Call(name, args))
                if method_dispatch_call_compat(name, items, schemas).is_some() =>
            {
                calls::inline_positional_call(
                    name, args, items,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            // Guarded claim: `cond ⇒ ClaimName` — constraints wrapped in guard; declarations unconditional.
            BodyItem::Constraint(Expr::Binary(BinOp::Implies, ant, cons))
                if matches!(cons.as_ref(),
                    Expr::Identifier(n) if schemas.contains_key(n)) =>
            {
                calls::inline_guarded_claim(
                    ant, cons,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            // `∀` over subclaim call: expand statically (translate_bool has no solver access; COUNTEREXAMPLES #26).
            BodyItem::Constraint(Expr::Forall(vars, range, body))
                if body_contains_subschema_call(body, items, schemas) =>
            {
                subschema::inline_forall_subschema(
                    vars, range, body, items,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            BodyItem::Constraint(e) => {
                // BODY_MARKERS are metadata identifiers with no Bool translation; skip silently.
                if let crate::core::ast::Expr::Identifier(s) = e {
                    if crate::core::ast::BODY_MARKERS.contains(&s.as_str()) { continue; }
                }
                if let Some(b) = translate_bool(e, ctx, env, schemas) {
                    track_assert(solver, &guarded_bool(b, guard), tracker);
                } else {
                    let lenient = crate::runtime::lenient::lenient_enabled();
                    let pretty = format!("{e:?}");
                    if lenient {
                        eprintln!("warning: dropped constraint (couldn't translate to Bool): {pretty}");
                    } else {
                        eprintln!("error: dropped constraint (couldn't translate to Bool):");
                        eprintln!("       {pretty}");
                        eprintln!();
                        eprintln!("This constraint can't be expressed as a Z3 Bool with the");
                        eprintln!("current translator — almost certainly a translator gap.");
                        eprintln!("Either rewrite the constraint to a supported shape, or");
                        eprintln!("set EVIDENT_LENIENT=1 to demote this to a warning.");
                        std::process::exit(1);
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if !guard_is_satisfiable(solver, guard) { continue; }
                if try_enter(visited, claim_name).is_none() { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                    exit_frame(visited, claim_name);
                    continue;
                };
                inline_body_items_guarded(
                    &claim.body, env, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, claim_name);
            }
            BodyItem::ClaimCall { name, mappings } => {
                calls::inline_claim_call(
                    name, mappings,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            BodyItem::HaltsWithin { fsm_name, n } => {
                // The `halts_within` surface was removed (halting is implicit in the
                // embed constraint `F(seed, fsm_state)`). The parser no longer
                // produces this variant; reaching it means a removed surface was
                // somehow reconstituted (e.g. a decoded self-hosted AST). Refuse
                // loudly to UNSAT rather than silently drop the constraint.
                if !guard_is_satisfiable(solver, guard) { continue; }
                eprintln!("[halts_within] the `halts_within({fsm_name}, {n})` surface \
                           was removed; embed `{fsm_name}(seed, fsm_state)` instead");
                let false_bool = Bool::from_bool(ctx, false);
                track_assert(solver, &guarded_bool(false_bool, guard), tracker);
            }
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}
