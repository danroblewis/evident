//! `evident infer-types <file>` — Stage 6 user-facing self-hosted
//! inference. Loads `stdlib/ast.ev` + the two pass files and runs:
//!
//!   * `literal_types.ev` rules — pattern-match fixed body shapes
//!     (head Membership, single-body assignments, 2-body
//!     declaration+assignment).
//!   * `iter_types.ev` rules — iterate via ∃ over the user's first
//!     claim's body to find Membership / String / Int / Bool
//!     assignments anywhere.
//!
//! Each successful rule prints a one-line summary of the inferred
//! variable + type. Order goes most-specific → most-general:
//! literal_types.ev's extract first, then membership+assignment,
//! then single-assignment, then iter_types.ev's existentials.
//!
//! Exit codes:
//!   0 — at least one rule produced bindings
//!   1 — load / encode error
//!   2 — usage error
//!   3 — no rule matched (the program doesn't fit any v0.1 pattern)

use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, Value};

const STDLIB_AST:    &str = "stdlib/ast.ev";
const LITERAL_TYPES: &str = "stdlib/passes/literal_types.ev";
const ITER_TYPES:    &str = "stdlib/passes/iter_types.ev";

/// Rules invoked via `query_with_program` (no body Seq needed).
const PROGRAM_RULES: &[&str] = &[
    "extract_first_membership",
    "infer_string_from_membership_plus_assignment",
    "infer_int_from_membership_plus_assignment",
    "infer_bool_from_membership_plus_assignment",
    "infer_string_from_single_assignment",
    "infer_int_from_single_assignment",
    "infer_bool_from_single_assignment",
];

/// Rules invoked via `query_with_program_and_body` (need the
/// iteration-friendly flat Seq(BodyItem) injection).
const ITER_RULES: &[&str] = &[
    "has_membership_of_var",
    "has_string_assignment",
    "has_int_assignment",
    "has_bool_assignment",
];

/// Tag for output labels: how the rule renders the inferred fact.
fn label_for(rule: &str) -> &'static str {
    if rule.starts_with("has_") { "found via iteration" }
    else if rule.starts_with("extract_") { "extracted from declaration" }
    else if rule.starts_with("infer_") && rule.contains("membership_plus") {
        "inferred from declaration + assignment"
    } else if rule.starts_with("infer_") {
        "inferred from literal assignment"
    } else { "matched" }
}

/// Pull the variable name + type out of common binding shapes.
/// Different rule families bind slightly different names —
/// extract/infer use `inferred_var`/`inferred_type`; iter uses
/// `target_var`/`target_type` (or `target_var`/literal).
fn render_bindings(rule: &str, b: &std::collections::HashMap<String, Value>)
    -> Option<(String, String, String)>
{
    let claim_name = b.get("claim_name").and_then(|v| match v {
        Value::Str(s) => Some(s.clone()),
        _ => None,
    }).unwrap_or_default();
    // literal_types.ev: inferred_var, inferred_type
    if let (Some(Value::Str(v)), Some(Value::Str(t))) =
        (b.get("inferred_var"), b.get("inferred_type"))
    {
        return Some((v.clone(), t.clone(), claim_name));
    }
    // iter_types.ev with target_type (has_membership_of_var)
    if let (Some(Value::Str(v)), Some(Value::Str(t))) =
        (b.get("target_var"), b.get("target_type"))
    {
        return Some((v.clone(), t.clone(), claim_name));
    }
    // iter_types.ev assignment rules: derive type from rule name.
    if let Some(Value::Str(v)) = b.get("target_var") {
        let typ = if rule.contains("string") { "String" }
                  else if rule.contains("int") { "Int" }
                  else if rule.contains("bool") { "Bool" }
                  else { "?" };
        return Some((v.clone(), typ.to_string(), claim_name));
    }
    None
}

