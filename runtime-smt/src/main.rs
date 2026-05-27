//! `runtime-smt` CLI.
//!
//!   runtime-smt solve <file.smt2>     — the N0 floor: parse + solve + print model.
//!   runtime-smt run   <fixture.smt2>  — the N2 loop: run the FSM(s) to halt,
//!                                       dispatching effects (Println → stdout),
//!                                       and exit with the FSM's exit code.

use std::path::Path;
use std::process::ExitCode;

use runtime_smt::driver::DEFAULT_MAX_TICKS;
use runtime_smt::meta::load_file;
use runtime_smt::scheduler::run;
use runtime_smt::{solve_smtlib, SolveOutcome};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("solve") if args.len() >= 3 => cmd_solve(&args[2]),
        Some("run") if args.len() >= 3 => cmd_run(&args[2]),
        _ => {
            eprintln!("usage:\n  runtime-smt solve <file.smt2>\n  runtime-smt run <fixture.smt2>");
            ExitCode::from(2)
        }
    }
}

fn cmd_solve(path: &str) -> ExitCode {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    match solve_smtlib(&text) {
        Ok(SolveOutcome::Sat(m)) => {
            println!("sat");
            for (name, val) in &m.bindings {
                println!("{name} = {val}");
            }
            ExitCode::SUCCESS
        }
        Ok(SolveOutcome::Unsat) => {
            println!("unsat");
            ExitCode::SUCCESS
        }
        Ok(SolveOutcome::Unknown) => {
            println!("unknown");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_run(path: &str) -> ExitCode {
    let problem = match load_file(Path::new(path)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot load {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let mut stdout = std::io::stdout().lock();
    match run(&problem, &mut stdout, DEFAULT_MAX_TICKS) {
        Ok(report) => ExitCode::from(report.exit_code.clamp(0, 255) as u8),
        Err(e) => {
            eprintln!("run failed: {e}");
            ExitCode::FAILURE
        }
    }
}
