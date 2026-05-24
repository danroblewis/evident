//! `inline_body_items` — the recursive constraint-translation walker.
//! Handles `Membership` (declare-if-new), `Constraint` (translate +
//! assert), `Passthrough` (`..ClaimName`), and `ClaimCall` (with
//! mappings + per-call fresh Z3 names for the claim's unmapped
//! internals).
//!
//! Bare-identifier-as-passthrough (`Constraint(Identifier(name))`
//! where `name` is a known claim) is handled BEFORE this walker
//! runs by a self-hosted desugar pass —
//! `stdlib/passes/desugar_passthrough.ev` paired with
//! `commands/desugar.rs::auto_apply_desugar`, wired into every CLI
//! subcommand. By the time the AST arrives here, the rewrite has
//! already turned the bare form into an explicit `Passthrough` node.
//!
//! The dispatch loop below classifies each body item and delegates
//! the heavy arms to sibling modules: `membership::inline_membership`
//! for `Membership`, and the `calls::*` functions for the various
//! claim/subclaim invocation shapes.

use std::collections::HashMap;

use z3::{Context, Solver};
use z3::ast::Bool;

use crate::core::ast::*;
use crate::pretty;
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

/// Recursively translate a list of body items into the solver. Used by
/// the constraint-translation pass of both `evaluate` and `build_cache`,
/// and also called recursively when a `Passthrough`, bare-identifier
/// passthrough, or `ClaimCall` references another claim's body.
///
/// Without this, passthroughs only inlined `Constraint` items — any
/// `ClaimCall` (e.g. `PlayerPhysics(state mapsto state.player, …)`)
/// inside a passthrough was silently dropped. Same problem inside a
/// `ClaimCall`: nested claim calls in the called claim's body were
/// dropped. That broke `..DotCollectGameEngine` (no player, no physics,
/// no background — black screen).
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

/// Translate `items` and assert each derived constraint into the
/// solver, additionally tagging every assertion with one of `trackers`
/// so a later `solver.get_unsat_core()` can name the offending
/// top-level body item. `trackers[i]` corresponds to `items[i]` —
/// passing fewer trackers than items means tail items go untracked.
/// Used by `evaluate_with_core` to surface unsat-cores back to the
/// test runner; the regular `evaluate` path passes `None` for
/// zero overhead.
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
            // Bare-identifier-as-passthrough handling moved to the
            // self-hosted desugar pass (`stdlib/passes/desugar_passthrough.ev`
            // + `commands/desugar.rs::auto_apply_desugar`). By the time
            // any constraint arrives here, the rewrite has already turned
            // `Constraint(Identifier(name))` (when `name` is a known
            // claim) into `Passthrough(name)`. Bare-identifier
            // constraints whose name does NOT match a claim fall through
            // to the regular Constraint arm below, same as before.

            // Positional claim invocation: `Constraint(Call(name, args))`
            // whose `name` matches a known claim is treated as a
            // ClaimCall with positional args bound by index to the
            // claim's first-line params (or first N Memberships in
            // declaration order). Encourages the
            //   claim Foo(items ∈ Seq, keys ∈ Seq, asc ∈ Bool)
            //       …body using items/keys/asc…
            //   ⋮
            //   Foo(my_items, my_keys, true)
            // pattern over the longer mapsto form.
            // Tuple-in-claim invocation: `(a, b, c) ∈ claim_name` is
            // the relational reading of a positional claim call —
            // "this tuple is in the set of satisfying assignments."
            // Desugars to the same positional ClaimCall path as
            // `claim_name(a, b, c)`. The dispatch is here (not in
            // exprs.rs's InExpr handler) because it has to fire at
            // BodyItem position, not as a value-producing
            // expression — a claim invocation contributes constraints,
            // not a Bool.
            // Subschema dispatch (priority over receiver-prefix):
            // `(args) ∈ recv.subclaim_name` where `recv` is a body
            // Membership of a record type T AND `subclaim_name` is
            // declared as `subclaim …` inside T. The receiver's
            // fields get rebound onto T's bare-name fields so the
            // subclaim body's references (`renderer`, etc.) resolve
            // to the leaves of `recv`.
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
            // Subschema dispatch (priority): `recv.subclaim_name(args)`
            // where `recv` is a body Membership of a record type T
            // AND `subclaim_name` is a SubclaimDecl inside T's body.
            // Field-rebinds T's leaves onto bare names so the
            // subclaim body's references (`renderer`, …) resolve
            // to the receiver's leaves.
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
            // Guarded claim invocation: `cond ⇒ ClaimName` inlines the
            // claim's body but wraps each constraint in `cond ⇒ …`.
            // Declarations from the claim fire unconditionally; the
            // guard only narrows what the constraints assert. Composes
            // with an outer guard if we're already inside one.
            BodyItem::Constraint(Expr::Binary(BinOp::Implies, ant, cons))
                if matches!(cons.as_ref(),
                    Expr::Identifier(n) if schemas.contains_key(n)) =>
            {
                calls::inline_guarded_claim(
                    ant, cons,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            // `∀ vars ∈ range : body` where `body` contains a method-
            // style subclaim invocation (`recv.subclaim(args)`).
            //
            // translate_bool's ∀ translator runs the body through
            // translate_bool, which has no solver access and can't
            // fire the subclaim's per-iteration assertions (the
            // `out = ⟨…⟩` pin inside the subclaim body never lands,
            // leaving outputs free; see COUNTEREXAMPLES #26).
            //
            // Fix: expand the ∀ at AST level for known-length ranges
            // (coindexed of pinned-length Seqs, or a bare pinned Seq).
            // Each iteration becomes a regular BodyItem the inline
            // pass can dispatch — subclaim calls get full solver
            // access via inline_subschema_call as usual.
            BodyItem::Constraint(Expr::Forall(vars, range, body))
                if body_contains_subschema_call(body, items, schemas) =>
            {
                subschema::inline_forall_subschema(
                    vars, range, body, items,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            BodyItem::Constraint(e) => {
                // Recognized runtime markers (declared in
                // `crate::core::ast::BODY_MARKERS`) are bare identifiers
                // that carry metadata for some other runtime layer
                // — they have no Bool translation. Skip silently
                // so they don't trip the dropped-constraint diagnostic.
                if let crate::core::ast::Expr::Identifier(s) = e {
                    if crate::core::ast::BODY_MARKERS.contains(&s.as_str()) { continue; }
                }
                if let Some(b) = translate_bool(e, ctx, env, schemas) {
                    track_assert(solver, &guarded_bool(b, guard), tracker);
                } else {
                    let lenient = std::env::var("EVIDENT_LENIENT")
                        .map(|v| !v.is_empty() && v != "0")
                        .unwrap_or(false);
                    let pretty = pretty::expr(e);
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
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}
