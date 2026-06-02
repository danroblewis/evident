//! PROTOTYPE — Evident claim → SMT-LIB text → Z3 (parse) → solve.
//!
//! This is the first concrete slice of the north star in
//! `docs/design/smtlib-as-compile-target.md`: instead of building the Z3 AST
//! through the C API (what the rest of `translate/` does), this module emits
//! **SMT-LIB text** for a quantifier-free scalar subset, hands it to Z3's own
//! parser (`Z3_solver_from_string`), and solves. The round-trip proof lives in
//! `runtime/tests/smtlib_roundtrip.rs`: for a corpus of simple claims it asserts
//! that this path produces the SAME sat/unsat (and, where the model is forced,
//! the same model) as the default `EvidentRuntime::query` path.
//!
//! It is **additive and gated**: nothing on the default translate/query path
//! calls into here. The only callers are the round-trip test (the "dedicated
//! test entry") and anyone who opts in via [`is_enabled`] (`EVIDENT_SMTLIB=1`).
//!
//! Subset that transpiles today (see the findings doc for the full table):
//!   * scalar sorts `Int` / `Nat` / `Pos` / `Bool` / `Real` / `String`
//!   * arithmetic `+ - * /`, comparisons `= ≠ < ≤ > ≥`, `∧ ∨ ¬ ⇒`
//!   * set/range membership as a constraint (`x ∈ {1,2,3}`, `x ∈ {lo..hi}`)
//!   * ternary `(c ? a : b)` → `ite`
//! Anything else (Seq/Set values, enums, records, quantifiers, match, FSM
//! machinery, claim composition) returns [`SmtLibError`] — the boundary is
//! reported honestly, never silently mistranslated.

use std::collections::HashMap;
use std::fmt::Write as _;

use z3::{Config, Context, SatResult, Solver};

use crate::core::ast::{BinOp, BodyItem, Expr, Pins, SchemaDecl};
use crate::core::Value;

/// Out-of-subset / malformed-input signal. The prototype reports exactly what it
/// could not transpile rather than emitting wrong SMT-LIB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtLibError(pub String);

impl std::fmt::Display for SmtLibError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "smtlib: {}", self.0)
    }
}
impl std::error::Error for SmtLibError {}

fn err<T>(msg: impl Into<String>) -> Result<T, SmtLibError> {
    Err(SmtLibError(msg.into()))
}

/// The scalar SMT sorts the prototype handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sort {
    Int,
    Bool,
    Real,
    Str,
}

impl Sort {
    /// SMT-LIB sort name.
    fn smt(self) -> &'static str {
        match self {
            Sort::Int => "Int",
            Sort::Bool => "Bool",
            Sort::Real => "Real",
            Sort::Str => "String",
        }
    }
}

/// Map an Evident scalar type name to an SMT sort. `Nat`/`Pos` are `Int` with an
/// extra non-negativity / positivity bound emitted by the caller.
fn scalar_sort(type_name: &str) -> Option<Sort> {
    match type_name {
        "Int" | "Nat" | "Pos" => Some(Sort::Int),
        "Bool" => Some(Sort::Bool),
        "Real" => Some(Sort::Real),
        "String" => Some(Sort::Str),
        _ => None,
    }
}

/// Variable name → sort, built from the schema's `Membership` body items.
type Env = HashMap<String, Sort>;

/// Result of running a claim through the SMT-LIB path.
#[derive(Debug, Clone)]
pub struct SmtSolveResult {
    pub satisfied: bool,
    /// Extracted model for declared scalar consts (only populated when satisfied).
    pub bindings: HashMap<String, Value>,
    /// The SMT-LIB text that was handed to Z3 — useful for tests / debugging.
    pub smtlib: String,
}

