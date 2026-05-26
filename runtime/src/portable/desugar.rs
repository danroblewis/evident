//! `desugar` — the source-level `Seq(T)`-concat flattening
//! (`desugar_seq_concat`). **Sole implementation: the self-hosted Evident
//! pass** (`stdlib/passes/desugar.ev`). The canonical Rust gather/flatten/
//! rewrite walk (in `runtime/src/runtime/desugar.rs`) is deleted; the
//! production load path flattens through here.
//!
//! Two RECURSIVE, value-carrying kernels run as stack-FSMs over the SHARED
//! marshaler ([`crate::translate::ast_encoder`]):
//!   - `desugar_gather`  — body → `Assoc` cons-list of `name ↦ ⟨items⟩`
//!     bindings (pass-1). Structural match, no string equality, so it
//!     self-hosts cleanly.
//!   - `desugar_flatten` — an `Expr` Concat spine → an ordered chunk stream
//!     (literal items + identifier `FRef` markers), or `FFail`.
//!
//! ## What stays in Rust, and why
//!
//!   1. **The pre-order `rewrite` tree-walk** ([`rewrite`]) — which `Expr`
//!      nodes to visit and where to splice the flattened `SeqLit`. Each
//!      `Concat`'s splice value depends on `FRef` resolution, and that
//!      string-keyed lookup stays in Rust (in-solve string equality blows up
//!      Z3 on string-heavy flatten states — the validate/#18 cousin), so the
//!      walk stays with it. A whole-body-return `desugar_rewrite` FSM was
//!      evaluated and deferred (SEED-marshal).
//!   2. **The string-keyed `FRef` lookup** — resolving `FRef(name)` against
//!      the gathered map ([`flatten`]): the `name = key` compare is a
//!      `HashMap` lookup here, out of the per-tick solve.
//!
//! `unify_world_syntax`, the other desugar pass, stays canonical Rust (it
//! rewrites identifier strings by prefix-strip + format — no Evident
//! string-construction operator yet). `desugar` is a load-time pass; per-tick
//! runtime is untouched.

use std::collections::HashMap;

use super::{run_done_payload, EvidentRunner};
use crate::core::ast::{BinOp, BodyItem, Expr, SchemaDecl};
use crate::core::Value;
use crate::translate::ast_decoder::{decode_expr, decode_list};
use crate::translate::ast_encoder::{body_item_list_to_value, expr_to_value};

guarded_runner!(runner, "passes/desugar.ev", "desugar_gather");

// ─────────────────────────────────────────────────────────────────────
// The two stack-FSM kernels (Evident) + the FRef lookup (Rust)
// ─────────────────────────────────────────────────────────────────────

/// Pass-1: gather `name = ⟨items⟩` bindings via `desugar_gather`, then decode
/// the `Assoc` cons-list into a Rust `name → items` map. The string-keyed map
/// lives in Rust (string equality is unreliable in-solve — #18 / the validate
/// blow-up — so `flatten` does the lookup here). Empty on any failure.
///
/// Last-wins on duplicate names mirrors the canonical HashMap: `desugar_gather`
/// prepends, so the last binding sits at the cons head and `or_insert`
/// (first-seen-wins on the head-first walk) keeps it.
fn gather(runner: &EvidentRunner, body: &[BodyItem]) -> HashMap<String, Vec<Expr>> {
    let seed = body_item_list_to_value(body);
    let Some(assoc) = run_done_payload(runner, "desugar_gather", seed, "GDone", "desugar/evident")
    else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    let mut cur = &assoc;
    // Walk the `Assoc` spine (ANil | ACons(AEntry, Assoc)); each entry is
    // MakeAEntry(name, ExprList).
    while let Value::Enum { variant, fields, .. } = cur {
        match (variant.as_str(), fields.as_slice()) {
            ("ANil", _) => break,
            ("ACons", [entry, rest]) => {
                if let Value::Enum { variant: ev, fields: ef, .. } = entry {
                    if ev == "MakeAEntry" && ef.len() == 2 {
                        if let Value::Str(name) = &ef[0] {
                            if let Ok(items) =
                                decode_list(&ef[1], "ExprList", "ELNil", "ELCons", decode_expr)
                            {
                                map.entry(name.clone()).or_insert(items);
                            }
                        }
                    }
                }
                cur = rest;
            }
            _ => break,
        }
    }
    map
}

