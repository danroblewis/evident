//! Single-FSM legacy/delta scheduler — `run_with_shape`.
//!
//! The per-step path used when the program has exactly one
//! `fsm`-keyword'd schema AND `EVIDENT_SCHEDULER=legacy` is set
//! (or, transitionally, when the multi-FSM scheduler isn't
//! selected). State pinning + effect collection + halt detection
//! happen in a flat loop over `opts.max_steps`.

use crate::ast::EffectResult;
use crate::effect_dispatch::{DispatchContext, dispatch_all};
use crate::runtime::EvidentRuntime;
use crate::translate::Value;

use super::collect::collect_dispatchable_effects;
use super::fsm::MainShape;
use super::state::{encode_state_value, model_matches_value};
use super::timing::print_timing_summary;
use super::{LoopEnv, LoopOpts, LoopResult};

pub(super) fn run_with_shape(
    rt: &EvidentRuntime,
    shape: &MainShape,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    env: &LoopEnv,
) -> Result<LoopResult, String> {
    // Initial state: pin to the FIRST variant of the state enum.
    // Convention: programs declare the initial state as the first
    // variant of their state type. This prevents Z3 from picking a
    // non-initial variant on step 0 (which would silently skip the
    // program's setup).
    let mut last_results: Vec<EffectResult> = Vec::new();
    let mut current_state_value: Option<z3::ast::Datatype<'static>> = match &shape.state_type {
        Some(st) => {
            let enums = rt.enums_registry();
            let by_name = enums.by_name.borrow();
            by_name.get(st)
                .and_then(|(sort, _)| sort.variants.first()
                    .and_then(|v| v.constructor.apply(&[]).as_datatype()))
        }
        None => None,
    };
    if shape.state_type.is_some() && current_state_value.is_none() {
        return Err(format!(
            "could not pin initial state: enum `{}` has no nullary first variant",
            shape.state_type.as_deref().unwrap_or("?")));
    }

    let mut step_count = 0usize;
    let mut final_state_model: Option<Value> = None;
    // EVIDENT_LOOP_TIMING=1 → per-step solve+dispatch timing + summary.
    // Useful for figuring out where time goes in long-running demos
    // (Z3 solve vs FFI dispatch vs idle in delays).
    let timing = env.timing;
    let loop_t0 = std::time::Instant::now();
    let mut total_solve = std::time::Duration::ZERO;
    let mut total_dispatch = std::time::Duration::ZERO;

    while step_count < opts.max_steps {
        // Pin last_results as a Seq(Result) via the `given` map —
        // assert_seq_given handles the (DatatypeSeqVar, SeqEnum)
        // pair, asserting `len + arr[i]=elem` per element.
        let mut given: std::collections::HashMap<String, Value> =
            std::collections::HashMap::new();
        if let Some(lr_var) = &shape.last_results_var {
            let last_results_val = rt.effect_results_to_value(&last_results);
            given.insert(lr_var.clone(), last_results_val);
        }

        // Build pin list. For step 0 we don't pin state (Z3 picks
        // the initial — the user's main pins it via state.step = 0
        // pattern or similar).
        let pins: Vec<(&str, z3::ast::Datatype<'static>)> = match (&shape.state_var, &current_state_value) {
            (Some(name), Some(s)) => vec![(name.as_str(), s.clone())],
            _ => vec![],
        };

        let solve_t0 = std::time::Instant::now();
        let r = rt.query_with_pins_and_given(&shape.claim_name, &pins, &given)
            .map_err(|e| format!("solve step {step_count}: {e}"))?;
        let solve_dt = solve_t0.elapsed();
        total_solve += solve_dt;

        if !r.satisfied {
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: false,
                exit_code: ctx.exit_requested,
            });
        }

        // Read state_next + effects from model when those slots exist.
        let state_next_val: Option<&Value> = match &shape.state_next_var {
            Some(sn) => Some(r.bindings.get(sn)
                .ok_or_else(|| format!("step {step_count}: model has no `{}`", sn))?),
            None => None,
        };
        // Walk the entire model for dispatchable Effect / Seq(Effect)
        // bindings. The legacy `effects` Seq (when present) dispatches
        // first to preserve `last_results` indexing.
        let effects = collect_dispatchable_effects(rt, &shape.claim_name,
            &r.bindings, shape.effects_var.as_deref());

        // Halt-check: if effects empty AND state_next equals state, we
        // consider the program halted (fixpoint). User can also issue
        // `Effect::Exit(0)` to terminate immediately.
        let halted_by_fixpoint = effects.is_empty()
            && current_state_value.is_some()
            && state_next_val.is_some()
            && model_matches_value(state_next_val.unwrap(),
                shape.state_type.as_deref().unwrap_or(""));

        let dispatch_t0 = std::time::Instant::now();
        let new_results = dispatch_all(ctx, &effects);
        let dispatch_dt = dispatch_t0.elapsed();
        total_dispatch += dispatch_dt;

        if env.trace {
            eprintln!("[loop] step {step_count}: state_next={state_next_val:?} effects={effects:?}");
        }
        if timing {
            eprintln!(
                "[timing] step {step_count}: solve={:.2}ms dispatch={:.2}ms ({} effects)",
                solve_dt.as_secs_f64() * 1000.0,
                dispatch_dt.as_secs_f64() * 1000.0,
                effects.len(),
            );
        }
        // Re-encode state for the next step's pin. Handles nullary
        // and payload variants. Skip when state isn't part of this fsm.
        if let Some(snv) = state_next_val {
            current_state_value = encode_state_value(rt, snv);
            final_state_model = Some(snv.clone());
        }

        last_results = new_results;
        step_count += 1;

        // Effect::Exit handling: an FSM emitted Exit. Dispatch
        // already completed (other effects in this tick ran),
        // so halt cleanly with the requested code.
        if ctx.exit_requested.is_some() {
            if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        if halted_by_fixpoint {
            if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: true,
                exit_code: None,
            });
        }
    }

    if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
    Ok(LoopResult {
        steps: step_count,
        final_state: final_state_model,
        halted_clean: false,
        exit_code: None,
    })
}
