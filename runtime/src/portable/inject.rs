//! `inject` — FSM-aware membership injection, driven by the [`super`] swap
//! interface. This is the runtime's **biggest** pure pass
//! (`runtime/src/runtime/inject.rs`, ~590 LOC) and the first one whose
//! natural output is a *rewritten AST*, not a name-set. Self-hosting it
//! tested whether the proven subscriptions recipe (Evident stack-FSM walk
//! over the shared marshaler + `run()`) extends to AST-rewriting passes.
//!
//! ## What self-hosts, and what stays in Rust (the honest split)
//!
//! `inject.rs` has four sub-passes. Two are self-contained — they decide
//! what to inject from *one claim's body alone*:
//!
//!   * `inject_fsm_params`      — inject `state_next` / `last_results` /
//!                                `effects` when referenced + undeclared.
//!   * `inject_prev_tick_decls` — inject `_var` time-shift slots +
//!                                `is_first_tick`.
//!
//! Both rest on the SAME recursive expression walk — collect every
//! referenced identifier — then a small construction step. The walk is
//! self-hosted in `stdlib/passes/inject.ev` (`inject_collect`), driven here
//! exactly like `subscriptions`: marshal each constraint / claim-call expr
//! with the shared [`expr_to_value`], run the stack-FSM to a drained-stack
//! `IWDone`, decode the cons-list of raw identifier strings. The Rust shim
//! does only the parts Evident can't express — the `_`-strip / first-segment
//! split (no substring operator) and the membership *construction*.
//!
//! The other two sub-passes stay in Rust (delegated to the canonical impl):
//!
//!   * `inject_claim_arg_types` — type a fresh positional arg from the
//!                                *called claim's* signature.
//!   * `inject_lhs_eq_types`    — infer an `lhs = expr` type via field
//!                                chains + enum-variant lookup.
//!
//! Both resolve a name's type against the WHOLE-PROGRAM schema table +
//! enum registry — not one body, but every loaded claim and enum.
//! Marshaling all of that into the FSM per claim is a composite-INPUT
//! blow-up the `run()`/marshaler recipe doesn't faithfully support yet, so
//! they keep their native impl. See [`docs/self-hosting.md`] +
//! `examples/COUNTEREXAMPLES.md`.
//!
//! ## Why the rewritten AST isn't returned through `run()` (the gap)
//!
//! The tempting full self-host has `inject_collect`'s sibling
//! `fsm_params_build` *return the rewritten body* — a `BodyItemList` of
//! `BIMembership` nodes — through `run()`, decoded by the shared
//! `decode_list`. Two gaps block making that the cutover path:
//!
//!   1. **The marshaler drops `param_count`.** `inject` inserts at
//!      `SchemaDecl::param_count`, but the shared `encode_ast`/`decode_ast`
//!      bridge intentionally omits that field (it has no slot in
//!      `MakeSchemaDecl`). A whole-`SchemaDecl` round-trip therefore can't
//!      preserve the one number `inject`'s insertion depends on.
//!   2. **Deep string payloads don't round-trip Evident `=`.** A
//!      `BodyItemList` carrying string-bearing `BIMembership` nodes,
//!      returned through `run()`, round-trips its *variant* but fails
//!      Evident-level structural `=` on the nested strings (gap #18's
//!      family). The Rust *decode* path recovers correct values (the
//!      `fsm_params_build_decode_*` tests show this), so the FSM CAN
//!      construct + return AST — but Evident can't self-verify it, so the
//!      construction's faithfulness rests on a Rust assertion either way.
//!
//! So `EvidentInject` self-hosts the WALK (the structural bulk of
//! `fsm_params` + `prev_tick`) and does the construction + insertion in
//! Rust glue, exactly the emit-raw-data / Rust-decide split `subscriptions`
//! uses. Faithfulness is proven byte-for-byte against [`RustInject`] (which
//! is the production pipeline — it calls the canonical functions verbatim)
//! by the `equivalence_*` tests.
//!
//! ## Selecting an impl
//!
//! Construction is selection. [`default_impl`] returns the Rust impl unless
//! `EVIDENT_INJECT_IMPL=evident`. The production load path
//! (`runtime/src/runtime/load.rs`) is UNCHANGED — it calls the canonical
//! `inject_*` directly — so this port adds no per-tick cost and isn't the
//! default. `inject` is a load-time pass; per-tick runtime is untouched.

use std::collections::HashMap;
use std::path::Path;

