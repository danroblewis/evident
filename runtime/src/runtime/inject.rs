//! Two whole-program-table inject passes: `inject_claim_arg_types` and `inject_lhs_eq_types`.
//! Others (`inject_fsm_params`, `inject_prev_tick_decls`) self-host in `stdlib/passes/inject.ev`.

use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::HashMap;

/// Inject `X ∈ T` for undeclared fresh positional-arg names whose type is recoverable from
/// the called claim's params. Typo defense: ≥ 2 uses. Handles `recv.claim` (offset 1).
pub(crate) fn inject_claim_arg_types(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if s.external { return Ok(()); }

    let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &s.body {
        if let BodyItem::Membership { name, .. } = item {
            declared.insert(name.clone());
        }
    }

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

    let mut to_inject_map: HashMap<String, String> = HashMap::new();

    // Resolve a call name to (claim_name, arg_offset): subschema (offset 0),
    // receiver-prefix (offset 1, receiver counts as first arg), or plain (offset 0).
    let resolve = |name: &str| -> Option<(String, /*arg_offset:*/ usize)> {
        // §4 guard: an `fsm`-keyword schema in call position is an embedded FSM
        // (`lower_fsm_application` rewrites `F(seed, out)` → `out = RunFsm{..}`),
        // not a claim — never arg-type-infer its call.
        if matches!(schemas.get(name).map(|s| &s.keyword), Some(Keyword::Fsm)) {
            return None;
        }
        if schemas.contains_key(name) {
            return Some((name.to_string(), 0));
        }
        let (prefix, suffix) = name.rsplit_once('.')?;
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
        if schemas.contains_key(suffix)
            && !matches!(schemas.get(suffix).map(|s| &s.keyword), Some(Keyword::Fsm))
        {
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
            if arg_name.contains('.') { continue; }
            if declared.contains(arg_name) { continue; }
            if schemas.contains_key(arg_name) { continue; }
            let Some((_, param_type)) = claim_params.get(i + arg_offset) else { continue; };
            let count = uses.get(arg_name).copied().unwrap_or(0);
            if count < 2 { continue; }
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
    let mut entries: Vec<(String, String)> = to_inject_map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (i, (name, type_name)) in entries.into_iter().enumerate() {
        s.body.insert(insert_pos + i, BodyItem::Membership {
            name, type_name, pins: Pins::None,
        });
    }
    Ok(())
}

/// Inject `X ∈ T` for undeclared `X = Expr` body constraints when `T` is recoverable from
/// the RHS (enum variant, record ctor, field type). Recurses into SubclaimDecls.
pub(crate) fn inject_lhs_eq_types(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    enums: &crate::core::EnumRegistry,
) {
    use crate::core::ast::{BinOp, BodyItem, Expr, Pins};

    let mut declared_types: HashMap<String, String> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declared_types.insert(name.clone(), type_name.clone());
        }
    }
    let declared: std::collections::HashSet<String> =
        declared_types.keys().cloned().collect();

    // Walk a dotted path (e.g. `world.tick`) and return the leaf field's type.
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

    // Infer a type from an Expr recursively (ternary arms, match arms, binary ops).
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

    // Top-level: skip bare primitive literals — declare them explicitly (`x ∈ Int = 5`).
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

    let mut to_inject: Vec<(String, String)> = Vec::new();
    let mut already_queued: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &s.body {
        let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
        let Expr::Identifier(name) = lhs.as_ref() else { continue };
        if name.contains('.') { continue; }
        if declared.contains(name) { continue; }
        if already_queued.contains(name) { continue; }
        if schemas.contains_key(name) { continue; }
        let Some(ty) = infer_type(rhs, &declared_types, schemas, enums) else { continue };
        to_inject.push((name.clone(), ty));
        already_queued.insert(name.clone());
    }

    let insert_pos = s.param_count;
    for (i, (name, type_name)) in to_inject.into_iter().enumerate() {
        s.body.insert(insert_pos + i, BodyItem::Membership {
            name, type_name, pins: Pins::None,
        });
    }

    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            inject_lhs_eq_types(sub, schemas, enums);
        }
    }
}
