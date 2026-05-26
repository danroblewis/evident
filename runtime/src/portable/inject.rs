//! `inject` — FSM-aware membership injection. **Partial cutover (session
//! REVIVE-inject):** two of inject's four sub-passes now self-host in
//! Evident; the other two stay in Rust pending Gap D.
//!
//! ## The four sub-passes, and the split
//!
//! `inject` (the runtime's biggest pure pass) decides, per claim body,
//! which implicit memberships to splice in at `param_count`. It has four
//! sub-passes:
//!
//!   * `inject_fsm_params`      — `state_next` / `last_results` / `effects`
//!                                when referenced + undeclared.    ← Evident
//!   * `inject_prev_tick_decls` — `_var` time-shift slots +
//!                                `is_first_tick`.                  ← Evident
//!   * `inject_claim_arg_types` — type a fresh positional arg from the
//!                                *called claim's* signature.       ← Rust
//!   * `inject_lhs_eq_types`    — infer an `lhs = expr` type via field
//!                                chains + enum-variant lookup.     ← Rust
//!
//! The first two decide what to inject from **one claim's body alone**.
//! They cut over here. The last two resolve a name's type against the
//! **whole-program schema table + enum registry** — not one body, but every
//! loaded claim and enum. Marshaling all of that into an FSM per claim is a
//! composite-INPUT blow-up the `run()`/marshaler recipe doesn't faithfully
//! support yet (Gap D, `examples/COUNTEREXAMPLES.md` #27). They keep their
//! canonical Rust impl (`crate::runtime::inject::*`), called directly from
//! the load path; this shim never touches them.
//!
//! ## What runs where, for the two cut-over sub-passes
//!
//! `stdlib/passes/inject.ev` self-hosts THREE FSMs:
//!
//!   * `inject_collect`    — the recursive reference-collection WALK (the
//!                           structural bulk of both sub-passes): pop an
//!                           Expr off a stack, fold reachable identifiers.
//!   * `fsm_params_build`  — reads the six inject DECISIONS as marshaled-in
//!                           `Bool`s, COMPUTES `(reff ∧ ¬heff)` /
//!                           `(rsn ∧ ¬hsn ∧ hst)` on the destructured
//!                           payloads, and CONSTRUCTS the `BodyItemList` to
//!                           inject — returned through `run()`.
//!   * `prev_tick_build`   — maps the `(_var, type)` pairs to memberships +
//!                           conditionally prepends `is_first_tick` (the
//!                           destructured-Bool-as-ITE-cond decision).
//!
//! The DECISION + CONSTRUCTION self-hosting was blocked until GAPB fixed
//! gap #18 (a match-destructured `Bool` read its real value only when
//! embedded into a node, not when COMPUTED on) — see the keystone reads in
//! the two `*_build` FSMs. This shim does only the parts Evident can't /
//! shouldn't express:
//!
//!   1. **String-set membership stays in Rust.** "Is `state_next`
//!      reachable? already declared?" and the `_`-strip / first-segment
//!      split for `_var`s are string comparisons. Done over the walk's
//!      output here — NOT in the FSM — to dodge the in-solve string-theory
//!      blow-up the validate port hit (its in-FSM `nm = "FFICall"` is the
//!      cousin of #18). The FSM gets BOOLEANS, never raw name strings to
//!      compare.
//!   2. **The splice happens here, at `s.param_count`.** The `*_build` FSMs
//!      return only the *new* memberships; this shim inserts them at
//!      `s.param_count` (in hand from the `SchemaDecl`). The existing body
//!      items never round-trip the marshaler, so there is no decode-
//!      faithfulness risk on the user's constraints. (`param_count` now
//!      round-trips through the marshaler — GAPB — which would enable a
//!      whole-`SchemaDecl`-return design; that's deferred, since splicing at
//!      a number already in hand is simpler and risk-free.)
//!
//! ## The load-path entry points
//!
//! Production calls the free [`fsm_params`] / [`prev_tick`] functions, which
//! hold a per-thread lazily-built [`EvidentInject`] engine: the pass is
//! loaded and JIT-cached once per thread, then reused for every claim. The
//! engine locates `stdlib/` via the one [`crate::stdlib_path::stdlib_dir`]
//! resolver (session WW). `inject` is a load-time pass — per-tick runtime is
//! untouched.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::core::ast::{BodyItem, Expr, Keyword, Pins, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::decode_str;
use crate::translate::ast_encoder::expr_to_value;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The engine
// ─────────────────────────────────────────────────────────────────────

