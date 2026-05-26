//! `inject` — FSM-aware membership injection. **Partial cutover (session
//! REVIVE-inject):** two of inject's four sub-passes self-host in Evident;
//! the other two stay in Rust pending Gap D.
//!
//! ## The four sub-passes, and the split
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
//! The first two decide what to inject from **one claim's body alone** and cut
//! over here. The last two resolve a name's type against the **whole-program
//! schema table + enum registry** — a composite-INPUT blow-up the marshaler
//! recipe doesn't faithfully support yet (Gap D, COUNTEREXAMPLES #27). They
//! keep their canonical Rust impl (`crate::runtime::inject::*`), called
//! directly from the load path; this shim never touches them.
//!
//! ## What runs where, for the two cut-over sub-passes
//!
//! `stdlib/passes/inject.ev` self-hosts THREE FSMs:
//!   * `inject_collect`    — the recursive reference-collection WALK.
//!   * `fsm_params_build`  — reads the six inject DECISIONS as marshaled-in
//!                           `Bool`s, COMPUTES `(reff ∧ ¬heff)` etc. on the
//!                           destructured payloads (the #18 keystone), and
//!                           CONSTRUCTS the `BodyItemList` to inject.
//!   * `prev_tick_build`   — maps the `(_var, type)` pairs to memberships +
//!                           conditionally prepends `is_first_tick`.
//!
//! This shim does only the parts Evident can't / shouldn't express:
//!   1. **String-set membership stays in Rust.** "Is `state_next` reachable?
//!      already declared?" and the `_`-strip / first-segment split for `_var`s
//!      are string comparisons, done over the walk's output here — never in
//!      the FSM (the in-solve string-theory blow-up). The FSM gets BOOLEANS.
//!   2. **The splice happens here, at `s.param_count`.** The `*_build` FSMs
//!      return only the *new* memberships; this shim inserts them at
//!      `s.param_count` (in hand from the `SchemaDecl`), so existing body items
//!      never round-trip the marshaler.
//!
//! Production calls the free [`fsm_params`] / [`prev_tick`], which drive the
//! shared per-thread [`EvidentRunner`]. `inject` is a load-time pass —
//! per-tick runtime is untouched.

use std::collections::HashMap;

use super::{run_done_payload, run_name_list, work_node, EvidentRunner};
use crate::core::ast::{BodyItem, Expr, Keyword, Pins, SchemaDecl};
use crate::core::Value;
use crate::translate::ast_encoder::expr_to_value;

guarded_runner!(runner, "passes/inject.ev", "inject_collect");

// ─────────────────────────────────────────────────────────────────────
// The walk: collect every referenced identifier in a body
// ─────────────────────────────────────────────────────────────────────

