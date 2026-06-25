use crate::core::RuntimeError;
use crate::core::ast::SchemaDecl;
use std::collections::HashMap;

pub(crate) fn desugar_seq_concat(s: &mut SchemaDecl) {
    use crate::core::ast::{BinOp, BodyItem, Expr};
    if s.external { return; }

    let mut seq_lits: HashMap<String, Vec<Expr>> = HashMap::new();
    // Pinned lengths (`#name = N`): let `++` splice a Seq whose elements are
    // computed (e.g. coindexed-built) rather than literal, by expanding `name`
    // into `⟨name[0], …, name[N-1]⟩`. This is what makes
    // `effects = setup ++ built_draws ++ tail` assemble an N-element render
    // list without hand-enumerating every index.
    let mut seq_lengths: HashMap<String, usize> = HashMap::new();
    for item in &s.body {
        let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
        match (lhs.as_ref(), rhs.as_ref()) {
            (Expr::Identifier(name), Expr::SeqLit(items)) => {
                seq_lits.insert(name.clone(), items.clone());
            }
            (Expr::Cardinality(inner), Expr::Int(n)) if *n >= 0 => {
                if let Expr::Identifier(name) = inner.as_ref() {
                    seq_lengths.insert(name.clone(), *n as usize);
                }
            }
            _ => {}
        }
    }

    fn flatten(
        e: &Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
        seq_lengths: &HashMap<String, usize>,
    ) -> Option<Vec<Expr>> {
        match e {
            Expr::Binary(BinOp::Concat, l, r) => {
                let mut left = flatten(l, seq_lits, seq_lengths)?;
                let right = flatten(r, seq_lits, seq_lengths)?;
                left.extend(right);
                Some(left)
            }
            Expr::SeqLit(items) => Some(items.clone()),
            Expr::Identifier(name) => {
                if let Some(items) = seq_lits.get(name) {
                    Some(items.clone())
                } else {
                    // A pinned-length Seq with computed elements: expand by index.
                    let &len = seq_lengths.get(name)?;
                    Some((0..len).map(|i| Expr::Index(
                        Box::new(Expr::Identifier(name.clone())),
                        Box::new(Expr::Int(i as i64)))).collect())
                }
            }
            _ => None,
        }
    }

    fn rewrite(
        e: &mut Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
        seq_lengths: &HashMap<String, usize>,
    ) {
        if let Expr::Binary(BinOp::Concat, ..) = e {
            if let Some(items) = flatten(e, seq_lits, seq_lengths) {
                *e = Expr::SeqLit(items);
                return;
            }
        }
        match e {
            Expr::Binary(_, l, r)
            | Expr::Range(l, r)
            | Expr::InExpr(l, r)
            | Expr::Index(l, r) => { rewrite(l, seq_lits, seq_lengths); rewrite(r, seq_lits, seq_lengths); }
            Expr::Ternary(c, a, b) => {
                rewrite(c, seq_lits, seq_lengths); rewrite(a, seq_lits, seq_lengths); rewrite(b, seq_lits, seq_lengths);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es)
            | Expr::Call(_, es) => {
                for x in es { rewrite(x, seq_lits, seq_lengths); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                rewrite(r, seq_lits, seq_lengths); rewrite(b, seq_lits, seq_lengths);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                rewrite(i, seq_lits, seq_lengths);
            }
            Expr::Field(recv, _) => rewrite(recv, seq_lits, seq_lengths),
            Expr::Match(scr, arms) => {
                rewrite(scr, seq_lits, seq_lengths);
                for a in arms { rewrite(&mut a.body, seq_lits, seq_lengths); }
            }
            _ => {}
        }
    }

    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => rewrite(e, &seq_lits, &seq_lengths),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings.iter_mut() {
                    rewrite(&mut m.value, &seq_lits, &seq_lengths);
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

// ═════════════════════════ user-defined operator desugar ═════════════════════════

/// Desugar infix uses of a user-defined operator (`a · b`, `a × b`) into a
/// fresh result variable + the operator body inlined as constraints.
///
/// Dispatch is **type-directed**: for each `Expr::Binary(UserOp(sym), l, r)` we
/// resolve the operand type from the body's declared memberships (chasing record
/// fields through `schemas`), find that type's matching `OperatorDecl`, then:
///   1. mint a fresh result var `__op<N>` of the declared result type,
///   2. inline the operator body with operand-params → `l`/`r`, result-param →
///      the fresh var (substitution is dotted-name rewriting, the same canonical
///      record-leaf form the componentwise lift already uses),
///   3. replace the `Binary` node in place with `Identifier(fresh)`.
///
/// A `UserOp` whose operand type declares no such operator is left untouched —
/// it survives to encode time and fails loudly there (honest, never silently
/// dropped). A type with NO operator decl never reaches this rewrite, so the
/// existing componentwise lift over `+`/`-`/`=`/scalar `*` is wholly unchanged.
pub(crate) fn desugar_operators(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) {
    use crate::core::ast::{BinOp, BodyItem, Expr, Pins};
    if s.external { return; }

    // Local name → type map (the body's explicit memberships).
    let mut declared: HashMap<String, String> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declared.insert(name.clone(), type_name.clone());
        }
    }

    // Resolve the static type of an operand expression: a bare identifier, or a
    // dotted field-chain off one, chasing record fields through `schemas`.
    fn operand_type(
        e: &Expr,
        declared: &HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
    ) -> Option<String> {
        let dotted = expr_to_dotted(e)?;
        let mut parts = dotted.split('.');
        let head = parts.next()?;
        let mut cur = declared.get(head).cloned()?;
        for field in parts {
            let decl = schemas.get(&cur)?;
            let mut next = None;
            for item in &decl.body {
                if let BodyItem::Membership { name, type_name, .. } = item {
                    if name == field { next = Some(type_name.clone()); break; }
                }
            }
            cur = next?;
        }
        Some(cur)
    }

    let mut fresh_counter: usize = 0;
    let mut extra: Vec<BodyItem> = Vec::new();

    // Rewrite one expression bottom-up: inner operators desugar first so a fresh
    // result feeding into an outer operator already carries a resolvable type
    // (we re-snapshot `declared` with each injected result below).
    fn rewrite(
        e: &mut Expr,
        declared: &mut HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
        fresh_counter: &mut usize,
        extra: &mut Vec<BodyItem>,
    ) {
        // Children first.
        match e {
            Expr::Binary(_, l, r)
            | Expr::Range(l, r)
            | Expr::InExpr(l, r)
            | Expr::Index(l, r) => {
                rewrite(l, declared, schemas, fresh_counter, extra);
                rewrite(r, declared, schemas, fresh_counter, extra);
            }
            Expr::Ternary(c, a, b) => {
                rewrite(c, declared, schemas, fresh_counter, extra);
                rewrite(a, declared, schemas, fresh_counter, extra);
                rewrite(b, declared, schemas, fresh_counter, extra);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) | Expr::Call(_, es) => {
                for x in es { rewrite(x, declared, schemas, fresh_counter, extra); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                rewrite(r, declared, schemas, fresh_counter, extra);
                rewrite(b, declared, schemas, fresh_counter, extra);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Delta(i) | Expr::Matches(i, _) => {
                rewrite(i, declared, schemas, fresh_counter, extra);
            }
            Expr::Field(recv, _) => rewrite(recv, declared, schemas, fresh_counter, extra),
            Expr::Match(scr, arms) => {
                rewrite(scr, declared, schemas, fresh_counter, extra);
                for a in arms { rewrite(&mut a.body, declared, schemas, fresh_counter, extra); }
            }
            _ => {}
        }

        // Then this node, if it is a user operator we can resolve.
        let Expr::Binary(BinOp::UserOp(sym), l, r) = e else { return };
        let Some(ty) = operand_type(l, declared, schemas) else { return };
        let Some(decl) = schemas.get(&ty) else { return };
        let Some(op) = decl.operators.iter().find(|o| &o.symbol == sym) else { return };
        if op.operands.len() != 2 { return; }

        let fresh = format!("__op{}", *fresh_counter);
        *fresh_counter += 1;

        // operand/result param → concrete expr substitution.
        let mut subst: HashMap<String, Expr> = HashMap::new();
        subst.insert(op.operands[0].clone(), (**l).clone());
        subst.insert(op.operands[1].clone(), (**r).clone());
        subst.insert(op.result.clone(), Expr::Identifier(fresh.clone()));

        // Declare the fresh result + inject the substituted body constraints.
        extra.push(BodyItem::Membership {
            name: fresh.clone(),
            type_name: op.result_type.clone(),
            pins: Pins::None,
        });
        declared.insert(fresh.clone(), op.result_type.clone());
        for item in &op.body {
            if let BodyItem::Constraint(body_e) = item {
                let mut sub_e = body_e.clone();
                substitute_params(&mut sub_e, &subst);
                extra.push(BodyItem::Constraint(sub_e));
            }
        }

        *e = Expr::Identifier(fresh);
    }

    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) =>
                rewrite(e, &mut declared, schemas, &mut fresh_counter, &mut extra),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings.iter_mut() {
                    rewrite(&mut m.value, &mut declared, schemas, &mut fresh_counter, &mut extra);
                },
            _ => {}
        }
    }
    s.body.extend(extra);

    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            desugar_operators(sub, schemas);
        }
    }
}

