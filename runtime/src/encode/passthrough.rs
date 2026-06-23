//! Parameterized passthrough (`..Name(slot ↦ other, …)`) body rewriting.
//!
//! A bare `..Name` flat-mixes the included claim's body into the current scope
//! verbatim. A *parameterized* passthrough renames some of the included claim's
//! carried fields onto outer-scope names AND freshens the rest, so two
//! passthroughs of the same claim with different renames compose into
//! INDEPENDENT sub-systems.
//!
//! Example:
//! ```text
//! fsm one_d_random_walk
//!     a, da ∈ Int
//!     -1 ≤ da ≤ 1
//!     is_first_tick ⇒ a = 0
//!     ¬is_first_tick ⇒ Δa = da        -- already lowered to: a − _a = da
//! fsm random_walk
//!     x, y ∈ Int
//!     ..one_d_random_walk(a ↦ x)      -- a→x, da freshened to da__pt0
//!     ..one_d_random_walk(a ↦ y)      -- a→y, da freshened to da__pt1
//! ```
//!
//! The two instances must not share `da`; each gets its own freshened copy so
//! the two walks step independently.
//!
//! IMPORTANT: passthrough is expanded at ENCODE time, *after* the included
//! claim was lowered at load time (Δ desugared, `_var` prev-tick decls + the
//! shared `is_first_tick` injected). So the rewrite must also remap the
//! prev-tick form: renaming `a → x` must turn `_a` into `_x`, and freshening
//! `da → da__pt0` must turn `_da` into `_da__pt0`.

use std::collections::HashMap;

use crate::core::ast::{BodyItem, Expr, Mapping, SchemaDecl, walk_expr_mut};
use super::declare::next_call_id;

/// Load-time expansion: replace every *parameterized* passthrough
/// (`..Name(slot ↦ other, …)`, i.e. non-empty renames) in `body` with the
/// renamed + freshened body items of the referenced claim, looked up in
/// `schemas` (already-lowered).
///
/// Bare `..Name` passthroughs (empty renames) are left untouched — they compose
/// flat via the existing declare/inline machinery. Baking the expansion into the
/// AST once at load time means the declare pass and the inline pass both see the
/// same freshened names, with no runtime ID coordination.
pub(crate) fn expand_parameterized_passthroughs(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) {
    let mut out: Vec<BodyItem> = Vec::with_capacity(s.body.len());
    for item in std::mem::take(&mut s.body) {
        match item {
            BodyItem::Passthrough { name, renames } if !renames.is_empty() => {
                match schemas.get(&name) {
                    Some(claim) => out.extend(rewrite_passthrough_body(claim, &renames)),
                    None => {
                        eprintln!("warning: ..{name}(…) references unknown claim");
                        out.push(BodyItem::Passthrough { name, renames });
                    }
                }
            }
            other => out.push(other),
        }
    }
    s.body = out;
}

/// Names that are SHARED across all passthrough instances and must never be
/// renamed or freshened — they refer to the same single value per tick.
const SHARED_MARKERS: &[&str] = &["is_first_tick"];

/// Build the rewritten body for a parameterized passthrough.
///
/// `claim` is the included declaration; `renames` is the call-site rename list.
/// Returns a fresh `Vec<BodyItem>` with every carried-field identifier remapped:
/// renamed fields point at the outer-scope target, un-renamed carried fields are
/// freshened with a per-instance suffix so independent instances don't collide.
///
/// When `renames` is empty the caller should NOT use this path — bare `..Name`
/// composes unchanged.
pub(super) fn rewrite_passthrough_body(
    claim: &SchemaDecl,
    renames: &[Mapping],
) -> Vec<BodyItem> {
    let sub = build_substitution(claim, renames);
    let mut body = claim.body.clone();
    for item in &mut body {
        rewrite_body_item(item, &sub);
    }
    body
}

/// Map each carried-field name of the included claim to its replacement.
/// Renamed fields → the rename target identifier. Every other field-level
/// membership → a freshened name (`field__pt<id>`), making instances disjoint.
fn build_substitution(claim: &SchemaDecl, renames: &[Mapping]) -> HashMap<String, String> {
    let mut sub: HashMap<String, String> = HashMap::new();

    for m in renames {
        if let Expr::Identifier(target) = &m.value {
            sub.insert(m.slot.clone(), target.clone());
        }
    }

    let fresh = next_call_id();
    for item in &claim.body {
        if let BodyItem::Membership { name, .. } = item {
            // Prev-tick decls (`_a`, injected by the included claim's lowering)
            // are NOT freshened on their own — they ride their base name's
            // substitution via `remap_identifier` (so `_a` follows `a → x`).
            let base = name.strip_prefix('_').unwrap_or(name);
            if sub.contains_key(base)
                || name.starts_with('_')
                || SHARED_MARKERS.contains(&name.as_str())
            {
                continue;
            }
            sub.insert(name.clone(), format!("{name}__pt{fresh}"));
        }
    }
    sub
}

/// Apply the name substitution to a single body item: membership decl names and
/// every identifier inside constraints / claim-call mappings.
fn rewrite_body_item(item: &mut BodyItem, sub: &HashMap<String, String>) {
    match item {
        BodyItem::Membership { name, .. } => {
            // Use the same identifier remap as constraints so a prev-tick decl
            // (`_a`) follows its base's rename (`a → x` ⟹ `_a → _x`), and a
            // freshened base (`da → da__pt0`) renames its decl too.
            if let Some(repl) = remap_identifier(name, sub) {
                *name = repl;
            }
        }
        BodyItem::Constraint(e) => rewrite_expr(e, sub),
        BodyItem::ClaimCall { mappings, .. } => {
            for m in mappings {
                rewrite_expr(&mut m.value, sub);
            }
        }
        // Nested subclaims and further passthroughs inside an included claim are
        // rare; leave their structure intact (renaming would need to recurse the
        // whole subclaim scope). The repro and primary use is flat carried-var
        // composition, which lives in Membership + Constraint items.
        BodyItem::SubclaimDecl(_) | BodyItem::Passthrough { .. } => {}
    }
}

/// Rewrite every identifier leaf in an expression, honoring the `_`-prefixed
/// prev-tick convention: `_a` is the previous-tick form of `a`, so a rename of
/// `a` must shift `_a` in lockstep. Dotted leaves (`a.field`) remap on the
/// first segment.
fn rewrite_expr(e: &mut Expr, sub: &HashMap<String, String>) {
    walk_expr_mut(e, &mut |node| {
        if let Expr::Identifier(s) = node {
            if let Some(repl) = remap_identifier(s, sub) {
                *s = repl;
            }
        }
    });
}

/// Compute the replacement for an identifier string, or `None` if unchanged.
/// Strips a single leading `_` (prev-tick marker) and a trailing `.field` chain
/// to find the base carried name; if that base is in `sub`, reassembles the
/// remapped identifier with the same prefix/suffix.
fn remap_identifier(ident: &str, sub: &HashMap<String, String>) -> Option<String> {
    let (prefix, rest) = match ident.strip_prefix('_') {
        Some(r) => ("_", r),
        None => ("", ident),
    };
    let (base, dotted) = match rest.split_once('.') {
        Some((b, d)) => (b, Some(d)),
        None => (rest, None),
    };
    let repl = sub.get(base)?;
    Some(match dotted {
        Some(d) => format!("{prefix}{repl}.{d}"),
        None => format!("{prefix}{repl}"),
    })
}
