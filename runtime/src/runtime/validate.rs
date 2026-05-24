//! Schema validation:
//!   * `enforce_external_only` — non-external schemas can't construct FFI effects
//!   * `register_subclaims` — lift nested subclaim decls to the top-level schemas map

use super::errors::RuntimeError;
use crate::ast::{BodyItem, SchemaDecl};
use std::collections::HashMap;

/// Reject non-`external` schemas that try to construct FFI effects
/// (`FFICall` / `LibCall` / `FFIOpen` / `FFILookup`). The rule:
/// only `external` schemas (`external type` / `external claim` /
/// `external fsm`) may produce those effect values. Demos and
/// ordinary library code reach C through the `external claim`
/// wrappers in `packages/` and `stdlib/posix.ev`.
///
/// The check walks every `Constraint` body item's expression tree.
/// SubclaimDecl bodies are skipped — their own load pass handles them.
pub(super) fn enforce_external_only(s: &SchemaDecl) -> Result<(), RuntimeError> {
    use crate::ast::{BodyItem, Expr};
    if s.external { return Ok(()); }
    fn find_ffi_call(e: &Expr) -> Option<&'static str> {
        match e {
            Expr::Call(name, args) => {
                let banned = match name.as_str() {
                    "FFICall"   => Some("FFICall"),
                    "FFIOpen"   => Some("FFIOpen"),
                    "FFILookup" => Some("FFILookup"),
                    "LibCall"   => Some("LibCall"),
                    _ => None,
                };
                if banned.is_some() { return banned; }
                args.iter().filter_map(find_ffi_call).next()
            }
            Expr::Binary(_, l, r) =>
                find_ffi_call(l).or_else(|| find_ffi_call(r)),
            Expr::Not(i) | Expr::Cardinality(i) => find_ffi_call(i),
            Expr::Ternary(c, a, b) =>
                find_ffi_call(c).or_else(|| find_ffi_call(a))
                                .or_else(|| find_ffi_call(b)),
            Expr::Index(s, i) | Expr::Range(s, i) | Expr::InExpr(s, i) =>
                find_ffi_call(s).or_else(|| find_ffi_call(i)),
            Expr::Field(b, _) => find_ffi_call(b),
            Expr::Matches(e, _) => find_ffi_call(e),
            Expr::SeqLit(items) | Expr::SetLit(items) =>
                items.iter().filter_map(find_ffi_call).next(),
            Expr::Match(scr, arms) =>
                find_ffi_call(scr).or_else(|| arms.iter()
                    .filter_map(|a| find_ffi_call(&a.body)).next()),
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                find_ffi_call(r).or_else(|| find_ffi_call(b)),
            _ => None,
        }
    }
    for item in &s.body {
        if let BodyItem::Constraint(e) = item {
            if let Some(call) = find_ffi_call(e) {
                let kind = match s.keyword {
                    crate::ast::Keyword::Fsm => "fsm",
                    crate::ast::Keyword::Type => "type",
                    crate::ast::Keyword::Claim => "claim",
                    crate::ast::Keyword::Schema => "schema",
                    crate::ast::Keyword::Subclaim => "subclaim",
                };
                return Err(RuntimeError::Parse(format!(
                    "{kind} `{}` constructs `{call}(...)` but isn't \
                     declared `external`. Either mark this declaration \
                     `external claim` / `external type`, or move the \
                     FFI into an `external claim` helper and call it \
                     from here.",
                    s.name
                )));
            }
        }
    }
    Ok(())
}

/// Walk a schema body and register any nested `subclaim` declarations
/// into `schemas` (recursively, so a subclaim of a subclaim is also
/// reachable).
pub(super) fn register_subclaims(body: &[BodyItem], schemas: &mut HashMap<String, SchemaDecl>) {
    for item in body {
        if let BodyItem::SubclaimDecl(s) = item {
            schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, schemas);
        }
    }
}
