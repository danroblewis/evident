//! `evident effect-run <file>` — load + run an effect-driven multi-FSM program.

use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, effect_loop, stdlib_path};

pub fn cmd_effect_run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Usage: evident effect-run <file> [--max-steps N]");
        return ExitCode::from(2);
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
            "-h" | "--help" => {
                eprintln!("Usage: evident effect-run <file> [--max-steps N]");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("effect-run: unknown flag {other:?}");
                return ExitCode::from(2);
            }
            other => {
                if path.is_some() {
                    eprintln!("effect-run: multiple program paths given");
                    return ExitCode::from(2);
                }
                path = Some(other.to_string());
            }
        }
        i += 1;
    }
    let Some(path) = path else {
        eprintln!("effect-run: need a program path");
        return ExitCode::from(2);
    };

    let mut rt = EvidentRuntime::new();
    let stdlib = match stdlib_path::stdlib_dir() {
        Ok(d) => d,
        Err(e) => { eprintln!("effect-run: {e}"); return ExitCode::from(1); }
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
    super::common::auto_apply_desugar(&mut rt, &[path.clone()]);

    match effect_loop::run(&rt, &effect_loop::LoopOpts { max_steps }) {
        Ok(r) => {
            if let Some(code) = r.exit_code {
                return ExitCode::from(code.clamp(0, 255) as u8);
            }
            if !r.halted_clean {
                eprintln!("effect-run: did not halt cleanly after {} steps", r.steps);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("effect-run: {e}"); ExitCode::from(1) }
    }
}
