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

use std::collections::{HashMap, HashSet};
use z3::{Context, Solver};
use z3::ast::Bool;

use crate::ast::*;
use crate::pretty;
use super::types::{DatatypeRegistry, EnumRegistry, Var};
use super::declare::{declare_var, declare_var_named, next_call_id};
use super::exprs::{resolve_mapping, translate_bool};

/// Add `b` to the solver. With a tracker, use `assert_and_track` so
/// the constraint joins the unsat-core machinery; otherwise plain
/// `assert`. The tracker stays the same across every assertion derived
/// from one top-level body item, so the entire item shows up as one
/// entry in the core.
fn track_assert(solver: &Solver<'static>, b: &Bool<'static>, tracker: Option<&Bool<'static>>) {
    match tracker {
        Some(t) => solver.assert_and_track(b, t),
        None    => solver.assert(b),
    }
}

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
///
/// `visited` is a per-claim depth counter that bounds inlining
/// recursion. Each entry maps a claim name to how many frames of
/// it are currently on the inlining stack. A frame can re-enter the
/// same claim up to `MAX_INLINE_DEPTH` times — enough to walk a
/// recursive AST (transpilers, list emitters, etc.) but bounded so
/// pathological self-passthrough cycles don't OOM. Without unrolling
/// at all, the transpiler-as-recursive-claims pattern doesn't work
/// (Z3 invents arbitrary string values for un-asserted `tail_out`
/// bindings). The depth bound is overridable via
/// `EVIDENT_MAX_INLINE_DEPTH` for ASTs deeper than the default.

/// Default cap — large enough for any realistic shader/transpiler AST,
/// small enough that a self-passthrough loop trips it before the
/// translation context blows out.
const DEFAULT_MAX_INLINE_DEPTH: usize = 64;