/// Self-hosted injector for the two cut-over sub-passes. Holds an
/// [`EvidentRuntime`] with `stdlib/passes/inject.ev` loaded; drives the
/// walk + the two `*_build` construction FSMs. Build once and reuse — each
/// FSM's per-tick solve is JIT-cached across calls.
pub struct EvidentInject {
    rt: EvidentRuntime,
}

impl EvidentInject {
    /// The reference-collection walk FSM.
    const WALK_FSM: &'static str = "inject_collect";
    /// The fsm-params decision + construction FSM.
    const FPB_FSM: &'static str = "fsm_params_build";
    /// The prev-tick construction FSM.
    const PTB_FSM: &'static str = "prev_tick_build";

    /// Max-iteration guard for the nested runs. One expr node / one list
    /// element costs a small constant number of FSM ticks; the cap is far
    /// above any realistic claim, so a legitimate run never hits it.
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/inject.ev` from `stdlib_dir` into a fresh runtime. The
    /// pass is self-contained (it declares its own cons-list copy of the AST
    /// enums matching the shared marshaler), so no other stdlib file is
    /// needed.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("inject.ev"))
            .map_err(|e| format!("load passes/inject.ev: {e}"))?;
        Ok(Self { rt })
    }

    // ── The walk: collect every referenced identifier in a body ──
    //
    // Visits constraint exprs and claim-call mapping VALUES (not
    // memberships, pins, subclaims, passthroughs, or halts_within) — the
    // exact set the canonical `inject.rs` walk reached. Each expr is
    // marshaled with the shared `expr_to_value` and driven through
    // `inject_collect` to a drained-stack `IWDone(NameList)`; the cons-list
    // of raw identifier strings is decoded with the shared `decode_list`.
    fn collect_refs(&self, body: &[BodyItem]) -> Vec<String> {
        let mut out = Vec::new();
        for item in body {
            match item {
                BodyItem::Constraint(e) => self.walk_expr(e, &mut out),
                BodyItem::ClaimCall { mappings, .. } =>
                    for m in mappings { self.walk_expr(&m.value, &mut out); },
                _ => {}
            }
        }
        out
    }

    fn walk_expr(&self, e: &Expr, out: &mut Vec<String>) {
        let seed = work_expr(expr_to_value(e));
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. })
                if variant == "IWDone" && fields.len() == 1 =>
            {
                match crate::translate::ast_decoder::decode_list(
                    &fields[0], "NameList", "NameNil", "NameCons", decode_str)
                {
                    Ok(names) => out.extend(names),
                    Err(e) => eprintln!("[inject/evident] decode of name list failed: {e}"),
                }
            }
            Ok(other) => eprintln!("[inject/evident] walk returned non-IWDone: {other:?}"),
            Err(e) => eprintln!("[inject/evident] walk failed: {e}"),
        }
    }

    // ── inject_fsm_params — walk (Evident) + string-eq (Rust) +
    //    decision/construction (Evident `fsm_params_build`) + splice (Rust)
    //
    // Mirrors the deleted `runtime::inject::inject_fsm_params` exactly, but
    // the referenced-name set comes from the FSM walk and the
    // referenced/undeclared booleans drive the FSM construction instead of
    // an inline Rust `if` chain.
    pub fn fsm_params(&self, s: &mut SchemaDecl) {
        if !matches!(s.keyword, Keyword::Fsm) { return; }
        if s.external { return; }

        // Declared-membership scan (the state type + which canonical slots
        // the user already declared).
        let mut state_type: Option<String> = None;
        let mut have_state_next = false;
        let mut have_last_results = false;
        let mut have_effects = false;
        for item in &s.body {
            if let BodyItem::Membership { name, type_name, .. } = item {
                match name.as_str() {
                    "state" if state_type.is_none() => state_type = Some(type_name.clone()),
                    "state_next"   => have_state_next   = true,
                    "last_results" => have_last_results = true,
                    "effects"      => have_effects      = true,
                    _ => {}
                }
            }
        }

        // String-set membership: which slots are reachable (in Rust, off the
        // walk's output — never compared inside the solve).
        let refs = self.collect_refs(&s.body);
        let ref_state_next   = refs.iter().any(|n| n == "state_next");
        let ref_last_results = refs.iter().any(|n| n == "last_results");
        let ref_effects      = refs.iter().any(|n| n == "effects");

        // Hand the six decisions + the state type to the construction FSM,
        // which computes `(r ∧ ¬h …)` on the destructured Bools (the #18
        // keystone) and returns the memberships to inject.
        let seed = fpb_input(
            ref_state_next, have_state_next,
            ref_last_results, have_last_results,
            ref_effects, have_effects,
            state_type.as_deref().unwrap_or(""),
            state_type.is_some(),
        );
        let injected = self.run_build(Self::FPB_FSM, seed, "FPBDone");
        splice_at(s, injected);
    }

    // ── inject_prev_tick_decls — walk (Evident) + `_`-strip/lookup (Rust) +
    //    construction (Evident `prev_tick_build`) + splice (Rust)
    //
    // Mirrors the deleted `runtime::inject::inject_prev_tick_decls`. The
    // `_`-strip + first-segment split + declared-name lookup stays in Rust
    // (substring ops Evident lacks — the same honest split subscriptions.ev
    // documents); the resulting `(_var, type)` pairs + is_first_tick flag
    // drive the construction FSM.
    pub fn prev_tick(&self, s: &mut SchemaDecl) {
        if !matches!(s.keyword, Keyword::Fsm) { return; }
        if s.external { return; }

        let mut declared: HashMap<String, String> = HashMap::new();
        for item in &s.body {
            if let BodyItem::Membership { name, type_name, .. } = item {
                declared.insert(name.clone(), type_name.clone());
            }
        }

        let refs = self.collect_refs(&s.body);
        // `_count` → strip → `count`; `_pos.x` → strip → first segment
        // `pos`. Register the bare `_first_seg` keyed once, typed to match.
        let mut prev_refs: HashMap<String, String> = HashMap::new();
        for n in &refs {
            let Some(after_underscore) = n.strip_prefix('_') else { continue };
            let first_seg = after_underscore.split('.').next().unwrap_or(after_underscore);
            if let Some(ty) = declared.get(first_seg) {
                prev_refs.insert(format!("_{first_seg}"), ty.clone());
            }
        }
        // No `_var` reference at all → inject nothing (not even
        // is_first_tick). Matches the canonical pass's early return.
        if prev_refs.is_empty() { return; }

        // Pairs to inject = referenced prev-vars not already declared;
        // is_first_tick added once unless the user declared it.
        let pairs: Vec<(String, String)> = prev_refs.into_iter()
            .filter(|(name, _)| !declared.contains_key(name))
            .collect();
        let add_first_tick = !declared.contains_key("is_first_tick");

        let seed = ptb_input(&pairs, add_first_tick);
        let injected = self.run_build(Self::PTB_FSM, seed, "PTBDone");
        splice_at(s, injected);
    }

    /// Drive a `*_build` FSM to its `<done_variant>(BodyItemList)` halt and
    /// decode the returned cons-list into `BodyItem::Membership`s. Returns
    /// an empty vec (injecting nothing) on any run/decode failure, which is
    /// loud (`eprintln!`) but never silently wrong-valued.
    fn run_build(&self, fsm: &str, seed: Value, done_variant: &str) -> Vec<BodyItem> {
        match crate::effect_loop::run_nested(&self.rt, fsm, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. })
                if variant == done_variant && fields.len() == 1 =>
                decode_membership_list(&fields[0]),
            Ok(other) => {
                eprintln!("[inject/evident] {fsm} returned non-{done_variant}: {other:?}");
                Vec::new()
            }
            Err(e) => {
                eprintln!("[inject/evident] {fsm} run failed: {e}");
                Vec::new()
            }
        }
    }
}

impl Portable for EvidentInject {
    fn impl_name(&self) -> &'static str { "evident" }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry points — a per-thread cached engine
// ─────────────────────────────────────────────────────────────────────

thread_local! {
    /// One [`EvidentInject`] engine per thread, built lazily on first use.
    /// `EvidentRuntime` is `!Send`/`!Sync` (Z3 context, Cranelift module,
    /// `Rc`/`RefCell` interior), so a thread-local — not a global — is the
    /// right cache: loading happens single-threaded, paying the pass-load +
    /// JIT-compile cost once.
    static ENGINE: RefCell<Option<Rc<EvidentInject>>> = const { RefCell::new(None) };

    /// Re-entrancy guard. Set while the engine's private runtime is loading
    /// `inject.ev`: that load runs the production inject hooks
    /// ([`fsm_params`] / [`prev_tick`]) over the pass's own FSMs, which would
    /// re-enter here mid-build and re-borrow [`ENGINE`]. While set, the
    /// hooks no-op — every pass file (`inject.ev`, and the `validate.ev` /
    /// `subscriptions.ev` engines that build transitively) is now written in
    /// the terse `_state` form (session STATE-terse). `unify_state_syntax`
    /// runs FIRST on the load path (`load.rs:71`, before the inject hooks),
    /// rewriting each `fsm X(state ∈ T, halt ∈ Bool)` to the `state,
    /// state_next ∈ T` pair and CONSUMING the `_state` reads — so by the time
    /// these hooks see the body there is no `_var` left for `prev_tick` to
    /// inject and `state_next` is already declared (so `fsm_params` injects
    /// nothing either). The no-op is exactly what the canonical Rust pass
    /// produced too. (These FSMs are also short-circuited by
    /// [`is_self_hosted_pass_fsm`] before [`engine`] runs — belt and braces.)
    static BOOTSTRAPPING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn engine() -> Rc<EvidentInject> {
    ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            // The build loads inject.ev; that load re-enters fsm_params /
            // prev_tick for each pass FSM. Guard the window so the re-entry
            // short-circuits before touching `ENGINE` again.
            BOOTSTRAPPING.with(|b| b.set(true));
            let built = build_engine();
            BOOTSTRAPPING.with(|b| b.set(false));
            *slot = Some(Rc::new(built));
        }
        slot.as_ref().unwrap().clone()
    })
}

