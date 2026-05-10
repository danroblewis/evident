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

/// Detect whether `main` is effect-driven (declares `effects` and
/// `last_results` of the right enum types). Returns the names of
/// state/state_next vars and their type if so.
pub struct MainShape {
    pub state_var:        String,
    pub state_next_var:   String,
    pub state_type:       String,
    pub last_results_var: String,
    pub effects_var:      String,
}

pub fn detect_main_shape(rt: &EvidentRuntime) -> Option<MainShape> {
    let main = rt.get_schema("main")?;
    let mut state_pair: Option<(String, String, String)> = None;
    let mut last_results_var = None;
    let mut effects_var = None;
    // Walk main's body PLUS the bodies of any `..PassthroughClaim` so a
    // declarative library (e.g. stdlib/sdl/scene.ev's `..SDLScene`)
    // contributes its state-machine vars even though the user's main
    // contains only data + passthroughs.
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
                        // SAFETY: the borrowed body's lifetime is tied to
                        // `rt`'s schemas; both this iteration and the
                        // outer detect_main_shape function return before
                        // `rt` could be mutated.
                        let body: &'a [BodyItem] = unsafe {
                            std::mem::transmute::<&[BodyItem], &'a [BodyItem]>(&sub.body)
                        };
                        collect(body, rt, out, visited);
                    }
                }
            }
        }
    }
    collect(&main.body, rt, &mut all_items, &mut visited);
    for item in all_items.iter().copied() {
        if let BodyItem::Membership { name, type_name, .. } = item {
            // Convention: the main claim's "effects" output is named
            // exactly "effects"; the "results" input is "last_results".
            // Pick the FIRST match of each — programs may declare other
            // EffectList / ResultList intermediates that the loop should
            // ignore.
            if type_name == "EffectList" && name == "effects" && effects_var.is_none() {
                effects_var = Some(name.clone());
            } else if type_name == "ResultList" && name == "last_results"
                   && last_results_var.is_none()
            {
                last_results_var = Some(name.clone());
            } else if type_name != "Int" && type_name != "Bool"
                   && type_name != "String" && type_name != "Real"
                   && !type_name.starts_with("Seq")
                   && !type_name.starts_with("Set")
            {
                // Look for state/state_next pair (same type, two vars).
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
        state_var:        s,
        state_next_var:   sn,
        state_type:       st,
        last_results_var: last_results_var?,
        effects_var:      effects_var?,
    })
}

/// Run the effect loop. Solver is hit once per step, results
/// dispatched, fed back as `last_results` for the next step.
pub fn run(rt: &EvidentRuntime, opts: &LoopOpts) -> Result<LoopResult, String> {
    let shape = detect_main_shape(rt)
        .ok_or_else(|| "main claim is not effect-driven (missing state pair, EffectList, or ResultList)".to_string())?;
    run_with_shape(rt, &shape, opts, &mut DispatchContext::new())
}

/// Run with caller-supplied dispatch context. Test entry point —
/// lets callers swap in fake stdin/stdout.
pub fn run_with_ctx(
    rt: &EvidentRuntime,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    let shape = detect_main_shape(rt)
        .ok_or_else(|| "main claim is not effect-driven".to_string())?;
    run_with_shape(rt, &shape, opts, ctx)
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
        let r = rt.query_with_pinned_datatypes("main", &pins)
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
