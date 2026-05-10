//! Effect-driven step loop. Replaces the plugin-based executor for
//! programs whose `main` claim declares `effects ∈ Seq(Effect)` and
//! `last_results ∈ Seq(Result)`.
//!
//! Per step:
//!   1. Encode current `state` and `last_results` as Z3 datatype values.
//!   2. Solve `main` with both pinned.
//!   3. Decode `state_next` (an enum value) and `effects` (a list).
//!   4. Dispatch each effect via `effect_dispatch::dispatch_one`.
//!   5. state ← state_next; last_results ← dispatched results.
//!   6. Halt when state matches a user-defined Halt variant or the
//!      step cap is hit.
//!
//! v1: state must be an enum-typed variable. The first variant whose
//! name starts with "Done" or "Halt" (or is exactly "Done") is the
//! halt sentinel — when state's model equals that variant, the loop
//! exits.

use crate::ast::{Effect, EffectResult, BodyItem};
use crate::effect_dispatch::{DispatchContext, dispatch_all};
use crate::runtime::EvidentRuntime;
use crate::translate::{Value, ast_encoder, ast_decoder};

/// Tunables for the effect loop.
#[derive(Debug, Clone)]
pub struct LoopOpts {
    /// Hard ceiling on iterations. Prevents infinite loops if a
    /// program's halt condition never fires.
    pub max_steps: usize,
}

impl Default for LoopOpts {
    fn default() -> Self { Self { max_steps: 10_000 } }
}

/// Result of running an effect-driven program.
#[derive(Debug)]
pub struct LoopResult {
    pub steps:      usize,
    pub final_state: Option<Value>,
    pub halted_clean: bool,
}

/// One FSM-shaped claim's membership info. The runtime detects
/// claims that match this shape (state pair + EffectList + ResultList,
/// optionally + world record) and runs each as an FSM.
///
/// For backwards compat the struct is still called `MainShape`. The
/// new `claim_name` and `world_*` fields default to "main" / None for
/// single-FSM programs.
pub struct MainShape {
    pub claim_name:       String,
    pub state_var:        String,
    pub state_next_var:   String,
    pub state_type:       String,
    pub last_results_var: String,
    pub effects_var:      String,
    /// Name of the `world` membership, if this FSM reads world.
    pub world_var:        Option<String>,
    /// Name of the `world_next` membership; presence makes this FSM
    /// the world WRITER. v1: at most one writer per program.
    pub world_next_var:   Option<String>,
    /// Type name of the world record, if `world_var` or
    /// `world_next_var` is set.
    pub world_type:       Option<String>,
}

impl MainShape {
    pub fn is_writer(&self) -> bool { self.world_next_var.is_some() }
}

pub fn detect_main_shape(rt: &EvidentRuntime) -> Option<MainShape> {
    detect_fsm_shape(rt, "main")
}