/// Render an identifier / dotted field-chain as its canonical dotted name
/// (`a` → "a", `dot.pos` → "dot.pos"). Returns `None` for anything else.
fn expr_to_dotted(e: &crate::core::ast::Expr) -> Option<String> {
    use crate::core::ast::Expr;
    match e {
        Expr::Identifier(n) => Some(n.clone()),
        Expr::Field(recv, field) => Some(format!("{}.{}", expr_to_dotted(recv)?, field)),
        _ => None,
    }
}

/// Substitute operator params throughout a body expr. An operand/result param
/// name is replaced by its bound expr; a field-chain whose head is a param
/// (`a.x` with `a` bound to identifier `p`) collapses to the dotted leaf
/// (`p.x`) — the canonical record-leaf form the encoder's env already keys on.
fn substitute_params(
    e: &mut crate::core::ast::Expr,
    subst: &HashMap<String, crate::core::ast::Expr>,
) {
    use crate::core::ast::Expr;

    // A `Field`-chain whose head identifier is a substituted param: rebuild it
    // as a dotted identifier when the operand is itself dotted/identifier.
    if let Expr::Field(..) = e {
        if let Some(dotted) = expr_to_dotted(e) {
            let head = dotted.split('.').next().unwrap_or(&dotted);
            if let Some(bound) = subst.get(head) {
                if let Some(bound_dotted) = expr_to_dotted(bound) {
                    let rest = &dotted[head.len()..]; // includes leading '.'
                    *e = Expr::Identifier(format!("{bound_dotted}{rest}"));
                    return;
                }
            }
        }
    }
    if let Expr::Identifier(n) = e {
        if let Some(bound) = subst.get(n) {
            *e = bound.clone();
            return;
        }
    }
    crate::core::ast::walk_children_mut(e, &mut |c| substitute_params(c, subst));
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

    let mut have_last_results = false;
    let mut have_effects = false;
    for item in &s.body {
        if let BodyItem::Membership { name, .. } = item {
            match name.as_str() {
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
    let mut ref_last_results = false;
    let mut ref_effects = false;
    {
        let mut targets: Vec<(&str, &mut bool)> = vec![
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

/// Forward-difference desugar: `Δe ≡ (e − _e)` (NEW minus OLD).
///
/// Walks the body and replaces every `Expr::Delta(inner)` with
/// `Expr::Binary(Sub, inner, prev_of(inner))`, where `prev_of` mirrors the
/// `_var` previous-tick convention used by `inject_prev_tick_decls`: the first
/// dotted segment of an identifier gets a leading underscore. So `Δx` becomes
/// `x − _x`, and `Δx = -1` constrains `x = _x - 1`.
///
/// Run BEFORE `inject_prev_tick_decls` so the generated `_x` reference gets its
/// prev-tick declaration + `is_first_tick` injected.
pub(crate) fn desugar_delta(s: &mut SchemaDecl) {
    use crate::core::ast::{BinOp, BodyItem, Expr};

    // Time-shift one tick: every identifier leaf gets one more leading
    // underscore (`x → _x`, `_x → __x`), and the shift recurses through
    // arithmetic so it is TOTAL over already-lowered Δ expressions.
    // `prev_of(x − _x) = _x − __x`, which is what `ΔΔ` needs. Literals
    // shift to themselves (a constant is the same one tick ago).
    fn prev_of(e: &Expr) -> Expr {
        match e {
            Expr::Identifier(n) => {
                let (first, rest) = match n.split_once('.') {
                    Some((f, r)) => (f, Some(r)),
                    None => (n.as_str(), None),
                };
                let prev = match rest {
                    Some(r) => format!("_{first}.{r}"),
                    None => format!("_{first}"),
                };
                Expr::Identifier(prev)
            }
            Expr::Field(recv, fld) => {
                Expr::Field(Box::new(prev_of(recv)), fld.clone())
            }
            Expr::Index(a, b) => {
                Expr::Index(Box::new(prev_of(a)), Box::new(prev_of(b)))
            }
            Expr::Binary(op, l, r) => {
                Expr::Binary(op.clone(), Box::new(prev_of(l)), Box::new(prev_of(r)))
            }
            // Literals are the same one tick ago.
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => e.clone(),
            // Any other shape: shift it whole (best effort). The trampoline
            // carries the prev value of whatever leaves are inside.
            _ => e.clone(),
        }
    }

    // Lower `Delta(inner)` to `inner − prev_of(inner)`, lowering the
    // INNERMOST Delta first. For `Δx`: `x − _x`. For `ΔΔx`:
    // `Δx` lowers to `x − _x`, then the outer Δ yields
    // `(x − _x) − prev_of(x − _x)` = `(x − _x) − (_x − __x)` = `x − 2·_x + __x`.
    fn rewrite(e: &mut Expr) {
        if let Expr::Delta(inner) = e {
            // Lower the inner expression first (handles nested Δ → second
            // difference, and any Δ buried deeper in a compound inner).
            crate::core::ast::walk_expr_mut(inner, &mut rewrite);
            let cur = std::mem::replace(inner.as_mut(), Expr::Int(0));
            let prev = prev_of(&cur);
            *e = Expr::Binary(BinOp::Sub, Box::new(cur), Box::new(prev));
        }
    }

    for item in &mut s.body {
        match item {
            BodyItem::Constraint(e) => crate::core::ast::walk_expr_mut(e, &mut rewrite),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings {
                    crate::core::ast::walk_expr_mut(&mut m.value, &mut rewrite);
                }
            }
            _ => {}
        }
    }
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

    // Collect every prev-tick reference, keyed by underscore-prefixed name
    // (`_pos`, `__pos`). For an N-underscore ref we inject ALL levels
    // 1..=N (so `__pos` also injects `_pos`); `max_depth` records the
    // deepest history any ref reaches (1 = `_var`, 2 = `__var`).
    let mut prev_refs: HashMap<String, String> = HashMap::new();
    let mut max_depth: usize = 0;
    // A bare `is_first_tick` reference (e.g. from a `:=` initial-value seed, or
    // a hand-written `is_first_tick ⇒ …` guard with no `_var` read) must also
    // trigger the Bool decl below — otherwise the seed constraint references an
    // undeclared name and is silently dropped.
    let mut refs_first_tick = false;
    fn walk(e: &Expr, declared: &HashMap<String, String>,
            prev_refs: &mut HashMap<String, String>, max_depth: &mut usize,
            refs_first_tick: &mut bool) {
        crate::core::ast::walk_expr(e, &mut |n| {
            if let Expr::Identifier(n) = n {
                if n == "is_first_tick" { *refs_first_tick = true; return; }
                let depth = n.chars().take_while(|c| *c == '_').count();
                if depth == 0 { return; }
                let base = &n[depth..];
                let first_seg = base.split('.').next().unwrap_or(base);
                if let Some(ty) = declared.get(first_seg) {
                    if depth > *max_depth { *max_depth = depth; }
                    for k in 1..=depth {
                        let key = format!("{}{first_seg}", "_".repeat(k));
                        prev_refs.insert(key, ty.clone());
                    }
                }
            }
        });
    }
    for item in &s.body {
        match item {
            BodyItem::Constraint(e) => walk(e, &declared, &mut prev_refs, &mut max_depth, &mut refs_first_tick),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&m.value, &declared, &mut prev_refs, &mut max_depth, &mut refs_first_tick); },
            _ => {}
        }
    }

    if prev_refs.is_empty() && !refs_first_tick { return Ok(()); }

    let mut to_inject: Vec<BodyItem> = Vec::new();
    // Deterministic order: shallowest history first (`_pos` before `__pos`),
    // then by name.
    let mut prev_sorted: Vec<(&String, &String)> = prev_refs.iter().collect();
    prev_sorted.sort_by(|a, b| {
        let da = a.0.chars().take_while(|c| *c == '_').count();
        let db = b.0.chars().take_while(|c| *c == '_').count();
        da.cmp(&db).then_with(|| a.0.cmp(b.0))
    });
    for (prev_name, ty) in prev_sorted {
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
    // `is_second_tick` is the tick-1 bootstrap flag — only meaningful when a
    // two-tick-history (`__var`) reference exists.
    if max_depth >= 2 && !declared.contains_key("is_second_tick") {
        to_inject.push(BodyItem::Membership {
            name: "is_second_tick".to_string(),
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
                // A user operator should have been desugared already; if one
                // survives we cannot infer its result type here.
                BinOp::UserOp(_) => None,
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
