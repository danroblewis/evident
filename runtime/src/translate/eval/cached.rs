//! Per-step cached query path: translate the schema body once (`build_cache`), then reuse it
//! per tick via push/assert-givens/check/pop (`run_cached`) or for n-models sampling.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int};
use z3::{Context, SatResult};

use crate::core::ast::*;
use crate::core::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
use super::super::declare::{apply_seq_lengths, apply_set_candidates};
use super::super::extract::{assert_seq_given, assert_set_given, extract_seq, extract_seq_composite, extract_set, unescape_z3_string};
use super::super::inline::inline_body_items;
use super::super::preprocess::{apply_pinned_ints, collect_pinned_ints};
use super::solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64, real_value_to_f64};
use super::decode::{extract_enum_value, extract_seq_enum};

/// Translate the schema body once into a fresh solver; returns a `CachedSchema` for reuse.
/// Pass only the structural-subset of `given` (quantifier-bound values) to bake correct unrolling.
pub fn build_cache(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    given: &HashMap<String, Value>,
    arith_solver: u32,
) -> CachedSchema<'static> {
    // Install the thread-local EnumRegistry so enum constructors in body items resolve.
    // Without this those constraints silently drop and outputs end up undefined.
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
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    CachedSchema { env, solver, arith_solver }
}

/// Sample up to `n` distinct models via blocking clauses (one outer push + per-iteration
/// ¬(scalar assignment)). Seq/Set/composite vars excluded from blocking (v1 limitation).
pub fn sample_cached_inner<'ctx>(
    cached: &CachedSchema<'ctx>,
    given: &HashMap<String, Value>,
    n: usize,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Vec<HashMap<String, Value>> {
    cached.solver.push();
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => cached.solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&crate::translate::z3_string(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    cached.solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    cached.solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }

    let mut out: Vec<HashMap<String, Value>> = Vec::new();
    for _ in 0..n {
        if !matches!(cached.solver.check(), SatResult::Sat) {
            break;
        }
        let Some(model) = cached.solver.get_model() else { break };

        let mut bindings: HashMap<String, Value> = HashMap::new();
        let mut block_terms: Vec<Bool<'ctx>> = Vec::new();

        for (name, var) in cached.env.iter() {
            match var {
                Var::IntVar(i) => {
                    if let Some(v) = model.eval(i, true).and_then(|x| x.as_i64()) {
                        bindings.insert(name.clone(), Value::Int(v));
                        block_terms.push(i._eq(&Int::from_i64(ctx, v)));
                    }
                }
                Var::BoolVar(b) => {
                    if let Some(v) = model.eval(b, true).and_then(|x| x.as_bool()) {
                        bindings.insert(name.clone(), Value::Bool(v));
                        block_terms.push(b._eq(&Bool::from_bool(ctx, v)));
                    }
                }
                Var::RealVar(r) => {
                    if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                        let f = real_value_to_f64(num, den);
                        bindings.insert(name.clone(), Value::Real(f));
                        block_terms.push(r._eq(&real_from_f64(ctx, f)));
                    }
                }
                Var::StrVar(s) => {
                    if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                        let v = unescape_z3_string(&v);
                        bindings.insert(name.clone(), Value::Str(v.clone()));
                        if let Ok(lit) = crate::translate::z3_string(ctx, &v) {
                            block_terms.push(s._eq(&lit));
                        }
                    }
                }
                Var::SeqVar { arr, len, elem } => {
                    if let Some(v) = extract_seq(arr, len, *elem, &model, ctx) {
                        bindings.insert(name.clone(), v);
                    }
                    // Seq blocking non-trivial; skipped for v1 (documented limitation).
                }
                Var::PinnedInt(v) => {
                    bindings.insert(name.clone(), Value::Int(*v));
                    // PinnedInts are constants — no blocking term.
                }
                Var::SetVar { set, elem, candidates } => {
                    if let Some(v) = extract_set(set, *elem, candidates, &model, ctx) {
                        bindings.insert(name.clone(), v);
                    }
                    // Set blocking non-trivial; skipped for v1.
                }
                Var::DatatypeSetVar { .. } => {
                    // Composite Set extraction unsupported in v1; constraint side works but
                    // check/all_solutions omits the binding.
                }
                Var::DatatypeSeqVar { arr, len, dt, fields, type_name } => {
                    let extracted = if fields.is_empty() {
                        extract_seq_enum(arr, len, type_name, *dt, &model, ctx, enums)
                    } else {
                        extract_seq_composite(arr, len, fields.as_slice(), *dt, &model, ctx, enums)
                    };
                    if let Some(v) = extracted {
                        bindings.insert(name.clone(), v);
                    }
                    // Blocking on composite/enum seq elements skipped for v1.
                }
                Var::EnumVar { ast, enum_name, dt } => {
                    if let Some(v) = extract_enum_value(ast, enum_name, dt, &model, ctx, enums) {
                        bindings.insert(name.clone(), v.clone());
                        // Use model-evaluated value so payload-variant constructors block
                        // correctly (they take args; nullary ctor.apply(&[]) won't work).
                        if let Some(chosen) = model.eval(ast, true) {
                            block_terms.push(ast._eq(&chosen));
                        }
                    }
                }
                Var::EnumValue { .. } => { /* literal constant, no model state */ }
                Var::EnumCtor { .. }  => { /* constructor reference, no model state */ }
            }
        }

        out.push(bindings);

        // No scalar blocking terms → same model forever; bail.
        if block_terms.is_empty() {
            break;
        }
        let refs: Vec<&Bool<'ctx>> = block_terms.iter().collect();
        let conj = Bool::and(ctx, &refs);
        cached.solver.assert(&conj.not());
    }

    cached.solver.pop(1);
    out
}

