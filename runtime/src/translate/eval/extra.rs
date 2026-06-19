//! One-shot evaluate variants that inject extra value bindings into
//! a fresh solver beyond the schema's body and the user's `given` map.
//!
//!   * `evaluate_with_extra_assertion`     — pin a single enum-typed
//!                                            variable to a Z3 Datatype
//!                                            value. Used by
//!                                            `query_with_program`.
//!   * `evaluate_with_extra_assertions`    — same as above, but for
//!                                            multiple pins in one
//!                                            solve. Used by the
//!                                            multi-FSM scheduler to
//!                                            pin `state` +
//!                                            `last_results` per tick.
//!   * `evaluate_with_program_and_body`    — like `_with_extra_assertion`
//!                                            but injects both a
//!                                            `Program` enum value AND
//!                                            a flat `Seq(BodyItem)` for
//!                                            self-hosted desugar passes.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, String as Z3Str};
use z3::{Context, SatResult};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
use super::super::declare::{apply_seq_lengths, apply_set_candidates};
use super::super::inline::inline_body_items;
use super::super::preprocess::{apply_pinned_ints, collect_pinned_ints};
use super::solver::{declare_and_assert, make_tuned_solver, populate_enum_variants, real_from_f64};
use super::decode::extract_binding;

/// Stage 3 helper: like `evaluate`, but additionally asserts that
/// the variable named `extra_var` (which must have been declared in
/// the schema body as an enum-typed Membership) equals `extra_value`.
/// Used by `EvidentRuntime::query_with_program` to inject an
/// encoded `Program` value into a self-hosted pass.
///
/// Implementation: copy of `evaluate` with one extra `solver.assert`
/// after pass 2 and before the satisfiability check. Cleaner than
/// extending `evaluate` with an Option<extra_assertion> closure
/// because the AST-value injection is a niche operation that
/// shouldn't pollute the main entry point's signature.
pub fn evaluate_with_extra_assertion(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
    extra_var: &str,
    extra_value: z3::ast::Datatype<'static>,
) -> EvalResult {
    let _enum_guard = super::super::exprs::EnumRegistryGuard::new(enums);
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, enums);

    // Pass 1: declare. (Same as evaluate.)
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
    // Populate Set var candidates from given Value::Set* — needed before
    // body translation so `#s` cardinality folds to a literal count.
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    // Inject the extra value. Look up the variable, must be EnumVar
    // (i.e. enum-typed Membership). Anything else gets a warning;
    // assertion is silently skipped to avoid forcing UNSAT.
    if let Some(Var::EnumVar { ast, .. }) = env.get(extra_var) {
        solver.assert(&ast._eq(&extra_value));
    } else {
        eprintln!("warning: query_with_program: variable `{}` is not enum-typed in `{}`",
                  extra_var, schema.name);
    }

    let check_result = solver.check();
    let satisfied = matches!(check_result, SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, enums);
            }
        }
    }
    EvalResult { satisfied, bindings }
}