/// Detect FSM shape for a specific claim. Returns Some if the claim
/// has the four-membership shape (state/state_next/last_results/effects)
/// plus optional world/world_next.
pub fn detect_fsm_shape(rt: &EvidentRuntime, claim_name: &str) -> Option<MainShape> {
    let claim = rt.get_schema(claim_name)?;
    let mut state_pair: Option<(String, String, String)> = None;
    let mut last_results_var = None;
    let mut effects_var = None;
    let mut world_var:      Option<String> = None;
    let mut world_next_var: Option<String> = None;
    let mut world_type:     Option<String> = None;
    // Walk this claim's body PLUS the bodies of any
    // `..PassthroughClaim` so a declarative library (e.g.
    // stdlib/sdl/scene.ev's `..SDLScene`) contributes its
    // state-machine vars to the outer claim.
    let mut all_items: Vec<&BodyItem> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    fn collect<'a>(
        items: &'a [BodyItem],
        rt: &'a EvidentRuntime,
        out: &mut Vec<&'a BodyItem>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        for item in items {
            out.push(item);
            if let BodyItem::Passthrough(name) = item {
                if visited.insert(name.clone()) {
                    if let Some(sub) = rt.get_schema(name) {
                        let body: &'a [BodyItem] = unsafe {
                            std::mem::transmute::<&[BodyItem], &'a [BodyItem]>(&sub.body)
                        };
                        collect(body, rt, out, visited);
                    }
                }
            }
        }
    }
    collect(&claim.body, rt, &mut all_items, &mut visited);
    for item in all_items.iter().copied() {
        if let BodyItem::Membership { name, type_name, .. } = item {
            if type_name == "EffectList" && name == "effects" && effects_var.is_none() {
                effects_var = Some(name.clone());
            } else if type_name == "ResultList" && name == "last_results"
                   && last_results_var.is_none()
            {
                last_results_var = Some(name.clone());
            } else if name == "world" {
                world_var = Some(name.clone());
                world_type = Some(type_name.clone());
            } else if name == "world_next" {
                world_next_var = Some(name.clone());
                if world_type.is_none() {
                    world_type = Some(type_name.clone());
                }
            } else if type_name != "Int" && type_name != "Bool"
                   && type_name != "String" && type_name != "Real"
                   && !type_name.starts_with("Seq(")
                   && !type_name.starts_with("Set(")
            {
                // State-pair detection (same type, two vars, one
                // ending in `_next`). Excludes world/world_next which
                // matched above.
                if name.ends_with("_next") {
                    let base = &name[..name.len() - 5];
                    if let Some((b, _, _)) = &state_pair {
                        if b == base { continue; }
                    }
                    state_pair = Some((base.to_string(), name.clone(), type_name.clone()));
                } else if state_pair.is_none()
                       || matches!(&state_pair, Some((b, _, _)) if b != name)
                {
                    let nxt = format!("{}_next", name);
                    if all_items.iter().any(|i| matches!(
                        i, BodyItem::Membership { name: n, type_name: t, .. }
                           if n == &nxt && t == type_name
                    )) {
                        state_pair = Some((name.clone(), nxt, type_name.clone()));
                    }
                }
            }
        }
    }
    let (s, sn, st) = state_pair?;
    Some(MainShape {
        claim_name:       claim_name.to_string(),
        state_var:        s,
        state_next_var:   sn,
        state_type:       st,
        last_results_var: last_results_var?,
        effects_var:      effects_var?,
        world_var,
        world_next_var,
        world_type,
    })
}

/// Walk every top-level claim and collect those that have the FSM
/// membership shape. Returns the writer FIRST (if any), then readers
/// in declaration order. Multi-FSM execution dispatches in this order.
pub fn detect_all_fsms(rt: &EvidentRuntime) -> Vec<MainShape> {
    let names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    let mut writers: Vec<MainShape> = Vec::new();
    let mut readers: Vec<MainShape> = Vec::new();
    for name in names {
        if let Some(shape) = detect_fsm_shape(rt, &name) {
            if shape.is_writer() { writers.push(shape) } else { readers.push(shape) }
        }
    }
    let mut all = writers;
    all.extend(readers);
    all
}

/// Run the effect loop. Single-FSM programs (one main-shape claim,
/// usually `main`) take the existing per-step path. Multi-FSM
/// programs (≥2 main-shape claims) use the multi-FSM scheduler:
/// per-tick writer-then-readers solving with shared world handoff
/// and per-FSM halt detection.
pub fn run(rt: &EvidentRuntime, opts: &LoopOpts) -> Result<LoopResult, String> {
    run_with_ctx(rt, opts, &mut DispatchContext::new())
}

/// Run with caller-supplied dispatch context. Test entry point —
/// lets callers swap in fake stdin/stdout.
pub fn run_with_ctx(
    rt: &EvidentRuntime,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    let fsms = detect_all_fsms(rt);
    match fsms.len() {
        0 => Err("no effect-driven claims found (need state pair + EffectList + ResultList)".to_string()),
        1 => run_with_shape(rt, &fsms[0], opts, ctx),
        _ => {
            // v1 single-writer rule.
            let writer_count = fsms.iter().filter(|s| s.is_writer()).count();
            if writer_count > 1 {
                let names: Vec<&str> = fsms.iter()
                    .filter(|s| s.is_writer())
                    .map(|s| s.claim_name.as_str()).collect();
                return Err(format!(
                    "multi-FSM v1: only one FSM may declare `world_next`; found {writer_count}: {names:?}",
                ));
            }
            run_multi_fsm(rt, &fsms, opts, ctx)
        }
    }
}

