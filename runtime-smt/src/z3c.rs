//! N0 — the Z3 floor.
//!
//! A thin, RAII-clean wrapper over the Z3 C API (`z3-sys`). This is the only
//! module that touches raw Z3 pointers; everything above it works in terms of
//! [`Value`] and [`SolveOutcome`].
//!
//! ## Isolation by construction
//!
//! The legacy runtime's test flakiness traced to leaked `Z3_context`s and
//! `thread_local` solver caches that accumulated across queries. We don't
//! reproduce that. A [`Z3Ctx`] owns exactly one config + context and frees both
//! on `Drop`. A [`Solver`] borrows its `Z3Ctx` (lifetime-tied) and frees its
//! own ref on `Drop`. There are **no globals and no `thread_local`s** in this
//! crate — an engine instance is a value you create, use, and drop, and when it
//! drops, every Z3 resource it held is gone. Two engines never share state.

use std::ffi::{CStr, CString};

use z3_sys::*;

/// A decoded Z3 model value. Mirrors the scalar + composite shapes the legacy
/// runtime returns from a query, kept deliberately small.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Real(f64),
    Str(String),
    /// A datatype (enum) value, kept structurally as constructor + decoded
    /// args so it round-trips back into SMT-LIB (needed to thread an enum
    /// state value into the next tick). Displays as `Ctor` / `Ctor(a, b)` —
    /// matches the legacy CLI's enum rendering.
    Enum { ctor: String, args: Vec<Value> },
    /// A sequence value, rendered element-by-element.
    Seq(Vec<Value>),
}

impl Value {
    /// Construct a nullary enum value (`Ctor`).
    pub fn nullary(ctor: impl Into<String>) -> Value {
        Value::Enum { ctor: ctor.into(), args: Vec::new() }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{i}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Real(r) => write!(f, "{r}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Enum { ctor, args } if args.is_empty() => write!(f, "{ctor}"),
            Value::Enum { ctor, args } => {
                write!(f, "{ctor}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
            Value::Seq(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
        }
    }
}

/// Outcome of solving an SMT-LIB problem.
#[derive(Debug, Clone, PartialEq)]
pub enum SolveOutcome {
    /// Satisfiable, with the full model decoded as name → value for every
    /// assigned constant.
    Sat(Model),
    Unsat,
    /// Z3 returned `unknown` (incomplete theory, timeout, etc).
    Unknown,
}

/// A decoded model: every constant Z3 assigned, by name. Bindings are sorted by
/// name for deterministic output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub bindings: Vec<(String, Value)>,
}

impl Model {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.bindings.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

/// Error from the Z3 layer (parse rejection of generated SMT-LIB, etc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Z3Error(pub String);

impl std::fmt::Display for Z3Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "z3: {}", self.0)
    }
}
impl std::error::Error for Z3Error {}

/// An owned Z3 context. Frees the underlying config + context on `Drop`.
pub struct Z3Ctx {
    ctx: Z3_context,
}

impl Z3Ctx {
    /// Create a fresh, model-generating context.
    pub fn new() -> Self {
        unsafe {
            let cfg = Z3_mk_config();
            let model = CString::new("model").unwrap();
            let on = CString::new("true").unwrap();
            Z3_set_param_value(cfg, model.as_ptr(), on.as_ptr());
            let ctx = Z3_mk_context(cfg);
            Z3_del_config(cfg);
            // Install the NULL error handler so a parse/usage error sets the
            // error code instead of invoking Z3's default handler (which can
            // terminate the process). We always check `error()` after fallible
            // calls. Matches what the high-level `z3` crate does.
            Z3_set_error_handler(ctx, None);
            Z3Ctx { ctx }
        }
    }

    /// Raw handle, for the rare call not yet wrapped. Use sparingly.
    pub fn raw(&self) -> Z3_context {
        self.ctx
    }

    /// Last error message, or `None` if the context is in the OK state.
    fn error(&self) -> Option<String> {
        unsafe {
            let code = Z3_get_error_code(self.ctx);
            if code == ErrorCode::OK {
                return None;
            }
            let p = Z3_get_error_msg(self.ctx, code);
            Some(cstr(p))
        }
    }
}

