//! Effect-driven tick loop — the executor for `evident effect-run`.
//!
//! Per tick, for each `fsm`-keyword'd claim (declaration order):
//!   1. Encode current `state` + `_var` previous values + last_results
//!      + the shared world snapshot as Z3 givens/pins.
//!   2. Solve the FSM with `rt.query_with_pins_and_given`.
//!   3. Decode `state_next` and the ordered `effects`.
//!   4. Dispatch each effect via `effect_dispatch`; feed results back
//!      as `last_results`.
//!   5. state ← state_next; merge `world.X` writes into the snapshot.
//!   6. Halt on `Effect::Exit`, at `max_steps`, or at a fixpoint.
//!
//! All input/output is via the FFI effects the program emits
//! (LibCall, Println, ParseInt, …). There are no async event sources,
//! no world-field plugins, and no subscription scheduler — those were
//! removed in the single-FSM teardown. The one piece of bridge
//! machinery that remains is the **declarative FTI install** for typed
//! resources (`win ∈ SDL_Window (…)`), run once at startup; see
//! `install.rs`.

use crate::effect_dispatch::DispatchContext;
use crate::runtime::EvidentRuntime;
use crate::core::Value;

mod collect;
mod fsm;
mod install;
mod scheduler;
mod seq_chains;
mod state;
mod timing;
mod toposort;

// ── Public re-exports ────────────────────────────────────────
pub use fsm::{MainShape, all_fsms, detect_main_shape, resolve_fsm};

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

/// Snapshot of the `EVIDENT_*` diagnostic env vars the loop consults.
/// Read ONCE at startup; per-tick code references the cached fields.
#[derive(Debug, Clone)]
pub(crate) struct LoopEnv {
    /// `EVIDENT_LOOP_TRACE` — gate per-tick scheduling diagnostics.
    pub(crate) trace:  bool,
    /// `EVIDENT_LOOP_TIMING` — gate per-step solve/dispatch timing.
    pub(crate) timing: bool,
}

impl LoopEnv {
    fn from_process_env() -> Self {
        Self {
            trace:  std::env::var("EVIDENT_LOOP_TRACE").is_ok(),
            timing: std::env::var("EVIDENT_LOOP_TIMING").is_ok(),
        }
    }
}

/// Result of running an effect-driven program.
#[derive(Debug)]
pub struct LoopResult {
    pub steps:       usize,
    pub final_state: Option<Value>,
    pub halted_clean: bool,
    /// `Some(code)` iff an FSM emitted `Effect::Exit(code)` during the
    /// run. Recorded at end-of-tick so co-scheduled effects complete
    /// before halting.
    pub exit_code: Option<i32>,
}

/// Run the effect loop with a default dispatch context.
pub fn run(rt: &EvidentRuntime, opts: &LoopOpts) -> Result<LoopResult, String> {
    run_with_ctx(rt, opts, &mut DispatchContext::new())
}

/// Run with a caller-supplied dispatch context. Test entry point —
/// lets callers swap in fake stdin/stdout.
pub fn run_with_ctx(
    rt: &EvidentRuntime,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    let fsms = all_fsms(rt);
    if fsms.is_empty() {
        return Err("no fsm schemas found (declare one with the `fsm` keyword)".to_string());
    }
    let env = LoopEnv::from_process_env();

    // ── Declarative FTI install ───────────────────────────────────
    // Each typed-resource parameter with an `install ∈ Seq(InstallStep)`
    // body member (e.g. `win ∈ SDL_Window (…)`) runs its install Seq
    // once, here, against the SAME DispatchContext the per-tick loop
    // uses — so libffi handles (window ptr, renderer ptr) created at
    // install are visible to per-tick `ArgHandle` lookups. The bound
    // fields land in the world snapshot under `<fsm>.<param>.<field>`.
    let mut world_snapshot: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    for fsm in &fsms {
        for (param_name, type_name, pins) in &fsm.install_params {
            let writes = install::run_declarative_install(
                rt, &fsm.claim_name, param_name, type_name, pins, ctx)?;
            for (k, v) in writes {
                world_snapshot.insert(k, v);
            }
        }
    }

    if env.trace {
        eprintln!("[loop] startup: fsms=[{}]",
            fsms.iter().map(|f| f.claim_name.as_str()).collect::<Vec<_>>().join(","));
    }

    scheduler::run_loop(rt, &fsms, opts, ctx, &mut world_snapshot, &env)
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

fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨⟩ ∧ #last_results = 0)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩ ∧ #last_results = 0)
").unwrap();
        let shape = detect_main_shape(&rt).expect("should detect");
        assert_eq!(shape.state_var.as_deref(), Some("state"));
        assert_eq!(shape.state_next_var.as_deref(), Some("state_next"));
        assert_eq!(shape.state_type.as_deref(), Some("S"));
        assert_eq!(shape.effects_var.as_deref(), Some("effects"));
        assert_eq!(shape.last_results_var.as_deref(), Some("last_results"));
    }

    #[test]
    fn smart_inject_skips_unreferenced_slots() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
enum S = Init | Done

fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨⟩)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩)
").unwrap();
        let shape = detect_main_shape(&rt).expect("should detect");
        assert_eq!(shape.state_var.as_deref(), Some("state"));
        assert_eq!(shape.state_next_var.as_deref(), Some("state_next"));
        assert_eq!(shape.effects_var.as_deref(), Some("effects"));
        assert_eq!(shape.last_results_var, None,
            "last_results never referenced → should not be auto-injected");
    }

    #[test]
    fn halt_after_one_step_when_state_reaches_done() {
        let mut rt = EvidentRuntime::new();
        rt.load_file(std::path::Path::new("../stdlib/runtime.ev")).unwrap();
        rt.load_source("\
enum S = Init | Done

fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨⟩)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩)
").unwrap();
        let mut ctx = ctx_silent();
        let r = run_with_ctx(&rt, &LoopOpts { max_steps: 5 }, &mut ctx).unwrap();
        assert!(r.steps <= 5);
        assert!(r.halted_clean || r.steps == 5);
    }
}
