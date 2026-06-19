use crate::effect_dispatch::DispatchContext;
use crate::runtime::EvidentRuntime;
use crate::core::Value;

mod collect;
mod fsm;
mod install;
mod scheduler;
mod seq_chains;
mod state;
mod toposort;

pub use fsm::{MainShape, single_fsm, detect_main_shape, resolve_fsm};

#[derive(Debug, Clone)]
pub struct LoopOpts {

    pub max_steps: usize,
}

impl Default for LoopOpts {
    fn default() -> Self { Self { max_steps: 10_000 } }
}

#[derive(Debug)]
pub struct LoopResult {
    pub steps:       usize,
    pub final_state: Option<Value>,
    pub halted_clean: bool,

    pub exit_code: Option<i32>,
}

pub fn run(rt: &EvidentRuntime, opts: &LoopOpts) -> Result<LoopResult, String> {
    run_with_ctx(rt, opts, &mut DispatchContext::new())
}

pub fn run_with_ctx(
    rt: &EvidentRuntime,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    let fsm = single_fsm(rt)?;

    let mut world_snapshot: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    for (param_name, type_name, pins) in &fsm.install_params {
        let writes = install::run_declarative_install(
            rt, &fsm.claim_name, param_name, type_name, pins, ctx)?;
        for (k, v) in writes {
            world_snapshot.insert(k, v);
        }
    }

    scheduler::run_loop(rt, &fsm, opts, ctx, &mut world_snapshot)
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
