//! Solve & model-extraction: turn a `SchemaDecl` + given bindings into an
//! `EvalResult`. Owns the tuned solver, the one-shot `evaluate`, the cached
//! build/run path, the extra-assertions (pins) variant, and the Z3-model →
//! `Value` decoders.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::{Context, Params, SatResult, Solver};

use crate::core::ast::*;
use crate::core::{CompiledModel, DatatypeRegistry, EnumRegistry, EvalResult, SeqElem, Value, Var};

use super::declare::{apply_seq_lengths, apply_set_candidates, declare_var};
use super::extract::{assert_seq_given, assert_set_given, extract_seq, extract_seq_composite, extract_set, unescape_z3_string};
use super::inline::inline_body_items;
use super::declare::{apply_pinned_ints, collect_pinned_ints};

// ───────────────────────── one-shot evaluate ─────────────────────────

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
            BodyItem::ClaimCall { .. } => {

            }
            BodyItem::SubclaimDecl(_) => {

            }

            BodyItem::Constraint(_) => {}
        }
    }

    let seq_lens = super::declare::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited, false);

    for (name, value) in given {
        let Some(var) = env.get(name) else { continue };
        match (var, value) {
            (Var::IntVar(v),  Value::Int(n))  => solver.assert(&v._eq(&Int::from_i64(ctx, *n))),
            (Var::BoolVar(v), Value::Bool(b)) => solver.assert(&v._eq(&Bool::from_bool(ctx, *b))),
            (Var::RealVar(v), Value::Real(f)) => solver.assert(&v._eq(&real_from_f64(ctx, *f))),
            (Var::StrVar(v),  Value::Str(s))  => solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),

            (Var::PinnedInt(v), Value::Int(n)) if *v == *n => {}
            (Var::PinnedInt(_), Value::Int(_)) => solver.assert(&Bool::from_bool(ctx, false)),
            (Var::EnumVar { ast, .. }, val @ Value::Enum { .. }) => {
                if let Some(reg) = enums {
                    if let Some(dt) = super::effect_codec::value_enum_to_datatype(val, ctx, reg) {
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

    let check_result = solver.check();
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
                    Var::DatatypeSetVar { .. } => {  }
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
                    Var::EnumValue { .. } => {  }
                    Var::EnumCtor { .. }  => {  }
                }
            }
        }
    }
    EvalResult { satisfied, bindings }
}

// ───────────────────────── solver tuning + shared helpers ─────────────────────────

fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let (num, den) = f64_to_int_rational(f);
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}

fn f64_to_int_rational(f: f64) -> (String, String) {
    let s = f.to_string();
    if let Some(dot) = s.find('.') {
        let (int_part, frac_with_dot) = s.split_at(dot);
        let frac = &frac_with_dot[1..];
        let num = format!("{}{}", int_part, frac);
        let den = format!("1{}", "0".repeat(frac.len()));
        (num, den)
    } else {
        (s, "1".to_string())
    }
}

fn real_value_to_f64(num: i64, den: i64) -> f64 {
    if den == 0 { 0.0 } else { num as f64 / den as f64 }
}

fn apply_solver_tuning(ctx: &Context, solver: &Solver, arith_solver: u32) {
    if arith_solver == 0 { return; }
    let mut params = Params::new(ctx);
    params.set_u32("smt.arith.solver", arith_solver);
    solver.set_params(&params);
}

fn make_tuned_solver<'ctx>(ctx: &'ctx Context, arith_solver: u32) -> Solver<'ctx> {
    let solver = z3::Tactic::new(ctx, "solve-eqs")
        .and_then(&z3::Tactic::new(ctx, "smt"))
        .solver();
    apply_solver_tuning(ctx, &solver, arith_solver);
    solver
}

fn populate_enum_variants<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    enums: Option<&EnumRegistry>,
) where 'ctx: 'static {
    let Some(reg) = enums else { return };
    for (enum_name, (dt, variants)) in reg.by_name.borrow().iter() {
        for (idx, variant) in variants.iter().enumerate() {
            if variant.fields.is_empty() {

                let ctor = &dt.variants[idx].constructor;
                let ast = ctor.apply(&[]).as_datatype()
                    .expect("nullary enum variant must yield a Datatype value");
                env.insert(variant.name.clone(), Var::EnumValue { ast });
            } else {
                env.insert(variant.name.clone(), Var::EnumCtor {
                    dt: *dt,
                    variant_idx: idx,
                    field_types: variant.fields.iter()
                        .map(|f| f.type_name.clone()).collect(),
                });
            }
            let _ = enum_name;
        }
    }
}