/// Locate `stdlib/` and load the inject pass into a fresh engine. Panics
/// with the resolver's path-list diagnostic on failure — there is no
/// Rust-walk fallback for the two cut-over sub-passes (session
/// REVIVE-inject), the same hard-error policy subscriptions uses.
fn build_engine() -> EvidentInject {
    let dir = crate::stdlib_path::stdlib_dir().unwrap_or_else(|e| panic!(
        "inject: cannot locate stdlib to load the inject pass \
         (the sole impl of fsm_params / prev_tick since session REVIVE-inject): {e}"));
    EvidentInject::new(&dir).unwrap_or_else(|e| panic!(
        "inject: failed to load passes/inject.ev from {}: {e}", dir.display()))
}

/// The runtime's own self-hosted-pass FSMs. They live in `stdlib/passes/`,
/// are written in the terse `_state` form (session STATE-terse) which
/// `unify_state_syntax` rewrites to the `state, state_next ∈ T` pair before
/// these hooks run, and reference no implicit slot, so injection is a
/// guaranteed no-op for them. Skipping them by name does two things: (1)
/// it's the cheap correct answer,
/// and (2) — crucially — it breaks a per-load cross-engine cascade. Every
/// load runs `validate`'s hook, which builds the *validate* engine, which
/// loads `validate.ev` — and that file declares `fsm validate_walk`. Without
/// this skip, processing `validate_walk` would build the (heavier) *inject*
/// engine on every single load, even for programs with no FSM of their own.
/// A new self-hosted pass FSM must be added here; the cost of forgetting is
/// only the cascade slowdown, never a wrong injection (these never need any).
fn is_self_hosted_pass_fsm(name: &str) -> bool {
    matches!(name,
        "inject_collect" | "fsm_params_build" | "prev_tick_build"
        | "validate_walk" | "subscriptions_walk" | "pretty_walk")
}

