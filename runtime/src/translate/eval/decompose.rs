//! Compile-time structural analysis: `analyze_decomposition` returns independent `Component`s;
//! `classify_components` adds a per-component functional verdict via 2-copy uniqueness check.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, SatResult};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Value, Var};
use super::super::declare::{apply_seq_lengths, apply_set_candidates};
use super::super::extract::{assert_seq_given, assert_set_given};
use super::super::inline::inline_body_items;
use super::super::preprocess::{apply_pinned_ints, collect_pinned_ints};
use super::solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64};

/// Build solver like `evaluate`, read constraints + free var names without check().
/// `given` keys are broadcast constants excluded from `var_names`.
pub fn analyze_decomposition(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> Vec<crate::decompose::Component> {
    let _enum_guard = super::super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Mirrors evaluate's build phase (declare, pin, inline); skips check() and model extraction.
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

    // Pin given values (same as evaluate's Pass 3).
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(_), _) => {}
            (Var::EnumVar { ast, .. }, val @ Value::Enum { .. }) => {
                if let Some(reg) = enums {
                    if let Some(dt) = super::super::encode_ast::value_enum_to_datatype(val, ctx, reg) {
                        solver.assert(&ast._eq(&dt));
                    }
                }
            }
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    solver.assert(&b);
                }
            }
        }
    }

    // Collect free-var names: exclude given keys (broadcast constants) and enum variants.
    let var_names: Vec<String> = env.iter()
        .filter(|(n, _)| !given.contains_key(n.as_str()))
        .filter(|(_, v)| !matches!(v, Var::EnumValue { .. } | Var::EnumCtor { .. }))
        .map(|(n, _)| n.clone())
        .collect();

    let assertions = solver.get_assertions();
    crate::decompose::decompose(ctx, &assertions, &var_names)
}

/// A component plus a functional verdict: UNSAT on the 2-copy check = functional;
/// SAT or Unknown = non-functional.
#[derive(Debug, Clone)]
pub struct ClassifiedComponent {
    pub component: crate::decompose::Component,
    pub functional: bool,
}

/// Like `analyze_decomposition` plus a per-component functional verdict: SAT +
/// assert-a-different-model + UNSAT = functional. Cost: 1 push/assert/check/pop per component.
pub fn classify_components(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
) -> Vec<ClassifiedComponent> {
    let _enum_guard = super::super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Mirrors analyze_decomposition's build phase.
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
    let pinned = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(_), _) => {}
            (Var::EnumVar { ast, .. }, val @ Value::Enum { .. }) => {
                if let Some(reg) = enums {
                    if let Some(dt) = super::super::encode_ast::value_enum_to_datatype(val, ctx, reg) {
                        solver.assert(&ast._eq(&dt));
                    }
                }
            }
            _ => {
                if let Some(b) = assert_seq_given(var, value, ctx, enums) {
                    solver.assert(&b);
                } else if let Some(b) = assert_set_given(var, value, ctx) {
                    solver.assert(&b);
                }
            }
        }
    }

    let var_names: Vec<String> = env.iter()
        .filter(|(n, _)| !given.contains_key(n.as_str()))
        .filter(|(_, v)| !matches!(v, Var::EnumValue { .. } | Var::EnumCtor { .. }))
        .map(|(n, _)| n.clone())
        .collect();
    let assertions = solver.get_assertions();
    let components = crate::decompose::decompose(ctx, &assertions, &var_names);

    // If UNSAT under given, mark all components non-functional — "vacuously functional"
    // would let callers skip Z3 and return wrong bindings.
    let initial = solver.check();
    if !matches!(initial, SatResult::Sat) {
        return components.into_iter().map(|c|
            ClassifiedComponent { component: c, functional: false }
        ).collect();
    }
    let model = match solver.get_model() {
        Some(m) => m,
        None => return components.into_iter().map(|c|
            ClassifiedComponent { component: c, functional: false }
        ).collect(),
    };

    // Per-component 2-copy check: assert ≥1 var differs from model; UNSAT → functional.
    let mut out = Vec::with_capacity(components.len());
    for component in components {
        // Build differs-from-model disjunction; Seq/Set/Datatype are unsupported → non-functional.
        let mut disjuncts: Vec<Bool<'static>> = Vec::new();
        let mut skip_due_to_unsupported_var = false;
        for name in &component.vars {
            let Some(var) = env.get(name) else { continue };
            match var {
                Var::IntVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::BoolVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::RealVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::StrVar(v) => {
                    if let Some(val) = model.eval(v, true) {
                        disjuncts.push(v._eq(&val).not());
                    }
                }
                Var::PinnedInt(_) => {} // Already a literal; can't differ.
                Var::EnumVar { ast, .. } => {
                    if let Some(val) = model.eval(ast, true) {
                        disjuncts.push(ast._eq(&val).not());
                    }
                }
                Var::EnumValue { .. } | Var::EnumCtor { .. } => {} // Enum constant; can't differ.
                Var::SeqVar { arr, len, .. } | Var::DatatypeSeqVar { arr, len, .. } => {
                    // Naive arr ≠ model_arr is trivially SAT (Z3 picks an out-of-range index).
                    // Encode: len ≠ model_len ∨ ∃ k ∈ [0,len). arr[k] ≠ model_arr[k].
                    let len_val = model.eval(len, true);
                    let arr_val = model.eval(arr, true);
                    if let Some(lv) = &len_val {
                        disjuncts.push(len._eq(lv).not());
                    }
                    if let Some(av) = &arr_val {
                        let k = z3::ast::Int::fresh_const(ctx, "fz_k");
                        let zero = z3::ast::Int::from_i64(ctx, 0);
                        let in_range = z3::ast::Bool::and(ctx, &[&zero.le(&k), &k.lt(len)]);
                        let elem_diff = arr.select(&k)._eq(&av.select(&k)).not();
                        // Z3 treats a fresh free constant existentially in assertion context.
                        disjuncts.push(z3::ast::Bool::and(ctx, &[&in_range, &elem_diff]));
                    }
                }
                Var::SetVar { set, .. } | Var::DatatypeSetVar { set, .. } => {
                    if let Some(val) = model.eval(set, true) {
                        disjuncts.push(set._eq(&val).not());
                    }
                }
            }
        }
        let functional = if skip_due_to_unsupported_var {
            false
        } else if disjuncts.is_empty() {
            // All pinned ints or unsupported types; treat as functional.
            true
        } else {
            solver.push();
            let disj_refs: Vec<&Bool<'static>> = disjuncts.iter().collect();
            solver.assert(&Bool::or(ctx, &disj_refs));
            let r = solver.check();
            solver.pop(1);
            matches!(r, SatResult::Unsat)
        };
        out.push(ClassifiedComponent { component, functional });
    }
    out
}
