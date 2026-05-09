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
const PROPAGATION:   &str = "stdlib/passes/propagation.ev";
const CONSISTENCY:   &str = "stdlib/passes/consistency.ev";

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
    // Stage 9: cross-body-item propagation through `=`.
    "propagate_string",
    "propagate_int",
    "propagate_bool",
];

/// Stage 10: consistency-check rules. Each is SAT precisely when
/// the named bug is present in the user's code. Unlike inference
/// rules, finding a SAT here is a USER ERROR worth surfacing.
const CONFLICT_RULES: &[&str] = &[
    "conflict_string_decl_with_int_assignment",
    "conflict_int_decl_with_string_assignment",
    "conflict_bool_decl_with_int_assignment",
    "conflict_bool_decl_with_string_assignment",
];

/// One inference fact: in `claim_name`, variable `var` was inferred
/// to have type `type_name`, by `source_rule`. Multiple inferences
/// for the same `(claim_name, var)` with different types signal
/// ambiguity.
#[derive(Debug, Clone)]
pub struct Inference {
    pub claim_name: String,
    pub var: String,
    pub type_name: String,
    pub source_rule: String,
}

/// Public callable used by `evident query --infer-types` (and any
/// future flag that needs inference results without printing them).
/// Sets up its own EvidentRuntime, loads the inference passes,
/// loads the user files, runs every rule per claim, and returns the
/// flat list of (claim, var, type, rule) tuples it found.
///
/// Returns `Err` only on load/encode failure. Empty result + no
/// error means the inference rules ran cleanly but found nothing
/// to bind.
pub fn collect_inferences(user_files: &[String])
    -> Result<Vec<Inference>, String>
{
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST))
        .map_err(|e| format!("load {STDLIB_AST}: {e}"))?;
    for f in [LITERAL_TYPES, ITER_TYPES, PROPAGATION, CONSISTENCY] {
        rt.load_file(Path::new(f))
            .map_err(|e| format!("load {f}: {e}"))?;
    }
    rt.mark_system_loads_complete();
    for path in user_files {
        rt.load_file(Path::new(path))
            .map_err(|e| format!("load {path}: {e}"))?;
    }

    // Encode the user's Program ONCE and reuse it across every
    // rule call below. Without this cache, each rule re-walked the
    // user's full AST and rebuilt an identical Datatype value —
    // ~70-85% of the inference overhead on big programs (see
    // commit `e767b52`'s notes for measurements on mario_shader).
    let prog_value = rt.encode_program_value()
        .map_err(|e| format!("encode program: {e}"))?;

    let mut out: Vec<Inference> = Vec::new();
    for rule in PROGRAM_RULES {
        if let Ok(r) = rt.query_with_program_value(
            rule, "program", prog_value.clone(),
        ) {
            if r.satisfied {
                if let Some((var, typ, claim)) = render_bindings(rule, &r.bindings) {
                    out.push(Inference {
                        claim_name: claim, var, type_name: typ,
                        source_rule: rule.to_string(),
                    });
                }
            }
        }
    }
    // ITER_RULES never reference `program` — only `body` and
    // `body_len`. Use the body-only injection path which skips
    // asserting the deep encoded-Program equality. On big programs
    // this saves the bulk of the inference cost.
    let n_claims = rt.user_claim_count();
    for claim_idx in 0..n_claims {
        let claim_name = rt.user_claim_name(claim_idx).unwrap_or_default();
        for rule in ITER_RULES {
            if let Ok(Some(r)) = rt.query_with_nth_claim_body_only(
                rule, "body", claim_idx,
            ) {
                if r.satisfied {
                    if let Some((var, typ, _)) = render_bindings(rule, &r.bindings) {
                        out.push(Inference {
                            claim_name: claim_name.clone(),
                            var, type_name: typ,
                            source_rule: rule.to_string(),
                        });
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Convenience for the `query`/`sample`/`execute` paths: run the
/// inference pipeline against `user_files`, filter to unambiguous
/// `(claim, var)` keys, and apply each as a Membership to `rt`.
/// Returns the count of memberships actually added (skipping ones
/// the user already declared). On any failure, prints a warning
/// to stderr and returns 0 — non-fatal so the caller's main flow
/// continues even if inference machinery is broken or files are
/// missing.
///
/// Quietness: nothing printed when 0 inferences are applied;
/// otherwise one stderr line `inference: added N Membership(s)`.
pub fn auto_apply_inferences(
    rt: &mut EvidentRuntime,
    user_files: &[String],
) -> usize {
    let inferences = match collect_inferences(user_files) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: inference pipeline failed: {e}");
            eprintln!("(continuing without inferences; pass --strict to suppress this message)");
            return 0;
        }
    };
    let unambiguous = unambiguous_inferences(&inferences);
    let mut applied = 0usize;
    for inf in &unambiguous {
        match rt.add_membership_to_claim(&inf.claim_name, &inf.var, &inf.type_name) {
            Ok(true)  => { applied += 1; }
            Ok(false) => { /* user already declared, skip silently */ }
            Err(e) => eprintln!("warning: couldn't add Membership for `{}` in `{}`: {e}",
                                inf.var, inf.claim_name),
        }
    }
    if applied > 0 {
        eprintln!("inference: added {applied} Membership(s)");
    }
    applied
}


/// Filter `inferences` to those that are unambiguous —
/// `(claim_name, var)` keys mapped to exactly one distinct type.
/// Discards conflicts; useful for callers that want to apply
/// inferences as Memberships and need confidence the type is
/// uncontested.
pub fn unambiguous_inferences(inferences: &[Inference]) -> Vec<Inference> {
    use std::collections::HashMap;
    let mut by_key: HashMap<(String, String), Vec<&Inference>> = HashMap::new();
    for inf in inferences {
        by_key.entry((inf.claim_name.clone(), inf.var.clone()))
            .or_default()
            .push(inf);
    }
    let mut out = Vec::new();
    for (_key, infs) in by_key {
        let mut types: std::collections::BTreeSet<&str> =
            infs.iter().map(|i| i.type_name.as_str()).collect();
        if types.len() == 1 {
            // Pick the first inference for this (claim, var); type
            // is the same across all of them.
            out.push(infs[0].clone());
            let _ = types.pop_first();
        }
        // Else ambiguous — drop.
    }
    out
}

/// Tag for output labels: how the rule renders the inferred fact.
fn label_for(rule: &str) -> &'static str {
    if rule.starts_with("propagate_") { "propagated through `=`" }
    else if rule.starts_with("has_") { "found via iteration" }
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
    // iter_types.ev assignment rules + propagation.ev rules:
    // derive type from rule name (`has_string_*` / `propagate_string`,
    // etc.).
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
    // Stage 10: parse --strict flag (any position).
    let strict = args.iter().any(|a| a == "--strict");
    let positional: Vec<&String> = args.iter()
        .filter(|a| a.as_str() != "--strict").collect();
    if positional.is_empty() {
        eprintln!("infer-types: need <file.ev>");
        eprintln!("       evident infer-types [--strict] <file.ev>");
        eprintln!();
        eprintln!("Loads stdlib/ast.ev + the inference passes (literal_types,");
        eprintln!("iter_types, propagation, consistency), encodes the user's");
        eprintln!("program, runs every rule, and prints recovered bindings.");
        eprintln!();
        eprintln!("--strict: exit 4 if any consistency check fires (a real bug)");
        eprintln!("          OR any variable's inferred type is ambiguous.");
        return ExitCode::from(2);
    }
    let user_path = positional[0];

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
    if let Err(e) = rt.load_file(Path::new(PROPAGATION)) {
        eprintln!("error: failed to load {PROPAGATION}: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = rt.load_file(Path::new(CONSISTENCY)) {
        eprintln!("error: failed to load {CONSISTENCY}: {e}");
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
    // Encode the user's Program ONCE; reuse across every rule call
    // below. Saves the ~70-85% of inference cost spent re-walking
    // the AST per rule on big programs (mario_shader etc.).
    let prog_value = match rt.encode_program_value() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: encode program failed: {e}");
            return ExitCode::from(1);
        }
    };
    // PROGRAM_RULES match on the WHOLE program value — they
    // pattern-match the SchemaList shape internally. Run once.
    for rule in PROGRAM_RULES {
        match rt.query_with_program_value(rule, "program", prog_value.clone()) {
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
            // Body-only path: skips the encoded-Program assertion.
            // ITER_RULES never reference `program`, so the empty
            // value is harmless and saves the deep-equality cost.
            match rt.query_with_nth_claim_body_only(rule, "body", claim_idx) {
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
    // Stage 10: run the consistency checks per claim. Each rule's
    // SAT means "found this bug." Print + collect for strict-mode.
    // Same body-only path — consistency rules also don't use program.
    let mut conflicts_found: Vec<(String, String, String)> = Vec::new();
    for claim_idx in 0..n_claims {
        let claim_name = rt.user_claim_name(claim_idx).unwrap_or_default();
        for rule in CONFLICT_RULES {
            match rt.query_with_nth_claim_body_only(rule, "body", claim_idx) {
                Ok(Some(r)) if r.satisfied => {
                    let bad_var = r.bindings.get("bad_var")
                        .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                        .unwrap_or_default();
                    eprintln!("⚠ consistency: {rule} fires in claim `{}` for var `{}`",
                              claim_name, bad_var);
                    conflicts_found.push((claim_name.clone(),
                                          rule.to_string(), bad_var));
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    // --strict path: ambiguity (in the aggregator) OR any conflict
    // → exit 4. Detect ambiguity by re-walking the inferences map.
    let has_ambiguity = {
        use std::collections::BTreeMap;
        let mut by: BTreeMap<(String, String), std::collections::BTreeSet<String>>
            = BTreeMap::new();
        for (claim, _, var, typ, _) in &inferences {
            by.entry((claim.clone(), var.clone())).or_default().insert(typ.clone());
        }
        by.values().any(|types| types.len() > 1)
    };

    aggregate_and_print(&inferences);

    if strict && (!conflicts_found.is_empty() || has_ambiguity) {
        if !conflicts_found.is_empty() {
            eprintln!();
            eprintln!("strict mode: {} consistency conflict(s) found", conflicts_found.len());
        }
        if has_ambiguity {
            eprintln!();
            eprintln!("strict mode: at least one variable has an ambiguous inferred type");
        }
        return ExitCode::from(4);
    }

    if any_match || !conflicts_found.is_empty() {
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