/// `inject_fsm_params`, self-hosted. The load path's sole entry point for
/// the `state_next` / `last_results` / `effects` injection.
///
/// Both sub-passes are no-ops for non-`fsm` / `external` schemas and for the
/// runtime's own pass FSMs, so those checks happen HERE, before [`engine`] —
/// a program with no (user) FSM never pays the one-time inject-engine build
/// (loading `inject.ev` + JIT). This keeps the cutover's setup cost confined
/// to programs that actually declare FSMs and avoids the cross-engine
/// cascade (see [`is_self_hosted_pass_fsm`]).
pub fn fsm_params(s: &mut SchemaDecl) {
    if !matches!(s.keyword, Keyword::Fsm) || s.external { return; }
    if is_self_hosted_pass_fsm(&s.name) { return; }
    if BOOTSTRAPPING.with(|b| b.get()) { return; }
    engine().fsm_params(s);
}

/// `inject_prev_tick_decls`, self-hosted. The load path's sole entry point
/// for the `_var` time-shift + `is_first_tick` injection.
pub fn prev_tick(s: &mut SchemaDecl) {
    if !matches!(s.keyword, Keyword::Fsm) || s.external { return; }
    if is_self_hosted_pass_fsm(&s.name) { return; }
    if BOOTSTRAPPING.with(|b| b.get()) { return; }
    engine().prev_tick(s);
}

