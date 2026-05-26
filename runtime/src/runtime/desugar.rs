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

/// Generalized terse `_state` syntax — `unify_world_syntax` with the
/// hardcoded `world`/`World` names replaced by *any FSM state var*. When an
/// `fsm` declares a first-line param `X ∈ T` but NOT an explicit `X_next`,
/// and the body references `_X` (the previous-tick form), the author is
/// using the terse time-shift for that state var:
///   * `X` / `X.…`  (a bare write, not under `_`) is this tick's value.
///   * `_X` / `_X.…`                              reads the previous tick.
///
/// The non-scheduler embedding machinery (`fsm_unroll/compose.rs`,
/// `effect_loop/nested.rs`) and the scheduler's writer detection all expect
/// the literal `X, X_next ∈ T` pair, so this rewrites the body in-place to
/// that shape — exactly as `unify_world_syntax` does for `world`:
///   * Every bare `X` / `X.…` reference → `X_next` / `X_next.…`
///     (this-tick value: one var that's both written and read in the body).
///   * Every `_X` / `_X.…` reference → `X` / `X.…` (the prev-tick read,
///     the detectors' input const).
///   * Auto-inject `X_next ∈ T` so downstream pair detection sees it.
///
/// Generalizes over BOTH state shapes: a record state (`_X.field` accesses)
/// AND a bare enum/primitive state (`X = Push(...)` whole-value writes,
/// `_X` whole-value reads) — the prefix-or-bare rewrite covers both.
///
/// **Scope / safety gates** (matches `docs/design/fsms-as-functions-impl.md`
/// § 3, § 6):
///   * Only `Keyword::Fsm`, non-`external` schemas (a `claim` that happens
///     to reference `_foo` is not an FSM and is left alone).
///   * Only **param-position** memberships (index `< param_count`). A
///     scheduler primitive `_var` self-feedback var (`test_20`'s
///     `count ∈ Int = (is_first_tick ? 0 : _count + 1)`) is a *body* item,
///     not a param, so it stays on the `_var` machinery (`prev_tick`),
///     untouched.
///   * Only when the body actually references `_X` — the unambiguous terse
///     signal, mirroring `unify_world_syntax`'s `_world.` trigger.
///   * Skip if `X_next` is already declared (the explicit pair — back-compat;
///     this is what makes the pass INERT on the un-migrated corpus).
///   * Skip a primitive (`Int`/`Bool`/`Real`/`String`) state var when the
///     schema declares no `halt ∈ Bool` — a scheduler primitive
///     self-feedback var, which the `_var` machinery owns. (Enum/record
///     vars rewrite regardless; embedded-`Int` vars like `decrement`'s
///     `count` rewrite because they declare `halt`.)
///   * `world`/`world_next` are owned by `unify_world_syntax` above; this
///     pass never touches them. The two should merge later — kept separate
///     here so `world`'s well-tested pin-rewriting stays byte-identical.
pub(super) fn unify_state_syntax(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

    // `halt ∈ Bool` present? — the embedded-target signal that lets a
    // primitive state var (e.g. `decrement`'s `count ∈ Int`) be paired.
    let has_halt = s.body.iter().any(|item| matches!(item,
        BodyItem::Membership { name, type_name, .. }
            if name == "halt" && type_name == "Bool"));

    // Every membership name declared at SOURCE level (before the inject
    // passes run — they fire later in `load.rs`). Used to detect an
    // already-declared explicit `X_next` pair.
    let declared: HashSet<String> = s.body.iter().filter_map(|item| match item {
        BodyItem::Membership { name, .. } => Some(name.clone()),
        _ => None,
    }).collect();

    // Candidate terse state vars: param-position memberships `X ∈ T`.
    let mut candidates: Vec<(String, String)> = Vec::new();   // (name, type)
    for (i, item) in s.body.iter().enumerate() {
        if i >= s.param_count { break; }   // params are the first `param_count` items
        let BodyItem::Membership { name, type_name, .. } = item else { continue };
        if name == "world" || name == "world_next" { continue; } // owned by unify_world_syntax
        if name.ends_with("_next") { continue; }
        if declared.contains(&format!("{name}_next")) { continue; } // explicit pair → leave
        let primitive = matches!(type_name.as_str(),
            "Int" | "Bool" | "Real" | "String");
        if primitive && !has_halt { continue; } // scheduler primitive self-feedback var
        candidates.push((name.clone(), type_name.clone()));
    }
    if candidates.is_empty() { return Ok(()); }

    // Keep only candidates the body actually references as `_X` (the terse
    // signal). `_X` means an Identifier equal to `_X` or starting `_X.`.
    fn uses_underscore(e: &Expr, var: &str) -> bool {
        fn is_underscore_ref(n: &str, var: &str) -> bool {
            match n.strip_prefix('_') {
                Some(rest) => rest == var || rest.starts_with(&format!("{var}.")),
                None => false,
            }
        }
        match e {
            Expr::Identifier(n) => is_underscore_ref(n, var),
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => false,
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                es.iter().any(|x| uses_underscore(x, var)),
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                uses_underscore(a, var) || uses_underscore(b, var),
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                uses_underscore(r, var) || uses_underscore(b, var),
            Expr::Call(_, args) => args.iter().any(|x| uses_underscore(x, var)),
            Expr::Cardinality(i) | Expr::Not(i) => uses_underscore(i, var),
            Expr::Field(recv, _) => uses_underscore(recv, var),
            Expr::Binary(_, l, r) =>
                uses_underscore(l, var) || uses_underscore(r, var),
            Expr::Ternary(c, a, b) =>
                uses_underscore(c, var) || uses_underscore(a, var)
                    || uses_underscore(b, var),
            Expr::Match(scr, arms) =>
                uses_underscore(scr, var)
                    || arms.iter().any(|a| uses_underscore(&a.body, var)),
            Expr::Matches(e, _) => uses_underscore(e, var),
            Expr::RunFsm { init, .. } => uses_underscore(init, var),
        }
    }
    let body_uses = |var: &str| -> bool {
        s.body.iter().any(|item| match item {
            BodyItem::Constraint(e) => uses_underscore(e, var),
            BodyItem::ClaimCall { mappings, .. } =>
                mappings.iter().any(|m| uses_underscore(&m.value, var)),
            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => named.iter().any(|m| uses_underscore(&m.value, var)),
                Pins::Positional(vals) => vals.iter().any(|v| uses_underscore(v, var)),
                Pins::None => false,
            },
            _ => false,
        })
    };
    let targets: HashSet<String> = candidates.into_iter()
        .filter(|(name, _)| body_uses(name))
        .map(|(name, _)| name)
        .collect();
    if targets.is_empty() { return Ok(()); }

    // Rewrite an identifier string against the target set:
    //   "_X" / "_X.rest"  → "X" / "X.rest"        (read previous tick)
    //   "X"  / "X.rest"   → "X_next" / "X_next.rest" (write current tick)
    // One pass, one rewrite per identifier — same discipline as
    // `unify_world_syntax`'s `rewrite_ident`.
    let rewrite_ident = |name: &str| -> Option<String> {
        // Read-prev branch first (so `_X` doesn't fall through to write).
        if let Some(rest) = name.strip_prefix('_') {
            let head = rest.split('.').next().unwrap_or(rest);
            if targets.contains(head) {
                return Some(rest.to_string());
            }
        }
        let head = name.split('.').next().unwrap_or(name);
        if targets.contains(head) {
            if name == head {
                return Some(format!("{head}_next"));
            }
            if let Some(tail) = name.strip_prefix(&format!("{head}.")) {
                return Some(format!("{head}_next.{tail}"));
            }
        }
        None
    };
    fn walk(e: &mut Expr, rw: &impl Fn(&str) -> Option<String>) {
        match e {
            Expr::Identifier(n) => { if let Some(nn) = rw(n) { *n = nn; } }
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                for x in es { walk(x, rw); },
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                { walk(a, rw); walk(b, rw); }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                { walk(r, rw); walk(b, rw); }
            Expr::Call(_, args) => for a in args { walk(a, rw); },
            Expr::Cardinality(i) | Expr::Not(i) => walk(i, rw),
            Expr::Field(recv, _) => walk(recv, rw),
            Expr::Binary(_, l, r) => { walk(l, rw); walk(r, rw); }
            Expr::Ternary(c, a, b) => { walk(c, rw); walk(a, rw); walk(b, rw); }
            Expr::Match(scr, arms) => {
                walk(scr, rw);
                for arm in arms { walk(arm.body.as_mut(), rw); }
            }
            Expr::Matches(e, _) => walk(e, rw),
            Expr::RunFsm { init, .. } => walk(init, rw),
        }
    }
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => walk(e, &rewrite_ident),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&mut m.value, &rewrite_ident); },
            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => for m in named { walk(&mut m.value, &rewrite_ident); },
                Pins::Positional(vals) => for v in vals { walk(v, &rewrite_ident); },
                Pins::None => {}
            },
            _ => {}
        }
    }

    // Inject `X_next ∈ T` for each target so pair detection finds it.
    // Insert at `param_count` (the first non-param slot), preserving the
    // declared order of the source's state vars.
    let mut insert_pos = s.param_count;
    for (name, type_name) in s.body.iter()
        .take(s.param_count)
        .filter_map(|item| match item {
            BodyItem::Membership { name, type_name, .. } if targets.contains(name) =>
                Some((name.clone(), type_name.clone())),
            _ => None,
        })
        .collect::<Vec<_>>()
    {
        s.body.insert(insert_pos, BodyItem::Membership {
            name: format!("{name}_next"),
            type_name,
            pins: Pins::None,
        });
        insert_pos += 1;
    }
    Ok(())
}

