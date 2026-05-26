//! Effect-driven step loop for programs whose `main` claim declares
//! `effects ∈ Seq(Effect)` and `last_results ∈ Seq(Result)`.

use crate::effect_dispatch::DispatchContext;
use crate::runtime::EvidentRuntime;
use crate::core::Value;

mod collect;
mod fsm;
mod nested;
mod scheduler;
mod state;
mod timing;
mod toposort;

// Public re-exports: keep every symbol accessible as `crate::effect_loop::X`.
pub use fsm::{MainShape, all_fsms, detect_main_shape, resolve_fsm};
pub use nested::{run_nested, run_nested_capturing, validate_run_target, RunError};

/// Tunables for the effect loop.
#[derive(Debug, Clone)]
pub struct LoopOpts {
    /// Hard ceiling on iterations; prevents infinite loops.
    pub max_steps: usize,
}

impl Default for LoopOpts {
    fn default() -> Self { Self { max_steps: 10_000 } }
}

/// `EVIDENT_*` env vars snapshot read once at startup; avoids per-tick syscalls.
#[derive(Debug, Clone)]
pub(crate) struct LoopEnv {
    /// `EVIDENT_LOOP_TRACE` — per-tick scheduling diagnostics.
    pub(crate) trace:          bool,
    /// `EVIDENT_LOOP_TIMING` — per-step solve/dispatch timing.
    pub(crate) timing:         bool,
    /// `EVIDENT_TICK_MS` — FrameTimer interval (opt-in).
    pub(crate) tick_ms:        Option<u64>,
    /// `EVIDENT_CLOCK_MS` — WallClock interval (default 100).
    pub(crate) clock_ms:       u64,
    /// `EVIDENT_FILE_WATCH` — path to watch (FileWatcher).
    pub(crate) file_watch:     Option<String>,
    /// `EVIDENT_FILE_WATCH_MS` — FileWatcher poll interval (default 200).
    pub(crate) file_watch_ms:  u64,
    /// `EVIDENT_FILE_INPUT` — path to read (FileLineReader).
    pub(crate) file_input:     Option<String>,
}

impl LoopEnv {
    fn from_process_env() -> Self {
        Self {
            trace:         std::env::var("EVIDENT_LOOP_TRACE").is_ok(),
            timing:        std::env::var("EVIDENT_LOOP_TIMING").is_ok(),
            tick_ms:       std::env::var("EVIDENT_TICK_MS").ok()
                               .and_then(|s| s.parse().ok())
                               .filter(|&n: &u64| n > 0),
            clock_ms:      std::env::var("EVIDENT_CLOCK_MS").ok()
                               .and_then(|s| s.parse().ok())
                               .filter(|&n: &u64| n > 0)
                               .unwrap_or(100),
            file_watch:    std::env::var("EVIDENT_FILE_WATCH").ok(),
            file_watch_ms: std::env::var("EVIDENT_FILE_WATCH_MS").ok()
                               .and_then(|s| s.parse().ok())
                               .filter(|&n: &u64| n > 0)
                               .unwrap_or(200),
            file_input:    std::env::var("EVIDENT_FILE_INPUT").ok(),
        }
    }
}

/// Result of running an effect-driven program.
#[derive(Debug)]
pub struct LoopResult {
    pub steps:      usize,
    pub final_state: Option<Value>,
    pub halted_clean: bool,
    /// `Some(code)` iff a FSM emitted `Effect::Exit`; set at end-of-tick.
    pub exit_code: Option<i32>,
}

/// Run the effect loop; multi-FSM programs use the subscription scheduler.
pub fn run(rt: &EvidentRuntime, opts: &LoopOpts) -> Result<LoopResult, String> {
    run_with_ctx(rt, opts, &mut DispatchContext::new())
}