// ─────────────────────────────────────────────────────────────────────
// Small helpers — seeds in, memberships out, splice
// ─────────────────────────────────────────────────────────────────────

/// Splice `items` into `s.body` at `s.param_count` (the first-line-param
/// insertion index). The one piece kept in Rust because the index is right
/// here in the `SchemaDecl`; the existing body never round-trips.
fn splice_at(s: &mut SchemaDecl, items: Vec<BodyItem>) {
    let insert_pos = s.param_count;
    for (i, item) in items.into_iter().enumerate() {
        s.body.insert(insert_pos + i, item);
    }
}

/// Wrap an already-marshaled `Expr` `Value` as the walk FSM's `Work` node.
fn work_expr(inner: Value) -> Value {
    Value::Enum {
        enum_name: "Work".to_string(),
        variant: "WExpr".to_string(),
        fields: vec![inner],
    }
}

/// Pack the six fsm-params decisions + state type into `MakeFPBInput(...)`
/// — `run_nested`'s coerce wraps it into `FPBInit(FPBInput)`.
#[allow(clippy::too_many_arguments)]
fn fpb_input(
    rsn: bool, hsn: bool, rlr: bool, hlr: bool, reff: bool, heff: bool,
    state_type: &str, has_state: bool,
) -> Value {
    Value::Enum {
        enum_name: "FPBInput".to_string(),
        variant: "MakeFPBInput".to_string(),
        fields: vec![
            Value::Bool(rsn), Value::Bool(hsn),
            Value::Bool(rlr), Value::Bool(hlr),
            Value::Bool(reff), Value::Bool(heff),
            Value::Str(state_type.to_string()), Value::Bool(has_state),
        ],
    }
}

