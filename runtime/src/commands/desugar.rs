//! Self-hosted desugar pipeline.
//!
//! Sibling of `infer_types.rs`, but where inference ADDS facts
//! (Memberships derived from program shape), desugar REWRITES body
//! items into normalized forms that the rest of the runtime knows
//! how to handle without special-case logic.
//!
//! Currently one rewrite:
//!   `BodyItem::Constraint(Expr::Identifier(name))` where `name` is
//!   a known schema → `BodyItem::Passthrough(name)`. Previously this
//!   was handled inline by a match arm in `translate/inline.rs`;
//!   that arm is now removed.
//!
//! Pipeline shape:
//!   1. Spin up an isolated EvidentRuntime, load `stdlib/ast.ev` +
//!      `stdlib/passes/desugar_passthrough.ev`, then load the user's
//!      files. (Mirrors `collect_inferences` to avoid polluting the
//!      caller's runtime with stdlib schemas.)
//!   2. For each user-defined claim and each index in its body, query
//!      `is_passthrough_at_index` with `target_idx` pinned. SAT means
//!      that body item is a bare-identifier constraint; the model
//!      binds `target_name`.
//!   3. Filter `target_name` against the set of known schema names
//!      (the part Evident can't easily check from inside the pass —
//!      iterating `program.schemas` LinkedList isn't supported yet).
//!   4. Apply each (claim_name, body_idx, name) triple to the
//!      caller's runtime via `replace_body_item_in_claim`.
//!
//! This is a deliberate proof-of-concept of the desugar shape, not
//! a payoff in lines-of-code reduction (the inline.rs match arm was
//! ~13 lines; this file is more). What it ships is the rails for
//! larger future migrations and the integration test that proves the
//! self-hosted pass actually drives runtime behavior.

use std::collections::HashSet;
use std::path::Path;

use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::ast::{BodyItem, Expr};

use super::common::load_runtime_with_passes;

const DESUGAR_PASSTHROUGH: &str = "stdlib/passes/desugar_passthrough.ev";
const RULE_NAME:           &str = "is_passthrough_at_index";

/// One detected rewrite: in `claim_name`, replace `body[body_idx]`
/// with `BodyItem::Passthrough(target_name)`.
#[derive(Debug, Clone)]
pub struct Rewrite {
    pub claim_name:  String,
    pub body_idx:    usize,
    pub target_name: String,
}

/// Find every (claim, body_idx, name) triple where the body item is
/// a bare-identifier constraint AND the identifier names a known
/// schema. Spins up its own runtime so the caller's state isn't
/// touched.
pub fn collect_passthrough_rewrites(user_files: &[String])
    -> Result<Vec<Rewrite>, String>
{
    let rt = load_runtime_with_passes(&[DESUGAR_PASSTHROUGH], user_files)?;

    // Set of every claim name the user (transitively) loaded — the
    // filter for "is target_name a known schema". `schema_names`
    // includes system schemas too, but that's fine: the match arm
    // we're replacing didn't distinguish either.
    let known: HashSet<String> = rt.schema_names().map(|s| s.to_string()).collect();

    let mut out: Vec<Rewrite> = Vec::new();

    let mut indices: std::collections::BTreeSet<usize> =
        std::collections::BTreeSet::new();
    for f in user_files {
        for i in rt.user_claim_indices_in_file(Path::new(f)) {
            indices.insert(i);
        }
    }
    for claim_idx in indices {
        let claim_name = rt.user_claim_name(claim_idx).unwrap_or_default();
        let body_len = rt.user_claim_body_len(claim_idx).unwrap_or(0);
        for body_idx in 0..body_len {
            let mut given = std::collections::HashMap::new();
            given.insert("target_idx".to_string(), Value::Int(body_idx as i64));
            let r = rt.query_with_nth_claim_body_only_given(
                RULE_NAME, "body", claim_idx, given,
            );
            let Ok(Some(qr)) = r else { continue };
            if !qr.satisfied { continue; }
            let Some(Value::Str(name)) = qr.bindings.get("target_name") else { continue };
            if !known.contains(name) { continue; }
            out.push(Rewrite {
                claim_name: claim_name.clone(),
                body_idx,
                target_name: name.clone(),
            });
        }
    }
    Ok(out)
}

/// Apply every detected rewrite to `rt`. Quiet on success; prints
/// one stderr warning if the pipeline fails (non-fatal — caller
/// continues without rewrites). Returns the number of body items
/// actually rewritten.
pub fn auto_apply_desugar(
    rt: &mut EvidentRuntime,
    user_files: &[String],
) -> usize {
    let rewrites = match collect_passthrough_rewrites(user_files) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: desugar pipeline failed: {e}");
            return 0;
        }
    };
    let mut applied = 0usize;
    for r in &rewrites {
        let new_item = BodyItem::Passthrough(r.target_name.clone());
        // Sanity check: only rewrite if the body item still matches
        // the expected shape (defends against running twice or against
        // a body that mutated since collect_passthrough_rewrites ran).
        let still_matches = rt.get_schema(&r.claim_name)
            .and_then(|s| s.body.get(r.body_idx))
            .map(|item| matches!(item,
                BodyItem::Constraint(Expr::Identifier(n)) if n == &r.target_name
            ))
            .unwrap_or(false);
        if !still_matches { continue; }
        if let Ok(true) = rt.replace_body_item_in_claim(&r.claim_name, r.body_idx, new_item) {
            applied += 1;
        }
    }
    applied
}
