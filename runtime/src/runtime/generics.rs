//! Generic monomorphization: expand `type Edge<T>` / `claim Toposort<T>`
//! references into concrete copies before translation.

use super::errors::RuntimeError;
use crate::ast::{BodyItem, SchemaDecl};
use std::collections::{HashMap, HashSet};

/// Parse "Edge<Rect>" into ("Edge", "Rect"). Returns None for
/// type-name strings that aren't a generic instantiation (no `<`,
/// or unbalanced angle brackets).
///
/// Handles nested generic args by counting depth: "Edge<Pair<Int,
/// String>>" parses to ("Edge", "Pair<Int, String>").
pub(super) fn split_generic_head(type_name: &str) -> Option<(String, String)> {
    let bytes = type_name.as_bytes();
    let lt = bytes.iter().position(|&b| b == b'<')?;
    if !type_name.ends_with('>') { return None; }
    // Verify balanced.
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 && i != bytes.len() - 1 { return None; }
            }
            _ => {}
        }
    }
    if depth != 0 { return None; }
    let name = type_name[..lt].to_string();
    let inner = type_name[lt + 1..bytes.len() - 1].to_string();
    Some((name, inner))
}

/// Split a comma-separated arg list at the TOP level — commas
/// inside nested `<...>` are not splits. "Pair<Int, String>, Bool"
/// → ["Pair<Int, String>", "Bool"].
pub(super) fn split_top_level_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = args.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth -= 1,
            b',' if depth == 0 => {
                out.push(args[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() { out.push(tail.to_string()); }
    out
}

/// Replace every token in `s` matching a key in `subst` with its
/// value. Tokens are maximal runs of identifier-char (ASCII
/// alphanumeric + `_`); other characters are passed through. Used
/// to substitute type parameters in a type-name string —
/// "Seq(T)" with `T → Rect` becomes "Seq(Rect)", "Pair<T, U>"
/// with `T → A, U → B` becomes "Pair<A, B>", `T_total` (an
/// unrelated identifier) is left alone.
pub(super) fn substitute_idents(s: &str, subst: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cur = String::new();
    fn is_ident_char(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }
    for c in s.chars() {
        if is_ident_char(c) {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                if let Some(rep) = subst.get(&cur) { out.push_str(rep); }
                else { out.push_str(&cur); }
                cur.clear();
            }
            out.push(c);
        }
    }
    if !cur.is_empty() {
        if let Some(rep) = subst.get(&cur) { out.push_str(rep); }
        else { out.push_str(&cur); }
    }
    out
}

/// Apply a type-param substitution to every `type_name` in a
/// SchemaDecl's body. Recurses into subclaim bodies.
pub(super) fn substitute_type_params_in_body(body: &mut Vec<BodyItem>, subst: &HashMap<String, String>) {
    use crate::ast::BodyItem;
    for item in body.iter_mut() {
        match item {
            BodyItem::Membership { type_name, .. } => {
                *type_name = substitute_idents(type_name, subst);
            }
            BodyItem::SubclaimDecl(sub) => {
                substitute_type_params_in_body(&mut sub.body, subst);
            }
            _ => {}
        }
    }
}

/// Collect every (composite_name, generic_head, args_str) tuple
/// referenced anywhere in the schemas map. Used by
/// `monomorphize_generics` to find work to do.
pub(super) fn collect_generic_uses(schemas: &HashMap<String, SchemaDecl>) -> Vec<(String, String, String)> {
    use crate::ast::BodyItem;
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    fn walk_expr(e: &crate::ast::Expr, out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
        use crate::ast::Expr;
        match e {
            // `Foo<Bar>(args, …)` — positional generic invocation
            // (e.g. `Permutation<Int>(a, b)` as a body constraint).
            Expr::Call(name, args) => {
                collect_from_type_name(name, out, seen);
                for a in args { walk_expr(a, out, seen); }
            }
            Expr::Binary(_, l, r) | Expr::Range(l, r)
            | Expr::InExpr(l, r) | Expr::Index(l, r) => {
                walk_expr(l, out, seen); walk_expr(r, out, seen);
            }
            Expr::Ternary(c, a, b) => {
                walk_expr(c, out, seen); walk_expr(a, out, seen); walk_expr(b, out, seen);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) => {
                for x in es { walk_expr(x, out, seen); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                walk_expr(r, out, seen); walk_expr(b, out, seen);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                walk_expr(i, out, seen);
            }
            Expr::Field(recv, _) => walk_expr(recv, out, seen),
            Expr::Match(scr, arms) => {
                walk_expr(scr, out, seen);
                for arm in arms { walk_expr(&arm.body, out, seen); }
            }
            _ => {}
        }
    }
    fn walk(body: &[BodyItem], out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
        for item in body {
            match item {
                BodyItem::Membership { type_name, .. } => {
                    collect_from_type_name(type_name, out, seen);
                }
                BodyItem::SubclaimDecl(sub) => walk(&sub.body, out, seen),
                // Generic claim invocations: `FirstEqual<Rect>(a ↦ …)`.
                BodyItem::ClaimCall { name, mappings } => {
                    collect_from_type_name(name, out, seen);
                    for m in mappings { walk_expr(&m.value, out, seen); }
                }
                // Generic passthrough: `..Edge<Rect>`.
                BodyItem::Passthrough(name) => {
                    collect_from_type_name(name, out, seen);
                }
                // Body constraints can contain `Foo<Bar>(args)` positional
                // invocations or `Foo<Bar>` constructor calls inline.
                BodyItem::Constraint(e) => walk_expr(e, out, seen),
            }
        }
    }
    fn collect_from_type_name(t: &str, out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
        // Handle the simple generic form "Edge<Rect>".
        if let Some((head, args)) = split_generic_head(t) {
            if seen.insert(t.to_string()) {
                out.push((t.to_string(), head.clone(), args.clone()));
            }
            // Each top-level arg may itself be a generic.
            for arg in split_top_level_args(&args) {
                collect_from_type_name(&arg, out, seen);
            }
            return;
        }
        // Handle "Seq(Edge<Rect>)" — recurse into the inner.
        if let Some(inner) = strip_seq_wrapper(t) {
            collect_from_type_name(inner, out, seen);
        }
    }
    for s in schemas.values() {
        walk(&s.body, &mut out, &mut seen);
    }
    out
}

/// If `t` is `"Seq(X)"`, `"Set(X)"`, `"Bag(X)"`, or `"Map(X)"`,
/// return Some(X). Otherwise None.
pub(super) fn strip_seq_wrapper(t: &str) -> Option<&str> {
    for prefix in &["Seq(", "Set(", "Bag(", "Map("] {
        if let Some(rest) = t.strip_prefix(prefix) {
            if let Some(inner) = rest.strip_suffix(')') {
                return Some(inner);
            }
        }
    }
    None
}

/// Monomorphize: produce concrete SchemaDecls for every generic
/// instantiation referenced in the program. After this pass, every
/// type_name containing `<` resolves to a real schema in the map.
///
/// Iterates to a fixed point: monomorphized schemas can themselves
/// reference generic types (`Toposort<T>`'s body has
/// `edges ∈ Seq(Edge<T>)`, which after substitution becomes
/// `edges ∈ Seq(Edge<Rect>)` — that's a new instantiation to expand).
pub(super) fn monomorphize_generics(
    schemas: &mut HashMap<String, SchemaDecl>,
    schema_order: &mut Vec<String>,
) -> Result<(), RuntimeError> {
    for _iteration in 0..50 {
        let needed = collect_generic_uses(schemas);
        let mut produced = 0;
        for (composite_name, generic_head, args_str) in needed {
            if schemas.contains_key(&composite_name) { continue; }
            let generic = match schemas.get(&generic_head) {
                Some(g) => g,
                None => continue,  // not a generic we know about; leave it
            };
            if generic.type_params.is_empty() {
                // Referenced like `Foo<Bar>` but Foo isn't generic.
                return Err(RuntimeError::Parse(format!(
                    "type `{}` referenced with type arguments `<{}>` but \
                     isn't declared as generic",
                    generic_head, args_str)));
            }
            let args = split_top_level_args(&args_str);
            if args.len() != generic.type_params.len() {
                return Err(RuntimeError::Parse(format!(
                    "type `{}` expects {} type argument(s), got {}: `{}`",
                    generic_head, generic.type_params.len(), args.len(),
                    composite_name)));
            }
            let mut subst: HashMap<String, String> = HashMap::new();
            for (p, a) in generic.type_params.iter().zip(args.iter()) {
                subst.insert(p.clone(), a.clone());
            }
            let mut mono = generic.clone();
            mono.name = composite_name.clone();
            mono.type_params = Vec::new();
            substitute_type_params_in_body(&mut mono.body, &subst);
            schemas.insert(composite_name.clone(), mono);
            schema_order.push(composite_name);
            produced += 1;
        }
        if produced == 0 { return Ok(()); }
    }
    Err(RuntimeError::Parse(
        "monomorphize_generics: didn't converge after 50 iterations (cycle?)".to_string()))
}