/// Opt-in flag for anyone wiring the prototype into a non-test path. The default
/// translate/query path never consults this; it exists so the env var named in
/// the design doc has a single source of truth.
pub fn is_enabled() -> bool {
    std::env::var("EVIDENT_SMTLIB").map(|s| s != "0" && !s.is_empty()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Emit: SchemaDecl → SMT-LIB text
// ---------------------------------------------------------------------------

/// Emit SMT-LIB text (`declare-const` + `assert` lines, no `check-sat`) for a
/// claim's free-query semantics: every declared scalar is a fresh const, every
/// constraint is asserted. Returns the declared `(name, sort)` list alongside.
fn emit(schema: &SchemaDecl) -> Result<(String, Vec<(String, Sort)>), SmtLibError> {
    let mut env: Env = HashMap::new();
    let mut declared: Vec<(String, Sort)> = Vec::new();
    let mut out = String::new();

    // Pass 1: declarations (so forward references in constraints resolve).
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, pins } = item {
            let sort = scalar_sort(type_name)
                .ok_or_else(|| SmtLibError(format!("unsupported type `{type_name}` for `{name}`")))?;
            if env.contains_key(name) {
                continue; // re-declaration; first wins (matches declare.rs idempotence)
            }
            env.insert(name.clone(), sort);
            declared.push((name.clone(), sort));
            let _ = writeln!(out, "(declare-const {name} {})", sort.smt());
            // Nat/Pos lower bound.
            match type_name.as_str() {
                "Nat" => { let _ = writeln!(out, "(assert (>= {name} 0))"); }
                "Pos" => { let _ = writeln!(out, "(assert (> {name} 0))"); }
                _ => {}
            }
            if !matches!(pins, Pins::None) {
                return err(format!("pins on scalar `{name}` not supported in prototype"));
            }
        }
    }

    // Pass 2: constraints.
    for item in &schema.body {
        match item {
            BodyItem::Membership { .. } => {} // handled above
            BodyItem::Constraint(e) => {
                let s = expr(e, &env)?;
                let _ = writeln!(out, "(assert {s})");
            }
            BodyItem::Passthrough(n) => return err(format!("passthrough `..{n}` not supported")),
            BodyItem::SubclaimDecl(_) => return err("subclaim not supported"),
            BodyItem::ClaimCall { name, .. } => return err(format!("claim call `{name}` not supported")),
            BodyItem::HaltsWithin { .. } => return err("halts_within not supported"),
        }
    }

    Ok((out, declared))
}

/// Public entry: just the SMT-LIB text (for inspection / tests).
pub fn schema_to_smtlib(schema: &SchemaDecl) -> Result<String, SmtLibError> {
    emit(schema).map(|(text, _)| text)
}

// ---------------------------------------------------------------------------
// Expr → SMT-LIB s-expression
// ---------------------------------------------------------------------------

/// Best-effort sort inference for an expression, used to pick `div` vs `/` and
/// to validate branch sorts. `None` when the prototype can't determine it.
fn sort_of(e: &Expr, env: &Env) -> Option<Sort> {
    match e {
        Expr::Int(_) => Some(Sort::Int),
        Expr::Real(_) => Some(Sort::Real),
        Expr::Bool(_) => Some(Sort::Bool),
        Expr::Str(_) => Some(Sort::Str),
        Expr::Identifier(n) => env.get(n).copied(),
        Expr::Not(_) => Some(Sort::Bool),
        Expr::Binary(op, a, b) => match op {
            BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            | BinOp::And | BinOp::Or | BinOp::Implies => Some(Sort::Bool),
            BinOp::Concat => Some(Sort::Str),
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                // Arithmetic sort = Real if either operand is Real, else Int.
                match (sort_of(a, env), sort_of(b, env)) {
                    (Some(Sort::Real), _) | (_, Some(Sort::Real)) => Some(Sort::Real),
                    (Some(Sort::Int), _) | (_, Some(Sort::Int)) => Some(Sort::Int),
                    _ => None,
                }
            }
        },
        Expr::Ternary(_, t, f) => sort_of(t, env).or_else(|| sort_of(f, env)),
        Expr::InExpr(..) => Some(Sort::Bool),
        _ => None,
    }
}

