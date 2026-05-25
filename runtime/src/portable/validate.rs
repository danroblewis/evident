//! `validate` — the load-time external-only check, **self-hosted as the
//! sole implementation**.
//!
//! Reject non-`external` schemas that construct FFI effects (`FFICall`,
//! `FFIOpen`, `FFILookup`, `LibCall`). Demos and ordinary library code
//! reach C through the `external claim` wrappers in `packages/` and
//! `stdlib/posix.ev`; the runtime refuses to compile a non-`external`
//! claim that does so itself.
//!
//! As of the validate cutover (session VV) there is exactly one
//! implementation: [`EvidentValidate`]. The canonical Rust
//! `enforce_external_only` in `runtime/src/runtime/validate.rs` and the
//! parallel `RustValidate` impl that used to live here are both deleted.
//! The load path in `runtime/src/runtime/load.rs` builds one
//! `EvidentValidate` per runtime (lazily, cached) and routes every
//! schema's check through it.
//!
//! The split between Rust and Evident:
//!
//!   * **Rust owns the walk.** [`find_ffi_call`] recurses the `Expr`
//!     tree of each `Constraint` body item — the language can't yet
//!     recurse over `Expr` trees (see `docs/self-hosting.md`).
//!   * **Evident owns the decision.** At every `Call` node the walker
//!     asks `ValidateExpr` in `stdlib/passes/validate.ev` whether the
//!     call name is one of the banned FFI primitives. "What counts as a
//!     banned call" is the rule, and it lives in Evident.
//!
//! Why `nm ∈ String` rather than the natural-feeling `e ∈ Expr` with
//! `match e { ECall(nm, _) ⇒ … }`: the runtime's match-destructure on a
//! given-pinned enum's String payload doesn't preserve byte equality
//! with a source-literal string — `nm = "FFICall"` evaluates to false on
//! both JIT and slow paths even when the bytes match. Pinning the name
//! directly side-steps that gap and keeps the rule's decision logic in
//! Evident (see `stdlib/passes/validate.ev` and the given-pinned-enum
//! String-equality gap in `examples/COUNTEREXAMPLES.md`).

use std::collections::HashMap;
use std::path::Path;

use crate::core::ast::{BodyItem, Expr, Keyword, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `enforce_external_only`'s Rust-level signature. Returns `Ok(())` when
/// the schema passes the check and `Err(msg)` with a human-readable
/// diagnostic otherwise. The seam returns `String`; the load path maps
/// it to `RuntimeError::Parse`.
pub trait ValidateImpl: Portable {
    fn enforce_external_only(&self, s: &SchemaDecl) -> Result<(), String>;
}

/// `kind` label used in the diagnostic message.
fn keyword_label(kw: &Keyword) -> &'static str {
    match kw {
        Keyword::Fsm      => "fsm",
        Keyword::Type     => "type",
        Keyword::Claim    => "claim",
        Keyword::Schema   => "schema",
        Keyword::Subclaim => "subclaim",
    }
}

/// Format the diagnostic. Identical wording to the historical canonical
/// impl so the message text callers see is unchanged by the cutover.
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
// The walk — recurses the Expr tree, parameterised by the per-Call
// classifier. The classifier is the one piece that lives in Evident.
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
// Evident impl — calls stdlib/passes/validate.ev for each Call name
// ─────────────────────────────────────────────────────────────────────

/// Pass-driven validator. Holds an [`EvidentRuntime`] with
/// `stdlib/passes/validate.ev` loaded. Build once and reuse across
/// schemas to amortise the load cost (the load path caches one instance
/// per runtime).
///
/// The per-call classifier is a pure function of the call *name*, so its
/// verdict is memoized in `decisions`. A program has far fewer distinct
/// call names than call sites (every `Color(...)`, `IVec2(...)`, claim
/// call, etc. repeats), so the underlying `ValidateExpr` query runs once
/// per *distinct name* rather than once per call site. Without this the
/// per-call query — a full solve on the validator's nested runtime,
/// ~15ms each — turns validating a call-heavy file into seconds. With
/// it, the cost is `O(distinct names)` queries amortised across the
/// whole file load.
pub struct EvidentValidate {
    rt: EvidentRuntime,
    decisions: std::cell::RefCell<HashMap<String, Option<String>>>,
}

impl EvidentValidate {
    /// Pass-claim name that classifies a single call name.
    const CLAIM: &'static str = "ValidateExpr";

    /// Load `passes/validate.ev` from `stdlib_dir` into a fresh runtime.
    /// `stdlib_dir` is the repo's `stdlib/` directory.
    ///
    /// `ValidateExpr` is a pure `String → String` claim and references
    /// no AST type, so this no longer loads `stdlib/ast.ev` — the
    /// validator builds from a single 1-claim file with no enum
    /// registration, which keeps the per-runtime build cost tiny.
    ///
    /// The nested runtime is flagged as the validator bootstrap before
    /// loading so its own load path skips the external-only check —
    /// otherwise loading `validate.ev` would recurse into building
    /// another `EvidentValidate`.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.set_validate_bootstrap(true);
        rt.load_file(&stdlib_dir.join("passes").join("validate.ev"))
            .map_err(|e| format!("load passes/validate.ev: {e}"))?;
        Ok(Self { rt, decisions: std::cell::RefCell::new(HashMap::new()) })
    }

    /// Ask the pass whether `name` is a banned FFI primitive, memoizing
    /// the verdict per name. Pins `nm ∈ String` directly, runs
    /// `ValidateExpr`, and reads the `out` String binding back. Returns
    /// `Some(name)` when the pass classifies it as banned, `None`
    /// otherwise.
    fn classify_via_pass(&self, name: &str) -> Option<String> {
        if let Some(cached) = self.decisions.borrow().get(name) {
            return cached.clone();
        }
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("nm".to_string(), Value::Str(name.to_string()));
        let verdict = match self.rt.query(Self::CLAIM, &given) {
            Ok(qr) if qr.satisfied => match qr.bindings.get("out") {
                Some(Value::Str(s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            },
            _ => None,
        };
        self.decisions.borrow_mut().insert(name.to_string(), verdict.clone());
        verdict
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
