//! `evident effect-run-smtlib <fixture.json>` — run an SMT-LIB-driven multi-FSM
//! program through the EXISTING scheduler (strategy 2 of runtime-evolve).
//!
//! The fixture is one JSON program (optional shared `world` + one-or-more FSMs,
//! each with metadata + SMT-LIB constraint text). The runtime registers the
//! synthetic `fsm` shapes + the SMT-LIB registry, then runs the *same*
//! `effect_loop::run` as `effect-run`. The Evident-source path is untouched.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use evident_runtime::{effect_loop, smtlib_fsm, stdlib_path, EvidentRuntime};

fn print_help() {
    eprintln!("Usage: evident effect-run-smtlib <fixture.json> [flags]");
    eprintln!();
    eprintln!("Run an SMT-LIB-driven multi-FSM program through the existing engine.");
    eprintln!("The fixture is a JSON program: an optional `world` record plus an");
    eprintln!("`fsms` array, each entry carrying `meta` + `smtlib` (inline) or");
    eprintln!("`smtlib_file` (resolved relative to the fixture).");
    eprintln!();
    eprintln!("  --max-steps N   cap the scheduler at N ticks (default: 10000)");
    eprintln!("  --timing        per-tick solve+dispatch timing (EVIDENT_LOOP_TIMING=1)");
    eprintln!("  -h, --help      this message");
}

pub fn cmd_effect_run_smtlib(args: &[String]) -> ExitCode {
    if args.is_empty() || args.iter().any(|a| matches!(a.as_str(), "-h" | "--help")) {
        print_help();
        return if args.is_empty() { ExitCode::from(2) } else { ExitCode::SUCCESS };
    }
    let mut path: Option<String> = None;
    let mut max_steps = 10_000usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--max-steps" => {
                i += 1;
                max_steps = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(10_000);
            }
            "--timing" => std::env::set_var("EVIDENT_LOOP_TIMING", "1"),
            other if other.starts_with('-') => {
                eprintln!("effect-run-smtlib: unknown flag {other:?}");
                return ExitCode::from(2);
            }
            other => {
                if path.is_some() {
                    eprintln!("effect-run-smtlib: multiple fixtures given");
                    return ExitCode::from(2);
                }
                path = Some(other.to_string());
            }
        }
        i += 1;
    }
    let Some(path) = path else {
        eprintln!("effect-run-smtlib: need a fixture path");
        return ExitCode::from(2);
    };

    let mut rt = EvidentRuntime::new();
    // Load runtime.ev for the builtin Effect/Result enums (consistency with
    // effect-run; the SMT-LIB effect assembly is registry-free but enum state /
    // last_results support relies on these being registered).
    match stdlib_path::stdlib_dir() {
        Ok(stdlib) => {
            let runtime_ev = stdlib.join("runtime.ev");
            if let Err(e) = rt.load_file(&runtime_ev) {
                eprintln!("effect-run-smtlib: load {}: {e}", runtime_ev.display());
                return ExitCode::from(1);
            }
        }
        Err(e) => {
            eprintln!("effect-run-smtlib: {e}");
            return ExitCode::from(1);
        }
    }

    // Parse the fixture; resolve `smtlib_file` references relative to the dir.
    let json = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("effect-run-smtlib: read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let base_dir: PathBuf = Path::new(&path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let resolve = |file: &str| -> Result<String, String> {
        std::fs::read_to_string(base_dir.join(file)).map_err(|e| e.to_string())
    };
    let program = match smtlib_fsm::parse_fixture(&json, &resolve) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("effect-run-smtlib: {path}: {e}");
            return ExitCode::from(1);
        }
    };
    if program.fsms.is_empty() {
        eprintln!("effect-run-smtlib: {path}: no FSMs in fixture");
        return ExitCode::from(1);
    }
    rt.register_smtlib_program(program);

    match effect_loop::run(&rt, &effect_loop::LoopOpts { max_steps }) {
        Ok(r) => {
            if let Some(code) = r.exit_code {
                return ExitCode::from(code.clamp(0, 255) as u8);
            }
            if !r.halted_clean {
                eprintln!("effect-run-smtlib: did not halt cleanly after {} steps", r.steps);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("effect-run-smtlib: {e}");
            ExitCode::from(1)
        }
    }
}
