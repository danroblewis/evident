//! User-claim introspection + body-item rewrites used by the
//! `--infer-types` and desugar pipelines.
//!
//! The two *mutations* self-host: the AST walk + `SchemaDecl` rebuild run in
//! `stdlib/passes/introspect.ev` via `portable::introspect` (the first
//! AST-rebuild port). What stays Rust here is the leaf that the other ports
//! also keep — the string/length DECISION (already declared? idx in range?)
//! and the dual-update bookkeeping (mirror into `program.schemas`, flush the
//! cache). The pure-read accessors below stay Rust: routing a `vec.len()`
//! through a solve buys nothing.

use crate::core::RuntimeError;
use super::EvidentRuntime;
use std::path::Path;

impl EvidentRuntime {
    /// Inject `var_name ∈ type_name` at the head of the named claim (`--infer-types`).
    /// Idempotent (`Ok(false)` if already declared); mutates schemas + program + cache.
    /// The head-insert rebuild self-hosts; the idempotency check is the Rust leaf.
    pub fn add_membership_to_claim(
        &mut self,
        claim_name: &str,
        var_name: &str,
        type_name: &str,
    ) -> Result<bool, RuntimeError> {
        use crate::core::ast::BodyItem;
        let already_declared = |body: &[BodyItem]| -> bool {
            body.iter().any(|i| matches!(
                i, BodyItem::Membership { name, .. } if name == var_name
            ))
        };
        let schema = self.schemas.get_mut(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        if already_declared(&schema.body) {
            return Ok(false);
        }
        crate::portable::introspect::prepend_membership(schema, var_name, type_name);
        // Mirror in program.schemas so the encoder sees the same shape on subsequent queries.
        for s in &mut self.program.schemas {
            if s.name == claim_name && !already_declared(&s.body) {
                crate::portable::introspect::prepend_membership(s, var_name, type_name);
            }
        }
        self.cache.borrow_mut().clear();
        Ok(true)
    }

    /// Replace `body[body_idx]` of the named claim; mirrors `add_membership_to_claim`'s
    /// dual-update pattern (schemas + program.schemas + cache flush). The walk-to-index
    /// rebuild self-hosts; the bounds check is the Rust leaf.
    pub fn replace_body_item_in_claim(
        &mut self,
        claim_name: &str,
        body_idx: usize,
        new_item: crate::core::ast::BodyItem,
    ) -> Result<bool, RuntimeError> {
        let schema = self.schemas.get_mut(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        if body_idx >= schema.body.len() { return Ok(false); }
        crate::portable::introspect::replace_body_item(schema, body_idx, &new_item);
        for s in &mut self.program.schemas {
            if s.name == claim_name && body_idx < s.body.len() {
                crate::portable::introspect::replace_body_item(s, body_idx, &new_item);
            }
        }
        self.cache.borrow_mut().clear();
        Ok(true)
    }

    /// Number of user-loaded claims (after `mark_system_loads_complete`).
    pub fn user_claim_count(&self) -> usize {
        self.user_program().schemas.len()
    }

    /// Name of the n-th user claim.
    pub fn user_claim_name(&self, idx: usize) -> Option<String> {
        self.user_program().schemas.get(idx).map(|s| s.name.clone())
    }

    /// Body length of the n-th user claim.
    pub fn user_claim_body_len(&self, idx: usize) -> Option<usize> {
        self.user_program().schemas.get(idx).map(|s| s.body.len())
    }

    /// Indices of user claims originating from `path` (skips imported helpers).
    /// Falls back to all user-claim indices if no origin tracking exists for `path`.
    pub fn user_claim_indices_in_file(&self, path: &Path) -> Vec<usize> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let origins = self.schema_origins.borrow();
        let mut out = Vec::new();
        let user = self.user_program();
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
