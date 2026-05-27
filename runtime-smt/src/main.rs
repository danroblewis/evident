//! `runtime-smt` CLI.
//!
//!   runtime-smt solve <file.smt2>           — N0 floor: parse + solve + print model.
//!   runtime-smt run   <fixture.smt2> [--cache]
//!                                           — N2/N3 loop: run the FSM(s) to halt,
//!                                             dispatching effects (Println→stdout),
//!                                             exit with the FSM's exit code.
//!                                             --cache memoizes tick solves (N4a)
//!                                             and prints hit/miss stats to stderr.
//!   runtime-smt transpile <claim.ev>        — N4b front-end: transpile a scalar
//!                                             Evident claim to SMT-LIB and solve it.
//!   runtime-smt fsm <file.ev> [--dump]       — convergence front-end: transpile an
//!                                             Evident FSM to a fixture, then run it
//!                                             (effects→stdout, exit with its code).
//!                                             --dump prints the fixture instead.

use std::path::Path;
use std::process::ExitCode;

use runtime_smt::driver::DEFAULT_MAX_TICKS;
use runtime_smt::frontend::transpile_claim;
use runtime_smt::fsm_frontend::transpile_fsm;
use runtime_smt::meta::load_str;
use runtime_smt::meta::load_file;
use runtime_smt::scheduler::{run, run_cached};
use runtime_smt::{solve_smtlib, SolveOutcome, TickCache};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("solve") if args.len() >= 3 => cmd_solve(&args[2]),
        Some("run") if args.len() >= 3 => cmd_run(&args[2], args.iter().any(|a| a == "--cache")),
        Some("transpile") if args.len() >= 3 => cmd_transpile(&args[2]),
        Some("fsm") if args.len() >= 3 => cmd_fsm(&args[2], args.iter().any(|a| a == "--dump")),
        _ => {
            eprintln!(
                "usage:\n  runtime-smt solve <file.smt2>\n  runtime-smt run <fixture.smt2> [--cache]\n  runtime-smt transpile <claim.ev>\n  runtime-smt fsm <file.ev> [--dump]"
            );
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
    print_solve(solve_smtlib(&text))
}

fn cmd_transpile(path: &str) -> ExitCode {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let smtlib = match transpile_claim(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    print_solve(solve_smtlib(&smtlib))
}

fn print_solve(r: Result<SolveOutcome, runtime_smt::Z3Error>) -> ExitCode {
    match r {
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

fn cmd_fsm(path: &str, dump: bool) -> ExitCode {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let fixture = match transpile_fsm(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if dump {
        print!("{fixture}");
        return ExitCode::SUCCESS;
    }
    let problem = match load_str(&fixture) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("transpiled fixture failed to load: {e}");
            return ExitCode::FAILURE;
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

fn cmd_run(path: &str, cached: bool) -> ExitCode {
    let problem = match load_file(Path::new(path)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot load {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let mut stdout = std::io::stdout().lock();
    let result = if cached {
        let mut cache = TickCache::new();
        let r = run_cached(&problem, &mut stdout, DEFAULT_MAX_TICKS, &mut cache);
        if let Ok(rep) = &r {
            eprintln!(
                "[cache] {} ticks, {} solves ({} hits)",
                rep.ticks,
                cache.misses(),
                cache.hits()
            );
        }
        r
    } else {
        run(&problem, &mut stdout, DEFAULT_MAX_TICKS)
    };
    match result {
        Ok(report) => ExitCode::from(report.exit_code.clamp(0, 255) as u8),
        Err(e) => {
            eprintln!("run failed: {e}");
            ExitCode::FAILURE
        }
    }
}
