//! `evident parse <file>` — debug helper. Loads the file, prints each
//! parsed schema name on its own line. Not in `evident.py`; useful for
//! quick "did this parse?" checks during runtime development.

use std::path::Path;
use std::process::ExitCode;

use evident_runtime::EvidentRuntime;

pub fn cmd_parse(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("parse: need <file.ev>");
        return ExitCode::from(2);
    }
    let path = &args[0];
    let mut rt = EvidentRuntime::new();
    match rt.load_file(Path::new(path)) {
        Ok(()) => {
            for s in rt.schema_names() { println!("{}", s); }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("parse error: {e}"); ExitCode::from(1) }
    }
}
