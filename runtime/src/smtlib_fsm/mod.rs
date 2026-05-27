//! SMT-LIB-driven FSMs — strategy 2 of runtime-evolve.
//!
//! This module lets the EXISTING multi-FSM engine run an FSM whose per-tick
//! constraint is authored as **raw SMT-LIB text + metadata**, bypassing the
//! Evident lexer/parser/translate. The design (see
//! `docs/design/runtime-evolve-seam.md`) is:
//!
//!   1. Metadata builds a *synthetic* `fsm`-keyword `SchemaDecl` (Memberships
//!      only, no constraints). `resolve_fsm` / `MainShape` / `all_fsms` / the
//!      `_var` time-shift scan all work off that shape with zero new code.
//!   2. The behavior lives in the SMT-LIB text. The scheduler's single per-tick
//!      call — `query_with_pins_and_given` — is intercepted for SMT-LIB FSMs and
//!      routed to [`solve_tick`], which parses the SMT-LIB into the runtime's
//!      leaked Z3 context, asserts the tick's `given`/pins, solves, and assembles
//!      the `bindings` the scheduler consumes (scalar outputs + an `effects`
//!      `Value::SeqEnum` built from the metadata effect template).
//!
//! Everything above the seam — state threading, effect collection/dispatch,
//! halt, event sources — is the existing engine, reused unchanged.
//!
//! **v1 scope: scalar-state FSMs** (`Int`/`Bool`/`Real`/`String` vars threaded
//! via the `_name` time-shift) with metadata-templated effects. Enum `state`
//! driven by SMT-LIB `(declare-datatypes …)` is the documented entanglement
//! boundary and is not handled here.

use std::collections::HashMap;

use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
use z3::{Context, SatResult, Solver};

use crate::core::ast::{BodyItem, Expr, Keyword, Pins, SchemaDecl};
use crate::core::{QueryResult, Value};

mod meta;
pub use meta::{
    parse_fixture, parse_meta, ArgSource, EffectSpec, FixtureProgram, FsmMeta, SmtSort, VarDecl,
    WorldDecl,
};

mod decode;
pub use decode::{solve_smtlib_decode_all, DecodeOutcome};

#[cfg(test)]
mod tests;

/// A loaded SMT-LIB FSM: the metadata (shape + effect template) plus the raw
/// SMT-LIB constraint text (declare-consts + asserts, no `check-sat`).
#[derive(Debug, Clone)]
pub struct SmtLibFsm {
    pub meta: FsmMeta,
    pub smtlib: String,
}

impl SmtLibFsm {
    /// Build the synthetic `fsm`-keyword `SchemaDecl` the scheduler resolves
    /// `MainShape` from. Body is Memberships only — the behavior is the SMT-LIB.
    pub fn synthetic_schema(&self) -> SchemaDecl {
        build_synthetic_schema(&self.meta)
    }
}

// ---------------------------------------------------------------------------
// Synthetic SchemaDecl: metadata → the shape `resolve_fsm` walks
// ---------------------------------------------------------------------------

fn membership(name: &str, type_name: &str) -> BodyItem {
    BodyItem::Membership {
        name: name.to_string(),
        type_name: type_name.to_string(),
        pins: Pins::None,
    }
}

/// Build the Memberships-only `fsm` schema. We declare the FSM's scalar vars
/// (so the `_name` time-shift scan and resolve_fsm see them), plus the
/// `effects` / `last_results` slots so `MainShape` resolves `effects_var` /
/// `last_results_var`, plus world membership(s) for multi-FSM coordination.
fn build_synthetic_schema(meta: &FsmMeta) -> SchemaDecl {
    let mut body: Vec<BodyItem> = Vec::new();
    for v in &meta.vars {
        // Skip the auto-injected `is_first_tick` (the engine provides it) — but
        // DO declare `_name` time-shift vars so the scan injects prev values.
        if v.name == "is_first_tick" {
            continue;
        }
        // `world.X` / `world_next.X` vars are NOT record-leaf Memberships (the
        // `world`/`world_next` record Memberships below cover them). Instead emit
        // a dotted-Identifier marker constraint so the world-access-set walk
        // (`portable::subscriptions::access_sets`) classifies the read/write —
        // this is what wakes reader FSMs and scopes writer snapshots. The marker
        // is never translated; the SMT-LIB path intercepts before evaluate().
        if v.name.starts_with("world.") || v.name.starts_with("world_next.") {
            body.push(BodyItem::Constraint(Expr::Identifier(v.name.clone())));
            continue;
        }
        body.push(membership(&v.name, v.sort.evident_type()));
    }
    if let Some(eff) = &meta.effects_var {
        body.push(membership(eff, "Seq(Effect)"));
    }
    if let Some(lr) = &meta.last_results_var {
        body.push(membership(lr, "Seq(Result)"));
    }
    if let Some(w) = &meta.world_var {
        let ty = meta.world_type.as_deref().unwrap_or("World");
        body.push(membership(w, ty));
    }
    if let Some(wn) = &meta.world_next_var {
        let ty = meta.world_type.as_deref().unwrap_or("World");
        body.push(membership(wn, ty));
    }
    SchemaDecl {
        keyword: Keyword::Fsm,
        name: meta.fsm.clone(),
        type_params: vec![],
        param_count: 0,
        external: false,
        body,
    }
}

