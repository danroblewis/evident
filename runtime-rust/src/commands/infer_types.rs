//! `evident infer-types <file>` — Stage 3 user-facing demo. Loads
//! `stdlib/ast.ev` + `stdlib/passes/literal_types.ev`, marks both
//! as system, then loads the user's file and runs each inference
//! rule (`infer_string_from_single_assignment`,
//! `infer_int_from_single_assignment`,
//! `infer_bool_from_single_assignment`) against the encoded program.
//! Prints the bindings any rule satisfied.
//!
//! v0.1: pass rules are narrow (single-claim, single-body-item).
//! Real iteration over arbitrary-length BodyItemLists is Stage 4.
//!
//! Exit codes:
//!   0 — at least one rule produced bindings
//!   1 — load / encode error
//!   2 — usage error
//!   3 — no rule matched (the program doesn't fit any v0.1 pattern;
//!       not an error, but worth distinguishing from "matched")

use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, Value};

const STDLIB_AST:    &str = "stdlib/ast.ev";
const LITERAL_TYPES: &str = "stdlib/passes/literal_types.ev";

const RULES: &[&str] = &[
    "infer_string_from_single_assignment",
    "infer_int_from_single_assignment",
    "infer_bool_from_single_assignment",
];

pub fn cmd_infer_types(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("infer-types: need <file.ev>");
        eprintln!("       evident infer-types <file.ev>");
        eprintln!();
        eprintln!("Loads stdlib/ast.ev + stdlib/passes/literal_types.ev,");
        eprintln!("encodes the user's program, runs each inference rule,");
        eprintln!("and prints any bindings recovered.");
        return ExitCode::from(2);
    }
    let user_path = &args[0];

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(Path::new(STDLIB_AST)) {
        eprintln!("error: failed to load {STDLIB_AST}: {e}");
        eprintln!("       (run from the repo root or ensure {STDLIB_AST} is reachable)");
        return ExitCode::from(1);
    }
    if let Err(e) = rt.load_file(Path::new(LITERAL_TYPES)) {
        eprintln!("error: failed to load {LITERAL_TYPES}: {e}");
        return ExitCode::from(1);
    }
    rt.mark_system_loads_complete();

    if let Err(e) = rt.load_file(Path::new(user_path)) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }

    let mut any_match = false;
    for rule in RULES {
        match rt.query_with_program(rule, "program") {
            Ok(r) if r.satisfied => {
                any_match = true;
                let var = r.bindings.get("inferred_var")
                    .and_then(|v| if let Value::Str(s) = v { Some(s.as_str()) } else { None })
                    .unwrap_or("?");
                let typ = r.bindings.get("inferred_type")
                    .and_then(|v| if let Value::Str(s) = v { Some(s.as_str()) } else { None })
                    .unwrap_or("?");
                let claim_name = r.bindings.get("claim_name")
                    .and_then(|v| if let Value::Str(s) = v { Some(s.as_str()) } else { None })
                    .unwrap_or("?");
                println!("{}: inferred `{}` ∈ {} (in claim `{}`)",
                         rule, var, typ, claim_name);
            }
            Ok(_) => { /* rule didn't match this program; silent */ }
            Err(e) => {
                eprintln!("error: rule `{rule}` failed: {e}");
                return ExitCode::from(1);
            }
        }
    }

    if any_match {
        ExitCode::SUCCESS
    } else {
        eprintln!("no inference rule matched this program.");
        eprintln!("(v0.1 rules are narrow — single-claim, single-body-item");
        eprintln!(" programs of shape `claim NAME : var = literal` only.)");
        ExitCode::from(3)
    }
}
