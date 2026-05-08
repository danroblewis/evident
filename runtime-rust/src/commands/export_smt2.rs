//! `evident export-smt2 <file> <claim>` — print one claim's
//! constraints as SMT-LIB v2 text on stdout. Out-of-scope features
//! (passthroughs, datatypes, sequences, …) error with a pointer to
//! `docs/design/smt-lib-as-ir.md` so the user knows the v1 limits.

use std::path::PathBuf;
use std::process::ExitCode;

use evident_runtime::{smtlib, EvidentRuntime};

pub fn cmd_export_smt2(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: evident export-smt2 <file> <claim>");
        return ExitCode::from(2);
    }
    let path = PathBuf::from(&args[0]);
    let name = &args[1];

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(&path) {
        eprintln!("export-smt2: load {}: {e}", path.display());
        return ExitCode::from(2);
    }

    let Some(schema) = rt.get_schema(name) else {
        eprintln!("export-smt2: no claim/type/schema named {:?} in {}",
                  name, path.display());
        return ExitCode::from(2);
    };

    match smtlib::export(schema) {
        Ok(s) => { print!("{s}"); ExitCode::SUCCESS }
        Err(e) => {
            eprintln!("export-smt2: {e}");
            eprintln!();
            eprintln!("v1 of the exporter handles primitives + arithmetic +");
            eprintln!("comparison + logical + bounded quantifiers. See");
            eprintln!("docs/design/smt-lib-as-ir.md for what's in/out of scope.");
            ExitCode::from(1)
        }
    }
}
