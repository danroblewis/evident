//! Subscription-driven scheduler.
//!
//! Per tick:
//!   1. Solve writer(s) (if any), capture world_next.* values.
//!   2. Solve each reader with world.* pinned to writer's new values
//!      (or the previous tick's snapshot if no writer).
//!   3. Dispatch all FSMs' effects (writers first, readers in order).
//!   4. FSMs not woken by subscriptions are skipped this tick.
//! Program halts implicitly when no FSM is scheduled in a tick (and
//! no async event source can wake one), or when an FSM emits Exit.

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
    // Transitive access sets for the initial `fsms`, parallel-indexed.
    // Computed once in `run_with_ctx` (and reused for the multi-writer
    // check) so the costly self-hosted subscriptions walk runs at most
    // once per FSM at load — not twice. Spawned FSMs get a fresh walk.
    initial_access: &[crate::subscriptions::AccessSets],
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
    event_rx: Option<&std::sync::mpsc::Receiver<crate::event_sources::SchedulerEvent>>,
    event_sources: &mut [Box<dyn crate::event_sources::EventSource>],
    env: &LoopEnv,
) -> Result<LoopResult, String> {
    use std::collections::HashMap;
    // Convert to owned Vec so we can grow at runtime (Effect::SpawnFsm).
    let mut fsms: Vec<MainShape> = fsms.to_vec();
    // Per-FSM mutable state. We track BOTH the encoded Datatype
    // (for the next tick's pin) and the decoded Value (for halt
    // detection — fixpoint = state_next_val equals previous tick's
    // state value).
    struct FsmRt {
        current_state:   Option<z3::ast::Datatype<'static>>,
        current_state_v: Option<Value>,
        last_results:    Vec<EffectResult>,
        halted:          bool,
        /// Per-FSM cache of every variable's value at end of the
        /// previous tick. Used to pin `_name` references this tick
        /// (the runtime half of the `_var` time-shift convention —
        /// see docs/design/state-machines-as-relations.md).
        /// Empty on tick 0: `is_first_tick` will be pinned true.
        prev_values:     HashMap<String, Value>,
    }
    // ── Seed initial state ────────────────────────────────────
    // Seed each FSM's initial state to its enum's first variant. This
    // is convention: the first variant declared in `enum FooState =
    // Init | …` is the starting state. Without this pin, Z3 picks an
    // arbitrary satisfying state on tick 0 — often a Done state that
    // immediately self-loops with no effects, halting the FSM before
    // any work happens.
    //
    // Halt-check below only fires if state_next is variant-named
    // "Done"/"Halt", so the seeded Init pin doesn't cause spurious
    // halts (we never set current_state_v to a value matching that
    // pattern unless the user explicitly transitions there).
    // Closure: build the seeded initial state for an FSM's resolved
    // param info. Used for both startup-collected FSMs and
    // dynamically spawned ones.
    let seed_state = |s: &MainShape| -> (Option<z3::ast::Datatype<'static>>, Option<Value>) {
        let Some(state_type) = s.state_type.as_ref() else { return (None, None); };
        let enums = rt.enums_registry();
        let by_name = enums.by_name.borrow();
        let entry = by_name.get(state_type);
        // Only seed if the first variant is nullary. Payload
        // variants need actual values, which we don't have at
        // seed time — let Z3 pick on tick 0 instead.
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
    // Note: with a payload first-variant the FSM starts with no
    // pinned state; Z3 picks on tick 0. Document as a current
    // limitation if it bites — the workaround is to declare a
    // nullary state as the first variant.

    // ── World snapshot bootstrap ──────────────────────────────
    // Tick 0 starts with no shared world; the writer's body must
    // initialize world_next without depending on world (typically
    // via state-pattern guards: `state matches Init ⇒ world_next.x = …`).
    let mut world_snapshot: HashMap<String, Value> = HashMap::new();
    // Pre-populate plugin-managed fields with type defaults so
    // Z3 doesn't pick arbitrary values on tick 0 before any
    // plugin write has been applied. Without this, an FSM
    // reading `world.stdin_seq` on tick 0 would see an
    // unconstrained Int (any value Z3 chooses).
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
    // Per-FSM solve time + tick count, indexed parallel to `fsms`.
    let mut per_fsm_solve: Vec<std::time::Duration> = vec![std::time::Duration::ZERO; fsms.len()];
    let mut per_fsm_ticks: Vec<usize> = vec![0; fsms.len()];

    // ── Subscription scheduling state ─────────────────────────
    // Subscription-driven scheduling: see docs/design/fsm-subscriptions.md
    // for the full model. Access sets feed both scheduling decisions
    // (wake on read-set delta) and multi-writer snapshot scoping (each
    // writer's snapshot updates are limited to its own write-set so
    // disjoint writers don't clobber each other).
    // Transitive (passthrough-resolving) access sets — see
    // `fsm::full_world_access`. Using the local-only per-claim walk
    // (`portable::subscriptions::access_sets`) here would drop the
    // writes of any FSM that writes world through a `..Passthrough`
    // claim, since `my_writes` scoping below filters world_next
    // bindings against this set.
    // Reuse the sets computed once in `run_with_ctx` (see `initial_access`
    // doc on the signature). Owned so spawned FSMs can push their own.
    let mut access_sets: Vec<crate::subscriptions::AccessSets> = initial_access.to_vec();
    // Per-FSM "world fields that changed since I was last scheduled."
    // When the FSM is scheduled, this is consumed (cleared). Writers
    // populate it on other FSMs after their solve.
    let mut pending_changes: Vec<std::collections::HashSet<String>> =
        vec![std::collections::HashSet::new(); fsms.len()];
    // Self-feedback: did this FSM emit effects last tick? If so, it
    // has new last_results to consume → schedule it next.
    let mut had_effects_last: Vec<bool> = vec![false; fsms.len()];
    // State-change feedback: did this FSM transition to a new state
    // last tick? If so, schedule it next — the body can compute
    // different things when state pins to a new value, even if
    // world and last_results are unchanged. Without this, an FSM
    // that does Idle→Frame(N) on one tick (silently, no effects)
    // would never run its Frame(N) body.
    let mut state_changed_last: Vec<bool> = vec![false; fsms.len()];
    // External-event feedback: an async event source (e.g.
    // FrameTimer) fired since this FSM was last scheduled.
    // Currently we coarsely wake every FSM on every external
    // event — Phase 4 v3.5 will add per-FSM subscription matching.
    let mut external_event: Vec<bool> = vec![false; fsms.len()];
    // Local FIFO of plugin-queued world writes drained from
    // event sources. We apply one per tick so each change is
    // visible to subscribers; remaining entries wait for the
    // next tick. Prevents fast sources from collapsing many
    // values into "last wins."
    let mut pending_world_writes: std::collections::VecDeque<(String, Value)> =
        std::collections::VecDeque::new();

    // Clear any percolated-effect residue left by load-time / pre-loop
    // queries (e.g. an FTI install that resolved a `run`). From here on
    // the accumulator is drained per-FSM-per-tick (Phase 3b), so it stays
    // scoped to the FSM whose body solved the `run`.
    let _ = crate::runtime::take_percolated_effects();

    // ── Phase 3: tick loop ────────────────────────────────────
    while step_count < opts.max_steps {
        // Any active FSMs left? If not, program halted.
        if fsm_rt.iter().all(|f| f.halted) {
            if timing {
                let rows: Vec<(&str, std::time::Duration, usize)> = fsms.iter().enumerate()
                    .map(|(i, f)| (f.claim_name.as_str(), per_fsm_solve[i], per_fsm_ticks[i]))
                    .collect();
                print_timing_summary_full(loop_t0, step_count, total_solve, total_dispatch, &rows);
            }
            return Ok(LoopResult {
                steps: step_count,
                // Synthesize a final-state value from the writer's
                // last seen state if available; otherwise the first
                // active FSM's. Multi-FSM doesn't have a single
                // "final_state" the way single-FSM does, so this is
                // best-effort.
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
                exit_code: ctx.exit_requested,
            });
        }

        // ── Phase 3a: drain plugin writes into world ──────────
        // Drain plugin world writes — applying ONE entry per tick
        // (so subscribers see each individual change with its own
        // wake). Sources may produce writes faster than ticks can
        // consume them; we move source-side queues into a local
        // FIFO so nothing is lost.
        for src in event_sources.iter_mut() {
            let writes = src.drain_writes();
            pending_world_writes.append(&mut writes.into_iter().collect());
        }
        // Dual policy: STATE writes (dotted keys, FTI) apply
        // ALL queued values immediately — only the latest
        // matters; intermediate values would be invisible
        // anyway because the FSM only solves once per tick.
        // EVENT writes (bare keys, world reserved fields)
        // apply ONE per tick — each individual value matters
        // (e.g. each stdin line is a discrete event).
        //
        // For FTI: a bridge writing 5 values between ticks
        // collapses to "the latest count," consistent with
        // the field's role as continuous state.
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
        // Re-queue event writes for the one-per-tick draining.
        pending_world_writes = event_writes;

        // Apply all state writes (last value wins per key).
        // For FTI keys (`<fsm>.<param>.<field>`), wake the
        // matching FSM if its access_sets include the
        // stripped `<param>.<field>` (or its first segment,
        // <param>, since access_sets stores top-level field
        // names from `world.X` reads but FTI param-field
        // reads land directly in env without expansion).
        for (field, val) in state_writes {
            let key = field.clone();  // dotted, used as-is
            let changed = world_snapshot.get(&key) != Some(&val);
            if changed {
                world_snapshot.insert(key.clone(), val);
                // FTI wake distribution: if the key matches
                // `<claim>.<rest>` for some FSM's claim_name,
                // wake that FSM.
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
            // Bare field name → world.X pin.
            let key = format!("world.{field}");
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

        // ── Phase 3b: per-FSM solve ───────────────────────────
        // Per-tick effect ordering: writer first, then readers in
        // declaration order (which is the order in `fsms`).
        let mut all_effects: Vec<(usize, Vec<crate::core::ast::Effect>)> = Vec::new();
        // Track which FSMs we actually scheduled this tick — used
        // for clearing self-feedback flags at the end.
        let mut scheduled_this_tick: Vec<bool> = vec![false; fsms.len()];

        for (idx, fsm) in fsms.iter().enumerate() {
            if fsm_rt[idx].halted { continue; }

            // Scheduling decision. Four triggers wake an FSM:
            //   1. Bootstrap (tick 0)
            //   2. Self-feedback: emitted effects last tick → fresh
            //      last_results to consume.
            //   3. World delta: a field in the FSM's read-set was
            //      written since this FSM was last scheduled.
            //   4. State change: transitioned to a new state value.
            // All others stay asleep this tick. `pending_changes` is
            // cleared on schedule (events consumed).
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

            // Build per-FSM pin list (state as Datatype; last_results
            // goes through the given map below as a Seq(Result)).
            let pins: Vec<(&str, z3::ast::Datatype<'static>)> = match (&fsm.state_var, &fsm_rt[idx].current_state) {
                (Some(name), Some(s)) => vec![(name.as_str(), s.clone())],
                _ => vec![],
            };

            // Build per-FSM view of the snapshot: include all
            // world.X entries as-is, plus FTI keys whose prefix
            // matches THIS fsm's claim_name (with prefix stripped
            // so they match env's `param.field` keys). Also include
            // last_results as a Seq(Result) — pinned via the given
            // map's assert_seq_given path.
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
            // Time-shift convention: for every `_name` in this fsm's
            // body whose `name` we have a previous-tick value for,
            // pin `_name` to that value. Also pin `is_first_tick`
            // (true iff we have no previous values yet — i.e., tick
            // 0 for this fsm). See docs/design/state-machines-as-
            // relations.md for the framing.
            if let Some(claim) = rt.get_schema(&fsm.claim_name) {
                let is_first = fsm_rt[idx].prev_values.is_empty();
                let mut sees_underscore = false;
                for item in &claim.body {
                    if let crate::core::ast::BodyItem::Membership { name, .. } = item {
                        if let Some(stripped) = name.strip_prefix('_') {
                            sees_underscore = true;
                            // Primitive case: prev_values has a direct
                            // entry for `stripped` (Int / Bool / etc.).
                            if let Some(prev) = fsm_rt[idx].prev_values.get(stripped) {
                                fsm_view.insert(name.clone(), prev.clone());
                            }
                            // Record case: prev_values has per-field
                            // entries like `pos.x` / `pos.y` (records
                            // get flattened at translation). Mirror
                            // every `stripped.<field>` entry into
                            // `_name.<field>` so `_pos.x` resolves.
                            let prefix = format!("{stripped}.");
                            for (k, v) in &fsm_rt[idx].prev_values {
                                if let Some(field) = k.strip_prefix(&prefix) {
                                    fsm_view.insert(
                                        format!("{name}.{field}"),
                                        v.clone(),
                                    );
                                }
                            }
                            // If no previous value yet, leave `_name`
                            // unconstrained — the fsm's body should
                            // gate via `is_first_tick`.
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
            // Also expose state_var's CURRENT-TICK Value in fsm_view
            // so the function-izer fast-path (which reads given, not
            // pins) sees the state. Z3 ignores duplicate equality
            // constraints, so this is safe to leave in alongside the
            // existing Datatype pin.
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

            // Read state_next + effects when those slots exist.
            let state_next_val: Option<&Value> = match &fsm.state_next_var {
                Some(sn) => Some(r.bindings.get(sn)
                    .ok_or_else(|| format!("FSM `{}` step {step_count}: model has no `{}`",
                        fsm.claim_name, sn))?),
                None => None,
            };
            // Walk the entire model for dispatchable Effect / Seq(Effect)
            // bindings (see collect_dispatchable_effects). Same ordering
            // rules apply per FSM: legacy `effects` Seq first if present,
            // then other Effect-typed bindings dedup'd by value.
            let effects = collect_dispatchable_effects(rt, &fsm.claim_name,
                &r.bindings, fsm.effects_var.as_deref());

            // Child-FSM effects percolation (session RR). If this FSM's
            // body called `run(F, init)` and F emitted effects, those
            // were CAPTURED, not dispatched, during the resolve (the
            // child is a pure function — runtime/nested.rs). Drain them
            // here and dispatch them as part of THIS (the parent's) tick,
            // child effects first (the run produced the value before the
            // parent decided its own effects), then the parent's own.
            // Single dispatch, in child-tick order, by the parent. The
            // drain also empties the accumulator for the next FSM.
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

            // Halt is implicit under subscription scheduling: FSMs that
            // fixpoint just stop being scheduled (no inputs to wake
            // them); the program halts when no FSM is scheduled at all
            // in a tick.

            // Writer? Capture world_next.* for snapshot. The snapshot
            // becomes the `world.*` given for subsequent FSM solves
            // this tick AND the writer's own world.* given next tick.
            //
            // Phase 2: also compute the field-level delta (which
            // fields actually changed value) and distribute to other
            // FSMs whose read-set includes a changed field. The
            // writer is excluded from its own deltas — own writes
            // shouldn't self-schedule (Phase 1 discovery).
            //
            // Multi-writer (Phase 4 v3.7+): each writer MERGES its
            // own world_next.X fields into the snapshot rather than
            // clearing it. Writers' write-sets are disjoint
            // (enforced at load), so this is well-defined. Within
            // a tick, writers run in declaration order (writers
            // first via all_fsms ordering); a later writer's
            // body sees the earlier writers' just-written fields.
            if fsm.is_writer() {
                let mut just_changed: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                // Only consume fields that this writer actually
                // owns (its write-set). Z3 may produce world_next
                // bindings for fields outside the write-set if the
                // body references them; ignoring those keeps each
                // writer scoped to its own fields.
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

            // Mark whether this solve transitioned to a new state.
            // Drives the state-change wake trigger next tick.
            state_changed_last[idx] = match state_next_val {
                Some(snv) => fsm_rt[idx].current_state_v.as_ref()
                    .map(|prev| prev != snv).unwrap_or(true),
                None => false,
            };

            // Update next-tick state for this FSM (only when this fsm
            // has a state-pair).
            if let Some(snv) = state_next_val {
                fsm_rt[idx].current_state = encode_state_value(rt, snv);
                fsm_rt[idx].current_state_v = Some(snv.clone());
            }

            // Capture every non-prefix variable's bound value for
            // the next tick's `_name` pinning. The underscore-prefix
            // bindings themselves (and the `is_first_tick` flag)
            // are skipped — they're rebuilt fresh each tick.
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

        // ── Phase 3c: dispatch all effects ────────────────────
        // Dispatch all effects in order, capturing each FSM's
        // results into its own last_results for next tick. Also
        // update the per-FSM self-feedback flag — true iff this FSM
        // emitted at least one effect this tick (so its
        // last_results will be fresh next tick).
        let dispatch_t0 = std::time::Instant::now();
        // Reset self-feedback for FSMs we scheduled this tick;
        // unscheduled ones keep whatever they had (they didn't
        // observe last_results yet).
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

        // ── Phase 3d: handle Effect::SpawnFsm ─────────────────
        // Effect::SpawnFsm handling: any spawn requests
        // accumulated during dispatch get instantiated as new
        // FsmRt entries here. They join the scheduler from the
        // next tick. v1: shares the parent's world; no
        // per-instance world. See docs/design/fsm-spawning.md.
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
                // World-less FSMs have provably-empty access sets — skip
                // the (costly) self-hosted walk, mirroring the load-time
                // guard in `run_with_ctx`.
                let aset = if shape.world_type.is_none() {
                    crate::subscriptions::AccessSets::default()
                } else {
                    rt.get_schema(&shape.claim_name)
                        .map(crate::portable::subscriptions::access_sets)
                        .unwrap_or_default()
                };
                // Spawn-arg seeding: pin the new FSM's state to
                // `FirstVariant(spawn_arg)` if the first variant
                // takes a single Int payload. Otherwise fall back
                // to the regular seed (nullary first variant) or
                // None (Z3 picks).
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
                had_effects_last.push(true);   // bootstrap-equivalent
                state_changed_last.push(true); // ensure first-tick scheduling
                external_event.push(false);
            }
        }

        step_count += 1;

        // ── Phase 3e: exit + halt + event-wait ────────────────
        // Effect::Exit handling: checked first — works in both
        // legacy and delta mode, takes priority over the no-FSM
        // halt and over event-wait.
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

        // Halt criterion: if no FSM was scheduled this tick, no
        // work happened — and since scheduling decisions are
        // deterministic from world deltas + self-feedback +
        // state-feedback, no work would happen next tick either.
        // Halt cleanly UNLESS an async event source can wake us:
        // block on the channel, then continue the loop on the
        // next event.
        if scheduled_this_tick.iter().all(|s| !s) && pending_world_writes.is_empty() {
            if let Some(rx) = event_rx {
                // Per-FSM event subscription matching. If ANY FSM
                // declared an explicit subscription, only wake FSMs
                // whose subscription set contains the event's name.
                // If NO FSM declared any subscription, fall back to
                // coarse wake (every alive FSM) for v3 back-compat.
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