/// Run with caller-supplied dispatch context (lets tests swap in fake stdin/stdout).
pub fn run_with_ctx(
    rt: &EvidentRuntime,
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String> {
    let fsms = all_fsms(rt);
    let env = LoopEnv::from_process_env();

    // Two plugin trigger paths: marker-type subscription and reserved World-field auto-install.
    let mut event_sources: Vec<Box<dyn crate::event_sources::EventSource>> = Vec::new();
    let (event_tx, event_rx) = std::sync::mpsc::channel::<crate::event_sources::SchedulerEvent>();
    let mut plugin_writes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let world_fields: std::collections::HashMap<String, String> = fsms.iter()
        .find_map(|f| f.world_type.as_ref())
        .and_then(|wt| rt.get_schema(wt))
        .map(|w| {
            w.body.iter().filter_map(|item| {
                if let crate::core::ast::BodyItem::Membership { name, type_name, .. } = item {
                    Some((name.clone(), type_name.clone()))
                } else { None }
            }).collect()
        })
        .unwrap_or_default();
    let fsm_event_subscriptions: std::collections::HashSet<String> = fsms.iter()
        .flat_map(|f| f.event_subscriptions.iter().cloned())
        .collect();
    let fsm_using_identifier = |ident: &str| -> Option<String> {
        for fsm in &fsms {
            if let Some(claim) = rt.get_schema(&fsm.claim_name) {
                if crate::subscriptions::body_references_identifier(claim, ident) {
                    return Some(fsm.claim_name.clone());
                }
            }
        }
        None
    };
    // Closure for reflection plugins: produces a Value::Enum tree of the loaded
    // program (catches missing stdlib/ast.ev before the FSM tries to pin it).
    let encode_program = || -> Result<crate::Value, String> {
        let by_name = rt.enums_registry().by_name.borrow();
        if !by_name.contains_key("Program") {
            return Err(
                "reflection plugin: world declares a `Program` field, \
                 but `stdlib/ast.ev` isn't imported — add \
                 `import \"stdlib/ast.ev\"` to make the AST schema \
                 available".to_string());
        }
        drop(by_name);
        Ok(crate::translate::ast_encoder::program_to_value(
            &rt.program_ast(),
        ))
    };
    let plugin_ctx = crate::event_sources::WorldPluginCtx {
        world_fields:            &world_fields,
        fsm_event_subscriptions: &fsm_event_subscriptions,
        env_tick_ms:             env.tick_ms,
        env_clock_ms:            env.clock_ms,
        env_file_watch:          env.file_watch.as_deref(),
        env_file_watch_ms:       env.file_watch_ms,
        env_file_input:          env.file_input.as_deref(),
        fsm_using_identifier:    &fsm_using_identifier,
        encode_program:          &encode_program,
    };

    // Each registry entry decides for itself whether to install; scheduler is bridge-agnostic.
    for installer in crate::event_sources::WORLD_PLUGIN_INSTALLERS {
        if let Some(install) = installer(&plugin_ctx, &event_tx)? {
            for k in install.plugin_writes { plugin_writes.insert(k); }
            if install.owns_stdin { ctx.stdin_owned_by_plugin = true; }
            event_sources.push(install.source);
        }
    }

    // FTI: typed-resource bridges declared as FSM params (e.g. `t ∈ Timer (interval_ms ↦ 50)`).
    // Keys are `<fsm>.<param>.<field>` so two FSMs sharing a param type get distinct bridges.
    for fsm in &fsms {
        for (param_name, type_name, pins) in &fsm.fti_params {
            let fti_ctx = crate::fti::FtiContext {
                claim_name:  fsm.claim_name.clone(),
                param_name:  param_name.clone(),
                env_tick_ms: env.tick_ms,
            };
            // Declarative install: if type body has `install ∈ Seq(InstallStep)`, use
            // generic mechanism. Falls back to INSTALLERS only for thread-driven sources.
            let has_declarative = rt.get_schema(type_name).map(|s|
                s.body.iter().any(|i| matches!(i,
                    crate::core::ast::BodyItem::Membership { name, type_name: ty, .. }
                    if name == "install" && ty == "Seq(InstallStep)"))
            ).unwrap_or(false);
            if has_declarative {
                let mut src = crate::event_sources::DeclarativeInstallSource::new();
                // CRITICAL: pass the scheduler's DispatchContext — HandleRegistry IDs must
                // be in the same registry per-tick dispatch uses, or ArgHandle lookups fail.
                src.run_install(rt, type_name, &fti_ctx, pins, &event_tx, ctx)?;
                if let Some(type_decl) = rt.get_schema(type_name) {
                    // Register non-install fields as plugin-owned writes.
                    for item in &type_decl.body {
                        if let crate::core::ast::BodyItem::Membership { name, type_name: _, .. } = item {
                            if name == "install" { continue; }
                            let key = format!("{}.{}.{}", fsm.claim_name, param_name, name);
                            plugin_writes.insert(key);
                        }
                    }
                }
                event_sources.push(Box::new(src));
                continue;
            }
            let Some(install_fn) = crate::fti::fti_install_fn(type_name)
                else { continue };
            let install = install_fn(&fti_ctx, pins, event_tx.clone())?;
            event_sources.push(install.source);
            for k in install.keys { plugin_writes.insert(k); }
        }
    }
    // Drop our sender; when all source senders drop, receiver returns Err → all sources dead.
    drop(event_tx);
    let event_rx = if event_sources.is_empty() { None } else { Some(event_rx) };

    if env.trace {
        eprintln!("[loop] startup: fsms=[{}] plugin_writes=[{}]",
            fsms.iter().map(|f| f.claim_name.as_str()).collect::<Vec<_>>().join(","),
            plugin_writes.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","),
        );
    }

    // Transitive access sets computed once; shared with the scheduler (avoids a second costly
    // self-hosted walk). FSMs with no world membership get empty sets — skips the walk.
    let initial_access: Vec<crate::subscriptions::AccessSets> = fsms.iter()
        .map(|f| if f.world_type.is_none() {
            crate::subscriptions::AccessSets::default()
        } else {
            fsm::full_world_access(rt, &f.claim_name)
        })
        .collect();

    // Multi-writer disjoint check: every writer FSM + plugin must own disjoint fields.
    // Write-sets are transitive so passthrough-delegated writes are caught.
    {
        let mut writer_sets: Vec<(String, std::collections::HashSet<String>)> = fsms.iter()
            .zip(&initial_access)
            .filter(|(f, _)| f.is_writer())
            .map(|(f, aset)| (f.claim_name.clone(), aset.writes.clone()))
            .collect();
        for pf in &plugin_writes {
            let mut s = std::collections::HashSet::new();
            s.insert(pf.clone());
            writer_sets.push((format!("<plugin>:{pf}"), s));
        }
        for i in 0..writer_sets.len() {
            for j in (i + 1)..writer_sets.len() {
                let (a_name, a_writes) = &writer_sets[i];
                let (b_name, b_writes) = &writer_sets[j];
                let overlap: Vec<&String> = a_writes.intersection(b_writes).collect();
                if !overlap.is_empty() {
                    for source in &mut event_sources { source.stop(); }
                    return Err(format!(
                        "multi-FSM: writers `{a_name}` and `{b_name}` both write \
                         to world fields {overlap:?}. Each world field must have \
                         at most one writer (single-owner rule). Fix by either: \
                         (1) merging the two FSMs into one writer for that field, \
                         (2) splitting the field so each writer owns a distinct \
                         one, or (3) making one FSM a reader (drop its \
                         `world_next` membership and read the field via `world.X`)."
                    ));
                }
            }
        }
    }

    let result = if fsms.is_empty() {
        Err("no fsm schemas found (declare one with the `fsm` keyword)".to_string())
    } else {
        scheduler::run_scheduler(rt, &fsms, &initial_access, opts, ctx, event_rx.as_ref(), &mut event_sources, &env)
    };
    // Stop all event sources (explicit stop avoids leaking threads on Err paths).
    for source in &mut event_sources {
        source.stop();
    }
    result
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
        rt.load_file(&crate::stdlib_path::stdlib_dir().unwrap().join("runtime.ev")).unwrap();
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
        rt.load_file(&crate::stdlib_path::stdlib_dir().unwrap().join("runtime.ev")).unwrap();
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
        rt.load_file(&crate::stdlib_path::stdlib_dir().unwrap().join("runtime.ev")).unwrap();
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
