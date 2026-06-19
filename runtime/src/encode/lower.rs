use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::HashMap;

pub(crate) fn desugar_seq_concat(s: &mut SchemaDecl) {
    use crate::core::ast::{BinOp, BodyItem, Expr};
    if s.external { return; }

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

    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            desugar_seq_concat(sub);
        }
    }
}

pub(crate) fn unify_world_syntax(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
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
    if has_world_next { return Ok(()); }

    fn uses_underscore_world(e: &Expr) -> bool {
        let mut found = false;
        crate::core::ast::walk_expr(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                if n.starts_with("_world.") { found = true; }
            }
        });
        found
    }
    let uses_new_syntax = s.body.iter().any(|item| match item {
        BodyItem::Constraint(e) => uses_underscore_world(e),
        BodyItem::ClaimCall { mappings, .. } =>
            mappings.iter().any(|m| uses_underscore_world(&m.value)),
        _ => false,
    });
    if !uses_new_syntax { return Ok(()); }

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
        crate::core::ast::walk_expr_mut(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                if let Some(new_n) = rewrite_ident(n) { *n = new_n; }
            }
        });
    }
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => walk(e),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&mut m.value); },

            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => for m in named { walk(&mut m.value); },
                Pins::Positional(vals) => for v in vals { walk(v); },
                Pins::None => {}
            },
            _ => {}
        }
    }

    let insert_pos = s.param_count;
    s.body.insert(insert_pos, BodyItem::Membership {
        name: "world_next".to_string(),
        type_name: world_ty,
        pins: Pins::None,
    });
    Ok(())
}

// ═════════════════════════ FSM param + type-inference injection ═════════════════════════

pub(crate) fn inject_fsm_params(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) {
        return Ok(());
    }
    if s.external {
        return Ok(());
    }

    let mut state_type: Option<String> = None;
    let mut have_state_next = false;
    let mut have_last_results = false;
    let mut have_effects = false;
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            match name.as_str() {
                "state" if state_type.is_none() => state_type = Some(type_name.clone()),
                "state_next"   => have_state_next   = true,
                "last_results" => have_last_results = true,
                "effects"      => have_effects      = true,
                _ => {}
            }
        }
    }

    fn walk(e: &Expr, targets: &mut [(&str, &mut bool)]) {
        crate::core::ast::walk_expr(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                for (name, hit) in targets.iter_mut() {
                    if n == *name { **hit = true; }
                }
            }
        });
    }
    let mut ref_state_next = false;
    let mut ref_last_results = false;
    let mut ref_effects = false;
    {
        let mut targets: Vec<(&str, &mut bool)> = vec![
            ("state_next",   &mut ref_state_next),
            ("last_results", &mut ref_last_results),
            ("effects",      &mut ref_effects),
        ];
        for item in &s.body {
            match item {
                BodyItem::Constraint(e) => walk(e, &mut targets),
                BodyItem::ClaimCall { mappings, .. } =>
                    for m in mappings { walk(&m.value, &mut targets); },
                _ => {}
            }
        }
    }

    let mut injected: Vec<BodyItem> = Vec::new();
    if !have_state_next && ref_state_next {
        if let Some(st) = &state_type {
            injected.push(BodyItem::Membership {
                name: "state_next".to_string(),
                type_name: st.clone(),
                pins: Pins::None,
            });
        }
    }
    if !have_last_results && ref_last_results {
        injected.push(BodyItem::Membership {
            name: "last_results".to_string(),
            type_name: "Seq(Result)".to_string(),
            pins: Pins::None,
        });
    }
    if !have_effects && ref_effects {
        injected.push(BodyItem::Membership {
            name: "effects".to_string(),
            type_name: "Seq(Effect)".to_string(),
            pins: Pins::None,
        });
    }
    let insert_pos = s.param_count;
    for (i, item) in injected.into_iter().enumerate() {
        s.body.insert(insert_pos + i, item);
    }
    Ok(())
}

pub(crate) fn inject_prev_tick_decls(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Keyword, Pins, Expr};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

    let mut declared: HashMap<String, String> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declared.insert(name.clone(), type_name.clone());
        }
    }

    let mut prev_refs: HashMap<String, String> = HashMap::new();
    fn walk(e: &Expr, declared: &HashMap<String, String>,
            prev_refs: &mut HashMap<String, String>) {
        crate::core::ast::walk_expr(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                let Some(after_underscore) = n.strip_prefix('_') else { return; };
                let first_seg = after_underscore
                    .split('.').next().unwrap_or(after_underscore);
                if let Some(ty) = declared.get(first_seg) {
                    let key = format!("_{first_seg}");
                    prev_refs.insert(key, ty.clone());
                }
            }
        });
    }
    for item in &s.body {
        match item {
            BodyItem::Constraint(e) => walk(e, &declared, &mut prev_refs),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&m.value, &declared, &mut prev_refs); },
            _ => {}
        }
    }

    if prev_refs.is_empty() { return Ok(()); }

    let mut to_inject: Vec<BodyItem> = Vec::new();
    for (prev_name, ty) in &prev_refs {
        if !declared.contains_key(prev_name) {
            to_inject.push(BodyItem::Membership {
                name: prev_name.clone(),
                type_name: ty.clone(),
                pins: Pins::None,
            });
        }
    }
    if !declared.contains_key("is_first_tick") {
        to_inject.push(BodyItem::Membership {
            name: "is_first_tick".to_string(),
            type_name: "Bool".to_string(),
            pins: Pins::None,
        });
    }
    let insert_pos = s.param_count;
    for (i, item) in to_inject.into_iter().enumerate() {
        s.body.insert(insert_pos + i, item);
    }
    Ok(())
}

pub(crate) fn inject_claim_arg_types(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) -> Result<(), RuntimeError> {
    use crate::core::ast::{BodyItem, Expr, Keyword, Pins};
    if s.external { return Ok(()); }

    let _ = Keyword::Fsm;

    let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &s.body {
        if let BodyItem::Membership { name, .. } = item {
            declared.insert(name.clone());
        }
    }

    let mut uses: HashMap<String, usize> = HashMap::new();
    fn walk(e: &Expr, uses: &mut HashMap<String, usize>) {
        crate::core::ast::walk_expr(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                *uses.entry(n.clone()).or_default() += 1;
            }
        });
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

    let resolve = |name: &str| -> Option<(String,  usize)> {
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