#[cfg(test)]
mod state_syntax_tests {
    use super::unify_state_syntax;
    use crate::core::ast::{BinOp, BodyItem, Expr, Keyword, Pins, SchemaDecl};

    fn mem(name: &str, ty: &str) -> BodyItem {
        BodyItem::Membership { name: name.into(), type_name: ty.into(), pins: Pins::None }
    }
    fn eq(lhs: Expr, rhs: Expr) -> BodyItem {
        BodyItem::Constraint(Expr::Binary(BinOp::Eq, Box::new(lhs), Box::new(rhs)))
    }
    fn id(n: &str) -> Expr { Expr::Identifier(n.into()) }

    fn fsm(name: &str, body: Vec<BodyItem>, param_count: usize) -> SchemaDecl {
        SchemaDecl {
            keyword: Keyword::Fsm, name: name.into(), type_params: vec![],
            body, param_count, external: false,
        }
    }

    /// Names of all `X ∈ T` memberships, sorted, as `"X ∈ T"`.
    fn memberships(s: &SchemaDecl) -> Vec<String> {
        let mut v: Vec<String> = s.body.iter().filter_map(|i| match i {
            BodyItem::Membership { name, type_name, .. } => Some(format!("{name} ∈ {type_name}")),
            _ => None,
        }).collect();
        v.sort();
        v
    }