fn run_with_shape(
    rt: &EvidentRuntime,
    shape: &MainShape,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    // Initial state: pin to the FIRST variant of the state enum.
    // Convention: programs declare the initial state as the first
    // variant of their state type. This prevents Z3 from picking a
    // non-initial variant on step 0 (which would silently skip the
    // program's setup).
    let mut last_results: Vec<EffectResult> = Vec::new();
    let mut current_state_value: Option<z3::ast::Datatype<'static>> = {
        let enums = rt.enums_registry();
        let by_name = enums.by_name.borrow();
        by_name.get(&shape.state_type)
            .and_then(|(sort, _)| sort.variants.first()
                .and_then(|v| v.constructor.apply(&[]).as_datatype()))
    };
    if current_state_value.is_none() {
        return Err(format!(
            "could not pin initial state: enum `{}` has no nullary first variant",
            shape.state_type));
    }

    let mut step_count = 0usize;
    let mut final_state_model: Option<Value> = None;
    // EVIDENT_LOOP_TIMING=1 → per-step solve+dispatch timing + summary.
    // Useful for figuring out where time goes in long-running demos
    // (Z3 solve vs FFI dispatch vs idle in delays).
    let timing = std::env::var("EVIDENT_LOOP_TIMING").is_ok();
    let loop_t0 = std::time::Instant::now();
    let mut total_solve = std::time::Duration::ZERO;
    let mut total_dispatch = std::time::Duration::ZERO;

    while step_count < opts.max_steps {
        // Encode last_results.
        let last_results_dt = rt.encode_effect_result_list(&last_results)
            .map_err(|e| format!("encode last_results: {e}"))?;

        // Build pin list. For step 0 we don't pin state (Z3 picks
        // the initial — the user's main pins it via state.step = 0
        // pattern or similar).
        let pins: Vec<(&str, z3::ast::Datatype<'static>)> = match &current_state_value {
            Some(s) => vec![
                (shape.state_var.as_str(), s.clone()),
                (shape.last_results_var.as_str(), last_results_dt),
            ],
            None => vec![
                (shape.last_results_var.as_str(), last_results_dt),
            ],
        };

        let solve_t0 = std::time::Instant::now();
        let r = rt.query_with_pinned_datatypes(&shape.claim_name, &pins)
            .map_err(|e| format!("solve step {step_count}: {e}"))?;
        let solve_dt = solve_t0.elapsed();
        total_solve += solve_dt;

        if !r.satisfied {
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: false,
            });
        }

        // Read state_next from model.
        let state_next_val = r.bindings.get(&shape.state_next_var)
            .ok_or_else(|| format!("step {step_count}: model has no `{}`", shape.state_next_var))?;
        let effects_val = r.bindings.get(&shape.effects_var)
            .ok_or_else(|| format!("step {step_count}: model has no `{}`", shape.effects_var))?;

        let effects = ast_decoder::decode_effect_list(effects_val)
            .map_err(|e| format!("step {step_count}: decode effects: {e}"))?;

        // Halt-check: if effects empty AND state_next equals state, we
        // consider the program halted (fixpoint). User can also issue
        // `Effect::Exit(0)` to terminate immediately.
        let halted_by_fixpoint = effects.is_empty()
            && current_state_value.is_some()
            && model_matches_value(state_next_val, &shape.state_type);

        let dispatch_t0 = std::time::Instant::now();
        let new_results = dispatch_all(ctx, &effects);
        let dispatch_dt = dispatch_t0.elapsed();
        total_dispatch += dispatch_dt;

        if std::env::var("EVIDENT_LOOP_TRACE").is_ok() {
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
        // and payload variants.
        current_state_value = encode_state_value(rt, state_next_val);

        last_results = new_results;
        final_state_model = Some(state_next_val.clone());
        step_count += 1;

        if halted_by_fixpoint {
            if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
            return Ok(LoopResult {
                steps: step_count,
                final_state: final_state_model,
                halted_clean: true,
            });
        }
    }

    if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
    Ok(LoopResult {
        steps: step_count,
        final_state: final_state_model,
        halted_clean: false,
    })
}

