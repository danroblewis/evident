//! Strategy-1 of the dual-engine behavior-contract proof: the greenfield
//! SMT-LIB engine (`runtime_smt::solve_tick`) plugged into the portable
//! `runtime_contract::FsmEngine` harness, run over all 15 fixtures.
//!
//! The adapter feeds the fully-pinned single-tick problem (`pinned_smtlib()` =
//! problem ++ prev ++ inputs, with the pins as inline asserts) to `solve_tick`
//! as the transition relation, with empty `prev`/`given` maps. Every model key
//! the contract checks (positive `expect.model` keys + negative
//! `expect.forbidden` keys) is registered as a `StateVar.next` so the engine's
//! model extractor surfaces it.

use std::collections::BTreeMap;

use runtime_contract::{
    fixtures_dir_from_manifest, load_fixtures, run_matrix, CVal, FsmEngine, Outcome,
};
use runtime_smt::{solve_tick, EffectSpec, FsmSpec, Sort, StateVar, TickError, Value};

/// Convert a native `runtime_smt::Value` into the engine-neutral `CVal`.
fn conv(v: &Value) -> CVal {
    match v {
        Value::Int(i) => CVal::Int(*i),
        Value::Bool(b) => CVal::Bool(*b),
        Value::Real(r) => CVal::Real(*r),
        Value::Str(s) => CVal::Str(s.clone()),
        Value::Enum { ctor, args } => CVal::Enum {
            enum_name: String::new(),
            variant: ctor.clone(),
            fields: args.iter().map(conv).collect(),
        },
        Value::Seq(xs) => CVal::SeqEnum(xs.iter().map(conv).collect()),
    }
}

/// The greenfield SMT-LIB FSM engine as a behavior-contract participant.
struct GreenfieldEngine;

impl FsmEngine for GreenfieldEngine {
    fn name(&self) -> &str {
        "runtime-smt (greenfield)"
    }

    fn tick(&self, fx: &runtime_contract::Fixture) -> Outcome {
        // The fully-pinned problem IS the transition: pins are inline asserts,
        // so prev/given maps stay empty.
        let transition = fx.pinned_smtlib();

        // Register every model key the contract checks as a `next` state var.
        // `extract()` reads only `StateVar.next`; `prev`/`sort`/`init` are
        // placeholders here (pins live in the SMT-LIB text, not these fields).
        let mut state = Vec::new();
        for k in fx
            .meta
            .expect_model
            .keys()
            .chain(fx.meta.expect_forbidden.keys())
        {
            state.push(StateVar {
                prev: format!("__prev__{k}"),
                next: k.clone(),
                sort: Sort::Int,
                init: None,
            });
        }

        // Only surface effects when they were genuinely in the portable SMT.
        let effects = if fx.meta.effects_in_smt {
            fx.meta.effects_var.clone().map(|var| EffectSpec { var })
        } else {
            None
        };

        let fsm = FsmSpec {
            name: fx.meta.fsm_claim.clone(),
            transition,
            state,
            given: vec![],
            effects,
            halt: None,
            last_results: None,
            world_writes: vec![],
            world_reads: vec![],
        };

        match solve_tick(&fsm, &BTreeMap::new(), &BTreeMap::new()) {
            Ok(tm) => {
                let model = tm
                    .next_state
                    .iter()
                    .map(|(k, v)| (k.clone(), conv(v)))
                    .collect();
                // `None` ≠ empty effects — only report effects when in the SMT.
                let effects = if fx.meta.effects_in_smt {
                    Some(
                        tm.effects
                            .iter()
                            .map(|ev| CVal::Enum {
                                enum_name: "Effect".into(),
                                variant: ev.ctor.clone(),
                                fields: ev.args.iter().map(conv).collect(),
                            })
                            .collect(),
                    )
                } else {
                    None
                };
                Outcome::Sat { model, effects }
            }
            Err(TickError::Unsat) => Outcome::Unsat,
            Err(e) => Outcome::Unsupported(format!("{e}")),
        }
    }
}

#[test]
fn greenfield_contract_matrix() {
    let fixtures = load_fixtures(&fixtures_dir_from_manifest(env!("CARGO_MANIFEST_DIR")));
    assert!(
        fixtures.len() >= 15,
        "expected 15 fixtures, found {}",
        fixtures.len()
    );

    let eng = GreenfieldEngine;
    let report = run_matrix(&[&eng], &fixtures);
    eprintln!("\n{}\n", report.to_text());
    assert!(
        !report.any_fail(),
        "greenfield engine FAILED some fixtures:\n{:#?}",
        report.failures()
    );
}
