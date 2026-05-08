//! `evident` — CLI for the Rust port of the Evident runtime.
//! Mirrors `evident.py`'s subcommand shape so the two tools are
//! interchangeable for everything the Rust runtime currently supports.
//!
//! Subcommands:
//!   query   <files…> <schema> [--given k=v …] [--json]
//!   check   <files…>
//!   sample  <files…> <schema> [-n N] [--given k=v …] [--json]
//!   test    [path]
//!   execute <file>          — run schema main as a constraint automaton
//!                             (headless: stdin → solver → stdout, or SDL
//!                              when SDLInput / SDLOutput / SDLWindow vars
//!                              are declared in main)
//!   parse   <file>          — Rust-only, debug helper
//!
//! Parked behind plugin work (covered by the Python `evident.py` but
//! not yet by this binary):
//!   batch     — stdin ↔ Seq round-trip
//!   repl      — interactive session
//!
//! Each subcommand's implementation lives in `src/commands/<name>.rs`.
//! See `src/commands.rs` (entry) and `src/commands/common.rs` (shared
//! flag parsing, value formatting, runtime loading).

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
        "query"   => commands::query::cmd_query(&args[1..]),
        "check"   => commands::check::cmd_check(&args[1..]),
        "sample"  => commands::sample::cmd_sample(&args[1..]),
        "test"    => commands::test::cmd_test(&args[1..]),
        "parse"   => commands::parse::cmd_parse(&args[1..]),
        "dump-ast" => commands::dump_ast::cmd_dump_ast(&args[1..]),
        "infer-types" => commands::infer_types::cmd_infer_types(&args[1..]),
        "execute" => commands::execute::cmd_execute(&args[1..]),
        "transpile-shader" => commands::transpile_shader::cmd_transpile_shader(&args[1..]),
        "export-smt2"      => commands::export_smt2::cmd_export_smt2(&args[1..]),
        "import-smt2"      => commands::import_smt2::cmd_import_smt2(&args[1..]),
        "batch" | "repl" => {
            eprintln!("error: '{}' is not yet implemented in the Rust runtime.", args[0]);
            eprintln!("       Use evident.py for these subcommands. See PROGRESS.md for status.");
            ExitCode::from(2)
        }
        "help" | "--help" | "-h" => { usage(); ExitCode::SUCCESS }
        other => {
            eprintln!("unknown subcommand: {}", other);
            usage();
            ExitCode::from(2)
        }
    }
}
