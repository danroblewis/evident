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

use super::common::load_runtime_with_passes;

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


/// One inference fact: in `claim_name`, variable `var` was inferred
/// to have type `type_name`, by `source_rule`. Multiple inferences
/// for the same `(claim_name, var)` with different types signal
/// ambiguity.
#[derive(Debug, Clone)]
pub struct Inference {
    pub claim_name: String,
    pub var: String,
    pub type_name: String,
}

/// Public callable used by `evident query` (and any future flag
/// that needs inference results without printing them).
/// Sets up its own EvidentRuntime, loads the inference passes,
/// loads the user files, runs every rule per claim DIRECTLY
/// DEFINED in those files (skipping transitively-imported claims
/// that are typically library helpers), and returns the flat list
/// of (claim, var, type, rule) tuples it found.
///
/// Returns `Err` only on load/encode failure. Empty result + no
/// error means the inference rules ran cleanly but found nothing
/// to bind.
pub fn collect_inferences(user_files: &[String])
    -> Result<Vec<Inference>, String>
{
    let rt = load_runtime_with_passes(
        &[LITERAL_TYPES, ITER_TYPES, PROPAGATION, CONSISTENCY],
        user_files,
    )?;

    let mut out: Vec<Inference> = Vec::new();

    // PROGRAM_RULES pattern-match the whole Program shape — they
    // require `MakeProgram(SchLCons(_, SchLNil), …)` (exactly one
    // user schema). For multi-schema user programs (mario_shader
    // and other real-world code with multiple claims), they're
    // structurally UNSAT and we'd just pay solver setup cost for
    // nothing. Skip them.
    let n_claims = rt.user_claim_count();
    if n_claims == 1 {
        // Encode the user's Program ONCE and reuse across the
        // PROGRAM_RULES loop. Cached so we don't re-walk the AST
        // per rule. Only built when single-claim — ITER_RULES use
        // a cheap empty Program injection instead.
        let prog_value = rt.encode_program_value()
            .map_err(|e| format!("encode program: {e}"))?;
        for rule in PROGRAM_RULES {
            if let Ok(r) = rt.query_with_program_value(
                rule, "program", prog_value.clone(),
            ) {
                if r.satisfied {
                    if let Some((var, typ, claim)) = render_bindings(rule, &r.bindings) {
                        out.push(Inference {
                            claim_name: claim, var, type_name: typ,
                        });
                    }
                }
            }
        }
    }
    // ITER_RULES never reference `program` — only `body` and
    // `body_len`. Use the body-only injection path which skips
    // asserting the deep encoded-Program equality. Restrict to
    // claims directly defined in the user's specified files, not
    // transitively imported helpers (those are usually library
    // code with explicit type declarations and don't benefit from
    // inference).
    let mut indices: std::collections::BTreeSet<usize> =
        std::collections::BTreeSet::new();
    for f in user_files {
        for i in rt.user_claim_indices_in_file(Path::new(f)) {
            indices.insert(i);
        }
    }
    for claim_idx in indices {
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

