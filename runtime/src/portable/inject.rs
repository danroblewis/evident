//! FSM membership injection; `inject_fsm_params` / `inject_prev_tick_decls`
//! self-host in Evident. String-set membership stays in Rust (#18 cousin).

use std::collections::HashMap;

use super::{run_done_payload, run_name_list, work_node, EvidentRunner};
use crate::core::ast::{BodyItem, Expr, Keyword, Pins, SchemaDecl};
use crate::core::Value;
use crate::translate::ast_encoder::expr_to_value;

guarded_runner!(runner, "passes/inject.ev", "inject_collect");

/// Collect all referenced identifiers in body constraints and claim-call values.
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

    let refs = collect_refs(runner, &s.body);
    let ref_state_next = refs.iter().any(|n| n == "state_next");
    let ref_last_results = refs.iter().any(|n| n == "last_results");
    let ref_effects = refs.iter().any(|n| n == "effects");

    let seed = fpb_input(
        ref_state_next, have_state_next,
        ref_last_results, have_last_results,
        ref_effects, have_effects,
        state_type.as_deref().unwrap_or(""), state_type.is_some(),
    );
    let injected = run_build(runner, "fsm_params_build", seed, "FPBDone");
    splice_at(s, injected);
}

/// `_`-strip, first-segment split, and declared-name lookup stay in Rust
/// (no substring ops in Evident); pairs + is_first_tick flag feed `prev_tick_build`.
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
    if prev_refs.is_empty() {
        return;
    }

    let pairs: Vec<(String, String)> = prev_refs.into_iter()
        .filter(|(name, _)| !declared.contains_key(name))
        .collect();
    let add_first_tick = !declared.contains_key("is_first_tick");

    let seed = ptb_input(&pairs, add_first_tick);
    let injected = run_build(runner, "prev_tick_build", seed, "PTBDone");
    splice_at(s, injected);
}

/// Drive a `*_build` FSM to `<done>(BodyItemList)` and decode into memberships.
fn run_build(runner: &EvidentRunner, fsm: &str, seed: Value, done_variant: &str) -> Vec<BodyItem> {
    run_done_payload(runner, fsm, seed, done_variant, "inject/evident")
        .map(|p| decode_membership_list(&p))
        .unwrap_or_default()
}

/// Skip inject for the runtime's own pass FSMs — they reference no implicit
/// slot, and skipping breaks the cross-engine cascade on every load.
fn is_self_hosted_pass_fsm(name: &str) -> bool {
    matches!(
        name,
        "inject_collect" | "fsm_params_build" | "prev_tick_build"
            | "validate_walk" | "subscriptions_walk" | "pretty_walk"
    )
}

/// Inject `state_next` / `last_results` / `effects` when referenced + undeclared.
/// No-op for non-fsm, external, or runtime-internal schemas.
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

/// Inject `_var` time-shift slots and `is_first_tick` when referenced.
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

/// Splice `items` into `s.body` at `s.param_count` (first-line-param index).
fn splice_at(s: &mut SchemaDecl, items: Vec<BodyItem>) {
    let insert_pos = s.param_count;
    for (i, item) in items.into_iter().enumerate() {
        s.body.insert(insert_pos + i, item);
    }
}

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

/// Decode a `BodyItemList` cons-list into `BodyItem::Membership`s.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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

    #[test]
    fn walk_reaches_canonical_identifiers() {
        use crate::core::ast::BinOp;
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
        for want in ["state_next \u{2208} S", "_count \u{2208} Int", "is_first_tick \u{2208} Bool"] {
            assert!(got.iter().any(|g| g == want), "expected `{want}` injected; got {got:?}");
        }
        assert_eq!(got.len(), 3, "exactly three injected; got {got:?}");
    }

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