/// Resolve a `Concat` subtree `e` against the gathered `bindings`. Mirrors the
/// canonical `flatten`: `Some(items)` when every operand resolves (a literal
/// `⟨…⟩` or a bound identifier), `None` otherwise.
///
/// The FSM returns a head-first chunk stream (`FDone(FChunks)`) — each chunk a
/// literal item (`FLitItem`) or an unresolved identifier ref (`FRef`); `FFail`
/// for a non-resolvable operand shape. We reverse to source order, then
/// expand: `FLitItem` contributes itself; `FRef(n)` contributes `bindings[n]`
/// (VERBATIM) or fails the whole flatten if `n` is unbound.
fn flatten(runner: &EvidentRunner, e: &Expr, bindings: &HashMap<String, Vec<Expr>>) -> Option<Vec<Expr>> {
    let chunks = match runner.run_fsm("desugar_flatten", expr_to_value(e)) {
        Ok(Value::Enum { variant, fields, .. }) if variant == "FDone" && fields.len() == 1 => {
            fields[0].clone()
        }
        Ok(Value::Enum { variant, .. }) if variant == "FFail" => return None,
        other => {
            eprintln!("[desugar/evident] flatten returned an unexpected state: {other:?}");
            return None;
        }
    };
    // Walk the head-first `FChunks` spine into source order.
    let mut rev: Vec<&Value> = Vec::new();
    let mut cur = &chunks;
    while let Value::Enum { variant, fields, .. } = cur {
        match (variant.as_str(), fields.as_slice()) {
            ("FCNil", _) => break,
            ("FCCons", [chunk, rest]) => {
                rev.push(chunk);
                cur = rest;
            }
            _ => return None,
        }
    }
    let mut out: Vec<Expr> = Vec::new();
    for chunk in rev.into_iter().rev() {
        let Value::Enum { variant, fields, .. } = chunk else { return None };
        match (variant.as_str(), fields.as_slice()) {
            ("FLitItem", [item]) => out.push(decode_expr(item).ok()?),
            ("FRef", [Value::Str(name)]) => out.extend(bindings.get(name)?.iter().cloned()),
            _ => return None,
        }
    }
    Some(out)
}

/// Pre-order rewrite of one Expr — a faithful copy of the canonical
/// `rewrite`, with `flatten` delegated to the Evident FSM (+ Rust
/// ref-resolution). A `Concat` that fully flattens is replaced by a single
/// `SeqLit` (no further recursion into it); everything else recurses into
/// children, in the same order.
fn rewrite(runner: &EvidentRunner, e: &mut Expr, bindings: &HashMap<String, Vec<Expr>>) {
    if let Expr::Binary(BinOp::Concat, ..) = e {
        if let Some(items) = flatten(runner, e, bindings) {
            *e = Expr::SeqLit(items);
            return;
        }
    }
    match e {
        Expr::Binary(_, l, r)
        | Expr::Range(l, r)
        | Expr::InExpr(l, r)
        | Expr::Index(l, r) => {
            rewrite(runner, l, bindings);
            rewrite(runner, r, bindings);
        }
        Expr::Ternary(c, a, b) => {
            rewrite(runner, c, bindings);
            rewrite(runner, a, bindings);
            rewrite(runner, b, bindings);
        }
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) | Expr::Call(_, es) => {
            for x in es {
                rewrite(runner, x, bindings);
            }
        }
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
            rewrite(runner, r, bindings);
            rewrite(runner, b, bindings);
        }
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => rewrite(runner, i, bindings),
        Expr::Field(recv, _) => rewrite(runner, recv, bindings),
        Expr::Match(scr, arms) => {
            rewrite(runner, scr, bindings);
            for a in arms {
                rewrite(runner, &mut a.body, bindings);
            }
        }
        _ => {}
    }
}

