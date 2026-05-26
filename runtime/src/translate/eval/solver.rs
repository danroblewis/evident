//! Solver helpers for all `evaluate*` entry points: f64↔Z3-Real, `make_tuned_solver`,
//! `populate_enum_variants`, and `declare_and_assert`.

use std::collections::HashMap;
use z3::ast::{Bool, Real};
use z3::{Context, Params, Solver};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use super::super::declare::declare_var;

/// Build an exact Z3 Real from f64 by splitting Rust's Display form into integer
/// num/den strings (e.g. `3.14` → `("314","100")`). NaN/inf fall back to 0.
pub(super) fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let (num, den) = f64_to_int_rational(f);
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}

/// `3.14` → `("314","100")`, `-3.14` → `("-314","100")`, `42` → `("42","1")`.
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

/// Convert Z3 model Real `(num,den)` to f64. Lossy but fine for display/tolerance use.
pub(super) fn real_value_to_f64(num: i64, den: i64) -> f64 {
    if den == 0 { 0.0 } else { num as f64 / den as f64 }
}

/// Set `smt.arith.solver`; pass 0 to skip. Policy lives in `runtime::SolveHistory`.
fn apply_solver_tuning(ctx: &Context, solver: &Solver, arith_solver: u32) {
    if arith_solver == 0 { return; }
    let mut params = Params::new(ctx);
    params.set_u32("smt.arith.solver", arith_solver);
    solver.set_params(&params);
}

/// Build a solver via `EVIDENT_TACTICS` (default "solve-eqs"; "off"/"standard"/"aggressive"/custom).
/// `smt` always appended as terminal tactic.
pub(super) fn make_tuned_solver<'ctx>(ctx: &'ctx Context, arith_solver: u32) -> Solver<'ctx> {
    let chain = std::env::var("EVIDENT_TACTICS").ok();
    let chain_spec = chain.as_deref().unwrap_or("solve-eqs");
    let solver = match chain_spec {
        "" | "off" => Solver::new(ctx),
        spec => {
            let mut names: Vec<&str> = match spec {
                "simplify"   => vec!["simplify"],
                "standard"   => vec!["simplify", "propagate-values", "solve-eqs"],
                "aggressive" => vec!["simplify", "propagate-values", "solve-eqs",
                                     "elim-uncnstr", "propagate-ineqs"],
                custom => custom.split(',').map(|s| s.trim()).collect(),
            };
            // Preprocessors alone return Unknown; always append smt as terminal.
            if !names.last().map(|n| *n == "smt").unwrap_or(false) {
                names.push("smt");
            }
            let mut t = z3::Tactic::new(ctx, names[0]);
            for n in &names[1..] {
                t = t.and_then(&z3::Tactic::new(ctx, n));
            }
            t.solver()
        }
    };
    apply_solver_tuning(ctx, &solver, arith_solver);
    solver
}

/// Pre-populate env with one `Var::EnumValue` per variant so bare identifiers
/// like `Mon` resolve via env-lookup. Schema-local declarations overwrite on collision.
pub(super) fn populate_enum_variants<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    enums: Option<&EnumRegistry>,
) where 'ctx: 'static {
    let Some(reg) = enums else { return };
    for (enum_name, (dt, variants)) in reg.by_name.borrow().iter() {
        for (idx, variant) in variants.iter().enumerate() {
            if variant.fields.is_empty() {
                // Nullary: pre-apply constructor so bare identifiers resolve directly.
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

/// Declare a typed Z3 const and immediately assert its type-implied invariants
/// (Nat/Pos/Seq-length non-negativity). Bundles `declare_var` + assert.
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
