//! Generic Z3-model decoder for the SMT-LIB FSM path — the enum-state increment.
//!
//! strategy-2 v1 (`solve_tick`) reads only *scalar* outputs (`Int`/`Bool`/`Real`/
//! `Str`) by name+sort, because enum `state` driven by an SMT-LIB
//! `(declare-datatypes …)` is its documented entanglement boundary: the typed
//! `z3` crate would need the runtime's *registered* `DatatypeSort` to build a
//! const of that sort, and a parser-created duplicate of the same name won't
//! reconcile.
//!
//! This module crosses that boundary without the registered sort: it walks the
//! solved model's constants **generically via raw `z3-sys`** (the same shape as
//! the greenfield `runtime-smt` engine's `z3c::decode_model`), decoding each
//! assigned constant — scalars, datatypes (enums) as `Value::Enum`, and
//! sequences as `Value::SeqEnum` — directly from the model. No typed Sort, no
//! duplicate-sort reconciliation needed.
//!
//! It is **purely additive**: a new function on the SMT-LIB path, not called by
//! `solve_tick`, the `effect-run-smtlib` command, or the existing tests. The
//! Evident-source decode (`translate/eval/decode.rs`) is untouched.

use std::collections::HashMap;
use std::ffi::CStr;

use z3::Context;
use z3_sys::*;

use crate::core::Value;

use super::raw_ctx;

/// Outcome of solving the pinned single-tick SMT-LIB and decoding the full model.
pub enum DecodeOutcome {
    /// SAT — every assigned constant decoded by name.
    Sat(HashMap<String, Value>),
    /// UNSAT — the pinned transition has no model (negative fixture witness).
    Unsat,
    /// Z3 returned `unknown`, or the parser rejected the text.
    Err(String),
}

/// Parse `smtlib` (declare-consts + asserts + inline pins, NO `check-sat`) into a
/// fresh raw solver on `ctx`'s Z3 context, solve, and decode every assigned
/// model constant into a [`Value`]. Reuses the runtime's `'static` context (the
/// strategy-2 "reuse the existing runtime" property) but its own solver, which
/// it ref-drops; the decoded `Value`s own no Z3 handles, so nothing escapes.
pub fn solve_smtlib_decode_all(ctx: &Context, smtlib: &str) -> DecodeOutcome {
    let raw = raw_ctx(ctx);
    let cstr = match std::ffi::CString::new(smtlib) {
        Ok(c) => c,
        Err(_) => return DecodeOutcome::Err("SMT-LIB text contained an interior NUL".into()),
    };
    unsafe {
        let solver = Z3_mk_solver(raw);
        Z3_solver_inc_ref(raw, solver);
        // Defer the dec_ref so every early return frees the solver.
        let _guard = SolverGuard { raw, solver };

        Z3_solver_from_string(raw, solver, cstr.as_ptr());
        if let Some(msg) = z3_err(raw) {
            return DecodeOutcome::Err(format!("rejected SMT-LIB: {msg}"));
        }

        let r = Z3_solver_check(raw, solver);
        if r == Z3_L_FALSE {
            return DecodeOutcome::Unsat;
        }
        if r != Z3_L_TRUE {
            return DecodeOutcome::Err("z3 returned unknown".into());
        }

        let model = Z3_solver_get_model(raw, solver);
        Z3_model_inc_ref(raw, model);
        let out = decode_model(raw, model);
        Z3_model_dec_ref(raw, model);
        DecodeOutcome::Sat(out)
    }
}

struct SolverGuard {
    raw: Z3_context,
    solver: Z3_solver,
}
impl Drop for SolverGuard {
    fn drop(&mut self) {
        unsafe { Z3_solver_dec_ref(self.raw, self.solver) }
    }
}

unsafe fn z3_err(raw: Z3_context) -> Option<String> {
    let code = Z3_get_error_code(raw);
    if code == ErrorCode::OK {
        return None;
    }
    Some(cstr_to_string(Z3_get_error_msg(raw, code)))
}

