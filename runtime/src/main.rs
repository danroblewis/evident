//! `evident` — CLI for the Evident runtime.
//!
//! Subcommands:
//!   check        <files…>
//!   test         [path]
//!   effect-run   <file>           — run an effect-driven program
//!   lint         <file>

use std::process::ExitCode;

mod commands;

use commands::common::usage;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "check"       => commands::check::cmd_check(&args[1..]),
        "test"        => commands::test::cmd_test(&args[1..]),
        "effect-run"  => commands::effect_run::cmd_effect_run(&args[1..]),
        "lint"        => commands::lint::cmd_lint(&args[1..]),
        "help" | "--help" | "-h" => { usage(); ExitCode::SUCCESS }
        other => {
            eprintln!("unknown subcommand: {}", other);
            usage();
            ExitCode::from(2)
        }
    }
}