/// Pack the `(_var, type)` pairs + is_first_tick flag into
/// `MakePTBInput(StrPairList, Bool)` — wrapped into `PTBInit(PTBInput)`.
fn ptb_input(pairs: &[(String, String)], add_first_tick: bool) -> Value {
    let mut list = Value::Enum {
        enum_name: "StrPairList".to_string(),
        variant: "SPLNil".to_string(),
        fields: vec![],
    };
    for (name, ty) in pairs.iter().rev() {
        let pair = Value::Enum {
            enum_name: "StrPair".to_string(),
            variant: "MakeStrPair".to_string(),
            fields: vec![Value::Str(name.clone()), Value::Str(ty.clone())],
        };
        list = Value::Enum {
            enum_name: "StrPairList".to_string(),
            variant: "SPLCons".to_string(),
            fields: vec![pair, list],
        };
    }
    Value::Enum {
        enum_name: "PTBInput".to_string(),
        variant: "MakePTBInput".to_string(),
        fields: vec![list, Value::Bool(add_first_tick)],
    }
}

/// Walk a returned `BodyItemList` cons-list `Value` (`BILNil` / `BILCons`)
/// and rebuild each `BIMembership(name, type, _)` as a
/// `BodyItem::Membership`. The `*_build` FSMs only ever emit `BIMembership`
/// with `PNone` pins, so pins are read as `Pins::None`.
fn decode_membership_list(v: &Value) -> Vec<BodyItem> {
    let mut out = Vec::new();
    let mut cur = v;
    loop {
        let Value::Enum { variant, fields, .. } = cur else { break };
        match variant.as_str() {
            "BILNil" => break,
            "BILCons" if fields.len() == 2 => {
                if let Value::Enum { variant: bv, fields: bf, .. } = &fields[0] {
                    if bv == "BIMembership" && bf.len() == 3 {
                        if let (Value::Str(name), Value::Str(ty)) = (&bf[0], &bf[1]) {
                            out.push(BodyItem::Membership {
                                name: name.clone(),
                                type_name: ty.clone(),
                                pins: Pins::None,
                            });
                        }
                    }
                }
                cur = &fields[1];
            }
            _ => break,
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// Tests — correctness against a golden snapshot of the corpus
// ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const STDLIB: &str = "../stdlib";

    fn evident_impl() -> EvidentInject {
        EvidentInject::new(Path::new(STDLIB)).expect("load stdlib/passes/inject.ev")
    }

    /// Run both cut-over sub-passes through the Evident engine, in the load
    /// path's order (fsm_params, then — modulo the two Rust sub-passes
    /// load.rs interleaves — prev_tick), and return the set of membership
    /// names the body gained.
    fn injected_names(ev: &EvidentInject, raw: &SchemaDecl) -> Vec<String> {
        let before: HashSet<String> = raw.body.iter().filter_map(|i| match i {
            BodyItem::Membership { name, .. } => Some(name.clone()),
            _ => None,
        }).collect();
        let mut s = raw.clone();
        ev.fsm_params(&mut s);
        ev.prev_tick(&mut s);
        let mut added: Vec<String> = s.body.iter().filter_map(|i| match i {
            BodyItem::Membership { name, type_name, .. } if !before.contains(name) =>
                Some(format!("{name} \u{2208} {type_name}")),
            _ => None,
        }).collect();
        added.sort();
        added
    }

    /// Golden: the membership set each corpus FSM gains from the two
    /// cut-over sub-passes, captured from the canonical Rust impl BEFORE its
    /// deletion (`dump_golden`, session REVIVE-inject). The self-hosted
    /// pipeline must reproduce it byte-for-byte. This is the cutover's
    /// correctness contract — it replaces the equivalence-vs-RustInject test
    /// (RustInject's two sub-passes no longer exist to compare against).
    const GOLDEN: &[(&str, &str, &str)] = &[
        ("../examples/test_09_two_fsms.ev", "consumer", "effects \u{2208} Seq(Effect) | last_results \u{2208} Seq(Result) | state_next \u{2208} CState"),
        ("../examples/test_09_two_fsms.ev", "producer", "effects \u{2208} Seq(Effect) | state_next \u{2208} PState"),
        ("../examples/test_14_stdin.ev", "echo", "effects \u{2208} Seq(Effect) | state_next \u{2208} EState"),
        ("../examples/test_15_signal.ev", "guard", "effects \u{2208} Seq(Effect) | state_next \u{2208} SState"),
        ("../examples/test_18_reflection.ev", "reflect_demo", "effects \u{2208} Seq(Effect) | state_next \u{2208} RState"),
        ("../examples/test_21_mario/main.ev", "display", "_frame \u{2208} Int | _world \u{2208} World | is_first_tick \u{2208} Bool"),
        ("../examples/test_21_mario/main.ev", "game", "_game_clock \u{2208} Int | _world \u{2208} World | effects \u{2208} Seq(Effect) | is_first_tick \u{2208} Bool"),
        ("../examples/test_21_mario/main.ev", "keyboard", "_kb_frame \u{2208} Int | _world \u{2208} World | effects \u{2208} Seq(Effect) | is_first_tick \u{2208} Bool | last_results \u{2208} Seq(Result)"),
        ("../examples/test_25_per_component_jit.ev", "sim", "effects \u{2208} Seq(Effect)"),
        ("../examples/test_26_value_cache.ev", "driver", "_n \u{2208} Int | effects \u{2208} Seq(Effect) | is_first_tick \u{2208} Bool"),
        ("../examples/test_26_value_cache.ev", "expensive", "effects \u{2208} Seq(Effect)"),
        ("../examples/test_30_jit_gap_closures.ev", "gaps", "_world \u{2208} World | effects \u{2208} Seq(Effect) | is_first_tick \u{2208} Bool | state_next \u{2208} Phase"),
        ("../examples/test_31_symbolic_regression.ev", "regressor", "effects \u{2208} Seq(Effect)"),
        ("../examples/test_32_llm_functionizer.ev", "classifier", "effects \u{2208} Seq(Effect) | state_next \u{2208} CState"),
        ("../examples/test_32_llm_functionizer.ev", "printer", "effects \u{2208} Seq(Effect) | state_next \u{2208} PState"),
    ];

    /// The self-hosted fsm_params + prev_tick reproduce the canonical Rust
    /// injection set on every FSM in the corpus. (Order within a body is not
    /// part of the contract — prev_tick is HashMap-order-nondeterministic in
    /// the canonical pass too — so this compares the gained-membership set,
    /// `name ∈ type`, as a sorted multiset.)
    #[test]
    fn matches_golden_on_corpus() {
        let ev = evident_impl();
        let mut by_file: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for (file, name, want) in GOLDEN {
            by_file.entry(file).or_default().push((name, want));
        }
        let mut checked = 0;
        for (file, wants) in &by_file {
            let path = Path::new(file);
            assert!(path.exists(), "corpus file {file} not found; update GOLDEN");
            let src = std::fs::read_to_string(path).unwrap();
            let prog = crate::parser::parse(&src)
                .unwrap_or_else(|e| panic!("parse {file}: {e}"));
            for (name, want) in wants {
                let raw = prog.schemas.iter().find(|s| &s.name == name)
                    .unwrap_or_else(|| panic!("{file}: no schema `{name}`"));
                let got = injected_names(&ev, raw).join(" | ");
                assert_eq!(&got, want, "{file}::{name} injection diverged from golden");
                checked += 1;
            }
        }
        assert_eq!(checked, GOLDEN.len());
    }

    /// Every FSM the golden does NOT list gains nothing (the two sub-passes
    /// are conservative: a non-fsm, or an fsm with no canonical-slot /
    /// `_var` reference, is untouched). Guards against over-injection.
    #[test]
    fn non_golden_fsms_untouched() {
        let ev = evident_impl();
        let golden_keys: HashSet<(&str, &str)> =
            GOLDEN.iter().map(|(f, n, _)| (*f, *n)).collect();
        let files: HashSet<&str> = GOLDEN.iter().map(|(f, _, _)| *f).collect();
        for file in files {
            let src = std::fs::read_to_string(file).unwrap();
            let prog = crate::parser::parse(&src).unwrap();
            for raw in &prog.schemas {
                if golden_keys.contains(&(file, raw.name.as_str())) { continue; }
                let got = injected_names(&ev, raw);
                assert!(got.is_empty(),
                    "{file}::{} unexpectedly gained {got:?}", raw.name);
            }
        }
    }

    /// The self-hosted walk reaches exactly the identifiers the canonical
    /// walk reached: a hand-built FSM body referencing `state_next` /
    /// `effects` / a `_prev` var gets the same memberships.
    #[test]
    fn walk_reaches_canonical_identifiers() {
        use crate::core::ast::{BinOp, Program};
        // fsm f(state ∈ S) :  state_next = state ;  out = _count + 1
        let body = vec![
            mem("state", "S"),
            mem("count", "Int"),
            BodyItem::Constraint(Expr::Binary(BinOp::Eq,
                Box::new(Expr::Identifier("state_next".into())),
                Box::new(Expr::Identifier("state".into())))),
            BodyItem::Constraint(Expr::Binary(BinOp::Eq,
                Box::new(Expr::Identifier("out".into())),
                Box::new(Expr::Binary(BinOp::Add,
                    Box::new(Expr::Identifier("_count".into())),
                    Box::new(Expr::Int(1)))))),
        ];
        let raw = SchemaDecl {
            keyword: Keyword::Fsm, name: "f".into(), type_params: vec![],
            body, param_count: 1, external: false,
        };
        let _ = Program { schemas: vec![raw.clone()], imports: vec![], enums: vec![] };
        let got = injected_names(&evident_impl(), &raw);
        // state_next ∈ S, _count ∈ Int, is_first_tick ∈ Bool.
        for want in ["state_next \u{2208} S", "_count \u{2208} Int", "is_first_tick \u{2208} Bool"] {
            assert!(got.iter().any(|g| g == want),
                "expected `{want}` injected; got {got:?}");
        }
        assert_eq!(got.len(), 3, "exactly three injected; got {got:?}");
    }

    /// The `fsm_params_build` FSM constructs + returns a `BodyItemList` whose
    /// strings the Rust decode path recovers intact — the composite-AST-
    /// return half. (The Evident-side structural `=` check lives inline in
    /// `inject.ev`'s `sat_build_*` claims, which GAPB's #18 fix made
    /// faithful.)
    #[test]
    fn fsm_params_build_decodes_faithfully() {
        let ev = evident_impl();
        let seed = fpb_input(true, false, true, false, true, false, "GameState", true);
        let got = ev.run_build("fsm_params_build", seed, "FPBDone");
        let pairs: Vec<(String, String)> = got.iter().filter_map(|i| match i {
            BodyItem::Membership { name, type_name, .. } => Some((name.clone(), type_name.clone())),
            _ => None,
        }).collect();
        assert_eq!(pairs, vec![
            ("state_next".to_string(), "GameState".to_string()),
            ("last_results".to_string(), "Seq(Result)".to_string()),
            ("effects".to_string(), "Seq(Effect)".to_string()),
        ]);
    }

    #[test]
    fn impl_name_is_evident() {
        assert_eq!(evident_impl().impl_name(), "evident");
    }

    fn mem(name: &str, type_name: &str) -> BodyItem {
        BodyItem::Membership {
            name: name.to_string(), type_name: type_name.to_string(), pins: Pins::None,
        }
    }
}
