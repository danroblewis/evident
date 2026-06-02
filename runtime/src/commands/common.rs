//! Shared helpers for `cmd_*` subcommands. Only `auto_apply_desugar` is exposed
//! after the sample/test command deletion.

use std::collections::HashMap;
use std::path::Path;

use evident_runtime::ast::{BodyItem, Expr};
use evident_runtime::{EvidentRuntime, Value, stdlib_path};

fn load_runtime_with_passes(
    pass_files: &[&str],
    user_files: &[String],
) -> Result<EvidentRuntime, String> {
    let stdlib = stdlib_path::stdlib_dir()?;
    let mut rt = EvidentRuntime::new();
    let ast = stdlib.join("ast.ev");
    rt.load_file(&ast)
        .map_err(|e| format!("load {}: {e}", ast.display()))?;
    for f in pass_files {
        let p = stdlib.join(f);
        rt.load_file(&p)
            .map_err(|e| format!("load {}: {e}", p.display()))?;
    }
    rt.mark_system_loads_complete();
    for path in user_files {
        rt.load_file(Path::new(path))
            .map_err(|e| format!("load {path}: {e}"))?;
    }
    Ok(rt)
}

// Self-hosted desugar pass (bare-identifier → passthrough). Rule lives in
// `stdlib/passes/desugar_passthrough.ev`.
const DESUGAR_PASSTHROUGH: &str = "passes/desugar_passthrough.ev";
const PASSTHROUGH_RULE:    &str = "is_passthrough_at_index";

#[derive(Debug, Clone)]
struct Rewrite {
    claim_name:  String,
    body_idx:    usize,
    target_name: String,
}

fn collect_passthrough_rewrites(user_files: &[String]) -> Result<Vec<Rewrite>, String> {
    let rt = load_runtime_with_passes(&[DESUGAR_PASSTHROUGH], user_files)?;
    let known: std::collections::HashSet<String> =
        rt.schema_names().map(|s| s.to_string()).collect();
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
            let mut given = HashMap::new();
            given.insert("target_idx".to_string(), Value::Int(body_idx as i64));
            let r = rt.query_with_nth_claim_body_only_given(
                PASSTHROUGH_RULE, "body", claim_idx, given,
            );
            let Ok(Some(qr)) = r else { continue };
            if !qr.satisfied { continue; }
            let Some(Value::Str(name)) = qr.bindings.get("target_name") else { continue };
            if !known.contains(name) { continue; }
            out.push(Rewrite { claim_name: claim_name.clone(), body_idx, target_name: name.clone() });
        }
    }
    Ok(out)
}

pub fn auto_apply_desugar(rt: &mut EvidentRuntime, user_files: &[String]) -> usize {
    let rewrites = match collect_passthrough_rewrites(user_files) {
        Ok(v) => v,
        Err(e) => { eprintln!("warning: desugar pipeline failed: {e}"); return 0; }
    };
    let mut applied = 0usize;
    for r in &rewrites {
        let new_item = BodyItem::Passthrough(r.target_name.clone());
        let still_matches = rt.get_schema(&r.claim_name)
            .and_then(|s| s.body.get(r.body_idx))
            .map(|item| matches!(item,
                BodyItem::Constraint(Expr::Identifier(n)) if n == &r.target_name))
            .unwrap_or(false);
        if !still_matches { continue; }
        if let Ok(true) = rt.replace_body_item_in_claim(&r.claim_name, r.body_idx, new_item) {
            applied += 1;
        }
    }
    applied
}
