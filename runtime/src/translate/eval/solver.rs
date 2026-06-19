//! Solver construction + numeric / enum-setup helpers shared by every
//! `evaluate*` entry point in this module.
//!
//! Three concerns:
//!   * **Real-literal conversions** — `real_from_f64`, `real_value_to_f64`
//!     bridge Rust f64 and Z3's exact rational Real sort.
//!   * **Tuned solver construction** — `apply_solver_tuning`,
//!     `make_tuned_solver` build a `Solver` with the `solve-eqs` tactic
//!     chain and arith-solver tuning.
//!   * **Env priming + declare convenience** — `populate_enum_variants`,
//!     `declare_and_assert` pre-seed the env with enum constants and
//!     bundle `declare_var` + assert-post-conditions into one call.
//!
//! All helpers here are `pub(super)` and only used by sibling modules
//! under `translate::eval`.

use std::collections::HashMap;
use z3::ast::{Bool, Real};
use z3::{Context, Params, Solver};

use crate::core::ast::*;
use crate::core::{DatatypeRegistry, EnumRegistry, Var};
use super::super::declare::declare_var;

/// Build a Z3 Real literal from an f64 source value.
///
/// Splits `f.to_string()` (Rust's shortest-round-trip Display form,
/// so `3.14` formats as `"3.14"`) into pure-integer numerator and
/// denominator strings: `"3.14"` → `("314", "100")` → exact
/// rational `157/50` in Z3. Z3's numeral parser accepts integer
/// num/den directly, but is finicky about decimals in `"num/den"`
/// concatenation, so we do the split ourselves.
///
/// Edge cases: NaN / inf fall back to 0 (constraint solvers don't
/// have useful NaN semantics).
pub(super) fn real_from_f64<'ctx>(ctx: &'ctx Context, f: f64) -> Real<'ctx> {
    if f.is_nan() || f.is_infinite() {
        return Real::from_real(ctx, 0, 1);
    }
    let (num, den) = f64_to_int_rational(f);
    Real::from_real_str(ctx, &num, &den)
        .unwrap_or_else(|| Real::from_real(ctx, 0, 1))
}

/// `3.14` → `("314", "100")`. `-3.14` → `("-314", "100")`.
/// `42` → `("42", "1")`. Used by `real_from_f64` to feed Z3 only
/// integer numerator/denominator strings.
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

/// Convert a Z3 model's Real binding to f64. Z3 returns the exact
/// rational `(num, den)`; we project to f64 for the public Value
/// shape. Lossy in general; fine for the binding-display + tolerance-
/// based equality use cases.
pub(super) fn real_value_to_f64(num: i64, den: i64) -> f64 {
    if den == 0 { 0.0 } else { num as f64 / den as f64 }
}

/// Set `smt.arith.solver` to `arith_solver` on `solver`. Pass `0` to
/// skip (lets Z3 use its built-in default). The runtime's fixed default
/// is `2` (Z3's simplex-based arithmetic solver — empirically the best
/// across our workloads).
fn apply_solver_tuning(ctx: &Context, solver: &Solver, arith_solver: u32) {
    if arith_solver == 0 { return; }
    let mut params = Params::new(ctx);
    params.set_u32("smt.arith.solver", arith_solver);
    solver.set_params(&params);
}

/// Build a solver wrapped with the `solve-eqs` Z3 tactic preprocessing
/// chain. `solve-eqs` substitutes equality-defined variables before
/// solving — a 1.3-1.6× speedup across our workloads (`bench_tactics`
/// example) with no soundness regression (never converts SAT to UNSAT).
/// A terminal `smt` tactic decides SAT/UNSAT.
///
/// Tactics run as preprocessing inside the solver; substitutions
/// happen automatically. Model extraction goes through the original
/// variable names because Z3's tactic-derived solver handles the
/// model conversion under the hood.
pub(super) fn make_tuned_solver<'ctx>(ctx: &'ctx Context, arith_solver: u32) -> Solver<'ctx> {
    let solver = z3::Tactic::new(ctx, "solve-eqs")
        .and_then(&z3::Tactic::new(ctx, "smt"))
        .solver();
    apply_solver_tuning(ctx, &solver, arith_solver);
    solver
}

/// For every enum in the registry, pre-populate `env` with one
/// `Var::EnumValue` per variant name. Lets bare identifiers like
/// `Mon`, `Tue`, … resolve via the existing env-lookup path in
/// `translate_*` without any new code in exprs.rs.
///
/// Variant names are globally unique across all enums (enforced at
/// `register_enum`), so there's no clash risk. If a variant collides
/// with a user-declared variable name, the user's declaration in the
/// schema body will overwrite this entry — schema-local takes
/// precedence over the language-level constant.
pub(super) fn populate_enum_variants<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    enums: Option<&EnumRegistry>,
) where 'ctx: 'static {
    let Some(reg) = enums else { return };
    for (enum_name, (dt, variants)) in reg.by_name.borrow().iter() {
        for (idx, variant) in variants.iter().enumerate() {
            if variant.fields.is_empty() {
                // Nullary variant — pre-apply the constructor and stash
                // the Datatype value directly. Lets bare identifiers
                // resolve via env-lookup with no special-casing.
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

/// Allocate a typed Z3 const for `(name, type_name)` and immediately
/// issue any type-implied invariants on the solver. `declare_var`'s
/// own concern is allocation only — it returns a list of `Bool`
/// constraints (Nat / Pos / Seq-length non-negativity) that the caller
/// must assert. This helper bundles the common case.
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
