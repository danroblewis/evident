//! `validate` — second port driven by the [`super`] swap interface.
//!
//! Load-time external-only check (`enforce_external_only`): reject
//! non-`external` schemas that construct FFI effects (`FFICall`,
//! `FFIOpen`, `FFILookup`, `LibCall`). The canonical Rust impl lives at
//! `runtime/src/runtime/validate.rs` (called from the load path in
//! `runtime/src/runtime/load.rs`); this module exposes the same rule
//! through the [`ValidateImpl`] trait with two interchangeable backings:
//!
//!   * [`RustValidate`] — a native re-implementation, kept here so the
//!     portable seam doesn't have to widen `runtime::validate`'s
//!     `pub(super)` surface. Mirrors `find_ffi_call` in the canonical
//!     module line-for-line.
//!   * [`EvidentValidate`] — owns an [`EvidentRuntime`] with
//!     `stdlib/passes/validate.ev` loaded. Walks the schema body in
//!     Rust and queries `ValidateExpr` at each Call node for the
//!     banned-name decision. Structural recursion lives on the Rust
//!     side (Evident can't recurse over Expr trees yet — see
//!     `docs/self-hosting.md`); the decision logic — "what counts as a
//!     banned call" — lives in Evident.
//!
//! Because the two impls share the walker and differ only in the
//! per-Call predicate, `EvidentValidate` is fully faithful to
//! `RustValidate` (byte-identical diagnostics on every input). That's
//! what the equivalence test pins.

use std::collections::HashMap;
use std::path::Path;

use crate::core::ast::{BinOp, BodyItem, Expr, Keyword, MatchArm, MatchPattern, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `enforce_external_only`'s Rust-level signature, independent of which
/// impl backs it. Returns `Ok(())` when the schema passes the check and
/// `Err(msg)` with a human-readable diagnostic otherwise. Mirrors the
/// canonical `runtime::validate::enforce_external_only` minus the
/// `RuntimeError` wrapper — the seam returns `String` so the equivalence
/// test compares textually.
pub trait ValidateImpl: Portable {
    fn enforce_external_only(&self, s: &SchemaDecl) -> Result<(), String>;
}

/// `kind` label used in the diagnostic message — must match the
/// canonical impl in `runtime/src/runtime/validate.rs` exactly.
fn keyword_label(kw: &Keyword) -> &'static str {
    match kw {
        Keyword::Fsm      => "fsm",
        Keyword::Type     => "type",
        Keyword::Claim    => "claim",
        Keyword::Schema   => "schema",
        Keyword::Subclaim => "subclaim",
    }
}

/// Format the diagnostic. The exact wording matches
/// `runtime/src/runtime/validate.rs` so both impls' error strings are
/// byte-identical.
fn error_msg(kind: &str, name: &str, call: &str) -> String {
    format!(
        "{kind} `{name}` constructs `{call}(...)` but isn't \
         declared `external`. Either mark this declaration \
         `external claim` / `external type`, or move the \
         FFI into an `external claim` helper and call it \
         from here."
    )
}

// ─────────────────────────────────────────────────────────────────────
// Shared walk — used by both impls, parameterised by the per-Call
// classifier. Mirrors the recursion in `runtime/src/runtime/validate.rs`
// `find_ffi_call` exactly. Keeping the walker shared guarantees the two
// impls only differ on the decision predicate, which is what makes the
// port byte-identical-faithful.
// ─────────────────────────────────────────────────────────────────────

