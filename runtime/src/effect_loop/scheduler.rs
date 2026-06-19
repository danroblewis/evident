//! Minimal tick loop.
//!
//! Each tick, for every `fsm`-keyword'd claim in declaration order:
//!   1. Pin its current state (Datatype) + `_var` previous-tick values
//!      + last_results + the shared world snapshot.
//!   2. Solve the FSM via `rt.query_with_pins_and_given`.
//!   3. Collect the ordered Effect sequence from the model.
//!   4. Capture this tick's `world.X` writes into the shared snapshot,
//!      and every binding into `prev_values` for next tick's `_var`.
//!   5. Advance `state_next → state`.
//! After all FSMs solve, dispatch every effect in order, feeding each
//! FSM's results back as its `last_results` for the next tick.
//!
//! Halt: on `Effect::Exit(code)` (graceful, end-of-tick), at
//! `max_steps`, or at a fixpoint (no FSM changed state, emitted an
//! effect, or wrote a world field this tick → nothing more can happen).
//!
//! This is the only execution path. Single-FSM programs are the common
//! case; multi-FSM programs (a shared `world ∈ World` handed off
//! between several FSMs via `_world.X`/`world.X`) are supported by
//! merging every writer's `world.X` bindings into one snapshot each
//! tick — writers' fields are expected to be disjoint by construction.

use std::collections::HashMap;

use crate::core::ast::EffectResult;
use crate::effect_dispatch::{dispatch_all, DispatchContext};
use crate::runtime::EvidentRuntime;
use crate::core::Value;

use super::collect::collect_dispatchable_effects;
use super::fsm::MainShape;
use super::state::encode_state_value;
use super::timing::print_timing_summary_full;
use super::{LoopEnv, LoopOpts, LoopResult};

/// Per-FSM mutable run state.
struct FsmRt {
    current_state:   Option<z3::ast::Datatype<'static>>,
    current_state_v: Option<Value>,
    last_results:    Vec<EffectResult>,
    /// Every variable's value at end of the previous tick. Used to
    /// pin `_name` references this tick (the `_var` time-shift
    /// convention). Empty on tick 0 → `is_first_tick` pins true.
    prev_values:     HashMap<String, Value>,
}

/// Seed an FSM's initial state to its enum's first (nullary) variant.
/// The first variant declared in `enum FooState = Init | …` is the
/// starting state. Payload first-variants can't be seeded (no value to
/// supply) → Z3 picks on tick 0.
fn seed_state(
    rt: &EvidentRuntime,
    s: &MainShape,
) -> (Option<z3::ast::Datatype<'static>>, Option<Value>) {
    let Some(state_type) = s.state_type.as_ref() else { return (None, None); };
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let entry = by_name.get(state_type);
    let dt = entry.and_then(|(sort, _)| {
        let first = sort.variants.first()?;
        if first.constructor.arity() == 0 {
            first.constructor.apply(&[]).as_datatype()
        } else {
            None
        }
    });
    let val = entry.and_then(|(sort, decl_variants)| {
        let first = decl_variants.first()?;
        if sort.variants.first().map(|v| v.constructor.arity()).unwrap_or(0) == 0 {
            Some(Value::Enum {
                enum_name: state_type.clone(),
                variant:   first.name.clone(),
                fields:    Vec::new(),
            })
        } else {
            None
        }
    });
    (dt, val)
}

