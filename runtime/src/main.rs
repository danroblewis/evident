//! `evident` CLI — subcommands: sample, test, effect-run.

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
        "sample"      => commands::sample::cmd_sample(&args[1..]),
        "test"        => commands::test::cmd_test(&args[1..]),
        "effect-run"  => commands::effect_run::cmd_effect_run(&args[1..]),
        "dump-smtlib" => commands::dump_smtlib::cmd_dump_smtlib(&args[1..]),
        "help" | "--help" | "-h" => { usage(); ExitCode::SUCCESS }
        other => {
            eprintln!("unknown subcommand: {}", other);
            usage();
            ExitCode::from(2)
        }
    }
}