fn declare_and_assert(
    ctx: &'static Context,
    solver: &Solver<'static>,
    env: &mut HashMap<String, Var<'static>>,
    name: &str,
    type_name: &str,
    schemas: &HashMap<String, SchemaDecl>,
    registry: Option<&DatatypeRegistry>,
    enums: Option<&EnumRegistry>,
) {
    let post: Vec<Bool<'static>> = declare_var(ctx, env, name, type_name, schemas, registry, enums);
    for c in &post { solver.assert(c); }
}

// ───────────────────────── cached build + run ─────────────────────────

pub fn build_cache(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    given: &HashMap<String, Value>,
    arith_solver: u32,
) -> CompiledModel<'static> {
    build_cache_opts(schema, schemas, ctx, registry, enums, given, arith_solver, true)
}

/// Like `build_cache`, but `pin_ints` controls the equality-constant-folding
/// optimization. With `pin_ints = true` (the query/viz default), a body
/// constraint like `x = 7` folds `x` into the literal `7` so the solver never
/// declares `x` — faster, but the residue (`(> 7 10)`) is what serializes to
/// SMT-LIB. The user-facing `export_claim` passes `false` so every named var
/// stays a declared Z3 constant and `x = 7` survives as `(assert (= x 7))` —
/// the faithful relational model Ana hands to z3 (#192).
#[allow(clippy::too_many_arguments)]
pub fn build_cache_opts(
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    given: &HashMap<String, Value>,
    arith_solver: u32,
    pin_ints: bool,
) -> CompiledModel<'static> {

    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
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

    let seq_lens = super::declare::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    if pin_ints {
        let pinned = collect_pinned_ints(&schema.body, given, &seq_lens);
        apply_pinned_ints(&mut env, &pinned);
    }
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();

    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited, true);

    CompiledModel { env, solver, arith_solver }
}

pub fn run_cached<'ctx>(
    cached: &CompiledModel<'ctx>,
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
            (Var::StrVar(v),  Value::Str(s))  => cached.solver.assert(&v._eq(&Z3Str::from_str(ctx, s).expect("nul in str"))),

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
    let check_t0 = std::time::Instant::now();
    let check_result = cached.solver.check();
    let check_dt = check_t0.elapsed();
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
                    Var::DatatypeSetVar { .. } => {  }
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
                    Var::EnumValue { .. } => {  }
                    Var::EnumCtor { .. }  => {  }
                }
            }
        }
    }
    let _ = (check_dt, extract_t0);
    cached.solver.pop(1);
    EvalResult { satisfied, bindings }
}