/// Like `evaluate_with_extra_assertion` but pins multiple enum-typed
/// variables in one solve. Used by the effect loop to pin both
/// `state` and `last_results` per step.
pub fn evaluate_with_extra_assertions(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    arith_solver: u32,
    pins: &[(&str, z3::ast::Datatype<'static>)],
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
                // Same as `evaluate` — pull in the passthrough'd
                // claim's Memberships so Pass 1.5's seq-length /
                // pinned-int collection can apply to them before
                // any `∀` over the inherited Seq tries to unroll.
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
            _ => {}
        }
    }

    let seq_lens = super::super::preprocess::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);
    // Populate Set var candidates from given Value::Set* — needed before
    // body translation so `#s` cardinality folds to a literal count.
    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited);

    for (var_name, value) in pins {
        if let Some(Var::EnumVar { ast, .. }) = env.get(*var_name) {
            solver.assert(&ast._eq(value));
        } else {
            eprintln!("warning: pin: variable `{}` is not enum-typed in `{}`",
                      var_name, schema.name);
        }
    }

    // Apply scalar `given` values (Int/Bool/String/Real). Same loop
    // as `evaluate`'s pass 3 — needed for the multi-FSM scheduler to
    // pin world.* fields each tick. Without this, callers using
    // query_with_pins_and_given for sub-field pins silently get free
    // (model-picked) values for the supposedly-given fields.
    //
    // `Value::Enum` values are also accepted here, so plugins that
    // write enum-typed world fields (currently the reflection
    // bridge's `world.program`) flow through the same path. The
    // value is re-encoded as a Z3 Datatype against the registry,
    // then asserted equal to the EnumVar's ast — same shape as the
    // explicit `pins` list above, just discovered through the
    // `given` map instead.
    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  =>
                solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),
            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            (Var::EnumVar { ast, .. }, val @ Value::Enum { .. }) => {
                if let Some(reg) = enums {
                    if let Some(dt) = super::super::encode_ast::value_enum_to_datatype(val, ctx, reg) {
                        solver.assert(&ast._eq(&dt));
                    } else {
                        eprintln!("warning: given `{name}`: enum value did not encode \
                                   (registry missing variant?); leaving free");
                    }
                }
            }
            _ => {
                // Seq pin: (DatatypeSeqVar, SeqEnum) / (SeqVar, SeqInt) etc.
                // The multi-FSM scheduler routes `last_results ∈ Seq(Result)`
                // through here; without this the pin is silently dropped and
                // the FSM solves with an unconstrained Seq.
                if let Some(b) = super::super::extract::assert_seq_given(var, value, ctx, enums) {
                    solver.assert(&b);
                }
            }
        }
    }

    let check_result = solver.check();
    let satisfied = matches!(check_result, SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, enums);
            }
        }
    }
    EvalResult { satisfied, bindings }
}

/// Stage 5.5: like `evaluate_with_extra_assertion` but injects two
/// related values — the encoded `Program` datatype AND a flat
/// `Seq(BodyItem)` for the user's first claim's body. Lets a
/// self-hosted pass iterate over arbitrary-length user programs
/// via `∀ i ∈ {0..#body-1} : …`.
///
/// The seq-injection asserts both `#body = items.len()` and
/// `body[i] = encoded(items[i])` for each i, so the variable's
/// model is fully pinned to the user's source.
pub fn evaluate_with_program_and_body(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: &EnumRegistry,
    arith_solver: u32,
    program_var: &str,
    program_value: z3::ast::Datatype<'static>,
    body_var: &str,
    body_items: &[BodyItem],
) -> EvalResult {
    let _enum_guard = super::super::exprs::EnumRegistryGuard::new(Some(enums));
    let solver = make_tuned_solver(ctx, arith_solver);
    let mut env: HashMap<String, Var<'static>> = HashMap::new();
    populate_enum_variants(&mut env, Some(enums));

    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name, .. } => {
                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), Some(enums));
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(claim) = schemas.get(claim_name) {
                    for sub in &claim.body {
                        if let BodyItem::Membership { name, type_name, .. } = sub {
                            if !env.contains_key(name) {
                                declare_and_assert(ctx, &solver, &mut env, name, type_name, schemas, Some(registry), Some(enums));
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

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, Some(enums), &mut visited);

    // Program injection.
    if let Some(Var::EnumVar { ast, .. }) = env.get(program_var) {
        solver.assert(&ast._eq(&program_value));
    } else {
        eprintln!("warning: program var `{}` is not enum-typed in `{}`",
                  program_var, schema.name);
    }
    // Body Seq injection.
    if let Some(Var::DatatypeSeqVar { arr, len, fields, .. }) = env.get(body_var) {
        if !fields.is_empty() {
            eprintln!("warning: body var `{}` is record-typed seq, not enum-typed; skipping",
                      body_var);
        } else {
            match super::super::encode_ast::encode_body_items_into_seq(
                body_items, arr, len, ctx, enums,
            ) {
                Ok(asserts) => for a in &asserts { solver.assert(a); },
                Err(e) => eprintln!("warning: body encode failed: {e}"),
            }
        }
    } else {
        eprintln!("warning: body var `{}` is not a Seq(enum) in `{}`",
                  body_var, schema.name);
    }

    let satisfied = matches!(solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                extract_binding(name, var, &model, ctx, &mut bindings, Some(enums));
            }
        }
    }
    EvalResult { satisfied, bindings }
}
