//! Evident claim → SMT-LIB text → Z3 (parse) → solve.
//!
//! This is the first concrete slice of the north star in
//! `docs/design/smtlib-as-compile-target.md`: instead of building the Z3 AST
//! through the C API (what the rest of `translate/` does), this module emits
//! **SMT-LIB text** for a quantifier-free scalar/string subset, hands it to Z3's
//! own parser (`Z3_solver_from_string`), and solves. The cross-check proof lives
//! in `runtime/tests/smtlib_roundtrip.rs` + `smtlib_snapshots.rs`: for a corpus
//! of claims (including the real `examples/test_39_string_ops.ev`) it asserts
//! that this path produces the SAME sat/unsat (and, where the model is forced,
//! the same model) as the default `EvidentRuntime::query` path.
//!
//! It is **additive and gated**: nothing on the default translate/query path
//! calls into here. The callers are the cross-check tests, the
//! `evident dump-smtlib` CLI ([`schema_to_smtlib_artifact`], which writes a
//! runnable `.smt2` file), and anyone who opts in via [`is_enabled`]
//! (`EVIDENT_SMTLIB=1`).
//!
//! Subset that transpiles today (see the findings doc for the full table):
//!   * scalar sorts `Int` / `Nat` / `Pos` / `Bool` / `Real` / `String`
//!   * arithmetic `+ - * /`, comparisons `= ≠ < ≤ > ≥`, `∧ ∨ ¬ ⇒`
//!   * set/range membership as a constraint (`x ∈ {1,2,3}`, `x ∈ {lo..hi}`)
//!   * ternary `(c ? a : b)` → `ite`
//!   * string builtins → Z3 `str.*`: `substr`, `index_of`, `replace`, `char_at`,
//!     `str_from_int`, `str_len`/`#s`, `starts_with`, `ends_with`,
//!     `str_contains`, infix `sub ∈ text`, `++` concat
//!   * pre-bound `given` values (CLI `--given k=v`) → `(assert (= k <lit>))`
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
/// claim. Every declared scalar is a fresh const, every constraint is asserted,
/// and each `given` (CLI `--given k=v`, mirroring `EvidentRuntime::query`'s
/// pre-bound values) becomes an `(assert (= k <lit>))`. Returns the declared
/// `(name, sort)` list alongside.
fn emit(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
) -> Result<(String, Vec<(String, Sort)>), SmtLibError> {
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

    // Pass 3: pinned givens → equality assertions. Sorted for deterministic output
    // (snapshot stability). A given naming a non-scalar / undeclared var is an
    // error, never silently dropped.
    let mut keys: Vec<&String> = given.keys().collect();
    keys.sort();
    for k in keys {
        let sort = env.get(k).copied().ok_or_else(|| {
            SmtLibError(format!("given `{k}` is not a declared scalar in `{}`", schema.name))
        })?;
        let lit = value_to_smt(&given[k], sort)?;
        let _ = writeln!(out, "(assert (= {k} {lit}))");
    }

    Ok((out, declared))
}

/// Convert a pre-bound `Value` to an SMT-LIB literal for its declared scalar sort.
/// An `Int` given for a `Real` const is promoted; any other mismatch is an error.
fn value_to_smt(v: &Value, sort: Sort) -> Result<String, SmtLibError> {
    match (v, sort) {
        (Value::Int(i), Sort::Int) => Ok(int_lit(*i)),
        (Value::Bool(b), Sort::Bool) => Ok(if *b { "true".into() } else { "false".into() }),
        (Value::Real(r), Sort::Real) => Ok(real_lit(*r)),
        (Value::Int(i), Sort::Real) => Ok(real_lit(*i as f64)),
        (Value::Str(s), Sort::Str) => Ok(str_lit(s)),
        _ => err(format!("given value {v:?} doesn't fit declared sort `{}`", sort.smt())),
    }
}

/// Public entry: just the SMT-LIB body text (declarations + asserts, no
/// `check-sat`), free-query semantics (no pinned givens).
pub fn schema_to_smtlib(schema: &SchemaDecl) -> Result<String, SmtLibError> {
    emit(schema, &HashMap::new()).map(|(text, _)| text)
}