use crate::core::ast::{BodyItem, Expr, Keyword, Pins, SchemaDecl};
use crate::core::{EnumRegistry, Value};
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_list, decode_str};
use crate::translate::ast_encoder::expr_to_value;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `inject`'s Rust-level signature, independent of which impl backs it.
/// Runs the full four-sub-pass injection pipeline on one schema in the
/// same order as the load path. `schemas` / `enums` give the
/// whole-program context the type-inference sub-passes need.
pub trait InjectImpl: Portable {
    fn inject(
        &self,
        s: &mut SchemaDecl,
        schemas: &HashMap<String, SchemaDecl>,
        enums: &EnumRegistry,
    );
}

// ─────────────────────────────────────────────────────────────────────
// Rust impl — IS the production pipeline (calls the canonical fns)
// ─────────────────────────────────────────────────────────────────────

/// Native injector. Calls `runtime::inject`'s canonical functions in the
/// exact order `runtime/src/runtime/load.rs` does — so `RustInject` is
/// byte-identical to production, and doubles as the reference the Evident
/// impl is proven against. Total, fast, always correct — the default.
pub struct RustInject;

impl Portable for RustInject {
    fn impl_name(&self) -> &'static str { "rust" }
}

impl InjectImpl for RustInject {
    fn inject(
        &self,
        s: &mut SchemaDecl,
        schemas: &HashMap<String, SchemaDecl>,
        enums: &EnumRegistry,
    ) {
        let _ = crate::runtime::inject::inject_fsm_params(s);
        crate::runtime::inject::inject_lhs_eq_types(s, schemas, enums);
        let _ = crate::runtime::inject::inject_prev_tick_decls(s);
        let _ = crate::runtime::inject::inject_claim_arg_types(s, schemas);
    }
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — self-hosted walk for fsm_params + prev_tick
// ─────────────────────────────────────────────────────────────────────

/// Pass-driven injector. Holds an [`EvidentRuntime`] with
/// `stdlib/passes/inject.ev` loaded; drives `inject_collect` (the
/// reference-collection stack-FSM) for the two self-contained sub-passes
/// and delegates the two whole-program-table sub-passes to the canonical
/// Rust impl (see the module docs for why). Build once and reuse — the
/// FSM's per-tick solve is JIT-cached across calls.
pub struct EvidentInject {
    rt: EvidentRuntime,
}

impl EvidentInject {
    /// The reference-collection walk FSM.
    const WALK_FSM: &'static str = "inject_collect";
    /// The (gap-documented, test-only) composite-AST-return demo FSM.
    #[cfg(test)]
    const BUILD_FSM: &'static str = "ast_return_demo";