fn find_ffi_call(e: &Expr, classify: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    match e {
        Expr::Call(name, args) => {
            if let Some(b) = classify(name) { return Some(b); }
            args.iter().find_map(|a| find_ffi_call(a, classify))
        }
        Expr::Binary(_, l, r) =>
            find_ffi_call(l, classify).or_else(|| find_ffi_call(r, classify)),
        Expr::Not(i) | Expr::Cardinality(i) => find_ffi_call(i, classify),
        Expr::Ternary(c, a, b) =>
            find_ffi_call(c, classify)
                .or_else(|| find_ffi_call(a, classify))
                .or_else(|| find_ffi_call(b, classify)),
        Expr::Index(s, i) | Expr::Range(s, i) | Expr::InExpr(s, i) =>
            find_ffi_call(s, classify).or_else(|| find_ffi_call(i, classify)),
        Expr::Field(b, _) => find_ffi_call(b, classify),
        Expr::Matches(e, _) => find_ffi_call(e, classify),
        Expr::SeqLit(items) | Expr::SetLit(items) =>
            items.iter().find_map(|a| find_ffi_call(a, classify)),
        Expr::Match(scr, arms) =>
            find_ffi_call(scr, classify).or_else(|| arms.iter()
                .find_map(|a| find_ffi_call(&a.body, classify))),
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
            find_ffi_call(r, classify).or_else(|| find_ffi_call(b, classify)),
        _ => None,
    }
}