/// Like [`schema_to_smtlib`] but with pre-bound `given` values asserted as
/// equalities (mirrors `EvidentRuntime::query`'s pinned inputs).
pub fn schema_to_smtlib_with_given(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
) -> Result<String, SmtLibError> {
    emit(schema, given).map(|(text, _)| text)
}

/// A self-contained, runnable `.smt2` artifact: a header comment, the
/// declarations + asserts, then `(check-sat)` and `(get-model)`. This is what
/// `evident dump-smtlib` writes to disk and what the snapshot test pins. The
/// solve path ([`solve`]) consumes the body form only (it calls `check` itself),
/// so the artifact's extra directives never affect a programmatic solve.
///
/// `get-model` errors harmlessly under `z3` when the claim is UNSAT — that is
/// faithful (an `unsat_*` fixture has no model).
pub fn schema_to_smtlib_artifact(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
) -> Result<String, SmtLibError> {
    let (body, _) = emit(schema, given)?;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "; SMT-LIB for claim `{}` — generated by `evident dump-smtlib`.",
        schema.name
    );
    let _ = writeln!(
        out,
        "; Quantifier-free scalar/string subset; see docs/perf/smtlib-prototype-findings.md."
    );
    out.push_str(&body);
    out.push_str("(check-sat)\n(get-model)\n");
    Ok(out)
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
        Expr::Call(name, args) => str_call_sort(name, args.len()),
        // `#text` on a String is `str.len` (Int). `#` on a Seq/Set is out of subset.
        Expr::Cardinality(inner) => match sort_of(inner, env) {
            Some(Sort::Str) => Some(Sort::Int),
            _ => None,
        },
        _ => None,
    }
}