/// Push, assert per-tick givens, check, extract model, pop. Reuses all cached translation.
pub fn run_cached<'ctx>(
    cached: &CachedSchema<'ctx>,
    given: &HashMap<String, Value>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> EvalResult {
    cached.solver.push();
    for (name, value) in given {
        let Some(var) = cached.env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => cached.solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => cached.solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => cached.solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&crate::translate::z3_string(ctx, s).expect("nul in str"))),
            // PinnedInt: already folded at cache build; mismatching value forces UNSAT.
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => cached.solver.assert(&Bool::from_bool(ctx, false)),
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    cached.solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    cached.solver.assert(&b);
                } else {
                    eprintln!("warning: type mismatch for given {:?}", name);
                }
            }
        }
    }
    let check_result = cached.solver.check();
    let satisfied = matches!(check_result, SatResult::Sat);
    let mut bindings = HashMap::new();
    let extract_t0 = std::time::Instant::now();
    if satisfied {
        if let Some(model) = cached.solver.get_model() {
            for (name, var) in cached.env.iter() {
                match var {
                    Var::IntVar(i) => {
                        if let Some(v) = model.eval(i, true).and_then(|x| x.as_i64()) {
                            bindings.insert(name.clone(), Value::Int(v));
                        }
                    }
                    Var::BoolVar(b) => {
                        if let Some(v) = model.eval(b, true).and_then(|x| x.as_bool()) {
                            bindings.insert(name.clone(), Value::Bool(v));
                        }
                    }
                    Var::RealVar(r) => {
                        if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                            bindings.insert(name.clone(), Value::Real(real_value_to_f64(num, den)));
                        }
                    }
                    Var::StrVar(s) => {
                        if let Some(v) = model.eval(s, true).and_then(|x| x.as_string()) {
                            bindings.insert(name.clone(), Value::Str(unescape_z3_string(&v)));
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
                    Var::EnumValue { .. } => { /* literal, no model state */ }
                    Var::EnumCtor { .. }  => { /* constructor reference, no model state */ }
                }
            }
        }
    }
    let _ = extract_t0;
    cached.solver.pop(1);
    EvalResult { satisfied, bindings, unsat_core_items: None }
}