/// Gather + rewrite one schema (and recurse into subclaims). Subclaims are
/// held as Rust `SchemaDecl`s (never round-tripped through the marshaler), so
/// their `external` flag and nested-ctor `MatchPattern`s survive intact — the
/// in-place, never-round-trip-an-untouched-node discipline that keeps the
/// rewrite lossless despite the marshaler's `MatchPattern` history.
fn rewrite_schema(runner: &EvidentRunner, s: &mut SchemaDecl) {
    if s.external {
        return;
    }
    let bindings = gather(runner, &s.body);
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => rewrite(runner, e, &bindings),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings.iter_mut() {
                    rewrite(runner, &mut m.value, &bindings);
                }
            }
            _ => {}
        }
    }
    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            rewrite_schema(runner, sub);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point
// ─────────────────────────────────────────────────────────────────────

/// Flatten `Seq(T)` concatenations in `s` (in place) via the self-hosted
/// `desugar_gather` / `desugar_flatten` pass. **The runtime's sole
/// `desugar_seq_concat` entry point** — `runtime::desugar::desugar_seq_concat`
/// (on the load path) delegates here.
///
/// A schema with no `++` Concat ANYWHERE is a byte-identical no-op, so
/// [`schema_has_seq_concat`] short-circuits it (most `++` is String concat,
/// left alone) — keeping `desugar` near-free on the common case and skipping
/// the engine build entirely. The guarded runner short-circuits the bootstrap
/// re-entry (the trusted pass file has no Seq concat anyway).
pub fn desugar_seq_concat(s: &mut SchemaDecl) {
    if !schema_has_seq_concat(s) {
        return;
    }
    let Some(runner) = runner() else { return };
    rewrite_schema(&runner, s);
}

// ─────────────────────────────────────────────────────────────────────
// Concat-free fast path
// ─────────────────────────────────────────────────────────────────────

/// Does `s` contain a `Seq`-concat (`Expr::Binary(Concat, …)`) ANYWHERE the
/// pass would rewrite it — a `Constraint` expr, a `ClaimCall` mapping value,
/// or a nested subclaim's body? A cheap, pure-Rust structural scan; `false`
/// means [`desugar_seq_concat`] short-circuits.
fn schema_has_seq_concat(s: &SchemaDecl) -> bool {
    s.body.iter().any(|item| match item {
        BodyItem::Constraint(e) => expr_has_concat(e),
        BodyItem::ClaimCall { mappings, .. } => mappings.iter().any(|m| expr_has_concat(&m.value)),
        BodyItem::SubclaimDecl(sub) => schema_has_seq_concat(sub),
        _ => false,
    })
}

/// True if `e` contains a `Concat` binary anywhere in its tree. Visits the
/// same nodes as [`rewrite`] so the guard and the rewrite agree on
/// reachability.
fn expr_has_concat(e: &Expr) -> bool {
    match e {
        Expr::Binary(BinOp::Concat, ..) => true,
        Expr::Binary(_, l, r)
        | Expr::Range(l, r)
        | Expr::InExpr(l, r)
        | Expr::Index(l, r) => expr_has_concat(l) || expr_has_concat(r),
        Expr::Ternary(c, a, b) => expr_has_concat(c) || expr_has_concat(a) || expr_has_concat(b),
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) | Expr::Call(_, es) => {
            es.iter().any(expr_has_concat)
        }
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => expr_has_concat(r) || expr_has_concat(b),
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => expr_has_concat(i),
        Expr::Field(recv, _) => expr_has_concat(recv),
        Expr::Match(scr, arms) => {
            expr_has_concat(scr) || arms.iter().any(|a| expr_has_concat(&a.body))
        }
        _ => false,
    }
}
