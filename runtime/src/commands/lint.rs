//! `evident lint <file>` — static lint checks over the parsed AST.
//!
//! Currently one rule:
//!   * `duplicate_membership_in_body` — the same variable name is
//!     declared (via `∈`) twice in a single claim's body. Almost
//!     always a mistake (and even when intentional, flagging it is
//!     correct).
//!
//! Each finding is SAT-equivalent to "found a problem." The rule is a
//! plain Rust scan over each loaded claim's body items — no solver,
//! no encoded-program reflection.
//!
//! Exit codes:
//!   0 — no lints fired
//!   1 — load error
//!   2 — usage error
//!   5 — at least one lint fired (distinguishes "found bugs" from
//!        "ran cleanly")

use std::process::ExitCode;

use evident_runtime::ast::BodyItem;

use super::common::load_runtime;

pub fn cmd_lint(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("lint: need <file.ev>");
        eprintln!("       evident lint <file.ev>");
        eprintln!();
        eprintln!("Scans each claim's body for duplicate variable");
        eprintln!("declarations and reports any findings.");
        eprintln!();
        eprintln!("Exit 0 — clean. Exit 5 — at least one lint fired.");
        return ExitCode::from(2);
    }
    let user_path = &args[0];

    let rt = match load_runtime(&[user_path.clone()]) {
        Ok(r) => r,
        Err(e) => { eprintln!("error: {e}"); return ExitCode::from(1); }
    };

    // Stable, deterministic order so output doesn't depend on the
    // schema-map iteration order.
    let mut names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    names.sort();

    let mut findings = 0usize;
    for claim_name in &names {
        let Some(schema) = rt.get_schema(claim_name) else { continue };
        for (var, type_a, type_b) in duplicate_memberships(&schema.body) {
            findings += 1;
            println!(
                "duplicate_membership_in_body: in claim `{}`, variable `{}` is declared twice (`{}` and `{}`)",
                claim_name, var, type_a, type_b
            );
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

/// Find every variable declared via Membership more than once in a
/// single body. Returns `(var, first_type, later_type)` for the first
/// redeclaration of each name — in declaration order, so output is
/// stable.
fn duplicate_memberships(body: &[BodyItem]) -> Vec<(String, String, String)> {
    let mut first_type: std::collections::HashMap<&str, &str> =
        std::collections::HashMap::new();
    let mut seen_dup: std::collections::HashSet<&str> =
        std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            match first_type.get(name.as_str()) {
                None => { first_type.insert(name.as_str(), type_name.as_str()); }
                Some(&earlier) => {
                    if seen_dup.insert(name.as_str()) {
                        out.push((name.clone(), earlier.to_string(), type_name.clone()));
                    }
                }
            }
        }
    }
    out
}
