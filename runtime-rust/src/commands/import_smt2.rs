//! `evident import-smt2 <file> [name]` — read SMT-LIB v2 text and
//! emit Evident source on stdout, wrapped in a single `claim` block.
//! `name` defaults to `imported` if omitted.

use std::path::PathBuf;
use std::process::ExitCode;

use evident_runtime::smtlib;

pub fn cmd_import_smt2(args: &[String]) -> ExitCode {
    if args.is_empty() || args.len() > 2 {
        eprintln!("usage: evident import-smt2 <file> [claim_name]");
        return ExitCode::from(2);
    }
    let path = PathBuf::from(&args[0]);
    let name = args.get(1).cloned().unwrap_or_else(|| "imported".to_string());

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("import-smt2: read {}: {e}", path.display());
            return ExitCode::from(2);
        }
    };

    match smtlib::import(&src) {
        Ok(items) => {
            print!("{}", smtlib::body_items_to_evident(&name, &items));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("import-smt2: {e}");
            eprintln!();
            eprintln!("v1 of the importer handles declare-const, zero-arg");
            eprintln!("declare-fun, and assert with arithmetic + comparison");
            eprintln!("+ logical ops. See docs/design/smt-lib-as-ir.md for");
            eprintln!("what's in/out of scope.");
            ExitCode::from(1)
        }
    }
}