fn max_inline_depth() -> usize {
    std::env::var("EVIDENT_MAX_INLINE_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_INLINE_DEPTH)
}

/// Try to enter a frame of `name` on the inlining stack. Returns
/// `Some(depth)` (the post-increment count) on success, `None` if
/// we'd exceed the depth cap. `depth > 1` ⇒ this is a recursive
/// frame; callers use that to force fresh per-call declarations
/// for body-internal Memberships, otherwise the env-clone would
/// shadow them with outer-scope vars and recursive claims would
/// self-reference (e.g. `out = "x " ++ tail_out` where `tail_out`
/// is the SAME Z3 const as the outer call's `tail_out`).
fn try_enter(visited: &mut HashMap<String, usize>, name: &str) -> Option<usize> {
    let max = max_inline_depth();
    let cnt = visited.entry(name.to_string()).or_insert(0);
    if *cnt >= max {
        None
    } else {
        *cnt += 1;
        Some(*cnt)
    }
}

/// Counterpart to `try_enter` — call after the inlined body has been
/// translated. Removes the entry entirely when its count hits zero
/// so subsequent same-name lookups don't see stale state.
fn exit_frame(visited: &mut HashMap<String, usize>, name: &str) {
    if let Some(cnt) = visited.get_mut(name) {
        *cnt -= 1;
        if *cnt == 0 { visited.remove(name); }
    }
}
/// Wrap a constraint in an `antecedent ⇒ constraint` implication when
/// a guard is active (i.e. we're inlining a claim's body under
/// `state.step = 0 ⇒ ClaimName`). Returns the constraint unchanged
/// when no guard is in effect.
fn wrap_with_guard(c: Expr, guard: &Option<Expr>) -> Expr {
    match guard {
        None => c,
        Some(g) => Expr::Binary(
            crate::ast::BinOp::Implies,
            Box::new(g.clone()),
            Box::new(c),
        ),
    }
}

/// Compose two guards: `outer ∧ inner`. Used when entering a nested
/// guarded-claim invocation so the inner claim's constraints fire only
/// when both antecedents hold.
fn compose_guards(outer: &Option<Expr>, inner: Expr) -> Option<Expr> {
    match outer {
        None => Some(inner),
        Some(o) => Some(Expr::Binary(
            crate::ast::BinOp::And,
            Box::new(o.clone()),
            Box::new(inner),
        )),
    }
}

pub(super) fn inline_body_items(
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
pub(super) fn inline_body_items_tracked(
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

fn inline_body_items_guarded(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Expr>,
    tracker: Option<&Bool<'static>>,
) {
    for item in items {
        match item {
            BodyItem::Membership { name, type_name, pins } => {
                // Top-level Memberships are pre-declared by pass 1, so the
                // declare_var call is a no-op there. Useful when the helper
                // recurses into a passthrough's body that introduces
                // variables not yet in env (e.g. a nested claim's locals).
                if !env.contains_key(name) {
                    declare_var(ctx, solver, env, name, type_name, schemas, Some(registry), enums);
                }
                // Resolve `pins` to a list of (field-name, value-expr)
                // pairs. Named is direct; Positional looks up the type's
                // body Membership order to map positions to field names.
                let resolved_pins: Vec<(String, Expr)> = match pins {
                    crate::ast::Pins::None => Vec::new(),
                    crate::ast::Pins::Named(maps) => maps.iter()
                        .map(|m| (m.slot.clone(), m.value.clone())).collect(),
                    crate::ast::Pins::Positional(args) => {
                        // Look up the type's field order from its
                        // SchemaDecl. Strict count match required.
                        let Some(schema) = schemas.get(type_name) else {
                            eprintln!(
                                "error: positional pin on unknown type `{}`",
                                type_name
                            );
                            std::process::exit(1);
                        };
                        let field_order: Vec<String> = schema.body.iter()
                            .filter_map(|item| match item {
                                BodyItem::Membership { name, .. } => Some(name.clone()),
                                _ => None,
                            })
                            .collect();
                        // Partial allowed: too few args pin the leading
                        // fields and leave the rest free. Too many is
                        // a real error — the user is asking to pin
                        // fields that don't exist.
                        if args.len() > field_order.len() {
                            eprintln!(
                                "error: too many positional pins on `{}`: \
                                 type declares {} fields, got {} args",
                                type_name, field_order.len(), args.len()
                            );
                            std::process::exit(1);
                        }
                        field_order.into_iter()
                            .zip(args.iter().cloned())
                            .collect()
                    }
                };
                // Fire each pin as `name.field = value`. Same machinery
                // and same dropped-constraint policy as a regular
                // Constraint — a pin to a non-existent field is the
                // same kind of silent error as a generic dropped
                // translation, so it shares the hard-fail behavior.
                for (slot, value) in resolved_pins {
                    let lhs = Expr::Identifier(format!("{}.{}", name, slot));
                    let eq = Expr::Binary(
                        crate::ast::BinOp::Eq,
                        Box::new(lhs),
                        Box::new(value.clone()),
                    );
                    let guarded_eq = wrap_with_guard(eq.clone(), guard);
                    if let Some(b) = translate_bool(&guarded_eq, ctx, env, schemas) {
                        track_assert(solver, &b, tracker);
                    } else {
                        let lenient = std::env::var("EVIDENT_LENIENT")
                            .map(|v| !v.is_empty() && v != "0")
                            .unwrap_or(false);
                        let pretty = pretty::expr(&eq);
                        if lenient {
                            eprintln!(
                                "warning: type-use pin didn't translate: {}",
                                pretty
                            );
                        } else {
                            eprintln!(
                                "error: type-use pin didn't translate: {}",
                                pretty
                            );
                            eprintln!();
                            eprintln!(
                                "The field `{}` probably doesn't exist on type `{}`,",
                                slot, type_name
                            );
                            eprintln!(
                                "or its type doesn't accept the pinned value's shape."
                            );
                            eprintln!(
                                "Set EVIDENT_LENIENT=1 to demote this to a warning."
                            );
                            std::process::exit(1);
                        }
                    }
                }
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
            BodyItem::Constraint(Expr::Call(name, args)) if schemas.contains_key(name) => {
                let Some(depth) = try_enter(visited, name) else { continue };
                let Some(claim) = schemas.get(name) else { exit_frame(visited, name); continue };

                // Pair positional args with the claim's first N Membership
                // body items (which include first-line params, since
                // those desugar to Memberships at the head of the body).
                let slot_names: Vec<String> = claim.body.iter()
                    .filter_map(|i| if let BodyItem::Membership { name, .. } = i {
                        Some(name.clone())
                    } else { None })
                    .take(args.len())
                    .collect();
                if slot_names.len() != args.len() {
                    eprintln!(
                        "warning: positional ClaimCall to `{}` got {} args but \
                         the claim has only {} param Memberships",
                        name, args.len(), slot_names.len()
                    );
                    exit_frame(visited, name);
                    continue;
                }
                let mappings: Vec<crate::ast::Mapping> = slot_names.into_iter()
                    .zip(args.iter())
                    .map(|(slot, value)| crate::ast::Mapping {
                        slot, value: value.clone(),
                    })
                    .collect();

                // Same binding logic as the named-mapsto ClaimCall arm
                // below — bind args, declare per-call Z3 names for any
                // claim-internal vars, recurse with fresh inner env.
                let mut inner = env.clone();
                let slot_set: std::collections::HashSet<String> =
                    mappings.iter().map(|m| m.slot.clone()).collect();
                for m in &mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
                    if bound.is_empty() {
                        eprintln!("warning: positional arg didn't resolve: {:?}", m.value);
                    }
                    for (k, v) in bound {
                        inner.insert(k, v);
                    }
                }
                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        let slot_prefix = format!("{}.", vname);
                        let already_bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&slot_prefix));
                        // Recursive frames force-shadow body-internal
                        // (non-slot) Memberships so each invocation gets
                        // a fresh Z3 const. Without this the inner call
                        // inherits the outer's Z3 const via env clone
                        // and `out = ... ++ tail_out` becomes a
                        // self-equation `T = ... ++ T` (UNSAT).
                        let force_fresh = depth > 1 && !slot_set.contains(vname);
                        if force_fresh {
                            // declare_var_named's idempotence guard
                            // would skip the re-declaration; pop the
                            // inherited entry first so the fresh decl
                            // sticks.
                            inner.remove(vname);
                            let dotted: Vec<String> = inner.keys()
                                .filter(|k| k.starts_with(&slot_prefix))
                                .cloned().collect();
                            for k in dotted { inner.remove(&k); }
                        }
                        if !already_bound || force_fresh {
                            let z3_name = format!("{}__{}__call{}", name, vname, call_id);
                            declare_var_named(ctx, solver, &mut inner, vname, &z3_name,
                                              type_name, schemas, Some(registry), enums);
                        }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, name);
            }
            // Guarded claim invocation: `cond ⇒ ClaimName` inlines the
            // claim's body but wraps each constraint in `cond ⇒ …`.
            // Declarations from the claim fire unconditionally; the
            // guard only narrows what the constraints assert. Composes
            // with an outer guard if we're already inside one.
            BodyItem::Constraint(Expr::Binary(crate::ast::BinOp::Implies, ant, cons))
                if matches!(cons.as_ref(),
                    Expr::Identifier(n) if schemas.contains_key(n)) =>
            {
                let claim_name = match cons.as_ref() {
                    Expr::Identifier(n) => n,
                    _ => unreachable!(),
                };
                if try_enter(visited, claim_name).is_none() { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    exit_frame(visited, claim_name); continue
                };
                let new_guard = compose_guards(guard, (**ant).clone());
                inline_body_items_guarded(
                    &claim.body, env, solver, schemas, ctx, registry, enums, visited, &new_guard, tracker
                );
                exit_frame(visited, claim_name);
            }
            BodyItem::Constraint(e) => {
                let guarded = wrap_with_guard(e.clone(), guard);
                if let Some(b) = translate_bool(&guarded, ctx, env, schemas) {
                    track_assert(solver, &b, tracker);
                } else {
                    let lenient = std::env::var("EVIDENT_LENIENT")
                        .map(|v| !v.is_empty() && v != "0")
                        .unwrap_or(false);
                    let pretty = pretty::expr(&guarded);
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
                let Some(depth) = try_enter(visited, name) else { continue };
                let Some(claim) = schemas.get(name) else {
                    eprintln!("warning: ClaimCall to unknown claim {}", name);
                    exit_frame(visited, name);
                    continue;
                };
                let mut inner = env.clone();
                let slot_set: std::collections::HashSet<String> =
                    mappings.iter().map(|m| m.slot.clone()).collect();
                for m in mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
                    if bound.is_empty() {
                        eprintln!("warning: mapping value didn't resolve: {:?}", m.value);
                    }
                    for (k, v) in bound {
                        inner.insert(k, v);
                    }
                }
                // Declare any of the claim's own variables that weren't
                // bound by a mapping (the claim's "internal" parameters,
                // like AxisPhysics's `intended` / `target`). Each
                // invocation gets a per-call suffix on the Z3 name so
                // two invocations of the same claim get distinct Z3
                // constants — without this, both AxisPhysics calls in
                // PlayerPhysics share one `intended` Z3 var and the
                // x-axis vs. y-axis branches contradict → UNSAT.
                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        let slot_prefix = format!("{}.", vname);
                        let already_bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&slot_prefix));
                        // See positional-call arm: recursive frames
                        // shadow body-internal Memberships so each
                        // invocation gets a fresh Z3 const.
                        let force_fresh = depth > 1 && !slot_set.contains(vname);
                        if force_fresh {
                            // declare_var_named's idempotence guard
                            // would skip the re-declaration; pop the
                            // inherited entry first so the fresh decl
                            // sticks.
                            inner.remove(vname);
                            let dotted: Vec<String> = inner.keys()
                                .filter(|k| k.starts_with(&slot_prefix))
                                .cloned().collect();
                            for k in dotted { inner.remove(&k); }
                        }
                        if !already_bound || force_fresh {
                            let z3_name = format!("{}__{}__call{}", name, vname, call_id);
                            declare_var_named(ctx, solver, &mut inner, vname, &z3_name,
                                              type_name, schemas, Some(registry), enums);
                        }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, name);
            }
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}
