//! Source-level desugarings: Seq concat flattening, unified-world syntax,
//! and the user-vs-system boundary snapshot.

use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::{HashMap, HashSet};

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
/// This pass walks the body twice: first to gather every
/// `name = ⟨items⟩` binding into a name→items map, then to walk
/// every body expression and rewrite each `Concat` subtree into a
/// flat `SeqLit`. The flattener resolves operands by:
///   * `SeqLit(items)` → use `items`.
///   * `Identifier(name)` → look up `seq_lits[name]`.
///   * `Concat(a, b)` → recurse.
///
/// If a `Concat` subtree fully resolves, it's replaced by a single
/// `SeqLit` of the flattened items. Concat nested inside a `Ternary`,
/// `Match` arm, claim-call argument, or further `Binary` ops is
/// rewritten too. If any operand can't be resolved (an opaque Seq
/// var coming from a claim invocation, for example), that subtree
/// is left alone and the translator will fail with the usual
/// "couldn't translate to Bool" error pointing at it.
pub(super) fn desugar_seq_concat(s: &mut SchemaDecl) {
    use crate::core::ast::{BinOp, BodyItem, Expr};
    if s.external { return; }

    // Pass 1: gather SeqLit bindings.
    let mut seq_lits: HashMap<String, Vec<Expr>> = HashMap::new();
    for item in &s.body {
        let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
        if let (Expr::Identifier(name), Expr::SeqLit(items)) =
            (lhs.as_ref(), rhs.as_ref())
        {
            seq_lits.insert(name.clone(), items.clone());
        }
    }

    fn flatten(
        e: &Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
    ) -> Option<Vec<Expr>> {
        match e {
            Expr::Binary(BinOp::Concat, l, r) => {
                let mut left = flatten(l, seq_lits)?;
                let right = flatten(r, seq_lits)?;
                left.extend(right);
                Some(left)
            }
            Expr::SeqLit(items) => Some(items.clone()),
            Expr::Identifier(name) => seq_lits.get(name).cloned(),
            _ => None,
        }
    }

    // Replace any Concat subexpression that fully flattens into a
    // SeqLit. Walks the entire tree so Concat inside Ternary,
    // Match arms, Call args, etc. all get rewritten.
    fn rewrite(
        e: &mut Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
    ) {
        if let Expr::Binary(BinOp::Concat, ..) = e {
            if let Some(items) = flatten(e, seq_lits) {
                *e = Expr::SeqLit(items);
                return;
            }
        }
        match e {
            Expr::Binary(_, l, r)
            | Expr::Range(l, r)
            | Expr::InExpr(l, r)
            | Expr::Index(l, r) => { rewrite(l, seq_lits); rewrite(r, seq_lits); }
            Expr::Ternary(c, a, b) => {
                rewrite(c, seq_lits); rewrite(a, seq_lits); rewrite(b, seq_lits);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es)
            | Expr::Call(_, es) => {
                for x in es { rewrite(x, seq_lits); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                rewrite(r, seq_lits); rewrite(b, seq_lits);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                rewrite(i, seq_lits);
            }
            Expr::Field(recv, _) => rewrite(recv, seq_lits),
            Expr::Match(scr, arms) => {
                rewrite(scr, seq_lits);
                for a in arms { rewrite(&mut a.body, seq_lits); }
            }
            _ => {}
        }
    }

    // Pass 2: walk every body item's expressions and rewrite Concat in place.
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => rewrite(e, &seq_lits),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings.iter_mut() {
                    rewrite(&mut m.value, &seq_lits);
                }
            }
            _ => {}
        }
    }

    // Recurse into subclaims.
    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            desugar_seq_concat(sub);
        }
    }
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
