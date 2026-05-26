//! UNSAT-core variant of `evaluate`: tags body-item assertions with tracker bools so an
//! UNSAT result maps back to source body-item indices via `get_unsat_core`.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, SatResult};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
use super::super::declare::apply_seq_lengths;
use super::super::extract::assert_seq_given;
use super::super::preprocess::{apply_pinned_ints, collect_pinned_ints};
use super::solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64};
use super::decode::extract_binding;

/// Like `evaluate`, but tags body-item assertions with trackers; UNSAT maps core back to
/// body-item indices. Givens are not tracked — core reflects schema-own conflicts only.
pub fn evaluate_with_core(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> EvalResult {
    let _enum_guard = super::super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name, .. } => {
                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name, .. } = sub {
                            if !env.contains_key(name) {
                                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), enums);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let seq_lens = super::super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    let trackers: Vec<Bool<'static>> = (0..schema.body.len())
        .map(|i| Bool::new_const(ctx, format!("__core_{i}__")))
        .collect();

    let mut visited: HashMap<String, usize> = HashMap::new();
    super::super::inline::inline_body_items_tracked(
        &schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited, &trackers,
    );

    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    solver.assert(&b);
                }
            }
        }
    }

    let result = solver.check_assumptions(&trackers);
    let satisfied = matches!(result, SatResult::Sat);
    let mut bindings = HashMap::new();
    let mut core_items: Option<Vec<usize>> = None;

    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, enums);
            }
        }
    } else {
        let core = solver.get_unsat_core();
        let mut indices: Vec<usize> = core.iter()
            .filter_map(|b| {
                let s = format!("{b}");
                s.strip_prefix("__core_")
                    .and_then(|rest| rest.strip_suffix("__"))
                    .and_then(|n| n.parse().ok())
            })
            .collect();
        indices.sort();
        indices.dedup();
        core_items = Some(indices);
    }
    EvalResult { satisfied, bindings, unsat_core_items: core_items }
}