pub(super) fn run_loop(
    rt: &EvidentRuntime,
    fsms: &[MainShape],
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    world_snapshot: &mut HashMap<String, Value>,
    env: &LoopEnv,
) -> Result<LoopResult, String> {
    let mut fsm_rt: Vec<FsmRt> = fsms
        .iter()
        .map(|s| {
            let (initial_dt, initial_val) = seed_state(rt, s);
            FsmRt {
                current_state:   initial_dt,
                current_state_v: initial_val,
                last_results:    Vec::new(),
                prev_values:     HashMap::new(),
            }
        })
        .collect();

    // ── World snapshot defaults ───────────────────────────────────
    // Pre-populate world fields with type defaults so Z3 doesn't pick
    // arbitrary values on tick 0 before any write has landed.
    for fsm in fsms {
        if let Some(wt) = &fsm.world_type {
            if let Some(world_schema) = rt.get_schema(wt) {
                for item in &world_schema.body {
                    if let crate::core::ast::BodyItem::Membership { name, type_name, .. } = item {
                        let key = format!("world.{name}");
                        if world_snapshot.contains_key(&key) { continue; }
                        let default = match type_name.as_str() {
                            "Int"    => Some(Value::Int(0)),
                            "Bool"   => Some(Value::Bool(false)),
                            "String" => Some(Value::Str(String::new())),
                            "Real"   => Some(Value::Real(0.0)),
                            _        => None,
                        };
                        if let Some(d) = default {
                            world_snapshot.insert(key, d);
                        }
                    }
                }
            }
        }
    }

    let mut step_count = 0usize;
    let timing = env.timing;
    let loop_t0 = std::time::Instant::now();
    let mut total_solve = std::time::Duration::ZERO;
    let mut total_dispatch = std::time::Duration::ZERO;
    let mut per_fsm_solve: Vec<std::time::Duration> =
        vec![std::time::Duration::ZERO; fsms.len()];
    let mut per_fsm_ticks: Vec<usize> = vec![0; fsms.len()];

    let summary = |loop_t0, steps, solve, dispatch, solves: &[std::time::Duration], ticks: &[usize]| {
        if timing {
            let rows: Vec<(&str, std::time::Duration, usize)> = fsms
                .iter()
                .enumerate()
                .map(|(i, f)| (f.claim_name.as_str(), solves[i], ticks[i]))
                .collect();
            print_timing_summary_full(loop_t0, steps, solve, dispatch, &rows);
        }
    };

    while step_count < opts.max_steps {
        // Effects collected this tick: (fsm_idx, ordered effects).
        let mut all_effects: Vec<(usize, Vec<crate::core::ast::Effect>)> = Vec::new();
        let mut any_state_changed = false;
        let mut any_world_write = false;

        for (idx, fsm) in fsms.iter().enumerate() {
            // Pin list: state as Datatype.
            let pins: Vec<(&str, z3::ast::Datatype<'static>)> =
                match (&fsm.state_var, &fsm_rt[idx].current_state) {
                    (Some(name), Some(s)) => vec![(name.as_str(), s.clone())],
                    _ => vec![],
                };

            // Per-FSM solve view: world snapshot (with this FSM's
            // install keys de-prefixed to `param.field`), last_results,
            // `_var` previous values, and the current state value.
            let mut fsm_view: HashMap<String, Value> = if fsm.install_params.is_empty() {
                world_snapshot.clone()
            } else {
                let mut v = world_snapshot.clone();
                let prefix = format!("{}.", fsm.claim_name);
                for (k, val) in world_snapshot.iter() {
                    if let Some(stripped) = k.strip_prefix(&prefix) {
                        v.insert(stripped.to_string(), val.clone());
                    }
                }
                v
            };
            if let Some(lr_var) = &fsm.last_results_var {
                let lr = rt.effect_results_to_value(&fsm_rt[idx].last_results);
                fsm_view.insert(lr_var.clone(), lr);
            }
            // `_var` time-shift: pin every `_name` from prev_values
            // (primitive or per-field for records), plus `is_first_tick`.
            if let Some(claim) = rt.get_schema(&fsm.claim_name) {
                let is_first = fsm_rt[idx].prev_values.is_empty();
                let mut sees_underscore = false;
                for item in &claim.body {
                    if let crate::core::ast::BodyItem::Membership { name, .. } = item {
                        if let Some(stripped) = name.strip_prefix('_') {
                            sees_underscore = true;
                            if let Some(prev) = fsm_rt[idx].prev_values.get(stripped) {
                                fsm_view.insert(name.clone(), prev.clone());
                            }
                            let prefix = format!("{stripped}.");
                            for (k, v) in &fsm_rt[idx].prev_values {
                                if let Some(field) = k.strip_prefix(&prefix) {
                                    fsm_view.insert(format!("{name}.{field}"), v.clone());
                                }
                            }
                        }
                    }
                }
                if sees_underscore {
                    fsm_view.insert("is_first_tick".to_string(), Value::Bool(is_first));
                }
            }
            if let (Some(state_name), Some(state_v)) =
                (&fsm.state_var, &fsm_rt[idx].current_state_v)
            {
                fsm_view.insert(state_name.clone(), state_v.clone());
            }

            let solve_t0 = std::time::Instant::now();
            let r = rt
                .query_with_pins_and_given(&fsm.claim_name, &pins, &fsm_view)
                .map_err(|e| format!("FSM `{}` solve step {step_count}: {e}", fsm.claim_name))?;
            let solve_dt = solve_t0.elapsed();
            total_solve += solve_dt;
            per_fsm_solve[idx] += solve_dt;
            per_fsm_ticks[idx] += 1;

            if !r.satisfied {
                eprintln!("[loop] FSM `{}` returned UNSAT on tick {step_count}", fsm.claim_name);
                summary(loop_t0, step_count, total_solve, total_dispatch,
                        &per_fsm_solve, &per_fsm_ticks);
                return Ok(LoopResult {
                    steps: step_count,
                    final_state: fsm_rt[idx].current_state_v.clone(),
                    halted_clean: false,
                    exit_code: ctx.exit_requested,
                });
            }

            let state_next_val: Option<Value> = match &fsm.state_next_var {
                Some(sn) => Some(
                    r.bindings.get(sn)
                        .ok_or_else(|| format!(
                            "FSM `{}` step {step_count}: model has no `{}`",
                            fsm.claim_name, sn))?
                        .clone(),
                ),
                None => None,
            };

            let effects = collect_dispatchable_effects(
                rt, &fsm.claim_name, &r.bindings, fsm.effects_var.as_deref());

            // Merge this FSM's `world_next.X` writes into the shared
            // snapshot under the `world.X` keys, which become the
            // `world.X` reads for every FSM next tick. (The `_world`/
            // `world` time-shift desugar maps source `world.X = …`
            // writes onto `world_next.X` bindings.) Writers are
            // expected to write disjoint fields; last write wins.
            if fsm.world_next_var.is_some() {
                for (k, v) in r.bindings.iter() {
                    if let Some(field) = k.strip_prefix("world_next.") {
                        let key = format!("world.{field}");
                        if world_snapshot.get(&key) != Some(v) {
                            any_world_write = true;
                        }
                        world_snapshot.insert(key, v.clone());
                    }
                }
            }

            // Track state change for fixpoint detection.
            if let Some(snv) = &state_next_val {
                let changed = fsm_rt[idx].current_state_v.as_ref()
                    .map(|prev| prev != snv).unwrap_or(true);
                if changed { any_state_changed = true; }
                fsm_rt[idx].current_state = encode_state_value(rt, snv);
                fsm_rt[idx].current_state_v = Some(snv.clone());
            }

            // Capture bindings for next tick's `_var` pins.
            for (k, v) in r.bindings.iter() {
                if k.starts_with('_') { continue; }
                if k == "is_first_tick" { continue; }
                fsm_rt[idx].prev_values.insert(k.clone(), v.clone());
            }

            if env.trace {
                eprintln!("[loop] tick {step_count} fsm={}: state_next={state_next_val:?} effects={effects:?}",
                    fsm.claim_name);
            }
            if timing {
                eprintln!("[timing] tick {step_count} fsm={}: solve={:.2}ms ({} effects)",
                    fsm.claim_name, solve_dt.as_secs_f64() * 1000.0, effects.len());
            }
            all_effects.push((idx, effects));
        }

        // ── Dispatch all effects in order ─────────────────────────
        let dispatch_t0 = std::time::Instant::now();
        let mut any_effect = false;
        for (fsm_idx, effects) in all_effects {
            if !effects.is_empty() { any_effect = true; }
            let results = dispatch_all(ctx, &effects);
            fsm_rt[fsm_idx].last_results = results;
        }
        total_dispatch += dispatch_t0.elapsed();

        step_count += 1;

        // ── Exit (graceful, end-of-tick) ──────────────────────────
        if ctx.exit_requested.is_some() {
            summary(loop_t0, step_count, total_solve, total_dispatch,
                    &per_fsm_solve, &per_fsm_ticks);
            return Ok(LoopResult {
                steps: step_count,
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        // ── Fixpoint halt ─────────────────────────────────────────
        // Nothing changed this tick (no state transition, no effect,
        // no world write) → nothing can change next tick either.
        if !any_state_changed && !any_effect && !any_world_write {
            summary(loop_t0, step_count, total_solve, total_dispatch,
                    &per_fsm_solve, &per_fsm_ticks);
            return Ok(LoopResult {
                steps: step_count,
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }
    }

    summary(loop_t0, step_count, total_solve, total_dispatch,
            &per_fsm_solve, &per_fsm_ticks);
    Ok(LoopResult {
        steps: step_count,
        final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
        halted_clean: false,
        exit_code: ctx.exit_requested,
    })
}