/// Translate one expression to an SMT-LIB s-expression string.
fn expr(e: &Expr, env: &Env) -> Result<String, SmtLibError> {
    match e {
        Expr::Identifier(n) => {
            if env.contains_key(n) {
                Ok(n.clone())
            } else {
                // An undeclared bare identifier is almost always a record field /
                // enum ctor / out-of-subset name. Report rather than emit garbage.
                err(format!("undeclared identifier `{n}` (out of scalar subset)"))
            }
        }
        Expr::Int(i) => Ok(int_lit(*i)),
        Expr::Real(r) => Ok(real_lit(*r)),
        Expr::Bool(b) => Ok(if *b { "true".into() } else { "false".into() }),
        Expr::Str(s) => Ok(str_lit(s)),

        Expr::Not(inner) => Ok(format!("(not {})", expr(inner, env)?)),

        Expr::Binary(op, a, b) => binary(op, a, b, env),

        Expr::Ternary(c, t, f) => {
            let cs = expr(c, env)?;
            let ts = expr(t, env)?;
            let fs = expr(f, env)?;
            Ok(format!("(ite {cs} {ts} {fs})"))
        }

        // `lhs ∈ {a, b, c}` → (or (= lhs a) ...); `lhs ∈ {lo..hi}` → range bound.
        Expr::InExpr(lhs, rhs) => in_expr(lhs, rhs, env),

        // Everything below is out of the prototype subset.
        Expr::SetLit(_) => err("set literal (not as ∈ RHS) unsupported"),
        Expr::SeqLit(_) => err("sequence literal unsupported"),
        Expr::Range(..) => err("bare range unsupported (only as ∈ RHS)"),
        Expr::Tuple(_) => err("tuple unsupported"),
        Expr::Forall(..) => err("∀ quantifier unsupported (quantifier-free subset)"),
        Expr::Exists(..) => err("∃ quantifier unsupported (quantifier-free subset)"),
        Expr::Call(n, _) => err(format!("call `{n}` unsupported")),
        Expr::Cardinality(_) => err("cardinality `#` unsupported"),
        Expr::Index(..) => err("indexing `[]` unsupported"),
        Expr::Field(..) => err("field access unsupported (records out of subset)"),
        Expr::Match(..) => err("match unsupported"),
        Expr::Matches(..) => err("matches recognizer unsupported"),
        Expr::RunFsm { .. } => err("run(fsm) unsupported"),
    }
}

fn binary(op: &BinOp, a: &Expr, b: &Expr, env: &Env) -> Result<String, SmtLibError> {
    // `≠` has no SMT primitive; lower to (not (= ..)).
    if *op == BinOp::Neq {
        let (x, y) = (expr(a, env)?, expr(b, env)?);
        return Ok(format!("(not (= {x} {y}))"));
    }
    let sym = match op {
        BinOp::Eq => "=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::Implies => "=>",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        // `++` string concatenation → Z3 seq concat.
        BinOp::Concat => "str.++",
        BinOp::Div => {
            // Int division is `div`; Real division is `/`. Infer from operand sorts.
            match sort_of(&Expr::Binary(BinOp::Add, Box::new(a.clone()), Box::new(b.clone())), env) {
                Some(Sort::Real) => "/",
                _ => "div",
            }
        }
        BinOp::Neq => unreachable!("handled above"),
    };
    let (x, y) = (expr(a, env)?, expr(b, env)?);
    Ok(format!("({sym} {x} {y})"))
}

fn in_expr(lhs: &Expr, rhs: &Expr, env: &Env) -> Result<String, SmtLibError> {
    let l = expr(lhs, env)?;
    match rhs {
        Expr::Range(lo, hi) => {
            let lo = expr(lo, env)?;
            let hi = expr(hi, env)?;
            Ok(format!("(and (>= {l} {lo}) (<= {l} {hi}))"))
        }
        Expr::SetLit(elems) => {
            if elems.is_empty() {
                return Ok("false".into());
            }
            let mut parts = Vec::with_capacity(elems.len());
            for el in elems {
                parts.push(format!("(= {l} {})", expr(el, env)?));
            }
            if parts.len() == 1 {
                Ok(parts.pop().unwrap())
            } else {
                Ok(format!("(or {})", parts.join(" ")))
            }
        }
        _ => err("∈ RHS must be a set literal or range in the prototype subset"),
    }
}

