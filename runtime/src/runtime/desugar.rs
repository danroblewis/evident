//! Source-level desugarings: Seq concat flattening, unified-world syntax,
//! and the user-vs-system boundary snapshot.

use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::HashSet;

/// Marks the system/user boundary: schemas loaded after `mark_system_loads_complete()`
/// are the user's program for AST encoding purposes.
#[derive(Default, Clone)]
pub struct SystemBoundary {
    pub schemas: HashSet<String>,
    pub enums:   HashSet<String>,
}

/// Flatten `++` Seq concat chains into a single `SeqLit`. Self-hosted
/// (REVIVE-desugar): delegates to `portable::desugar::desugar_seq_concat`.
pub(crate) fn desugar_seq_concat(s: &mut SchemaDecl) {
    crate::portable::desugar::desugar_seq_concat(s);
}

/// Rewrite `world.X` / `_world.X` unified syntax to the legacy `world` / `world_next`
/// pair the scheduler expects; injects `world_next ∈ World`. Skips external fsms.
pub(super) fn unify_world_syntax(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

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

    // Only rewrite when the body uses `_world.X`. Without this, legacy read-only fsms
    // (no `world_next`) would have `world.X` reads wrongly promoted, failing single-owner.
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

    // One-pass rewrite: `_world.X` → `world.X`, `world.X` → `world_next.X`.
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
            // Pin values in Memberships also need rewriting: `pos ↦ _world.player.pos`
            // must be promoted to `world.player.pos` like other `_world` reads.
            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => for m in named { walk(&mut m.value); },
                Pins::Positional(vals) => for v in vals { walk(v); },
                Pins::None => {}
            },
            _ => {}
        }
    }

    // Inject `world_next ∈ World` so the scheduler's writer-shape detection finds it.
    let insert_pos = s.param_count;
    s.body.insert(insert_pos, BodyItem::Membership {
        name: "world_next".to_string(),
        type_name: world_ty,
        pins: Pins::None,
    });
    Ok(())
}

/// Like `unify_world_syntax` but for any FSM state param `X`: rewrites `_X`/`X` to the
/// `X`/`X_next` pair. Skips: non-fsm, external, non-param vars, primitives without `halt`.
pub(super) fn unify_state_syntax(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

    // `halt ∈ Bool` present? Allows pairing a primitive state var (e.g. `count ∈ Int`).
    let has_halt = s.body.iter().any(|item| matches!(item,
        BodyItem::Membership { name, type_name, .. }
            if name == "halt" && type_name == "Bool"));

    // Source-level membership names (before inject passes run); detect explicit `X_next` pair.
    let declared: HashSet<String> = s.body.iter().filter_map(|item| match item {
        BodyItem::Membership { name, .. } => Some(name.clone()),
        _ => None,
    }).collect();

    // Candidate terse state vars: param-position memberships `X ∈ T`.
    let mut candidates: Vec<(String, String)> = Vec::new();
    for (i, item) in s.body.iter().enumerate() {
        if i >= s.param_count { break; }
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

    // Keep only candidates the body references as `_X` (the terse signal).
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

    // One-pass rewrite: `_X`/`_X.rest` → `X`/`X.rest` (read prev); `X`/`X.rest` → `X_next`/`X_next.rest`.
    // Read-prev branch first so `_X` doesn't fall through to the write branch.
    let rewrite_ident = |name: &str| -> Option<String> {
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

    // Inject `X_next ∈ T` at param_count (first non-param slot), preserving source order.
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

    /// All memberships sorted as `"X ∈ T"` strings.
    fn memberships(s: &SchemaDecl) -> Vec<String> {
        let mut v: Vec<String> = s.body.iter().filter_map(|i| match i {
            BodyItem::Membership { name, type_name, .. } => Some(format!("{name} ∈ {type_name}")),
            _ => None,
        }).collect();
        v.sort();
        v
    }

    #[test]
    fn rewrites_enum_state_to_pair() {
        // fsm f(state ∈ SV, halt ∈ Bool) :  state = _state ;  halt = _state
        let mut s = fsm("f", vec![
            mem("state", "SV"), mem("halt", "Bool"),
            eq(id("state"), id("_state")),
            eq(id("halt"), id("_state")),
        ], 2);
        unify_state_syntax(&mut s).unwrap();
        // state_next ∈ SV should be injected.
        assert!(memberships(&s).contains(&"state_next ∈ SV".to_string()),
            "expected injected state_next ∈ SV, got {:?}", memberships(&s));
        // First constraint (shifted to index 3 after state_next injected at 2):
        // state_next = state
        let c0 = match &s.body[3] { BodyItem::Constraint(e) => crate::pretty::expr(e), x => panic!("{x:?}") };
        assert_eq!(c0, "state_next = state");
    }

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