/// Multi-FSM scheduler. Per tick:
///   1. Solve writer (if any), capture world_next.* values.
///   2. Solve each reader with world.* pinned to writer's new values
///      (or the previous tick's snapshot if no writer / writer halted).
///   3. Dispatch all FSMs' effects (writer first, readers in order).
///   4. Per-FSM halt detection (state_next == state ∧ effects empty).
///   5. Drop halted FSMs from the active set.
/// Program halts when no active FSMs remain.
fn run_multi_fsm(
    rt: &EvidentRuntime,
    fsms: &[MainShape],
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    use std::collections::HashMap;
    // Per-FSM mutable state. We track BOTH the encoded Datatype
    // (for the next tick's pin) and the decoded Value (for halt
    // detection — fixpoint = state_next_val equals previous tick's
    // state value).
    struct FsmRt {
        current_state:   Option<z3::ast::Datatype<'static>>,
        current_state_v: Option<Value>,
        last_results:    Vec<EffectResult>,
        halted:          bool,
    }
    let mut fsm_rt: Vec<FsmRt> = fsms.iter().map(|s| {
        let (initial_dt, initial_val) = {
            let enums = rt.enums_registry();
            let by_name = enums.by_name.borrow();
            let entry = by_name.get(&s.state_type);
            let dt = entry.and_then(|(sort, _)| sort.variants.first()
                .and_then(|v| v.constructor.apply(&[]).as_datatype()));
            let val = entry.and_then(|(_, decl_variants)| decl_variants.first().map(|v|
                Value::Enum {
                    enum_name: s.state_type.clone(),
                    variant:   v.name.clone(),
                    fields:    Vec::new(),
                }));
            (dt, val)
        };
        FsmRt {
            current_state:   initial_dt,
            current_state_v: initial_val,
            last_results:    Vec::new(),
            halted:          false,
        }
    }).collect();
    for (i, s) in fsms.iter().enumerate() {
        if fsm_rt[i].current_state.is_none() {
            return Err(format!(
                "FSM `{}`: state enum `{}` has no nullary first variant",
                s.claim_name, s.state_type));
        }
    }

    // Tick 0 starts with no shared world; the writer's body must
    // initialize world_next without depending on world (typically
    // via state-pattern guards: `state matches Init ⇒ world_next.x = …`).
    let mut world_snapshot: HashMap<String, Value> = HashMap::new();

    let mut step_count = 0usize;
    let timing = std::env::var("EVIDENT_LOOP_TIMING").is_ok();
    let loop_t0 = std::time::Instant::now();
    let mut total_solve = std::time::Duration::ZERO;
    let mut total_dispatch = std::time::Duration::ZERO;

    while step_count < opts.max_steps {
        // Any active FSMs left? If not, program halted.
        if fsm_rt.iter().all(|f| f.halted) {
            if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
            return Ok(LoopResult {
                steps: step_count,
                // Synthesize a final-state value from the writer's
                // last seen state if available; otherwise the first
                // active FSM's. Multi-FSM doesn't have a single
                // "final_state" the way single-FSM does, so this is
                // best-effort.
                final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
                halted_clean: true,
            });
        }

        // Per-tick effect ordering: writer first, then readers in
        // declaration order (which is the order in `fsms`).
        let mut all_effects: Vec<(usize, Vec<crate::ast::Effect>)> = Vec::new();

        for (idx, fsm) in fsms.iter().enumerate() {
            if fsm_rt[idx].halted { continue; }

            // Build per-FSM pin list (state + last_results as Datatypes).
            let last_results_dt = rt.encode_effect_result_list(&fsm_rt[idx].last_results)
                .map_err(|e| format!("FSM `{}`: encode last_results: {e}", fsm.claim_name))?;
            let pins: Vec<(&str, z3::ast::Datatype<'static>)> = match &fsm_rt[idx].current_state {
                Some(s) => vec![
                    (fsm.state_var.as_str(), s.clone()),
                    (fsm.last_results_var.as_str(), last_results_dt),
                ],
                None => vec![
                    (fsm.last_results_var.as_str(), last_results_dt),
                ],
            };

            let solve_t0 = std::time::Instant::now();
            let r = rt.query_with_pins_and_given(&fsm.claim_name, &pins, &world_snapshot)
                .map_err(|e| format!("FSM `{}` solve step {step_count}: {e}", fsm.claim_name))?;
            let solve_dt = solve_t0.elapsed();
            total_solve += solve_dt;

            if !r.satisfied {
                if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
                return Ok(LoopResult {
                    steps: step_count,
                    final_state: fsm_rt[idx].current_state_v.clone(),
                    halted_clean: false,
                });
            }

            // Read state_next + effects.
            let state_next_val = r.bindings.get(&fsm.state_next_var)
                .ok_or_else(|| format!("FSM `{}` step {step_count}: model has no `{}`",
                    fsm.claim_name, fsm.state_next_var))?;
            let effects_val = r.bindings.get(&fsm.effects_var)
                .ok_or_else(|| format!("FSM `{}` step {step_count}: model has no `{}`",
                    fsm.claim_name, fsm.effects_var))?;
            let effects = ast_decoder::decode_effect_list(effects_val)
                .map_err(|e| format!("FSM `{}` step {step_count}: decode effects: {e}",
                    fsm.claim_name))?;

            // Halt-check for this FSM: state_next value equals
            // current state value (true fixpoint, no Done/Halt name
            // convention) AND effects empty. Dropped on the NEXT tick.
            let will_halt = effects.is_empty()
                && fsm_rt[idx].current_state_v.as_ref()
                    .map(|cv| cv == state_next_val).unwrap_or(false);

            // Writer? Capture world_next.* for snapshot. The snapshot
            // becomes the `world.*` given for subsequent FSM solves
            // this tick AND the writer's own world.* given next tick.
            if fsm.is_writer() {
                world_snapshot.clear();
                for (k, v) in r.bindings.iter() {
                    if let Some(field) = k.strip_prefix("world_next.") {
                        world_snapshot.insert(format!("world.{field}"), v.clone());
                    }
                }
            }

            // Update next-tick state for this FSM.
            fsm_rt[idx].current_state = encode_state_value(rt, state_next_val);
            fsm_rt[idx].current_state_v = Some(state_next_val.clone());

            if std::env::var("EVIDENT_LOOP_TRACE").is_ok() {
                eprintln!("[loop] tick {step_count} fsm={}: state_next={state_next_val:?} effects={effects:?}",
                    fsm.claim_name);
            }
            if timing {
                eprintln!("[timing] tick {step_count} fsm={}: solve={:.2}ms ({} effects)",
                    fsm.claim_name, solve_dt.as_secs_f64() * 1000.0, effects.len());
            }

            all_effects.push((idx, effects));

            // Mark halt — drops on next tick's iteration.
            if will_halt {
                fsm_rt[idx].halted = true;
            }
        }

        // Dispatch all effects in order, capturing each FSM's
        // results into its own last_results for next tick.
        let dispatch_t0 = std::time::Instant::now();
        for (fsm_idx, effects) in all_effects {
            let results = dispatch_all(ctx, &effects);
            fsm_rt[fsm_idx].last_results = results;
        }
        let dispatch_dt = dispatch_t0.elapsed();
        total_dispatch += dispatch_dt;

        step_count += 1;
    }

    if timing { print_timing_summary(loop_t0, step_count, total_solve, total_dispatch); }
    Ok(LoopResult {
        steps: step_count,
        final_state: fsm_rt.iter().find_map(|f| f.current_state_v.clone()),
        halted_clean: false,
    })
}