/// Result sort of a supported string builtin call, or `None` if the name/arity is
/// out of subset. Mirrors `translate/exprs/string_ops.rs`.
fn str_call_sort(name: &str, arity: usize) -> Option<Sort> {
    match (name, arity) {
        ("substr", 3) | ("replace", 3) | ("char_at", 2) | ("str_from_int", 1) => Some(Sort::Str),
        ("str_len", 1) | ("index_of", 2) | ("index_of", 3) => Some(Sort::Int),
        ("str_contains", 2) | ("starts_with", 2) | ("ends_with", 2) => Some(Sort::Bool),
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

        // `lhs ∈ {a, b, c}` → (or (= lhs a) ...); `lhs ∈ {lo..hi}` → range bound;
        // `sub ∈ text` (String RHS) → (str.contains text sub).
        Expr::InExpr(lhs, rhs) => in_expr(lhs, rhs, env),

        // String builtins → Z3 `str.*` (see `str_call`).
        Expr::Call(name, args) => str_call(name, args, env),
        // `#text` on a String → `(str.len text)`.
        Expr::Cardinality(inner) => match sort_of(inner, env) {
            Some(Sort::Str) => Ok(format!("(str.len {})", expr(inner, env)?)),
            _ => err("cardinality `#` only supported on String in subset"),
        },

        // Everything below is out of the prototype subset.
        Expr::SetLit(_) => err("set literal (not as ∈ RHS) unsupported"),
        Expr::SeqLit(_) => err("sequence literal unsupported"),
        Expr::Range(..) => err("bare range unsupported (only as ∈ RHS)"),
        Expr::Tuple(_) => err("tuple unsupported"),
        Expr::Forall(..) => err("∀ quantifier unsupported (quantifier-free subset)"),
        Expr::Exists(..) => err("∃ quantifier unsupported (quantifier-free subset)"),
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

/// String builtins → Z3 `str.*` s-expressions. Lowering matches
/// `translate/exprs/string_ops.rs` exactly (arg order, sign reattachment) so the
/// SMT-LIB path and the C-API path produce the same models.
fn str_call(name: &str, args: &[Expr], env: &Env) -> Result<String, SmtLibError> {
    let a = |i: usize| expr(&args[i], env);
    match (name, args.len()) {
        // String-producing.
        ("substr", 3) => Ok(format!("(str.substr {} {} {})", a(0)?, a(1)?, a(2)?)),
        ("replace", 3) => Ok(format!("(str.replace {} {} {})", a(0)?, a(1)?, a(2)?)),
        ("char_at", 2) => Ok(format!("(str.at {} {})", a(0)?, a(1)?)),
        ("str_from_int", 1) => {
            // `Z3_mk_int_to_str` only handles naturals (negatives → ""); the C-API
            // reattaches the sign as `n<0 ? "-" ++ from_int(-n) : from_int(n)`.
            let n = a(0)?;
            Ok(format!(
                "(ite (>= {n} 0) (str.from_int {n}) (str.++ \"-\" (str.from_int (- 0 {n}))))"
            ))
        }
        // Int-producing.
        ("str_len", 1) => Ok(format!("(str.len {})", a(0)?)),
        ("index_of", 2) => Ok(format!("(str.indexof {} {} 0)", a(0)?, a(1)?)),
        ("index_of", 3) => Ok(format!("(str.indexof {} {} {})", a(0)?, a(1)?, a(2)?)),
        // Bool-producing. `starts_with(text, pre)` ↦ `(str.prefixof pre text)`.
        ("str_contains", 2) => Ok(format!("(str.contains {} {})", a(0)?, a(1)?)),
        ("starts_with", 2) => Ok(format!("(str.prefixof {} {})", a(1)?, a(0)?)),
        ("ends_with", 2) => Ok(format!("(str.suffixof {} {})", a(1)?, a(0)?)),
        _ => err(format!("call `{name}/{}` unsupported (not a known string builtin)", args.len())),
    }
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
        // `sub ∈ text` where `text` is a String → substring containment.
        _ if sort_of(rhs, env) == Some(Sort::Str) => {
            let r = expr(rhs, env)?;
            Ok(format!("(str.contains {r} {l})"))
        }
        _ => err("∈ RHS must be a set literal, range, or String (str.contains) in the prototype subset"),
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
    solve_with_given(schema, &HashMap::new())
}

/// Like [`solve`] but with pre-bound `given` values (mirrors
/// `EvidentRuntime::query(name, given)`).
pub fn solve_with_given(
    schema: &SchemaDecl,
    given: &HashMap<String, Value>,
) -> Result<SmtSolveResult, SmtLibError> {
    let (text, declared) = emit(schema, given)?;

    let mut cfg = Config::new();
    cfg.set_model_generation(true);
    let ctx = Context::new(&cfg);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ast::{Keyword, SchemaDecl};

    fn scalar_schema() -> SchemaDecl {
        // n ∈ Nat ; n > 5
        SchemaDecl {
            keyword: Keyword::Claim,
            name: "T".into(),
            type_params: vec![],
            param_count: 0,
            external: false,
            body: vec![
                BodyItem::Membership { name: "n".into(), type_name: "Nat".into(), pins: Pins::None },
                BodyItem::Constraint(Expr::Binary(
                    BinOp::Gt,
                    Box::new(Expr::Identifier("n".into())),
                    Box::new(Expr::Int(5)),
                )),
            ],
        }
    }

    #[test]
    fn emits_declare_and_bound() {
        let text = schema_to_smtlib(&scalar_schema()).unwrap();
        assert!(text.contains("(declare-const n Int)"), "got:\n{text}");
        assert!(text.contains("(assert (>= n 0))"), "Nat bound missing:\n{text}");
        assert!(text.contains("(assert (> n 5))"), "got:\n{text}");
    }

    #[test]
    fn solves_sat_with_model() {
        let r = solve(&scalar_schema()).unwrap();
        assert!(r.satisfied);
        match r.bindings.get("n") {
            Some(Value::Int(v)) => assert!(*v > 5),
            other => panic!("expected Int > 5, got {other:?}"),
        }
    }

    #[test]
    fn negative_int_literal() {
        assert_eq!(int_lit(-5), "(- 5)");
        assert_eq!(int_lit(0), "0");
        assert_eq!(int_lit(7), "7");
    }

    #[test]
    fn unsupported_type_is_reported() {
        let s = SchemaDecl {
            keyword: Keyword::Claim,
            name: "T".into(),
            type_params: vec![],
            param_count: 0,
            external: false,
            body: vec![BodyItem::Membership {
                name: "xs".into(),
                type_name: "Seq(Int)".into(),
                pins: Pins::None,
            }],
        };
        assert!(schema_to_smtlib(&s).is_err());
    }
}