/// SMT-LIB int literal — negatives wrap in `(- n)`.
fn int_lit(i: i64) -> String {
    if i < 0 {
        // i64::MIN guard: format the magnitude as u64 to avoid overflow on negate.
        format!("(- {})", (i as i128).unsigned_abs())
    } else {
        i.to_string()
    }
}

/// SMT-LIB real literal — must carry a decimal point; negatives wrap in `(- r)`.
fn real_lit(r: f64) -> String {
    let mag = r.abs();
    let mut s = format!("{mag}");
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        s.push_str(".0");
    }
    if r.is_sign_negative() && r != 0.0 {
        format!("(- {s})")
    } else {
        s
    }
}

/// SMT-LIB string literal: double-quoted, internal `"` doubled. Non-ASCII left
/// as-is (Z3 accepts UTF-8 in string literals); the prototype corpus is ASCII.
fn str_lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Solve: SMT-LIB text → Z3 → sat/unsat (+ model)
// ---------------------------------------------------------------------------

// `z3::Context` is a single-field newtype around `Z3_context`; we reach the raw
// pointer to query Z3's error state after parsing. The assert guards the layout.
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

/// Run a claim through the full prototype path: emit SMT-LIB, parse it with Z3,
/// solve, and (when sat) extract the scalar model. Errors if the claim is out of
/// the transpilable subset, or if Z3 rejects the generated SMT-LIB.
pub fn solve(schema: &SchemaDecl) -> Result<SmtSolveResult, SmtLibError> {
    let (text, declared) = emit(schema)?;

    let mut cfg = Config::new();
    cfg.set_model_generation(true);
    // Serialized through the global setup lock (see crate::z3_ctx) so concurrent
    // creation never races Z3's global init.
    let ctx = { let _g = crate::z3_ctx::setup_guard(); Context::new(&cfg) };
    let solver = Solver::new(&ctx);

    solver.from_string(text.clone());

    // Detect a parser rejection — the z3 crate wrapper swallows it, so check the
    // raw error code. A malformed emit would otherwise silently add no assertions.
    let code = unsafe { z3_sys::Z3_get_error_code(raw_ctx(&ctx)) };
    if code != z3_sys::ErrorCode::OK {
        let msg = unsafe {
            let p = z3_sys::Z3_get_error_msg(raw_ctx(&ctx), code);
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        };
        return err(format!("Z3 rejected generated SMT-LIB ({code:?}): {msg}\n{text}"));
    }

    let satisfied = match solver.check() {
        SatResult::Sat => true,
        SatResult::Unsat => false,
        SatResult::Unknown => {
            return err("Z3 returned Unknown on a quantifier-free scalar problem");
        }
    };

    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, sort) in &declared {
                if let Some(v) = read_const(&ctx, &model, name, *sort) {
                    bindings.insert(name.clone(), v);
                }
            }
        }
    }

    Ok(SmtSolveResult { satisfied, bindings, smtlib: text })
}

/// Read one declared const out of the model by reconstructing its handle (Z3
/// consts are identified by name + sort, so a fresh `*::new_const` resolves to
/// the symbol the parser created).
fn read_const(ctx: &Context, model: &z3::Model, name: &str, sort: Sort) -> Option<Value> {
    use z3::ast::{Bool, Int, Real, String as Z3Str};
    match sort {
        Sort::Int => {
            let c = Int::new_const(ctx, name);
            model.eval(&c, true)?.as_i64().map(Value::Int)
        }
        Sort::Bool => {
            let c = Bool::new_const(ctx, name);
            model.eval(&c, true)?.as_bool().map(Value::Bool)
        }
        Sort::Real => {
            let c = Real::new_const(ctx, name);
            let (num, den) = model.eval(&c, true)?.as_real()?;
            Some(Value::Real(num as f64 / den as f64))
        }
        Sort::Str => {
            let c = Z3Str::new_const(ctx, name);
            model.eval(&c, true)?.as_string().map(Value::Str)
        }
    }
}
