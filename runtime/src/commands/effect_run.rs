//! `evident effect-run <file>` — load + run an effect-driven program
//! via the new effect loop. Skips the plugin-based executor entirely.
//!
//! Programs eligible for this runner declare a main claim with:
//!   state, state_next ∈ <enum>
//!   last_results      ∈ Seq(Result)
//!   effects           ∈ EffectList
//!
//! and import "stdlib/runtime.ev" for the Effect/Result/EffectList
//! types.

use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, effect_loop};

const STDLIB_RUNTIME: &str = "stdlib/runtime.ev";

pub fn cmd_effect_run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("effect-run: need a program path");
        return ExitCode::from(2);
    }
    let path = &args[0];
    let mut max_steps = 10_000usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--max-steps" => {
                i += 1;
                let v = args.get(i).and_then(|s| s.parse().ok())
                    .unwrap_or(10_000);
                max_steps = v;
            }
            other => {
                eprintln!("effect-run: unknown flag {other:?}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(Path::new(STDLIB_RUNTIME)) {
        eprintln!("effect-run: load {STDLIB_RUNTIME}: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = rt.load_file(Path::new(path)) {
        eprintln!("effect-run: load {path}: {e}");
        return ExitCode::from(1);
    }
    super::desugar::auto_apply_desugar(&mut rt, &[path.clone()]);

    match effect_loop::run(&rt, &effect_loop::LoopOpts { max_steps }) {
        Ok(r) => {
            // Print Z3 functionizer + JIT stats summary if requested.
            if std::env::var("EVIDENT_FUNCTIONIZE_STATS").is_ok() {
                rt.functionize_stats().print_summary();
            }
            // Effect::Exit(code) propagates as the process exit code.
            // Other halt paths exit 0 on clean halt, 1 on max_steps.
            if let Some(code) = r.exit_code {
                let clamped = code.clamp(0, 255) as u8;
                return ExitCode::from(clamped);
            }
            if !r.halted_clean {
                eprintln!("effect-run: did not halt cleanly after {} steps", r.steps);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("effect-run: {e}");
            ExitCode::from(1)
        }
    }
}