// ---------------------------------------------------------------------------
// raw_ctx: reach the Z3_context behind a z3::Context to detect parse errors
// (the z3 crate's `from_string` swallows them). Mirrors translate/smtlib.rs.
// ---------------------------------------------------------------------------

const _: () = {
    assert!(
        std::mem::size_of::<Context>() == std::mem::size_of::<z3_sys::Z3_context>(),
        "z3::Context is no longer a single-pointer newtype; raw_ctx is unsound"
    );
};

#[inline]
fn raw_ctx(ctx: &Context) -> z3_sys::Z3_context {
    // SAFETY: layout verified by the const assert above.
    unsafe { *(ctx as *const Context as *const z3_sys::Z3_context) }
}

/// True if Z3 is in an error state (e.g. the SMT-LIB parser rejected the text).
fn z3_error(ctx: &Context) -> Option<String> {
    let code = unsafe { z3_sys::Z3_get_error_code(raw_ctx(ctx)) };
    if code == z3_sys::ErrorCode::OK {
        return None;
    }
    let msg = unsafe {
        let p = z3_sys::Z3_get_error_msg(raw_ctx(ctx), code);
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    };
    Some(format!("{code:?}: {msg}"))
}

// ---------------------------------------------------------------------------
// The per-tick solve — the SMT-LIB counterpart of evaluate()/run_cached()
// ---------------------------------------------------------------------------

/// Solve one tick of an SMT-LIB FSM. Parses the constraint text into the
/// runtime's leaked context, asserts the tick's scalar `given` + `pins`,
/// checks, and assembles `bindings` (scalar outputs + the `effects` SeqEnum).
///
/// On a Z3 parse error this returns an UNSAT `QueryResult` after logging — the
/// scheduler treats UNSAT as a hard stop, which is the right failure mode for a
/// malformed fixture.
pub fn solve_tick(
    rt: &crate::runtime::EvidentRuntime,
    fsm: &SmtLibFsm,
    _pins: &[(&str, z3::ast::Datatype<'static>)],
    given: &HashMap<String, Value>,
) -> QueryResult {
    let ctx: &'static Context = rt.z3_context();
    let solver = Solver::new(ctx);

    // Parse the base constraint text (declare-consts + asserts) into the solver.
    solver.from_string(fsm.smtlib.clone());
    if let Some(e) = z3_error(ctx) {
        eprintln!("[smtlib-fsm] `{}`: Z3 rejected SMT-LIB ({e})", fsm.meta.fsm);
        return QueryResult { satisfied: false, bindings: HashMap::new() };
    }

    let sorts: HashMap<&str, SmtSort> =
        fsm.meta.vars.iter().map(|v| (v.name.as_str(), v.sort)).collect();

    // Assert each scalar `given` whose name is a declared const. Non-scalar
    // givens (last_results SeqEnum, enum state) are not part of the v1 subset.
    for (name, value) in given {
        let Some(sort) = sorts.get(name.as_str()) else { continue };
        if let Some(assertion) = scalar_eq(ctx, name, *sort, value) {
            solver.assert(&assertion);
        }
    }

    // Input bindings: pull a payload out of the previous tick's `last_results`
    // into a declared const (the "read an effect result" pattern). On a miss
    // (tick 0, wrong variant, out of range) the binding's default applies.
    for binding in &fsm.meta.inputs {
        let value = resolve_input_binding(binding, fsm, given);
        if let Some(assertion) = scalar_eq(ctx, &binding.var, binding.sort, &value) {
            solver.assert(&assertion);
        }
    }

    let satisfied = match solver.check() {
        SatResult::Sat => true,
        SatResult::Unsat => false,
        SatResult::Unknown => {
            eprintln!("[smtlib-fsm] `{}`: Z3 returned Unknown", fsm.meta.fsm);
            false
        }
    };

    let mut bindings: HashMap<String, Value> = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            // Scalar outputs the scheduler threads as state / writes / reads.
            for name in &fsm.meta.outputs {
                if let Some(sort) = sorts.get(name.as_str()) {
                    if let Some(v) = read_scalar(ctx, &model, name, *sort) {
                        bindings.insert(name.clone(), v);
                    }
                }
            }
            // Assemble the effect list from the template, keyed on model values.
            if let Some(eff_var) = &fsm.meta.effects_var {
                let effects = assemble_effects(ctx, &model, &fsm.meta, &sorts);
                bindings.insert(eff_var.clone(), Value::SeqEnum(effects));
            }
        }
    }

    QueryResult { satisfied, bindings }
}

