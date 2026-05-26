//! Subscription-driven multi-FSM scheduler: writers solve first, then readers with updated world.
//! Halts implicitly when no FSM is scheduled and no async source can wake one.

use crate::core::ast::EffectResult;
use crate::effect_dispatch::{DispatchContext, dispatch_all};
use crate::runtime::EvidentRuntime;
use crate::core::Value;

use super::collect::collect_dispatchable_effects;
use super::fsm::{MainShape, resolve_fsm};
use super::state::{encode_state_value, seed_state_with_arg};
use super::timing::print_timing_summary_full;
use super::{LoopEnv, LoopOpts, LoopResult};

pub(super) fn run_scheduler(
    rt: &EvidentRuntime,
    fsms: &[MainShape],
    // Computed once in `run_with_ctx`; reused here to avoid a second costly walk per FSM.
    initial_access: &[crate::subscriptions::AccessSets],
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    event_rx: Option<&std::sync::mpsc::Receiver<crate::event_sources::SchedulerEvent>>,
    event_sources: &mut [Box<dyn crate::event_sources::EventSource>],
    env: &LoopEnv,
) -> Result<LoopResult, String> {
    use std::collections::HashMap;
    let mut fsms: Vec<MainShape> = fsms.to_vec(); // owned so SpawnFsm can grow it
    // Tracks both encoded Datatype (for next-tick pin) and decoded Value (for halt detection).
    struct FsmRt {
        current_state:   Option<z3::ast::Datatype<'static>>,
        current_state_v: Option<Value>,
        last_results:    Vec<EffectResult>,
        halted:          bool,
        /// Previous-tick bindings for `_name` time-shift pinning; empty on tick 0.
        prev_values:     HashMap<String, Value>,
    }
    // Seed each FSM to its enum's first variant. Without this Z3 picks an arbitrary state on
    // tick 0 — often Done, halting before any work. Only nullary first variants are seeded.
    let seed_state = |s: &MainShape| -> (Option<z3::ast::Datatype<'static>>, Option<Value>) {
        let Some(state_type) = s.state_type.as_ref() else { return (None, None); };
        let enums = rt.enums_registry();
        let by_name = enums.by_name.borrow();
        let entry = by_name.get(state_type);
        // Only nullary first variants are seeded; payload variants let Z3 pick on tick 0.
        let dt = entry.and_then(|(sort, _)| {
            let first = sort.variants.first()?;
            if first.constructor.arity() == 0 {
                first.constructor.apply(&[]).as_datatype()
            } else { None }
        });
        let val = entry.and_then(|(sort, decl_variants)| {
            let first = decl_variants.first()?;
            if sort.variants.first().map(|v| v.constructor.arity()).unwrap_or(0) == 0 {
                Some(Value::Enum {
                    enum_name: state_type.clone(),
                    variant:   first.name.clone(),
                    fields:    Vec::new(),
                })
            } else { None }
        });
        (dt, val)
    };
    let mut fsm_rt: Vec<FsmRt> = fsms.iter().map(|s| {
        let (initial_dt, initial_val) = seed_state(s);
        FsmRt {
            current_state:   initial_dt,
            current_state_v: initial_val,
            last_results:    Vec::new(),
            halted:          false,
            prev_values:     HashMap::new(),
        }
    }).collect();

    let mut world_snapshot: HashMap<String, Value> = HashMap::new();
    // Pre-populate plugin-managed fields with type defaults so Z3 doesn't pick arbitrary
    // values on tick 0 (e.g. `world.stdin_seq` would be unconstrained Int).
    if let Some(_world_type_name) = fsms.iter().find_map(|f| f.world_type.as_ref()) {
        for fsm in &fsms {
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
    }

    let mut step_count = 0usize;
    let timing = env.timing;
    let loop_t0 = std::time::Instant::now();
    let mut total_solve = std::time::Duration::ZERO;
    let mut total_dispatch = std::time::Duration::ZERO;
    let mut per_fsm_solve: Vec<std::time::Duration> = vec![std::time::Duration::ZERO; fsms.len()];
    let mut per_fsm_ticks: Vec<usize> = vec![0; fsms.len()];

    // Transitive access sets; owned so spawned FSMs can push their own.
    let mut access_sets: Vec<crate::subscriptions::AccessSets> = initial_access.to_vec();
    // World fields changed since each FSM was last scheduled; cleared on schedule.
    let mut pending_changes: Vec<std::collections::HashSet<String>> =
        vec![std::collections::HashSet::new(); fsms.len()];
    // Self-feedback: emitted effects last tick → fresh last_results next tick.
    let mut had_effects_last: Vec<bool> = vec![false; fsms.len()];
    // State-change feedback: prevents missing Frame(N) bodies after silent transitions.
    let mut state_changed_last: Vec<bool> = vec![false; fsms.len()];
    // External-event feedback: async source fired (coarse wake; all alive FSMs).
    let mut external_event: Vec<bool> = vec![false; fsms.len()];
    // FIFO of plugin world-writes; drain one event-write per tick so each is visible.
    let mut pending_world_writes: std::collections::VecDeque<(String, Value)> =
        std::collections::VecDeque::new();

    // Clear percolated-effect residue from load-time queries before the tick loop.
    let _ = crate::runtime::take_percolated_effects();

    while step_count < opts.max_steps {
        if fsm_rt.iter().all(|f| f.halted) {
            if timing {
                let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                    .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                    .collect();
                print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
            }
            return Ok(LoopResult {
                steps: step_count,
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        // Drain plugin writes: one event-write per tick so each is individually observable.
        for src in event_sources.iter_mut() {
            let writes = src.drain_writes();
            pending_world_writes.append(&mut writes.into_iter().collect());
        }
        // STATE writes (dotted/FTI keys): apply all immediately — last wins.
        // EVENT writes (bare world keys): apply one per tick — each value matters.
        let mut event_writes: std::collections::VecDeque<(String, Value)> =
            std::collections::VecDeque::new();
        let mut state_writes: Vec<(String, Value)> = Vec::new();
        while let Some((field, val)) = pending_world_writes.pop_front() {
            if field.contains('.') {
                state_writes.push((field, val));
            } else {
                event_writes.push_back((field, val));
            }
        }
        pending_world_writes = event_writes;

        for (field, val) in state_writes {
            let key = field.clone();
            let changed = world_snapshot.get(&key) != Some(&val);
            if changed {
                world_snapshot.insert(key.clone(), val);
                // FTI wake: if key matches `<claim>.<rest>`, wake that FSM.
                for (j, fsm) in fsms.iter().enumerate() {
                    let prefix = format!("{}.", fsm.claim_name);
                    if let Some(rest) = key.strip_prefix(&prefix) {
                        // Top-level segment of `rest` is the param name.
                        let param = rest.split('.').next().unwrap_or(rest);
                        if fsm.fti_params.iter().any(|(p, _, _)| p == param) {
                            pending_changes[j].insert(rest.to_string());
                        }
                    }
                }
            }
        }

        if let Some((field, val)) = pending_world_writes.pop_front() {
            let key = format!("world.{field}"); // bare field → world.X pin
            let changed = world_snapshot.get(&key) != Some(&val);
            if changed {
                world_snapshot.insert(key, val);
                for (j, _f) in fsms.iter().enumerate() {
                    if access_sets[j].reads.contains(&field) {
                        pending_changes[j].insert(field.clone());
                    }
                }
            }
        }

        let mut all_effects: Vec<(usize, Vec<crate::core::ast::Effect>)> = Vec::new();
        let mut scheduled_this_tick: Vec<bool> = vec![false; fsms.len()];

        for (idx, fsm) in fsms.iter().enumerate() {
            if fsm_rt[idx].halted { continue; }

            // Wake triggers: (1) bootstrap, (2) self-feedback (effects), (3) world delta,
            // (4) state change. All others stay asleep; pending_changes cleared on schedule.
            if step_count > 0 {
                let woken = had_effects_last[idx]
                    || !pending_changes[idx].is_empty()
                    || state_changed_last[idx]
                    || external_event[idx];
                if !woken {
                    if env.trace {
                        eprintln!("[loop] tick {step_count} fsm={}: skipped (no inputs)",
                            fsm.claim_name);
                    }
                    continue;
                }
                pending_changes[idx].clear();
                external_event[idx] = false;
            }
            scheduled_this_tick[idx] = true;

            let pins: Vec<(&str, z3::ast::Datatype<'static>)> = match (&fsm.state_var, &fsm_rt[idx].current_state) {
                (Some(name), Some(s)) => vec![(name.as_str(), s.clone())],
                _ => vec![],
            };

            // Per-FSM snapshot view: world.X entries + FTI keys (prefix stripped) + last_results.
            let mut fsm_view: HashMap<String, Value> = if fsm.fti_params.is_empty() {
                world_snapshot.clone()
            } else {
                let mut v = world_snapshot.clone();
                let prefix = format!("{}.", fsm.claim_name);
                for (k, val) in &world_snapshot {
                    if let Some(stripped) = k.strip_prefix(&prefix) {
                        v.insert(stripped.to_string(), val.clone());
                    }
                }
                v
            };
            if let Some(lr_var) = &fsm.last_results_var {
                let last_results_val = rt.effect_results_to_value(&fsm_rt[idx].last_results);
                fsm_view.insert(lr_var.clone(), last_results_val);
            }
            // `_name` time-shift: pin each `_name` to the previous-tick value of `name`.
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
                            // Records are flattened; mirror `stripped.<field>` → `_name.<field>`.
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
                    fsm_view.insert(
                        "is_first_tick".to_string(),
                        Value::Bool(is_first),
                    );
                }
            }
            // Expose state in given so the functionizer fast-path sees it (Z3 ignores dup eqs).
            if let (Some(state_name), Some(state_v)) = (&fsm.state_var, &fsm_rt[idx].current_state_v) {
                fsm_view.insert(state_name.clone(), state_v.clone());
            }
            let solve_input: &HashMap<String, Value> = &fsm_view;

            let solve_t0 = std::time::Instant::now();
            let r = rt.query_with_pins_and_given(&fsm.claim_name, &pins, solve_input)
                .map_err(|e| format!("FSM `{}` solve step {step_count}: {e}", fsm.claim_name))?;
            let solve_dt = solve_t0.elapsed();
            total_solve += solve_dt;
            per_fsm_solve[idx] += solve_dt;
            per_fsm_ticks[idx] += 1;

            if !r.satisfied {
                eprintln!("[loop] FSM `{}` returned UNSAT on tick {step_count}", fsm.claim_name);
                if timing {
                    let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                        .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                        .collect();
                    print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
                }
                return Ok(LoopResult {
                    steps: step_count,
                    final_state: fsm_rt[idx].current_state_v.clone(),
                    halted_clean: false,
                    exit_code: ctx.exit_requested,
                });
            }

            let state_next_val: Option<&Value> = match &fsm.state_next_var {
                Some(sn) => Some(r.bindings.get(sn)
                    .ok_or_else(|| format!("FSM `{}` step {step_count}: model has no `{}`",
                        fsm.claim_name, sn))?),
                None => None,
            };
            let effects = collect_dispatchable_effects(rt, &fsm.claim_name,
                &r.bindings, fsm.effects_var.as_deref());

            // Percolated child effects (from `run(F, init)` inside this FSM's body).
            // Dispatch child effects first (child-tick order), then parent's own.
            let percolated = crate::runtime::take_percolated_effects();
            let effects = if percolated.is_empty() {
                effects
            } else {
                if env.trace {
                    eprintln!("[loop] tick {step_count} fsm={}: {} percolated child effect(s)",
                        fsm.claim_name, percolated.len());
                }
                let mut combined = percolated;
                combined.extend(effects);
                combined
            };

            // Writer: merge this FSM's world_next.X fields into the snapshot.
            // Only fields in the write-set are consumed; Z3 may bind others (ignored).
            if fsm.is_writer() {
                let mut just_changed: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let my_writes = &access_sets[idx].writes;
                for (k, v) in r.bindings.iter() {
                    if let Some(field) = k.strip_prefix("world_next.") {
                        let first = field.split('.').next().unwrap_or(field);
                        if !my_writes.contains(first) { continue; }
                        let key = format!("world.{field}");
                        if world_snapshot.get(&key) != Some(v) {
                            just_changed.insert(first.to_string());
                        }
                        world_snapshot.insert(key, v.clone());
                    }
                }
                for j in 0..fsms.len() {
                    if j == idx { continue; }
                    for f in &just_changed {
                        if access_sets[j].reads.contains(f) {
                            pending_changes[j].insert(f.clone());
                        }
                    }
                }
            }

            state_changed_last[idx] = match state_next_val {
                Some(snv) => fsm_rt[idx].current_state_v.as_ref()
                    .map(|prev| prev != snv).unwrap_or(true),
                None => false,
            };

            if let Some(snv) = state_next_val {
                fsm_rt[idx].current_state = encode_state_value(rt, snv);
                fsm_rt[idx].current_state_v = Some(snv.clone());
            }

            // Capture bindings for next-tick `_name` pinning; skip `_` prefixes and is_first_tick.
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

        let dispatch_t0 = std::time::Instant::now();
        // Reset self-feedback only for scheduled FSMs; unscheduled ones haven't observed yet.
        for (i, was_scheduled) in scheduled_this_tick.iter().enumerate() {
            if *was_scheduled { had_effects_last[i] = false; }
        }
        for (fsm_idx, effects) in all_effects {
            let emitted_anything = !effects.is_empty();
            let results = dispatch_all(ctx, &effects);
            fsm_rt[fsm_idx].last_results = results;
            had_effects_last[fsm_idx] = emitted_anything;
        }
        let dispatch_dt = dispatch_t0.elapsed();
        total_dispatch += dispatch_dt;

        // SpawnFsm: instantiate queued spawns; joins scheduler next tick.
        if !ctx.pending_spawns.is_empty() {
            for (claim_name, spawn_arg) in std::mem::take(&mut ctx.pending_spawns) {
                let shape = match resolve_fsm(rt, &claim_name) {
                    Some(s) => s,
                    None => {
                        eprintln!("[loop] spawn: schema `{claim_name}` isn't \
                                   declared with the `fsm` keyword (or is \
                                   `external fsm`); spawn ignored.");
                        continue;
                    }
                };
                if env.trace {
                    eprintln!("[loop] tick {step_count}: spawned `{claim_name}` \
                               as FSM #{} with arg={spawn_arg}", fsms.len());
                }
                // Skip costly walk for world-less FSMs; access sets are provably empty.
                let aset = if shape.world_type.is_none() {
                    crate::subscriptions::AccessSets::default()
                } else {
                    rt.get_schema(&shape.claim_name)
                        .map(crate::portable::subscriptions::access_sets)
                        .unwrap_or_default()
                };
                // Seed state to FirstVariant(spawn_arg) if Int-payload first variant; else fallback.
                let (initial_dt, initial_val) = seed_state_with_arg(rt, &shape, spawn_arg)
                    .unwrap_or_else(|| seed_state(&shape));
                fsms.push(shape);
                access_sets.push(aset);
                fsm_rt.push(FsmRt {
                    current_state:   initial_dt,
                    current_state_v: initial_val,
                    last_results:    Vec::new(),
                    halted:          false,
                    prev_values:     HashMap::new(),
                });
                per_fsm_solve.push(std::time::Duration::ZERO);
                per_fsm_ticks.push(0);
                pending_changes.push(std::collections::HashSet::new());
                had_effects_last.push(true);   // bootstrap: schedule on first tick
                state_changed_last.push(true); // ensure first-tick scheduling
                external_event.push(false);
            }
        }

        step_count += 1;

        // Exit takes priority over halt/event-wait.
        if ctx.exit_requested.is_some() {
            if timing {
                let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                    .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                    .collect();
                print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
            }
            return Ok(LoopResult {
                steps: step_count,
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        // No FSM scheduled this tick → would halt. Block on async event if available.
        if scheduled_this_tick.iter().all(|s| !s) && pending_world_writes.is_empty() {
            if let Some(rx) = event_rx {
                // If any FSM has explicit subscriptions, only wake matching FSMs.
                // Otherwise coarse-wake all alive FSMs for back-compat.
                let any_explicit = fsms.iter()
                    .any(|f| !f.event_subscriptions.is_empty());
                match rx.recv() {
                    Ok(crate::event_sources::SchedulerEvent::Tick { name }) => {
                        if env.trace {
                            eprintln!("[loop] tick {step_count}: woke on event {name}");
                        }
                        for (i, fsm) in fsms.iter().enumerate() {
                            if fsm_rt[i].halted { continue; }
                            let matches = if any_explicit {
                                fsm.event_subscriptions.contains(&name)
                            } else {
                                true  // coarse wake
                            };
                            if matches { external_event[i] = true; }
                        }
                        continue;
                    }
                    Ok(crate::event_sources::SchedulerEvent::Closed { .. }) | Err(_) => {
                        // All sources dead; fall through to halt.
                    }
                }
            }
            if timing {
                let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                    .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                    .collect();
                print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
            }
            return Ok(LoopResult {
                steps: step_count,
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }
    }

    if timing {
        let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
            .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
            .collect();
        print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
    }
    Ok(LoopResult {
        steps: step_count,
        final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
        halted_clean: false,
        exit_code: ctx.exit_requested,
    })
}
