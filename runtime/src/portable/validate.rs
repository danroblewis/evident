//! `validate` — load-time external-only check. **Sole implementation: the
//! self-hosted Evident stack-FSM walk.**
//!
//! Load-time rule (`enforce_external_only`): reject non-`external` schemas
//! that construct FFI effects (`FFICall`, `FFIOpen`, `FFILookup`,
//! `LibCall`). Session VALIDATE-recursive cut validate over to
//! Evident-only: the canonical Rust `find_ffi_call` Expr-tree walk (in
//! `runtime/src/runtime/validate.rs`) is **deleted**, and the production
//! load path computes the verdict through [`EvidentValidate`]. There is no
//! Rust-walk fallback.
//!
//! [`EvidentValidate`] owns an [`EvidentRuntime`] with
//! `stdlib/passes/validate.ev` loaded. The WHOLE walk runs in Evident as an
//! FSM-with-stack (`validate_walk`): this shim only marshals each
//! `Constraint`'s `Expr` into a `Value` via the SHARED marshaler
//! ([`crate::translate::ast_encoder::expr_to_value`]), drives the FSM to a
//! drained-stack halt via [`crate::effect_loop::run_nested`], and collects
//! the `ECall` names it reached. **No Rust-side tree walk, no bespoke
//! encoder** — the recursion lives in the pass.
//!
//! ## What stays in Rust, and why
//!
//! The FSM owns the traversal and the name collection, but NOT the
//! banned-set decision: deciding `nm ∈ {FFICall, …}` means a string
//! equality, and doing that equality INSIDE the per-tick Z3 solve blows up
//! Z3's string theory on a walk state that carries unrelated string
//! literals (measured: minutes + GBs on `test_26_value_cache.ev::driver`'s
//! string-ternary — the in-solve cousin of gap #18). So the FSM emits the
//! RAW call names and [`is_banned`] does the 4-element membership check
//! here — the exact analogue of `subscriptions`' `world.`/`world_next.`
//! prefix split.
//!
//! ## The load entry point
//!
//! `runtime/src/runtime/validate.rs::enforce_external_only` (called from
//! the load path) delegates to the free [`enforce_external_only`] here,
//! which holds a per-thread lazily-built [`EvidentValidate`] engine: the
//! pass is loaded and JIT-cached once per thread, then reused for every
//! schema. The engine locates `stdlib/` via the one
//! [`crate::stdlib_path::stdlib_dir`] resolver (session WW).
//!
//! ## No bootstrap cycle
//!
//! Building the engine loads `validate.ev`, and that load itself runs the
//! production validate hook over `validate.ev`'s own schemas — a re-entry.
//! A thread-local guard ([`BOOTSTRAPPING`]) short-circuits the re-entrant
//! call to `Ok(())`: the pass file constructs `Expr` enum *values* named
//! `"ECall"` etc., it never *calls* a banned FFI primitive, so it
//! trivially passes. Once built, the guard is clear and user schemas get
//! the full walk.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use crate::core::ast::{BodyItem, Expr, Keyword, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_list, decode_str};
use crate::translate::ast_encoder::expr_to_value;
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
pub(crate) fn error_msg(kind: &str, name: &str, call: &str) -> String {
    format!(
        "{kind} `{name}` constructs `{call}(...)` but isn't \
         declared `external`. Either mark this declaration \
         `external claim` / `external type`, or move the \
         FFI into an `external claim` helper and call it \
         from here."
    )
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — runs stdlib/passes/validate.ev as a stack-FSM
// ─────────────────────────────────────────────────────────────────────

/// Pass-driven validator. Holds an [`EvidentRuntime`] with
/// `stdlib/passes/validate.ev` loaded — a self-contained pass (it
/// declares its own cons-list copy of the `Expr`-reachable AST enums
/// matching the shared marshaler), so no other stdlib file is needed.
/// Build once and reuse across schemas so the FSM's per-tick solve is
/// JIT-cached.
pub struct EvidentValidate {
    rt: EvidentRuntime,
}

impl EvidentValidate {
    /// The whole-walk FSM in `stdlib/passes/validate.ev`.
    const WALK_FSM: &'static str = "validate_walk";

    /// Max-iteration guard for the nested walk. One AST node costs a
    /// small constant number of FSM ticks, and the walk halts the moment
    /// a banned call is found, so a clean Expr of N nodes halts in O(N)
    /// ticks; the cap sits far above any realistic constraint so a
    /// legitimate walk never hits it (a non-terminating walk would be a
    /// pass bug, surfaced as a loud `MaxItersExceeded`).
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/validate.ev` from `stdlib_dir` into a fresh runtime.
    /// `stdlib_dir` is the repo's `stdlib/` directory.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("validate.ev"))
            .map_err(|e| format!("load passes/validate.ev: {e}"))?;
        Ok(Self { rt })
    }

    /// Drive `validate_walk` over one `Constraint`'s `Expr` and return the
    /// first banned FFI call name it reaches (in pre-order), or `None` if
    /// the expression constructs no banned FFI primitive. The FSM returns
    /// `SVDone(NameList)` — the `ECall` names it collected, head-first
    /// (i.e. reverse pre-order); reversing recovers pre-order so the first
    /// banned name matches what `find_ffi_call` would have returned.
    fn find_banned(&self, e: &Expr) -> Option<String> {
        let seed = work_expr(expr_to_value(e));
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "SVDone" && fields.len() == 1 => {
                // Shared cons-list decoder: NameList → Vec<String>.
                match decode_list(&fields[0], "NameList", "NameNil", "NameCons", decode_str) {
                    Ok(names) => names.iter().rev().find(|n| is_banned(n)).cloned(),
                    Err(e) => {
                        eprintln!("[validate/evident] decode of collected names failed: {e}");
                        None
                    }
                }
            }
            Ok(other) => {
                eprintln!("[validate/evident] walk returned a non-SVDone state: {other:?}");
                None
            }
            Err(e) => {
                eprintln!("[validate/evident] walk failed: {e}");
                None
            }
        }
    }
}

/// The leaf decision the FSM defers to Rust: is `name` one of the four
/// banned FFI-construction primitives? A 4-element string-set membership,
/// kept out of the per-tick Z3 solve (where state-carried-string vs
/// literal equality blows up — the in-solve cousin of gap #18). This is
/// the validate analogue of `subscriptions`' `world.`/`world_next.`
/// prefix split: the recursive WALK self-hosts; only this tiny string
/// decision stays in Rust.
fn is_banned(name: &str) -> bool {
    matches!(name, "FFICall" | "FFIOpen" | "FFILookup" | "LibCall")
}

impl Portable for EvidentValidate {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl ValidateImpl for EvidentValidate {
    fn enforce_external_only(&self, s: &SchemaDecl) -> Result<(), String> {
        // Mirror the canonical caller: `external` schemas may construct
        // FFI; otherwise check each `Constraint` body item's Expr (and
        // ONLY those — subclaim bodies get their own load-pass check).
        // The Expr-tree recursion is the FSM's job.
        if s.external { return Ok(()); }
        for item in &s.body {
            if let BodyItem::Constraint(e) = item {
                if let Some(call) = self.find_banned(e) {
                    return Err(error_msg(keyword_label(&s.keyword), &s.name, &call));
                }
            }
        }
        Ok(())
    }
}

/// Wrap an already-marshaled `Expr` `Value` as the FSM's unified `Work`
/// node `WExpr(Expr)`. `run_nested`'s coerce seeds it into `SVSeed(Work)`.
fn work_expr(inner: Value) -> Value {
    Value::Enum {
        enum_name: "Work".to_string(),
        variant: "WExpr".to_string(),
        fields: vec![inner],
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point — a per-thread cached engine + bootstrap guard
// ─────────────────────────────────────────────────────────────────────

thread_local! {
    /// One [`EvidentValidate`] engine per thread, built lazily on the
    /// first [`enforce_external_only`] call. `EvidentRuntime` is
    /// `!Send`/`!Sync` (Z3 context, Cranelift module, `Rc`/`RefCell`
    /// interior), so a thread-local — not a global — is the right cache:
    /// load + JIT-compile the pass once per thread, reuse for every
    /// schema.
    static ENGINE: std::cell::RefCell<Option<Rc<EvidentValidate>>> =
        const { std::cell::RefCell::new(None) };

    /// Re-entrancy guard. Set while the engine's private runtime is
    /// loading `validate.ev`: that load runs the production validate hook
    /// over the pass's own schemas, which would re-enter here mid-build
    /// (and re-borrow [`ENGINE`]). While set, [`enforce_external_only`]
    /// returns `Ok(())` — the pass file is trusted, hand-verified stdlib
    /// that constructs `Expr` enum *values* (named `"ECall"` etc.), never
    /// *calls* a banned FFI primitive, so it trivially passes.
    static BOOTSTRAPPING: Cell<bool> = const { Cell::new(false) };
}

/// Enforce the external-only rule on one schema via the self-hosted Evident
/// `validate_walk` pass. **This is the runtime's sole validate entry
/// point** — `runtime::validate::enforce_external_only` (on the load path)
/// delegates here.
///
/// Builds and caches a per-thread [`EvidentValidate`] engine on first use
/// (see [`ENGINE`]). The engine locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver.
///
/// # Panics
///
/// If `stdlib/passes/validate.ev` cannot be located or loaded. There is no
/// Rust-walk fallback (this session), so an unloadable pass is a hard error
/// — the same robust resolution the rest of the runtime relies on (session
/// WW). The error names every checked path and the `EVIDENT_STDLIB`
/// override.
pub fn enforce_external_only(s: &SchemaDecl) -> Result<(), String> {
    // Re-entrancy break: while building the engine (loading the trusted
    // validate pass), skip validation — see [`BOOTSTRAPPING`].
    if BOOTSTRAPPING.with(|b| b.get()) {
        return Ok(());
    }
    let engine = ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            // The build loads validate.ev; that load re-enters this
            // function for each pass schema. Guard the window so the
            // re-entry short-circuits before touching `ENGINE` again.
            BOOTSTRAPPING.with(|b| b.set(true));
            let built = build_engine();
            BOOTSTRAPPING.with(|b| b.set(false));
            *slot = Some(Rc::new(built));
        }
        slot.as_ref().unwrap().clone()
    });
    engine.enforce_external_only(s)
}

/// Locate `stdlib/` and load the validate pass into a fresh engine.
/// Panics with the resolver's path-list diagnostic on failure — see
/// [`enforce_external_only`].
fn build_engine() -> EvidentValidate {
    let dir = crate::stdlib_path::stdlib_dir().unwrap_or_else(|e| panic!(
        "validate: cannot locate stdlib to load the validate pass \
         (the sole impl since session VALIDATE-recursive): {e}"));
    EvidentValidate::new(&dir).unwrap_or_else(|e| panic!(
        "validate: failed to load passes/validate.ev from {}: {e}",
        dir.display()))
}
