//! Source-level desugarings: Seq concat flattening, unified-world syntax,
//! and the user-vs-system boundary snapshot.

use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::HashSet;

/// Snapshot of "everything loaded so far is the system layer."
/// Schemas / enums loaded after a `mark_system_loads_complete()` call
/// are treated as the user's program for the purposes of AST encoding.
#[derive(Default, Clone)]
pub struct SystemBoundary {
    pub schemas: HashSet<String>,
    pub enums:   HashSet<String>,
}

/// Desugar `Seq(T)` concatenation. The user writes
///
/// ```text
/// effects = sky_effs ++ rect_effs ++ ⟨present_eff⟩ ++ input_effs
/// ```
///
/// flattening each `Concat` subtree into a single `SeqLit` when every
/// operand resolves to a literal sequence (a `⟨…⟩` or an identifier bound to
/// one), recursing into subclaims.
///
/// **Self-hosted (session REVIVE-desugar).** The two-pass gather/flatten/
/// rewrite walk that used to live here is **deleted**; the transform's
/// recursive kernels (`desugar_gather` + `desugar_flatten`) now run in
/// Evident as stack-FSMs in `stdlib/passes/desugar.ev`. This is a thin
/// adapter that delegates to the cached per-thread engine in
/// [`crate::portable::desugar::desugar_seq_concat`] (which keeps the
/// pre-order `rewrite` tree-walk and the string-keyed `FRef` lookup in Rust
/// — see that module for the faithfulness/perf split). Behavior is pinned
/// byte-for-byte by `runtime/tests/desugar_correctness.rs`.
///
/// `unify_world_syntax` (below) is the *other* desugar pass and stays
/// canonical Rust — it rewrites identifier strings by prefix-strip, which
/// Evident has no operator for.
pub(crate) fn desugar_seq_concat(s: &mut SchemaDecl) {
    crate::portable::desugar::desugar_seq_concat(s);
}

/// Unified-state world syntax. When an fsm declares
/// `world ∈ World` but NOT `world_next ∈ World`, the user is
/// using the `_var` time-shift convention for shared state:
///   * `world.X` reads/writes the current tick's value.
///   * `_world.X` reads the previous tick's value.
///
/// The multi-FSM scheduler still expects the legacy writer
/// pattern (`world` read-only + `world_next` write-only), so
/// this pass rewrites the body in-place to that shape:
///   * Every `world.X` reference (read or write) → `world_next.X`.
///     That makes it one Z3 var that's both constrained and
///     read within the same body — same semantics as the new
///     model's "this-tick value."
///   * Every `_world.X` reference → `world.X`. That's the
///     scheduler's "read of previous snapshot" path.
///   * Auto-inject `world_next ∈ World` so downstream
///     translation sees the legacy shape.
///
/// External fsms are skipped (they don't carry user logic).
pub(super) fn unify_world_syntax(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

    // Find `world` membership type (if any) and whether
    // `world_next` is already declared.
    let mut world_type: Option<String> = None;
    let mut has_world_next = false;
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            if name == "world" { world_type = Some(type_name.clone()); }
            if name == "world_next" { has_world_next = true; }
        }
    }
    let Some(world_ty) = world_type else { return Ok(()); };
    if has_world_next { return Ok(()); }   // legacy pattern; leave alone.

    // Only trigger the rewrite when the body actually uses
    // `_world.X` references — that's the unambiguous signal that
    // the user is in the unified-syntax world. Without this check,
    // legacy read-only fsms (declare `world ∈ World`, no `world_next`,
    // never write to world) would have their reads of `world.X`
    // wrongly promoted to writes, and the scheduler's single-owner-
    // per-field check would reject the program.
    fn uses_underscore_world(e: &Expr) -> bool {
        match e {
            Expr::Identifier(n) => n.starts_with("_world."),
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => false,
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                es.iter().any(uses_underscore_world),
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                uses_underscore_world(a) || uses_underscore_world(b),
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                uses_underscore_world(r) || uses_underscore_world(b),
            Expr::Call(_, args) => args.iter().any(uses_underscore_world),
            Expr::Cardinality(i) | Expr::Not(i) => uses_underscore_world(i),
            Expr::Field(recv, _) => uses_underscore_world(recv),
            Expr::Binary(_, l, r) =>
                uses_underscore_world(l) || uses_underscore_world(r),
            Expr::Ternary(c, a, b) =>
                uses_underscore_world(c) || uses_underscore_world(a)
                    || uses_underscore_world(b),
            Expr::Match(scr, arms) =>
                uses_underscore_world(scr)
                    || arms.iter().any(|a| uses_underscore_world(&a.body)),
            Expr::Matches(e, _) => uses_underscore_world(e),
            Expr::RunFsm { init, .. } => uses_underscore_world(init),
        }
    }
    let uses_new_syntax = s.body.iter().any(|item| match item {
        BodyItem::Constraint(e) => uses_underscore_world(e),
        BodyItem::ClaimCall { mappings, .. } =>
            mappings.iter().any(|m| uses_underscore_world(&m.value)),
        _ => false,
    });
    if !uses_new_syntax { return Ok(()); }

    // Rewrite Identifier strings in the body.
    //   "_world.X" → "world.X"
    //   "world.X"  → "world_next.X"
    // Same walk so both happen in one pass without re-matching.
    fn rewrite_ident(name: &str) -> Option<String> {
        if let Some(rest) = name.strip_prefix("_world.") {
            return Some(format!("world.{rest}"));
        }
        if let Some(rest) = name.strip_prefix("world.") {
            return Some(format!("world_next.{rest}"));
        }
        None
    }
    fn walk(e: &mut Expr) {
        match e {
            Expr::Identifier(n) => {
                if let Some(new_n) = rewrite_ident(n) { *n = new_n; }
            }
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                for x in es { walk(x); },
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                { walk(a); walk(b); }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                { walk(r); walk(b); }
            Expr::Call(_, args) => for a in args { walk(a); },
            Expr::Cardinality(i) | Expr::Not(i) => walk(i),
            Expr::Field(recv, _) => walk(recv),
            Expr::Binary(_, l, r) => { walk(l); walk(r); }
            Expr::Ternary(c, a, b) => { walk(c); walk(a); walk(b); }
            Expr::Match(scr, arms) => {
                walk(scr);
                for arm in arms { walk(arm.body.as_mut()); }
            }
            Expr::Matches(e, _) => walk(e),
            Expr::RunFsm { init, .. } => walk(init),
        }
    }
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => walk(e),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&mut m.value); },
            // Pin values inside type-use Memberships also need
            // rewriting — `mario ∈ MarioSprite (pos ↦ _world.player.pos)`
            // desugars at translate time to `mario.pos =
            // _world.player.pos`, which only resolves if the RHS has
            // been promoted to `world.player.pos` like the rest of the
            // body's `_world` reads.
            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => for m in named { walk(&mut m.value); },
                Pins::Positional(vals) => for v in vals { walk(v); },
                Pins::None => {}
            },
            _ => {}
        }
    }

    // Inject `world_next ∈ World` so the scheduler's writer-shape
    // detection finds it.
    let insert_pos = s.param_count;
    s.body.insert(insert_pos, BodyItem::Membership {
        name: "world_next".to_string(),
        type_name: world_ty,
        pins: Pins::None,
    });
    Ok(())
}
