//! `evident check <files…>` — report SAT/UNSAT for every loaded
//! schema (no `--given`, no flags).

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::ast::BodyItem;
use evident_runtime::EvidentRuntime;

use super::common::{load_runtime, split_files_and_flags};

/// Generic-Seq parameters (`s ∈ Seq` with no element type) only
/// have a meaningful element sort at the call site via names-match.
/// Standalone evaluation of such claims would emit "unknown type
/// Seq for s" and then drop downstream constraints. Detect and
/// skip — the claim is a library helper, not a top-level test.
fn has_generic_seq_param(rt: &EvidentRuntime, name: &str) -> bool {
    let Some(decl) = rt.get_schema(name) else { return false };
    decl.body.iter().any(|item| matches!(item,
        BodyItem::Membership { type_name, .. } if type_name == "Seq"))
}

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
    let rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    let mut names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    names.sort();
    let empty = HashMap::new();
    let mut any_unsat = false;
    for name in &names {
        if has_generic_seq_param(&rt, name) {
            println!("SKIP   {name}  (generic Seq param — library helper)");
            continue;
        }
        match rt.query(name, &empty) {
            Ok(r) if r.satisfied  => println!("SAT    {name}"),
            Ok(_)                 => { println!("UNSAT  {name}"); any_unsat = true; }
            Err(e)                => { println!("ERROR  {name}: {e}"); any_unsat = true; }
        }
    }
    if any_unsat { ExitCode::from(1) } else { ExitCode::SUCCESS }
}
