//! `inline_body_items` — the recursive constraint-translation walker.
//! Handles `Membership` (declare-if-new), `Constraint` (translate +
//! assert), `Passthrough` (`..ClaimName`), bare-identifier-as-passthrough
//! (`Constraint(Identifier(name))` whose name is a known claim), and
//! `ClaimCall` (with mappings + per-call fresh Z3 names for the
//! claim's unmapped internals).

use std::collections::{HashMap, HashSet};
use z3::{Context, Solver};

use crate::ast::*;
use crate::pretty;
use super::types::{DatatypeRegistry, Var};
use super::declare::{declare_var, declare_var_named, next_call_id};
use super::exprs::{resolve_mapping, translate_bool};

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
/// `visited` blocks recursion through cycles (`A` passthroughs `B`,
/// `B` passthroughs `A`). Each entry is the claim name currently being
/// inlined; we add on enter, remove on exit.
pub(super) fn inline_body_items(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    visited: &mut HashSet<String>,
) {
    for item in items {
        match item {
            BodyItem::Membership { name, type_name } => {
                // Top-level Memberships are pre-declared by pass 1, so this
                // is a no-op there. Useful when the helper recurses into a
                // passthrough's body that introduces variables not yet in
                // env (e.g. a nested claim's locals).
                if !env.contains_key(name) {
                    declare_var(ctx, solver, env, name, type_name, schemas, Some(registry));
                }
            }
            // Bare-identifier-as-passthrough: `Constraint(Identifier(name))`
            // whose name matches a known claim is treated as `..ClaimName`.
            // Falls through to the regular Constraint arm if the name
            // isn't a claim (e.g. a Bool variable named like a claim).
            BodyItem::Constraint(Expr::Identifier(name)) if schemas.contains_key(name) => {
                if visited.contains(name) { continue; }
                let Some(claim) = schemas.get(name) else { continue };
                visited.insert(name.clone());
                inline_body_items(&claim.body, env, solver, schemas, ctx, registry, visited);
                visited.remove(name);
            }
            BodyItem::Constraint(e) => {
                if let Some(b) = translate_bool(e, ctx, env) {
                    solver.assert(&b);
                } else {
                    // Hard-fail by default. A dropped constraint is silently-
                    // incorrect — the user thinks their constraint fired but
                    // the solver never saw it. Almost always a translator gap;
                    // very rarely an actual program error.
                    //
                    // Escape hatch: `EVIDENT_LENIENT=1` demotes this to a
                    // warning. Useful for incrementally-broken programs (e.g.
                    // mid-refactor) and for tests that intentionally exercise
                    // the un-translatable path.
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
                if visited.contains(claim_name) { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                    continue;
                };
                visited.insert(claim_name.clone());
                inline_body_items(&claim.body, env, solver, schemas, ctx, registry, visited);
                visited.remove(claim_name);
            }
            BodyItem::ClaimCall { name, mappings } => {
                if visited.contains(name) { continue; }
                let Some(claim) = schemas.get(name) else {
                    eprintln!("warning: ClaimCall to unknown claim {}", name);
                    continue;
                };
                let mut inner = env.clone();
                for m in mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, env);
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
                    if let BodyItem::Membership { name: vname, type_name } = sub {
                        let slot_prefix = format!("{}.", vname);
                        let already_bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&slot_prefix));
                        if !already_bound {
                            let z3_name = format!("{}__{}__call{}", name, vname, call_id);
                            declare_var_named(ctx, solver, &mut inner, vname, &z3_name,
                                              type_name, schemas, Some(registry));
                        }
                    }
                }
                visited.insert(name.clone());
                inline_body_items(&claim.body, &mut inner, solver, schemas, ctx, registry, visited);
                visited.remove(name);
            }
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}
