//! The `Membership` body-item arm: declare the variable (if pass 1
//! didn't already), fire any type-use pins, and inherit the type's
//! body Constraints onto the instance — including element-level
//! invariants for `Seq(SomeType)` declarations.

use std::collections::HashMap;

use z3::{Context, Solver};
use z3::ast::{Ast, Bool};

use crate::core::ast::*;
use crate::pretty;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use crate::translate::declare::declare_var;
use crate::translate::exprs::translate_bool;
use super::guards::{guarded_bool, track_assert};
use super::rewrite::{rewrite_idents_with_prefix, substitute_bound_var};

#[allow(clippy::too_many_arguments)]
pub(super) fn inline_membership(
    name: &str,
    type_name: &str,
    pins: &Pins,
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    // Top-level Memberships are pre-declared by pass 1, so the
    // declare_var call is a no-op there. Useful when the helper
    // recurses into a passthrough's body that introduces
    // variables not yet in env (e.g. a nested claim's locals).
    if !env.contains_key(name) {
        let post = declare_var(ctx, env, name, type_name, schemas, Some(registry), enums);
        for c in &post { track_assert(solver, c, tracker); }
    }
    // Resolve `pins` to a list of (field-name, value-expr)
    // pairs. Named is direct; Positional looks up the type's
    // body Membership order to map positions to field names.
    let resolved_pins: Vec<(String, Expr)> = match pins {
        crate::core::ast::Pins::None => Vec::new(),
        crate::core::ast::Pins::Named(maps) => maps.iter()
            .map(|m| (m.slot.clone(), m.value.clone())).collect(),
        crate::core::ast::Pins::Positional(args) => {
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
            crate::core::ast::BinOp::Eq,
            Box::new(lhs),
            Box::new(value.clone()),
        );
        if let Some(b) = translate_bool(&eq, ctx, env, schemas) {
            track_assert(solver, &guarded_bool(b, guard), tracker);
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

    // Inherit the type's body Constraints onto this instance.
    // For each `Constraint(e)` in the type's body, rewrite any
    // identifier whose leading dotted segment names one of the
    // type's own fields by prefixing `name.`. Skip if the type
    // is not a user-defined schema (built-ins like Int / Nat /
    // Seq(...) etc. — they have no body to inherit).
    //
    // This is what makes `mario ∈ MarioSprite (pos ↦ p)` mean
    // "mario satisfies MarioSprite's invariants" rather than
    // "mario has MarioSprite's leaf fields but no constraints
    // between them." Without it, body equalities like
    // `hat = Rect(Color(220, …), pos, …)` in the type body
    // produce no constraint on `mario.hat`, and the instance
    // ends up free.
    if let Some(type_schema) = schemas.get(type_name) {
        let field_set: std::collections::HashSet<String> = type_schema
            .body
            .iter()
            .filter_map(|item| match item {
                BodyItem::Membership { name: n, .. } => Some(n.clone()),
                _ => None,
            })
            .collect();
        for item in &type_schema.body {
            if let BodyItem::Constraint(e) = item {
                let rewritten = rewrite_idents_with_prefix(e, name, &field_set);
                if let Some(b) = translate_bool(&rewritten, ctx, env, schemas) {
                    track_assert(solver, &guarded_bool(b, guard), tracker);
                }
                // Silently skip on translation failure — the
                // type body might contain shapes that only
                // apply when used with a passthrough (e.g.,
                // bare claim names that match-by-name). The
                // hard-fail policy stays on direct body items
                // of the calling schema.
            }
        }
    }

    // Element-level invariant inheritance for `Seq(SomeType)`:
    // when SomeType has body Constraints (e.g. `#effs = 2`),
    // emit per-element substituted versions over the Seq's
    // pinned indices. Without this, a user `plat_effs ∈
    // Seq(EffectPair)` declaration wouldn't auto-pin each
    // bundle's inner length — the user would have to write
    // `∀ i ∈ {0..3} : #plat_effs[i].effs = 2` by hand.
    //
    // The substitution treats each Seq element as a record
    // value reached by `Index(Identifier(name), Int(i))`,
    // and the type's bare field references become
    // `Field(Index(name, i), field_name)` per iteration.
    if let Some(inner) = type_name.strip_prefix("Seq(")
        .and_then(|s| s.strip_suffix(')'))
    {
        if let Some(inner_schema) = schemas.get(inner) {
            let len_opt = env.get(name).and_then(|v| {
                if let Some((_, len, _, _, _)) = v.as_datatype_seq() {
                    len.simplify().as_i64()
                } else if let Some((_, len, _)) = v.as_seq() {
                    len.simplify().as_i64()
                } else { None }
            });
            if let Some(n) = len_opt {
                let field_set: std::collections::HashSet<String> =
                    inner_schema.body.iter()
                        .filter_map(|item| match item {
                            BodyItem::Membership { name: n, .. } => Some(n.clone()),
                            _ => None,
                        })
                        .collect();
                for i in 0..n {
                    for item in &inner_schema.body {
                        if let BodyItem::Constraint(e) = item {
                            // Build elem_expr = Index(Identifier(name), Int(i)).
                            // For each of the inner type's field
                            // names, substitute bare refs to
                            // `Field(elem_expr, field_name)`.
                            let mut substituted = e.clone();
                            for fname in &field_set {
                                let elem = Expr::Field(
                                    Box::new(Expr::Index(
                                        Box::new(Expr::Identifier(name.to_string())),
                                        Box::new(Expr::Int(i)),
                                    )),
                                    fname.clone(),
                                );
                                substituted = substitute_bound_var(
                                    &substituted, fname, &elem);
                            }
                            if let Some(b) = translate_bool(
                                &substituted, ctx, env, schemas)
                            {
                                track_assert(solver, &guarded_bool(b, guard), tracker);
                            }
                        }
                    }
                }
            }
        }
    }
}
