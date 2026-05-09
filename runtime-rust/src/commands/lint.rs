//! `evident lint <file>` — Stage 11 self-hosted lint pass.
//!
//! Loads `stdlib/ast.ev` + `stdlib/passes/lint_duplicate_decls.ev`,
//! marks both as system, then loads the user's file and runs every
//! lint rule. Each rule is SAT precisely when its bug class is
//! present; SAT means "found a problem."
//!
//! Demonstrates that self-hosting works for non-inference passes
//! too — the same encode/inject/query pipeline applies.
//!
//! Exit codes:
//!   0 — no lints fired
//!   1 — load / encode error
//!   2 — usage error
//!   5 — at least one lint fired (distinguishes "found bugs" from
//!        "ran cleanly")

use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, Value};

const STDLIB_AST: &str = "stdlib/ast.ev";
const LINT_DUPS:  &str = "stdlib/passes/lint_duplicate_decls.ev";

const LINT_RULES: &[&str] = &[
    "duplicate_membership_in_body",
];

pub fn cmd_lint(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("lint: need <file.ev>");
        eprintln!("       evident lint <file.ev>");
        eprintln!();
        eprintln!("Loads stdlib/ast.ev + the lint passes, encodes the user's");
        eprintln!("program, runs each lint rule, and reports any findings.");
        eprintln!();
        eprintln!("Exit 0 — clean. Exit 5 — at least one lint fired.");
        return ExitCode::from(2);
    }
    let user_path = &args[0];

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(Path::new(STDLIB_AST)) {
        eprintln!("error: failed to load {STDLIB_AST}: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = rt.load_file(Path::new(LINT_DUPS)) {
        eprintln!("error: failed to load {LINT_DUPS}: {e}");
        return ExitCode::from(1);
    }
    rt.mark_system_loads_complete();

    if let Err(e) = rt.load_file(Path::new(user_path)) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }

    let n_claims = rt.user_claim_count();
    let mut findings = 0usize;
    for claim_idx in 0..n_claims {
        let claim_name = rt.user_claim_name(claim_idx).unwrap_or_default();
        for rule in LINT_RULES {
            match rt.query_with_program_and_nth_claim_body(
                rule, "program", "body", claim_idx,
            ) {
                Ok(Some(r)) if r.satisfied => {
                    findings += 1;
                    let dup_var = r.bindings.get("dup_var")
                        .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                        .unwrap_or_default();
                    let type_a = r.bindings.get("type_a")
                        .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                        .unwrap_or_default();
                    let type_b = r.bindings.get("type_b")
                        .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                        .unwrap_or_default();
                    println!("{}: in claim `{}`, variable `{}` is declared twice (`{}` and `{}`)",
                             rule, claim_name, dup_var, type_a, type_b);
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    if findings == 0 {
        println!("no lint issues found.");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("{} lint issue(s) found", findings);
        ExitCode::from(5)
    }
}
