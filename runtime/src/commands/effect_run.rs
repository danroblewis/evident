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

use evident_runtime::{EvidentRuntime, effect_loop, stdlib_path};

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
    eprintln!("  --functionizer NAME      choose the functionize strategy:");
    eprintln!("                             cranelift (default) — translate the Z3 AST to native code");
    eprintln!("                             symbolic            — genetic-programming search for a");
    eprintln!("                                                   closed-form closure matching the IO");
    eprintln!("                             llm                 — LLM code-gen (needs ANTHROPIC_API_KEY;");
    eprintln!("                                                   silently falls back without one)");
    eprintln!("                             satisfier           — sample partially-constrained vars");
    eprintln!("                                                   (sets EVIDENT_SATISFIER=1)");
    eprintln!("                           (also via EVIDENT_FUNCTIONIZER=NAME, or a");
    eprintln!("                            `-- functionizer: NAME` marker line in the program)");
    eprintln!();
    eprintln!("Diagnostic IR dumps (env vars):");
    eprintln!("  EVIDENT_FZ_DUMP_BODY=1     raw simplified Z3 assertions (extractor input)");
    eprintln!("  EVIDENT_FZ_DUMP_PROGRAM=1  per-claim Z3Program IR, just before JIT codegen");
    eprintln!("  EVIDENT_JIT_DUMP=1         Cranelift CLIF (codegen output)");
    eprintln!();
    eprintln!("Z3 tuning:");
    eprintln!("  --lenient                demote dropped-constraint errors to warnings");
    eprintln!("                           (sets EVIDENT_LENIENT=1)");
    eprintln!("  --arith-solver N         Z3 smt.arith.solver setting (0..6; default 2)");
    eprintln!();
    eprintln!("Misc:");
    eprintln!("  -h, --help               this message");
}

/// Resolve the functionize strategy name from (in priority order) the
/// `--functionizer` flag, `EVIDENT_FUNCTIONIZER`, or a
/// `-- functionizer: NAME` marker line in the program source. Returns
/// `None` when nothing selects a strategy (caller uses the default).
fn resolve_functionizer(flag: &Option<String>, path: &str) -> Option<String> {
    if let Some(f) = flag {
        return Some(f.trim().to_lowercase());
    }
    if let Ok(env) = std::env::var("EVIDENT_FUNCTIONIZER") {
        if !env.trim().is_empty() {
            return Some(env.trim().to_lowercase());
        }
    }
    // Scan the source for a marker comment: a `--` comment line whose
    // payload is `functionizer: NAME`. Kept deliberately simple — first
    // match wins. Unreadable file → no marker (load_file reports the
    // real error later).
    let src = std::fs::read_to_string(path).ok()?;
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("--") {
            if let Some(name) = rest.trim().strip_prefix("functionizer:") {
                let name = name.trim().to_lowercase();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
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
    let mut functionizer_flag: Option<String> = None;
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
            "--functionizer" => {
                i += 1;
                match args.get(i) {
                    Some(v) => functionizer_flag = Some(v.clone()),
                    None => {
                        eprintln!("effect-run: --functionizer needs a NAME (cranelift | symbolic | llm | satisfier)");
                        return ExitCode::from(2);
                    }
                }
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

    let Some(path) = path else {
        eprintln!("effect-run: need a program path");
        eprintln!("Run `evident effect-run --help` for the flag list.");
        return ExitCode::from(2);
    };

    // Resolve which functionize strategy to mount. Precedence:
    //   1. --functionizer NAME flag
    //   2. EVIDENT_FUNCTIONIZER env var
    //   3. a `-- functionizer: NAME` marker line in the program source
    //   4. default (cranelift)
    // The marker lets an opt-in-functionizer demo (test_31) select its
    // strategy with no CLI flag — needed because the demo test harness
    // runs `effect-run <file>` with a fixed command line. The flag/env
    // override the marker so the same file can be A/B'd against
    // cranelift (`--functionizer cranelift`).
    let functionizer_name = resolve_functionizer(&functionizer_flag, &path);
    let mut rt = match functionizer_name.as_deref() {
        Some("symbolic") => {
            use evident_runtime::functionize::symbolic::SymbolicFunctionizer;
            // The symbolic strategy announces each closed form it
            // rediscovers to stdout (proof it ran, vs the cranelift
            // fallback) — opt-in so library/unit-test uses stay quiet.
            if std::env::var("EVIDENT_SYMBOLIC_ANNOUNCE").is_err() {
                std::env::set_var("EVIDENT_SYMBOLIC_ANNOUNCE", "1");
            }
            EvidentRuntime::with_functionizer(Box::new(SymbolicFunctionizer::new()))
        }
        Some("llm") => {
            // LLM code-gen needs ANTHROPIC_API_KEY. Without one, the
            // generator would decline on every component (no network
            // call), so we print a notice and use the default
            // Cranelift strategy — JIT for what it can compile, Z3
            // slow path for what it can't.
            let have_key = std::env::var("ANTHROPIC_API_KEY").ok()
                .filter(|k| !k.is_empty()).is_some();
            if have_key {
                eprintln!("[fz] llm functionizer active (ANTHROPIC_API_KEY found); \
                           components Cranelift can't compile are sent to the LLM, \
                           validated against Z3, and cached.");
                EvidentRuntime::with_functionizer(Box::new(
                    evident_runtime::functionize::llm::LlmFunctionizer::new()))
            } else {
                eprintln!("[fz] llm functionizer requires ANTHROPIC_API_KEY, skipping — \
                           falling back to the Cranelift JIT + Z3 slow path.");
                EvidentRuntime::new()
            }
        }
        Some("satisfier") => {
            // SatisfierFunctionizer samples values for vars that are
            // bounded but not fully defined. Sampler emission in the
            // extractor is gated by EVIDENT_SATISFIER — set it here
            // so the two stay in lockstep.
            std::env::set_var("EVIDENT_SATISFIER", "1");
            EvidentRuntime::with_functionizer(Box::new(
                evident_runtime::functionize::satisfier::SatisfierFunctionizer::new()))
        }
        Some("cranelift") | None => EvidentRuntime::new(),
        Some(other) => {
            eprintln!("effect-run: unknown functionizer {other:?} \
                       (expected: cranelift | symbolic | llm | satisfier)");
            return ExitCode::from(2);
        }
    };
    let stdlib = match stdlib_path::stdlib_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("effect-run: {e}");
            return ExitCode::from(1);
        }
    };
    let runtime_ev = stdlib.join("runtime.ev");
    if let Err(e) = rt.load_file(&runtime_ev) {
        eprintln!("effect-run: load {}: {e}", runtime_ev.display());
        return ExitCode::from(1);
    }
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