fn enforce_with<F: Fn(&str) -> Option<String>>(
    s: &SchemaDecl,
    classify: F,
) -> Result<(), String> {
    if s.external { return Ok(()); }
    for item in &s.body {
        if let BodyItem::Constraint(e) = item {
            if let Some(call) = find_ffi_call(e, &classify) {
                return Err(error_msg(keyword_label(&s.keyword), &s.name, &call));
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Rust impl — the canonical predicate, inlined here so the portable
// module doesn't need a wider surface on `runtime::validate`. Behaviour
// matches `runtime/src/runtime/validate.rs::enforce_external_only` 1:1.
// ─────────────────────────────────────────────────────────────────────

/// Native validator. Total, fast, always correct — the default.
pub struct RustValidate;

impl Portable for RustValidate {
    fn impl_name(&self) -> &'static str { "rust" }
}

impl ValidateImpl for RustValidate {
    fn enforce_external_only(&self, s: &SchemaDecl) -> Result<(), String> {
        enforce_with(s, classify_native)
    }
}

fn classify_native(name: &str) -> Option<String> {
    match name {
        "FFICall"   => Some("FFICall".to_string()),
        "FFIOpen"   => Some("FFIOpen".to_string()),
        "FFILookup" => Some("FFILookup".to_string()),
        "LibCall"   => Some("LibCall".to_string()),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — calls stdlib/passes/validate.ev for each Call name
// ─────────────────────────────────────────────────────────────────────

/// Pass-driven validator. Holds an [`EvidentRuntime`] with
/// `stdlib/ast.ev` + `stdlib/passes/validate.ev` loaded. `ValidateExpr`
/// JIT-caches after the first query, so per-Call classification is a
/// JIT function call plus marshaling — not a full Z3 solve. Build once
/// and reuse across schemas to amortise the load cost.
pub struct EvidentValidate {
    rt: EvidentRuntime,
}

impl EvidentValidate {
    /// Pass-claim name that classifies a single Expr node.
    const CLAIM: &'static str = "ValidateExpr";

    /// Load `ast.ev` + `passes/validate.ev` from `stdlib_dir` into a
    /// fresh runtime. `stdlib_dir` is the repo's `stdlib/` directory.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("ast.ev"))
            .map_err(|e| format!("load ast.ev: {e}"))?;
        rt.load_file(&stdlib_dir.join("passes").join("validate.ev"))
            .map_err(|e| format!("load passes/validate.ev: {e}"))?;
        Ok(Self { rt })
    }

    /// Ask the pass whether `name` is a banned FFI primitive. Pins
    /// `nm ∈ String` directly, runs `ValidateExpr`, and reads the
    /// `out` String binding back. Returns `Some(name)` when the pass
    /// classifies it as banned, `None` otherwise.
    ///
    /// Why `nm ∈ String` rather than the natural-feeling
    /// `e ∈ Expr` with `match e { ECall(nm, _) ⇒ … }`: the runtime's
    /// match-destructure on a given-pinned enum's String payload
    /// doesn't preserve byte equality with a source-literal string —
    /// `nm = "FFICall"` evaluates to false on both JIT and slow paths
    /// even when the bytes match. Pinning the name directly side-steps
    /// that gap and keeps the rule's decision logic in Evident (see
    /// `stdlib/passes/validate.ev` and gap #18 in
    /// `examples/COUNTEREXAMPLES.md`). When the gap is closed we can
    /// flip back to pinning `e ∈ Expr` with no shim change beyond
    /// rebuilding the given.
    fn classify_via_pass(&self, name: &str) -> Option<String> {
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("nm".to_string(), Value::Str(name.to_string()));
        match self.rt.query(Self::CLAIM, &given) {
            Ok(qr) if qr.satisfied => match qr.bindings.get("out") {
                Some(Value::Str(s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            },
            _ => None,
        }
    }
}

impl Portable for EvidentValidate {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl ValidateImpl for EvidentValidate {
    fn enforce_external_only(&self, s: &SchemaDecl) -> Result<(), String> {
        enforce_with(s, |name| self.classify_via_pass(name))
    }
}

// ─────────────────────────────────────────────────────────────────────
// Marshaling: Rust Expr → Value::Enum tree (matches stdlib/ast.ev)
// ─────────────────────────────────────────────────────────────────────
//
// A self-contained mirror of the private `*_to_value` family in
// translate/encode_ast.rs — same shape `portable/pretty.rs` uses, kept
// local so this port owns its marshaling. Unify with `encode_ast.rs`
// once that surface is public (the existing TODO in pretty.rs).

fn ev(enum_name: &str, variant: &str, fields: Vec<Value>) -> Value {
    Value::Enum {
        enum_name: enum_name.to_string(),
        variant: variant.to_string(),
        fields,
    }
}

fn encode_expr(e: &Expr) -> Value {
    match e {
        Expr::Identifier(s) => ev("Expr", "EIdentifier", vec![Value::Str(s.clone())]),
        Expr::Int(n)        => ev("Expr", "EInt",        vec![Value::Int(*n)]),
        Expr::Real(f)       => ev("Expr", "EReal",       vec![Value::Real(*f)]),
        Expr::Bool(b)       => ev("Expr", "EBool",       vec![Value::Bool(*b)]),
        Expr::Str(s)        => ev("Expr", "EStr",        vec![Value::Str(s.clone())]),
        Expr::SetLit(items) => ev("Expr", "ESetLit",     vec![encode_expr_list(items)]),
        Expr::SeqLit(items) => ev("Expr", "ESeqLit",     vec![encode_expr_list(items)]),
        Expr::Tuple(items)  => ev("Expr", "ETuple",      vec![encode_expr_list(items)]),
        Expr::Range(lo, hi) => ev("Expr", "ERange",      vec![encode_expr(lo), encode_expr(hi)]),
        Expr::InExpr(l, r)  => ev("Expr", "EInExpr",     vec![encode_expr(l), encode_expr(r)]),
        Expr::Forall(vs, range, body) =>
            ev("Expr", "EForall", vec![encode_string_list(vs), encode_expr(range), encode_expr(body)]),
        Expr::Exists(vs, range, body) =>
            ev("Expr", "EExists", vec![encode_string_list(vs), encode_expr(range), encode_expr(body)]),
        Expr::Call(name, args) =>
            ev("Expr", "ECall", vec![Value::Str(name.clone()), encode_expr_list(args)]),
        Expr::Cardinality(inner) => ev("Expr", "ECardinality", vec![encode_expr(inner)]),
        Expr::Index(seq, idx)    => ev("Expr", "EIndex", vec![encode_expr(seq), encode_expr(idx)]),
        Expr::Field(base, name)  => ev("Expr", "EField", vec![encode_expr(base), Value::Str(name.clone())]),
        Expr::Binary(op, l, r)   => ev("Expr", "EBinary", vec![encode_binop(op), encode_expr(l), encode_expr(r)]),
        Expr::Not(inner)         => ev("Expr", "ENot", vec![encode_expr(inner)]),
        Expr::Ternary(c, a, b)   => ev("Expr", "ETernary", vec![encode_expr(c), encode_expr(a), encode_expr(b)]),
        Expr::Match(scr, arms)   => ev("Expr", "EMatch", vec![encode_expr(scr), encode_match_arm_list(arms)]),
        Expr::Matches(e, pat)    => ev("Expr", "EMatches", vec![encode_expr(e), encode_match_pattern(pat)]),
    }
}

fn encode_binop(op: &BinOp) -> Value {
    let v = match op {
        BinOp::Eq => "OpEq", BinOp::Neq => "OpNeq", BinOp::Lt => "OpLt", BinOp::Le => "OpLe",
        BinOp::Gt => "OpGt", BinOp::Ge => "OpGe", BinOp::And => "OpAnd", BinOp::Or => "OpOr",
        BinOp::Implies => "OpImplies", BinOp::Add => "OpAdd", BinOp::Sub => "OpSub",
        BinOp::Mul => "OpMul", BinOp::Div => "OpDiv", BinOp::Concat => "OpConcat",
    };
    ev("BinOp", v, vec![])
}

fn encode_match_arm(a: &MatchArm) -> Value {
    ev("MatchArm", "MakeMatchArm", vec![encode_match_pattern(&a.pattern), encode_expr(&a.body)])
}

fn encode_match_pattern(p: &MatchPattern) -> Value {
    match p {
        MatchPattern::Wildcard => ev("MatchPattern", "PatWildcard", vec![]),
        MatchPattern::Ctor { name, binds } =>
            ev("MatchPattern", "PatCtor", vec![Value::Str(name.clone()), encode_bind_list(binds)]),
    }
}

fn cons_list(
    enum_name: &str,
    cons: &str,
    nil: &str,
    items: impl DoubleEndedIterator<Item = Value>,
) -> Value {
    let mut acc = ev(enum_name, nil, vec![]);
    for head in items.rev() {
        acc = ev(enum_name, cons, vec![head, acc]);
    }
    acc
}

fn encode_expr_list(items: &[Expr]) -> Value {
    cons_list("ExprList", "ELCons", "ELNil", items.iter().map(encode_expr))
}
fn encode_string_list(items: &[String]) -> Value {
    cons_list("StringList", "SLCons", "SLNil", items.iter().map(|s| Value::Str(s.clone())))
}
fn encode_match_arm_list(items: &[MatchArm]) -> Value {
    cons_list("MatchArmList", "MALCons", "MALNil", items.iter().map(encode_match_arm))
}
fn encode_bind_list(binds: &[Option<String>]) -> Value {
    cons_list("BindList", "BLCons", "BLNil", binds.iter().map(|b| match b {
        None => ev("MatchBind", "BindWildcard", vec![]),
        Some(n) => ev("MatchBind", "BindName", vec![Value::Str(n.clone())]),
    }))
}

// ─────────────────────────────────────────────────────────────────────
// Selection
// ─────────────────────────────────────────────────────────────────────

/// Pick an impl by `EVIDENT_VALIDATE_IMPL` (`rust` | `evident`),
/// defaulting to the Rust impl. `evident` requires `EVIDENT_STDLIB_DIR`
/// to point at the repo `stdlib/`; if loading fails it falls back to
/// Rust.
pub fn default_impl() -> Box<dyn ValidateImpl> {
    if std::env::var("EVIDENT_VALIDATE_IMPL").as_deref() == Ok("evident") {
        if let Ok(dir) = std::env::var("EVIDENT_STDLIB_DIR") {
            if let Ok(ev) = EvidentValidate::new(Path::new(&dir)) {
                return Box::new(ev);
            }
        }
    }
    Box::new(RustValidate)
}
