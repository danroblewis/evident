//! UNSAT-core variant of `evaluate`. Tracks per-body-item trackers
//! so that on UNSAT we can map Z3's `get_unsat_core` back to source
//! body-item indices for diagnostics.

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

/// Same as `evaluate`, but tags every Z3 assertion derived from a
/// top-level body item with a unique tracker bool so an UNSAT result
/// produces a usable `unsat_core_items` (indices into `schema.body`).
///
/// Givens are NOT tracked — the user's `given` values are external
/// inputs, not constraints we can ask the user to fix. The core
/// reflects the conflict among the schema's own body items only.
///
/// On SAT, behaves exactly like `evaluate` (the trackers are still
/// asserted but Z3 ignores them); the cost is one extra implication
/// per assertion. On UNSAT, the core is mapped from tracker name
/// (`__core_<i>__`) back to body item index.
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

    // Pass 1: declarations (same as evaluate).
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
            // (Bare-identifier-as-passthrough desugared upstream — see
            // build_cache notes.)
            _ => {}
        }
    }

    let seq_lens = super::super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    // Allocate one tracker bool per top-level body item, with a name
    // that encodes the index so we can map the core back to source.
    let trackers: Vec<Bool<'static>> = (0..schema.body.len())
        .map(|i| Bool::new_const(ctx, format!("__core_{i}__")))
        .collect();

    let mut visited: HashMap<String, usize> = HashMap::new();
    super::super::inline::inline_body_items_tracked(
        &schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited, &trackers,
    );

    // Givens — same as evaluate, NOT tracked.
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

    // check_assumptions activates the trackers and asks Z3 to find a
    // satisfying assignment under them. On UNSAT, get_unsat_core
    // returns the subset that's actually needed for the conflict.
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