// ───────────────────────── extra-assertions (pins) variant ─────────────────────────

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
    let _enum_guard = super::exprs::EnumRegistryGuard::new(enums);
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
                } else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                }
            }
            _ => {}
        }
    }

    let seq_lens = super::declare::collect_seq_lengths_with_schemas(
        &schema.body, given, Some(schemas));
    let pinned   = collect_pinned_ints(&schema.body, given, &seq_lens);
    apply_pinned_ints(&mut env, &pinned);
    apply_seq_lengths(&mut env, &seq_lens, ctx);

    apply_set_candidates(&env, given);

    let mut visited: HashMap<String, usize> = HashMap::new();
    inline_body_items(&schema.body, &mut env, &solver, schemas, ctx, registry, enums, &mut visited, false);

    for (var_name, value) in pins {
        if let Some(Var::EnumVar { ast, .. }) = env.get(*var_name) {
            solver.assert(&ast._eq(value));
        } else {
            eprintln!("warning: pin: variable `{}` is not enum-typed in `{}`",
                      var_name, schema.name);
        }
    }

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
                    if let Some(dt) = super::effect_codec::value_enum_to_datatype(val, ctx, reg) {
                        solver.assert(&ast._eq(&dt));
                    } else {
                        eprintln!("warning: given `{name}`: enum value did not encode \
                                   (registry missing variant?); leaving free");
                    }
                }
            }
            _ => {

                if let Some(b) = super::extract::assert_seq_given(var, value, ctx, enums) {
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

// ───────────────────────── model → Value decoders ─────────────────────────

pub(crate) fn extract_binding(
    name: &str, var: &Var<'static>, model: &z3::Model<'_>, ctx: &'static Context,
    bindings: &mut HashMap<String, Value>,
    enums: Option<&EnumRegistry>,
) {
    match var {
        Var::IntVar(i) => {
            if let Some(val) = model.eval(i, true) {
                if let Some(n) = val.as_i64() {
                    bindings.insert(name.to_string(), Value::Int(n));
                }
            }
        }
        Var::BoolVar(b) => {
            if let Some(val) = model.eval(b, true) {
                if let Some(bv) = val.as_bool() {
                    bindings.insert(name.to_string(), Value::Bool(bv));
                }
            }
        }
        Var::RealVar(r) => {
            if let Some((num, den)) = model.eval(r, true).and_then(|x| x.as_real()) {
                bindings.insert(name.to_string(), Value::Real(real_value_to_f64(num, den)));
            }
        }
        Var::StrVar(s) => {
            if let Some(val) = model.eval(s, true) {
                if let Some(sv) = val.as_string() {
                    bindings.insert(name.to_string(), Value::Str(unescape_z3_string(&sv)));
                }
            }
        }
        Var::SeqVar { arr, len, elem } => {
            if let Some(v) = extract_seq(arr, len, *elem, model, ctx) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::PinnedInt(v) => { bindings.insert(name.to_string(), Value::Int(*v)); }
        Var::SetVar { set, elem, candidates } => {
            if let Some(v) = extract_set(set, *elem, candidates, model, ctx) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::DatatypeSetVar { .. } => {  }
        Var::DatatypeSeqVar { arr, len, dt, fields, type_name } => {
            let extracted = if fields.is_empty() {
                extract_seq_enum(arr, len, type_name, *dt, model, ctx, enums)
            } else {
                extract_seq_composite(arr, len, fields.as_slice(), *dt, model, ctx, enums)
            };
            if let Some(v) = extracted {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::EnumVar { ast, enum_name, dt } => {
            if let Some(v) = extract_enum_value(ast, enum_name, dt, model, ctx, enums) {
                bindings.insert(name.to_string(), v);
            }
        }
        Var::EnumValue { .. } => {  }
        Var::EnumCtor { .. }  => {  }
    }
}

pub(super) fn extract_enum_value<'ctx>(
    ast: &z3::ast::Datatype<'ctx>,
    enum_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let evaluated = model.eval(ast, true)?;

    let mut active_idx: Option<usize> = None;
    for (i, variant) in dt.variants.iter().enumerate() {
        let test = variant.tester.apply(&[&evaluated]).as_bool()?;
        if let Some(true) = model.eval(&test, true).and_then(|b| b.as_bool()) {
            active_idx = Some(i);
            break;
        }
    }
    let idx = active_idx?;
    let variant = &dt.variants[idx];
    let variant_name = variant.constructor.name();

    let mut field_values: Vec<Value> = Vec::new();
    if let Some(reg) = enums {
        if let Some((_, decl_variants)) = reg.by_name.borrow().get(enum_name) {
            if let Some(decl_variant) = decl_variants.get(idx) {
                let mut acc_idx: usize = 0;
                for decl_field in decl_variant.fields.iter() {
                    if let Some(inner) = crate::core::parse_seq_type(&decl_field.type_name) {

                        let helper_name = crate::core::internal_cons_helper_name(inner);
                        let has_helper = reg.by_name.borrow().contains_key(&helper_name);
                        if has_helper {
                            let acc = &variant.accessors[acc_idx];
                            let cons_dyn = acc.apply(&[&evaluated]);
                            let extracted = extract_internal_cons_seq(
                                &helper_name, inner, &cons_dyn, model, ctx, enums);
                            if let Some(v) = extracted {
                                field_values.push(v);
                            }
                            acc_idx += 1;
                            continue;
                        }

                        let arr_acc = &variant.accessors[acc_idx];
                        let len_acc = &variant.accessors[acc_idx + 1];
                        let arr_dyn = arr_acc.apply(&[&evaluated]);
                        let len_dyn = len_acc.apply(&[&evaluated]);
                        let extracted = extract_seq_payload(
                            inner, &arr_dyn, &len_dyn, model, ctx, enums);
                        if let Some(v) = extracted {
                            field_values.push(v);
                        }
                        acc_idx += 2;
                        continue;
                    }
                    let accessor = &variant.accessors[acc_idx];
                    let raw = accessor.apply(&[&evaluated]);
                    let extracted = match decl_field.type_name.as_str() {
                        "Int" | "Nat" | "Pos" => raw.as_int()
                            .and_then(|i| model.eval(&i, true))
                            .and_then(|x| x.as_i64())
                            .map(Value::Int),
                        "Bool" => raw.as_bool()
                            .and_then(|b| model.eval(&b, true))
                            .and_then(|x| x.as_bool())
                            .map(Value::Bool),
                        "String" => raw.as_string()
                            .and_then(|s| model.eval(&s, true))
                            .and_then(|x| x.as_string())
                            .map(|s| Value::Str(unescape_z3_string(&s))),
                        "Real" => raw.as_real()
                            .and_then(|r| model.eval(&r, true))
                            .and_then(|x| x.as_real())
                            .map(|(num, den)| Value::Real(real_value_to_f64(num, den))),

                        ref_type => {
                            let target: &str = if ref_type == enum_name { enum_name }
                                               else { ref_type };
                            let nested_dt = reg.by_name.borrow().get(target)
                                .map(|(d, _)| *d);
                            if let Some(nested_dt) = nested_dt {
                                raw.as_datatype().and_then(|child_ast| {
                                    extract_enum_value(&child_ast, target,
                                                       nested_dt, model, ctx, enums)
                                })
                            } else { None }
                        }
                    };
                    if let Some(v) = extracted {
                        field_values.push(v);
                    }
                    acc_idx += 1;
                }
            }
        }
    }
    Some(Value::Enum {
        enum_name: enum_name.to_string(),
        variant: variant_name,
        fields: field_values,
    })
}

fn extract_seq_payload<'ctx>(
    inner_type: &str,
    arr_dyn: &z3::ast::Dynamic<'ctx>,
    len_dyn: &z3::ast::Dynamic<'ctx>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let len = len_dyn.as_int()?;
    match inner_type {
        "Int" | "Nat" | "Pos" => {
            let arr = arr_dyn.as_array()?;
            super::extract::extract_seq(&arr, &len, SeqElem::Int, model, ctx)
        }
        "Bool" => {
            let arr = arr_dyn.as_array()?;
            super::extract::extract_seq(&arr, &len, SeqElem::Bool, model, ctx)
        }
        "String" => {
            let arr = arr_dyn.as_array()?;
            super::extract::extract_seq(&arr, &len, SeqElem::Str, model, ctx)
        }
        enum_type => {

            let reg = enums?;
            let dt = reg.by_name.borrow().get(enum_type).map(|(d, _)| *d)?;
            let arr = arr_dyn.as_array()?;
            extract_seq_enum(&arr, &len, enum_type, dt, model, ctx, enums)
        }
    }
}

fn extract_internal_cons_seq<'ctx>(
    helper_name: &str,
    elem_type: &str,
    cons_dyn: &z3::ast::Dynamic<'ctx>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let reg = enums?;
    let by_name = reg.by_name.borrow();
    let (helper_dt, helper_variants) = by_name.get(helper_name)?;
    let helper_dt: &'static z3::DatatypeSort<'static> = *helper_dt;
    let empty_idx = helper_variants.iter().position(|v| v.fields.is_empty())?;
    let cell_idx = helper_variants.iter().position(|v| v.fields.len() == 2)?;
    let (elem_dt, _) = by_name.get(elem_type)?;
    let elem_dt: &'static z3::DatatypeSort<'static> = *elem_dt;
    drop(by_name);

    let empty_tester = &helper_dt.variants[empty_idx].tester;
    let cell_v = &helper_dt.variants[cell_idx];
    let head_acc = &cell_v.accessors[0];
    let tail_acc = &cell_v.accessors[1];

    let mut out: Vec<Value> = Vec::new();
    let mut cur = cons_dyn.clone();

    for _ in 0..10_000 {
        let is_empty_bool = empty_tester.apply(&[&cur]).as_bool()?;
        let is_empty = model.eval(&is_empty_bool, true)?.as_bool()?;
        if is_empty {
            return Some(Value::SeqEnum(out));
        }
        let head_dyn = head_acc.apply(&[&cur]);
        let head_dt = head_dyn.as_datatype()?;
        let head_val = extract_enum_value(&head_dt, elem_type, elem_dt, model, ctx, enums)?;
        out.push(head_val);
        cur = tail_acc.apply(&[&cur]);
    }
    None
}

pub(super) fn extract_seq_enum<'ctx>(
    arr: &z3::ast::Array<'ctx>,
    len: &Int<'ctx>,
    type_name: &str,
    dt: &'static z3::DatatypeSort<'static>,
    model: &z3::Model<'ctx>,
    ctx: &'ctx Context,
    enums: Option<&EnumRegistry>,
) -> Option<Value> {
    let n = model.eval(len, true)?.as_i64()?;
    if n < 0 { return None; }
    let mut out: Vec<Value> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let idx = Int::from_i64(ctx, i);
        let elem_dyn = arr.select(&idx);
        let elem = elem_dyn.as_datatype()?;
        let v = extract_enum_value(&elem, type_name, dt, model, ctx, enums)?;
        out.push(v);
    }
    Some(Value::SeqEnum(out))
}
