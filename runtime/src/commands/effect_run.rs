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

/// Print the supported flags. Kept close to the parser so the
/// help text and the parser don't drift apart.
fn print_help() {
    eprintln!("Usage: evident effect-run <file> [flags]");
    eprintln!();
    eprintln!("Execution:");
    eprintln!("  --max-steps N            cap the scheduler at N ticks (default: 10000)");
    eprintln!();
    eprintln!("Timing / tracing:");
    eprintln!("  --timing                 per-tick solve+dispatch timing + final summary");
    eprintln!("                           (alias for EVIDENT_LOOP_TIMING=1)");
    eprintln!("  --dispatch-timing        per-effect dispatch timing");
    eprintln!("                           (alias for EVIDENT_DISPATCH_TIMING=1)");
    eprintln!("  --trace                  high-volume scheduler+functionizer trace");
    eprintln!("                           (sets EVIDENT_FUNCTIONIZE_TRACE=1)");
    eprintln!();
    eprintln!("Profiling:");
    eprintln!("  --profile-functionizer   per-claim functionizer + JIT stats summary");
    eprintln!("                           (sets EVIDENT_FUNCTIONIZE_STATS=1)");
    eprintln!("  --profile-z3             aggregate Z3 statistics across solves");
    eprintln!("                           (conflicts, decisions, propagations, restarts)");
    eprintln!("  --profile-z3-trace FILE  write Z3 axiom-profiler trace to FILE");
    eprintln!("                           (post-process with z3 axiom_profiler)");
    eprintln!("  --profile-z3-unsat-cores extract UNSAT cores when claims fail —");
    eprintln!("                           shows which assertions caused the conflict");
    eprintln!("  --profile-all            shorthand for --timing --profile-functionizer");
    eprintln!("                                       --profile-z3");
    eprintln!();
    eprintln!("Functionizer / Cranelift JIT:");
    eprintln!("  --no-functionizer        disable functionize entirely (EVIDENT_FUNCTIONIZE=0)");
    eprintln!();
    eprintln!("Z3 tuning:");
    eprintln!("  --lenient                demote dropped-constraint errors to warnings");
    eprintln!("                           (sets EVIDENT_LENIENT=1)");
    eprintln!("  --arith-solver N         Z3 smt.arith.solver setting (0..6; default 2)");
    eprintln!();
    eprintln!("Misc:");
    eprintln!("  -h, --help               this message");
}

pub fn cmd_effect_run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_help();
        return ExitCode::from(2);
    }
    if args.iter().any(|a| matches!(a.as_str(), "-h" | "--help")) {
        print_help();
        return ExitCode::SUCCESS;
    }
    let mut path: Option<String> = None;
    let mut max_steps = 10_000usize;
    let mut profile_z3 = false;
    let mut profile_z3_trace_file: Option<String> = None;
    let mut profile_z3_unsat_cores = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--max-steps" => {
                i += 1;
                let v = args.get(i).and_then(|s| s.parse().ok())
                    .unwrap_or(10_000);
                max_steps = v;
            }
            "--timing" => {
                std::env::set_var("EVIDENT_LOOP_TIMING", "1");
            }
            "--dispatch-timing" => {
                std::env::set_var("EVIDENT_DISPATCH_TIMING", "1");
            }
            "--trace" => {
                std::env::set_var("EVIDENT_FUNCTIONIZE_TRACE", "1");
            }
            "--profile-functionizer" => {
                std::env::set_var("EVIDENT_FUNCTIONIZE_STATS", "1");
            }
            "--profile-z3" => {
                profile_z3 = true;
                std::env::set_var("EVIDENT_PROFILE_Z3", "1");
            }
            "--profile-z3-trace" => {
                i += 1;
                let file = args.get(i).cloned()
                    .unwrap_or_else(|| "z3_profile.log".to_string());
                profile_z3_trace_file = Some(file);
            }
            "--profile-z3-unsat-cores" => {
                profile_z3_unsat_cores = true;
                std::env::set_var("EVIDENT_PROFILE_Z3_UNSAT_CORES", "1");
            }
            "--profile-all" => {
                std::env::set_var("EVIDENT_LOOP_TIMING", "1");
                std::env::set_var("EVIDENT_FUNCTIONIZE_STATS", "1");
                std::env::set_var("EVIDENT_PROFILE_Z3", "1");
                profile_z3 = true;
            }
            "--no-functionizer" => {
                std::env::set_var("EVIDENT_FUNCTIONIZE", "0");
            }
            "--lenient" => {
                std::env::set_var("EVIDENT_LENIENT", "1");
            }
            "--arith-solver" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    std::env::set_var("EVIDENT_Z3_ARITH_SOLVER", v);
                }
            }
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with("--") || other.starts_with('-') => {
                eprintln!("effect-run: unknown flag {other:?}");
                eprintln!("Run `evident effect-run --help` for the flag list.");
                return ExitCode::from(2);
            }
            other => {
                // Non-flag arg = the program path. First one wins;
                // subsequent positionals are an error.
                if path.is_some() {
                    eprintln!("effect-run: multiple program paths given: {:?}, {:?}",
                              path.unwrap(), other);
                    return ExitCode::from(2);
                }
                path = Some(other.to_string());
            }
        }
        i += 1;
    }

    // Configure Z3 profiling BEFORE any Z3 context is created.
    // Global params are read at context construction.
    if let Some(file) = &profile_z3_trace_file {
        // Set global Z3 params for axiom-profiler-compatible
        // trace logging. Must run before any Solver is created.
        evident_runtime::z3_profile::enable_trace(file);
    }
    if profile_z3_unsat_cores {
        // Solver-level `unsat_core` parameter — applied per-solver
        // by the runtime's `make_tuned_solver` (see translate/eval.rs).
        // The env var is the signal there.
        std::env::set_var("EVIDENT_PROFILE_Z3_UNSAT_CORES", "1");
    }

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(Path::new(STDLIB_RUNTIME)) {
        eprintln!("effect-run: load {STDLIB_RUNTIME}: {e}");
        return ExitCode::from(1);
    }
    let Some(path) = path else {
        eprintln!("effect-run: need a program path");
        eprintln!("Run `evident effect-run --help` for the flag list.");
        return ExitCode::from(2);
    };
    if let Err(e) = rt.load_file(Path::new(&path)) {
        eprintln!("effect-run: load {path}: {e}");
        return ExitCode::from(1);
    }
    super::desugar::auto_apply_desugar(&mut rt, &[path.clone()]);

    match effect_loop::run(&rt, &effect_loop::LoopOpts { max_steps }) {
        Ok(r) => {
            // Print profiling summaries if requested.
            if std::env::var("EVIDENT_FUNCTIONIZE_STATS").is_ok() {
                rt.functionize_stats().print_summary();
            }
            if profile_z3 {
                evident_runtime::z3_profile::print_summary();
            }
            if let Some(file) = &profile_z3_trace_file {
                eprintln!("[profile-z3] trace written to {file}");
                eprintln!("[profile-z3] post-process via Z3's axiom_profiler tool:");
                eprintln!("[profile-z3]   python3 -m z3.axiom_profiler {file}");
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
