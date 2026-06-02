//! Generic-type monomorphization. `Edge<Rect>` → concrete schema with `T→Rect`.
//! Iterates to fixpoint; new schemas may reference further generics.

use std::collections::{HashMap, HashSet};

use crate::core::ast::{BodyItem, Expr, SchemaDecl};
use crate::core::RuntimeError;

fn is_generic_head(t: &str) -> bool {
    let bytes = t.as_bytes();
    if !bytes.iter().any(|&b| b == b'<') { return false; }
    if !t.ends_with('>') { return false; }
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 && i != bytes.len() - 1 { return false; }
            }
            _ => {}
        }
    }
    depth == 0
}

fn split_top_level_args(args: &str) -> Vec<String> {
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

fn strip_seq_wrapper(t: &str) -> Option<&str> {
    for prefix in &["Seq(", "Set(", "Bag(", "Map("] {
        if let Some(rest) = t.strip_prefix(prefix) {
            if let Some(inner) = rest.strip_suffix(')') {
                return Some(inner);
            }
        }
    }
    None
}

fn rust_split_head(t: &str) -> (String, String) {
    match t.find('<') {
        Some(lt) if t.ends_with('>') => (t[..lt].to_string(), t[lt + 1..t.len() - 1].to_string()),
        _ => (t.to_string(), String::new()),
    }
}

fn collect_from_type_name(t: &str, out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
    if is_generic_head(t) {
        let (head, args) = rust_split_head(t);
        if seen.insert(t.to_string()) {
            out.push((t.to_string(), head, args.clone()));
        }
        for arg in split_top_level_args(&args) {
            collect_from_type_name(&arg, out, seen);
        }
        return;
    }
    if let Some(inner) = strip_seq_wrapper(t) {
        collect_from_type_name(inner, out, seen);
    }
}

fn collect_from_body(body: &[BodyItem], out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
    for item in body {
        match item {
            BodyItem::Membership { type_name, .. } => collect_from_type_name(type_name, out, seen),
            BodyItem::Passthrough(n) => collect_from_type_name(n, out, seen),
            BodyItem::ClaimCall { name, mappings } => {
                collect_from_type_name(name, out, seen);
                for m in mappings { collect_from_expr(&m.value, out, seen); }
            }
            BodyItem::Constraint(e) => collect_from_expr(e, out, seen),
            BodyItem::SubclaimDecl(s) => collect_from_body(&s.body, out, seen),
        }
    }
}

fn collect_from_expr(e: &Expr, out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
    match e {
        Expr::Call(name, args) => {
            collect_from_type_name(name, out, seen);
            for a in args { collect_from_expr(a, out, seen); }
        }
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) => {
            collect_from_expr(a, out, seen); collect_from_expr(b, out, seen);
        }
        Expr::Ternary(a, b, c) => {
            collect_from_expr(a, out, seen);
            collect_from_expr(b, out, seen);
            collect_from_expr(c, out, seen);
        }
        Expr::SetLit(xs) | Expr::SeqLit(xs) | Expr::Tuple(xs) =>
            for x in xs { collect_from_expr(x, out, seen); },
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
            collect_from_expr(r, out, seen); collect_from_expr(b, out, seen);
        }
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => collect_from_expr(i, out, seen),
        Expr::Field(b, _) => collect_from_expr(b, out, seen),
        Expr::Match(scr, arms) => {
            collect_from_expr(scr, out, seen);
            for a in arms { collect_from_expr(&a.body, out, seen); }
        }
        Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
    }
}

fn subst_type_name(t: &str, params: &[String], args: &[String]) -> String {
    let mut cur = t.to_string();
    for (p, a) in params.iter().zip(args.iter()) {
        cur = cur.replacen(p, a, 1);
    }
    cur
}

fn apply_substitution_to_body(body: &mut [BodyItem], params: &[String], args: &[String]) {
    for item in body.iter_mut() {
        match item {
            BodyItem::Membership { type_name, .. } => {
                *type_name = subst_type_name(type_name, params, args);
            }
            BodyItem::SubclaimDecl(sub) => {
                apply_substitution_to_body(&mut sub.body, params, args);
            }
            _ => {}
        }
    }
}

pub(super) fn monomorphize_generics(
    schemas: &mut HashMap<String, SchemaDecl>,
    schema_order: &mut Vec<String>,
) -> Result<(), RuntimeError> {
    for _iteration in 0..50 {
        let mut needed: Vec<(String, String, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut keys: Vec<&String> = schemas.keys().collect();
        keys.sort();
        for k in keys {
            let s = &schemas[k];
            collect_from_body(&s.body, &mut needed, &mut seen);
        }
        let mut produced = 0;
        for (composite_name, generic_head, args_str) in needed {
            if schemas.contains_key(&composite_name) { continue; }
            let generic = match schemas.get(&generic_head) {
                Some(g) => g,
                None => continue,
            };
            if generic.type_params.is_empty() {
                return Err(RuntimeError::Parse(format!(
                    "type `{}` referenced with type arguments `<{}>` but isn't declared as generic",
                    generic_head, args_str)));
            }
            let args = split_top_level_args(&args_str);
            if args.len() != generic.type_params.len() {
                return Err(RuntimeError::Parse(format!(
                    "type `{}` expects {} type argument(s), got {}: `{}`",
                    generic_head, generic.type_params.len(), args.len(), composite_name)));
            }
            let params = generic.type_params.clone();
            let mut mono = generic.clone();
            mono.name = composite_name.clone();
            mono.type_params = Vec::new();
            apply_substitution_to_body(&mut mono.body, &params, &args);
            schemas.insert(composite_name.clone(), mono);
            schema_order.push(composite_name);
            produced += 1;
        }
        if produced == 0 { return Ok(()); }
    }
    Err(RuntimeError::Parse("monomorphize_generics: didn't converge after 50 iterations (cycle?)".to_string()))
}