    /// Bare enum/Int state var: `_state` read + `state` write → pair.
    #[test]
    fn rewrites_enum_state_to_pair() {
        // fsm f(state ∈ SV, halt ∈ Bool) :  state = _state ;  halt = _state
        let mut s = fsm("f", vec![
            mem("state", "SV"), mem("halt", "Bool"),
            eq(id("state"), id("_state")),
            eq(id("halt"), id("_state")),
        ], 2);
        unify_state_syntax(&mut s).unwrap();
        // state_next ∈ SV injected; `state`(write)→state_next, `_state`(read)→state.
        assert!(memberships(&s).contains(&"state_next ∈ SV".to_string()),
            "expected injected state_next ∈ SV, got {:?}", memberships(&s));
        // First constraint (shifted to index 3 after state_next injected at 2):
        // state_next = state
        let c0 = match &s.body[3] { BodyItem::Constraint(e) => crate::pretty::expr(e), x => panic!("{x:?}") };
        assert_eq!(c0, "state_next = state");
    }

    /// Primitive state var WITH `halt` (embedded `decrement` shape) → paired.
    #[test]
    fn rewrites_primitive_with_halt() {
        // fsm decrement(count ∈ Int, halt ∈ Bool): count = _count ; halt = _count
        let mut s = fsm("decrement", vec![
            mem("count", "Int"), mem("halt", "Bool"),
            eq(id("count"), id("_count")),
            eq(id("halt"), id("_count")),
        ], 2);
        unify_state_syntax(&mut s).unwrap();
        assert!(memberships(&s).contains(&"count_next ∈ Int".to_string()),
            "expected count_next ∈ Int, got {:?}", memberships(&s));
    }