/// Decode every constant the model assigns, keyed by name (sorted for
/// determinism is unnecessary — caller looks up by name).
unsafe fn decode_model(ctx: Z3_context, m: Z3_model) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let n = Z3_model_get_num_consts(ctx, m);
    for i in 0..n {
        let decl = Z3_model_get_const_decl(ctx, m, i);
        let name = cstr_to_string(Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, decl)));
        let interp = Z3_model_get_const_interp(ctx, m, decl);
        if interp.is_null() {
            continue;
        }
        out.insert(name, read_ast_value(ctx, interp));
    }
    out
}

/// Recursively decode a Z3 model AST into a [`Value`], dispatching on sort kind.
/// Port of `runtime-smt::z3c::read_ast_value`, producing `core::Value`.
unsafe fn read_ast_value(ctx: Z3_context, ast: Z3_ast) -> Value {
    let sort = Z3_get_sort(ctx, ast);
    if Z3_is_string_sort(ctx, sort) {
        return Value::Str(cstr_to_string(Z3_get_string(ctx, ast)));
    }
    match Z3_get_sort_kind(ctx, sort) {
        SortKind::Int => {
            let mut iv: i64 = 0;
            Z3_get_numeral_int64(ctx, ast, &mut iv);
            Value::Int(iv)
        }
        SortKind::Bool => Value::Bool(Z3_get_bool_value(ctx, ast) == Z3_L_TRUE),
        SortKind::Real => {
            let s = cstr_to_string(Z3_get_numeral_string(ctx, ast));
            Value::Real(parse_rational(&s))
        }
        SortKind::Datatype => read_datatype_value(ctx, ast, sort),
        SortKind::Seq => Value::SeqEnum(gather_seq_elems(ctx, ast)),
        _ => Value::Str(cstr_to_string(Z3_ast_to_string(ctx, ast))),
    }
}

/// Decode a datatype value as `Value::Enum { enum_name, variant, fields }`.
unsafe fn read_datatype_value(ctx: Z3_context, ast: Z3_ast, sort: Z3_sort) -> Value {
    let enum_name = cstr_to_string(Z3_get_symbol_string(ctx, Z3_get_sort_name(ctx, sort)));
    let app = Z3_to_app(ctx, ast);
    let decl = Z3_get_app_decl(ctx, app);
    let variant = cstr_to_string(Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, decl)));
    let n = Z3_get_app_num_args(ctx, app);
    let mut fields = Vec::with_capacity(n as usize);
    for i in 0..n {
        fields.push(read_ast_value(ctx, Z3_get_app_arg(ctx, app, i)));
    }
    Value::Enum { enum_name, variant, fields }
}

/// Walk a Z3 sequence model value (`seq.++` / `seq.unit` / `seq.empty`) into its
/// elements. Port of `runtime-smt::z3c::gather_seq_elems`.
unsafe fn gather_seq_elems(ctx: Z3_context, ast: Z3_ast) -> Vec<Value> {
    unsafe fn go(ctx: Z3_context, ast: Z3_ast, out: &mut Vec<Value>) {
        let app = Z3_to_app(ctx, ast);
        let decl = Z3_get_app_decl(ctx, app);
        let name = cstr_to_string(Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, decl)));
        let n = Z3_get_app_num_args(ctx, app);
        if name == "seq.++" {
            for i in 0..n {
                go(ctx, Z3_get_app_arg(ctx, app, i), out);
            }
        } else if name == "seq.unit" {
            out.push(read_ast_value(ctx, Z3_get_app_arg(ctx, app, 0)));
        } else if name.contains("empty") || n == 0 {
            // empty sequence — no elements
        } else {
            out.push(read_ast_value(ctx, ast));
        }
    }
    let mut out = Vec::new();
    go(ctx, ast, &mut out);
    out
}

fn parse_rational(s: &str) -> f64 {
    match s.split_once('/') {
        None => s.trim().parse().unwrap_or(0.0),
        Some((num, den)) => {
            let n: f64 = num.trim().parse().unwrap_or(0.0);
            let d: f64 = den.trim().parse().unwrap_or(1.0);
            if d != 0.0 {
                n / d
            } else {
                0.0
            }
        }
    }
}

unsafe fn cstr_to_string(p: Z3_string) -> String {
    if p.is_null() {
        return String::new();
    }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}
