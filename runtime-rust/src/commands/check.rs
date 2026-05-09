//! `evident check <files…>` — report SAT/UNSAT for every loaded
//! schema (no `--given`, no flags).

use std::collections::HashMap;
use std::process::ExitCode;

use super::common::{load_runtime, split_files_and_flags};

pub fn cmd_check(args: &[String]) -> ExitCode {
    let (files, flag_args) = split_files_and_flags(args);
    if files.is_empty() {
        eprintln!("check: need at least one file");
        return ExitCode::from(2);
    }
    if !flag_args.is_empty() {
        eprintln!("check: doesn't take flags (got {:?})", flag_args);
        return ExitCode::from(2);
    }
    let mut rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    super::desugar::auto_apply_desugar(&mut rt, &files);
    let mut names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    names.sort();
    let empty = HashMap::new();
    let mut any_unsat = false;
    for name in &names {
        match rt.query(name, &empty) {
            Ok(r) if r.satisfied  => println!("SAT    {name}"),
            Ok(_)                 => { println!("UNSAT  {name}"); any_unsat = true; }
            Err(e)                => { println!("ERROR  {name}: {e}"); any_unsat = true; }
        }
    }
    if any_unsat { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