    /// Primitive state var WITHOUT `halt` (scheduler self-feedback) → left
    /// alone (the `_var` machinery owns it). Note this is the param-position
    /// case; the corpus uses the body-item form which is also excluded.
    #[test]
    fn skips_primitive_without_halt() {
        let mut s = fsm("counter", vec![
            mem("count", "Int"),
            eq(id("count"), id("_count")),
        ], 1);
        unify_state_syntax(&mut s).unwrap();
        assert!(!memberships(&s).contains(&"count_next ∈ Int".to_string()),
            "primitive self-feedback var must not be paired: {:?}", memberships(&s));
    }

    /// Explicit pair (X and X_next both declared) → inert (back-compat).
    #[test]
    fn inert_on_explicit_pair() {
        let mut s = fsm("f", vec![
            mem("state", "SV"), mem("state_next", "SV"), mem("halt", "Bool"),
            eq(id("state_next"), id("state")),
        ], 3);
        let before = s.body.len();
        unify_state_syntax(&mut s).unwrap();
        assert_eq!(s.body.len(), before, "explicit-pair fsm must be untouched");
    }

    /// Body var referenced as `_X` but NOT a param → not a state var.
    #[test]
    fn skips_non_param_body_var() {
        // fsm f(state ∈ S) :  count ∈ Int(body) ;  count = _count
        let mut s = fsm("f", vec![
            mem("state", "S"),      // param 0
            mem("count", "Int"),    // body item (index 1 ≥ param_count 1)
            eq(id("count"), id("_count")),
        ], 1);
        unify_state_syntax(&mut s).unwrap();
        assert!(!memberships(&s).contains(&"count_next ∈ Int".to_string()),
            "non-param body var must not be paired: {:?}", memberships(&s));
        // And `state` (a param) isn't referenced as `_state`, so no pair either.
        assert!(!memberships(&s).contains(&"state_next ∈ S".to_string()));
    }

    /// `world` is owned by `unify_world_syntax`; this pass never pairs it.
    #[test]
    fn never_touches_world() {
        let mut s = fsm("g", vec![
            mem("world", "World"),
            eq(Expr::Field(Box::new(id("world")), "x".into()),
               Expr::Field(Box::new(id("_world")), "x".into())),
        ], 1);
        unify_state_syntax(&mut s).unwrap();
        assert!(!memberships(&s).contains(&"world_next ∈ World".to_string()),
            "world must be left to unify_world_syntax: {:?}", memberships(&s));
    }

    /// Non-`fsm` keyword referencing `_foo` is not an FSM → untouched.
    #[test]
    fn skips_non_fsm_keyword() {
        let mut s = fsm("c", vec![
            mem("state", "SV"),
            eq(id("state"), id("_state")),
        ], 1);
        s.keyword = Keyword::Claim;
        unify_state_syntax(&mut s).unwrap();
        assert!(!memberships(&s).contains(&"state_next ∈ SV".to_string()));
    }

    /// Record state via `.field` accesses (`_state.x` read, `state.x` write).
    #[test]
    fn rewrites_record_field_accesses() {
        // fsm f(state ∈ Vec, halt ∈ Bool):  state.x = _state.x ; halt = _state.done
        let mut s = fsm("f", vec![
            mem("state", "Vec"), mem("halt", "Bool"),
            eq(Expr::Field(Box::new(id("state")), "x".into()),
               Expr::Field(Box::new(id("_state")), "x".into())),
        ], 2);
        unify_state_syntax(&mut s).unwrap();
        assert!(memberships(&s).contains(&"state_next ∈ Vec".to_string()));
        let c = match &s.body[3] { BodyItem::Constraint(e) => crate::pretty::expr(e), x => panic!("{x:?}") };
        assert_eq!(c, "state_next.x = state.x");
    }
}