impl Default for Z3Ctx {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Z3Ctx {
    fn drop(&mut self) {
        unsafe { Z3_del_context(self.ctx) }
    }
}

/// A solver bound to a [`Z3Ctx`]. Frees its own ref on `Drop`. Reusable across
/// ticks via [`Solver::reset`] — the engine keeps one solver for the life of a
/// run and resets it each tick rather than rebuilding a Context (cheap, and
/// keeps the no-leak invariant trivially true).
pub struct Solver<'c> {
    ctx: &'c Z3Ctx,
    solver: Z3_solver,
}

impl<'c> Solver<'c> {
    pub fn new(ctx: &'c Z3Ctx) -> Self {
        unsafe {
            let solver = Z3_mk_solver(ctx.ctx);
            Z3_solver_inc_ref(ctx.ctx, solver);
            Solver { ctx, solver }
        }
    }

    /// Drop all asserted formulas (and the backtracking stack). The context's
    /// declarations created by `from_string` persist, so a fresh tick can
    /// re-`from_string` cleanly.
    pub fn reset(&self) {
        unsafe { Z3_solver_reset(self.ctx.ctx, self.solver) }
    }

    /// Parse an SMT-LIB string and add its assertions (and declarations) to this
    /// solver. Surfaces a Z3 parse rejection as an `Err` rather than silently
    /// adding nothing (the high-level `z3` crate swallows this).
    pub fn from_string(&self, smtlib: &str) -> Result<(), Z3Error> {
        let c = CString::new(smtlib)
            .map_err(|_| Z3Error("SMT-LIB text contained an interior NUL".into()))?;
        unsafe { Z3_solver_from_string(self.ctx.ctx, self.solver, c.as_ptr()) };
        if let Some(msg) = self.ctx.error() {
            return Err(Z3Error(format!("rejected SMT-LIB: {msg}\n---\n{smtlib}")));
        }
        Ok(())
    }

    /// Check satisfiability and, if sat, decode the full model.
    pub fn check(&self) -> SolveOutcome {
        unsafe {
            let r = Z3_solver_check(self.ctx.ctx, self.solver);
            if r == Z3_L_TRUE {
                let m = Z3_solver_get_model(self.ctx.ctx, self.solver);
                Z3_model_inc_ref(self.ctx.ctx, m);
                let model = decode_model(self.ctx.ctx, m);
                Z3_model_dec_ref(self.ctx.ctx, m);
                SolveOutcome::Sat(model)
            } else if r == Z3_L_FALSE {
                SolveOutcome::Unsat
            } else {
                SolveOutcome::Unknown
            }
        }
    }

}

impl Drop for Solver<'_> {
    fn drop(&mut self) {
        unsafe { Z3_solver_dec_ref(self.ctx.ctx, self.solver) }
    }
}

/// One-shot convenience: fresh context + solver, parse, solve, decode. The
/// "floor" entry point — roughly raw Z3 with a typed result.
pub fn solve_smtlib(smtlib: &str) -> Result<SolveOutcome, Z3Error> {
    let ctx = Z3Ctx::new();
    let solver = Solver::new(&ctx);
    solver.from_string(smtlib)?;
    Ok(solver.check())
}

// ---------------------------------------------------------------------------
// Model decoding (generic walk over the assigned constants)
// ---------------------------------------------------------------------------

/// Decode every constant the model assigns into a [`Model`]. Generic over sort:
/// datatypes become formatted `Ctor(..)` strings, sequences are walked element
/// by element, scalars are read directly.
fn decode_model(ctx: Z3_context, m: Z3_model) -> Model {
    let mut bindings = Vec::new();
    unsafe {
        let n = Z3_model_get_num_consts(ctx, m);
        for i in 0..n {
            let decl = Z3_model_get_const_decl(ctx, m, i);
            let name = cstr(Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, decl)));
            let interp = Z3_model_get_const_interp(ctx, m, decl);
            if interp.is_null() {
                continue;
            }
            bindings.push((name, read_ast_value(ctx, interp)));
        }
    }
    bindings.sort_by(|a, b| a.0.cmp(&b.0));
    Model { bindings }
}

