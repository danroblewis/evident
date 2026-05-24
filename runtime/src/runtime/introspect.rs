//! User-claim introspection + body-item rewrites used by the
//! `--infer-types` and desugar pipelines.

use crate::core::RuntimeError;
use super::EvidentRuntime;
use std::path::Path;

impl EvidentRuntime {
    /// Inject a `Membership` body item at the head of the named claim.
    /// Used by the `--infer-types` flag pipeline: after running the
    /// self-hosted inference passes against a separate runtime, the
    /// query path calls this to graft the inferred declarations onto
    /// the user's claims before solving.
    ///
    /// Returns `Ok(true)` if a Membership was added, `Ok(false)` if
    /// the variable was already declared in the claim's body (the
    /// idempotent skip lets callers loop over inferences without
    /// double-checking). `Err(UnknownSchema)` if the named claim
    /// doesn't exist.
    ///
    /// Mutates both `self.schemas` (the lookup table) and
    /// `self.program.schemas` (the parsed Program — for encoder
    /// consistency on subsequent calls). Clears the cache so a
    /// re-query rebuilds with the new shape.
    pub fn add_membership_to_claim(
        &mut self,
        claim_name: &str,
        var_name: &str,
        type_name: &str,
    ) -> Result<bool, RuntimeError> {
        use crate::core::ast::{BodyItem, Pins};
        let already_declared = |body: &[BodyItem]| -> bool {
            body.iter().any(|i| matches!(
                i, BodyItem::Membership { name, .. } if name == var_name
            ))
        };
        let new_item = BodyItem::Membership {
            name: var_name.to_string(),
            type_name: type_name.to_string(),
            pins: Pins::None,
        };
        // Update the lookup table.
        let schema = self.schemas.get_mut(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        if already_declared(&schema.body) {
            return Ok(false);
        }
        schema.body.insert(0, new_item.clone());
        // Mirror in self.program.schemas so the encoder sees the same
        // body shape on subsequent queries.
        for s in &mut self.program.schemas {
            if s.name == claim_name && !already_declared(&s.body) {
                s.body.insert(0, new_item.clone());
            }
        }
        // Cached solver still has the old body asserted; flush.
        self.cache.borrow_mut().clear();
        Ok(true)
    }

    /// Replace `body[body_idx]` of the named claim with `new_item`.
    /// Mirrors `add_membership_to_claim`'s dual-update pattern so
    /// both the schemas lookup and the encoder see the rewrite.
    pub fn replace_body_item_in_claim(
        &mut self,
        claim_name: &str,
        body_idx: usize,
        new_item: crate::core::ast::BodyItem,
    ) -> Result<bool, RuntimeError> {
        let schema = self.schemas.get_mut(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        if body_idx >= schema.body.len() { return Ok(false); }
        schema.body[body_idx] = new_item.clone();
        for s in &mut self.program.schemas {
            if s.name == claim_name && body_idx < s.body.len() {
                s.body[body_idx] = new_item.clone();
            }
        }
        self.cache.borrow_mut().clear();
        Ok(true)
    }

    /// Number of claims the user has loaded (after
    /// `mark_system_loads_complete`). Used by callers iterating over
    /// claims with `query_with_program_and_nth_claim_body`.
    pub fn user_claim_count(&self) -> usize {
        self.user_program().schemas.len()
    }

    /// Name of the n-th user claim, if any. Used by the CLI to
    /// label per-claim inference output.
    pub fn user_claim_name(&self, idx: usize) -> Option<String> {
        self.user_program().schemas.get(idx).map(|s| s.name.clone())
    }

    /// Body length of the n-th user claim. Used by the desugar
    /// pipeline to bound the index loop over `body[i]` queries.
    pub fn user_claim_body_len(&self, idx: usize) -> Option<usize> {
        self.user_program().schemas.get(idx).map(|s| s.body.len())
    }

    /// Indices into `user_program().schemas` for claims directly
    /// defined in `path` (not pulled in via `import`). Used by the
    /// inference pipeline to skip helper claims from imported
    /// libraries — for `mario_shader.ev` (which imports `engine.ev`
    /// and `level_data.ev` adding 20+ helper claims), this cuts
    /// per-claim iteration from 26 schemas to typically 1-3.
    ///
    /// Returns indices in the same order as `user_program().schemas`.
    /// Falls back to all user-claim indices if the runtime has no
    /// origin tracking for `path` (which can happen with
    /// `load_source` instead of `load_file`).
    pub fn user_claim_indices_in_file(&self, path: &Path) -> Vec<usize> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let origins = self.schema_origins.borrow();
        let mut out = Vec::new();
        let user = self.user_program();
        // If we have NO origins recorded for this path, the file
        // likely wasn't loaded via load_file (e.g. tests use
        // load_source). Fall back to all user claims.
        let has_any = origins.values().any(|p| *p == canonical);
        if !has_any {
            return (0..user.schemas.len()).collect();
        }
        for (i, s) in user.schemas.iter().enumerate() {
            if let Some(origin) = origins.get(&s.name) {
                if *origin == canonical {
                    out.push(i);
                }
            }
        }
        out
    }
}