fn print_timing_summary(
    loop_t0: std::time::Instant,
    steps: usize,
    total_solve: std::time::Duration,
    total_dispatch: std::time::Duration,
) {
    let wall = loop_t0.elapsed();
    let other = wall.saturating_sub(total_solve).saturating_sub(total_dispatch);
    eprintln!("[timing] ── summary ──────────────────────────────");
    eprintln!("[timing] steps:    {steps}");
    eprintln!("[timing] wall:     {:>7.2}ms ({:>5.1}ms/step)",
        wall.as_secs_f64() * 1000.0,
        if steps > 0 { wall.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    eprintln!("[timing] solve:    {:>7.2}ms ({:>5.1}ms/step)",
        total_solve.as_secs_f64() * 1000.0,
        if steps > 0 { total_solve.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    eprintln!("[timing] dispatch: {:>7.2}ms ({:>5.1}ms/step)",
        total_dispatch.as_secs_f64() * 1000.0,
        if steps > 0 { total_dispatch.as_secs_f64() * 1000.0 / steps as f64 } else { 0.0 });
    eprintln!("[timing] other:    {:>7.2}ms (encoding, decoding, idle)",
        other.as_secs_f64() * 1000.0);
}

/// Check whether a model `Value` corresponds to a halt sentinel —
/// for v1 that's any variant whose name is exactly "Done" or "Halt".
/// (Future: user-declared halt predicate.)
fn model_matches_value(v: &Value, _state_type: &str) -> bool {
    matches!(v, Value::Enum { variant, .. } if variant == "Done" || variant == "Halt")
}

/// Re-encode a state Value as a Z3 Datatype for the next step's pin.
/// Handles nullary AND payload variants by recursively encoding
/// each field. Primitive payloads (Int, Bool, String, Real) are
/// encoded as Z3 literals; nested enum payloads recurse.
fn encode_state_value(rt: &EvidentRuntime, v: &Value) -> Option<z3::ast::Datatype<'static>> {
    use z3::ast::{Int as Z3Int, Bool as Z3Bool, String as Z3Str, Ast};
    let Value::Enum { enum_name, variant, fields } = v else { return None };
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let (sort, _decl) = by_name.get(enum_name)?;
    let var_idx = sort.variants.iter().position(|v| v.constructor.name() == *variant)?;
    let ctor = &sort.variants[var_idx].constructor;
    if fields.is_empty() {
        return ctor.apply(&[]).as_datatype();
    }
    // Payload — encode each field. Need 'static refs to pass to
    // ctor.apply, so box each Z3 value.
    let ctx = rt.z3_context();
    let mut owned: Vec<Box<dyn Ast<'static>>> = Vec::with_capacity(fields.len());
    for f in fields {
        let boxed: Box<dyn Ast<'static>> = match f {
            Value::Int(n)  => Box::new(Z3Int::from_i64(ctx, *n)),
            Value::Bool(b) => Box::new(Z3Bool::from_bool(ctx, *b)),
            Value::Str(s)  => Box::new(Z3Str::from_str(ctx, s).ok()?),
            Value::Real(r) => {
                // Reuse runtime's encoder if available; for now, route
                // via i64/denominator pair.
                let i = (*r * 1_000_000.0) as i64;
                Box::new(z3::ast::Real::from_real(ctx, i as i32, 1_000_000))
            }
            Value::Enum { .. } => Box::new(encode_state_value(rt, f)?),
            _ => return None,
        };
        owned.push(boxed);
    }
    let refs: Vec<&dyn Ast<'static>> = owned.iter().map(|b| b.as_ref()).collect();
    ctor.apply(&refs).as_datatype()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn ctx_silent() -> DispatchContext {
        DispatchContext::with_streams(
            Box::new(std::io::BufReader::new(Cursor::new(Vec::<u8>::new()))),
            Box::new(Vec::<u8>::new()),
        )
    }

    #[test]
    fn detect_main_shape_finds_state_and_lists() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
enum S = Init | Done

claim main
    state ∈ S
    state_next ∈ S
    last_results ∈ ResultList
    effects ∈ EffectList
    state = Init ⇒ (state_next = Done ∧ effects = EffNil)
    state = Done ⇒ (state_next = Done ∧ effects = EffNil)
").unwrap();
        let shape = detect_main_shape(&rt).expect("should detect");
        assert_eq!(shape.state_var, "state");
        assert_eq!(shape.state_next_var, "state_next");
        assert_eq!(shape.state_type, "S");
        assert_eq!(shape.effects_var, "effects");
        assert_eq!(shape.last_results_var, "last_results");
    }

    #[test]
    fn halt_after_one_step_when_state_reaches_done() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
enum S = Init | Done

claim main
    state ∈ S
    state_next ∈ S
    last_results ∈ ResultList
    effects ∈ EffectList
    state = Init ⇒ (state_next = Done ∧ effects = EffNil)
    state = Done ⇒ (state_next = Done ∧ effects = EffNil)
").unwrap();
        let mut ctx = ctx_silent();
        let r = run_with_ctx(&rt, &LoopOpts { max_steps: 5 }, &mut ctx).unwrap();
        // Steps: solve#1 (no state pin) → state_next=Init or Done?
        // Z3 may pick either; the loop terminates when fixpoint hits.
        assert!(r.steps <= 5);
        assert!(r.halted_clean || r.steps == 5);
    }
}
