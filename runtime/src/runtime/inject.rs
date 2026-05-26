//! FSM-aware membership injectors (the two whole-program-table sub-passes):
//!   * `inject_claim_arg_types` — fresh output names in positional calls
//!   * `inject_lhs_eq_types` — Identifier = Expr chained-membership inference
//!
//! The other two sub-passes — `inject_fsm_params` (implicit `state_next` /
//! `last_results` / `effects`) and `inject_prev_tick_decls` (`_var`
//! time-shift slots + `is_first_tick`) — were self-contained (they decide
//! from one body alone), and as of session REVIVE-inject they self-host in
//! Evident: see `stdlib/passes/inject.ev` + `crate::portable::inject`.
//! `runtime/src/runtime/load.rs` calls the Evident `fsm_params` / `prev_tick`
//! free functions for those, and the two functions here directly. The two
//! kept here resolve a name's type against the whole-program schema table +
//! enum registry, which the marshaler can't yet thread into the FSM per
//! claim (Gap D, `examples/COUNTEREXAMPLES.md` #27).

use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::HashMap;

/// Infer types for fresh names used as positional args in claim
/// calls. When the user writes
///
///   set_draw_color(win.renderer, Color(...), sky_eff)
///   effects = ⟨sky_eff, ...⟩
///
/// `sky_eff` is not declared as a Membership but its type is recoverable
/// from `set_draw_color`'s third param (`out ∈ Effect`). This pass
/// auto-injects `sky_eff ∈ Effect` so the user can drop the manual
/// decl line.
///
/// **Typo defense**: we only infer when the name appears in ≥ 2
/// expression positions across the body. If a name shows up exactly
/// once (just the call site), it might be a typo of an intended
/// reference — leave it alone so translation fails loudly. The common
/// case (claim-call output threaded into the effects list) hits ≥ 2
/// uses naturally.
///
/// Handles method-style `recv.claim(args)` too: the receiver counts
/// as a positional arg, shifting the arg-to-param mapping by 1.
pub(crate) fn inject_claim_arg_types(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if s.external { return Ok(()); }
    // Apply to fsm bodies and ordinary claim bodies alike — the
    // pattern is the same wherever a positional claim call has a
    // fresh output arg.
    let _ = Keyword::Fsm;

    // Step 1: declared names (memberships).
    let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &s.body {
        if let BodyItem::Membership { name, .. } = item {
            declared.insert(name.clone());
        }
    }

    // Step 2: count Identifier references across body expressions.
    let mut uses: HashMap<String, usize> = HashMap::new();
    fn walk(e: &Expr, uses: &mut HashMap<String, usize>) {
        match e {
            Expr::Identifier(n) => { *uses.entry(n.clone()).or_default() += 1; }
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                for x in es { walk(x, uses); },
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                { walk(a, uses); walk(b, uses); }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                { walk(r, uses); walk(b, uses); }
            Expr::Call(_, args) => for a in args { walk(a, uses); },
            Expr::Cardinality(i) | Expr::Not(i) => walk(i, uses),
            Expr::Field(recv, _) => walk(recv, uses),
            Expr::Binary(_, l, r) => { walk(l, uses); walk(r, uses); }
            Expr::Ternary(c, a, b) =>
                { walk(c, uses); walk(a, uses); walk(b, uses); }
            Expr::Match(scr, arms) => {
                walk(scr, uses);
                for arm in arms { walk(&arm.body, uses); }
            }
            Expr::Matches(e, _) => walk(e, uses),
            Expr::RunFsm { init, .. } => walk(init, uses),
        }
    }
    for item in &s.body {
        match item {
            BodyItem::Constraint(e) => walk(e, &mut uses),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&m.value, &mut uses); },
            _ => {}
        }
    }

    // Step 3: scan body for positional claim calls. For each
    // Identifier arg that's fresh + multi-use, look up the
    // corresponding param's type from the called claim and queue
    // a Membership injection.
    let mut to_inject_map: HashMap<String, String> = HashMap::new();

    // Dispatch resolver, subschema-aware. Three flavors:
    //   * Subschema: `recv.subclaim` where recv is a body Membership
    //     of record T AND T's body has SubclaimDecl `subclaim`.
    //     Args bind to subclaim's leading Memberships starting at
    //     position 0 (the receiver is NOT a positional arg — it
    //     provides the parent-type field scope at invocation time).
    //   * Receiver-prefix: `recv.claim` where claim is just a plain
    //     known schema. Receiver becomes the first positional arg,
    //     so args bind starting at slot 1.
    //   * Plain: bare `claim(args)`. Args bind from slot 0.
    let resolve = |name: &str| -> Option<(String, /*arg_offset:*/ usize)> {
        if schemas.contains_key(name) {
            return Some((name.to_string(), 0));
        }
        let (prefix, suffix) = name.rsplit_once('.')?;
        // Subschema check: prefix is a single-segment Membership of
        // a record type with `suffix` as a SubclaimDecl.
        if !prefix.contains('.') {
            for item in &s.body {
                if let BodyItem::Membership { name: mname, type_name, .. } = item {
                    if mname == prefix {
                        if let Some(type_decl) = schemas.get(type_name) {
                            let has_sub = type_decl.body.iter().any(|i|
                                matches!(i, BodyItem::SubclaimDecl(sub) if sub.name == suffix));
                            if has_sub {
                                return Some((suffix.to_string(), 0));
                            }
                        }
                    }
                }
            }
        }
        // Receiver-prefix fallback.
        if schemas.contains_key(suffix) {
            return Some((suffix.to_string(), 1));
        }
        None
    };

    let process_call = |claim_name: &str, arg_offset: usize, args: &[Expr],
                        declared: &std::collections::HashSet<String>,
                        uses: &HashMap<String, usize>,
                        to_inject_map: &mut HashMap<String, String>| {
        let Some(claim) = schemas.get(claim_name) else { return; };
        // Leading Memberships (first-line params + body Memberships).
        let claim_params: Vec<(String, String)> = claim.body.iter()
            .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
                Some((name.clone(), type_name.clone()))
            } else { None })
            .take(claim.param_count.max(args.len() + arg_offset))
            .collect();
        for (i, arg) in args.iter().enumerate() {
            let Expr::Identifier(arg_name) = arg else { continue; };
            if arg_name.contains('.') { continue; }   // field-access, not fresh
            if declared.contains(arg_name) { continue; }
            if schemas.contains_key(arg_name) { continue; }   // claim/type name
            let Some((_, param_type)) = claim_params.get(i + arg_offset) else { continue; };
            let count = uses.get(arg_name).copied().unwrap_or(0);
            if count < 2 { continue; }                // typo defense
            // First call wins if multiple sites disagree on type.
            to_inject_map.entry(arg_name.clone()).or_insert_with(|| param_type.clone());
        }
    };

    for item in &s.body {
        match item {
            BodyItem::Constraint(Expr::Call(name, args)) => {
                if let Some((cn, off)) = resolve(name) {
                    process_call(&cn, off, args, &declared, &uses, &mut to_inject_map);
                }
            }
            BodyItem::Constraint(Expr::InExpr(lhs, rhs)) => {
                if let (Expr::Tuple(items), Expr::Identifier(rname)) =
                    (lhs.as_ref(), rhs.as_ref())
                {
                    if let Some((cn, off)) = resolve(rname) {
                        process_call(&cn, off, items, &declared, &uses, &mut to_inject_map);
                    }
                }
            }
            _ => {}
        }
    }

    if to_inject_map.is_empty() { return Ok(()); }
    let insert_pos = s.param_count;
    // Stable order for diagnostics.
    let mut entries: Vec<(String, String)> = to_inject_map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (i, (name, type_name)) in entries.into_iter().enumerate() {
        s.body.insert(insert_pos + i, BodyItem::Membership {
            name, type_name, pins: Pins::None,
        });
    }
    Ok(())
}

