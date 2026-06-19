use std::collections::HashMap;
use z3::ast::{Bool, Real};
use z3::{Context, Params, Solver};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use super::super::declare::declare_var;

pub(super) fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
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

pub(super) fn real_value_to_f64(num: i64, den: i64) -> f64 {
    if den == 0 { 0.0 } else { num as f64 / den as f64 }
}

fn apply_solver_tuning(ctx: &Context, solver: &Solver, arith_solver: u32) {
    if arith_solver == 0 { return; }
    let mut params = Params::new(ctx);
    params.set_u32("smt.arith.solver", arith_solver);
    solver.set_params(&params);
}

pub(super) fn make_tuned_solver<'ctx>(ctx: &'ctx Context, arith_solver: u32) -> Solver<'ctx> {
    let solver = z3::Tactic::new(ctx, "solve-eqs")
        .and_then(&z3::Tactic::new(ctx, "smt"))
        .solver();
    apply_solver_tuning(ctx, &solver, arith_solver);
    solver
}

pub(super) fn populate_enum_variants<'ctx>(
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

pub(super) fn declare_and_assert(
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
