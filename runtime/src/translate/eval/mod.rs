//! Public evaluate entry points. One-shot: `evaluate` (canonical), `_with_extra_assertion(s)`,
//! `_with_program_and_body`, `_with_core`. Cached: `build_cache`/`run_cached`/`sample_cached_inner`.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int};
use z3::{Context, SatResult};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
use super::declare::{apply_seq_lengths, apply_set_candidates};
use super::extract::{assert_seq_given, assert_set_given, extract_seq, extract_seq_composite, extract_set, unescape_z3_string};
use super::inline::inline_body_items;
use super::preprocess::{apply_pinned_ints, collect_pinned_ints};

mod solver;
mod decode;
mod cached;
mod extra;
mod core;
mod decompose;

use solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64, real_value_to_f64};
use decode::extract_enum_value;

pub use cached::{build_cache, run_cached, sample_cached_inner};
pub use extra::{evaluate_with_extra_assertion, evaluate_with_extra_assertions, evaluate_with_program_and_body};
pub use self::core::evaluate_with_core;
pub use decompose::{analyze_decomposition, classify_components, ClassifiedComponent};
pub(crate) use decode::extract_binding;

// Preserve pub(super) visibility for translate::extract's composite-Seq path.
pub(super) use decode::extract_seq_enum;

/// Evaluate a schema: declare leaf Z3 vars (dotted prefix per field), pin given
/// values, assert body constraints, return SAT/bindings. THE canonical entry point.
pub fn evaluate(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> EvalResult {
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Pass 1: declare vars; passthroughs import leaves; collisions reuse (names-match).
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
                } else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                }
            }
            BodyItem::ClaimCall { .. } => {} // declarations added in pass 2
            BodyItem::SubclaimDecl(_) => {} // registered at load; no parent constraints
            BodyItem::Constraint(_) => {}
            BodyItem::HaltsWithin { .. } => {} // lowered in pass 2 via inline walker
        }
    }

    // Pass 1.5: pin ints + propagate seq lengths so quantifier ranges unroll.
    let seq_lens = super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    apply_set_candidates(&env, given);

    // Pass 2: translate and assert body constraints.
    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    // Pass 3: assert given ground facts; undeclared names ignored.
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&crate::translate::z3_string(ctx, s).expect("nul in str"))),
            // PinnedInt already folded in; if values disagree, force UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            (Var::EnumVar { ast, .. }, val @ Value::Enum { .. }) => {
                if let Some(reg) = enums {
                    if let Some(dt) = super::encode_ast::value_enum_to_datatype(val, ctx, reg) {
                        solver.assert(&ast._eq(&dt));
                    }
                }
            }
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }

    let check_t0 = std::time::Instant::now();
    let check_result = solver.check();
    crate::z3_profile::record_check_stats(&solver, Some(&schema.name), check_t0.elapsed());
    let satisfied = matches!(check_result, SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                match var {
                    Var::IntVar(i) => {
                        if let Some(val) = model.eval(i, true) {
                            if let Some(n) = val.as_i64() {
                                bindings.insert(name.clone(), Value::Int(n));
                            }
                        }
                    }
                    Var::BoolVar(b) => {
                        if let Some(val) = model.eval(b, true) {
                            if let Some(bv) = val.as_bool() {
                                bindings.insert(name.clone(), Value::Bool(bv));
                            }
                        }
                    }
                    Var::RealVar(r) => {
                        if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                            bindings.insert(name.clone(), Value::Real(real_value_to_f64(num, den)));
                        }
                    }
                    Var::StrVar(s) => {
                        if let Some(val) = model.eval(s, true) {
                            if let Some(sv) = val.as_string() {
                                bindings.insert(name.clone(), Value::Str(unescape_z3_string(&sv)));
                            }
                        }
                    }
                    Var::SeqVar { arr, len, elem } => {
                        if let Some(v) = extract_seq(arr, len, *elem, &model, ctx) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::PinnedInt(v) => {
                        bindings.insert(name.clone(), Value::Int(*v));
                    }
                    Var::SetVar { set, elem, candidates } => {
                        if let Some(v) = extract_set(set, *elem, candidates, &model, ctx) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::DatatypeSetVar { .. } => { /* unsupported in v1 */ }
                    Var::DatatypeSeqVar { arr, len, dt, fields, type_name } => {
                        let extracted = if fields.is_empty() {
                            extract_seq_enum(arr, len, type_name, *dt, &model, ctx, enums)
                        } else {
                            extract_seq_composite(arr, len, fields.as_slice(), *dt, &model, ctx, enums)
                        };
                        if let Some(v) = extracted {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::EnumVar { ast, enum_name, dt } => {
                        if let Some(v) = extract_enum_value(ast, enum_name, dt, &model, ctx, enums) {
                            bindings.insert(name.clone(), v);
                        }
                    }
                    Var::EnumValue { .. } => { /* literal */ }
                    Var::EnumCtor { .. }  => { /* constructor */ }
                }
            }
        }
    }
    EvalResult { satisfied, bindings, unsat_core_items: None }
}
