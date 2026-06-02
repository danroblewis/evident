//! `evident` CLI — only subcommand: effect-run.

use std::process::ExitCode;

mod commands;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: evident effect-run <file> [--max-steps N]");
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "effect-run"  => commands::effect_run::cmd_effect_run(&args[1..]),
        "help" | "--help" | "-h" => {
            eprintln!("Usage: evident effect-run <file> [--max-steps N]");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown subcommand: {}", other);
            ExitCode::from(2)
        }
    }
}