/// Resolve an input binding to the value to assert: the matching payload field
/// from `last_results[index]`, or the binding's literal default on any miss.
fn resolve_input_binding(
    binding: &meta::InputBinding,
    fsm: &SmtLibFsm,
    given: &HashMap<String, Value>,
) -> Value {
    let lr_key = fsm.meta.last_results_var.as_deref().unwrap_or("last_results");
    let extracted = match given.get(lr_key) {
        Some(Value::SeqEnum(items)) => items.get(binding.index).and_then(|item| match item {
            Value::Enum { variant, fields, .. }
                if *variant == binding.variant && !fields.is_empty() =>
            {
                Some(fields[0].clone())
            }
            _ => None,
        }),
        _ => None,
    };
    extracted.unwrap_or_else(|| arg_literal_value(&binding.default))
}

/// A literal `ArgSource` as a `Value`; `ArgSource::Var` has no literal value
/// (it requires a model) so it falls back to an empty string.
fn arg_literal_value(arg: &ArgSource) -> Value {
    match arg {
        ArgSource::LitInt(n) => Value::Int(*n),
        ArgSource::LitStr(s) => Value::Str(s.clone()),
        ArgSource::LitBool(b) => Value::Bool(*b),
        ArgSource::Var(_) => Value::Str(String::new()),
    }
}

/// Build a `name == value` Z3 Bool for a scalar `given`, or `None` on a
/// sort/value mismatch (silently skipped — the scheduler may pass extra keys).
fn scalar_eq<'ctx>(
    ctx: &'ctx Context,
    name: &str,
    sort: SmtSort,
    value: &Value,
) -> Option<Bool<'ctx>> {
    match (sort, value) {
        (SmtSort::Int, Value::Int(n)) => {
            Some(Int::new_const(ctx, name)._eq(&Int::from_i64(ctx, *n)))
        }
        (SmtSort::Bool, Value::Bool(b)) => {
            Some(Bool::new_const(ctx, name)._eq(&Bool::from_bool(ctx, *b)))
        }
        (SmtSort::Real, Value::Real(r)) => {
            let scaled = (*r * 1_000_000.0) as i32;
            Some(Real::new_const(ctx, name)._eq(&Real::from_real(ctx, scaled, 1_000_000)))
        }
        (SmtSort::Str, Value::Str(s)) => {
            let z = crate::translate::z3_string(ctx, s).ok()?;
            Some(Z3Str::new_const(ctx, name)._eq(&z))
        }
        _ => None,
    }
}

/// Read one scalar const out of the model by name+sort. Z3 interns symbols, so
/// a fresh `*::new_const` resolves to the symbol the SMT-LIB parser created.
fn read_scalar(ctx: &Context, model: &z3::Model, name: &str, sort: SmtSort) -> Option<Value> {
    match sort {
        SmtSort::Int => {
            let c = Int::new_const(ctx, name);
            model.eval(&c, true)?.as_i64().map(Value::Int)
        }
        SmtSort::Bool => {
            let c = Bool::new_const(ctx, name);
            model.eval(&c, true)?.as_bool().map(Value::Bool)
        }
        SmtSort::Real => {
            let c = Real::new_const(ctx, name);
            let (num, den) = model.eval(&c, true)?.as_real()?;
            Some(Value::Real(num as f64 / den as f64))
        }
        SmtSort::Str => {
            let c = Z3Str::new_const(ctx, name);
            model
                .eval(&c, true)?
                .as_string()
                .map(|s| Value::Str(crate::translate::unescape_z3_string(&s)))
        }
    }
}

/// Build the per-tick `Value::Enum{Effect}` list from the metadata template.
/// Each `EffectSpec` is included when its guard Bool evaluates true in the model
/// (an unguarded spec always fires). Effect args resolve to literals or to
/// scalar model values.
fn assemble_effects(
    ctx: &Context,
    model: &z3::Model,
    meta: &FsmMeta,
    sorts: &HashMap<&str, SmtSort>,
) -> Vec<Value> {
    let mut out = Vec::new();
    for spec in &meta.effects {
        if let Some(guard) = &spec.guard {
            let b = Bool::new_const(ctx, guard.as_str());
            let fired = model.eval(&b, true).and_then(|x| x.as_bool()).unwrap_or(false);
            if !fired {
                continue;
            }
        }
        let fields: Vec<Value> = spec
            .args
            .iter()
            .map(|a| resolve_arg(ctx, model, a, sorts))
            .collect();
        out.push(Value::Enum {
            enum_name: "Effect".to_string(),
            variant: spec.variant.clone(),
            fields,
        });
    }
    out
}

/// Resolve an effect argument to a `Value`: a literal, or the model value of a
/// scalar const.
fn resolve_arg(
    ctx: &Context,
    model: &z3::Model,
    arg: &ArgSource,
    sorts: &HashMap<&str, SmtSort>,
) -> Value {
    match arg {
        ArgSource::LitInt(n) => Value::Int(*n),
        ArgSource::LitStr(s) => Value::Str(s.clone()),
        ArgSource::LitBool(b) => Value::Bool(*b),
        ArgSource::Var(name) => {
            let sort = sorts.get(name.as_str()).copied().unwrap_or(SmtSort::Int);
            read_scalar(ctx, model, name, sort).unwrap_or(Value::Int(0))
        }
    }
}