pub fn cmd_infer_types(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("infer-types: need <file.ev>");
        eprintln!("       evident infer-types <file.ev>");
        eprintln!();
        eprintln!("Loads stdlib/ast.ev + the literal_types and iter_types passes,");
        eprintln!("encodes the user's program, runs every inference rule, and");
        eprintln!("prints any bindings recovered.");
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
    if let Err(e) = rt.load_file(Path::new(ITER_TYPES)) {
        eprintln!("error: failed to load {ITER_TYPES}: {e}");
        return ExitCode::from(1);
    }
    rt.mark_system_loads_complete();

    if let Err(e) = rt.load_file(Path::new(user_path)) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }

    // Stage 8: track inferences per-claim so the aggregator can
    // group output. (claim_name, rule, var, type, label) tuples.
    let mut inferences: Vec<(String, String, String, String, String)> = Vec::new();
    let mut any_match = false;
    // PROGRAM_RULES match on the WHOLE program value — they
    // pattern-match the SchemaList shape internally. Run once.
    for rule in PROGRAM_RULES {
        match rt.query_with_program(rule, "program") {
            Ok(r) if r.satisfied => {
                any_match = true;
                if let Some((var, typ, claim)) = render_bindings(rule, &r.bindings) {
                    let where_ = if claim.is_empty() { String::new() }
                                 else { format!(" (in claim `{}`)", claim) };
                    println!("{rule}: {} `{}` ∈ {}{}", label_for(rule), var, typ, where_);
                    inferences.push((claim.clone(), rule.to_string(), var, typ,
                                     label_for(rule).to_string()));
                }
            }
            Ok(_)  => {}
            Err(e) => {
                eprintln!("error: rule `{rule}` failed: {e}");
                return ExitCode::from(1);
            }
        }
    }
    // ITER_RULES inject one body at a time. Loop over every user
    // claim — each call sees a different body Seq. The pass file
    // doesn't change; we just call it N times.
    let n_claims = rt.user_claim_count();
    for claim_idx in 0..n_claims {
        let claim_name = rt.user_claim_name(claim_idx).unwrap_or_default();
        for rule in ITER_RULES {
            match rt.query_with_program_and_nth_claim_body(
                rule, "program", "body", claim_idx,
            ) {
                Ok(Some(r)) if r.satisfied => {
                    any_match = true;
                    if let Some((var, typ, _)) = render_bindings(rule, &r.bindings) {
                        println!("{rule}: {} `{}` ∈ {} (in claim `{}`)",
                                 label_for(rule), var, typ, claim_name);
                        inferences.push((claim_name.clone(), rule.to_string(),
                                         var, typ, label_for(rule).to_string()));
                    }
                }
                Ok(_)  => {}
                Err(e) => {
                    eprintln!("error: rule `{rule}` failed: {e}");
                    return ExitCode::from(1);
                }
            }
        }
    }
    aggregate_and_print(&inferences);

    if any_match {
        ExitCode::SUCCESS
    } else {
        eprintln!("no inference rule matched this program.");
        eprintln!("(v0.1 rules cover: head Membership, single-body");
        eprintln!(" assignments, 2-body decl+assignment, and ∃-over-body");
        eprintln!(" Membership/String/Int/Bool assignment.)");
        ExitCode::from(3)
    }
}

/// Stage 8: dedupe and aggregate inferences across all rules and
/// all user claims into a unified table grouped by claim.
///
/// For each claim, vars get one row per declared type (single row
/// when unambiguous). The same `(var, type)` pair found by multiple
/// rules collapses to one row with `(via R1, R2, …)` attribution.
/// Ambiguity (multiple types for one var in one claim) is flagged
/// with `*ambiguous*` per the Stage 7 contract.
fn aggregate_and_print(
    inferences: &[(String, String, String, String, String)],
    // (claim_name, rule_name, var, type_name, label) tuples
) {
    if inferences.is_empty() { return; }
    use std::collections::BTreeMap;
    // claim → var → (type → Vec<rule>)
    let mut by_claim: BTreeMap<String,
        BTreeMap<String, BTreeMap<String, Vec<String>>>> = BTreeMap::new();
    for (claim, rule, var, typ, _label) in inferences {
        by_claim.entry(claim.clone()).or_default()
            .entry(var.clone()).or_default()
            .entry(typ.clone()).or_default()
            .push(rule.clone());
    }

    println!();
    println!("Inferred types:");
    let multi_claim = by_claim.len() > 1
        || (by_claim.len() == 1 && !by_claim.contains_key(""));
    for (claim, by_var) in &by_claim {
        let prefix = if multi_claim {
            if claim.is_empty() {
                println!("  (no claim attribution):");
            } else {
                println!("  in claim `{}`:", claim);
            }
            "    "
        } else {
            "  "
        };
        let max_var = by_var.keys().map(|v| v.len()).max().unwrap_or(0);
        for (var, types) in by_var {
            if types.len() == 1 {
                let (typ, rules) = types.iter().next().unwrap();
                let rules_str = rules.join(", ");
                println!("{prefix}{:<width$} : {}    (via {})",
                         var, typ, rules_str, width = max_var);
            } else {
                println!("{prefix}{:<width$} : *ambiguous* — got {} different types:",
                         var, types.len(), width = max_var);
                for (typ, rules) in types {
                    println!("{prefix}    {} (via {})", typ, rules.join(", "));
                }
            }
        }
    }
}