/// Recursively decode a Z3 model value AST into a [`Value`], dispatching on sort
/// kind. A direct port of `runtime-c/src/solve.cpp::read_ast_value`.
pub(crate) fn read_ast_value(ctx: Z3_context, ast: Z3_ast) -> Value {
    unsafe {
        let sort = Z3_get_sort(ctx, ast);
        if Z3_is_string_sort(ctx, sort) {
            let p = Z3_get_string(ctx, ast);
            return Value::Str(cstr(p));
        }
        match Z3_get_sort_kind(ctx, sort) {
            SortKind::Int => {
                let mut iv: i64 = 0;
                Z3_get_numeral_int64(ctx, ast, &mut iv);
                Value::Int(iv)
            }
            SortKind::Bool => Value::Bool(Z3_get_bool_value(ctx, ast) == Z3_L_TRUE),
            SortKind::Real => {
                let p = Z3_get_numeral_string(ctx, ast);
                Value::Real(parse_rational(&cstr(p)))
            }
            SortKind::Datatype => read_datatype_value(ctx, ast),
            SortKind::Seq => Value::Seq(gather_seq_elems(ctx, ast)),
            _ => {
                // Fallback: stringify whatever it is.
                let p = Z3_ast_to_string(ctx, ast);
                Value::Str(cstr(p))
            }
        }
    }
}

/// Decode a datatype value as `Ctor` or `Ctor(arg, ...)`.
unsafe fn read_datatype_value(ctx: Z3_context, ast: Z3_ast) -> Value {
    let app = Z3_to_app(ctx, ast);
    let decl = Z3_get_app_decl(ctx, app);
    let name = cstr(Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, decl)));
    let n = Z3_get_app_num_args(ctx, app);
    let mut args = Vec::with_capacity(n as usize);
    for i in 0..n {
        args.push(read_ast_value(ctx, Z3_get_app_arg(ctx, app, i)));
    }
    Value::Enum { ctor: name, args }
}

/// Walk a Z3 sequence model value (`seq.++` / `seq.unit` / `seq.empty`) into its
/// elements. Port of `runtime-c/src/solve.cpp::gather_seq_elems`. Homebrew's
/// `z3.h` exposes no seq-element C API, so we walk by app-decl name.
unsafe fn gather_seq_elems(ctx: Z3_context, ast: Z3_ast) -> Vec<Value> {
    fn go(ctx: Z3_context, ast: Z3_ast, out: &mut Vec<Value>) {
        unsafe {
            let app = Z3_to_app(ctx, ast);
            let decl = Z3_get_app_decl(ctx, app);
            let name = cstr(Z3_get_symbol_string(ctx, Z3_get_decl_name(ctx, decl)));
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
    }
    let mut out = Vec::new();
    go(ctx, ast, &mut out);
    out
}

/// Parse a Z3 numeral string ("3", "-2", "3/2") into an f64.
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

/// Convert a Z3 C string (`*const c_char`) to an owned `String`. `null` → "".
pub(crate) fn cstr(p: Z3_string) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_solves_hardcoded_smtlib() {
        let smt = "(declare-const n Int)\n(assert (> n 5))\n(assert (< n 8))";
        match solve_smtlib(smt).unwrap() {
            SolveOutcome::Sat(m) => match m.get("n") {
                Some(Value::Int(v)) => assert!(*v > 5 && *v < 8, "got n = {v}"),
                other => panic!("expected Int, got {other:?}"),
            },
            other => panic!("expected Sat, got {other:?}"),
        }
    }

    #[test]
    fn floor_reports_unsat() {
        let smt = "(declare-const b Bool)\n(assert b)\n(assert (not b))";
        assert_eq!(solve_smtlib(smt).unwrap(), SolveOutcome::Unsat);
    }

    #[test]
    fn floor_rejects_malformed_smtlib() {
        assert!(solve_smtlib("(declare-const n Intt)").is_err());
    }

    #[test]
    fn floor_decodes_datatype_value() {
        // A tiny enum: pick the Green ctor.
        let smt = "(declare-datatypes ((C 0)) (((Red) (Green) (Blue))))\n\
                   (declare-const c C)\n(assert (= c Green))";
        match solve_smtlib(smt).unwrap() {
            SolveOutcome::Sat(m) => assert_eq!(m.get("c"), Some(&Value::nullary("Green"))),
            other => panic!("expected Sat, got {other:?}"),
        }
    }

    #[test]
    fn two_contexts_are_independent() {
        // Drop one engine's context entirely; a second solves fine. No shared
        // state, no leak across instances.
        {
            let _ = solve_smtlib("(declare-const a Int)(assert (= a 1))").unwrap();
        }
        let r = solve_smtlib("(declare-const a Int)(assert (= a 2))").unwrap();
        match r {
            SolveOutcome::Sat(m) => assert_eq!(m.get("a"), Some(&Value::Int(2))),
            other => panic!("{other:?}"),
        }
    }
}
