//! Compile-time structural analysis. Builds a solver the same way
//! `evaluate` does, then reads back the asserted Bool constraints +
//! free-variable names *without* calling `check()`.
//!
//!   * `analyze_decomposition`   — returns a list of independent
//!                                  `Component`s (groups of variables
//!                                  that share at least one constraint).
//!   * `classify_components`     — same as above, plus a per-component
//!                                  functional-vs-non-functional verdict
//!                                  derived from a 2-copy uniqueness
//!                                  check.
//!
//! Both feed `compile-claims-to-functions.md` — they exist to let the
//! function-izer split a composed claim into independent sub-models
//! before native-compiling each piece.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, SatResult};

use crate::ast::*;
use super::super::types::{DatatypeRegistry, EnumRegistry, Value, Var};
use super::super::declare::{apply_seq_lengths, apply_set_candidates};
use super::super::extract::{assert_seq_given, assert_set_given};
use super::super::inline::inline_body_items;
use super::super::preprocess::{apply_pinned_ints, collect_pinned_ints};
use super::solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64};

/// Analysis-only entry: build the solver exactly the way `evaluate`
/// does, then read out the asserted Bool constraints and free-variable
/// names *without* calling `solver.check()`. Intended for compile-time
/// structural passes — currently the decomposition pass that re-separates
/// composed claims into independent sub-models. See
/// `crate::decompose` for the union-find walker, and
/// `docs/design/compile-claims-to-functions.md` ("Decomposition") for
/// the architectural framing.
///
/// `given_keys` are treated as broadcast constants — they don't link
/// components, so the returned `var_names` excludes them.
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

    // The setup phases below mirror `evaluate`'s build phase exactly —
    // declare, pin, inline. We deliberately skip the final `solver.check()`
    // and the model-extraction loop.
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

    // Pin given values (same as evaluate's Pass 3) so given-keyed vars
    // get assertions that we can then exclude from var_names.
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

    // Collect free-variable names: every entry in env EXCLUDING
    //   - given-keyed names (those are broadcast constants), and
    //   - enum variant constants like `Init`, `Done` populated by
    //     `populate_enum_variants` (those are runtime-constant
    //     values, not user-declared variables).
    let var_names: Vec<String> = env.iter()
        .filter(|(n, _)| !given.contains_key(n.as_str()))
        .filter(|(_, v)| !matches!(v, Var::EnumValue { .. } | Var::EnumCtor { .. }))
        .map(|(n, _)| n.clone())
        .collect();

    let assertions = solver.get_assertions();
    crate::decompose::decompose(ctx, &assertions, &var_names)
}

/// A component plus a functionality verdict — whether the component's
/// variables are uniquely determined by the inputs (given + already-
/// determined components). UNSAT of the "another distinct model exists"
/// check means functional; SAT or UNKNOWN means non-functional.
#[derive(Debug, Clone)]
pub struct ClassifiedComponent {
    pub component: crate::decompose::Component,
    /// `true` iff the 2-copy uniqueness check is UNSAT for this
    /// component's variables — i.e., no two distinct satisfying models
    /// disagree on this component's variables, holding `given` fixed.
    /// `false` means non-functional OR Z3 returned Unknown.
    pub functional: bool,
}

/// Same as `analyze_decomposition`, plus a per-component verdict:
/// is this component function-shaped (outputs uniquely determined
/// by inputs)?
///
/// The check is: solver.check() → if SAT, capture the model, then
/// assert "at least one variable in the component differs from its
/// model value" and check again. UNSAT means functional; SAT means
/// another distinct model exists → non-functional.
///
/// Per-component cost: 1 push, 1 assert, 1 check, 1 pop. Cheap
/// relative to a full solve.
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

    // Mirror analyze_decomposition's build phase.
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

    // If the body is UNSAT under given, mark every component as
    // NON-functional. Don't say "vacuously functional" — that's
    // technically true (no two distinct models exist when no model
    // exists) but it lets downstream callers (the function-izer's
    // rt.query hook) skip the solve entirely and produce SAT=true
    // with wrong bindings. Force the caller to go through Z3 so it
    // correctly returns UNSAT.
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

    // Per-component 2-copy check: assert that AT LEAST ONE variable
    // in this component differs from its current model value. UNSAT
    // means no such alternative model exists → functional.
    let mut out = Vec::with_capacity(components.len());
    for component in components {
        // Build the "differs from model" disjunction. Skip variables
        // whose types we can't easily compare against a Z3 value
        // (Seq, Set, Datatype) for v1; treat presence of such vars
        // as "couldn't classify, assume non-functional."
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
                    // A Seq's "value" is its length + array contents at
                    // in-range indices. The array can have arbitrary
                    // values outside [0, len), so naive `arr ≠ model_arr`
                    // is trivially SAT (Z3 picks a different out-of-range
                    // index). Encode the in-range existential explicitly:
                    //   len ≠ model_len  ∨  ∃ k ∈ [0, len). arr[k] ≠ model_arr[k]
                    let len_val = model.eval(len, true);
                    let arr_val = model.eval(arr, true);
                    if let Some(lv) = &len_val {
                        disjuncts.push(len._eq(lv).not());
                    }
                    if let Some(av) = &arr_val {
                        let k = z3::ast::Int::fresh_const(ctx, "fz_k");
                        let zero = z3::ast::Int::from_i64(ctx, 0);
                        let lo = zero.le(&k);
                        let hi = k.lt(len);
                        let in_range = z3::ast::Bool::and(ctx, &[&lo, &hi]);
                        let elem_diff = arr.select(&k)._eq(&av.select(&k)).not();
                        // Existential `∃k. P(k)` over Int is encoded as
                        // a fresh `k` AST plus `P(k)` asserted; Z3 treats
                        // free constants existentially in the assertion
                        // context.
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
            // No differable vars (component is all pinned ints or only
            // contains unsupported types we skipped). Treat as functional.
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