    /// Max-iteration guard for the nested walk. One expr node costs a
    /// small constant number of FSM ticks; the cap is far above any
    /// realistic claim, so a legitimate walk never hits it.
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/inject.ev` from `stdlib_dir` into a fresh runtime.
    /// The pass is self-contained (it declares its own cons-list copy of
    /// the AST enums matching the shared marshaler), so no other stdlib
    /// file is needed.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("inject.ev"))
            .map_err(|e| format!("load passes/inject.ev: {e}"))?;
        Ok(Self { rt })
    }

    // ── The walk: collect every referenced identifier in a body ──
    //
    // Mirrors the invocation structure of inject.rs's `walk` callers EXACTLY:
    // it visits constraint exprs and claim-call mapping VALUES (not
    // memberships, pins, subclaims, passthroughs, or halts_within). Each
    // expr is marshaled with the shared `expr_to_value` and driven through
    // `inject_collect` to a drained-stack `IWDone(NameList)`; the cons-list
    // of raw identifier strings is decoded with the shared `decode_list`.
    // The union (with duplicates) of all returned strings is the same set
    // inject.rs's Rust walk would reach — so the downstream decisions are
    // byte-identical.
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
                match decode_list(&fields[0], "NameList", "NameNil", "NameCons", decode_str) {
                    Ok(names) => out.extend(names),
                    Err(e) => eprintln!("[inject/evident] decode of name list failed: {e}"),
                }
            }
            Ok(other) => eprintln!("[inject/evident] walk returned non-IWDone: {other:?}"),
            Err(e) => eprintln!("[inject/evident] walk failed: {e}"),
        }
    }

    // ── inject_fsm_params, with the self-hosted walk swapped in ──
    //
    // Mirrors `runtime::inject::inject_fsm_params` 1:1 apart from sourcing
    // the referenced-name set from `collect_refs` (the FSM) instead of an
    // inline Rust tree walk.
    fn inject_fsm_params(&self, s: &mut SchemaDecl) {
        if !matches!(s.keyword, Keyword::Fsm) { return; }
        if s.external { return; }

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

        let refs = self.collect_refs(&s.body);
        let ref_state_next   = refs.iter().any(|n| n == "state_next");
        let ref_last_results = refs.iter().any(|n| n == "last_results");
        let ref_effects      = refs.iter().any(|n| n == "effects");

        let mut injected: Vec<BodyItem> = Vec::new();
        if !have_state_next && ref_state_next {
            if let Some(st) = &state_type {
                injected.push(membership("state_next", st));
            }
        }
        if !have_last_results && ref_last_results {
            injected.push(membership("last_results", "Seq(Result)"));
        }
        if !have_effects && ref_effects {
            injected.push(membership("effects", "Seq(Effect)"));
        }
        let insert_pos = s.param_count;
        for (i, item) in injected.into_iter().enumerate() {
            s.body.insert(insert_pos + i, item);
        }
    }

    // ── inject_prev_tick_decls, with the self-hosted walk swapped in ──
    //
    // Mirrors `runtime::inject::inject_prev_tick_decls` 1:1 apart from
    // sourcing the referenced identifiers from `collect_refs`. The
    // `_`-strip + first-segment split stays in Rust (no substring op in
    // Evident — the same honest split subscriptions.ev documents).
    fn inject_prev_tick_decls(&self, s: &mut SchemaDecl) {
        if !matches!(s.keyword, Keyword::Fsm) { return; }
        if s.external { return; }

        let mut declared: HashMap<String, String> = HashMap::new();
        for item in &s.body {
            if let BodyItem::Membership { name, type_name, .. } = item {
                declared.insert(name.clone(), type_name.clone());
            }
        }

        let refs = self.collect_refs(&s.body);
        let mut prev_refs: HashMap<String, String> = HashMap::new();
        for n in &refs {
            let Some(after_underscore) = n.strip_prefix('_') else { continue };
            let first_seg = after_underscore.split('.').next().unwrap_or(after_underscore);
            if let Some(ty) = declared.get(first_seg) {
                prev_refs.insert(format!("_{first_seg}"), ty.clone());
            }
        }

        if prev_refs.is_empty() { return; }

        let mut to_inject: Vec<BodyItem> = Vec::new();
        for (prev_name, ty) in &prev_refs {
            if !declared.contains_key(prev_name) {
                to_inject.push(membership(prev_name, ty));
            }
        }
        if !declared.contains_key("is_first_tick") {
            to_inject.push(membership("is_first_tick", "Bool"));
        }
        let insert_pos = s.param_count;
        for (i, item) in to_inject.into_iter().enumerate() {
            s.body.insert(insert_pos + i, item);
        }
    }

    /// Drive `ast_return_demo` (the composite-AST-return demonstration) and
    /// DECODE its returned `BodyItemList` into raw `(name, type)` pairs by
    /// walking the cons-list `Value` directly. Shows the FSM genuinely
    /// constructs + returns AST whose string VALUES the Rust decode path
    /// recovers — composite AST return IS faithful (see module docs).
    #[cfg(test)]
    fn ast_return_demo_decoded(&self) -> Vec<(String, String)> {
        // Seed is a bare Bool; coerce_init wraps it into FPBInit(Bool).
        let final_state = crate::effect_loop::run_nested(
            &self.rt, Self::BUILD_FSM, Value::Bool(true), Self::MAX_STEPS,
        ).expect("ast_return_demo run");
        let Value::Enum { variant, fields, .. } = &final_state else {
            panic!("expected FPBDone enum, got {final_state:?}");
        };
        assert_eq!(variant, "FPBDone");
        decode_membership_specs(&fields[0])
    }
}

impl Portable for EvidentInject {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl InjectImpl for EvidentInject {
    fn inject(
        &self,
        s: &mut SchemaDecl,
        schemas: &HashMap<String, SchemaDecl>,
        enums: &EnumRegistry,
    ) {
        self.inject_fsm_params(s);                                          // self-hosted walk
        crate::runtime::inject::inject_lhs_eq_types(s, schemas, enums);     // canonical (gap)
        self.inject_prev_tick_decls(s);                                     // self-hosted walk
        let _ = crate::runtime::inject::inject_claim_arg_types(s, schemas); // canonical (gap)
    }
}

// ─────────────────────────────────────────────────────────────────────
// Small helpers
// ─────────────────────────────────────────────────────────────────────

/// A bare `name ∈ type_name` membership — the only body item `inject`
/// ever constructs.
fn membership(name: &str, type_name: &str) -> BodyItem {
    BodyItem::Membership {
        name: name.to_string(),
        type_name: type_name.to_string(),
        pins: Pins::None,
    }
}

/// Wrap an already-marshaled `Expr` `Value` as the FSM's `Work` node.
fn work_expr(inner: Value) -> Value {
    Value::Enum {
        enum_name: "Work".to_string(),
        variant: "WExpr".to_string(),
        fields: vec![inner],
    }
}

/// Walk a returned `BodyItemList` cons-list `Value` and pull out each
/// `BIMembership`'s `(name, type)` strings. Used by the gap-probe tests.
#[cfg(test)]
fn decode_membership_specs(v: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur = v;
    loop {
        let Value::Enum { variant, fields, .. } = cur else { break };
        match variant.as_str() {
            "BILNil" => break,
            "BILCons" => {
                if let Value::Enum { variant: bv, fields: bf, .. } = &fields[0] {
                    if bv == "BIMembership" {
                        let name = if let Value::Str(s) = &bf[0] { s.clone() } else { String::new() };
                        let ty   = if let Value::Str(s) = &bf[1] { s.clone() } else { String::new() };
                        out.push((name, ty));
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
// Selection
// ─────────────────────────────────────────────────────────────────────

/// Pick an impl by `EVIDENT_INJECT_IMPL` (`rust` | `evident`), defaulting
/// to the Rust impl. `evident` locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver; if locating or loading
/// fails it falls back to Rust. The production load path does NOT use this
/// — it calls the canonical functions directly, so the default has no
/// effect on the runtime; this seam is for cross-validation + opt-in.
pub fn default_impl() -> Box<dyn InjectImpl> {
    if std::env::var("EVIDENT_INJECT_IMPL").as_deref() == Ok("evident") {
        if let Ok(dir) = crate::stdlib_path::stdlib_dir() {
            if let Ok(ev) = EvidentInject::new(&dir) {
                return Box::new(ev);
            }
        }
    }
    Box::new(RustInject)
}

// ─────────────────────────────────────────────────────────────────────
// Tests — equivalence vs the canonical pipeline, on the examples corpus
// ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const STDLIB: &str = "../stdlib";

    fn evident_impl() -> EvidentInject {
        EvidentInject::new(Path::new(STDLIB)).expect("load stdlib/passes/inject.ev")
    }

    /// Build the `(schemas_map, enums)` context the type-inference
    /// sub-passes consult, from a parsed program. `by_variant` is
    /// populated exactly as `register_enums` does (no Z3 needed — `inject`
    /// only reads `by_variant`). The schema map is the pre-inject parsed
    /// schemas. Both impls receive the same context, so equivalence holds
    /// regardless of how complete it is; this makes it realistic.
    fn context(prog: &crate::core::ast::Program)
        -> (HashMap<String, SchemaDecl>, EnumRegistry)
    {
        let schemas: HashMap<String, SchemaDecl> =
            prog.schemas.iter().map(|s| (s.name.clone(), s.clone())).collect();
        let enums = EnumRegistry::new();
        {
            let mut bv = enums.by_variant.borrow_mut();
            for ed in &prog.enums {
                for (i, v) in ed.variants.iter().enumerate() {
                    bv.insert(v.name.clone(), (ed.name.clone(), i));
                }
            }
        }
        (schemas, enums)
    }

    /// Canonical comparison key: a body as a SORTED multiset of its
    /// per-item debug strings. `inject_prev_tick_decls` inserts the
    /// `_var` memberships in `HashMap` iteration order, so the *position*
    /// of injected items is not part of the contract (the canonical Rust
    /// pass is itself order-nondeterministic there); multiset equality is
    /// the right equivalence relation. fsm_params / lhs_eq / claim_arg all
    /// inject deterministically.
    fn body_multiset(s: &SchemaDecl) -> Vec<String> {
        let mut v: Vec<String> = s.body.iter().map(|i| format!("{i:?}")).collect();
        v.sort();
        v
    }

    /// The corpus of `.ev` files (relative to the `runtime/` package dir,
    /// the cwd cargo uses for tests). Mirrors what the demo runner covers.
    const CORPUS: &[&str] = &[
        "../examples/test_09_two_fsms.ev",
        "../examples/test_14_stdin.ev",
        "../examples/test_15_signal.ev",
        "../examples/test_18_reflection.ev",
        "../examples/test_25_per_component_jit.ev",
        "../examples/test_26_value_cache.ev",
        "../examples/test_30_jit_gap_closures.ev",
        "../examples/test_31_symbolic_regression.ev",
        "../examples/test_32_llm_functionizer.ev",
        "../examples/test_21_mario/main.ev",
    ];

    /// EvidentInject and RustInject produce byte/multiset-identical bodies
    /// for every schema across the corpus. RustInject IS the production
    /// pipeline (it calls the canonical `inject_*`), so this proves the
    /// self-hosted walk is faithful to production.
    #[test]
    fn equivalence_on_corpus() {
        let ev = evident_impl();
        let rust = RustInject;
        let mut checked_schemas = 0;
        let mut checked_fsms = 0;
        for file in CORPUS {
            let path = Path::new(file);
            assert!(path.exists(), "corpus file {file} not found; update CORPUS");
            let src = std::fs::read_to_string(path).unwrap();
            let prog = crate::parser::parse(&src)
                .unwrap_or_else(|e| panic!("parse {file}: {e}"));
            let (schemas, enums) = context(&prog);
            for raw in &prog.schemas {
                let mut a = raw.clone();
                let mut b = raw.clone();
                rust.inject(&mut a, &schemas, &enums);
                ev.inject(&mut b, &schemas, &enums);
                assert_eq!(body_multiset(&a), body_multiset(&b),
                    "{file}::{} diverged\n  rust={:#?}\n  evident={:#?}",
                    raw.name, a.body, b.body);
                checked_schemas += 1;
                if matches!(raw.keyword, Keyword::Fsm) { checked_fsms += 1; }
            }
        }
        assert!(checked_schemas >= 20, "expected ≥20 schemas; checked {checked_schemas}");
        assert!(checked_fsms >= 5, "expected ≥5 fsm schemas; checked {checked_fsms}");
    }

    /// The self-hosted walk reaches exactly the identifiers the canonical
    /// Rust walk reaches: on a hand-built FSM body that references
    /// `state_next` / `effects` / a `_prev` var, EvidentInject injects the
    /// same memberships RustInject does. (A focused unit-level check that
    /// complements the corpus sweep.)
    #[test]
    fn walk_reaches_canonical_identifiers() {
        use crate::core::ast::{BinOp, Program};
        // fsm f(state ∈ S) :  state_next = state ;  out = _count + 1
        let body = vec![
            membership("state", "S"),
            membership("count", "Int"),
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
        let prog = Program { schemas: vec![raw.clone()], imports: vec![], enums: vec![] };
        let (schemas, enums) = context(&prog);

        let mut a = raw.clone();
        let mut b = raw.clone();
        RustInject.inject(&mut a, &schemas, &enums);
        evident_impl().inject(&mut b, &schemas, &enums);
        assert_eq!(body_multiset(&a), body_multiset(&b));
        // Concretely: state_next ∈ S, _count ∈ Int, is_first_tick ∈ Bool injected.
        let names: HashSet<String> = b.body.iter().filter_map(|i| match i {
            BodyItem::Membership { name, .. } => Some(name.clone()),
            _ => None,
        }).collect();
        for want in ["state_next", "_count", "is_first_tick"] {
            assert!(names.contains(want), "expected `{want}` injected; got {names:?}");
        }
    }

    // ── Composite-AST-return gap characterisation ──
    //
    // `fsm_params_build` constructs a `BodyItemList` of `BIMembership`
    // nodes and returns it through `run()`. These tests show the Rust
    // DECODE path recovers correct string values (so the FSM genuinely
    // produces usable AST) — while the matching inline Evident `=` claims
    // were REMOVED from inject.ev because structural `=` on the nested
    // strings is unfaithful (the gap; see module docs + COUNTEREXAMPLES).

    /// Composite AST RETURN is faithful: `ast_return_demo` constructs a
    /// two-element `BodyItemList` of string-bearing `BIMembership` nodes
    /// and returns it through `run()`; the Rust decode path recovers it
    /// with correct strings + structure. This is the headline finding —
    /// returning a rewritten-AST fragment through `run()` works. (What
    /// blocks a FULL AST-returning inject cutover is NOT this: it's the
    /// marshaler's `param_count` drop + gap #18 on in-FSM DECISIONS. See
    /// module docs.)
    #[test]
    fn ast_return_is_faithful() {
        let got = evident_impl().ast_return_demo_decoded();
        assert_eq!(got, vec![
            ("state_next".to_string(), "GameState".to_string()),
            ("effects".to_string(), "Seq(Effect)".to_string()),
        ], "the FSM-constructed, run()-returned BodyItemList must decode with intact strings");
    }

    #[test]
    fn impl_name_is_evident() {
        assert_eq!(evident_impl().impl_name(), "evident");
        assert_eq!(RustInject.impl_name(), "rust");
    }
}