/// Infer Memberships for body-level `Identifier = Expr` constraints
/// whose LHS is undeclared and whose RHS has a recoverable type.
///
/// Lets a subclaim or fsm body write
///
///   out = LibCall("...", "...", "...", ⟨…⟩)
///
/// without first declaring `out ∈ Effect` — the RHS's `LibCall` is
/// an Effect-constructor variant, so we inject `out ∈ Effect` at
/// the head of the body. Same idea for record constructors
/// (`pos = IVec2(3, 4)` infers `pos ∈ IVec2`).
///
/// Recursive: also processes SubclaimDecls inside the body, so the
/// inference fires inside types' rendering subclaims.
pub(crate) fn inject_lhs_eq_types(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    enums: &crate::core::EnumRegistry,
) {
    use crate::core::ast::{BinOp, BodyItem, Expr, Pins};

    // Collect names already declared in this body, with their types
    // (for field-access inference via dotted identifiers).
    let mut declared_types: HashMap<String, String> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declared_types.insert(name.clone(), type_name.clone());
        }
    }
    let declared: std::collections::HashSet<String> =
        declared_types.keys().cloned().collect();

    // Walk a dotted identifier path (`world.tick`, `win.pos.x`) and
    // return the leaf field's declared type. Looks up `head` in the
    // current body's memberships, then chains through schema bodies
    // for each subsequent segment.
    fn lookup_field_type(
        dotted: &str,
        declared_types: &HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
    ) -> Option<String> {
        let mut parts = dotted.split('.');
        let head = parts.next()?;
        let mut current_type = declared_types.get(head).cloned()?;
        for field in parts {
            let type_decl = schemas.get(&current_type)?;
            let mut next_type: Option<String> = None;
            for item in &type_decl.body {
                if let BodyItem::Membership { name, type_name, .. } = item {
                    if name == field { next_type = Some(type_name.clone()); break; }
                }
            }
            current_type = next_type?;
        }
        Some(current_type)
    }

    // Top-level inference: looks at an Expr and returns its type if
    // determinable. Leaves bare primitive literals (`Int(_)`, etc.)
    // to query-time inference. Recursive helpers DO match literals
    // because they're inside compound shapes (ternary arms, binary
    // operands) where the chained-membership inference is the only
    // way to know the result type.
    //
    // Recursive: ternaries / matches descend into arms; binary ops
    // either yield Bool (comparisons / logical) or recurse into
    // operands (arithmetic).
    fn infer_recursive(
        e: &Expr,
        declared_types: &HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
        enums: &crate::core::EnumRegistry,
    ) -> Option<String> {
        match e {
            Expr::Int(_)  => Some("Int".to_string()),
            Expr::Bool(_) => Some("Bool".to_string()),
            Expr::Str(_)  => Some("String".to_string()),
            Expr::Real(_) => Some("Real".to_string()),
            Expr::Identifier(n) => {
                if let Some(t) = declared_types.get(n) { return Some(t.clone()); }
                if n.contains('.') {
                    return lookup_field_type(n, declared_types, schemas);
                }
                None
            }
            Expr::Field(recv, field) => {
                // Compose the dotted path from receiver if it's an Identifier.
                if let Expr::Identifier(head) = recv.as_ref() {
                    return lookup_field_type(
                        &format!("{head}.{field}"), declared_types, schemas);
                }
                None
            }
            Expr::Call(name, _) => {
                if let Some((enum_name, _)) = enums.by_variant.borrow().get(name) {
                    return Some(enum_name.clone());
                }
                if schemas.contains_key(name) {
                    return Some(name.clone());
                }
                None
            }
            Expr::Ternary(_, a, b) =>
                infer_recursive(a, declared_types, schemas, enums)
                    .or_else(|| infer_recursive(b, declared_types, schemas, enums)),
            Expr::Match(_, arms) => arms.iter().find_map(|arm|
                infer_recursive(&arm.body, declared_types, schemas, enums)),
            Expr::Binary(op, l, r) => match op {
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                | BinOp::Eq | BinOp::Neq
                | BinOp::And | BinOp::Or | BinOp::Implies =>
                    Some("Bool".to_string()),
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div =>
                    infer_recursive(l, declared_types, schemas, enums)
                        .or_else(|| infer_recursive(r, declared_types, schemas, enums)),
                BinOp::Concat => Some("String".to_string()),
            },
            Expr::Not(_) => Some("Bool".to_string()),
            Expr::Cardinality(_) => Some("Int".to_string()),
            _ => None,
        }
    }

    // Top-level wrapper: skip bare primitive literals. Top-level
    // untyped primitives (`x = 5`) are intentionally left untyped —
    // declare them explicitly (`x ∈ Int = 5`).
    fn infer_type(
        e: &Expr,
        declared_types: &HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
        enums: &crate::core::EnumRegistry,
    ) -> Option<String> {
        if matches!(e, Expr::Int(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Real(_)) {
            return None;
        }
        infer_recursive(e, declared_types, schemas, enums)
    }

    // Walk body constraints; queue inferrable Memberships.
    let mut to_inject: Vec<(String, String)> = Vec::new();
    let mut already_queued: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &s.body {
        let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
        let Expr::Identifier(name) = lhs.as_ref() else { continue };
        if name.contains('.') { continue; }                  // field access on existing record
        if declared.contains(name) { continue; }              // already declared
        if already_queued.contains(name) { continue; }        // first eq wins
        if schemas.contains_key(name) { continue; }           // not a fresh local — claim/type name
        let Some(ty) = infer_type(rhs, &declared_types, schemas, enums) else { continue };
        to_inject.push((name.clone(), ty));
        already_queued.insert(name.clone());
    }

    // Inject at body head (after first-line params).
    let insert_pos = s.param_count;
    for (i, (name, type_name)) in to_inject.into_iter().enumerate() {
        s.body.insert(insert_pos + i, BodyItem::Membership {
            name, type_name, pins: Pins::None,
        });
    }

    // Recurse into nested subclaims.
    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            inject_lhs_eq_types(sub, schemas, enums);
        }
    }
}
