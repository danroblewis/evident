//! `runtime-smt` CLI.
//!
//! Phase 0: `runtime-smt solve <file.smt2>` — the floor. Parse the SMT-LIB
//! file, solve, print sat/unsat and (when sat) the model bindings. Later
//! milestones add `run <fixture>` for the full tick loop.

use std::process::ExitCode;

use runtime_smt::{solve_smtlib, SolveOutcome};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] != "solve" {
        eprintln!("usage: runtime-smt solve <file.smt2>");
        return ExitCode::from(2);
    }
    let path = &args[2];
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