/// Visit constraint exprs and claim-call mapping VALUES (the exact set the
/// canonical walk reached); each expr is marshaled and driven through
/// `inject_collect` to a drained-stack `IWDone(NameList)`.
fn collect_refs(runner: &EvidentRunner, body: &[BodyItem]) -> Vec<String> {
    let mut out = Vec::new();
    for item in body {
        match item {
            BodyItem::Constraint(e) => walk_expr(runner, e, &mut out),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings {
                    walk_expr(runner, &m.value, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

fn walk_expr(runner: &EvidentRunner, e: &Expr, out: &mut Vec<String>) {
    let seed = work_node("Work", "WExpr", expr_to_value(e));
    out.extend(run_name_list(runner, "inject_collect", seed, "IWDone", "inject/evident"));
}

// ─────────────────────────────────────────────────────────────────────
// inject_fsm_params — walk (Evident) + string-eq (Rust) +
//   decision/construction (Evident fsm_params_build) + splice (Rust)
// ─────────────────────────────────────────────────────────────────────

/// Mirrors the deleted `runtime::inject::inject_fsm_params`: the referenced-
/// name set comes from the FSM walk, and the referenced/undeclared booleans
/// drive the `fsm_params_build` construction FSM instead of an inline Rust
/// `if` chain.
fn fsm_params_impl(runner: &EvidentRunner, s: &mut SchemaDecl) {
    // Declared-membership scan (state type + which canonical slots exist).
    let mut state_type: Option<String> = None;
    let mut have_state_next = false;
    let mut have_last_results = false;
    let mut have_effects = false;
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            match name.as_str() {
                "state" if state_type.is_none() => state_type = Some(type_name.clone()),
                "state_next" => have_state_next = true,
                "last_results" => have_last_results = true,
                "effects" => have_effects = true,
                _ => {}
            }
        }
    }

    // String-set membership: which slots are reachable (in Rust, off the
    // walk's output — never compared inside the solve).
    let refs = collect_refs(runner, &s.body);
    let ref_state_next = refs.iter().any(|n| n == "state_next");
    let ref_last_results = refs.iter().any(|n| n == "last_results");
    let ref_effects = refs.iter().any(|n| n == "effects");

    // Hand the six decisions + the state type to the construction FSM, which
    // computes `(r ∧ ¬h …)` on the destructured Bools and returns the
    // memberships to inject.
    let seed = fpb_input(
        ref_state_next, have_state_next,
        ref_last_results, have_last_results,
        ref_effects, have_effects,
        state_type.as_deref().unwrap_or(""), state_type.is_some(),
    );
    let injected = run_build(runner, "fsm_params_build", seed, "FPBDone");
    splice_at(s, injected);
}

// ─────────────────────────────────────────────────────────────────────
// inject_prev_tick_decls — walk (Evident) + `_`-strip/lookup (Rust) +
//   construction (Evident prev_tick_build) + splice (Rust)
// ─────────────────────────────────────────────────────────────────────

/// Mirrors the deleted `runtime::inject::inject_prev_tick_decls`. The
/// `_`-strip + first-segment split + declared-name lookup stays in Rust
/// (substring ops Evident lacks); the resulting `(_var, type)` pairs +
/// is_first_tick flag drive the `prev_tick_build` FSM.
fn prev_tick_impl(runner: &EvidentRunner, s: &mut SchemaDecl) {
    let mut declared: HashMap<String, String> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declared.insert(name.clone(), type_name.clone());
        }
    }

    let refs = collect_refs(runner, &s.body);
    // `_count` → strip → `count`; `_pos.x` → strip → first segment `pos`.
    let mut prev_refs: HashMap<String, String> = HashMap::new();
    for n in &refs {
        let Some(after_underscore) = n.strip_prefix('_') else { continue };
        let first_seg = after_underscore.split('.').next().unwrap_or(after_underscore);
        if let Some(ty) = declared.get(first_seg) {
            prev_refs.insert(format!("_{first_seg}"), ty.clone());
        }
    }
    // No `_var` reference at all → inject nothing (not even is_first_tick).
    if prev_refs.is_empty() {
        return;
    }

    // Pairs to inject = referenced prev-vars not already declared;
    // is_first_tick added once unless the user declared it.
    let pairs: Vec<(String, String)> = prev_refs.into_iter()
        .filter(|(name, _)| !declared.contains_key(name))
        .collect();
    let add_first_tick = !declared.contains_key("is_first_tick");

    let seed = ptb_input(&pairs, add_first_tick);
    let injected = run_build(runner, "prev_tick_build", seed, "PTBDone");
    splice_at(s, injected);
}

/// Drive a `*_build` FSM to its `<done>(BodyItemList)` halt and decode the
/// returned cons-list into `BodyItem::Membership`s. Empty on any failure.
fn run_build(runner: &EvidentRunner, fsm: &str, seed: Value, done_variant: &str) -> Vec<BodyItem> {
    run_done_payload(runner, fsm, seed, done_variant, "inject/evident")
        .map(|p| decode_membership_list(&p))
        .unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────
// Production entry points
// ─────────────────────────────────────────────────────────────────────

/// The runtime's own self-hosted-pass FSMs. They live in `stdlib/passes/`,
/// are written in the terse `_state` form that `unify_state_syntax` rewrites
/// before these hooks run, and reference no implicit slot, so injection is a
/// guaranteed no-op for them. Skipping them by name is the cheap correct
/// answer AND breaks a per-load cross-engine cascade: every load runs
/// validate's hook, which loads `validate.ev` (declaring `fsm validate_walk`);
/// without this skip, processing `validate_walk` would build the heavier
/// inject engine on every load, even for programs with no FSM of their own.
fn is_self_hosted_pass_fsm(name: &str) -> bool {
    matches!(
        name,
        "inject_collect" | "fsm_params_build" | "prev_tick_build"
            | "validate_walk" | "subscriptions_walk" | "pretty_walk"
    )
}

/// `inject_fsm_params`, self-hosted. The load path's sole entry point for the
/// `state_next` / `last_results` / `effects` injection.
///
/// No-op for non-`fsm` / `external` schemas and for the runtime's own pass
/// FSMs, checked HERE before the runner is built — a program with no (user)
/// FSM never pays the one-time inject-engine build, and the cross-engine
/// cascade is avoided.
pub fn fsm_params(s: &mut SchemaDecl) {
    if !matches!(s.keyword, Keyword::Fsm) || s.external {
        return;
    }
    if is_self_hosted_pass_fsm(&s.name) {
        return;
    }
    let Some(runner) = runner() else { return };
    fsm_params_impl(&runner, s);
}

/// `inject_prev_tick_decls`, self-hosted. The load path's sole entry point for
/// the `_var` time-shift + `is_first_tick` injection.
pub fn prev_tick(s: &mut SchemaDecl) {
    if !matches!(s.keyword, Keyword::Fsm) || s.external {
        return;
    }
    if is_self_hosted_pass_fsm(&s.name) {
        return;
    }
    let Some(runner) = runner() else { return };
    prev_tick_impl(&runner, s);
}

// ─────────────────────────────────────────────────────────────────────
// Small helpers — seeds in, memberships out, splice
// ─────────────────────────────────────────────────────────────────────

/// Splice `items` into `s.body` at `s.param_count` (the first-line-param
/// insertion index). Kept in Rust because the index is right here in the
/// `SchemaDecl`; the existing body never round-trips.
fn splice_at(s: &mut SchemaDecl, items: Vec<BodyItem>) {
    let insert_pos = s.param_count;
    for (i, item) in items.into_iter().enumerate() {
        s.body.insert(insert_pos + i, item);
    }
}

/// Pack the six fsm-params decisions + state type into `MakeFPBInput(...)` —
/// `run_nested`'s coerce wraps it into `FPBInit(FPBInput)`.
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

/// Walk a returned `BodyItemList` cons-list `Value` (`BILNil` / `BILCons`) and
/// rebuild each `BIMembership(name, type, _)` as a `BodyItem::Membership`. The
/// `*_build` FSMs only ever emit `BIMembership` with `PNone` pins.
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

    /// Run both cut-over sub-passes through the production free functions (the
    /// per-thread cached runner, resolved via stdlib_dir()) in the load path's
    /// order, and return the set of membership names the body gained.
    fn injected_names(raw: &SchemaDecl) -> Vec<String> {
        let before: HashSet<String> = raw.body.iter().filter_map(|i| match i {
            BodyItem::Membership { name, .. } => Some(name.clone()),
            _ => None,
        }).collect();
        let mut s = raw.clone();
        fsm_params(&mut s);
        prev_tick(&mut s);
        let mut added: Vec<String> = s.body.iter().filter_map(|i| match i {
            BodyItem::Membership { name, type_name, .. } if !before.contains(name) =>
                Some(format!("{name} \u{2208} {type_name}")),
            _ => None,
        }).collect();
        added.sort();
        added
    }

    /// Golden: the membership set each corpus FSM gains from the two cut-over
    /// sub-passes, captured from the canonical Rust impl BEFORE its deletion.
    /// The self-hosted pipeline must reproduce it byte-for-byte.
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
    /// part of the contract — prev_tick is HashMap-order-nondeterministic — so
    /// this compares the gained-membership set as a sorted multiset.)
    #[test]
    fn matches_golden_on_corpus() {
        let mut by_file: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
        for (file, name, want) in GOLDEN {
            by_file.entry(file).or_default().push((name, want));
        }
        let mut checked = 0;
        for (file, wants) in &by_file {
            let path = std::path::Path::new(file);
            assert!(path.exists(), "corpus file {file} not found; update GOLDEN");
            let src = std::fs::read_to_string(path).unwrap();
            let prog = crate::parser::parse(&src)
                .unwrap_or_else(|e| panic!("parse {file}: {e}"));
            for (name, want) in wants {
                let raw = prog.schemas.iter().find(|s| &s.name == name)
                    .unwrap_or_else(|| panic!("{file}: no schema `{name}`"));
                let got = injected_names(raw).join(" | ");
                assert_eq!(&got, want, "{file}::{name} injection diverged from golden");
                checked += 1;
            }
        }
        assert_eq!(checked, GOLDEN.len());
    }

    /// Every FSM the golden does NOT list gains nothing (the two sub-passes are
    /// conservative). Guards against over-injection.
    #[test]
    fn non_golden_fsms_untouched() {
        let golden_keys: HashSet<(&str, &str)> =
            GOLDEN.iter().map(|(f, n, _)| (*f, *n)).collect();
        let files: HashSet<&str> = GOLDEN.iter().map(|(f, _, _)| *f).collect();
        for file in files {
            let src = std::fs::read_to_string(file).unwrap();
            let prog = crate::parser::parse(&src).unwrap();
            for raw in &prog.schemas {
                if golden_keys.contains(&(file, raw.name.as_str())) {
                    continue;
                }
                let got = injected_names(raw);
                assert!(got.is_empty(), "{file}::{} unexpectedly gained {got:?}", raw.name);
            }
        }
    }

    /// The self-hosted walk reaches exactly the identifiers the canonical walk
    /// reached: a hand-built FSM body referencing `state_next` / `_count` gets
    /// the same memberships.
    #[test]
    fn walk_reaches_canonical_identifiers() {
        use crate::core::ast::BinOp;
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
        let got = injected_names(&raw);
        // state_next ∈ S, _count ∈ Int, is_first_tick ∈ Bool.
        for want in ["state_next \u{2208} S", "_count \u{2208} Int", "is_first_tick \u{2208} Bool"] {
            assert!(got.iter().any(|g| g == want), "expected `{want}` injected; got {got:?}");
        }
        assert_eq!(got.len(), 3, "exactly three injected; got {got:?}");
    }

    /// The `fsm_params_build` FSM constructs + returns a `BodyItemList` whose
    /// strings the Rust decode path recovers intact — the composite-AST-return
    /// half.
    #[test]
    fn fsm_params_build_decodes_faithfully() {
        let runner = runner().expect("inject engine");
        let seed = fpb_input(true, false, true, false, true, false, "GameState", true);
        let got = run_build(&runner, "fsm_params_build", seed, "FPBDone");
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

    fn mem(name: &str, type_name: &str) -> BodyItem {
        BodyItem::Membership {
            name: name.to_string(), type_name: type_name.to_string(), pins: Pins::None,
        }
    }
}
